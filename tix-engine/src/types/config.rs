use crate::types::errors::TixError;
use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub branch_prefix: String,
    pub github_base_url: String,
    pub default_repository_owner: String,
    pub code_directory: PathBuf,
    pub tickets_directory: PathBuf,
    pub configured_repositories: Vec<RepositoryConfig>,
}

impl Config {
    pub fn load_from(path: PathBuf) -> Result<Self, TixError> {
        let config_content = fs::read_to_string(path).map_err(TixError::IoError)?;
        toml::from_str(&config_content).map_err(TixError::ParseError)
    }

    pub fn save(&self, path: PathBuf) -> Result<(), TixError> {
        let toml_data = toml::to_string_pretty(&self).map_err(TixError::SerializationError)?;
        let mut file = fs::File::create(path).map_err(TixError::IoError)?;
        file.write_all(toml_data.as_bytes())
            .map_err(TixError::IoError)
    }

    pub fn default_path() -> Result<PathBuf, TixError> {
        dirs::config_local_dir()
            .map(|p| p.join("tix").join("config.toml"))
            .ok_or_else(|| TixError::ConfigNotFound("could not determine config directory".into()))
    }
}
