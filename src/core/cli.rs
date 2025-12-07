//! Command-line interface definitions for tix.

use clap::builder::Styles;
use clap::builder::styling::AnsiColor;
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};

const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Yellow.on_default().bold())
    .placeholder(AnsiColor::BrightBlack.on_default())
    .error(AnsiColor::Red.on_default().bold());

#[derive(Parser, Debug)]
/// Root CLI parser for tix.
#[command(name = "tix", author, version, about, styles = HELP_STYLES)]
pub struct Cli {
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
/// Supported subcommands for tix.
pub enum Commands {
    /// Add a repository worktree to an existing ticket
    Add {
        /// Repository alias
        repo: String,

        /// Ticket name. If omitted, tries to infer from current directory
        #[arg(short, long)]
        ticket: Option<String>,

        /// Base branch to checkout. Defaults to repo default
        #[arg(short, long)]
        branch: Option<String>,
    },

    /// Register a repository in the configuration
    AddRepo {
        // Repository reference
        /// Formats: "my-repo", "owner/my-repo", or "https://github.com/owner/my-repo"
        repo: String,

        /// Optional alias. Defaults the repo name
        #[arg(short, long)]
        alias: Option<String>,
    },

    /// View or set configuration values
    Config {
        /// The config key (e.g., "git_base_url")
        key: String,

        /// The value to be set. If omitted, shows the current value
        value: Option<String>,
    },

    /// Delete a ticket workspace and its worktrees
    Destroy {
        /// Ticket name
        ticket: String,

        /// Skip confirmation prompts
        #[arg(short, long)]
        force: bool,
    },

    /// Initialize tix configuration interactively
    Init,

    /// Remove a repository worktree from a ticket
    Remove {
        /// Repository alias to remove
        repo: String,

        /// Ticket name. If omitted, inferred from context
        #[arg(short, long)]
        ticket: Option<String>,
    },

    /// Create a new ticket workspace with repository worktrees
    Setup {
        /// Ticket name (e.g., JIRA-123)
        ticket: String,

        /// Optional description
        #[arg(short, long)]
        description: Option<String>,

        /// Clone all configured repositories
        #[arg(short, long)]
        all: bool,

        /// Specific repo aliases to include
        #[arg(num_args(0..))]
        repos: Vec<String>,
    },

    /// Clone all registered repositories
    SetupRepos,

    /// Validate configuration and environment
    Doctor,

    // Req 1: Support shell completions
    /// Generate shell completions
    Completions { shell: clap_complete::Shell },

    /// Check for a newer release and install it
    Update,
}
