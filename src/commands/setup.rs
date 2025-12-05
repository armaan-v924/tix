use crate::git;
use crate::ticket::Ticket;
use crate::config::Config;

use anyhow::Result;
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

    // 1. Create or load the ticket directory
    if !ticket_dir.exists() {
        info!("Creating ticket directory at {:?}", ticket_dir);
        fs::create_dir_all(&ticket_dir)?;

        Ticket::create(&ticket_dir, ticket_id, description.as_ref())?;
    } else {
        debug!("Using existing ticket directory {:?}", ticket_dir);

        // Check for metadata
        if let Err(e) = Ticket::load(&ticket_dir) {
            warn!("Missing .tix metadata in existing directory: {}", e);
            info!("Initializing new .tix stamp");
            Ticket::create(&ticket_dir, ticket_id, description.as_ref())?;
        }
    }

    // 2. Determine Target Repositories
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
            return Ok(());
        }
        valid
    } else {
        // no repos specified, not --all
        warn!("No repositories specified.");
        info!("Created empty ticket environment.");
        info!("Hint: Use 'tix add <repo>' to add worktrees later.");
        return Ok(());
    };

    /// 3. Create worktrees
    let mut branch_name = format!("{}/{}", config.branch_prefix, ticket_id);

    if let Some(desc) = &description {
        let sanitized = sanitize_description(desc);
        if !sanitized.is_empty() {
            branch_name.push('-');
            branch_name.push_str(&sanitized);
        }
    }

    info!("Target branch: {}", branch_name);

    // 4. Create worktrees
    for alias in target_repos {
        if let Some(repo_def) = config.repositories.get(&alias) {
            info!("Setting up worktree for '{}'...", alias);

            let target_worktree_path = ticket_dir.join(&alias);

            match git::create_worktree(&repo_def.path, &target_worktree_path, &branch_name, None) {
                Ok(_) => info!("Created worktree: {:?}", target_worktree_path),
                Err(e) => error!("Failed to create worktree for '{}': {}", alias, e),
            }
        }
    }

    info!("Setup for {} complete!", ticket_id);
    Ok(())
}

/// Helper: specific sanitation for git branch compatibility
fn sanitize_description(input: &str) -> String {
    let mut result = String::new();
    let mut last_was_hyphen = true; // Start true to trim leading hyphens

    for c in input.chars() {
        if c.is_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_hyphen = false;
        } else {
            // Treat everything else (spaces, symbols) as a separator
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        }
    }

    // Trim trailing hyphen if exists
    if result.ends_with('-') {
        result.pop();
    }

    result
}
