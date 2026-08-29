use crate::{
    CURRENT_PROTOCOL_VERSION, Principal, ProtocolVersion, RequestId, SemanticError, TaskEvent,
    TaskId,
};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

macro_rules! event_string_enum {
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

event_string_enum!(EventObjectKind {
    Task => "task",
    Provider => "provider",
    Policy => "policy",
    Runtime => "runtime",
    Index => "index",
    Artifact => "artifact",
    Health => "health",
    Privilege => "privilege",
    ProtectedApi => "protected_api",
});

event_string_enum!(AuditActorKind {
    Principal => "principal",
    System => "system",
});

event_string_enum!(AuditDomain {
    Task => "task",
    Provider => "provider",
    Policy => "policy",
    Runtime => "runtime",
    Index => "index",
    Artifact => "artifact",
    Health => "health",
    Privilege => "privilege",
    ProtectedApi => "protected_api",
    Visual => "visual",
});

event_string_enum!(AuditResult {
    Succeeded => "succeeded",
    Failed => "failed",
    Denied => "denied",
    ApprovalRequired => "approval_required",
    Cancelled => "cancelled",
    Interrupted => "interrupted",
});

event_string_enum!(TaskEventStreamFrameKind {
    Event => "event",
    End => "end",
    Error => "error",
});

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignificantEvent {
    pub object_kind: EventObjectKind,
    pub object_ref: String,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<Principal>,
    pub event_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_summary: Option<String>,
    #[serde(default)]
    pub safe_data: serde_json::Value,
    pub occurred_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SignificantEventPage {
    pub events: Vec<SignificantEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_from_sequence: Option<u64>,
    pub latest_sequence: u64,
    pub gap_before_page: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_sequence: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditActor {
    pub kind: AuditActorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<Principal>,
}

impl AuditActor {
    #[must_use]
    pub const fn principal(principal: Principal) -> Self {
        Self {
            kind: AuditActorKind::Principal,
            principal: Some(principal),
        }
    }

    #[must_use]
    pub const fn system() -> Self {
        Self {
            kind: AuditActorKind::System,
            principal: None,
        }
    }

    #[must_use]
    pub const fn is_valid(&self) -> bool {
        matches!(
            (self.kind, self.principal),
            (AuditActorKind::Principal, Some(_)) | (AuditActorKind::System, None)
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRecord {
    pub schema_version: u32,
    pub actor: AuditActor,
    pub domain: AuditDomain,
    pub action: String,
    pub result: AuditResult,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    pub occurred_at_ms: i64,
}

impl AuditRecord {
    pub const SCHEMA_VERSION: u32 = 1;

    #[must_use]
    pub fn new(
        actor: AuditActor,
        domain: AuditDomain,
        action: impl Into<String>,
        result: AuditResult,
        reason_code: impl Into<String>,
        occurred_at_ms: i64,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            actor,
            domain,
            action: action.into(),
            result,
            reason_code: reason_code.into(),
            target_ref: None,
            task_id: None,
            request_id: None,
            occurred_at_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskEventStreamFrame {
    pub version: ProtocolVersion,
    pub request_id: RequestId,
    pub stream: String,
    pub frame: TaskEventStreamFrameKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<TaskEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<SemanticError>,
}

impl TaskEventStreamFrame {
    #[must_use]
    pub fn event(request_id: RequestId, event: TaskEvent) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            stream: "task.events".into(),
            frame: TaskEventStreamFrameKind::Event,
            event: Some(event),
            terminal_state: None,
            error: None,
        }
    }

    #[must_use]
    pub fn end(request_id: RequestId, terminal_state: impl Into<String>) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            stream: "task.events".into(),
            frame: TaskEventStreamFrameKind::End,
            event: None,
            terminal_state: Some(terminal_state.into()),
            error: None,
        }
    }

    #[must_use]
    pub fn error(request_id: RequestId, error: SemanticError) -> Self {
        Self {
            version: CURRENT_PROTOCOL_VERSION,
            request_id,
            stream: "task.events".into(),
            frame: TaskEventStreamFrameKind::Error,
            event: None,
            terminal_state: None,
            error: Some(error),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.version
            .ensure_compatible()
            .map_err(|_| "incompatible stream protocol version")?;
        if self.stream != "task.events" {
            return Err("unexpected stream identifier");
        }
        match (
            self.frame,
            self.event.is_some(),
            self.terminal_state.is_some(),
            self.error.is_some(),
        ) {
            (TaskEventStreamFrameKind::Event, true, false, false)
            | (TaskEventStreamFrameKind::End, false, true, false)
            | (TaskEventStreamFrameKind::Error, false, false, true) => Ok(()),
            _ => Err("invalid task event stream frame shape"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SemanticErrorCode, TaskState};
    use serde_json::json;

    #[test]
    fn audit_record_has_only_allowlisted_top_level_fields() {
        let mut record = AuditRecord::new(
            AuditActor::principal(Principal::new(1000, 1000)),
            AuditDomain::Policy,
            "policy.check",
            AuditResult::Denied,
            "rule_reject",
            10,
        );
        record.target_ref = Some("action:network.expose".into());
        let value = serde_json::to_value(record).unwrap();
        let object = value.as_object().unwrap();
        assert_eq!(
            object.keys().cloned().collect::<Vec<_>>(),
            vec![
                "action",
                "actor",
                "domain",
                "occurred_at_ms",
                "reason_code",
                "result",
                "schema_version",
                "target_ref"
            ]
        );
        assert!(object.get("payload").is_none());
        assert!(object.get("headers").is_none());
        assert!(object.get("command").is_none());
    }

    #[test]
    fn visual_audit_domain_has_stable_wire_value() {
        assert_eq!(
            serde_json::to_string(&AuditDomain::Visual).unwrap(),
            r#""visual""#
        );
    }

    #[test]
    fn audit_actor_shape_is_explicit() {
        assert!(AuditActor::principal(Principal::new(1, 2)).is_valid());
        assert!(AuditActor::system().is_valid());
        assert!(
            !AuditActor {
                kind: AuditActorKind::System,
                principal: Some(Principal::new(1, 2)),
            }
            .is_valid()
        );
    }

    #[test]
    fn stream_frame_requires_exact_shape() {
        let request_id = RequestId::new();
        let event = TaskEvent {
            task_id: TaskId::new(),
            sequence: 1,
            event_kind: "task.created".into(),
            source_ref: None,
            safe_summary: Some("created".into()),
            safe_data: json!({}),
            occurred_at_ms: 1,
        };
        assert!(
            TaskEventStreamFrame::event(request_id, event)
                .validate()
                .is_ok()
        );
        assert!(
            TaskEventStreamFrame::end(RequestId::new(), TaskState::Succeeded.as_str())
                .validate()
                .is_ok()
        );
        assert!(
            TaskEventStreamFrame::error(
                RequestId::new(),
                SemanticError::new(SemanticErrorCode::StaleResource, "gap"),
            )
            .validate()
            .is_ok()
        );
    }
}
