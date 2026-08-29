use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaginationError {
    EmptyCursor,
    ZeroLimit,
}

impl fmt::Display for PaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCursor => f.write_str("pagination cursor must not be empty"),
            Self::ZeroLimit => f.write_str("pagination limit must be greater than zero"),
        }
    }
}

impl Error for PaginationError {}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct OpaqueCursor(String);

impl OpaqueCursor {
    pub fn new(value: impl Into<String>) -> Result<Self, PaginationError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PaginationError::EmptyCursor);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpaqueCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for OpaqueCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpaqueCursor(<opaque>)")
    }
}

impl Serialize for OpaqueCursor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for OpaqueCursor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PageLimit(u32);

impl PageLimit {
    pub fn new(value: u32) -> Result<Self, PaginationError> {
        if value == 0 {
            return Err(PaginationError::ZeroLimit);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Serialize for PageLimit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for PageLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageRequest {
    pub limit: PageLimit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<OpaqueCursor>,
}

impl PageRequest {
    #[must_use]
    pub const fn new(limit: PageLimit, cursor: Option<OpaqueCursor>) -> Self {
        Self { limit, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_rejects_unbounded_shapes() {
        assert_eq!(PageLimit::new(0), Err(PaginationError::ZeroLimit));
        assert_eq!(OpaqueCursor::new(""), Err(PaginationError::EmptyCursor));
    }

    #[test]
    fn cursor_debug_is_opaque() {
        let cursor = OpaqueCursor::new("internal-position-token").unwrap();
        assert_eq!(format!("{cursor:?}"), "OpaqueCursor(<opaque>)");
        assert!(!format!("{cursor:?}").contains(cursor.as_str()));
    }

    #[test]
    fn page_request_round_trips() {
        let request = PageRequest::new(
            PageLimit::new(25).unwrap(),
            Some(OpaqueCursor::new("next-page").unwrap()),
        );
        let json = serde_json::to_string(&request).unwrap();
        let decoded: PageRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
    }
}
