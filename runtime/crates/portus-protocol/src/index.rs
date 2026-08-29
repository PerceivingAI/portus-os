use crate::{EvidenceStrength, Freshness, IndexHandle, Principal};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexEnumParseError {
    kind: &'static str,
    value: String,
}

impl IndexEnumParseError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_string(),
        }
    }
}

impl fmt::Display for IndexEnumParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported {} value '{}'", self.kind, self.value)
    }
}

impl Error for IndexEnumParseError {}

macro_rules! index_string_enum {
    ($name:ident, $kind:literal, { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = IndexEnumParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(IndexEnumParseError::new($kind, value)),
                }
            }
        }
    };
}

index_string_enum!(IndexResourceType, "index resource type", {
    ApplicationDefinition => "application_definition",
    ApplicationInstance => "application_instance",
    Process => "process",
    OpenRcService => "openrc_service",
    Window => "window",
    Workspace => "workspace",
    Display => "display",
    ProviderRegistration => "provider_registration",
    ProviderResource => "provider_resource",
    RegisteredCapability => "registered_capability",
});

index_string_enum!(IndexSourceKind, "index source kind", {
    Applications => "applications",
    Proc => "proc",
    OpenRc => "openrc",
    X11 => "x11",
    I3 => "i3",
    Providers => "providers",
    Correlation => "correlation",
});

index_string_enum!(ControlPathKind, "control path kind", {
    RegisteredProvider => "registered_provider",
    StructuredApi => "structured_api",
    StructuredCli => "structured_cli",
    ApplicationAdapter => "application_adapter",
    NativeSystem => "native_system",
    Accessibility => "accessibility",
    ProcessWindow => "process_window",
    VisualFallback => "visual_fallback",
});

index_string_enum!(IndexHealthState, "index health state", {
    Initializing => "initializing",
    Healthy => "healthy",
    Degraded => "degraded",
    Rebuilding => "rebuilding",
    Unavailable => "unavailable",
});

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexObservationInput {
    pub resource_type: IndexResourceType,
    pub source_id: String,
    pub source_kind: IndexSourceKind,
    pub source_generation: String,
    pub native_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoritative_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Principal>,
    pub freshness: Freshness,
    pub observed_at_ms: i64,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub control_paths: Vec<ControlPathKind>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexObservation {
    pub index_handle: IndexHandle,
    pub resource_type: IndexResourceType,
    pub source_id: String,
    pub source_kind: IndexSourceKind,
    pub source_generation: String,
    pub native_identity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoritative_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Principal>,
    pub freshness: Freshness,
    pub observed_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub control_paths: Vec<ControlPathKind>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexRelationInput {
    pub from_authoritative_ref: String,
    pub to_authoritative_ref: String,
    pub relation_kind: String,
    pub evidence_strength: EvidenceStrength,
    pub source_id: String,
    pub source_kind: IndexSourceKind,
    pub reason_code: String,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexRelation {
    pub from_handle: IndexHandle,
    pub to_handle: IndexHandle,
    pub relation_kind: String,
    pub evidence_strength: EvidenceStrength,
    pub source_id: String,
    pub source_kind: IndexSourceKind,
    pub reason_code: String,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexSourceStatus {
    pub source_id: String,
    pub source_kind: IndexSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Principal>,
    pub source_generation: String,
    pub health: crate::HealthState,
    pub reason_code: String,
    pub last_attempt_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexPage<T> {
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub partial: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_enums_use_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&IndexResourceType::ApplicationInstance).unwrap(),
            r#""application_instance""#
        );
        assert_eq!(
            IndexSourceKind::from_str("openrc").unwrap(),
            IndexSourceKind::OpenRc
        );
        assert_eq!(
            serde_json::to_string(&IndexResourceType::ProviderResource).unwrap(),
            r#""provider_resource""#
        );
        assert!(IndexResourceType::from_str("filesystem_file").is_err());
    }

    #[test]
    fn observation_round_trip_preserves_generation_identity() {
        let observation = IndexObservation {
            index_handle: IndexHandle::new(),
            resource_type: IndexResourceType::Process,
            source_id: "proc".into(),
            source_kind: IndexSourceKind::Proc,
            source_generation: "boot-a".into(),
            native_identity: "pid=42/start=100".into(),
            authoritative_ref: Some("process:boot-a:42:100".into()),
            owner: Some(Principal::new(1000, 1000)),
            freshness: Freshness::Recent,
            observed_at_ms: 10,
            updated_at_ms: 10,
            metadata: serde_json::json!({"pid":42,"comm":"demo"}),
            control_paths: vec![ControlPathKind::NativeSystem],
        };
        let encoded = serde_json::to_string(&observation).unwrap();
        let decoded: IndexObservation = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, observation);
    }
}
