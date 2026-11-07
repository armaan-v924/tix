//! `tix ticket remove` — remove single worktrees from a ticket without
//! destroying it.

use crate::tix::config::load_engine_config;
use crate::tix::ticket::{
    TicketSharedArgs, WorktreeStatus, backing_repository, load_ticket_config, prune_worktree,
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

    /// Remove worktrees with uncommitted changes, discarding them
    #[arg(short, long)]
    pub force: bool,
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
/// A worktree with uncommitted changes (untracked files included) is
/// refused unless `--force`, which discards them. The batch stops at the
/// first refusal; worktrees already removed stay removed.
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
        backing_repository(name, entry, &engine)?;
    }

    for name in &args.names {
        let status = prune_worktree(&root, name, &ticket.worktrees[name], &engine, args.force)
            .map_err(|e| match e {
                SdkError::Engine(dirty @ TixError::Dirty { .. }) => SdkError::Message(format!(
                    "could not remove worktree '{name}': {dirty} — commit or stash the changes, or pass --force to discard them"
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
