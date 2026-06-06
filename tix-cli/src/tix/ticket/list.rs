use super::TicketSharedArgs;
use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
