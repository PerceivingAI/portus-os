use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use zeroize::Zeroize;

/// Bounded reusable secret material whose Debug representation is always redacted
/// and whose owned buffer is zeroed on drop.
pub struct SecretMaterial(String);

impl SecretMaterial {
    pub const MAX_BYTES: usize = 64 * 1024;

    pub fn new(value: String) -> Result<Self, &'static str> {
        if value.is_empty() || value.len() > Self::MAX_BYTES || value.contains(['\0', '\n', '\r']) {
            return Err("secret material is empty, oversized, or contains a line separator");
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[must_use]
    pub fn expose_for_serialization(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretMaterial {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretMaterial([REDACTED])")
    }
}

impl Serialize for SecretMaterial {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretMaterial {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}
