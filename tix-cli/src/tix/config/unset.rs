//! `tix config unset` — delete one key from the config document.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use tix_sdk::SdkError;
use tix_sdk::document::{TixDocument, with_write};

/// Remove a key from the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path to remove (e.g. `defaults.branch_prefix`)
    pub key: ConfigKeyPath,

    /// Validate the removal and print it, with the value it would delete,
    /// without writing
    #[arg(long)]
    pub dry_run: bool,
}

/// Deletes `<section>.<key>`, whatever it holds.
///
/// The counterpart to `tix config set`, and a key operation throughout: it
/// takes the same `<section>.<key>` path, so a whole section cannot be
/// deleted by accident, and a key that is already unset is an error rather
/// than a no-op.
///
/// Deleting a key a section *requires* fails the write — `[cli]` needs both
/// of its keys, and the section is re-deserialized before anything reaches
/// disk. The table the key lived in stays, empty if it has to be: an empty
/// `[defaults]` reads exactly like an absent one, and removing it would
/// take the user's comments with it.
///
/// `--dry-run` deletes from a copy in memory, validation included, and
/// prints what would go on stdout instead of writing: an unset key, or one
/// the section requires, fails it the same way.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let apply = |document: &mut TixDocument| {
        let key = &args.key;
        let previous = edit::current(document, key);
        edit::holding_table(document, key)?
            .remove(key.leaf())
            .ok_or_else(|| SdkError::Message(format!("'{key}' is not set")))?;
        validate_section(document, key.section())?;
        Ok(previous.unwrap_or_default())
    };

    if args.dry_run {
        let previous = apply(&mut TixDocument::load(&app.context.config_path)?)?;
        println!(
            "would unset {} (currently {previous}; {})",
            args.key,
            app.context.config_path.display()
        );
        return Ok(());
    }

    with_write(&app.context.config_path, apply)?;

    tracing::info!("unset {} ({})", args.key, app.context.config_path.display());
    Ok(())
}
