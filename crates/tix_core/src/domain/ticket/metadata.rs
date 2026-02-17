use crate::domain::ids::{BranchName, RepoAlias, TicketId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// On-disk schema for `<ticket_root>/.tix/info.toml`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketMetadata {
    pub id: TicketId,

    /// Optional human description (freeform).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// RFC3339 timestamp string for now (simple + portable)
    // TODO: Switch to chrono types
    pub created_at: String,

    /// Default branch name for the ticket (e.g., `prefix/KEY-###-desc`)
    pub branch_name: BranchName,

    /// Repositories linked to this ticket, keyed by repo alias
    ///
    /// Using a map means stable serialization order and easy lookup
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repos: BTreeMap<RepoAlias, TicketRepoState>
}

/// Per-repo per-ticket state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketRepoState {
    /// Optional per-repo branch override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_name: Option<BranchName>,

    /// Optional worktree path (absolute or relative to ticket root; decide later).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<PathBuf>,
}

impl TicketMetadata {
    pub fn new(id: TicketId, created_at: String, branch_name: BranchName) -> Self {
        Self {
            id,
            description: None,
            created_at,
            branch_name,
            repos: BTreeMap::new(),
        }
    }
}