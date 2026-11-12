//! `tix ticket destroy` — prune every worktree, then delete the ticket.

use crate::tix::config::load_engine_config;
use crate::tix::plan::{Action, Plan};
use crate::tix::ticket::remove::prune_action;
use crate::tix::ticket::{
    PrunePlan, TicketRef, TicketSharedArgs, WorktreeStatus, load_ticket_config, plan_prune,
    prune_worktree, require_ticket_root, uncommitted,
};
use std::path::Path;
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

    /// Print what would be pruned and deleted, and what would stop the
    /// destroy, without asking or touching anything; exits non-zero where
    /// the real run would fail. Combine with --force to preview a forced
    /// destroy
    #[arg(long)]
    pub dry_run: bool,
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
///
/// **`--dry-run`** — the same checks, nothing done: no question is asked,
/// and each worktree is run through [`plan_prune`] instead of pruned. The
/// result is a [`Plan`] printed in the `--output` format. Without `--force`
/// it stops where the real run would stop, on the same refusal; with it, it
/// says what force would get past (how many uncommitted changes it would
/// discard), and a worktree that would fail to prune is reported and the
/// plan carries on, as the real forced run would.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let selector = args.target.as_ref().or(args.shared.ticket.as_ref());
    let root = require_ticket_root(&app.context, selector, args.shared.project.as_deref())?;
    let ticket = load_ticket_config(&root)?;

    let engine = load_engine_config(&app.context)?;

    if args.dry_run {
        return dry_run(app, &root, &ticket, &engine, args.force);
    }

    if !args.force {
        preflight(&root, &ticket)?;
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
            (Err(e), false) => return Err(refusal(name, e, false)),
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

/// Refuses a ticket holding a worktree directory that is no longer a
/// checkout: deleting the ticket deletes it too, and nothing can check it
/// for uncommitted work first. `--force` skips this.
fn preflight(root: &Path, ticket: &tix_sdk::TicketConfig) -> Result<(), SdkError> {
    let mut names: Vec<&String> = ticket.worktrees.keys().collect();
    names.sort();
    match names
        .into_iter()
        .find(|name| WorktreeStatus::of(&root.join(name)) == WorktreeStatus::NotAGitRepository)
    {
        Some(name) => Err(SdkError::Message(format!(
            "worktree '{name}' at {} is not a git repository, so tix cannot check it for \
             uncommitted work — move anything you need out of it, then pass --force",
            root.join(name).display()
        ))),
        None => Ok(()),
    }
}

/// `--dry-run`: what [`run`] would do to the ticket at `root`, planned and
/// printed in the requested output format, with nothing done.
fn dry_run(
    app: &crate::tix::utils::App,
    root: &Path,
    ticket: &tix_sdk::TicketConfig,
    engine: &tix_sdk::EngineConfig,
    force: bool,
) -> Result<(), SdkError> {
    let mut plan = Plan::new("destroy");
    if !force && let Err(e) = preflight(root, ticket) {
        return plan.finish(app.output, Err(e));
    }

    let mut entries: Vec<(&String, &WorktreeConfig)> = ticket.worktrees.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (name, entry) in entries {
        let path = root.join(name);
        let found = match plan_prune(root, name, entry, engine, force) {
            Ok(found) => found,
            Err(e) if force => {
                plan.push(
                    Action::new(
                        "prune-fails",
                        format!(
                            "would fail to prune worktree {name} ({e}) and delete it anyway \
                             (--force), leaving its registration in {} dangling",
                            entry.repo
                        ),
                    )
                    .with("worktree", name.as_str())
                    .with("repo", entry.repo.as_str())
                    .with_path("path", &path)
                    .with("forced", true)
                    .with("reason", e.to_string()),
                );
                continue;
            }
            Err(e) => return plan.finish(app.output, Err(refusal(name, e, true))),
        };
        let (kind, summary) = match &found {
            PrunePlan::Prune => (
                "prune",
                format!("would prune worktree {name} ({})", path.display()),
            ),
            PrunePlan::Discard { changes } => (
                "prune",
                format!(
                    "would prune worktree {name} ({}), discarding {} (--force)",
                    path.display(),
                    uncommitted(*changes)
                ),
            ),
            PrunePlan::ForcePast { reason } => (
                "prune",
                format!(
                    "would prune worktree {name} ({}) past: {reason} (--force)",
                    path.display()
                ),
            ),
            PrunePlan::Missing => (
                "drop",
                format!("would drop worktree {name}: missing on disk"),
            ),
            // Only reachable with --force: the pre-flight refused it otherwise.
            PrunePlan::NotAGitRepository => (
                "delete-unchecked",
                format!(
                    "would delete {} unchecked: not a git repository (--force)",
                    path.display()
                ),
            ),
        };
        plan.push(prune_action(
            kind,
            summary,
            name,
            &entry.repo,
            &path,
            &found,
        ));
    }

    plan.push(
        Action::new(
            "delete-ticket",
            format!("would destroy {} ({})", ticket.key, root.display()),
        )
        .with("ticket", ticket.key.as_str())
        .with_path("path", root),
    );
    plan.finish(app.output, Ok(()))
}

/// The error a non-forced destroy stops with when `name` cannot be pruned —
/// or, for `dry_run`, would stop with.
fn refusal(name: &str, error: SdkError, dry_run: bool) -> SdkError {
    let fix = match error {
        SdkError::Engine(TixError::Dirty { .. }) => "commit or stash the changes",
        _ => "fix it",
    };
    SdkError::Message(if dry_run {
        format!(
            "destroy would stop at worktree '{name}': {error} — nothing would be deleted \
             past it; {fix} (or pass --force) first"
        )
    } else {
        format!(
            "could not prune worktree '{name}': {error} — nothing further was deleted; \
             {fix} (or pass --force) and retry"
        )
    })
}
