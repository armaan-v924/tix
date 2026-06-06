use crate::types::errors::TixError;
use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

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

    /// Loads a `Config` from a TOML file at `path`.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the file cannot be read.
    /// * `TixError::ParseError` - If the file cannot be parsed as TOML.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::Config;
    /// # use std::path::PathBuf;
    /// let config = Config::load_from(&PathBuf::from("config.toml"));
    /// ```
    pub fn load_from(path: &PathBuf) -> Result<Self, TixError> {
        let resolved_path = Self::resolve_path(path)?;
        let config_content = fs::read_to_string(&resolved_path).map_err(TixError::IoError)?;
        toml::from_str(&config_content).map_err(TixError::ParseError)
    }

    /// Serializes the config to TOML and writes it to `path`.
    ///
    /// Creates parent directories if they do not already exist.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the file cannot be written.
    /// * `TixError::SerializationError` - If the configuration cannot be serialized to TOML.
    ///
    /// # Example
    /// ```no_run
    /// # use tix_engine::Config;
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

    /// Returns the default config file path (`~/.config/tix/config.toml` on Linux/macOS).
    ///
    /// The path may not yet exist on the filesystem.
    ///
    /// # Errors
    /// * `TixError::ConfigNotFound` - If the config directory cannot be determined.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::Config;
    /// let path = Config::default_path().unwrap();
    /// ```
    pub fn default_path() -> Result<PathBuf, TixError> {
        dirs::config_local_dir()
            .map(|p| p.join("tix").join("config.toml"))
            .ok_or_else(|| TixError::ConfigNotFound("could not determine config directory".into()))
    }

    /// Canonicalizes `path`, resolving symlinks and relative segments.
    ///
    /// # Errors
    /// * `TixError::IoError` - If the path does not exist or cannot be resolved.
    fn resolve_path(path: &PathBuf) -> Result<PathBuf, TixError> {
        fs::canonicalize(path).map_err(TixError::IoError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::repository::RepositoryConfig;
    use tempfile::tempdir;

    /// A valid TOML fixture loads without error.
    #[test]
    fn test_config_load() {
        assert!(Config::load_from(&PathBuf::from("test_artifacts/test_config_valid.toml")).is_ok());
    }

    /// A missing file returns `TixError::IoError`.
    #[test]
    fn test_config_load_invalid_path() {
        let result = Config::load_from(&PathBuf::from("/this/is/an/invalid/path"));
        assert!(matches!(result, Err(TixError::IoError(_))));
    }

    /// Malformed TOML returns `TixError::ParseError`.
    #[test]
    fn test_config_load_invalid_toml() {
        let result = Config::load_from(&PathBuf::from("test_artifacts/test_config_invalid.toml"));
        assert!(matches!(result, Err(TixError::ParseError(_))));
    }

    /// Load → mutate → save → reload produces identical config.
    #[test]
    fn test_config_save_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");

        let mut config =
            Config::load_from(&PathBuf::from("test_artifacts/test_config_valid.toml")).unwrap();
        config.configured_repositories.push(RepositoryConfig::new(
            "https://github.com/owner/repo.git".into(),
            "repo".into(),
            PathBuf::from("/code/repo"),
        ));
        config.save_to(&path).unwrap();

        let reloaded = Config::load_from(&path).unwrap();
        assert_eq!(reloaded, config);
    }

    /// `default_path` ends with `tix/config.toml`.
    #[test]
    fn test_config_default_path() {
        let path = Config::default_path().unwrap();
        assert!(path.ends_with("tix/config.toml"));
    }
}
