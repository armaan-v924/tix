//! `tix ticket remove` — remove single worktrees from a ticket without
//! destroying it.

use crate::tix::ticket::{
    TicketSharedArgs, WorktreeStatus, load_engine_config, load_ticket_config, prune_worktree,
    require_ticket_root,
};
use tix_sdk::{SdkError, TixError};
use tracing::{info, warn};

/// Remove a worktree from a ticket without destroying it
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
/// A worktree whose directory was deleted by hand, or is no longer a git
/// checkout, is dropped from the document with a warning (and git's stale
/// registration pruned) rather than failing; a directory that is still
/// there is left in place.
///
/// The ticket directory and every other worktree are untouched; removing
/// the last worktree leaves a valid, empty ticket. Each successful prune is
/// written back immediately through the format-preserving layer.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let root = require_ticket_root(
        &app.context,
        args.shared.ticket.as_ref(),
        args.shared.project.as_deref(),
    )?;
    let ticket = load_ticket_config(&root)?;

    let engine = load_engine_config(&app.context)?;

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
                if tracked.is_empty() {
                    "none".to_string()
                } else {
                    tracked.join(", ")
                }
            ))
        })?;
        if !engine.configured_repositories.contains_key(&entry.repo) {
            return Err(SdkError::Engine(TixError::RepoNotFound(format!(
                "worktree '{}' belongs to '{}', which is no longer a registered repository",
                name, entry.repo
            ))));
        }
    }

    for name in &args.names {
        let status = prune_worktree(&root, name, &ticket.worktrees[name], &engine, false)
            .map_err(|e| match e {
                SdkError::Engine(dirty @ TixError::Dirty { .. }) => SdkError::Message(format!(
                    "could not remove worktree '{name}': {dirty} — commit or stash the changes first"
                )),
                e => e,
            })?;
        match status {
            WorktreeStatus::NotAGitRepository => warn!(
                "left {} in place: it is not a git checkout, so tix cannot tell whether it holds work",
                root.join(name).display()
            ),
            _ => info!("removed worktree {name}"),
        }
    }
    Ok(())
}
