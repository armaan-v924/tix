mod core;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use core::cli::{Cli, Commands, PluginCommands};
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
            // For zsh, we need to modify the output to work with eval
            if shell == clap_complete::Shell::Zsh {
                let mut buffer = Vec::new();
                clap_complete::generate(shell, &mut cmd, "tix", &mut buffer);
                let completion_script = String::from_utf8(buffer)
                    .context("Failed to generate valid UTF-8 completion script")?;

                // Replace #compdef directive with a comment to make it eval-friendly
                // Process line by line to handle the first line robustly
                let modified_script = completion_script
                    .lines()
                    .enumerate()
                    .map(|(i, line)| {
                        if i == 0 && line.starts_with("#compdef") {
                            // Add a space after # to make it a regular comment
                            line.replacen("#compdef", "# compdef", 1)
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                println!("{}", modified_script);
            } else {
                clap_complete::generate(shell, &mut cmd, "tix", &mut std::io::stdout());
            }
            Ok(())
        }
        Commands::Add {
            repo,
            ticket,
            branch,
        } => core::commands::add::run(&repo, ticket.as_deref(), branch.as_deref()),
        Commands::AddRepo { repo, alias } => core::commands::add_repo::run(&repo, alias),
        Commands::Config { key, value, edit } => {
            core::commands::config_cmd::run(key.as_deref(), value.as_deref(), edit)
        }
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
        Commands::Info { ticket } => core::commands::info::run(ticket.as_deref()),
        Commands::List => core::commands::list::run(),
        Commands::Plugins { command } => match command {
            PluginCommands::List => core::commands::plugins::list(),
            PluginCommands::Register {
                name,
                entrypoint,
                description,
                python,
            } => core::commands::plugins::register(
                &name,
                &entrypoint,
                description.as_deref(),
                python.as_deref(),
            ),
            PluginCommands::Deregister { name } => core::commands::plugins::deregister(&name),
            PluginCommands::Clean { name } => core::commands::plugins::clean(name.as_deref()),
        },
        Commands::Plugin(args) => core::plugins::run_external(args),
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
