use std::fmt;

pub enum TixError {
    GitError(git2::Error),
    IoError(std::io::Error),
    ParseError(toml::de::Error),
    SerializationError(toml::ser::Error),
    ConfigNotFound(String),
    RepoNotFound(String),
    Message(String),
}

impl fmt::Display for TixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TixError::GitError(e) => write!(f, "git error: {}", e),
            TixError::IoError(e) => write!(f, "io error: {}", e),
            TixError::ParseError(e) => write!(f, "serde error: {}", e),
            TixError::SerializationError(e) => write!(f, "serialization error: {}", e),
            TixError::ConfigNotFound(s) => write!(f, "config not found: {}", s),
            TixError::RepoNotFound(s) => write!(f, "repo not found: {}", s),
            TixError::Message(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Debug for TixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for TixError {}

impl From<std::io::Error> for TixError {
    fn from(e: std::io::Error) -> Self {
        TixError::IoError(e)
    }
}

impl From<toml::de::Error> for TixError {
    fn from(e: toml::de::Error) -> Self {
        TixError::ParseError(e)
    }
}

impl From<toml::ser::Error> for TixError {
    fn from(e: toml::ser::Error) -> Self {
        TixError::SerializationError(e)
    }
}

impl From<git2::Error> for TixError {
    fn from(e: git2::Error) -> Self {
        TixError::GitError(e)
    }
}

impl From<&str> for TixError {
    fn from(s: &str) -> Self {
        TixError::Message(s.to_string())
    }
}

impl From<String> for TixError {
    fn from(s: String) -> Self {
        TixError::Message(s)
    }
}
