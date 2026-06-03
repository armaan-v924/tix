use serde::Serialize;
use std::path::PathBuf;

/// A git worktree associated with a repository and ticket branch.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Worktree {
    /// The alias of the repository.
    pub repo_alias: String,
    /// The path to the worktree directory.
    pub path: PathBuf,
    /// The branch of the worktree.
    pub branch: String,
}
