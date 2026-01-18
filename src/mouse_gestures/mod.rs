mod overlay;
mod service;

pub use overlay::{mouse_gesture_overlay, StrokeOverlay};
pub use service::{
    mouse_gesture_service, MockMouseHookBackend, MouseGestureEventSink, MouseGestureService,
    MouseHookBackend, TrackOutcome,
};
