use crate::{
    ActiveGraphicalContext, GraphicalTargetExpectation, KeyboardAction, PointerAction,
    ScreenshotCapture, ValidatedGraphicalTarget,
};
use portus_protocol::Principal;
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualBackendFailure {
    SourceUnavailable,
    TargetUnavailable,
    FocusFailed,
    CaptureFailed,
    PointerFailed,
    KeyboardFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualBackendError {
    pub reason: VisualBackendFailure,
}

impl VisualBackendError {
    #[must_use]
    pub const fn new(reason: VisualBackendFailure) -> Self {
        Self { reason }
    }
}

impl fmt::Display for VisualBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self.reason {
            VisualBackendFailure::SourceUnavailable => "graphical source unavailable",
            VisualBackendFailure::TargetUnavailable => "graphical target unavailable",
            VisualBackendFailure::FocusFailed => "graphical target focus failed",
            VisualBackendFailure::CaptureFailed => "screenshot capture failed",
            VisualBackendFailure::PointerFailed => "pointer action failed",
            VisualBackendFailure::KeyboardFailed => "keyboard action failed",
        };
        f.write_str(value)
    }
}

impl Error for VisualBackendError {}

pub trait VisualBackend {
    fn active_context(
        &mut self,
        principal: Principal,
    ) -> Result<ActiveGraphicalContext, VisualBackendError>;

    fn revalidate_target(
        &mut self,
        principal: Principal,
        expected: &GraphicalTargetExpectation,
    ) -> Result<ValidatedGraphicalTarget, VisualBackendError>;

    fn focus_target(
        &mut self,
        principal: Principal,
        target: &ValidatedGraphicalTarget,
    ) -> Result<(), VisualBackendError>;

    fn capture(
        &mut self,
        principal: Principal,
        target: &ValidatedGraphicalTarget,
    ) -> Result<ScreenshotCapture, VisualBackendError>;

    fn pointer(
        &mut self,
        principal: Principal,
        target: &ValidatedGraphicalTarget,
        action: PointerAction,
    ) -> Result<(), VisualBackendError>;

    fn keyboard(
        &mut self,
        principal: Principal,
        target: &ValidatedGraphicalTarget,
        action: &KeyboardAction,
    ) -> Result<(), VisualBackendError>;
}
