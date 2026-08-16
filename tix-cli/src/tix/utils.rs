use clap;

use clap::builder::styling::{AnsiColor, Effects, Styles};

pub fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Cyan.on_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputType {
    Json,
    Toml,
    Default,
}

impl OutputType {
    /// The value's wire form, as forwarded to plugins via `--tix-output`.
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputType::Json => "json",
            OutputType::Toml => "toml",
            OutputType::Default => "default",
        }
    }
}

/// Everything a subcommand receives: the SDK context plus tix's resolved
/// presentation globals. The host resolves these once, up front — they are
/// what plugin dispatch forwards as settled `--tix-*` values (#68).
pub struct App {
    /// The SDK context (resolved config path).
    pub context: tix_sdk::context::Context,
    /// The resolved global output format (`--output`, hoisted from
    /// per-subcommand flags so `tix foo --json | jq` works through plugins).
    pub output: OutputType,
    /// The resolved log level.
    pub log_level: tracing::Level,
}

/// Prompts on stderr and reads one line from stdin; an empty answer takes
/// `default` when one is offered.
///
/// The prompt goes to stderr so interactive commands stay pipeable — stdout
/// carries only results.
pub fn prompt(label: &str, default: Option<&str>) -> Result<String, tix_sdk::SdkError> {
    use std::io::{BufRead, Write};

    match default {
        Some(default) => eprint!("{label} [{default}]: "),
        None => eprint!("{label}: "),
    }
    std::io::stderr().flush().map_err(tix_sdk::SdkError::from)?;

    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(tix_sdk::SdkError::from)?;
    let answer = answer.trim();

    match (answer.is_empty(), default) {
        (false, _) => Ok(answer.to_string()),
        (true, Some(default)) => Ok(default.to_string()),
        (true, None) => Err(tix_sdk::SdkError::Message(format!(
            "{label} is required"
        ))),
    }
}
