use crate::tix::context::Context;
use tix_engine::TixError;
use clap;

#[derive(clap::Args, Debug)]
pub struct Args {}

pub fn run(_context: &Context, args: Args) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
