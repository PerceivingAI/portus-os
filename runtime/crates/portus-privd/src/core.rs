use crate::PolicyRepository;
use portus_audit::{AuditSink, NullAuditSink};
use portus_policy::{
    AdminMutation, PolicyError, PolicySnapshot, SubjectPolicy, apply_admin_mutation,
};
use portus_protocol::{
    AuditActor, AuditDomain, AuditRecord, AuditResult as AuditOutcome, PolicyEffect, Principal,
    PrivilegedOperationRequest, PrivilegedOperationResult, RequestEnvelope, RequestId,
    ResponseEnvelope, SemanticError, SemanticErrorCode,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub trait OperationExecutor: Send + Sync {
    fn execute(
        &self,
        principal: Principal,
        request: &PrivilegedOperationRequest,
    ) -> PrivilegeResult<PrivilegedOperationResult>;
}

#[derive(Default)]
pub struct UnavailableExecutor;

impl OperationExecutor for UnavailableExecutor {
    fn execute(
        &self,
        _principal: Principal,
        _request: &PrivilegedOperationRequest,
    ) -> PrivilegeResult<PrivilegedOperationResult> {
        Err(PrivilegeError::ExecutorUnavailable)
    }
}

#[derive(Debug)]
pub enum PrivilegeError {
    Policy(PolicyError),
    ExecutorUnavailable,
    ExecutionFailed,
}

impl fmt::Display for PrivilegeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Policy(error) => write!(f, "privilege policy error: {error}"),
            Self::ExecutorUnavailable => {
                f.write_str("privileged operation executor is unavailable")
            }
            Self::ExecutionFailed => f.write_str("privileged operation failed"),
        }
    }
}

impl std::error::Error for PrivilegeError {}
impl From<PolicyError> for PrivilegeError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}
pub type PrivilegeResult<T> = Result<T, PrivilegeError>;

pub struct PrivilegeCore {
    policy: Mutex<PolicySnapshot>,
    repository: Arc<dyn PolicyRepository>,
    executor: Arc<dyn OperationExecutor>,
    audit: Arc<dyn AuditSink>,
    audit_failures: AtomicU64,
}

impl PrivilegeCore {
    #[must_use]
    pub fn new(
        snapshot: PolicySnapshot,
        repository: Arc<dyn PolicyRepository>,
        executor: Arc<dyn OperationExecutor>,
    ) -> Self {
        Self::new_with_audit(snapshot, repository, executor, Arc::new(NullAuditSink))
    }

    #[must_use]
    pub fn new_with_audit(
        snapshot: PolicySnapshot,
        repository: Arc<dyn PolicyRepository>,
        executor: Arc<dyn OperationExecutor>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            policy: Mutex::new(snapshot),
            repository,
            executor,
            audit,
            audit_failures: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn audit_failures(&self) -> u64 {
        self.audit_failures.load(Ordering::Relaxed)
    }

    pub fn dispatch_use(
        &self,
        principal: Principal,
        request: RequestEnvelope<Value>,
    ) -> ResponseEnvelope<Value> {
        if let Err(error) = request.validate() {
            return protocol_failure(request.request_id, error.semantic_code(), error.to_string());
        }
        let request_id = request.request_id;
        let result = if request.method == "privilege.execute" {
            self.execute(principal, request_id, request.params)
        } else {
            Err(SemanticError::new(
                SemanticErrorCode::Unsupported,
                "privileged use method is not implemented",
            ))
        };
        to_response(request_id, result)
    }

    pub fn dispatch_admin(
        &self,
        principal: Principal,
        request: RequestEnvelope<Value>,
    ) -> ResponseEnvelope<Value> {
        if let Err(error) = request.validate() {
            return protocol_failure(request.request_id, error.semantic_code(), error.to_string());
        }
        let request_id = request.request_id;
        if principal.uid() != 0 {
            let error = SemanticError::new(
                SemanticErrorCode::PermissionDenied,
                "policy administration requires authenticated UID 0",
            );
            self.audit_admin(
                principal,
                request_id,
                &request.method,
                None,
                false,
                &Err(error.clone()),
            );
            return ResponseEnvelope::failure(request_id, error);
        }
        let method = request.method.clone();
        let target = admin_target(&request.params);
        let root_equivalent_change = self.admin_root_equivalent_change(&method, &request.params);
        let result = match method.as_str() {
            "policy.admin.show" => self.admin_show(request.params),
            "policy.admin.grant" => self.admin_grant(request.params),
            "policy.admin.revoke" => self.admin_revoke(request.params),
            "policy.admin.bundle.set" => self.admin_bundle_set(request.params),
            _ => Err(SemanticError::new(
                SemanticErrorCode::Unsupported,
                "policy admin method is not implemented",
            )),
        };
        self.audit_admin(
            principal,
            request_id,
            &method,
            target,
            root_equivalent_change,
            &result,
        );
        to_response(request_id, result)
    }

    fn execute(
        &self,
        principal: Principal,
        request_id: RequestId,
        params: Value,
    ) -> Result<Value, SemanticError> {
        let request: PrivilegedOperationRequest = parse_params(params)?;
        let decision = {
            let policy = self.policy.lock().map_err(|_| internal_error())?;
            policy
                .evaluate(
                    principal,
                    &portus_protocol::PolicyActionContext {
                        action: request.action.clone(),
                        resource: request.resource.clone(),
                    },
                )
                .map_err(policy_semantic)?
        };
        if decision.enforcement_class
            != portus_protocol::PolicyEnforcementClass::PrivilegedTypedOperation
        {
            return Err(SemanticError::new(
                SemanticErrorCode::Unsupported,
                "action is not an executable privileged typed operation",
            ));
        }
        let result = match decision.effect {
            PolicyEffect::Reject => Err(SemanticError::new(
                SemanticErrorCode::PermissionDenied,
                "administrator policy rejects this privileged operation",
            )),
            PolicyEffect::Prompt => Err(SemanticError::new(
                SemanticErrorCode::ApprovalRequired,
                "administrator approval is required; no privileged mutation was performed",
            )),
            PolicyEffect::Allow => self
                .executor
                .execute(principal, &request)
                .map_err(executor_semantic)
                .and_then(|value| serde_json::to_value(value).map_err(|_| internal_error())),
        };
        self.audit_use(
            principal,
            request_id,
            &request,
            &decision.reason_code,
            &result,
        );
        result
    }

    fn admin_show(&self, params: Value) -> Result<Value, SemanticError> {
        let params: AdminShowParams = parse_params(params)?;
        let policy = self.policy.lock().map_err(|_| internal_error())?;
        let view = policy.subject_view(params.uid).map_err(policy_semantic)?;
        serde_json::to_value(view).map_err(|_| internal_error())
    }

    fn admin_grant(&self, params: Value) -> Result<Value, SemanticError> {
        let params: AdminGrantParams = parse_params(params)?;
        self.mutate_subject(
            params.uid,
            AdminMutation::Grant {
                action: params.action,
                effect: params.effect,
                resource: params.resource,
                ack_root_equivalent: params.ack_root_equivalent,
            },
        )
    }

    fn admin_revoke(&self, params: Value) -> Result<Value, SemanticError> {
        let params: AdminRevokeParams = parse_params(params)?;
        self.mutate_subject(
            params.uid,
            AdminMutation::Revoke {
                action: params.action,
                resource: params.resource,
            },
        )
    }

    fn admin_bundle_set(&self, params: Value) -> Result<Value, SemanticError> {
        let params: AdminBundleParams = parse_params(params)?;
        self.mutate_subject(
            params.uid,
            AdminMutation::BundleSet {
                bundle: params.bundle,
                enabled: params.enabled,
            },
        )
    }

    fn mutate_subject(&self, uid: u32, mutation: AdminMutation) -> Result<Value, SemanticError> {
        let mut policy = self.policy.lock().map_err(|_| internal_error())?;
        let mut candidate: SubjectPolicy = policy.subject(uid);
        apply_admin_mutation(&policy, &mut candidate, mutation).map_err(policy_semantic)?;
        self.repository.commit_subject(&candidate).map_err(|_| {
            SemanticError::new(
                SemanticErrorCode::Unavailable,
                "administrator policy update could not be committed",
            )
        })?;
        policy.replace_subject(candidate).map_err(policy_semantic)?;
        let view = policy.subject_view(uid).map_err(policy_semantic)?;
        serde_json::to_value(view).map_err(|_| internal_error())
    }

    fn admin_root_equivalent_change(&self, method: &str, params: &Value) -> bool {
        let Some(action) = params.get("action").and_then(Value::as_str) else {
            return false;
        };
        let Ok(policy) = self.policy.lock() else {
            return false;
        };
        let Some(definition) = policy.action(action) else {
            return false;
        };
        if !definition.root_equivalent {
            return false;
        }
        match method {
            "policy.admin.grant" => params
                .get("effect")
                .and_then(Value::as_str)
                .is_some_and(|effect| effect != "reject"),
            "policy.admin.revoke" => true,
            _ => false,
        }
    }

    fn audit_use(
        &self,
        principal: Principal,
        request_id: RequestId,
        request: &PrivilegedOperationRequest,
        reason: &str,
        result: &Result<Value, SemanticError>,
    ) {
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::Privilege,
            request.action.clone(),
            audit_outcome(result),
            result
                .as_ref()
                .err()
                .map_or(reason, |error| error.code.as_str()),
            now_ms(),
        );
        record.target_ref = request
            .resource
            .as_ref()
            .map(|resource| format!("{}:{resource}", request.action))
            .or_else(|| Some(request.action.clone()));
        record.request_id = Some(request_id);
        self.record_audit(&record);
    }

    fn audit_admin(
        &self,
        principal: Principal,
        request_id: RequestId,
        method: &str,
        target: Option<String>,
        root_equivalent_change: bool,
        result: &Result<Value, SemanticError>,
    ) {
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::Policy,
            method,
            audit_outcome(result),
            result.as_ref().err().map_or(
                if root_equivalent_change {
                    "root_equivalent_policy_change"
                } else {
                    "ok"
                },
                |error| error.code.as_str(),
            ),
            now_ms(),
        );
        record.target_ref = target;
        record.request_id = Some(request_id);
        self.record_audit(&record);
    }

    fn record_audit(&self, record: &AuditRecord) {
        if self.audit.record(record).is_err() {
            self.audit_failures.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminShowParams {
    uid: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminGrantParams {
    uid: u32,
    action: String,
    effect: PolicyEffect,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    ack_root_equivalent: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminRevokeParams {
    uid: u32,
    action: String,
    #[serde(default)]
    resource: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminBundleParams {
    uid: u32,
    bundle: String,
    enabled: bool,
}

fn parse_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, SemanticError> {
    serde_json::from_value(params).map_err(|_| {
        SemanticError::new(
            SemanticErrorCode::InvalidRequest,
            "request parameters do not match the expected typed schema",
        )
    })
}
fn policy_semantic(_error: PolicyError) -> SemanticError {
    SemanticError::new(
        SemanticErrorCode::InvalidRequest,
        "policy action or mutation is invalid",
    )
}
fn executor_semantic(error: PrivilegeError) -> SemanticError {
    match error {
        PrivilegeError::ExecutorUnavailable => SemanticError::new(
            SemanticErrorCode::Unavailable,
            "privileged operation adapter is unavailable",
        ),
        _ => SemanticError::new(SemanticErrorCode::Internal, "privileged operation failed"),
    }
}
fn internal_error() -> SemanticError {
    SemanticError::new(
        SemanticErrorCode::Internal,
        "privilege service internal state is unavailable",
    )
}
fn to_response(
    request_id: RequestId,
    result: Result<Value, SemanticError>,
) -> ResponseEnvelope<Value> {
    match result {
        Ok(value) => ResponseEnvelope::success(request_id, value),
        Err(error) => ResponseEnvelope::failure(request_id, error),
    }
}
fn protocol_failure(
    request_id: RequestId,
    code: SemanticErrorCode,
    message: String,
) -> ResponseEnvelope<Value> {
    ResponseEnvelope::failure(request_id, SemanticError::new(code, message))
}
fn audit_outcome(result: &Result<Value, SemanticError>) -> AuditOutcome {
    match result {
        Ok(_) => AuditOutcome::Succeeded,
        Err(error) => match error.code {
            SemanticErrorCode::PermissionDenied => AuditOutcome::Denied,
            SemanticErrorCode::ApprovalRequired => AuditOutcome::ApprovalRequired,
            SemanticErrorCode::Cancelled => AuditOutcome::Cancelled,
            SemanticErrorCode::Interrupted => AuditOutcome::Interrupted,
            _ => AuditOutcome::Failed,
        },
    }
}
fn admin_target(params: &Value) -> Option<String> {
    params
        .get("uid")
        .and_then(Value::as_u64)
        .map(|uid| format!("uid:{uid}"))
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_policy::{
        ActionDefinition, ActionRegistry, BundleDefinition, BundleSelection, GlobalPolicy,
        GrantDefinition, PolicyResult,
    };
    use portus_protocol::PolicyEnforcementClass;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    #[derive(Default)]
    struct CapturingAudit {
        records: Mutex<Vec<AuditRecord>>,
    }
    impl AuditSink for CapturingAudit {
        fn record(&self, record: &AuditRecord) -> portus_audit::AuditResult<()> {
            self.records.lock().unwrap().push(record.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryRepository {
        commits: Mutex<Vec<SubjectPolicy>>,
    }
    impl PolicyRepository for MemoryRepository {
        fn commit_subject(&self, subject: &SubjectPolicy) -> PolicyResult<()> {
            self.commits.lock().unwrap().push(subject.clone());
            Ok(())
        }
    }
    #[derive(Default)]
    struct CountingExecutor {
        calls: AtomicUsize,
    }
    impl OperationExecutor for CountingExecutor {
        fn execute(
            &self,
            _principal: Principal,
            request: &PrivilegedOperationRequest,
        ) -> PrivilegeResult<PrivilegedOperationResult> {
            self.calls.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(PrivilegedOperationResult {
                action: request.action.clone(),
                resource: request.resource.clone(),
                safe_summary: "fixture operation completed".into(),
            })
        }
    }

    fn snapshot() -> PolicySnapshot {
        PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            ActionRegistry {
                policy_version: 1,
                actions: vec![
                    ActionDefinition {
                        id: "service.restart".into(),
                        label: "Restart service".into(),
                        class: PolicyEnforcementClass::PrivilegedTypedOperation,
                        resource_kind: Some("openrc_service".into()),
                        resource_required: true,
                        root_equivalent: false,
                    },
                    ActionDefinition {
                        id: "root.shell".into(),
                        label: "Root shell".into(),
                        class: PolicyEnforcementClass::RootEquivalent,
                        resource_kind: None,
                        resource_required: false,
                        root_equivalent: true,
                    },
                ],
            },
            vec![BundleDefinition {
                policy_version: 1,
                id: "system-administration".into(),
                label: "System Administration".into(),
                broad_default: true,
                grants: vec![],
            }],
            vec![SubjectPolicy {
                policy_version: 1,
                uid: 1000,
                label: None,
                bundles: vec![BundleSelection {
                    id: "system-administration".into(),
                    enabled: true,
                }],
                grants: vec![
                    GrantDefinition {
                        action: "service.restart".into(),
                        effect: PolicyEffect::Allow,
                        resources: vec!["allowed".into()],
                    },
                    GrantDefinition {
                        action: "service.restart".into(),
                        effect: PolicyEffect::Prompt,
                        resources: vec!["prompt".into()],
                    },
                    GrantDefinition {
                        action: "service.restart".into(),
                        effect: PolicyEffect::Reject,
                        resources: vec!["rejected".into()],
                    },
                ],
            }],
        )
        .unwrap()
    }

    #[test]
    fn allow_executes_but_prompt_and_reject_have_no_side_effect() {
        let executor = Arc::new(CountingExecutor::default());
        let core = PrivilegeCore::new(
            snapshot(),
            Arc::new(MemoryRepository::default()),
            executor.clone(),
        );
        let principal = Principal::new(1000, 1000);
        let allowed = core.dispatch_use(
            principal,
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"allowed"}),
            ),
        );
        assert!(allowed.ok);
        let prompt = core.dispatch_use(
            principal,
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"prompt"}),
            ),
        );
        assert_eq!(
            prompt.error.unwrap().code,
            SemanticErrorCode::ApprovalRequired
        );
        let rejected = core.dispatch_use(
            principal,
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"rejected"}),
            ),
        );
        assert_eq!(
            rejected.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        assert_eq!(executor.calls.load(AtomicOrdering::Relaxed), 1);
    }

    #[test]
    fn transport_eligibility_never_bypasses_policy_or_identity() {
        let executor = Arc::new(CountingExecutor::default());
        let core = PrivilegeCore::new(
            snapshot(),
            Arc::new(MemoryRepository::default()),
            executor.clone(),
        );
        let other = core.dispatch_use(
            Principal::new(2000, 2000),
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"allowed"}),
            ),
        );
        assert_eq!(
            other.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        let spoof = core.dispatch_use(
            Principal::new(2000, 2000),
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"allowed","uid":1000}),
            ),
        );
        assert_eq!(spoof.error.unwrap().code, SemanticErrorCode::InvalidRequest);
        assert_eq!(executor.calls.load(AtomicOrdering::Relaxed), 0);
    }

    #[test]
    fn non_root_admin_is_rejected_and_root_equivalent_requires_ack() {
        let repository = Arc::new(MemoryRepository::default());
        let core = PrivilegeCore::new(
            snapshot(),
            repository.clone(),
            Arc::new(CountingExecutor::default()),
        );
        let denied = core.dispatch_admin(Principal::new(1000,1000), RequestEnvelope::new("policy.admin.grant", json!({"uid":1000,"action":"root.shell","effect":"allow","resource":null,"ack_root_equivalent":true})));
        assert_eq!(
            denied.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        let no_ack = core.dispatch_admin(Principal::new(0,0), RequestEnvelope::new("policy.admin.grant", json!({"uid":1000,"action":"root.shell","effect":"allow","resource":null,"ack_root_equivalent":false})));
        assert_eq!(
            no_ack.error.unwrap().code,
            SemanticErrorCode::InvalidRequest
        );
        let accepted = core.dispatch_admin(Principal::new(0,0), RequestEnvelope::new("policy.admin.grant", json!({"uid":1000,"action":"root.shell","effect":"allow","resource":null,"ack_root_equivalent":true})));
        assert!(accepted.ok);
        assert_eq!(repository.commits.lock().unwrap().len(), 1);
    }

    #[test]
    fn privilege_and_admin_actions_are_audited_without_payloads() {
        let audit = Arc::new(CapturingAudit::default());
        let core = PrivilegeCore::new_with_audit(
            snapshot(),
            Arc::new(MemoryRepository::default()),
            Arc::new(CountingExecutor::default()),
            audit.clone(),
        );
        let allowed = core.dispatch_use(
            Principal::new(1000, 1000),
            RequestEnvelope::new(
                "privilege.execute",
                json!({"action":"service.restart","resource":"allowed"}),
            ),
        );
        assert!(allowed.ok);
        let denied_admin = core.dispatch_admin(
            Principal::new(1000, 1000),
            RequestEnvelope::new("policy.admin.show", json!({"uid":1000})),
        );
        assert_eq!(
            denied_admin.error.unwrap().code,
            SemanticErrorCode::PermissionDenied
        );
        let root_change = core.dispatch_admin(
            Principal::new(0, 0),
            RequestEnvelope::new(
                "policy.admin.grant",
                json!({"uid":1000,"action":"root.shell","effect":"allow","resource":null,"ack_root_equivalent":true}),
            ),
        );
        assert!(root_change.ok);
        let records = audit.records.lock().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].domain, AuditDomain::Privilege);
        assert_eq!(records[0].action, "service.restart");
        assert_eq!(records[1].result, AuditOutcome::Denied);
        assert_eq!(records[2].action, "policy.admin.grant");
        assert_eq!(records[2].reason_code, "root_equivalent_policy_change");
        let encoded = serde_json::to_value(&records[2]).unwrap();
        for forbidden in ["params", "payload", "argv", "environment", "command"] {
            assert!(encoded.get(forbidden).is_none());
        }
    }

    #[test]
    fn generic_root_primitives_do_not_exist() {
        let core = PrivilegeCore::new(
            snapshot(),
            Arc::new(MemoryRepository::default()),
            Arc::new(CountingExecutor::default()),
        );
        for method in [
            "exec",
            "shell",
            "run_as_root",
            "write_arbitrary_root_file",
            "install_arbitrary_local_package",
        ] {
            let response = core.dispatch_use(
                Principal::new(1000, 1000),
                RequestEnvelope::new(method, json!({})),
            );
            assert_eq!(response.error.unwrap().code, SemanticErrorCode::Unsupported);
        }
    }
}
