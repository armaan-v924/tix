use crate::tix::context::Context;
use tix_engine::TixError;

pub fn run(_context: &Context, args: Vec<String>) -> Result<(), TixError> {
    println!("{:#?}", args);
    Ok(())
}
