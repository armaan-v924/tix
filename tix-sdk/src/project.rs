//! Projects: named namespaces one level above tickets.
//!
//! A project is a directory under the tickets directory that groups tickets:
//! loose tickets live at `<tickets_directory>/<KEY>/`, project tickets at
//! `<tickets_directory>/<project>/<KEY>/`. What makes a directory a project
//! is the **file** `.tix/project.toml`, created empty
//! ([projects](https://tix.armaanv.dev/latest/concepts/projects/)).
//!
//! The marker is an assertion, not metadata. It is what lets the discovery
//! walk ([`crate::discovery::discover_from`]) recognise a project without
//! reading config, and it is what the listing uses too, so there is one
//! definition of "project" rather than two that can disagree — a stray
//! directory under the tickets directory is not a project. It is empty on
//! purpose: adding fields to a document that exists is additive, adding a
//! file where none existed is not, so the file is settled now and its
//! contents wait for a reason to exist.
//!
//! Projects are created implicitly on first use ([`Project::ensure`]);
//! there is no separate "create a project" step.

use crate::discovery::{discover_from, is_ticket_root, same_directory};
use crate::error::SdkError;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Returns true if `path` is a project root: a directory containing the file
/// `.tix/project.toml`.
///
/// The predicate is the file, not the `.tix/` directory, for the same reason
/// [`is_ticket_root`] tests its own file: tickets have a `.tix/` too.
pub fn is_project_root(path: &Path) -> bool {
    marker_path(path).is_file()
}

/// The marker file of the project rooted at `root`: `<root>/.tix/project.toml`.
pub fn marker_path(root: &Path) -> PathBuf {
    root.join(".tix").join("project.toml")
}

/// A project: its name, and the directory its tickets live in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The project name — the directory name.
    pub name: String,
    /// The project directory. Its tickets are its immediate children.
    pub root: PathBuf,
}

impl Project {
    /// The project called `name` under `tickets_directory`, whether or not
    /// it exists yet.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] when `name` cannot be a directory name: empty,
    /// `.`/`..`, or containing `/` or `\`.
    pub fn named(tickets_directory: &Path, name: &str) -> Result<Self, SdkError> {
        if name.is_empty() {
            return Err(SdkError::Message(
                "the project name is empty — it becomes a directory name".to_string(),
            ));
        }
        if name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(SdkError::Message(format!(
                "'{name}' is not a valid project name — it becomes a directory name, so '/', \
                 '\\', '.' and '..' are rejected"
            )));
        }
        Ok(Self {
            name: name.to_string(),
            root: tickets_directory.join(name),
        })
    }

    /// The project rooted at `root`, named for its directory. `None` when the
    /// path has no final component (e.g. `/`).
    ///
    /// Does not check the marker; see [`is_project_root`].
    pub fn at(root: &Path) -> Option<Self> {
        let name = root.file_name()?.to_string_lossy().into_owned();
        Some(Self {
            name,
            root: root.to_path_buf(),
        })
    }

    /// Whether this project exists: its root carries the marker.
    pub fn exists(&self) -> bool {
        is_project_root(&self.root)
    }

    /// Creates the project if it does not exist yet: the directory, and the
    /// empty marker inside it. A no-op for an existing project.
    ///
    /// A directory of that name that is *not* yet a project — created by
    /// hand, say — is adopted by writing the marker into it.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] when the directory is a ticket root: a loose
    /// ticket already has that name, and a ticket cannot also be a project.
    /// IO errors propagate.
    pub fn ensure(&self) -> Result<(), SdkError> {
        if is_ticket_root(&self.root) {
            return Err(SdkError::Message(format!(
                "cannot use '{}' as a project — {} is already a ticket",
                self.name,
                self.root.display()
            )));
        }
        let marker = marker_path(&self.root);
        if let Some(parent) = marker.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // `create_new` so a concurrent creator never truncates a marker that
        // has since grown contents.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
        {
            Ok(_) => {
                debug!(path = %self.root.display(), "project created");
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Whether the ticket at `ticket_root` belongs to this project — its
    /// nearest enclosing project is this one.
    ///
    /// Compared as directories, not as path strings, so a project reached
    /// through a symlink still matches.
    pub fn contains(&self, ticket_root: &Path) -> bool {
        project_of(ticket_root).is_some_and(|project| same_directory(&project.root, &self.root))
    }
}

/// The project the ticket at `ticket_root` belongs to, if any: the nearest
/// project root above it.
///
/// Walks from the ticket rather than reading config, so a ticket outside the
/// tickets directory still reports its project.
pub fn project_of(ticket_root: &Path) -> Option<Project> {
    discover_from(ticket_root)
        .project
        .as_deref()
        .and_then(Project::at)
}

/// Resolves the project a command is scoped to.
///
/// In order:
///
/// 1. **Flag** — `flag` names a project under `tickets_directory`. It need
///    not exist yet; commands that create tickets call [`Project::ensure`].
/// 2. **Inferred** — the nearest project above `cwd`, found by the discovery
///    walk ([`discover_from`]). Like ticket discovery, this is not bounded by
///    `tickets_directory`.
/// 3. **Loose** — `None`: tickets live directly in `tickets_directory`.
///
/// Pass the **logical** cwd ([`crate::discovery::logical_cwd`]).
///
/// # Errors
///
/// [`SdkError::Message`] when `flag` is not a valid project name.
pub fn resolve_project(
    flag: Option<&str>,
    tickets_directory: &Path,
    cwd: &Path,
) -> Result<Option<Project>, SdkError> {
    if let Some(name) = flag {
        return Project::named(tickets_directory, name).map(Some);
    }
    Ok(discover_from(cwd).project.as_deref().and_then(Project::at))
}

/// The directory tickets of `project` live in: the project root, or
/// `tickets_directory` itself for loose tickets.
pub fn tickets_home<'a>(project: Option<&'a Project>, tickets_directory: &'a Path) -> &'a Path {
    project.map_or(tickets_directory, |project| &project.root)
}

/// Every project directly under `tickets_directory`, sorted by name.
///
/// A project is a child directory carrying the marker; anything else — a
/// loose ticket, a stray directory, a file — is not one. A missing tickets
/// directory means no projects.
///
/// # Errors
///
/// IO errors other than the directory not existing.
pub fn list_projects(tickets_directory: &Path) -> Result<Vec<Project>, SdkError> {
    let entries = match std::fs::read_dir(tickets_directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut projects: Vec<Project> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_project_root(path) && !is_ticket_root(path))
        .filter_map(|path| Project::at(&path))
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_ticket_root(root: &Path) {
        std::fs::create_dir_all(root.join(".tix")).unwrap();
        std::fs::write(root.join(".tix/ticket.toml"), "[ticket]\nkey = \"K\"\n").unwrap();
    }

    // --- names ---

    /// A project name becomes a directory name, so it is held to the same
    /// rules as a ticket key.
    #[test]
    fn test_invalid_names_rejected() {
        let tickets = Path::new("/t");
        for name in ["", ".", "..", "a/b", "a\\b"] {
            assert!(Project::named(tickets, name).is_err(), "{name:?}");
        }
        let project = Project::named(tickets, "acme").unwrap();
        assert_eq!(project.root, Path::new("/t/acme"));
    }

    // --- ensure ---

    /// First use creates the directory and the empty marker; a second use is
    /// a no-op that leaves the marker alone.
    #[test]
    fn test_ensure_creates_marker_once() {
        let dir = tempdir().unwrap();
        let project = Project::named(&dir.path().join("tickets"), "acme").unwrap();
        assert!(!project.exists());

        project.ensure().unwrap();
        assert!(project.exists());
        let marker = marker_path(&project.root);
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "");

        std::fs::write(&marker, "# hand-written\n").unwrap();
        project.ensure().unwrap();
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            "# hand-written\n"
        );
    }

    /// A loose ticket's name cannot be reused as a project.
    #[test]
    fn test_ensure_refuses_a_ticket() {
        let dir = tempdir().unwrap();
        make_ticket_root(&dir.path().join("JIRA-1"));
        let project = Project::named(dir.path(), "JIRA-1").unwrap();
        assert!(project.ensure().is_err());
        assert!(!project.exists());
    }

    // --- resolve_project ---

    /// The flag wins over inference, and need not name an existing project.
    #[test]
    fn test_flag_wins() {
        let dir = tempdir().unwrap();
        let tickets = dir.path().join("tickets");
        let inferred = Project::named(&tickets, "inferred").unwrap();
        inferred.ensure().unwrap();

        let resolved = resolve_project(Some("flagged"), &tickets, &inferred.root).unwrap();
        assert_eq!(resolved, Some(Project::named(&tickets, "flagged").unwrap()));
    }

    /// Without a flag, the project is inferred from anywhere inside it —
    /// including deep inside one of its tickets.
    #[test]
    fn test_inferred_from_inside_a_project_ticket() {
        let dir = tempdir().unwrap();
        let tickets = dir.path().join("tickets");
        let project = Project::named(&tickets, "acme").unwrap();
        project.ensure().unwrap();
        let ticket = project.root.join("JIRA-1");
        make_ticket_root(&ticket);
        let deep = ticket.join("api/src");
        std::fs::create_dir_all(&deep).unwrap();

        for cwd in [&project.root, &ticket, &deep] {
            assert_eq!(
                resolve_project(None, &tickets, cwd).unwrap(),
                Some(project.clone())
            );
        }
    }

    /// Outside any project — in a loose ticket, or nowhere at all — the
    /// scope is loose.
    #[test]
    fn test_loose_when_nothing_found() {
        let dir = tempdir().unwrap();
        let tickets = dir.path().join("tickets");
        let ticket = tickets.join("JIRA-1");
        make_ticket_root(&ticket);

        assert_eq!(resolve_project(None, &tickets, &ticket).unwrap(), None);
        assert_eq!(resolve_project(None, &tickets, dir.path()).unwrap(), None);
    }

    /// A directory under the tickets directory without the marker is not a
    /// project, so it is not inferred either.
    #[test]
    fn test_stray_directory_is_not_inferred() {
        let dir = tempdir().unwrap();
        let tickets = dir.path().join("tickets");
        let stray = tickets.join("stray");
        let ticket = stray.join("JIRA-1");
        make_ticket_root(&ticket);

        assert_eq!(resolve_project(None, &tickets, &ticket).unwrap(), None);
    }

    // --- membership ---

    /// A ticket belongs to the project directly above it, and to no other.
    #[test]
    fn test_contains_and_project_of() {
        let dir = tempdir().unwrap();
        let acme = Project::named(dir.path(), "acme").unwrap();
        let other = Project::named(dir.path(), "other").unwrap();
        acme.ensure().unwrap();
        other.ensure().unwrap();
        let ticket = acme.root.join("JIRA-1");
        make_ticket_root(&ticket);
        let loose = dir.path().join("JIRA-2");
        make_ticket_root(&loose);

        assert_eq!(project_of(&ticket), Some(acme.clone()));
        assert!(acme.contains(&ticket));
        assert!(!other.contains(&ticket));
        assert_eq!(project_of(&loose), None);
        assert!(!acme.contains(&loose));
    }

    // --- list_projects ---

    /// Only marked directories are projects: loose tickets, stray
    /// directories and files are not.
    #[test]
    fn test_list_projects_by_marker() {
        let dir = tempdir().unwrap();
        let tickets = dir.path();
        Project::named(tickets, "zeta").unwrap().ensure().unwrap();
        Project::named(tickets, "acme").unwrap().ensure().unwrap();
        make_ticket_root(&tickets.join("JIRA-1"));
        std::fs::create_dir_all(tickets.join("stray/JIRA-2")).unwrap();
        make_ticket_root(&tickets.join("stray/JIRA-2"));
        std::fs::write(tickets.join("notes.txt"), "").unwrap();

        let names: Vec<String> = list_projects(tickets)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["acme", "zeta"]);
    }

    /// No tickets directory yet means no projects, not an error.
    #[test]
    fn test_list_projects_missing_directory() {
        let dir = tempdir().unwrap();
        assert!(list_projects(&dir.path().join("nope")).unwrap().is_empty());
    }

    /// `tickets_home` is the project root, or the tickets directory itself.
    #[test]
    fn test_tickets_home() {
        let tickets = Path::new("/t");
        let project = Project::named(tickets, "acme").unwrap();
        assert_eq!(tickets_home(Some(&project), tickets), Path::new("/t/acme"));
        assert_eq!(tickets_home(None, tickets), tickets);
    }
}
