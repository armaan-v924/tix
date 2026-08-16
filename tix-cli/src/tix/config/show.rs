//! `tix config show` — print the whole global config document.

use tix_sdk::document::TixDocument;
use crate::tix::utils::OutputType;
use tix_sdk::SdkError;

/// Print the global config document
#[derive(clap::Args, Debug)]
pub struct Args {
}

/// Prints the global config document.
///
/// The default (and TOML) output is the parsed document rendered as-is —
/// comments, formatting, and plugin tables preserved, straight from the
/// format-preserving DOM. JSON output converts the document's *values*
/// (comments cannot survive a format that has none).
pub fn run(app: &crate::tix::utils::App, _args: Args) -> Result<(), SdkError> {
    let document = TixDocument::load(&app.context.config_path)?;
    match app.output {
        OutputType::Default | OutputType::Toml => print!("{document}"),
        OutputType::Json => {
            let value: toml::Value = toml::from_str(&document.to_string())?;
            let json = serde_json::to_string_pretty(&value)
                .map_err(|e| SdkError::Message(format!("json conversion failed: {e}")))?;
            println!("{json}");
        }
    }
    Ok(())
}
