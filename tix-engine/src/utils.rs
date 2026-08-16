//! Small domain probes over resolved paths.

use std::path::Path;

/// Returns true when `path` opens as a git repository (a worktree checkout
/// counts — its `.git` file resolves to the parent repo).
///
/// A probe, not a promise: frontends use this for status displays where a
/// broken worktree should render as a warning rather than abort — the
/// all-or-nothing path is [`TicketConfig::resolve`](crate::TicketConfig::resolve).
///
/// # Examples
///
/// ```
/// # use std::path::Path;
/// assert!(!tix_engine::opens_as_git_repository(Path::new("/definitely/not/a/repo")));
/// ```
pub fn opens_as_git_repository(path: &Path) -> bool {
    git2::Repository::open(path).is_ok()
}
