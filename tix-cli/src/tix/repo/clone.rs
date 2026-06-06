use crate::tix::repo::RepoAlias;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub repo_aliases: Vec<RepoAlias>,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
