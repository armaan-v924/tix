//! Plugin dispatch: exec `tix-<name>` with the invocation contract
//! ([contract](https://tix.armaanv.dev/latest/plugins/specification/#1-the-exec-contract)).
//!
//! Unknown subcommands land here via the clap catch-all — builtins always
//! win, so dispatch is only ever reached for non-builtin names. There is no
//! plugin registry: `PATH` is the registry.

use crate::tix::config::CliConfig;
use crate::tix::utils::App;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tix_sdk::delta::{Delta, DeltaTarget};
use tix_sdk::document::with_write;
use tix_sdk::host::{PROTOCOL, PROTOCOL_MISMATCH_EXIT, prescan_globals};
use tix_sdk::{Defaults, EngineConfig, SdkError, TicketConfig};
use tracing::debug;

/// Recursion guard: `tix-foo` shelling `tix foo` forever is the one failure
/// mode that damages more than tix (spec §5.5).
const DEPTH_ENV: &str = "TIX_DEPTH";
const DEPTH_CAP: u32 = 10;

/// Dispatches `tix <name> <args…>` to the `tix-<name>` binary on `PATH`.
///
/// The host resolves its own globals **before** forwarding — the raw args
/// are pre-scanned (spec §5.3, the same SDK code plugins build on) and the
/// plugin receives settled `--tix-*` values, never raw flags. stdin/stdout/
/// stderr are inherited: the plugin owns the terminal. The child's exit code
/// propagates unmodified, except **125**, which is reserved for protocol
/// mismatch and reported as a versioning problem instead.
///
/// On exit 0 the `--tix-delta` file is handed to the diff-back apply path;
/// a nonzero exit discards it.
pub fn run(app: &App, args: Vec<String>) -> Result<(), SdkError> {
    let mut args = args.into_iter();
    let name = args
        .next()
        .ok_or_else(|| SdkError::Message("no plugin name in external args".to_string()))?;
    let raw_args: Vec<String> = args.collect();

    let Some(binary) = find_plugin(&name) else {
        return Err(SdkError::Message(format!(
            "unknown command '{name}': not a builtin, and no 'tix-{name}' executable on PATH"
        )));
    };

    // Fork-bomb guard: increment TIX_DEPTH, hard-fail past the cap.
    let depth: u32 = std::env::var(DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if depth >= DEPTH_CAP {
        return Err(SdkError::Message(format!(
            "tix invocation depth {depth} exceeds the cap of {DEPTH_CAP} — \
             a plugin is probably re-invoking itself"
        )));
    }

    // Pre-scan the raw args for tix's own globals (external_subcommand
    // captured them raw), then settle values for forwarding.
    let (globals, user_args) = prescan_globals(&raw_args);
    let log_level = globals
        .log_level
        .or_else(|| globals.verbose.then(|| "trace".to_string()))
        .or_else(|| globals.quiet.then(|| "warn".to_string()))
        .unwrap_or_else(|| app.log_level.to_string().to_lowercase());
    let output = globals
        .output
        .unwrap_or_else(|| app.output.as_str().to_string());
    // Per-process color resolution: the plugin's stdout is ours (inherited).
    let color = std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();

    // Ticket context from the discovery walk; its absence is forwarded as an
    // absent flag, which is load-bearing (setup-shaped plugins run without).
    let ticket_root = tix_sdk::discovery::discover_ticket_root()?;
    let repo_context = ticket_root
        .as_deref()
        .and_then(|root| detect_current_repo(root).transpose())
        .transpose()?;

    // Host-created temp file for the outbound delta (spec §5.2). Kept alive
    // until after the wait; deleted on drop.
    let delta_file = tempfile::NamedTempFile::new().map_err(SdkError::from)?;

    let mut command = std::process::Command::new(&binary);
    command
        .arg("--tix-protocol")
        .arg(PROTOCOL.to_string())
        .arg("--tix-config")
        .arg(&app.context.config_path)
        .arg("--tix-delta")
        .arg(delta_file.path())
        .arg("--tix-log-level")
        .arg(&log_level)
        .arg("--tix-output")
        .arg(&output)
        .arg("--tix-color")
        .arg(color.to_string())
        .env(DEPTH_ENV, (depth + 1).to_string());
    if let Some(root) = &ticket_root {
        command.arg("--tix-ticket").arg(root);
    }
    if let Some((alias, path)) = &repo_context {
        command.arg("--tix-repo").arg(alias);
        command.arg("--tix-repo-dir").arg(path);
    }
    command.args(&user_args);

    debug!(plugin = %binary.display(), "dispatching");
    let status = command
        .status()
        .map_err(|e| SdkError::Message(format!("could not exec '{}': {e}", binary.display())))?;

    match status.code() {
        // The delta is applied only on exit 0; a failed plugin's
        // half-intended writes are discarded with the temp file.
        Some(0) => apply_delta_if_present(app, delta_file.path(), ticket_root.as_deref()),
        Some(PROTOCOL_MISMATCH_EXIT) => {
            // Reserved: report a versioning problem, not a plugin crash, and
            // keep 125 out of the propagated range.
            Err(SdkError::Message(format!(
                "plugin 'tix-{name}' speaks a different tix protocol (host speaks {PROTOCOL}) — \
                 rebuild or update the plugin"
            )))
        }
        Some(code) => {
            // The plugin's error is the plugin's to report; tix adds nothing.
            std::process::exit(code);
        }
        None => Err(SdkError::Message(format!(
            "plugin 'tix-{name}' was terminated by a signal"
        ))),
    }
}

/// First `tix-<name>` executable on `PATH`, shell semantics.
fn find_plugin(name: &str) -> Option<PathBuf> {
    let binary_name = format!("tix-{name}");
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&binary_name))
        .find(|candidate| is_executable(candidate))
}

/// Which repo worktree the cwd is inside, when it is inside one: the
/// longest-prefix match of the logical cwd against `ticket_root/<name>` for
/// every worktree the ticket tracks (v2's `detect_current_repo`).
fn detect_current_repo(
    ticket_root: &std::path::Path,
) -> Result<Option<(String, PathBuf)>, SdkError> {
    let document =
        tix_sdk::document::TixDocument::load(&ticket_root.join(".tix").join("ticket.toml"))?;
    let Some(ticket): Option<TicketConfig> = document.section("ticket")? else {
        return Ok(None);
    };
    let cwd = tix_sdk::discovery::logical_cwd()?;

    let mut best: Option<(String, PathBuf)> = None;
    for name in ticket.worktrees.keys() {
        let worktree_path = ticket_root.join(name);
        let is_better = cwd.starts_with(&worktree_path)
            && best.as_ref().is_none_or(|(_, current)| {
                worktree_path.components().count() > current.components().count()
            });
        if is_better {
            best = Some((ticket.worktrees[name].repo.clone(), worktree_path));
        }
    }
    Ok(best)
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// Applies the plugin's outbound delta, if it wrote one (spec §6).
///
/// The apply happens against a **fresh parse at apply time** under the
/// exclusive lock — never the host's startup snapshot — so it merges with
/// whatever a nested `tix` or a second terminal wrote meanwhile. After the
/// ops land, the host re-deserializes the sections it has types for:
/// a delta that breaks a host-owned section ([engine]/[cli]/[defaults] or
/// [ticket]) is rejected wholesale and nothing is written. Plugin sections
/// pass unvalidated — the host has no schema for them, and the plugin
/// validated its own on the way in. Writes outside the plugin's own section
/// are allowed, unsupported: applied if revalidation passes.
fn apply_delta_if_present(
    app: &App,
    delta_path: &Path,
    ticket_root: Option<&Path>,
) -> Result<(), SdkError> {
    // No file written (or nothing in it) means no changes.
    let bytes = std::fs::read(delta_path).unwrap_or_default();
    if bytes.is_empty() {
        return Ok(());
    }

    let delta = Delta::parse(&bytes)?;
    let target_path = match delta.target {
        DeltaTarget::Global => app.context.config_path.clone(),
        DeltaTarget::Ticket => ticket_root
            .ok_or_else(|| {
                SdkError::PluginImplementation(
                    "delta targets the ticket document, but the plugin ran outside a ticket"
                        .to_string(),
                )
            })?
            .join(".tix")
            .join("ticket.toml"),
    };

    debug!(target = ?delta.target, ops = delta.ops.len(), "applying plugin delta");
    with_write(&target_path, |document| {
        delta.apply_ops(document)?;

        // Revalidate host-owned sections; reject the whole delta if any no
        // longer parse. with_write discards on Err, so nothing is written.
        let reject = |e: SdkError| {
            SdkError::PluginImplementation(format!("delta breaks a host-owned section: {e}"))
        };
        match delta.target {
            DeltaTarget::Global => {
                document.section::<EngineConfig>("engine").map_err(reject)?;
                document.section::<CliConfig>("cli").map_err(reject)?;
                document.section::<Defaults>("defaults").map_err(reject)?;
            }
            DeltaTarget::Ticket => {
                document.section::<TicketConfig>("ticket").map_err(reject)?;
            }
        }
        Ok(())
    })
}
