use crate::tix::context::Context;
use tix_engine::TixError;
use super::TicketSharedArgs;
use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(flatten)]
    pub shared: TicketSharedArgs,
}

pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
