use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const STAMP_DIR: &str = ".tix";
const METADATA_FILE: &str = "info.toml";

#[derive(Serialize, Deserialize, Debug)]
pub struct TicketMetadata {
    pub id: String,
    pub description: Option<String>,
    pub created_at: String, // ISO 8601
}

pub struct Ticket {
    pub root: PathBuf,
    pub metadata: TicketMetadata,
}

impl Ticket {
    // Creates a new ticket stamp in the target directory
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

    /// Checks if a directory is a valid tix workspace
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
