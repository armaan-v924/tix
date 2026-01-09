//! View or set configuration values.

use crate::core::config::Config;
use anyhow::{Context, Result, bail};
use log::{debug, info, warn};
use std::path::PathBuf;
use std::process::Command;

/// Set a key to a value or show the current value if `value` is None.
/// If `key` is None, print the full config.
pub fn run(key: Option<&str>, value: Option<&str>, edit: bool) -> Result<()> {
    validate_edit_usage(key, value, edit)?;
    if key.is_none() && value.is_some() {
        bail!("Config value provided without a key");
    }

    let config_path = Config::config_path()?;
    debug!("Loading config from {:?}", config_path);
    let mut config = Config::load()?;

    if edit && key.is_none() {
        ensure_config_file(&config)?;
        open_in_editor(&config_path)?;
        return Ok(());
    }

    if key.is_none() {
        let toml_string = toml::to_string_pretty(&config)?;
        println!("{}", toml_string);
        return Ok(());
    }

    let key = key.unwrap();
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

fn validate_edit_usage(key: Option<&str>, value: Option<&str>, edit: bool) -> Result<()> {
    if edit && key.is_some() && value.is_some() {
        bail!("Cannot combine --edit with a key and value");
    }
    Ok(())
}

fn ensure_config_file(config: &Config) -> Result<()> {
    let path = Config::config_path()?;
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toml_string = toml::to_string_pretty(config)?;
    std::fs::write(path, toml_string)?;
    Ok(())
}

fn open_in_editor(path: &PathBuf) -> Result<()> {
    let editor = std::env::var("EDITOR").map_err(|_| {
        anyhow::anyhow!("$EDITOR is not set; set it or run `tix config` to view the file")
    })?;
    let status = spawn_editor(&editor, path)?;
    if !status.success() {
        bail!("Editor exited with status {}", status);
    }
    Ok(())
}

fn spawn_editor(editor: &str, path: &PathBuf) -> Result<std::process::ExitStatus> {
    let path_str = path.display().to_string();
    if cfg!(windows) {
        let cmd = format!("{} \"{}\"", editor, path_str);
        return Command::new("cmd")
            .arg("/C")
            .arg(cmd)
            .status()
            .map_err(Into::into);
    }
    let cmd = format!("{} {}", editor, shell_escape(&path_str));
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .status()
        .map_err(Into::into)
}

fn shell_escape(value: &str) -> String {
    let mut escaped = String::from("'");
    for c in value.chars() {
        if c == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(c);
        }
    }
    escaped.push('\'');
    escaped
}

#[cfg(test)]
mod tests {
    use crate::core::{config::Config, defaults};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn base_config() -> Config {
        Config {
            branch_prefix: defaults::DEFAULT_BRANCH_PREFIX.into(),
            github_base_url: defaults::DEFAULT_GITHUB_BASE_URL.into(),
            default_repository_owner: defaults::DEFAULT_REPOSITORY_OWNER.into(),
            code_directory: PathBuf::from(defaults::DEFAULT_CODE_DIR_FALLBACK),
            tickets_directory: PathBuf::from(defaults::DEFAULT_TICKETS_DIR_FALLBACK),
            repositories: HashMap::new(),
            plugins: HashMap::new(),
            jira_base_url: None,
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

    #[test]
    fn edit_with_key_and_value_errors() {
        assert!(super::validate_edit_usage(Some("branch_prefix"), Some("hotfix"), true).is_err());
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
