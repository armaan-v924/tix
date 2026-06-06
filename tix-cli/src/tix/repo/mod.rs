pub mod add;
pub mod clone;

// ---

use clap::{Args, Subcommand};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoAlias(pub String);
impl FromStr for RepoAlias {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RepoAlias(s.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef(pub String);
impl FromStr for RepoRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // resolve using defaults if necessary
        Ok(RepoRef(s.to_string()))
    }
}

#[derive(Args)]
pub struct RepoArgs {
    #[command(subcommand)]
    pub command: RepoCommands,
}

#[derive(Subcommand)]
pub enum RepoCommands {
    Add(add::Args),
    Clone(clone::Args),
}
