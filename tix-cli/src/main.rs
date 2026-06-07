mod tix;

use crate::tix::{Commands, cli, config, plugin, repo, ticket};
use clap::Parser;
use tracing_subscriber::fmt;

fn main() {
    let parsed = tix::TixParser::parse();
    let log_level = parsed.resolve_log_level();

    fmt()
        .with_max_level(log_level)
        .with_writer(std::io::stdout)
        .init();

    match parsed.command {
        Commands::Cli(args) => match args.command {
            cli::CliCommands::Completions(args) => cli::completions::run(args),
            cli::CliCommands::Update(args) => cli::update::run(args),
        },
        Commands::Config(args) => match args.command {
            config::ConfigCommands::Get(args) => config::get::run(args),
            config::ConfigCommands::Init(args) => config::init::run(args),
            config::ConfigCommands::Set(args) => config::set::run(args),
            config::ConfigCommands::Show(args) => config::show::run(args),
        },
        Commands::Repo(args) => match args.command {
            repo::RepoCommands::Add(args) => repo::add::run(args),
            repo::RepoCommands::Clone(args) => repo::clone::run(args),
        },
        Commands::Ticket(args) => match args.command {
            ticket::TicketCommands::Add(args) => ticket::add::run(args),
            ticket::TicketCommands::Destroy(args) => ticket::destroy::run(args),
            ticket::TicketCommands::Info(args) => ticket::info::run(args),
            ticket::TicketCommands::List(args) => ticket::list::run(args),
            ticket::TicketCommands::Remove(args) => ticket::remove::run(args),
            ticket::TicketCommands::Setup(args) => ticket::setup::run(args),
        },
        Commands::Add(args) => ticket::add::run(args),
        Commands::Destroy(args) => ticket::destroy::run(args),
        Commands::Info(args) => ticket::info::run(args),
        Commands::List(args) => ticket::list::run(args),
        Commands::Remove(args) => ticket::remove::run(args),
        Commands::Setup(args) => ticket::setup::run(args),

        Commands::External(args) => plugin::run(args),
    }
}
