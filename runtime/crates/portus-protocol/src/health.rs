use crate::{HealthState, RecoveryDisposition};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthEnumParseError {
    kind: &'static str,
    value: String,
}

impl HealthEnumParseError {
    fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_string(),
        }
    }
}

impl fmt::Display for HealthEnumParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported {} value '{}'", self.kind, self.value)
    }
}

impl Error for HealthEnumParseError {}

macro_rules! health_string_enum {
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
            type Err = HealthEnumParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok(Self::$variant),)+
                    _ => Err(HealthEnumParseError::new($kind, value)),
                }
            }
        }
    };
}

health_string_enum!(HealthComponentType, "health component type", {
    Runtime => "runtime",
    State => "state",
    Index => "index",
    IndexSource => "index_source",
    ProviderRegistry => "provider_registry",
    Provider => "provider",
    Policy => "policy",
    Audit => "audit",
    TaskRuntime => "task_runtime",
    Privilege => "privilege",
    ProtectedApi => "protected_api",
    Storage => "storage",
    Memory => "memory",
    Service => "service",
    Codex => "codex",
});

health_string_enum!(HealthReasonCode, "health reason code", {
    Ready => "ready",
    Starting => "starting",
    Stopping => "stopping",
    NotProbed => "not_probed",
    StatusUnavailable => "status_unavailable",
    ServiceNotRunning => "service_not_running",
    ServiceRestartExhausted => "service_restart_exhausted",
    SocketUnavailable => "socket_unavailable",
    IpcFailed => "ipc_failed",
    StateUnavailable => "state_unavailable",
    StateIntegrityFailed => "state_integrity_failed",
    SourceDisconnected => "source_disconnected",
    SourceStale => "source_stale",
    ProviderDegraded => "provider_degraded",
    ProviderUnavailable => "provider_unavailable",
    UpstreamUnreachable => "upstream_unreachable",
    TlsFailure => "tls_failure",
    PolicyUnavailable => "policy_unavailable",
    AuditWriteFailed => "audit_write_failed",
    ResourceLow => "resource_low",
    ResourceCritical => "resource_critical",
    ResourceUnavailable => "resource_unavailable",
    ReconciliationRequired => "reconciliation_required",
    ReconciliationFailed => "reconciliation_failed",
    RebuildRequired => "rebuild_required",
    RebuildFailed => "rebuild_failed",
    ConfigurationInvalid => "configuration_invalid",
    Incompatible => "incompatible",
    RecoveryExhausted => "recovery_exhausted",
    ManualRecoveryRequired => "manual_recovery_required",
});

health_string_enum!(RecoveryActionKind, "recovery action kind", {
    Probe => "probe",
    Reconcile => "reconcile",
    Restart => "restart",
    Repair => "repair",
});

health_string_enum!(RecoveryAttemptOutcome, "recovery attempt outcome", {
    Succeeded => "succeeded",
    Failed => "failed",
    Exhausted => "exhausted",
    Skipped => "skipped",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HealthObservation {
    pub component_ref: String,
    pub component_type: HealthComponentType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<crate::Principal>,

    pub health_state: HealthState,
    pub reason_code: HealthReasonCode,
    pub summary: String,
    pub source: String,
    pub observed_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_healthy_at_ms: Option<i64>,
    pub recovery_disposition: RecoveryDisposition,
    pub recovery_attempt_count: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub safe_details: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    pub component_ref: String,
    pub action_kind: RecoveryActionKind,
    pub attempt_number: u16,
    pub started_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
    pub outcome: RecoveryAttemptOutcome,
    pub reason_code: HealthReasonCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_enums_have_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&HealthReasonCode::RecoveryExhausted).unwrap(),
            r#""recovery_exhausted""#
        );
        assert_eq!(
            "provider_registry".parse::<HealthComponentType>().unwrap(),
            HealthComponentType::ProviderRegistry
        );
        assert!("everything_is_fine".parse::<HealthReasonCode>().is_err());
    }

    #[test]
    fn observation_shape_is_typed_and_payload_free() {
        let observation = HealthObservation {
            component_ref: "runtime:portusd".into(),
            owner: None,
            component_type: HealthComponentType::Runtime,
            health_state: HealthState::Healthy,
            reason_code: HealthReasonCode::Ready,
            summary: "runtime is ready".into(),
            source: "portusd".into(),
            observed_at_ms: 10,
            source_generation: None,
            last_healthy_at_ms: Some(10),
            recovery_disposition: RecoveryDisposition::Observe,
            recovery_attempt_count: 0,
            safe_details: BTreeMap::new(),
        };
        let value = serde_json::to_value(observation).unwrap();
        assert_eq!(value["health_state"], "healthy");
        assert!(value.get("payload").is_none());
        assert!(value.get("environment").is_none());
    }
}
