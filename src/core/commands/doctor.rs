//! Validate configuration and environment for tix.

use crate::core::config::{Config, RepoDefinition};
use log::{error, info, warn};
use std::path::Path;

/// Run a series of checks and report issues.
pub fn run() -> anyhow::Result<()> {
    let config = Config::load()?;

    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    check_string("branch_prefix", &config.branch_prefix, &mut errors, &mut warnings);
    check_path("tickets_directory", &config.tickets_directory, true, &mut errors, &mut warnings);
    check_path("code_directory", &config.code_directory, true, &mut errors, &mut warnings);
    check_string(
        "github_base_url",
        &config.github_base_url,
        &mut errors,
        &mut warnings,
    );
    check_string(
        "default_repository_owner",
        &config.default_repository_owner,
        &mut errors,
        &mut warnings,
    );

    for (alias, repo) in &config.repositories {
        check_repo(alias, repo, &mut warnings);
    }

    for e in &errors {
        error!("{}", e);
    }
    for w in &warnings {
        warn!("{}", w);
    }

    if errors.is_empty() {
        info!("Doctor check passed with {} warning(s).", warnings.len());
        Ok(())
    } else {
        anyhow::bail!("Doctor found {} error(s). See logs above.", errors.len());
    }
}

fn check_string(field: &str, value: &str, errors: &mut Vec<String>, _warnings: &mut Vec<String>) {
    if value.trim().is_empty() {
        errors.push(format!("{} is not set", field));
    }
}

fn check_path(field: &str, path: &Path, must_exist: bool, errors: &mut Vec<String>, _warnings: &mut Vec<String>) {
    if path.as_os_str().is_empty() {
        errors.push(format!("{} is not set", field));
        return;
    }
    if must_exist && !path.exists() {
        errors.push(format!("{} does not exist: {:?}", field, path));
    }
}

fn check_repo(alias: &str, repo: &RepoDefinition, warnings: &mut Vec<String>) {
    if repo.url.trim().is_empty() {
        warnings.push(format!("Repo '{}' has empty url", alias));
    }
    if repo.path.as_os_str().is_empty() {
        warnings.push(format!("Repo '{}' has empty path", alias));
    } else if !repo.path.exists() {
        warnings.push(format!(
            "Repo '{}' path does not exist (will be cloned by setup-repos): {:?}",
            alias, repo.path
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{check_path, check_repo, check_string};
    use crate::core::config::RepoDefinition;
    use std::path::PathBuf;

    #[test]
    fn check_string_flags_empty() {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        check_string("branch_prefix", "", &mut errors, &mut warnings);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn check_path_requires_existence_when_requested() {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        check_path("code_directory", PathBuf::from("/nonexistent").as_path(), true, &mut errors, &mut warnings);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn check_repo_warns_on_missing_path() {
        let mut warnings = Vec::new();
        check_repo(
            "api",
            &RepoDefinition {
                url: "git@github.com:org/api.git".into(),
                path: PathBuf::from("/nope/api"),
            },
            &mut warnings,
        );
        assert!(!warnings.is_empty());
    }
}
