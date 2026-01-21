mod overlay;
mod service;

pub use overlay::{mouse_gesture_overlay, StrokeOverlay};
pub use service::{
    format_mouse_gesture_hook_status, mouse_gesture_service, HookTrackingState,
    MockMouseHookBackend, MouseGestureEventSink, MouseGestureHookStatus, MouseGestureService,
    MouseHookBackend, TrackOutcome, MAX_TRACK_POINTS,
};

#[cfg(windows)]
pub use service::{should_ignore_event, MG_PASSTHROUGH_MARK};
