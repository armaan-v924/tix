//! Plugin discovery for `tix --help` — the "Plugins" section
//! (`design/spec.md` §5.6).
//!
//! Listing is best-effort by contract: a plugin whose handshake is missing,
//! broken, slow, or malicious degrades to its bare name. **The listing
//! itself never fails**, and handshake output is treated as untrusted input.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::debug;

/// How long one `print-cli-help` handshake may run before it is killed.
const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(300);

/// Longest description rendered next to a plugin name.
const DESCRIPTION_CAP: usize = 100;

/// Builds the "Plugins:" help section, or `None` when no plugins are on
/// `PATH` — zero plugins simply omits the section.
pub fn plugins_help_section() -> Option<String> {
    let plugins = discover_plugins();
    if plugins.is_empty() {
        return None;
    }

    let width = plugins.keys().map(String::len).max().unwrap_or(0);
    let mut section = String::from("Plugins:\n");
    for (name, path) in &plugins {
        match handshake_description(path) {
            Some(description) => {
                section.push_str(&format!("  {name:width$}  {description}\n"));
            }
            None => section.push_str(&format!("  {name}\n")),
        }
    }
    section.pop(); // trailing newline
    Some(section)
}

/// Scans every `PATH` directory for executables named `tix-*`.
///
/// Deduplicated by name with the **first** match on `PATH` winning (shell
/// semantics); the `tix-` prefix is stripped for display. Returned sorted by
/// name (BTreeMap) for stable output.
fn discover_plugins() -> BTreeMap<String, PathBuf> {
    let mut plugins: BTreeMap<String, PathBuf> = BTreeMap::new();
    let Some(path_var) = std::env::var_os("PATH") else {
        return plugins;
    };
    for dir in std::env::split_paths(&path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            let Some(stripped) = name.strip_prefix("tix-") else {
                continue;
            };
            if stripped.is_empty() || !is_executable_file(&entry.path()) {
                continue;
            }
            // First match on PATH wins; later directories don't override.
            plugins
                .entry(stripped.to_string())
                .or_insert_with(|| entry.path());
        }
    }
    plugins
}

/// Runs `<plugin> print-cli-help` **bare** — no `--tix-*` flags — and
/// returns a sanitized one-line description.
///
/// Any failure shape (spawn error, nonzero exit, timeout, empty or garbage
/// output) degrades to `None`: the plugin still lists by name.
fn handshake_description(plugin: &Path) -> Option<String> {
    let mut child = std::process::Command::new(plugin)
        .arg("print-cli-help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Poll rather than block: a hung plugin must not hang `tix --help`.
    let deadline = Instant::now() + HANDSHAKE_DEADLINE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                debug!(plugin = %plugin.display(), "handshake timed out or errored — killing");
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }

    // Bounded read: output is untrusted, so never slurp unbounded bytes.
    let mut raw = Vec::with_capacity(512);
    child
        .stdout
        .take()?
        .take(4096)
        .read_to_end(&mut raw)
        .ok()?;
    sanitize(&raw)
}

/// Strips the handshake output down to one printable line, capped.
fn sanitize(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let clean: String = line
        .chars()
        .filter(|c| !c.is_control())
        .take(DESCRIPTION_CAP)
        .collect();
    let clean = clean.trim().to_string();
    (!clean.is_empty()).then_some(clean)
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One printable line survives; control characters and later lines don't.
    #[test]
    fn test_sanitize_strips_and_caps() {
        assert_eq!(
            sanitize(b"deploys things\nsecond line ignored").as_deref(),
            Some("deploys things")
        );
        assert_eq!(
            sanitize(b"\x1b[31mred\x07 alert\x1b[0m").as_deref(),
            Some("[31mred alert[0m")
        );
        assert_eq!(sanitize(b"").as_deref(), None);
        assert_eq!(sanitize(b"\n\n   \n").as_deref(), None);
        let long = vec![b'a'; 500];
        assert_eq!(sanitize(&long).unwrap().len(), DESCRIPTION_CAP);
    }
}
