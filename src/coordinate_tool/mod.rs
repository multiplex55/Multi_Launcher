//! Coordinate inspector domain state, passive runtime, and native surfaces.
//!
//! Capture state and passive rendering remain testable without native windows;
//! Win32 interception and clipboard/GUI integration stay behind feature seams.

pub mod capture;
pub mod controller;
pub mod model;
pub mod native;
pub(crate) mod native_effects;
pub mod render;
pub mod settings;

pub use capture::{
    ButtonEdge, CaptureButton, CaptureDisposition, CaptureInput, CaptureKey, CaptureModifiers,
    CaptureOutcome, CapturePhase, CaptureRuntime, CaptureSessionId, CaptureStatus,
    CoordinateCaptureController, NativeCoordinateCaptureRuntime,
};
pub use controller::{
    ClickObservationLease, ClickObservationProvider, CoordinateRenderFrame,
    CoordinateRuntimeFactory, CoordinateSampler, CoordinateSurfaceBackend,
    CoordinateToolController, MouseGestureClickObservationProvider,
};
pub use model::{
    ClickButton, ClickEffectStatus, ClickPressEvent, ClickPressSink, CoordinateEffectsStatus,
    CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, CoordinateUnavailable,
    CursorEffectStatus, ForegroundClientGeometry, FormattedCoordinate, MonitorGeometry, MonitorId,
    PhysicalPoint, PhysicalRect, PhysicalSize, clamp_hud_origin, format_coordinate,
};
pub(crate) use native::NativeCoordinatePointSampler;
pub use native::NativeCoordinateRuntimeFactory;
pub use render::ClickEffectFrame;
pub use settings::{
    ClickEffectPreferences, ClickEffectStyle, CoordinateOffset, CoordinateToolPreferences,
    CrosshairColor, CrosshairPreferences, HaloPreferences, HudDetail, ZoomMode, ZoomPreferences,
};
