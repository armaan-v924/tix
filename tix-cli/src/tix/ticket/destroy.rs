//! `tix ticket destroy` — prune every worktree, then delete the ticket.

use crate::tix::config::load_engine_config;
use crate::tix::plan::{Action, Plan};
use crate::tix::ticket::remove::prune_action;
use crate::tix::ticket::result::TicketOutput;
use crate::tix::ticket::{
    PrunePlan, TicketRef, TicketSharedArgs, WorktreeStatus, load_ticket_config, plan_prune,
    prune_worktree, require_ticket_root, uncommitted,
};
use std::path::Path;
use tix_sdk::discovery::TIX_DIR;
use tix_sdk::{EngineConfig, SdkError, TicketConfig, TixError, WorktreeConfig};
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
/// **Default path** — checks each worktree before confirming or pruning.
/// The directory is deleted only after every worktree prunes cleanly.
/// Each successful prune deletes its entry
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
    let mut output = TicketOutput::new("destroy", args.dry_run);
    let result = execute(app, args, &mut output);
    output.finish(app.output, result)
}

fn execute(
    app: &crate::tix::utils::App,
    args: Args,
    output: &mut TicketOutput,
) -> Result<(), SdkError> {
    let selector = args.target.as_ref().or(args.shared.ticket.as_ref());
    let root = require_ticket_root(&app.context, selector, args.shared.project.as_deref())?;
    output.ticket(&root)?;
    let ticket = load_ticket_config(&root)?;

    let engine = load_engine_config(&app.context)?;

    let mut entries: Vec<_> = ticket.worktrees.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    if let Some(plan) = &mut output.plan {
        return dry_run(plan, &root, &ticket, &engine, &entries, args.force);
    }

    if !args.force {
        for (name, entry) in &entries {
            check_worktree(&root, name, entry, &engine, false).map_err(|error| {
                output.worktree_failed(name, &root.join(name), &entry.repo, &error);
                output.set_error(&error);
                refusal(name, error, false)
            })?;
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
        output.declined();
        return Ok(());
    }

    let mut leftovers: Vec<String> = Vec::new();
    for (name, entry) in entries {
        let pruned = prune_worktree(&root, name, entry, &engine, args.force);
        let path = root.join(name);
        match (pruned, args.force) {
            (Ok(status), _) => {
                output.pruned(name, &path, &entry.repo, status);
                if status == WorktreeStatus::Ok {
                    info!("pruned worktree {name}");
                }
            }
            // Forced deletion means deleted: pruning failures are warnings.
            (Err(e), true) => {
                output.worktree_failed(name, &path, &entry.repo, &e);
                warn!(name = %name, error = %e, "could not prune worktree, continuing (--force)");
                leftovers.push(format!("{} ({})", name, entry.repo));
            }
            // The default path aborts before deleting anything further.
            (Err(e), false) => {
                output.worktree_failed(name, &path, &entry.repo, &e);
                output.set_error(&e);
                return Err(refusal(name, e, false));
            }
        }
    }

    std::fs::remove_dir_all(&root).map_err(SdkError::from)?;
    output.changed();
    info!("destroyed {} ({})", ticket.key, root.display());

    if !leftovers.is_empty() {
        warn!(
            "left dangling: {} — run `git worktree prune` in the affected source repos",
            leftovers.join(", ")
        );
    }
    Ok(())
}

/// Checks deletion of a single worktree before any worktree is pruned.
fn check_worktree(
    root: &Path,
    name: &str,
    entry: &WorktreeConfig,
    engine: &EngineConfig,
    force: bool,
) -> Result<PrunePlan, SdkError> {
    let path = root.join(name);
    if !force && WorktreeStatus::of(&path) == WorktreeStatus::NotAGitRepository {
        return Err(SdkError::Message(format!(
            "worktree '{name}' at {} is not a git repository, so tix cannot check it for \
             uncommitted work — move anything you need out of it, then pass --force",
            path.display()
        )));
    }
    plan_prune(root, name, entry, engine, force)
}

/// `--dry-run`: what [`run`] would do to the ticket at `root`, planned and
/// printed in the requested output format, with nothing done.
fn dry_run(
    plan: &mut Plan,
    root: &Path,
    ticket: &TicketConfig,
    engine: &EngineConfig,
    entries: &[(&String, &WorktreeConfig)],
    force: bool,
) -> Result<(), SdkError> {
    for (name, entry) in entries {
        let path = root.join(name);
        let found = match check_worktree(root, name, entry, engine, force) {
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
            Err(e) => {
                plan.clear_actions();
                plan.set_error(&e);
                return Err(refusal(name, e, true));
            }
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

    let unmanaged = match unmanaged_entries(root, ticket) {
        Ok(entries) => entries,
        Err(e) => {
            plan.clear_actions();
            return Err(e.into());
        }
    };
    let mut summary = format!("would destroy {} ({})", ticket.key, root.display());
    if !unmanaged.is_empty() {
        summary.push_str(&format!(
            ", including unmanaged ticket entries: {unmanaged:?}"
        ));
    }
    plan.push(
        Action::new("delete-ticket", summary)
            .with("ticket", ticket.key.as_str())
            .with_path("path", root)
            .with("unmanaged_entries", unmanaged),
    );
    Ok(())
}

/// Direct ticket-root entries outside the tix metadata directory and
/// tracked worktree directories.
fn unmanaged_entries(root: &Path, ticket: &TicketConfig) -> std::io::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == TIX_DIR {
            continue;
        }
        if name
            .to_str()
            .is_some_and(|name| ticket.worktrees.contains_key(name))
            && entry.file_type()?.is_dir()
        {
            continue;
        }
        entries.push(name.to_string_lossy().into_owned());
    }
    entries.sort();
    Ok(entries)
}

/// The error a non-forced destroy stops with when `name` cannot be pruned —
/// or, for `dry_run`, would stop with.
fn refusal(name: &str, error: SdkError, dry_run: bool) -> SdkError {
    let fix = match error {
        SdkError::Message(_) => return error,
        SdkError::Engine(TixError::Dirty { .. }) => "commit or stash the changes",
        _ => "fix it",
    };
    SdkError::Message(if dry_run {
        format!("destroy would stop at worktree '{name}': {error}. {fix} (or pass --force) first")
    } else {
        format!(
            "could not prune worktree '{name}': {error} — nothing further was deleted; \
             {fix} (or pass --force) and retry"
        )
    })
}
