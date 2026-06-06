use clap;

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum Shells {
    Zsh,
    Bash,
    Fish,
}

#[derive(clap::Args, Debug)]
pub struct Args {
    pub shell: Shells,
}

pub fn run(args: Args) {
    println!("{:#?}", args);
}
