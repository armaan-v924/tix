//! Destroy a ticket workspace after safety checks.

use crate::core::commands::setup::sanitize_description;
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{anyhow, bail, Context, Result};
use log::{debug, error, info, warn};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Run the destroy command.
pub fn run(ticket_id: &str, force: bool) -> Result<()> {
    let config = Config::load()?;
    let ticket_dir = config.tickets_directory.join(ticket_id);

    if !ticket_dir.exists() {
        warn!(
            "Ticket directory {:?} does not exist; nothing to destroy.",
            ticket_dir
        );
        return Ok(());
    }

    ensure_not_inside(&ticket_dir)?;

    // Try to load metadata to reconstruct branch names; fall back to missing metadata.
    let ticket_meta = match Ticket::load(&ticket_dir) {
        Ok(t) => Some(t.metadata),
        Err(e) => {
            warn!(
                "Could not load ticket metadata: {}. Proceeding with limited cleanup.",
                e
            );
            None
        }
    };

    let branch_name = build_branch_name(
        &config,
        ticket_id,
        ticket_meta.as_ref().and_then(|m| m.description.as_ref()),
    );
    let worktree_name = branch_name.replace('/', "_");

    let worktree_dirs = worktree_dirs(&ticket_dir);
    debug!("Found worktree directories: {:?}", worktree_dirs);

    // Safety checks: ensure clean unless --force
    if !force {
        for dir in &worktree_dirs {
            match git::is_clean(dir) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(anyhow!(
                        "Worktree at {:?} has uncommitted changes. Use --force to override.",
                        dir
                    ));
                }
                Err(e) => warn!("Could not check clean status for {:?}: {}", dir, e),
            }
        }
    }

    // Remove directories
    for dir in &worktree_dirs {
        if dir.exists() {
            info!("Removing worktree directory {:?}", dir);
            if let Err(e) = fs::remove_dir_all(dir) {
                error!("Failed to remove {:?}: {}", dir, e);
            }
        }
    }

    // Prune worktree metadata for known repos
    for (alias, repo_def) in &config.repositories {
        let target_path = ticket_dir.join(alias);
        if target_path.exists() {
            debug!(
                "Pruning worktree metadata '{}' in repo {:?}",
                worktree_name, repo_def.path
            );
            if let Err(e) = git::remove_worktree(&repo_def.path, &worktree_name) {
                warn!(
                    "Failed to prune worktree '{}' for repo '{}': {}",
                    worktree_name, alias, e
                );
            }
        }
    }

    info!("Removing ticket directory {:?}", ticket_dir);
    fs::remove_dir_all(&ticket_dir)?;
    info!("Destroyed ticket '{}'", ticket_id);
    Ok(())
}

fn worktree_dirs(ticket_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(read) = fs::read_dir(ticket_dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().map(|n| n != ".tix").unwrap_or(false) {
                dirs.push(path);
            }
        }
    }
    dirs
}

fn ensure_not_inside(ticket_dir: &Path) -> Result<()> {
    let current = env::current_dir().context("Failed to get current directory")?;
    let ticket_canon = ticket_dir
        .canonicalize()
        .unwrap_or_else(|_| ticket_dir.to_path_buf());
    let current_canon = current.canonicalize().unwrap_or(current);

    if current_canon.starts_with(&ticket_canon) {
        bail!(
            "Refusing to destroy the ticket while you are inside {:?}",
            ticket_dir
        );
    }
    Ok(())
}

fn build_branch_name(config: &Config, ticket_id: &str, description: Option<&String>) -> String {
    let mut branch_name = format!("{}/{}", config.branch_prefix, ticket_id);
    if let Some(desc) = description {
        let sanitized = sanitize_description(desc);
        if !sanitized.is_empty() {
            branch_name.push('-');
            branch_name.push_str(&sanitized);
        }
    }
    branch_name
}

#[cfg(test)]
mod tests {
    use super::{build_branch_name, ensure_not_inside};
    use crate::core::config::Config;
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    fn base_config() -> Config {
        Config {
            branch_prefix: "feature".into(),
            github_base_url: "git@github.com".into(),
            default_repository_owner: "my-org".into(),
            code_directory: PathBuf::from("/code"),
            tickets_directory: PathBuf::from("/tickets"),
            repositories: HashMap::new(),
        }
    }

    #[test]
    fn branch_name_includes_description() {
        let cfg = base_config();
        let desc = "Short Summary".to_string();
        let name = build_branch_name(&cfg, "JIRA-1", Some(&desc));
        assert_eq!(name, "feature/JIRA-1-short-summary");
    }

    #[test]
    fn ensure_not_inside_detects_nested() {
        let tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp-tests-destroy");
        let nested = tmp.join("child");
        fs::create_dir_all(&nested).unwrap();
        let cwd = env::current_dir().unwrap();
        env::set_current_dir(&nested).unwrap();
        let res = ensure_not_inside(&tmp);
        env::set_current_dir(cwd).unwrap();
        assert!(res.is_err());
    }
}
