pub mod add;
pub mod destroy;
pub mod info;
pub mod list;
pub mod remove;
pub mod setup;

// ---

use clap::{Args, Subcommand};
pub use tix_sdk::discovery::TicketRef;

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
        assert_eq!(
            sanitize_description("Fix: the (login) bug!!"),
            "fix-the-login-bug"
        );
        assert_eq!(sanitize_description("---"), "");
        assert!(
            sanitize_description(
                "a very long description that goes on and on and on far past the cap"
            )
            .len()
                <= 40
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

/// Loads the required `[cli]` section from the resolved global config.
///
/// Shared by every ticket subcommand; the "no [cli] section" error points at
/// `tix config init` rather than leaking a parse detail.
pub fn load_cli_config(
    context: &tix_sdk::context::Context,
) -> Result<crate::tix::config::CliConfig, tix_sdk::SdkError> {
    let document = tix_sdk::document::TixDocument::load(&context.config_path)?;
    document.section("cli")?.ok_or_else(|| {
        tix_sdk::SdkError::Message(
            "global config has no [cli] section — run `tix config init`".to_string(),
        )
    })
}

/// Reads the `[ticket]` section of the ticket document at `ticket_root`.
///
/// # Errors
///
/// [`tix_sdk::TixError::TicketNotFound`] (wrapped) when the document has no
/// `[ticket]` section; parse errors propagate from the document layer.
pub fn load_ticket_config(
    ticket_root: &std::path::Path,
) -> Result<tix_sdk::TicketConfig, tix_sdk::SdkError> {
    let document =
        tix_sdk::document::TixDocument::load(&ticket_root.join(".tix").join("ticket.toml"))?;
    document.section("ticket")?.ok_or_else(|| {
        tix_sdk::SdkError::Engine(tix_sdk::TixError::TicketNotFound(format!(
            "{} has no [ticket] section",
            ticket_root.join(".tix/ticket.toml").display()
        )))
    })
}

/// Resolves the ticket a command operates on — `--ticket` override or the
/// discovery walk — and errors clearly when neither yields one.
///
/// The error is the "requires ticket context" message every ticket-scoped
/// command shares; commands that merely prefer context call the discovery
/// layer directly.
pub fn require_ticket_root(
    context: &tix_sdk::context::Context,
    ticket: Option<&TicketRef>,
) -> Result<std::path::PathBuf, tix_sdk::SdkError> {
    let cli = load_cli_config(context)?;
    tix_sdk::discovery::resolve_ticket_root(ticket, &cli.tickets_directory)?.ok_or_else(|| {
        tix_sdk::SdkError::Engine(tix_sdk::TixError::TicketNotFound(
            "not inside a ticket — cd into one or pass --ticket <path|id>".to_string(),
        ))
    })
}

/// Create, inspect, and manage ticket workspaces
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
