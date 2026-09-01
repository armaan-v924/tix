//! Renders the configuration reference from the config types themselves.
//!
//! The section structs already carry doc comments — they are published as
//! rustdoc at `/crates` — so `schemars` turns those, the field types, and
//! which fields are required into a schema this module formats. A key cannot
//! be added to the config without appearing here, and `just check-docs`
//! fails until the rendered page catches up.
//!
//! The `schema` feature exists only for this. It is off by default, so
//! nothing shipped compiles `schemars`.

use crate::mdx::Page;
use schemars::{JsonSchema, schema_for};
use serde_json::Value;

/// One documented config section, e.g. `[cli]`.
struct Section {
    /// The TOML table name as written in the file.
    table: String,
    /// Which document the section belongs in.
    document: Document,
    /// What this section is for, in the reader's terms.
    ///
    /// Deliberately not the struct's own doc comment: those explain why the
    /// type is shaped as it is — which section types exist, why there is no
    /// whole-document struct — which is right for the crate docs and wrong
    /// here. The *field* docs are reused, because those describe the keys.
    intro: &'static str,
    schema: Value,
}

#[derive(PartialEq, Clone, Copy)]
enum Document {
    Global,
    Ticket,
}

impl Document {
    fn heading(self) -> &'static str {
        match self {
            Document::Global => "The global config",
            Document::Ticket => "The ticket document",
        }
    }
}

/// Builds a section from a type carrying the `JsonSchema` derive.
fn section<T: JsonSchema>(table: &str, document: Document, intro: &'static str) -> Section {
    Section {
        table: table.to_string(),
        document,
        intro,
        schema: serde_json::to_value(schema_for!(T)).expect("schema serializes"),
    }
}

/// Renders the configuration reference.
pub fn render() -> Page {
    let sections = vec![
        section::<tix_cli::tix::config::CliConfig>("cli", Document::Global, CLI_INTRO),
        section::<tix_sdk::EngineConfig>("engine", Document::Global, ENGINE_INTRO),
        section::<tix_sdk::Defaults>("defaults", Document::Global, DEFAULTS_INTRO),
        section::<tix_sdk::TicketConfig>("ticket", Document::Ticket, TICKET_SECTION_INTRO),
    ];

    let mut body = crate::mdx::frontmatter(
        "Configuration",
        "Every key tix reads, in the global config and the ticket document.",
        10,
        3,
    );
    body.push_str(PREAMBLE);

    let mut current: Option<Document> = None;
    for section in &sections {
        if current != Some(section.document) {
            body.push_str(&format!("\n## {}\n", section.document.heading()));
            body.push_str(match section.document {
                Document::Global => GLOBAL_INTRO,
                Document::Ticket => TICKET_INTRO,
            });
            current = Some(section.document);
        }
        body.push_str(&render_section(section));
    }
    body.push_str(PLUGIN_TABLES);

    Page {
        path: "reference/configuration.mdx".to_string(),
        body,
    }
}

/// One section: its heading, its own doc comment, its key table, and a table
/// for each nested type it refers to.
fn render_section(section: &Section) -> String {
    let mut out = format!("\n### `[{}]`\n{}\n", section.table, section.intro);
    out.push_str(&key_table(&section.schema, &section.schema));

    // A field whose values are a struct (`configured_repositories`,
    // `worktrees`) gets that struct's own table, since its keys are what the
    // user actually writes in the file.
    for (name, definition) in nested_types(&section.schema) {
        out.push_str(&format!(
            "\n#### `[{}.{}.<name>]`\n\nEach entry is a table of its own:\n\n",
            section.table, name.0
        ));
        out.push_str(&key_table(&definition, &section.schema));
    }
    out
}

/// Field name of the referring property, kept so the sub-table can be titled
/// by where it appears rather than by its Rust type name.
struct FieldName(String);

/// Every `$defs` entry reachable from a property of `schema`, paired with the
/// property that reaches it.
fn nested_types(schema: &Value) -> Vec<(FieldName, Value)> {
    let Some(defs) = schema.get("$defs").and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (field, property) in properties {
        let Some(name) = referenced_definition(property) else {
            continue;
        };
        if let Some(definition) = defs.get(&name) {
            out.push((FieldName(field.clone()), definition.clone()));
        }
    }
    out
}

/// The `$defs` name a property points at, whether directly or as the value
/// type of a map.
fn referenced_definition(property: &Value) -> Option<String> {
    let reference = property
        .get("$ref")
        .or_else(|| {
            property
                .get("additionalProperties")
                .and_then(|v| v.get("$ref"))
        })
        .or_else(|| property.get("items").and_then(|v| v.get("$ref")))?;
    reference
        .as_str()?
        .strip_prefix("#/$defs/")
        .map(ToString::to_string)
}

/// The key table for one object schema.
fn key_table(schema: &Value, root: &Value) -> String {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return String::new();
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut out = String::from("| Key | Type | Description |\n| --- | --- | --- |\n");
    // Required keys first, then alphabetically — the order someone filling in
    // the file needs them, rather than the order serde happens to emit.
    let mut names: Vec<&String> = properties.keys().collect();
    names.sort_by_key(|name| (!required.contains(&name.as_str()), (*name).clone()));

    for name in names {
        let property = &properties[name];
        let mut description = property
            .get("description")
            .and_then(Value::as_str)
            .map(|doc| crate::mdx::cell(&summarize(doc)))
            .unwrap_or_else(|| "—".to_string());
        if !required.contains(&name.as_str()) {
            description.push_str("<br />Optional.");
        }
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            name,
            type_name(property, root),
            description
        ));
    }
    out
}

/// A TOML-shaped name for a property's type.
///
/// JSON Schema words the config in terms it was not written in: the file is
/// TOML, so a map is a table and an array of strings is an array of strings,
/// not `object` and `array`.
fn type_name(property: &Value, _root: &Value) -> String {
    // A `$defs` name is a Rust type name, which appears nowhere in a TOML
    // file. The shape is what matters, and the entry's own table follows.
    if referenced_definition(property).is_some() {
        return match property.get("additionalProperties") {
            Some(_) => "table of tables".to_string(),
            None => "table".to_string(),
        };
    }
    match scalar_type(property) {
        Some("array") => {
            let item = property
                .get("items")
                .and_then(scalar_type)
                .unwrap_or("value");
            format!("array of {item}")
        }
        Some("object") => "table".to_string(),
        Some(other) => other.to_string(),
        None => "value".to_string(),
    }
}

/// The JSON Schema type of a property.
///
/// An optional field is emitted as `["string", "null"]` rather than a bare
/// string; the null is an artefact of `Option`, and every key's optionality
/// is already stated in its own column.
fn scalar_type(property: &Value) -> Option<&str> {
    let type_field = property.get("type")?;
    if let Some(name) = type_field.as_str() {
        return Some(name);
    }
    type_field
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|name| *name != "null")
}

/// Trims a rustdoc comment down to what belongs in a reference table.
///
/// Struct-level docs carry `# Examples` sections with runnable doctests,
/// which are right for rustdoc and wrong here, and intra-doc links render as
/// stray brackets outside rustdoc.
fn summarize(doc: &str) -> String {
    let body = doc
        .split('\n')
        .take_while(|line| !line.starts_with("# "))
        .collect::<Vec<_>>()
        .join("\n");
    strip_doc_links(body.trim())
}

/// Turns `[`Type::method`]` into `` `Type::method` ``, and `[text](target)`
/// rustdoc references into plain text.
fn strip_doc_links(doc: &str) -> String {
    let mut out = String::with_capacity(doc.len());
    let mut chars = doc.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '[' {
            out.push(c);
            continue;
        }
        let mut inner = String::new();
        for c in chars.by_ref() {
            if c == ']' {
                break;
            }
            inner.push(c);
        }
        // A trailing `(...)` makes it a link with a target; drop the target.
        let had_target = chars.peek() == Some(&'(');
        if had_target {
            for c in chars.by_ref() {
                if c == ')' {
                    break;
                }
            }
        }
        // `[`Type::method`]` and `[text](target)` are rustdoc links. A bare
        // `[defaults]` is a TOML table name and must survive intact.
        if had_target || inner.starts_with('`') {
            out.push_str(&inner);
        } else {
            out.push('[');
            out.push_str(&inner);
            out.push(']');
        }
    }
    out
}

/// Hand-written framing. As with the CLI reference, the prose lives in the
/// generator so the drift check covers the whole page rather than its tables.
const PREAMBLE: &str = r#"
Every key tix reads, generated from the types it parses the files with.

Both documents are TOML, written through a format-preserving layer: only the
value being changed is rewritten, so comments, key order, and plugin tables
survive. Unknown keys are an error rather than being ignored.
"#;

/// Where the global config lives and how its location is resolved.
const GLOBAL_INTRO: &str = r#"
Created by `tix config init`. Located by `--config`, then `TIX_CONFIG_PATH`,
then the platform config directory (`~/.config/tix/config.toml` on Linux,
`~/Library/Application Support/tix/config.toml` on macOS).

Every key below is addressable as `<section>.<key>`:

```sh
tix config get defaults.branch_prefix
tix config set defaults.branch_prefix feature
tix config unset defaults.branch_prefix
```

A value is read as TOML when it parses as one and as a string otherwise.
List keys take an array literal, or one element at a time:

```sh
tix config set defaults.repositories '["backend", "frontend"]'
tix config add defaults.repositories infra
tix config remove defaults.repositories backend
```

`add` creates the list if it does not exist yet and confirms a duplicate
(`--force` skips the question); `remove` takes one element off, front first,
and errors if it is not there. Both keep the array's layout and comments as
written. Keys with a command of their own — `tix repo add` for
`[engine].configured_repositories` — are still easier reached that way.
"#;

/// Where the ticket document lives and what it is for.
const TICKET_INTRO: &str = r#"
One per ticket, at `.tix/ticket.toml` under the ticket root. Its presence is
what makes a directory a ticket. `tix setup`, `tix add`, and `tix remove`
maintain it; it is documented because you will read it.
"#;

/// Plugins own their own tables in both documents.
const PLUGIN_TABLES: &str = r#"
## Plugin tables

Both documents may carry a `[<plugin>]` table per plugin. tix has no schema
for one, and they survive every write it makes. `tix config set
<plugin>.<key>` reaches one like any other section, writing the value as
given — there is no type to check it against. A plugin's configuration is
documented by that plugin.
"#;

/// What `[cli]` is for.
const CLI_INTRO: &str = r#"
Where tix puts things. Both keys are required.
"#;

/// What `[engine]` is for.
const ENGINE_INTRO: &str = r#"
The repositories tix knows about. Maintained by `tix repo add`.
"#;

/// What `[defaults]` is for.
const DEFAULTS_INTRO: &str = r#"
Read **once**, when a ticket or worktree is created, then recorded in the
ticket document — changing one never rewrites an existing ticket. See
[creation-time seeds](../../concepts/seeds/). Every key is optional.
"#;

/// What `[ticket]` is for.
const TICKET_SECTION_INTRO: &str = r#"
The ticket's identity and the worktrees it currently has.
"#;
