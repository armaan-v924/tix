use clap::{Parser, Subcommand};
use clap_verbosity_flag::Verbosity;

#[derive(Parser, Debug)]
#[command(name = "tix", author, version, about)]
pub struct Cli {
    #[command(flatten)]
    pub verbose: Verbosity,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    // Add a repo to an existing ticket
    Add {
        /// Repository alias
        repo: String,

        /// Ticket name. If omitted, tries to infer from current directory
        #[arg(short, long)]
        ticekt: Option<String>,

        /// Base branch to checkout. Defaults to repo default
        #[arg(short, long)]
        branch: Option<String>,
    },

    AddRepo {
        // Repository reference
        /// Formats: "my-repo", "owner/my-repo", or "https://github.com/owner/my-repo"
        repo: String,

        /// Optional aalias. Defaults the repo name
        #[arg(short, long)]
        alias: Option<String>,
    },

    Config {
        /// The config key (e.g., "git_base_url")
        key: String,

        // The value to be set. If omitted, shows the current value
        value: Option<String>,
    },

    Destroy {
        /// Ticket name
        ticket: String,

        /// Skip confirmation prompts
        #[arg(short, long)]
        force: bool,
    },

    Init,

    Remove {
        /// Repository alias to remove
        repo: String,

        /// Ticket name. If ommitted, inferred from context
        #[arg(short, long)]
        ticket: Option<String>,
    },

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

    SetupRepos,

    // Req 1: Support shell completions
    /// Generate shell completions
    Completions {
        shell: clap_complete::Shell,
    },
}
