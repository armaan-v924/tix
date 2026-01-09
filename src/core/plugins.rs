//! Python plugin registry and execution.

use crate::core::commands::common::locate_ticket_root;
use crate::core::config::{Config, PluginDefinition, RepoDefinition};
use crate::core::ticket::Ticket;
use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, path};

#[derive(Serialize, Debug)]
/// Context passed to plugins (serialized as JSON).
pub struct PluginContext {
    /// Registered plugin name.
    pub plugin_name: String,
    /// Absolute path to the ticket root.
    pub ticket_root: PathBuf,
    /// Working directory when tix was invoked.
    pub current_working_dir: PathBuf,
    /// Repo alias for the working directory (if inside a repo worktree).
    pub current_repo_alias: Option<String>,
    /// Repo worktree path for the working directory (if inside a repo worktree).
    pub current_repo_path: Option<PathBuf>,
    /// Ticket metadata from `.tix/info.toml`.
    pub ticket: crate::core::ticket::TicketMetadata,
    /// Full config snapshot at invocation time (read-only by convention).
    pub config: Config,
    /// Configured code directory.
    pub code_directory: PathBuf,
    /// Configured tickets directory.
    pub tickets_directory: PathBuf,
    /// Plugin-specific cache directory.
    pub plugin_cache_dir: PathBuf,
    /// Plugin-specific global state directory.
    pub plugin_state_dir: PathBuf,
    /// Plugin-specific per-ticket state directory.
    pub plugin_ticket_state_dir: PathBuf,
    /// Repository definitions keyed by alias.
    pub repositories: HashMap<String, RepoDefinition>,
}

/// Load plugins from config and return a sorted list.
pub fn list_plugins() -> Result<Vec<(String, PluginDefinition)>> {
    let config = Config::load()?;
    let mut plugins: Vec<_> = config.plugins.into_iter().collect();
    plugins.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(plugins)
}

/// Entry point for external subcommand routing.
pub fn run_external(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        bail!("No plugin specified. Run `tix plugins list`.");
    }
    let name = &args[0];
    let plugin_args = &args[1..];
    run_plugin(name, plugin_args)
}

/// Run a registered plugin by name with the provided arguments.
pub fn run_plugin(name: &str, args: &[String]) -> Result<()> {
    let config = Config::load()?;
    let config_path = Config::config_path()?;
    let working_dir = env::current_dir().context("Failed to resolve current directory")?;

    let plugin = config
        .plugins
        .get(name)
        .cloned()
        .with_context(|| format!("Unknown plugin '{}'. Run `tix plugins list`.", name))?;

    let entrypoint = resolve_entrypoint(&config_path, &plugin.entrypoint);
    validate_entrypoint(&entrypoint)?;

    let ticket_root = locate_ticket_root(None, &config)?;
    let ticket = Ticket::load(&ticket_root)?;
    let plugin_cache_dir = plugin_cache_dir(name, true)?;
    let plugin_state_dir = plugin_state_dir(name, true)?;
    let plugin_ticket_state_dir = plugin_ticket_state_dir(&ticket_root, name, true)?;
    let (current_repo_alias, current_repo_path) =
        detect_current_repo(&ticket_root, &ticket.metadata, &working_dir);

    let context = PluginContext {
        plugin_name: name.to_string(),
        ticket_root: ticket_root.clone(),
        current_working_dir: working_dir.clone(),
        current_repo_alias,
        current_repo_path,
        ticket: ticket.metadata,
        config: config.clone(),
        code_directory: config.code_directory,
        tickets_directory: config.tickets_directory,
        plugin_cache_dir: plugin_cache_dir.clone(),
        plugin_state_dir: plugin_state_dir.clone(),
        plugin_ticket_state_dir: plugin_ticket_state_dir.clone(),
        repositories: config.repositories,
    };

    let context_file = write_context_file(&ticket_root, &context)?;
    let context_path = context_file.path().to_path_buf();
    let project_root = find_uv_project_root(&entrypoint)?;

    let mut command = Command::new("uv");
    command.arg("run").arg("--project").arg(&project_root);
    if let Some(python) = plugin.python.as_deref() {
        command.arg("--python").arg(python);
    }
    command
        .arg("--")
        .arg("python")
        .arg("-c")
        .arg(python_shim())
        .arg(&entrypoint)
        .args(args)
        .current_dir(&ticket_root)
        .env("TIX_CONTEXT_PATH", &context_path)
        .env("TIX_TICKET_ROOT", &ticket_root)
        .env("TIX_PLUGIN_CACHE_DIR", &plugin_cache_dir)
        .env("TIX_PLUGIN_STATE_DIR", &plugin_state_dir)
        .env("TIX_PLUGIN_TICKET_STATE_DIR", &plugin_ticket_state_dir);

    let status = command
        .status()
        .with_context(|| format!("Failed to run plugin '{}' via uv", name))?;

    if !status.success() {
        bail!("Plugin '{}' exited with status {}", name, status);
    }

    Ok(())
}

fn resolve_entrypoint(config_path: &Path, entrypoint: &Path) -> PathBuf {
    if entrypoint.is_absolute() {
        return entrypoint.to_path_buf();
    }
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(entrypoint)
}

fn detect_current_repo(
    ticket_root: &Path,
    ticket: &crate::core::ticket::TicketMetadata,
    working_dir: &Path,
) -> (Option<String>, Option<PathBuf>) {
    let mut candidates: Vec<String> = ticket.repo_branches.keys().cloned().collect();
    for alias in &ticket.repos {
        if !candidates.contains(alias) {
            candidates.push(alias.clone());
        }
    }

    let mut best_match: Option<(String, PathBuf)> = None;
    for alias in candidates {
        let candidate_path = ticket_root.join(&alias);
        if working_dir.starts_with(&candidate_path) {
            let replace = match &best_match {
                Some((_, existing_path)) => {
                    candidate_path.as_os_str().len() > existing_path.as_os_str().len()
                }
                None => true,
            };
            if replace {
                best_match = Some((alias, candidate_path));
            }
        }
    }

    match best_match {
        Some((alias, path)) => (Some(alias), Some(path)),
        None => (None, None),
    }
}

fn validate_entrypoint(entrypoint: &Path) -> Result<()> {
    if !entrypoint.exists() {
        bail!(
            "Plugin entrypoint '{}' does not exist",
            entrypoint.display()
        );
    }
    if entrypoint.is_dir() {
        bail!(
            "Plugin entrypoint '{}' is a directory, expected a file",
            entrypoint.display()
        );
    }
    Ok(())
}

fn write_context_file(
    ticket_root: &Path,
    context: &PluginContext,
) -> Result<tempfile::NamedTempFile> {
    let stamp_dir = ticket_root.join(".tix");
    std::fs::create_dir_all(&stamp_dir)?;
    let mut file = tempfile::NamedTempFile::new_in(&stamp_dir)?;
    serde_json::to_writer(&mut file, context)?;
    file.flush()?;
    Ok(file)
}

fn python_shim() -> &'static str {
    r#"
import json
import sys
import importlib.util
from dataclasses import dataclass
from typing import Any, Dict, List

@dataclass
class TixPluginContext:
    plugin_name: str
    ticket_root: str
    current_working_dir: str
    ticket: Dict[str, Any]
    config: Dict[str, Any]
    code_directory: str
    tickets_directory: str
    plugin_cache_dir: str
    plugin_state_dir: str
    plugin_ticket_state_dir: str
    repositories: Dict[str, Any]

def load_context(path: str) -> TixPluginContext:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    return TixPluginContext(**data)

def load_plugin(entrypoint: str):
    spec = importlib.util.spec_from_file_location("tix_plugin", entrypoint)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not load plugin from {entrypoint}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def main():
    if len(sys.argv) < 2:
        raise RuntimeError("Missing plugin entrypoint")
    entrypoint = sys.argv[1]
    argv = sys.argv[2:]
    ctx_path = os.environ.get("TIX_CONTEXT_PATH")
    if not ctx_path:
        raise RuntimeError("TIX_CONTEXT_PATH is not set")
    ctx = load_context(ctx_path)
    module = load_plugin(entrypoint)
    if not hasattr(module, "main"):
        raise RuntimeError("Plugin must define a main(context, argv) function")
    module.main(ctx, argv)

if __name__ == "__main__":
    import os
    main()
"#
}

fn find_uv_project_root(entrypoint: &Path) -> Result<PathBuf> {
    let mut current = entrypoint
        .parent()
        .context("Plugin entrypoint has no parent directory")?
        .to_path_buf();
    loop {
        let candidate = current.join("pyproject.toml");
        if candidate.exists() {
            return Ok(current);
        }
        if !current.pop() {
            break;
        }
    }
    bail!("No pyproject.toml found for plugin; ensure it is a uv project")
}

pub fn plugin_cache_root() -> Result<PathBuf> {
    if let Some(path) = xdg_cache_home_path() {
        return Ok(path);
    }
    let dirs = ProjectDirs::from("", "", "tix").context("Could not determine cache directory")?;
    Ok(dirs.cache_dir().to_path_buf())
}

pub fn plugin_cache_dir(plugin_name: &str, create: bool) -> Result<PathBuf> {
    let base = plugin_cache_root()?;
    let sanitized = sanitize_plugin_name(plugin_name);
    let state_dir = base.join("plugins").join(sanitized);
    if create {
        std::fs::create_dir_all(&state_dir)?;
    }
    Ok(state_dir)
}

pub fn plugin_state_root() -> Result<PathBuf> {
    if let Some(path) = xdg_state_home_path() {
        return Ok(path);
    }
    let dirs = ProjectDirs::from("", "", "tix").context("Could not determine state directory")?;
    let state_dir = dirs
        .state_dir()
        .context("State directory is not available on this platform")?;
    Ok(state_dir.to_path_buf())
}

pub fn plugin_state_dir(plugin_name: &str, create: bool) -> Result<PathBuf> {
    let base = plugin_state_root()?;
    let sanitized = sanitize_plugin_name(plugin_name);
    let state_dir = base.join("plugins").join(sanitized);
    if create {
        std::fs::create_dir_all(&state_dir)?;
    }
    Ok(state_dir)
}

pub fn plugin_ticket_state_dir(
    ticket_root: &Path,
    plugin_name: &str,
    create: bool,
) -> Result<PathBuf> {
    let sanitized = sanitize_plugin_name(plugin_name);
    let state_dir = ticket_root.join(".tix").join("plugins").join(sanitized);
    if create {
        std::fs::create_dir_all(&state_dir)?;
    }
    Ok(state_dir)
}

pub fn remove_plugin_cache(plugin_name: &str) -> Result<bool> {
    let state_dir = plugin_cache_dir(plugin_name, false)?;
    if !state_dir.exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&state_dir)?;
    Ok(true)
}

fn xdg_cache_home_path() -> Option<PathBuf> {
    let dir = env::var_os("XDG_CACHE_HOME")?;
    let dir: &path::Path = dir.as_ref();
    if dir.as_os_str().is_empty() {
        return None;
    }
    if !dir.is_absolute()
        || dir
            .components()
            .any(|c| matches!(c, path::Component::ParentDir))
    {
        return None;
    }
    Some(dir.join("tix"))
}

fn xdg_state_home_path() -> Option<PathBuf> {
    let dir = env::var_os("XDG_STATE_HOME")?;
    let dir: &path::Path = dir.as_ref();
    if dir.as_os_str().is_empty() {
        return None;
    }
    if !dir.is_absolute()
        || dir
            .components()
            .any(|c| matches!(c, path::Component::ParentDir))
    {
        return None;
    }
    Some(dir.join("tix"))
}

fn sanitize_plugin_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "plugin".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{PluginContext, find_uv_project_root, resolve_entrypoint};
    use crate::core::config::{Config, RepoDefinition};
    use crate::core::ticket::TicketMetadata;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn resolve_entrypoint_respects_absolute_path() {
        let entry = Path::new("/tmp/plugin.py");
        let resolved = resolve_entrypoint(Path::new("/config/config.toml"), entry);
        assert_eq!(resolved, PathBuf::from("/tmp/plugin.py"));
    }

    #[test]
    fn resolve_entrypoint_uses_config_dir_for_relative() {
        let entry = Path::new("plugins/my.py");
        let resolved = resolve_entrypoint(Path::new("/config/config.toml"), entry);
        assert_eq!(resolved, PathBuf::from("/config/plugins/my.py"));
    }

    #[test]
    fn context_is_serializable() {
        let mut repos = HashMap::new();
        repos.insert(
            "api".to_string(),
            RepoDefinition {
                url: "https://example.com/api".into(),
                path: PathBuf::from("/code/api"),
            },
        );
        let ctx = PluginContext {
            plugin_name: "myplugin".into(),
            ticket_root: PathBuf::from("/tickets/JIRA-1"),
            current_working_dir: PathBuf::from("/tickets/JIRA-1/api"),
            current_repo_alias: Some("api".into()),
            current_repo_path: Some(PathBuf::from("/tickets/JIRA-1/api")),
            ticket: TicketMetadata {
                id: "JIRA-1".into(),
                description: Some("Test".into()),
                created_at: "2024-01-01T00:00:00Z".into(),
                branch: "feature/JIRA-1-test".into(),
                repos: vec!["api".into()],
                repo_branches: HashMap::new(),
                repo_worktrees: HashMap::new(),
            },
            config: Config {
                branch_prefix: "feature".into(),
                github_base_url: "https://github.com".into(),
                default_repository_owner: "my-org".into(),
                code_directory: PathBuf::from("/code"),
                tickets_directory: PathBuf::from("/tickets"),
                repositories: HashMap::new(),
                plugins: HashMap::new(),
                jira_base_url: None,
            },
            code_directory: PathBuf::from("/code"),
            tickets_directory: PathBuf::from("/tickets"),
            plugin_cache_dir: PathBuf::from("/cache/tix/plugins/myplugin"),
            plugin_state_dir: PathBuf::from("/state/tix/plugins/myplugin"),
            plugin_ticket_state_dir: PathBuf::from("/tickets/JIRA-1/.tix/plugins/myplugin"),
            repositories: repos,
        };
        let serialized = serde_json::to_string(&ctx).unwrap();
        assert!(serialized.contains("\"ticket_root\""));
        assert!(serialized.contains("\"repositories\""));
    }

    #[test]
    fn plugin_state_dir_uses_xdg_cache_home() {
        static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap();

        let temp = tempfile::TempDir::new().unwrap();
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", temp.path());
        }

        let cache_dir = super::plugin_cache_dir("my-plugin", true).unwrap();
        assert!(cache_dir.starts_with(temp.path()));
        assert!(cache_dir.ends_with("tix/plugins/my-plugin"));
        assert!(cache_dir.exists());

        unsafe {
            std::env::remove_var("XDG_CACHE_HOME");
        }
    }

    #[test]
    fn plugin_state_dir_uses_xdg_state_home() {
        static ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap();

        let temp = tempfile::TempDir::new().unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", temp.path());
        }

        let state_dir = super::plugin_state_dir("my-plugin", true).unwrap();
        assert!(state_dir.starts_with(temp.path()));
        assert!(state_dir.ends_with("tix/plugins/my-plugin"));
        assert!(state_dir.exists());

        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn plugin_ticket_state_dir_creates_under_ticket() {
        let temp = tempfile::TempDir::new().unwrap();
        let ticket_root = temp.path().join("JIRA-1");
        std::fs::create_dir_all(&ticket_root).unwrap();

        let ticket_state = super::plugin_ticket_state_dir(&ticket_root, "my-plugin", true).unwrap();

        assert!(ticket_state.ends_with(".tix/plugins/my-plugin"));
        assert!(ticket_state.exists());
    }

    #[test]
    fn detect_current_repo_returns_match() {
        let ticket_root = Path::new("/tickets/JIRA-1");
        let ticket = TicketMetadata {
            id: "JIRA-1".into(),
            description: None,
            created_at: "2024-01-01T00:00:00Z".into(),
            branch: "feature/JIRA-1".into(),
            repos: vec!["api".into(), "web".into()],
            repo_branches: HashMap::new(),
            repo_worktrees: HashMap::new(),
        };
        let cwd = Path::new("/tickets/JIRA-1/api/src");

        let (alias, path) = super::detect_current_repo(ticket_root, &ticket, cwd);
        assert_eq!(alias, Some("api".into()));
        assert_eq!(path, Some(PathBuf::from("/tickets/JIRA-1/api")));
    }

    #[test]
    fn find_uv_project_root_finds_parent_pyproject() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_root = temp.path().join("plugin");
        let nested = project_root.join("src");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            project_root.join("pyproject.toml"),
            "[project]\nname = \"demo\"",
        )
        .unwrap();

        let entrypoint = nested.join("plugin.py");
        std::fs::write(&entrypoint, "print('hi')").unwrap();

        let root = find_uv_project_root(&entrypoint).unwrap();
        assert_eq!(root, project_root);
    }

    #[test]
    fn find_uv_project_root_errors_without_pyproject() {
        let temp = tempfile::TempDir::new().unwrap();
        let entrypoint = temp.path().join("plugin.py");
        std::fs::write(&entrypoint, "print('hi')").unwrap();

        let err = find_uv_project_root(&entrypoint).unwrap_err();
        assert!(
            err.to_string()
                .contains("No pyproject.toml found for plugin")
        );
    }
}
