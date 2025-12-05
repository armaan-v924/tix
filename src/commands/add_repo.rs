//! Register a repository in the configuration without cloning it.

use crate::config::{Config, RepoDefinition};
use anyhow::{bail, Context, Result};
use log::{debug, info, warn};

/// Resolve the desired alias and repo definition for a given user input.
/// This is separated for testability.
pub fn plan_repo_registration(
    config: &Config,
    repo_input: &str,
    alias: Option<&str>,
) -> Result<(String, RepoDefinition)> {
    if config.code_directory.as_os_str().is_empty() {
        bail!("code_directory is not configured; run `tix init` first");
    }
    if repo_input.trim().is_empty() {
        bail!("Repository input cannot be empty");
    }

    let parsed = parse_repo_input(config, repo_input)?;
    let alias = alias
        .filter(|a| !a.trim().is_empty())
        .map(|a| a.to_string())
        .unwrap_or_else(|| parsed.name.clone());

    let local_path = config.code_directory.join(&alias);
    let repo_def = RepoDefinition {
        url: parsed.url,
        path: local_path,
    };

    Ok((alias, repo_def))
}

/// Add a repository entry to config and save.
pub fn run(repo_input: &str, alias: Option<String>) -> Result<()> {
    let mut config = Config::load()?;
    let (alias, repo_def) = plan_repo_registration(&config, repo_input, alias.as_deref())?;

    debug!(
        "Registering repo input '{}' as alias '{}' with url '{}' and path {:?}",
        repo_input, alias, repo_def.url, repo_def.path
    );
    if config.repositories.contains_key(&alias) {
        warn!("Alias '{}' already exists. Overwriting existing entry.", alias);
    }
    config.repositories.insert(alias.clone(), repo_def);
    config.save().context("Failed to save updated config")?;

    info!("Registered repository '{}' in config", alias);
    Ok(())
}

struct ParsedRepo {
    name: String,
    url: String,
}

fn parse_repo_input(config: &Config, input: &str) -> Result<ParsedRepo> {
    let trimmed = input.trim().trim_end_matches('/');

    // Case 1: Full URL (ssh or https)
    if trimmed.contains("://") || trimmed.contains('@') {
        let name = repo_name_from_path(trimmed)
            .ok_or_else(|| anyhow::anyhow!("Could not infer repo name from URL '{}'", trimmed))?;
        debug!("Detected full URL input; inferred repo name '{}'", name);
        return Ok(ParsedRepo {
            name,
            url: trimmed.to_string(),
        });
    }

    // Case 2: owner/name
    if trimmed.contains('/') {
        let mut parts = trimmed.split('/');
        let owner = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing owner in '{}'", trimmed))?;
        let name = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing repo name in '{}'", trimmed))?;
        debug!("Detected owner/name input '{}'; owner='{}', name='{}'", trimmed, owner, name);
        let url = build_url(&clean_base(&config.github_base_url), owner, name)?;
        return Ok(ParsedRepo {
            name: name.to_string(),
            url,
        });
    }

    // Case 3: name only
    if config.default_repository_owner.is_empty() {
        bail!("default_repository_owner is not set; run `tix init` or pass owner/repo");
    }
    let owner = &config.default_repository_owner;
    let name = trimmed;
    debug!(
        "Detected name-only input '{}'; using default owner '{}'",
        name, owner
    );
    let url = build_url(&clean_base(&config.github_base_url), owner, name)?;
    Ok(ParsedRepo {
        name: name.to_string(),
        url,
    })
}

fn build_url(base: &str, owner: &str, name: &str) -> Result<String> {
    if base.is_empty() {
        bail!("github_base_url is not set; run `tix init`");
    }
    let path = format!("{}/{}", owner, name);
    let url = if base.contains("://") {
        format!("{}/{}.git", base.trim_end_matches('/'), path)
    } else {
        format!("{}:{}.git", base.trim_end_matches(':'), path)
    };
    Ok(url)
}

fn clean_base(base: &str) -> String {
    base.trim_end_matches(['/',' ']).to_string()
}

fn repo_name_from_path(input: &str) -> Option<String> {
    let maybe = input
        .trim_end_matches('/')
        .rsplit(&['/', ':'][..])
        .next()?;
    let name = maybe.trim_end_matches(".git");
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_url, parse_repo_input, plan_repo_registration, repo_name_from_path};
    use crate::config::Config;
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
    fn repo_name_from_url_handles_git_suffix() {
        assert_eq!(
            repo_name_from_path("git@github.com:owner/repo.git"),
            Some("repo".into())
        );
        assert_eq!(
            repo_name_from_path("https://github.com/owner/repo"),
            Some("repo".into())
        );
    }

    #[test]
    fn build_url_supports_https_and_ssh_bases() {
        assert_eq!(
            build_url("https://github.com", "owner", "repo").unwrap(),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            build_url("git@github.com", "owner", "repo").unwrap(),
            "git@github.com:owner/repo.git"
        );
    }

    #[test]
    fn parse_repo_input_full_url_keeps_input() {
        let config = base_config();
        let parsed = parse_repo_input(&config, "git@github.com:foo/bar.git").unwrap();
        assert_eq!(parsed.name, "bar");
        assert_eq!(parsed.url, "git@github.com:foo/bar.git");
    }

    #[test]
    fn parse_repo_input_owner_name_uses_base() {
        let config = base_config();
        let parsed = parse_repo_input(&config, "foo/bar").unwrap();
        assert_eq!(parsed.name, "bar");
        assert_eq!(parsed.url, "git@github.com:foo/bar.git");
    }

    #[test]
    fn parse_repo_input_name_only_uses_default_owner() {
        let config = base_config();
        let parsed = parse_repo_input(&config, "service").unwrap();
        assert_eq!(parsed.name, "service");
        assert_eq!(parsed.url, "git@github.com:my-org/service.git");
    }

    #[test]
    fn plan_registration_sets_alias_and_path() {
        let config = base_config();
        let (alias, def) =
            plan_repo_registration(&config, "git@github.com:foo/bar.git", Some("api")).unwrap();
        assert_eq!(alias, "api");
        assert_eq!(def.url, "git@github.com:foo/bar.git");
        assert_eq!(def.path, PathBuf::from("/code/api"));
    }
}
