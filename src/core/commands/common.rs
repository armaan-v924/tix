//! Shared helpers for commands to reduce drift.

use crate::core::config::Config;
use anyhow::{Result, bail};
use std::env;
use std::path::PathBuf;

/// Build the default branch name for a ticket (with optional description).
pub fn build_branch_name(config: &Config, ticket_id: &str, description: Option<&String>) -> String {
    let mut branch_name = format!("{}/{}", config.branch_prefix, ticket_id);
    if let Some(desc) = description {
        let sanitized = sanitize_description(desc);
        if !sanitized.is_empty() {
            branch_name.push('-');
            branch_name.push_str(&sanitized);
        }
    }
    branch_name
}

/// Sanitize free-form text for inclusion in a git branch name (lowercase, alnum, single hyphens).
pub fn sanitize_description(input: &str) -> String {
    let mut result = String::new();
    let mut last_was_hyphen = true; // Start true to trim leading hyphens

    for c in input.chars() {
        if c.is_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            // Treat everything else (spaces, symbols) as a separator
            result.push('-');
            last_was_hyphen = true;
        }
    }

    // Trim trailing hyphen if exists
    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Locate the ticket root for a command, either from a provided id or by walking up.
pub fn locate_ticket_root(ticket: Option<&str>, config: &Config) -> Result<PathBuf> {
    if let Some(id) = ticket {
        return Ok(config.tickets_directory.join(id));
    }

    if let Some(dir) = find_ticket_root_from_cwd() {
        return Ok(dir);
    }

    bail!("Could not infer ticket. Run inside a ticket directory or provide --ticket.");
}

fn find_ticket_root_from_cwd() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        let candidate = current.join(".tix").join("info.toml");
        if candidate.exists() {
            return Some(current);
        }

        if !current.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{build_branch_name, locate_ticket_root, sanitize_description};
    use crate::core::{config::Config, defaults};
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn base_config() -> Config {
        Config {
            branch_prefix: defaults::DEFAULT_BRANCH_PREFIX.into(),
            github_base_url: defaults::DEFAULT_GITHUB_BASE_URL.into(),
            default_repository_owner: defaults::DEFAULT_REPOSITORY_OWNER.into(),
            code_directory: PathBuf::from(defaults::DEFAULT_CODE_DIR_FALLBACK),
            tickets_directory: PathBuf::from(defaults::DEFAULT_TICKETS_DIR_FALLBACK),
            repositories: HashMap::new(),
            plugins: HashMap::new(),
        }
    }

    #[test]
    fn branch_name_includes_description() {
        let cfg = base_config();
        let desc = "Short Summary".to_string();
        let name = build_branch_name(&cfg, "JIRA-1", Some(&desc));
        assert_eq!(name, "feature/JIRA-1-short-summary");
    }

    #[test]
    fn locate_ticket_root_walks_upwards() {
        static CWD_LOCK: Mutex<()> = Mutex::new(());
        let _guard = CWD_LOCK.lock().unwrap();

        let tmp = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp-tests-common");
        let nested = tmp.join("nested/child");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(tmp.join(".tix")).unwrap();
        fs::write(tmp.join(".tix/info.toml"), "dummy").unwrap();

        let cwd = env::current_dir().unwrap();
        env::set_current_dir(&nested).unwrap();
        let found = locate_ticket_root(None, &base_config()).unwrap();
        env::set_current_dir(cwd).unwrap();

        assert_eq!(found, tmp);
    }

    #[test]
    fn sanitize_description_matches_branch_rules() {
        assert_eq!(sanitize_description("Short Summary"), "short-summary");
        assert_eq!(
            sanitize_description("Feat: Payment/Auth"),
            "feat-payment-auth"
        );
    }
}
