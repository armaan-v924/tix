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
    let answer = ask(&match default {
        Some(default) => format!("{label} [{default}]"),
        None => label.to_string(),
    })?;

    match (answer.is_empty(), default) {
        (false, _) => Ok(answer),
        (true, Some(default)) => Ok(default.to_string()),
        (true, None) => Err(tix_sdk::SdkError::Message(format!("{label} is required"))),
    }
}

/// Prompts for a value that may be left unset: an empty answer is `None`,
/// not an error.
///
/// The optional counterpart to [`prompt`], for keys whose absence is a
/// meaningful state rather than a missing answer — every `[defaults]` seed.
pub fn prompt_optional(label: &str) -> Result<Option<String>, tix_sdk::SdkError> {
    let answer = ask(&format!("{label} (optional)"))?;
    Ok((!answer.is_empty()).then_some(answer))
}

/// Asks a yes/no question, defaulting to no.
///
/// One place for the CLI's confirmation convention: `[y/N]`, an empty
/// answer declines, and only `y`/`yes` accepts. A command that offers a way
/// past the question spells it `--force`.
///
/// Reads through `ask` rather than [`prompt`], whose own `[default]`
/// suffix would print a second bracket after the `[y/N]`.
pub fn confirm(question: &str) -> Result<bool, tix_sdk::SdkError> {
    let answer = ask(&format!("{question} [y/N]"))?;
    Ok(matches!(answer.to_lowercase().as_str(), "y" | "yes"))
}

/// Writes `label` to stderr and reads one trimmed line from stdin.
fn ask(label: &str) -> Result<String, tix_sdk::SdkError> {
    use std::io::{BufRead, Write};

    eprint!("{label}: ");
    std::io::stderr().flush().map_err(tix_sdk::SdkError::from)?;

    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(tix_sdk::SdkError::from)?;
    Ok(answer.trim().to_string())
}
