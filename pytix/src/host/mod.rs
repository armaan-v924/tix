//! The `pytix.host` namespace: bindings for `tix-sdk`.
//!
//! Where `pytix.*` binds the engine — domain operations over resolved paths —
//! this namespace binds the layer that decides what those paths are and how
//! documents are read and written. It is the plugin's handle on the process
//! that invoked it: the flags the host forwarded, the documents those flags
//! point at, the plugin's own state directory, and the delta channel back.
//!
//! The surface is deliberately narrower than the Rust SDK's and may trail
//! it: it grows as real Python plugins need it, ahead of nothing.

pub mod context;
pub mod delta;
pub mod document;

use crate::error::sdk_error;
use pyo3::prelude::*;
use std::path::PathBuf;

/// The global cache directory for `plugin`, created on call.
///
/// A convenience only — there is no consistency benefit to tix mediating what
/// the platform cache location already names. Per-*ticket* state is
/// `HostContext.state_dir`, which does need the host to say where the ticket
/// is.
///
/// Raises `TixError` when the platform cache directory cannot be determined,
/// or if creation fails.
#[pyfunction]
pub fn cache_dir(plugin: &str) -> PyResult<PathBuf> {
    tix_sdk::state::cache_dir(plugin).map_err(sdk_error)
}
