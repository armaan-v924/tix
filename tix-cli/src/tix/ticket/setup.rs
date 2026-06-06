use crate::tix::repo::RepoAlias;
use crate::tix::ticket::TicketRef;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(short, long)]
    pub description: Option<String>,

    #[arg(value_hint = clap::ValueHint::DirPath)]
    pub ticket: TicketRef,

    #[arg(short, long, group = "repos")]
    pub all: bool,

    #[arg(group = "repos")]
    pub repo_aliases: Vec<RepoAlias>,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
