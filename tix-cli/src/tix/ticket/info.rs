//! `tix ticket info` — display a ticket's metadata and worktree table.

use crate::tix::ticket::{TicketSharedArgs, load_ticket_config, require_ticket_root};
use crate::tix::utils::OutputType;
use std::path::{Path, PathBuf};
use tix_sdk::{SdkError, TicketConfig};

/// Show a ticket's metadata and worktrees
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// Output format
    #[arg(short, long)]
    pub output: Option<OutputType>,
}

/// One row of the worktree table: recorded state plus its on-disk status.
struct WorktreeRow {
    name: String,
    repo: String,
    branch: String,
    path: PathBuf,
    status: WorktreeStatus,
}

/// Whether a recorded worktree actually resolves on disk.
enum WorktreeStatus {
    Ok,
    MissingDirectory,
    NotAGitRepository,
}

impl WorktreeStatus {
    fn of(path: &Path) -> Self {
        if !path.is_dir() {
            WorktreeStatus::MissingDirectory
        } else if !tix_sdk::opens_as_git_repository(path) {
            WorktreeStatus::NotAGitRepository
        } else {
            WorktreeStatus::Ok
        }
    }

    fn marker(&self) -> &'static str {
        match self {
            WorktreeStatus::Ok => "",
            WorktreeStatus::MissingDirectory => "  ⚠ missing on disk",
            WorktreeStatus::NotAGitRepository => "  ⚠ not a git repository",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            WorktreeStatus::Ok => "ok",
            WorktreeStatus::MissingDirectory => "missing",
            WorktreeStatus::NotAGitRepository => "not-a-git-repository",
        }
    }
}

/// Prints key, description, path, and the per-repo worktree table.
///
/// Pure frontend over the recorded `[ticket]` section: each worktree is
/// checked individually so one that fails to resolve renders with a warning
/// marker instead of failing the whole command — which is also why this does
/// not go through the all-or-nothing [`TicketConfig::resolve`].
///
/// There is deliberately no single ticket branch to print: branches are
/// per-worktree.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let root = require_ticket_root(&app.context, args.shared.ticket.as_ref())?;
    let ticket: TicketConfig = load_ticket_config(&root)?;

    // Grouped by repo, then by name within a repo.
    let mut rows: Vec<WorktreeRow> = ticket
        .worktrees
        .iter()
        .map(|(name, entry)| {
            let path = root.join(name);
            WorktreeRow {
                name: name.clone(),
                repo: entry.repo.clone(),
                branch: entry.branch.clone(),
                status: WorktreeStatus::of(&path),
                path,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.name.cmp(&b.name)));

    match app.output {
        OutputType::Default => {
            if ticket.description.is_empty() {
                println!("{}", ticket.key);
            } else {
                println!("{}  {}", ticket.key, ticket.description);
            }
            println!("{}", root.display());
            if rows.is_empty() {
                println!("\nno worktrees");
            } else {
                let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
                let repo_width = rows.iter().map(|r| r.repo.len()).max().unwrap_or(0);
                println!();
                for row in &rows {
                    println!(
                        "{:name_width$}  {:repo_width$}  {}{}",
                        row.name,
                        row.repo,
                        row.branch,
                        row.status.marker()
                    );
                }
            }
        }
        OutputType::Json => {
            let worktrees: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "name": row.name,
                        "repo": row.repo,
                        "branch": row.branch,
                        "path": row.path,
                        "status": row.status.label(),
                    })
                })
                .collect();
            let value = serde_json::json!({
                "key": ticket.key,
                "description": ticket.description,
                "path": root,
                "worktrees": worktrees,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        }
        OutputType::Toml => {
            println!("path = {:?}\n", root.display().to_string());
            print!("{}", toml::to_string(&ticket)?);
        }
    }
    Ok(())
}
