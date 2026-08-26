//! Bindings for the ticket types: the recorded [`TicketConfig`] and the live
//! [`Ticket`] resolving it against disk yields.

use crate::engine::worktree::{PyWorktree, PyWorktreeConfig};
use crate::error::engine_error;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tix_engine::{Ticket, TicketConfig};

/// The `[ticket]` section of a ticket document: the ticket's identity plus one
/// entry per worktree.
///
/// Mirrors [`tix_engine::TicketConfig`]. Note what is *not* here: no
/// `load_from`, no path, no way to reach a `.tix/ticket.toml`. Engine types do
/// no IO — reading and writing the document is `pytix.host`'s job, and this
/// type only describes the shape once it has been read.
///
/// There is likewise no single `branch`: worktrees in one ticket share a
/// branch *prefix*, not a branch, so each entry carries its own.
#[pyclass(
    name = "TicketConfig",
    module = "pytix",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, PartialEq)]
pub struct PyTicketConfig {
    inner: TicketConfig,
}

impl PyTicketConfig {
    /// Wraps a section the host has parsed out of a ticket document.
    pub(crate) fn from_inner(inner: TicketConfig) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyTicketConfig {
    /// Describes a ticket `key` with `description` and, optionally, its
    /// recorded worktrees keyed by directory name.
    #[new]
    #[pyo3(signature = (key, description, worktrees = None))]
    fn new(
        key: String,
        description: String,
        worktrees: Option<HashMap<String, PyWorktreeConfig>>,
    ) -> Self {
        Self {
            inner: TicketConfig {
                key,
                description,
                worktrees: worktrees
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, entry)| (name, entry.inner))
                    .collect(),
            },
        }
    }

    /// The ticket's unique key, e.g. `JIRA-123`.
    #[getter]
    fn key(&self) -> &str {
        &self.inner.key
    }

    /// The human-readable description of the ticket.
    #[getter]
    fn description(&self) -> &str {
        &self.inner.description
    }

    /// The recorded worktrees, keyed by directory name under the ticket root.
    #[getter]
    fn worktrees(&self) -> HashMap<String, PyWorktreeConfig> {
        self.inner
            .worktrees
            .iter()
            .map(|(name, entry)| (name.clone(), PyWorktreeConfig::from_inner(entry.clone())))
            .collect()
    }

    /// Validates this config against the ticket workspace at `path` and
    /// returns a live `Ticket`.
    ///
    /// `path` is the already-resolved ticket directory (the parent of
    /// `.tix/`); the engine does no discovery, so the host — or the caller —
    /// supplies it. Every recorded worktree must exist there and open as a git
    /// repository. Untracked directories under the ticket root are ignored.
    ///
    /// This is the only way to obtain a `Ticket`, and it is a *validation*,
    /// not a document read: nothing is parsed and nothing is written.
    ///
    /// Raises `TixError` if the directory is missing, if a recorded worktree
    /// is absent, or if one is present but not a git repository.
    fn resolve(&self, path: PathBuf) -> PyResult<PyTicket> {
        self.inner
            .clone()
            .resolve(path)
            .map(PyTicket::from_inner)
            .map_err(engine_error)
    }

    fn __repr__(&self) -> String {
        format!(
            "TicketConfig(key={:?}, description={:?}, worktrees={:?})",
            self.inner.key,
            self.inner.description,
            self.inner.worktrees.keys().collect::<Vec<_>>()
        )
    }
}

/// A live, validated ticket: its directory existed and every recorded worktree
/// was present on disk at the moment it was resolved.
///
/// Mirrors [`tix_engine::Ticket`]. Resolve per operation, use, and discard —
/// holding one across arbitrary time says nothing about the state of disk now.
#[pyclass(name = "Ticket", module = "pytix", frozen)]
pub struct PyTicket {
    inner: Ticket,
}

impl PyTicket {
    /// Wraps a ticket the engine has just validated.
    fn from_inner(inner: Ticket) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyTicket {
    /// The ticket's unique key, e.g. `JIRA-123`.
    #[getter]
    fn key(&self) -> &str {
        &self.inner.config.key
    }

    /// The human-readable description of the ticket.
    #[getter]
    fn description(&self) -> &str {
        &self.inner.config.description
    }

    /// The ticket workspace directory.
    #[getter]
    fn path(&self) -> PathBuf {
        self.inner.path.clone()
    }

    /// The verified live worktrees, in worktree-name order.
    #[getter]
    fn worktrees(&self) -> Vec<PyWorktree> {
        self.inner
            .worktrees()
            .iter()
            .cloned()
            .map(PyWorktree::from_inner)
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "Ticket(key={:?}, path={:?}, worktrees={})",
            self.inner.config.key,
            self.inner.path.display().to_string(),
            self.inner.worktrees().len()
        )
    }
}
