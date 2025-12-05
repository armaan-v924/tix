//! Ticket metadata stamp stored inside each ticket workspace.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

const STAMP_DIR: &str = ".tix";
const METADATA_FILE: &str = "info.toml";

#[derive(Serialize, Deserialize, Debug)]
/// Metadata written to `.tix/info.toml` inside a ticket workspace.
pub struct TicketMetadata {
    /// Ticket identifier (e.g., `JIRA-123`).
    pub id: String,
    /// Optional description captured during setup.
    #[serde(default)]
    pub description: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String, // ISO 8601
    /// Branch name associated with the ticket.
    #[serde(default)]
    pub branch: String,
    /// Repo aliases currently tracked in this ticket (legacy list).
    #[serde(default)]
    pub repos: Vec<String>,
    /// Mapping of repo alias to branch name.
    #[serde(default)]
    pub repo_branches: HashMap<String, String>,
}

/// Represents a ticket workspace and its metadata.
pub struct Ticket {
    pub root: PathBuf,
    pub metadata: TicketMetadata,
}

impl Ticket {
    /// Create a new `.tix/info.toml` stamp under `root` for the given ticket `id`.
    pub fn create(
        root: &Path,
        id: &str,
        description: Option<&String>,
        default_branch: &str,
        repo_branches: &[(String, String)],
    ) -> Result<Self> {
        let stamp_dir = root.join(STAMP_DIR);
        fs::create_dir_all(&stamp_dir).context("Failed to create .tix directory")?;

        let mut repo_branch_map: HashMap<String, String> = HashMap::new();
        for (alias, branch) in repo_branches {
            repo_branch_map.insert(alias.clone(), branch.clone());
        }

        let repos = repo_branch_map.keys().cloned().collect();

        let metadata = TicketMetadata {
            id: id.to_string(),
            description: description.cloned(),
            created_at: chrono::Local::now().to_rfc3339(),
            branch: default_branch.to_string(),
            repos,
            repo_branches: repo_branch_map,
        };

        // Write info.toml
        write_metadata(root, &metadata)?;

        Ok(Ticket {
            root: root.to_path_buf(),
            metadata,
        })
    }

    /// Load metadata from an existing ticket workspace. Errors if the stamp is missing/invalid.
    pub fn load(root: &Path) -> Result<Self> {
        let meta_path = root.join(STAMP_DIR).join(METADATA_FILE);

        if !meta_path.exists() {
            anyhow::bail!("Not a valid tix workspace (missing .tix/info.toml)");
        }

        let content = fs::read_to_string(meta_path)?;
        let metadata: TicketMetadata = toml::from_str(&content)?;

        // Compatibility: if repo_branches is empty but repos exist, seed with ticket branch.
        let mut metadata = metadata;
        if metadata.repo_branches.is_empty() && !metadata.repos.is_empty() {
            for alias in &metadata.repos {
                metadata
                    .repo_branches
                    .insert(alias.clone(), metadata.branch.clone());
            }
        }

        Ok(Ticket {
            root: root.to_path_buf(),
            metadata,
        })
    }

    /// Add repo aliases to the metadata with a given branch, preserving uniqueness and not overwriting existing branches.
    pub fn add_repos_with_branch(root: &Path, repos: &[String], branch: &str) -> Result<()> {
        let mut ticket = Ticket::load(root)?;
        for r in repos {
            if !ticket.metadata.repos.contains(r) {
                ticket.metadata.repos.push(r.clone());
            }
            ticket
                .metadata
                .repo_branches
                .entry(r.clone())
                .or_insert_with(|| branch.to_string());
        }
        write_metadata(root, &ticket.metadata)
    }

    /// Add a single repo->branch mapping.
    pub fn add_repo_branch(root: &Path, repo: &str, branch: &str) -> Result<()> {
        let mut ticket = Ticket::load(root)?;
        if !ticket.metadata.repos.contains(&repo.to_string()) {
            ticket.metadata.repos.push(repo.to_string());
        }
        ticket
            .metadata
            .repo_branches
            .entry(repo.to_string())
            .or_insert_with(|| branch.to_string());
        write_metadata(root, &ticket.metadata)
    }

    /// Remove a repo alias from metadata.
    pub fn remove_repo(root: &Path, repo: &str) -> Result<()> {
        let mut ticket = Ticket::load(root)?;
        ticket
            .metadata
            .repos
            .retain(|existing| existing != repo);
        ticket.metadata.repo_branches.remove(repo);
        write_metadata(root, &ticket.metadata)
    }

    /// Ensure the branch name is recorded (set if empty).
    pub fn ensure_branch(root: &Path, branch: &str) -> Result<()> {
        let mut ticket = Ticket::load(root)?;
        if ticket.metadata.branch.is_empty() {
            ticket.metadata.branch = branch.to_string();
            write_metadata(root, &ticket.metadata)?;
        }
        Ok(())
    }
}

fn write_metadata(root: &Path, metadata: &TicketMetadata) -> Result<()> {
    let stamp_dir = root.join(STAMP_DIR);
    fs::create_dir_all(&stamp_dir).context("Failed to create .tix directory")?;
    let toml_string = toml::to_string_pretty(metadata)?;
    fs::write(stamp_dir.join(METADATA_FILE), toml_string)?;
    Ok(())
}
/// Sanitize a branch name for use as a git worktree name.
pub fn worktree_name_for_branch(branch: &str) -> String {
    branch.replace('/', "_")
}
