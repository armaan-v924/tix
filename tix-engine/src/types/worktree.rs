use std::path::PathBuf;

use serde::Serialize;

#[derive(Serialize)]
pub struct Worktree {
    pub repo_alias: String,
    pub path: PathBuf,
}
