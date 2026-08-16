//! The SDK's error type.
//!
//! Document parsing and serialization are SDK concerns (`design/spec.md`
//! §2.2) — the `ParseError`/`SerializationError` variants that used to live
//! on the engine's [`TixError`] migrate here, so `tix-engine` carries no
//! runtime `toml` dependency.

use std::fmt;
use tix_engine::TixError;

/// An error from the SDK's context-and-consistency layer, or from the
/// engine underneath it.
pub enum SdkError {
    /// An engine operation failed.
    Engine(TixError),
    /// A TOML document could not be parsed.
    Parse(toml::de::Error),
    /// A value could not be serialized to TOML.
    Serialization(toml::ser::Error),
    /// The config file was not found at the resolved path.
    ///
    /// Migrated from the engine: config location is an SDK concern, and the
    /// engine never touches config paths.
    ConfigNotFound(String),
    /// A freeform SDK-level error message.
    Message(String),
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SdkError::Engine(e) => fmt::Display::fmt(e, f),
            SdkError::Parse(e) => write!(f, "parse error: {}", e),
            SdkError::Serialization(e) => write!(f, "serialization error: {}", e),
            SdkError::ConfigNotFound(s) => write!(f, "config not found: {}", s),
            SdkError::Message(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Debug for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for SdkError {}

impl From<TixError> for SdkError {
    fn from(e: TixError) -> Self {
        SdkError::Engine(e)
    }
}

impl From<toml::de::Error> for SdkError {
    fn from(e: toml::de::Error) -> Self {
        SdkError::Parse(e)
    }
}

impl From<toml::ser::Error> for SdkError {
    fn from(e: toml::ser::Error) -> Self {
        SdkError::Serialization(e)
    }
}

impl From<std::io::Error> for SdkError {
    fn from(e: std::io::Error) -> Self {
        SdkError::Engine(TixError::IoError(e))
    }
}

impl From<String> for SdkError {
    fn from(s: String) -> Self {
        SdkError::Message(s)
    }
}

impl From<&str> for SdkError {
    fn from(s: &str) -> Self {
        SdkError::Message(s.to_string())
    }
}
