use crate::domain::errors::IdParseError;
use std::fmt;
use std::fmt::Formatter;
use serde::{Deserialize, Serialize};

const MAX_LENGTH: usize = 64;

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
pub struct TicketId(String);

impl TicketId {
    pub fn parse(input_string: &str) -> Result<Self, IdParseError> {
        let input_string = input_string.trim();
        if input_string.is_empty() {
            return Err(IdParseError::Empty);
        }
        if input_string.len() > MAX_LENGTH {
            return Err(IdParseError::TooLong { max: MAX_LENGTH });
        }

        let (project_key, ticket_number) = input_string
            .split_once('-')
            .ok_or(IdParseError::InvalidFormat)?;
        if project_key.is_empty() || ticket_number.is_empty() {
            return Err(IdParseError::InvalidFormat);
        }

        if !project_key.chars().next().unwrap().is_ascii_uppercase() {
            return Err(IdParseError::InvalidFormat);
        }
        if !project_key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return Err(IdParseError::InvalidFormat);
        }
        if !ticket_number.chars().all(|c| c.is_ascii_digit()) {
            return Err(IdParseError::InvalidFormat);
        }

        Ok(Self(input_string.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for TicketId {
    type Error = IdParseError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl AsRef<str> for TicketId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid() {
        let id = TicketId::parse("ABC-123").unwrap();
        assert_eq!(id.as_str(), "ABC-123");
    }

    #[test]
    fn rejects_lowercase() {
        assert!(TicketId::parse("abc-123").is_err());
    }
}
