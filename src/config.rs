use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RepoDefinition {
    pub url: String,   // Remote URL (git@github.com:owner/repo.git)
    pub path: PathBuf, // Local code path (~/code/repo)
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    pub branch_prefix: String,
    pub github_base_url: String,
    pub default_repository_owner: String,
    pub code_directory: PathBuf,
    pub tickets_directory: PathBuf,

    pub repositories: HashMap<String, RepoDefinition>,
}

impl Config {
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

    pub fn save(&self) -> Result<()> {
        let dirs =
            ProjectDirs::from("", "", "tix").context("Could not determine config directory")?;

        std::fs::create_dir_all(dirs.config_dir())?;

        let toml_string = toml::to_string_pretty(self)?;
        std::fs::write(dirs.config_dir().join("config.toml"), toml_string)?;
        Ok(())
    }
}
