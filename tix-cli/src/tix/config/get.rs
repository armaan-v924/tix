use crate::tix::config::ConfigKey;
use crate::tix::utils::OutputType;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(short, long)]
    pub output: Option<OutputType>,

    pub key: Vec<ConfigKey>,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
