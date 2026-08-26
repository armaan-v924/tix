//! Bindings for the worktree types: the recorded [`WorktreeConfig`] and the
//! live [`Worktree`].

use pyo3::prelude::*;
use std::path::PathBuf;
use tix_engine::{Worktree, WorktreeConfig};

/// One worktree's entry in a ticket document: which repository it belongs to
/// and which branch it has checked out.
///
/// Mirrors [`tix_engine::WorktreeConfig`]. Frozen because a config value that
/// has already been handed to the engine must not mutate underneath it —
/// build a new one instead.
#[pyclass(name = "WorktreeConfig", module = "pytix", frozen, eq, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyWorktreeConfig {
    pub(crate) inner: WorktreeConfig,
}

impl PyWorktreeConfig {
    /// Wraps a recorded entry produced inside the engine.
    pub(crate) fn from_inner(inner: WorktreeConfig) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyWorktreeConfig {
    /// Records a worktree of `repo` sitting on `branch`.
    #[new]
    fn new(repo: String, branch: String) -> Self {
        Self {
            inner: WorktreeConfig { repo, branch },
        }
    }

    /// The alias of the repository this worktree belongs to.
    #[getter]
    fn repo(&self) -> &str {
        &self.inner.repo
    }

    /// The branch checked out in this worktree.
    #[getter]
    fn branch(&self) -> &str {
        &self.inner.branch
    }

    fn __repr__(&self) -> String {
        format!(
            "WorktreeConfig(repo={:?}, branch={:?})",
            self.inner.repo, self.inner.branch
        )
    }
}

/// A live git worktree: one that existed on disk at the moment it was
/// produced, by resolving a ticket or by creating it.
///
/// Mirrors [`tix_engine::Worktree`]. There is deliberately no constructor —
/// a `Worktree` is evidence, and Python code must not be able to forge it.
#[pyclass(name = "Worktree", module = "pytix", frozen, eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyWorktree {
    inner: Worktree,
}

impl PyWorktree {
    /// Wraps a worktree the engine has just verified or created.
    pub(crate) fn from_inner(inner: Worktree) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyWorktree {
    /// The worktree directory name under the ticket root.
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// The alias of the repository this worktree belongs to.
    #[getter]
    fn repo_alias(&self) -> &str {
        &self.inner.repo_alias
    }

    /// The full path to the worktree directory.
    #[getter]
    fn path(&self) -> PathBuf {
        self.inner.path.clone()
    }

    /// The branch checked out in this worktree.
    #[getter]
    fn branch(&self) -> &str {
        &self.inner.branch
    }

    fn __repr__(&self) -> String {
        format!(
            "Worktree(name={:?}, repo_alias={:?}, path={:?}, branch={:?})",
            self.inner.name,
            self.inner.repo_alias,
            self.inner.path.display().to_string(),
            self.inner.branch
        )
    }
}
