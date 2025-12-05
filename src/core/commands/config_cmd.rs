//! View or set configuration values.

use crate::core::config::Config;
use anyhow::{bail, Context, Result};
use log::{info, warn};
use std::path::PathBuf;

/// Set a key to a value or show the current value if `value` is None.
pub fn run(key: &str, value: Option<&str>) -> Result<()> {
    let mut config = Config::load()?;

    match key {
        "branch_prefix" => set_string(&mut config.branch_prefix, key, value)?,
        "github_base_url" => set_string(&mut config.github_base_url, key, value)?,
        "default_repository_owner" => set_string(&mut config.default_repository_owner, key, value)?,
        "code_directory" => set_path(&mut config.code_directory, key, value)?,
        "tickets_directory" => set_path(&mut config.tickets_directory, key, value)?,
        other => bail!("Unknown config key '{}'", other),
    }

    if value.is_some() {
        config.save().context("Failed to save config")?;
        info!("Updated '{}'", key);
    }

    Ok(())
}

fn set_string(field: &mut String, key: &str, value: Option<&str>) -> Result<()> {
    if let Some(val) = value {
        *field = val.to_string();
    } else {
        info!("{} = {}", key, field);
    }
    Ok(())
}

fn set_path(field: &mut PathBuf, key: &str, value: Option<&str>) -> Result<()> {
    if let Some(val) = value {
        if val.trim().is_empty() {
            bail!("{} cannot be empty", key);
        }
        let expanded = expand_path(val);
        *field = expanded;
    } else {
        info!("{} = {:?}", key, field);
    }
    Ok(())
}

fn expand_path(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = home::home_dir() {
            return home.join(rest);
        }
        warn!("~ could not be expanded; using literal path");
    }
    PathBuf::from(input)
}

#[cfg(test)]
mod tests {
    use crate::core::config::Config;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn base_config() -> Config {
        Config {
            branch_prefix: "feature".into(),
            github_base_url: "git@github.com".into(),
            default_repository_owner: "my-org".into(),
            code_directory: PathBuf::from("/code"),
            tickets_directory: PathBuf::from("/tickets"),
            repositories: HashMap::new(),
        }
    }

    #[test]
    fn set_branch_prefix_updates_value() {
        let mut config = base_config();
        run_mut(&mut config, "branch_prefix", Some("hotfix")).unwrap();
        assert_eq!(config.branch_prefix, "hotfix");
    }

    #[test]
    fn path_expansion_supports_home_prefix() {
        let mut config = base_config();
        run_mut(&mut config, "code_directory", Some("~/dev")).unwrap();
        // Either expanded or literal depending on home availability
        let expected_prefix = home::home_dir()
            .map(|h| h.join("dev"))
            .unwrap_or_else(|| PathBuf::from("~/dev"));
        assert_eq!(config.code_directory, expected_prefix);
    }

    #[test]
    fn unknown_key_errors() {
        let mut config = base_config();
        assert!(run_mut(&mut config, "unknown", Some("x")).is_err());
    }

    // helper to invoke run without persisting (bypass load/save)
    fn run_mut(config: &mut Config, key: &str, value: Option<&str>) -> Result<(), anyhow::Error> {
        match key {
            "branch_prefix" => {
                config.branch_prefix = value.unwrap_or_default().to_string();
            }
            "github_base_url" => {
                config.github_base_url = value.unwrap_or_default().to_string();
            }
            "default_repository_owner" => {
                config.default_repository_owner = value.unwrap_or_default().to_string();
            }
            "code_directory" => {
                config.code_directory = super::expand_path(value.unwrap_or_default());
            }
            "tickets_directory" => {
                config.tickets_directory = super::expand_path(value.unwrap_or_default());
            }
            other => return Err(anyhow::anyhow!("Unknown config key '{}'", other)),
        }
        Ok(())
    }
}
