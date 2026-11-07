use crate::types::{errors::TixError, worktree::Worktree};
use git2::{RepositoryState, WorktreeAddOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// The configuration for a repository.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RepositoryConfig {
    /// The remote URL of the repository.
    pub remote: String,
    /// The path to the code directory of the repository.
    pub code_path: PathBuf,
}

impl RepositoryConfig {
    /// Creates a new `RepositoryConfig`.
    ///
    /// # Arguments
    ///
    /// * `remote` - The remote URL of the repository.
    /// * `code_path` - The path to the code directory of the repository.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::path::PathBuf;
    /// # use tix_engine::RepositoryConfig;
    /// let config = RepositoryConfig::new("https://github.com/user/repo.git".to_string(), PathBuf::from("~/code/repo"));
    /// ```
    pub fn new(remote: String, code_path: PathBuf) -> Self {
        Self { remote, code_path }
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
    ///     PathBuf::from("/home/user/code/repo"),
    /// );
    /// let repo = config.resolve("my-repo");
    /// ```
    pub fn resolve(self, alias: &str) -> Result<Repository, TixError> {
        debug!(alias = %alias, path = %self.code_path.display(), "opening repository");
        let repo = git2::Repository::open(&self.code_path).map_err(|e| {
            // debug, not error: failing to open is an expected probe on the
            // ensure() path (resolve-then-clone); the Err carries the story
            // for callers that treat it as fatal.
            debug!(alias = %alias, error = %e, "failed to open repository");
            TixError::GitError(e)
        })?;
        debug!(alias = %alias, "repository opened");
        Ok(Repository {
            alias: alias.to_string(),
            config: self,
            repo,
        })
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
    ///     PathBuf::from("/home/user/code/repo"),
    /// );
    /// let repo = config.clone_remote("my-repo");
    /// ```
    pub fn clone_remote(self, alias: &str) -> Result<Repository, TixError> {
        info!("cloning {alias} from {}", self.remote);
        let repo = git2::build::RepoBuilder::new()
            .fetch_options(crate::auth::fetch_options())
            .clone(&self.remote, &self.code_path)
            .map_err(|e| {
                debug!(alias = %alias, error = %e, "clone failed");
                TixError::GitError(e)
            })?;
        debug!(alias = %alias, path = %self.code_path.display(), "clone complete");
        Ok(Repository {
            alias: alias.to_string(),
            config: self,
            repo,
        })
    }

    /// Opens the repository if already cloned, or clones it first if not.
    ///
    /// Tries [`Self::resolve`] first. If that fails with a [`TixError::GitError`], falls back to
    /// [`Self::clone_remote`]. Other errors propagate without triggering a clone.
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
    ///     PathBuf::from("/home/user/code/repo"),
    /// );
    /// let repo = config.ensure("my-repo");
    /// ```
    pub fn ensure(self, alias: &str) -> Result<Repository, TixError> {
        let config = self.clone();
        match self.resolve(alias) {
            Ok(repo) => Ok(repo),
            Err(TixError::GitError(_)) => config.clone_remote(alias),
            Err(e) => Err(e),
        }
    }
}

/// Represents a Git repository.
pub struct Repository {
    /// The alias identifying this repository, sourced from the config HashMap key.
    pub alias: String,
    /// The configuration of the repository.
    pub config: RepositoryConfig,
    repo: git2::Repository,
}

impl Repository {
    /// Creates a git worktree named `name` for `branch` at `path`.
    ///
    /// `name` is the worktree directory name under the ticket root — the key
    /// of the worktree's entry in
    /// [`TicketConfig::worktrees`](crate::TicketConfig::worktrees) — and
    /// `path` is the full, already-resolved directory the worktree should
    /// appear at. The engine takes both as given; deriving them from a ticket
    /// is the frontend's job.
    ///
    /// A new `branch` is cut from `base`, a refish resolved as
    /// [`Self::resolve_base`] describes: `None` means origin's default branch, a bare
    /// branch name is picked between local and `origin` by ancestry, and
    /// anything qualified (`origin/main`, `refs/heads/main`, a tag, a SHA)
    /// resolves literally. The resolved ref name is returned as
    /// [`Worktree::base`] so the caller can record it.
    ///
    /// The source checkout at [`RepositoryConfig::code_path`] is never
    /// touched: its HEAD, working tree, and local branches stay as they are,
    /// and whether it is dirty, diverged, or on some other branch does not
    /// matter. A worktree needs a commit, not a checkout.
    ///
    /// If `branch` already exists locally the worktree checks it out as-is.
    /// With no `base` the worktree records none — tix did not cut the branch,
    /// so it cannot say what from. With an explicit `base`, the branch must
    /// already contain it; otherwise this errors rather than reuse a branch
    /// under a base it was not cut from. Existing branches are never moved.
    ///
    /// The branch name is always `branch`, never the worktree name.
    ///
    /// # Errors
    ///
    /// - [`TixError::Message`] if the base cannot be resolved, or an existing
    ///   `branch` does not contain an explicit `base`
    /// - [`TixError::GitError`] if worktree creation fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// // Cut from origin's default branch.
    /// let worktree = repo.create_worktree(
    ///     "my-repo",
    ///     "feature/JIRA-123-fix-login",
    ///     &PathBuf::from("/home/user/tickets/JIRA-123/my-repo"),
    ///     None,
    /// );
    /// // Stacked on a sibling branch.
    /// let stacked = repo.create_worktree(
    ///     "my-repo-part2",
    ///     "stack/JIRA-123-part-2",
    ///     &PathBuf::from("/home/user/tickets/JIRA-123/my-repo-part2"),
    ///     Some("feature/JIRA-123-fix-login"),
    /// );
    /// ```
    pub fn create_worktree(
        &self,
        name: &str,
        branch: &str,
        path: &Path,
        base: Option<&str>,
    ) -> Result<Worktree, TixError> {
        debug!(alias = %self.alias, name, branch, base, "creating worktree");

        // The worktree must open on `branch` by that exact name. Without an
        // explicit reference, git2 would create a branch named after the
        // *worktree* — wrong the moment name and branch differ (per-worktree
        // branches).
        let (local_branch, recorded_base) = match self
            .repo
            .find_branch(branch, git2::BranchType::Local)
        {
            Ok(existing) => {
                let recorded = match base {
                    None => None,
                    Some(spec) => {
                        let resolved = self.resolve_base_commit(Some(spec))?;
                        let tip = existing
                            .get()
                            .peel_to_commit()
                            .map_err(TixError::GitError)?
                            .id();
                        if !self.contains(tip, resolved.commit)? {
                            debug!(alias = %self.alias, branch, base = %resolved.name, "existing branch does not contain base");
                            return Err(TixError::from(format!(
                                "branch '{branch}' already exists and does not contain '{}' — \
                                     reuse it without a base, or pick another branch",
                                resolved.name
                            )));
                        }
                        Some(resolved.name)
                    }
                };
                debug!(alias = %self.alias, branch, "reusing existing local branch");
                (existing, recorded)
            }
            Err(_) => {
                let resolved = self.resolve_base_commit(base)?;
                debug!(alias = %self.alias, branch, base = %resolved.name, commit = %resolved.commit, "creating branch at base");
                let commit = self
                    .repo
                    .find_commit(resolved.commit)
                    .map_err(TixError::GitError)?;
                let created = self
                    .repo
                    .branch(branch, &commit, false)
                    .map_err(TixError::GitError)?;
                (created, Some(resolved.name))
            }
        };
        let reference = Some(local_branch.into_reference());

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(TixError::IoError)?;
        }

        let mut options = WorktreeAddOptions::new();
        options.reference(reference.as_ref());

        // Git registers worktrees under a repo-unique name. The config-level
        // `name` (directory under the ticket root) collides the moment two
        // tickets include the same repo, so registration uses the sanitized
        // branch instead — branches are unique per repo by git's own rules
        // (v2 parity: repo_worktrees held sanitized names).
        let registration = registration_name(branch);
        let worktree = self
            .repo
            .worktree(&registration, path, Some(&options))
            .map(|_| Worktree {
                name: name.to_string(),
                branch: branch.to_string(),
                base: recorded_base,
                repo_alias: self.alias.clone(),
                path: path.to_path_buf(),
            })
            .map_err(|e| {
                debug!(alias = %self.alias, name, error = %e, "failed to create worktree");
                TixError::GitError(e)
            })?;
        debug!(alias = %self.alias, name, path = %path.display(), base = worktree.base.as_deref(), "worktree created");
        Ok(worktree)
    }

    /// Resolves `base` to the ref a new branch should be cut from, returning
    /// the ref's name and the commit it points at.
    ///
    /// - **`None`** — origin's default branch, as `origin/<default>`. Read
    ///   from `refs/remotes/origin/HEAD` when present (no network); otherwise
    ///   asked of origin and cached there. Per repository, so a ticket
    ///   spanning a `main` repo and a `master` repo needs no flags.
    /// - **Qualified** (`origin/<b>`, `refs/...`) — resolved literally. An
    ///   `origin/<b>` is fetched first, so it means origin's `<b>` now.
    /// - **A bare branch name** — fetched, then picked between
    ///   `refs/heads/<b>` and `refs/remotes/origin/<b>` by ancestry:
    ///
    ///   | local vs `origin/<b>` | base |
    ///   |---|---|
    ///   | behind or equal | `origin/<b>` |
    ///   | ahead | `<b>` — unpushed commits are the base (stacking) |
    ///   | only one exists | that one |
    ///   | diverged | error naming both |
    ///
    /// - **Anything else** (a tag, a SHA, `HEAD~2`) — resolved literally.
    ///
    /// A failed fetch is a warning, not an error: the base resolves against
    /// whatever refs are already local, so offline work stays possible.
    ///
    /// Never touches the source checkout's HEAD, working tree, or branches.
    ///
    /// # Errors
    ///
    /// [`TixError::Message`] if the default branch cannot be determined, a
    /// bare name has diverged from origin, or `base` names no commit.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// assert_eq!(repo.resolve_base(None).unwrap(), "origin/main");
    /// ```
    pub fn resolve_base(&self, base: Option<&str>) -> Result<String, TixError> {
        self.resolve_base_commit(base).map(|resolved| resolved.name)
    }

    /// [`Self::resolve_base`], keeping the commit for the caller.
    fn resolve_base_commit(&self, base: Option<&str>) -> Result<ResolvedBase, TixError> {
        let Some(spec) = base else {
            let default = self.default_branch()?;
            self.fetch_or_warn(&default);
            let commit = self.commit_at(&format!("refs/remotes/origin/{default}"))?.ok_or_else(|| {
                TixError::from(format!(
                    "origin's default branch is '{default}', but refs/remotes/origin/{default} does not exist — \
                     fetch it, or pass an explicit base"
                ))
            })?;
            return Ok(ResolvedBase {
                name: format!("origin/{default}"),
                commit,
            });
        };

        if let Some(branch) = spec
            .strip_prefix("refs/remotes/origin/")
            .or_else(|| spec.strip_prefix("origin/"))
        {
            self.fetch_or_warn(branch);
            return self.resolve_literal(spec);
        }
        if spec.starts_with("refs/") || !git2::Branch::name_is_valid(spec).unwrap_or(false) {
            return self.resolve_literal(spec);
        }

        // A bare name. Fetch it, then let ancestry decide between local and
        // origin — preferring either unconditionally is wrong one way or the
        // other: remote cuts a stacked branch from an older commit, local
        // reintroduces a stale base.
        self.fetch_or_warn(spec);
        let local = self.commit_at(&format!("refs/heads/{spec}"))?;
        let remote = self.commit_at(&format!("refs/remotes/origin/{spec}"))?;
        let remote_name = format!("origin/{spec}");
        let resolved = match (local, remote) {
            // Neither is a branch: a tag or a SHA, which a bare name may also be.
            (None, None) => return self.resolve_literal(spec),
            (Some(commit), None) => ResolvedBase {
                name: spec.to_string(),
                commit,
            },
            (None, Some(commit)) => ResolvedBase {
                name: remote_name,
                commit,
            },
            (Some(local), Some(remote)) if self.contains(remote, local)? => ResolvedBase {
                name: remote_name,
                commit: remote,
            },
            (Some(local), Some(remote)) if self.contains(local, remote)? => ResolvedBase {
                name: spec.to_string(),
                commit: local,
            },
            (Some(_), Some(_)) => {
                debug!(alias = %self.alias, base = spec, "local and origin have diverged");
                return Err(TixError::from(format!(
                    "'{spec}' has diverged from 'origin/{spec}' — name the one to branch from: \
                     'refs/heads/{spec}' or 'origin/{spec}'"
                )));
            }
        };
        debug!(alias = %self.alias, base = spec, resolved = %resolved.name, "resolved bare base by ancestry");
        Ok(resolved)
    }

    /// Resolves `spec` exactly as git would, to a commit.
    fn resolve_literal(&self, spec: &str) -> Result<ResolvedBase, TixError> {
        let commit = self
            .repo
            .revparse_single(spec)
            .and_then(|object| object.peel_to_commit())
            .map_err(|e| {
                debug!(alias = %self.alias, base = spec, error = %e, "base did not resolve");
                TixError::from(format!(
                    "base '{spec}' does not resolve to a commit in '{}'",
                    self.alias
                ))
            })?;
        Ok(ResolvedBase {
            name: spec.to_string(),
            commit: commit.id(),
        })
    }

    /// The commit `reference` points at, or `None` if no such reference
    /// exists.
    fn commit_at(&self, reference: &str) -> Result<Option<git2::Oid>, TixError> {
        match self.repo.find_reference(reference) {
            Ok(found) => Ok(Some(
                found.peel_to_commit().map_err(TixError::GitError)?.id(),
            )),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(TixError::GitError(e)),
        }
    }

    /// Whether `commit` contains `ancestor` — is it, or descends from it.
    fn contains(&self, commit: git2::Oid, ancestor: git2::Oid) -> Result<bool, TixError> {
        Ok(commit == ancestor
            || self
                .repo
                .graph_descendant_of(commit, ancestor)
                .map_err(TixError::GitError)?)
    }

    /// Fetches `branch` from origin, updating its remote-tracking ref.
    ///
    /// A failure is logged and swallowed: the caller resolves against local
    /// refs either way, and being offline should not stop work that a local
    /// ref can satisfy.
    fn fetch_or_warn(&self, branch: &str) {
        debug!(alias = %self.alias, branch, "fetching from origin");
        let fetched = self.repo.find_remote("origin").and_then(|mut remote| {
            remote.fetch(&[branch], Some(&mut crate::auth::fetch_options()), None)
        });
        if let Err(e) = fetched {
            warn!(alias = %self.alias, branch, error = %e, "fetch failed, resolving against local refs");
        }
    }

    /// The name of origin's default branch (`main`, `master`, …).
    ///
    /// `refs/remotes/origin/HEAD` answers without the network after a normal
    /// clone. Absent that, origin is asked and the answer cached as that ref,
    /// so the next call is local again.
    ///
    /// # Errors
    ///
    /// [`TixError::Message`] if there is no cached answer and origin cannot
    /// be asked.
    fn default_branch(&self) -> Result<String, TixError> {
        const ORIGIN_HEAD: &str = "refs/remotes/origin/HEAD";
        const ORIGIN_PREFIX: &str = "refs/remotes/origin/";

        if let Ok(head) = self.repo.find_reference(ORIGIN_HEAD)
            && let Ok(Some(target)) = head.symbolic_target()
            && let Some(branch) = target.strip_prefix(ORIGIN_PREFIX)
        {
            debug!(alias = %self.alias, branch, "default branch from origin/HEAD");
            return Ok(branch.to_string());
        }

        let unknown = |reason: String| {
            debug!(alias = %self.alias, reason = %reason, "cannot determine default branch");
            TixError::from(format!(
                "cannot determine the default branch of '{}' ({reason}) — pass an explicit base",
                self.alias
            ))
        };
        debug!(alias = %self.alias, "no origin/HEAD, asking origin for its default branch");
        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(|e| unknown(e.message().to_string()))?;
        // connect_auth, not connect: plain connect() passes no credential
        // callbacks and fails on every private or SSH remote.
        let head = {
            let connection = remote
                .connect_auth(
                    git2::Direction::Fetch,
                    Some(crate::auth::remote_callbacks()),
                    None,
                )
                .map_err(|e| unknown(e.message().to_string()))?;
            let head = connection
                .default_branch()
                .map_err(|e| unknown(e.message().to_string()))?;
            head.as_str().ok().map(str::to_string)
        };
        let branch = head
            .as_deref()
            .and_then(|head| head.strip_prefix("refs/heads/"))
            .ok_or_else(|| unknown(format!("origin reported HEAD as {head:?}")))?
            .to_string();

        // Cache, so the next resolution needs no network. Not fatal: the
        // answer is already in hand.
        if let Err(e) = self.repo.reference_symbolic(
            ORIGIN_HEAD,
            &format!("{ORIGIN_PREFIX}{branch}"),
            true,
            "tix: cache origin's default branch",
        ) {
            warn!(alias = %self.alias, error = %e, "could not cache origin/HEAD");
        }
        debug!(alias = %self.alias, branch, "default branch from origin");
        Ok(branch)
    }

    /// Finds the registered worktree whose recorded path is `path`, if any.
    ///
    /// Paths are normalized before comparison — git records canonicalized
    /// paths (on macOS, `/var/...` becomes `/private/var/...`), so a raw
    /// equality check would miss legitimate matches.
    fn find_worktree_by_path(&self, path: &Path) -> Result<Option<git2::Worktree>, TixError> {
        let target = normalize_path(path);
        let names = self.repo.worktrees().map_err(TixError::GitError)?;
        for name in names.iter().filter_map(|n| n.ok().flatten()) {
            let worktree = self.repo.find_worktree(name).map_err(TixError::GitError)?;
            if normalize_path(worktree.path()) == target {
                return Ok(Some(worktree));
            }
        }
        Ok(None)
    }

    /// Prunes the git worktree living at `path` from this repository.
    ///
    /// Lookup is by the worktree's recorded path rather than its registration
    /// name — the path is what the ticket document knows
    /// (`<ticket_root>/<name>`), and it stays unambiguous even for orphaned
    /// worktrees whose directory is already gone.
    ///
    /// Pass `force: true` to remove even if the worktree is in a dirty state. Without `force`,
    /// returns an error if the worktree fails validation (e.g. has uncommitted changes).
    ///
    /// # Errors
    ///
    /// - [`TixError::WorktreeNotFound`] if no registered worktree records `path`
    /// - [`TixError::InvalidWorktree`] if git's validation of the worktree
    ///   fails without `force` (e.g. its directory is gone)
    /// - [`TixError::Dirty`] if the worktree has uncommitted changes without
    ///   `force`
    /// - [`TixError::GitError`] if pruning fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// repo.remove_worktree(&PathBuf::from("/home/user/tickets/JIRA-123/my-repo"), false);
    /// ```
    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<(), TixError> {
        debug!(alias = %self.alias, path = %path.display(), "removing worktree");

        let worktree = self
            .find_worktree_by_path(path)?
            .ok_or_else(|| TixError::WorktreeNotFound(path.display().to_string()))?;

        if !force {
            // validate() catches structural breakage (e.g. the directory is
            // gone). It says nothing about cleanliness — that takes opening
            // the worktree and asking for its status.
            if let Err(e) = worktree.validate() {
                debug!(alias = %self.alias, path = %path.display(), error = %e, "worktree failed validation");
                return Err(TixError::InvalidWorktree {
                    path: path.to_path_buf(),
                    reason: e.message().to_string(),
                });
            }
            let worktree_repo =
                git2::Repository::open(worktree.path()).map_err(TixError::GitError)?;
            let changes = uncommitted_changes(&worktree_repo)?;
            if changes > 0 {
                debug!(alias = %self.alias, path = %path.display(), changes, "worktree has uncommitted changes");
                return Err(TixError::Dirty {
                    path: path.to_path_buf(),
                    changes,
                });
            }
        }

        let mut opts = git2::WorktreePruneOptions::new();
        opts.working_tree(true).valid(true);

        worktree.prune(Some(&mut opts)).map_err(|e| {
            debug!(alias = %self.alias, path = %path.display(), "failed to remove worktree: {e}");
            TixError::GitError(e)
        })?;
        debug!(alias = %self.alias, path = %path.display(), "worktree removed");
        Ok(())
    }

    /// Prunes git's registration of the worktree at `path`, leaving whatever
    /// is on disk at `path` alone.
    ///
    /// For a worktree whose directory is gone, or no longer a git checkout:
    /// there is nothing to check for uncommitted changes, and nothing this
    /// should delete. Returns whether a registration existed — `false` when
    /// git has already forgotten it (say, after `git worktree prune`).
    ///
    /// # Errors
    ///
    /// [`TixError::GitError`] if listing or pruning registrations fails
    /// (e.g. the worktree is locked).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// repo.prune_worktree_registration(&PathBuf::from("/home/user/tickets/JIRA-123/my-repo"));
    /// ```
    pub fn prune_worktree_registration(&self, path: &Path) -> Result<bool, TixError> {
        let Some(worktree) = self.find_worktree_by_path(path)? else {
            debug!(alias = %self.alias, path = %path.display(), "no worktree registration to prune");
            return Ok(false);
        };
        // valid(true): a registration whose directory still exists (just not
        // as a checkout) passes git's validation, and must still go.
        let mut opts = git2::WorktreePruneOptions::new();
        opts.valid(true).working_tree(false);
        worktree
            .prune(Some(&mut opts))
            .map_err(TixError::GitError)?;
        debug!(alias = %self.alias, path = %path.display(), "worktree registration pruned");
        Ok(true)
    }

    /// Syncs the source checkout against origin's default branch (`main`,
    /// `master`, … — discovered per repository, see
    /// [`Self::resolve_base`]). See [`Self::sync_base`].
    ///
    /// Worktree creation does not call this: it never touches the source
    /// checkout. This is the explicit way to bring that checkout up to date.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// repo.sync(false);
    /// ```
    pub fn sync(&self, force: bool) -> Result<(), TixError> {
        let branch = self.default_branch()?;
        self.sync_base(&branch, force)
    }

    /// Fetches and fast-forwards `branch` from `origin`.
    ///
    /// Errors if the repository is mid-operation (merge, rebase, etc.) or if the local branch has
    /// diverged from remote without `force`. With `force`, discards all local changes and resets
    /// to the remote state. Also errors if the branch is already checked out in another worktree.
    ///
    /// # Errors
    ///
    /// - [`TixError::Dirty`] for uncommitted changes without `force`
    /// - [`TixError::Diverged`] if the branch cannot fast-forward without `force`
    /// - [`TixError::Message`] for a mid-operation repository or a worktree conflict
    /// - [`TixError::GitError`] for underlying git failures
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::RepositoryConfig;
    /// # use std::path::PathBuf;
    /// let repo = RepositoryConfig::new(
    ///     "https://github.com/owner/repo.git".to_string(),
    ///     PathBuf::from("/home/user/code/repo"),
    /// ).resolve("my-repo").unwrap();
    /// repo.sync_base("main", false);
    /// ```
    pub fn sync_base(&self, branch: &str, force: bool) -> Result<(), TixError> {
        debug!(alias = %self.alias, branch, force, "syncing repository");

        // RepositoryState::Clean means no git operation is in progress.
        // force cannot recover from this — the user must resolve it manually.
        if self.repo.state() != RepositoryState::Clean {
            debug!(alias = %self.alias, "repository is mid-operation, cannot sync");
            return Err(TixError::from(
                "repository is mid-operation (merge, rebase, etc), resolve it before syncing",
            ));
        }

        // if anything is dirty and force is not set, bail rather than silently discarding work.
        if !force {
            let changes = uncommitted_changes(&self.repo)?;
            if changes > 0 {
                debug!(alias = %self.alias, changes, "repository has uncommitted changes");
                return Err(TixError::Dirty {
                    path: self.config.code_path.clone(),
                    changes,
                });
            }
        }

        // fetch so we have up-to-date remote refs before checking branch existence.
        // this also means if the branch only exists on origin and not locally,
        // we'll have it available to create a local tracking branch from.
        debug!(alias = %self.alias, branch, "fetching from origin");
        let mut remote = self
            .repo
            .find_remote("origin")
            .map_err(TixError::GitError)?;
        remote
            .fetch(&[branch], Some(&mut crate::auth::fetch_options()), None)
            .map_err(TixError::GitError)?;

        // check if the branch exists locally. if not, try to create it from
        // the remote ref we just fetched — if origin doesn't have it either, error.
        if self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .is_err()
        {
            debug!(alias = %self.alias, branch, "branch not found locally, creating from origin");
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
            debug!(alias = %self.alias, branch, "local branch created from origin");
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
                debug!(alias = %self.alias, branch, "branch has diverged from remote");
                return Err(TixError::Diverged {
                    branch: branch.to_string(),
                });
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
                    debug!(alias = %self.alias, branch, worktree = name, "branch is checked out in another worktree");
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
            // force() discards modified tracked files and remove_untracked removes
            // files git doesn't know about — the same set uncommitted_changes
            // counts. Ignored files (build caches, `.env`) are deliberately kept:
            // they were never "changes", so forcing past a dirty check must not
            // delete them (#175).
            warn!(alias = %self.alias, "force flag set — discarding all local changes");
            let mut clean_opts = git2::build::CheckoutBuilder::new();
            clean_opts.force().remove_untracked(true);
            self.repo
                .checkout_head(Some(&mut clean_opts))
                .map_err(TixError::GitError)?;
        }

        // set_head points HEAD at the branch ref without touching the working tree.
        // checkout_head then updates the working tree to match.
        debug!(alias = %self.alias, branch, "switching HEAD");
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
            debug!(alias = %self.alias, branch, "force resetting to remote");
            branch_ref
                .set_target(fetch_commit.id(), "sync_base: force reset")
                .map_err(TixError::GitError)?;
        } else {
            debug!(alias = %self.alias, branch, "fast-forwarding to remote");
            branch_ref
                .set_target(fetch_commit.id(), "sync_base: fast-forward")
                .map_err(TixError::GitError)?;
        }

        // final checkout to update the working tree to the new branch pointer
        self.repo
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(TixError::GitError)?;

        debug!(alias = %self.alias, branch, "sync complete");
        Ok(())
    }
}

/// A resolved base: the ref name recorded in the ticket document, and the
/// commit a new branch is cut at.
struct ResolvedBase {
    name: String,
    commit: git2::Oid,
}

/// Counts the working-tree entries that would be lost by discarding local
/// changes: modified, staged, and untracked files.
///
/// Ignored files are excluded on purpose. Build output, dependency caches, and
/// `.env` files are routinely present in a clean checkout, and `git status`
/// does not report them either. Passing explicit options matters: with
/// `statuses(None)`, libgit2 falls back to defaults that include ignored files.
fn uncommitted_changes(repo: &git2::Repository) -> Result<usize, TixError> {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(TixError::GitError)?;
    Ok(statuses.len())
}

/// The repo-unique name a worktree is registered under: the branch with path
/// separators flattened. Branch uniqueness per repo (enforced by git) makes
/// this collision-free in practice; a rare sanitization collision surfaces as
/// a clear git error at creation.
fn registration_name(branch: &str) -> String {
    branch.replace(['/', '\\'], "-")
}

/// Canonicalizes `path` for comparison, tolerating paths that no longer
/// exist (an orphaned worktree's directory) by canonicalizing the parent and
/// re-joining the final component.
fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|parent| parent.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // --- RepositoryConfig ---

    /// `new` populates all fields correctly.
    #[test]
    fn test_repository_config_new() {
        let config = RepositoryConfig::new(
            "https://github.com/owner/repo.git".into(),
            PathBuf::from("/code/repo"),
        );
        assert_eq!(config.remote, "https://github.com/owner/repo.git");
        assert_eq!(config.code_path, PathBuf::from("/code/repo"));
    }

    /// Serializing to TOML and deserializing back produces an identical value.
    #[test]
    fn test_repository_config_toml_round_trip() {
        let config = RepositoryConfig::new(
            "https://github.com/owner/repo.git".into(),
            PathBuf::from("/code/repo"),
        );
        let toml = toml::to_string(&config).unwrap();
        let deserialized: RepositoryConfig = toml::from_str(&toml).unwrap();
        assert_eq!(deserialized, config);
    }

    // --- clone_remote / resolve ---

    /// A valid remote clones successfully and returns a `Repository` pointed at the workdir.
    #[test]
    fn test_clone_remote_valid() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let config = RepositoryConfig::new(
            dir.path().join("remote").to_str().unwrap().into(),
            dir.path().join("local"),
        );
        assert!(config.clone_remote("test").is_ok());
    }

    /// An invalid/nonexistent remote URL returns `TixError::GitError`.
    #[test]
    fn test_clone_remote_invalid_url() {
        let dir = tempdir().unwrap();
        let config = RepositoryConfig::new(
            "https://invalid.invalid/repo.git".into(),
            dir.path().join("local"),
        );
        assert!(matches!(
            config.clone_remote("test"),
            Err(TixError::GitError(_))
        ));
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
            dir.path().join("local"),
        );
        assert!(config.resolve("test").is_ok());
    }

    /// A path that is not a git repository returns `TixError::GitError`.
    #[test]
    fn test_resolve_invalid_path() {
        let remote = "this/is/an/invalid/repo/dir";
        let dir = tempdir().unwrap();
        let config = RepositoryConfig::new(remote.into(), dir.path().join("local"));
        assert!(matches!(config.resolve("test"), Err(TixError::GitError(_))));
    }

    // --- create_worktree ---

    /// A remote with one commit on `main`, cloned, wrapped as a `Repository`.
    fn fixture(dir: &Path) -> (git2::Repository, Repository) {
        let remote = test_helpers::init_bare_repo(&dir.join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.join("remote"), &dir.join("local"));
        let repo = test_helpers::tix_repo(&dir.join("remote"), local);
        (remote, repo)
    }

    /// The commit a worktree's HEAD sits on.
    fn head_of(path: &Path) -> git2::Oid {
        git2::Repository::open(path)
            .unwrap()
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
    }

    /// The tip of `branch` in the bare remote.
    fn remote_tip(remote: &git2::Repository, branch: &str) -> git2::Oid {
        remote
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
    }

    /// A ticket whose branch doesn't exist locally creates a new branch at
    /// origin's default branch, and records that as its base.
    #[test]
    fn test_create_worktree_new_branch() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());

        let path = dir.path().join("worktrees/feature");
        let worktree = repo
            .create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(worktree.name, "feature");
        assert_eq!(worktree.branch, "feature");
        assert_eq!(worktree.base.as_deref(), Some("origin/main"));
        assert_eq!(worktree.path, path);
        assert!(path.exists());
        assert_eq!(head_of(&path), remote_tip(&remote, "main"));
    }

    /// With no base, the branch is cut from origin as it is *now* — fetched,
    /// not the stale remote-tracking ref of the last fetch.
    #[test]
    fn test_create_worktree_default_base_is_fetched() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_commit_to_bare(&remote, "landed after clone");

        let path = dir.path().join("worktrees/feature");
        repo.create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(head_of(&path), remote_tip(&remote, "main"));
    }

    /// The source checkout is never touched: a dirty, mid-merge root sitting
    /// on another branch keeps its HEAD, its files, and its local `main`.
    #[test]
    fn test_create_worktree_leaves_source_checkout_alone() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_commit_to_bare(&remote, "landed after clone");
        let stale_main = repo.repo.refname_to_id("refs/heads/main").unwrap();
        {
            let head = repo.repo.head().unwrap().peel_to_commit().unwrap();
            repo.repo.branch("elsewhere", &head, false).unwrap();
        }
        repo.repo.set_head("refs/heads/elsewhere").unwrap();
        test_helpers::make_dirty(&repo.repo);
        test_helpers::make_mid_operation(&repo.repo);

        let path = dir.path().join("worktrees/feature");
        repo.create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(head_of(&path), remote_tip(&remote, "main"));
        assert_eq!(repo.repo.head().unwrap().shorthand(), Ok("elsewhere"));
        assert!(dir.path().join("local/dirty.txt").exists());
        assert_eq!(repo.repo.state(), RepositoryState::Merge);
        assert_eq!(
            repo.repo.refname_to_id("refs/heads/main").unwrap(),
            stale_main
        );
    }

    /// A local `main` that has diverged from origin does not block the
    /// default base, which is origin's branch — not the local one.
    #[test]
    fn test_create_worktree_diverged_local_main_does_not_block() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_commit_to_bare(&remote, "remote commit");
        test_helpers::add_commit(&repo.repo, "main", "local commit");

        let path = dir.path().join("worktrees/feature");
        let worktree = repo
            .create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(worktree.base.as_deref(), Some("origin/main"));
        assert_eq!(head_of(&path), remote_tip(&remote, "main"));
    }

    /// A repository whose default branch is `master` works with no base
    /// given — the default is discovered, never assumed to be `main`.
    #[test]
    fn test_create_worktree_master_default_branch() {
        let dir = tempdir().unwrap();
        let remote = git2::Repository::init_bare(dir.path().join("remote")).unwrap();
        remote.set_head("refs/heads/master").unwrap();
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        let path = dir.path().join("worktrees/feature");
        let worktree = repo
            .create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(worktree.base.as_deref(), Some("origin/master"));
        assert_eq!(head_of(&path), remote_tip(&remote, "master"));
    }

    /// Without `refs/remotes/origin/HEAD`, origin is asked for its default
    /// branch, and the answer is cached as that ref.
    #[test]
    fn test_default_branch_discovered_and_cached() {
        let dir = tempdir().unwrap();
        let remote = git2::Repository::init_bare(dir.path().join("remote")).unwrap();
        remote.set_head("refs/heads/trunk").unwrap();
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        if let Ok(mut head) = local.find_reference("refs/remotes/origin/HEAD") {
            head.delete().unwrap();
        }
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert_eq!(repo.resolve_base(None).unwrap(), "origin/trunk");
        let cached = repo
            .repo
            .find_reference("refs/remotes/origin/HEAD")
            .unwrap();
        assert_eq!(
            cached.symbolic_target().unwrap(),
            Some("refs/remotes/origin/trunk")
        );
    }

    /// No cached default and no origin to ask is an error, not a guess.
    #[test]
    fn test_default_branch_unknown_errors() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        if let Ok(mut head) = repo.repo.find_reference("refs/remotes/origin/HEAD") {
            head.delete().unwrap();
        }
        repo.repo.remote_delete("origin").unwrap();

        assert!(matches!(repo.resolve_base(None), Err(TixError::Message(_))));
    }

    /// A ticket whose branch already exists locally reuses it untouched, and
    /// records no base — tix did not cut it.
    #[test]
    fn test_create_worktree_existing_local() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        let tip = test_helpers::add_branch_commit(&repo.repo, "feature", "work");

        let path = dir.path().join("worktrees/feature");
        let worktree = repo
            .create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert_eq!(worktree.name, "feature");
        assert_eq!(worktree.branch, "feature");
        assert_eq!(worktree.base, None);
        assert_eq!(worktree.path, path);
        assert_eq!(head_of(&path), tip);
    }

    /// An existing branch that contains an explicit base is reused, and the
    /// base recorded.
    #[test]
    fn test_create_worktree_existing_local_contains_base() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        let tip = test_helpers::add_branch_commit(&repo.repo, "feature", "work");

        let path = dir.path().join("worktrees/feature");
        let worktree = repo
            .create_worktree("feature", "feature", &path, Some("main"))
            .unwrap();

        assert_eq!(worktree.base.as_deref(), Some("origin/main"));
        assert_eq!(head_of(&path), tip);
    }

    /// An existing branch that does not contain an explicit base errors —
    /// and is not moved to it, which would be destructive.
    #[test]
    fn test_create_worktree_existing_local_lacks_base() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        let tip = test_helpers::add_branch_commit(&repo.repo, "feature", "work");
        test_helpers::add_commit_to_bare(&remote, "main moved on");

        let path = dir.path().join("worktrees/feature");
        assert!(matches!(
            repo.create_worktree("feature", "feature", &path, Some("main")),
            Err(TixError::Message(_))
        ));
        assert!(!path.exists());
        assert_eq!(repo.repo.refname_to_id("refs/heads/feature").unwrap(), tip);
    }

    /// Creating a worktree with a name that already exists returns `TixError::GitError`.
    #[test]
    fn test_create_worktree_duplicate_name() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());

        let path = dir.path().join("worktrees/feature");
        repo.create_worktree("feature", "feature", &path, None)
            .unwrap();

        assert!(matches!(
            repo.create_worktree("feature", "feature", &path, None),
            Err(TixError::GitError(_))
        ));
    }

    /// A worktree whose name differs from its branch opens on the requested
    /// branch — created fresh when absent — never on a branch named after
    /// the worktree.
    #[test]
    fn test_create_worktree_name_differs_from_branch() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());

        let path = dir.path().join("worktrees/my-repo");
        let worktree = repo
            .create_worktree("my-repo", "feature/JIRA-1-fix", &path, None)
            .unwrap();

        assert_eq!(worktree.name, "my-repo");
        assert_eq!(worktree.branch, "feature/JIRA-1-fix");
        let opened = git2::Repository::open(&path).unwrap();
        assert_eq!(opened.head().unwrap().shorthand(), Ok("feature/JIRA-1-fix"));
    }

    /// The stacked case: a base checked out in a sibling worktree, with
    /// unpushed commits, is accepted, and those commits are the base.
    #[test]
    fn test_create_worktree_stacked_on_sibling() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        let part1 = dir.path().join("worktrees/part1");
        repo.create_worktree("part1", "stack/part-1", &part1, None)
            .unwrap();
        let part1_tip = test_helpers::add_branch_commit(&repo.repo, "stack/part-1", "part 1");

        let part2 = dir.path().join("worktrees/part2");
        let worktree = repo
            .create_worktree("part2", "stack/part-2", &part2, Some("stack/part-1"))
            .unwrap();

        assert_eq!(worktree.base.as_deref(), Some("stack/part-1"));
        assert_eq!(head_of(&part2), part1_tip);
    }

    // --- resolve_base ---

    /// A bare name whose local branch is behind origin resolves to origin —
    /// always cut from fresh.
    #[test]
    fn test_resolve_base_bare_local_behind() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_commit_to_bare(&remote, "remote commit");

        assert_eq!(repo.resolve_base(Some("main")).unwrap(), "origin/main");
    }

    /// Equal local and origin resolve to origin.
    #[test]
    fn test_resolve_base_bare_equal() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());

        assert_eq!(repo.resolve_base(Some("main")).unwrap(), "origin/main");
    }

    /// A bare name whose local branch is ahead resolves to the local branch:
    /// the unpushed commits are the base.
    #[test]
    fn test_resolve_base_bare_local_ahead() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        test_helpers::add_commit(&repo.repo, "main", "local commit");

        assert_eq!(repo.resolve_base(Some("main")).unwrap(), "main");
    }

    /// A bare name that exists only on origin — never fetched before —
    /// resolves to origin.
    #[test]
    fn test_resolve_base_bare_only_remote() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_remote_branch(&remote, "release");

        assert_eq!(
            repo.resolve_base(Some("release")).unwrap(),
            "origin/release"
        );
    }

    /// A bare name that exists only locally resolves to the local branch.
    #[test]
    fn test_resolve_base_bare_only_local() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        test_helpers::add_branch_commit(&repo.repo, "local-only", "work");

        assert_eq!(repo.resolve_base(Some("local-only")).unwrap(), "local-only");
    }

    /// Diverged local and origin are an error naming both — either guess
    /// would be wrong.
    #[test]
    fn test_resolve_base_bare_diverged() {
        let dir = tempdir().unwrap();
        let (remote, repo) = fixture(dir.path());
        test_helpers::add_commit_to_bare(&remote, "remote commit");
        test_helpers::add_commit(&repo.repo, "main", "local commit");

        let error = repo.resolve_base(Some("main")).unwrap_err().to_string();
        assert!(error.contains("refs/heads/main"), "{error}");
        assert!(error.contains("origin/main"), "{error}");
        // Qualifying it is the way out.
        assert_eq!(
            repo.resolve_base(Some("refs/heads/main")).unwrap(),
            "refs/heads/main"
        );
        assert_eq!(
            repo.resolve_base(Some("origin/main")).unwrap(),
            "origin/main"
        );
    }

    /// Qualified bases, tags, and SHAs resolve literally.
    #[test]
    fn test_resolve_base_literal() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        let head = repo.repo.head().unwrap().peel_to_commit().unwrap();
        repo.repo
            .tag_lightweight("v1.2.0", head.as_object(), false)
            .unwrap();
        let sha = head.id().to_string();

        assert_eq!(repo.resolve_base(Some("v1.2.0")).unwrap(), "v1.2.0");
        assert_eq!(repo.resolve_base(Some(&sha)).unwrap(), sha);
        assert_eq!(repo.resolve_base(Some(&sha[..10])).unwrap(), &sha[..10]);
        assert_eq!(repo.resolve_base(Some("HEAD")).unwrap(), "HEAD");
    }

    /// A base that names nothing is an error.
    #[test]
    fn test_resolve_base_unknown() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());

        assert!(matches!(
            repo.resolve_base(Some("no-such-branch")),
            Err(TixError::Message(_))
        ));
    }

    /// A failed fetch does not stop a base that resolves locally — offline
    /// work stays possible.
    #[test]
    fn test_resolve_base_fetch_failure_falls_back_to_local() {
        let dir = tempdir().unwrap();
        let (_remote, repo) = fixture(dir.path());
        std::fs::remove_dir_all(dir.path().join("remote")).unwrap();

        assert_eq!(repo.resolve_base(Some("main")).unwrap(), "origin/main");
        assert_eq!(repo.resolve_base(None).unwrap(), "origin/main");
    }

    // --- remove_worktree ---

    /// Removing a worktree that doesn't exist returns `TixError::WorktreeNotFound`.
    #[test]
    fn test_remove_worktree_not_found() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.remove_worktree(&dir.path().join("worktrees/nonexistent"), false),
            Err(TixError::WorktreeNotFound(_))
        ));
    }

    /// Removing an orphaned worktree (missing directory) without `force` returns
    /// `TixError::InvalidWorktree`.
    #[test]
    fn test_remove_worktree_orphaned_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::orphan_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), false),
            Err(TixError::InvalidWorktree { .. })
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

        assert!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), true)
                .is_ok()
        );
    }

    /// A worktree with uncommitted changes refuses removal without `force` —
    /// git2's `validate()` alone would pass it.
    #[test]
    fn test_remove_worktree_dirty_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        std::fs::write(dir.path().join("worktrees/feature/dirty.txt"), "wip").unwrap();
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(matches!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), false),
            Err(TixError::Dirty { changes: 1, .. })
        ));
        // Force discards it.
        assert!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), true)
                .is_ok()
        );
    }

    /// Ignored files are not uncommitted work: a worktree holding only build
    /// output or `.env`-style files is clean and removes without `force`.
    #[test]
    fn test_remove_worktree_ignored_files_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::add_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        test_helpers::make_ignored(&local, &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), false)
                .is_ok()
        );
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

        assert!(
            repo.remove_worktree(&dir.path().join("worktrees/feature"), false)
                .is_ok()
        );
        assert!(!dir.path().join("worktrees/feature").exists());
    }

    // --- prune_worktree_registration ---

    /// An orphaned worktree's registration is pruned, and a second call
    /// finds nothing left to prune.
    #[test]
    fn test_prune_worktree_registration_orphaned() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::orphan_worktree(&local, "feature", &dir.path().join("worktrees/feature"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);
        let path = dir.path().join("worktrees/feature");

        assert!(repo.prune_worktree_registration(&path).unwrap());
        assert!(repo.repo.worktrees().unwrap().is_empty());
        assert!(!repo.prune_worktree_registration(&path).unwrap());
    }

    /// A directory that is no longer a checkout loses its registration but
    /// keeps its contents: nothing could check them for uncommitted work.
    #[test]
    fn test_prune_worktree_registration_keeps_directory() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        let path = dir.path().join("worktrees/feature");
        test_helpers::add_worktree(&local, "feature", &path);
        std::fs::remove_file(path.join(".git")).unwrap();
        std::fs::write(path.join("notes.txt"), "keep me").unwrap();
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.prune_worktree_registration(&path).unwrap());
        assert!(repo.repo.worktrees().unwrap().is_empty());
        assert!(path.join("notes.txt").is_file());
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
            Err(TixError::Dirty { .. })
        ));
    }

    /// Ignored files in the main checkout (build output, `.env`) don't block a
    /// sync — they are what `create_worktree` used to trip over (#175).
    #[test]
    fn test_sync_base_ignored_files_no_force() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_ignored(&local, &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", false).is_ok());
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

    /// `force` discards untracked work but keeps ignored files: build caches
    /// and `.env` files are not changes, so forcing must not delete them.
    #[test]
    fn test_sync_base_force_keeps_ignored_files() {
        let dir = tempdir().unwrap();
        let remote = test_helpers::init_bare_repo(&dir.path().join("remote"));
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&dir.path().join("remote"), &dir.path().join("local"));
        test_helpers::make_dirty(&local);
        test_helpers::make_ignored(&local, &dir.path().join("local"));
        let repo = test_helpers::tix_repo(&dir.path().join("remote"), local);

        assert!(repo.sync_base("main", true).is_ok());
        assert!(!dir.path().join("local/dirty.txt").exists());
        assert!(dir.path().join("local/build/artifact.o").exists());
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
            Err(TixError::Diverged { .. })
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
pub(crate) mod test_helpers {
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
            alias: "default".into(),
            config: RepositoryConfig::new(remote.to_str().unwrap().into(), code_path),
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

    /// Ignores `build/` via `info/exclude` (shared by every worktree of
    /// `repo`) and writes a file under it in `workdir` — present on disk, but
    /// not a change git would report.
    pub fn make_ignored(repo: &git2::Repository, workdir: &Path) {
        let exclude = repo.commondir().join("info/exclude");
        std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
        std::fs::write(&exclude, "build/\n").unwrap();
        std::fs::create_dir_all(workdir.join("build")).unwrap();
        std::fs::write(workdir.join("build/artifact.o"), "compiled").unwrap();
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

    /// Commits onto local `branch`, creating it at HEAD first if absent, and
    /// returns the new tip. Leaves HEAD and the working tree alone.
    pub fn add_branch_commit(repo: &git2::Repository, branch: &str, message: &str) -> git2::Oid {
        if repo.find_branch(branch, git2::BranchType::Local).is_err() {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch(branch, &head, false).unwrap();
        }
        add_commit(repo, branch, message).id()
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
