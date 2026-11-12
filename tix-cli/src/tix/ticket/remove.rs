//! `tix ticket remove` — remove single worktrees from a ticket without
//! destroying it.

use crate::tix::config::load_engine_config;
use crate::tix::ticket::{
    TicketSharedArgs, WorktreeStatus, backing_repository, check_prune_worktree, load_ticket_config,
    prune_worktree, require_ticket_root,
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

    /// Print what would be removed, and what would be refused, without
    /// removing anything; exits non-zero where the real run would fail
    #[arg(long)]
    pub dry_run: bool,
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
///
/// `--dry-run` walks the same batch through the same checks
/// ([`check_prune_worktree`]) and prints one `would …` line per worktree on
/// stdout. It stops where the real run would stop, on the same refusal
/// (worded as a prediction), so its exit code answers "would this work?".
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

    if args.dry_run {
        for name in &args.names {
            let path = root.join(name);
            let status =
                check_prune_worktree(&root, name, &ticket.worktrees[name], &engine, args.force)
                    .map_err(|e| refusal(name, e, true))?;
            match status {
                WorktreeStatus::Ok => println!("would remove worktree {name} ({})", path.display()),
                WorktreeStatus::MissingDirectory => {
                    println!("would drop worktree {name} from the ticket: missing on disk")
                }
                WorktreeStatus::NotAGitRepository => println!(
                    "would drop worktree {name} from the ticket: not a git repository \
                     ({} would be left in place)",
                    path.display()
                ),
            }
        }
        return Ok(());
    }

    for name in &args.names {
        let status = prune_worktree(&root, name, &ticket.worktrees[name], &engine, args.force)
            .map_err(|e| refusal(name, e, false))?;
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

/// `error` from pruning `name` — or, for `dry_run`, from checking it —
/// worded with the way past a dirty worktree.
fn refusal(name: &str, error: SdkError, dry_run: bool) -> SdkError {
    let lead = if dry_run {
        "would refuse to remove"
    } else {
        "could not remove"
    };
    match error {
        SdkError::Engine(dirty @ TixError::Dirty { .. }) => SdkError::Message(format!(
            "{lead} worktree '{name}': {dirty} — commit or stash the changes, or pass --force to discard them"
        )),
        e => e,
    }
}
