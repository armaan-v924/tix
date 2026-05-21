use crate::types::repository::Repository;
use crate::types::worktree::Worktree;
use serde::Serialize;
use std::path::PathBuf;

/// A ticket represents a work item.
/// Tickets may contain one or more worktrees with a common branch.
#[derive(Serialize)]
pub struct Ticket {
    /// A unique identifier for the ticket.
    pub key: String,
    /// A human-readable description of the ticket.
    pub description: String,
    /// The branch name shared by all worktrees in the ticket.
    pub branch: String,
    /// The path to the ticket's worktree directory.
    pub path: PathBuf,
    /// The worktrees associated with the ticket.
    pub worktrees: Vec<Worktree>, // todo : split into worktree config and worktree live object
}

impl Ticket {
    /// Creates a new ticket with the given key, description, branch, path, and worktrees.
    ///
    /// # Arguments
    ///
    /// * `key` - A unique identifier for the ticket.
    /// * `description` - A human-readable description of the ticket.
    /// * `branch` - The branch name shared by all worktrees in the ticket.
    /// * `path` - The path to the ticket's worktree directory.
    /// * `worktrees` - The worktrees associated with the ticket.
    ///
    /// # Returns
    ///
    /// A new `Ticket` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use tix_engine::types::Ticket;
    /// let ticket = Ticket::new("TIX-123".into(), "Fix bug".into(), "branch-1".into(), PathBuf::new(), vec![]);
    /// ```
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

    /// Adds a worktree for the given repository to the ticket.
    ///
    /// # Arguments
    ///
    /// * `repo` - The repository to add.
    ///
    /// # Returns
    ///
    /// The path to the ticket's worktree directory, if the repository was added successfully.
    ///
    /// # Examples
    ///
    /// ```
    /// use tix_engine::types::Ticket;
    /// let ticket = Ticket::new("TIX-123".into(), "Fix bug".into(), "branch-1".into(), PathBuf::new(), vec![]);
    /// ```
    fn add(repo: Repository) -> Option<PathBuf> {
        todo!("add repo to ticket")
    }

    /// Removes a worktree for the given repository from the ticket.
    ///
    /// # Arguments
    ///
    /// * `repo` - The repository to remove.
    ///
    /// # Returns
    ///
    /// The path to the ticket's worktree directory, if the repository was removed successfully.
    ///
    /// # Examples
    ///
    /// ```
    /// use tix_engine::types::Ticket;
    /// let ticket = Ticket::new("TIX-123".into(), "Fix bug".into(), "branch-1".into(), PathBuf::new(), vec![]);
    /// ```
    fn remove(repo: Repository) -> Option<PathBuf> {
        todo!("remove from a ticket")
    }
}
