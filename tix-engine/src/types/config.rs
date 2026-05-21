use crate::types::errors::TixError;
use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Tix Engine's configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Config {
    /// The branch prefix to use for branch names, e.g. `feat/`.
    pub branch_prefix: String,
    /// The base URL for GitHub.
    pub github_base_url: String,
    /// The default repository owner when adding a repo, e.g. `owner/repo`.
    pub default_repository_owner: String, // todo: move this to the frontend, engine doesn't need to know about it
    /// The default directory to store cloned repositories (the worktree root).
    pub code_directory: PathBuf,
    /// The default directory to store ticket directories.
    pub tickets_directory: PathBuf,
    /// Source Repositories configured by the user.
    pub configured_repositories: Vec<RepositoryConfig>,
}

impl Config {
    /// Returns a new `Config` instance with the specified values.
    ///
    /// # Arguments
    /// * `branch_prefix` - The branch prefix to use for branch names.
    /// * `github_base_url` - The base URL for GitHub.
    /// * `default_repository_owner` - The default repository owner.
    /// * `code_directory` - The directory to store code files.
    /// * `tickets_directory` - The directory to store ticket files.
    /// * `configured_repositories` - The list of configured repositories.
    ///
    /// # Returns
    /// A new `Config` instance with the specified values.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::types::config::Config;
    /// # use std::path::PathBuf;
    /// let config = Config::new(
    ///     "feat/".to_string(),
    ///     "https://github.com".to_string(),
    ///     "owner".to_string(),
    ///     PathBuf::new(),
    ///     PathBuf::new(),
    ///     Vec::new(),
    /// );
    /// ```
    pub fn new(
        branch_prefix: String,
        github_base_url: String,
        default_repository_owner: String,
        code_directory: PathBuf,
        tickets_directory: PathBuf,
        configured_repositories: Vec<RepositoryConfig>,
    ) -> Self {
        Self {
            branch_prefix,
            github_base_url,
            default_repository_owner,
            code_directory,
            tickets_directory,
            configured_repositories,
        }
    }

    /// Returns an empty configuration with default values.
    ///
    /// # Returns
    /// A new `Config` instance with default values.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::types::config::Config;
    /// let config = Config::empty();
    /// ```
    pub fn empty() -> Self {
        Self {
            branch_prefix: "".into(),
            github_base_url: "".into(),
            default_repository_owner: "".into(),
            code_directory: PathBuf::new(),
            tickets_directory: PathBuf::new(),
            configured_repositories: Vec::new(),
        }
    }

    /// Loads a `Config` instance from the specified path.
    ///
    /// # Arguments
    /// * `path` - The path to the configuration file.
    ///
    /// # Returns
    /// A `Result` containing the loaded `Config` instance, or an error if loading fails.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the file cannot be read.
    /// * `TixError::ParseError` - If the file cannot be parsed as TOML.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::types::config::Config;
    /// # use std::path::PathBuf;
    /// let config = Config::load_from(&PathBuf::from("config.toml"));
    /// ```
    pub fn load_from(path: &PathBuf) -> Result<Self, TixError> {
        let resolved_path = Self::resolve_path(path)?;
        let config_content = fs::read_to_string(&resolved_path).map_err(TixError::IoError)?;
        toml::from_str(&config_content).map_err(TixError::ParseError)
    }

    /// Saves the `Config` instance to the specified path.
    /// Note that the parent directory will be created if it does not exist.
    ///
    /// # Arguments
    /// * `path` - The path to save the configuration file.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the file cannot be written.
    /// * `TixError::SerializationError` - If the configuration cannot be serialized to TOML.
    ///
    /// # Example
    /// ```no_run
    /// # use tix_engine::types::config::Config;
    /// # use std::path::PathBuf;
    /// # let config = Config::empty();
    /// config.save_to(&PathBuf::from("/etc/tix/config.toml")).unwrap();
    /// ```
    pub fn save_to(&self, path: &PathBuf) -> Result<(), TixError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(TixError::IoError)?;
        }
        let toml_data = toml::to_string_pretty(&self).map_err(TixError::SerializationError)?;
        let mut file = fs::File::create(path).map_err(TixError::IoError)?;
        file.write_all(toml_data.as_bytes())
            .map_err(TixError::IoError)
    }

    /// Returns the default path for the configuration file.
    /// Uses the system local config directory (e.g., `~/.config/tix/config.toml`).
    /// Warning: This path may not yet exist on the system.
    ///
    /// # Returns
    /// The default path for the configuration file.
    ///
    /// # Errors
    /// * `TixError::ConfigNotFound` - If the config directory cannot be determined.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::types::config::Config;
    /// let path = Config::default_path().unwrap();
    /// ```
    pub fn default_path() -> Result<PathBuf, TixError> {
        dirs::config_local_dir()
            .map(|p| p.join("tix").join("config.toml"))
            .ok_or_else(|| TixError::ConfigNotFound("could not determine config directory".into()))
    }

    /// Resolves a path to its canonical form, following symlinks and resolving relative paths.
    /// Private helper function.
    ///
    /// # Arguments
    /// * `path` - The path to resolve.
    ///
    /// # Returns
    /// The resolved path.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the path cannot be resolved.
    fn resolve_path(path: &PathBuf) -> Result<PathBuf, TixError> {
        fs::canonicalize(path).map_err(TixError::IoError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that the `Config` can be loaded from a valid TOML file.
    #[test]
    fn test_config_load() {
        let config = Config::load_from(&PathBuf::from("test_artifacts/test_config_valid.toml"));
        assert!(config.is_ok(), "config did not load successfully");
    }

    /// Tests that the `Config` cannot be loaded from an invalid path.
    #[test]
    fn test_config_load_invalid_path() -> Result<(), TixError> {
        let result = Config::load_from(&PathBuf::from("/this/is/an/invalid/path"));
        let err = result.unwrap_err(); // panics with the Ok value if it's not Err
        assert!(matches!(err, TixError::IoError(_)), "got {:?}", err);
        Ok(())
    }

    /// Tests that the `Config` cannot be loaded from an invalid TOML file.
    #[test]
    fn test_config_load_invalid_toml() -> Result<(), TixError> {
        let result = Config::load_from(&PathBuf::from("test_artifacts/test_config_invalid.toml"));
        let err = result.unwrap_err();
        assert!(matches!(err, TixError::ParseError(_)), "got {:?}", err);
        Ok(())
    }

    /// Tests that the `Config` can be saved to a file and loaded back successfully.
    #[test]
    fn test_config_save() {
        let config = Config::empty();
        let path = PathBuf::from("test_artifacts/test_config_save.toml");
        config.save_to(&path).unwrap();

        // load the saved config and verify it matches the original
        let loaded_config = Config::load_from(&path).unwrap();
        assert_eq!(loaded_config, config);
    }

    /// Tests that the `Config` default path is resolved correctly.
    #[test]
    fn test_config_default_path() {
        let path = Config::default_path().unwrap(); // unwrap is safe here; test should panic if the path is invalid
        assert!(
            path.ends_with("tix/config.toml"),
            "default path is not correct"
        );
    }
}
