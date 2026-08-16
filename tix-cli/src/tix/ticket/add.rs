//! `tix ticket add` — add repository worktrees to an existing ticket.

use crate::tix::context::Context;
use crate::tix::document::{TixDocument, with_write};
use crate::tix::repo::RepoAlias;
use crate::tix::ticket::{
    TicketSharedArgs, derive_branch_name, load_ticket_config, require_ticket_root,
};
use tix_engine::{Defaults, EngineConfig, TixError};
use tracing::{error, info};

/// Arguments for `tix ticket add`.
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// Branch for the new worktree(s); derived from [defaults] + ticket
    /// context when omitted
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Registered repository aliases to add
    #[arg(required = true)]
    pub repo_aliases: Vec<RepoAlias>,
}

/// Adds a worktree per given alias to the current (or `--ticket`) ticket.
///
/// The branch is `--branch` when given (created if it doesn't exist), else
/// derived exactly as `tix ticket setup` derives it —
/// `<branch_prefix>/<key>-<sanitized-description>` — but evaluated **at add
/// time** against the *current* `[defaults].branch_prefix` and the ticket's
/// recorded key/description.
///
/// An alias the ticket already tracks errors: multiple worktrees of one repo
/// is #85, which needs its own speccing before any behavior here changes.
/// The ticket document is updated per successful worktree through the
/// format-preserving layer, so plugin tables and comments in
/// `.tix/ticket.toml` survive.
pub fn run(context: &Context, args: Args) -> Result<(), TixError> {
    let root = require_ticket_root(context, args.shared.ticket.as_ref())?;
    let ticket = load_ticket_config(&root)?;

    let document = TixDocument::load(&context.config_path)?;
    let engine: EngineConfig = document.section_or_default("engine")?;
    let defaults: Defaults = document.section_or_default("defaults")?;

    // Validate the whole batch up front: unknown aliases and already-tracked
    // aliases are user errors, not partial-failure material.
    for alias in &args.repo_aliases {
        if !engine.configured_repositories.contains_key(&alias.0) {
            let mut known: Vec<&str> = engine
                .configured_repositories
                .keys()
                .map(String::as_str)
                .collect();
            known.sort();
            return Err(TixError::RepoNotFound(format!(
                "'{}' is not a registered repository (registered: {})",
                alias.0,
                if known.is_empty() { "none".to_string() } else { known.join(", ") }
            )));
        }
        if ticket.worktrees.contains_key(&alias.0) {
            return Err(TixError::Message(format!(
                "ticket '{}' already has a worktree for '{}' — multiple worktrees of one repo is #85",
                ticket.key, alias.0
            )));
        }
    }

    let branch = args.branch.clone().unwrap_or_else(|| {
        derive_branch_name(
            defaults.branch_prefix.as_deref(),
            &ticket.key,
            (!ticket.description.is_empty()).then_some(ticket.description.as_str()),
        )
    });

    let ticket_document_path = root.join(".tix").join("ticket.toml");
    let mut failures: Vec<(String, TixError)> = Vec::new();
    for alias in &args.repo_aliases {
        let repo_config = engine.configured_repositories[&alias.0].clone();
        let created = repo_config
            .ensure(&alias.0)
            .and_then(|repo| repo.create_worktree(&alias.0, &branch, &root.join(&alias.0), false));
        match created {
            Ok(worktree) => {
                info!(alias = %alias.0, branch = %worktree.branch, "worktree added");
                // Record each success immediately — a later failure must not
                // lose the worktrees that already exist on disk.
                with_write(&ticket_document_path, |doc| {
                    // An explicit table so the entry renders as
                    // [ticket.worktrees.<alias>], matching setup's output.
                    let mut entry = toml_edit::Table::new();
                    entry["repo"] = toml_edit::value(alias.0.as_str());
                    entry["branch"] = toml_edit::value(worktree.branch.as_str());
                    doc.doc_mut()["ticket"]["worktrees"][&alias.0] =
                        toml_edit::Item::Table(entry);
                    Ok(())
                })?;
                println!("{}", root.join(&alias.0).display());
            }
            Err(e) => {
                error!(alias = %alias.0, error = %e, "failed to add worktree");
                failures.push((alias.0.clone(), e));
            }
        }
    }

    if !failures.is_empty() {
        let names: Vec<&str> = failures.iter().map(|(alias, _)| alias.as_str()).collect();
        return Err(TixError::Message(format!(
            "{} of {} worktrees failed: {}",
            failures.len(),
            args.repo_aliases.len(),
            names.join(", ")
        )));
    }
    Ok(())
}
