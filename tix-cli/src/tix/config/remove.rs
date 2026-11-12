//! `tix config remove` — drop one element from a list key.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use tix_sdk::SdkError;
use tix_sdk::document::{TixDocument, with_write};

/// Remove a value from a list in the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path of the list (e.g. `defaults.repositories`)
    pub key: ConfigKeyPath,

    /// The value to remove (parsed as a TOML value when possible, else a
    /// string)
    pub value: String,

    /// Validate the removal and print the list it would leave, without
    /// writing
    #[arg(long)]
    pub dry_run: bool,
}

/// Removes the first element of `<section>.<key>` equal to `value`.
///
/// Strictly an element operation: an unset key, a key that holds something
/// other than a list, and a value that isn't in the list are all errors
/// rather than no-ops — each one means the command did not do what was
/// asked, and a silent success would hide a typo. `tix config unset`
/// removes a key.
///
/// Elements match on what they *mean*, not how they are written, so a value
/// typed as `backend` removes `'backend'` from the file. Duplicates are
/// removed one call at a time, front first, mirroring the appends that
/// created them.
///
/// `--dry-run` removes from a copy in memory, validation included, and
/// prints the list that would be left on stdout instead of writing.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let value = edit::parse_value(&args.value);

    let apply = |document: &mut TixDocument| {
        let key = &args.key;
        let array = edit::existing_array(document, key)?;
        let index = edit::position(array, &value).ok_or_else(|| {
            SdkError::Message(format!(
                "{} is not in '{key}' (it holds {})",
                edit::literal(&value),
                edit::render(array)
            ))
        })?;
        edit::remove_in_layout(array, index);
        let rendered = edit::render(array);
        validate_section(document, key.section())?;
        Ok(rendered)
    };

    if args.dry_run {
        let list = apply(&mut TixDocument::load(&app.context.config_path)?)?;
        println!(
            "would remove {} from {}, leaving {} = {list} ({})",
            edit::literal(&value),
            args.key,
            args.key,
            app.context.config_path.display()
        );
        return Ok(());
    }

    let list = with_write(&app.context.config_path, apply)?;

    tracing::info!(
        "{} = {list} ({})",
        args.key,
        app.context.config_path.display()
    );
    Ok(())
}
