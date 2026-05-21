use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct Worktree {
    pub repo_alias: String,
    pub path: PathBuf,
}
