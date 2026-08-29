//! Policy-controlled graphical fallback for PortusOS.
//!
//! This crate deliberately does not implement a second window manager or a
//! generic desktop-automation service. It consumes generation-scoped graphical
//! observations from the System Index, performs policy/routing/precondition
//! checks, and delegates the final screenshot/pointer/keyboard effect to a
//! narrow backend selected by the installed Linux profile.
//!
//! P14 keeps package-specific X11 command bindings outside the host-safe core.
//! The exact Artix screenshot/input tools remain a Linux verification gate.

mod backend;
mod controller;
mod model;

pub use backend::{VisualBackend, VisualBackendError, VisualBackendFailure};
pub use controller::{VisualController, visual_action_registry};
pub use model::{
    ACTION_VISUAL_CAPTURE, ACTION_VISUAL_KEYBOARD, ACTION_VISUAL_POINTER, ActiveGraphicalContext,
    CaptureRequest, CaptureResult, CaptureRetention, FallbackJustification,
    GraphicalTargetExpectation, GraphicalTargetKind, KeyboardAction, KeyboardRequest,
    PointerAction, PointerButton, PointerRequest, ScreenSensitivity, ScreenshotCapture,
    TargetGeometry, ValidatedGraphicalTarget, VisualError, VisualOperationKind, VisualReceipt,
    expectation_from_index,
};
