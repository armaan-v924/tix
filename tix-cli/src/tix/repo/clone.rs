use crate::tix::context::Context;
use tix_engine::TixError;
use crate::tix::repo::RepoAlias;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub repo_aliases: Vec<RepoAlias>,
}

pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
