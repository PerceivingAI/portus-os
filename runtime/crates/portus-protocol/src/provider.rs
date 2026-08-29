use crate::ProviderRegistrationId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceValueError {
    field: &'static str,
}

impl ResourceValueError {
    const fn empty(field: &'static str) -> Self {
        Self { field }
    }
}

impl fmt::Display for ResourceValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} must not be empty", self.field)
    }
}

impl Error for ResourceValueError {}

macro_rules! non_empty_string_type {
    ($name:ident, $field:literal, $debug_value:expr) => {
        #[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ResourceValueError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(ResourceValueError::empty($field));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                if $debug_value {
                    f.debug_tuple(stringify!($name)).field(&self.0).finish()
                } else {
                    write!(f, "{}(<opaque>)", stringify!($name))
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

non_empty_string_type!(ResourceType, "resource_type", true);
non_empty_string_type!(ProviderResourceId, "resource_id", false);

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderResourceRef {
    pub provider_registration_id: ProviderRegistrationId,
    pub resource_type: ResourceType,
    pub resource_id: ProviderResourceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
}

impl ProviderResourceRef {
    #[must_use]
    pub const fn new(
        provider_registration_id: ProviderRegistrationId,
        resource_type: ResourceType,
        resource_id: ProviderResourceId,
    ) -> Self {
        Self {
            provider_registration_id,
            resource_type,
            resource_id,
            generation: None,
        }
    }

    #[must_use]
    pub fn with_generation(mut self, generation: impl Into<String>) -> Self {
        self.generation = Some(generation.into());
        self
    }
}

impl fmt::Debug for ProviderResourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderResourceRef")
            .field("provider_registration_id", &self.provider_registration_id)
            .field("resource_type", &self.resource_type)
            .field("resource_id", &"<opaque>")
            .field("generation", &self.generation.as_ref().map(|_| "<opaque>"))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_resource_ref_round_trips_without_generic_parsing() {
        let reference = ProviderResourceRef::new(
            ProviderRegistrationId::new(),
            ResourceType::new("browser-tab").unwrap(),
            ProviderResourceId::new("tab:internal-provider-value").unwrap(),
        )
        .with_generation("session-7");

        let json = serde_json::to_string(&reference).unwrap();
        let decoded: ProviderResourceRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, reference);
    }

    #[test]
    fn debug_does_not_emit_opaque_provider_resource_id() {
        let reference = ProviderResourceRef::new(
            ProviderRegistrationId::new(),
            ResourceType::new("test-resource").unwrap(),
            ProviderResourceId::new("do-not-log-this-id").unwrap(),
        )
        .with_generation("do-not-log-generation");

        let debug = format!("{reference:?}");
        assert!(!debug.contains("do-not-log-this-id"));
        assert!(!debug.contains("do-not-log-generation"));
    }

    #[test]
    fn empty_resource_fields_are_rejected() {
        assert!(ResourceType::new("").is_err());
        assert!(ProviderResourceId::new("").is_err());
    }
}
