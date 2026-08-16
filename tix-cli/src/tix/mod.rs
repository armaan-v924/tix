pub mod cli;
pub mod config;
pub mod repo;
pub mod ticket;

pub mod plugin;
pub mod plugin_listing;
pub mod utils;

// ---

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, error::ErrorKind};
use std::path::PathBuf;

use crate::tix::utils::styles;

#[derive(Parser)]
#[command(name = "tix", version, styles = styles())]
pub struct TixParser {
    /// Path to the global config file (overrides TIX_CONFIG_PATH and the
    /// platform default)
    #[arg(long, global = true, value_hint = clap::ValueHint::FilePath)]
    pub config: Option<PathBuf>,

    /// Output format for commands that support it (also forwarded to
    /// plugins as --tix-output)
    #[arg(short, long, global = true)]
    pub output: Option<crate::tix::utils::OutputType>,

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
    /// Parses argv, appending the "Plugins" section to **root** help.
    ///
    /// The section is built only when root help is about to render
    /// (`tix -h`, `tix --help`, bare `tix help`) — plugin discovery execs
    /// `print-cli-help` handshakes, which must not run on every invocation.
    /// Subcommand help (`tix repo --help`) is untouched: plugins are
    /// top-level commands only.
    pub fn parse_with_plugin_help() -> Self {
        let mut command = TixParser::command();
        if root_help_requested() {
            if let Some(section) = plugin_listing::plugins_help_section() {
                command = command.after_help(section);
            }
        }
        let mut matches = command.get_matches();
        match TixParser::from_arg_matches_mut(&mut matches) {
            Ok(parsed) => parsed,
            Err(e) => e.format(&mut TixParser::command()).exit(),
        }
    }

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

/// True when this invocation renders the **root** help: the first
/// non-global-flag token is `-h`/`--help`, or it is a bare `help` with no
/// subcommand after it.
fn root_help_requested() -> bool {
    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return true,
            "help" => return args.peek().is_none(),
            // Global flags (and their values) may precede the subcommand.
            "-v" | "--verbose" | "-q" | "--quiet" => continue,
            "--log-level" | "--config" | "-o" | "--output" => {
                args.next();
            }
            _ => return false,
        }
    }
    false
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

    /// Generate shell completions (top-level alias for `tix cli completions`)
    Completions(crate::tix::cli::completions::Args),

    #[command(external_subcommand)]
    External(Vec<String>),
}

impl Commands {
    /// Whether this subcommand requires the global config file to exist.
    ///
    /// Everything does, except the two commands that must be able to run
    /// before a config exists: `tix config init` (creates it) and
    /// `tix cli completions` (touches no config at all).
    pub fn requires_config(&self) -> bool {
        !matches!(
            self,
            Commands::Config(config::ConfigArgs {
                command: config::ConfigCommands::Init(_),
            }) | Commands::Cli(cli::CliArgs {
                command: cli::CliCommands::Completions(_),
            }) | Commands::Completions(_)
        )
    }
}
