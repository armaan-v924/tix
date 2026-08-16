//! `tix ticket destroy` — prune every worktree, then delete the ticket.

use tix_sdk::document::{TixDocument, with_write};
use crate::tix::ticket::{TicketRef, TicketSharedArgs, load_ticket_config, require_ticket_root};
use tix_sdk::{SdkError, EngineConfig, TixError, WorktreeConfig};
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
/// **`--force`** — no confirmation, worktrees force-pruned (uncommitted
/// changes discarded), and the directory is deleted **even if pruning
/// fails** — forced deletion means deleted. Whatever had to be left behind
/// (e.g. stale registrations in the source repos) is warned about, with a
/// pointer at `git worktree prune`.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let selector = args.target.as_ref().or(args.shared.ticket.as_ref());
    let root = require_ticket_root(&app.context, selector)?;
    let ticket = load_ticket_config(&root)?;

    let document = TixDocument::load(&app.context.config_path)?;
    let engine: EngineConfig = document.section_or_default("engine")?;

    if !args.force {
        let answer = crate::tix::utils::prompt(
            &format!("Destroy ticket '{}' at {}? [y/N]", ticket.key, root.display()),
            Some("n"),
        )?;
        if !matches!(answer.to_lowercase().as_str(), "y" | "yes") {
            println!("aborted");
            return Ok(());
        }
    }

    let mut entries: Vec<(&String, &WorktreeConfig)> = ticket.worktrees.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    let ticket_document_path = root.join(".tix").join("ticket.toml");
    let mut leftovers: Vec<String> = Vec::new();
    for (name, entry) in entries {
        let pruned = engine
            .configured_repositories
            .get(&entry.repo)
            .cloned()
            .ok_or_else(|| {
                TixError::RepoNotFound(format!(
                    "worktree '{}' belongs to '{}', which is not a registered repository",
                    name, entry.repo
                ))
            })
            .and_then(|config| config.resolve(&entry.repo))
            .and_then(|repo| repo.remove_worktree(&root.join(name), args.force));

        match (pruned, args.force) {
            (Ok(()), _) => {
                info!(name = %name, "worktree pruned");
                // Keep the document agreeing with disk so an abort partway
                // leaves the ticket resolvable for a retry.
                with_write(&ticket_document_path, |doc| {
                    if let Some(worktrees) = doc.doc_mut()["ticket"]["worktrees"].as_table_mut() {
                        worktrees.remove(name);
                    }
                    Ok(())
                })?;
            }
            // Forced deletion means deleted: pruning failures are warnings.
            (Err(e), true) => {
                warn!(name = %name, error = %e, "could not prune worktree, continuing (--force)");
                leftovers.push(format!("{} ({})", name, entry.repo));
            }
            // The default path aborts before deleting anything further.
            (Err(e), false) => {
                return Err(SdkError::Message(format!(
                    "could not prune worktree '{}': {e} — nothing further was deleted; \
                     fix it (or pass --force) and retry",
                    name
                )));
            }
        }
    }

    std::fs::remove_dir_all(&root).map_err(SdkError::from)?;
    println!("destroyed {} ({})", ticket.key, root.display());

    if !leftovers.is_empty() {
        warn!(
            "left dangling: {} — run `git worktree prune` in the affected source repos",
            leftovers.join(", ")
        );
    }
    Ok(())
}
