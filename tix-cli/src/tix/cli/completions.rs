use crate::tix::context::Context;
use tix_engine::TixError;
use clap;

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Shells {
    Zsh,
    Bash,
    Fish,
}

#[derive(clap::Args, Debug)]
pub struct Args {
    pub shell: Shells,
}

pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
