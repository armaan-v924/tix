use tix_sdk::context::Context;
use tix_sdk::SdkError;

pub fn run(_context: &Context, args: Vec<String>) -> Result<(), SdkError> {
    println!("{:#?}", args);
    Ok(())
}
