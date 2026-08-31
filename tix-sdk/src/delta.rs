//! Diff-back config deltas (`design/spec.md` §6).
//!
//! Diff-back exists **only** because config has a single-writer constraint —
//! the host. It is not an RPC channel: everything else a plugin does
//! (worktrees, syncing, resolution) goes directly through `tix-sdk →
//! tix-engine` in-process. A plugin that wants config changed writes a
//! [`Delta`] into its `--tix-delta` file; the host applies it after a clean
//! exit, against a **fresh parse** of the target document.
//!
//! Formats are asymmetric by design: inbound config is TOML (real files;
//! `tomllib` is Python stdlib ≥3.11), the outbound delta is JSON (Python has
//! no stdlib TOML *writer*). The JSON→TOML value mapping rides the JSON
//! text form — `1` stays an integer, `1.0` a float — with one tagged escape
//! hatch for the single inexpressible type:
//! `{"$datetime": "2026-07-19T09:00:00Z"}`.

use crate::document::TixDocument;
use crate::error::SdkError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Which document a delta targets. Exactly two exist, so no provenance
/// tracking is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeltaTarget {
    /// The global config file.
    Global,
    /// The ticket document (`.tix/ticket.toml`).
    Ticket,
}

/// One delta operation: put `value` at the dotted key path `set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeltaOp {
    /// Dotted path into the target document, e.g. `myplugin.branch`.
    pub set: String,
    /// The value, in JSON. Mapped to TOML by text form at apply time.
    pub value: serde_json::Value,
}

/// A config delta: a target document plus an ordered op list.
///
/// Ops are ordered; overlapping keys are last-writer-wins.
///
/// # Examples
///
/// The plugin side — build and write into the host's `--tix-delta` file:
///
/// ```no_run
/// # use tix_sdk::delta::{Delta, DeltaTarget};
/// # fn main() -> Result<(), tix_sdk::SdkError> {
/// # let host = tix_sdk::host::HostContext::from_env()?;
/// let delta = Delta::new(DeltaTarget::Ticket)
///     .set("myplugin.branch", "main")?
///     .set("myplugin.retries", 3)?;
/// delta.write_to(host.delta_path.as_deref().expect("host always passes --tix-delta"))?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    /// The document the ops apply to.
    pub target: DeltaTarget,
    /// Ordered operations; overlapping keys last-writer-wins.
    pub ops: Vec<DeltaOp>,
}

impl Delta {
    /// An empty delta against `target`.
    pub fn new(target: DeltaTarget) -> Self {
        Self {
            target,
            ops: Vec::new(),
        }
    }

    /// Appends a set op. `value` is anything serializable; typed datetimes
    /// are the caller's job to tag (see [`Self::set_datetime`]).
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] if `value` does not serialize to JSON.
    pub fn set(mut self, path: &str, value: impl Serialize) -> Result<Self, SdkError> {
        let value = serde_json::to_value(value)
            .map_err(|e| SdkError::Message(format!("unserializable delta value: {e}")))?;
        self.ops.push(DeltaOp {
            set: path.to_string(),
            value,
        });
        Ok(self)
    }

    /// Appends a set op whose value is a TOML datetime, using the tagged
    /// form (`{"$datetime": …}`) — the one type JSON cannot express.
    pub fn set_datetime(mut self, path: &str, datetime: &str) -> Self {
        self.ops.push(DeltaOp {
            set: path.to_string(),
            value: serde_json::json!({ "$datetime": datetime }),
        });
        self
    }

    /// Serializes into the `--tix-delta` file. No file written ⇒ no changes,
    /// so plugins with nothing to say simply never call this.
    ///
    /// # Errors
    ///
    /// IO errors from the write.
    pub fn write_to(&self, path: &Path) -> Result<(), SdkError> {
        let json = serde_json::to_string(self)
            .map_err(|e| SdkError::Message(format!("could not serialize delta: {e}")))?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Parses a delta the host read back from the `--tix-delta` file.
    ///
    /// # Errors
    ///
    /// [`SdkError::PluginImplementation`] — a malformed delta is a bug in
    /// the plugin, reported as such, and nothing gets written.
    pub fn parse(bytes: &[u8]) -> Result<Self, SdkError> {
        serde_json::from_slice(bytes)
            .map_err(|e| SdkError::PluginImplementation(format!("malformed config delta: {e}")))
    }

    /// Applies every op to `document` as path traversals over the
    /// format-preserving DOM.
    ///
    /// The host **never deserializes plugin tables** — a typed round-trip
    /// would strip every `[<plugin>]` table plus comments (spec §6.2). Ops
    /// navigate to the dotted path, creating intermediate tables as needed,
    /// and put the mapped value there; untouched subtrees are never
    /// interpreted. Ordered application makes overlapping keys
    /// last-writer-wins.
    ///
    /// Revalidation of host-owned sections is the caller's job — the host
    /// has the types, this module has the mechanics.
    ///
    /// # Errors
    ///
    /// [`SdkError::PluginImplementation`] for an empty path, an unmappable
    /// value, or a path that runs through a key the document already holds a
    /// non-table under.
    pub fn apply_ops(&self, document: &mut TixDocument) -> Result<(), SdkError> {
        for op in &self.ops {
            let segments: Vec<&str> = op.set.split('.').collect();
            if segments.iter().any(|segment| segment.is_empty()) {
                return Err(SdkError::PluginImplementation(format!(
                    "empty path segment in delta op '{}'",
                    op.set
                )));
            }
            let (leaf, tables) = segments.split_last().expect("split never yields empty");

            // Missing levels materialize as real tables, so a nested op
            // appends an `[a.b]` section — bare indexing would leave an
            // inline `a = { b.key = … }` at the top of the document instead
            // (#146).
            let table = crate::document::table_at(document.doc_mut().as_item_mut(), tables)
                .map_err(|e| {
                    SdkError::PluginImplementation(format!("delta op '{}': {e}", op.set))
                })?;
            table[leaf] = toml_edit::Item::Value(json_to_toml_value(&op.value, &op.set)?);
        }
        Ok(())
    }
}

/// Maps a JSON value to a TOML value by its text form: `1` → integer,
/// `1.0` → float, string/bool direct, arrays and objects recurse, and the
/// tagged `{"$datetime": "…"}` form becomes a native TOML datetime.
fn json_to_toml_value(value: &serde_json::Value, path: &str) -> Result<toml_edit::Value, SdkError> {
    use serde_json::Value as Json;
    Ok(match value {
        Json::String(s) => toml_edit::Value::from(s.as_str()),
        Json::Bool(b) => toml_edit::Value::from(*b),
        // serde_json preserves the JSON text distinction: is_i64 for `1`,
        // f64 for `1.0`.
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml_edit::Value::from(i)
            } else if let Some(f) = n.as_f64() {
                toml_edit::Value::from(f)
            } else {
                return Err(SdkError::PluginImplementation(format!(
                    "unrepresentable number {n} in delta op '{path}'"
                )));
            }
        }
        Json::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(json_to_toml_value(item, path)?);
            }
            toml_edit::Value::Array(array)
        }
        Json::Object(map) => {
            // The one tagged form: a single-key {"$datetime": "..."} object.
            if let Some(Json::String(datetime)) = map.get("$datetime")
                && map.len() == 1
            {
                let parsed: toml_edit::Datetime = datetime.parse().map_err(|e| {
                    SdkError::PluginImplementation(format!(
                        "invalid $datetime '{datetime}' in delta op '{path}': {e}"
                    ))
                })?;
                toml_edit::Value::from(parsed)
            } else {
                let mut table = toml_edit::InlineTable::new();
                for (key, item) in map {
                    table.insert(key, json_to_toml_value(item, path)?);
                }
                toml_edit::Value::InlineTable(table)
            }
        }
        Json::Null => {
            return Err(SdkError::PluginImplementation(format!(
                "null is not representable in TOML (delta op '{path}')"
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The round-trip a plugin performs: build → write → parse.
    #[test]
    fn test_write_parse_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delta.json");
        let delta = Delta::new(DeltaTarget::Ticket)
            .set("myplugin.branch", "main")
            .unwrap()
            .set("myplugin.retries", 3)
            .unwrap();
        delta.write_to(&path).unwrap();

        let parsed = Delta::parse(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed, delta);
    }

    /// The wire format matches the spec example.
    #[test]
    fn test_wire_format() {
        let delta = Delta::new(DeltaTarget::Ticket)
            .set("myplugin.branch", "main")
            .unwrap();
        assert_eq!(
            serde_json::to_string(&delta).unwrap(),
            r#"{"target":"ticket","ops":[{"set":"myplugin.branch","value":"main"}]}"#
        );
    }

    /// JSON text form drives the TOML type: 1 integer, 1.0 float; datetime
    /// via the tagged form; arrays and tables recurse.
    #[test]
    fn test_json_to_toml_mapping() {
        let delta = Delta::parse(
            br#"{"target":"global","ops":[
                {"set":"p.int","value":1},
                {"set":"p.float","value":1.0},
                {"set":"p.text","value":"hi"},
                {"set":"p.flag","value":true},
                {"set":"p.list","value":[1,"two"]},
                {"set":"p.table","value":{"a":1}},
                {"set":"p.when","value":{"$datetime":"2026-07-19T09:00:00Z"}}
            ]}"#,
        )
        .unwrap();
        let mut document = TixDocument::empty();
        delta.apply_ops(&mut document).unwrap();
        let text = document.to_string();
        assert!(text.contains("int = 1"), "{text}");
        assert!(text.contains("float = 1.0"), "{text}");
        assert!(text.contains(r#"text = "hi""#), "{text}");
        assert!(text.contains("flag = true"), "{text}");
        assert!(text.contains(r#"list = [1, "two"]"#), "{text}");
        assert!(text.contains("when = 2026-07-19T09:00:00Z"), "{text}");
    }

    /// Applying against an existing document touches only the addressed
    /// keys — comments and foreign tables survive.
    #[test]
    fn test_apply_preserves_untouched_content() {
        let source =
            "# keep this comment\n[engine]\n\n[myplugin]\nbranch = \"old\" # inline\nkeep = 1\n";
        let mut document = TixDocument::parse(source).unwrap();
        let delta = Delta::new(DeltaTarget::Global)
            .set("myplugin.branch", "new")
            .unwrap();
        delta.apply_ops(&mut document).unwrap();

        let text = document.to_string();
        assert!(text.contains("# keep this comment"));
        assert!(text.contains("keep = 1"));
        assert!(text.contains(r#"branch = "new""#));
    }

    /// A section created by a delta renders as an appended `[table]`, after
    /// existing content — never an inline `a = { … }` at the document top.
    #[test]
    fn test_new_section_appends_as_table() {
        let mut document = TixDocument::parse("[ticket]\nkey = \"JIRA-1\"\n").unwrap();
        let delta = Delta::new(DeltaTarget::Ticket)
            .set("myplugin.branch", "main")
            .unwrap();
        delta.apply_ops(&mut document).unwrap();
        let text = document.to_string();
        assert!(
            text.starts_with("[ticket]"),
            "existing content stays first:\n{text}"
        );
        assert!(text.contains("[myplugin]"), "header table form:\n{text}");
    }

    /// Overlapping keys are last-writer-wins, in op order.
    #[test]
    fn test_last_writer_wins() {
        let mut document = TixDocument::empty();
        let delta = Delta::new(DeltaTarget::Global)
            .set("p.key", "first")
            .unwrap()
            .set("p.key", "second")
            .unwrap();
        delta.apply_ops(&mut document).unwrap();
        assert!(document.to_string().contains(r#"key = "second""#));
    }

    /// Malformed JSON and unrepresentable values are plugin bugs.
    #[test]
    fn test_malformed_is_plugin_implementation_error() {
        assert!(matches!(
            Delta::parse(b"{not json"),
            Err(SdkError::PluginImplementation(_))
        ));
        let delta =
            Delta::parse(br#"{"target":"global","ops":[{"set":"p.x","value":null}]}"#).unwrap();
        assert!(matches!(
            delta.apply_ops(&mut TixDocument::empty()),
            Err(SdkError::PluginImplementation(_))
        ));
    }
}
