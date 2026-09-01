//! `tix ticket add` — add repository worktrees to an existing ticket.

use crate::tix::repo::RepoAlias;
use crate::tix::ticket::{
    TicketSharedArgs, derive_branch_name, load_ticket_config, require_ticket_root,
};
use tix_sdk::document::{TixDocument, with_write};
use tix_sdk::{Defaults, EngineConfig, SdkError, TicketConfig, TixError};
use tracing::{error, info};

/// Add repository worktrees to a ticket
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    /// Branch for the new worktree(s); derived from `[defaults]` + ticket
    /// context when omitted, and required when the derived branch is taken
    #[arg(short, long)]
    pub branch: Option<String>,

    /// Directory name for the new worktree, defaulting to the repository
    /// alias; required when a worktree already holds that name
    // `--as` mirrors `tix repo add --as`: one word, one meaning — "the name
    // this lands under".
    #[arg(long = "as", value_name = "NAME")]
    pub name: Option<String>,

    /// Registered repository aliases to add
    #[arg(required = true)]
    pub repo_aliases: Vec<RepoAlias>,
}

/// One worktree this invocation intends to create: the directory it will
/// occupy under the ticket root, and the repository behind it.
#[derive(Debug, PartialEq)]
struct Planned {
    /// The worktree directory name — the ticket map's key.
    name: String,
    /// The alias of the repository the worktree comes from.
    alias: String,
}

/// The branch the new worktrees check out, and where it came from.
///
/// A branch already in use is a different error depending on the source:
/// `--branch` is the remedy to point at only when it wasn't given.
#[derive(Clone, Copy)]
enum Branch<'a> {
    /// Given explicitly as `--branch`.
    Explicit(&'a str),
    /// Derived from `[defaults]` and the ticket's key and description.
    Derived(&'a str),
}

impl Branch<'_> {
    fn name(&self) -> &str {
        match self {
            Branch::Explicit(branch) | Branch::Derived(branch) => branch,
        }
    }
}

/// Adds a worktree per given alias to the current (or `--ticket`) ticket.
///
/// The worktree lands in `<ticket root>/<name>`, where `<name>` is `--as`
/// when given and the repo alias otherwise — the map key is the directory,
/// so the single-worktree case reads exactly as it always has, and a repo
/// can appear more than once under distinct names.
///
/// The branch is `--branch` when given (created if it doesn't exist), else
/// derived exactly as `tix ticket setup` derives it —
/// `<branch_prefix>/<key>-<sanitized-description>` — but evaluated **at add
/// time** against the *current* `[defaults].branch_prefix` and the ticket's
/// recorded key/description.
///
/// The ticket document is updated per successful worktree through the
/// format-preserving layer, so plugin tables and comments in
/// `.tix/ticket.toml` survive.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let root = require_ticket_root(&app.context, args.shared.ticket.as_ref())?;
    let ticket = load_ticket_config(&root)?;

    let document = TixDocument::load(&app.context.config_path)?;
    let engine: EngineConfig = document.section_or_default("engine")?;
    let defaults: Defaults = document.section_or_default("defaults")?;

    // Validate the whole batch up front: an unknown alias, a taken name, and
    // a branch already checked out are user errors, not partial-failure
    // material.
    for alias in &args.repo_aliases {
        if !engine.configured_repositories.contains_key(&alias.0) {
            let mut known: Vec<&str> = engine
                .configured_repositories
                .keys()
                .map(String::as_str)
                .collect();
            known.sort();
            return Err(SdkError::Engine(TixError::RepoNotFound(format!(
                "'{}' is not a registered repository (registered: {})",
                alias.0,
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ))));
        }
    }

    let branch_name = args.branch.clone().unwrap_or_else(|| {
        derive_branch_name(
            defaults.branch_prefix.as_deref(),
            &ticket.key,
            (!ticket.description.is_empty()).then_some(ticket.description.as_str()),
        )
    });
    let branch = match args.branch {
        Some(_) => Branch::Explicit(&branch_name),
        None => Branch::Derived(&branch_name),
    };
    let aliases: Vec<String> = args
        .repo_aliases
        .iter()
        .map(|alias| alias.0.clone())
        .collect();
    let planned = plan(&aliases, args.name.as_deref(), branch, &ticket)?;

    let ticket_document_path = root.join(".tix").join("ticket.toml");
    let mut failures: Vec<(String, TixError)> = Vec::new();
    for worktree in &planned {
        let repo_config = engine.configured_repositories[&worktree.alias].clone();
        let path = root.join(&worktree.name);
        let created = repo_config
            .ensure(&worktree.alias)
            .and_then(|repo| repo.create_worktree(&worktree.name, &branch_name, &path, false));
        match created {
            Ok(created) => {
                info!(name = %worktree.name, alias = %worktree.alias, branch = %created.branch, "worktree added");
                // Record each success immediately — a later failure must not
                // lose the worktrees that already exist on disk.
                with_write(&ticket_document_path, |doc| {
                    // An explicit table reached through `table_at`, so the
                    // entry renders as [ticket.worktrees.<name>], matching
                    // setup's output — indexing the path directly would
                    // collapse the worktrees into one inline line (#146).
                    let mut entry = toml_edit::Table::new();
                    entry["repo"] = toml_edit::value(worktree.alias.as_str());
                    entry["branch"] = toml_edit::value(created.branch.as_str());
                    doc.table_at(&["ticket", "worktrees"])?
                        .insert(&worktree.name, toml_edit::Item::Table(entry));
                    Ok(())
                })?;
                println!("{}", path.display());
            }
            Err(e) => {
                error!(name = %worktree.name, error = %e, "failed to add worktree");
                failures.push((worktree.name.clone(), e));
            }
        }
    }

    if !failures.is_empty() {
        let names: Vec<&str> = failures.iter().map(|(name, _)| name.as_str()).collect();
        return Err(SdkError::Message(format!(
            "{} of {} worktrees failed: {}",
            failures.len(),
            planned.len(),
            names.join(", ")
        )));
    }
    Ok(())
}

/// Resolves the aliases and `--as` into the worktree directories to create.
///
/// Both defaults stay in force for a repo the ticket already tracks; what
/// forces an explicit flag is a *collision*, not the repo's presence. The
/// name defaults to the alias until a worktree holds that directory, and the
/// branch derives until a worktree of the same repo has it checked out — the
/// two conditions that make the defaults unusable, and exactly the pair a
/// second worktree of a repo sitting at its default name trips. So
/// `tix add api` after `tix remove api` still just works, while the second
/// worktree of a live `api` is named and branched by hand rather than
/// invented.
fn plan(
    aliases: &[String],
    explicit_name: Option<&str>,
    branch: Branch<'_>,
    ticket: &TicketConfig,
) -> Result<Vec<Planned>, SdkError> {
    if let Some(name) = explicit_name {
        if aliases.len() != 1 {
            return Err(SdkError::Message(format!(
                "--as names a single worktree, but {} repositories were given — \
                 add them one command at a time",
                aliases.len()
            )));
        }
        validate_name(name)?;
    }

    let mut planned: Vec<Planned> = Vec::with_capacity(aliases.len());
    for alias in aliases {
        let name = explicit_name.unwrap_or(alias.as_str()).to_string();

        if let Some(entry) = ticket.worktrees.get(&name) {
            // Same repo under its default name is the ordinary "add it
            // twice" slip, and it needs a branch as well as a name — say so
            // once instead of erroring twice in a row.
            return Err(SdkError::Message(if &entry.repo == alias {
                format!(
                    "ticket '{}' already has a worktree of '{alias}' at '{name}' (on {}) — \
                     add a second with `--as <name> --branch <branch>`",
                    ticket.key, entry.branch
                )
            } else {
                format!(
                    "ticket '{}' already has a worktree named '{name}' (of '{}') — \
                     pick another name",
                    ticket.key, entry.repo
                )
            }));
        }

        if let Some(holder) = branch_holder(ticket, alias, branch.name()) {
            return Err(SdkError::Message(match branch {
                Branch::Explicit(branch) => format!(
                    "'{alias}' already has {branch} checked out in worktree '{holder}' — \
                     git allows a branch in one worktree at a time"
                ),
                Branch::Derived(branch) => format!(
                    "the branch this derives, {branch}, is already checked out in worktree \
                     '{holder}' — pass --branch <branch> for the new worktree"
                ),
            }));
        }

        if planned.iter().any(|other| other.name == name) {
            return Err(SdkError::Message(format!(
                "'{alias}' was given twice — a second worktree of one repository needs \
                 `--as <name> --branch <branch>`, so add it in its own command"
            )));
        }
        planned.push(Planned {
            name,
            alias: alias.clone(),
        });
    }
    Ok(planned)
}

/// The worktree of `alias` that already has `branch` checked out, if any.
///
/// Scoped to the repository, because a branch is: the same name under a
/// different repo is a different branch and no conflict at all. Only this
/// ticket is visible here — the same branch in another ticket's worktree, or
/// in the source checkout itself, is git's to reject at creation.
fn branch_holder<'a>(ticket: &'a TicketConfig, alias: &str, branch: &str) -> Option<&'a str> {
    // Sorted so a tie names the same worktree run to run — HashMap iteration
    // order is not stable.
    let mut holders: Vec<&str> = ticket
        .worktrees
        .iter()
        .filter(|(_, entry)| entry.repo == alias && entry.branch == branch)
        .map(|(name, _)| name.as_str())
        .collect();
    holders.sort_unstable();
    holders.into_iter().next()
}

/// Rejects `--as` values that cannot be a directory under the ticket root.
///
/// The worktree name *is* that directory — which is what makes the
/// filesystem enforce uniqueness for free — so separators, the relative
/// names, and the ticket's own `.tix` metadata directory are all out.
fn validate_name(name: &str) -> Result<(), SdkError> {
    if name.is_empty() || name.contains(['/', '\\']) || matches!(name, "." | ".." | ".tix") {
        return Err(SdkError::Message(format!(
            "'{name}' is not a valid worktree name — it becomes a directory under the ticket \
             root, so '/', '\\', '.', '..', and '.tix' are rejected"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    use tix_sdk::WorktreeConfig;

    /// A ticket tracking the given `(name, repo, branch)` worktrees.
    fn ticket(worktrees: &[(&str, &str, &str)]) -> TicketConfig {
        TicketConfig {
            key: "JIRA-123".to_string(),
            description: "Fix the login bug".to_string(),
            worktrees: worktrees
                .iter()
                .map(|(name, repo, branch)| {
                    (
                        name.to_string(),
                        WorktreeConfig {
                            repo: repo.to_string(),
                            branch: branch.to_string(),
                        },
                    )
                })
                .collect(),
        }
    }

    fn aliases(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// Without `--as`, a worktree is named after its repository — the common
    /// path, where name and alias coincide.
    #[test]
    fn test_name_defaults_to_alias() {
        let planned = plan(
            &aliases(&["api", "web"]),
            None,
            Branch::Derived("feature/JIRA-123"),
            &ticket(&[]),
        )
        .unwrap();
        let names: Vec<&str> = planned.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["api", "web"]);
        assert_eq!(planned[0].alias, "api");
    }

    /// `--as` renames the directory a single new worktree lands in.
    #[test]
    fn test_explicit_name_on_a_fresh_repo() {
        let planned = plan(
            &aliases(&["api"]),
            Some("api-spike"),
            Branch::Derived("feature/JIRA-123"),
            &ticket(&[]),
        )
        .unwrap();
        assert_eq!(
            planned,
            vec![Planned {
                name: "api-spike".to_string(),
                alias: "api".to_string(),
            }]
        );
    }

    /// The #85 case: a second worktree of a live repo, named and branched
    /// explicitly, plans a distinct directory backed by the same repo.
    #[test]
    fn test_second_worktree_of_tracked_repo() {
        let tracked = ticket(&[("api", "api", "feature/JIRA-123")]);
        let planned = plan(
            &aliases(&["api"]),
            Some("api-spike"),
            Branch::Explicit("spike/JIRA-123-auth"),
            &tracked,
        )
        .unwrap();
        assert_eq!(planned[0].name, "api-spike");
        assert_eq!(planned[0].alias, "api");
    }

    /// A repo sitting at its default name is the "added it twice" slip: the
    /// hint asks for both flags, since the branch is taken as well.
    #[test]
    fn test_default_name_taken_errors() {
        let tracked = ticket(&[("api", "api", "feature/JIRA-123")]);
        let error = plan(
            &aliases(&["api"]),
            None,
            Branch::Derived("feature/JIRA-123"),
            &tracked,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("--as <name> --branch"),
            "{error}"
        );
    }

    /// A tracked repo whose default *name* is free plans that name — the
    /// presence of another worktree of the repo is not itself a collision.
    /// This is `tix add api` after `tix remove api`.
    #[test]
    fn test_free_default_name_beside_another_worktree_of_the_repo() {
        let tracked = ticket(&[("api-spike", "api", "spike/JIRA-123-auth")]);
        let planned = plan(
            &aliases(&["api"]),
            None,
            Branch::Derived("feature/JIRA-123"),
            &tracked,
        )
        .unwrap();
        assert_eq!(planned[0].name, "api");
        assert_eq!(planned[0].alias, "api");
    }

    /// A free name but a derived branch a sibling holds errors, pointing at
    /// `--branch` — the flag that would fix it.
    #[test]
    fn test_derived_branch_already_checked_out_errors() {
        let tracked = ticket(&[("api", "api", "feature/JIRA-123")]);
        let error = plan(
            &aliases(&["api"]),
            Some("api-spike"),
            Branch::Derived("feature/JIRA-123"),
            &tracked,
        )
        .unwrap_err();
        assert!(error.to_string().contains("pass --branch"), "{error}");
    }

    /// An explicit branch a sibling holds errors before git would: a branch
    /// lives in one worktree at a time.
    #[test]
    fn test_explicit_branch_already_checked_out_errors() {
        let tracked = ticket(&[("api", "api", "spike/x")]);
        let error = plan(
            &aliases(&["api"]),
            Some("api-spike"),
            Branch::Explicit("spike/x"),
            &tracked,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("one worktree at a time"),
            "{error}"
        );
    }

    /// The same branch name under a *different* repository is a different
    /// branch, and no conflict.
    #[test]
    fn test_same_branch_in_another_repo_is_not_a_conflict() {
        let tracked = ticket(&[("web", "web", "feature/JIRA-123")]);
        let planned = plan(
            &aliases(&["api"]),
            None,
            Branch::Derived("feature/JIRA-123"),
            &tracked,
        )
        .unwrap();
        assert_eq!(planned[0].name, "api");
    }

    /// A name already taken by another repo's worktree is rejected before
    /// anything touches disk.
    #[test]
    fn test_name_collision_errors() {
        let tracked = ticket(&[("api-spike", "web", "feature/JIRA-123")]);
        let error = plan(
            &aliases(&["api"]),
            Some("api-spike"),
            Branch::Explicit("spike/x"),
            &tracked,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("already has a worktree named"),
            "{error}"
        );
    }

    /// `--as` names one worktree, so it cannot apply to a batch.
    #[test]
    fn test_name_with_multiple_aliases_errors() {
        let error = plan(
            &aliases(&["api", "web"]),
            Some("api-spike"),
            Branch::Explicit("spike/x"),
            &ticket(&[]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("single worktree"), "{error}");
    }

    /// The same alias twice in one command would collide on the default
    /// name; that is the multiple-worktree case, spelled out.
    #[test]
    fn test_duplicate_alias_errors() {
        let error = plan(
            &aliases(&["api", "api"]),
            None,
            Branch::Derived("feature/JIRA-123"),
            &ticket(&[]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("given twice"), "{error}");
    }

    /// A `--as` value that cannot be a directory under the ticket root is
    /// rejected.
    #[test]
    fn test_invalid_names_rejected() {
        for name in ["", "a/b", "a\\b", ".", "..", ".tix"] {
            assert!(
                plan(
                    &aliases(&["api"]),
                    Some(name),
                    Branch::Explicit("spike/x"),
                    &ticket(&[])
                )
                .is_err(),
                "expected '{name}' to be rejected"
            );
        }
    }
}
