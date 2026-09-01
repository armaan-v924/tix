//! Renders the command tree as roff man pages, one per command.
//!
//! Walks `clap::Command` directly rather than the documentation model:
//! `clap_mangen` consumes a `Command`, and reformatting the model back into
//! one would be a lossy round trip for no gain.

use clap::Command;
use std::io;

/// One rendered man page.
pub struct ManPage {
    /// Filename, e.g. `tix-ticket-setup.1`.
    pub path: String,
    pub body: Vec<u8>,
}

/// Renders `command` and every subcommand beneath it.
pub fn render(command: &Command) -> io::Result<Vec<ManPage>> {
    let mut pages = Vec::new();
    walk(command, None, &mut pages)?;
    Ok(pages)
}

/// Renders one command, then recurses. `parent_slug` is the dashed name of
/// the enclosing command, e.g. `tix-ticket`.
fn walk(command: &Command, parent_slug: Option<&str>, out: &mut Vec<ManPage>) -> io::Result<()> {
    if !crate::model::is_documented(command) {
        return Ok(());
    }
    let slug = match parent_slug {
        Some(parent) => format!("{parent}-{}", command.get_name()),
        None => command.get_name().to_string(),
    };

    // `display_name` is what clap_mangen puts in the .TH title, so setting it
    // to the dashed path gives `tix-ticket-setup(1)` rather than `setup(1)`.
    let titled = command.clone().display_name(&slug);
    let mut body = Vec::new();
    clap_mangen::Man::new(titled).render(&mut body)?;
    out.push(ManPage {
        path: format!("{slug}.1"),
        body,
    });

    for sub in command.get_subcommands() {
        walk(sub, Some(&slug), out)?;
    }
    Ok(())
}
