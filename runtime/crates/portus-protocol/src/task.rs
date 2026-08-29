use crate::{Principal, TaskId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

macro_rules! task_string_enum {
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

task_string_enum!(TaskState {
    Created => "created",
    Queued => "queued",
    Starting => "starting",
    Running => "running",
    Waiting => "waiting",
    Paused => "paused",
    Reconciling => "reconciling",
    Cancelling => "cancelling",
    Succeeded => "succeeded",
    Failed => "failed",
    Cancelled => "cancelled",
    Interrupted => "interrupted",
});

impl TaskState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

task_string_enum!(WaitingReason {
    Approval => "approval",
    UserInput => "user_input",
    Provider => "provider",
    Resource => "resource",
    Dependency => "dependency",
    RateLimit => "rate_limit",
    ExternalCondition => "external_condition",
});

task_string_enum!(ExecutionRelationshipMode {
    Managed => "managed",
    Associated => "associated",
});

task_string_enum!(TaskBackendKind {
    NativeProcess => "native_process",
    CodexRoot => "codex_root",
    CodexSubagent => "codex_subagent",
    Provider => "provider",
    OpenRcService => "openrc_service",
    Application => "application",
    ChildTask => "child_task",
});

task_string_enum!(ExecutionRelationshipStatus {
    Starting => "starting",
    Running => "running",
    Stopped => "stopped",
    Lost => "lost",
    Unknown => "unknown",
});

task_string_enum!(RetrySafety {
    Never => "never",
    Idempotent => "idempotent",
    ContractSafe => "contract_safe",
});

task_string_enum!(TaskResultKind {
    Success => "success",
    Failure => "failure",
    Cancelled => "cancelled",
    Interrupted => "interrupted",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub project_ref: String,
    pub owner: Principal,
    pub workspace_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionReference {
    pub session_ref: String,
    pub owner: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ref: Option<String>,
    pub session_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_observation: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: TaskId,
    pub owner: Principal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub objective_summary: String,
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<WaitingReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of_task_id: Option<TaskId>,
    pub requester_surface: String,
    pub retry_safety: RetrySafety,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
    pub updated_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_kind: Option<TaskResultKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    pub last_event_sequence: u64,
    pub attempt_count: u32,
    pub managed_relationships: u32,
    pub associated_relationships: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskExecutionRelationship {
    pub relation_id: i64,
    pub task_id: TaskId,
    pub mode: ExecutionRelationshipMode,
    pub backend_kind: TaskBackendKind,
    pub backend_ref: String,
    pub generation_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_ref: Option<String>,
    pub status: ExecutionRelationshipStatus,
    pub cancellation_supported: bool,
    pub reconciliation_supported: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: TaskId,
    pub sequence: u64,
    pub event_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_summary: Option<String>,
    #[serde(default)]
    pub safe_data: Value,
    pub occurred_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskView {
    #[serde(flatten)]
    pub task: TaskSummary,
    pub relationships: Vec<TaskExecutionRelationship>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskPage {
    pub items: Vec<TaskSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskEventPage {
    pub events: Vec<TaskEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_from_sequence: Option<u64>,
    pub latest_sequence: u64,
    pub gap_before_page: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_state: Option<TaskState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_enums_use_locked_wire_values() {
        assert_eq!(
            serde_json::to_string(&TaskState::Reconciling).unwrap(),
            r#""reconciling""#
        );
        assert_eq!(
            serde_json::to_string(&WaitingReason::UserInput).unwrap(),
            r#""user_input""#
        );
        assert_eq!(
            serde_json::to_string(&ExecutionRelationshipMode::Managed).unwrap(),
            r#""managed""#
        );
        assert_eq!(
            serde_json::to_string(&RetrySafety::ContractSafe).unwrap(),
            r#""contract_safe""#
        );
    }

    #[test]
    fn only_locked_states_are_terminal() {
        for state in [
            TaskState::Succeeded,
            TaskState::Failed,
            TaskState::Cancelled,
            TaskState::Interrupted,
        ] {
            assert!(state.is_terminal());
        }
        for state in [
            TaskState::Created,
            TaskState::Queued,
            TaskState::Starting,
            TaskState::Running,
            TaskState::Waiting,
            TaskState::Paused,
            TaskState::Reconciling,
            TaskState::Cancelling,
        ] {
            assert!(!state.is_terminal());
        }
    }
}
