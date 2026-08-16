pub mod cli;
pub mod config;
pub mod repo;
pub mod ticket;

pub mod discovery;
pub mod plugin;
pub mod utils;

// ---

use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};

use crate::tix::utils::styles;

#[derive(Parser)]
#[command(name = "tix", version, styles = styles())]
pub struct TixParser {
    /// Set the log level (trace, debug, info, warn, error)
    #[arg(long, global = true)]
    pub log_level: Option<tracing::Level>,

    /// Set the log level to trace (shorthand for --log-level trace)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Set the log level to warn (shorthand for --log-level warn)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

impl TixParser {
    pub fn resolve_log_level(&self) -> tracing::Level {
        let flags = [self.log_level.is_some(), self.verbose, self.quiet];

        if flags.iter().filter(|&&f| f).count() > 1 {
            let mut command = TixParser::command();
            command
                .error(
                    ErrorKind::ArgumentConflict,
                    "Only one of --verbose, --quiet, --log-level may be specified",
                )
                .exit()
        }

        if self.verbose {
            return tracing::Level::TRACE;
        }
        if self.quiet {
            return tracing::Level::WARN;
        }
        self.log_level.unwrap_or(tracing::Level::INFO)
    }
}

#[derive(Subcommand)]
pub enum Commands {
    Cli(crate::tix::cli::CliArgs),
    Ticket(crate::tix::ticket::TicketArgs),
    Config(crate::tix::config::ConfigArgs),
    Repo(crate::tix::repo::RepoArgs),

    Add(crate::tix::ticket::add::Args),
    Destroy(crate::tix::ticket::destroy::Args),
    Info(crate::tix::ticket::info::Args),
    List(crate::tix::ticket::list::Args),
    Remove(crate::tix::ticket::remove::Args),
    Setup(crate::tix::ticket::setup::Args),

    #[command(external_subcommand)]
    External(Vec<String>),
}
