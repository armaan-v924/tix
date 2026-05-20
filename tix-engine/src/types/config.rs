use std::path::PathBuf;

use serde::Serialize;

use crate::types::errors::TixError;
use crate::types::plugins::Plugin;
use crate::types::repository::Repository;

#[derive(Serialize)]
pub struct Config {
    pub branch_prefix: String,
    pub github_base_url: String,
    pub default_repository_owner: String,
    pub code_directory: PathBuf,
    pub tickets_directory: PathBuf,
    pub configured_repositories: Vec<Repository>,
    pub plugins: Plugin,
}

impl Config {
    pub fn load_from() -> Self {
        todo!()
    }

    pub fn save(&Self) -> Result<(), TixError> {
        todo!()
    }

    pub fn default_path() -> PathBuf {
        todo!()
    }
}
