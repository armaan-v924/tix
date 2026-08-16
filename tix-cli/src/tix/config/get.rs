//! `tix config get` — read value(s) from the `[cli]` section.

use crate::tix::config::ConfigKey;
use tix_sdk::document::TixDocument;
use crate::tix::utils::OutputType;
use tix_sdk::SdkError;

/// Read a value from the [cli] section
#[derive(clap::Args, Debug)]
pub struct Args {
    /// The `[cli]` key(s) to read
    #[arg(required = true)]
    pub key: Vec<ConfigKey>,
}

/// Reads each requested key as a path into `[cli]` and prints its value.
///
/// A single key prints the bare value (scripting-friendly); multiple keys
/// print `key = value` lines. Keys are validated at parse time by the
/// [`ConfigKey`] value enum; a key that is valid but unset in the document
/// errors here.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let document = TixDocument::load(&app.context.config_path)?;

    let mut pairs = Vec::with_capacity(args.key.len());
    for key in &args.key {
        let item = document
            .doc()
            .get("cli")
            .and_then(|cli| cli.get(key.toml_key()))
            .ok_or_else(|| {
                SdkError::Message(format!(
                    "'{}' is not set in [cli] (config: {})",
                    key.toml_key(),
                    app.context.config_path.display()
                ))
            })?;
        pairs.push((key.toml_key(), item.clone()));
    }

    match app.output {
        OutputType::Default => {
            if let [(_, item)] = pairs.as_slice() {
                println!("{}", render_bare(item));
            } else {
                for (key, item) in &pairs {
                    println!("{key} = {}", item.to_string().trim());
                }
            }
        }
        OutputType::Toml => {
            for (key, item) in &pairs {
                println!("{key} = {}", item.to_string().trim());
            }
        }
        OutputType::Json => {
            let mut object = serde_json::Map::new();
            for (key, item) in &pairs {
                object.insert((*key).to_string(), item_to_json(key, item)?);
            }
            println!("{}", serde_json::to_string_pretty(&object).unwrap());
        }
    }
    Ok(())
}

/// A scalar's natural text (unquoted strings); non-scalars render as TOML.
fn render_bare(item: &toml_edit::Item) -> String {
    match item.as_str() {
        Some(s) => s.to_string(),
        None => item.to_string().trim().to_string(),
    }
}

/// Converts one TOML item to JSON by round-tripping it through a real TOML
/// parse — handles every value shape without a hand-written mapping.
fn item_to_json(key: &str, item: &toml_edit::Item) -> Result<serde_json::Value, SdkError> {
    let table: toml::Table = toml::from_str(&format!("{key} = {}", item.to_string().trim()))?;
    let value = table.get(key).cloned().unwrap_or(toml::Value::String(String::new()));
    serde_json::to_value(value).map_err(|e| SdkError::Message(format!("json conversion failed: {e}")))
}
