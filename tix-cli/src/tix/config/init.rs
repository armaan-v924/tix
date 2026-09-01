//! `tix config init` — interactive first-time config creation.

use crate::tix::config::validate_section;
use crate::tix::utils::{prompt, prompt_optional};
use tix_sdk::SdkError;
use tix_sdk::document::TixDocument;

/// Create the global config interactively
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Overwrite an existing config file
    #[arg(long)]
    pub force: bool,
}

/// Creates the global config interactively.
///
/// Prompts for each required `[cli]` field with a pre-filled default and
/// then for the `[defaults]` seeds, which are optional and skipped by an
/// empty answer. Writes to the resolved config path — the same resolution
/// every command uses (`--config` > `TIX_CONFIG_PATH` > platform default),
/// which is why this command is exempt from the config-must-exist check.
/// Parent directories are created; an existing file is refused without
/// `--force`.
///
/// The seeds are prompted here rather than left to `tix config set` because
/// they are stamped into a ticket at creation and never re-read (#137): a
/// `branch_prefix` set after the first `tix ticket setup` is a prefix that
/// ticket will never have.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let path = &app.context.config_path;
    if path.exists() && !args.force {
        return Err(SdkError::Message(format!(
            "config already exists at {} — pass --force to overwrite",
            path.display()
        )));
    }

    let home = dirs::home_dir();
    let default_tickets = home.as_ref().map(|h| h.join("tickets"));
    let tickets_directory = prompt(
        "Tickets directory",
        default_tickets.as_deref().and_then(|p| p.to_str()),
    )?;
    let default_code = home.as_ref().map(|h| h.join("code"));
    let code_directory = prompt(
        "Code directory (where repos are cloned)",
        default_code.as_deref().and_then(|p| p.to_str()),
    )?;

    let mut document = TixDocument::empty();
    // An explicit [cli] table, not the inline `cli = { … }` form index-based
    // auto-creation would produce.
    let mut cli = toml_edit::Table::new();
    cli["tickets_directory"] = toml_edit::value(tickets_directory);
    cli["code_directory"] = toml_edit::value(code_directory);
    document.doc_mut()["cli"] = toml_edit::Item::Table(cli);

    let defaults = prompt_defaults()?;
    // No section at all rather than an empty one: absent is what every read
    // of `[defaults]` already treats as "nothing seeded".
    if !defaults.is_empty() {
        document.doc_mut()["defaults"] = toml_edit::Item::Table(defaults);
    }

    // Cheap guard against this command and the section types drifting apart:
    // what it scaffolds must be what they parse.
    validate_section(&document, "cli")?;
    validate_section(&document, "defaults")?;
    document.save(path)?;

    println!("Wrote {}", path.display());
    Ok(())
}

/// Prompts for the `[defaults]` seeds, returning the table they populate —
/// empty when every answer was skipped.
///
/// `repositories` is prompted as a comma-separated list rather than the
/// TOML array literal `tix config set` takes: this is a prompt, not a TOML
/// value slot, and the seed names registered repositories, which is why it
/// comes last — on a first run there are none yet, and `tix repo add` is
/// still ahead.
fn prompt_defaults() -> Result<toml_edit::Table, SdkError> {
    eprintln!("\n[defaults] — stamped into each new ticket at creation; blank to skip.");

    let mut defaults = toml_edit::Table::new();
    let scalars = [
        ("branch_prefix", "Branch prefix"),
        ("github_base_url", "GitHub base URL"),
        ("default_repository_owner", "Default repository owner"),
    ];
    for (key, label) in scalars {
        if let Some(answer) = prompt_optional(label)? {
            defaults[key] = toml_edit::value(answer);
        }
    }

    let repositories = prompt_optional("Repositories a new ticket includes, comma-separated")?
        .map(|answer| {
            answer
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .collect::<toml_edit::Array>()
        })
        .filter(|names| !names.is_empty());
    if let Some(names) = repositories {
        defaults["repositories"] = toml_edit::value(names);
    }

    Ok(defaults)
}
