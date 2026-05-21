use crate::types::repository::Repository;
use crate::types::worktree::Worktree;
use serde::Serialize;
use std::path::PathBuf;

/// A work item with one or more git worktrees sharing a common branch.
#[derive(Serialize, Debug)]
pub struct Ticket {
    /// A unique identifier for the ticket (e.g. `JIRA-123`).
    pub key: String,
    /// A human-readable description of the ticket.
    pub description: String,
    /// The branch name shared by all worktrees in this ticket.
    pub branch: String,
    /// The path to the ticket workspace directory.
    pub path: PathBuf,
    /// The worktrees associated with this ticket.
    pub worktrees: Vec<Worktree>,
}

impl Ticket {
    /// Creates a new `Ticket`.
    fn new(
        key: String,
        description: String,
        branch: String,
        path: PathBuf,
        worktrees: Vec<Worktree>,
    ) -> Self {
        // TODO: ensure path exists
        // TODO: resolve branch from key and description
        Self {
            key,
            branch,
            description,
            path,
            worktrees,
        }
    }

    /// Adds a worktree for `repo` to this ticket.
    fn add(repo: Repository) -> Option<PathBuf> {
        todo!("add repo to ticket")
    }

    /// Removes the worktree for `repo` from this ticket.
    fn remove(repo: Repository) -> Option<PathBuf> {
        todo!("remove from a ticket")
    }
}
