use crate::{
    CredentialStore, ProtectedApiAuthorizer, StoreError, UpstreamRequest, UpstreamTransport,
};
use portus_audit::{AuditSink, NullAuditSink};
use portus_protected_api::{
    AdminAction, AdminRequest, CredentialMetadata, DefinitionCatalog, ProviderError,
    ProviderErrorCode, ProviderResponse, ProviderSuccess, UseAction, UseRequest,
};
use portus_protocol::{
    AuditActor, AuditDomain, AuditRecord, AuditResult, PolicyEffect, Principal, RequestId,
};
use serde_json::Value;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub struct ProtectedApiCore {
    store: Mutex<CredentialStore>,
    definitions: DefinitionCatalog,
    authorizer: Arc<dyn ProtectedApiAuthorizer>,
    upstream: Arc<dyn UpstreamTransport>,
    audit: Arc<dyn AuditSink>,
    audit_failures: AtomicU64,
}

impl ProtectedApiCore {
    #[must_use]
    pub fn new(
        store: CredentialStore,
        definitions: DefinitionCatalog,
        authorizer: Arc<dyn ProtectedApiAuthorizer>,
        upstream: Arc<dyn UpstreamTransport>,
    ) -> Self {
        Self::new_with_audit(
            store,
            definitions,
            authorizer,
            upstream,
            Arc::new(NullAuditSink),
        )
    }

    #[must_use]
    pub fn new_with_audit(
        store: CredentialStore,
        definitions: DefinitionCatalog,
        authorizer: Arc<dyn ProtectedApiAuthorizer>,
        upstream: Arc<dyn UpstreamTransport>,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            store: Mutex::new(store),
            definitions,
            authorizer,
            upstream,
            audit,
            audit_failures: AtomicU64::new(0),
        }
    }

    #[must_use]
    pub fn audit_failures(&self) -> u64 {
        self.audit_failures.load(Ordering::Relaxed)
    }

    pub fn dispatch_use(&self, principal: Principal, request: UseRequest) -> ProviderResponse {
        let request_id = request.request_id;
        let action = match request.into_action() {
            Ok(action) => action,
            Err(error) => return ProviderResponse::failure(request_id, error),
        };
        let result = match action {
            UseAction::CredentialList => self
                .visible_credentials(principal)
                .map(|credentials| ProviderSuccess::CredentialList { credentials }),
            UseAction::CredentialShow { credential_ref } => self
                .visible_credential(principal, &credential_ref)
                .map(|credential| ProviderSuccess::CredentialShow { credential }),
            UseAction::Request {
                credential_ref,
                operation,
                payload,
            } => {
                let result = self.execute_request(
                    principal,
                    request_id,
                    &credential_ref,
                    &operation,
                    payload,
                );
                self.audit_use(principal, request_id, &credential_ref, &operation, &result);
                result
            }
            UseAction::Health => self.health(principal),
        };
        match result {
            Ok(value) => ProviderResponse::success(request_id, value),
            Err(error) => ProviderResponse::failure(request_id, error),
        }
    }

    pub fn dispatch_admin(&self, principal: Principal, request: AdminRequest) -> ProviderResponse {
        let request_id = request.request_id;
        let action = match request.into_action() {
            Ok(action) => action,
            Err(error) => return ProviderResponse::failure(request_id, error),
        };
        let audit_action = admin_action_name(&action);
        let audit_target = admin_action_target(&action);
        if principal.uid() != 0 {
            let error = ProviderError::new(
                ProviderErrorCode::PermissionDenied,
                "protected credential administration requires authenticated UID 0",
            );
            self.audit_admin(
                principal,
                request_id,
                audit_action,
                audit_target.as_deref(),
                &Err(error.clone()),
            );
            return ProviderResponse::failure(request_id, error);
        }
        let result = match action {
            AdminAction::CredentialProvision {
                credential_ref,
                provider_id,
                safe_label,
                secret,
            } => {
                if self.definitions.get(&provider_id).is_none() {
                    Err(ProviderError::new(
                        ProviderErrorCode::ProviderDefinitionInvalid,
                        "credential provider definition is unavailable",
                    ))
                } else {
                    self.require_audit_preflight(
                        principal,
                        request_id,
                        "credential.provision",
                        Some(&credential_ref),
                    )
                    .and_then(|()| {
                        let result = self
                            .store
                            .lock()
                            .map_err(|_| internal())?
                            .provision(
                                &credential_ref,
                                &provider_id,
                                safe_label.as_deref(),
                                &secret,
                            )
                            .map_err(store_error)?;
                        Ok(ProviderSuccess::CredentialMutation { credential: result })
                    })
                }
            }
            AdminAction::CredentialRotate {
                credential_ref,
                secret,
            } => self
                .require_audit_preflight(
                    principal,
                    request_id,
                    "credential.rotate",
                    Some(&credential_ref),
                )
                .and_then(|()| {
                    let result = self
                        .store
                        .lock()
                        .map_err(|_| internal())?
                        .rotate(&credential_ref, &secret)
                        .map_err(store_error)?;
                    Ok(ProviderSuccess::CredentialMutation { credential: result })
                }),
            AdminAction::CredentialRevoke { credential_ref } => self
                .require_audit_preflight(
                    principal,
                    request_id,
                    "credential.revoke",
                    Some(&credential_ref),
                )
                .and_then(|()| {
                    let result = self
                        .store
                        .lock()
                        .map_err(|_| internal())?
                        .revoke(&credential_ref)
                        .map_err(store_error)?;
                    Ok(ProviderSuccess::CredentialMutation { credential: result })
                }),
            AdminAction::CredentialDelete { credential_ref } => self
                .require_audit_preflight(
                    principal,
                    request_id,
                    "credential.delete",
                    Some(&credential_ref),
                )
                .and_then(|()| {
                    self.store
                        .lock()
                        .map_err(|_| internal())?
                        .delete(&credential_ref)
                        .map_err(store_error)?;
                    Ok(ProviderSuccess::CredentialDeleted { credential_ref })
                }),
            AdminAction::CredentialShow { credential_ref } => self
                .store
                .lock()
                .map_err(|_| internal())
                .and_then(|store| store.show(&credential_ref).map_err(store_error))
                .map(|credential| ProviderSuccess::CredentialShow { credential }),
            AdminAction::CredentialList => self
                .store
                .lock()
                .map_err(|_| internal())
                .and_then(|store| store.list().map_err(store_error))
                .map(|credentials| ProviderSuccess::CredentialList { credentials }),
        };
        self.audit_admin(
            principal,
            request_id,
            audit_action,
            audit_target.as_deref(),
            &result,
        );
        match result {
            Ok(value) => ProviderResponse::success(request_id, value),
            Err(error) => ProviderResponse::failure(request_id, error),
        }
    }

    fn execute_request(
        &self,
        principal: Principal,
        request_id: RequestId,
        credential_ref: &str,
        operation: &str,
        payload: Value,
    ) -> Result<ProviderSuccess, ProviderError> {
        match self
            .authorizer
            .effect(principal, credential_ref, operation)?
        {
            PolicyEffect::Reject => {
                return Err(ProviderError::new(
                    ProviderErrorCode::PermissionDenied,
                    "protected API policy rejects this operation",
                ));
            }
            PolicyEffect::Prompt => {
                return Err(ProviderError::new(
                    ProviderErrorCode::ApprovalRequired,
                    "protected API operation requires approval; no upstream request was sent",
                ));
            }
            PolicyEffect::Allow => {}
        }
        let metadata = self
            .store
            .lock()
            .map_err(|_| internal())?
            .show(credential_ref)
            .map_err(store_error)?;
        let definition = self.definitions.get(&metadata.provider_id).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "credential provider definition is unavailable",
            )
        })?;
        let operation_definition = definition.operation(operation)?;
        let payload_bytes = serde_json::to_vec(&payload).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::InvalidRequest,
                "provider request payload is not valid JSON",
            )
        })?;
        if payload_bytes.len() > definition.limits.max_request_bytes {
            return Err(ProviderError::new(
                ProviderErrorCode::RequestTooLarge,
                "protected request exceeds provider bound",
            ));
        }
        if operation_definition.streaming {
            return Err(ProviderError::new(
                ProviderErrorCode::OperationNotAllowed,
                "streaming is not supported by protocol v1",
            ));
        }
        self.require_audit_preflight(
            principal,
            request_id,
            "protected-api.request",
            Some(&format!("{credential_ref}|{operation}")),
        )?;
        let secret = self
            .store
            .lock()
            .map_err(|_| internal())?
            .load_secret(credential_ref)
            .map_err(store_error)?;
        self.upstream
            .execute(UpstreamRequest {
                definition,
                operation,
                body: payload_bytes,
                secret: &secret,
            })
            .and_then(|response| {
                if (300..400).contains(&response.status) {
                    return Err(ProviderError::new(
                        ProviderErrorCode::RedirectRejected,
                        "credential-bearing redirect was rejected",
                    )
                    .with_upstream_status(response.status));
                }
                if response.body.len() > definition.limits.max_response_bytes {
                    return Err(ProviderError::new(
                        ProviderErrorCode::ResponseTooLarge,
                        "protected upstream response exceeds configured bound",
                    ));
                }
                if !(200..300).contains(&response.status) {
                    return Err(ProviderError::new(
                        ProviderErrorCode::UpstreamError,
                        "protected upstream returned a non-success status",
                    )
                    .with_upstream_status(response.status));
                }
                let body: Value = serde_json::from_slice(&response.body).map_err(|_| {
                    ProviderError::new(
                        ProviderErrorCode::UpstreamError,
                        "protected upstream success body is not JSON",
                    )
                })?;
                Ok(ProviderSuccess::Request {
                    provider_id: metadata.provider_id.clone(),
                    operation: operation.into(),
                    upstream_status: response.status,
                    body,
                })
            })
    }

    fn visible_credentials(
        &self,
        principal: Principal,
    ) -> Result<Vec<CredentialMetadata>, ProviderError> {
        let all = self
            .store
            .lock()
            .map_err(|_| internal())?
            .list()
            .map_err(store_error)?;
        Ok(all
            .into_iter()
            .filter(|credential| self.can_discover(principal, credential))
            .collect())
    }

    fn visible_credential(
        &self,
        principal: Principal,
        credential_ref: &str,
    ) -> Result<CredentialMetadata, ProviderError> {
        let credential = self
            .store
            .lock()
            .map_err(|_| internal())?
            .show(credential_ref)
            .map_err(store_error)?;
        if self.can_discover(principal, &credential) {
            Ok(credential)
        } else {
            Err(ProviderError::new(
                ProviderErrorCode::CredentialNotFound,
                "credential reference is not visible to caller",
            ))
        }
    }

    fn can_discover(&self, principal: Principal, credential: &CredentialMetadata) -> bool {
        self.definitions
            .get(&credential.provider_id)
            .is_some_and(|definition| {
                definition.operations.keys().any(|operation| {
                    self.authorizer
                        .effect(principal, &credential.credential_ref, operation)
                        .is_ok_and(|effect| effect == PolicyEffect::Allow)
                })
            })
    }

    fn health(&self, principal: Principal) -> Result<ProviderSuccess, ProviderError> {
        let integrity = self
            .store
            .lock()
            .map_err(|_| internal())?
            .integrity_check()
            .map_err(store_error)?;
        let count = self.visible_credentials(principal)?.len();
        Ok(ProviderSuccess::Health {
            health: if integrity && !self.definitions.is_empty() {
                "healthy"
            } else {
                "degraded"
            }
            .into(),
            reason_code: if !integrity {
                "store_integrity_failed"
            } else if self.definitions.is_empty() {
                "no_provider_definitions"
            } else {
                "ready"
            }
            .into(),
            credential_count: count,
            provider_count: self.definitions.len(),
            audit_write_failures: self.audit_failures(),
        })
    }

    fn require_audit_preflight(
        &self,
        principal: Principal,
        request_id: RequestId,
        action: &str,
        target: Option<&str>,
    ) -> Result<(), ProviderError> {
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::ProtectedApi,
            format!("{action}.preflight"),
            AuditResult::Succeeded,
            "audit_ready",
            now_ms(),
        );
        record.request_id = Some(request_id);
        record.target_ref = target.map(ToOwned::to_owned);
        self.audit.record(&record).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::StoreUnavailable,
                "protected API audit sink is unavailable before security-sensitive operation",
            )
        })
    }

    fn audit_use(
        &self,
        principal: Principal,
        request_id: RequestId,
        credential_ref: &str,
        operation: &str,
        result: &Result<ProviderSuccess, ProviderError>,
    ) {
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::ProtectedApi,
            "protected-api.request",
            outcome(result),
            result
                .as_ref()
                .map_or_else(|error| error.code.as_str(), |_| "ok"),
            now_ms(),
        );
        record.request_id = Some(request_id);
        record.target_ref = Some(format!("{credential_ref}|{operation}"));
        self.record_audit(&record);
    }

    fn audit_admin(
        &self,
        principal: Principal,
        request_id: RequestId,
        action: &str,
        target: Option<&str>,
        result: &Result<ProviderSuccess, ProviderError>,
    ) {
        let mut record = AuditRecord::new(
            AuditActor::principal(principal),
            AuditDomain::ProtectedApi,
            action,
            outcome(result),
            result
                .as_ref()
                .map_or_else(|error| error.code.as_str(), |_| "ok"),
            now_ms(),
        );
        record.request_id = Some(request_id);
        record.target_ref = target.map(ToOwned::to_owned);
        self.record_audit(&record);
    }

    fn record_audit(&self, record: &AuditRecord) {
        if self.audit.record(record).is_err() {
            self.audit_failures.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn store_error(error: StoreError) -> ProviderError {
    match error {
        StoreError::NotFound => ProviderError::new(
            ProviderErrorCode::CredentialNotFound,
            "credential reference was not found",
        ),
        StoreError::Revoked => ProviderError::new(
            ProviderErrorCode::CredentialRevoked,
            "credential is revoked",
        ),
        StoreError::Invalid(_) => ProviderError::new(
            ProviderErrorCode::InvalidRequest,
            "credential mutation is invalid",
        ),
        StoreError::IncompatibleSchema(_) | StoreError::Sql(_) => ProviderError::new(
            ProviderErrorCode::StoreUnavailable,
            "protected credential store is unavailable",
        ),
    }
}
fn internal() -> ProviderError {
    ProviderError::new(
        ProviderErrorCode::Internal,
        "protected API service internal state is unavailable",
    )
}
fn outcome(result: &Result<ProviderSuccess, ProviderError>) -> AuditResult {
    match result {
        Ok(_) => AuditResult::Succeeded,
        Err(error) => match error.code {
            ProviderErrorCode::PermissionDenied => AuditResult::Denied,
            ProviderErrorCode::ApprovalRequired => AuditResult::ApprovalRequired,
            _ => AuditResult::Failed,
        },
    }
}
fn admin_action_name(action: &AdminAction) -> &'static str {
    match action {
        AdminAction::CredentialProvision { .. } => "credential.provision",
        AdminAction::CredentialRotate { .. } => "credential.rotate",
        AdminAction::CredentialRevoke { .. } => "credential.revoke",
        AdminAction::CredentialDelete { .. } => "credential.delete",
        AdminAction::CredentialShow { .. } => "credential.show",
        AdminAction::CredentialList => "credential.list",
    }
}
fn admin_action_target(action: &AdminAction) -> Option<String> {
    match action {
        AdminAction::CredentialProvision { credential_ref, .. }
        | AdminAction::CredentialRotate { credential_ref, .. }
        | AdminAction::CredentialRevoke { credential_ref }
        | AdminAction::CredentialDelete { credential_ref }
        | AdminAction::CredentialShow { credential_ref } => Some(credential_ref.clone()),
        AdminAction::CredentialList => None,
    }
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
    use crate::{UpstreamResponse, UpstreamTransport};
    use portus_audit::{AuditError, AuditResult as SinkResult};
    use portus_protected_api::{
        AuthenticationDefinition, DefinitionLimits, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
        MAX_TIMEOUT_MS, OperationDefinition, ProviderDefinition, SecretMaterial,
    };
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    struct FixedAuthorizer;
    impl ProtectedApiAuthorizer for FixedAuthorizer {
        fn effect(
            &self,
            principal: Principal,
            _credential_ref: &str,
            _operation: &str,
        ) -> Result<PolicyEffect, ProviderError> {
            Ok(match principal.uid() {
                1000 => PolicyEffect::Allow,
                1001 => PolicyEffect::Prompt,
                _ => PolicyEffect::Reject,
            })
        }
    }

    #[derive(Default)]
    struct RecordingUpstream {
        calls: AtomicUsize,
        expected_secret: Mutex<Vec<u8>>,
        status: Mutex<u16>,
    }
    impl UpstreamTransport for RecordingUpstream {
        fn execute(&self, request: UpstreamRequest<'_>) -> Result<UpstreamResponse, ProviderError> {
            self.calls.fetch_add(1, AtomicOrdering::Relaxed);
            assert_eq!(
                request
                    .definition
                    .operation_url(request.operation)
                    .unwrap()
                    .as_str(),
                "https://api.openai.com/v1/responses"
            );
            let expected = self.expected_secret.lock().unwrap();
            if !expected.is_empty() {
                assert_eq!(request.secret.as_bytes(), expected.as_slice());
            }
            Ok(UpstreamResponse {
                status: *self.status.lock().unwrap(),
                body: br#"{"id":"response_1","ok":true}"#.to_vec(),
            })
        }
    }

    struct FailingAudit;
    impl AuditSink for FailingAudit {
        fn record(&self, _record: &AuditRecord) -> SinkResult<()> {
            Err(AuditError::InvalidRecord("fixture failure"))
        }
    }

    fn definition() -> ProviderDefinition {
        ProviderDefinition {
            schema_version: 1,
            provider_id: "openai".into(),
            origin: "https://api.openai.com".into(),
            authentication: AuthenticationDefinition {
                kind: "bearer".into(),
                header: "Authorization".into(),
                prefix: "Bearer ".into(),
            },
            limits: DefinitionLimits {
                max_request_bytes: MAX_REQUEST_BYTES,
                max_response_bytes: MAX_RESPONSE_BYTES,
                timeout_ms: MAX_TIMEOUT_MS,
            },
            operations: BTreeMap::from([(
                "openai.responses.create".into(),
                OperationDefinition {
                    method: "POST".into(),
                    path: "/v1/responses".into(),
                    streaming: false,
                },
            )]),
        }
    }

    fn core(upstream: Arc<RecordingUpstream>) -> ProtectedApiCore {
        ProtectedApiCore::new(
            CredentialStore::open_in_memory().unwrap(),
            DefinitionCatalog::from_definitions(vec![definition()]).unwrap(),
            Arc::new(FixedAuthorizer),
            upstream,
        )
    }

    fn provision(core: &ProtectedApiCore, secret: &str) {
        let response = core.dispatch_admin(
            Principal::new(0, 0),
            AdminRequest::new(AdminAction::CredentialProvision {
                credential_ref: "openai/main".into(),
                provider_id: "openai".into(),
                safe_label: Some("Main OpenAI".into()),
                secret: SecretMaterial::new(secret.into()).unwrap(),
            }),
        );
        assert!(response.ok, "{response:?}");
    }

    #[test]
    fn use_succeeds_without_exporting_reusable_secret() {
        let upstream = Arc::new(RecordingUpstream::default());
        *upstream.expected_secret.lock().unwrap() = b"super-secret-key".to_vec();
        *upstream.status.lock().unwrap() = 200;
        let core = core(upstream.clone());
        provision(&core, "super-secret-key");
        let response = core.dispatch_use(
            Principal::new(1000, 1000),
            UseRequest::new(UseAction::Request {
                credential_ref: "openai/main".into(),
                operation: "openai.responses.create".into(),
                payload: serde_json::json!({"model":"test","input":"hello"}),
            }),
        );
        assert!(response.ok, "{response:?}");
        assert_eq!(upstream.calls.load(AtomicOrdering::Relaxed), 1);
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(!encoded.contains("super-secret-key"));
        assert!(!format!("{response:?}").contains("super-secret-key"));
    }

    #[test]
    fn cross_user_prompt_and_reject_never_reach_upstream() {
        let upstream = Arc::new(RecordingUpstream::default());
        *upstream.status.lock().unwrap() = 200;
        let core = core(upstream.clone());
        provision(&core, "secret");
        for (uid, code) in [
            (1001, ProviderErrorCode::ApprovalRequired),
            (2000, ProviderErrorCode::PermissionDenied),
        ] {
            let response = core.dispatch_use(
                Principal::new(uid, uid),
                UseRequest::new(UseAction::Request {
                    credential_ref: "openai/main".into(),
                    operation: "openai.responses.create".into(),
                    payload: serde_json::json!({}),
                }),
            );
            assert_eq!(response.error.unwrap().code, code);
        }
        let guessed_missing = core.dispatch_use(
            Principal::new(2000, 2000),
            UseRequest::new(UseAction::Request {
                credential_ref: "openai/does-not-exist".into(),
                operation: "openai.responses.create".into(),
                payload: serde_json::json!({}),
            }),
        );
        assert_eq!(
            guessed_missing.error.unwrap().code,
            ProviderErrorCode::PermissionDenied
        );
        assert_eq!(upstream.calls.load(AtomicOrdering::Relaxed), 0);
        let hidden = core.dispatch_use(
            Principal::new(2000, 2000),
            UseRequest::new(UseAction::CredentialShow {
                credential_ref: "openai/main".into(),
            }),
        );
        assert_eq!(
            hidden.error.unwrap().code,
            ProviderErrorCode::CredentialNotFound
        );
    }

    #[test]
    fn redirect_is_rejected_even_if_transport_returns_it() {
        let upstream = Arc::new(RecordingUpstream::default());
        *upstream.status.lock().unwrap() = 302;
        let core = core(upstream.clone());
        provision(&core, "secret");
        let response = core.dispatch_use(
            Principal::new(1000, 1000),
            UseRequest::new(UseAction::Request {
                credential_ref: "openai/main".into(),
                operation: "openai.responses.create".into(),
                payload: serde_json::json!({}),
            }),
        );
        assert_eq!(
            response.error.unwrap().code,
            ProviderErrorCode::RedirectRejected
        );
        assert_eq!(upstream.calls.load(AtomicOrdering::Relaxed), 1);
    }

    #[test]
    fn rotation_changes_generation_and_revocation_stops_use() {
        let upstream = Arc::new(RecordingUpstream::default());
        *upstream.status.lock().unwrap() = 200;
        let core = core(upstream.clone());
        provision(&core, "generation-one");
        let rotated = core.dispatch_admin(
            Principal::new(0, 0),
            AdminRequest::new(AdminAction::CredentialRotate {
                credential_ref: "openai/main".into(),
                secret: SecretMaterial::new("generation-two".into()).unwrap(),
            }),
        );
        assert_eq!(rotated.credential.as_ref().unwrap().generation, 2);
        let revoked = core.dispatch_admin(
            Principal::new(0, 0),
            AdminRequest::new(AdminAction::CredentialRevoke {
                credential_ref: "openai/main".into(),
            }),
        );
        assert!(revoked.ok);
        let response = core.dispatch_use(
            Principal::new(1000, 1000),
            UseRequest::new(UseAction::Request {
                credential_ref: "openai/main".into(),
                operation: "openai.responses.create".into(),
                payload: serde_json::json!({}),
            }),
        );
        assert_eq!(
            response.error.unwrap().code,
            ProviderErrorCode::CredentialRevoked
        );
        assert_eq!(upstream.calls.load(AtomicOrdering::Relaxed), 0);
    }

    #[test]
    fn audit_failure_prevents_secret_mutation_before_commit() {
        let upstream = Arc::new(RecordingUpstream::default());
        let core = ProtectedApiCore::new_with_audit(
            CredentialStore::open_in_memory().unwrap(),
            DefinitionCatalog::from_definitions(vec![definition()]).unwrap(),
            Arc::new(FixedAuthorizer),
            upstream,
            Arc::new(FailingAudit),
        );
        let response = core.dispatch_admin(
            Principal::new(0, 0),
            AdminRequest::new(AdminAction::CredentialProvision {
                credential_ref: "openai/main".into(),
                provider_id: "openai".into(),
                safe_label: None,
                secret: SecretMaterial::new("secret".into()).unwrap(),
            }),
        );
        assert_eq!(
            response.error.unwrap().code,
            ProviderErrorCode::StoreUnavailable
        );
        let listed = core.dispatch_admin(
            Principal::new(0, 0),
            AdminRequest::new(AdminAction::CredentialList),
        );
        assert!(listed.credentials.as_ref().unwrap().is_empty());
    }

    #[test]
    fn non_root_admin_and_guessed_export_actions_fail() {
        let upstream = Arc::new(RecordingUpstream::default());
        let core = core(upstream);
        let denied = core.dispatch_admin(
            Principal::new(1000, 1000),
            AdminRequest::new(AdminAction::CredentialList),
        );
        assert_eq!(
            denied.error.unwrap().code,
            ProviderErrorCode::PermissionDenied
        );
        for action in [
            "credential.export",
            "credential.get_raw",
            "credential.reveal",
        ] {
            let value = serde_json::json!({"protocol_version":1,"request_id":RequestId::new(),"action":action,"credential_ref":"openai/main"});
            let request: AdminRequest = serde_json::from_value(value).unwrap();
            assert!(request.validate().is_err());
        }
    }
}
