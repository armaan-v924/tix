use crate::types::repository::Repository;
use crate::types::worktree::{Worktree, WorktreeConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// The `[ticket]` section of the ticket document (`.tix/ticket.toml`).
///
/// Serializable ticket metadata: identity plus per-worktree state. This is a
/// *section* type, not the whole ticket document — plugins get their own
/// `[<plugin>]` tables in the same file, parsed by the SDK's generic document
/// layer.
///
/// Self-contained and portable: no path is baked in, no IO methods exist here,
/// and nothing depends on the global config — the frontend/SDK owns all
/// `.tix/ticket.toml` reading and writing.
///
/// # Examples
///
/// Round-tripping the `[ticket]` section of a ticket document:
///
/// ```
/// # use tix_engine::TicketConfig;
/// let config: TicketConfig = toml::from_str(
///     r#"
///     key = "JIRA-123"
///     description = "Fix the login bug"
///
///     [worktrees.backend]
///     repo = "backend"
///     branch = "feature/JIRA-123-fix-login"
///
///     [worktrees.frontend]
///     repo = "frontend"
///     branch = "feature/JIRA-123-fix-login-ui"
///     "#,
/// )
/// .unwrap();
///
/// assert_eq!(config.key, "JIRA-123");
/// assert_eq!(config.worktrees["backend"].branch, "feature/JIRA-123-fix-login");
/// assert_eq!(config.worktrees["frontend"].repo, "frontend");
///
/// let restored: TicketConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
/// assert_eq!(restored, config);
/// ```
///
/// A freshly created ticket may have no worktrees yet — the map defaults to
/// empty when the field is absent:
///
/// ```
/// # use tix_engine::TicketConfig;
/// let config: TicketConfig = toml::from_str(
///     r#"
///     key = "JIRA-456"
///     description = "A ticket with no worktrees yet"
///     "#,
/// )
/// .unwrap();
/// assert!(config.worktrees.is_empty());
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TicketConfig {
    /// A unique identifier for the ticket (e.g. `JIRA-123`).
    pub key: String,
    /// A human-readable description of the ticket.
    pub description: String,
    /// Worktree state, keyed by worktree directory name under the ticket root.
    ///
    /// Keyed by directory name rather than repo alias so that multiple
    /// worktrees of one repository need no schema change; the single-worktree
    /// case degenerates to `name == alias`. Each entry records its own branch —
    /// there is deliberately no single shared `branch` field, since worktrees
    /// share a branch *prefix*, not a branch.
    #[serde(default)]
    pub worktrees: HashMap<String, WorktreeConfig>,
}

/// A work item with one or more git worktrees sharing a common branch.
#[derive(Serialize, Debug)]
pub struct Ticket {
    /// An identifier for the ticket (e.g. `JIRA-123`).
    pub key: String,
    /// A human-readable description of the ticket.
    pub description: String,
    /// The branch name shared by all worktrees in this ticket.
    pub branch: String,
    /// The path to the ticket workspace directory.
    pub path: PathBuf,
    /// The [`Worktree`] associated with this ticket.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> TicketConfig {
        let mut worktrees = HashMap::new();
        worktrees.insert(
            "backend".to_string(),
            WorktreeConfig {
                repo: "backend".to_string(),
                branch: "feature/JIRA-123-fix-login".to_string(),
            },
        );
        worktrees.insert(
            "frontend".to_string(),
            WorktreeConfig {
                repo: "frontend".to_string(),
                branch: "feature/JIRA-123-fix-login-ui".to_string(),
            },
        );
        TicketConfig {
            key: "JIRA-123".to_string(),
            description: "Fix the login bug".to_string(),
            worktrees,
        }
    }

    /// A full `[ticket]` section deserializes with per-worktree branches intact.
    #[test]
    fn test_deserialize_full_section() {
        let toml = r#"
key = "JIRA-123"
description = "Fix the login bug"

[worktrees.backend]
repo = "backend"
branch = "feature/JIRA-123-fix-login"

[worktrees.frontend]
repo = "frontend"
branch = "feature/JIRA-123-fix-login-ui"
"#;
        let config: TicketConfig = toml::from_str(toml).unwrap();
        assert_eq!(config, sample_config());
    }

    /// Serializing and deserializing preserves all data, including distinct
    /// branches across worktrees.
    #[test]
    fn test_round_trip() {
        let config = sample_config();
        let restored: TicketConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
    }

    /// A document with no `worktrees` table parses to an empty map.
    #[test]
    fn test_missing_worktrees_defaults_empty() {
        let toml = r#"
key = "JIRA-456"
description = "No worktrees yet"
"#;
        let config: TicketConfig = toml::from_str(toml).unwrap();
        assert!(config.worktrees.is_empty());
    }

    /// Unknown fields in the `[ticket]` section are rejected.
    #[test]
    fn test_rejects_unknown_fields() {
        let toml = r#"
key = "JIRA-123"
description = "Fix the login bug"
branch = "a-single-shared-branch-is-an-oversight"
"#;
        assert!(toml::from_str::<TicketConfig>(toml).is_err());
    }

    /// Unknown fields inside a worktree entry are rejected.
    #[test]
    fn test_rejects_unknown_worktree_fields() {
        let toml = r#"
key = "JIRA-123"
description = "Fix the login bug"

[worktrees.backend]
repo = "backend"
branch = "feature/x"
extra = "nope"
"#;
        assert!(toml::from_str::<TicketConfig>(toml).is_err());
    }

    /// Two worktrees of the same repository coexist under distinct directory
    /// names — the shape #85 (multiple worktrees per repo) relies on.
    #[test]
    fn test_multiple_worktrees_of_one_repo() {
        let toml = r#"
key = "JIRA-789"
description = "Compare two branches side by side"

[worktrees.backend]
repo = "backend"
branch = "feature/JIRA-789-attempt-1"

[worktrees.backend-alt]
repo = "backend"
branch = "feature/JIRA-789-attempt-2"
"#;
        let config: TicketConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.worktrees.len(), 2);
        assert_eq!(config.worktrees["backend"].repo, "backend");
        assert_eq!(config.worktrees["backend-alt"].repo, "backend");
        assert_ne!(
            config.worktrees["backend"].branch,
            config.worktrees["backend-alt"].branch
        );
    }
}
