//! Add a repo worktree to an existing ticket.

use crate::core::commands::common::{build_branch_name, locate_ticket_root};
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{Context, Result, anyhow, bail};
use log::{info, warn};
use std::path::Path;

/// Run the add command.
pub fn run(repo_alias: &str, ticket: Option<&str>, branch: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let ticket_root = locate_ticket_root(ticket, &config)?;
    ensure_ticket_exists(&ticket_root)?;

    let ticket_meta = Ticket::load(&ticket_root).context(
        "Failed to load ticket metadata. Run from a valid ticket directory or specify --ticket",
    )?;

    let repo_def = config
        .repositories
        .get(repo_alias)
        .ok_or_else(|| anyhow!("Alias '{}' is not registered in config", repo_alias))?;

    let target_worktree = ticket_root.join(repo_alias);
    if target_worktree.exists() {
        bail!(
            "Worktree for '{}' already exists at {:?}. Refusing to overwrite.",
            repo_alias,
            target_worktree
        );
    }

    let branch_name = build_branch_name(
        &config,
        &ticket_meta.metadata.id,
        ticket_meta.metadata.description.as_ref(),
    );
    // Prefer recorded branch for this repo, then ticket branch, then computed branch
    let branch_name = ticket_meta
        .metadata
        .repo_branches
        .get(repo_alias)
        .cloned()
        .or_else(|| {
            if !ticket_meta.metadata.branch.is_empty() {
                Some(ticket_meta.metadata.branch.clone())
            } else {
                None
            }
        })
        .unwrap_or(branch_name);
    if !ticket_meta.metadata.repo_branches.contains_key(repo_alias) {
        warn!(
            "No stored branch for repo '{}'; using computed branch '{}'",
            repo_alias, branch_name
        );
    }
    let base_ref = branch.map(|s| s.to_string());

    info!(
        "Adding worktree for repo '{}' into {:?} on branch '{}'",
        repo_alias, target_worktree, branch_name
    );

    // Ensure repo is up to date before branching.
    git::fetch_and_fast_forward(&repo_def.path, "origin").with_context(|| {
        format!(
            "Failed to update repo '{}' at {:?}",
            repo_alias, repo_def.path
        )
    })?;

    git::create_worktree(
        &repo_def.path,
        &target_worktree,
        &branch_name,
        base_ref.as_deref(),
    )
    .context("Failed to create worktree")?;

    info!("Created worktree at {:?}", target_worktree);
    Ticket::ensure_branch(&ticket_root, &branch_name)?;
    Ticket::add_repo_branch(&ticket_root, repo_alias, &branch_name)?;
    Ok(())
}

fn ensure_ticket_exists(ticket_dir: &Path) -> Result<()> {
    if !ticket_dir.exists() {
        bail!(
            "Ticket directory {:?} does not exist. Run from a ticket or pass a valid --ticket.",
            ticket_dir
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests moved to commands::common to reduce duplication.
}
