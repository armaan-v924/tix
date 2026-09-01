//! The `tix` command-line frontend.
//!
//! This crate exists as a library so that the command tree can be inspected
//! without running the binary: [`tix::TixParser`] is the single clap
//! definition behind `--help`, shell completions, the generated CLI
//! reference, and the man pages. Anything that describes the CLI derives
//! from it, so a new flag cannot appear in one and be missing from another.
//!
//! The binary itself is a thin dispatch layer over [`tix::Commands`]; see
//! `src/main.rs`.

pub mod tix;
