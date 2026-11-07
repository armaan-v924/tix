pub mod add;
pub mod destroy;
pub mod info;
pub mod list;
pub mod remove;
pub mod setup;

// ---

use clap::{Args, Subcommand};
pub use tix_sdk::discovery::TicketRef;

/// Derives the branch name seeded into a new worktree:
/// `<prefix>/<key>-<sanitized-description>` (v2 parity).
///
/// The prefix and the description are each optional and drop out cleanly:
/// no prefix means no `<prefix>/`, no (or fully unsanitizable) description
/// means the key alone. Derivation happens **once** — at `tix ticket setup`
/// or `tix ticket add` — and the result is written into the ticket document;
/// later changes to `[defaults]` never rename an existing branch.
///
/// # Examples
///
/// ```text
/// derive_branch_name(Some("feature"), "JIRA-123", Some("Fix the Login Bug!"))
///     == "feature/JIRA-123-fix-the-login-bug"
/// derive_branch_name(None, "JIRA-123", None) == "JIRA-123"
/// ```
pub fn derive_branch_name(prefix: Option<&str>, key: &str, description: Option<&str>) -> String {
    let sanitized = description.map(sanitize_description).unwrap_or_default();
    let stem = if sanitized.is_empty() {
        key.to_string()
    } else {
        format!("{key}-{sanitized}")
    };
    match prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}/{stem}"),
        _ => stem,
    }
}

/// Lowercases, maps every non-alphanumeric run to a single `-`, trims the
/// ends, and caps the result at 40 characters (without splitting a word run
/// mid-hyphen cleanup).
fn sanitize_description(description: &str) -> String {
    const MAX_LENGTH: usize = 40;
    let mut out = String::with_capacity(description.len());
    for c in description.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    let capped = trimmed.chars().take(MAX_LENGTH).collect::<String>();
    capped.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod branch_name_tests {
    use super::*;

    /// Full derivation: prefix, key, sanitized description.
    #[test]
    fn test_full_derivation() {
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", Some("Fix the Login Bug!")),
            "feature/JIRA-123-fix-the-login-bug"
        );
    }

    /// Prefix and description each drop out cleanly when absent.
    #[test]
    fn test_optional_parts_drop_out() {
        assert_eq!(
            derive_branch_name(None, "JIRA-123", Some("desc")),
            "JIRA-123-desc"
        );
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", None),
            "feature/JIRA-123"
        );
        assert_eq!(derive_branch_name(None, "JIRA-123", None), "JIRA-123");
        assert_eq!(derive_branch_name(Some(""), "JIRA-123", None), "JIRA-123");
    }

    /// Sanitization collapses punctuation runs, trims, and caps length.
    #[test]
    fn test_sanitization() {
        assert_eq!(
            sanitize_description("Fix: the (login) bug!!"),
            "fix-the-login-bug"
        );
        assert_eq!(sanitize_description("---"), "");
        assert!(
            sanitize_description(
                "a very long description that goes on and on and on far past the cap"
            )
            .len()
                <= 40
        );
    }

    /// A description that sanitizes to nothing behaves like no description.
    #[test]
    fn test_unsanitizable_description() {
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", Some("!!!")),
            "feature/JIRA-123"
        );
    }
}

/// Loads the required `[cli]` section from the resolved global config.
///
/// Shared by every ticket subcommand; the "no `[cli]` section" error points at
/// `tix config init` rather than leaking a parse detail.
pub fn load_cli_config(
    context: &tix_sdk::context::Context,
) -> Result<crate::tix::config::CliConfig, tix_sdk::SdkError> {
    let document = tix_sdk::document::TixDocument::load(&context.config_path)?;
    document.section("cli")?.ok_or_else(|| {
        tix_sdk::SdkError::Message(
            "global config has no [cli] section — run `tix config init`".to_string(),
        )
    })
}

/// Loads the `[engine]` section from the resolved global config — the
/// registered repositories — defaulting to empty when it is absent.
///
/// For commands that need nothing else from the global config; those that
/// read several sections load the document once themselves.
pub fn load_engine_config(
    context: &tix_sdk::context::Context,
) -> Result<tix_sdk::EngineConfig, tix_sdk::SdkError> {
    tix_sdk::document::TixDocument::load(&context.config_path)?.section_or_default("engine")
}

/// Reads the `[ticket]` section of the ticket document at `ticket_root`.
///
/// # Errors
///
/// [`tix_sdk::TixError::TicketNotFound`] (wrapped) when the document has no
/// `[ticket]` section; parse errors propagate from the document layer.
pub fn load_ticket_config(
    ticket_root: &std::path::Path,
) -> Result<tix_sdk::TicketConfig, tix_sdk::SdkError> {
    let path = tix_sdk::discovery::ticket_document_path(ticket_root);
    let document = tix_sdk::document::TixDocument::load(&path)?;
    document.section("ticket")?.ok_or_else(|| {
        tix_sdk::SdkError::Engine(tix_sdk::TixError::TicketNotFound(format!(
            "{} has no [ticket] section",
            path.display()
        )))
    })
}

/// Whether a recorded worktree actually resolves on disk — what `tix info`
/// flags, and what decides how `remove` and `destroy` handle it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeStatus {
    /// A directory that opens as a git repository.
    Ok,
    /// No directory at the recorded path.
    MissingDirectory,
    /// A directory, but not one git can open.
    NotAGitRepository,
}

impl WorktreeStatus {
    /// Classifies the worktree recorded at `path`.
    pub fn of(path: &std::path::Path) -> Self {
        if !path.is_dir() {
            WorktreeStatus::MissingDirectory
        } else if !tix_sdk::opens_as_git_repository(path) {
            WorktreeStatus::NotAGitRepository
        } else {
            WorktreeStatus::Ok
        }
    }

    /// The suffix `tix info` prints after the row.
    pub fn marker(&self) -> &'static str {
        match self {
            WorktreeStatus::Ok => "",
            WorktreeStatus::MissingDirectory => "  ⚠ missing on disk",
            WorktreeStatus::NotAGitRepository => "  ⚠ not a git repository",
        }
    }

    /// The `status` value in `tix info -o json`.
    pub fn label(&self) -> &'static str {
        match self {
            WorktreeStatus::Ok => "ok",
            WorktreeStatus::MissingDirectory => "missing",
            WorktreeStatus::NotAGitRepository => "not-a-git-repository",
        }
    }
}

/// Prunes the worktree `name` of the ticket at `root` through its source
/// repository, then deletes its `[ticket.worktrees.<name>]` entry. Returns
/// what was found on disk.
///
/// A worktree that is [missing](WorktreeStatus::MissingDirectory) or
/// [not a git repository](WorktreeStatus::NotAGitRepository) — usually
/// deleted by hand — has nothing to check and nothing to remove: only git's
/// stale registration is pruned, if there still is one, and the entry is
/// dropped with a warning. Whatever is at its path stays there.
///
/// The entry goes as soon as the prune lands, so a batch that fails partway
/// leaves the ticket document agreeing with disk and a retry resumes.
/// `force` prunes past uncommitted changes, discarding them.
///
/// # Errors
///
/// [`tix_sdk::TixError::RepoNotFound`] when `entry.repo` is not registered;
/// otherwise whatever resolving the repository, pruning, or writing the
/// ticket document returns.
pub fn prune_worktree(
    root: &std::path::Path,
    name: &str,
    entry: &tix_sdk::WorktreeConfig,
    engine: &tix_sdk::EngineConfig,
    force: bool,
) -> Result<WorktreeStatus, tix_sdk::SdkError> {
    let config = engine
        .configured_repositories
        .get(&entry.repo)
        .cloned()
        .ok_or_else(|| {
            tix_sdk::TixError::RepoNotFound(format!(
                "worktree '{}' belongs to '{}', which is not a registered repository",
                name, entry.repo
            ))
        })?;
    let repo = config.resolve(&entry.repo)?;
    let path = root.join(name);
    let status = WorktreeStatus::of(&path);
    match status {
        WorktreeStatus::Ok => repo.remove_worktree(&path, force)?,
        WorktreeStatus::MissingDirectory | WorktreeStatus::NotAGitRepository => {
            let pruned = repo.prune_worktree_registration(&path)?;
            tracing::warn!(
                "worktree '{name}' is {}; dropping it from the ticket{}",
                match status {
                    WorktreeStatus::MissingDirectory => "missing on disk",
                    _ => "not a git repository",
                },
                if pruned {
                    format!(
                        " and pruning its registration in {}",
                        repo.config.code_path.display()
                    )
                } else {
                    String::new()
                }
            );
        }
    }

    tix_sdk::document::with_write(&tix_sdk::discovery::ticket_document_path(root), |doc| {
        // `table_at` rather than a plain lookup: a document written by a
        // version that collapsed the section into an inline table (#146)
        // holds no `Table` to remove from.
        doc.table_at(&["ticket", "worktrees"])?.remove(name);
        Ok(())
    })?;
    Ok(status)
}

/// Resolves the ticket a command operates on — `--ticket` override or the
/// discovery walk — and errors clearly when neither yields one.
///
/// `project` is the `--project` flag, and it means one thing whichever way
/// the ticket is named:
///
/// - **Id form** (`--ticket JIRA-1`): the id resolves inside the command's
///   project — the flag, else the project inferred from cwd, else loose
///   (directly in the tickets directory). From inside `acme/JIRA-1`,
///   `--ticket JIRA-2` therefore means `acme/JIRA-2`.
/// - **Path form or discovery**: the ticket is already pinned down, so the
///   flag is an assertion that it belongs to that project, and a mismatch
///   errors — in keeping with `--ticket` being an assertion itself.
///
/// The error for "no ticket" is the "requires ticket context" message every
/// ticket-scoped command shares; commands that merely prefer context call
/// the discovery layer directly.
pub fn require_ticket_root(
    context: &tix_sdk::context::Context,
    ticket: Option<&TicketRef>,
    project: Option<&str>,
) -> Result<std::path::PathBuf, tix_sdk::SdkError> {
    let cli = load_cli_config(context)?;
    resolve_ticket_in(
        ticket,
        project,
        &cli.tickets_directory,
        &tix_sdk::discovery::logical_cwd()?,
    )
}

/// [`require_ticket_root`] over an explicit tickets directory and (logical)
/// cwd — the part that needs no config.
fn resolve_ticket_in(
    ticket: Option<&TicketRef>,
    project: Option<&str>,
    tickets_directory: &std::path::Path,
    cwd: &std::path::Path,
) -> Result<std::path::PathBuf, tix_sdk::SdkError> {
    use tix_sdk::project::{Project, resolve_project, tickets_home};
    use tix_sdk::{SdkError, TixError};

    if let Some(id_form @ TicketRef::Id(id)) = ticket {
        let scope = resolve_project(project, tickets_directory, cwd)?;
        let home = tickets_home(scope.as_ref(), tickets_directory);
        return tix_sdk::discovery::resolve_override(id_form, home).map_err(|e| match &scope {
            // Inferred, not asked for: say where the id was looked up and
            // how to reach the loose ticket it may have meant.
            Some(inferred) if project.is_none() && tickets_directory.join(id).is_dir() => {
                SdkError::Message(format!(
                    "{e} — inside project '{}', ids resolve within it; pass the path \
                         (e.g. '{}') for the loose ticket",
                    inferred.name,
                    tickets_directory.join(id).display()
                ))
            }
            _ => e,
        });
    }

    let root = match ticket {
        Some(path_form) => tix_sdk::discovery::resolve_override(path_form, tickets_directory)?,
        None => tix_sdk::discovery::discover_ticket_root_from(cwd).ok_or_else(|| {
            SdkError::Engine(TixError::TicketNotFound(
                "not inside a ticket — cd into one or pass --ticket <path|id>".to_string(),
            ))
        })?,
    };
    if let Some(name) = project {
        let asserted = Project::named(tickets_directory, name)?;
        if !asserted.contains(&root) {
            let actual = tix_sdk::project::project_of(&root).map_or_else(
                || "it is a loose ticket".to_string(),
                |p| format!("it is in '{}'", p.name),
            );
            return Err(SdkError::Message(format!(
                "the ticket at {} is not in project '{name}' — {actual}",
                root.display()
            )));
        }
    }
    Ok(root)
}

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    fn make_ticket(root: &Path) {
        std::fs::create_dir_all(root.join(".tix")).unwrap();
        std::fs::write(root.join(".tix/ticket.toml"), "").unwrap();
    }

    /// `<tickets>/JIRA-1` (loose) and `<tickets>/acme/JIRA-2` (in `acme`).
    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let tickets = dir.path().join("tickets");
        make_ticket(&tickets.join("JIRA-1"));
        tix_sdk::project::Project::named(&tickets, "acme")
            .unwrap()
            .ensure()
            .unwrap();
        make_ticket(&tickets.join("acme/JIRA-2"));
        (dir, tickets)
    }

    fn id(s: &str) -> TicketRef {
        TicketRef::Id(s.to_string())
    }

    /// An id resolves loose from outside any project, and within the
    /// project when `--project` names it.
    #[test]
    fn test_id_resolves_loose_or_in_flagged_project() {
        let (dir, tickets) = fixture();
        let outside = dir.path();
        assert_eq!(
            resolve_ticket_in(Some(&id("JIRA-1")), None, &tickets, outside).unwrap(),
            tickets.join("JIRA-1")
        );
        assert_eq!(
            resolve_ticket_in(Some(&id("JIRA-2")), Some("acme"), &tickets, outside).unwrap(),
            tickets.join("acme/JIRA-2")
        );
        assert!(resolve_ticket_in(Some(&id("JIRA-2")), None, &tickets, outside).is_err());
    }

    /// Deep inside a project ticket, a bare id means a sibling in the same
    /// project — and the loose ticket of that name needs its path.
    #[test]
    fn test_id_inferred_from_inside_project_ticket() {
        let (_dir, tickets) = fixture();
        let deep = tickets.join("acme/JIRA-2/api/src");
        std::fs::create_dir_all(&deep).unwrap();
        make_ticket(&tickets.join("acme/JIRA-3"));

        assert_eq!(
            resolve_ticket_in(Some(&id("JIRA-3")), None, &tickets, &deep).unwrap(),
            tickets.join("acme/JIRA-3")
        );
        let error = resolve_ticket_in(Some(&id("JIRA-1")), None, &tickets, &deep)
            .unwrap_err()
            .to_string();
        assert!(error.contains("inside project 'acme'"), "{error}");
    }

    /// Discovery from inside a project ticket finds that ticket; `--project`
    /// then checks membership instead of choosing.
    #[test]
    fn test_discovery_inside_project_ticket_with_assertion() {
        let (_dir, tickets) = fixture();
        let deep = tickets.join("acme/JIRA-2/api");
        std::fs::create_dir_all(&deep).unwrap();

        let ticket = tickets.join("acme/JIRA-2");
        assert_eq!(
            resolve_ticket_in(None, None, &tickets, &deep).unwrap(),
            ticket
        );
        assert_eq!(
            resolve_ticket_in(None, Some("acme"), &tickets, &deep).unwrap(),
            ticket
        );
        assert!(resolve_ticket_in(None, Some("other"), &tickets, &deep).is_err());
    }

    /// With a path, `--project` asserts membership: a loose ticket is in no
    /// project.
    #[test]
    fn test_path_form_with_project_asserts() {
        let (dir, tickets) = fixture();
        let loose = TicketRef::Path(tickets.join("JIRA-1"));
        assert!(resolve_ticket_in(Some(&loose), None, &tickets, dir.path()).is_ok());
        let error = resolve_ticket_in(Some(&loose), Some("acme"), &tickets, dir.path())
            .unwrap_err()
            .to_string();
        assert!(error.contains("loose ticket"), "{error}");
    }
}

/// Create, inspect, and manage ticket workspaces
#[derive(Args)]
pub struct TicketArgs {
    #[command(subcommand)]
    pub command: TicketCommands,
}

#[derive(Args, Debug)]
pub struct TicketSharedArgs {
    /// The ticket to act on: a bare id resolved under the tickets
    /// directory, or a path (anything containing a separator, or `.`/`..`).
    /// Discovered by walking up from the current directory when omitted
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    pub ticket: Option<TicketRef>,

    /// The project a bare ticket id resolves in. Inferred from the current
    /// directory when omitted; with a path or a discovered ticket, asserts
    /// the ticket belongs to it
    #[arg(short, long, value_name = "NAME")]
    pub project: Option<String>,
}

#[derive(Subcommand)]
pub enum TicketCommands {
    Add(add::Args),
    Destroy(destroy::Args),
    Info(info::Args),
    List(list::Args),
    Remove(remove::Args),
    Setup(setup::Args),
}
