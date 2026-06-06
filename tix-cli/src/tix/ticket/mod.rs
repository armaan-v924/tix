pub mod add;
pub mod destroy;
pub mod info;
pub mod list;
pub mod remove;
pub mod setup;

// ---

use clap::{Args, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TicketRef {
    Path(PathBuf),
    Id(String),
}
impl FromStr for TicketRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let path = std::path::Path::new(s);
        if path.is_absolute() || s.starts_with("./") || s.starts_with("../") {
            Ok(TicketRef::Path(path.to_path_buf()))
        } else {
            Ok(TicketRef::Id(s.to_string()))
        }
    }
}

#[derive(Args)]
pub struct TicketArgs {
    #[command(subcommand)]
    pub command: TicketCommands,
}

#[derive(Args, Debug)]
pub struct TicketSharedArgs {
    #[arg(short, long, value_hint = clap::ValueHint::DirPath)]
    pub ticket: Option<TicketRef>,
}

#[derive(Subcommand)]
pub enum TicketCommands {
    Add(add::Args),
    Destroy(destroy::Args),
    Info(info::Args),
    List(list::Args),
    Remove(remove::Args),
    Setup(setup::Args),
}
