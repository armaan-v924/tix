use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tix Engine's configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Config {
    /// Source Repositories configured by the user.
    pub configured_repositories: HashMap<String, RepositoryConfig>,
}

impl Config {
    /// Creates a new `Config` with the given repositories.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::Config;
    /// let config = Config::new(Vec::new());
    /// ```
    pub fn new(configured_repositories: HashMap<String, RepositoryConfig>) -> Self {
        Self {
            configured_repositories: configured_repositories,
        }
    }

    /// Creates a `Config` with all fields set to empty/default values.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::Config;
    /// let config = Config::empty();
    /// ```
    pub fn empty() -> Self {
        Self {
            configured_repositories: HashMap::new(),
        }
    }
}
