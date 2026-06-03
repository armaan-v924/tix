use std::fmt;

/// An error that can occur during tix-engine operations.
pub enum TixError {
    /// A git2 operation failed.
    GitError(git2::Error),
    /// An I/O operation failed.
    IoError(std::io::Error),
    /// A TOML document could not be parsed.
    ParseError(toml::de::Error),
    /// A value could not be serialized to TOML.
    SerializationError(toml::ser::Error),
    /// The config file was not found at the given path.
    ConfigNotFound(String),
    /// The repository was not found at the given path.
    RepoNotFound(String),
    /// A freeform error message.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `From<io::Error>` wraps into `IoError`.
    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        assert!(matches!(TixError::from(io_err), TixError::IoError(_)));
    }

    /// `From<&str>` wraps into `Message`.
    #[test]
    fn test_from_str() {
        assert!(matches!(TixError::from("oops"), TixError::Message(_)));
    }

    /// `From<String>` wraps into `Message`.
    #[test]
    fn test_from_string() {
        assert!(matches!(TixError::from("oops".to_string()), TixError::Message(_)));
    }

    /// `From<toml::de::Error>` wraps into `ParseError`.
    #[test]
    fn test_from_toml_de_error() {
        let de_err = toml::from_str::<toml::Value>("[[[not valid").unwrap_err();
        assert!(matches!(TixError::from(de_err), TixError::ParseError(_)));
    }

    /// `Display` produces a non-empty string for every variant.
    #[test]
    fn test_display_non_empty() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "x");
        let git_err = git2::Repository::open("/nonexistent/path/for/tix/test").err().unwrap();
        let parse_err = toml::from_str::<toml::Value>("[[[").unwrap_err();

        let variants: Vec<TixError> = vec![
            TixError::GitError(git_err),
            TixError::IoError(io_err),
            TixError::ParseError(parse_err),
            TixError::ConfigNotFound("path".into()),
            TixError::RepoNotFound("path".into()),
            TixError::Message("something went wrong".into()),
        ];

        for err in variants {
            assert!(!err.to_string().is_empty(), "Display was empty for: {:?}", err);
        }
    }
}
