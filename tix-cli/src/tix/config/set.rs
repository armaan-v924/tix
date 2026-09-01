//! `tix config set` — write one key anywhere in the config document,
//! through the format-preserving document layer.

use crate::tix::config::{ConfigKeyPath, validate_section};
use tix_sdk::SdkError;
use tix_sdk::document::with_write;

/// Set a value in the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path to set (e.g. `defaults.branch_prefix`)
    pub key: ConfigKeyPath,

    /// The new value (parsed as a TOML value when possible, else a string)
    pub value: String,
}

/// Sets `<section>.<key>` and persists atomically.
///
/// The edit is a targeted path traversal over the format-preserving DOM —
/// plugin tables, comments, and formatting elsewhere in the file survive
/// byte-identical (applies to the CLI itself, not just plugin deltas).
/// Missing levels along the path materialize as real headered tables via
/// [`tix_sdk::document::TixDocument::table_at`], never as the collapsed
/// inline form bare indexing would produce (#146).
///
/// Before writing, the edited section is re-deserialized into the type that
/// owns it ([`validate_section`]); a value that type rejects fails the whole
/// write, and nothing lands on disk ([`with_write`] discards on error). A
/// `[<plugin>]` table has no such type and is written as given.
///
/// The whole cycle runs under the exclusive advisory lock (#67), so
/// concurrent `tix config set` invocations serialize rather than clobber.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    with_write(&app.context.config_path, |document| {
        // `1` stays an integer, `true` a bool, `["a", "b"]` an array — the
        // shape a list-valued key needs until add/remove verbs exist.
        // Anything that isn't valid TOML (a bare path, say) is a string.
        let value: toml_edit::Value = args
            .value
            .parse()
            .unwrap_or_else(|_| toml_edit::Value::from(args.value.as_str()));
        document.table_at(&args.key.table_path())?[args.key.leaf()] = toml_edit::Item::Value(value);

        validate_section(document, args.key.section())
    })?;
    println!(
        "{} = {} ({})",
        args.key,
        args.value,
        app.context.config_path.display()
    );
    Ok(())
}
