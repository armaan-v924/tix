//! Display ticket information.

use crate::core::config::Config;
use crate::core::ticket::Ticket;
use anyhow::{Result, bail};
use std::env;
use std::path::PathBuf;

/// Run the info command.
pub fn run(ticket: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let ticket_root = locate_ticket_root(ticket, &config)?;

    let ticket_meta = Ticket::load(&ticket_root)?;
    
    let description = ticket_meta
        .metadata
        .description
        .as_deref()
        .unwrap_or("");
    
    println!("[{}] {}", ticket_meta.metadata.id, description);
    
    Ok(())
}

fn locate_ticket_root(ticket: Option<&str>, config: &Config) -> Result<PathBuf> {
    if let Some(id) = ticket {
        return Ok(config.tickets_directory.join(id));
    }

    if let Some(dir) = find_ticket_root_from_cwd() {
        return Ok(dir);
    }

    bail!("Could not infer ticket. Run inside a ticket directory or provide --ticket.");
}

fn find_ticket_root_from_cwd() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        let candidate = current.join(".tix").join("info.toml");
        if candidate.exists() {
            return Some(current);
        }

        if !current.pop() {
            break;
        }
    }
    None
}
