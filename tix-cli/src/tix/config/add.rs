//! `tix config add` — append one element to a list key.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use crate::tix::utils::confirm;
use tix_sdk::SdkError;
use tix_sdk::document::{TixDocument, with_write};

/// Append a value to a list in the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path of the list (e.g. `defaults.repositories`)
    pub key: ConfigKeyPath,

    /// The value to append (parsed as a TOML value when possible, else a
    /// string)
    pub value: String,

    /// Append without confirming, even if the value is already in the list
    #[arg(short, long)]
    pub force: bool,
}

/// Appends to the list at `<section>.<key>`, creating it when unset.
///
/// Duplicates are allowed — a list in the config is the user's, and tix has
/// no business deciding that two identical entries are a mistake — but
/// appending one is nearly always a slip, so it is confirmed first.
/// `--force` skips the question, which is also what a script wants.
///
/// The append matches the layout already in the file: one element per line
/// in a multi-line array, inline in an inline one, comments where they were
/// ([`edit::push_in_layout`]). As with `tix config set`, the edited section
/// is re-deserialized before the write lands, so an element of the wrong
/// type fails without touching the file.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let value = edit::parse_value(&args.value);

    // Read once outside the write cycle so the question is not asked while
    // holding the exclusive lock. Nothing is riding on the answer staying
    // true: duplicates are permitted, so the worst a concurrent write can
    // do is add one the user was not asked about.
    if !args.force {
        let document = TixDocument::load(&app.context.config_path)?;
        let mut item = Some(document.doc().as_item());
        for segment in args.key.segments() {
            item = item.and_then(|item| item.get(segment));
        }
        let present = item
            .and_then(|item| item.as_array())
            .is_some_and(|array| edit::position(array, &value).is_some());
        if present
            && !confirm(&format!(
                "'{}' is already in {}. Add it again?",
                args.value, args.key
            ))?
        {
            println!("aborted");
            return Ok(());
        }
    }

    let list = with_write(&app.context.config_path, |document| {
        let array = edit::array_for_add(document, &args.key)?;
        edit::push_in_layout(array, value);
        let rendered = edit::render(array);
        validate_section(document, args.key.section())?;
        Ok(rendered)
    })?;

    println!(
        "{} = {list} ({})",
        args.key,
        app.context.config_path.display()
    );
    Ok(())
}
