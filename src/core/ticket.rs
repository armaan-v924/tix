//! Ticket metadata stamp stored inside each ticket workspace.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const STAMP_DIR: &str = ".tix";
const METADATA_FILE: &str = "info.toml";

#[derive(Serialize, Deserialize, Debug)]
/// Metadata written to `.tix/info.toml` inside a ticket workspace.
pub struct TicketMetadata {
    /// Ticket identifier (e.g., `JIRA-123`).
    pub id: String,
    /// Optional description captured during setup.
    pub description: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String, // ISO 8601
}

/// Represents a ticket workspace and its metadata.
pub struct Ticket {
    pub root: PathBuf,
    pub metadata: TicketMetadata,
}

impl Ticket {
    /// Create a new `.tix/info.toml` stamp under `root` for the given ticket `id`.
    pub fn create(root: &Path, id: &str, description: Option<&String>) -> Result<Self> {
        let stamp_dir = root.join(STAMP_DIR);
        fs::create_dir_all(&stamp_dir).context("Failed to create .tix directory")?;

        let metadata = TicketMetadata {
            id: id.to_string(),
            description: description.cloned(),
            created_at: chrono::Local::now().to_rfc3339(),
        };

        // Write info.toml
        let toml_string = toml::to_string_pretty(&metadata)?;
        fs::write(stamp_dir.join(METADATA_FILE), toml_string)?;

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

        Ok(Ticket {
            root: root.to_path_buf(),
            metadata,
        })
    }
}
