use crate::{
    NewSignificantEvent, PortusState, StateError, StateResult,
    event::{insert_significant_event_tx, prune_object_tx},
};
use portus_protocol::{
    EventObjectKind, ExecutionRelationshipMode, ExecutionRelationshipStatus, Principal,
    ProjectRecord, RetrySafety, SessionReference, TaskBackendKind, TaskEvent, TaskEventPage,
    TaskExecutionRelationship, TaskId, TaskPage, TaskResultKind, TaskState, TaskSummary, TaskView,
    WaitingReason,
};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::str::FromStr;

pub const MAX_TASK_OBJECTIVE_BYTES: usize = 2048;
pub const MAX_TASK_TITLE_BYTES: usize = 256;
pub const MAX_TASK_REASON_BYTES: usize = 512;
pub const MAX_TASK_RESULT_BYTES: usize = 2048;
pub const MAX_TASK_EVENT_SUMMARY_BYTES: usize = 1024;
pub const MAX_TASK_EVENT_DATA_BYTES: usize = 4096;
pub const MAX_TASK_REF_BYTES: usize = 512;

#[derive(Clone, Debug)]
pub struct NewTaskRecord {
    pub task_id: TaskId,
    pub owner: Principal,
    pub title: Option<String>,
    pub objective_summary: String,
    pub requester_surface: String,
    pub project_ref: Option<String>,
    pub session_ref: Option<String>,
    pub parent_task_id: Option<TaskId>,
    pub retry_of_task_id: Option<TaskId>,
    pub retry_safety: RetrySafety,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct NewExecutionRelationship {
    pub task_id: TaskId,
    pub mode: ExecutionRelationshipMode,
    pub backend_kind: TaskBackendKind,
    pub backend_ref: String,
    pub generation_ref: String,
    pub process_id: Option<u32>,
    pub correlation_ref: Option<String>,
    pub status: ExecutionRelationshipStatus,
    pub cancellation_supported: bool,
    pub reconciliation_supported: bool,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Default)]
pub struct TaskListFilter {
    pub state: Option<TaskState>,
    pub project_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskTransition<'a> {
    pub expected: TaskState,
    pub next: TaskState,
    pub state_reason: Option<&'a str>,
    pub waiting_reason: Option<WaitingReason>,
    pub result_kind: Option<TaskResultKind>,
    pub result_summary: Option<&'a str>,
    pub event_kind: &'a str,
    pub event_summary: Option<&'a str>,
    pub source_ref: Option<&'a str>,
    pub event_data: Value,
    pub occurred_at_ms: i64,
}

impl PortusState {
    pub fn register_project(
        &self,
        principal: Principal,
        project_ref: &str,
        workspace_path: &str,
        display_name: Option<&str>,
        now_ms: i64,
    ) -> StateResult<ProjectRecord> {
        validate_nonempty(project_ref, MAX_TASK_REF_BYTES, "project reference")?;
        validate_nonempty(workspace_path, MAX_TASK_REF_BYTES, "workspace path")?;
        validate_optional(display_name, MAX_TASK_TITLE_BYTES, "project display name")?;
        self.connection.execute(
            "INSERT INTO projects(project_ref, owner_uid, owner_gid, workspace_path, created_at_ms, display_name, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5) ON CONFLICT(project_ref) DO UPDATE SET workspace_path=excluded.workspace_path, display_name=excluded.display_name, updated_at_ms=excluded.updated_at_ms WHERE projects.owner_uid=excluded.owner_uid AND projects.owner_gid=excluded.owner_gid",
            params![project_ref, principal.uid(), principal.gid(), workspace_path, now_ms, display_name],
        )?;
        self.project_visible(project_ref, principal)?
            .ok_or_else(|| {
                StateError::InvalidTaskState(
                    "project reference belongs to another principal".into(),
                )
            })
    }

    pub fn project_visible(
        &self,
        project_ref: &str,
        principal: Principal,
    ) -> StateResult<Option<ProjectRecord>> {
        self.connection
            .query_row(
                "SELECT project_ref, owner_uid, owner_gid, workspace_path, display_name, created_at_ms, updated_at_ms FROM projects WHERE project_ref=?1 AND owner_uid=?2 AND owner_gid=?3",
                params![project_ref, principal.uid(), principal.gid()],
                |row| {
                    Ok(ProjectRecord {
                        project_ref: row.get(0)?,
                        owner: Principal::new(row.get(1)?, row.get(2)?),
                        workspace_path: row.get(3)?,
                        display_name: row.get(4)?,
                        created_at_ms: row.get(5)?,
                        updated_at_ms: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StateError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_session_reference(
        &self,
        principal: Principal,
        session_ref: &str,
        session_kind: &str,
        project_ref: Option<&str>,
        session_name: Option<&str>,
        working_directory: Option<&str>,
        role: Option<&str>,
        model_name: Option<&str>,
        status_observation: Option<&str>,
        now_ms: i64,
    ) -> StateResult<SessionReference> {
        validate_nonempty(session_ref, MAX_TASK_REF_BYTES, "session reference")?;
        validate_nonempty(session_kind, 64, "session kind")?;
        for (value, max, field) in [
            (session_name, MAX_TASK_TITLE_BYTES, "session name"),
            (working_directory, MAX_TASK_REF_BYTES, "working directory"),
            (role, 128, "session role"),
            (model_name, 128, "model name"),
            (status_observation, 128, "status observation"),
        ] {
            validate_optional(value, max, field)?;
        }
        if let Some(project_ref) = project_ref
            && self.project_visible(project_ref, principal)?.is_none()
        {
            return Err(StateError::InvalidTaskState(
                "session project is not visible to owner".into(),
            ));
        }
        self.connection.execute(
            "INSERT INTO session_refs(session_ref, owner_uid, owner_gid, project_ref, session_kind, created_at_ms, session_name, working_directory, role, model_name, status_observation, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?6) ON CONFLICT(session_ref) DO UPDATE SET project_ref=excluded.project_ref, session_kind=excluded.session_kind, session_name=excluded.session_name, working_directory=excluded.working_directory, role=excluded.role, model_name=excluded.model_name, status_observation=excluded.status_observation, updated_at_ms=excluded.updated_at_ms WHERE session_refs.owner_uid=excluded.owner_uid AND session_refs.owner_gid=excluded.owner_gid",
            params![session_ref, principal.uid(), principal.gid(), project_ref, session_kind, now_ms, session_name, working_directory, role, model_name, status_observation],
        )?;
        self.session_visible(session_ref, principal)?
            .ok_or_else(|| {
                StateError::InvalidTaskState(
                    "session reference belongs to another principal".into(),
                )
            })
    }

    pub fn session_visible(
        &self,
        session_ref: &str,
        principal: Principal,
    ) -> StateResult<Option<SessionReference>> {
        self.connection
            .query_row(
                "SELECT session_ref, owner_uid, owner_gid, project_ref, session_kind, session_name, working_directory, role, model_name, status_observation, created_at_ms, updated_at_ms FROM session_refs WHERE session_ref=?1 AND owner_uid=?2 AND owner_gid=?3",
                params![session_ref, principal.uid(), principal.gid()],
                |row| {
                    Ok(SessionReference {
                        session_ref: row.get(0)?,
                        owner: Principal::new(row.get(1)?, row.get(2)?),
                        project_ref: row.get(3)?,
                        session_kind: row.get(4)?,
                        session_name: row.get(5)?,
                        working_directory: row.get(6)?,
                        role: row.get(7)?,
                        model_name: row.get(8)?,
                        status_observation: row.get(9)?,
                        created_at_ms: row.get(10)?,
                        updated_at_ms: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(StateError::from)
    }

    pub fn create_task(&mut self, spec: &NewTaskRecord) -> StateResult<TaskView> {
        validate_nonempty(
            &spec.objective_summary,
            MAX_TASK_OBJECTIVE_BYTES,
            "task objective summary",
        )?;
        validate_optional(spec.title.as_deref(), MAX_TASK_TITLE_BYTES, "task title")?;
        validate_nonempty(&spec.requester_surface, 64, "requester surface")?;
        if let Some(project_ref) = spec.project_ref.as_deref()
            && self.project_visible(project_ref, spec.owner)?.is_none()
        {
            return Err(StateError::InvalidTaskState(
                "task project is not visible to owner".into(),
            ));
        }
        if let Some(session_ref) = spec.session_ref.as_deref()
            && self.session_visible(session_ref, spec.owner)?.is_none()
        {
            return Err(StateError::InvalidTaskState(
                "task session is not visible to owner".into(),
            ));
        }
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO tasks(task_id, owner_uid, owner_gid, objective_summary, state, project_ref, session_ref, parent_task_id, retry_of_task_id, created_at_ms, title, requester_surface, retry_safety, last_event_sequence, updated_at_ms) VALUES (?1, ?2, ?3, ?4, 'created', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?9)",
            params![
                spec.task_id.to_string(),
                spec.owner.uid(),
                spec.owner.gid(),
                spec.objective_summary,
                spec.project_ref,
                spec.session_ref,
                spec.parent_task_id.map(|id| id.to_string()),
                spec.retry_of_task_id.map(|id| id.to_string()),
                spec.created_at_ms,
                spec.title,
                spec.requester_surface,
                spec.retry_safety.as_str(),
            ],
        )?;
        insert_significant_event_tx(
            &tx,
            &NewSignificantEvent {
                object_kind: EventObjectKind::Task,
                object_ref: spec.task_id.to_string(),
                principal: Some(spec.owner),
                event_kind: "task.created".into(),
                reason_code: Some("identity_created".into()),
                source_ref: Some("portus-state".into()),
                safe_summary: Some("task identity created".into()),
                safe_data: Value::Object(Default::default()),
                occurred_at_ms: spec.created_at_ms,
            },
            1,
        )?;
        prune_object_tx(&tx, EventObjectKind::Task, &spec.task_id.to_string(), 1)?;
        tx.commit()?;
        self.task_view_visible(&spec.task_id, spec.owner)?
            .ok_or_else(|| StateError::InvalidTaskState("created task is not readable".into()))
    }

    pub fn list_tasks_visible(
        &self,
        principal: Principal,
        filter: &TaskListFilter,
        limit: u16,
        after: Option<&TaskId>,
    ) -> StateResult<TaskPage> {
        if limit == 0 || limit > 200 {
            return Err(StateError::InvalidTaskState(
                "task page limit must be between 1 and 200".into(),
            ));
        }
        validate_optional(
            filter.project_ref.as_deref(),
            MAX_TASK_REF_BYTES,
            "project filter",
        )?;
        let mut sql = String::from("SELECT task_id FROM tasks WHERE owner_uid=?1 AND owner_gid=?2");
        let mut bind_index = 3;
        if filter.state.is_some() {
            sql.push_str(&format!(" AND state=?{bind_index}"));
            bind_index += 1;
        }
        if filter.project_ref.is_some() {
            sql.push_str(&format!(" AND project_ref=?{bind_index}"));
            bind_index += 1;
        }
        if after.is_some() {
            sql.push_str(&format!(" AND task_id < ?{bind_index}"));
            bind_index += 1;
        }
        sql.push_str(&format!(" ORDER BY task_id DESC LIMIT ?{bind_index}"));

        let mut values: Vec<Box<dyn rusqlite::ToSql>> =
            vec![Box::new(principal.uid()), Box::new(principal.gid())];
        if let Some(state) = filter.state {
            values.push(Box::new(state.as_str().to_string()));
        }
        if let Some(project_ref) = &filter.project_ref {
            values.push(Box::new(project_ref.clone()));
        }
        if let Some(after) = after {
            values.push(Box::new(after.to_string()));
        }
        values.push(Box::new(i64::from(limit) + 1));
        let params = rusqlite::params_from_iter(values.iter().map(|value| value.as_ref()));
        let mut statement = self.connection.prepare(&sql)?;
        let ids = statement
            .query_map(params, |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = ids.len() > usize::from(limit);
        let mut items = Vec::new();
        for id in ids.into_iter().take(usize::from(limit)) {
            let task_id = TaskId::from_str(&id).map_err(|error| {
                StateError::InvalidTaskState(format!("invalid stored task id: {error}"))
            })?;
            if let Some(view) = self.task_view_visible(&task_id, principal)? {
                items.push(view.task);
            }
        }
        let next_cursor = has_more
            .then(|| items.last().map(|task| task.task_id.to_string()))
            .flatten();
        Ok(TaskPage { items, next_cursor })
    }

    pub fn task_view_visible(
        &self,
        task_id: &TaskId,
        principal: Principal,
    ) -> StateResult<Option<TaskView>> {
        let Some(task) = self.task_summary_visible(task_id, principal)? else {
            return Ok(None);
        };
        let relationships = self.task_relationships(task_id)?;
        Ok(Some(TaskView {
            task,
            relationships,
        }))
    }

    pub fn task_events_visible(
        &self,
        task_id: &TaskId,
        principal: Principal,
        after_sequence: Option<u64>,
        limit: u16,
    ) -> StateResult<Option<TaskEventPage>> {
        let Some(task) = self.task_summary_visible(task_id, principal)? else {
            return Ok(None);
        };
        if limit == 0 || limit > 200 {
            return Err(StateError::InvalidTaskState(
                "task event limit must be between 1 and 200".into(),
            ));
        }
        let task_ref = task_id.to_string();
        let page = self.significant_events_for_object(
            EventObjectKind::Task,
            &task_ref,
            after_sequence,
            limit,
        )?;
        let events = page
            .events
            .into_iter()
            .map(|event| TaskEvent {
                task_id: *task_id,
                sequence: event.sequence,
                event_kind: event.event_kind,
                source_ref: event.source_ref,
                safe_summary: event.safe_summary,
                safe_data: event.safe_data,
                occurred_at_ms: event.occurred_at_ms,
            })
            .collect();
        Ok(Some(TaskEventPage {
            events,
            retained_from_sequence: page.retained_from_sequence,
            latest_sequence: page.latest_sequence.max(task.last_event_sequence),
            gap_before_page: page.gap_before_page,
            next_sequence: page.next_sequence,
            terminal_state: task.state.is_terminal().then_some(task.state),
        }))
    }

    pub fn transition_task(
        &mut self,
        task_id: &TaskId,
        principal: Principal,
        transition: &TaskTransition<'_>,
    ) -> StateResult<TaskView> {
        validate_optional(
            transition.state_reason,
            MAX_TASK_REASON_BYTES,
            "task state reason",
        )?;
        validate_optional(
            transition.result_summary,
            MAX_TASK_RESULT_BYTES,
            "task result summary",
        )?;
        validate_nonempty(transition.event_kind, 128, "task event kind")?;
        validate_optional(
            transition.event_summary,
            MAX_TASK_EVENT_SUMMARY_BYTES,
            "task event summary",
        )?;
        validate_optional(
            transition.source_ref,
            MAX_TASK_REF_BYTES,
            "task event source",
        )?;
        let encoded_event = serde_json::to_string(&transition.event_data).map_err(|_| {
            StateError::InvalidTaskState("task event data is not serializable".into())
        })?;
        if encoded_event.len() > MAX_TASK_EVENT_DATA_BYTES {
            return Err(StateError::InvalidTaskState(
                "task event data exceeds bounded size".into(),
            ));
        }
        let current = self
            .task_summary_visible(task_id, principal)?
            .ok_or_else(|| StateError::InvalidTaskState("task is not visible".into()))?;
        if current.state != transition.expected {
            return Err(StateError::TaskPreconditionFailed {
                expected: transition.expected.to_string(),
                found: current.state.to_string(),
            });
        }
        let sequence = current.last_event_sequence.saturating_add(1);
        let started_at = if transition.next == TaskState::Running && current.started_at_ms.is_none()
        {
            Some(transition.occurred_at_ms)
        } else {
            current.started_at_ms
        };
        let finished_at = transition
            .next
            .is_terminal()
            .then_some(transition.occurred_at_ms)
            .or(current.finished_at_ms);
        let tx = self.connection.transaction()?;
        let changed = tx.execute(
            "UPDATE tasks SET state=?1, state_reason=?2, waiting_reason=?3, result_kind=?4, result_summary=?5, started_at_ms=?6, finished_at_ms=?7, last_event_sequence=?8, updated_at_ms=?9, cancellation_requested_at_ms=CASE WHEN ?1='cancelling' THEN ?9 ELSE cancellation_requested_at_ms END WHERE task_id=?10 AND owner_uid=?11 AND owner_gid=?12 AND state=?13",
            params![
                transition.next.as_str(),
                transition.state_reason,
                transition.waiting_reason.map(WaitingReason::as_str),
                transition.result_kind.map(TaskResultKind::as_str),
                transition.result_summary,
                started_at,
                finished_at,
                i64::try_from(sequence).unwrap_or(i64::MAX),
                transition.occurred_at_ms,
                task_id.to_string(),
                principal.uid(),
                principal.gid(),
                transition.expected.as_str(),
            ],
        )?;
        if changed != 1 {
            let found: Option<String> = tx
                .query_row(
                    "SELECT state FROM tasks WHERE task_id=?1 AND owner_uid=?2 AND owner_gid=?3",
                    params![task_id.to_string(), principal.uid(), principal.gid()],
                    |row| row.get(0),
                )
                .optional()?;
            return Err(StateError::TaskPreconditionFailed {
                expected: transition.expected.to_string(),
                found: found.unwrap_or_else(|| "not_visible".into()),
            });
        }
        insert_significant_event_tx(
            &tx,
            &NewSignificantEvent {
                object_kind: EventObjectKind::Task,
                object_ref: task_id.to_string(),
                principal: Some(principal),
                event_kind: transition.event_kind.to_string(),
                reason_code: transition.state_reason.map(ToOwned::to_owned),
                source_ref: transition.source_ref.map(ToOwned::to_owned),
                safe_summary: transition.event_summary.map(ToOwned::to_owned),
                safe_data: transition.event_data.clone(),
                occurred_at_ms: transition.occurred_at_ms,
            },
            sequence,
        )?;
        prune_object_tx(&tx, EventObjectKind::Task, &task_id.to_string(), sequence)?;
        tx.commit()?;
        self.task_view_visible(task_id, principal)?
            .ok_or_else(|| StateError::InvalidTaskState("transitioned task is not readable".into()))
    }

    pub fn start_task_attempt(
        &self,
        task_id: &TaskId,
        backend_kind: TaskBackendKind,
        backend_ref: &str,
        retry_safe: bool,
        started_at_ms: i64,
    ) -> StateResult<(i64, u32)> {
        validate_nonempty(backend_ref, MAX_TASK_REF_BYTES, "attempt backend reference")?;
        let next: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM task_attempts WHERE task_id=?1",
            params![task_id.to_string()],
            |row| row.get(0),
        )?;
        self.connection.execute(
            "INSERT INTO task_attempts(task_id, attempt_number, backend_kind, backend_ref, started_at_ms, retry_safe) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![task_id.to_string(), next, backend_kind.as_str(), backend_ref, started_at_ms, i64::from(retry_safe)],
        )?;
        let attempt_id = self.connection.last_insert_rowid();
        Ok((attempt_id, u32::try_from(next).unwrap_or(u32::MAX)))
    }

    pub fn finish_task_attempt(
        &self,
        attempt_id: i64,
        outcome: &str,
        failure_classification: Option<&str>,
        exit_code: Option<i32>,
        finished_at_ms: i64,
    ) -> StateResult<()> {
        validate_nonempty(outcome, 64, "attempt outcome")?;
        validate_optional(failure_classification, 128, "failure classification")?;
        self.connection.execute(
            "UPDATE task_attempts SET finished_at_ms=?1, outcome=?2, failure_classification=?3, exit_code=?4 WHERE attempt_id=?5",
            params![finished_at_ms, outcome, failure_classification, exit_code, attempt_id],
        )?;
        Ok(())
    }

    pub fn add_task_relationship(&self, relation: &NewExecutionRelationship) -> StateResult<i64> {
        validate_nonempty(
            &relation.backend_ref,
            MAX_TASK_REF_BYTES,
            "backend reference",
        )?;
        validate_nonempty(
            &relation.generation_ref,
            MAX_TASK_REF_BYTES,
            "generation reference",
        )?;
        validate_optional(
            relation.correlation_ref.as_deref(),
            MAX_TASK_REF_BYTES,
            "correlation reference",
        )?;
        self.connection.execute(
            "INSERT INTO task_execution_relationships(task_id, mode, backend_kind, backend_ref, generation_ref, process_id, correlation_ref, status, cancellation_supported, reconciliation_supported, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                relation.task_id.to_string(),
                relation.mode.as_str(),
                relation.backend_kind.as_str(),
                relation.backend_ref,
                relation.generation_ref,
                relation.process_id.map(i64::from),
                relation.correlation_ref,
                relation.status.as_str(),
                i64::from(relation.cancellation_supported),
                i64::from(relation.reconciliation_supported),
                relation.created_at_ms,
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn update_task_relationship_status(
        &self,
        relation_id: i64,
        status: ExecutionRelationshipStatus,
        now_ms: i64,
    ) -> StateResult<()> {
        self.connection.execute(
            "UPDATE task_execution_relationships SET status=?1, updated_at_ms=?2, finished_at_ms=CASE WHEN ?1 IN ('stopped','lost') THEN ?2 ELSE finished_at_ms END WHERE relation_id=?3",
            params![status.as_str(), now_ms, relation_id],
        )?;
        Ok(())
    }

    pub fn nonterminal_task_ids(&self) -> StateResult<Vec<(TaskId, Principal, TaskState)>> {
        let mut statement = self.connection.prepare(
            "SELECT task_id, owner_uid, owner_gid, state FROM tasks WHERE state NOT IN ('succeeded','failed','cancelled','interrupted') ORDER BY task_id",
        )?;
        statement
            .query_map([], |row| {
                let task_id = parse_task_id(row.get(0)?, 0)?;
                let state = parse_enum::<TaskState>(row.get::<_, String>(3)?, 3)?;
                Ok((task_id, Principal::new(row.get(1)?, row.get(2)?), state))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    pub fn task_counts_visible(&self, principal: Principal) -> StateResult<(u64, u64)> {
        let active: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM tasks WHERE owner_uid=?1 AND owner_gid=?2 AND state NOT IN ('succeeded','failed','cancelled','interrupted')",
            params![principal.uid(), principal.gid()],
            |row| row.get(0),
        )?;
        let terminal: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM tasks WHERE owner_uid=?1 AND owner_gid=?2 AND state IN ('succeeded','failed','cancelled','interrupted')",
            params![principal.uid(), principal.gid()],
            |row| row.get(0),
        )?;
        Ok((
            u64::try_from(active).unwrap_or(0),
            u64::try_from(terminal).unwrap_or(0),
        ))
    }

    fn task_summary_visible(
        &self,
        task_id: &TaskId,
        principal: Principal,
    ) -> StateResult<Option<TaskSummary>> {
        self.connection
            .query_row(
                "SELECT t.task_id, t.owner_uid, t.owner_gid, t.title, t.objective_summary, t.state, t.state_reason, t.waiting_reason, t.project_ref, t.session_ref, t.parent_task_id, t.retry_of_task_id, t.requester_surface, t.retry_safety, t.created_at_ms, t.started_at_ms, t.finished_at_ms, t.updated_at_ms, t.result_kind, t.result_summary, t.last_event_sequence, (SELECT COUNT(*) FROM task_attempts a WHERE a.task_id=t.task_id), (SELECT COUNT(*) FROM task_execution_relationships r WHERE r.task_id=t.task_id AND r.mode='managed'), (SELECT COUNT(*) FROM task_execution_relationships r WHERE r.task_id=t.task_id AND r.mode='associated') FROM tasks t WHERE t.task_id=?1 AND t.owner_uid=?2 AND t.owner_gid=?3",
                params![task_id.to_string(), principal.uid(), principal.gid()],
                task_summary_from_row,
            )
            .optional()
            .map_err(StateError::from)
    }

    fn task_relationships(&self, task_id: &TaskId) -> StateResult<Vec<TaskExecutionRelationship>> {
        let mut statement = self.connection.prepare(
            "SELECT relation_id, mode, backend_kind, backend_ref, generation_ref, process_id, correlation_ref, status, cancellation_supported, reconciliation_supported, created_at_ms, updated_at_ms, finished_at_ms FROM task_execution_relationships WHERE task_id=?1 ORDER BY relation_id",
        )?;
        statement
            .query_map(params![task_id.to_string()], |row| {
                let mode = parse_enum::<ExecutionRelationshipMode>(row.get::<_, String>(1)?, 1)?;
                let backend_kind = parse_enum::<TaskBackendKind>(row.get::<_, String>(2)?, 2)?;
                let status =
                    parse_enum::<ExecutionRelationshipStatus>(row.get::<_, String>(7)?, 7)?;
                let process_id: Option<i64> = row.get(5)?;
                Ok(TaskExecutionRelationship {
                    relation_id: row.get(0)?,
                    task_id: *task_id,
                    mode,
                    backend_kind,
                    backend_ref: row.get(3)?,
                    generation_ref: row.get(4)?,
                    process_id: process_id.and_then(|value| u32::try_from(value).ok()),
                    correlation_ref: row.get(6)?,
                    status,
                    cancellation_supported: row.get::<_, i64>(8)? != 0,
                    reconciliation_supported: row.get::<_, i64>(9)? != 0,
                    created_at_ms: row.get(10)?,
                    updated_at_ms: row.get(11)?,
                    finished_at_ms: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }
}

fn task_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskSummary> {
    let task_id = parse_task_id(row.get(0)?, 0)?;
    let state = parse_enum::<TaskState>(row.get::<_, String>(5)?, 5)?;
    let waiting_reason = row
        .get::<_, Option<String>>(7)?
        .map(|value| parse_enum::<WaitingReason>(value, 7))
        .transpose()?;
    let parent_task_id = row
        .get::<_, Option<String>>(10)?
        .map(|value| parse_task_id(value, 10))
        .transpose()?;
    let retry_of_task_id = row
        .get::<_, Option<String>>(11)?
        .map(|value| parse_task_id(value, 11))
        .transpose()?;
    let retry_safety = parse_enum::<RetrySafety>(row.get::<_, String>(13)?, 13)?;
    let result_kind = row
        .get::<_, Option<String>>(18)?
        .map(|value| parse_enum::<TaskResultKind>(value, 18))
        .transpose()?;
    let last_event: i64 = row.get(20)?;
    let attempt_count: i64 = row.get(21)?;
    let managed: i64 = row.get(22)?;
    let associated: i64 = row.get(23)?;
    Ok(TaskSummary {
        task_id,
        owner: Principal::new(row.get(1)?, row.get(2)?),
        title: row.get(3)?,
        objective_summary: row.get(4)?,
        state,
        state_reason: row.get(6)?,
        waiting_reason,
        project_ref: row.get(8)?,
        session_ref: row.get(9)?,
        parent_task_id,
        retry_of_task_id,
        requester_surface: row.get(12)?,
        retry_safety,
        created_at_ms: row.get(14)?,
        started_at_ms: row.get(15)?,
        finished_at_ms: row.get(16)?,
        updated_at_ms: row.get(17)?,
        result_kind,
        result_summary: row.get(19)?,
        last_event_sequence: u64::try_from(last_event).unwrap_or(0),
        attempt_count: u32::try_from(attempt_count).unwrap_or(u32::MAX),
        managed_relationships: u32::try_from(managed).unwrap_or(u32::MAX),
        associated_relationships: u32::try_from(associated).unwrap_or(u32::MAX),
    })
}

fn parse_task_id(value: String, column: usize) -> rusqlite::Result<TaskId> {
    TaskId::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })
}

fn validate_nonempty(value: &str, max_bytes: usize, field: &str) -> StateResult<()> {
    if value.trim().is_empty() || value.len() > max_bytes {
        Err(StateError::InvalidTaskState(format!(
            "{field} is empty or exceeds {max_bytes} bytes"
        )))
    } else {
        Ok(())
    }
}

fn validate_optional(value: Option<&str>, max_bytes: usize, field: &str) -> StateResult<()> {
    match value {
        Some(value) => validate_nonempty(value, max_bytes, field),
        None => Ok(()),
    }
}

fn parse_enum<T>(value: String, column: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: ToString,
{
    T::from_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LATEST_SCHEMA_VERSION;
    use std::{fs, path::PathBuf};

    struct TestDb {
        dir: PathBuf,
        path: PathBuf,
    }

    impl TestDb {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("portus-task-state-{name}-{}", TaskId::new()));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("portus.db");
            Self { dir, path }
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn task_spec(owner: Principal) -> NewTaskRecord {
        NewTaskRecord {
            task_id: TaskId::new(),
            owner,
            title: Some("bounded task".into()),
            objective_summary: "prove durable task state".into(),
            requester_surface: "test".into(),
            project_ref: None,
            session_ref: None,
            parent_task_id: None,
            retry_of_task_id: None,
            retry_safety: RetrySafety::Never,
            created_at_ms: 10,
        }
    }

    #[test]
    fn schema_v5_task_record_round_trips_and_filters_by_principal() {
        let test = TestDb::new("roundtrip");
        let mut state = PortusState::open(&test.path).unwrap();
        assert_eq!(state.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
        let owner = Principal::new(1000, 1000);
        let spec = task_spec(owner);
        let view = state.create_task(&spec).unwrap();
        assert_eq!(view.task.state, TaskState::Created);
        assert_eq!(view.task.last_event_sequence, 1);
        assert!(
            state
                .task_view_visible(&spec.task_id, Principal::new(1001, 1001))
                .unwrap()
                .is_none()
        );
        let page = state
            .list_tasks_visible(owner, &TaskListFilter::default(), 50, None)
            .unwrap();
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn expected_state_transition_is_atomic_and_records_significant_event() {
        let test = TestDb::new("transition");
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let spec = task_spec(owner);
        state.create_task(&spec).unwrap();
        let transition = TaskTransition {
            expected: TaskState::Created,
            next: TaskState::Starting,
            state_reason: Some("launching"),
            waiting_reason: None,
            result_kind: None,
            result_summary: None,
            event_kind: "task.starting",
            event_summary: Some("execution starting"),
            source_ref: None,
            event_data: serde_json::json!({"backend":"native_process"}),
            occurred_at_ms: 20,
        };
        let view = state
            .transition_task(&spec.task_id, owner, &transition)
            .unwrap();
        assert_eq!(view.task.state, TaskState::Starting);
        let stale = state
            .transition_task(&spec.task_id, owner, &transition)
            .unwrap_err();
        assert!(matches!(stale, StateError::TaskPreconditionFailed { .. }));
        let events = state
            .task_events_visible(&spec.task_id, owner, None, 50)
            .unwrap()
            .unwrap();
        assert_eq!(events.events.len(), 2);
        assert_eq!(events.events[1].event_kind, "task.starting");
    }

    #[test]
    fn project_and_session_references_are_principal_scoped_without_transcripts() {
        let test = TestDb::new("session");
        let state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let project = state
            .register_project(owner, "project:demo", "/workspace/demo", Some("Demo"), 1)
            .unwrap();
        assert_eq!(project.project_ref, "project:demo");
        let session = state
            .register_session_reference(
                owner,
                "codex:thread-1",
                "codex_root",
                Some("project:demo"),
                Some("worker"),
                Some("/workspace/demo"),
                Some("project-worker"),
                Some("model-ref"),
                Some("idle"),
                2,
            )
            .unwrap();
        assert_eq!(session.session_ref, "codex:thread-1");
        assert!(
            state
                .session_visible("codex:thread-1", Principal::new(1001, 1001))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn task_event_data_is_bounded() {
        let test = TestDb::new("event-bound");
        let mut state = PortusState::open(&test.path).unwrap();
        let owner = Principal::new(1000, 1000);
        let spec = task_spec(owner);
        state.create_task(&spec).unwrap();
        let huge = "x".repeat(MAX_TASK_EVENT_DATA_BYTES + 1);
        let transition = TaskTransition {
            expected: TaskState::Created,
            next: TaskState::Starting,
            state_reason: None,
            waiting_reason: None,
            result_kind: None,
            result_summary: None,
            event_kind: "task.starting",
            event_summary: None,
            source_ref: None,
            event_data: serde_json::json!({"value":huge}),
            occurred_at_ms: 2,
        };
        assert!(
            state
                .transition_task(&spec.task_id, owner, &transition)
                .is_err()
        );
    }
}
