use super::ConfigKey;

use clap;

#[derive(clap::Args, Debug)]
pub struct Args {
    pub key: ConfigKey,
    pub value: String,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
