use clap;

use clap::builder::styling::{AnsiColor, Effects, Styles};

pub fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputType {
    Json,
    Toml,
    Default,
}

/// Prompts on stderr and reads one line from stdin; an empty answer takes
/// `default` when one is offered.
///
/// The prompt goes to stderr so interactive commands stay pipeable — stdout
/// carries only results.
pub fn prompt(label: &str, default: Option<&str>) -> Result<String, tix_engine::TixError> {
    use std::io::{BufRead, Write};

    match default {
        Some(default) => eprint!("{label} [{default}]: "),
        None => eprint!("{label}: "),
    }
    std::io::stderr().flush().map_err(tix_engine::TixError::IoError)?;

    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(tix_engine::TixError::IoError)?;
    let answer = answer.trim();

    match (answer.is_empty(), default) {
        (false, _) => Ok(answer.to_string()),
        (true, Some(default)) => Ok(default.to_string()),
        (true, None) => Err(tix_engine::TixError::Message(format!(
            "{label} is required"
        ))),
    }
}
