//! Clone all registered repositories into the configured code directory.

use crate::core::config::{Config, RepoDefinition};
use crate::core::git;
use anyhow::{Context, Result, bail};
use log::{debug, info, warn};
use std::fs;

/// Run the setup-repos command: clone any missing repositories.
pub fn run() -> Result<()> {
    let config = Config::load()?;

    if config.repositories.is_empty() {
        warn!("No repositories registered. Use `tix add-repo` to add some.");
        return Ok(());
    }

    if config.code_directory.as_os_str().is_empty() {
        bail!("code_directory is not configured; run `tix init` first");
    }

    fs::create_dir_all(&config.code_directory).with_context(|| {
        format!(
            "Failed to ensure code directory at {:?}",
            config.code_directory
        )
    })?;

    info!(
        "Ensuring repositories are cloned under {:?}",
        config.code_directory
    );

    let plan = compute_clone_plan(&config)?;
    if plan.is_empty() {
        info!("All repositories already exist. Nothing to do.");
        return Ok(());
    }

    let mut failures = Vec::new();

    for (alias, repo_def) in plan {
        if let Some(parent) = repo_def.path.parent() {
            fs::create_dir_all(parent).ok();
        }

        info!(
            "Cloning '{}' from {} into {:?}",
            alias, repo_def.url, repo_def.path
        );

        if let Err(e) = git::clone_repo(&repo_def.url, &repo_def.path) {
            let enriched = e.context(format!(
                "Failed to clone '{}' from {} into {:?}",
                alias, repo_def.url, repo_def.path
            ));
            failures.push(enriched);
        } else {
            info!("Cloned '{}'", alias);
        }
    }

    if failures.is_empty() {
        info!("setup-repos complete.");
        Ok(())
    } else {
        // Bubble the first error with the rest summarized for top-level logging.
        let mut iter = failures.into_iter();
        let mut err = iter
            .next()
            .unwrap()
            .context("Failed to clone one or more repositories");
        let mut summaries = Vec::new();
        for extra in iter {
            summaries.push(extra.to_string());
        }
        if !summaries.is_empty() {
            err = err.context(format!(
                "Additional clone failures:\n{}",
                summaries
                    .into_iter()
                    .map(|s| format!("- {}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        Err(err)
    }
}

/// Determine which repositories need cloning (i.e., their target path does not exist).
pub fn compute_clone_plan(config: &Config) -> Result<Vec<(String, RepoDefinition)>> {
    let mut plan = Vec::new();

    for (alias, repo_def) in &config.repositories {
        debug!(
            "Inspecting repo '{}' with target path {:?}",
            alias, repo_def.path
        );

        if repo_def.path.exists() {
            info!(
                "Repo '{}' already exists at {:?}, skipping.",
                alias, repo_def.path
            );
            continue;
        }

        plan.push((alias.clone(), repo_def.clone()));
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::compute_clone_plan;
    use crate::core::{
        config::{Config, RepoDefinition},
        defaults,
    };
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tix-test-{}", nanos));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn base_config(root: &PathBuf) -> Config {
        Config {
            branch_prefix: defaults::DEFAULT_BRANCH_PREFIX.into(),
            github_base_url: defaults::DEFAULT_GITHUB_BASE_URL.into(),
            default_repository_owner: defaults::DEFAULT_REPOSITORY_OWNER.into(),
            code_directory: root.join(defaults::DEFAULT_CODE_DIR_BASENAME),
            tickets_directory: root.join(defaults::DEFAULT_TICKETS_DIR_BASENAME),
            repositories: HashMap::new(),
            plugins: HashMap::new(),
            jira_base_url: None,
        }
    }

    #[test]
    fn compute_clone_plan_skips_existing_paths() {
        let root = unique_temp_dir();
        let mut config = base_config(&root);

        let existing_path = config.code_directory.join("existing");
        fs::create_dir_all(&existing_path).unwrap();

        let missing_path = config.code_directory.join("missing");

        config.repositories.insert(
            "exists".into(),
            RepoDefinition {
                url: "git@github.com:org/existing.git".into(),
                path: existing_path.clone(),
            },
        );
        config.repositories.insert(
            "missing".into(),
            RepoDefinition {
                url: "git@github.com:org/missing.git".into(),
                path: missing_path.clone(),
            },
        );

        let plan = compute_clone_plan(&config).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, "missing");
        assert_eq!(plan[0].1.path, missing_path);
    }
}
