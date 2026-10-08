//! Coordinate inspector domain state, passive runtime, and native surfaces.
//!
//! Capture, clipboard access, commands, and GUI integration are owned by later
//! integration layers. Coordinate conversion and rendering remain testable
//! without native windows.

pub mod controller;
pub mod model;
pub mod native;
pub mod render;
pub mod settings;

pub use controller::{
    CoordinateRenderFrame, CoordinateRuntimeFactory, CoordinateSampler, CoordinateSurfaceBackend,
    CoordinateToolController,
};
pub use model::{
    CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, CoordinateUnavailable,
    ForegroundClientGeometry, FormattedCoordinate, MonitorGeometry, MonitorId, PhysicalPoint,
    PhysicalRect, PhysicalSize, clamp_hud_origin, format_coordinate,
};
pub use native::NativeCoordinateRuntimeFactory;
pub use settings::{
    CoordinateOffset, CoordinateToolPreferences, CrosshairColor, CrosshairPreferences, HudDetail,
};
