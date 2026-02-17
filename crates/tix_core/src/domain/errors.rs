use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum IdParseError {
    #[error("Value cannot be empty")]
    Empty,

    #[error("value is too long (max {max})")]
    TooLong { max: usize },

    #[error("invalid format")]
    InvalidFormat,

    #[error("invalid characters: {ch}")]
    InvalidCharacters { ch: char },
}
