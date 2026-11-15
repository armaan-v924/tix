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

/// Lists ignored entries relative to the worktree root. Entire ignored
/// directories are represented by one entry ending in a slash.
///
/// # Errors
///
/// Returns a Git error when the worktree cannot be opened or inspected.
pub fn ignored_worktree_paths(path: &Path) -> Result<Vec<String>, crate::TixError> {
    let repo = git2::Repository::open(path)?;
    let mut options = git2::StatusOptions::new();
    options
        .include_ignored(true)
        .include_untracked(false)
        .no_refresh(true);
    let statuses = repo.statuses(Some(&mut options))?;
    let mut paths: Vec<_> = statuses
        .iter()
        .filter(|entry| entry.status().is_ignored())
        .map(|entry| String::from_utf8_lossy(entry.path_bytes()).into_owned())
        .collect();
    paths.sort();
    Ok(paths)
}

pub(crate) fn worktree_statuses(
    repo: &git2::Repository,
) -> Result<git2::Statuses<'_>, crate::TixError> {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .no_refresh(true);
    Ok(repo.statuses(Some(&mut options))?)
}
