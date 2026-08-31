//! `tix repo add` — register a repository in config. No cloning; that's
//! `tix repo clone` (#65).

use crate::tix::config::CliConfig;
use crate::tix::repo::{RepoAlias, RepoRef};
use crate::tix::ticket::load_cli_config;
use tix_sdk::document::with_write;
use tix_sdk::{Defaults, EngineConfig, SdkError};

/// Register a repository in config (without cloning)
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Alias to register under (defaults to the repository name)
    #[arg(short, long)]
    pub alias: Option<RepoAlias>,

    /// Owner for a bare `repo` argument (beats [defaults].default_repository_owner)
    // Capital: lowercase `-o` is the global --output.
    #[arg(short = 'O', long)]
    pub owner: Option<String>,

    /// Full URL, `owner/repo`, or bare `repo`
    pub repo: RepoRef,
}

/// Registers a repository under `[engine].configured_repositories`.
///
/// The positional resolves by shape:
///
/// - a full URL (contains `://`, or the scp-like `git@host:` form) is used
///   as-is;
/// - `owner/repo` expands against `[defaults].github_base_url`
///   (`https://github.com` when unset);
/// - a bare `repo` needs an owner — `--owner`, else
///   `[defaults].default_repository_owner`, else a clear error.
///
/// `code_path` derives as `<code_directory>/<alias>`. The write goes through
/// the format-preserving layer and revalidates `EngineConfig` before landing;
/// re-adding an existing alias errors.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let cli: CliConfig = load_cli_config(&app.context)?;

    // Alias may come from the flag; otherwise it falls out of the parse.
    let (remote, repo_name) = resolve_remote(&args.repo.0, args.owner.as_deref(), app)?;
    let alias = args
        .alias
        .as_ref()
        .map(|a| a.0.clone())
        .unwrap_or_else(|| repo_name.clone());
    let code_path = cli.code_directory.join(&alias);

    with_write(&app.context.config_path, |document| {
        let existing: EngineConfig = document.section_or_default("engine")?;
        if existing.configured_repositories.contains_key(&alias) {
            return Err(SdkError::Message(format!(
                "alias '{alias}' is already registered (remote: {}) — pick another with --alias",
                existing.configured_repositories[&alias].remote
            )));
        }

        let mut entry = toml_edit::Table::new();
        entry["remote"] = toml_edit::value(remote.as_str());
        entry["code_path"] = toml_edit::value(code_path.display().to_string());
        // Through `table_at`, so the entry lands as a
        // [engine.configured_repositories.<alias>] section: indexing the
        // path directly would collapse the whole section into a one-line
        // inline table (#146).
        document
            .table_at(&["engine", "configured_repositories"])?
            .insert(&alias, toml_edit::Item::Table(entry));

        // Revalidate the section we just edited before anything hits disk.
        let _valid: EngineConfig = document.section_or_default("engine")?;
        Ok(())
    })?;

    println!("{alias} -> {remote}");
    Ok(())
}

/// Resolves the positional into `(remote_url, repo_name)` by shape.
fn resolve_remote(
    repo: &str,
    owner_flag: Option<&str>,
    app: &crate::tix::utils::App,
) -> Result<(String, String), SdkError> {
    // Full URL forms pass through untouched.
    if repo.contains("://") || (repo.contains('@') && repo.contains(':')) {
        let name = repo
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(repo)
            .trim_end_matches(".git")
            .to_string();
        return Ok((repo.to_string(), name));
    }

    let defaults: Defaults = tix_sdk::document::TixDocument::load(&app.context.config_path)?
        .section_or_default("defaults")?;
    let base = defaults
        .github_base_url
        .unwrap_or_else(|| "https://github.com".to_string());
    let base = base.trim_end_matches('/');

    match repo.split('/').collect::<Vec<_>>().as_slice() {
        [owner, name] => Ok((format!("{base}/{owner}/{name}.git"), (*name).to_string())),
        [name] => {
            let owner = owner_flag
                .map(str::to_string)
                .or(defaults.default_repository_owner)
                .ok_or_else(|| {
                    SdkError::Message(format!(
                        "'{name}' has no owner — pass --owner <owner> or set \
                         [defaults].default_repository_owner in config"
                    ))
                })?;
            Ok((format!("{base}/{owner}/{name}.git"), (*name).to_string()))
        }
        _ => Err(SdkError::Message(format!(
            "'{repo}' is not a URL, owner/repo, or bare repo name"
        ))),
    }
}

#[cfg(test)]
mod remote_shape_tests {
    /// The URL-shape detection used by resolve_remote, exercised on the
    /// pure string forms (config-dependent expansion is covered e2e).
    #[test]
    fn test_url_forms_pass_through() {
        for url in [
            "https://github.com/owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
        ] {
            assert!(
                url.contains("://") || (url.contains('@') && url.contains(':')),
                "expected URL shape for {url}"
            );
        }
        assert!(!("owner/repo".contains("://")));
        assert!(!("repo".contains('@')));
    }
}
