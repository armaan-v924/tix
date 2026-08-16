use crate::tix::context::Context;
use super::ConfigKey;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub key: ConfigKey,
    pub value: String,
}

pub fn run(_context: &Context, args: Args) {
    println!("{:#?}", args);
}
