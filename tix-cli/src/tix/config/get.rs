//! `tix config get` — read value(s) from anywhere in the config document.

use crate::tix::config::ConfigPath;
use crate::tix::utils::OutputType;
use tix_sdk::SdkError;
use tix_sdk::document::TixDocument;

/// Read a value from the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>[.<key>]` path(s) to read (e.g. `defaults.branch_prefix`)
    #[arg(required = true)]
    pub key: Vec<ConfigPath>,
}

/// Reads each requested path out of the document and prints what is there.
///
/// A single value prints bare — a string without its quotes, everything
/// else as the TOML literal `tix config set` would take back, so a list key
/// round-trips through the two commands. Anything else (several paths, or
/// one naming a whole table) prints as TOML at its real position in the
/// document, `[defaults]` header and all, which is also what makes the JSON
/// shape mirror the file rather than flattening it.
///
/// A path that is valid but unset errors: absent and empty are different,
/// and a script reading a key that was never written should hear about it.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let document = TixDocument::load(&app.context.config_path)?;

    let mut found = Vec::with_capacity(args.key.len());
    for path in &args.key {
        let mut item = document.doc().as_item();
        for segment in path.segments() {
            item = item.get(segment).ok_or_else(|| {
                SdkError::Message(format!(
                    "'{path}' is not set (config: {})",
                    app.context.config_path.display()
                ))
            })?;
        }
        found.push((path, item.clone()));
    }

    // A lone value is the only output that is not a document fragment.
    if let ([(_, item)], OutputType::Default) = (found.as_slice(), app.output)
        && let Some(text) = bare_value(item)
    {
        println!("{text}");
        return Ok(());
    }

    let extract = rebuild(&found)?;
    match app.output {
        // A table cloned out of the document brings its own blank-line
        // prefix; the extract starts at its first line.
        OutputType::Default | OutputType::Toml => {
            print!("{}", extract.to_string().trim_start_matches('\n'))
        }
        OutputType::Json => {
            let value: toml::Value = toml::from_str(&extract.to_string())?;
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| SdkError::Message(format!("json conversion failed: {e}")))?;
            println!("{json}");
        }
    }
    Ok(())
}

/// A value's natural text: a string's own characters, unquoted, and every
/// other value's TOML literal. `None` for a table, which is a piece of
/// document rather than a value and has no bare form.
fn bare_value(item: &toml_edit::Item) -> Option<String> {
    match item.as_value()? {
        toml_edit::Value::String(text) => Some(text.value().to_string()),
        value => Some(value.to_string().trim().to_string()),
    }
}

/// Rebuilds the requested items into a document of their own, each at the
/// path it was read from.
///
/// Reconstructing the position is what keeps the output honest: a table
/// printed on its own would carry sub-headers relative to nothing, and a
/// value would print a dotted key TOML reads back as a different shape.
/// Rebuilt this way, the output is a valid fragment of the config file, and
/// the JSON conversion comes for free by parsing it.
///
/// # Errors
///
/// [`SdkError::Message`] if two requested paths disagree about the document
/// shape (`defaults` and `defaults.branch_prefix.x`, say) — the same
/// non-table diagnostic any nested write raises.
fn rebuild(found: &[(&ConfigPath, toml_edit::Item)]) -> Result<TixDocument, SdkError> {
    let mut extract = TixDocument::empty();
    for (path, item) in found {
        let segments = path.segments();
        let (leaf, parents) = segments.split_last().expect("non-empty by construction");
        extract.table_at(parents)?.insert(leaf, item.clone());
    }
    Ok(extract)
}
