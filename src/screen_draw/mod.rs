//! Shared domain types for the desktop-wide Screen Draw feature.
//!
//! Native window ownership and rendering live in later layers. This module is
//! deliberately platform-independent so settings and geometry can be tested
//! without creating desktop resources.

pub mod capture;
pub mod controller;
pub mod document;
pub mod export;
pub mod geometry;
pub mod hit_test;
mod hotkeys;
pub(crate) mod launcher_parking;
pub mod model;
mod native_canvas;
mod native_overlay;
pub mod native_runtime;
pub mod raster;
mod recovery;
pub mod settings;
pub(crate) mod window_layers;

pub use capture::{ScreenDrawCaptureBackend, ScreenDrawSessionSnapshot};
pub use controller::{
    ScreenDrawCapturePoll, ScreenDrawController, ScreenDrawEditorHandoff, ScreenDrawGeneration,
    ScreenDrawParkingRequest, ScreenDrawRegionPickerReady, ScreenDrawState,
    ScreenDrawTransitionError,
};
pub use document::{
    AnnotationDocument, DocumentError, EraserDrag, TransientInk, TransientStroke, TransientStrokeId,
};
pub use export::{ExportOutcome, ExportSource};
pub use geometry::{
    CropPlan, DesktopPoint, DesktopRect, DesktopSize, LocalPoint, clamp_toolbar_position, plan_crop,
};
pub use hit_test::{annotation_hit_test, kind_hit_test, stroke_hit_test};
pub use model::{
    AnnotationId, AnnotationKind, AnnotationObject, ArrowAnnotation, CanvasBackground,
    ExportBackground, ExportDestination, ExportRequest, ExportScope, LineAnnotation, RgbaColor,
    ScreenDrawMode, ScreenDrawTool, ShapeAnnotation, ShapeStyle, Stroke, StrokePoint,
    TextAnnotation, ToolbarOrientation,
};
pub use native_runtime::{
    ExportRenderRequest, NativeEmergencyHandle, NativeRuntimeState, NativeSessionCommand,
    NativeSessionEvent, NativeSessionHandle,
};
pub use raster::{RasterBackground, RasterError, render_document_into, selected_background};
pub use recovery::ScreenDrawRecoveryBridge;
pub use settings::{HotkeyChord, PALETTE_SLOT_COUNT, ScreenDrawSettings, SettingsValidationError};
