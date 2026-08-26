use crate::tix::context::Context;
use super::TicketSharedArgs;
use crate::tix::repo::RepoAlias;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,

    pub repo_aliases: Vec<RepoAlias>,
}

pub fn run(_context: &Context, args: Args) {
    println!("{:#?}", args);
}
