//! Configuration model and persistence for tix.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Definition of a registered repository (remote URL and local path).
pub struct RepoDefinition {
    /// Remote URL (e.g., `git@github.com:owner/repo.git`).
    pub url: String,
    /// Local code path (e.g., `~/code/repo`).
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Default)]
/// Global configuration values loaded from `config.toml`.
pub struct Config {
    /// Default branch prefix for ticket branches (e.g., `feature`).
    pub branch_prefix: String,
    /// Base URL for GitHub SSH/HTTPS clones.
    pub github_base_url: String,
    /// Default repository owner used when only a repo name is provided.
    pub default_repository_owner: String,
    /// Directory where source repositories live locally.
    pub code_directory: PathBuf,
    /// Directory where ticket worktrees are created.
    pub tickets_directory: PathBuf,

    /// Map of repository aliases to their definitions.
    pub repositories: HashMap<String, RepoDefinition>,
}

impl Config {
    /// Load configuration from the OS config directory (e.g., `~/.config/tix/config.toml`).
    /// Returns `Config::default()` if the file does not exist.
    pub fn load() -> Result<Self> {
        let dirs =
            ProjectDirs::from("", "", "tix").context("Could not determine config directory")?;
        let config_path = dirs.config_dir().join("config.toml");

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    /// Persist the configuration to the OS config directory, creating it if needed.
    pub fn save(&self) -> Result<()> {
        let dirs =
            ProjectDirs::from("", "", "tix").context("Could not determine config directory")?;

        std::fs::create_dir_all(dirs.config_dir())?;

        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(dirs.config_dir().join("config.toml"), toml_string)?;
        Ok(())
    }
}
