use anyhow::{bail, Context, Result};
use git2::{BranchType, Commit, Repository, StatusOptions, WorktreeAddOptions};
use std::path::Path;

// Checks if a directory has uncommited changes (modified or staged)
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

// Create a worktree
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
            b
        }
        Err(_) => {
            // Case B: Branch does not exist. Create it from HEAD.
            let commit = get_base_commit(&repo, base_ref)?;

            repo.branch(branch_name, &commit, false)
                .context(format!("Failed to create branch '{}'", branch_name))?
        }
    };

    // 2. Add the worktree
    let mut worktree_options = WorktreeAddOptions::new();

    // We convert the Branch object into a Reference to pass to the options
    let branch_ref = branch.into_reference();
    worktree_options.reference(Some(&branch_ref));

    repo.worktree(
        branch_name, // metadata name for the worktree
        target_path, // disk path
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

pub fn clone_repo(url: &str, target: &Path) -> Result<()> {
    Repository::clone(url, target).context("Failed to clone repository")?;
    Ok(())
}
