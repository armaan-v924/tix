use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The recorded state of one worktree in a ticket document — the value type of
/// [`TicketConfig::worktrees`](crate::TicketConfig::worktrees).
///
/// The map key (the worktree directory name under the ticket root) lives in
/// [`TicketConfig`](crate::TicketConfig); this struct carries what that
/// directory *is*: which repository it belongs to and which branch it has
/// checked out. Repos in one ticket may sit on different branches — worktrees
/// share a branch *prefix* rather than one branch.
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
/// assert_eq!(entry.base, None);
/// ```
///
/// A worktree tix cut a branch for also records the base it was cut from:
///
/// ```toml
/// [ticket.worktrees.tix-part2]
/// repo = "tix"
/// branch = "stack/JIRA-1-part-2"
/// base = "stack/JIRA-1-part-1"
/// ```
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorktreeConfig {
    /// The alias of the repository this worktree belongs to.
    pub repo: String,
    /// The branch checked out in this worktree.
    pub branch: String,
    /// The ref `branch` was cut from, as resolved at creation — a name such
    /// as `origin/main` or a sibling's `stack/JIRA-1-part-1`, never a SHA.
    ///
    /// A name is what a pull request's base takes, and it is what makes
    /// stack topology recoverable from disk: git itself only remembers the
    /// merge-base. Absent when tix did not cut the branch (it already
    /// existed) or the entry predates the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// A live git worktree associated with a repository and ticket branch.
///
/// Produced by [`TicketConfig::resolve`](crate::TicketConfig::resolve) (from
/// recorded state verified against disk) or by
/// [`Repository::create_worktree`](crate::Repository::create_worktree) (from a
/// worktree it just created). Holding one means the worktree existed at the
/// time it was produced.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Worktree {
    /// The worktree directory name under the ticket root — the key of this
    /// worktree's entry in [`TicketConfig::worktrees`](crate::TicketConfig::worktrees).
    pub name: String,
    /// The alias of the repository.
    pub repo_alias: String,
    /// The path to the worktree directory.
    pub path: PathBuf,
    /// The branch of the worktree.
    pub branch: String,
    /// The ref the branch was cut from — see [`WorktreeConfig::base`].
    pub base: Option<String>,
}
