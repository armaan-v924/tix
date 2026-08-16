mod tix;

use crate::tix::context::Context;
use crate::tix::{Commands, cli, config, plugin, repo, ticket};
use clap::Parser;
use tracing_subscriber::fmt;

/// Prints a CLI error to stderr and exits nonzero.
///
/// The frontend owns process control and user-facing output — the engine
/// only ever returns errors.
fn fail(error: tix_engine::TixError) -> ! {
    eprintln!("error: {error}");
    std::process::exit(1);
}

fn main() {
    let parsed = tix::TixParser::parse_with_plugin_help();
    let log_level = parsed.resolve_log_level();

    // Diagnostics go to stderr: stdout carries results only, so
    // `tix ticket list --output json | jq` works even when warnings fire.
    fmt()
        .with_max_level(log_level)
        .with_writer(std::io::stderr)
        .init();

    // Resolve the config path once (--config > TIX_CONFIG_PATH > platform
    // default) and hand it to every subcommand. Existence is enforced here,
    // centrally, for every command except the two that must run before a
    // config exists (`tix config init`, `tix cli completions`).
    let context = Context::resolve(parsed.config.clone()).unwrap_or_else(|e| fail(e));
    if parsed.command.requires_config()
        && let Err(e) = context.require_config_path()
    {
        fail(e);
    }

    let result = match parsed.command {
        Commands::Cli(args) => match args.command {
            cli::CliCommands::Completions(args) => cli::completions::run(&context, args),
            cli::CliCommands::Update(args) => cli::update::run(&context, args),
        },
        Commands::Config(args) => match args.command {
            config::ConfigCommands::Get(args) => config::get::run(&context, args),
            config::ConfigCommands::Init(args) => config::init::run(&context, args),
            config::ConfigCommands::Set(args) => config::set::run(&context, args),
            config::ConfigCommands::Show(args) => config::show::run(&context, args),
        },
        Commands::Repo(args) => match args.command {
            repo::RepoCommands::Add(args) => repo::add::run(&context, args),
            repo::RepoCommands::Clone(args) => repo::clone::run(&context, args),
        },
        Commands::Ticket(args) => match args.command {
            ticket::TicketCommands::Add(args) => ticket::add::run(&context, args),
            ticket::TicketCommands::Destroy(args) => ticket::destroy::run(&context, args),
            ticket::TicketCommands::Info(args) => ticket::info::run(&context, args),
            ticket::TicketCommands::List(args) => ticket::list::run(&context, args),
            ticket::TicketCommands::Remove(args) => ticket::remove::run(&context, args),
            ticket::TicketCommands::Setup(args) => ticket::setup::run(&context, args),
        },
        Commands::Add(args) => ticket::add::run(&context, args),
        Commands::Destroy(args) => ticket::destroy::run(&context, args),
        Commands::Info(args) => ticket::info::run(&context, args),
        Commands::List(args) => ticket::list::run(&context, args),
        Commands::Remove(args) => ticket::remove::run(&context, args),
        Commands::Setup(args) => ticket::setup::run(&context, args),
        Commands::Completions(args) => cli::completions::run(&context, args),

        Commands::External(args) => plugin::run(&context, args),
    };

    if let Err(e) = result {
        fail(e);
    }
}
