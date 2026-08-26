//! Ticket discovery: which ticket does a command operate on?
//!
//! Owned by `tix-cli` and written here first; promoted to `tix-sdk` (#96) so
//! plugins resolve tickets identically to the canonical frontend. Explicitly
//! *not* an engine concern — `tix-engine` takes resolved paths as given
//! (`design/spec.md` §4).
//!
//! Two paths in:
//!
//! - **Discovery** ([`discover_ticket_root`]): walk upward from the logical
//!   cwd testing for the *file* `.tix/ticket.toml`; nearest ancestor wins.
//! - **Override** ([`resolve_override`]): a `--ticket <path | id>` argument,
//!   disambiguated by shape ([`TicketRef`]). Both forms are assertions, not
//!   starting points — a near-miss (e.g. a subdirectory of a ticket) errors
//!   rather than re-walking.

use crate::error::SdkError;
use std::path::{Path, PathBuf};
use tix_engine::TixError;
use tracing::debug;

/// A `--ticket` argument: one flag, two forms, disambiguated by shape
/// (`design/spec.md` §4).
///
/// - **Path** — the argument contains a path separator, is absolute, or is
///   `.`/`..`. Asserts *this path is the ticket root*.
/// - **Id** — any bare name, resolved against the configured tickets
///   directory.
///
/// A bare name is always an id; a ticket directory in cwd must be written
/// `./NAME`. See [`resolve_override`] for the resolution semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketRef {
    /// An asserted ticket-root path (`./NAME`, `some/dir`, `/abs/path`, `.`, `..`).
    Path(PathBuf),
    /// A bare ticket id, resolved as `tickets_directory.join(id)`.
    Id(String),
}

impl std::str::FromStr for TicketRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = Path::new(s);
        let is_path_shape = s == "." || s == ".." || path.is_absolute() || s.contains(['/', '\\']);
        if is_path_shape {
            Ok(TicketRef::Path(path.to_path_buf()))
        } else {
            Ok(TicketRef::Id(s.to_string()))
        }
    }
}

/// Returns true if `path` is a ticket root: a directory containing the file
/// `.tix/ticket.toml`.
///
/// The predicate is the **file**, not the bare `.tix/` directory — projects
/// live above tickets with their own `.tix/`, so testing for the directory
/// would halt an upward walk at a project and misreport it as a ticket
/// (`design/spec.md` §4).
pub fn is_ticket_root(path: &Path) -> bool {
    path.join(".tix").join("ticket.toml").is_file()
}

/// The logical current working directory — `$PWD` as the shell reports it.
///
/// Ticket directories contain worktrees and people symlink into them, so
/// logical vs. physical paths can resolve *different tickets* from the same
/// `cd`. Discovery MUST NOT canonicalize (spec §4): we walk what the user
/// sees in their prompt, matching git's behavior. `$PWD` is trusted only when
/// it is absolute and still names the same directory as the process cwd;
/// otherwise the process cwd is the fallback.
pub fn logical_cwd() -> Result<PathBuf, SdkError> {
    let process_cwd =
        std::env::current_dir().map_err(|e| SdkError::Engine(TixError::IoError(e)))?;
    if let Some(pwd) = std::env::var_os("PWD") {
        let pwd = PathBuf::from(pwd);
        if pwd.is_absolute() && same_directory(&pwd, &process_cwd) {
            return Ok(pwd);
        }
    }
    Ok(process_cwd)
}

/// Walks upward from the logical cwd and returns the nearest ticket root.
///
/// Equivalent to [`discover_ticket_root_from`] starting at [`logical_cwd`].
/// Returns `Ok(None)` when no ancestor is a ticket root — commands that
/// require ticket context should turn that into a clear error; there is
/// deliberately **no** fallback to the tickets directory.
pub fn discover_ticket_root() -> Result<Option<PathBuf>, SdkError> {
    Ok(discover_ticket_root_from(&logical_cwd()?))
}

/// Walks upward from `start`, testing each directory for `.tix/ticket.toml`.
///
/// - Nearest ancestor wins.
/// - Ceiling is the filesystem root, stopping early at a device boundary
///   (matching git's `GIT_DISCOVERY_ACROSS_FILESYSTEM` default).
/// - The walk is **not** bounded by the configured tickets directory — that
///   coupling was rejected because a moved or symlinked ticket would fail
///   with no visible cause (spec §4).
pub fn discover_ticket_root_from(start: &Path) -> Option<PathBuf> {
    let start_device = device_of(start);
    let mut current = start;
    loop {
        if is_ticket_root(current) {
            debug!(path = %current.display(), "ticket root found");
            return Some(current.to_path_buf());
        }
        let parent = current.parent()?;
        // Stop early at a device boundary: only when both devices are known
        // and differ. An unreadable directory does not end the walk.
        if let (Some(a), Some(b)) = (start_device, device_of(parent))
            && a != b
        {
            debug!(path = %parent.display(), "stopping discovery at device boundary");
            return None;
        }
        current = parent;
    }
}

/// Resolves an explicit `--ticket` override, skipping the discovery walk.
///
/// Disambiguated by shape at parse time (see [`TicketRef`]):
///
/// - **Path form** (`./NAME`, `some/dir`, `/abs/path`, `.`, `..`): asserts
///   *this path is the ticket root*. The cwd is left alone.
/// - **Id form** (any bare name): resolved as `tickets_directory.join(id)`,
///   v2 parity. A ticket directory in cwd must be written `./NAME` — a bare
///   name is always an id.
///
/// Both forms are assertions: a path that is not a ticket root errors rather
/// than being used as a starting point for a new walk.
///
/// As a safety net (not a bound), a resolved ticket outside
/// `tickets_directory` is logged at debug and accepted.
///
/// # Errors
///
/// [`TixError::TicketNotFound`] if the asserted path is not a ticket root.
pub fn resolve_override(ticket: &TicketRef, tickets_directory: &Path) -> Result<PathBuf, SdkError> {
    let (root, described) = match ticket {
        TicketRef::Path(path) => (path.clone(), format!("path '{}'", path.display())),
        TicketRef::Id(id) => (tickets_directory.join(id), format!("ticket id '{id}'")),
    };

    if !is_ticket_root(&root) {
        return Err(SdkError::Engine(TixError::TicketNotFound(format!(
            "{described} is not a ticket root (no .tix/ticket.toml at {})",
            root.display()
        ))));
    }

    if !root.starts_with(tickets_directory) {
        debug!(
            path = %root.display(),
            tickets_directory = %tickets_directory.display(),
            "resolved ticket lies outside the tickets directory"
        );
    }

    Ok(root)
}

/// Resolves the ticket a command operates on: the `--ticket` override when
/// given, the discovery walk otherwise.
///
/// Returns `Ok(None)` only when there is no override and the walk found
/// nothing. Commands that require ticket context should error clearly on
/// `None`; commands that merely prefer it (or create tickets) may proceed.
pub fn resolve_ticket_root(
    ticket: Option<&TicketRef>,
    tickets_directory: &Path,
) -> Result<Option<PathBuf>, SdkError> {
    match ticket {
        Some(ticket_ref) => resolve_override(ticket_ref, tickets_directory).map(Some),
        None => discover_ticket_root(),
    }
}

/// Returns true when two paths name the same directory (same device+inode on
/// Unix; canonical-path equality elsewhere).
#[cfg(unix)]
fn same_directory(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_directory(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// The device id `path` lives on, when it can be determined.
#[cfg(unix)]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
fn device_of(_path: &Path) -> Option<u64> {
    // Windows has no cheap device-id equivalent here; the walk simply runs to
    // the filesystem root.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Creates `.tix/ticket.toml` under `root`, making it a ticket root.
    fn make_ticket_root(root: &Path) {
        std::fs::create_dir_all(root.join(".tix")).unwrap();
        std::fs::write(
            root.join(".tix/ticket.toml"),
            "key = \"JIRA-1\"\ndescription = \"\"\n",
        )
        .unwrap();
    }

    // --- is_ticket_root ---

    /// The predicate is the file `.tix/ticket.toml`, not the `.tix` directory.
    #[test]
    fn test_bare_tix_directory_is_not_a_ticket_root() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".tix")).unwrap();
        assert!(!is_ticket_root(dir.path()));

        std::fs::write(dir.path().join(".tix/ticket.toml"), "").unwrap();
        assert!(is_ticket_root(dir.path()));
    }

    // --- discover_ticket_root_from ---

    /// The walk finds the ticket root from the root itself and from nested
    /// subdirectories.
    #[test]
    fn test_walk_finds_root_from_subdirectory() {
        let dir = tempdir().unwrap();
        let ticket = dir.path().join("tickets/JIRA-1");
        make_ticket_root(&ticket);
        let deep = ticket.join("backend/src/module");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(discover_ticket_root_from(&ticket), Some(ticket.clone()));
        assert_eq!(discover_ticket_root_from(&deep), Some(ticket));
    }

    /// The nearest ancestor wins when tickets nest.
    #[test]
    fn test_nearest_ancestor_wins() {
        let dir = tempdir().unwrap();
        let outer = dir.path().join("outer");
        let inner = outer.join("inner");
        make_ticket_root(&outer);
        make_ticket_root(&inner);
        let deep = inner.join("some/dir");
        std::fs::create_dir_all(&deep).unwrap();

        assert_eq!(discover_ticket_root_from(&deep), Some(inner));
    }

    /// A walk with no ticket root above returns None rather than falling back
    /// anywhere.
    #[test]
    fn test_walk_finds_nothing() {
        let dir = tempdir().unwrap();
        let deep = dir.path().join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(discover_ticket_root_from(&deep), None);
    }

    /// A project directory (bare `.tix/`, no ticket.toml) above a ticket does
    /// not halt the walk, and is not itself reported as a ticket.
    #[test]
    fn test_project_directory_does_not_capture_walk() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(project.join(".tix")).unwrap();
        std::fs::write(project.join(".tix/project.toml"), "").unwrap();

        let ticket = project.join("JIRA-1");
        make_ticket_root(&ticket);
        let inside_ticket = ticket.join("src");
        std::fs::create_dir_all(&inside_ticket).unwrap();

        // From inside the ticket: the ticket wins.
        assert_eq!(discover_ticket_root_from(&inside_ticket), Some(ticket));
        // From the project level (above the ticket): nothing — the project's
        // bare .tix/ is not a ticket root.
        assert_eq!(discover_ticket_root_from(&project), None);
    }

    // --- resolve_override ---

    /// The path form asserts the given path is a ticket root and returns it.
    #[test]
    fn test_override_path_form_valid() {
        let dir = tempdir().unwrap();
        let ticket = dir.path().join("JIRA-1");
        make_ticket_root(&ticket);

        let resolved = resolve_override(&TicketRef::Path(ticket.clone()), dir.path()).unwrap();
        assert_eq!(resolved, ticket);
    }

    /// A near-miss — a subdirectory of a ticket — errors rather than
    /// re-walking: the override is an assertion, not a starting point.
    #[test]
    fn test_override_path_form_near_miss_errors() {
        let dir = tempdir().unwrap();
        let ticket = dir.path().join("JIRA-1");
        make_ticket_root(&ticket);
        let subdir = ticket.join("backend");
        std::fs::create_dir_all(&subdir).unwrap();

        assert!(matches!(
            resolve_override(&TicketRef::Path(subdir), dir.path()),
            Err(SdkError::Engine(TixError::TicketNotFound(_)))
        ));
    }

    /// The id form resolves against the tickets directory.
    #[test]
    fn test_override_id_form_valid() {
        let dir = tempdir().unwrap();
        let ticket = dir.path().join("JIRA-1");
        make_ticket_root(&ticket);

        let resolved = resolve_override(&TicketRef::Id("JIRA-1".to_string()), dir.path()).unwrap();
        assert_eq!(resolved, ticket);
    }

    /// An id that names no ticket under the tickets directory errors.
    #[test]
    fn test_override_id_form_missing_errors() {
        let dir = tempdir().unwrap();
        assert!(matches!(
            resolve_override(&TicketRef::Id("NOPE-404".to_string()), dir.path()),
            Err(SdkError::Engine(TixError::TicketNotFound(_)))
        ));
    }

    /// A valid ticket outside the tickets directory is accepted — the debug
    /// log is a safety net, not a bound.
    #[test]
    fn test_override_outside_tickets_directory_accepted() {
        let dir = tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere/JIRA-1");
        make_ticket_root(&elsewhere);
        let tickets_directory = dir.path().join("tickets");
        std::fs::create_dir_all(&tickets_directory).unwrap();

        let resolved =
            resolve_override(&TicketRef::Path(elsewhere.clone()), &tickets_directory).unwrap();
        assert_eq!(resolved, elsewhere);
    }

    // --- resolve_ticket_root ---

    /// With an override present, discovery is skipped entirely.
    #[test]
    fn test_resolve_prefers_override() {
        let dir = tempdir().unwrap();
        let ticket = dir.path().join("JIRA-1");
        make_ticket_root(&ticket);

        let resolved =
            resolve_ticket_root(Some(&TicketRef::Path(ticket.clone())), dir.path()).unwrap();
        assert_eq!(resolved, Some(ticket));
    }
}
