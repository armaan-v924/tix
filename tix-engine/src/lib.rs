#![deny(missing_docs)]
//! Core library for the tix workspace manager.
//!
//! `tix-engine` provides the types and operations used by all tix frontends:
//! the CLI (`tix-cli`) and the Python bindings (`pytix`). It is intentionally
//! free of any UI concerns — all output, prompting, and formatting live in the
//! frontends.
//!
//! # The engine contract
//!
//! This crate performs **domain operations over already-resolved paths** and
//! nothing else (`design/spec.md` §2.2):
//!
//! - **Resolved paths in.** No discovery, no path resolution, no layout
//!   policy — where config, tickets, and code live is frontend/SDK business
//!   and must not leak in here.
//! - **No ambient IO.** No env vars, no stdout/stderr, no process control.
//!   Tracing macros (`info!`, `debug!`, …) are fine — they emit to whatever
//!   subscriber the frontend wired up. Enforced by audit:
//!   `grep -r "process::exit\|println!\|eprintln!\|env::var\|tracing_subscriber" src/`
//!   must return nothing.
//! - **Runtime dependencies are `git2`, `serde`, and `tracing` — only.**
//!   No `clap` (arg parsing), no `dirs` (config location), no `toml`
//!   (document parsing): those concerns are all SDK-side. `toml` appears as
//!   a dev-dependency solely to round-trip test the serde section shapes.
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

/// Credential wiring for authenticated remotes.
mod auth;

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
pub use utils::opens_as_git_repository;

#[cfg(test)]
mod build_config {
    /// libgit2 must be compiled with TLS and SSH transports.
    ///
    /// Not a test of git2 itself but of *our* feature selection: git2 0.21
    /// ships `default = []`, so a bare dependency declaration silently
    /// yields a binary whose every remote operation fails at runtime with
    /// "there is no TLS stream available". Nothing else in the suite
    /// notices, because the failure needs a real network clone to surface.
    #[test]
    fn test_transports_compiled_in() {
        let version = git2::Version::get();
        assert!(version.https(), "libgit2 built without HTTPS support");
        assert!(version.ssh(), "libgit2 built without SSH support");
    }
}
