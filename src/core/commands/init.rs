//! Interactive `tix init` command to bootstrap configuration.

use crate::core::{config::Config, defaults};
use anyhow::Result;
use dialoguer::Input;
use log::info;
use std::fs;
use std::path::{Path, PathBuf};

/// Expand a path string, handling a leading "~/" to the user's home directory.
pub fn expand_path(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = home::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}

/// Run the interactive init flow: prompt for core configuration fields and persist them.
pub fn run() -> Result<()> {
    let mut config = Config::load()?;

    // Defaults for prompts
    let home_dir = home::home_dir();
    let default_tickets = default_path_str(
        &config.tickets_directory,
        home_dir
            .as_ref()
            .map(|h| h.join(defaults::DEFAULT_TICKETS_DIR_BASENAME)),
        defaults::DEFAULT_TICKETS_DIR_FALLBACK,
    );
    let default_code = default_path_str(
        &config.code_directory,
        home_dir
            .as_ref()
            .map(|h| h.join(defaults::DEFAULT_CODE_DIR_BASENAME)),
        defaults::DEFAULT_CODE_DIR_FALLBACK,
    );
    let default_branch_prefix = fallback(&config.branch_prefix, defaults::DEFAULT_BRANCH_PREFIX);
    let default_github_base = fallback(&config.github_base_url, defaults::DEFAULT_GITHUB_BASE_URL);
    let default_owner = fallback(
        &config.default_repository_owner,
        defaults::DEFAULT_REPOSITORY_OWNER,
    );

    let tickets_input: String = Input::new()
        .with_prompt("Tickets directory")
        .default(default_tickets.clone())
        .interact_text()?;
    let code_input: String = Input::new()
        .with_prompt("Code directory")
        .default(default_code.clone())
        .interact_text()?;
    let branch_prefix_input: String = Input::new()
        .with_prompt("Branch prefix")
        .default(default_branch_prefix.to_string())
        .interact_text()?;
    let github_base_input: String = Input::new()
        .with_prompt("GitHub base URL")
        .default(default_github_base.to_string())
        .interact_text()?;
    let owner_input: String = Input::new()
        .with_prompt("Default repository owner")
        .default(default_owner.to_string())
        .interact_text()?;

    config.tickets_directory = expand_path(&tickets_input);
    config.code_directory = expand_path(&code_input);
    config.branch_prefix = branch_prefix_input;
    config.github_base_url = github_base_input;
    config.default_repository_owner = owner_input;

    // Ensure directories exist
    fs::create_dir_all(&config.tickets_directory)?;
    fs::create_dir_all(&config.code_directory)?;

    config.save()?;

    let config_path = Config::config_path()?;
    info!("Configuration saved at {:?}", config_path);
    Ok(())
}

fn fallback<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() { default } else { value }
}

fn default_path_str(current: &Path, candidate: Option<PathBuf>, fallback: &str) -> String {
    if !current.as_os_str().is_empty() {
        current.to_string_lossy().to_string()
    } else if let Some(c) = candidate {
        c.to_string_lossy().to_string()
    } else {
        fallback.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::expand_path;
    use std::path::PathBuf;

    #[test]
    fn expand_path_handles_home_prefix() {
        // If home is available, "~/" should expand; otherwise it should be passed through.
        let expanded = expand_path("~/mydir");
        if let Some(home) = home::home_dir() {
            assert_eq!(expanded, home.join("mydir"));
        } else {
            assert_eq!(expanded, PathBuf::from("~/mydir"));
        }
    }

    #[test]
    fn expand_path_passthrough_other_paths() {
        assert_eq!(expand_path("/tmp/example"), PathBuf::from("/tmp/example"));
        assert_eq!(expand_path("relative/path"), PathBuf::from("relative/path"));
    }
}
