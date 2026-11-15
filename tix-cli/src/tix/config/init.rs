//! `tix config init` — first-time config creation.

use crate::tix::config::validate_section;
use crate::tix::plan::{Action, Plan};
use crate::tix::utils::OutputType;
use crate::tix::utils::{prompt, prompt_optional};
use std::path::Path;
use tix_sdk::document::TixDocument;
use tix_sdk::{SdkError, TixError};

/// Create the global config with prompts or explicit options
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Overwrite an existing config file
    #[arg(long)]
    pub force: bool,

    /// Print the config that would be written, using the same inputs as
    /// initialization (with --output json or toml, a plan carrying it)
    #[arg(long)]
    pub dry_run: bool,

    /// Directory for tickets. Supply both directories to skip all prompts
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::DirPath, requires = "code_directory",
        value_parser = clap::builder::NonEmptyStringValueParser::new())]
    tickets_directory: Option<String>,

    /// Directory where repositories are cloned
    #[arg(long, value_name = "PATH", value_hint = clap::ValueHint::DirPath, requires = "tickets_directory",
        value_parser = clap::builder::NonEmptyStringValueParser::new())]
    code_directory: Option<String>,

    #[command(flatten)]
    seeds: SeedInputs,
}

#[derive(clap::Args, Debug)]
struct SeedInputs {
    /// Branch prefix for newly created worktrees
    #[arg(long, requires = "tickets_directory")]
    branch_prefix: Option<String>,

    /// Base URL for GitHub repositories
    #[arg(long, requires = "tickets_directory")]
    github_base_url: Option<String>,

    /// Owner to use for bare repository names
    #[arg(long, requires = "tickets_directory")]
    default_repository_owner: Option<String>,

    /// Repository aliases included in new tickets, comma-separated or repeated
    #[arg(
        long,
        value_name = "ALIAS",
        value_delimiter = ',',
        requires = "tickets_directory"
    )]
    repositories: Vec<String>,
}

/// Creates the global config from prompts or explicit arguments.
///
/// Prompts for each required `[cli]` field with a pre-filled default and
/// then for the `[defaults]` seeds, which are optional and skipped by an
/// empty answer. Writes to the resolved config path — the same resolution
/// every command uses (`--config` > `TIX_CONFIG_PATH` > platform default),
/// which is why this command is exempt from the config-must-exist check.
/// Parent directories are created; an existing file is refused without
/// `--force`.
///
/// The seeds are prompted here rather than left to `tix config set` because
/// they are stamped into a ticket at creation and never re-read (#137): a
/// `branch_prefix` set after the first `tix ticket setup` is a prefix that
/// ticket will never have.
///
/// `--dry-run` uses the same input mode and checks. An existing file is
/// refused without `--force`. A successful preview prints the document instead of saving
/// it, so `--dry-run --force` shows what an overwrite would leave. With
/// `--output json` or `toml` it prints a [`Plan`] whose action carries the
/// document's data.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let path = &app.context.config_path;
    let exists = path.exists();
    if exists && !args.force {
        let refusal = already_exists(path);
        return match args.dry_run {
            true => Plan::new("config init").finish(app.output, Err(refusal)),
            false => Err(refusal),
        };
    }

    let (tickets_directory, code_directory, seeds) =
        match (args.tickets_directory, args.code_directory) {
            (Some(tickets), Some(code)) => (tickets, code, args.seeds),
            (None, None) => prompt_inputs()?,
            _ => {
                return Err(SdkError::Message(
                    "supply both --tickets-directory and --code-directory".into(),
                ));
            }
        };

    let mut document = TixDocument::empty();
    // An explicit [cli] table, not the inline `cli = { … }` form index-based
    // auto-creation would produce.
    let mut cli = toml_edit::Table::new();
    cli["tickets_directory"] = toml_edit::value(tickets_directory);
    cli["code_directory"] = toml_edit::value(code_directory);
    document.doc_mut()["cli"] = toml_edit::Item::Table(cli);

    let defaults = seed_table(seeds);
    // No section at all rather than an empty one: absent is what every read
    // of `[defaults]` already treats as "nothing seeded".
    if !defaults.is_empty() {
        document.doc_mut()["defaults"] = toml_edit::Item::Table(defaults);
    }

    // Cheap guard against this command and the section types drifting apart:
    // what it scaffolds must be what they parse.
    validate_section(&document, "cli")?;
    validate_section(&document, "defaults")?;
    if args.dry_run {
        if app.output == OutputType::Default {
            print!("{document}");
            return Ok(());
        }
        let content: toml::Table = toml::from_str(&document.to_string())?;
        let mut plan = Plan::new("config init");
        plan.push(
            Action::new(
                if exists { "overwrite" } else { "write" },
                format!(
                    "would {} {}",
                    if exists { "overwrite" } else { "write" },
                    path.display()
                ),
            )
            .with_path("path", path)
            .with("content", content),
        );
        return plan.finish(app.output, Ok(()));
    }
    if args.force {
        document.save(path)?;
    } else {
        document.save_new(path).map_err(|error| match &error {
            SdkError::Engine(TixError::IoError(io))
                if io.kind() == std::io::ErrorKind::AlreadyExists =>
            {
                already_exists(path)
            }
            _ => error,
        })?;
    }

    tracing::info!("wrote {}", path.display());
    Ok(())
}

fn already_exists(path: &Path) -> SdkError {
    SdkError::Message(format!(
        "config already exists at {} — pass --force to overwrite",
        path.display()
    ))
}

fn prompt_inputs() -> Result<(String, String, SeedInputs), SdkError> {
    let home = dirs::home_dir();
    let default_tickets = home.as_ref().map(|h| h.join("tickets"));
    let tickets = prompt(
        "Tickets directory",
        default_tickets.as_deref().and_then(|p| p.to_str()),
    )?;
    let default_code = home.as_ref().map(|h| h.join("code"));
    let code = prompt(
        "Code directory (where repos are cloned)",
        default_code.as_deref().and_then(|p| p.to_str()),
    )?;
    eprintln!("\n[defaults] — stamped into each new ticket at creation; blank to skip.");
    let seeds = SeedInputs {
        branch_prefix: prompt_optional("Branch prefix")?,
        github_base_url: prompt_optional("GitHub base URL")?,
        default_repository_owner: prompt_optional("Default repository owner")?,
        repositories: prompt_optional("Repositories a new ticket includes, comma-separated")?
            .map(|answer| answer.split(',').map(str::to_owned).collect())
            .unwrap_or_default(),
    };
    Ok((tickets, code, seeds))
}

fn seed_table(seeds: SeedInputs) -> toml_edit::Table {
    let mut defaults = toml_edit::Table::new();
    for (key, value) in [
        ("branch_prefix", seeds.branch_prefix),
        ("github_base_url", seeds.github_base_url),
        ("default_repository_owner", seeds.default_repository_owner),
    ] {
        if let Some(value) = value {
            defaults[key] = toml_edit::value(value);
        }
    }
    let names: toml_edit::Array = seeds
        .repositories
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if !names.is_empty() {
        defaults["repositories"] = toml_edit::value(names);
    }
    defaults
}
