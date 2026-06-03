use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt};

mod tix;

fn main() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(Level::INFO.as_str()));

    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stdout)
        .init();
}
