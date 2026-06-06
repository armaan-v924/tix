use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tix Engine's configuration.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Config {
    engine: Engine,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Engine {
    /// Source Repositories configured by the user.
    pub configured_repositories: HashMap<String, RepositoryConfig>,
}

impl Config {
    /// Creates a new `Config` with the given repositories.
    ///
    /// # Example
    /// ```
    /// # use tix_engine::Config;
    /// # use std::collections::HashMap;
    /// let config = Config::new(HashMap::new());
    /// ```
    pub fn new(configured_repositories: HashMap<String, RepositoryConfig>) -> Self {
        Self {
            engine: Engine::new(configured_repositories),
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
            engine: Engine::new(HashMap::new()),
        }
    }
}

impl Engine {
    fn new(configured_repositories: HashMap<String, RepositoryConfig>) -> Self {
        Self { configured_repositories }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_config() -> Config {
        let mut repos = HashMap::new();
        repos.insert(
            "my-repo".to_string(),
            RepositoryConfig::new(
                "https://github.com/owner/repo.git".to_string(),
                PathBuf::from("/home/user/code/repo"),
            ),
        );
        Config::new(repos)
    }

    /// A TOML document with an `[engine]` section deserializes correctly.
    #[test]
    fn test_deserialize_engine_section() {
        let toml = r#"
[engine.configured_repositories.my-repo]
remote = "https://github.com/owner/repo.git"
code_path = "/home/user/code/repo"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config, sample_config());
    }

    /// Extra unknown top-level tables are silently ignored.
    #[test]
    fn test_deserialize_ignores_unknown_top_level_tables() {
        let toml = r#"
[engine.configured_repositories.my-repo]
remote = "https://github.com/owner/repo.git"
code_path = "/home/user/code/repo"

[cli]
color = true
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config, sample_config());
    }

    /// Unknown fields under `[engine]` are rejected.
    #[test]
    fn test_deserialize_rejects_unknown_engine_fields() {
        let toml = r#"
[engine]
unknown_field = "bad"
"#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }

    /// Serializing and deserializing a Config preserves all data under `[engine]` nesting.
    #[test]
    fn test_round_trip() {
        let config = sample_config();
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("[engine."));
        let restored: Config = toml::from_str(&toml).unwrap();
        assert_eq!(config, restored);
    }
}
