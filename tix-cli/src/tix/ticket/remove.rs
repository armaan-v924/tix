use super::TicketSharedArgs;
use crate::tix::repo::RepoAlias;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    pub repo_aliases: Vec<RepoAlias>,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
