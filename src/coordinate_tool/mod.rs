//! Coordinate inspector domain state, passive runtime, and native surfaces.
//!
//! Capture state and passive rendering remain testable without native windows;
//! Win32 interception and clipboard/GUI integration stay behind feature seams.

pub mod capture;
pub mod controller;
pub mod model;
pub mod native;
pub mod render;
pub mod settings;

pub use capture::{
    ButtonEdge, CaptureButton, CaptureDisposition, CaptureInput, CaptureKey, CaptureModifiers,
    CaptureOutcome, CapturePhase, CaptureRuntime, CaptureSessionId, CaptureStatus,
    CoordinateCaptureController, NativeCoordinateCaptureRuntime,
};
pub use controller::{
    CoordinateRenderFrame, CoordinateRuntimeFactory, CoordinateSampler, CoordinateSurfaceBackend,
    CoordinateToolController,
};
pub use model::{
    CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, CoordinateUnavailable,
    ForegroundClientGeometry, FormattedCoordinate, MonitorGeometry, MonitorId, PhysicalPoint,
    PhysicalRect, PhysicalSize, clamp_hud_origin, format_coordinate,
};
pub(crate) use native::NativeCoordinatePointSampler;
pub use native::NativeCoordinateRuntimeFactory;
pub use settings::{
    CoordinateOffset, CoordinateToolPreferences, CrosshairColor, CrosshairPreferences,
    HaloPreferences, HudDetail, ZoomMode, ZoomPreferences,
};
