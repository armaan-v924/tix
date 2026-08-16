//! `tix ticket remove` — remove single worktrees from a ticket without
//! destroying it.

use crate::tix::context::Context;
use crate::tix::document::{TixDocument, with_write};
use crate::tix::ticket::{TicketSharedArgs, load_ticket_config, require_ticket_root};
use tix_engine::{EngineConfig, TixError};
use tracing::info;

/// Arguments for `tix ticket remove`.
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// Worktree name(s) from the ticket's worktree map (a bare repo alias
    /// matches its default worktree)
    #[arg(required = true, value_name = "NAME")]
    pub names: Vec<String>,
}

/// Prunes the named worktrees and deletes their entries from the ticket
/// document.
///
/// `<name>` addresses the **worktree directory name** — the ticket map's key
/// — not a repo alias: with #85 a repo can have several worktrees, so the
/// alias alone is not an address. In the single-worktree case they coincide
/// (`name == alias`), which is what makes bare-alias usage work today.
///
/// The ticket directory and every other worktree are untouched; removing
/// the last worktree leaves a valid, empty ticket. Each successful prune is
/// written back immediately through the format-preserving layer.
pub fn run(context: &Context, args: Args) -> Result<(), TixError> {
    let root = require_ticket_root(context, args.shared.ticket.as_ref())?;
    let ticket = load_ticket_config(&root)?;

    let document = TixDocument::load(&context.config_path)?;
    let engine: EngineConfig = document.section_or_default("engine")?;

    // Validate the batch up front: every name must be tracked, and every
    // backing repo must still be registered (pruning goes through the repo).
    let mut tracked: Vec<&str> = ticket.worktrees.keys().map(String::as_str).collect();
    tracked.sort();
    for name in &args.names {
        let entry = ticket.worktrees.get(name).ok_or_else(|| {
            TixError::WorktreeNotFound(format!(
                "'{}' is not a tracked worktree of ticket '{}' (tracked: {})",
                name,
                ticket.key,
                if tracked.is_empty() { "none".to_string() } else { tracked.join(", ") }
            ))
        })?;
        if !engine.configured_repositories.contains_key(&entry.repo) {
            return Err(TixError::RepoNotFound(format!(
                "worktree '{}' belongs to '{}', which is no longer a registered repository",
                name, entry.repo
            )));
        }
    }

    let ticket_document_path = root.join(".tix").join("ticket.toml");
    for name in &args.names {
        let entry = &ticket.worktrees[name];
        let repo = engine.configured_repositories[&entry.repo]
            .clone()
            .resolve(&entry.repo)?;
        repo.remove_worktree(&root.join(name), false)?;
        info!(name = %name, repo = %entry.repo, "worktree removed");

        // Drop the entry as soon as the prune lands, so an error later in
        // the batch leaves the document agreeing with disk.
        with_write(&ticket_document_path, |doc| {
            if let Some(worktrees) = doc.doc_mut()["ticket"]["worktrees"].as_table_mut() {
                worktrees.remove(name);
            }
            Ok(())
        })?;
        println!("removed {name}");
    }
    Ok(())
}
