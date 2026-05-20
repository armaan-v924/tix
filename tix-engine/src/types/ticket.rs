use std::path::PathBuf;

use crate::types::worktree::Worktree;
use serde::Serialize;

use crate::types::repository::Repository;

#[derive(Serialize)]
pub struct Ticket {
    pub key: String,
    pub description: String,
    pub branch: String,
    pub path: PathBuf,
    worktrees: Vec<Worktree>,
}

impl Ticket {
    fn new(
        key: String,
        description: String,
        branch: Option<String>,
        path: PathBuf,
        worktrees: Vec<Worktree>,
    ) -> Self {
        // TODO: ensure path exists
        // TODO: resolve branch from key and description
        let branch = "branch-1".into();
        Self {
            key,
            branch,
            description,
            path,
            worktrees,
        }
    }

    fn add(repo: Repository) -> Option<PathBuf> {
        todo!("add repo to ticket")
    }

    fn remove(repo: Repository) -> Option<PathBuf> {
        todo!("remove from a ticket")
    }
}
