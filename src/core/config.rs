//! Configuration model and persistence for tix.

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, path::Path};

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Definition of a registered repository (remote URL and local path).
pub struct RepoDefinition {
    /// Remote URL (e.g., `git@github.com:owner/repo.git`).
    pub url: String,
    /// Local code path (e.g., `~/code/repo`).
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Definition of a registered plugin.
pub struct PluginDefinition {
    /// Path to the plugin entrypoint (e.g., `/path/to/plugin.py`).
    pub entrypoint: PathBuf,
    /// Optional description shown in listings.
    #[serde(default)]
    pub description: String,
    /// Optional Python interpreter override (e.g., `python3.11`).
    #[serde(default)]
    pub python: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
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

    /// Map of plugin names to their definitions.
    #[serde(default)]
    pub plugins: HashMap<String, PluginDefinition>,

    /// Optional base URL for Jira (e.g., `https://company.atlassian.net/browse`).
    #[serde(default)]
    pub jira_base_url: Option<String>,
}

impl Config {
    /// Load configuration from the OS config directory (e.g., `~/.config/tix/config.toml`).
    /// Returns `Config::default()` if the file does not exist.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = std::fs::read_to_string(config_path)?;
        let config: Config = toml::from_str(&content)?;

        Ok(config)
    }

    /// Persist the configuration to the OS config directory, creating it if needed.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(config_path, toml_string)?;
        Ok(())
    }

    /// Path to the configuration file (e.g., `~/.config/tix/config.toml`).
    pub fn config_path() -> Result<PathBuf> {
        if let Some(path) = xdg_config_home_path() {
            return Ok(path.join("config.toml"));
        }

        let dirs =
            ProjectDirs::from("", "", "tix").context("Could not determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }
}

/// Resolve `$XDG_CONFIG_HOME/tix` when the variable is set and non-empty.
fn xdg_config_home_path() -> Option<PathBuf> {
    let dir = env::var_os("XDG_CONFIG_HOME")?;
    let dir: &Path = dir.as_ref();
    if dir.as_os_str().is_empty() {
        return None;
    }
    // Validate that the path is absolute and does not contain any parent directory components
    if !dir.is_absolute()
        || dir
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(dir.join("tix"))
}
