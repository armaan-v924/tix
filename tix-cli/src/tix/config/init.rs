//! `tix config init` — interactive first-time config creation.

use crate::tix::context::Context;
use crate::tix::document::TixDocument;
use crate::tix::utils::prompt;
use tix_engine::TixError;

/// Arguments for `tix config init`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Overwrite an existing config file
    #[arg(long)]
    pub force: bool,
}

/// Creates the global config interactively.
///
/// Prompts for each required `[cli]` field with a pre-filled default, then
/// writes to the resolved config path — the same resolution every command
/// uses (`--config` > `TIX_CONFIG_PATH` > platform default), which is why
/// this command is exempt from the config-must-exist check. Parent
/// directories are created; an existing file is refused without `--force`.
pub fn run(context: &Context, args: Args) -> Result<(), TixError> {
    let path = &context.config_path;
    if path.exists() && !args.force {
        return Err(TixError::Message(format!(
            "config already exists at {} — pass --force to overwrite",
            path.display()
        )));
    }

    let default_tickets = dirs::home_dir().map(|home| home.join("tickets"));
    let tickets_directory = prompt(
        "Tickets directory",
        default_tickets.as_deref().and_then(|p| p.to_str()),
    )?;

    let mut document = TixDocument::empty();
    // An explicit [cli] table, not the inline `cli = { … }` form index-based
    // auto-creation would produce.
    let mut cli = toml_edit::Table::new();
    cli["tickets_directory"] = toml_edit::value(tickets_directory);
    document.doc_mut()["cli"] = toml_edit::Item::Table(cli);
    document.save(path)?;

    println!("Wrote {}", path.display());
    Ok(())
}
