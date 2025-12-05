//! tix: manage ticket-scoped git worktrees across multiple repositories.
//!
//! The library exposes the core modules used by the CLI so they can be tested and documented.
pub mod cli;
pub mod commands;
pub mod config;
pub mod git;
pub mod ticket;
