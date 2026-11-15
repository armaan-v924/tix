//! Machine-readable CLI errors derived from the SDK and engine error types.

use serde::Serialize;
use tix_sdk::{SdkError, TixError};

#[derive(Debug, Serialize)]
pub(crate) struct Diagnostic {
    code: &'static str,
    message: String,
}

impl From<&SdkError> for Diagnostic {
    fn from(error: &SdkError) -> Self {
        let code = match error {
            SdkError::ConfigNotFound(_) => "config-not-found",
            SdkError::Parse(_) => "invalid-document",
            SdkError::Serialization(_) => "serialization-error",
            SdkError::PluginImplementation(_) => "plugin-error",
            SdkError::Engine(engine) => match engine {
                TixError::GitError(_) => "git-error",
                TixError::IoError(_) => "io-error",
                TixError::RepoNotFound(_) => "repo-not-found",
                TixError::TicketNotFound(_) => "ticket-not-found",
                TixError::WorktreeNotFound(_) => "worktree-not-found",
                TixError::Dirty { .. } => "dirty-worktree",
                TixError::InvalidWorktree { .. } => "invalid-worktree",
                TixError::Diverged { .. } => "diverged-branch",
                TixError::BranchLacksBase { .. } => "branch-lacks-base",
                TixError::DefaultBranchNotFetched { .. } => "default-branch-not-fetched",
                TixError::AmbiguousBase { .. } => "ambiguous-base",
                TixError::UnknownDefaultBranch { .. } => "unknown-default-branch",
                _ => "operation-failed",
            },
            SdkError::Message(_) => "operation-failed",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}
