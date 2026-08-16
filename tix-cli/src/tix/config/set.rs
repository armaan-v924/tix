//! `tix config set` — write one `[cli]` key through the format-preserving
//! document layer.

use crate::tix::config::{CliConfig, ConfigKey};
use crate::tix::context::Context;
use crate::tix::document::with_write;
use tix_engine::TixError;

/// Arguments for `tix config set`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `[cli]` key to set
    pub key: ConfigKey,

    /// The new value (parsed as a TOML value when possible, else a string)
    pub value: String,
}

/// Sets `[cli].<key>` and persists atomically.
///
/// The edit is a targeted path traversal over the format-preserving DOM —
/// plugin tables, comments, and formatting elsewhere in the file survive
/// byte-identical (spec §6.2 applies to the CLI itself, not just plugin
/// deltas). Before writing, the resulting `[cli]` section is re-deserialized
/// into [`CliConfig`]; a value the type rejects fails the whole write, and
/// nothing lands on disk ([`with_write`] discards on error).
///
/// The whole cycle runs under the exclusive advisory lock (#67), so
/// concurrent `tix config set` invocations serialize rather than clobber.
pub fn run(context: &Context, args: Args) -> Result<(), TixError> {
    with_write(&context.config_path, |document| {
        // `1` stays an integer, `true` a bool; anything that isn't valid
        // TOML (a bare path, say) is a string.
        let value: toml_edit::Value = args
            .value
            .parse()
            .unwrap_or_else(|_| toml_edit::Value::from(args.value.as_str()));
        // entry().or_insert with an explicit table so a missing [cli]
        // materializes as a real section, not an inline `cli = { … }`.
        document
            .doc_mut()
            .entry("cli")
            .or_insert(toml_edit::table())[args.key.toml_key()] = toml_edit::Item::Value(value);

        // Revalidate: the edited section must still parse as CliConfig
        // (deny_unknown_fields; type-incompatible values fail here).
        let _valid: CliConfig = document.section("cli")?.ok_or_else(|| {
            TixError::Message("edit produced no [cli] section — this is a bug".to_string())
        })?;
        Ok(())
    })?;
    println!(
        "{} = {} ({})",
        args.key.toml_key(),
        args.value,
        context.config_path.display()
    );
    Ok(())
}
