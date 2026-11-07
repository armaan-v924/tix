//! `tix ticket destroy` — prune every worktree, then delete the ticket.

use crate::tix::ticket::{
    TicketRef, TicketSharedArgs, WorktreeStatus, load_engine_config, load_ticket_config,
    prune_worktree, require_ticket_root,
};
use tix_sdk::{SdkError, TixError, WorktreeConfig};
use tracing::{info, warn};

/// Destroy a ticket: prune its worktrees, delete its directory
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// The ticket to destroy (id under the tickets directory, or a path);
    /// discovered from cwd when omitted
    #[arg(value_name = "TICKET", conflicts_with = "ticket")]
    pub target: Option<TicketRef>,

    /// Skip confirmation, force-prune (discarding uncommitted changes), and
    /// delete the ticket directory even if pruning fails
    #[arg(short, long)]
    pub force: bool,
}

/// Destroys a ticket workspace: every tracked worktree pruned, then the
/// directory removed.
///
/// **Default path** — confirms first, prunes each worktree *without* force
/// (dirty worktrees abort), and deletes the directory only after every
/// tracked worktree pruned cleanly. Each successful prune deletes its entry
/// from the ticket document immediately, so an abort partway leaves a
/// resolvable ticket for a retry.
///
/// A worktree whose directory was deleted by hand is dropped with a warning
/// rather than failing. One that is there but no longer a git checkout
/// cannot be checked for uncommitted work, so the default path refuses up
/// front, before confirming; `--force` deletes it with the rest.
///
/// **`--force`** — no confirmation, worktrees force-pruned (uncommitted
/// changes discarded), and the directory is deleted **even if pruning
/// fails** — forced deletion means deleted. Whatever had to be left behind
/// (e.g. stale registrations in the source repos) is warned about, with a
/// pointer at `git worktree prune`.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let selector = args.target.as_ref().or(args.shared.ticket.as_ref());
    let root = require_ticket_root(&app.context, selector, args.shared.project.as_deref())?;
    let ticket = load_ticket_config(&root)?;

    let engine = load_engine_config(&app.context)?;

    // Deleting the ticket deletes a worktree directory that is no longer a
    // checkout too, and nothing can check it for uncommitted work first.
    if !args.force {
        let mut names: Vec<&String> = ticket.worktrees.keys().collect();
        names.sort();
        if let Some(name) = names
            .into_iter()
            .find(|name| WorktreeStatus::of(&root.join(name)) == WorktreeStatus::NotAGitRepository)
        {
            return Err(SdkError::Message(format!(
                "worktree '{name}' at {} is not a git repository, so tix cannot check it for \
                 uncommitted work — move anything you need out of it, then pass --force",
                root.join(name).display()
            )));
        }
    }

    if !args.force
        && !crate::tix::utils::confirm(&format!(
            "Destroy ticket '{}' at {}?",
            ticket.key,
            root.display()
        ))?
    {
        info!("aborted");
        return Ok(());
    }

    let mut entries: Vec<(&String, &WorktreeConfig)> = ticket.worktrees.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let mut leftovers: Vec<String> = Vec::new();
    for (name, entry) in entries {
        let pruned = prune_worktree(&root, name, entry, &engine, args.force);
        match (pruned, args.force) {
            (Ok(WorktreeStatus::Ok), _) => info!("pruned worktree {name}"),
            // Already gone or no checkout: prune_worktree warned.
            (Ok(_), _) => {}
            // Forced deletion means deleted: pruning failures are warnings.
            (Err(e), true) => {
                warn!(name = %name, error = %e, "could not prune worktree, continuing (--force)");
                leftovers.push(format!("{} ({})", name, entry.repo));
            }
            // The default path aborts before deleting anything further.
            (Err(e), false) => {
                let fix = match e {
                    SdkError::Engine(TixError::Dirty { .. }) => "commit or stash the changes",
                    _ => "fix it",
                };
                return Err(SdkError::Message(format!(
                    "could not prune worktree '{name}': {e} — nothing further was deleted; \
                     {fix} (or pass --force) and retry"
                )));
            }
        }
    }

    std::fs::remove_dir_all(&root).map_err(SdkError::from)?;
    info!("destroyed {} ({})", ticket.key, root.display());

    if !leftovers.is_empty() {
        warn!(
            "left dangling: {} — run `git worktree prune` in the affected source repos",
            leftovers.join(", ")
        );
    }
    Ok(())
}
