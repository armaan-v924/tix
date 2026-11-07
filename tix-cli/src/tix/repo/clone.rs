//! `tix repo clone` — clone registered repositories to their code paths.

use crate::tix::repo::RepoAlias;
use crate::tix::ticket::load_engine_config;
use tix_sdk::{SdkError, TixError};
use tracing::{error, info};

/// Clone registered repositories to their code paths
#[derive(clap::Args, Debug)]
#[group(required = true, multiple = false)]
pub struct Args {
    /// Clone every registered repository not already on disk
    #[arg(short, long, group = "targets")]
    pub all: bool,

    /// Registered aliases to clone
    #[arg(group = "targets")]
    pub repo_aliases: Vec<RepoAlias>,
}

/// Clones each target via [`tix_sdk::RepositoryConfig::ensure`] — a repo already on
/// disk is a reported no-op, not an error.
///
/// The batch never aborts on one failure: every target is attempted, each
/// outcome is reported on its own line, and the exit is nonzero if anything
/// failed. Unknown aliases error up front listing the registered set.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let engine = load_engine_config(&app.context)?;

    let mut registered: Vec<&str> = engine
        .configured_repositories
        .keys()
        .map(String::as_str)
        .collect();
    registered.sort();

    let targets: Vec<String> = if args.all {
        registered.iter().map(|s| s.to_string()).collect()
    } else {
        for alias in &args.repo_aliases {
            if !engine.configured_repositories.contains_key(&alias.0) {
                return Err(SdkError::Engine(TixError::RepoNotFound(format!(
                    "'{}' is not a registered repository (registered: {})",
                    alias.0,
                    if registered.is_empty() {
                        "none".to_string()
                    } else {
                        registered.join(", ")
                    }
                ))));
            }
        }
        args.repo_aliases
            .iter()
            .map(|alias| alias.0.clone())
            .collect()
    };

    let mut failed = 0usize;
    for alias in &targets {
        let config = engine.configured_repositories[alias].clone();
        let already_present = config.code_path.exists();
        match config.ensure(alias) {
            Ok(_) if already_present => info!("{alias}: already on disk"),
            Ok(repo) => info!("{alias}: cloned to {}", repo.config.code_path.display()),
            Err(e) => {
                error!("{alias}: clone failed: {e}");
                failed += 1;
            }
        }
    }

    if failed > 0 {
        return Err(SdkError::Message(format!(
            "{failed} of {} clones failed",
            targets.len()
        )));
    }
    Ok(())
}
