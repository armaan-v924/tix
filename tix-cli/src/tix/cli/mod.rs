pub mod completions;
pub mod update;

// ---

use clap::{Args, Subcommand};

#[derive(Args)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: CliCommands,
}

#[derive(Subcommand)]
pub enum CliCommands {
    Completions(completions::Args),
    Update(update::Args),
}
