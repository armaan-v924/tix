//! Destroy a ticket workspace after safety checks.

use crate::core::commands::common::build_branch_name;
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{Context, Result, anyhow, bail};
use log::{debug, info, warn};
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

    let worktree_dirs = worktree_dirs(&ticket_dir);
    debug!("Found worktree directories: {:?}", worktree_dirs);
    let aliases_to_prune = aliases_to_prune(&worktree_dirs, ticket_meta.as_ref());

    // Safety checks: ensure clean unless --force
    if !force {
        for dir in &worktree_dirs {
            let is_clean = git::is_clean(dir)
                .with_context(|| format!("Could not check clean status for {:?}", dir))?;
            if !is_clean {
                return Err(anyhow!(
                    "Worktree at {:?} has uncommitted changes. Use --force to override.",
                    dir
                ));
            }
        }
    }

    // Remove directories
    for dir in &worktree_dirs {
        if dir.exists() {
            info!("Removing worktree directory {:?}", dir);
            fs::remove_dir_all(dir)
                .with_context(|| format!("Failed to remove worktree directory {:?}", dir))?;
        }
    }

    prune_worktrees(&config, ticket_id, ticket_meta.as_ref(), &aliases_to_prune)?;

    info!("Removing ticket directory {:?}", ticket_dir);
    fs::remove_dir_all(&ticket_dir)
        .with_context(|| format!("Failed to remove ticket directory {:?}", ticket_dir))?;
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

fn aliases_to_prune(
    worktree_dirs: &[PathBuf],
    meta: Option<&crate::core::ticket::TicketMetadata>,
) -> Vec<String> {
    if let Some(m) = meta {
        return m.repo_branches.keys().cloned().collect::<Vec<String>>();
    }

    let mut aliases = Vec::new();
    for dir in worktree_dirs {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            aliases.push(name.to_string());
        }
    }
    aliases
}

fn prune_worktrees(
    config: &Config,
    ticket_id: &str,
    meta: Option<&crate::core::ticket::TicketMetadata>,
    aliases: &[String],
) -> Result<()> {
    for alias in aliases {
        let repo_def = match config.repositories.get(alias) {
            Some(def) => def,
            None => {
                warn!(
                    "Repo alias '{}' not found in config; skipping worktree pruning",
                    alias
                );
                continue;
            }
        };

        let branch = meta
            .and_then(|m| m.repo_branches.get(alias))
            .cloned()
            .or_else(|| meta.map(|m| m.branch.clone()))
            .unwrap_or_else(|| {
                warn!(
                    "No stored branch for repo '{}'; deriving branch name for pruning",
                    alias
                );
                build_branch_name(config, ticket_id, meta.and_then(|m| m.description.as_ref()))
            });
        let worktree_name = meta
            .and_then(|m| m.repo_worktrees.get(alias))
            .cloned()
            .unwrap_or_else(|| {
                warn!(
                    "No stored worktree name for repo '{}'; deriving from branch '{}'",
                    alias, branch
                );
                crate::core::ticket::worktree_name_for_branch(&branch)
            });

        debug!(
            "Pruning worktree metadata '{}' in repo {:?}",
            worktree_name, repo_def.path
        );
        git::remove_worktree(&repo_def.path, &worktree_name).with_context(|| {
            format!(
                "Failed to prune worktree '{}' for repo '{}' at {:?}",
                worktree_name, alias, repo_def.path
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_not_inside;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[test]
    fn ensure_not_inside_detects_nested() {
        static CWD_LOCK: Mutex<()> = Mutex::new(());
        let _guard = CWD_LOCK.lock().unwrap();

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
