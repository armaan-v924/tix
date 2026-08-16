//! The smallest possible tix plugin — `tix-toy`.
//!
//! Demonstrates (and continuously compiles) the plugin side of the
//! invocation contract: parse host flags, answer `print-cli-help`, pass the
//! protocol check, read your own `[toy]` section, and see your user args.
//!
//! Try it by hand (plugins take real paths, so no host is needed):
//!
//! ```text
//! cargo run -p tix-sdk --example toy-plugin -- \
//!     --tix-protocol 1 --tix-config ~/.config/tix/config.toml hello --loud
//! ```

use serde::Deserialize;
use tix_sdk::document::TixDocument;
use tix_sdk::host::HostContext;

/// The plugin's own `[toy]` table in global config — its schema belongs to
/// the plugin alone; tix never deserializes it.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ToyConfig {
    greeting: Option<String>,
}

fn main() {
    // Handles print-cli-help, strips --tix-*, checks the protocol (exit 125
    // on mismatch), and errors usefully when run without --tix-config.
    let host = HostContext::from_env_or_exit("a toy plugin demonstrating the tix SDK");

    let document = TixDocument::load(&host.config_path).expect("config parses");
    let config: ToyConfig = document
        .section_or_default("toy")
        .expect("[toy] section parses");

    let greeting = config.greeting.unwrap_or_else(|| "hello".to_string());
    println!("{greeting} from tix-toy!");
    println!("  ticket:    {:?}", host.ticket_root);
    println!("  user args: {:?}", host.user_args);
}
