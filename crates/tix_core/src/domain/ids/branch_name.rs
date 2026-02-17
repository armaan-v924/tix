use crate::domain::errors::IdParseError;
use std::fmt;
use serde::{Deserialize, Serialize};

const MAX_LENGTH: usize = 255;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(input_string: &str) -> Result<Self, IdParseError> {
        let input_string = input_string.trim();
        if input_string.is_empty() {
            return Err(IdParseError::Empty);
        }
        if input_string.len() > MAX_LENGTH {
            return Err(IdParseError::TooLong { max: MAX_LENGTH });
        }

        for character in input_string.chars() {
            if character.is_control() || character.is_whitespace() {
                return Err(IdParseError::InvalidCharacters { ch: character });
            }
        }

        Ok(Self(input_string.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for BranchName {
    type Error = IdParseError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl AsRef<str> for BranchName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
