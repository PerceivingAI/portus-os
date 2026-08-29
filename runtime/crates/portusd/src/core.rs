use crate::{EventHub, RuntimeResult};
use portus_artifact::{
    FilesystemCleanupOutcome, FilesystemRegistrationRequest, ProviderRegistrationRequest,
    delete_expected_filesystem_content, prepare_filesystem_registration,
    prepare_provider_registration, reconcile_filesystem,
};
use portus_audit::{AuditSink, NullAuditSink};
use portus_health::{
    DisabledHealthProbes, HealthProbeSet, RESTART_WINDOW_MS, RestartBudgetDecision,
    evaluate_restart_budget,
};
use portus_index::{
    DisabledIndexSources, IndexRescanDomain, IndexSourceSet, SourceBatch, correlate,
};
use portus_policy::{PolicyPaths, PolicySnapshot, PolicyTrust};
use portus_protocol::{
    ArtifactAvailabilityState, ArtifactId, ArtifactLocator, ArtifactView, AuditActor, AuditDomain,
    AuditRecord, AuditResult as AuditOutcome, CURRENT_PROTOCOL_VERSION, ControlPathKind,
    EvidenceStrength, Freshness, HealthComponentType, HealthObservation, HealthReasonCode,
    HealthState, IndexHealthState, IndexObservationInput, IndexRelationInput, IndexResourceType,
    IndexSourceKind, IndexSourceStatus, PolicyActionContext, PolicyEffect, Principal,
    ProtocolError, ProviderRegistrationId, RecoveryAttempt, RecoveryDisposition, RequestEnvelope,
    RequestId, ResponseEnvelope, SemanticError, SemanticErrorCode, TaskId, TaskState,
};
use portus_provider::ManifestTrust;
use portus_state::{DatabaseReadiness, IndexQueryFilter, PortusState, TaskListFilter};
use portus_task::{ManagedProcessSpec, TaskEngine, TaskError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RuntimeReadiness {
    Starting = 0,
    Ready = 1,
    Stopping = 2,
}

impl RuntimeReadiness {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Ready,
            2 => Self::Stopping,
            _ => Self::Starting,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeMethod {
    Ping,
    RuntimeStatus,
    StateStatus,
    CapabilityList,
    CapabilityShow,
    CapabilityProviderList,
    CapabilityProviderShow,
    IndexQuery,
    IndexShow,
    IndexTopology,
    IndexRefresh,
    IndexRescan,
    IndexReconcile,
    IndexRebuild,
    PolicyEffective,
    PolicyCheck,

    TaskList,
    HealthList,
    HealthShow,
    ArtifactList,
    ArtifactShow,

    HealthDegraded,

    TaskShow,
    TaskEvents,
    TaskCancel,
    IndexStatus,
}

impl RuntimeMethod {
    fn parse(method: &str) -> Option<Self> {
        match method {
            "runtime.ping" => Some(Self::Ping),
            "runtime.status" => Some(Self::RuntimeStatus),
            "state.status" => Some(Self::StateStatus),
            "capability.list" => Some(Self::CapabilityList),
            "capability.show" => Some(Self::CapabilityShow),
            "capability.provider.list" => Some(Self::CapabilityProviderList),
            "capability.provider.show" => Some(Self::CapabilityProviderShow),
            "index.query" => Some(Self::IndexQuery),
            "index.show" => Some(Self::IndexShow),
            "index.topology" => Some(Self::IndexTopology),
            "index.refresh" => Some(Self::IndexRefresh),
            "index.rescan" => Some(Self::IndexRescan),
            "index.reconcile" => Some(Self::IndexReconcile),
            "index.rebuild" => Some(Self::IndexRebuild),
            "policy.effective" => Some(Self::PolicyEffective),
            "policy.check" => Some(Self::PolicyCheck),
            "health.list" => Some(Self::HealthList),
            "health.show" => Some(Self::HealthShow),
            "health.degraded" => Some(Self::HealthDegraded),
            "artifact.list" => Some(Self::ArtifactList),
            "artifact.show" => Some(Self::ArtifactShow),
            "index.status" => Some(Self::IndexStatus),
            "task.list" => Some(Self::TaskList),
            "task.show" => Some(Self::TaskShow),
            "task.events" => Some(Self::TaskEvents),
            "task.cancel" => Some(Self::TaskCancel),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct ProviderRegistryStatus {
    health: HealthState,
    reason_code: String,
    active_count: usize,
}

#[derive(Clone, Debug)]
struct PolicyStatus {
    health: HealthState,
    reason_code: String,
}

pub struct RuntimeCore {
    state: Mutex<PortusState>,
    provider_registry: Mutex<ProviderRegistryStatus>,
    policy: Mutex<Option<PolicySnapshot>>,
    policy_status: Mutex<PolicyStatus>,
    index_sources: Arc<dyn IndexSourceSet>,
    readiness: AtomicU8,
    health_probes: Arc<dyn HealthProbeSet>,

    events: EventHub,
    task_engine: TaskEngine,
    audit: Arc<dyn AuditSink>,
    audit_failures: AtomicU64,
}

impl RuntimeCore {
    pub fn open(state_path: impl AsRef<Path>) -> RuntimeResult<Arc<Self>> {
        Self::open_with_index_sources(state_path, Arc::new(DisabledIndexSources))
    }

    pub fn open_with_index_sources(
        state_path: impl AsRef<Path>,
        index_sources: Arc<dyn IndexSourceSet>,
    ) -> RuntimeResult<Arc<Self>> {
        Self::open_with_sources_and_audit(
            state_path,
            index_sources,
            Arc::new(DisabledHealthProbes),
            Arc::new(NullAuditSink),
        )
    }

    pub fn open_with_index_sources_and_audit(
        state_path: impl AsRef<Path>,
        index_sources: Arc<dyn IndexSourceSet>,
        audit: Arc<dyn AuditSink>,
    ) -> RuntimeResult<Arc<Self>> {
        Self::open_with_sources_and_audit(
            state_path,
            index_sources,
            Arc::new(DisabledHealthProbes),
            audit,
        )
    }

    pub fn open_with_sources_and_audit(
        state_path: impl AsRef<Path>,
        index_sources: Arc<dyn IndexSourceSet>,
        health_probes: Arc<dyn HealthProbeSet>,
        audit: Arc<dyn AuditSink>,
    ) -> RuntimeResult<Arc<Self>> {
        let mut state = PortusState::open(state_path)?;
        state.prepare_index_restart(unix_time_ms())?;
        let events = EventHub::default();
        let task_engine = TaskEngine::with_event_sink(Arc::new(events.clone()));
        task_engine.reconcile_after_runtime_restart(&mut state)?;
        Ok(Arc::new(Self {
            state: Mutex::new(state),
            provider_registry: Mutex::new(ProviderRegistryStatus {
                health: HealthState::Unknown,
                reason_code: "not_reconciled".into(),
                active_count: 0,
            }),
            policy: Mutex::new(None),
            policy_status: Mutex::new(PolicyStatus {
                health: HealthState::Unknown,
                reason_code: "not_loaded".into(),
            }),
            index_sources,
            readiness: AtomicU8::new(RuntimeReadiness::Starting as u8),
            health_probes,

            events,
            task_engine,
            audit,
            audit_failures: AtomicU64::new(0),
        }))
    }

    pub fn restart_budget_for_internal_use(
        &self,
        component_ref: &str,
        now_ms: i64,
        healthy_since_ms: Option<i64>,
    ) -> RuntimeResult<RestartBudgetDecision> {
        let state = self.state.lock().map_err(|_| {
            crate::RuntimeError::State(portus_state::StateError::InvalidHealthState(
                "state lock poisoned".into(),
            ))
        })?;
        let attempts = state
            .restart_attempt_times_since(component_ref, now_ms.saturating_sub(RESTART_WINDOW_MS))?;
        Ok(evaluate_restart_budget(now_ms, &attempts, healthy_since_ms))
    }

    pub fn record_recovery_attempt_for_internal_use(
        &self,
        attempt: &RecoveryAttempt,
    ) -> RuntimeResult<()> {
        self.state
            .lock()
            .map_err(|_| {
                crate::RuntimeError::State(portus_state::StateError::InvalidHealthState(
                    "state lock poisoned".into(),
                ))
            })?
            .record_recovery_attempt(attempt)?;
        Ok(())
    }

    pub fn recover_component_for_internal_use(
        &self,
        principal: Principal,
        component_ref: &str,
    ) -> Result<Value, SemanticError> {
        match component_ref {
            "index:system" => self.reconcile_index_domain(IndexRescanDomain::All, principal),
            _ => Err(SemanticError::new(
                SemanticErrorCode::Unsupported,
                "no safe automatic recovery adapter is registered for this component",
            )),
        }
    }

    pub fn register_filesystem_artifact_for_internal_use(
        &self,
        request: FilesystemRegistrationRequest,
    ) -> RuntimeResult<ArtifactView> {
        let spec = prepare_filesystem_registration(request, unix_time_ms())?;
        let view = self
            .state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?
            .register_artifact(&spec)?;
        let _ = self
            .events
            .publish("artifact.registered", Some(spec.artifact_id.to_string()));
        Ok(view)
    }

    pub fn register_provider_artifact_for_internal_use(
        &self,
        request: ProviderRegistrationRequest,
    ) -> RuntimeResult<ArtifactView> {
        let spec = prepare_provider_registration(request, unix_time_ms())?;
        let view = self
            .state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?
            .register_artifact(&spec)?;
        let _ = self
            .events
            .publish("artifact.registered", Some(spec.artifact_id.to_string()));
        Ok(view)
    }

    pub fn reconcile_artifact_for_internal_use(
        &self,
        artifact_id: &ArtifactId,
        principal: Principal,
    ) -> RuntimeResult<ArtifactView> {
        let view = {
            let state = self.state.lock().map_err(|_| {
                crate::RuntimeError::InvalidConfiguration("state lock poisoned".into())
            })?;
            state
                .artifact_view_visible(artifact_id, principal)?
                .ok_or_else(|| {
                    crate::RuntimeError::State(portus_state::StateError::InvalidArtifactState(
                        "artifact is not visible to caller".into(),
                    ))
                })?
        };
        if view.artifact.availability_state == ArtifactAvailabilityState::Removed {
            return Ok(view);
        }
        let (availability, integrity) = match &view.artifact.locator {
            ArtifactLocator::Filesystem { .. } => {
                let result = reconcile_filesystem(&view.artifact)?;
                (result.availability, result.integrity)
            }
            ArtifactLocator::ProviderResource { reference } => {
                let state = self.state.lock().map_err(|_| {
                    crate::RuntimeError::InvalidConfiguration("state lock poisoned".into())
                })?;
                match state.provider_resource_availability(reference)?.as_deref() {
                    Some("available") => (
                        ArtifactAvailabilityState::Available,
                        view.artifact.integrity_kind,
                    ),
                    Some("unavailable") => (
                        ArtifactAvailabilityState::Unavailable,
                        view.artifact.integrity_kind,
                    ),
                    Some("stale" | "removed") | None => (
                        ArtifactAvailabilityState::Missing,
                        view.artifact.integrity_kind,
                    ),
                    Some(_) => (
                        ArtifactAvailabilityState::Unavailable,
                        view.artifact.integrity_kind,
                    ),
                }
            }
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?;
        state.update_artifact_observation(artifact_id, availability, integrity, unix_time_ms())?;
        let updated = state
            .artifact_view_visible(artifact_id, principal)?
            .ok_or_else(|| {
                crate::RuntimeError::State(portus_state::StateError::InvalidArtifactState(
                    "reconciled artifact is no longer visible".into(),
                ))
            })?;
        drop(state);
        let _ = self
            .events
            .publish("artifact.reconciled", Some(artifact_id.to_string()));
        Ok(updated)
    }

    pub fn forget_artifact_for_internal_use(
        &self,
        artifact_id: &ArtifactId,
        principal: Principal,
    ) -> RuntimeResult<()> {
        self.state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?
            .forget_artifact_metadata(artifact_id, principal, unix_time_ms())?;
        let _ = self
            .events
            .publish("artifact.forgotten", Some(artifact_id.to_string()));
        Ok(())
    }

    pub fn cleanup_artifact_for_internal_use(
        &self,
        artifact_id: &ArtifactId,
        principal: Principal,
    ) -> RuntimeResult<ArtifactView> {
        let now = unix_time_ms();
        let view = {
            let state = self.state.lock().map_err(|_| {
                crate::RuntimeError::InvalidConfiguration("state lock poisoned".into())
            })?;
            let view = state
                .artifact_view_visible(artifact_id, principal)?
                .ok_or_else(|| {
                    crate::RuntimeError::State(portus_state::StateError::InvalidArtifactState(
                        "artifact is not visible to caller".into(),
                    ))
                })?;
            if principal.uid() != 0 && view.artifact.owner != principal {
                return Err(crate::RuntimeError::State(
                    portus_state::StateError::InvalidArtifactState(
                        "caller does not own artifact cleanup authority".into(),
                    ),
                ));
            }
            let eligibility = state.artifact_cleanup_eligibility(artifact_id, now)?;
            if eligibility != portus_state::ArtifactCleanupEligibility::Eligible {
                return Err(crate::RuntimeError::ArtifactCleanupBlocked(eligibility));
            }
            view
        };
        match &view.artifact.locator {
            ArtifactLocator::Filesystem { .. } => {
                match delete_expected_filesystem_content(&view.artifact)? {
                    FilesystemCleanupOutcome::Deleted
                    | FilesystemCleanupOutcome::AlreadyMissing => {}
                }
            }
            ArtifactLocator::ProviderResource { .. } => {
                return Err(crate::RuntimeError::ArtifactCleanupUnsupported);
            }
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?;
        state.mark_artifact_removed(artifact_id, now)?;
        let updated = state
            .artifact_view_visible(artifact_id, principal)?
            .ok_or_else(|| {
                crate::RuntimeError::State(portus_state::StateError::InvalidArtifactState(
                    "cleaned artifact is no longer visible".into(),
                ))
            })?;
        drop(state);
        let _ = self
            .events
            .publish("artifact.removed", Some(artifact_id.to_string()));
        Ok(updated)
    }

    #[must_use]
    pub fn readiness(&self) -> RuntimeReadiness {
        RuntimeReadiness::from_u8(self.readiness.load(Ordering::Acquire))
    }

    #[must_use]
    pub fn health_state(&self) -> HealthState {
        match self.readiness() {
            RuntimeReadiness::Ready => HealthState::Healthy,
            RuntimeReadiness::Starting | RuntimeReadiness::Stopping => HealthState::Unavailable,
        }
    }

    pub fn mark_ready(&self) {
        self.readiness
            .store(RuntimeReadiness::Ready as u8, Ordering::Release);
        let _ = self.events.publish("runtime.ready", None);
    }

    pub fn mark_stopping(&self) {
        self.readiness
            .store(RuntimeReadiness::Stopping as u8, Ordering::Release);
        let _ = self.events.publish("runtime.stopping", None);
    }

    #[must_use]
    pub fn events(&self) -> &EventHub {
        &self.events
    }

    /// Starts the first narrow Portus-managed native process backend for
    /// first-party runtime/subsystem use. This is deliberately not exposed as
    /// a generic JSONL method or `portus-os task create` command.
    pub fn launch_managed_process_for_internal_use(
        &self,
        principal: Principal,
        spec: ManagedProcessSpec,
    ) -> RuntimeResult<portus_protocol::TaskView> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::RuntimeError::InvalidConfiguration("state lock poisoned".into()))?;
        self.task_engine
            .launch_managed_process(&mut state, principal, spec)
            .map_err(crate::RuntimeError::from)
    }

    pub fn reconcile_policy(&self, paths: &PolicyPaths, trust: PolicyTrust) {
        let loaded = PolicySnapshot::load(paths, trust);
        let mut status = match self.policy_status.lock() {
            Ok(status) => status,
            Err(_) => return,
        };
        match loaded {
            Ok(snapshot) => {
                if let Ok(mut policy) = self.policy.lock() {
                    *policy = Some(snapshot);
                    status.health = HealthState::Healthy;
                    status.reason_code = "ready".into();
                } else {
                    status.health = HealthState::Unavailable;
                    status.reason_code = "policy_lock_poisoned".into();
                }
            }
            Err(_) => {
                if let Ok(mut policy) = self.policy.lock() {
                    *policy = None;
                }
                status.health = HealthState::Degraded;
                status.reason_code = "load_failed".into();
            }
        }
    }

    #[cfg(test)]
    fn install_policy_snapshot_for_test(&self, snapshot: PolicySnapshot) {
        *self.policy.lock().unwrap() = Some(snapshot);
        let mut status = self.policy_status.lock().unwrap();
        status.health = HealthState::Healthy;
        status.reason_code = "ready".into();
    }

    pub fn reconcile_provider_manifests(&self, directory: impl AsRef<Path>, trust: ManifestTrust) {
        let directory = directory.as_ref();
        let (outcome, observed_count) = match self.state.lock() {
            Ok(mut state) => {
                let outcome = portus_provider::reconcile_directory(&mut state, directory, trust)
                    .map(|report| report.active.len())
                    .map_err(|_| ());
                let observed_count = state.active_system_provider_count().unwrap_or(0);
                (outcome, observed_count)
            }
            Err(_) => (Err(()), 0),
        };
        let mut status = match self.provider_registry.lock() {
            Ok(status) => status,
            Err(_) => return,
        };
        let audit_succeeded = outcome.is_ok();
        match outcome {
            Ok(active_count) => {
                status.health = HealthState::Healthy;
                status.reason_code = "ready".into();
                status.active_count = active_count;
                let _ = self.events.publish("providers.reconciled", None);
            }
            Err(()) => {
                status.health = HealthState::Degraded;
                status.reason_code = "reconciliation_failed".into();
                status.active_count = observed_count;
                let _ = self.events.publish("providers.reconciliation_failed", None);
            }
        }
        drop(status);
        let record = AuditRecord::new(
            AuditActor::system(),
            AuditDomain::Provider,
            "provider.reconcile",
            if audit_succeeded {
                AuditOutcome::Succeeded
            } else {
                AuditOutcome::Failed
            },
            if audit_succeeded {
                "ready"
            } else {
                "reconciliation_failed"
            },
            unix_time_ms(),
        );
        self.record_audit(&record);
    }

    pub fn dispatch(
        &self,
        principal: Principal,
        request: RequestEnvelope<Value>,
    ) -> ResponseEnvelope<Value> {
        if let Err(error) = request.validate() {
            return protocol_failure(request.request_id, error);
        }

        let request_id = request.request_id;
        let method = request.method.clone();
        let audit_target = audit_target(&method, &request.params);
        let result = match RuntimeMethod::parse(&request.method) {
            Some(RuntimeMethod::Ping) => self.runtime_ping(request.params),
            Some(RuntimeMethod::RuntimeStatus) => self.runtime_status(principal, request.params),
            Some(RuntimeMethod::StateStatus) => self.state_status(request.params),
            Some(RuntimeMethod::CapabilityList) => self.capability_list(principal, request.params),
            Some(RuntimeMethod::CapabilityShow) => self.capability_show(principal, request.params),
            Some(RuntimeMethod::CapabilityProviderList) => {
                self.capability_provider_list(principal, request.params)
            }
            Some(RuntimeMethod::CapabilityProviderShow) => {
                self.capability_provider_show(principal, request.params)
            }
            Some(RuntimeMethod::IndexQuery) => self.index_query(principal, request.params),
            Some(RuntimeMethod::IndexShow) => self.index_show(principal, request.params),
            Some(RuntimeMethod::IndexTopology) => self.index_topology(principal, request.params),
            Some(RuntimeMethod::IndexRefresh) => self.index_refresh(principal, request.params),
            Some(RuntimeMethod::IndexRescan) => self.index_rescan(principal, request.params),
            Some(RuntimeMethod::IndexReconcile) => self.index_reconcile(principal, request.params),
            Some(RuntimeMethod::IndexRebuild) => self.index_rebuild(principal, request.params),
            Some(RuntimeMethod::PolicyEffective) => {
                self.policy_effective(principal, request.params)
            }
            Some(RuntimeMethod::PolicyCheck) => {
                self.policy_check(principal, request_id, request.params)
            }
            Some(RuntimeMethod::HealthList) => self.health_list(principal, request.params),
            Some(RuntimeMethod::HealthShow) => self.health_show(principal, request.params),
            Some(RuntimeMethod::HealthDegraded) => self.health_degraded(principal, request.params),
            Some(RuntimeMethod::ArtifactList) => self.artifact_list(principal, request.params),
            Some(RuntimeMethod::ArtifactShow) => self.artifact_show(principal, request.params),
            Some(RuntimeMethod::IndexStatus) => self.index_status(principal, request.params),
            Some(RuntimeMethod::TaskList) => self.task_list(principal, request.params),
            Some(RuntimeMethod::TaskShow) => self.task_show(principal, request.params),
            Some(RuntimeMethod::TaskEvents) => self.task_events(principal, request.params),
            Some(RuntimeMethod::TaskCancel) => self.task_cancel(principal, request.params),
            None => Err(SemanticError::new(
                SemanticErrorCode::Unsupported,
                "runtime method is not implemented",
            )),
        };

        self.audit_dispatch(principal, request_id, &method, audit_target, &result);

        match result {
            Ok(value) => ResponseEnvelope::success(request_id, value),
            Err(error) => ResponseEnvelope::failure(request_id, error),
        }
    }

    fn audit_dispatch(
        &self,
        principal: Principal,
        request_id: RequestId,
        method: &str,
        target_ref: Option<String>,
        result: &Result<Value, SemanticError>,
    ) {
        let domain = match method {
            "task.cancel" => AuditDomain::Task,
            "index.rebuild" => AuditDomain::Index,
            _ => return,
        };
        let (outcome, reason_code) = match result {
            Ok(_) => (AuditOutcome::Succeeded, "ok".to_string()),
            Err(error) => (
                match error.code {
                    SemanticErrorCode::PermissionDenied => AuditOutcome::Denied,
                    SemanticErrorCode::ApprovalRequired => AuditOutcome::ApprovalRequired,
                    SemanticErrorCode::Cancelled => AuditOutcome::Cancelled,
                    SemanticErrorCode::Interrupted => AuditOutcome::Interrupted,
                    _ => AuditOutcome::Failed,
                },
                error.code.as_str().to_string(),
            ),
        };
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            domain,
            method,
            outcome,
            reason_code,
            unix_time_ms(),
        );
        record.target_ref = target_ref;
        record.request_id = Some(request_id);
        self.record_audit(&record);
    }

    fn record_audit(&self, record: &AuditRecord) {
        if self.audit.record(record).is_err() {
            self.audit_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn runtime_ping(&self, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        if self.readiness() != RuntimeReadiness::Ready {
            return Err(SemanticError::new(
                SemanticErrorCode::Unavailable,
                "runtime is not ready",
            ));
        }
        Ok(json!({
            "status": "ready",
            "protocol_version": CURRENT_PROTOCOL_VERSION.get()
        }))
    }

    fn runtime_status(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        let schema_version = self.schema_version()?;
        let provider_registry = self
            .provider_registry
            .lock()
            .map_err(|_| internal_error("provider registry status lock poisoned"))?;
        let index = self.index_status_value(principal).unwrap_or_else(|_| {
            json!({
                "generation": 0,
                "state": "unavailable",
                "reason_code": "status_unavailable",
                "last_reconcile_at_ms": null,
                "sources": []
            })
        });
        let policy_status = self
            .policy_status
            .lock()
            .map_err(|_| internal_error("policy status lock poisoned"))?;
        let tasks = self.task_status_value(principal).unwrap_or_else(|_| {
            json!({
                "state": "unavailable",
                "reason_code": "status_unavailable",
                "active": 0,
                "terminal": 0
            })
        });
        Ok(json!({
            "readiness": readiness_wire(self.readiness()),
            "health": self.health_state(),
            "protocol_version": CURRENT_PROTOCOL_VERSION.get(),
            "schema_version": schema_version,
            "principal": { "uid": principal.uid(), "gid": principal.gid() },
            "live_event_subscribers": self.events.subscriber_count(),
            "audit": {
                "write_failures": self.audit_failures.load(Ordering::Relaxed)
            },
            "provider_registry": {
                "health": provider_registry.health,
                "reason_code": provider_registry.reason_code,
                "active_count": provider_registry.active_count
            },
            "policy": {
                "health": policy_status.health,
                "reason_code": policy_status.reason_code
            },
            "index": index,
            "tasks": tasks
        }))
    }

    fn state_status(&self, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let schema_version = state.schema_version().map_err(state_error)?;
        let readiness = match state.readiness() {
            DatabaseReadiness::Ready => "ready",
            DatabaseReadiness::IntegrityFailure => "integrity_failure",
            DatabaseReadiness::UnsupportedSchema => "unsupported_schema",
        };
        Ok(json!({ "readiness": readiness, "schema_version": schema_version }))
    }

    fn health_list(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        let observations = self.current_health_catalogue(principal)?;
        let degraded = observations.iter().any(|item| {
            matches!(
                item.health_state,
                HealthState::Degraded | HealthState::Unavailable
            )
        });
        Ok(json!({
            "components": observations,
            "degraded": degraded,
            "observed_at_ms": unix_time_ms(),
        }))
    }

    fn health_show(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: HealthShowParams = parse_params(params)?;
        validate_optional_filter(
            &Some(params.component_ref.clone()),
            192,
            "health component reference",
        )?;
        let observations = self.current_health_catalogue(principal)?;
        observations
            .into_iter()
            .find(|item| item.component_ref == params.component_ref)
            .map(|item| {
                serde_json::to_value(item)
                    .map_err(|_| internal_error("failed to encode health observation"))
            })
            .transpose()?
            .ok_or_else(|| {
                SemanticError::new(
                    SemanticErrorCode::NotFound,
                    "health component is not visible",
                )
            })
    }

    fn health_degraded(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        let observations = self
            .current_health_catalogue(principal)?
            .into_iter()
            .filter(|item| {
                matches!(
                    item.health_state,
                    HealthState::Degraded | HealthState::Unavailable
                )
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "components": observations,
            "count": observations.len(),
            "observed_at_ms": unix_time_ms(),
        }))
    }

    fn current_health_catalogue(
        &self,
        principal: Principal,
    ) -> Result<Vec<HealthObservation>, SemanticError> {
        let now = unix_time_ms();
        let provider_registry = self
            .provider_registry
            .lock()
            .map_err(|_| internal_error("provider registry status lock poisoned"))?
            .clone();
        let policy_status = self
            .policy_status
            .lock()
            .map_err(|_| internal_error("policy status lock poisoned"))?
            .clone();
        let task_status = self.task_status_value(principal)?;
        let audit_failures = self.audit_failures.load(Ordering::Relaxed);
        let (database_readiness, index_status, providers) = {
            let state = self
                .state
                .lock()
                .map_err(|_| internal_error("state lock poisoned"))?;
            let database_readiness = state.readiness();
            let index_status = state.index_runtime_status(principal).map_err(state_error)?;
            let providers = state
                .list_providers_visible(principal, 128, None)
                .map_err(state_error)?
                .items;
            (database_readiness, index_status, providers)
        };

        let mut observations = Vec::new();
        observations.push(runtime_health_observation(self.readiness(), now));
        observations.push(state_health_observation(database_readiness, now));
        observations.push(simple_health_observation(
            "provider-registry:system",
            None,
            HealthComponentType::ProviderRegistry,
            provider_registry.health,
            state_reason_for_health(
                provider_registry.health,
                HealthReasonCode::ProviderDegraded,
                HealthReasonCode::ProviderUnavailable,
            ),
            disposition_for_reconcilable(provider_registry.health),
            now,
        ));
        observations.push(simple_health_observation(
            "policy:system",
            None,
            HealthComponentType::Policy,
            policy_status.health,
            match policy_status.health {
                HealthState::Healthy => HealthReasonCode::Ready,
                HealthState::Unknown => HealthReasonCode::NotProbed,
                HealthState::Degraded | HealthState::Unavailable => {
                    HealthReasonCode::PolicyUnavailable
                }
            },
            match policy_status.health {
                HealthState::Healthy | HealthState::Unknown => RecoveryDisposition::Observe,
                HealthState::Degraded | HealthState::Unavailable => {
                    RecoveryDisposition::AdministratorRequired
                }
            },
            now,
        ));

        let (index_health, index_reason, index_disposition) = match index_status.state {
            IndexHealthState::Healthy => (
                HealthState::Healthy,
                HealthReasonCode::Ready,
                RecoveryDisposition::Observe,
            ),
            IndexHealthState::Degraded => (
                HealthState::Degraded,
                HealthReasonCode::SourceDisconnected,
                RecoveryDisposition::Reconcile,
            ),
            IndexHealthState::Unavailable => (
                HealthState::Unavailable,
                HealthReasonCode::SourceDisconnected,
                RecoveryDisposition::Reconcile,
            ),
            IndexHealthState::Initializing | IndexHealthState::Rebuilding => (
                HealthState::Unknown,
                HealthReasonCode::ReconciliationRequired,
                RecoveryDisposition::Reconcile,
            ),
        };
        observations.push(simple_health_observation(
            "index:system",
            None,
            HealthComponentType::Index,
            index_health,
            index_reason,
            index_disposition,
            now,
        ));
        for source in index_status.sources {
            let component_ref = source.owner.map_or_else(
                || format!("index-source:{}", source.source_id),
                |owner| format!("index-source:{}:uid{}", source.source_id, owner.uid()),
            );
            let (reason, disposition) = match source.health {
                HealthState::Healthy => (HealthReasonCode::Ready, RecoveryDisposition::Observe),
                HealthState::Degraded | HealthState::Unavailable => (
                    HealthReasonCode::SourceDisconnected,
                    RecoveryDisposition::Reconcile,
                ),
                HealthState::Unknown => (HealthReasonCode::NotProbed, RecoveryDisposition::Observe),
            };
            let mut observation = simple_health_observation(
                &component_ref,
                source.owner,
                HealthComponentType::IndexSource,
                source.health,
                reason,
                disposition,
                now,
            );
            observation.source_generation = Some(source.source_generation);
            observation.last_healthy_at_ms = source.last_success_at_ms;
            observations.push(observation);
        }

        for provider in providers {
            let health = parse_provider_health(&provider.health_state);
            let reason = match health {
                HealthState::Healthy => HealthReasonCode::Ready,
                HealthState::Degraded => HealthReasonCode::ProviderDegraded,
                HealthState::Unavailable => HealthReasonCode::ProviderUnavailable,
                HealthState::Unknown => HealthReasonCode::NotProbed,
            };
            observations.push(simple_health_observation(
                &provider.provider_id.to_string(),
                provider.owner,
                HealthComponentType::Provider,
                health,
                reason,
                match health {
                    HealthState::Healthy | HealthState::Unknown => RecoveryDisposition::Observe,
                    HealthState::Degraded | HealthState::Unavailable => {
                        RecoveryDisposition::Reconcile
                    }
                },
                now,
            ));
        }

        let task_ready = task_status.get("state").and_then(Value::as_str) == Some("ready");
        observations.push(simple_health_observation(
            "task-runtime:portusd",
            None,
            HealthComponentType::TaskRuntime,
            if task_ready {
                HealthState::Healthy
            } else {
                HealthState::Unavailable
            },
            if task_ready {
                HealthReasonCode::Ready
            } else {
                HealthReasonCode::StatusUnavailable
            },
            RecoveryDisposition::Observe,
            now,
        ));
        let mut audit_details = BTreeMap::new();
        audit_details.insert("write_failures".into(), audit_failures.to_string());
        let mut audit_observation = simple_health_observation(
            "audit:portusd",
            None,
            HealthComponentType::Audit,
            if audit_failures == 0 {
                HealthState::Healthy
            } else {
                HealthState::Degraded
            },
            if audit_failures == 0 {
                HealthReasonCode::Ready
            } else {
                HealthReasonCode::AuditWriteFailed
            },
            if audit_failures == 0 {
                RecoveryDisposition::Observe
            } else {
                RecoveryDisposition::AdministratorRequired
            },
            now,
        );
        audit_observation.safe_details = audit_details;
        observations.push(audit_observation);
        observations.extend(self.health_probes.collect(now));
        observations.sort_by(|left, right| left.component_ref.cmp(&right.component_ref));
        observations.retain(|item| {
            item.owner.is_none() || item.owner == Some(principal) || principal.uid() == 0
        });

        observations.dedup_by(|left, right| left.component_ref == right.component_ref);

        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| internal_error("state lock poisoned"))?;
            for observation in &observations {
                state
                    .record_health_observation(observation)
                    .map_err(state_error)?;
            }
        }
        Ok(observations)
    }

    fn policy_effective(
        &self,
        principal: Principal,
        params: Value,
    ) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        let policy = self
            .policy
            .lock()
            .map_err(|_| internal_error("policy lock poisoned"))?;
        let snapshot = policy.as_ref().ok_or_else(|| {
            SemanticError::new(
                SemanticErrorCode::SourceUnavailable,
                "administrator policy is unavailable",
            )
        })?;
        let view = snapshot.effective(principal).map_err(policy_error)?;
        serde_json::to_value(view).map_err(|_| internal_error("failed to encode effective policy"))
    }

    fn policy_check(
        &self,
        principal: Principal,
        request_id: RequestId,
        params: Value,
    ) -> Result<Value, SemanticError> {
        let context: PolicyActionContext = parse_params(params)?;
        let decision = {
            let policy = self
                .policy
                .lock()
                .map_err(|_| internal_error("policy lock poisoned"))?;
            let snapshot = policy.as_ref().ok_or_else(|| {
                SemanticError::new(
                    SemanticErrorCode::SourceUnavailable,
                    "administrator policy is unavailable",
                )
            })?;
            snapshot
                .evaluate(principal, &context)
                .map_err(policy_error)?
        };
        let audit_result = match decision.effect {
            PolicyEffect::Allow => AuditOutcome::Succeeded,
            PolicyEffect::Prompt => AuditOutcome::ApprovalRequired,
            PolicyEffect::Reject => AuditOutcome::Denied,
        };
        let mut audit = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::Policy,
            "policy.check",
            audit_result,
            decision.reason_code.clone(),
            unix_time_ms(),
        );
        audit.target_ref = Some(match decision.resource.as_deref() {
            Some(resource) => format!("{}:{resource}", decision.action),
            None => decision.action.clone(),
        });
        audit.request_id = Some(request_id);
        self.record_audit(&audit);
        serde_json::to_value(decision)
            .map_err(|_| internal_error("failed to encode policy decision"))
    }

    fn task_status_value(&self, principal: Principal) -> Result<Value, SemanticError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        self.task_engine
            .refresh_all(&mut state)
            .map_err(task_error)?;
        let (active, terminal) = state.task_counts_visible(principal).map_err(state_error)?;
        Ok(json!({
            "state": "ready",
            "reason_code": "ready",
            "active": active,
            "terminal": terminal
        }))
    }

    fn task_list(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: TaskListParams = parse_params(params)?;
        validate_limit(params.limit)?;
        validate_optional_filter(&params.project_ref, 512, "task project filter")?;
        let cursor = params
            .cursor
            .as_deref()
            .map(str::parse::<TaskId>)
            .transpose()
            .map_err(|_| invalid_request("task cursor is invalid"))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        self.task_engine
            .refresh_all(&mut state)
            .map_err(task_error)?;
        let page = state
            .list_tasks_visible(
                principal,
                &TaskListFilter {
                    state: params.state,
                    project_ref: params.project_ref,
                },
                params.limit,
                cursor.as_ref(),
            )
            .map_err(state_error)?;
        serde_json::to_value(page).map_err(|_| internal_error("failed to encode task page"))
    }

    fn task_show(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: TaskShowParams = parse_params(params)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        self.task_engine
            .refresh_all(&mut state)
            .map_err(task_error)?;
        match state
            .task_view_visible(&params.task_id, principal)
            .map_err(state_error)?
        {
            Some(view) => {
                serde_json::to_value(view).map_err(|_| internal_error("failed to encode task view"))
            }
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "task is not visible to this caller",
            )),
        }
    }

    pub fn task_event_page_for_stream(
        &self,
        principal: Principal,
        task_id: &TaskId,
        after_sequence: Option<u64>,
        limit: u16,
    ) -> Result<portus_protocol::TaskEventPage, SemanticError> {
        validate_limit(limit)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        self.task_engine
            .refresh_all(&mut state)
            .map_err(task_error)?;
        state
            .task_events_visible(task_id, principal, after_sequence, limit)
            .map_err(state_error)?
            .ok_or_else(|| {
                SemanticError::new(
                    SemanticErrorCode::NotFound,
                    "task is not visible to this caller",
                )
            })
    }

    fn task_events(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: TaskEventsParams = parse_params(params)?;
        validate_limit(params.limit)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        self.task_engine
            .refresh_all(&mut state)
            .map_err(task_error)?;
        match state
            .task_events_visible(
                &params.task_id,
                principal,
                params.after_sequence,
                params.limit,
            )
            .map_err(state_error)?
        {
            Some(page) if page.gap_before_page => Err(SemanticError::new(
                SemanticErrorCode::StaleResource,
                "requested task event sequence is older than retained history",
            )
            .with_detail("retained_from", json!(page.retained_from_sequence))
            .with_detail("latest_sequence", json!(page.latest_sequence))),
            Some(page) => serde_json::to_value(page)
                .map_err(|_| internal_error("failed to encode task events")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "task is not visible to this caller",
            )),
        }
    }

    fn task_cancel(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: TaskCancelParams = parse_params(params)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let view = self
            .task_engine
            .cancel_task(&mut state, principal, &params.task_id, params.if_state)
            .map_err(task_error)?;
        serde_json::to_value(view).map_err(|_| internal_error("failed to encode cancelled task"))
    }

    fn capability_list(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: ListParams = parse_params(params)?;
        validate_limit(params.limit)?;
        if params
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 96)
        {
            return Err(invalid_request("capability cursor is invalid"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let page = state
            .list_capabilities_visible(principal, params.limit, params.cursor.as_deref())
            .map_err(state_error)?;
        serde_json::to_value(page).map_err(|_| internal_error("failed to encode capability page"))
    }

    fn capability_show(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: CapabilityShowParams = parse_params(params)?;
        if params.capability_id.is_empty() || params.capability_id.len() > 96 {
            return Err(invalid_request("capability id is invalid"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .capability_visible_by_id(&params.capability_id, principal)
            .map_err(state_error)?
        {
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode capability view")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "capability is not registered for this caller",
            )),
        }
    }

    fn capability_provider_list(
        &self,
        principal: Principal,
        params: Value,
    ) -> Result<Value, SemanticError> {
        let params: ListParams = parse_params(params)?;
        validate_limit(params.limit)?;
        let after = params
            .cursor
            .as_deref()
            .map(str::parse::<ProviderRegistrationId>)
            .transpose()
            .map_err(|_| invalid_request("provider cursor is invalid"))?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let page = state
            .list_providers_visible(principal, params.limit, after.as_ref())
            .map_err(state_error)?;
        serde_json::to_value(page).map_err(|_| internal_error("failed to encode provider page"))
    }

    fn capability_provider_show(
        &self,
        principal: Principal,
        params: Value,
    ) -> Result<Value, SemanticError> {
        let params: ProviderShowParams = parse_params(params)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .provider_visible_by_id(&params.provider_id, principal)
            .map_err(state_error)?
        {
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode provider view")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "provider registration is not visible to this caller",
            )),
        }
    }

    fn artifact_list(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: ListParams = parse_params(params)?;
        validate_limit(params.limit)?;
        let after = params
            .cursor
            .as_deref()
            .map(str::parse::<ArtifactId>)
            .transpose()
            .map_err(|_| invalid_request("artifact cursor is invalid"))?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let page = state
            .list_artifacts_visible(principal, params.limit, after.as_ref())
            .map_err(state_error)?;
        serde_json::to_value(page).map_err(|_| internal_error("failed to encode artifact page"))
    }

    fn artifact_show(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: ArtifactShowParams = parse_params(params)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .artifact_view_visible(&params.artifact_id, principal)
            .map_err(state_error)?
        {
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode artifact view")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "artifact is not visible to this caller",
            )),
        }
    }

    pub fn warm_index_initial(&self) {
        let _ = self.reconcile_index_domain(IndexRescanDomain::All, Principal::new(0, 0));
    }

    fn index_query(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: IndexQueryParams = parse_params(params)?;
        validate_limit(params.limit)?;
        validate_optional_filter(&params.application, 256, "application filter")?;
        validate_optional_filter(&params.provider, 128, "provider filter")?;
        validate_optional_filter(&params.capability, 128, "capability filter")?;
        validate_optional_filter(&params.workspace, 128, "workspace filter")?;
        validate_optional_filter(&params.display, 128, "display filter")?;
        let cursor = params
            .cursor
            .as_deref()
            .map(str::parse::<portus_protocol::IndexHandle>)
            .transpose()
            .map_err(|_| invalid_request("index cursor is invalid"))?;
        let filter = IndexQueryFilter {
            resource_type: params.resource_type,
            freshness: params.freshness,
            source_kind: params.source_kind,
            application: params.application,
            provider: params.provider,
            capability: params.capability,
            workspace: params.workspace,
            display: params.display,
            evidence: params.evidence,
            changed_since_ms: params.changed_since_ms,
            control_path: params.control_path,
        };
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let page = state
            .query_index_visible(principal, &filter, params.limit, cursor.as_ref())
            .map_err(state_error)?;
        serde_json::to_value(page).map_err(|_| internal_error("failed to encode index page"))
    }

    fn index_show(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: IndexResourceParams = parse_params(params)?;
        validate_resource_ref(&params.resource_ref)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .index_view_visible(principal, &params.resource_ref)
            .map_err(state_error)?
        {
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode index resource")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "index resource is not visible to this caller",
            )),
        }
    }

    fn index_topology(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: IndexTopologyParams = parse_params(params)?;
        validate_resource_ref(&params.resource_ref)?;
        if !(1..=6).contains(&params.depth) {
            return Err(invalid_request("topology depth must be between 1 and 6"));
        }
        validate_limit(params.limit)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .index_topology_visible(principal, &params.resource_ref, params.depth, params.limit)
            .map_err(state_error)?
        {
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode index topology")),
            None => Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "index resource is not visible to this caller",
            )),
        }
    }

    fn index_refresh(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: IndexResourceParams = parse_params(params)?;
        validate_resource_ref(&params.resource_ref)?;
        let resource = {
            let state = self
                .state
                .lock()
                .map_err(|_| internal_error("state lock poisoned"))?;
            state
                .index_view_visible(principal, &params.resource_ref)
                .map_err(state_error)?
                .map(|view| view.resource)
        };
        let Some(resource) = resource else {
            return Err(SemanticError::new(
                SemanticErrorCode::NotFound,
                "index resource is not visible to this caller",
            ));
        };
        if resource.freshness == Freshness::Historical {
            return Err(SemanticError::new(
                SemanticErrorCode::StaleResource,
                "index resource generation is historical and cannot be refreshed as current",
            ));
        }
        let domain = match resource.resource_type {
            IndexResourceType::ApplicationDefinition => IndexRescanDomain::Applications,
            IndexResourceType::ApplicationInstance
            | IndexResourceType::Process
            | IndexResourceType::Window
            | IndexResourceType::Workspace
            | IndexResourceType::Display => IndexRescanDomain::Runtime,
            IndexResourceType::OpenRcService => IndexRescanDomain::Services,
            IndexResourceType::ProviderRegistration
            | IndexResourceType::ProviderResource
            | IndexResourceType::RegisteredCapability => IndexRescanDomain::Providers,
        };
        self.reconcile_index_domain(domain, principal)?;
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        match state
            .index_view_visible(principal, &resource.index_handle.to_string())
            .map_err(state_error)?
        {
            Some(view) if view.resource.freshness == Freshness::Historical => {
                Err(SemanticError::new(
                    SemanticErrorCode::StaleResource,
                    "resource disappeared or changed generation during refresh",
                ))
            }
            Some(view) => serde_json::to_value(view)
                .map_err(|_| internal_error("failed to encode refreshed index resource")),
            None => Err(SemanticError::new(
                SemanticErrorCode::StaleResource,
                "resource is no longer present after refresh",
            )),
        }
    }

    fn index_rescan(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        let params: IndexRescanParams = parse_params(params)?;
        let domain = parse_index_domain(&params.domain)?;
        if domain == IndexRescanDomain::All {
            return Err(invalid_request("index rescan requires a named domain"));
        }
        self.reconcile_index_domain(domain, principal)
    }

    fn index_reconcile(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        self.reconcile_index_domain(IndexRescanDomain::All, principal)
    }

    fn index_rebuild(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        if principal.uid() != 0 {
            return Err(SemanticError::new(
                SemanticErrorCode::PermissionDenied,
                "index rebuild is root-only until the P9 policy path is implemented",
            ));
        }
        let now = unix_time_ms();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| internal_error("state lock poisoned"))?;
            state.rebuild_index_derived(now).map_err(state_error)?;
        }
        self.reconcile_index_domain(IndexRescanDomain::All, principal)
    }

    fn index_status(&self, principal: Principal, params: Value) -> Result<Value, SemanticError> {
        parse_empty(params)?;
        self.index_status_value(principal)
    }

    fn index_status_value(&self, principal: Principal) -> Result<Value, SemanticError> {
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let status = state.index_runtime_status(principal).map_err(state_error)?;
        let caller_degraded = status.sources.iter().any(|source| {
            source.owner.is_some()
                && matches!(
                    source.health,
                    HealthState::Degraded | HealthState::Unavailable | HealthState::Unknown
                )
        });
        let mut value = serde_json::to_value(status)
            .map_err(|_| internal_error("failed to encode index status"))?;
        if caller_degraded && value.get("state").and_then(Value::as_str) == Some("healthy") {
            value["state"] = json!("degraded");
            value["reason_code"] = json!("caller_source_degraded");
        }
        Ok(value)
    }

    fn reconcile_index_domain(
        &self,
        domain: IndexRescanDomain,
        principal: Principal,
    ) -> Result<Value, SemanticError> {
        let now = unix_time_ms();
        let mut batches = self.index_sources.collect(domain, principal, now).batches;
        if matches!(
            domain,
            IndexRescanDomain::Providers | IndexRescanDomain::All
        ) {
            batches.push(self.provider_index_batch(now)?);
        }
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| internal_error("state lock poisoned"))?;
            state
                .set_index_runtime_state(IndexHealthState::Initializing, "reconciling", now, false)
                .map_err(state_error)?;
            for batch in &batches {
                state
                    .reconcile_index_source(&batch.status, &batch.observations, now)
                    .map_err(state_error)?;
                state
                    .replace_index_relations_for_source(&batch.status.source_id, &batch.relations)
                    .map_err(state_error)?;
            }

            let current = state.current_index_observations().map_err(state_error)?;
            let correlation = correlate(&current, now);
            let source_degraded = batches.iter().any(|batch| {
                matches!(
                    batch.status.health,
                    HealthState::Degraded | HealthState::Unavailable | HealthState::Unknown
                )
            });
            let correlation_status = IndexSourceStatus {
                source_id: portus_index::CORRELATION_SOURCE_ID.into(),
                source_kind: IndexSourceKind::Correlation,
                owner: None,
                source_generation: "correlation-v1".into(),
                health: if source_degraded {
                    HealthState::Degraded
                } else {
                    HealthState::Healthy
                },
                reason_code: if source_degraded {
                    "source_partial"
                } else {
                    "ready"
                }
                .into(),
                last_attempt_at_ms: now,
                last_success_at_ms: Some(now),
            };
            state
                .reconcile_index_source(&correlation_status, &correlation.observations, now)
                .map_err(state_error)?;
            state
                .replace_index_relations_for_source(
                    portus_index::CORRELATION_SOURCE_ID,
                    &correlation.relations,
                )
                .map_err(state_error)?;

            let system_sources = state
                .index_sources_visible(Principal::new(0, 0))
                .map_err(state_error)?;
            let degraded = system_sources.iter().any(|source| {
                matches!(
                    source.health,
                    HealthState::Degraded | HealthState::Unavailable | HealthState::Unknown
                )
            });
            state
                .set_index_runtime_state(
                    if degraded {
                        IndexHealthState::Degraded
                    } else {
                        IndexHealthState::Healthy
                    },
                    if degraded { "source_degraded" } else { "ready" },
                    now,
                    true,
                )
                .map_err(state_error)?;
        }
        let _ = self.events.publish("index.reconciled", None);
        self.index_status_value(principal)
    }

    fn provider_index_batch(&self, now: i64) -> Result<SourceBatch, SemanticError> {
        let registry_status = self
            .provider_registry
            .lock()
            .map_err(|_| internal_error("provider registry status lock poisoned"))?
            .clone();
        let state = self
            .state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?;
        let registrations = state
            .list_providers_visible(Principal::new(0, 0), 200, None)
            .map_err(state_error)?
            .items
            .into_iter()
            .filter(|registration| registration.scope == "system")
            .collect::<Vec<_>>();
        // Provider/source identity is stable across ordinary health/resource updates.
        // Registration/resource native identities carry their own generation, so a
        // status probe must not rotate every provider index handle.
        let generation = "provider-registry-v1".to_string();
        let provider_resources = state
            .provider_resources_visible(Principal::new(0, 0))
            .map_err(state_error)?
            .into_iter()
            .filter(|resource| resource.provider_scope == "system")
            .collect::<Vec<_>>();
        let mut observations = Vec::new();
        let mut relations = Vec::new();
        for registration in registrations {
            let Some(view) = state
                .provider_visible_by_id(&registration.provider_id, Principal::new(0, 0))
                .map_err(state_error)?
            else {
                continue;
            };
            let provider_ref = registration.provider_id.to_string();
            observations.push(IndexObservationInput {
                resource_type: IndexResourceType::ProviderRegistration,
                source_id: "providers".into(),
                source_kind: IndexSourceKind::Providers,
                source_generation: generation.clone(),
                native_identity: provider_ref.clone(),
                authoritative_ref: Some(provider_ref.clone()),
                owner: None,
                freshness: Freshness::Recent,
                observed_at_ms: now,
                metadata: json!({
                    "provider_id": provider_ref,
                    "provider_type": registration.provider_type,
                    "label": registration.display_label,
                    "health": registration.health_state,
                    "compatibility": registration.compatibility_state,
                }),
                control_paths: vec![ControlPathKind::RegisteredProvider],
            });
            for capability in view.capabilities {
                let capability_ref = format!(
                    "provider-capability:{}:{}",
                    registration.provider_id, capability.capability_id
                );
                observations.push(IndexObservationInput {
                    resource_type: IndexResourceType::RegisteredCapability,
                    source_id: "providers".into(),
                    source_kind: IndexSourceKind::Providers,
                    source_generation: generation.clone(),
                    native_identity: capability_ref.clone(),
                    authoritative_ref: Some(capability_ref.clone()),
                    owner: None,
                    freshness: Freshness::Recent,
                    observed_at_ms: now,
                    metadata: json!({
                        "provider_id": registration.provider_id,
                        "provider_type": registration.provider_type,
                        "capability_id": capability.capability_id,
                        "contract_version": capability.contract_version,
                        "availability": capability.availability_state,
                    }),
                    control_paths: vec![ControlPathKind::RegisteredProvider],
                });
                relations.push(IndexRelationInput {
                    from_authoritative_ref: provider_ref.clone(),
                    to_authoritative_ref: capability_ref,
                    relation_kind: "provider_capability".into(),
                    evidence_strength: EvidenceStrength::Authoritative,
                    source_id: "providers".into(),
                    source_kind: IndexSourceKind::Providers,
                    reason_code: "capability_registry".into(),
                    observed_at_ms: now,
                });
            }
        }
        for resource in provider_resources {
            let encoded_reference = serde_json::to_string(&resource.reference)
                .map_err(|_| internal_error("provider resource reference encoding failed"))?;
            let resource_ref = format!("provider-resource:{encoded_reference}");
            let freshness = match resource.availability_state.as_str() {
                "available" => Freshness::Recent,
                "stale" => Freshness::Stale,
                "unavailable" => Freshness::Unavailable,
                _ => Freshness::Historical,
            };
            let provider_ref = resource.reference.provider_registration_id.to_string();
            observations.push(IndexObservationInput {
                resource_type: IndexResourceType::ProviderResource,
                source_id: "providers".into(),
                source_kind: IndexSourceKind::Providers,
                source_generation: generation.clone(),
                native_identity: resource_ref.clone(),
                authoritative_ref: Some(resource_ref.clone()),
                owner: resource.owner,
                freshness,
                observed_at_ms: resource.updated_at_ms,
                metadata: json!({
                    "provider_id": resource.reference.provider_registration_id,
                    "provider_type": resource.provider_type,
                    "resource_type": resource.reference.resource_type,
                    "provider_resource_ref": resource.reference,
                    "availability": resource.availability_state,
                }),
                control_paths: vec![ControlPathKind::RegisteredProvider],
            });
            relations.push(IndexRelationInput {
                from_authoritative_ref: provider_ref,
                to_authoritative_ref: resource_ref,
                relation_kind: "provider_resource".into(),
                evidence_strength: EvidenceStrength::Authoritative,
                source_id: "providers".into(),
                source_kind: IndexSourceKind::Providers,
                reason_code: "provider_resource_registry".into(),
                observed_at_ms: resource.updated_at_ms,
            });
        }

        Ok(SourceBatch {
            status: IndexSourceStatus {
                source_id: "providers".into(),
                source_kind: IndexSourceKind::Providers,
                owner: None,
                source_generation: generation,
                health: registry_status.health,
                reason_code: registry_status.reason_code,
                last_attempt_at_ms: now,
                last_success_at_ms: (registry_status.health != HealthState::Unavailable)
                    .then_some(now),
            },
            observations,
            relations,
        })
    }

    fn schema_version(&self) -> Result<u32, SemanticError> {
        self.state
            .lock()
            .map_err(|_| internal_error("state lock poisoned"))?
            .schema_version()
            .map_err(state_error)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    limit: u16,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskListParams {
    limit: u16,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    state: Option<TaskState>,
    #[serde(default)]
    project_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskShowParams {
    task_id: TaskId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskEventsParams {
    task_id: TaskId,
    #[serde(default)]
    after_sequence: Option<u64>,
    limit: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskCancelParams {
    task_id: TaskId,
    #[serde(default)]
    if_state: Option<TaskState>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilityShowParams {
    capability_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderShowParams {
    provider_id: ProviderRegistrationId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactShowParams {
    artifact_id: ArtifactId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexQueryParams {
    limit: u16,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    resource_type: Option<IndexResourceType>,
    #[serde(default)]
    freshness: Option<Freshness>,
    #[serde(default)]
    source_kind: Option<IndexSourceKind>,
    #[serde(default)]
    application: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    evidence: Option<EvidenceStrength>,
    #[serde(default)]
    changed_since_ms: Option<i64>,
    #[serde(default)]
    control_path: Option<ControlPathKind>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexResourceParams {
    resource_ref: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexTopologyParams {
    resource_ref: String,
    depth: u8,
    limit: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexRescanParams {
    domain: String,
}

fn simple_health_observation(
    component_ref: &str,
    owner: Option<Principal>,
    component_type: HealthComponentType,
    health_state: HealthState,
    reason_code: HealthReasonCode,
    recovery_disposition: RecoveryDisposition,
    observed_at_ms: i64,
) -> HealthObservation {
    HealthObservation {
        component_ref: component_ref.into(),
        owner,
        component_type,
        health_state,
        reason_code,
        summary: format!("{component_ref} health is {}", health_state.as_str()),
        source: "portusd-health".into(),
        observed_at_ms,
        source_generation: None,
        last_healthy_at_ms: (health_state == HealthState::Healthy).then_some(observed_at_ms),
        recovery_disposition,
        recovery_attempt_count: 0,
        safe_details: BTreeMap::new(),
    }
}

fn runtime_health_observation(readiness: RuntimeReadiness, now: i64) -> HealthObservation {
    match readiness {
        RuntimeReadiness::Ready => simple_health_observation(
            "runtime:portusd",
            None,
            HealthComponentType::Runtime,
            HealthState::Healthy,
            HealthReasonCode::Ready,
            RecoveryDisposition::Observe,
            now,
        ),
        RuntimeReadiness::Starting => simple_health_observation(
            "runtime:portusd",
            None,
            HealthComponentType::Runtime,
            HealthState::Unavailable,
            HealthReasonCode::Starting,
            RecoveryDisposition::Observe,
            now,
        ),
        RuntimeReadiness::Stopping => simple_health_observation(
            "runtime:portusd",
            None,
            HealthComponentType::Runtime,
            HealthState::Unavailable,
            HealthReasonCode::Stopping,
            RecoveryDisposition::Observe,
            now,
        ),
    }
}

fn state_health_observation(readiness: DatabaseReadiness, now: i64) -> HealthObservation {
    match readiness {
        DatabaseReadiness::Ready => simple_health_observation(
            "state:portus.db",
            None,
            HealthComponentType::State,
            HealthState::Healthy,
            HealthReasonCode::Ready,
            RecoveryDisposition::Observe,
            now,
        ),
        DatabaseReadiness::IntegrityFailure => simple_health_observation(
            "state:portus.db",
            None,
            HealthComponentType::State,
            HealthState::Unavailable,
            HealthReasonCode::StateIntegrityFailed,
            RecoveryDisposition::AdministratorRequired,
            now,
        ),
        DatabaseReadiness::UnsupportedSchema => simple_health_observation(
            "state:portus.db",
            None,
            HealthComponentType::State,
            HealthState::Unavailable,
            HealthReasonCode::Incompatible,
            RecoveryDisposition::AdministratorRequired,
            now,
        ),
    }
}

fn state_reason_for_health(
    health: HealthState,
    degraded: HealthReasonCode,
    unavailable: HealthReasonCode,
) -> HealthReasonCode {
    match health {
        HealthState::Healthy => HealthReasonCode::Ready,
        HealthState::Degraded => degraded,
        HealthState::Unavailable => unavailable,
        HealthState::Unknown => HealthReasonCode::NotProbed,
    }
}

fn disposition_for_reconcilable(health: HealthState) -> RecoveryDisposition {
    match health {
        HealthState::Healthy | HealthState::Unknown => RecoveryDisposition::Observe,
        HealthState::Degraded | HealthState::Unavailable => RecoveryDisposition::Reconcile,
    }
}

fn parse_provider_health(value: &str) -> HealthState {
    match value {
        "healthy" => HealthState::Healthy,
        "degraded" => HealthState::Degraded,
        "unavailable" => HealthState::Unavailable,
        _ => HealthState::Unknown,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthShowParams {
    component_ref: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

fn audit_target(method: &str, params: &Value) -> Option<String> {
    match method {
        "task.cancel" => serde_json::from_value::<TaskCancelParams>(params.clone())
            .ok()
            .map(|params| params.task_id.to_string()),
        "artifact.show" => serde_json::from_value::<ArtifactShowParams>(params.clone())
            .ok()
            .map(|params| params.artifact_id.to_string()),
        "index.rebuild" => Some("index:derived".into()),
        _ => None,
    }
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, SemanticError> {
    serde_json::from_value(params)
        .map_err(|_| invalid_request("method parameters do not match the expected schema"))
}

fn validate_limit(limit: u16) -> Result<(), SemanticError> {
    if (1..=200).contains(&limit) {
        Ok(())
    } else {
        Err(invalid_request("limit must be between 1 and 200"))
    }
}

fn validate_optional_filter(
    value: &Option<String>,
    max_bytes: usize,
    field: &str,
) -> Result<(), SemanticError> {
    if value
        .as_ref()
        .is_some_and(|value| value.is_empty() || value.len() > max_bytes)
    {
        Err(invalid_request(&format!(
            "{field} is empty or exceeds {max_bytes} bytes"
        )))
    } else {
        Ok(())
    }
}

fn validate_resource_ref(value: &str) -> Result<(), SemanticError> {
    if value.is_empty() || value.len() > 1024 {
        Err(invalid_request("resource reference is invalid"))
    } else {
        Ok(())
    }
}

fn parse_index_domain(value: &str) -> Result<IndexRescanDomain, SemanticError> {
    match value {
        "applications" => Ok(IndexRescanDomain::Applications),
        "runtime" => Ok(IndexRescanDomain::Runtime),
        "providers" => Ok(IndexRescanDomain::Providers),
        "services" => Ok(IndexRescanDomain::Services),
        "all" => Ok(IndexRescanDomain::All),
        _ => Err(invalid_request("unknown index rescan domain")),
    }
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn invalid_request(message: &str) -> SemanticError {
    SemanticError::new(SemanticErrorCode::InvalidRequest, message)
}

fn parse_empty(params: Value) -> Result<EmptyParams, SemanticError> {
    serde_json::from_value(params).map_err(|_| {
        SemanticError::new(
            SemanticErrorCode::InvalidRequest,
            "method parameters do not match the expected schema",
        )
    })
}

fn protocol_failure(
    request_id: portus_protocol::RequestId,
    error: ProtocolError,
) -> ResponseEnvelope<Value> {
    ResponseEnvelope::failure(
        request_id,
        SemanticError::new(error.semantic_code(), error.to_string()),
    )
}

fn state_error(_error: portus_state::StateError) -> SemanticError {
    SemanticError::new(
        SemanticErrorCode::Unavailable,
        "Portus state is unavailable",
    )
}

fn policy_error(_error: portus_policy::PolicyError) -> SemanticError {
    SemanticError::new(
        SemanticErrorCode::InvalidRequest,
        "policy action/context is invalid",
    )
}

fn task_error(error: TaskError) -> SemanticError {
    match error {
        TaskError::NotFound => SemanticError::new(
            SemanticErrorCode::NotFound,
            "task is not visible to this caller",
        ),
        TaskError::PreconditionFailed { expected, found } => SemanticError::new(
            SemanticErrorCode::PreconditionFailed,
            "task state precondition failed",
        )
        .with_detail("expected", json!(expected.as_str()))
        .with_detail("found", json!(found.as_str())),
        TaskError::InvalidTransition { from, to } => SemanticError::new(
            SemanticErrorCode::Conflict,
            "task lifecycle transition is not allowed",
        )
        .with_detail("from", json!(from.as_str()))
        .with_detail("to", json!(to.as_str())),
        TaskError::Unsupported(_) => SemanticError::new(
            SemanticErrorCode::Unsupported,
            "task backend does not support the requested lifecycle operation",
        ),
        TaskError::InvalidSpec(_) => SemanticError::new(
            SemanticErrorCode::InvalidRequest,
            "managed task specification is invalid",
        ),
        TaskError::State(_) | TaskError::Io(_) => SemanticError::new(
            SemanticErrorCode::Unavailable,
            "task state or managed backend is unavailable",
        ),
    }
}

fn internal_error(message: &str) -> SemanticError {
    SemanticError::new(SemanticErrorCode::Internal, message)
}

const fn readiness_wire(readiness: RuntimeReadiness) -> &'static str {
    match readiness {
        RuntimeReadiness::Starting => "starting",
        RuntimeReadiness::Ready => "ready",
        RuntimeReadiness::Stopping => "stopping",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_policy::{
        ActionDefinition, ActionRegistry, BundleDefinition, BundleSelection, GlobalPolicy,
        GrantDefinition, SubjectPolicy,
    };
    use portus_protocol::{
        EventObjectKind, ProtocolVersion, RetrySafety, SemanticErrorCode, TaskState,
    };
    use std::{fs, path::PathBuf};

    struct TestCore {
        dir: PathBuf,
        core: Arc<RuntimeCore>,
    }

    #[derive(Default)]
    struct CapturingAudit {
        records: Mutex<Vec<AuditRecord>>,
        fail: bool,
    }

    impl AuditSink for CapturingAudit {
        fn record(&self, record: &AuditRecord) -> portus_audit::AuditResult<()> {
            if self.fail {
                return Err(portus_audit::AuditError::InvalidRecord("fixture failure"));
            }
            self.records.lock().unwrap().push(record.clone());
            Ok(())
        }
    }

    impl TestCore {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("portusd-core-{}", portus_protocol::TaskId::new()));
            fs::create_dir_all(&dir).unwrap();
            let core = RuntimeCore::open(dir.join("portus.db")).unwrap();
            core.mark_ready();
            Self { dir, core }
        }

        fn new_with_audit(audit: Arc<dyn AuditSink>) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "portusd-core-audit-{}",
                portus_protocol::TaskId::new()
            ));
            fs::create_dir_all(&dir).unwrap();
            let core = RuntimeCore::open_with_index_sources_and_audit(
                dir.join("portus.db"),
                Arc::new(DisabledIndexSources),
                audit,
            )
            .unwrap();
            core.mark_ready();
            Self { dir, core }
        }
    }

    fn provider_spec(
        provider_type: &str,
        capability_id: &str,
        owner: Option<Principal>,
    ) -> portus_state::ProviderRegistrationSpec {
        portus_state::ProviderRegistrationSpec {
            provider_type: provider_type.into(),
            display_label: format!("{provider_type} fixture"),
            scope: if owner.is_some() { "user" } else { "system" }.into(),
            owner,
            manifest_id: format!("{provider_type}.toml"),
            manifest_version: 1,
            software_version: "1.0.0".into(),
            lifecycle_ownership: "provider-owned".into(),
            compatibility_state: "unknown".into(),
            health_state: "unknown".into(),
            health_reason: Some("not_probed".into()),
            policy_domain_owner: "provider".into(),
            interfaces: vec![portus_state::ProviderInterfaceSpec {
                interface_id: "cli".into(),
                interface_type: "executable".into(),
                contract_version: 1,
                target: format!("/usr/bin/{provider_type}"),
                structured_output: true,
            }],
            capabilities: vec![portus_state::ProviderCapabilitySpec {
                capability_id: capability_id.into(),
                contract_version: 1,
                interface_ids: vec!["cli".into()],
            }],
            resources: vec![portus_state::ProviderResourceTypeSpec {
                resource_type: "fixture-session".into(),
                authority: "provider".into(),
                lifetime: "session".into(),
            }],
            skills: Vec::new(),
            health_integration_kind: "structured-cli".into(),
            health_reference: Some("cli".into()),
        }
    }

    fn test_policy_snapshot() -> PolicySnapshot {
        PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            ActionRegistry {
                policy_version: 1,
                actions: vec![ActionDefinition {
                    id: "service.restart".into(),
                    label: "Restart service".into(),
                    class: portus_protocol::PolicyEnforcementClass::PrivilegedTypedOperation,
                    resource_kind: Some("openrc_service".into()),
                    resource_required: true,
                    root_equivalent: false,
                }],
            },
            vec![BundleDefinition {
                policy_version: 1,
                id: "system-administration".into(),
                label: "System Administration".into(),
                broad_default: true,
                grants: vec![GrantDefinition {
                    action: "service.restart".into(),
                    effect: PolicyEffect::Allow,
                    resources: vec!["portusd".into()],
                }],
            }],
            vec![SubjectPolicy {
                policy_version: 1,
                uid: 1000,
                label: Some("master".into()),
                bundles: vec![BundleSelection {
                    id: "system-administration".into(),
                    enabled: true,
                }],
                grants: vec![
                    GrantDefinition {
                        action: "service.restart".into(),
                        effect: PolicyEffect::Prompt,
                        resources: vec!["sshd".into()],
                    },
                    GrantDefinition {
                        action: "service.restart".into(),
                        effect: PolicyEffect::Reject,
                        resources: vec!["firewall".into()],
                    },
                ],
            }],
        )
        .unwrap()
    }

    impl Drop for TestCore {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn ping_and_status_use_real_state_readiness() {
        let test = TestCore::new();
        let principal = Principal::new(1000, 100);
        let ping = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.ping", json!({})));
        assert!(ping.ok);
        let status = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.status", json!({})));
        assert!(status.ok);
        let result = status.result.unwrap();
        assert_eq!(result["principal"]["uid"], 1000);
        assert_eq!(
            result["schema_version"],
            portus_state::LATEST_SCHEMA_VERSION
        );
    }

    #[test]
    fn policy_effective_and_check_use_authenticated_principal_and_typed_context() {
        let test = TestCore::new();
        test.core
            .install_policy_snapshot_for_test(test_policy_snapshot());
        let principal = Principal::new(1000, 1000);
        let effective = test.core.dispatch(
            principal,
            RequestEnvelope::new("policy.effective", json!({})),
        );
        assert!(effective.ok);
        assert_eq!(effective.result.as_ref().unwrap()["principal"]["uid"], 1000);

        for (resource, effect) in [
            ("portusd", "allow"),
            ("sshd", "prompt"),
            ("firewall", "reject"),
        ] {
            let checked = test.core.dispatch(
                principal,
                RequestEnvelope::new(
                    "policy.check",
                    json!({"action":"service.restart","resource":resource}),
                ),
            );
            assert!(checked.ok);
            assert_eq!(checked.result.unwrap()["effect"], effect);
        }

        let spoof = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "policy.check",
                json!({"action":"service.restart","resource":"portusd","uid":0}),
            ),
        );
        assert_eq!(spoof.error.unwrap().code, SemanticErrorCode::InvalidRequest);
    }

    #[test]
    fn policy_check_is_audited_with_decision_outcome() {
        let audit = Arc::new(CapturingAudit::default());
        let test = TestCore::new_with_audit(audit.clone());
        test.core
            .install_policy_snapshot_for_test(test_policy_snapshot());
        let request = RequestEnvelope::new(
            "policy.check",
            json!({"action":"service.restart","resource":"sshd"}),
        );
        let request_id = request.request_id;
        let response = test.core.dispatch(Principal::new(1000, 1000), request);
        assert!(response.ok);
        let records = audit.records.lock().unwrap();
        let record = records.last().unwrap();
        assert_eq!(record.domain, AuditDomain::Policy);
        assert_eq!(record.result, AuditOutcome::ApprovalRequired);
        assert_eq!(record.request_id, Some(request_id));
    }

    #[test]
    fn runtime_restart_open_preserves_existing_durable_state() {
        let dir = std::env::temp_dir().join(format!(
            "portusd-restart-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let state_path = dir.join("portus.db");
        let task_id = portus_protocol::TaskId::new();
        let principal = Principal::new(2000, 2000);
        {
            let state = portus_state::PortusState::open(&state_path).unwrap();
            state
                .insert_task_fixture(&task_id, principal, "survive runtime restart", "running", 1)
                .unwrap();
        }
        {
            let core = RuntimeCore::open(&state_path).unwrap();
            core.mark_ready();
            assert_eq!(core.readiness(), RuntimeReadiness::Ready);
        }
        let state = portus_state::PortusState::open_read_only(&state_path).unwrap();
        let record = state
            .task_for_principal(&task_id, principal)
            .unwrap()
            .unwrap();
        assert_eq!(record.state, "interrupted");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn caller_supplied_identity_fields_are_rejected() {
        let test = TestCore::new();
        let principal = Principal::new(1000, 100);
        let response = test.core.dispatch(
            principal,
            RequestEnvelope::new("runtime.status", json!({"uid": 0, "gid": 0})),
        );
        assert!(!response.ok);
        assert_eq!(
            response.error.unwrap().code,
            SemanticErrorCode::InvalidRequest
        );
    }

    #[test]
    fn runtime_status_uses_authenticated_principal_argument_only() {
        let test = TestCore::new();
        let principal = Principal::new(1234, 4321);
        let response = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.status", json!({})));
        let result = response.result.unwrap();
        assert_eq!(result["principal"], json!({"uid":1234,"gid":4321}));
    }

    #[test]
    fn incompatible_protocol_fails_closed() {
        let test = TestCore::new();
        let mut request = RequestEnvelope::new("runtime.ping", json!({}));
        request.version = ProtocolVersion::new(CURRENT_PROTOCOL_VERSION.get() + 1);
        let response = test.core.dispatch(Principal::new(1, 1), request);
        assert_eq!(
            response.error.unwrap().code,
            SemanticErrorCode::IncompatibleProtocol
        );
    }

    #[test]
    fn unknown_method_is_explicitly_unsupported() {
        let test = TestCore::new();
        let response = test.core.dispatch(
            Principal::new(1, 1),
            RequestEnvelope::new("files.read", json!({"path":"/etc/shadow"})),
        );
        assert_eq!(response.error.unwrap().code, SemanticErrorCode::Unsupported);
    }

    #[test]
    fn reconciled_provider_catalogue_is_available_through_typed_runtime_methods() {
        let test = TestCore::new();
        let manifests = test.dir.join("manifests");
        fs::create_dir_all(&manifests).unwrap();
        fs::write(
            manifests.join("test-provider.toml"),
            r#"manifest_version = 1

[provider]
type = "test-provider"
label = "Test Provider"
scope_support = ["system"]
software_version = "1.0.0"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/test-provider"
structured_output = true

[[capabilities]]
id = "test.control"
contract_version = 1
interfaces = ["cli"]

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#,
        )
        .unwrap();
        test.core.reconcile_provider_manifests(
            &manifests,
            portus_provider::ManifestTrust::PretrustedFixture,
        );
        let principal = Principal::new(1000, 1000);
        let list = test.core.dispatch(
            principal,
            RequestEnvelope::new("capability.list", json!({"limit":50,"cursor":null})),
        );
        assert!(list.ok);
        assert_eq!(
            list.result.unwrap()["items"][0]["capability_id"],
            "test.control"
        );

        let providers = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "capability.provider.list",
                json!({"limit":50,"cursor":null}),
            ),
        );
        let provider_id = providers.result.unwrap()["items"][0]["provider_id"]
            .as_str()
            .unwrap()
            .to_string();
        let shown = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "capability.provider.show",
                json!({"provider_id":provider_id}),
            ),
        );
        assert_eq!(shown.result.unwrap()["provider_type"], "test-provider");

        let status = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.status", json!({})));
        let result = status.result.unwrap();
        assert_eq!(result["provider_registry"]["health"], "healthy");
        assert_eq!(result["provider_registry"]["active_count"], 1);
    }

    #[test]
    fn p17_principal_isolation_holds_across_tasks_providers_index_and_artifacts() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let other = Principal::new(1001, 1001);

        let task = {
            let mut state = test.core.state.lock().unwrap();
            test.core
                .task_engine
                .register_associated_execution(
                    &mut state,
                    owner,
                    portus_task::AssociatedExecutionSpec {
                        backend_kind: portus_protocol::TaskBackendKind::CodexRoot,
                        backend_ref: "codex:p17-owner".into(),
                        generation_ref: "generation:p17-owner".into(),
                        correlation_ref: None,
                        title: Some("P17 owner task".into()),
                        objective_summary: "prove cross-surface principal isolation".into(),
                        requester_surface: "p17-test".into(),
                        project_ref: None,
                        session_ref: None,
                        retry_safety: RetrySafety::Never,
                    },
                )
                .unwrap()
        };

        let (private_provider_id, system_provider_id) = {
            let mut state = test.core.state.lock().unwrap();
            let private_provider = state
                .reconcile_provider_registration(
                    &provider_spec("private-fixture", "private.control", Some(owner)),
                    10,
                )
                .unwrap();
            let system_provider = state
                .reconcile_provider_registration(
                    &provider_spec("system-fixture", "browser.control", None),
                    10,
                )
                .unwrap();
            let reference = portus_protocol::ProviderResourceRef::new(
                system_provider.provider_id,
                portus_protocol::ResourceType::new("fixture-session").unwrap(),
                portus_protocol::ProviderResourceId::new("owner-session").unwrap(),
            )
            .with_generation("generation-owner");
            state
                .reconcile_provider_resource_refs(
                    &system_provider.provider_id,
                    "fixture-session",
                    Some(owner),
                    &[portus_state::ProviderResourceRuntimeSpec {
                        reference,
                        availability_state: "available".into(),
                    }],
                    10,
                )
                .unwrap();
            (private_provider.provider_id, system_provider.provider_id)
        };
        {
            let mut registry = test.core.provider_registry.lock().unwrap();
            registry.health = HealthState::Healthy;
            registry.reason_code = "ready".into();
            registry.active_count = 2;
        }

        let file = test.dir.join("p17-private-artifact.txt");
        fs::write(&file, b"private P17 artifact").unwrap();
        let artifact = test
            .core
            .register_filesystem_artifact_for_internal_use(
                portus_artifact::FilesystemRegistrationRequest::retained(
                    owner,
                    &file,
                    portus_protocol::ArtifactType::File,
                ),
            )
            .unwrap();

        let rescan = test.core.dispatch(
            owner,
            RequestEnvelope::new("index.rescan", json!({"domain":"providers"})),
        );
        assert!(rescan.ok);

        let owner_tasks = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.list",
                json!({"limit":50,"cursor":null,"state":null,"project_ref":null}),
            ),
        );
        assert!(
            owner_tasks.result.unwrap()["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["task_id"] == task.task.task_id.to_string())
        );
        let other_tasks = test.core.dispatch(
            other,
            RequestEnvelope::new(
                "task.list",
                json!({"limit":50,"cursor":null,"state":null,"project_ref":null}),
            ),
        );
        assert!(
            other_tasks.result.unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let owner_providers = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "capability.provider.list",
                json!({"limit":50,"cursor":null}),
            ),
        );
        assert!(
            owner_providers.result.unwrap()["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["provider_id"] == private_provider_id.to_string())
        );
        let other_providers = test.core.dispatch(
            other,
            RequestEnvelope::new(
                "capability.provider.list",
                json!({"limit":50,"cursor":null}),
            ),
        );
        let other_provider_items = other_providers.result.unwrap();
        let other_provider_items = other_provider_items["items"].as_array().unwrap();
        assert!(
            !other_provider_items
                .iter()
                .any(|item| item["provider_id"] == private_provider_id.to_string())
        );
        assert!(
            other_provider_items
                .iter()
                .any(|item| item["provider_id"] == system_provider_id.to_string())
        );

        for (principal, expected) in [(owner, 1_usize), (other, 0_usize)] {
            let indexed = test.core.dispatch(
                principal,
                RequestEnvelope::new(
                    "index.query",
                    json!({"limit":50,"resource_type":"provider_resource"}),
                ),
            );
            assert_eq!(
                indexed.result.unwrap()["items"].as_array().unwrap().len(),
                expected
            );
        }

        let owner_artifact = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "artifact.show",
                json!({"artifact_id":artifact.artifact.artifact_id}),
            ),
        );
        assert!(owner_artifact.ok);
        let hidden_artifact = test.core.dispatch(
            other,
            RequestEnvelope::new(
                "artifact.show",
                json!({"artifact_id":artifact.artifact.artifact_id}),
            ),
        );
        assert_eq!(
            hidden_artifact.error.unwrap().code,
            SemanticErrorCode::NotFound
        );
    }

    #[test]
    fn p17_generic_privilege_payload_cannot_route_through_portusd() {
        let test = TestCore::new();
        let principal = Principal::new(1000, 1000);
        let response = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "privilege.execute",
                json!({
                    "action":"service.restart",
                    "resource":"portusd",
                    "uid":0,
                    "command":"/bin/sh"
                }),
            ),
        );
        assert_eq!(response.error.unwrap().code, SemanticErrorCode::Unsupported);
    }

    #[test]
    fn provider_resources_enter_index_as_principal_filtered_opaque_refs_only() {
        let test = TestCore::new();
        let manifest = portus_provider::ProviderManifest::parse(
            "browser-fixture.toml",
            r#"manifest_version = 1

[provider]
type = "browser-fixture"
label = "Browser Fixture"
scope_support = ["system"]
software_version = "1.0.0"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/browser-fixture"
structured_output = true

[[capabilities]]
id = "browser.control"
contract_version = 1
interfaces = ["cli"]

[[resources]]
type = "browser-session"
authority = "provider"
lifetime = "session"

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#,
        )
        .unwrap();
        let registration = {
            let mut state = test.core.state.lock().unwrap();
            let spec = manifest
                .to_system_registration_spec("browser-fixture.toml".into())
                .unwrap();
            let registration = state.reconcile_provider_registration(&spec, 1).unwrap();
            for (owner, id) in [
                (Principal::new(1000, 1000), "br_owner_a"),
                (Principal::new(1001, 1001), "br_owner_b"),
            ] {
                let reference = portus_protocol::ProviderResourceRef::new(
                    registration.provider_id,
                    portus_protocol::ResourceType::new("browser-session").unwrap(),
                    portus_protocol::ProviderResourceId::new(id).unwrap(),
                )
                .with_generation("session-generation");
                state
                    .reconcile_provider_resource_refs(
                        &registration.provider_id,
                        "browser-session",
                        Some(owner),
                        &[portus_state::ProviderResourceRuntimeSpec {
                            reference,
                            availability_state: "available".into(),
                        }],
                        10,
                    )
                    .unwrap();
            }
            registration
        };
        {
            let mut registry = test.core.provider_registry.lock().unwrap();
            registry.health = HealthState::Healthy;
            registry.reason_code = "ready".into();
            registry.active_count = 1;
        }

        let principal = Principal::new(1000, 1000);
        let rescan = test.core.dispatch(
            principal,
            RequestEnvelope::new("index.rescan", json!({"domain":"providers"})),
        );
        assert!(rescan.ok);

        let query = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "index.query",
                json!({"limit":50,"resource_type":"provider_resource"}),
            ),
        );
        assert!(query.ok);
        let result = query.result.unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        let item = &result["items"][0];
        assert_eq!(item["resource_type"], "provider_resource");
        assert_eq!(item["owner"]["uid"], 1000);
        assert_eq!(item["source_generation"], "provider-registry-v1");
        assert_eq!(
            item["metadata"]["provider_id"],
            registration.provider_id.to_string()
        );
        assert_eq!(item["metadata"]["resource_type"], "browser-session");
        let metadata = serde_json::to_string(&item["metadata"]).unwrap();
        assert!(!metadata.contains("url"));
        assert!(!metadata.contains("snapshot"));
        assert!(!metadata.contains("dom"));
        assert!(!metadata.contains("tab"));

        let handle = item["index_handle"].as_str().unwrap();
        let shown = test.core.dispatch(
            principal,
            RequestEnvelope::new("index.show", json!({"resource_ref":handle})),
        );
        assert!(shown.ok);
        assert!(
            shown.result.unwrap()["relations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|relation| relation["relation_kind"] == "provider_resource")
        );

        for (other, expected) in [
            (Principal::new(1001, 1001), 1_usize),
            (Principal::new(1002, 1002), 0_usize),
        ] {
            let query = test.core.dispatch(
                other,
                RequestEnvelope::new(
                    "index.query",
                    json!({"limit":50,"resource_type":"provider_resource"}),
                ),
            );
            assert!(query.ok);
            assert_eq!(
                query.result.unwrap()["items"].as_array().unwrap().len(),
                expected
            );
        }
    }

    #[test]
    fn p17_provider_failure_degrades_only_its_capability_and_not_runtime() {
        let test = TestCore::new();
        let principal = Principal::new(1000, 1000);
        let (degraded_id, healthy_id) = {
            let mut state = test.core.state.lock().unwrap();
            let degraded = state
                .reconcile_provider_registration(
                    &provider_spec("degraded-fixture", "degraded.control", None),
                    10,
                )
                .unwrap();
            let healthy = state
                .reconcile_provider_registration(
                    &provider_spec("healthy-fixture", "healthy.control", None),
                    10,
                )
                .unwrap();
            state
                .update_provider_runtime_status(
                    &degraded.provider_id,
                    &portus_state::ProviderRuntimeStatusSpec {
                        compatibility_state: "compatible".into(),
                        health_state: "degraded".into(),
                        health_reason: Some("source_unavailable".into()),
                        capabilities: vec![portus_state::ProviderCapabilityRuntimeSpec {
                            capability_id: "degraded.control".into(),
                            availability_state: "unavailable".into(),
                            reason_code: Some("source_unavailable".into()),
                        }],
                    },
                    20,
                )
                .unwrap();
            state
                .update_provider_runtime_status(
                    &healthy.provider_id,
                    &portus_state::ProviderRuntimeStatusSpec {
                        compatibility_state: "compatible".into(),
                        health_state: "healthy".into(),
                        health_reason: Some("ready".into()),
                        capabilities: vec![portus_state::ProviderCapabilityRuntimeSpec {
                            capability_id: "healthy.control".into(),
                            availability_state: "available".into(),
                            reason_code: Some("ready".into()),
                        }],
                    },
                    20,
                )
                .unwrap();
            (degraded.provider_id, healthy.provider_id)
        };
        {
            let mut registry = test.core.provider_registry.lock().unwrap();
            registry.health = HealthState::Healthy;
            registry.reason_code = "ready".into();
            registry.active_count = 2;
        }

        let degraded = test.core.dispatch(
            principal,
            RequestEnvelope::new("health.degraded", json!({})),
        );
        let components = degraded.result.unwrap();
        let components = components["components"].as_array().unwrap();
        assert!(
            components
                .iter()
                .any(|item| item["component_ref"] == degraded_id.to_string())
        );
        assert!(
            !components
                .iter()
                .any(|item| item["component_ref"] == healthy_id.to_string())
        );

        let healthy_capability = test.core.dispatch(
            principal,
            RequestEnvelope::new(
                "capability.show",
                json!({"capability_id":"healthy.control"}),
            ),
        );
        assert_eq!(
            healthy_capability.result.unwrap()["providers"][0]["availability_state"],
            "available"
        );
        let ping = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.ping", json!({})));
        assert!(ping.ok);
    }

    #[test]
    fn invalid_manifest_degrades_registry_without_taking_runtime_down() {
        let test = TestCore::new();
        let manifests = test.dir.join("manifests");
        fs::create_dir_all(&manifests).unwrap();
        fs::write(
            manifests.join("bad.toml"),
            "manifest_version = 1\nhealth_check = 'sh -c bad'\n",
        )
        .unwrap();
        test.core.reconcile_provider_manifests(
            &manifests,
            portus_provider::ManifestTrust::PretrustedFixture,
        );
        let principal = Principal::new(1000, 1000);
        let status = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.status", json!({})));
        let result = status.result.unwrap();
        assert_eq!(result["provider_registry"]["health"], "degraded");
        assert_eq!(
            result["provider_registry"]["reason_code"],
            "reconciliation_failed"
        );
        let ping = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.ping", json!({})));
        assert!(ping.ok);
    }

    struct FixtureIndexSources {
        collections: Mutex<Vec<portus_index::SourceCollection>>,
    }

    impl FixtureIndexSources {
        fn new(collections: Vec<portus_index::SourceCollection>) -> Self {
            Self {
                collections: Mutex::new(collections),
            }
        }
    }

    impl IndexSourceSet for FixtureIndexSources {
        fn collect(
            &self,
            _domain: IndexRescanDomain,
            _principal: Principal,
            _observed_at_ms: i64,
        ) -> portus_index::SourceCollection {
            let mut collections = self.collections.lock().unwrap();
            if collections.len() > 1 {
                collections.remove(0)
            } else {
                collections.first().cloned().unwrap_or_default()
            }
        }
    }

    fn fixture_process_collection(health: HealthState) -> portus_index::SourceCollection {
        let now = 10;
        let status = IndexSourceStatus {
            source_id: "proc".into(),
            source_kind: IndexSourceKind::Proc,
            owner: None,
            source_generation: "boot-fixture".into(),
            health,
            reason_code: if health == HealthState::Healthy {
                "ready"
            } else {
                "fixture_unavailable"
            }
            .into(),
            last_attempt_at_ms: now,
            last_success_at_ms: (health != HealthState::Unavailable).then_some(now),
        };
        let observations = if health == HealthState::Unavailable {
            Vec::new()
        } else {
            [Principal::new(1000, 1000), Principal::new(1001, 1001)]
                .into_iter()
                .enumerate()
                .map(|(offset, owner)| {
                    let pid = 40 + offset as u32;
                    IndexObservationInput {
                        resource_type: IndexResourceType::Process,
                        source_id: "proc".into(),
                        source_kind: IndexSourceKind::Proc,
                        source_generation: "boot-fixture".into(),
                        native_identity: format!("{pid}:100"),
                        authoritative_ref: Some(format!("process:boot-fixture:{pid}:100")),
                        owner: Some(owner),
                        freshness: Freshness::Recent,
                        observed_at_ms: now,
                        metadata: json!({"pid":pid,"ppid":1,"start_ticks":100,"comm":"fixture","exe_basename":"fixture"}),
                        control_paths: vec![ControlPathKind::NativeSystem],
                    }
                })
                .collect()
        };
        portus_index::SourceCollection {
            batches: vec![SourceBatch {
                status,
                observations,
                relations: Vec::new(),
            }],
        }
    }

    #[test]
    fn index_runtime_methods_use_injected_sources_and_principal_filtering() {
        let dir = std::env::temp_dir().join(format!(
            "portusd-index-fixture-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let sources = Arc::new(FixtureIndexSources::new(vec![fixture_process_collection(
            HealthState::Healthy,
        )]));
        let core = RuntimeCore::open_with_index_sources(dir.join("portus.db"), sources).unwrap();
        core.mark_ready();
        let principal = Principal::new(1000, 1000);
        let rescan = core.dispatch(
            principal,
            RequestEnvelope::new("index.rescan", json!({"domain":"runtime"})),
        );
        assert!(rescan.ok);
        assert_eq!(rescan.result.unwrap()["state"], "healthy");

        let query = core.dispatch(
            principal,
            RequestEnvelope::new("index.query", json!({"limit":50,"resource_type":"process"})),
        );
        assert!(query.ok);
        let result = query.result.unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        assert_eq!(result["items"][0]["owner"]["uid"], 1000);
        assert!(
            result["items"][0]["index_handle"]
                .as_str()
                .unwrap()
                .starts_with("idx_")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn source_outage_degrades_index_without_disabling_runtime() {
        let dir = std::env::temp_dir().join(format!(
            "portusd-index-outage-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let sources = Arc::new(FixtureIndexSources::new(vec![fixture_process_collection(
            HealthState::Unavailable,
        )]));
        let core = RuntimeCore::open_with_index_sources(dir.join("portus.db"), sources).unwrap();
        core.mark_ready();
        let principal = Principal::new(1000, 1000);
        let rescan = core.dispatch(
            principal,
            RequestEnvelope::new("index.rescan", json!({"domain":"runtime"})),
        );
        assert!(rescan.ok);
        assert_eq!(rescan.result.unwrap()["state"], "degraded");
        let ping = core.dispatch(principal, RequestEnvelope::new("runtime.ping", json!({})));
        assert!(ping.ok);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn non_root_index_rebuild_is_denied_without_mutation() {
        let test = TestCore::new();
        let response = test.core.dispatch(
            Principal::new(1000, 1000),
            RequestEnvelope::new("index.rebuild", json!({})),
        );
        assert_eq!(
            response.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
    }

    #[test]
    fn managed_task_is_visible_through_read_rpcs_but_not_cross_principal() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let other = Principal::new(1001, 1001);
        let task = test
            .core
            .launch_managed_process_for_internal_use(owner, long_running_task_spec())
            .unwrap();
        assert_eq!(task.task.state, TaskState::Running);

        let list = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.list",
                json!({"limit":50,"cursor":null,"state":"running","project_ref":null}),
            ),
        );
        assert!(list.ok);
        assert_eq!(
            list.result.as_ref().unwrap()["items"][0]["task_id"],
            task.task.task_id.to_string()
        );

        let shown = test.core.dispatch(
            owner,
            RequestEnvelope::new("task.show", json!({"task_id":task.task.task_id})),
        );
        assert!(shown.ok);
        assert_eq!(shown.result.as_ref().unwrap()["state"], "running");
        assert_eq!(
            shown.result.as_ref().unwrap()["relationships"][0]["mode"],
            "managed"
        );

        let events = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.events",
                json!({"task_id":task.task.task_id,"after_sequence":0,"limit":50}),
            ),
        );
        assert!(events.ok);
        assert!(
            events.result.as_ref().unwrap()["events"]
                .as_array()
                .unwrap()
                .len()
                >= 3
        );

        let hidden = test.core.dispatch(
            other,
            RequestEnvelope::new("task.show", json!({"task_id":task.task.task_id})),
        );
        assert_eq!(hidden.error.unwrap().code, SemanticErrorCode::NotFound);

        let cancelled = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.cancel",
                json!({"task_id":task.task.task_id,"if_state":"running"}),
            ),
        );
        assert!(cancelled.ok);
        assert_eq!(cancelled.result.unwrap()["state"], "cancelled");
    }

    #[test]
    fn committed_task_transitions_wake_only_matching_subscribers() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let task = test
            .core
            .launch_managed_process_for_internal_use(owner, long_running_task_spec())
            .unwrap();
        let matching = test
            .core
            .events()
            .subscribe_object(EventObjectKind::Task, task.task.task_id.to_string());
        let unrelated = test
            .core
            .events()
            .subscribe_object(EventObjectKind::Task, TaskId::new().to_string());

        let cancelled = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.cancel",
                json!({"task_id":task.task.task_id,"if_state":"running"}),
            ),
        );
        assert!(cancelled.ok);
        let first = matching
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        let second = matching
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        assert_eq!(first.object_kind, EventObjectKind::Task);
        assert_eq!(first.object_ref, task.task.task_id.to_string());
        assert!(first.object_sequence.is_some());
        assert!(second.object_sequence > first.object_sequence);
        assert!(unrelated.try_recv().is_err());
    }

    #[test]
    fn task_events_rpc_rejects_a_resume_point_older_than_retention() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let task = {
            let mut state = test.core.state.lock().unwrap();
            test.core
                .task_engine
                .register_associated_execution(
                    &mut state,
                    owner,
                    portus_task::AssociatedExecutionSpec {
                        backend_kind: portus_protocol::TaskBackendKind::CodexRoot,
                        backend_ref: "codex:fixture".into(),
                        generation_ref: "generation:fixture".into(),
                        correlation_ref: Some("thread:fixture".into()),
                        title: Some("retention fixture".into()),
                        objective_summary: "prove stale task event resume rejection".into(),
                        requester_surface: "test".into(),
                        project_ref: None,
                        session_ref: None,
                        retry_safety: RetrySafety::Never,
                    },
                )
                .unwrap()
        };
        {
            let mut state = test.core.state.lock().unwrap();
            for index in 0..520_u64 {
                state
                    .append_significant_event(&portus_state::NewSignificantEvent {
                        object_kind: EventObjectKind::Task,
                        object_ref: task.task.task_id.to_string(),
                        principal: Some(owner),
                        event_kind: "task.test_progress".into(),
                        reason_code: Some("fixture".into()),
                        source_ref: Some("test".into()),
                        safe_summary: Some(format!("bounded event {index}")),
                        safe_data: json!({"index":index}),
                        occurred_at_ms: index as i64,
                    })
                    .unwrap();
            }
        }
        let response = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.events",
                json!({"task_id":task.task.task_id,"after_sequence":1,"limit":100}),
            ),
        );
        let error = response.error.unwrap();
        assert_eq!(error.code, SemanticErrorCode::StaleResource);
        assert!(error.details.contains_key("retained_from"));
        assert!(error.details.contains_key("latest_sequence"));
    }

    #[test]
    fn task_cancel_precondition_fails_without_mutating_running_task() {
        let test = TestCore::new();
        let owner = Principal::new(2000, 2000);
        let task = test
            .core
            .launch_managed_process_for_internal_use(owner, long_running_task_spec())
            .unwrap();
        let stale = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.cancel",
                json!({"task_id":task.task.task_id,"if_state":"waiting"}),
            ),
        );
        let error = stale.error.unwrap();
        assert_eq!(error.code, SemanticErrorCode::PreconditionFailed);
        assert_eq!(error.details["expected"], "waiting");
        assert_eq!(error.details["found"], "running");

        let shown = test.core.dispatch(
            owner,
            RequestEnvelope::new("task.show", json!({"task_id":task.task.task_id})),
        );
        assert_eq!(shown.result.unwrap()["state"], "running");

        let cleanup = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.cancel",
                json!({"task_id":task.task.task_id,"if_state":"running"}),
            ),
        );
        assert!(cleanup.ok);
    }

    #[test]
    fn generic_task_creation_rpc_does_not_exist_and_status_has_task_counts() {
        let test = TestCore::new();
        let owner = Principal::new(3000, 3000);
        let unsupported = test.core.dispatch(
            owner,
            RequestEnvelope::new("task.create", json!({"command":"whoami"})),
        );
        assert_eq!(
            unsupported.error.unwrap().code,
            SemanticErrorCode::Unsupported
        );

        let task = test
            .core
            .launch_managed_process_for_internal_use(owner, long_running_task_spec())
            .unwrap();
        let status = test
            .core
            .dispatch(owner, RequestEnvelope::new("runtime.status", json!({})));
        assert_eq!(status.result.as_ref().unwrap()["tasks"]["state"], "ready");
        assert_eq!(status.result.as_ref().unwrap()["tasks"]["active"], 1);

        let cleanup = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "task.cancel",
                json!({"task_id":task.task.task_id,"if_state":"running"}),
            ),
        );
        assert!(cleanup.ok);
    }

    fn long_running_task_spec() -> ManagedProcessSpec {
        #[cfg(windows)]
        {
            let mut spec = ManagedProcessSpec::new(
                "ping.exe",
                "fixture.runtime.wait",
                "runtime task fixture awaiting cancellation",
            );
            spec.args = vec!["-n".into(), "6".into(), "127.0.0.1".into()];
            spec.requester_surface = "runtime-test".into();
            spec.retry_safety = RetrySafety::Never;
            spec
        }
        #[cfg(not(windows))]
        {
            let mut spec = ManagedProcessSpec::new(
                "sleep",
                "fixture.runtime.wait",
                "runtime task fixture awaiting cancellation",
            );
            spec.args = vec!["5".into()];
            spec.requester_surface = "runtime-test".into();
            spec.retry_safety = RetrySafety::Never;
            spec
        }
    }

    #[test]
    fn denied_security_sensitive_dispatch_is_audited_with_authenticated_actor() {
        let audit = Arc::new(CapturingAudit::default());
        let test = TestCore::new_with_audit(audit.clone());
        let principal = Principal::new(1000, 1000);
        let request = RequestEnvelope::new("index.rebuild", json!({}));
        let request_id = request.request_id;
        let response = test.core.dispatch(principal, request);
        assert_eq!(
            response.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        let records = audit.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.actor, AuditActor::principal(principal));
        assert_eq!(record.domain, AuditDomain::Index);
        assert_eq!(record.action, "index.rebuild");
        assert_eq!(record.result, AuditOutcome::Denied);
        assert_eq!(record.reason_code, "permission_denied");
        assert_eq!(record.target_ref.as_deref(), Some("index:derived"));
        assert_eq!(record.request_id, Some(request_id));
    }

    #[test]
    fn provider_reconciliation_uses_system_audit_actor() {
        let audit = Arc::new(CapturingAudit::default());
        let test = TestCore::new_with_audit(audit.clone());
        let manifests = test.dir.join("missing-manifests");
        test.core
            .reconcile_provider_manifests(&manifests, ManifestTrust::PretrustedFixture);
        let records = audit.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].actor, AuditActor::system());
        assert_eq!(records[0].domain, AuditDomain::Provider);
        assert_eq!(records[0].action, "provider.reconcile");
        assert_eq!(records[0].result, AuditOutcome::Failed);
    }

    #[test]
    fn audit_write_failure_is_visible_without_changing_operation_result() {
        let audit = Arc::new(CapturingAudit {
            records: Mutex::new(Vec::new()),
            fail: true,
        });
        let test = TestCore::new_with_audit(audit);
        let principal = Principal::new(1000, 1000);
        let response = test
            .core
            .dispatch(principal, RequestEnvelope::new("index.rebuild", json!({})));
        assert_eq!(
            response.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        let status = test
            .core
            .dispatch(principal, RequestEnvelope::new("runtime.status", json!({})));
        assert_eq!(status.result.unwrap()["audit"]["write_failures"], 1);
    }

    struct FixtureHealthProbes {
        observations: Vec<HealthObservation>,
    }

    impl HealthProbeSet for FixtureHealthProbes {
        fn collect(&self, _now_ms: i64) -> Vec<HealthObservation> {
            self.observations.clone()
        }
    }

    #[test]
    fn health_rpcs_use_fresh_catalogue_and_enforce_principal_visibility() {
        let dir = std::env::temp_dir().join(format!(
            "portusd-health-fixture-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut mine = simple_health_observation(
            "storage:mine",
            Some(Principal::new(1000, 1000)),
            HealthComponentType::Storage,
            HealthState::Degraded,
            HealthReasonCode::ResourceLow,
            RecoveryDisposition::Observe,
            10,
        );
        mine.safe_details
            .insert("available_bytes".into(), "100".into());
        let other = simple_health_observation(
            "storage:other",
            Some(Principal::new(1001, 1001)),
            HealthComponentType::Storage,
            HealthState::Unavailable,
            HealthReasonCode::ResourceUnavailable,
            RecoveryDisposition::Observe,
            10,
        );
        let core = RuntimeCore::open_with_sources_and_audit(
            dir.join("portus.db"),
            Arc::new(DisabledIndexSources),
            Arc::new(FixtureHealthProbes {
                observations: vec![mine, other],
            }),
            Arc::new(NullAuditSink),
        )
        .unwrap();
        core.mark_ready();
        let principal = Principal::new(1000, 1000);
        let listed = core.dispatch(principal, RequestEnvelope::new("health.list", json!({})));
        assert!(listed.ok);
        let result = listed.result.unwrap();
        let refs = result["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| value["component_ref"].as_str())
            .collect::<Vec<_>>();
        assert!(refs.contains(&"storage:mine"));
        assert!(!refs.contains(&"storage:other"));
        assert_eq!(result["degraded"], true);

        let degraded = core.dispatch(
            principal,
            RequestEnvelope::new("health.degraded", json!({})),
        );
        let degraded = degraded.result.unwrap();
        assert!(
            degraded["components"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["component_ref"] == "storage:mine")
        );

        let hidden = core.dispatch(
            principal,
            RequestEnvelope::new("health.show", json!({"component_ref":"storage:other"})),
        );
        assert_eq!(hidden.error.unwrap().code, SemanticErrorCode::NotFound);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recovery_is_allowlisted_and_restart_budget_uses_durable_attempts() {
        let dir = std::env::temp_dir().join(format!(
            "portusd-health-recovery-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let sources = Arc::new(FixtureIndexSources::new(vec![fixture_process_collection(
            HealthState::Healthy,
        )]));
        let core = RuntimeCore::open_with_index_sources(dir.join("portus.db"), sources).unwrap();
        core.mark_ready();
        core.install_policy_snapshot_for_test(test_policy_snapshot());
        let policy_before = core.dispatch(
            Principal::new(1000, 1000),
            RequestEnvelope::new("policy.effective", json!({})),
        );
        let recovered = core
            .recover_component_for_internal_use(Principal::new(0, 0), "index:system")
            .unwrap();
        assert!(
            recovered["sources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|source| source["source_id"] == "proc" && source["health"] == "healthy")
        );
        let unsupported = core
            .recover_component_for_internal_use(Principal::new(0, 0), "policy:system")
            .unwrap_err();
        assert_eq!(unsupported.code, SemanticErrorCode::Unsupported);

        let now = 1_000_000_i64;
        let policy_after = core.dispatch(
            Principal::new(1000, 1000),
            RequestEnvelope::new("policy.effective", json!({})),
        );
        assert_eq!(policy_before.result, policy_after.result);

        for (number, at) in [
            (1_u16, now - 100_000),
            (2_u16, now - 10_000),
            (3_u16, now - 1_000),
        ] {
            core.record_recovery_attempt_for_internal_use(&RecoveryAttempt {
                component_ref: "service:fixture".into(),
                action_kind: portus_protocol::RecoveryActionKind::Restart,
                attempt_number: number,
                started_at_ms: at,
                finished_at_ms: Some(at + 1),
                outcome: portus_protocol::RecoveryAttemptOutcome::Failed,
                reason_code: HealthReasonCode::ServiceNotRunning,
                safe_summary: Some("fixture restart failed".into()),
            })
            .unwrap();
        }
        assert_eq!(
            core.restart_budget_for_internal_use("service:fixture", now, None)
                .unwrap(),
            RestartBudgetDecision::Exhausted
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn artifact_rpc_requires_deliberate_registration_and_filters_private_records() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let other = Principal::new(1001, 1001);
        let file = test.dir.join("deliberate-report.txt");
        fs::write(&file, b"deliberate artifact").unwrap();

        let before = test.core.dispatch(
            owner,
            RequestEnvelope::new("artifact.list", json!({"limit":50,"cursor":null})),
        );
        assert!(before.ok);
        assert!(
            before.result.unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let mut request = portus_artifact::FilesystemRegistrationRequest::retained(
            owner,
            &file,
            portus_protocol::ArtifactType::Report,
        );
        request.safe_display_name = Some("deliberate-report.txt".into());
        let registered = test
            .core
            .register_filesystem_artifact_for_internal_use(request)
            .unwrap();
        let artifact_id = registered.artifact.artifact_id;

        let listed = test.core.dispatch(
            owner,
            RequestEnvelope::new("artifact.list", json!({"limit":50,"cursor":null})),
        );
        let result = listed.result.unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        assert_eq!(result["items"][0]["artifact_id"], artifact_id.to_string());

        let shown = test.core.dispatch(
            owner,
            RequestEnvelope::new("artifact.show", json!({"artifact_id":artifact_id})),
        );
        assert!(shown.ok);
        assert_eq!(
            shown.result.as_ref().unwrap()["artifact"]["integrity_kind"],
            "verified"
        );

        let hidden = test.core.dispatch(
            other,
            RequestEnvelope::new("artifact.show", json!({"artifact_id":artifact_id})),
        );
        assert_eq!(hidden.error.unwrap().code, SemanticErrorCode::NotFound);
    }

    #[test]
    fn artifact_reconcile_marks_mismatch_without_rewriting_digest() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let file = test.dir.join("mutable-report.txt");
        fs::write(&file, b"generation one").unwrap();
        let registered = test
            .core
            .register_filesystem_artifact_for_internal_use(
                portus_artifact::FilesystemRegistrationRequest::retained(
                    owner,
                    &file,
                    portus_protocol::ArtifactType::Report,
                ),
            )
            .unwrap();
        let artifact_id = registered.artifact.artifact_id;
        let original_digest = registered.artifact.sha256.clone();
        fs::write(&file, b"generation two is different").unwrap();

        let reconciled = test
            .core
            .reconcile_artifact_for_internal_use(&artifact_id, owner)
            .unwrap();
        assert_eq!(
            reconciled.artifact.integrity_kind,
            portus_protocol::ArtifactIntegrityKind::Mismatch
        );
        assert_eq!(reconciled.artifact.sha256, original_digest);
        assert_eq!(
            reconciled.artifact.availability_state,
            portus_protocol::ArtifactAvailabilityState::Available
        );
    }

    #[test]
    fn artifact_cleanup_requires_eligibility_and_exact_registered_content() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);

        let retained_file = test.dir.join("retained.txt");
        fs::write(&retained_file, b"retain me").unwrap();
        let mut retained_request = portus_artifact::FilesystemRegistrationRequest::retained(
            owner,
            &retained_file,
            portus_protocol::ArtifactType::File,
        );
        retained_request.cleanup_authority = portus_protocol::ArtifactCleanupAuthority::Portus;
        let retained = test
            .core
            .register_filesystem_artifact_for_internal_use(retained_request)
            .unwrap();
        let blocked = test
            .core
            .cleanup_artifact_for_internal_use(&retained.artifact.artifact_id, owner)
            .unwrap_err();
        assert!(matches!(
            blocked,
            crate::RuntimeError::ArtifactCleanupBlocked(
                portus_state::ArtifactCleanupEligibility::Retained
            )
        ));
        assert!(retained_file.exists());

        let changed_file = test.dir.join("changed-temporary.txt");
        fs::write(&changed_file, b"expected bytes").unwrap();
        let mut changed_request = portus_artifact::FilesystemRegistrationRequest::retained(
            owner,
            &changed_file,
            portus_protocol::ArtifactType::File,
        );
        changed_request.retention_kind = portus_protocol::ArtifactRetentionKind::Temporary;
        changed_request.cleanup_authority = portus_protocol::ArtifactCleanupAuthority::Portus;
        let changed = test
            .core
            .register_filesystem_artifact_for_internal_use(changed_request)
            .unwrap();
        fs::write(&changed_file, b"replacement bytes").unwrap();
        let mismatch = test
            .core
            .cleanup_artifact_for_internal_use(&changed.artifact.artifact_id, owner)
            .unwrap_err();
        assert!(matches!(
            mismatch,
            crate::RuntimeError::Artifact(portus_artifact::ArtifactError::ExpectedTargetMismatch)
        ));
        assert!(changed_file.exists());

        let temporary_file = test.dir.join("temporary.txt");
        fs::write(&temporary_file, b"delete exactly this").unwrap();
        let mut temporary_request = portus_artifact::FilesystemRegistrationRequest::retained(
            owner,
            &temporary_file,
            portus_protocol::ArtifactType::File,
        );
        temporary_request.retention_kind = portus_protocol::ArtifactRetentionKind::Temporary;
        temporary_request.cleanup_authority = portus_protocol::ArtifactCleanupAuthority::Portus;
        let temporary = test
            .core
            .register_filesystem_artifact_for_internal_use(temporary_request)
            .unwrap();
        let cleaned = test
            .core
            .cleanup_artifact_for_internal_use(&temporary.artifact.artifact_id, owner)
            .unwrap();
        assert!(!temporary_file.exists());
        assert_eq!(
            cleaned.artifact.availability_state,
            portus_protocol::ArtifactAvailabilityState::Removed
        );
    }

    #[test]
    fn provider_artifact_stays_bound_to_original_provider_generation() {
        let test = TestCore::new();
        let manifests = test.dir.join("artifact-provider-manifests");
        fs::create_dir_all(&manifests).unwrap();
        let manifest_path = manifests.join("artifact-provider.toml");
        let manifest = r#"manifest_version = 1

[provider]
type = "artifact-provider"
label = "Artifact Provider"
scope_support = ["system"]
software_version = "1.0.0"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/artifact-provider"
structured_output = true

[[capabilities]]
id = "artifact.fetch"
contract_version = 1
interfaces = ["cli"]

[[resources]]
type = "document"
authority = "provider"
lifetime = "durable"

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#;
        fs::write(&manifest_path, manifest).unwrap();
        test.core.reconcile_provider_manifests(
            &manifests,
            portus_provider::ManifestTrust::PretrustedFixture,
        );
        let owner = Principal::new(1000, 1000);
        let original_provider_id = {
            let state = test.core.state.lock().unwrap();
            state
                .list_providers_visible(owner, 50, None)
                .unwrap()
                .items
                .into_iter()
                .find(|item| item.provider_type == "artifact-provider")
                .unwrap()
                .provider_id
        };
        let reference = portus_protocol::ProviderResourceRef::new(
            original_provider_id,
            portus_protocol::ResourceType::new("document").unwrap(),
            portus_protocol::ProviderResourceId::new("document-17").unwrap(),
        )
        .with_generation("generation-a");
        {
            let state = test.core.state.lock().unwrap();
            state
                .record_provider_resource_ref(&reference, Some(owner), "available", 10)
                .unwrap();
        }
        let registered = test
            .core
            .register_provider_artifact_for_internal_use(
                portus_artifact::ProviderRegistrationRequest {
                    owner,
                    reference: reference.clone(),
                    artifact_type: portus_protocol::ArtifactType::Report,
                    confidentiality: portus_protocol::ArtifactConfidentiality::Private,
                    retention_kind: portus_protocol::ArtifactRetentionKind::Retained,
                    expires_at_ms: None,
                    provider_digest_sha256: None,
                    size_bytes: None,
                    media_type: Some("application/pdf".into()),
                    created_at_ms: Some(10),
                    project_ref: None,
                    safe_display_name: Some("provider-report.pdf".into()),
                    safe_metadata: BTreeMap::new(),
                    source_task_id: None,
                    shared_with: Vec::new(),
                    cleanup_authority: portus_protocol::ArtifactCleanupAuthority::None,
                    cleanup_ref: None,
                },
            )
            .unwrap();
        let artifact_id = registered.artifact.artifact_id;

        fs::remove_file(&manifest_path).unwrap();
        test.core.reconcile_provider_manifests(
            &manifests,
            portus_provider::ManifestTrust::PretrustedFixture,
        );
        fs::write(&manifest_path, manifest).unwrap();
        test.core.reconcile_provider_manifests(
            &manifests,
            portus_provider::ManifestTrust::PretrustedFixture,
        );
        let replacement_provider_id = {
            let state = test.core.state.lock().unwrap();
            state
                .list_providers_visible(owner, 50, None)
                .unwrap()
                .items
                .into_iter()
                .find(|item| item.provider_type == "artifact-provider")
                .unwrap()
                .provider_id
        };
        assert_ne!(replacement_provider_id, original_provider_id);

        let reconciled = test
            .core
            .reconcile_artifact_for_internal_use(&artifact_id, owner)
            .unwrap();
        assert_eq!(
            reconciled.artifact.availability_state,
            portus_protocol::ArtifactAvailabilityState::Missing
        );
        assert_eq!(
            reconciled
                .provider_resource
                .as_ref()
                .unwrap()
                .provider_registration_id,
            original_provider_id
        );
    }

    #[test]
    fn diagnostic_bundle_can_be_deliberately_registered_and_resolved() {
        let test = TestCore::new();
        let owner = Principal::new(1000, 1000);
        let file = test.dir.join("doctor-bundle.json");
        fs::write(&file, br#"{"schema_version":1,"checks":[]}"#).unwrap();
        let registered = test
            .core
            .register_filesystem_artifact_for_internal_use(
                portus_artifact::FilesystemRegistrationRequest::diagnostic_bundle(owner, &file),
            )
            .unwrap();
        assert_eq!(
            registered.artifact.artifact_type,
            portus_protocol::ArtifactType::DiagnosticBundle
        );
        assert_eq!(
            registered.artifact.retention_kind,
            portus_protocol::ArtifactRetentionKind::Temporary
        );
        assert_eq!(
            registered.artifact.confidentiality,
            portus_protocol::ArtifactConfidentiality::Private
        );
        let shown = test.core.dispatch(
            owner,
            RequestEnvelope::new(
                "artifact.show",
                json!({"artifact_id":registered.artifact.artifact_id}),
            ),
        );
        assert!(shown.ok);
    }

    #[test]
    fn stopping_runtime_no_longer_reports_ping_ready() {
        let test = TestCore::new();
        test.core.mark_stopping();
        let response = test.core.dispatch(
            Principal::new(1, 1),
            RequestEnvelope::new("runtime.ping", json!({})),
        );
        assert_eq!(response.error.unwrap().code, SemanticErrorCode::Unavailable);
    }
}
