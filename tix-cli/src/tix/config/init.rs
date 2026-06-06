use clap;

#[derive(clap::Args, Debug)]
pub struct Args {}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
