//! The single Python exception raised by every binding in this extension.
//!
//! Rust splits its failures across two types — [`tix_engine::TixError`] for
//! domain operations and [`tix_sdk::SdkError`] for the context-and-consistency
//! layer on top of it — because the crates are layered and the engine must not
//! know about documents. Python has no such layering to preserve: a plugin
//! author catches "tix failed", not "tix failed at a particular architectural
//! altitude". Both collapse onto [`TixError`], whose message is the Rust
//! `Display` output, so the diagnostic survives even though the variant does
//! not.

use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use tix_engine::TixError as EngineError;
use tix_sdk::SdkError;

pyo3::create_exception!(
    pytix,
    TixError,
    PyException,
    "Raised by every failing `pytix` operation, engine or host."
);

/// Wraps an engine failure as the Python-visible exception.
///
/// Free functions rather than `From` impls: both source types are foreign to
/// this crate and so is [`PyErr`], so the orphan rule rules out the blanket
/// conversion that would let `?` do this implicitly.
///
/// The engine names no way past its refusals; every one an argument gets
/// past is worded with that hint here, in Python's terms — `force=True`, or
/// the `base=` of `Repository.create_worktree` and `Repository.resolve_base`.
pub fn engine_error(error: EngineError) -> PyErr {
    let hint = match &error {
        EngineError::Dirty { .. } => "pass force=True to discard them".to_string(),
        EngineError::InvalidWorktree { .. } => "pass force=True to remove it anyway".to_string(),
        EngineError::Diverged { .. } => "pass force=True to reset to origin".to_string(),
        EngineError::BranchLacksBase { .. } => {
            "pass base=None to reuse it as it is, or another branch".to_string()
        }
        EngineError::DefaultBranchNotFetched { branch } => {
            format!("run `git fetch origin {branch}` in the source checkout, or pass base=")
        }
        EngineError::AmbiguousBase { spec } => {
            format!("pass base='refs/heads/{spec}' or base='origin/{spec}' to pick one")
        }
        EngineError::UnknownDefaultBranch { .. } => "pass an explicit base=".to_string(),
        _ => return TixError::new_err(error.to_string()),
    };
    TixError::new_err(format!("{error} ({hint})"))
}

/// Wraps an SDK failure as the Python-visible exception. See [`engine_error`].
pub fn sdk_error(error: SdkError) -> PyErr {
    TixError::new_err(error.to_string())
}

/// A message-only failure originating in the bindings themselves — a value
/// Python handed us that has no TOML or JSON counterpart, say.
pub fn message_error(message: impl Into<String>) -> PyErr {
    TixError::new_err(message.into())
}
