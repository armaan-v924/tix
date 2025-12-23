//! Plugin management commands.

use crate::core::config::{Config, PluginDefinition};
use crate::core::plugins;
use anyhow::{Context, Result, bail};
use log::info;
use std::env;
use std::path::PathBuf;

/// List registered plugins.
pub fn list() -> Result<()> {
    let plugins = plugins::list_plugins()?;
    if plugins.is_empty() {
        info!("No plugins registered.");
        return Ok(());
    }

    for (name, plugin) in plugins {
        if plugin.description.trim().is_empty() {
            info!("{} ({})", name, plugin.entrypoint.display());
        } else {
            info!("{} - {}", name, plugin.description);
        }
    }
    Ok(())
}

/// Register a plugin definition in config.
pub fn register(
    name: &str,
    entrypoint: &str,
    description: Option<&str>,
    python: Option<&str>,
) -> Result<()> {
    let mut config = Config::load()?;
    if config.plugins.contains_key(name) {
        bail!("Plugin '{}' is already registered", name);
    }

    let entrypoint_path = resolve_entrypoint_path(entrypoint)?;

    let plugin = PluginDefinition {
        entrypoint: entrypoint_path,
        description: description.unwrap_or_default().to_string(),
        python: python.map(|p| p.to_string()),
    };
    config.plugins.insert(name.to_string(), plugin);
    config.save()?;
    info!("Registered plugin '{}'", name);
    Ok(())
}

fn resolve_entrypoint_path(entrypoint: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(entrypoint);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir()
            .context("Failed to resolve current directory")?
            .join(candidate)
    };

    if !absolute.exists() {
        bail!("Entrypoint '{}' does not exist", entrypoint);
    }
    if absolute.is_dir() {
        bail!("Entrypoint '{}' is a directory", entrypoint);
    }

    let canonical = std::fs::canonicalize(&absolute).with_context(|| {
        format!(
            "Failed to resolve entrypoint '{}' to an absolute path",
            entrypoint
        )
    })?;
    Ok(canonical)
}

/// Remove a plugin registration and clear its cache.
pub fn deregister(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    if config.plugins.remove(name).is_none() {
        bail!("Plugin '{}' is not registered", name);
    }
    config.save()?;
    let removed = plugins::remove_plugin_cache(name)?;
    if removed {
        info!("Removed plugin '{}' and cleared cache", name);
    } else {
        info!("Removed plugin '{}' (no cache found)", name);
    }
    Ok(())
}

/// Clear plugin caches. When name is None, clears all plugin caches.
pub fn clean(name: Option<&str>) -> Result<()> {
    match name {
        Some(plugin) => {
            let removed = plugins::remove_plugin_cache(plugin)?;
            if removed {
                info!("Cleared plugin cache for '{}'", plugin);
            } else {
                info!("No cache found for '{}'", plugin);
            }
        }
        None => {
            let mut removed_any = false;
            let root = plugins::plugin_cache_root()?.join("plugins");
            if root.exists() {
                for entry in std::fs::read_dir(&root)? {
                    let entry = entry?;
                    if entry.path().is_dir() {
                        std::fs::remove_dir_all(entry.path())?;
                        removed_any = true;
                    }
                }
            }
            if removed_any {
                info!("Cleared all plugin caches");
            } else {
                info!("No plugin caches found");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::register;
    use crate::core::config::Config;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn register_resolves_relative_entrypoint_to_absolute() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let config_root = temp.path().join("config");
        let plugin_root = temp.path().join("plugin");
        fs::create_dir_all(&config_root).unwrap();
        fs::create_dir_all(&plugin_root).unwrap();

        let entrypoint = plugin_root.join("plugin.py");
        fs::write(&entrypoint, "print('hi')").unwrap();

        let original_cwd = std::env::current_dir().unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &config_root);
        }
        std::env::set_current_dir(&plugin_root).unwrap();

        register("my-plugin", "plugin.py", None, None).unwrap();

        let config = Config::load().unwrap();
        let plugin = config.plugins.get("my-plugin").unwrap();
        assert!(plugin.entrypoint.is_absolute());
        assert_eq!(
            plugin.entrypoint,
            PathBuf::from(entrypoint.canonicalize().unwrap())
        );

        std::env::set_current_dir(original_cwd).unwrap();
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
