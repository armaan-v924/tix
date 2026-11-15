//! Output for ticket mutations and their previews.

use crate::tix::diagnostic::Diagnostic;
use crate::tix::plan::Plan;
use crate::tix::ticket::WorktreeStatus;
use crate::tix::utils::OutputType;
use serde::Serialize;
use std::path::Path;
use tix_sdk::{BaseFetch, BaseResolution, SdkError, WorktreeConfig};

pub(crate) struct TicketOutput {
    result: Mutation,
    pub(crate) plan: Option<Plan>,
    changed: bool,
}

impl TicketOutput {
    pub(crate) fn new(command: &'static str, dry_run: bool) -> Self {
        Self {
            result: Mutation {
                command,
                outcome: Outcome::Succeeded,
                ticket_path: None,
                worktrees: Vec::new(),
                error: None,
            },
            plan: dry_run.then(|| Plan::new(command)),
            changed: false,
        }
    }

    pub(crate) fn ticket(&mut self, path: &Path) -> Result<(), SdkError> {
        let path = std::path::absolute(path)?.display().to_string();
        if let Some(plan) = &mut self.plan {
            plan.set_ticket_path(path.clone());
        }
        self.result.ticket_path = Some(path);
        Ok(())
    }

    pub(crate) fn changed(&mut self) {
        self.changed = true;
    }

    pub(crate) fn declined(&mut self) {
        self.result.outcome = Outcome::Declined;
    }

    pub(crate) fn created(
        &mut self,
        name: &str,
        path: &Path,
        entry: &WorktreeConfig,
        resolution: Option<BaseResolution>,
    ) {
        self.record_worktree(name, path, entry, "created", resolution);
    }

    pub(crate) fn adopted(&mut self, name: &str, path: &Path, entry: &WorktreeConfig) {
        self.record_worktree(name, path, entry, "adopted", None);
    }

    fn record_worktree(
        &mut self,
        name: &str,
        path: &Path,
        entry: &WorktreeConfig,
        outcome: &'static str,
        resolution: Option<BaseResolution>,
    ) {
        self.changed();
        self.result.worktrees.push(WorktreeResult {
            name: name.into(),
            repo: entry.repo.clone(),
            path: display_path(path),
            outcome,
            branch: Some(entry.branch.clone()),
            base: entry.base.clone(),
            base_commit: resolution.as_ref().map(|base| base.commit.clone()),
            base_fetch: resolution.map(|base| base.fetch),
            error: None,
        });
    }

    pub(crate) fn pruned(&mut self, name: &str, path: &Path, repo: &str, status: WorktreeStatus) {
        self.changed();
        self.result.worktrees.push(WorktreeResult {
            name: name.into(),
            repo: repo.into(),
            path: display_path(path),
            outcome: match status {
                WorktreeStatus::Ok => "removed",
                WorktreeStatus::MissingDirectory | WorktreeStatus::NotAGitRepository => "dropped",
            },
            branch: None,
            base: None,
            base_commit: None,
            base_fetch: None,
            error: None,
        });
    }

    pub(crate) fn set_error(&mut self, error: &SdkError) {
        self.result.error = Some(error.into());
    }

    pub(crate) fn worktree_failed(
        &mut self,
        name: &str,
        path: &Path,
        repo: &str,
        error: &SdkError,
    ) {
        self.result.worktrees.push(WorktreeResult {
            name: name.into(),
            repo: repo.into(),
            path: display_path(path),
            outcome: "failed",
            branch: None,
            base: None,
            base_commit: None,
            base_fetch: None,
            error: Some(error.into()),
        });
    }

    pub(crate) fn finish(
        mut self,
        output: OutputType,
        result: Result<(), SdkError>,
    ) -> Result<(), SdkError> {
        if let Some(plan) = self.plan {
            return plan.finish(output, result);
        }
        if let OutputType::Json = output {
            if self.result.error.is_none() {
                self.result.error = result.as_ref().err().map(Diagnostic::from);
            }
            if result.is_err()
                || self
                    .result
                    .worktrees
                    .iter()
                    .any(|entry| entry.error.is_some())
            {
                self.result.outcome = if self.changed {
                    Outcome::Partial
                } else {
                    Outcome::Failed
                };
            }
            let json = serde_json::to_string_pretty(&self.result)
                .map_err(|err| SdkError::Message(format!("json conversion failed: {err}")))?;
            println!("{json}");
        }
        result
    }
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Succeeded,
    Partial,
    Declined,
    Failed,
}

#[derive(Serialize)]
struct Mutation {
    command: &'static str,
    outcome: Outcome,
    ticket_path: Option<String>,
    worktrees: Vec<WorktreeResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Diagnostic>,
}

#[derive(Serialize)]
struct WorktreeResult {
    name: String,
    repo: String,
    path: String,
    outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_fetch: Option<BaseFetch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Diagnostic>,
}

// A relative worktree path has already been resolved successfully when
// recording its ticket. Keep a display path even if cwd disappears later.
fn display_path(path: &Path) -> String {
    std::path::absolute(path)
        .as_deref()
        .unwrap_or(path)
        .display()
        .to_string()
}
