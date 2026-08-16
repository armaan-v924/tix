use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The recorded state of one worktree in a ticket document — the value type of
/// [`TicketConfig::worktrees`](crate::TicketConfig::worktrees).
///
/// The map key (the worktree directory name under the ticket root) lives in
/// [`TicketConfig`](crate::TicketConfig); this struct carries what that
/// directory *is*: which repository it belongs to and which branch it has
/// checked out. Repos in one ticket may sit on different branches — worktrees
/// share a branch *prefix*, not a branch.
///
/// # Examples
///
/// As TOML, an entry appears as a sub-table of `worktrees` named after its
/// directory:
///
/// ```
/// # use tix_engine::WorktreeConfig;
/// let entry: WorktreeConfig = toml::from_str(
///     r#"
///     repo = "backend"
///     branch = "feature/JIRA-123-fix-login"
///     "#,
/// )
/// .unwrap();
/// assert_eq!(entry.repo, "backend");
/// assert_eq!(entry.branch, "feature/JIRA-123-fix-login");
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorktreeConfig {
    /// The alias of the repository this worktree belongs to.
    pub repo: String,
    /// The branch checked out in this worktree.
    pub branch: String,
}

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
