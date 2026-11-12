//! `tix ticket remove` — remove single worktrees from a ticket without
//! destroying it.

use crate::tix::config::load_engine_config;
use crate::tix::plan::{Action, Plan};
use crate::tix::ticket::{
    PrunePlan, TicketSharedArgs, WorktreeStatus, backing_repository, load_ticket_config,
    plan_prune, prune_worktree, require_ticket_root, uncommitted,
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
/// ([`plan_prune`]) and prints the result as a [`Plan`] in the `--output`
/// format. It stops where the real run would stop, on the same refusal
/// (worded as a prediction), so its exit code answers "would this work?";
/// with `--force` it says how many uncommitted changes would be discarded.
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
        let mut plan = Plan::new("remove");
        for name in &args.names {
            let entry = &ticket.worktrees[name];
            let path = root.join(name);
            let found = match plan_prune(&root, name, entry, &engine, args.force) {
                Ok(found) => found,
                Err(e) => return plan.finish(app.output, Err(refusal(name, e, true))),
            };
            let summary = match &found {
                PrunePlan::Prune => format!("would remove worktree {name} ({})", path.display()),
                PrunePlan::Discard { changes } => format!(
                    "would remove worktree {name} ({}), discarding {} (--force)",
                    path.display(),
                    uncommitted(*changes)
                ),
                PrunePlan::ForcePast { reason } => format!(
                    "would remove worktree {name} ({}) past: {reason} (--force)",
                    path.display()
                ),
                PrunePlan::Missing => {
                    format!("would drop worktree {name} from the ticket: missing on disk")
                }
                PrunePlan::NotAGitRepository => format!(
                    "would drop worktree {name} from the ticket: not a git repository \
                     ({} would be left in place)",
                    path.display()
                ),
            };
            plan.push(prune_action(
                match found {
                    PrunePlan::Missing | PrunePlan::NotAGitRepository => "drop",
                    _ => "remove",
                },
                summary,
                name,
                &entry.repo,
                &path,
                &found,
            ));
        }
        return plan.finish(app.output, Ok(()));
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

/// The plan entry for pruning worktree `name`, with what [`plan_prune`]
/// found recorded alongside it.
pub(crate) fn prune_action(
    kind: &'static str,
    summary: String,
    name: &str,
    repo: &str,
    path: &std::path::Path,
    found: &PrunePlan,
) -> Action {
    let action = Action::new(kind, summary)
        .with("worktree", name)
        .with("repo", repo)
        .with_path("path", path);
    match found {
        PrunePlan::Prune => action,
        PrunePlan::Discard { changes } => action
            .with("forced", true)
            .with("uncommitted_changes", *changes as i64),
        PrunePlan::ForcePast { reason } => {
            action.with("forced", true).with("reason", reason.as_str())
        }
        PrunePlan::Missing => action.with("reason", "missing on disk"),
        PrunePlan::NotAGitRepository => action.with("reason", "not a git repository"),
    }
}
