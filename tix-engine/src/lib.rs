#![deny(missing_docs)]
//! Core library for the tix workspace manager.
//!
//! `tix-engine` provides the types and operations used by all tix frontends:
//! the CLI ([`tix-cli`]) and the Python bindings ([`pytix`]). It is intentionally
//! free of any UI concerns — all output, prompting, and formatting live in the
//! frontends.
//!
//! # Domain model
//!
//! - A [`Config`] describes the user's environment: where code lives,
//!   where ticket directories are stored, and which repositories are registered.
//! - A [`RepositoryConfig`] is a registered source repository.
//!   Resolving one produces a [`Repository`] backed by a local git clone.
//! - A [`Ticket`] is a unit of work. Each ticket owns one or more
//!   git [`Worktree`]s that share a common branch prefix, keeping
//!   related changes grouped across repositories.
//! - [`TixError`] is the single error type returned by all fallible
//!   operations in this crate.

/// Shared types for the Tix engine.
mod types;

/// Shared utilities for the Tix engine.
mod utils;

pub use types::config::Config;
pub use types::errors::TixError;
pub use types::repository::Repository;
pub use types::repository::RepositoryConfig;
pub use types::ticket::Ticket;
pub use types::worktree::Worktree;
