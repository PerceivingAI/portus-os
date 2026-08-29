//! PortusOS durable task lifecycle and minimum managed-execution engine.
//!
//! This crate owns task transition semantics and the first narrow managed child
//! process backend. It intentionally exposes no shell-string or generic command
//! RPC surface; ordinary commands remain native/Codex operations.

use portus_protocol::{
    ExecutionRelationshipMode, ExecutionRelationshipStatus, Principal, RetrySafety,
    TaskBackendKind, TaskId, TaskResultKind, TaskState, TaskView,
};
use portus_state::{
    NewExecutionRelationship, NewTaskRecord, PortusState, StateError, TaskTransition,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub enum TaskError {
    State(StateError),
    InvalidTransition {
        from: TaskState,
        to: TaskState,
    },
    PreconditionFailed {
        expected: TaskState,
        found: TaskState,
    },
    NotFound,
    Unsupported(String),
    InvalidSpec(String),
    Io(std::io::Error),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::State(error) => write!(f, "task state error: {error}"),
            Self::InvalidTransition { from, to } => {
                write!(f, "illegal task transition from {from} to {to}")
            }
            Self::PreconditionFailed { expected, found } => {
                write!(
                    f,
                    "task state precondition expected {expected}, found {found}"
                )
            }
            Self::NotFound => f.write_str("task is not visible"),
            Self::Unsupported(message) => write!(f, "task operation is unsupported: {message}"),
            Self::InvalidSpec(message) => {
                write!(f, "invalid managed process specification: {message}")
            }
            Self::Io(error) => write!(f, "managed process error: {error}"),
        }
    }
}

impl std::error::Error for TaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::State(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StateError> for TaskError {
    fn from(value: StateError) -> Self {
        match value {
            StateError::TaskPreconditionFailed { expected, found } => {
                let expected = expected.parse().unwrap_or(TaskState::Interrupted);
                let found = found.parse().unwrap_or(TaskState::Interrupted);
                Self::PreconditionFailed { expected, found }
            }
            other => Self::State(other),
        }
    }
}

impl From<std::io::Error> for TaskError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type TaskResult<T> = Result<T, TaskError>;

pub trait TaskEventSink: Send + Sync {
    fn task_event_committed(&self, event: &portus_protocol::TaskEvent);
}

#[derive(Default)]
struct NullTaskEventSink;

impl TaskEventSink for NullTaskEventSink {
    fn task_event_committed(&self, _event: &portus_protocol::TaskEvent) {}
}

const MAX_MANAGED_ARGUMENTS: usize = 128;
const MAX_MANAGED_ARGUMENT_BYTES: usize = 4096;
const MAX_MANAGED_TOTAL_ARGUMENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct ManagedProcessSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub safe_backend_ref: String,
    pub title: Option<String>,
    pub objective_summary: String,
    pub requester_surface: String,
    pub project_ref: Option<String>,
    pub session_ref: Option<String>,
    pub retry_safety: RetrySafety,
}

#[derive(Clone, Debug)]
pub struct AssociatedExecutionSpec {
    pub backend_kind: TaskBackendKind,
    pub backend_ref: String,
    pub generation_ref: String,
    pub correlation_ref: Option<String>,
    pub title: Option<String>,
    pub objective_summary: String,
    pub requester_surface: String,
    pub project_ref: Option<String>,
    pub session_ref: Option<String>,
    pub retry_safety: RetrySafety,
}

impl ManagedProcessSpec {
    #[must_use]
    pub fn new(
        program: impl Into<PathBuf>,
        safe_backend_ref: impl Into<String>,
        objective_summary: impl Into<String>,
    ) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            safe_backend_ref: safe_backend_ref.into(),
            title: None,
            objective_summary: objective_summary.into(),
            requester_surface: "internal".into(),
            project_ref: None,
            session_ref: None,
            retry_safety: RetrySafety::Never,
        }
    }
}

struct ManagedChild {
    child: Child,
    owner: Principal,
    relation_id: i64,
    attempt_id: i64,
}

pub struct TaskEngine {
    children: Mutex<HashMap<TaskId, ManagedChild>>,
    event_sink: std::sync::Arc<dyn TaskEventSink>,
}

impl Default for TaskEngine {
    fn default() -> Self {
        Self {
            children: Mutex::new(HashMap::new()),
            event_sink: std::sync::Arc::new(NullTaskEventSink),
        }
    }
}

impl TaskEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_event_sink(event_sink: std::sync::Arc<dyn TaskEventSink>) -> Self {
        Self {
            children: Mutex::new(HashMap::new()),
            event_sink,
        }
    }

    pub fn register_associated_execution(
        &self,
        state: &mut PortusState,
        owner: Principal,
        spec: AssociatedExecutionSpec,
    ) -> TaskResult<TaskView> {
        if spec.backend_ref.trim().is_empty() || spec.generation_ref.trim().is_empty() {
            return Err(TaskError::InvalidSpec(
                "associated backend and generation references must be non-empty".into(),
            ));
        }
        let now = unix_time_ms();
        let task_id = TaskId::new();
        state.create_task(&NewTaskRecord {
            task_id,
            owner,
            title: spec.title,
            objective_summary: spec.objective_summary,
            requester_surface: spec.requester_surface,
            project_ref: spec.project_ref,
            session_ref: spec.session_ref,
            parent_task_id: None,
            retry_of_task_id: None,
            retry_safety: spec.retry_safety,
            created_at_ms: now,
        })?;
        self.notify_event(state, &task_id, owner, 1)?;
        self.transition(
            state,
            &task_id,
            owner,
            TaskState::Created,
            TaskState::Starting,
            Some("associating_existing_backend"),
            None,
            None,
            "task.starting",
            Some("existing backend association is being recorded"),
            json!({"backend_kind":spec.backend_kind.as_str()}),
        )?;
        state.add_task_relationship(&NewExecutionRelationship {
            task_id,
            mode: ExecutionRelationshipMode::Associated,
            backend_kind: spec.backend_kind,
            backend_ref: spec.backend_ref,
            generation_ref: spec.generation_ref,
            process_id: None,
            correlation_ref: spec.correlation_ref,
            status: ExecutionRelationshipStatus::Running,
            cancellation_supported: false,
            reconciliation_supported: false,
            created_at_ms: now,
        })?;
        self.transition(
            state,
            &task_id,
            owner,
            TaskState::Starting,
            TaskState::Running,
            Some("associated_backend_observed"),
            None,
            None,
            "task.running",
            Some("associated backend is recorded as running"),
            json!({"ownership":"associated"}),
        )
    }

    pub fn launch_managed_process(
        &self,
        state: &mut PortusState,
        owner: Principal,
        spec: ManagedProcessSpec,
    ) -> TaskResult<TaskView> {
        if spec.program.as_os_str().is_empty() {
            return Err(TaskError::InvalidSpec("program is empty".into()));
        }
        if spec.safe_backend_ref.trim().is_empty() {
            return Err(TaskError::InvalidSpec(
                "safe backend reference is empty".into(),
            ));
        }
        if spec.args.len() > MAX_MANAGED_ARGUMENTS {
            return Err(TaskError::InvalidSpec(
                "too many managed process arguments".into(),
            ));
        }
        let mut total_argument_bytes = 0_usize;
        for argument in &spec.args {
            let bytes = argument.to_string_lossy().len();
            if bytes > MAX_MANAGED_ARGUMENT_BYTES {
                return Err(TaskError::InvalidSpec(
                    "managed process argument exceeds bounded size".into(),
                ));
            }
            total_argument_bytes = total_argument_bytes.saturating_add(bytes);
        }
        if total_argument_bytes > MAX_MANAGED_TOTAL_ARGUMENT_BYTES {
            return Err(TaskError::InvalidSpec(
                "managed process argument vector exceeds bounded size".into(),
            ));
        }
        let now = unix_time_ms();
        let task_id = TaskId::new();
        state.create_task(&NewTaskRecord {
            task_id,
            owner,
            title: spec.title,
            objective_summary: spec.objective_summary,
            requester_surface: spec.requester_surface,
            project_ref: spec.project_ref,
            session_ref: spec.session_ref,
            parent_task_id: None,
            retry_of_task_id: None,
            retry_safety: spec.retry_safety,
            created_at_ms: now,
        })?;
        self.notify_event(state, &task_id, owner, 1)?;
        self.transition(
            state,
            &task_id,
            owner,
            TaskState::Created,
            TaskState::Starting,
            Some("launching_managed_backend"),
            None,
            None,
            "task.starting",
            Some("managed execution starting"),
            json!({"backend_kind":"native_process"}),
        )?;
        let (attempt_id, attempt_number) = state.start_task_attempt(
            &task_id,
            TaskBackendKind::NativeProcess,
            &spec.safe_backend_ref,
            spec.retry_safety != RetrySafety::Never,
            now,
        )?;

        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                state.finish_task_attempt(
                    attempt_id,
                    "spawn_failed",
                    Some("launch_error"),
                    None,
                    unix_time_ms(),
                )?;
                self.transition(
                    state,
                    &task_id,
                    owner,
                    TaskState::Starting,
                    TaskState::Failed,
                    Some("managed_backend_launch_failed"),
                    Some(TaskResultKind::Failure),
                    Some("managed backend could not be launched"),
                    "task.failed",
                    Some("managed backend launch failed"),
                    Value::Object(Default::default()),
                )?;
                return Err(TaskError::Io(error));
            }
        };
        let pid = child.id();
        let launched_at = unix_time_ms();
        let generation_ref = format!("managed:{task_id}:{attempt_number}:{pid}:{launched_at}");
        let relation_id = match state.add_task_relationship(&NewExecutionRelationship {
            task_id,
            mode: ExecutionRelationshipMode::Managed,
            backend_kind: TaskBackendKind::NativeProcess,
            backend_ref: spec.safe_backend_ref,
            generation_ref,
            process_id: Some(pid),
            correlation_ref: Some(format!("{task_id}:attempt:{attempt_number}")),
            status: ExecutionRelationshipStatus::Running,
            cancellation_supported: true,
            reconciliation_supported: false,
            created_at_ms: launched_at,
        }) {
            Ok(relation_id) => relation_id,
            Err(error) => {
                let _ = child.kill();
                let status = child.wait().ok();
                state.finish_task_attempt(
                    attempt_id,
                    "relationship_persist_failed",
                    Some("state_error"),
                    status.and_then(|status| status.code()),
                    unix_time_ms(),
                )?;
                let _ = self.transition(
                    state,
                    &task_id,
                    owner,
                    TaskState::Starting,
                    TaskState::Failed,
                    Some("managed_relationship_persist_failed"),
                    Some(TaskResultKind::Failure),
                    Some("managed backend was stopped because its durable relationship could not be recorded"),
                    "task.failed",
                    Some("managed backend relationship persistence failed"),
                    Value::Object(Default::default()),
                );
                return Err(TaskError::State(error));
            }
        };
        let running = match self.transition(
            state,
            &task_id,
            owner,
            TaskState::Starting,
            TaskState::Running,
            Some("managed_backend_running"),
            None,
            None,
            "task.running",
            Some("managed backend is running"),
            json!({"backend_kind":"native_process"}),
        ) {
            Ok(view) => view,
            Err(error) => {
                let _ = child.kill();
                let status = child.wait().ok();
                let now = unix_time_ms();
                let _ = state.update_task_relationship_status(
                    relation_id,
                    ExecutionRelationshipStatus::Stopped,
                    now,
                );
                let _ = state.finish_task_attempt(
                    attempt_id,
                    "task_running_transition_failed",
                    Some("state_error"),
                    status.and_then(|status| status.code()),
                    now,
                );
                return Err(error);
            }
        };
        self.children
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                task_id,
                ManagedChild {
                    child,
                    owner,
                    relation_id,
                    attempt_id,
                },
            );
        Ok(running)
    }

    pub fn refresh_all(&self, state: &mut PortusState) -> TaskResult<()> {
        let mut completed = Vec::new();
        {
            let mut children = self
                .children
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for (task_id, managed) in children.iter_mut() {
                if let Some(status) = managed.child.try_wait()? {
                    completed.push((
                        *task_id,
                        managed.owner,
                        managed.relation_id,
                        managed.attempt_id,
                        status.code(),
                    ));
                }
            }
            for (task_id, ..) in &completed {
                children.remove(task_id);
            }
        }
        for (task_id, owner, relation_id, attempt_id, exit_code) in completed {
            let Some(view) = state.task_view_visible(&task_id, owner)? else {
                continue;
            };
            state.update_task_relationship_status(
                relation_id,
                ExecutionRelationshipStatus::Stopped,
                unix_time_ms(),
            )?;
            if view.task.state.is_terminal() {
                continue;
            }
            let success = exit_code == Some(0);
            state.finish_task_attempt(
                attempt_id,
                if success { "succeeded" } else { "failed" },
                (!success).then_some("process_exit"),
                exit_code,
                unix_time_ms(),
            )?;
            let (next, result_kind, reason, result_summary, event_kind) =
                if view.task.state == TaskState::Cancelling {
                    (
                        TaskState::Cancelled,
                        TaskResultKind::Cancelled,
                        "cancellation_confirmed",
                        "managed backend stopped after cancellation",
                        "task.cancelled",
                    )
                } else if success {
                    (
                        TaskState::Succeeded,
                        TaskResultKind::Success,
                        "managed_backend_succeeded",
                        "managed backend completed successfully",
                        "task.succeeded",
                    )
                } else {
                    (
                        TaskState::Failed,
                        TaskResultKind::Failure,
                        "managed_backend_failed",
                        "managed backend exited unsuccessfully",
                        "task.failed",
                    )
                };
            self.transition(
                state,
                &task_id,
                owner,
                view.task.state,
                next,
                Some(reason),
                Some(result_kind),
                Some(result_summary),
                event_kind,
                Some(result_summary),
                json!({"exit_code":exit_code}),
            )?;
        }
        Ok(())
    }
    pub fn cancel_task(
        &self,
        state: &mut PortusState,
        owner: Principal,
        task_id: &TaskId,
        expected_state: Option<TaskState>,
    ) -> TaskResult<TaskView> {
        self.refresh_all(state)?;
        let view = state
            .task_view_visible(task_id, owner)?
            .ok_or(TaskError::NotFound)?;
        if let Some(expected) = expected_state
            && view.task.state != expected
        {
            return Err(TaskError::PreconditionFailed {
                expected,
                found: view.task.state,
            });
        }
        if view.task.state.is_terminal() {
            return Err(TaskError::Unsupported(
                "terminal tasks cannot be cancelled".into(),
            ));
        }
        let has_cancellable_managed = view.relationships.iter().any(|relationship| {
            relationship.mode == ExecutionRelationshipMode::Managed
                && relationship.cancellation_supported
                && relationship.status == ExecutionRelationshipStatus::Running
        });
        if !has_cancellable_managed {
            return Err(TaskError::Unsupported(
                "task has no live managed backend with confirmed cancellation support".into(),
            ));
        }
        let prior = view.task.state;
        self.transition(
            state,
            task_id,
            owner,
            prior,
            TaskState::Cancelling,
            Some("cancellation_requested"),
            None,
            None,
            "task.cancellation_requested",
            Some("cancellation requested"),
            Value::Object(Default::default()),
        )?;

        let mut managed = {
            self.children
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(task_id)
        }
        .ok_or_else(|| {
            TaskError::Unsupported("managed backend is no longer attached to this runtime".into())
        })?;

        let kill_result = managed.child.kill();
        let wait_result = managed.child.wait();
        match (kill_result, wait_result) {
            (Ok(()), Ok(status)) => {
                let now = unix_time_ms();
                state.update_task_relationship_status(
                    managed.relation_id,
                    ExecutionRelationshipStatus::Stopped,
                    now,
                )?;
                state.finish_task_attempt(
                    managed.attempt_id,
                    "cancelled",
                    None,
                    status.code(),
                    now,
                )?;
                self.transition(
                    state,
                    task_id,
                    owner,
                    TaskState::Cancelling,
                    TaskState::Cancelled,
                    Some("cancellation_confirmed"),
                    Some(TaskResultKind::Cancelled),
                    Some("managed backend cancellation was confirmed"),
                    "task.cancelled",
                    Some("managed backend cancellation confirmed"),
                    Value::Object(Default::default()),
                )
            }
            (Err(_), Ok(status)) => {
                let now = unix_time_ms();
                state.update_task_relationship_status(
                    managed.relation_id,
                    ExecutionRelationshipStatus::Stopped,
                    now,
                )?;
                let success = status.success();
                state.finish_task_attempt(
                    managed.attempt_id,
                    if success { "succeeded" } else { "failed" },
                    (!success).then_some("process_exit"),
                    status.code(),
                    now,
                )?;
                self.transition(
                    state,
                    task_id,
                    owner,
                    TaskState::Cancelling,
                    TaskState::Reconciling,
                    Some("cancellation_not_applied_backend_already_exited"),
                    None,
                    None,
                    "task.reconciling",
                    Some("backend exited before cancellation could be confirmed"),
                    Value::Object(Default::default()),
                )?;
                self.transition(
                    state,
                    task_id,
                    owner,
                    TaskState::Reconciling,
                    if success {
                        TaskState::Succeeded
                    } else {
                        TaskState::Failed
                    },
                    Some(if success {
                        "managed_backend_succeeded"
                    } else {
                        "managed_backend_failed"
                    }),
                    Some(if success {
                        TaskResultKind::Success
                    } else {
                        TaskResultKind::Failure
                    }),
                    Some(if success {
                        "managed backend completed before cancellation could be applied"
                    } else {
                        "managed backend failed before cancellation could be applied"
                    }),
                    if success {
                        "task.succeeded"
                    } else {
                        "task.failed"
                    },
                    Some("backend completion won the cancellation race"),
                    json!({"exit_code":status.code()}),
                )
            }
            (_, Err(error)) => {
                self.transition(
                    state,
                    task_id,
                    owner,
                    TaskState::Cancelling,
                    TaskState::Reconciling,
                    Some("cancellation_state_unknown"),
                    None,
                    None,
                    "task.reconciling",
                    Some("backend state became unknown during cancellation"),
                    Value::Object(Default::default()),
                )?;
                self.transition(
                    state,
                    task_id,
                    owner,
                    TaskState::Reconciling,
                    TaskState::Interrupted,
                    Some("backend_state_unverifiable"),
                    Some(TaskResultKind::Interrupted),
                    Some("backend state could not be established after cancellation"),
                    "task.interrupted",
                    Some("task interrupted because backend state is unverifiable"),
                    Value::Object(Default::default()),
                )?;
                Err(TaskError::Io(error))
            }
        }
    }

    pub fn reconcile_after_runtime_restart(&self, state: &mut PortusState) -> TaskResult<()> {
        for (task_id, owner, current) in state.nonterminal_task_ids()? {
            if current == TaskState::Created || current == TaskState::Queued {
                continue;
            }
            let reconciling = if current == TaskState::Reconciling {
                current
            } else {
                self.transition(
                    state,
                    &task_id,
                    owner,
                    current,
                    TaskState::Reconciling,
                    Some("runtime_restart"),
                    None,
                    None,
                    "task.reconciling",
                    Some("runtime restarted; backend state requires verification"),
                    Value::Object(Default::default()),
                )?
                .task
                .state
            };
            if reconciling == TaskState::Reconciling {
                self.transition(
                    state,
                    &task_id,
                    owner,
                    TaskState::Reconciling,
                    TaskState::Interrupted,
                    Some("backend_state_unverifiable"),
                    Some(TaskResultKind::Interrupted),
                    Some("current host-safe backend cannot prove prior live process generation after runtime restart"),
                    "task.interrupted",
                    Some("task interrupted rather than guessing prior backend state"),
                    Value::Object(Default::default()),
                )?;
            }
        }
        Ok(())
    }

    fn notify_event(
        &self,
        state: &PortusState,
        task_id: &TaskId,
        owner: Principal,
        sequence: u64,
    ) -> TaskResult<()> {
        let page = state
            .task_events_visible(task_id, owner, Some(sequence.saturating_sub(1)), 1)?
            .ok_or(TaskError::NotFound)?;
        if let Some(event) = page
            .events
            .into_iter()
            .find(|event| event.sequence == sequence)
        {
            self.event_sink.task_event_committed(&event);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &self,
        state: &mut PortusState,
        task_id: &TaskId,
        owner: Principal,
        from: TaskState,
        to: TaskState,
        reason: Option<&str>,
        result_kind: Option<TaskResultKind>,
        result_summary: Option<&str>,
        event_kind: &str,
        event_summary: Option<&str>,
        event_data: Value,
    ) -> TaskResult<TaskView> {
        if !can_transition(from, to) {
            return Err(TaskError::InvalidTransition { from, to });
        }
        let view = state
            .transition_task(
                task_id,
                owner,
                &TaskTransition {
                    expected: from,
                    next: to,
                    state_reason: reason,
                    waiting_reason: None,
                    result_kind,
                    result_summary,
                    event_kind,
                    event_summary,
                    source_ref: Some("portus-task"),
                    event_data,
                    occurred_at_ms: unix_time_ms(),
                },
            )
            .map_err(TaskError::from)?;
        self.notify_event(state, task_id, owner, view.task.last_event_sequence)?;
        Ok(view)
    }
}

#[must_use]
pub const fn can_transition(from: TaskState, to: TaskState) -> bool {
    use TaskState::{
        Cancelled, Cancelling, Created, Failed, Interrupted, Paused, Queued, Reconciling, Running,
        Starting, Succeeded, Waiting,
    };
    match from {
        Created => matches!(to, Queued | Starting | Cancelling),
        Queued => matches!(to, Starting | Cancelling | Failed | Reconciling),
        Starting => matches!(to, Running | Waiting | Cancelling | Failed | Reconciling),
        Running => matches!(
            to,
            Waiting | Paused | Cancelling | Succeeded | Failed | Reconciling
        ),
        Waiting => matches!(to, Running | Cancelling | Failed | Reconciling),
        Paused => matches!(to, Running | Queued | Cancelling | Reconciling),
        Reconciling => matches!(
            to,
            Running | Waiting | Paused | Cancelling | Succeeded | Failed | Cancelled | Interrupted
        ),
        Cancelling => matches!(to, Cancelled | Running | Reconciling | Failed),
        Succeeded | Failed | Cancelled | Interrupted => false,
    }
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf, thread, time::Duration};

    struct TestState {
        dir: PathBuf,
        state: PortusState,
    }

    impl TestState {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("portus-task-engine-{name}-{}", TaskId::new()));
            fs::create_dir_all(&dir).unwrap();
            let state = PortusState::open(dir.join("portus.db")).unwrap();
            Self { dir, state }
        }
    }

    impl Drop for TestState {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn legal_and_illegal_transitions_match_locked_state_machine() {
        assert!(can_transition(TaskState::Created, TaskState::Starting));
        assert!(can_transition(TaskState::Running, TaskState::Cancelling));
        assert!(can_transition(
            TaskState::Reconciling,
            TaskState::Interrupted
        ));
        assert!(!can_transition(TaskState::Succeeded, TaskState::Running));
        assert!(!can_transition(TaskState::Created, TaskState::Succeeded));
    }

    #[test]
    fn associated_execution_is_recorded_without_claiming_management() {
        let mut test = TestState::new("associated");
        let owner = Principal::new(1000, 1000);
        let task = TaskEngine::new()
            .register_associated_execution(
                &mut test.state,
                owner,
                AssociatedExecutionSpec {
                    backend_kind: TaskBackendKind::CodexRoot,
                    backend_ref: "codex:thread-42".into(),
                    generation_ref: "codex-session-generation-1".into(),
                    correlation_ref: Some("project:demo".into()),
                    title: Some("Associated Codex work".into()),
                    objective_summary: "track an already-running Codex root session".into(),
                    requester_surface: "test".into(),
                    project_ref: None,
                    session_ref: None,
                    retry_safety: RetrySafety::Never,
                },
            )
            .unwrap();
        assert_eq!(task.task.state, TaskState::Running);
        assert_eq!(task.task.managed_relationships, 0);
        assert_eq!(task.task.associated_relationships, 1);
        assert_eq!(
            task.relationships[0].mode,
            ExecutionRelationshipMode::Associated
        );
        assert!(!task.relationships[0].cancellation_supported);
    }

    #[test]
    fn managed_process_can_be_cancelled_only_after_confirmed_stop() {
        let mut test = TestState::new("cancel");
        let engine = TaskEngine::new();
        let owner = Principal::new(1000, 1000);
        let task = engine
            .launch_managed_process(&mut test.state, owner, long_running_spec())
            .unwrap();
        assert_eq!(task.task.state, TaskState::Running);
        let cancelled = engine
            .cancel_task(
                &mut test.state,
                owner,
                &task.task.task_id,
                Some(TaskState::Running),
            )
            .unwrap();
        assert_eq!(cancelled.task.state, TaskState::Cancelled);
        assert_eq!(cancelled.task.result_kind, Some(TaskResultKind::Cancelled));
        assert!(
            cancelled
                .relationships
                .iter()
                .all(|relation| { relation.status == ExecutionRelationshipStatus::Stopped })
        );
    }

    #[test]
    fn managed_process_completion_is_bounded_and_does_not_capture_output() {
        let mut test = TestState::new("complete");
        let engine = TaskEngine::new();
        let owner = Principal::new(1000, 1000);
        let task = engine
            .launch_managed_process(&mut test.state, owner, quick_success_spec())
            .unwrap();
        for _ in 0..100 {
            engine.refresh_all(&mut test.state).unwrap();
            let view = test
                .state
                .task_view_visible(&task.task.task_id, owner)
                .unwrap()
                .unwrap();
            if view.task.state.is_terminal() {
                assert_eq!(view.task.state, TaskState::Succeeded);
                assert!(view.task.result_summary.as_deref().unwrap().len() < 256);
                let events = test
                    .state
                    .task_events_visible(&task.task.task_id, owner, None, 200)
                    .unwrap()
                    .unwrap();
                let encoded = serde_json::to_string(&events).unwrap();
                assert!(!encoded.contains("captured-child-output-marker"));
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("managed child did not reach terminal state");
    }

    #[test]
    fn unsafe_launch_failure_is_not_automatically_retried() {
        let mut test = TestState::new("no-retry");
        let engine = TaskEngine::new();
        let owner = Principal::new(1000, 1000);
        let spec = ManagedProcessSpec::new(
            test.dir.join("definitely-missing-program"),
            "fixture.missing",
            "prove unsafe work is not retried",
        );
        let error = engine
            .launch_managed_process(&mut test.state, owner, spec)
            .unwrap_err();
        assert!(matches!(error, TaskError::Io(_)));
        let page = test
            .state
            .list_tasks_visible(owner, &portus_state::TaskListFilter::default(), 50, None)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].state, TaskState::Failed);
        assert_eq!(page.items[0].attempt_count, 1);
        assert_eq!(page.items[0].retry_safety, RetrySafety::Never);
    }

    #[test]
    fn runtime_restart_never_re_adopts_pid_without_generation_proof() {
        let mut test = TestState::new("restart");
        let owner = Principal::new(1000, 1000);
        let task_id = TaskId::new();
        test.state
            .create_task(&NewTaskRecord {
                task_id,
                owner,
                title: Some("restart fixture".into()),
                objective_summary: "prove conservative restart reconciliation".into(),
                requester_surface: "test".into(),
                project_ref: None,
                session_ref: None,
                parent_task_id: None,
                retry_of_task_id: None,
                retry_safety: RetrySafety::Never,
                created_at_ms: 1,
            })
            .unwrap();
        test.state
            .transition_task(
                &task_id,
                owner,
                &TaskTransition {
                    expected: TaskState::Created,
                    next: TaskState::Starting,
                    state_reason: Some("fixture"),
                    waiting_reason: None,
                    result_kind: None,
                    result_summary: None,
                    event_kind: "task.starting",
                    event_summary: Some("fixture starting"),
                    source_ref: Some("test"),
                    event_data: Value::Object(Default::default()),
                    occurred_at_ms: 2,
                },
            )
            .unwrap();
        test.state
            .add_task_relationship(&NewExecutionRelationship {
                task_id,
                mode: ExecutionRelationshipMode::Managed,
                backend_kind: TaskBackendKind::NativeProcess,
                backend_ref: "fixture.prior-process".into(),
                generation_ref: "boot-fixture:pid-4242:start-99".into(),
                process_id: Some(4242),
                correlation_ref: Some(format!("{task_id}:attempt:1")),
                status: ExecutionRelationshipStatus::Running,
                cancellation_supported: true,
                reconciliation_supported: false,
                created_at_ms: 2,
            })
            .unwrap();
        test.state
            .transition_task(
                &task_id,
                owner,
                &TaskTransition {
                    expected: TaskState::Starting,
                    next: TaskState::Running,
                    state_reason: Some("fixture"),
                    waiting_reason: None,
                    result_kind: None,
                    result_summary: None,
                    event_kind: "task.running",
                    event_summary: Some("fixture running"),
                    source_ref: Some("test"),
                    event_data: Value::Object(Default::default()),
                    occurred_at_ms: 3,
                },
            )
            .unwrap();

        TaskEngine::new()
            .reconcile_after_runtime_restart(&mut test.state)
            .unwrap();
        let view = test
            .state
            .task_view_visible(&task_id, owner)
            .unwrap()
            .unwrap();
        assert_eq!(view.task.state, TaskState::Interrupted);
        assert_eq!(
            view.task.state_reason.as_deref(),
            Some("backend_state_unverifiable")
        );
    }

    fn quick_success_spec() -> ManagedProcessSpec {
        #[cfg(windows)]
        {
            let mut spec = ManagedProcessSpec::new(
                "cmd.exe",
                "fixture.quick",
                "complete a bounded fixture process",
            );
            spec.args = vec!["/C".into(), "echo captured-child-output-marker".into()];
            spec
        }
        #[cfg(not(windows))]
        {
            let mut spec = ManagedProcessSpec::new(
                "sh",
                "fixture.quick",
                "complete a bounded fixture process",
            );
            spec.args = vec!["-c".into(), "printf captured-child-output-marker".into()];
            spec
        }
    }

    fn long_running_spec() -> ManagedProcessSpec {
        #[cfg(windows)]
        {
            let mut spec = ManagedProcessSpec::new(
                "ping.exe",
                "fixture.wait",
                "wait until explicitly cancelled",
            );
            spec.args = vec!["-n".into(), "30".into(), "127.0.0.1".into()];
            spec
        }
        #[cfg(not(windows))]
        {
            let mut spec =
                ManagedProcessSpec::new("sleep", "fixture.wait", "wait until explicitly cancelled");
            spec.args = vec!["30".into()];
            spec
        }
    }
}
