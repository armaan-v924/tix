//! `tix ticket list` — one line per ticket under the tickets directory.

use crate::tix::ticket::load_cli_config;
use crate::tix::utils::OutputType;
use tix_sdk::document::TixDocument;
use tix_sdk::{SdkError, TicketConfig};
use tracing::warn;

/// List all tickets in the tickets directory
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Output format
    #[arg(short, long)]
    pub output: Option<OutputType>,
}

/// Lists every ticket under `tickets_directory`.
///
/// Reads only `[ticket]` sections — no live `Ticket` resolution and no
/// worktree scanning, so listing stays fast on large ticket sets (directory
/// layout is frontend policy; the engine is never involved). Directories
/// without a `.tix/ticket.toml` are silently skipped; one with a malformed
/// document warns and is skipped — a broken ticket never breaks the listing.
pub fn run(app: &crate::tix::utils::App, _args: Args) -> Result<(), SdkError> {
    let cli = load_cli_config(&app.context)?;

    let mut tickets: Vec<TicketConfig> = Vec::new();
    let entries = match std::fs::read_dir(&cli.tickets_directory) {
        Ok(entries) => entries,
        Err(_) => {
            // No tickets directory yet simply means no tickets.
            print_tickets(&tickets, app.output)?;
            return Ok(());
        }
    };
    for entry in entries.filter_map(Result::ok) {
        let ticket_path = entry.path().join(".tix").join("ticket.toml");
        if !ticket_path.is_file() {
            continue;
        }
        let parsed: Result<Option<TicketConfig>, SdkError> =
            TixDocument::load(&ticket_path).and_then(|doc| doc.section("ticket"));
        match parsed {
            Ok(Some(ticket)) => tickets.push(ticket),
            Ok(None) => warn!(path = %ticket_path.display(), "no [ticket] section — skipping"),
            Err(e) => {
                warn!(path = %ticket_path.display(), error = %e, "malformed ticket document — skipping")
            }
        }
    }
    tickets.sort_by(|a, b| a.key.cmp(&b.key));

    print_tickets(&tickets, app.output)?;
    Ok(())
}

/// Renders the listing in the requested format.
fn print_tickets(tickets: &[TicketConfig], output: OutputType) -> Result<(), SdkError> {
    match output {
        OutputType::Default => {
            let width = tickets.iter().map(|t| t.key.len()).max().unwrap_or(0);
            for ticket in tickets {
                if ticket.description.is_empty() {
                    println!("{}", ticket.key);
                } else {
                    println!("{:width$}  {}", ticket.key, ticket.description);
                }
            }
        }
        OutputType::Json => {
            let entries: Vec<serde_json::Value> = tickets
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "key": t.key,
                        "description": t.description,
                        "worktrees": t.worktrees.len(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries).unwrap());
        }
        OutputType::Toml => {
            for ticket in tickets {
                let text = toml::to_string(ticket)?;
                println!("[[ticket]]\n{text}");
            }
        }
    }
    Ok(())
}
