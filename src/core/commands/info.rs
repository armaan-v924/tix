//! Display ticket information.

use crate::core::commands::common::locate_ticket_root;
use crate::core::config::Config;
use crate::core::ticket::Ticket;
use anyhow::Result;

/// Run the info command.
pub fn run(ticket: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let ticket_root = locate_ticket_root(ticket, &config)?;

    let ticket_meta = Ticket::load(&ticket_root)?;

    let description = ticket_meta.metadata.description.as_deref().unwrap_or("");

    println!("[{}] {}", ticket_meta.metadata.id, description);

    Ok(())
}
