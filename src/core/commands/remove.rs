//! Remove a repo worktree from an existing ticket with safety checks.

use crate::core::config::Config;
use crate::core::git;
use crate::core::ticket::Ticket;
use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use std::env;
use std::fs;
use std::path::PathBuf;

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
    match git::is_clean(&target_worktree) {
        Ok(true) => {}
        Ok(false) => bail!(
            "Worktree at {:?} has uncommitted changes. Commit or clean before removing.",
            target_worktree
        ),
        Err(e) => warn!(
            "Could not check clean status for {:?}: {}",
            target_worktree, e
        ),
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
            build_worktree_name(
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

    if let Err(e) = git::remove_worktree(&repo_def.path, &worktree_name) {
        warn!(
            "Failed to prune worktree metadata '{}' for repo '{}': {}",
            worktree_name, repo_alias, e
        );
    }

    info!(
        "Removed worktree '{}' from ticket '{}'",
        repo_alias, ticket_meta.metadata.id
    );
    let _ = Ticket::remove_repo(&ticket_root, repo_alias);
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

fn build_worktree_name(config: &Config, ticket_id: &str, description: Option<&String>) -> String {
    let mut branch_name = format!("{}/{}", config.branch_prefix, ticket_id);
    if let Some(desc) = description {
        let sanitized = crate::core::commands::setup::sanitize_description(desc);
        if !sanitized.is_empty() {
            branch_name.push('-');
            branch_name.push_str(&sanitized);
        }
    }
    branch_name.replace('/', "_")
}

#[cfg(test)]
mod tests {
    use super::{build_worktree_name, find_ticket_root_from_cwd};
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
    fn worktree_name_uses_branch_sanitization() {
        let cfg = base_config();
        let desc = "Short Summary".to_string();
        let name = build_worktree_name(&cfg, "JIRA-1", Some(&desc));
        assert_eq!(name, "feature_JIRA-1-short-summary");
    }

    #[test]
    fn find_ticket_root_walks_upwards() {
        let tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp-tests-remove");
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
