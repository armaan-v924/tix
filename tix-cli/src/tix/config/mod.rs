pub mod get;
pub mod init;
pub mod set;
pub mod show;

// ---

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The `[cli]` section of the global config — `tix-cli`'s own settings.
///
/// Owned by the frontend, not the engine (`design/spec.md` §3.2): directory
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
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CliConfig {
    /// Where ticket workspaces are created, and where id-form `--ticket`
    /// arguments resolve (`tickets_directory.join(id)`).
    pub tickets_directory: PathBuf,
    /// Where source repositories are cloned: `tix repo add` derives a new
    /// repo's `code_path` as `code_directory/<alias>` (v2 parity).
    pub code_directory: PathBuf,
}

/// A `[cli]` key addressable by `tix config get`/`set`.
///
/// Being a value enum, unknown keys are rejected at argument-parse time with
/// clap's own diagnostics. New `[cli]` fields grow a variant here alongside
/// their [`CliConfig`] field.
#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum ConfigKey {
    /// `tickets_directory` — where ticket workspaces live.
    #[value(alias = "tickets_directory")]
    TicketsDirectory,
    /// `code_directory` — where source repositories are cloned.
    #[value(alias = "code_directory")]
    CodeDirectory,
}

impl ConfigKey {
    /// The key's name in the `[cli]` table.
    pub fn toml_key(&self) -> &'static str {
        match self {
            ConfigKey::TicketsDirectory => "tickets_directory",
            ConfigKey::CodeDirectory => "code_directory",
        }
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
    Get(get::Args),
    Init(init::Args),
    Set(set::Args),
    Show(show::Args),
}
