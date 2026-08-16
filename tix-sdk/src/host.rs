//! The plugin side of the invocation contract (`design/spec.md` §5).
//!
//! A plugin binary's first act is [`HostContext::from_env`]: it answers the
//! bare `print-cli-help` handshake, strips every `--tix-*` flag out of argv,
//! checks the protocol, and hands back settled host values plus untouched
//! user args. The host resolved its globals before forwarding — plugins MUST
//! NOT reimplement flag precedence.

use crate::error::SdkError;
use std::path::PathBuf;

/// The protocol integer this SDK speaks.
///
/// Monotonic, starting at 1, independent of crate versions. Bumped **only**
/// for removal, rename, or semantic change of an existing flag or document —
/// never for additions (unknown `--tix-*` flags are ignored, so additions
/// are safe by construction and flag presence doubles as capability
/// detection). The version → change table lives in `tix-sdk/PROTOCOL.md`.
pub const PROTOCOL: u64 = 1;

/// Exit code reserved for protocol mismatch (`design/spec.md` §6.4).
///
/// The established tool-layer-error slot (`docker run`, `timeout(1)`,
/// `git bisect run` skip); excluded from the host's propagated range.
pub const PROTOCOL_MISMATCH_EXIT: i32 = 125;

/// Settled host values handed to a plugin, plus the user's own args.
///
/// Paths are the real files, not staged copies — a plugin can be hand-run
/// against arbitrary paths for debugging.
#[derive(Debug, Clone, PartialEq)]
pub struct HostContext {
    /// Path to the global config file.
    pub config_path: PathBuf,
    /// Path to the ticket directory (parent of `.tix`) — present only when
    /// the host ran inside a ticket. The absence is load-bearing:
    /// `tix ticket setup` creates tickets, so it runs without one.
    pub ticket_root: Option<PathBuf>,
    /// Host-created temp file for the outbound config delta. Stdout cannot
    /// carry it — stdout belongs to the user. No file written ⇒ no changes.
    pub delta_path: Option<PathBuf>,
    /// Alias of the repo worktree cwd is inside, when it is.
    pub repo: Option<String>,
    /// Path of that worktree.
    pub repo_dir: Option<PathBuf>,
    /// The host's resolved log level.
    pub log_level: Option<String>,
    /// The host's resolved output format (`json`/`toml`/`default`).
    pub output: Option<String>,
    /// The host's resolved color decision for this process.
    pub color: Option<bool>,
    /// Everything that was not a `--tix-*` flag, in order — the user's args,
    /// untouched.
    pub user_args: Vec<String>,
}

impl HostContext {
    /// Parses the process's own argv. See [`Self::from_args`].
    ///
    /// # Errors
    ///
    /// Same as [`Self::from_args`].
    pub fn from_env() -> Result<Self, SdkError> {
        Self::from_args(std::env::args().skip(1))
    }

    /// Parses host-injected flags out of `args`.
    ///
    /// - `print-cli-help` as the sole argument is handled **before** any
    ///   host flag is required (it is invoked bare, spec §5.6): the plugin's
    ///   registered description prints and the process exits 0. Register one
    ///   with [`Self::from_args_with_description`].
    /// - Known `--tix-*` flags are collected; **unknown `--tix-*` flags are
    ///   ignored** — additions never break older plugins, and checking
    ///   [`HostContext`] fields doubles as capability detection.
    /// - `--tix-protocol` is compared against [`PROTOCOL`]; a mismatch is a
    ///   *rebuild* situation, reported as such — callers should exit with
    ///   [`PROTOCOL_MISMATCH_EXIT`] (or use [`Self::from_env_or_exit`]).
    ///
    /// # Errors
    ///
    /// - [`SdkError::Message`] on protocol mismatch ("built for protocol N,
    ///   host speaks M — rebuild")
    /// - [`SdkError::Message`] when `--tix-config` is missing — the one flag
    ///   every host invocation carries
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, SdkError> {
        Self::parse(args, None)
    }

    /// [`Self::from_args`], answering `print-cli-help` with `description`.
    pub fn from_args_with_description(
        args: impl IntoIterator<Item = String>,
        description: &str,
    ) -> Result<Self, SdkError> {
        Self::parse(args, Some(description))
    }

    /// [`Self::from_env`], turning errors into the contract's process exits:
    /// protocol mismatch exits [`PROTOCOL_MISMATCH_EXIT`], anything else
    /// prints to stderr and exits 1. The convenience entry point for plugin
    /// `main`s.
    pub fn from_env_or_exit(description: &str) -> Self {
        match Self::parse(std::env::args().skip(1), Some(description)) {
            Ok(context) => context,
            Err(SdkError::Message(message)) if message.contains("— rebuild") => {
                eprintln!("error: {message}");
                std::process::exit(PROTOCOL_MISMATCH_EXIT);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }

    fn parse(
        args: impl IntoIterator<Item = String>,
        description: Option<&str>,
    ) -> Result<Self, SdkError> {
        let args: Vec<String> = args.into_iter().collect();

        // The handshake is answered before any host flag is required.
        if description.is_some() && args.iter().map(String::as_str).eq(["print-cli-help"]) {
            println!("{}", description.unwrap_or_default());
            std::process::exit(0);
        }

        let mut protocol: Option<u64> = None;
        let mut config_path: Option<PathBuf> = None;
        let mut ticket_root: Option<PathBuf> = None;
        let mut delta_path: Option<PathBuf> = None;
        let mut repo: Option<String> = None;
        let mut repo_dir: Option<PathBuf> = None;
        let mut log_level: Option<String> = None;
        let mut output: Option<String> = None;
        let mut color: Option<bool> = None;
        let mut user_args: Vec<String> = Vec::new();

        let mut iter = args.into_iter().peekable();
        while let Some(arg) = iter.next() {
            // The whole --tix-* prefix is reserved for the host; everything
            // else passes through untouched.
            let Some(flag) = arg.strip_prefix("--tix-") else {
                user_args.push(arg);
                continue;
            };
            // Both `--tix-flag value` and `--tix-flag=value` forms parse.
            let (name, value) = match flag.split_once('=') {
                Some((name, value)) => (name.to_string(), Some(value.to_string())),
                None => (flag.to_string(), iter.next()),
            };
            let value = value.unwrap_or_default();
            match name.as_str() {
                "protocol" => protocol = value.parse().ok(),
                "config" => config_path = Some(PathBuf::from(value)),
                "ticket" => ticket_root = Some(PathBuf::from(value)),
                "delta" => delta_path = Some(PathBuf::from(value)),
                "repo" => repo = Some(value),
                "repo-dir" => repo_dir = Some(PathBuf::from(value)),
                "log-level" => log_level = Some(value),
                "output" => output = Some(value),
                "color" => color = value.parse().ok(),
                // Unknown --tix-* flags are ignored by contract: additions
                // are safe with no protocol bump.
                _ => {}
            }
        }

        if let Some(sent) = protocol
            && sent != PROTOCOL
        {
            return Err(SdkError::Message(format!(
                "built for protocol {PROTOCOL}, host speaks {sent} — rebuild"
            )));
        }

        let config_path = config_path.ok_or_else(|| {
            SdkError::Message(
                "no --tix-config flag: this binary is a tix plugin, run it through `tix <name>` \
                 (or pass --tix-config <path> by hand for debugging)"
                    .to_string(),
            )
        })?;

        Ok(Self {
            config_path,
            ticket_root,
            delta_path,
            repo,
            repo_dir,
            log_level,
            output,
            color,
            user_args,
        })
    }

    /// The ticket root, for plugins that require ticket context.
    ///
    /// # Errors
    ///
    /// [`SdkError::Message`] with a user-facing "run inside a ticket" error
    /// when the host forwarded none.
    pub fn require_ticket(&self) -> Result<&PathBuf, SdkError> {
        self.ticket_root.as_ref().ok_or_else(|| {
            SdkError::Message(
                "this command requires ticket context — run it from inside a ticket \
                 or pass --ticket <path|id>"
                    .to_string(),
            )
        })
    }
}

/// Tix's own global flags, pre-scanned out of raw forwarded args.
///
/// `external_subcommand` captures everything after a plugin name raw, so
/// `tix foo --verbose` leaves the parsed globals untouched (spec §5.3). The
/// host pre-scans the raw args with this — the same code that defines the
/// SDK's view of the contract — resolves settled values, and forwards those;
/// the matched flags are consumed (they are tix's, not the plugin's).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PrescannedGlobals {
    /// `-v` / `--verbose` seen.
    pub verbose: bool,
    /// `-q` / `--quiet` seen.
    pub quiet: bool,
    /// `--log-level <level>` value.
    pub log_level: Option<String>,
    /// `-o` / `--output <format>` value.
    pub output: Option<String>,
    /// `--config <path>` value.
    pub config: Option<PathBuf>,
}

/// Splits raw forwarded args into tix's own globals and everything else.
///
/// Everything unmatched — including unknown flags — passes through in order
/// as the plugin's user args.
pub fn prescan_globals(args: &[String]) -> (PrescannedGlobals, Vec<String>) {
    let mut globals = PrescannedGlobals::default();
    let mut user_args = Vec::with_capacity(args.len());
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        let mut take_value = |inline: Option<&str>| -> Option<String> {
            inline
                .map(str::to_string)
                .or_else(|| iter.next().cloned())
        };
        match arg.split_once('=') {
            _ if arg == "-v" || arg == "--verbose" => globals.verbose = true,
            _ if arg == "-q" || arg == "--quiet" => globals.quiet = true,
            Some(("--log-level", value)) => globals.log_level = Some(value.to_string()),
            Some(("--output", value)) | Some(("-o", value)) => {
                globals.output = Some(value.to_string())
            }
            Some(("--config", value)) => globals.config = Some(PathBuf::from(value)),
            None if arg == "--log-level" => globals.log_level = take_value(None),
            None if arg == "--output" || arg == "-o" => globals.output = take_value(None),
            None if arg == "--config" => globals.config = take_value(None).map(PathBuf::from),
            _ => user_args.push(arg.clone()),
        }
    }
    (globals, user_args)
}

#[cfg(test)]
mod prescan_tests {
    use super::*;

    /// Globals are pulled out wherever they appear; the rest passes through
    /// in order.
    #[test]
    fn test_prescan_extracts_globals() {
        let args: Vec<String> = ["deploy", "--verbose", "--output", "json", "target", "--force"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (globals, user_args) = prescan_globals(&args);
        assert!(globals.verbose);
        assert_eq!(globals.output.as_deref(), Some("json"));
        assert_eq!(user_args, vec!["deploy", "target", "--force"]);
    }

    /// Unknown flags are the plugin's business.
    #[test]
    fn test_prescan_leaves_unknown_flags() {
        let args: Vec<String> = ["--frobnicate", "--log-level=debug"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (globals, user_args) = prescan_globals(&args);
        assert_eq!(globals.log_level.as_deref(), Some("debug"));
        assert_eq!(user_args, vec!["--frobnicate"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> + use<> {
        list.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Host flags are stripped, user args survive untouched and in order.
    #[test]
    fn test_strips_host_flags_keeps_user_args() {
        let context = HostContext::from_args(args(&[
            "--tix-protocol",
            "1",
            "--tix-config",
            "/cfg.toml",
            "--tix-delta=/tmp/delta.json",
            "sync",
            "--verbose",
            "some-file",
        ]))
        .unwrap();
        assert_eq!(context.config_path, PathBuf::from("/cfg.toml"));
        assert_eq!(context.delta_path, Some(PathBuf::from("/tmp/delta.json")));
        assert_eq!(context.user_args, vec!["sync", "--verbose", "some-file"]);
    }

    /// Unknown --tix-* flags are ignored — additions need no protocol bump.
    #[test]
    fn test_ignores_unknown_tix_flags() {
        let context = HostContext::from_args(args(&[
            "--tix-config",
            "/cfg.toml",
            "--tix-shiny-new-flag",
            "whatever",
            "user-arg",
        ]))
        .unwrap();
        assert_eq!(context.user_args, vec!["user-arg"]);
    }

    /// A protocol mismatch reports "rebuild", not a parse failure.
    #[test]
    fn test_protocol_mismatch() {
        let err = HostContext::from_args(args(&[
            "--tix-protocol",
            "999",
            "--tix-config",
            "/cfg.toml",
        ]))
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("built for protocol 1"), "{message}");
        assert!(message.contains("host speaks 999"), "{message}");
        assert!(message.contains("rebuild"), "{message}");
    }

    /// Ticket context is optional and its absence is load-bearing.
    #[test]
    fn test_ticket_optional() {
        let context =
            HostContext::from_args(args(&["--tix-config", "/cfg.toml"])).unwrap();
        assert!(context.ticket_root.is_none());
        assert!(context.require_ticket().is_err());

        let context = HostContext::from_args(args(&[
            "--tix-config",
            "/cfg.toml",
            "--tix-ticket",
            "/tickets/JIRA-1",
        ]))
        .unwrap();
        assert_eq!(
            context.require_ticket().unwrap(),
            &PathBuf::from("/tickets/JIRA-1")
        );
    }

    /// Missing --tix-config points at running through the host.
    #[test]
    fn test_missing_config_flag() {
        let err = HostContext::from_args(args(&["user-arg"])).unwrap_err();
        assert!(err.to_string().contains("tix plugin"));
    }
}
