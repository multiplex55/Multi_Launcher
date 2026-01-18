mod service;

pub use service::{
    mouse_gesture_service, MouseGestureEventSink, MouseGestureService, MouseHookBackend,
    MockMouseHookBackend, TrackOutcome,
};
