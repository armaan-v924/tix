//! `tix completions` — shell completion generation for built-in commands.
//!
//! Third-party plugin completions are not tix's problem: plugins are external
//! binaries with their own interfaces.

use crate::tix::context::Context;
use clap::CommandFactory;
use tix_engine::TixError;

/// Arguments for `tix completions`.
#[derive(clap::Args, Debug)]
#[command(after_help = "\
Installation:
  bash:        tix completions bash > /etc/bash_completion.d/tix
               (or: ~/.local/share/bash-completion/completions/tix)
  zsh:         tix completions zsh > ~/.zfunc/_tix
               (with `fpath+=~/.zfunc; autoload -Uz compinit; compinit` in ~/.zshrc)
  fish:        tix completions fish > ~/.config/fish/completions/tix.fish
  elvish:      tix completions elvish > ~/.config/elvish/lib/tix.elv
  powershell:  tix completions powershell >> $PROFILE")]
pub struct Args {
    /// The shell to generate completions for
    pub shell: clap_complete::Shell,
}

/// Generates completions from the clap command definition onto stdout — the
/// user pipes them into their shell's completion path (see `--help`).
pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    let mut command = crate::tix::TixParser::command();
    clap_complete::generate(
        args.shell,
        &mut command,
        "tix",
        &mut std::io::stdout().lock(),
    );
    Ok(())
}
