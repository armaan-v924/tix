//! Remove a repo worktree from an existing ticket with safety checks.

use crate::core::commands::common::{build_branch_name, locate_ticket_root};
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{Context, Result, anyhow, bail};
use log::{info, warn};
use std::fs;

/// Run the remove command.
pub fn run(repo_alias: &str, ticket: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let ticket_root = locate_ticket_root(ticket, &config)?;

    let ticket_meta = Ticket::load(&ticket_root).context("Failed to load ticket metadata")?;

    let repo_def = config
        .repositories
        .get(repo_alias)
        .ok_or_else(|| anyhow!("Alias '{}' is not registered in config", repo_alias))?;

    let target_worktree = ticket_root.join(repo_alias);
    if !target_worktree.exists() {
        bail!(
            "Worktree for '{}' does not exist at {:?}",
            repo_alias,
            target_worktree
        );
    }

    // Safety: ensure worktree is clean
    let is_clean = git::is_clean(&target_worktree).with_context(|| {
        format!(
            "Could not check clean status for worktree {:?}",
            target_worktree
        )
    })?;
    if !is_clean {
        bail!(
            "Worktree at {:?} has uncommitted changes. Commit or clean before removing.",
            target_worktree
        );
    }

    info!(
        "Removing worktree for '{}' at {:?}",
        repo_alias, target_worktree
    );
    fs::remove_dir_all(&target_worktree)
        .with_context(|| format!("Failed to remove {:?}", target_worktree))?;

    let branch_for_repo = ticket_meta
        .metadata
        .repo_branches
        .get(repo_alias)
        .cloned()
        .unwrap_or_else(|| {
            build_branch_name(
                &config,
                &ticket_meta.metadata.id,
                ticket_meta.metadata.description.as_ref(),
            )
        });
    let worktree_name = ticket_meta
        .metadata
        .repo_worktrees
        .get(repo_alias)
        .cloned()
        .unwrap_or_else(|| {
            warn!(
                "No stored worktree name for repo '{}'; deriving from branch '{}'",
                repo_alias, branch_for_repo
            );
            crate::core::ticket::worktree_name_for_branch(&branch_for_repo)
        });

    git::remove_worktree(&repo_def.path, &worktree_name).with_context(|| {
        format!(
            "Failed to prune worktree metadata '{}' for repo '{}'",
            worktree_name, repo_alias
        )
    })?;

    info!(
        "Removed worktree '{}' from ticket '{}'",
        repo_alias, ticket_meta.metadata.id
    );
    Ticket::remove_repo(&ticket_root, repo_alias)
        .with_context(|| format!("Failed to update ticket metadata for '{}'", repo_alias))?;
    Ok(())
}
