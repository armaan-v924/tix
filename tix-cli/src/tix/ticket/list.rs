//! `tix ticket list` — one line per ticket under the tickets directory,
//! grouped by project.

use crate::tix::ticket::load_cli_config;
use crate::tix::utils::OutputType;
use std::path::Path;
use tix_sdk::document::TixDocument;
use tix_sdk::project::{Project, list_projects};
use tix_sdk::{SdkError, TicketConfig};
use tracing::warn;

/// List all tickets in the tickets directory
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Output format
    #[arg(short, long)]
    pub output: Option<OutputType>,

    /// Show only this project's tickets
    #[arg(short, long, value_name = "NAME")]
    pub project: Option<String>,
}

/// The tickets in one place: a project, or loose (`project: None`).
#[derive(Debug, PartialEq)]
struct Group {
    project: Option<String>,
    tickets: Vec<TicketConfig>,
}

/// Lists every ticket under `tickets_directory`: loose tickets first, then
/// each project's, in name order. `--project` narrows it to one project.
///
/// A project is a directory carrying `.tix/project.toml` — the same marker
/// the discovery walk uses — not merely any subdirectory, so a stray
/// directory under the tickets directory is neither listed as a project nor
/// searched for tickets.
///
/// Reads only `[ticket]` sections — no live `Ticket` resolution and no
/// worktree scanning, so listing stays fast on large ticket sets (directory
/// layout is frontend policy; the engine is never involved). Directories
/// without a `.tix/ticket.toml` are silently skipped; one with a malformed
/// document warns and is skipped — a broken ticket never breaks the listing.
pub fn run(app: &crate::tix::utils::App, args: Args) -> Result<(), SdkError> {
    let cli = load_cli_config(&app.context)?;
    let groups = collect(&cli.tickets_directory, args.project.as_deref())?;
    print_groups(&groups, app.output, args.project.is_some())
}

/// Gathers the groups to print.
///
/// # Errors
///
/// [`SdkError::Message`] when `project` names no existing project — a typo
/// should fail rather than print an empty listing.
fn collect(tickets_directory: &Path, project: Option<&str>) -> Result<Vec<Group>, SdkError> {
    if let Some(name) = project {
        let project = Project::named(tickets_directory, name)?;
        if !project.exists() {
            let known: Vec<String> = list_projects(tickets_directory)?
                .into_iter()
                .map(|p| p.name)
                .collect();
            return Err(SdkError::Message(format!(
                "no project '{name}' in {} (projects: {})",
                tickets_directory.display(),
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            )));
        }
        return Ok(vec![Group {
            tickets: tickets_in(&project.root),
            project: Some(project.name),
        }]);
    }

    let mut groups = vec![Group {
        project: None,
        tickets: tickets_in(tickets_directory),
    }];
    for project in list_projects(tickets_directory)? {
        groups.push(Group {
            tickets: tickets_in(&project.root),
            project: Some(project.name),
        });
    }
    Ok(groups)
}

/// The tickets directly inside `directory`, sorted by key. A missing
/// directory has none.
fn tickets_in(directory: &Path) -> Vec<TicketConfig> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut tickets: Vec<TicketConfig> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let ticket_path = entry.path().join(".tix").join("ticket.toml");
        if !ticket_path.is_file() {
            continue;
        }
        let parsed: Result<Option<TicketConfig>, SdkError> =
            TixDocument::load(&ticket_path).and_then(|doc| doc.section("ticket"));
        match parsed {
            Ok(Some(ticket)) => tickets.push(ticket),
            Ok(None) => warn!(path = %ticket_path.display(), "no [ticket] section — skipping"),
            Err(e) => {
                warn!(path = %ticket_path.display(), error = %e, "malformed ticket document — skipping")
            }
        }
    }
    tickets.sort_by(|a, b| a.key.cmp(&b.key));
    tickets
}

/// Renders the listing in the requested format.
///
/// The default format stays the flat list it always was until a project
/// exists — and when `filtered` to one project, since a single group needs
/// no heading. Otherwise loose tickets come first, unheaded, then each
/// project under its name with its tickets indented. The machine formats are
/// flat either way, every ticket carrying its `project` (null or absent when
/// loose).
fn print_groups(groups: &[Group], output: OutputType, filtered: bool) -> Result<(), SdkError> {
    match output {
        OutputType::Default => print!("{}", render_default(groups, filtered)),
        OutputType::Json => {
            let entries: Vec<serde_json::Value> = groups
                .iter()
                .flat_map(|group| {
                    group.tickets.iter().map(|t| {
                        serde_json::json!({
                            "key": t.key,
                            "description": t.description,
                            "project": group.project,
                            "worktrees": t.worktrees.len(),
                        })
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries).unwrap());
        }
        OutputType::Toml => {
            for group in groups {
                for ticket in &group.tickets {
                    let text = toml::to_string(ticket)?;
                    let project = group
                        .project
                        .as_ref()
                        .map(|name| format!("project = {}\n", toml::Value::from(name.as_str())))
                        .unwrap_or_default();
                    println!("[[ticket]]\n{project}{text}");
                }
            }
        }
    }
    Ok(())
}

/// The default, human-readable rendering.
fn render_default(groups: &[Group], filtered: bool) -> String {
    let grouped = !filtered && groups.iter().any(|g| g.project.is_some());
    let width = groups
        .iter()
        .flat_map(|g| &g.tickets)
        .map(|t| t.key.len())
        .max()
        .unwrap_or(0);
    let line = |indent: &str, ticket: &TicketConfig| {
        if ticket.description.is_empty() {
            format!("{indent}{}\n", ticket.key)
        } else {
            format!("{indent}{:width$}  {}\n", ticket.key, ticket.description)
        }
    };

    let mut out = String::new();
    for group in groups {
        match (&group.project, grouped) {
            (Some(name), true) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(name);
                out.push('\n');
                if group.tickets.is_empty() {
                    out.push_str("  no tickets\n");
                }
                for ticket in &group.tickets {
                    out.push_str(&line("  ", ticket));
                }
            }
            _ => {
                for ticket in &group.tickets {
                    out.push_str(&line("", ticket));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_ticket(root: &Path, key: &str, description: &str) {
        std::fs::create_dir_all(root.join(".tix")).unwrap();
        std::fs::write(
            root.join(".tix/ticket.toml"),
            format!("[ticket]\nkey = \"{key}\"\ndescription = \"{description}\"\n"),
        )
        .unwrap();
    }

    fn keys(group: &Group) -> Vec<&str> {
        group.tickets.iter().map(|t| t.key.as_str()).collect()
    }

    /// A tickets directory holding loose tickets, two projects (one empty),
    /// and a stray directory with a ticket inside it.
    fn fixture(tickets: &Path) {
        make_ticket(&tickets.join("JIRA-2"), "JIRA-2", "loose two");
        make_ticket(&tickets.join("JIRA-1"), "JIRA-1", "");
        Project::named(tickets, "acme").unwrap().ensure().unwrap();
        make_ticket(&tickets.join("acme/ACME-1"), "ACME-1", "in acme");
        Project::named(tickets, "empty").unwrap().ensure().unwrap();
        make_ticket(&tickets.join("stray/STRAY-1"), "STRAY-1", "");
    }

    /// Loose tickets first, then each project in name order; a stray
    /// directory is not a project and its ticket is not listed.
    #[test]
    fn test_groups_by_project() {
        let dir = tempdir().unwrap();
        fixture(dir.path());

        let groups = collect(dir.path(), None).unwrap();
        let names: Vec<Option<&str>> = groups.iter().map(|g| g.project.as_deref()).collect();
        assert_eq!(names, [None, Some("acme"), Some("empty")]);
        assert_eq!(keys(&groups[0]), ["JIRA-1", "JIRA-2"]);
        assert_eq!(keys(&groups[1]), ["ACME-1"]);
        assert!(groups[2].tickets.is_empty());

        assert_eq!(
            render_default(&groups, false),
            "JIRA-1\nJIRA-2  loose two\n\nacme\n  ACME-1  in acme\n\nempty\n  no tickets\n"
        );
    }

    /// `--project` narrows the listing to one project, rendered flat.
    #[test]
    fn test_filter_to_one_project() {
        let dir = tempdir().unwrap();
        fixture(dir.path());

        let groups = collect(dir.path(), Some("acme")).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(keys(&groups[0]), ["ACME-1"]);
        assert_eq!(render_default(&groups, true), "ACME-1  in acme\n");
    }

    /// Filtering to a directory that is not a project — missing, or stray —
    /// errors rather than printing nothing.
    #[test]
    fn test_filter_to_non_project_errors() {
        let dir = tempdir().unwrap();
        fixture(dir.path());
        assert!(collect(dir.path(), Some("nope")).is_err());
        assert!(collect(dir.path(), Some("stray")).is_err());
    }

    /// With no projects at all, the listing is the flat list it always was.
    #[test]
    fn test_no_projects_renders_flat() {
        let dir = tempdir().unwrap();
        make_ticket(&dir.path().join("JIRA-1"), "JIRA-1", "one");
        std::fs::create_dir_all(dir.path().join("stray")).unwrap();

        let groups = collect(dir.path(), None).unwrap();
        assert_eq!(render_default(&groups, false), "JIRA-1  one\n");
    }

    /// No tickets directory yet simply means no tickets.
    #[test]
    fn test_missing_tickets_directory() {
        let dir = tempdir().unwrap();
        let groups = collect(&dir.path().join("nope"), None).unwrap();
        assert_eq!(render_default(&groups, false), "");
    }
}
