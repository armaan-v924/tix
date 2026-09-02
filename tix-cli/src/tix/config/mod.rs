pub mod add;
mod edit;
pub mod get;
pub mod init;
pub mod remove;
pub mod set;
pub mod show;
pub mod unset;

// ---

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use tix_sdk::SdkError;
use tix_sdk::document::TixDocument;

/// The `[cli]` section of the global config — `tix-cli`'s own settings.
///
/// Owned by the frontend rather than the engine
/// ([configuration](https://tix.armaanv.dev/latest/reference/configuration/)): directory
/// layout is frontend policy, and the engine has no `tickets_directory`.
/// Extracted from the parsed document via the section accessors
/// ([`tix_sdk::document::TixDocument::section`]).
///
/// # Examples
///
/// ```text
/// [cli]
/// tickets_directory = "/home/user/tickets"
/// code_directory = "/home/user/code"
/// ```
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CliConfig {
    /// Where key-form `tix ticket setup` creates ticket workspaces, and where
    /// id-form `--ticket` arguments resolve (`tickets_directory.join(id)`).
    /// The default location, not a bound: a path-form `tix ticket setup`
    /// creates outside it, and discovery finds tickets wherever they are.
    pub tickets_directory: PathBuf,
    /// Where source repositories are cloned: `tix repo add` derives a new
    /// repo's `code_path` as `code_directory/<alias>` (v2 parity).
    pub code_directory: PathBuf,
}

/// A dotted path addressing a place in the config document: a section
/// (`defaults`), a key in one (`defaults.branch_prefix`), or anything
/// deeper (`engine.configured_repositories.backend.remote`).
///
/// This replaces the two-variant `[cli]` value enum the config commands
/// started with (#137). Every section is addressable, plugin tables
/// included: anything that lives in the document is the user's to read and
/// write, and a section having its own dedicated command (`tix repo add`
/// for `[engine]`) makes that command the *primary* path to it, not the
/// only one.
///
/// What that costs is clap's parse-time diagnostic for an unknown key. The
/// path's *shape* is still rejected at parse time; whether the key it names
/// exists is settled later, against the type that owns the section
/// ([`validate_section`]) — or, for a plugin table, not at all.
///
/// A dot always separates segments, so a TOML key containing a literal dot
/// is unreachable. Nothing tix owns has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPath {
    /// At least one, none of them empty — the invariant [`FromStr`]
    /// establishes.
    segments: Vec<String>,
}

impl ConfigPath {
    /// The section the path addresses: its first segment.
    pub fn section(&self) -> &str {
        &self.segments[0]
    }

    /// The segments, borrowed for the SDK's `&[&str]` traversal APIs.
    pub fn segments(&self) -> Vec<&str> {
        self.segments.iter().map(String::as_str).collect()
    }
}

impl FromStr for ConfigPath {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let segments: Vec<String> = text.split('.').map(str::to_string).collect();
        if segments.iter().any(String::is_empty) {
            return Err(format!(
                "'{text}' is not a config path — expected `<section>[.<key>]`, \
                 with no empty segments"
            ));
        }
        Ok(Self { segments })
    }
}

impl fmt::Display for ConfigPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.segments.join("."))
    }
}

/// A [`ConfigPath`] that addresses a *key*, not a whole section — at least
/// `<section>.<key>`.
///
/// The distinction is the write path's: a document is a set of sections
/// (one table per consumer), so a bare `defaults` names a table, and
/// assigning a value over it would be a document tix cannot parse back.
/// Being a separate type, `tix config set defaults x` is refused by clap
/// before the command runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigKeyPath(ConfigPath);

impl ConfigKeyPath {
    /// The section the key lives in.
    pub fn section(&self) -> &str {
        self.0.section()
    }

    /// The table holding the key: every segment but the last. For the
    /// common `<section>.<key>` that is the section table alone; a deeper
    /// path names the tables between, which
    /// [`tix_sdk::document::TixDocument::table_at`] materializes as real
    /// (headered) tables on the way down.
    pub fn table_path(&self) -> Vec<&str> {
        let segments = self.0.segments();
        segments[..segments.len() - 1].to_vec()
    }

    /// Every segment, the key included — the whole path, for a traversal
    /// that does not care where the table ends and the key begins.
    pub fn segments(&self) -> Vec<&str> {
        self.0.segments()
    }

    /// The key itself: the last segment.
    pub fn leaf(&self) -> &str {
        self.0.segments.last().expect("non-empty by construction")
    }
}

impl FromStr for ConfigKeyPath {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let path: ConfigPath = text.parse()?;
        if path.segments.len() < 2 {
            return Err(format!(
                "'{text}' names a whole section — expected `<section>.<key>`, \
                 e.g. `defaults.branch_prefix`"
            ));
        }
        Ok(Self(path))
    }
}

impl fmt::Display for ConfigKeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Re-deserializes `section` into the type that owns it, so an edit that
/// broke it fails before anything reaches disk.
///
/// Only the sections tix itself models are checked. A `[<plugin>]` table
/// has no type here by design — tix has no schema for one — so a path into
/// one is written as given; the plugin is what reads it back.
///
/// # Errors
///
/// [`SdkError::Message`] with the deserialization diagnostic when the
/// edited section no longer parses: an unknown key (every section type is
/// `deny_unknown_fields`), or a value of the wrong type.
pub fn validate_section(document: &TixDocument, section: &str) -> Result<(), SdkError> {
    match section {
        "cli" => document.section::<CliConfig>(section).map(|_| ()),
        "engine" => document
            .section::<tix_sdk::EngineConfig>(section)
            .map(|_| ()),
        "defaults" => document.section::<tix_sdk::Defaults>(section).map(|_| ()),
        _ => Ok(()),
    }
}

/// Manage the global tix config
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommands,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    Add(add::Args),
    Get(get::Args),
    Init(init::Args),
    Remove(remove::Args),
    Set(set::Args),
    Show(show::Args),
    Unset(unset::Args),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section on its own is a valid read path.
    #[test]
    fn test_path_accepts_bare_section() {
        let path: ConfigPath = "defaults".parse().unwrap();
        assert_eq!(path.section(), "defaults");
        assert_eq!(path.segments(), vec!["defaults"]);
    }

    /// Paths are split on every dot, to any depth.
    #[test]
    fn test_path_splits_every_segment() {
        let path: ConfigPath = "engine.configured_repositories.backend.remote"
            .parse()
            .unwrap();
        assert_eq!(path.section(), "engine");
        assert_eq!(
            path.segments(),
            vec!["engine", "configured_repositories", "backend", "remote"]
        );
        assert_eq!(
            path.to_string(),
            "engine.configured_repositories.backend.remote"
        );
    }

    /// An empty segment — a leading, trailing, or doubled dot — is rejected,
    /// as is the empty string.
    #[test]
    fn test_path_rejects_empty_segments() {
        for text in ["", ".", "defaults.", ".defaults", "a..b"] {
            assert!(
                text.parse::<ConfigPath>().is_err(),
                "'{text}' should not parse"
            );
        }
    }

    /// A write path must reach a key inside a section.
    #[test]
    fn test_key_path_rejects_bare_section() {
        assert!("defaults".parse::<ConfigKeyPath>().is_err());
    }

    /// The holder of a key two segments deep is the section table itself,
    /// which `table_at` addresses as the empty path.
    #[test]
    fn test_key_path_splits_section_and_key() {
        let key: ConfigKeyPath = "defaults.branch_prefix".parse().unwrap();
        assert_eq!(key.section(), "defaults");
        assert_eq!(key.table_path(), vec!["defaults"]);
        assert_eq!(key.leaf(), "branch_prefix");
    }

    /// A deeper key nests under the tables between its section and itself.
    #[test]
    fn test_key_path_splits_nested_key() {
        let key: ConfigKeyPath = "engine.configured_repositories.backend.remote"
            .parse()
            .unwrap();
        assert_eq!(key.section(), "engine");
        assert_eq!(
            key.table_path(),
            vec!["engine", "configured_repositories", "backend"]
        );
        assert_eq!(key.leaf(), "remote");
    }
}
