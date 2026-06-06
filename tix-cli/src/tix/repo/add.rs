use crate::tix::repo::{RepoAlias, RepoRef};

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(short, long)]
    pub alias: Option<RepoAlias>,

    pub repo: RepoRef,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
