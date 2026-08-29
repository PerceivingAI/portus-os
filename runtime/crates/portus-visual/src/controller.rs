use crate::model::{
    has_structured_application_control, validate_generation, validate_keyboard, validate_pointer,
    validate_target_text,
};
use crate::{
    ACTION_VISUAL_CAPTURE, ACTION_VISUAL_KEYBOARD, ACTION_VISUAL_POINTER, CaptureRequest,
    CaptureResult, CaptureRetention, FallbackJustification, GraphicalTargetExpectation,
    GraphicalTargetKind, KeyboardRequest, PointerRequest, ScreenSensitivity, ScreenshotCapture,
    ValidatedGraphicalTarget, VisualBackend, VisualError, VisualOperationKind, VisualReceipt,
};
use portus_artifact::{FilesystemRegistrationRequest, prepare_filesystem_registration};
use portus_audit::AuditSink;
use portus_policy::{ActionDefinition, ActionRegistry, POLICY_VERSION, PolicySnapshot};
use portus_protocol::{
    ArtifactCleanupAuthority, ArtifactConfidentiality, ArtifactRetentionKind, ArtifactType,
    AuditActor, AuditDomain, AuditRecord, AuditResult, PolicyActionContext, PolicyEffect,
    PolicyEnforcementClass, Principal,
};
use std::fs;

pub struct VisualController;
impl VisualController {
    pub fn capture(
        backend: &mut dyn VisualBackend,
        policy: &PolicySnapshot,
        audit: &dyn AuditSink,
        principal: Principal,
        request: CaptureRequest,
        now_ms: i64,
    ) -> Result<CaptureResult, VisualError> {
        validate_common(
            &request.target,
            &request.available_control_paths,
            request.justification,
        )?;
        authorize(
            policy,
            audit,
            principal,
            VisualOperationKind::Capture,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        let target = revalidate_audited(
            backend,
            audit,
            principal,
            VisualOperationKind::Capture,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        require_normal_screen(&target)?;
        let capture = match backend.capture(principal, &target) {
            Ok(capture) => capture,
            Err(error) => {
                audit_failure(
                    audit,
                    principal,
                    VisualOperationKind::Capture,
                    &target.target_ref,
                    request.task_id,
                    now_ms,
                )
                .map_err(VisualError::AuditAfterSideEffect)?;
                return Err(error.into());
            }
        };
        let artifact_registration = match request.retention {
            CaptureRetention::Ephemeral => {
                validate_capture_file(&capture)?;
                None
            }
            CaptureRetention::TemporaryArtifact => {
                validate_capture_file(&capture)?;
                let mut artifact = FilesystemRegistrationRequest::retained(
                    principal,
                    &capture.path,
                    ArtifactType::Screenshot,
                );
                artifact.confidentiality = ArtifactConfidentiality::Private;
                artifact.retention_kind = ArtifactRetentionKind::Temporary;
                artifact.media_type = Some(capture.media_type.clone());
                artifact.source_task_id = Some(request.task_id);
                artifact.cleanup_authority = ArtifactCleanupAuthority::Task;
                artifact.cleanup_ref = Some(request.task_id.to_string());
                artifact.safe_display_name = Some("visual fallback screenshot".into());
                Some(prepare_filesystem_registration(artifact, now_ms)?)
            }
        };
        let receipt = VisualReceipt {
            operation: VisualOperationKind::Capture,
            target_ref: target.target_ref.clone(),
            source_generation: target.source_generation.clone(),
            task_id: request.task_id,
        };
        audit_success(
            audit,
            principal,
            VisualOperationKind::Capture,
            &target.target_ref,
            request.task_id,
            now_ms,
        )?;
        Ok(CaptureResult {
            receipt,
            capture,
            artifact_registration,
        })
    }

    pub fn pointer(
        backend: &mut dyn VisualBackend,
        policy: &PolicySnapshot,
        audit: &dyn AuditSink,
        principal: Principal,
        request: PointerRequest,
        now_ms: i64,
    ) -> Result<VisualReceipt, VisualError> {
        if request.target.kind != GraphicalTargetKind::Window {
            return Err(VisualError::InvalidTarget(
                "pointer fallback requires an X11 window target",
            ));
        }
        validate_common(
            &request.target,
            &request.available_control_paths,
            request.justification,
        )?;
        authorize(
            policy,
            audit,
            principal,
            VisualOperationKind::Pointer,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        let mut target = revalidate_audited(
            backend,
            audit,
            principal,
            VisualOperationKind::Pointer,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        require_normal_screen(&target)?;
        validate_pointer(request.action, target.geometry)?;
        if !target.focused {
            if let Err(error) = backend.focus_target(principal, &target) {
                audit_failure(
                    audit,
                    principal,
                    VisualOperationKind::Pointer,
                    &target.target_ref,
                    request.task_id,
                    now_ms,
                )
                .map_err(VisualError::AuditAfterSideEffect)?;
                return Err(error.into());
            }
            target = revalidate_audited(
                backend,
                audit,
                principal,
                VisualOperationKind::Pointer,
                &request.target,
                request.task_id,
                now_ms,
            )?;
            require_normal_screen(&target)?;
            if !target.focused {
                return Err(VisualError::TargetNotFocused);
            }
            validate_pointer(request.action, target.geometry)?;
        }
        if let Err(error) = backend.pointer(principal, &target, request.action) {
            audit_failure(
                audit,
                principal,
                VisualOperationKind::Pointer,
                &target.target_ref,
                request.task_id,
                now_ms,
            )
            .map_err(VisualError::AuditAfterSideEffect)?;
            return Err(error.into());
        }
        let receipt = VisualReceipt {
            operation: VisualOperationKind::Pointer,
            target_ref: target.target_ref.clone(),
            source_generation: target.source_generation.clone(),
            task_id: request.task_id,
        };
        audit_success(
            audit,
            principal,
            VisualOperationKind::Pointer,
            &target.target_ref,
            request.task_id,
            now_ms,
        )?;
        Ok(receipt)
    }

    pub fn keyboard(
        backend: &mut dyn VisualBackend,
        policy: &PolicySnapshot,
        audit: &dyn AuditSink,
        principal: Principal,
        request: KeyboardRequest,
        now_ms: i64,
    ) -> Result<VisualReceipt, VisualError> {
        if request.target.kind != GraphicalTargetKind::Window {
            return Err(VisualError::InvalidTarget(
                "keyboard fallback requires an X11 window target",
            ));
        }
        validate_keyboard(&request.action)?;
        validate_common(
            &request.target,
            &request.available_control_paths,
            request.justification,
        )?;
        authorize(
            policy,
            audit,
            principal,
            VisualOperationKind::Keyboard,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        let mut target = revalidate_audited(
            backend,
            audit,
            principal,
            VisualOperationKind::Keyboard,
            &request.target,
            request.task_id,
            now_ms,
        )?;
        require_normal_screen(&target)?;
        if !target.focused {
            if let Err(error) = backend.focus_target(principal, &target) {
                audit_failure(
                    audit,
                    principal,
                    VisualOperationKind::Keyboard,
                    &target.target_ref,
                    request.task_id,
                    now_ms,
                )
                .map_err(VisualError::AuditAfterSideEffect)?;
                return Err(error.into());
            }
            target = revalidate_audited(
                backend,
                audit,
                principal,
                VisualOperationKind::Keyboard,
                &request.target,
                request.task_id,
                now_ms,
            )?;
            require_normal_screen(&target)?;
            if !target.focused {
                return Err(VisualError::TargetNotFocused);
            }
        }
        if let Err(error) = backend.keyboard(principal, &target, &request.action) {
            audit_failure(
                audit,
                principal,
                VisualOperationKind::Keyboard,
                &target.target_ref,
                request.task_id,
                now_ms,
            )
            .map_err(VisualError::AuditAfterSideEffect)?;
            return Err(error.into());
        }
        let receipt = VisualReceipt {
            operation: VisualOperationKind::Keyboard,
            target_ref: target.target_ref.clone(),
            source_generation: target.source_generation.clone(),
            task_id: request.task_id,
        };
        audit_success(
            audit,
            principal,
            VisualOperationKind::Keyboard,
            &target.target_ref,
            request.task_id,
            now_ms,
        )?;
        Ok(receipt)
    }
}

#[must_use]
pub fn visual_action_registry() -> ActionRegistry {
    ActionRegistry {
        policy_version: POLICY_VERSION,
        actions: vec![
            ActionDefinition {
                id: ACTION_VISUAL_CAPTURE.into(),
                label: "Capture graphical screenshot".into(),
                class: PolicyEnforcementClass::UserNative,
                resource_kind: None,
                resource_required: false,
                root_equivalent: false,
            },
            ActionDefinition {
                id: ACTION_VISUAL_POINTER.into(),
                label: "Use graphical pointer fallback".into(),
                class: PolicyEnforcementClass::UserNative,
                resource_kind: None,
                resource_required: false,
                root_equivalent: false,
            },
            ActionDefinition {
                id: ACTION_VISUAL_KEYBOARD.into(),
                label: "Use graphical keyboard fallback".into(),
                class: PolicyEnforcementClass::UserNative,
                resource_kind: None,
                resource_required: false,
                root_equivalent: false,
            },
        ],
    }
}

fn validate_common(
    target: &GraphicalTargetExpectation,
    available_control_paths: &[portus_protocol::ControlPathKind],
    justification: FallbackJustification,
) -> Result<(), VisualError> {
    validate_target_text(&target.target_ref, "target reference")?;
    validate_generation(&target.source_generation)?;
    if justification == FallbackJustification::NoStructuredRoute
        && (has_structured_application_control(&target.observed_control_paths)
            || has_structured_application_control(available_control_paths))
    {
        return Err(VisualError::StructuredControlPreferred);
    }
    Ok(())
}

fn authorize(
    policy: &PolicySnapshot,
    audit: &dyn AuditSink,
    principal: Principal,
    operation: VisualOperationKind,
    target: &GraphicalTargetExpectation,
    task_id: portus_protocol::TaskId,
    now_ms: i64,
) -> Result<(), VisualError> {
    let decision = policy.evaluate(
        principal,
        &PolicyActionContext {
            action: operation.action_id().into(),
            resource: None,
        },
    )?;
    match decision.effect {
        PolicyEffect::Allow => Ok(()),
        PolicyEffect::Prompt => {
            audit_policy_stop(
                audit,
                principal,
                operation,
                target,
                task_id,
                PolicyStop::Prompt,
                now_ms,
            )?;
            Err(VisualError::ApprovalRequired)
        }
        PolicyEffect::Reject => {
            audit_policy_stop(
                audit,
                principal,
                operation,
                target,
                task_id,
                PolicyStop::Reject,
                now_ms,
            )?;
            Err(VisualError::PermissionDenied)
        }
    }
}

fn revalidate_audited(
    backend: &mut dyn VisualBackend,
    audit: &dyn AuditSink,
    principal: Principal,
    operation: VisualOperationKind,
    expected: &GraphicalTargetExpectation,
    task_id: portus_protocol::TaskId,
    now_ms: i64,
) -> Result<ValidatedGraphicalTarget, VisualError> {
    let current = match backend.revalidate_target(principal, expected) {
        Ok(current) => current,
        Err(error) => {
            audit_failure(
                audit,
                principal,
                operation,
                &expected.target_ref,
                task_id,
                now_ms,
            )?;
            return Err(error.into());
        }
    };
    if current.kind != expected.kind
        || current.target_ref != expected.target_ref
        || current.source_generation != expected.source_generation
    {
        audit_failure(
            audit,
            principal,
            operation,
            &expected.target_ref,
            task_id,
            now_ms,
        )?;
        return Err(VisualError::StaleTarget);
    }
    Ok(current)
}

fn require_normal_screen(target: &ValidatedGraphicalTarget) -> Result<(), VisualError> {
    if target.sensitivity == ScreenSensitivity::Normal {
        Ok(())
    } else {
        Err(VisualError::SensitiveTarget)
    }
}

fn validate_capture_file(capture: &ScreenshotCapture) -> Result<(), VisualError> {
    if !capture.path.is_absolute() {
        return Err(VisualError::InvalidAction(
            "screenshot backend must return an absolute path",
        ));
    }
    if !matches!(capture.media_type.as_str(), "image/png" | "image/jpeg") {
        return Err(VisualError::InvalidAction(
            "screenshot backend returned unsupported media type",
        ));
    }
    let metadata = fs::metadata(&capture.path)
        .map_err(|_| VisualError::InvalidAction("screenshot file is unavailable"))?;
    if !metadata.is_file() || metadata.len() > crate::model::MAX_SCREENSHOT_BYTES {
        return Err(VisualError::InvalidAction(
            "screenshot file is not regular or exceeds the bounded size",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum PolicyStop {
    Prompt,
    Reject,
}

fn audit_policy_stop(
    audit: &dyn AuditSink,
    principal: Principal,
    operation: VisualOperationKind,
    target: &GraphicalTargetExpectation,
    task_id: portus_protocol::TaskId,
    stop: PolicyStop,
    now_ms: i64,
) -> Result<(), VisualError> {
    let (result, reason) = match stop {
        PolicyStop::Prompt => (AuditResult::ApprovalRequired, "policy_prompt"),
        PolicyStop::Reject => (AuditResult::Denied, "policy_reject"),
    };
    let mut record = AuditRecord::new(
        AuditActor::principal(principal),
        AuditDomain::Visual,
        operation.action_id(),
        result,
        reason,
        now_ms,
    );
    record.target_ref = Some(target.target_ref.clone());
    record.task_id = Some(task_id);
    audit.record(&record)?;
    Ok(())
}

fn audit_failure(
    audit: &dyn AuditSink,
    principal: Principal,
    operation: VisualOperationKind,
    target_ref: &str,
    task_id: portus_protocol::TaskId,
    now_ms: i64,
) -> Result<(), portus_audit::AuditError> {
    let mut record = AuditRecord::new(
        AuditActor::principal(principal),
        AuditDomain::Visual,
        operation.action_id(),
        AuditResult::Failed,
        "operation_failed",
        now_ms,
    );
    record.target_ref = Some(target_ref.to_string());
    record.task_id = Some(task_id);
    audit.record(&record)
}

fn audit_success(
    audit: &dyn AuditSink,
    principal: Principal,
    operation: VisualOperationKind,
    target_ref: &str,
    task_id: portus_protocol::TaskId,
    now_ms: i64,
) -> Result<(), VisualError> {
    let mut record = AuditRecord::new(
        AuditActor::principal(principal),
        AuditDomain::Visual,
        operation.action_id(),
        AuditResult::Succeeded,
        "completed",
        now_ms,
    );
    record.target_ref = Some(target_ref.to_string());
    record.task_id = Some(task_id);
    audit
        .record(&record)
        .map_err(VisualError::AuditAfterSideEffect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActiveGraphicalContext, GraphicalTargetKind, KeyboardAction, PointerAction, PointerButton,
        ScreenSensitivity, TargetGeometry, VisualBackendError, VisualBackendFailure,
    };
    use portus_audit::{AuditError, AuditSink, NullAuditSink};
    use portus_policy::{
        BundleDefinition, BundleSelection, GlobalPolicy, GrantDefinition, SubjectPolicy,
    };
    use portus_protocol::{ControlPathKind, PolicyEffect, TaskId};
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct FakeBackend {
        targets: Vec<ValidatedGraphicalTarget>,
        capture: Option<ScreenshotCapture>,
        revalidate_failure: Option<VisualBackendFailure>,
        focus_calls: usize,
        capture_calls: usize,
        pointer_calls: usize,
        keyboard_calls: usize,
    }

    impl FakeBackend {
        fn normal_window(focused: bool) -> ValidatedGraphicalTarget {
            ValidatedGraphicalTarget {
                kind: GraphicalTargetKind::Window,
                target_ref: "window:g1:42".into(),
                source_generation: "g1".into(),
                geometry: Some(TargetGeometry {
                    x: 100,
                    y: 100,
                    width: 800,
                    height: 600,
                }),
                focused,
                sensitivity: ScreenSensitivity::Normal,
            }
        }
    }

    impl VisualBackend for FakeBackend {
        fn active_context(
            &mut self,
            _principal: Principal,
        ) -> Result<ActiveGraphicalContext, VisualBackendError> {
            Ok(ActiveGraphicalContext {
                source_generation: "g1".into(),
                display_ref: "display:g1:Virtual-1".into(),
                focused_window_ref: Some("window:g1:42".into()),
            })
        }

        fn revalidate_target(
            &mut self,
            _principal: Principal,
            _expected: &GraphicalTargetExpectation,
        ) -> Result<ValidatedGraphicalTarget, VisualBackendError> {
            if let Some(reason) = self.revalidate_failure {
                return Err(VisualBackendError::new(reason));
            }
            if self.targets.is_empty() {
                return Err(VisualBackendError::new(
                    VisualBackendFailure::TargetUnavailable,
                ));
            }
            Ok(self.targets.remove(0))
        }

        fn focus_target(
            &mut self,
            _principal: Principal,
            _target: &ValidatedGraphicalTarget,
        ) -> Result<(), VisualBackendError> {
            self.focus_calls += 1;
            Ok(())
        }

        fn capture(
            &mut self,
            _principal: Principal,
            _target: &ValidatedGraphicalTarget,
        ) -> Result<ScreenshotCapture, VisualBackendError> {
            self.capture_calls += 1;
            self.capture
                .clone()
                .ok_or_else(|| VisualBackendError::new(VisualBackendFailure::CaptureFailed))
        }

        fn pointer(
            &mut self,
            _principal: Principal,
            _target: &ValidatedGraphicalTarget,
            _action: PointerAction,
        ) -> Result<(), VisualBackendError> {
            self.pointer_calls += 1;
            Ok(())
        }

        fn keyboard(
            &mut self,
            _principal: Principal,
            _target: &ValidatedGraphicalTarget,
            _action: &KeyboardAction,
        ) -> Result<(), VisualBackendError> {
            self.keyboard_calls += 1;
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingAudit {
        records: Arc<Mutex<Vec<AuditRecord>>>,
        fail: bool,
    }

    impl AuditSink for RecordingAudit {
        fn record(&self, record: &AuditRecord) -> Result<(), AuditError> {
            if self.fail {
                return Err(AuditError::InvalidRecord("fixture failure"));
            }
            self.records
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(record.clone());
            Ok(())
        }
    }

    fn policy(effect: PolicyEffect) -> PolicySnapshot {
        let actions = visual_action_registry();
        PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            actions,
            vec![BundleDefinition {
                policy_version: 1,
                id: "visual".into(),
                label: "Visual fallback".into(),
                broad_default: true,
                grants: vec![
                    GrantDefinition {
                        action: ACTION_VISUAL_CAPTURE.into(),
                        effect,
                        resources: vec![],
                    },
                    GrantDefinition {
                        action: ACTION_VISUAL_POINTER.into(),
                        effect,
                        resources: vec![],
                    },
                    GrantDefinition {
                        action: ACTION_VISUAL_KEYBOARD.into(),
                        effect,
                        resources: vec![],
                    },
                ],
            }],
            vec![SubjectPolicy {
                policy_version: 1,
                uid: 1000,
                label: Some("master".into()),
                bundles: vec![BundleSelection {
                    id: "visual".into(),
                    enabled: true,
                }],
                grants: vec![],
            }],
        )
        .unwrap()
    }

    fn target() -> GraphicalTargetExpectation {
        GraphicalTargetExpectation {
            kind: GraphicalTargetKind::Window,
            target_ref: "window:g1:42".into(),
            source_generation: "g1".into(),
            observed_control_paths: vec![ControlPathKind::VisualFallback],
        }
    }

    fn screenshot_file() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("portus-visual-{}", TaskId::new()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("shot.png");
        fs::write(&file, b"not-real-png-but-bounded-fixture").unwrap();
        (dir, file)
    }

    #[test]
    fn policy_registry_is_action_scoped_and_non_root_equivalent() {
        let registry = visual_action_registry();
        assert_eq!(registry.actions.len(), 3);
        for action in registry.actions {
            assert!(!action.resource_required);
            assert!(action.resource_kind.is_none());
            assert!(!action.root_equivalent);
            assert_eq!(action.class, PolicyEnforcementClass::UserNative);
        }
    }

    #[test]
    fn reject_and_prompt_have_no_visual_side_effect() {
        for (effect, expected_prompt) in
            [(PolicyEffect::Reject, false), (PolicyEffect::Prompt, true)]
        {
            let mut backend = FakeBackend::default();
            let request = PointerRequest {
                task_id: TaskId::new(),
                target: target(),
                available_control_paths: vec![],
                justification: FallbackJustification::NoStructuredRoute,
                action: PointerAction::ClickAt {
                    x: 10,
                    y: 10,
                    button: PointerButton::Left,
                },
            };
            let result = VisualController::pointer(
                &mut backend,
                &policy(effect),
                &NullAuditSink,
                Principal::new(1000, 1000),
                request,
                1,
            );
            assert!(
                matches!(
                    result,
                    Err(VisualError::ApprovalRequired) if expected_prompt
                ) || matches!(result, Err(VisualError::PermissionDenied) if !expected_prompt)
            );
            assert_eq!(backend.pointer_calls, 0);
            assert_eq!(backend.focus_calls, 0);
        }
    }

    #[test]
    fn unfailed_structured_route_prevents_fallback_but_failed_route_allows_it() {
        let principal = Principal::new(1000, 1000);
        let request = PointerRequest {
            task_id: TaskId::new(),
            target: target(),
            available_control_paths: vec![ControlPathKind::RegisteredProvider],
            justification: FallbackJustification::NoStructuredRoute,
            action: PointerAction::ClickAt {
                x: 10,
                y: 10,
                button: PointerButton::Left,
            },
        };
        let mut backend = FakeBackend::default();
        assert!(matches!(
            VisualController::pointer(
                &mut backend,
                &policy(PolicyEffect::Allow),
                &NullAuditSink,
                principal,
                request,
                1,
            ),
            Err(VisualError::StructuredControlPreferred)
        ));
        assert_eq!(backend.pointer_calls, 0);

        let mut backend = FakeBackend {
            targets: vec![FakeBackend::normal_window(true)],
            ..FakeBackend::default()
        };
        let request = PointerRequest {
            task_id: TaskId::new(),
            target: target(),
            available_control_paths: vec![ControlPathKind::RegisteredProvider],
            justification: FallbackJustification::StructuredRouteFailed,
            action: PointerAction::ClickAt {
                x: 10,
                y: 10,
                button: PointerButton::Left,
            },
        };
        VisualController::pointer(
            &mut backend,
            &policy(PolicyEffect::Allow),
            &NullAuditSink,
            principal,
            request,
            1,
        )
        .unwrap();
        assert_eq!(backend.pointer_calls, 1);
    }

    #[test]
    fn unavailable_graphical_source_is_explicit_and_has_no_side_effect() {
        let mut backend = FakeBackend {
            revalidate_failure: Some(VisualBackendFailure::SourceUnavailable),
            ..FakeBackend::default()
        };
        let task_id = TaskId::new();
        let request = PointerRequest {
            task_id,
            target: target(),
            available_control_paths: vec![],
            justification: FallbackJustification::NoStructuredRoute,
            action: PointerAction::ClickAt {
                x: 10,
                y: 10,
                button: PointerButton::Left,
            },
        };
        let audit = RecordingAudit::default();
        assert!(matches!(
            VisualController::pointer(
                &mut backend,
                &policy(PolicyEffect::Allow),
                &audit,
                Principal::new(1000, 1000),
                request,
                1,
            ),
            Err(VisualError::Backend(VisualBackendError {
                reason: VisualBackendFailure::SourceUnavailable
            }))
        ));
        assert_eq!(backend.pointer_calls, 0);
        assert_eq!(backend.focus_calls, 0);
        let records = audit
            .records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, AuditResult::Failed);
        assert_eq!(records[0].task_id, Some(task_id));
    }

    #[test]
    fn changed_generation_fails_before_input() {
        let mut changed = FakeBackend::normal_window(true);
        changed.source_generation = "g2".into();
        changed.target_ref = "window:g2:42".into();
        let mut backend = FakeBackend {
            targets: vec![changed],
            ..FakeBackend::default()
        };
        let request = KeyboardRequest {
            task_id: TaskId::new(),
            target: target(),
            available_control_paths: vec![],
            justification: FallbackJustification::NoStructuredRoute,
            action: KeyboardAction::Text("hello".into()),
        };
        assert!(matches!(
            VisualController::keyboard(
                &mut backend,
                &policy(PolicyEffect::Allow),
                &NullAuditSink,
                Principal::new(1000, 1000),
                request,
                1,
            ),
            Err(VisualError::StaleTarget)
        ));
        assert_eq!(backend.keyboard_calls, 0);
    }

    #[test]
    fn unfocused_target_is_focused_then_revalidated_before_keyboard() {
        let mut backend = FakeBackend {
            targets: vec![
                FakeBackend::normal_window(false),
                FakeBackend::normal_window(true),
            ],
            ..FakeBackend::default()
        };
        let request = KeyboardRequest {
            task_id: TaskId::new(),
            target: target(),
            available_control_paths: vec![],
            justification: FallbackJustification::NoStructuredRoute,
            action: KeyboardAction::KeyChord(vec!["ctrl".into(), "l".into()]),
        };
        VisualController::keyboard(
            &mut backend,
            &policy(PolicyEffect::Allow),
            &NullAuditSink,
            Principal::new(1000, 1000),
            request,
            1,
        )
        .unwrap();
        assert_eq!(backend.focus_calls, 1);
        assert_eq!(backend.keyboard_calls, 1);
    }

    #[test]
    fn sensitive_or_unknown_screen_blocks_automated_visual_action() {
        for sensitivity in [ScreenSensitivity::Sensitive, ScreenSensitivity::Unknown] {
            let mut current = FakeBackend::normal_window(true);
            current.sensitivity = sensitivity;
            let mut backend = FakeBackend {
                targets: vec![current],
                ..FakeBackend::default()
            };
            let request = PointerRequest {
                task_id: TaskId::new(),
                target: target(),
                available_control_paths: vec![],
                justification: FallbackJustification::NoStructuredRoute,
                action: PointerAction::ClickAt {
                    x: 10,
                    y: 10,
                    button: PointerButton::Left,
                },
            };
            assert!(matches!(
                VisualController::pointer(
                    &mut backend,
                    &policy(PolicyEffect::Allow),
                    &NullAuditSink,
                    Principal::new(1000, 1000),
                    request,
                    1,
                ),
                Err(VisualError::SensitiveTarget)
            ));
            assert_eq!(backend.pointer_calls, 0);
        }
    }

    #[test]
    fn pointer_coordinates_are_target_relative_and_bounded() {
        let mut backend = FakeBackend {
            targets: vec![FakeBackend::normal_window(true)],
            ..FakeBackend::default()
        };
        let request = PointerRequest {
            task_id: TaskId::new(),
            target: target(),
            available_control_paths: vec![],
            justification: FallbackJustification::NoStructuredRoute,
            action: PointerAction::ClickAt {
                x: 900,
                y: 10,
                button: PointerButton::Left,
            },
        };
        assert!(matches!(
            VisualController::pointer(
                &mut backend,
                &policy(PolicyEffect::Allow),
                &NullAuditSink,
                Principal::new(1000, 1000),
                request,
                1,
            ),
            Err(VisualError::InvalidAction(_))
        ));
        assert_eq!(backend.pointer_calls, 0);
    }

    #[test]
    fn retained_screenshot_is_private_temporary_task_artifact_without_image_bytes() {
        let (dir, file) = screenshot_file();
        let task_id = TaskId::new();
        let mut backend = FakeBackend {
            targets: vec![FakeBackend::normal_window(true)],
            capture: Some(ScreenshotCapture {
                path: file.clone(),
                media_type: "image/png".into(),
            }),
            ..FakeBackend::default()
        };
        let result = VisualController::capture(
            &mut backend,
            &policy(PolicyEffect::Allow),
            &NullAuditSink,
            Principal::new(1000, 1000),
            CaptureRequest {
                task_id,
                target: target(),
                available_control_paths: vec![],
                justification: FallbackJustification::NoStructuredRoute,
                retention: CaptureRetention::TemporaryArtifact,
            },
            1,
        )
        .unwrap();
        let spec = result.artifact_registration.unwrap();
        assert_eq!(spec.artifact_type, ArtifactType::Screenshot);
        assert_eq!(spec.confidentiality, ArtifactConfidentiality::Private);
        assert_eq!(spec.retention_kind, ArtifactRetentionKind::Temporary);
        assert_eq!(spec.source_task_id, Some(task_id));
        assert_eq!(spec.cleanup_authority, ArtifactCleanupAuthority::Task);
        assert_eq!(spec.cleanup_ref, Some(task_id.to_string()));
        assert!(spec.sha256.is_some());
        assert_eq!(spec.size_bytes, Some(fs::metadata(&file).unwrap().len()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn successful_actions_are_audited_without_input_payload() {
        let audit = RecordingAudit::default();
        let mut backend = FakeBackend {
            targets: vec![FakeBackend::normal_window(true)],
            ..FakeBackend::default()
        };
        let task_id = TaskId::new();
        VisualController::keyboard(
            &mut backend,
            &policy(PolicyEffect::Allow),
            &audit,
            Principal::new(1000, 1000),
            KeyboardRequest {
                task_id,
                target: target(),
                available_control_paths: vec![],
                justification: FallbackJustification::NoStructuredRoute,
                action: KeyboardAction::Text("highly private text".into()),
            },
            55,
        )
        .unwrap();
        let records = audit
            .records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].domain, AuditDomain::Visual);
        assert_eq!(records[0].action, ACTION_VISUAL_KEYBOARD);
        assert_eq!(records[0].task_id, Some(task_id));
        let encoded = serde_json::to_string(&records[0]).unwrap();
        assert!(!encoded.contains("highly private text"));
    }

    #[test]
    fn audit_failure_after_input_is_explicitly_non_retry_safe() {
        let audit = RecordingAudit {
            fail: true,
            ..RecordingAudit::default()
        };
        let mut backend = FakeBackend {
            targets: vec![FakeBackend::normal_window(true)],
            ..FakeBackend::default()
        };
        let result = VisualController::pointer(
            &mut backend,
            &policy(PolicyEffect::Allow),
            &audit,
            Principal::new(1000, 1000),
            PointerRequest {
                task_id: TaskId::new(),
                target: target(),
                available_control_paths: vec![],
                justification: FallbackJustification::NoStructuredRoute,
                action: PointerAction::ClickAt {
                    x: 10,
                    y: 10,
                    button: PointerButton::Left,
                },
            },
            1,
        );
        assert!(matches!(result, Err(VisualError::AuditAfterSideEffect(_))));
        assert_eq!(backend.pointer_calls, 1);
    }
}
