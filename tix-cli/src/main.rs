mod tix;

use crate::tix::utils::App;
use tix_sdk::context::Context;
use crate::tix::{Commands, cli, config, plugin, repo, ticket};
use tracing_subscriber::fmt;

/// Prints a CLI error to stderr and exits nonzero.
///
/// The frontend owns process control and user-facing output — the engine
/// only ever returns errors.
fn fail(error: tix_sdk::SdkError) -> ! {
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
    let app = App {
        context,
        output: parsed.output.unwrap_or(tix::utils::OutputType::Default),
        log_level,
    };

    let result = match parsed.command {
        Commands::Cli(args) => match args.command {
            cli::CliCommands::Completions(args) => cli::completions::run(&app, args),
            cli::CliCommands::Update(args) => cli::update::run(&app, args),
        },
        Commands::Config(args) => match args.command {
            config::ConfigCommands::Get(args) => config::get::run(&app, args),
            config::ConfigCommands::Init(args) => config::init::run(&app, args),
            config::ConfigCommands::Set(args) => config::set::run(&app, args),
            config::ConfigCommands::Show(args) => config::show::run(&app, args),
        },
        Commands::Repo(args) => match args.command {
            repo::RepoCommands::Add(args) => repo::add::run(&app, args),
            repo::RepoCommands::Clone(args) => repo::clone::run(&app, args),
        },
        Commands::Ticket(args) => match args.command {
            ticket::TicketCommands::Add(args) => ticket::add::run(&app, args),
            ticket::TicketCommands::Destroy(args) => ticket::destroy::run(&app, args),
            ticket::TicketCommands::Info(args) => ticket::info::run(&app, args),
            ticket::TicketCommands::List(args) => ticket::list::run(&app, args),
            ticket::TicketCommands::Remove(args) => ticket::remove::run(&app, args),
            ticket::TicketCommands::Setup(args) => ticket::setup::run(&app, args),
        },
        Commands::Add(args) => ticket::add::run(&app, args),
        Commands::Destroy(args) => ticket::destroy::run(&app, args),
        Commands::Info(args) => ticket::info::run(&app, args),
        Commands::List(args) => ticket::list::run(&app, args),
        Commands::Remove(args) => ticket::remove::run(&app, args),
        Commands::Setup(args) => ticket::setup::run(&app, args),
        Commands::Completions(args) => cli::completions::run(&app, args),

        Commands::External(args) => plugin::run(&app, args),
    };

    if let Err(e) = result {
        fail(e);
    }
}
