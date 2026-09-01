//! Plugin state directory helpers
//! ([config vs state](https://tix.armaanv.dev/latest/plugins/specification/#plugin-state-vs-plugin-config)).
//!
//! **State is not config.** Config is human-editable settings in a
//! `[<plugin>]` table, read via section accessors and written via diff-back.
//! State is caches and derived data of any shape or size: plain files in a
//! directory, part of no document, no delta, and no protocol.
//!
//! Locations are SDK helpers, not contract flags — no env vars, no
//! `--tix-*` flags. Consistency comes from every plugin calling the same
//! helper, not from the host passing values. Directories are created
//! **lazily, on first use** (v2 pre-created them every invocation, littering
//! empty dirs for plugins that never stored anything).

use crate::error::SdkError;
use std::path::{Path, PathBuf};

/// Per-ticket state directory: `<ticket_root>/.tix/plugins/<name>/`,
/// created on call.
///
/// The only genuinely tix-shaped location — a plugin cannot locate a ticket
/// without being told where it is (`--tix-ticket`).
///
/// # Errors
///
/// [`SdkError::Engine`]-wrapped IO error if creation fails.
pub fn ticket_state_dir(ticket_root: &Path, plugin: &str) -> Result<PathBuf, SdkError> {
    let dir = ticket_root.join(".tix").join("plugins").join(plugin);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Global cache directory for a plugin, from the platform cache location;
/// created on call. Offered as convenience only — there is no consistency
/// benefit to tix mediating what `dirs::cache_dir()` already names.
///
/// # Errors
///
/// [`SdkError::Message`] when the platform cache directory cannot be
/// determined; IO errors if creation fails.
pub fn cache_dir(plugin: &str) -> Result<PathBuf, SdkError> {
    let base = dirs::cache_dir().ok_or_else(|| {
        SdkError::Message("cannot determine the platform cache directory".to_string())
    })?;
    let dir = base.join("tix").join(plugin);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The per-ticket dir lands under .tix/plugins/<name> and is created
    /// lazily — only when asked for.
    #[test]
    fn test_ticket_state_dir_lazy_creation() {
        let root = tempdir().unwrap();
        assert!(!root.path().join(".tix").exists());

        let dir = ticket_state_dir(root.path(), "myplugin").unwrap();
        assert_eq!(dir, root.path().join(".tix/plugins/myplugin"));
        assert!(dir.is_dir());

        // Idempotent.
        assert_eq!(ticket_state_dir(root.path(), "myplugin").unwrap(), dir);
    }
}
