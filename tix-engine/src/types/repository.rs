use crate::types::{errors::TixError, ticket::Ticket, worktree::Worktree};
use git2::{RepositoryState, WorktreeAddOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// The configuration for a repository.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepositoryConfig {
    /// The remote URL of the repository.
    pub remote: String,
    /// The alias of the repository.
    pub alias: String,
    /// The path to the code directory of the repository.
    pub code_path: PathBuf,
}

impl RepositoryConfig {
    /// Creates a new `RepositoryConfig`.
    ///
    /// # Arguments
    ///
    /// * `remote` - The remote URL of the repository.
    /// * `alias` - The alias of the repository.
    /// * `code_path` - The path to the code directory of the repository.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::PathBuf;
    /// # use tix_engine::RepositoryConfig;
    /// let config = RepositoryConfig::new("https://github.com/user/repo.git".to_string(), "alias".to_string(), PathBuf::from("~/code/repo"));
    /// ```
    pub fn new(remote: String, alias: String, code_path: PathBuf) -> Self {
        Self {
            remote,
            alias,
            code_path,
        }
    }

    /// Opens the already-cloned repository at [`Self::code_path`] and returns a live [`Repository`].
    ///
    /// Use [`Self::clone_remote`] if the repository has not been cloned yet.
    ///
    /// # Errors
    ///
    /// Returns [`TixError::GitError`] if the path is not a valid git repository.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let config = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// );
    /// let repo = config.resolve();
    /// ```
    pub fn resolve(self) -> Result<Repository, TixError> {
        todo!()
    }

    /// Clones [`Self::remote`] into [`Self::code_path`] and returns a live [`Repository`].
    ///
    /// Use [`Self::resolve`] if the repository is already cloned locally.
    ///
    /// # Errors
    ///
    /// Returns [`TixError::GitError`] if the clone fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let config = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// );
    /// let repo = config.clone_remote();
    /// ```
    pub fn clone_remote(self) -> Result<Repository, TixError> {
        todo!()
    }
}

/// Represents a Git repository.
pub struct Repository {
    /// The configuration of the repository.
    pub config: RepositoryConfig,
    repo: git2::Repository,
}

impl Repository {
    /// Creates a new `Repository` from a config and an already-open [`git2::Repository`].
    ///
    /// Prefer constructing via [`RepositoryConfig::resolve`] or [`RepositoryConfig::clone_remote`]
    /// rather than calling this directly.
    pub fn new(config: RepositoryConfig, repo: git2::Repository) -> Self {
        // TODO: ensure worktree directory and code directory exist.
        // TODO: Register to config
        // TODO: validate remote url
        Self { config, repo }
    }

    /// Clones the repository from [`RepositoryConfig::remote`] into [`RepositoryConfig::code_path`].
    ///
    /// Prefer calling this via [`RepositoryConfig::clone_remote`], which constructs the
    /// `Repository` for you after the clone succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`TixError::GitError`] if the clone fails.
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

    /// Creates a git worktree for the given ticket's branch at the ticket's path.
    ///
    /// Syncs the repository first (fetches and fast-forwards). Pass `force: true` to discard
    /// local changes before syncing.
    ///
    /// # Errors
    ///
    /// Returns [`TixError::GitError`] if the sync or worktree creation fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use tix_engine::Ticket;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve().unwrap();
    /// let ticket = Ticket { key: "JIRA-123".into(), description: "Fix bug".into(), branch: "JIRA-123".into(), path: PathBuf::from("/home/user/tickets/JIRA-123"), worktrees: vec![] };
    /// let worktree = repo.create_worktree(&ticket, false);
    /// ```
    pub fn create_worktree(&self, ticket: &Ticket, force: bool) -> Result<Worktree, TixError> {
        info!(alias = %self.config.alias, ticket = %ticket.key, branch = %ticket.branch, "creating worktree");
        self.sync(force)?;

        let branch = self
            .repo
            .find_branch(&ticket.branch, git2::BranchType::Local)
            .ok();
        let reference = branch.map(|b| b.into_reference());

        let mut options = WorktreeAddOptions::new();
        options.reference(reference.as_ref());

        let worktree = self.repo.worktree(&ticket.branch, &ticket.path, Some(&options))
            .map(|_| Worktree {
                branch: ticket.branch.clone(),
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

    /// Prunes the git worktree for the given `branch` from this repository.
    ///
    /// Pass `force: true` to remove even if the worktree is in a dirty state. Without `force`,
    /// returns an error if the worktree fails validation (e.g. has uncommitted changes).
    ///
    /// # Errors
    ///
    /// - [`TixError::Message`] if the worktree does not exist or is dirty without `force`
    /// - [`TixError::GitError`] if pruning fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use tix_engine::Ticket;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve().unwrap();
    /// let ticket = Ticket { key: "JIRA-123".into(), description: "Fix bug".into(), branch: "JIRA-123".into(), path: PathBuf::from("/home/user/tickets/JIRA-123"), worktrees: vec![] };
    /// repo.remove_worktree(&ticket, "JIRA-123", false);
    /// ```
    pub fn remove_worktree(
        &self,
        ticket: &Ticket,
        branch: &str,
        force: bool,
    ) -> Result<(), TixError> {
        info!(alias = %self.config.alias, ticket = %ticket.key, branch = %ticket.branch, "removing worktree");

        let worktree = self
            .repo
            .find_worktree(branch)
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

    /// Syncs the repository against the `main` branch. See [`Self::sync_base`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve().unwrap();
    /// repo.sync(false);
    /// ```
    pub fn sync(&self, force: bool) -> Result<(), TixError> {
        self.sync_base("main", force)
    }

    /// Fetches and fast-forwards `branch` from `origin`.
    ///
    /// Errors if the repository is mid-operation (merge, rebase, etc.) or if the local branch has
    /// diverged from remote without `force`. With `force`, discards all local changes and resets
    /// to the remote state. Also errors if the branch is already checked out in another worktree.
    ///
    /// # Errors
    ///
    /// - [`TixError::Message`] for dirty state, diverged branch, or worktree conflicts
    /// - [`TixError::GitError`] for underlying git failures
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     "repo".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve().unwrap();
    /// repo.sync_base("main", false);
    /// ```
    pub fn sync_base(&self, branch: &str, force: bool) -> Result<(), TixError> {
        info!(alias = %self.config.alias, branch, force, "syncing repository");

        // RepositoryState::Clean means no git operation is in progress.
        // force cannot recover from this — the user must resolve it manually.
        if self.repo.state() != RepositoryState::Clean {
            error!(alias = %self.config.alias, "repository is mid-operation, cannot sync");
            return Err(TixError::from(
                "repository is mid-operation (merge, rebase, etc), resolve it before syncing",
            ));
        }

        // statuses() returns all files with a non-clean state (modified, untracked, etc).
        // if anything is dirty and force is not set, bail rather than silently discarding work.
        if !force {
            let statuses = self.repo.statuses(None).map_err(TixError::GitError)?;
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
        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(TixError::GitError)?;
        remote
            .fetch(&[branch], None, None)
            .map_err(TixError::GitError)?;

        // check if the branch exists locally. if not, try to create it from
        // the remote ref we just fetched — if origin doesn't have it either, error.
        if self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .is_err()
        {
            debug!(alias = %self.config.alias, branch, "branch not found locally, creating from origin");
            let remote_ref = format!("refs/remotes/origin/{}", branch);
            let remote_commit = self
                .repo
                .find_reference(&remote_ref)
                .map_err(|_| TixError::from(format!("branch '{}' not found on origin", branch)))?;
            let commit = self
                .repo
                .reference_to_annotated_commit(&remote_commit)
                .map_err(TixError::GitError)?;
            let target = self
                .repo
                .find_commit(commit.id())
                .map_err(TixError::GitError)?;
            // false = don't force-overwrite if it somehow already exists
            self.repo
                .branch(branch, &target, false)
                .map_err(TixError::GitError)?;
            debug!(alias = %self.config.alias, branch, "local branch created from origin");
        }

        // check divergence before touching anything — if not fast-forwardable and
        // not force, we'd rather error now than after nuking local changes.
        let ref_name = format!("refs/heads/{}", branch);
        let fetch_head = self
            .repo
            .find_reference("FETCH_HEAD")
            .map_err(TixError::GitError)?;
        let fetch_commit = self
            .repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(TixError::GitError)?;
        if !force {
            // merge_analysis checks whether applying the remote commit is a
            // fast-forward — i.e. our local branch is a direct ancestor of the
            // remote, so we can move the pointer forward without losing anything.
            let (analysis, _) = self
                .repo
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
        let worktrees = self.repo.worktrees().map_err(TixError::GitError)?;
        for name in worktrees.iter().filter_map(|n| n.ok().flatten()) {
            // open the worktree as its own git2::Repository so we can inspect its HEAD
            let wt = self.repo.find_worktree(name).map_err(TixError::GitError)?;
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
            self.repo
                .checkout_head(Some(&mut clean_opts))
                .map_err(TixError::GitError)?;
        }

        // set_head points HEAD at the branch ref without touching the working tree.
        // checkout_head then updates the working tree to match.
        debug!(alias = %self.config.alias, branch, "switching HEAD");
        self.repo.set_head(&ref_name).map_err(TixError::GitError)?;
        self.repo
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(TixError::GitError)?;

        // move the local branch pointer to the fetched remote commit
        let mut branch_ref = self
            .repo
            .find_reference(&ref_name)
            .map_err(TixError::GitError)?;
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
        self.repo
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(TixError::GitError)?;

        info!(alias = %self.config.alias, branch, "sync complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_stub() {

    }
}

#[cfg(test)]
mod test_helpers {
    use super::*;
    use git2;
    use std::path::Path;

    /// Initialises an empty bare repository at `path`. Has no commits — call
    /// [`add_commit_to_bare`] immediately after to establish a branch.
    pub fn init_bare_repo(path: &Path) -> git2::Repository {
        git2::Repository::init_bare(path).unwrap()
    }

    /// Clones the bare repository at `remote` into `dest`, setting `origin` automatically.
    pub fn clone_repo(remote: &Path, dest: &Path) -> git2::Repository {
        git2::Repository::clone(remote.to_str().unwrap(), dest).unwrap()
    }

    /// Wraps a `git2::Repository` in a [`Repository`], pointing `origin` at `remote`.
    pub fn tix_repo(remote: &Path, local: git2::Repository) -> Repository {
        Repository {
            config: RepositoryConfig::new(remote.to_str().unwrap().into(), "default".into(), local.path().parent().unwrap().to_path_buf()),
            repo: local,
        }
    }

    /// Commits the current index state to `branch` in a working-directory repository.
    /// Chains onto the existing branch tip so history is preserved across multiple calls.
    pub fn add_commit(repo: &git2::Repository, branch: &str, message: &str) -> git2::Commit {
        let branch_ref = repo.find_branch(branch, git2::BranchType::Local).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        // resolve the current branch tip as parent; None on the first commit (root).
        let parent = branch_ref.get().target().and_then(|oid| repo.find_commit(oid).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = repo.commit(branch_ref.get().name().ok(), &sig, &sig, message, &tree, &parents).unwrap();
        repo.find_commit(oid).unwrap()
    }

    /// Commits directly to `HEAD` in a bare repository (no working directory or index staging).
    /// Chains onto the existing HEAD commit so history is preserved across multiple calls.
    pub fn add_commit_to_bare(repo: &git2::Repository, message: &str) -> git2::Commit {
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        // resolve HEAD as parent; None when the repo has no commits yet (root commit).
        let parent = repo.head().ok().and_then(|h| h.target()).and_then(|oid| repo.find_commit(oid).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap();
        repo.find_commit(oid).unwrap()
    }

    /// Writes an untracked file to the working directory so `repo.statuses()` is non-empty.
    pub fn make_dirty(repo: &git2::Repository) {
        const DIRTY_FILE: &str = "this is a dirty file";
        const DIRTY_PATH: &str = "dirty.txt";
        let path = repo.workdir().unwrap().join(DIRTY_PATH);
        std::fs::write(&path, DIRTY_FILE).unwrap();
    }

    /// Writes `.git/MERGE_HEAD` to put the repository into a non-`Clean` state,
    /// simulating a mid-merge without actually creating a conflict.
    pub fn make_mid_operation(repo: &git2::Repository) {
        let commit_sha = repo.head().unwrap().target().unwrap().to_string();
        let path = repo.path().join("MERGE_HEAD");
        std::fs::write(&path, commit_sha).unwrap();
    }

    /// Creates a git worktree for `branch` at `path`.
    pub fn add_worktree(repo: &git2::Repository, branch: &str, path: &Path) -> git2::Worktree {
        repo.worktree(
            branch,
            path,
            None,
        ).unwrap()
    }

    /// Creates a worktree for `branch` then deletes its directory, leaving the git
    /// metadata intact so `worktree.validate()` returns `Err`.
    pub fn orphan_worktree(repo: &git2::Repository, branch: &str, path: &Path) -> git2::Worktree {
        let worktree = repo.worktree(
            branch,
            path,
            None,
        ).unwrap();

        std::fs::remove_dir_all(path).unwrap();
        worktree
    }

    /// Constructs a minimal [`Ticket`] with the given `branch` and `path`.
    pub fn make_ticket(branch: &str, path: &Path) -> Ticket {
        Ticket {
            key: branch.to_string(),
            description: "".to_string(),
            branch: branch.to_string(),
            path: path.to_path_buf(),
            worktrees: Vec::new(),
        }
    }

    /// Creates `branch` on the bare remote without creating it locally,
    /// simulating a branch that exists on origin but has never been fetched.
    pub fn add_remote_branch(repo: &git2::Repository, branch: &str) {
        let head_oid = repo.head().unwrap().target().unwrap();
        repo.reference(&format!("refs/heads/{}", branch), head_oid, false, "add_remote_branch").unwrap();
    }

}
