mod overlay;
mod service;

pub use overlay::{mouse_gesture_overlay, StrokeOverlay};
pub use service::{
    mouse_gesture_service, MockMouseHookBackend, MouseGestureEventSink, MouseGestureService,
    MouseHookBackend, TrackOutcome,
};

#[cfg(windows)]
pub use service::{should_ignore_event, MG_PASSTHROUGH_MARK};
