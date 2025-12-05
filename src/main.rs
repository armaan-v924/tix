mod cli;
mod commands;
mod config;
mod git;
mod ticket;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Cli, Commands};

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
            ticekt,
            branch,
        } => todo!(),
        Commands::AddRepo { repo, alias } => todo!(),
        Commands::Config { key, value } => todo!(),
        Commands::Destroy { ticket, force } => todo!(),
        Commands::Init => todo!(),
        Commands::Remove { repo, ticket } => todo!(),
        Commands::Setup {
            ticket,
            all,
            repos,
            description,
        } => {
            commands::setup::run(&ticket, &repos, all, description)?;
        }
        Commands::SetupRepos => todo!(),
    }

    Ok(())
}
