//! tix core library surface

pub mod core;
// Re-export core modules for convenient `tix::git`, etc.
pub use core::{cli, commands, config, git, ticket};
