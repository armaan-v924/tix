use std::fmt;
use std::path::PathBuf;

/// An error that can occur during tix-engine operations.
///
/// Non-exhaustive: new failure modes get their own variants, so a `match`
/// outside this crate needs a wildcard arm.
#[non_exhaustive]
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
    /// A branch that already exists does not contain the base it was asked
    /// to be cut from, so reusing it under that base would be a lie and
    /// moving it would be destructive.
    BranchLacksBase {
        /// The existing branch.
        branch: String,
        /// The base it does not contain, as resolved (e.g. `origin/main`).
        base: String,
    },
    /// origin's default branch is known, but its remote-tracking ref is not
    /// present locally — origin lacks it, or fetching it failed.
    DefaultBranchNotFetched {
        /// The default branch's name on origin.
        branch: String,
    },
    /// A bare branch name names a local branch and an origin branch that
    /// have diverged, so neither is the obvious base.
    AmbiguousBase {
        /// The base as given.
        spec: String,
    },
    /// origin's default branch is neither cached nor obtainable from origin.
    UnknownDefaultBranch {
        /// The repository alias.
        alias: String,
        /// Why origin could not be asked.
        reason: String,
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
            TixError::BranchLacksBase { branch, base } => write!(
                f,
                "branch '{}' already exists and does not contain '{}'",
                branch, base
            ),
            TixError::DefaultBranchNotFetched { branch } => write!(
                f,
                "origin's default branch is '{0}', but refs/remotes/origin/{0} does not exist",
                branch
            ),
            TixError::AmbiguousBase { spec } => write!(
                f,
                "'{0}' has diverged from 'origin/{0}', so either could be the base",
                spec
            ),
            TixError::UnknownDefaultBranch { alias, reason } => write!(
                f,
                "cannot determine the default branch of '{}' ({})",
                alias, reason
            ),
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
            TixError::BranchLacksBase {
                branch: "feature".into(),
                base: "origin/main".into(),
            },
            TixError::DefaultBranchNotFetched {
                branch: "main".into(),
            },
            TixError::AmbiguousBase { spec: "dev".into() },
            TixError::UnknownDefaultBranch {
                alias: "api".into(),
                reason: "remote 'origin' does not exist".into(),
            },
            TixError::Message("something went wrong".into()),
        ];

        for err in variants {
            let message = err.to_string();
            assert!(!message.is_empty(), "Display was empty for: {:?}", err);
            assert!(gives_advice(&message).is_none(), "{message}");
        }
    }

    /// Words that only show up when a message tells the reader what to do.
    /// The engine has no UI: wording a way past an error (a flag, an
    /// argument, a command) is each frontend's job.
    const ADVICE: &[&str] = &["force", "pass", "pick", "reuse", "fetch", "run", "try"];

    /// The advice word `message` contains, if any. Whole words only, so a
    /// fact like "bypassed" or "fetched" is not flagged.
    fn gives_advice(message: &str) -> Option<&'static str> {
        message
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find_map(|word| {
                ADVICE
                    .iter()
                    .copied()
                    .find(|advice| word.eq_ignore_ascii_case(advice))
            })
            .or_else(|| message.contains("--").then_some("--"))
    }

    /// The advice check flags advice, and leaves facts alone.
    #[test]
    fn test_gives_advice() {
        assert_eq!(gives_advice("pass --force to discard them"), Some("pass"));
        assert_eq!(gives_advice("reuse it without a base"), Some("reuse"));
        assert_eq!(gives_advice("fetch it first"), Some("fetch"));
        assert_eq!(gives_advice("see --base"), Some("--"));
        assert_eq!(gives_advice("it was fetched, then bypassed"), None);
        assert_eq!(
            gives_advice("base 'x' does not resolve to a commit in 'api'"),
            None
        );
    }

    /// Each new variant's `Display` names its facts.
    #[test]
    fn test_base_variants_display_their_facts() {
        assert_eq!(
            TixError::BranchLacksBase {
                branch: "feature".into(),
                base: "origin/main".into(),
            }
            .to_string(),
            "branch 'feature' already exists and does not contain 'origin/main'"
        );
        assert_eq!(
            TixError::DefaultBranchNotFetched {
                branch: "trunk".into()
            }
            .to_string(),
            "origin's default branch is 'trunk', but refs/remotes/origin/trunk does not exist"
        );
        assert_eq!(
            TixError::AmbiguousBase { spec: "dev".into() }.to_string(),
            "'dev' has diverged from 'origin/dev', so either could be the base"
        );
        assert_eq!(
            TixError::UnknownDefaultBranch {
                alias: "api".into(),
                reason: "offline".into(),
            }
            .to_string(),
            "cannot determine the default branch of 'api' (offline)"
        );
    }
}
