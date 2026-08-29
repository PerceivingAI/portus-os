use crate::{ArtifactId, Principal, ProviderResourceRef, TaskId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, str::FromStr};

macro_rules! artifact_string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(format!("unknown {} value", stringify!($name))),
                }
            }
        }
    };
}

artifact_string_enum!(ArtifactType {
    File => "file",
    Report => "report",
    Release => "release",
    DiagnosticBundle => "diagnostic_bundle",
    Screenshot => "screenshot",
    Archive => "archive",
    Other => "other",
});

artifact_string_enum!(ArtifactConfidentiality {
    Private => "private",
    Shared => "shared",
    Public => "public",
});

artifact_string_enum!(ArtifactRetentionKind {
    Temporary => "temporary",
    Retained => "retained",
    Until => "until",
});

artifact_string_enum!(ArtifactAvailabilityState {
    Available => "available",
    Missing => "missing",
    Unavailable => "unavailable",
    Removed => "removed",
});

artifact_string_enum!(ArtifactIntegrityKind {
    Verified => "verified",
    Mismatch => "mismatch",
    ProviderAuthoritative => "provider_authoritative",
    Unverified => "unverified",
    NotApplicable => "not_applicable",
});

artifact_string_enum!(ArtifactCleanupAuthority {
    None => "none",
    Portus => "portus",
    Task => "task",
    Provider => "provider",
});

artifact_string_enum!(ArtifactTaskRelationshipKind {
    ProducedBy => "produced_by",
    RequiredBy => "required_by",
});

artifact_string_enum!(ArtifactHoldKind {
    Explicit => "explicit",
    Task => "task",
    Recovery => "recovery",
    Audit => "audit",
    Delivery => "delivery",
});

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactLocator {
    Filesystem { path: String },
    ProviderResource { reference: ProviderResourceRef },
}

impl ArtifactLocator {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Filesystem { .. } => "filesystem",
            Self::ProviderResource { .. } => "provider_resource",
        }
    }
}

impl fmt::Debug for ArtifactLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Filesystem { .. } => f.write_str("Filesystem(<path>)"),
            Self::ProviderResource { reference } => {
                f.debug_tuple("ProviderResource").field(reference).finish()
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactTaskRelationship {
    pub task_id: TaskId,
    pub kind: ArtifactTaskRelationshipKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactHold {
    pub kind: ArtifactHoldKind,
    pub holder_ref: String,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRegistrationSpec {
    pub artifact_id: ArtifactId,
    pub owner: Principal,
    pub artifact_type: ArtifactType,
    pub confidentiality: ArtifactConfidentiality,
    pub retention_kind: ArtifactRetentionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    pub locator: ArtifactLocator,
    pub integrity_kind: ArtifactIntegrityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub created_at_ms: i64,
    pub registered_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_display_name: Option<String>,
    #[serde(default)]
    pub safe_metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_task_id: Option<TaskId>,
    #[serde(default)]
    pub shared_with: Vec<Principal>,
    pub cleanup_authority: ArtifactCleanupAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_ref: Option<String>,
}

impl fmt::Debug for ArtifactRegistrationSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactRegistrationSpec")
            .field("artifact_id", &self.artifact_id)
            .field("owner", &self.owner)
            .field("artifact_type", &self.artifact_type)
            .field("confidentiality", &self.confidentiality)
            .field("retention_kind", &self.retention_kind)
            .field("locator", &self.locator)
            .field("integrity_kind", &self.integrity_kind)
            .field("has_sha256", &self.sha256.is_some())
            .field("size_bytes", &self.size_bytes)
            .field(
                "safe_metadata_keys",
                &self.safe_metadata.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_id: ArtifactId,
    pub owner: Principal,
    pub artifact_type: ArtifactType,
    pub confidentiality: ArtifactConfidentiality,
    pub retention_kind: ArtifactRetentionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    pub availability_state: ArtifactAvailabilityState,
    pub locator: ArtifactLocator,
    pub integrity_kind: ArtifactIntegrityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub created_at_ms: i64,
    pub registered_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_display_name: Option<String>,
    #[serde(default)]
    pub safe_metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_at_ms: Option<i64>,
    pub cleanup_authority: ArtifactCleanupAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_ref: Option<String>,
}

impl fmt::Debug for ArtifactRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactRecord")
            .field("artifact_id", &self.artifact_id)
            .field("owner", &self.owner)
            .field("artifact_type", &self.artifact_type)
            .field("confidentiality", &self.confidentiality)
            .field("retention_kind", &self.retention_kind)
            .field("availability_state", &self.availability_state)
            .field("locator", &self.locator)
            .field("integrity_kind", &self.integrity_kind)
            .field("has_sha256", &self.sha256.is_some())
            .field("size_bytes", &self.size_bytes)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactSummary {
    pub artifact_id: ArtifactId,
    pub owner: Principal,
    pub artifact_type: ArtifactType,
    pub confidentiality: ArtifactConfidentiality,
    pub retention_kind: ArtifactRetentionKind,
    pub availability_state: ArtifactAvailabilityState,
    pub integrity_kind: ArtifactIntegrityKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_display_name: Option<String>,
    pub registered_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactView {
    pub artifact: ArtifactRecord,
    #[serde(default)]
    pub task_relationships: Vec<ArtifactTaskRelationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_resource: Option<ProviderResourceRef>,
    #[serde(default)]
    pub shared_with: Vec<Principal>,
    #[serde(default)]
    pub holds: Vec<ArtifactHold>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactPage {
    pub items: Vec<ArtifactSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProviderRegistrationId, ProviderResourceId, ResourceType};

    #[test]
    fn artifact_enums_have_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&ArtifactType::DiagnosticBundle).unwrap(),
            "\"diagnostic_bundle\""
        );
        assert_eq!(
            ArtifactRetentionKind::from_str("retained").unwrap(),
            ArtifactRetentionKind::Retained
        );
        assert!(ArtifactIntegrityKind::from_str("changed").is_err());
    }

    #[test]
    fn locator_debug_redacts_paths_and_provider_resource_ids() {
        let filesystem = ArtifactLocator::Filesystem {
            path: "/private/report.pdf".into(),
        };
        assert!(!format!("{filesystem:?}").contains("report.pdf"));

        let provider = ArtifactLocator::ProviderResource {
            reference: ProviderResourceRef::new(
                ProviderRegistrationId::new(),
                ResourceType::new("document").unwrap(),
                ProviderResourceId::new("opaque-secretish-id").unwrap(),
            ),
        };
        assert!(!format!("{provider:?}").contains("opaque-secretish-id"));
    }
}
