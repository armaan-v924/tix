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
        debug!(alias = %self.alias, path = %self.code_path.display(), "opening repository");
        let repo = git2::Repository::open(&self.code_path).map_err(|e| {
            error!(alias = %self.alias, error = %e, "failed to open repository");
            TixError::GitError(e)
        })?;
        debug!(alias = %self.alias, "repository opened");
        Ok(Repository { config: self, repo })
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
        info!(alias = %self.alias, remote = %self.remote, "cloning repository");
        let repo = git2::Repository::clone(&self.remote, &self.code_path).map_err(|e| {
            error!(alias = %self.alias, error = %e, "clone failed");
            TixError::GitError(e)
        })?;
        info!(alias = %self.alias, path = %self.code_path.display(), "clone complete");
        Ok(Repository { config: self, repo })
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

        if let Some(parent) = ticket.path.parent() {
            std::fs::create_dir_all(parent).map_err(TixError::IoError)?;
        }

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
        opts.working_tree(true).valid(true);

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
        debug!(alias = %self.config.alias, branch, force, "syncing repository");

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
            if !analysis.is_fast_forward() && !analysis.is_up_to_date() {
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

        debug!(alias = %self.config.alias, branch, "sync complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    // use test_helpers::*;

    // --- clone_remote / resolve ---

    /// A valid remote clones successfully and returns a `Repository` pointed at the workdir.
    #[test]
    fn test_clone_remote_valid() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let config = RepositoryConfig::new(
            dir.path().join("remote").to_str().unwrap().into(),
            "test".into(),
            dir.path().join("local"),
        );
        assert!(config.clone_remote().is_ok());
    }

    /// An invalid/nonexistent remote URL returns `TixError::GitError`.
    #[test]
    fn test_clone_remote_invalid_url() {
        let dir = tempdir().unwrap();
        let config = RepositoryConfig::new(
            "https://invalid.invalid/repo.git".into(),
            "test".into(),
            dir.path().join("local"),
        );
        assert!(matches!(config.clone_remote(), Err(TixError::GitError(_))));
    }

    /// A valid path opens successfully and returns a `Repository`.
    #[test]
    fn test_resolve_valid() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let config = RepositoryConfig::new(
            dir.path().join("remote").to_str().unwrap().into(),
            "test".into(),
            dir.path().join("local"),
        );
        assert!(config.resolve().is_ok());
    }

    /// A path that is not a git repository returns `TixError::GitError`.
    #[test]
    fn test_resolve_invalid_path() {
        let remote = "this/is/an/invalid/repo/dir";
        let dir = tempdir().unwrap();
        let config = RepositoryConfig::new(
            remote.into(),
            "test".into(),
            dir.path().join("local").into(),
        );
        assert!(matches!(config.resolve(), Err(TixError::GitError(_))));
    }

    // --- create_worktree ---

    /// A ticket whose branch doesn't exist locally creates a new branch and returns the correct `Worktree`.
    #[test]
    fn test_create_worktree_new_branch() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        let worktree = repo.create_worktree(&ticket, false).unwrap();

        assert_eq!(worktree.branch, "feature");
        assert_eq!(worktree.path, dir.path().join("worktrees/feature"));
    }

    /// A ticket whose branch already exists locally reuses it and returns the correct `Worktree`.
    #[test]
    fn test_create_worktree_existing_local() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        {
            let oid = local.head().unwrap().target().unwrap();
            let commit = local.find_commit(oid).unwrap();
            local.branch("feature", &commit, false).unwrap();
        }
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        let worktree = repo.create_worktree(&ticket, false).unwrap();

        assert_eq!(worktree.branch, "feature");
        assert_eq!(worktree.path, dir.path().join("worktrees/feature"));
    }

    /// Creating a worktree with a name that already exists returns `TixError::GitError`.
    #[test]
    fn test_create_worktree_duplicate_name() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        repo.create_worktree(&ticket, false).unwrap();

        assert!(matches!(
            repo.create_worktree(&ticket, false),
            Err(TixError::GitError(_))
        ));
    }

    /// A sync failure propagates as an error before the worktree is created.
    #[test]
    fn test_create_worktree_sync_error() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_mid_operation(&local);
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        assert!(matches!(
            repo.create_worktree(&ticket, false),
            Err(TixError::Message(_))
        ));
    }

    // --- remove_worktree ---

    /// Removing a worktree that doesn't exist returns `TixError::Message`.
    #[test]
    fn test_remove_worktree_not_found() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket =
            test_helpers::make_ticket("nonexistent", &dir.path().join("worktrees/nonexistent"));
        assert!(matches!(
            repo.remove_worktree(&ticket, "nonexistent", false),
            Err(TixError::Message(_))
        ));
    }

    /// Removing an orphaned worktree (missing directory) without `force` returns `TixError::Message`.
    #[test]
    fn test_remove_worktree_orphaned_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::orphan_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        assert!(matches!(
            repo.remove_worktree(&ticket, "feature", false),
            Err(TixError::Message(_))
        ));
    }

    /// Removing an orphaned worktree with `force` succeeds even though the directory is gone.
    #[test]
    fn test_remove_worktree_orphaned_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::orphan_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        assert!(repo.remove_worktree(&ticket, "feature", true).is_ok());
    }

    /// Removing a valid worktree succeeds and returns `Ok(())`.
    #[test]
    fn test_remove_worktree_valid() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let ticket = test_helpers::make_ticket("feature", &dir.path().join("worktrees/feature"));
        assert!(repo.remove_worktree(&ticket, "feature", false).is_ok());
    }

    // --- sync_base ---

    /// A repository mid-operation (e.g. mid-merge) returns an error regardless of `force`.
    #[test]
    fn test_sync_base_mid_operation() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_mid_operation(&local);
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("main", false),
            Err(TixError::Message(_))
        ));
        assert!(matches!(
            repo.sync_base("main", true),
            Err(TixError::Message(_))
        ));
    }

    /// A dirty working tree without `force` returns an error rather than discarding changes.
    #[test]
    fn test_sync_base_dirty_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_dirty(&local);
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("main", false),
            Err(TixError::Message(_))
        ));
    }

    /// A dirty working tree with `force` discards changes and syncs successfully.
    #[test]
    fn test_sync_base_dirty_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_dirty(&local);
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", true).is_ok());
    }

    /// A repository with no `origin` remote returns `TixError::GitError` on fetch.
    #[test]
    fn test_sync_base_no_origin() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        local.remote_delete("origin").unwrap();
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("main", false),
            Err(TixError::GitError(_))
        ));
    }

    /// A branch that doesn't exist on the remote returns `TixError::Message`.
    #[test]
    fn test_sync_base_branch_not_on_remote() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("nonexistent", false),
            Err(TixError::Message(_))
        ));
    }

    /// A branch that exists on the remote but not locally is created as a local tracking branch.
    #[test]
    fn test_sync_base_branch_only_on_remote() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        // create feature on the remote before cloning so it exists on origin but won't have
        // a local tracking branch created automatically — only origin/feature will exist
        test_helpers::add_remote_branch(&remote, "feature");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("feature", false).is_ok());
    }

    /// A branch already up to date with the remote syncs successfully without moving the pointer.
    #[test]
    fn test_sync_base_already_up_to_date() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", false).is_ok());
    }

    /// A branch behind the remote fast-forwards successfully and the local pointer advances.
    #[test]
    fn test_sync_base_fast_forward() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_commit_to_bare(&remote, "second commit");
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", false).is_ok());
    }

    /// A branch that has diverged from the remote without `force` returns an error.
    #[test]
    fn test_sync_base_diverged_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_commit_to_bare(&remote, "remote commit");
        test_helpers::add_commit(&local, "main", "local commit");
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("main", false),
            Err(TixError::Message(_))
        ));
    }

    /// A branch that has diverged from the remote with `force` resets to the remote state.
    #[test]
    fn test_sync_base_diverged_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_commit_to_bare(&remote, "remote commit");
        test_helpers::add_commit(&local, "main", "local commit");
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", true).is_ok());
    }

    /// A branch already checked out in another worktree returns `TixError::Message`.
    #[test]
    fn test_sync_base_branch_checked_out_in_worktree() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        // create feature on the remote so sync_base can fetch it
        test_helpers::add_remote_branch(&remote, "feature");
        // create a local worktree on "feature" — git creates and checks out the branch there
        test_helpers::add_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.sync_base("feature", false),
            Err(TixError::Message(_))
        ));
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
        let repo = git2::Repository::init_bare(path).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        repo
    }

    /// Clones the bare repository at `remote` into `dest`, setting `origin` automatically.
    pub fn clone_repo(remote: &Path, dest: &Path) -> git2::Repository {
        git2::Repository::clone(remote.to_str().unwrap(), dest).unwrap()
    }

    /// Wraps a `git2::Repository` in a [`Repository`], pointing `origin` at `remote`.
    pub fn tix_repo(remote: &Path, local: git2::Repository) -> Repository {
        let code_path = local.path().parent().unwrap().to_path_buf();
        Repository {
            config: RepositoryConfig::new(
                remote.to_str().unwrap().into(),
                "default".into(),
                code_path,
            ),
            repo: local,
        }
    }

    /// Commits the current index state to `branch` in a working-directory repository.
    /// Chains onto the existing branch tip so history is preserved across multiple calls.
    pub fn add_commit<'a>(
        repo: &'a git2::Repository,
        branch: &str,
        message: &str,
    ) -> git2::Commit<'a> {
        let branch_ref = repo.find_branch(branch, git2::BranchType::Local).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        // resolve the current branch tip as parent; None on the first commit (root).
        let parent = branch_ref
            .get()
            .target()
            .and_then(|oid| repo.find_commit(oid).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = repo
            .commit(
                branch_ref.get().name().ok(),
                &sig,
                &sig,
                message,
                &tree,
                &parents,
            )
            .unwrap();
        repo.find_commit(oid).unwrap()
    }

    /// Commits directly to `HEAD` in a bare repository (no working directory or index staging).
    /// Chains onto the existing HEAD commit so history is preserved across multiple calls.
    pub fn add_commit_to_bare<'a>(repo: &'a git2::Repository, message: &str) -> git2::Commit<'a> {
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        // resolve HEAD as parent; None when the repo has no commits yet (root commit).
        let parent = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
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
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        repo.worktree(branch, path, None).unwrap()
    }

    /// Creates a worktree for `branch` then deletes its directory, leaving the git
    /// metadata intact so `worktree.validate()` returns `Err`.
    pub fn orphan_worktree(repo: &git2::Repository, branch: &str, path: &Path) -> git2::Worktree {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let worktree = repo.worktree(branch, path, None).unwrap();

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
        repo.reference(
            &format!("refs/heads/{}", branch),
            head_oid,
            false,
            "add_remote_branch",
        )
        .unwrap();
    }
}
