//! Nested `tix` invocation helper
//! ([contract](https://tix.armaanv.dev/latest/plugins/specification/#nested-tix-invocations)).
//!
//! A plugin may shell out to `tix`, but a nested host re-discovers from cwd
//! — which the plugin may have changed — so nested `tix` can silently
//! resolve a *different ticket*. Correct (cwd is the interface) but a
//! footgun for authors who mean "same ticket". The supported path is
//! skew-proof by construction: [`tix_command`] pins the current ticket.

use std::path::Path;
use std::process::Command;

/// Builds a `Command` for a nested `tix` invocation pinned to `ticket_root`.
///
/// The returned command already carries `--ticket <path>`, so it resolves
/// the same ticket regardless of what cwd has become. Callers add their
/// subcommand and args and spawn as usual.
///
/// The fork-bomb guard (`TIX_DEPTH`) is the *host's* job on dispatch —
/// nothing to set here.
///
/// # Examples
///
/// ```no_run
/// # use std::path::Path;
/// let mut command = tix_sdk::spawn::tix_command(Path::new("/home/user/tickets/JIRA-1"));
/// command.args(["ticket", "info"]);
/// let status = command.status().expect("tix not on PATH?");
/// ```
pub fn tix_command(ticket_root: &Path) -> Command {
    let mut command = Command::new("tix");
    command.arg("--ticket").arg(ticket_root);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pin rides at the front of the arg list.
    #[test]
    fn test_pins_ticket() {
        let command = tix_command(Path::new("/tickets/JIRA-1"));
        let args: Vec<_> = command
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(args, vec!["--ticket", "/tickets/JIRA-1"]);
        assert_eq!(command.get_program(), "tix");
    }
}
