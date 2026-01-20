mod overlay;
mod service;

pub use overlay::{decimate_points_for_overlay, mouse_gesture_overlay, StrokeOverlay};
pub use service::{
    mouse_gesture_service, HookTrackingState, MockMouseHookBackend, MouseGestureEventSink,
    MouseGestureService, MouseHookBackend, TrackOutcome, MAX_TRACK_POINTS,
};

#[cfg(windows)]
pub use service::{should_ignore_event, MG_PASSTHROUGH_MARK};
