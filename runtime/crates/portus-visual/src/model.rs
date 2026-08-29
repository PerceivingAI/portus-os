use portus_artifact::ArtifactError;
use portus_audit::AuditError;
use portus_policy::PolicyError;
use portus_protocol::{
    ArtifactRegistrationSpec, ControlPathKind, Freshness, IndexObservation, IndexResourceType,
    IndexSourceKind, Principal, TaskId,
};
use std::{error::Error, fmt, path::PathBuf};

pub const ACTION_VISUAL_CAPTURE: &str = "visual.capture";
pub const ACTION_VISUAL_POINTER: &str = "visual.pointer";
pub const ACTION_VISUAL_KEYBOARD: &str = "visual.keyboard";

pub const MAX_TARGET_REF_BYTES: usize = 512;
pub const MAX_GENERATION_BYTES: usize = 512;
pub const MAX_KEY_NAME_BYTES: usize = 64;
pub const MAX_KEY_CHORD_KEYS: usize = 8;
pub const MAX_TEXT_INPUT_BYTES: usize = 4096;
pub const MAX_SCREENSHOT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualOperationKind {
    Capture,
    Pointer,
    Keyboard,
}

impl VisualOperationKind {
    #[must_use]
    pub const fn action_id(self) -> &'static str {
        match self {
            Self::Capture => ACTION_VISUAL_CAPTURE,
            Self::Pointer => ACTION_VISUAL_POINTER,
            Self::Keyboard => ACTION_VISUAL_KEYBOARD,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicalTargetKind {
    Display,
    Window,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenSensitivity {
    Normal,
    Sensitive,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FallbackJustification {
    NoStructuredRoute,
    StructuredRouteFailed,
    UserRequestedVisual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerAction {
    Move {
        x: u32,
        y: u32,
    },
    ClickAt {
        x: u32,
        y: u32,
        button: PointerButton,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyboardAction {
    Text(String),
    KeyChord(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicalTargetExpectation {
    pub kind: GraphicalTargetKind,
    pub target_ref: String,
    pub source_generation: String,
    pub observed_control_paths: Vec<ControlPathKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedGraphicalTarget {
    pub kind: GraphicalTargetKind,
    pub target_ref: String,
    pub source_generation: String,
    pub geometry: Option<TargetGeometry>,
    pub focused: bool,
    pub sensitivity: ScreenSensitivity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveGraphicalContext {
    pub source_generation: String,
    pub display_ref: String,
    pub focused_window_ref: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreenshotCapture {
    pub path: PathBuf,
    pub media_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureRetention {
    Ephemeral,
    TemporaryArtifact,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureRequest {
    pub task_id: TaskId,
    pub target: GraphicalTargetExpectation,
    pub available_control_paths: Vec<ControlPathKind>,
    pub justification: FallbackJustification,
    pub retention: CaptureRetention,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointerRequest {
    pub task_id: TaskId,
    pub target: GraphicalTargetExpectation,
    pub available_control_paths: Vec<ControlPathKind>,
    pub justification: FallbackJustification,
    pub action: PointerAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardRequest {
    pub task_id: TaskId,
    pub target: GraphicalTargetExpectation,
    pub available_control_paths: Vec<ControlPathKind>,
    pub justification: FallbackJustification,
    pub action: KeyboardAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualReceipt {
    pub operation: VisualOperationKind,
    pub target_ref: String,
    pub source_generation: String,
    pub task_id: TaskId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureResult {
    pub receipt: VisualReceipt,
    pub capture: ScreenshotCapture,
    pub artifact_registration: Option<ArtifactRegistrationSpec>,
}

#[derive(Debug)]
pub enum VisualError {
    InvalidTarget(&'static str),
    InvalidAction(&'static str),
    StructuredControlPreferred,
    PermissionDenied,
    ApprovalRequired,
    StaleTarget,
    TargetNotFocused,
    SensitiveTarget,
    Backend(super::VisualBackendError),
    Policy(PolicyError),
    Artifact(ArtifactError),
    Audit(AuditError),
    AuditAfterSideEffect(AuditError),
}

impl fmt::Display for VisualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget(message) => write!(f, "invalid graphical target: {message}"),
            Self::InvalidAction(message) => write!(f, "invalid visual action: {message}"),
            Self::StructuredControlPreferred => {
                f.write_str("structured control is available and visual fallback was not justified")
            }
            Self::PermissionDenied => f.write_str("visual operation is denied by policy"),
            Self::ApprovalRequired => f.write_str("visual operation requires approval"),
            Self::StaleTarget => f.write_str("graphical target generation no longer matches"),
            Self::TargetNotFocused => {
                f.write_str("graphical input target could not be focused safely")
            }
            Self::SensitiveTarget => f.write_str(
                "automated visual operation is blocked on a sensitive or unclassified screen",
            ),
            Self::Backend(error) => write!(f, "visual backend error: {error}"),
            Self::Policy(error) => write!(f, "visual policy error: {error}"),
            Self::Artifact(error) => write!(f, "visual artifact error: {error}"),
            Self::Audit(error) => write!(f, "visual audit error: {error}"),
            Self::AuditAfterSideEffect(error) => write!(
                f,
                "visual operation may have completed but audit recording failed: {error}"
            ),
        }
    }
}

impl Error for VisualError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            Self::Policy(error) => Some(error),
            Self::Artifact(error) => Some(error),
            Self::Audit(error) | Self::AuditAfterSideEffect(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PolicyError> for VisualError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}

impl From<ArtifactError> for VisualError {
    fn from(value: ArtifactError) -> Self {
        Self::Artifact(value)
    }
}

impl From<AuditError> for VisualError {
    fn from(value: AuditError) -> Self {
        Self::Audit(value)
    }
}

impl From<super::VisualBackendError> for VisualError {
    fn from(value: super::VisualBackendError) -> Self {
        Self::Backend(value)
    }
}

pub fn expectation_from_index(
    observation: &IndexObservation,
    principal: Principal,
) -> Result<GraphicalTargetExpectation, VisualError> {
    if observation.owner != Some(principal) {
        return Err(VisualError::InvalidTarget(
            "target is not owned by the authenticated principal",
        ));
    }
    if !matches!(observation.freshness, Freshness::Live | Freshness::Recent) {
        return Err(VisualError::StaleTarget);
    }
    let kind = match (observation.resource_type, observation.source_kind) {
        (IndexResourceType::Window, IndexSourceKind::X11) => GraphicalTargetKind::Window,
        (IndexResourceType::Display, IndexSourceKind::I3) => GraphicalTargetKind::Display,
        _ => {
            return Err(VisualError::InvalidTarget(
                "only X11 windows and i3 displays are visual targets",
            ));
        }
    };
    let target_ref = observation
        .authoritative_ref
        .clone()
        .ok_or(VisualError::InvalidTarget(
            "target lacks authoritative generation-scoped identity",
        ))?;
    validate_target_text(&target_ref, "target reference")?;
    validate_generation(&observation.source_generation)?;
    if kind == GraphicalTargetKind::Window
        && !observation
            .control_paths
            .contains(&ControlPathKind::VisualFallback)
    {
        return Err(VisualError::InvalidTarget(
            "window does not advertise visual-fallback control",
        ));
    }
    Ok(GraphicalTargetExpectation {
        kind,
        target_ref,
        source_generation: observation.source_generation.clone(),
        observed_control_paths: observation.control_paths.clone(),
    })
}

pub(crate) fn validate_target_text(value: &str, field: &'static str) -> Result<(), VisualError> {
    if value.trim().is_empty()
        || value.len() > MAX_TARGET_REF_BYTES
        || value.contains(['\0', '\n', '\r'])
    {
        return Err(VisualError::InvalidTarget(field));
    }
    Ok(())
}

pub(crate) fn validate_generation(value: &str) -> Result<(), VisualError> {
    if value.trim().is_empty()
        || value.len() > MAX_GENERATION_BYTES
        || value.contains(['\0', '\n', '\r'])
    {
        return Err(VisualError::InvalidTarget("source generation"));
    }
    Ok(())
}

pub(crate) fn validate_keyboard(action: &KeyboardAction) -> Result<(), VisualError> {
    match action {
        KeyboardAction::Text(value) => {
            if value.is_empty() || value.len() > MAX_TEXT_INPUT_BYTES || value.contains('\0') {
                return Err(VisualError::InvalidAction(
                    "text input must be non-empty, bounded, and NUL-free",
                ));
            }
        }
        KeyboardAction::KeyChord(keys) => {
            if keys.is_empty() || keys.len() > MAX_KEY_CHORD_KEYS {
                return Err(VisualError::InvalidAction(
                    "key chord has invalid key count",
                ));
            }
            for key in keys {
                if key.is_empty()
                    || key.len() > MAX_KEY_NAME_BYTES
                    || !key.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.')
                    })
                {
                    return Err(VisualError::InvalidAction("key name is invalid"));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_pointer(
    action: PointerAction,
    geometry: Option<TargetGeometry>,
) -> Result<(), VisualError> {
    let Some(geometry) = geometry else {
        return Err(VisualError::InvalidTarget(
            "pointer input requires current target geometry",
        ));
    };
    if geometry.width == 0 || geometry.height == 0 {
        return Err(VisualError::InvalidTarget("target geometry is empty"));
    }
    let validate = |x: u32, y: u32| {
        if x >= geometry.width || y >= geometry.height {
            Err(VisualError::InvalidAction(
                "pointer coordinates fall outside the revalidated target",
            ))
        } else {
            Ok(())
        }
    };
    match action {
        PointerAction::Move { x, y } | PointerAction::ClickAt { x, y, .. } => validate(x, y),
    }
}
pub(crate) fn has_structured_application_control(paths: &[ControlPathKind]) -> bool {
    paths.iter().any(|path| {
        matches!(
            path,
            ControlPathKind::RegisteredProvider
                | ControlPathKind::StructuredApi
                | ControlPathKind::StructuredCli
                | ControlPathKind::ApplicationAdapter
                | ControlPathKind::Accessibility
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{IndexHandle, IndexObservation};
    use serde_json::json;

    fn window_observation(principal: Principal) -> IndexObservation {
        IndexObservation {
            index_handle: IndexHandle::new(),
            resource_type: IndexResourceType::Window,
            source_id: format!("x11:{}", principal.uid()),
            source_kind: IndexSourceKind::X11,
            source_generation: "boot:i3:42:100".into(),
            native_identity: "xid:99".into(),
            authoritative_ref: Some("window:boot:i3:42:100:99".into()),
            owner: Some(principal),
            freshness: Freshness::Recent,
            observed_at_ms: 1,
            updated_at_ms: 1,
            metadata: json!({"xid":99}),
            control_paths: vec![
                ControlPathKind::ProcessWindow,
                ControlPathKind::VisualFallback,
            ],
        }
    }

    #[test]
    fn index_window_becomes_generation_scoped_visual_target() {
        let principal = Principal::new(1000, 1000);
        let observation = window_observation(principal);
        let target = expectation_from_index(&observation, principal).unwrap();
        assert_eq!(target.kind, GraphicalTargetKind::Window);
        assert_eq!(target.target_ref, "window:boot:i3:42:100:99");
        assert_eq!(target.source_generation, "boot:i3:42:100");
        assert!(
            target
                .observed_control_paths
                .contains(&ControlPathKind::VisualFallback)
        );
    }

    #[test]
    fn stale_or_cross_principal_index_observation_cannot_become_target() {
        let principal = Principal::new(1000, 1000);
        let mut observation = window_observation(principal);
        observation.freshness = Freshness::Stale;
        assert!(matches!(
            expectation_from_index(&observation, principal),
            Err(VisualError::StaleTarget)
        ));

        let observation = window_observation(principal);
        assert!(matches!(
            expectation_from_index(&observation, Principal::new(1001, 1001)),
            Err(VisualError::InvalidTarget(_))
        ));
    }

    #[test]
    fn non_graphical_index_resource_is_not_promoted_to_visual_target() {
        let principal = Principal::new(1000, 1000);
        let mut observation = window_observation(principal);
        observation.resource_type = IndexResourceType::Process;
        observation.source_kind = IndexSourceKind::Proc;
        assert!(matches!(
            expectation_from_index(&observation, principal),
            Err(VisualError::InvalidTarget(_))
        ));
    }
}
