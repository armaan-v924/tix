use crate::types::errors::TixError;
use crate::types::worktree::{Worktree, WorktreeConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, error};

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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

impl TicketConfig {
    /// Validates this config against disk and returns a live [`Ticket`].
    ///
    /// `path` is the ticket workspace directory (the parent of `.tix/`),
    /// already resolved by the frontend — the engine does no discovery. Every
    /// entry in [`Self::worktrees`] is verified: its directory must exist
    /// under `path` and be openable as a git worktree. Untracked directories
    /// under the ticket root are ignored — scratch space is allowed.
    ///
    /// `resolve()` promises validity: if you hold a [`Ticket`], its directory
    /// and every recorded worktree existed at resolution time. Frontends
    /// resolve per command, use the result, and discard it.
    ///
    /// # Errors
    ///
    /// - [`TixError::TicketNotFound`] if `path` does not exist or is not a
    ///   directory
    /// - [`TixError::WorktreeNotFound`] if a recorded worktree's directory is
    ///   missing
    /// - [`TixError::GitError`] if a recorded worktree's directory exists but
    ///   cannot be opened as a git repository
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::TicketConfig;
    /// # use std::path::PathBuf;
    /// # fn main() -> Result<(), tix_engine::TixError> {
    /// let config: TicketConfig = toml::from_str(
    ///     r#"
    ///     key = "JIRA-123"
    ///     description = "Fix the login bug"
    ///
    ///     [worktrees.backend]
    ///     repo = "backend"
    ///     branch = "feature/JIRA-123-fix-login"
    ///     "#,
    /// )
    /// .unwrap();
    ///
    /// let ticket = config.resolve(PathBuf::from("/home/user/tickets/JIRA-123"))?;
    /// let branches: Vec<&str> = ticket
    ///     .worktrees()
    ///     .iter()
    ///     .map(|worktree| worktree.branch.as_str())
    ///     .collect();
    /// # Ok(())
    /// # }
    /// ```
    pub fn resolve(self, path: PathBuf) -> Result<Ticket, TixError> {
        debug!(key = %self.key, path = %path.display(), "resolving ticket");
        if !path.is_dir() {
            error!(key = %self.key, path = %path.display(), "ticket directory does not exist");
            return Err(TixError::TicketNotFound(path.display().to_string()));
        }

        // Sort by name so the resulting Vec is deterministic — HashMap
        // iteration order is not.
        let mut entries: Vec<(&String, &WorktreeConfig)> = self.worktrees.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let mut worktrees = Vec::with_capacity(entries.len());
        for (name, entry) in entries {
            let worktree_path = path.join(name);
            if !worktree_path.is_dir() {
                error!(key = %self.key, worktree = %name, path = %worktree_path.display(), "recorded worktree is missing on disk");
                return Err(TixError::WorktreeNotFound(format!(
                    "'{}' recorded in ticket '{}' but missing at {}",
                    name,
                    self.key,
                    worktree_path.display()
                )));
            }
            // The directory must actually be a git worktree, not just any
            // directory that happens to share the recorded name.
            git2::Repository::open(&worktree_path).map_err(|e| {
                error!(key = %self.key, worktree = %name, error = %e, "recorded worktree is not a git repository");
                TixError::GitError(e)
            })?;
            worktrees.push(Worktree {
                name: name.clone(),
                repo_alias: entry.repo.clone(),
                branch: entry.branch.clone(),
                path: worktree_path,
            });
        }

        debug!(key = %self.key, worktrees = worktrees.len(), "ticket resolved");
        Ok(Ticket {
            config: self,
            path,
            worktrees,
        })
    }
}

/// A live, validated ticket.
///
/// If you hold a `Ticket`, it is valid: the ticket directory exists and every
/// worktree recorded in its [`TicketConfig`] was present on disk at resolution
/// time. Construct one via [`TicketConfig::resolve`]; frontends resolve what
/// they need per command, use it, and discard it.
///
/// `Ticket` is deliberately not serializable — it is a live object, not
/// stored state. The stored form is [`TicketConfig`].
#[derive(Debug)]
pub struct Ticket {
    /// The `[ticket]` section this ticket was resolved from.
    pub config: TicketConfig,
    /// The path to the ticket workspace directory.
    pub path: PathBuf,
    /// The verified live worktrees, in worktree-name order.
    worktrees: Vec<Worktree>,
}

impl Ticket {
    /// The verified live worktrees of this ticket, sorted by worktree name.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use tix_engine::TicketConfig;
    /// # use std::path::PathBuf;
    /// # fn main() -> Result<(), tix_engine::TixError> {
    /// # let config: TicketConfig = toml::from_str(r#"
    /// # key = "JIRA-123"
    /// # description = "Fix the login bug"
    /// # "#).unwrap();
    /// let ticket = config.resolve(PathBuf::from("/home/user/tickets/JIRA-123"))?;
    /// let paths: Vec<_> = ticket.worktrees().iter().map(|w| &w.path).collect();
    /// # Ok(())
    /// # }
    /// ```
    pub fn worktrees(&self) -> &[Worktree] {
        &self.worktrees
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repository::test_helpers;
    use std::path::Path;
    use tempfile::tempdir;

    /// Builds a real git repo and a worktree of it at `ticket_root/<name>` on
    /// branch `<name>`, returning a `TicketConfig` entry-compatible pair.
    fn setup_worktree(dir: &Path, name: &str, ticket_root: &Path) {
        let remote_path = dir.join(format!("{name}-remote"));
        let local_path = dir.join(format!("{name}-local"));
        let remote = test_helpers::init_bare_repo(&remote_path);
        test_helpers::add_commit_to_bare(&remote, "initial commit");
        let local = test_helpers::clone_repo(&remote_path, &local_path);
        test_helpers::add_worktree(&local, name, &ticket_root.join(name));
    }

    fn config_with(worktrees: &[(&str, &str, &str)]) -> TicketConfig {
        TicketConfig {
            key: "JIRA-123".to_string(),
            description: "Fix the login bug".to_string(),
            worktrees: worktrees
                .iter()
                .map(|(name, repo, branch)| {
                    (
                        name.to_string(),
                        WorktreeConfig {
                            repo: repo.to_string(),
                            branch: branch.to_string(),
                        },
                    )
                })
                .collect(),
        }
    }

    // --- resolve ---

    /// A ticket directory with every recorded worktree present resolves into a
    /// live `Ticket` carrying verified worktrees in name order.
    #[test]
    fn test_resolve_valid() {
        let dir = tempdir().unwrap();
        let ticket_root = dir.path().join("ticket");
        std::fs::create_dir_all(&ticket_root).unwrap();
        setup_worktree(dir.path(), "backend", &ticket_root);
        setup_worktree(dir.path(), "frontend", &ticket_root);

        let config = config_with(&[
            ("frontend", "frontend", "frontend"),
            ("backend", "backend", "backend"),
        ]);
        let ticket = config.clone().resolve(ticket_root.clone()).unwrap();

        assert_eq!(ticket.config, config);
        assert_eq!(ticket.path, ticket_root);
        let names: Vec<&str> = ticket.worktrees().iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["backend", "frontend"]);
        assert_eq!(ticket.worktrees()[0].repo_alias, "backend");
        assert_eq!(ticket.worktrees()[0].path, ticket_root.join("backend"));
    }

    /// A missing ticket directory errors with `TicketNotFound`.
    #[test]
    fn test_resolve_missing_ticket_dir() {
        let dir = tempdir().unwrap();
        let config = config_with(&[]);
        assert!(matches!(
            config.resolve(dir.path().join("nonexistent")),
            Err(TixError::TicketNotFound(_))
        ));
    }

    /// A recorded worktree whose directory is missing errors with
    /// `WorktreeNotFound` — resolve() promises validity.
    #[test]
    fn test_resolve_missing_recorded_worktree() {
        let dir = tempdir().unwrap();
        let ticket_root = dir.path().join("ticket");
        std::fs::create_dir_all(&ticket_root).unwrap();

        let config = config_with(&[("backend", "backend", "feature/x")]);
        assert!(matches!(
            config.resolve(ticket_root),
            Err(TixError::WorktreeNotFound(_))
        ));
    }

    /// A recorded worktree whose directory exists but is not a git repository
    /// errors with `GitError`.
    #[test]
    fn test_resolve_recorded_worktree_not_git() {
        let dir = tempdir().unwrap();
        let ticket_root = dir.path().join("ticket");
        std::fs::create_dir_all(ticket_root.join("backend")).unwrap();

        let config = config_with(&[("backend", "backend", "feature/x")]);
        assert!(matches!(
            config.resolve(ticket_root),
            Err(TixError::GitError(_))
        ));
    }

    /// Untracked directories under the ticket root are ignored — scratch
    /// space is allowed.
    #[test]
    fn test_resolve_ignores_untracked_directories() {
        let dir = tempdir().unwrap();
        let ticket_root = dir.path().join("ticket");
        std::fs::create_dir_all(ticket_root.join("scratch-notes")).unwrap();
        setup_worktree(dir.path(), "backend", &ticket_root);

        let config = config_with(&[("backend", "backend", "backend")]);
        let ticket = config.resolve(ticket_root).unwrap();
        assert_eq!(ticket.worktrees().len(), 1);
    }

    /// A ticket with no recorded worktrees resolves to a live ticket with an
    /// empty worktree list.
    #[test]
    fn test_resolve_empty_worktrees() {
        let dir = tempdir().unwrap();
        let ticket_root = dir.path().join("ticket");
        std::fs::create_dir_all(&ticket_root).unwrap();

        let ticket = config_with(&[]).resolve(ticket_root).unwrap();
        assert!(ticket.worktrees().is_empty());
    }

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
