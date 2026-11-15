use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::TixError;

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
    /// The base resolved at creation, used to cut the branch or check the
    /// ancestry of an existing branch. It can be a ref such as `origin/main`
    /// or a literal commit SHA.
    ///
    /// A named base preserves the intended target for a stacked pull request.
    /// This field is absent when no base was resolved or the entry predates it.
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

/// Live Git state in a local checkout.
#[derive(Debug, Serialize)]
pub struct CheckoutStatus {
    /// The current local branch, absent for a detached HEAD.
    pub branch: Option<String>,
    /// Whether HEAD is detached.
    pub detached: bool,
    /// The full object ID of HEAD.
    pub head: String,
    /// Paths with changes staged in the index.
    pub staged: usize,
    /// Paths with unstaged tracked changes, including unresolved conflicts.
    pub unstaged: usize,
    /// Untracked files, excluding ignored files.
    pub untracked: usize,
    /// Tracking information, when the upstream ref is available locally.
    pub upstream: Option<UpstreamStatus>,
}

/// The relationship between HEAD and the locally known upstream ref.
#[derive(Debug, Serialize)]
pub struct UpstreamStatus {
    /// The full upstream ref name.
    pub reference: String,
    /// Commits on HEAD that the upstream does not contain.
    pub ahead: usize,
    /// Commits on the upstream that HEAD does not contain.
    pub behind: usize,
}

/// Inspects the given checkout using only local Git state.
///
/// # Errors
///
/// Returns a Git error when the checkout, HEAD, index, or upstream cannot
/// be inspected. An unconfigured or missing upstream is a valid result.
pub fn inspect_checkout(path: &Path) -> Result<CheckoutStatus, TixError> {
    let repo = git2::Repository::open(path)?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?.id();
    let detached = repo.head_detached()?;
    let (branch, upstream) = if detached {
        (None, None)
    } else {
        let branch = git2::Branch::wrap(head);
        let name = branch.name()?.map(str::to_owned);
        let upstream = match branch.upstream() {
            Ok(upstream) => {
                let reference = upstream.get();
                let (ahead, behind) =
                    repo.graph_ahead_behind(commit, reference.peel_to_commit()?.id())?;
                Some(UpstreamStatus {
                    reference: String::from_utf8_lossy(reference.name_bytes()).into_owned(),
                    ahead,
                    behind,
                })
            }
            Err(e) if e.code() == git2::ErrorCode::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        (name, upstream)
    };

    let mut status = CheckoutStatus {
        branch,
        detached,
        head: commit.to_string(),
        staged: 0,
        unstaged: 0,
        untracked: 0,
        upstream,
    };
    use git2::Status;
    let staged = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    let unstaged = Status::WT_MODIFIED
        | Status::WT_DELETED
        | Status::WT_RENAMED
        | Status::WT_TYPECHANGE
        | Status::WT_UNREADABLE
        | Status::CONFLICTED;
    for entry in crate::utils::worktree_statuses(&repo)?.iter() {
        let changes = entry.status();
        status.staged += usize::from(changes.intersects(staged));
        status.unstaged += usize::from(changes.intersects(unstaged));
        status.untracked += usize::from(changes.contains(Status::WT_NEW));
    }
    Ok(status)
}
