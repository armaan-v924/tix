mod core;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use core::cli::{Cli, Commands};

fn main() -> Result<()> {
    // 1. Parse Args
    let args = Cli::parse();

    // 2. Setup logging
    env_logger::Builder::new()
        .filter_level(args.verbose.log_level_filter())
        .init();

    // 3. Dispatch commands
    match args.command {
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "tix", &mut std::io::stdout());
        }
        Commands::Add {
            repo,
            ticket,
            branch,
        } => {
            core::commands::add::run(&repo, ticket.as_deref(), branch.as_deref())?;
        }
        Commands::AddRepo { repo, alias } => {
            core::commands::add_repo::run(&repo, alias)?;
        }
        Commands::Config { key, value } => {
            core::commands::config_cmd::run(&key, value.as_deref())?;
        }
        Commands::Destroy { ticket, force } => {
            core::commands::destroy::run(&ticket, force)?;
        }
        Commands::Init => {
            core::commands::init::run()?;
        }
        Commands::Remove { repo, ticket } => {
            core::commands::remove::run(&repo, ticket.as_deref())?;
        }
        Commands::Setup {
            ticket,
            all,
            repos,
            description,
        } => {
            core::commands::setup::run(&ticket, &repos, all, description)?;
        }
        Commands::SetupRepos => {
            core::commands::setup_repos::run()?;
        }
        Commands::Doctor => {
            core::commands::doctor::run()?;
        }
    }

    Ok(())
}
