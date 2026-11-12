//! `tix config unset` — delete one key from the config document.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use crate::tix::plan::{Action, Plan};
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
/// prints what would go as a [`Plan`] in the `--output` format instead of
/// writing: an unset key, or one the section requires, fails it the same
/// way.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let apply = |document: &mut TixDocument| {
        let key = &args.key;
        let previous = edit::current(document, key);
        edit::holding_table(document, key)?
            .remove(key.leaf())
            .ok_or_else(|| SdkError::Message(format!("'{key}' is not set")))?;
        validate_section(document, key.section())?;
        Ok(previous)
    };

    if args.dry_run {
        let mut plan = Plan::new("config unset");
        let previous = match apply(&mut TixDocument::load(&app.context.config_path)?) {
            Ok(previous) => previous,
            Err(e) => return plan.finish(app.output, Err(e)),
        };
        plan.push(
            Action::new(
                "unset",
                format!(
                    "would unset {} (currently {}; {})",
                    args.key,
                    previous.as_ref().map(edit::describe).unwrap_or_default(),
                    app.context.config_path.display()
                ),
            )
            .with("key", args.key.to_string())
            .with_some("previous", previous.as_ref().and_then(edit::to_data))
            .with_path("path", &app.context.config_path),
        );
        return plan.finish(app.output, Ok(()));
    }

    with_write(&app.context.config_path, apply)?;

    tracing::info!("unset {} ({})", args.key, app.context.config_path.display());
    Ok(())
}
