use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdParseError {
    InvalidPrefix { expected: &'static str },
    InvalidUuid,
    WrongUuidVersion { expected: u8, actual: Option<usize> },
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix { expected } => {
                write!(f, "identifier must start with '{expected}'")
            }
            Self::InvalidUuid => f.write_str("identifier contains an invalid UUID"),
            Self::WrongUuidVersion { expected, actual } => match actual {
                Some(actual) => write!(f, "identifier requires UUIDv{expected}, got UUIDv{actual}"),
                None => write!(
                    f,
                    "identifier requires UUIDv{expected}, got an unversioned UUID"
                ),
            },
        }
    }
}

impl Error for IdParseError {}

fn parse_prefixed_uuid(value: &str, prefix: &'static str) -> Result<Uuid, IdParseError> {
    let raw = value
        .strip_prefix(prefix)
        .ok_or(IdParseError::InvalidPrefix { expected: prefix })?;
    let uuid = Uuid::parse_str(raw).map_err(|_| IdParseError::InvalidUuid)?;
    let version = uuid.get_version_num();
    if version != 7 {
        return Err(IdParseError::WrongUuidVersion {
            expected: 7,
            actual: Some(version),
        });
    }
    Ok(uuid)
}

macro_rules! define_v7_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", Self::PREFIX, self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.to_string())
                    .finish()
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                parse_prefixed_uuid(value, Self::PREFIX).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

define_v7_id!(TaskId, "task_");
define_v7_id!(ProviderRegistrationId, "provider_");
define_v7_id!(ArtifactId, "artifact_");
define_v7_id!(IndexHandle, "idx_");
define_v7_id!(RequestId, "req_");

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_round_trip_as_prefixed_uuid_v7() {
        let ids = [
            TaskId::new().to_string(),
            ProviderRegistrationId::new().to_string(),
            ArtifactId::new().to_string(),
            IndexHandle::new().to_string(),
            RequestId::new().to_string(),
        ];

        assert!(ids[0].parse::<TaskId>().is_ok());
        assert!(ids[1].parse::<ProviderRegistrationId>().is_ok());
        assert!(ids[2].parse::<ArtifactId>().is_ok());
        assert!(ids[3].parse::<IndexHandle>().is_ok());
        assert!(ids[4].parse::<RequestId>().is_ok());
    }

    #[test]
    fn wrong_prefix_is_rejected() {
        let task = TaskId::new().to_string();
        let error = task.parse::<ProviderRegistrationId>().unwrap_err();
        assert!(matches!(error, IdParseError::InvalidPrefix { .. }));
    }

    #[test]
    fn non_v7_uuid_is_rejected() {
        let value = "task_550e8400-e29b-41d4-a716-446655440000";
        let error = value.parse::<TaskId>().unwrap_err();
        assert!(matches!(
            error,
            IdParseError::WrongUuidVersion {
                expected: 7,
                actual: Some(4)
            }
        ));
    }

    #[test]
    fn generated_ids_do_not_repeat_in_sample() {
        let ids: HashSet<_> = (0..4096).map(|_| TaskId::new()).collect();
        assert_eq!(ids.len(), 4096);
    }

    #[test]
    fn serde_revalidates_identifier_contract() {
        let id = ArtifactId::new();
        let json = serde_json::to_string(&id).unwrap();
        let decoded: ArtifactId = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, id);

        let invalid = r#""artifact_550e8400-e29b-41d4-a716-446655440000""#;
        assert!(serde_json::from_str::<ArtifactId>(invalid).is_err());
    }
}
