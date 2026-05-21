use serde::Serialize;
use std::path::PathBuf;

/// Represents a worktree for a repository.
#[derive(Serialize)]
pub struct Worktree {
    /// The alias of the repository.
    pub repo_alias: String,
    /// The path to the worktree directory.
    pub path: PathBuf,
    /// The branch of the worktree.
    pub branch: String,
}
