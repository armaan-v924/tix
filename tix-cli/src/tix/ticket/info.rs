//! `tix ticket info` — display a ticket's metadata and worktree table.

use crate::tix::ticket::{
    TicketSharedArgs, WorktreeStatus, load_ticket_config, require_ticket_root,
};
use crate::tix::utils::OutputType;
use serde::Serialize;
use std::path::PathBuf;
use tix_sdk::{CheckoutStatus, SdkError, TicketConfig, inspect_checkout};

/// Show a ticket's metadata and worktrees
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// Include live branch, HEAD, changes, and locally known upstream
    #[arg(long)]
    pub status: bool,

    /// Output format
    #[arg(short, long)]
    pub output: Option<OutputType>,
}

/// One row of the worktree table: recorded state plus its on-disk status.
struct WorktreeRow {
    name: String,
    repo: String,
    branch: String,
    base: Option<String>,
    path: PathBuf,
    status: WorktreeStatus,
    git: Option<GitState>,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
enum GitState {
    Available(CheckoutStatus),
    Unavailable { error: String },
}

impl GitState {
    fn inspect(path: &std::path::Path) -> Self {
        match inspect_checkout(path) {
            Ok(status) => Self::Available(status),
            Err(error) => Self::Unavailable {
                error: error.to_string(),
            },
        }
    }
}

impl std::fmt::Display for GitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Self::Available(status) => status,
            Self::Unavailable { error } => return write!(f, "Git status unavailable: {error}"),
        };
        write!(
            f,
            "{} at {} ({} staged, {} unstaged, {} untracked)",
            status.branch.as_deref().unwrap_or("detached HEAD"),
            status.head,
            status.staged,
            status.unstaged,
            status.untracked,
        )?;
        match &status.upstream {
            Some(upstream) => write!(
                f,
                ", {} ahead and {} behind {}",
                upstream.ahead, upstream.behind, upstream.reference,
            ),
            None => write!(f, ", no known upstream"),
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
/// per-worktree. Each worktree's recorded base — what its branch was cut
/// from — follows its branch when there is one.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let root = require_ticket_root(
        &app.context,
        args.shared.ticket.as_ref(),
        args.shared.project.as_deref(),
    )?;
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
                base: entry.base.clone(),
                status: WorktreeStatus::of(&path),
                git: args.status.then(|| GitState::inspect(&path)),
                path,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.name.cmp(&b.name)));
    let project = tix_sdk::project::project_of(&root).map(|project| project.name);

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
                // Padded only when some row has a base to show after it, so
                // a ticket without bases renders exactly as it always has.
                let branch_width = if rows.iter().any(|r| r.base.is_some()) {
                    rows.iter().map(|r| r.branch.len()).max().unwrap_or(0)
                } else {
                    0
                };
                println!();
                for row in &rows {
                    let line = format!(
                        "{:name_width$}  {:repo_width$}  {:branch_width$}{}",
                        row.name,
                        row.repo,
                        row.branch,
                        row.base
                            .as_deref()
                            .map(|base| format!("  ← {base}"))
                            .unwrap_or_default(),
                    );
                    println!("{}{}", line.trim_end(), row.status.marker());
                    if let Some(git) = &row.git {
                        println!("  {git}");
                    }
                }
            }
        }
        OutputType::Json => {
            let worktrees: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    let mut value = serde_json::json!({
                        "name": row.name,
                        "repo": row.repo,
                        "branch": row.branch,
                        "base": row.base,
                        "path": row.path,
                        "status": row.status.label(),
                    });
                    if let Some(git) = &row.git {
                        value["git"] = serde_json::json!(git);
                    }
                    value
                })
                .collect();
            let value = serde_json::json!({
                "key": ticket.key,
                "description": ticket.description,
                "project": project,
                "path": root,
                "worktrees": worktrees,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        }
        OutputType::Toml => {
            println!("path = {:?}", root.display().to_string());
            if let Some(name) = &project {
                println!("project = {}", toml::Value::from(name.as_str()));
            }
            println!();
            print!("{}", toml::to_string(&ticket)?);
            if args.status {
                let git: std::collections::BTreeMap<_, _> = rows
                    .iter()
                    .filter_map(|row| row.git.as_ref().map(|git| (&row.name, git)))
                    .collect();
                let mut document = tix_sdk::document::TixDocument::empty();
                document.set_section("git", &git)?;
                println!("\n{document}");
            }
        }
    }
    Ok(())
}
