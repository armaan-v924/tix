//! Git helpers built on `git2` for worktree management and safety checks.

use anyhow::{Context, Result};
use git2::build::CheckoutBuilder;
use git2::{BranchType, Commit, Repository, StatusOptions, WorktreeAddOptions};
use log::debug;
use std::path::Path;

/// Return `true` if the repository at `repo_path` has no modified/staged/untracked files.
pub fn is_clean(repo_path: &Path) -> Result<bool> {
    // Open the repo
    let repo =
        Repository::open(repo_path).context("Failed to open repository to check the status")?;

    // Configure status options (include untracked, exclude ignored)
    let mut options = StatusOptions::new();
    options.include_untracked(true);

    let statuses = repo
        .statuses(Some(&mut options))
        .context("Failed to read repository status.")?;

    // If there are 0 entries in the status list, it's clean.
    Ok(statuses.is_empty())
}

/// Create a git worktree at `target_path`, using `branch_name`, optionally created from `base_ref`.
pub fn create_worktree(
    repo_path: &Path,
    target_path: &Path,
    branch_name: &str,
    base_ref: Option<&str>,
) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open source repository")?;

    // 1. Resolve the branch
    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => {
            // Case A: Branch already exists, we'll use it.
            debug!("Found existing branch '{}'", branch_name);
            b
        }
        Err(_) => {
            // Case B: Branch does not exist. Create it from HEAD.
            debug!("Branch '{}' not found. Creating from base...", branch_name);

            let default = resolve_default_branch(&repo);
            let base = base_ref.or(default.as_deref());
            let commit = get_base_commit(&repo, base)?;

            repo.branch(branch_name, &commit, false)
                .context(format!("Failed to create branch '{}'", branch_name))?
        }
    };

    // 2. Add the worktree
    let mut worktree_options = WorktreeAddOptions::new();

    // We convert the Branch object into a Reference to pass to the options
    let branch_ref = branch.into_reference();
    worktree_options.reference(Some(&branch_ref));

    let worktree_name = branch_name.replace('/', "_");

    repo.worktree(
        &worktree_name, // metadata name for the worktree
        target_path,    // disk path
        Some(&mut worktree_options),
    )
    .context("Failed to create a worktree")?;

    Ok(())
}

fn get_base_commit<'a>(repo: &'a Repository, base: Option<&str>) -> Result<Commit<'a>> {
    let obj = match base {
        Some(rev) => {
            // revparse_single handles `main`, `origin/master`, `HEAD^`, etc.
            repo.revparse_single(rev)
                .context(format!("Could not find base reference '{}'", rev))?
        }
        None => {
            // Default to HEAD if no base provided
            repo.head()
                .context("Repo has no HEAD")?
                .peel_to_commit()
                .context("HEAD is not a commit")?
                .into_object()
        }
    };

    obj.peel_to_commit()
        .context("Base reference did not point to a commit")
}

/// Remove worktree metadata by name from the repository at `repo_path`.
pub fn remove_worktree(repo_path: &Path, worktree_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open repository")?;

    // Attempt to find the worktree by name
    let worktree = match repo.find_worktree(worktree_name) {
        Ok(wt) => wt,
        Err(_) => {
            return Ok(());
        }
    };

    worktree
        .prune(None)
        .context("Failed to prune worktree metadata")?;

    Ok(())
}

/// Clone a repository to `target`.
pub fn clone_repo(url: &str, target: &Path) -> Result<()> {
    Repository::clone(url, target).context("Failed to clone repository")?;
    Ok(())
}

/// Fetch from `remote_name` and fast-forward the current branch to its upstream if possible.
pub fn fetch_and_fast_forward(repo_path: &Path, remote_name: &str) -> Result<()> {
    let repo = Repository::open(repo_path).context("Failed to open repository for fetch")?;

    let mut remote = repo
        .find_remote(remote_name)
        .context(format!("Remote '{}' not found", remote_name))?;

    debug!(
        "Fetching all branches from '{}' in repo {:?}",
        remote_name, repo_path
    );
    let refspec = format!("refs/heads/*:refs/remotes/{}/*", remote_name);
    remote
        .fetch(&[&refspec], None, None)
        .context("Fetch failed")?;

    let head = match repo.head() {
        Ok(h) if h.is_branch() => h,
        _ => return Ok(()), // Detached or no head; nothing to fast-forward
    };

    let head_name = head.name().map(|n| n.to_string()).unwrap_or_default();
    if head_name.is_empty() {
        return Ok(());
    }

    let shorthand = head.shorthand().unwrap_or_default().to_string();
    let local_branch = repo
        .find_branch(&shorthand, BranchType::Local)
        .context("Failed to find local branch for HEAD")?;

    let upstream = match local_branch.upstream() {
        Ok(u) => u,
        Err(_) => return Ok(()), // No upstream configured
    };

    let upstream_oid = upstream
        .into_reference()
        .target()
        .context("Upstream reference had no target")?;
    let annotated = repo.find_annotated_commit(upstream_oid)?;
    let (analysis, _pref) = repo.merge_analysis(&[&annotated])?;

    if analysis.is_up_to_date() {
        debug!("Branch '{}' already up to date with upstream", shorthand);
        return Ok(());
    }

    if analysis.is_fast_forward() {
        debug!(
            "Fast-forwarding branch '{}' to upstream ({})",
            shorthand, upstream_oid
        );
        let mut reference = repo
            .find_reference(&head_name)
            .context("Failed to find HEAD reference for fast-forward")?;
        reference
            .set_target(upstream_oid, "Fast-forward to upstream")
            .context("Failed to set reference target during fast-forward")?;
        repo.set_head(&head_name)?;
        repo.checkout_head(Some(
            CheckoutBuilder::default().force(), // ensure worktree matches new commit
        ))?;
    }

    Ok(())
}

/// Resolve the default branch reference (e.g., origin/HEAD) to a revspec string.
pub fn resolve_default_branch(repo: &Repository) -> Option<String> {
    // Try remote HEAD first
    if let Ok(remote) = repo.find_remote("origin") {
        if let Ok(head) = remote.default_branch() {
            if let Some(name) = head.as_str() {
                return Some(name.to_string());
            }
        }
    }

    // Fallback to symbolic reference of HEAD
    repo.head()
        .ok()
        .and_then(|h| h.resolve().ok())
        .and_then(|r| r.name().map(|s| s.to_string()))
}
