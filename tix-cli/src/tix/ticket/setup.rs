//! `tix ticket setup` — create a new ticket workspace with worktrees.

use crate::tix::config::CliConfig;
use crate::tix::repo::RepoAlias;
use crate::tix::ticket::{TicketRef, derive_branch_name};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tix_sdk::document::TixDocument;
use tix_sdk::{Defaults, EngineConfig, SdkError, TicketConfig, TixError, WorktreeConfig};
use tracing::{error, info};

/// Create a new ticket workspace with worktrees
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Where the ticket goes: a bare key (e.g. JIRA-123) created under the
    /// tickets directory, or a path (anything containing a separator, or
    /// `.`/`..`) created at that location
    #[arg(value_hint = clap::ValueHint::DirPath)]
    pub target: TicketRef,

    /// The key recorded for the ticket, defaulting to the directory name.
    /// Path form only — a bare key already names itself
    #[arg(short, long)]
    pub key: Option<String>,

    /// Human-readable description; feeds branch name derivation
    #[arg(short, long)]
    pub description: Option<String>,

    /// Include every registered repository
    #[arg(short, long, group = "repos")]
    pub all: bool,

    /// Repositories to include (defaults to `[defaults].repositories`)
    #[arg(group = "repos")]
    pub repo_aliases: Vec<RepoAlias>,
}

/// Which shape the target argument took.
///
/// The two forms agree on everything except where missing directories may be
/// invented — see [`create_ticket_root`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// A bare key, placed under the configured tickets directory.
    Key,
    /// An explicit path, placed exactly where it points.
    Path,
}

/// Where a ticket will be created, and the key its document will record.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    /// The directory to create: `<tickets_directory>/<key>` for the key
    /// form, the resolved argument for the path form.
    root: PathBuf,
    /// The ticket key — the argument itself, the directory name, or `--key`.
    key: String,
    /// The shape the argument took.
    form: Form,
}

/// Resolves the creation target: where the ticket directory goes, and what
/// key it records.
///
/// Shape decides, exactly as `--ticket` is disambiguated ([`TicketRef`],
/// `design/spec.md` §4) — one rule for finding tickets and creating them:
///
/// - **Key form** (a bare name): created at `tickets_directory.join(key)`,
///   v2 parity, and the key is the argument.
/// - **Path form** (contains a separator, is absolute, or is `.`/`..`):
///   created at that path, wherever it points. The key defaults to the
///   directory name, which is the only thing the key form could ever have
///   given it; `--key` overrides when the directory should be named for
///   humans and the ticket for the tracker.
///
/// Relative paths resolve against `cwd`, which callers pass as the
/// **logical** cwd: never canonicalized, so a ticket created through a
/// symlink keeps the identity the user sees (the discovery rule, applied at
/// creation).
///
/// # Errors
///
/// [`SdkError::Message`] when the key is empty or unusable as a directory
/// name, when a path names no directory to create (`/`, `..` — the argument
/// must name the ticket directory itself, not the place it goes), or when
/// `--key` accompanies the key form, where it could only contradict the
/// argument that already names the ticket.
fn resolve_target(
    target: &TicketRef,
    key_override: Option<&str>,
    tickets_directory: &Path,
    cwd: &Path,
) -> Result<Target, SdkError> {
    match target {
        TicketRef::Id(key) => {
            if let Some(override_key) = key_override {
                return Err(SdkError::Message(format!(
                    "--key '{override_key}' conflicts with '{key}', which already names the ticket — \
                     pass a path (e.g. './{key}') to give the directory a name of its own"
                )));
            }
            let key = validated_key(key)?;
            Ok(Target {
                root: tickets_directory.join(&key),
                key,
                form: Form::Key,
            })
        }
        TicketRef::Path(path) => {
            let root = if path.is_absolute() {
                drop_current_dir_components(path)
            } else {
                drop_current_dir_components(&cwd.join(path))
            };
            let key = match key_override {
                Some(key) => validated_key(key)?,
                None => {
                    let name = root.file_name().ok_or_else(|| {
                        SdkError::Message(format!(
                            "'{}' does not name a directory to create — give the path the ticket \
                             itself should occupy, e.g. './client-work/JIRA-123'",
                            path.display()
                        ))
                    })?;
                    validated_key(&name.to_string_lossy())?
                }
            };
            Ok(Target {
                root,
                key,
                form: Form::Path,
            })
        }
    }
}

/// Drops `.` components, so `./client-work/JIRA-1` prints and is recorded as
/// the location it actually names.
///
/// Only `.` — it resolves to the directory it sits in whatever that directory
/// is, so removing it cannot change which path is meant. `..` is deliberately
/// left in place: collapsing it lexically would resolve *through* a symlink
/// and pick a different directory than the shell would (`design/spec.md` §4).
fn drop_current_dir_components(path: &Path) -> PathBuf {
    path.components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect()
}

/// Returns `key` if it can serve as both a ticket key and a directory name.
///
/// The key form cannot produce most of these (a name with a separator parses
/// as a path, not a key), but `--key` takes arbitrary text and the check is
/// what keeps it from writing a ticket that no directory can hold.
fn validated_key(key: &str) -> Result<String, SdkError> {
    if key.is_empty() {
        return Err(SdkError::Message(
            "the ticket key is empty — it becomes a directory name".to_string(),
        ));
    }
    if key.contains(['/', '\\']) {
        return Err(SdkError::Message(format!(
            "'{key}' is not a valid ticket key — it becomes a directory name, so '/' and '\\' are rejected"
        )));
    }
    if key == "." || key == ".." {
        return Err(SdkError::Message(format!(
            "'{key}' is not a valid ticket key — it becomes a directory name"
        )));
    }
    Ok(key.to_string())
}

/// Creates the ticket directory, refusing to overwrite one and — in the path
/// form — refusing to invent its parent.
///
/// The forms differ deliberately. `tickets_directory` is configured and
/// intentional, so the key form creates it when it is missing. An arbitrary
/// path is neither: a typo (`./cleint-work/JIRA-1`) would otherwise grow a
/// tree of worktrees somewhere the user never meant, and unwinding that costs
/// more than the `mkdir -p` the error asks for. Creating parents is the
/// loosening we could still make later; requiring them is not.
fn create_ticket_root(target: &Target) -> Result<(), SdkError> {
    if target.root.exists() {
        return Err(SdkError::Message(format!(
            "ticket '{}' already exists at {}",
            target.key,
            target.root.display()
        )));
    }
    match target.form {
        Form::Key => std::fs::create_dir_all(&target.root).map_err(SdkError::from),
        Form::Path => {
            let parent = target.root.parent().ok_or_else(|| {
                SdkError::Message(format!(
                    "{} has no parent directory to create the ticket in",
                    target.root.display()
                ))
            })?;
            if !parent.is_dir() {
                return Err(SdkError::Message(format!(
                    "{} does not exist — create it first (`mkdir -p`), or pass a bare key to \
                     create the ticket under the tickets directory",
                    parent.display()
                )));
            }
            std::fs::create_dir(&target.root).map_err(SdkError::from)
        }
    }
}

/// Creates a ticket workspace with a worktree per selected repo.
///
/// The target argument is disambiguated by shape (`resolve_target`): a bare
/// key lands under `tickets_directory`, a path lands where it points. Both
/// forms are otherwise identical — same seeds, same derivation, same
/// document.
///
/// Runs **without ticket context** by design — this is the command that
/// creates it. `[defaults]` is read **once**, here, and the derived values
/// are written into the ticket document; later changes to `[defaults]` never
/// touch this ticket (see
/// [creation-time seeds](https://tix.armaanv.dev/latest/concepts/seeds/)).
///
/// Repo selection: explicit aliases win, `--all` takes every registered
/// repo, and neither falls back to the `defaults.repositories` seed list.
///
/// Partial failure leaves successfully created worktrees in place and
/// records only them in `.tix/ticket.toml`; the command then errors naming
/// the repos that failed.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let document = TixDocument::load(&app.context.config_path)?;
    let cli: CliConfig = document.section("cli")?.ok_or_else(|| {
        SdkError::Message("global config has no [cli] section — run `tix config init`".to_string())
    })?;
    let engine: EngineConfig = document.section_or_default("engine")?;
    let defaults: Defaults = document.section_or_default("defaults")?;

    let target = resolve_target(
        &args.target,
        args.key.as_deref(),
        &cli.tickets_directory,
        &tix_sdk::discovery::logical_cwd()?,
    )?;

    // --- select repositories ---
    let selected: Vec<String> = if args.all {
        let mut aliases: Vec<String> = engine.configured_repositories.keys().cloned().collect();
        aliases.sort();
        aliases
    } else if !args.repo_aliases.is_empty() {
        args.repo_aliases
            .iter()
            .map(|alias| alias.0.clone())
            .collect()
    } else {
        defaults.repositories.clone()
    };
    for alias in &selected {
        if !engine.configured_repositories.contains_key(alias) {
            let mut known: Vec<&str> = engine
                .configured_repositories
                .keys()
                .map(String::as_str)
                .collect();
            known.sort();
            return Err(SdkError::Engine(TixError::RepoNotFound(format!(
                "'{alias}' is not a registered repository (registered: {})",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ))));
        }
    }

    // --- seed reads: derivation happens once, now ---
    let branch = derive_branch_name(
        defaults.branch_prefix.as_deref(),
        &target.key,
        args.description.as_deref(),
    );
    let description = args.description.clone().unwrap_or_default();

    create_ticket_root(&target)?;
    let ticket_root = &target.root;

    // --- create worktrees; collect successes and failures ---
    let mut worktrees: HashMap<String, WorktreeConfig> = HashMap::new();
    let mut failures: Vec<(String, TixError)> = Vec::new();
    for alias in &selected {
        let repo_config = engine.configured_repositories[alias].clone();
        let result = repo_config
            .ensure(alias)
            .and_then(|repo| repo.create_worktree(alias, &branch, &ticket_root.join(alias), false));
        match result {
            Ok(worktree) => {
                info!(alias = %alias, branch = %worktree.branch, "worktree created");
                worktrees.insert(
                    alias.clone(),
                    WorktreeConfig {
                        repo: alias.clone(),
                        branch: worktree.branch,
                    },
                );
            }
            Err(e) => {
                error!(alias = %alias, error = %e, "failed to create worktree");
                failures.push((alias.clone(), e));
            }
        }
    }

    // --- write the ticket document, recording only what actually exists ---
    let ticket = TicketConfig {
        key: target.key.clone(),
        description,
        worktrees,
    };
    let mut ticket_document = TixDocument::empty();
    ticket_document.set_section("ticket", &ticket)?;
    ticket_document.save(&ticket_root.join(".tix").join("ticket.toml"))?;

    println!("{}", ticket_root.display());

    if !failures.is_empty() {
        let names: Vec<&str> = failures.iter().map(|(alias, _)| alias.as_str()).collect();
        return Err(SdkError::Message(format!(
            "ticket created, but {} of {} worktrees failed: {} — fix and `tix ticket add` them",
            failures.len(),
            selected.len(),
            names.join(", ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The tickets directory and cwd used by the resolution tests, distinct
    /// so a target landing in the wrong one is visible.
    const TICKETS: &str = "/home/u/tickets";
    const CWD: &str = "/home/u/work";

    fn resolve(arg: &str, key: Option<&str>) -> Result<Target, SdkError> {
        let target: TicketRef = arg.parse().unwrap();
        resolve_target(&target, key, Path::new(TICKETS), Path::new(CWD))
    }

    // --- key form ---

    /// A bare name lands under the tickets directory and is its own key —
    /// v2 parity, unchanged by path-form creation.
    #[test]
    fn test_key_form_lands_under_tickets_directory() {
        let target = resolve("JIRA-123", None).unwrap();
        assert_eq!(target.root, Path::new("/home/u/tickets/JIRA-123"));
        assert_eq!(target.key, "JIRA-123");
        assert_eq!(target.form, Form::Key);
    }

    /// `--key` with a bare key could only contradict the argument that
    /// already names the ticket.
    #[test]
    fn test_key_form_rejects_key_override() {
        assert!(resolve("JIRA-123", Some("OTHER-1")).is_err());
    }

    /// An empty argument names no directory.
    #[test]
    fn test_empty_key_rejected() {
        assert!(resolve("", None).is_err());
    }

    // --- path form ---

    /// A relative path resolves against the cwd, not the tickets directory,
    /// and takes its key from the directory name.
    #[test]
    fn test_path_form_lands_at_the_path() {
        let target = resolve("./client-work/JIRA-1", None).unwrap();
        assert_eq!(target.root, Path::new("/home/u/work/client-work/JIRA-1"));
        assert_eq!(target.key, "JIRA-1");
        assert_eq!(target.form, Form::Path);
    }

    /// An absolute path is taken as given.
    #[test]
    fn test_path_form_absolute() {
        let target = resolve("/srv/tickets/JIRA-2", None).unwrap();
        assert_eq!(target.root, Path::new("/srv/tickets/JIRA-2"));
        assert_eq!(target.key, "JIRA-2");
    }

    /// A separator anywhere makes it a path — a bare name is never one, so
    /// `NAME/` and `a/b` both create where they point.
    #[test]
    fn test_separator_makes_a_path() {
        assert_eq!(
            resolve("client-work/JIRA-1", None).unwrap().root,
            Path::new("/home/u/work/client-work/JIRA-1")
        );
        assert_eq!(resolve("JIRA-1/", None).unwrap().key, "JIRA-1");
    }

    /// `--key` decouples the directory name from the recorded key: the
    /// capability the path form exists to enable.
    #[test]
    fn test_path_form_key_override() {
        let target = resolve("./client-work/acme-login", Some("JIRA-9")).unwrap();
        assert_eq!(
            target.root,
            Path::new("/home/u/work/client-work/acme-login")
        );
        assert_eq!(target.key, "JIRA-9");
    }

    /// A path with no final component has no key to take.
    #[test]
    fn test_path_form_without_a_name_errors() {
        assert!(resolve("/", None).is_err());
        assert!(resolve("..", None).is_err());
    }

    /// `.` takes the cwd's own name as the key; whether it may be *created*
    /// is [`create_ticket_root`]'s call, and it already exists.
    #[test]
    fn test_dot_takes_the_cwd_name() {
        let target = resolve(".", None).unwrap();
        assert_eq!(target.key, "work");
        assert_eq!(target.form, Form::Path);
    }

    /// An override that no directory could hold is rejected wherever it
    /// came from.
    #[test]
    fn test_key_override_is_validated() {
        assert!(resolve("./a/b", Some("has/separator")).is_err());
        assert!(resolve("./a/b", Some("")).is_err());
        assert!(resolve("./a/b", Some("..")).is_err());
    }

    // --- create_ticket_root ---

    fn target_at(root: PathBuf, form: Form) -> Target {
        Target {
            root,
            key: "JIRA-1".to_string(),
            form,
        }
    }

    /// The key form creates the tickets directory on the way down: it is
    /// configured and intentional.
    #[test]
    fn test_key_form_creates_missing_parents() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("tickets/JIRA-1");
        create_ticket_root(&target_at(root.clone(), Form::Key)).unwrap();
        assert!(root.is_dir());
    }

    /// The path form refuses to invent a parent — the typo net.
    #[test]
    fn test_path_form_requires_an_existing_parent() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("cleint-work/JIRA-1");
        assert!(create_ticket_root(&target_at(root.clone(), Form::Path)).is_err());
        assert!(!root.exists());
    }

    /// With the parent in place, the path form creates the ticket directory
    /// exactly where it points.
    #[test]
    fn test_path_form_creates_under_existing_parent() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("client-work");
        std::fs::create_dir(&parent).unwrap();
        let root = parent.join("JIRA-1");
        create_ticket_root(&target_at(root.clone(), Form::Path)).unwrap();
        assert!(root.is_dir());
    }

    /// Neither form overwrites what is already there.
    #[test]
    fn test_existing_root_is_never_reused() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("JIRA-1");
        std::fs::create_dir(&root).unwrap();
        assert!(create_ticket_root(&target_at(root.clone(), Form::Path)).is_err());
        assert!(create_ticket_root(&target_at(root, Form::Key)).is_err());
    }
}
