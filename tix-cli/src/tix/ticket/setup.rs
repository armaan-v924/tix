//! `tix ticket setup` — create a new ticket workspace with worktrees.

use crate::tix::config::CliConfig;
use crate::tix::repo::RepoAlias;
use crate::tix::ticket::derive_branch_name;
use std::collections::HashMap;
use std::path::PathBuf;
use tix_sdk::document::TixDocument;
use tix_sdk::{Defaults, EngineConfig, SdkError, TicketConfig, TixError, WorktreeConfig};
use tracing::{error, info};

/// Create a new ticket workspace with worktrees
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The ticket key, e.g. JIRA-123 (becomes the directory name; no / or \\)
    pub key: String,

    /// Human-readable description; feeds branch name derivation
    #[arg(short, long)]
    pub description: Option<String>,

    /// Include every registered repository
    #[arg(short, long, group = "repos")]
    pub all: bool,

    /// Repositories to include (defaults to `[defaults].repositories`)
    #[arg(group = "repos")]
    pub repo_aliases: Vec<RepoAlias>,
}

/// Creates `<tickets_directory>/<key>/` with a worktree per selected repo.
///
/// Runs **without ticket context** by design — this is the command that
/// creates it. `[defaults]` is read **once**, here, and the derived values
/// are written into the ticket document; later changes to `[defaults]` never
/// touch this ticket (see
/// [creation-time seeds](https://tix.armaanv.dev/latest/concepts/seeds/)).
///
/// Repo selection: explicit aliases win, `--all` takes every registered
/// repo, and neither falls back to the `defaults.repositories` seed list.
///
/// Partial failure leaves successfully created worktrees in place and
/// records only them in `.tix/ticket.toml`; the command then errors naming
/// the repos that failed.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    if args.key.is_empty() || args.key.contains(['/', '\\']) {
        return Err(SdkError::Message(format!(
            "'{}' is not a valid ticket key — it becomes a directory name, so '/' and '\\' are rejected (path-form creation is #114)",
            args.key
        )));
    }

    let document = TixDocument::load(&app.context.config_path)?;
    let cli: CliConfig = document.section("cli")?.ok_or_else(|| {
        SdkError::Message("global config has no [cli] section — run `tix config init`".to_string())
    })?;
    let engine: EngineConfig = document.section_or_default("engine")?;
    let defaults: Defaults = document.section_or_default("defaults")?;

    // --- select repositories ---
    let selected: Vec<String> = if args.all {
        let mut aliases: Vec<String> = engine.configured_repositories.keys().cloned().collect();
        aliases.sort();
        aliases
    } else if !args.repo_aliases.is_empty() {
        args.repo_aliases
            .iter()
            .map(|alias| alias.0.clone())
            .collect()
    } else {
        defaults.repositories.clone()
    };
    for alias in &selected {
        if !engine.configured_repositories.contains_key(alias) {
            let mut known: Vec<&str> = engine
                .configured_repositories
                .keys()
                .map(String::as_str)
                .collect();
            known.sort();
            return Err(SdkError::Engine(TixError::RepoNotFound(format!(
                "'{alias}' is not a registered repository (registered: {})",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ))));
        }
    }

    // --- seed reads: derivation happens once, now ---
    let branch = derive_branch_name(
        defaults.branch_prefix.as_deref(),
        &args.key,
        args.description.as_deref(),
    );
    let description = args.description.clone().unwrap_or_default();

    let ticket_root: PathBuf = cli.tickets_directory.join(&args.key);
    if ticket_root.exists() {
        return Err(SdkError::Message(format!(
            "ticket '{}' already exists at {}",
            args.key,
            ticket_root.display()
        )));
    }
    std::fs::create_dir_all(&ticket_root).map_err(SdkError::from)?;

    // --- create worktrees; collect successes and failures ---
    let mut worktrees: HashMap<String, WorktreeConfig> = HashMap::new();
    let mut failures: Vec<(String, TixError)> = Vec::new();
    for alias in &selected {
        let repo_config = engine.configured_repositories[alias].clone();
        let result = repo_config
            .ensure(alias)
            .and_then(|repo| repo.create_worktree(alias, &branch, &ticket_root.join(alias), false));
        match result {
            Ok(worktree) => {
                info!(alias = %alias, branch = %worktree.branch, "worktree created");
                worktrees.insert(
                    alias.clone(),
                    WorktreeConfig {
                        repo: alias.clone(),
                        branch: worktree.branch,
                    },
                );
            }
            Err(e) => {
                error!(alias = %alias, error = %e, "failed to create worktree");
                failures.push((alias.clone(), e));
            }
        }
    }

    // --- write the ticket document, recording only what actually exists ---
    let ticket = TicketConfig {
        key: args.key.clone(),
        description,
        worktrees,
    };
    let mut ticket_document = TixDocument::empty();
    ticket_document.set_section("ticket", &ticket)?;
    ticket_document.save(&ticket_root.join(".tix").join("ticket.toml"))?;

    println!("{}", ticket_root.display());

    if !failures.is_empty() {
        let names: Vec<&str> = failures.iter().map(|(alias, _)| alias.as_str()).collect();
        return Err(SdkError::Message(format!(
            "ticket created, but {} of {} worktrees failed: {} — fix and `tix ticket add` them",
            failures.len(),
            selected.len(),
            names.join(", ")
        )));
    }
    Ok(())
}
