use crate::tix::context::Context;
use clap;

#[derive(clap::Args, Debug)]
pub struct Args {}

pub fn run(_context: &Context, args: Args) {
    println!("{:#?}", args);
}
