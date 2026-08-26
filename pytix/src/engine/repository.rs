//! Bindings for the repository types: the registered [`RepositoryConfig`] and
//! the live [`Repository`] a resolved clone yields.

use crate::engine::worktree::PyWorktree;
use crate::error::engine_error;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::sync::Mutex;
use tix_engine::{Repository, RepositoryConfig};

/// A source repository registered in the global config: a remote and the local
/// path its clone lives at.
///
/// Mirrors [`tix_engine::RepositoryConfig`]. The three resolution methods
/// differ only in what they do when the clone is missing:
/// [`resolve`](Self::resolve) fails, [`clone_remote`](Self::clone_remote)
/// always clones, and [`ensure`](Self::ensure) clones only as a fallback.
#[pyclass(
    name = "RepositoryConfig",
    module = "pytix",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, PartialEq)]
pub struct PyRepositoryConfig {
    pub(crate) inner: RepositoryConfig,
}

#[pymethods]
impl PyRepositoryConfig {
    /// Registers `remote` as living at `code_path`. Neither is touched here —
    /// nothing is read, cloned, or validated until a resolution method runs.
    #[new]
    fn new(remote: String, code_path: PathBuf) -> Self {
        Self {
            inner: RepositoryConfig::new(remote, code_path),
        }
    }

    /// The remote URL of the repository.
    #[getter]
    fn remote(&self) -> &str {
        &self.inner.remote
    }

    /// The local path the clone lives at.
    #[getter]
    fn code_path(&self) -> PathBuf {
        self.inner.code_path.clone()
    }

    /// Opens the already-cloned repository at `code_path` under `alias`.
    ///
    /// Raises `TixError` if the path is not a git repository — use
    /// `clone_remote` or `ensure` when the clone may not exist yet.
    fn resolve(&self, alias: &str) -> PyResult<PyRepository> {
        self.inner
            .clone()
            .resolve(alias)
            .map(PyRepository::from_inner)
            .map_err(engine_error)
    }

    /// Clones `remote` into `code_path` and opens the result under `alias`.
    ///
    /// Raises `TixError` if the clone fails.
    fn clone_remote(&self, alias: &str) -> PyResult<PyRepository> {
        self.inner
            .clone()
            .clone_remote(alias)
            .map(PyRepository::from_inner)
            .map_err(engine_error)
    }

    /// Opens the clone if it exists, cloning it first if it does not.
    ///
    /// Raises `TixError` if the clone is needed and fails.
    fn ensure(&self, alias: &str) -> PyResult<PyRepository> {
        self.inner
            .clone()
            .ensure(alias)
            .map(PyRepository::from_inner)
            .map_err(engine_error)
    }

    fn __repr__(&self) -> String {
        format!(
            "RepositoryConfig(remote={:?}, code_path={:?})",
            self.inner.remote,
            self.inner.code_path.display().to_string()
        )
    }
}

/// A live git repository: an open clone, ready for worktree and sync
/// operations.
///
/// Mirrors [`tix_engine::Repository`]. Construct one through a
/// [`RepositoryConfig`](PyRepositoryConfig) resolution method.
///
/// The wrapped `git2::Repository` is `Send` but not `Sync`, while every
/// `#[pyclass]` must be both — the interpreter shares objects across threads
/// freely. A `Mutex` supplies the missing half and serializes concurrent
/// calls, which is also what libgit2 wants.
#[pyclass(name = "Repository", module = "pytix", frozen)]
pub struct PyRepository {
    inner: Mutex<Repository>,
}

impl PyRepository {
    /// Wraps a repository the engine has just opened or cloned.
    fn from_inner(inner: Repository) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    /// Runs `operation` against the wrapped repository under the lock.
    ///
    /// A poisoned lock means an earlier call panicked mid-operation; the
    /// repository handle itself is still valid, so the guard is recovered
    /// rather than turned into a second, less informative failure.
    fn with<T>(&self, operation: impl FnOnce(&Repository) -> T) -> T {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        operation(&guard)
    }
}

#[pymethods]
impl PyRepository {
    /// The alias this repository is registered under.
    #[getter]
    fn alias(&self) -> String {
        self.with(|repo| repo.alias.clone())
    }

    /// The config this repository was resolved from.
    #[getter]
    fn config(&self) -> PyRepositoryConfig {
        self.with(|repo| PyRepositoryConfig {
            inner: repo.config.clone(),
        })
    }

    /// Creates a worktree directory named `name` at `path`, checked out on
    /// `branch`, creating the branch at the synced head if it does not exist.
    ///
    /// `path` is the full, already-resolved directory — the engine derives no
    /// paths of its own. Syncs first; pass `force=True` to discard local
    /// changes before syncing.
    ///
    /// Raises `TixError` if the sync or the worktree creation fails.
    #[pyo3(signature = (name, branch, path, force = false))]
    fn create_worktree(
        &self,
        name: &str,
        branch: &str,
        path: PathBuf,
        force: bool,
    ) -> PyResult<PyWorktree> {
        self.with(|repo| repo.create_worktree(name, branch, &path, force))
            .map(PyWorktree::from_inner)
            .map_err(engine_error)
    }

    /// Prunes the worktree recorded at `path` from this repository.
    ///
    /// Pass `force=True` to remove one that is dirty or structurally broken.
    ///
    /// Raises `TixError` if no worktree records `path`, if it is dirty
    /// without `force`, or if pruning fails.
    #[pyo3(signature = (path, force = false))]
    fn remove_worktree(&self, path: PathBuf, force: bool) -> PyResult<()> {
        self.with(|repo| repo.remove_worktree(&path, force))
            .map_err(engine_error)
    }

    /// Fetches and fast-forwards `main`. See `sync_base`.
    #[pyo3(signature = (force = false))]
    fn sync(&self, force: bool) -> PyResult<()> {
        self.with(|repo| repo.sync(force)).map_err(engine_error)
    }

    /// Fetches and fast-forwards `branch` from `origin`.
    ///
    /// Pass `force=True` to discard local changes and reset to the remote
    /// state.
    ///
    /// Raises `TixError` if the repository is mid-operation, if the branch has
    /// diverged without `force`, if the branch is checked out in another
    /// worktree, or on any underlying git failure.
    #[pyo3(signature = (branch, force = false))]
    fn sync_base(&self, branch: &str, force: bool) -> PyResult<()> {
        self.with(|repo| repo.sync_base(branch, force))
            .map_err(engine_error)
    }

    fn __repr__(&self) -> String {
        self.with(|repo| {
            format!(
                "Repository(alias={:?}, code_path={:?})",
                repo.alias,
                repo.config.code_path.display().to_string()
            )
        })
    }
}
