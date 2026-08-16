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
/// ([`crate::tix::document::TixDocument::section`]).
///
/// # Examples
///
/// ```text
/// [cli]
/// tickets_directory = "/home/user/tickets"
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CliConfig {
    /// Where ticket workspaces are created, and where id-form `--ticket`
    /// arguments resolve (`tickets_directory.join(id)`).
    pub tickets_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum ConfigKey {}

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
