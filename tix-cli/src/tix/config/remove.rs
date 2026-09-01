//! `tix config remove` — drop one element from a list key.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use tix_sdk::SdkError;
use tix_sdk::document::with_write;

/// Remove a value from a list in the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path of the list (e.g. `defaults.repositories`)
    pub key: ConfigKeyPath,

    /// The value to remove (parsed as a TOML value when possible, else a
    /// string)
    pub value: String,
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
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let value = edit::parse_value(&args.value);

    let list = with_write(&app.context.config_path, |document| {
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
    })?;

    println!(
        "{} = {list} ({})",
        args.key,
        app.context.config_path.display()
    );
    Ok(())
}
