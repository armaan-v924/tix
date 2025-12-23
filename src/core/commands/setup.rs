//! Setup command: initialize a ticket workspace and create repo worktrees.

use crate::core::commands::common::build_branch_name;
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::fs;

pub fn run(
    ticket_id: &str,
    repos: &[String],
    all: bool,
    description: Option<String>,
) -> Result<()> {
    let config = Config::load()?;
    let ticket_dir = config.tickets_directory.join(ticket_id);

    // 1. Determine Target Repositories
    let target_repos: Vec<String> = if all {
        debug!("Flag --all detected. Selecting all registered repositories.");
        config.repositories.keys().cloned().collect()
    } else if !repos.is_empty() {
        let mut valid = Vec::new();
        for alias in repos {
            if config.repositories.contains_key(alias) {
                valid.push(alias.clone());
            } else {
                warn!("Alias '{}' is not registered in config. Skipping.", alias);
            }
        }

        if valid.is_empty() {
            warn!("No valid repositories matched your input");
        }
        valid
    } else {
        // no repos specified, not --all
        warn!("No repositories specified.");
        info!("Created empty ticket environment.");
        info!("Hint: Use 'tix add <repo>' to add worktrees later.");
        Vec::new()
    };

    // 2. Compute branch name
    let branch_name = build_branch_name(&config, ticket_id, description.as_ref());

    // 3. Create or load the ticket directory and metadata
    if !ticket_dir.exists() {
        info!("Creating ticket directory at {:?}", ticket_dir);
        fs::create_dir_all(&ticket_dir)?;

        let repo_branches: Vec<(String, String)> = target_repos
            .iter()
            .map(|r| (r.clone(), branch_name.clone()))
            .collect();
        Ticket::create(
            &ticket_dir,
            ticket_id,
            description.as_ref(),
            &branch_name,
            &repo_branches,
        )?;
    } else {
        debug!("Using existing ticket directory {:?}", ticket_dir);

        // Check for metadata
        match Ticket::load(&ticket_dir) {
            Ok(existing) => {
                if existing.metadata.branch.is_empty() {
                    Ticket::ensure_branch(&ticket_dir, &branch_name)?;
                }
                Ticket::add_repos_with_branch(&ticket_dir, &target_repos, &branch_name)?;
            }
            Err(e) => {
                warn!("Missing .tix metadata in existing directory: {}", e);
                info!("Initializing new .tix stamp");
                let repo_branches: Vec<(String, String)> = target_repos
                    .iter()
                    .map(|r| (r.clone(), branch_name.clone()))
                    .collect();
                Ticket::create(
                    &ticket_dir,
                    ticket_id,
                    description.as_ref(),
                    &branch_name,
                    &repo_branches,
                )?;
            }
        }
    }

    info!("Target branch: {}", branch_name);

    // 4. Create worktrees
    for alias in target_repos {
        if let Some(repo_def) = config.repositories.get(&alias) {
            info!("Setting up worktree for '{}'...", alias);

            let target_worktree_path = ticket_dir.join(&alias);

            info!(
                "Updating repository at {:?} before creating worktree",
                repo_def.path
            );
            git::fetch_and_fast_forward(&repo_def.path, "origin").map_err(|e| {
                error!(
                    "Failed to update repository '{}' at {:?}: {}",
                    alias, repo_def.path, e
                );
                e
            })?;

            git::create_worktree(&repo_def.path, &target_worktree_path, &branch_name, None)
                .with_context(|| {
                    format!(
                        "Failed to create worktree for '{}' at {:?}",
                        alias, target_worktree_path
                    )
                })?;
            info!("Created worktree: {:?}", target_worktree_path);
        }
    }

    info!("Setup for {} complete!", ticket_id);
    Ok(())
}

/// Sanitize free-form text for inclusion in a git branch name (lowercase, alnum, single hyphens).
#[allow(dead_code)]
pub fn sanitize_description(input: &str) -> String {
    crate::core::commands::common::sanitize_description(input)
}
