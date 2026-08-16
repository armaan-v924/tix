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
//! - An [`EngineConfig`] is the `[engine]` section of the global config:
//!   the repositories registered by the user. There is no whole-document
//!   type — documents are parsed generically by the frontend/SDK, and typed
//!   sections are extracted on demand.
//! - A [`RepositoryConfig`] is a registered source repository.
//!   Resolving one produces a [`Repository`] backed by a local git clone.
//! - A [`Defaults`] is the `[defaults]` section of the global config: seed
//!   values read once at ticket creation, never resolved at runtime.
//! - A [`TicketConfig`] is the `[ticket]` section of a ticket document
//!   (`.tix/ticket.toml`): the ticket's identity plus a map of worktree
//!   directory name → [`WorktreeConfig`] recording each worktree's repository
//!   and branch.
//! - A [`Ticket`] is a unit of work. Each ticket owns one or more
//!   git [`Worktree`]s that share a common branch prefix, keeping
//!   related changes grouped across repositories.
//! - [`TixError`] is the single error type returned by all fallible
//!   operations in this crate.

/// Shared types for the Tix engine.
mod types;

/// Shared utilities for the Tix engine.
mod utils;

pub use types::config::EngineConfig;
pub use types::defaults::Defaults;
pub use types::errors::TixError;
pub use types::repository::Repository;
pub use types::repository::RepositoryConfig;
pub use types::ticket::Ticket;
pub use types::ticket::TicketConfig;
pub use types::worktree::Worktree;
pub use types::worktree::WorktreeConfig;
