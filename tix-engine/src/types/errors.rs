use std::fmt;
use std::path::PathBuf;

/// An error that can occur during tix-engine operations.
pub enum TixError {
    /// A git2 operation failed.
    GitError(git2::Error),
    /// An I/O operation failed.
    IoError(std::io::Error),
    /// The repository was not found at the given path.
    RepoNotFound(String),
    /// The ticket directory was not found at the given path.
    TicketNotFound(String),
    /// A worktree recorded in the ticket document was not found on disk.
    WorktreeNotFound(String),
    /// A working tree has uncommitted changes (modified, staged, or
    /// untracked files) that the operation would discard.
    ///
    /// Raised by operations that take a `force` argument when it is unset;
    /// how to get past it is the caller's to explain, not the engine's.
    Dirty {
        /// The working tree with the changes.
        path: PathBuf,
        /// How many entries `git status` would report.
        changes: usize,
    },
    /// git's validation of a registered worktree failed, so it cannot be
    /// checked for uncommitted changes — typically its directory is gone.
    InvalidWorktree {
        /// The worktree's recorded path.
        path: PathBuf,
        /// What validation reported.
        reason: String,
    },
    /// A local branch has diverged from its counterpart on origin, so it
    /// cannot be fast-forwarded.
    Diverged {
        /// The branch name.
        branch: String,
    },
    /// A freeform error message.
    Message(String),
}

impl fmt::Display for TixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TixError::GitError(e) => write!(f, "git error: {}", e),
            TixError::IoError(e) => write!(f, "io error: {}", e),
            TixError::RepoNotFound(s) => write!(f, "repo not found: {}", s),
            TixError::TicketNotFound(s) => write!(f, "ticket not found: {}", s),
            TixError::WorktreeNotFound(s) => write!(f, "worktree not found: {}", s),
            TixError::Dirty { path, changes } => write!(
                f,
                "{} has {} uncommitted {}",
                path.display(),
                changes,
                if *changes == 1 { "change" } else { "changes" }
            ),
            TixError::InvalidWorktree { path, reason } => {
                write!(f, "worktree at {} is not valid: {}", path.display(), reason)
            }
            TixError::Diverged { branch } => {
                write!(f, "'{}' has diverged from origin", branch)
            }
            TixError::Message(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Debug for TixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for TixError {}

impl From<std::io::Error> for TixError {
    fn from(e: std::io::Error) -> Self {
        TixError::IoError(e)
    }
}

impl From<git2::Error> for TixError {
    fn from(e: git2::Error) -> Self {
        TixError::GitError(e)
    }
}

impl From<&str> for TixError {
    fn from(s: &str) -> Self {
        TixError::Message(s.to_string())
    }
}

impl From<String> for TixError {
    fn from(s: String) -> Self {
        TixError::Message(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `From<io::Error>` wraps into `IoError`.
    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        assert!(matches!(TixError::from(io_err), TixError::IoError(_)));
    }

    /// `From<&str>` wraps into `Message`.
    #[test]
    fn test_from_str() {
        assert!(matches!(TixError::from("oops"), TixError::Message(_)));
    }

    /// `From<String>` wraps into `Message`.
    #[test]
    fn test_from_string() {
        assert!(matches!(
            TixError::from("oops".to_string()),
            TixError::Message(_)
        ));
    }

    /// `Display` produces a non-empty string for every variant.
    #[test]
    fn test_display_non_empty() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let git_err = git2::Repository::open("/nonexistent/path/for/tix/test")
            .err()
            .unwrap();
        let variants: Vec<TixError> = vec![
            TixError::GitError(git_err),
            TixError::IoError(io_err),
            TixError::RepoNotFound("path".into()),
            TixError::TicketNotFound("path".into()),
            TixError::WorktreeNotFound("path".into()),
            TixError::Dirty {
                path: "path".into(),
                changes: 1,
            },
            TixError::InvalidWorktree {
                path: "path".into(),
                reason: "gone".into(),
            },
            TixError::Diverged {
                branch: "main".into(),
            },
            TixError::Message("something went wrong".into()),
        ];

        for err in variants {
            let message = err.to_string();
            assert!(!message.is_empty(), "Display was empty for: {:?}", err);
            // The engine has no UI: wording a way past an error (a flag, an
            // argument) is each frontend's job.
            assert!(!message.contains("force"), "{message}");
        }
    }
}
