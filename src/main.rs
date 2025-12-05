mod core;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use core::cli::{Cli, Commands};
use log::{debug, error};
use std::backtrace::{Backtrace, BacktraceStatus};
use std::process;

fn main() -> Result<()> {
    // 1. Parse Args
    let args = Cli::parse();

    // 2. Setup logging
    let log_level = args.verbose.log_level_filter();
    env_logger::Builder::new().filter_level(log_level).init();

    // 3. Dispatch commands
    let result = match args.command {
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "tix", &mut std::io::stdout());
            Ok(())
        }
        Commands::Add {
            repo,
            ticket,
            branch,
        } => core::commands::add::run(&repo, ticket.as_deref(), branch.as_deref()),
        Commands::AddRepo { repo, alias } => core::commands::add_repo::run(&repo, alias),
        Commands::Config { key, value } => core::commands::config_cmd::run(&key, value.as_deref()),
        Commands::Destroy { ticket, force } => core::commands::destroy::run(&ticket, force),
        Commands::Init => core::commands::init::run(),
        Commands::Remove { repo, ticket } => core::commands::remove::run(&repo, ticket.as_deref()),
        Commands::Setup {
            ticket,
            all,
            repos,
            description,
        } => core::commands::setup::run(&ticket, &repos, all, description),
        Commands::SetupRepos => core::commands::setup_repos::run(),
        Commands::Doctor => core::commands::doctor::run(),
        Commands::Update => core::commands::update::run(),
    };

    if let Err(err) = result {
        error!("{err}");
        debug!("Error details: {err:?}");
        for (idx, cause) in err.chain().skip(1).enumerate() {
            debug!("Caused by {}: {}", idx + 1, cause);
        }
        let bt = err.backtrace();
        let status = bt.status();
        if status != BacktraceStatus::Disabled && status != BacktraceStatus::Unsupported {
            debug!("Backtrace:\n{}", bt);
        } else {
            // Capture a backtrace even if the original error did not.
            let forced = Backtrace::force_capture();
            debug!("Backtrace (captured at exit):\n{}", forced);
        }
        process::exit(1);
    }

    Ok(())
}
