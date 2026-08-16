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

/// Derives the branch name seeded into a new worktree:
/// `<prefix>/<key>-<sanitized-description>` (v2 parity, spec §3.4).
///
/// The prefix and the description are each optional and drop out cleanly:
/// no prefix means no `<prefix>/`, no (or fully unsanitizable) description
/// means the key alone. Derivation happens **once** — at `tix ticket setup`
/// or `tix ticket add` — and the result is written into the ticket document;
/// later changes to `[defaults]` never rename an existing branch.
///
/// # Examples
///
/// ```text
/// derive_branch_name(Some("feature"), "JIRA-123", Some("Fix the Login Bug!"))
///     == "feature/JIRA-123-fix-the-login-bug"
/// derive_branch_name(None, "JIRA-123", None) == "JIRA-123"
/// ```
pub fn derive_branch_name(prefix: Option<&str>, key: &str, description: Option<&str>) -> String {
    let sanitized = description.map(sanitize_description).unwrap_or_default();
    let stem = if sanitized.is_empty() {
        key.to_string()
    } else {
        format!("{key}-{sanitized}")
    };
    match prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix}/{stem}"),
        _ => stem,
    }
}

/// Lowercases, maps every non-alphanumeric run to a single `-`, trims the
/// ends, and caps the result at 40 characters (without splitting a word run
/// mid-hyphen cleanup).
fn sanitize_description(description: &str) -> String {
    const MAX_LENGTH: usize = 40;
    let mut out = String::with_capacity(description.len());
    for c in description.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    let capped = trimmed.chars().take(MAX_LENGTH).collect::<String>();
    capped.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod branch_name_tests {
    use super::*;

    /// Full derivation: prefix, key, sanitized description.
    #[test]
    fn test_full_derivation() {
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", Some("Fix the Login Bug!")),
            "feature/JIRA-123-fix-the-login-bug"
        );
    }

    /// Prefix and description each drop out cleanly when absent.
    #[test]
    fn test_optional_parts_drop_out() {
        assert_eq!(
            derive_branch_name(None, "JIRA-123", Some("desc")),
            "JIRA-123-desc"
        );
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", None),
            "feature/JIRA-123"
        );
        assert_eq!(derive_branch_name(None, "JIRA-123", None), "JIRA-123");
        assert_eq!(derive_branch_name(Some(""), "JIRA-123", None), "JIRA-123");
    }

    /// Sanitization collapses punctuation runs, trims, and caps length.
    #[test]
    fn test_sanitization() {
        assert_eq!(sanitize_description("Fix: the (login) bug!!"), "fix-the-login-bug");
        assert_eq!(sanitize_description("---"), "");
        assert_eq!(
            sanitize_description(
                "a very long description that goes on and on and on far past the cap"
            )
            .len()
                <= 40,
            true
        );
    }

    /// A description that sanitizes to nothing behaves like no description.
    #[test]
    fn test_unsanitizable_description() {
        assert_eq!(
            derive_branch_name(Some("feature"), "JIRA-123", Some("!!!")),
            "feature/JIRA-123"
        );
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
