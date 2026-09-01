use crate::types::repository::RepositoryConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The `[engine]` section of the global config.
///
/// This is a *section* type, not the whole document — there is deliberately
/// no top-level typed struct for the global config (`design/spec.md` §3.2).
/// The document level belongs to the frontend/SDK's generic parsed tree,
/// which extracts sections on demand; a whole-document struct here would
/// drag frontend types (`[cli]`) and plugin tables into the engine and break
/// the layering.
///
/// Like every section type, the engine has no opinion on where the document
/// lives — the frontend/SDK resolves the path and owns all IO.
///
/// # Examples
///
/// Round-tripping the `[engine]` section:
///
/// ```
/// # use tix_engine::EngineConfig;
/// let engine: EngineConfig = toml::from_str(
///     r#"
///     [configured_repositories.my-repo]
///     remote = "https://github.com/owner/repo.git"
///     code_path = "/home/user/code/repo"
///     "#,
/// )
/// .unwrap();
///
/// assert!(engine.configured_repositories.contains_key("my-repo"));
///
/// let restored: EngineConfig = toml::from_str(&toml::to_string(&engine).unwrap()).unwrap();
/// assert_eq!(restored, engine);
/// ```
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    /// Source repositories registered by the user, keyed by alias.
    #[serde(default)]
    pub configured_repositories: HashMap<String, RepositoryConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_config() -> EngineConfig {
        let mut repos = HashMap::new();
        repos.insert(
            "my-repo".to_string(),
            RepositoryConfig::new(
                "https://github.com/owner/repo.git".to_string(),
                PathBuf::from("/home/user/code/repo"),
            ),
        );
        EngineConfig {
            configured_repositories: repos,
        }
    }

    /// The `[engine]` section subtree deserializes correctly.
    #[test]
    fn test_deserialize_section() {
        let toml = r#"
[configured_repositories.my-repo]
remote = "https://github.com/owner/repo.git"
code_path = "/home/user/code/repo"
"#;
        let config: EngineConfig = toml::from_str(toml).unwrap();
        assert_eq!(config, sample_config());
    }

    /// An empty section parses to the default (no repositories).
    #[test]
    fn test_empty_section() {
        let config: EngineConfig = toml::from_str("").unwrap();
        assert_eq!(config, EngineConfig::default());
    }

    /// Unknown fields in the section are rejected — `deny_unknown_fields`
    /// applies to this subtree only.
    #[test]
    fn test_rejects_unknown_fields() {
        let toml = r#"
unknown_field = "bad"
"#;
        assert!(toml::from_str::<EngineConfig>(toml).is_err());
    }

    /// Serializing and deserializing preserves all data.
    #[test]
    fn test_round_trip() {
        let config = sample_config();
        let restored: EngineConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(config, restored);
    }
}
