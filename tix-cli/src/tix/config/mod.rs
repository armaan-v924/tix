pub mod get;
pub mod init;
pub mod set;
pub mod show;

// ---

use clap::{Args, Subcommand};

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
