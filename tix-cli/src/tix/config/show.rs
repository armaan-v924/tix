//! `tix config show` — print the whole global config document.

use crate::tix::context::Context;
use crate::tix::document::TixDocument;
use crate::tix::utils::OutputType;
use tix_engine::TixError;

/// Arguments for `tix config show`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Output format (default preserves the file as-is)
    #[arg(short, long)]
    pub output: Option<OutputType>,
}

/// Prints the global config document.
///
/// The default (and TOML) output is the parsed document rendered as-is —
/// comments, formatting, and plugin tables preserved, straight from the
/// format-preserving DOM. JSON output converts the document's *values*
/// (comments cannot survive a format that has none).
pub fn run(context: &Context, args: Args) -> Result<(), TixError> {
    let document = TixDocument::load(&context.config_path)?;
    match args.output.unwrap_or(OutputType::Default) {
        OutputType::Default | OutputType::Toml => print!("{document}"),
        OutputType::Json => {
            let value: toml::Value = toml::from_str(&document.to_string())?;
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| TixError::Message(format!("json conversion failed: {e}")))?;
            println!("{json}");
        }
    }
    Ok(())
}
