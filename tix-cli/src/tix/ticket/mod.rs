pub mod add;
pub mod destroy;
pub mod info;
pub mod list;
pub mod remove;
pub mod setup;

// ---

use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;

/// A `--ticket` argument: one flag, two forms, disambiguated by shape
/// (`design/spec.md` §4).
///
/// - **Path** — the argument contains a path separator, is absolute, or is
///   `.`/`..`. Asserts *this path is the ticket root*.
/// - **Id** — any bare name, resolved against the configured tickets
///   directory.
///
/// A bare name is always an id; a ticket directory in cwd must be written
/// `./NAME`. See [`crate::tix::discovery::resolve_override`] for the
/// resolution semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketRef {
    /// An asserted ticket-root path (`./NAME`, `some/dir`, `/abs/path`, `.`, `..`).
    Path(PathBuf),
    /// A bare ticket id, resolved as `tickets_directory.join(id)`.
    Id(String),
}
impl FromStr for TicketRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = std::path::Path::new(s);
        let is_path_shape =
            s == "." || s == ".." || path.is_absolute() || s.contains(['/', '\\']);
        if is_path_shape {
            Ok(TicketRef::Path(path.to_path_buf()))
        } else {
            Ok(TicketRef::Id(s.to_string()))
        }
    }
}

#[cfg(test)]
mod ticket_ref_tests {
    use super::*;

    /// Absolute paths, `.`/`..`, and anything containing a separator parse as
    /// the path form.
    #[test]
    fn test_path_shapes() {
        for s in ["/abs/path", "./NAME", "../up", ".", "..", "some/dir"] {
            assert!(
                matches!(s.parse::<TicketRef>().unwrap(), TicketRef::Path(_)),
                "expected path form for {s:?}"
            );
        }
    }

    /// A bare name is always an id — a ticket directory in cwd must be
    /// written `./NAME`.
    #[test]
    fn test_bare_name_is_id() {
        for s in ["JIRA-123", "my-ticket", "NAME"] {
            assert!(
                matches!(s.parse::<TicketRef>().unwrap(), TicketRef::Id(_)),
                "expected id form for {s:?}"
            );
        }
    }
}

#[derive(Args)]
pub struct TicketArgs {
    #[command(subcommand)]
    pub command: TicketCommands,
}

#[derive(Args, Debug)]
pub struct TicketSharedArgs {
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    pub ticket: Option<TicketRef>,
}

#[derive(Subcommand)]
pub enum TicketCommands {
    Add(add::Args),
    Destroy(destroy::Args),
    Info(info::Args),
    List(list::Args),
    Remove(remove::Args),
    Setup(setup::Args),
}
