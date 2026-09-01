#![deny(missing_docs)]
//! The context-and-consistency layer shared by `tix-cli` and plugins.
//!
//! `tix-sdk` is deliberately coupled to `tix-cli`: it is the CLI's own
//! context layer, shipped as a crate so plugins inherit identical behavior
//! for free. Surface grows by **promotion from `tix-cli`** — a helper the
//! CLI wants applied consistently moves here — never by speculation about
//! plugin needs.
//!
//! What lives here:
//!
//! - [`context`] — global config path resolution
//!   (`--config` > `TIX_CONFIG_PATH` > platform default).
//! - [`discovery`] — the ticket walk and `--ticket` override semantics.
//! - [`document`] — the format-preserving TOML document layer: generic
//!   parse, typed section extraction, atomic locked writes.
//! - [`delta`] — diff-back config deltas: the plugin write helper and the
//!   host's JSON→TOML apply mechanics.
//! - [`host`] — the plugin side of the invocation contract: `--tix-*` flag
//!   parsing and the protocol check ([`host::PROTOCOL`]; the version →
//!   change table is published with the
//!   [plugin documentation](https://tix.armaanv.dev/latest/plugins/protocol/)).
//! - [`spawn`] — nested `tix` invocations pinned to the current ticket.
//! - [`state`] — plugin state directories, created lazily.
//! - [`SdkError`] — the SDK's error type; document parse/serialize errors
//!   live here, not on the engine.
//!
//! `tix-engine` is re-exported wholesale (tokio/axum style): depend on
//! `tix-sdk` alone and reach engine types as `tix_sdk::Ticket`,
//! `tix_sdk::RepositoryConfig`, and so on. Frontends that want no tix
//! layout policy may still depend on `tix-engine` directly.

pub mod context;
pub mod delta;
pub mod discovery;
pub mod document;
/// The SDK's error type.
pub mod error;
pub mod host;
pub mod spawn;
pub mod state;

pub use error::SdkError;
// Re-export the engine wholesale: the dependency chain is linear
// ({tix-cli, pytix} → tix-sdk → tix-engine), and the re-export is what
// makes lockstep versioning structurally true rather than conventional.
pub use tix_engine::*;
