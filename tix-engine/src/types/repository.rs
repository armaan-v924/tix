use crate::types::{errors::TixError, ticket::Ticket, worktree::Worktree};
use git2::{RepositoryState, WorktreeAddOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

#[derive(Serialize, Deserialize, Clone)]
pub struct RepositoryConfig {
    pub remote: String,
    pub alias: String,
    pub code_path: PathBuf,
}

impl RepositoryConfig {
    pub fn new(remote: String, alias: String, code_path: PathBuf) -> Self {
        Self {
            remote,
            alias,
            code_path,
        }
    }

    pub fn resolve(self) -> Result<Repository, TixError> {
        todo!()
    }

    pub fn clone_remote(self) -> Result<Repository, TixError> {
        todo!()
    }
}

pub struct Repository {
    pub config: RepositoryConfig,
    #[allow(unused)]
    repo: git2::Repository,
}

impl Repository {
    pub fn new(config: RepositoryConfig, repo: git2::Repository) -> Self {
        // TODO: ensure worktree directory and code directory exist.
        // TODO: Register to config
        // TODO: validate remote url
        Self { config, repo }
    }

    pub fn git_clone(&self) -> Result<PathBuf, TixError> {
        info!(alias = %self.config.alias, remote = %self.config.remote, "cloning repository");
        let git_repo = git2::Repository::clone(&self.config.remote, &self.config.code_path)
            .map_err(|e| {
                error!(alias = %self.config.alias, error = %e, "clone failed");
                TixError::GitError(e)
            })?;
        let path = git_repo
            .workdir()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| TixError::from("no workdir found"))?;
        info!(alias = %self.config.alias, path = %path.display(), "clone complete");
        Ok(path)
    }

    fn git_repo(&self) -> Result<git2::Repository, TixError> {
        git2::Repository::open(&self.config.code_path).map_err(TixError::GitError)
    }

    pub fn create_worktree(&self, ticket: &Ticket, force: bool) -> Result<Worktree, TixError> {
        info!(alias = %self.config.alias, ticket = %ticket.key, branch = %ticket.branch, "creating worktree");
        self.sync(force)?;
        let repo = self.git_repo()?;

        let branch = repo
            .find_branch(&ticket.branch, git2::BranchType::Local)
            .ok();
        let reference = branch.map(|b| b.into_reference());

        let mut options = WorktreeAddOptions::new();
        options.reference(reference.as_ref());

        let worktree = repo.worktree(&ticket.branch, &ticket.path, Some(&options))
            .map(|_| Worktree {
                repo_alias: self.config.alias.clone(),
                path: ticket.path.clone(),
            })
            .map_err(|e| {
                error!(alias = %self.config.alias, ticket = %ticket.key, error = %e, "failed to create worktree");
                TixError::GitError(e)
            })?;
        info!(alias = %self.config.alias, ticket = %ticket.key, path = %ticket.path.display(), "worktree created");
        Ok(worktree)
    }

    pub fn remove_worktree(&self, ticket: &Ticket, force: bool) -> Result<(), TixError> {
        info!(alias = %self.config.alias, ticket = %ticket.key, branch = %ticket.branch, "removing worktree");
        let repo = self.git_repo()?;

        let worktree = repo
            .find_worktree(&ticket.branch)
            .map_err(|_| TixError::from("worktree does not exist"))?;

        if !force && worktree.validate().is_err() {
            error!(alias = %self.config.alias, ticket = %ticket.key, "worktree is dirty, use force to remove");
            return Err(TixError::from(
                "worktree is not in a clean state, use force to remove",
            ));
        }

        let mut opts = git2::WorktreePruneOptions::new();
        opts.working_tree(true);

        worktree.prune(Some(&mut opts)).map_err(|e| {
            error!(alias = %self.config.alias, ticket = %ticket.key, error = %e, "failed to remove worktree");
            TixError::GitError(e)
        })?;
        info!(alias = %self.config.alias, ticket = %ticket.key, "worktree removed");
        Ok(())
    }

    pub fn sync(&self, force: bool) -> Result<(), TixError> {
        self.sync_base("main", force)
    }

    pub fn sync_base(&self, branch: &str, force: bool) -> Result<(), TixError> {
        info!(alias = %self.config.alias, branch, force, "syncing repository");
        let repo = self.git_repo()?;

        // RepositoryState::Clean means no git operation is in progress.
        // force cannot recover from this — the user must resolve it manually.
        if repo.state() != RepositoryState::Clean {
            error!(alias = %self.config.alias, "repository is mid-operation, cannot sync");
            return Err(TixError::from(
                "repository is mid-operation (merge, rebase, etc), resolve it before syncing",
            ));
        }

        // statuses() returns all files with a non-clean state (modified, untracked, etc).
        // if anything is dirty and force is not set, bail rather than silently discarding work.
        if !force {
            let statuses = repo.statuses(None).map_err(TixError::GitError)?;
            if !statuses.is_empty() {
                error!(alias = %self.config.alias, count = statuses.len(), "repository has uncommitted changes");
                return Err(TixError::from(
                    "repository has uncommitted changes, use force to discard them",
                ));
            }
        }

        // fetch so we have up-to-date remote refs before checking branch existence.
        // this also means if the branch only exists on origin and not locally,
        // we'll have it available to create a local tracking branch from.
        debug!(alias = %self.config.alias, branch, "fetching from origin");
        let mut remote = repo.find_remote("origin").map_err(TixError::GitError)?;
        remote
            .fetch(&[branch], None, None)
            .map_err(TixError::GitError)?;

        // check if the branch exists locally. if not, try to create it from
        // the remote ref we just fetched — if origin doesn't have it either, error.
        if repo.find_branch(branch, git2::BranchType::Local).is_err() {
            debug!(alias = %self.config.alias, branch, "branch not found locally, creating from origin");
            let remote_ref = format!("refs/remotes/origin/{}", branch);
            let remote_commit = repo
                .find_reference(&remote_ref)
                .map_err(|_| TixError::from(format!("branch '{}' not found on origin", branch)))?;
            let commit = repo
                .reference_to_annotated_commit(&remote_commit)
                .map_err(TixError::GitError)?;
            let target = repo.find_commit(commit.id()).map_err(TixError::GitError)?;
            // false = don't force-overwrite if it somehow already exists
            repo.branch(branch, &target, false)
                .map_err(TixError::GitError)?;
            debug!(alias = %self.config.alias, branch, "local branch created from origin");
        }

        // check divergence before touching anything — if not fast-forwardable and
        // not force, we'd rather error now than after nuking local changes.
        let ref_name = format!("refs/heads/{}", branch);
        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .map_err(TixError::GitError)?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(TixError::GitError)?;
        if !force {
            // merge_analysis checks whether applying the remote commit is a
            // fast-forward — i.e. our local branch is a direct ancestor of the
            // remote, so we can move the pointer forward without losing anything.
            let (analysis, _) = repo
                .merge_analysis(&[&fetch_commit])
                .map_err(TixError::GitError)?;
            if !analysis.is_fast_forward() {
                error!(alias = %self.config.alias, branch, "branch has diverged from remote");
                return Err(TixError::from(format!(
                    "'{}' has diverged from remote, use force to reset",
                    branch
                )));
            }
        }

        // git won't let you switch to a branch that's already checked out in another
        // worktree — each worktree must have a unique branch. we check all registered
        // worktrees and error with which one has the branch so the user knows where to look.
        let worktrees = repo.worktrees().map_err(TixError::GitError)?;
        for name in worktrees.iter().filter_map(|n| n.ok().flatten()) {
            // open the worktree as its own Repository so we can inspect its HEAD
            let wt = repo.find_worktree(name).map_err(TixError::GitError)?;
            let wt_repo = git2::Repository::open(wt.path()).map_err(TixError::GitError)?;
            if let Ok(head) = wt_repo.head() {
                // shorthand() gives the branch name (e.g. "main") rather than the
                // full ref path (e.g. "refs/heads/main")
                if head.shorthand() == Ok(branch) {
                    error!(alias = %self.config.alias, branch, worktree = name, "branch is checked out in another worktree");
                    return Err(TixError::from(format!(
                        "branch '{}' is checked out in worktree '{}'",
                        branch, name
                    )));
                }
            }
        }

        // --- destructive work starts here ---

        if force {
            // checkout_head rewrites the working tree to match HEAD.
            // force() discards modified tracked files, remove_untracked removes
            // files git doesn't know about, remove_ignored removes ignored files.
            // together this is a full nuke of local changes.
            warn!(alias = %self.config.alias, "force flag set — discarding all local changes");
            let mut clean_opts = git2::build::CheckoutBuilder::new();
            clean_opts
                .force()
                .remove_untracked(true)
                .remove_ignored(true);
            repo.checkout_head(Some(&mut clean_opts))
                .map_err(TixError::GitError)?;
        }

        // set_head points HEAD at the branch ref without touching the working tree.
        // checkout_head then updates the working tree to match.
        debug!(alias = %self.config.alias, branch, "switching HEAD");
        repo.set_head(&ref_name).map_err(TixError::GitError)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(TixError::GitError)?;

        // move the local branch pointer to the fetched remote commit
        let mut branch_ref = repo.find_reference(&ref_name).map_err(TixError::GitError)?;
        if force {
            // discard any local commits that aren't on the remote
            debug!(alias = %self.config.alias, branch, "force resetting to remote");
            branch_ref
                .set_target(fetch_commit.id(), "sync_base: force reset")
                .map_err(TixError::GitError)?;
        } else {
            debug!(alias = %self.config.alias, branch, "fast-forwarding to remote");
            branch_ref
                .set_target(fetch_commit.id(), "sync_base: fast-forward")
                .map_err(TixError::GitError)?;
        }

        // final checkout to update the working tree to the new branch pointer
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(TixError::GitError)?;

        info!(alias = %self.config.alias, branch, "sync complete");
        Ok(())
    }
}
