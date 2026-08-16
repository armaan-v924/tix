use crate::tix::context::Context;
use tix_engine::TixError;
use crate::tix::repo::{RepoAlias, RepoRef};

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(short, long)]
    pub alias: Option<RepoAlias>,

    pub repo: RepoRef,
}

pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
