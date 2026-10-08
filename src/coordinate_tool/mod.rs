//! Pure coordinate data and transient state for the coordinate inspector.
//!
//! Native sampling, clipboard access, commands, and presentation are owned by
//! later integration layers. This module keeps coordinate conversion and
//! inspector state deterministic and independently testable.

pub mod model;
pub mod settings;

pub use model::{
    CoordinateSample, CoordinateSpace, CoordinateToolRuntimeState, CoordinateUnavailable,
    FormattedCoordinate, MonitorGeometry, MonitorId, PhysicalPoint, PhysicalRect, PhysicalSize,
    clamp_hud_origin, format_coordinate,
};
pub use settings::{
    CoordinateOffset, CoordinateToolPreferences, CrosshairColor, CrosshairPreferences, HudDetail,
};
