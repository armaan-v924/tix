//! `tix config set` — write one key anywhere in the config document,
//! through the format-preserving document layer.

use crate::tix::config::{ConfigKeyPath, edit, validate_section};
use crate::tix::plan::{Action, Plan};
use tix_sdk::SdkError;
use tix_sdk::document::{TixDocument, with_write};

/// Set a value in the config document
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `<section>.<key>` path to set (e.g. `defaults.branch_prefix`)
    pub key: ConfigKeyPath,

    /// The new value (parsed as a TOML value when possible, else a string)
    pub value: String,

    /// Validate the change and print it, with the value it replaces,
    /// without writing
    #[arg(long)]
    pub dry_run: bool,
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
///
/// `--dry-run` makes the same edit to a copy in memory, validation
/// included, and prints it — with whatever it would overwrite — as a
/// [`Plan`] in the `--output` format instead of writing. A value the
/// section rejects fails it the same way.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let apply = |document: &mut TixDocument| {
        let previous = edit::current(document, &args.key);
        // `1` stays an integer, `true` a bool, `["a", "b"]` an array — the
        // shape a list-valued key needs until add/remove verbs exist.
        // Anything that isn't valid TOML (a bare path, say) is a string.
        let value: toml_edit::Value = args
            .value
            .parse()
            .unwrap_or_else(|_| toml_edit::Value::from(args.value.as_str()));
        let item = toml_edit::Item::Value(value);
        document.table_at(&args.key.table_path())?[args.key.leaf()] = item.clone();

        validate_section(document, args.key.section())?;
        Ok((item, previous))
    };

    if args.dry_run {
        let mut plan = Plan::new("config set");
        let (item, previous) = match apply(&mut TixDocument::load(&app.context.config_path)?) {
            Ok(edited) => edited,
            Err(e) => return plan.finish(app.output, Err(e)),
        };
        plan.push(
            Action::new(
                "set",
                format!(
                    "would set {} = {} ({}; {})",
                    args.key,
                    args.value,
                    previous.as_ref().map_or_else(
                        || "currently unset".to_string(),
                        |p| format!("replacing {}", edit::describe(p))
                    ),
                    app.context.config_path.display()
                ),
            )
            .with("key", args.key.to_string())
            .with_some("value", edit::to_data(&item))
            .with_some("previous", previous.as_ref().and_then(edit::to_data))
            .with_path("path", &app.context.config_path),
        );
        return plan.finish(app.output, Ok(()));
    }

    with_write(&app.context.config_path, apply)?;
    tracing::info!(
        "{} = {} ({})",
        args.key,
        args.value,
        app.context.config_path.display()
    );
    Ok(())
}
