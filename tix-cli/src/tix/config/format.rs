//! `tix config fmt` — rewrite a document in house style, on demand.

use crate::tix::ticket::{TicketRef, require_ticket_root};
use crate::tix::utils::{App, confirm};
use std::path::{Path, PathBuf};
use tix_sdk::SdkError;
use tix_sdk::document::{TixDocument, with_write};

/// Rewrite the global config (or a ticket document) in tix's house style
///
/// Every table becomes a headered `[section]` block. That includes inline
/// tables written on purpose (`alpha = { remote = "…" }`) and dotted keys
/// (`alpha.remote = "…"`), not only the one-line shape older versions of
/// tix collapsed sections into. Plugin tables are reformatted the same way
/// and their data is left as it is. Inline tables inside arrays have no
/// headered form and are left alone, as is anything already in house
/// style. The document's data never changes, and no version is stamped.
///
/// Unlike ordinary writes, this rewrites the document wholesale, so
/// comments and formatting are not guaranteed to survive. Comments near a
/// reformatted table are moved above its new header where possible. The
/// command asks before writing; `--force` (or `--yes`) skips the question.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Format a ticket's `.tix/ticket.toml` instead of the global config:
    /// a bare id resolved under the tickets directory, or a path (anything
    /// containing a separator, or `.`/`..`)
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    pub ticket: Option<TicketRef>,

    /// Write and print nothing; exit non-zero when the document is not
    /// already formatted (for pre-commit hooks and CI)
    #[arg(long, conflicts_with_all = ["dry_run", "force"])]
    pub check: bool,

    /// Print the formatted document to stdout instead of writing it
    #[arg(long, conflicts_with = "force")]
    pub dry_run: bool,

    /// Rewrite without confirming
    #[arg(short, long, visible_alias = "yes", visible_short_alias = 'y')]
    pub force: bool,
}

/// Normalizes the global config, or the `--ticket` document, through
/// [`TixDocument::normalize`].
///
/// `--check` and the write path share one test,
/// [`TixDocument::is_normalized`], so `--check` passing means a real run
/// would be a no-op. A document already in house style is not rewritten at
/// all, not even with identical bytes.
///
/// `--check` reports a failure by exit code alone, so it exits the process
/// itself: the usual error path would print `error: …` to stderr, and a
/// check that fails is a result, not an error.
pub fn run(app: &App, args: Args) -> Result<(), SdkError> {
    let path = document_path(app, args.ticket.as_ref())?;

    if args.check {
        if !check(&path)? {
            std::process::exit(1);
        }
        return Ok(());
    }

    let document = TixDocument::load(&path)?;
    if args.dry_run {
        let mut formatted = document;
        formatted.normalize();
        print!("{formatted}");
        return Ok(());
    }

    if document.is_normalized() {
        println!("{} is already formatted", path.display());
        return Ok(());
    }

    if !args.force
        && !confirm(&format!(
            "Rewrite {} in tix's house style? The whole document is rewritten: \
             comments and formatting may not survive",
            path.display()
        ))?
    {
        println!("aborted");
        return Ok(());
    }

    write_formatted(&path)?;
    println!("formatted {}", path.display());
    Ok(())
}

/// The document `fmt` operates on: the global config, or the `--ticket`
/// document resolved the way every ticket command resolves it.
///
/// Without `--ticket` this is the global config, *not* the ticket the
/// current directory is in. The command is `tix config …`, and walking up
/// to a ticket implicitly would make it rewrite a different file depending
/// on where it ran. `--ticket .` names the ticket at the current directory.
fn document_path(app: &App, ticket: Option<&TicketRef>) -> Result<PathBuf, SdkError> {
    match ticket {
        Some(ticket) => Ok(require_ticket_root(&app.context, Some(ticket))?
            .join(".tix")
            .join("ticket.toml")),
        None => Ok(app.context.config_path.clone()),
    }
}

/// Whether the document at `path` is already in house style — the test
/// behind `--check`.
fn check(path: &Path) -> Result<bool, SdkError> {
    Ok(TixDocument::load(path)?.is_normalized())
}

/// Normalizes the document at `path` in place, through the locked atomic
/// write. Returns whether anything was written.
///
/// The document is re-parsed under the write lock, so a write that landed
/// since the caller's read is formatted along with everything else rather
/// than lost.
fn write_formatted(path: &Path) -> Result<bool, SdkError> {
    if check(path)? {
        return Ok(false);
    }
    with_write(path, |document| {
        document.normalize();
        Ok(())
    })?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Documents in and out of house style, including the #146 shape and a
    /// collapsed ticket document.
    const DOCUMENTS: &[&str] = &[
        "# global tix config\n\
         engine = { configured_repositories = { alpha.remote = \"a.git\", \
         alpha.code_path = \"/code/alpha\" } }\n\
         [cli]\ntickets_directory = \"/t\"\ncode_directory = \"/code\"\n\n\
         # a plugin\n[myplugin]\nopts = { retries = 3 }\n",
        "[cli]\ntickets_directory = \"/t\"\ncode_directory = \"/code\"\n\n\
         [engine.configured_repositories.alpha]\nremote = \"a.git\"\n\
         code_path = \"/code/alpha\"\n",
        "[ticket]\nkey = \"T-1\"\ndescription = \"d\"\n\
         worktrees = { api.repo = \"api\", api.branch = \"T-1\" }\n",
        "[defaults]\nbranch_prefix = \"feature\" # kept\n",
    ];

    /// `--check` passes exactly when a real run leaves the file untouched,
    /// and fails exactly when it changes it. After one run, `--check`
    /// passes and a second run is a no-op.
    #[test]
    fn test_check_passes_iff_fmt_is_a_no_op() {
        let dir = tempdir().unwrap();
        for (index, text) in DOCUMENTS.iter().enumerate() {
            let path = dir.path().join(format!("doc{index}.toml"));
            std::fs::write(&path, text).unwrap();

            let passed = check(&path).unwrap();
            let wrote = write_formatted(&path).unwrap();
            let after = std::fs::read_to_string(&path).unwrap();
            assert_eq!(passed, after == *text, "{text}\n---\n{after}");
            assert_eq!(wrote, !passed, "{text}");

            assert!(check(&path).unwrap(), "{after}");
            assert!(!write_formatted(&path).unwrap(), "{after}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
        }
        // Both kinds are exercised.
        assert!(
            DOCUMENTS
                .iter()
                .any(|text| !TixDocument::parse(text).unwrap().is_normalized())
        );
        assert!(
            DOCUMENTS
                .iter()
                .any(|text| TixDocument::parse(text).unwrap().is_normalized())
        );
    }

    /// Formatting changes no data and adds nothing, so it stamps no
    /// document version (only `tix config init` and `tix migrate` do).
    #[test]
    fn test_fmt_does_not_stamp_a_version() {
        let dir = tempdir().unwrap();
        for (index, text) in DOCUMENTS.iter().enumerate() {
            let path = dir.path().join(format!("doc{index}.toml"));
            std::fs::write(&path, text).unwrap();
            write_formatted(&path).unwrap();

            let before: toml::Table = toml::from_str(text).unwrap();
            let after: toml::Table =
                toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(before, after);
            assert!(!after.contains_key("version"), "{after:?}");
        }
    }

    /// No `--ticket` means the global config; `--ticket <path>` means that
    /// ticket's document.
    #[test]
    fn test_document_path_resolution() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let tickets = dir.path().join("tickets");
        let ticket_root = tickets.join("T-1");
        std::fs::create_dir_all(ticket_root.join(".tix")).unwrap();
        std::fs::write(ticket_root.join(".tix/ticket.toml"), DOCUMENTS[2]).unwrap();
        std::fs::write(
            &config_path,
            format!(
                "[cli]\ntickets_directory = {:?}\ncode_directory = \"/code\"\n",
                tickets.display().to_string()
            ),
        )
        .unwrap();
        let app = App {
            context: tix_sdk::context::Context {
                config_path: config_path.clone(),
            },
            output: crate::tix::utils::OutputType::Default,
            log_level: tracing::Level::INFO,
        };

        assert_eq!(document_path(&app, None).unwrap(), config_path);
        for selector in ["T-1", ticket_root.to_str().unwrap()] {
            let ticket: TicketRef = selector.parse().unwrap();
            assert_eq!(
                document_path(&app, Some(&ticket)).unwrap(),
                ticket_root.join(".tix").join("ticket.toml"),
                "{selector}"
            );
        }
    }
}
