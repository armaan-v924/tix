//! Add a repo worktree to an existing ticket.

use crate::core::commands::setup::sanitize_description;
use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use std::env;
use std::path::{Path, PathBuf};

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
    let base_ref = branch.map(|s| s.to_string());

    info!(
        "Adding worktree for repo '{}' into {:?} on branch '{}'",
        repo_alias, target_worktree, branch_name
    );

    // Ensure repo is up to date before branching.
    git::fetch_and_fast_forward(&repo_def.path, "origin")
        .map_err(|e| {
            warn!(
                "Failed to update repo '{}' at {:?}: {}. Continuing.",
                repo_alias, repo_def.path, e
            );
            e
        })
        .ok();

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

fn locate_ticket_root(ticket: Option<&str>, config: &Config) -> Result<PathBuf> {
    if let Some(id) = ticket {
        return Ok(config.tickets_directory.join(id));
    }

    if let Some(dir) = find_ticket_root_from_cwd() {
        return Ok(dir);
    }

    bail!("Could not infer ticket. Run inside a ticket directory or provide --ticket.");
}

fn find_ticket_root_from_cwd() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        let candidate = current.join(".tix").join("info.toml");
        if candidate.exists() {
            return Some(current);
        }

        if !current.pop() {
            break;
        }
    }
    None
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
    use super::{build_branch_name, find_ticket_root_from_cwd};
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
    fn find_ticket_root_walks_upwards() {
        let tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp-tests-add");
        let nested = tmp.join("nested/child");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(tmp.join(".tix")).unwrap();
        fs::write(tmp.join(".tix/info.toml"), "dummy").unwrap();

        let cwd = env::current_dir().unwrap();
        env::set_current_dir(&nested).unwrap();
        let found = find_ticket_root_from_cwd();
        env::set_current_dir(cwd).unwrap();

        assert_eq!(found.unwrap(), tmp);
    }
}
