use std::fmt;

pub enum TixError {
    Git(git2::Error),
    Io(std::io::Error),
    NotFound(String),
    Message(String),
}

impl fmt::Display for TixError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TixError::Git(e) => write!(f, "git error: {}", e),
            TixError::Io(e) => write!(f, "io error: {}", e),
            TixError::NotFound(s) => write!(f, "not found: {}", s),
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

impl From<git2::Error> for TixError {
    fn from(e: git2::Error) -> Self {
        TixError::Git(e)
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
