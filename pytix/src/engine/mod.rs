//! The `pytix.*` namespace: bindings for `tix-engine`.
//!
//! One binding type per engine type, each a thin wrapper holding the engine
//! value and translating [`tix_engine::TixError`] into the module's
//! [`TixError`](crate::error::TixError) exception. No behaviour is added and
//! none is removed: the engine's contract — resolved paths in, no ambient IO,
//! no layout policy — is the binding's contract too.

pub mod repository;
pub mod ticket;
pub mod worktree;
