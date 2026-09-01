//! Documentation generator for the tix CLI.
//!
//! `cargo run -p xtask` walks the clap definition in `tix-cli` and writes
//! both the web reference (MDX, for the Starlight site under `docs/`) and
//! the man pages. Nothing it writes is edited by hand: `just check-docs`
//! regenerates and fails on any diff, so a flag added to the CLI cannot land
//! without its documentation.
//!
//! This is an xtask rather than a `build.rs` so that documentation is
//! generated on demand instead of on every build of the CLI.

mod config;
mod man;
mod mdx;
mod model;

use clap::CommandFactory;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tix_cli::tix::TixParser;

/// Where generated MDX lands, relative to the repository root.
const CONTENT_ROOT: &str = "docs/src/content/docs";

/// Where generated man pages land, relative to the repository root.
const MAN_ROOT: &str = "docs/man";

fn main() -> ExitCode {
    match generate(&repository_root()) {
        Ok(count) => {
            println!("wrote {count} files");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Generates every documentation artifact under `root`, returning how many
/// files were written.
///
/// # Errors
///
/// Any filesystem failure, and any roff rendering failure from `clap_mangen`.
fn generate(root: &Path) -> std::io::Result<usize> {
    // clap fills in implicit arguments and propagates globals into
    // subcommands at build time; walking an unbuilt command silently drops
    // both.
    let mut command = TixParser::command();
    command.build();

    let documented = model::document(&command);
    let mut written = 0;

    let mut pages = mdx::render(&documented);
    pages.push(config::render());
    for page in pages {
        write(
            &root.join(CONTENT_ROOT).join(&page.path),
            page.body.as_bytes(),
        )?;
        written += 1;
    }
    for page in man::render(&command)? {
        write(&root.join(MAN_ROOT).join(&page.path), &page.body)?;
        written += 1;
    }
    Ok(written)
}

/// Writes `contents` to `path`, creating parent directories as needed.
fn write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

/// The repository root, derived from this crate's location so the generator
/// works from any working directory.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ always has a parent")
        .to_path_buf()
}
