use multi_launcher::mouse_gestures::service::{
    CancelBehavior, CursorPositionProvider, HookEvent, MockHookBackend, MouseGestureConfig,
    MouseGestureService, NoMatchBehavior, OverlayFactory, RightClickBackend,
};
use multi_launcher::mouse_gestures::overlay::OverlayBackend;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

#[derive(Default)]
struct TestOverlayState {
    trail_clears: AtomicUsize,
    hint_hides: AtomicUsize,
}

#[derive(Clone)]
struct TestOverlayBackend {
    state: Arc<TestOverlayState>,
}

impl OverlayBackend for TestOverlayBackend {
    fn draw_trail_segment(&mut self, _from: (f32, f32), _to: (f32, f32), _color: [u8; 4], _width: f32) {
    }

    fn clear_trail(&mut self) {
        self.state.trail_clears.fetch_add(1, Ordering::SeqCst);
    }

    fn show_hint(&mut self, _text: &str, _position: (f32, f32)) {}

    fn hide_hint(&mut self) {
        self.state.hint_hides.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct TestOverlayFactory {
    state: Arc<TestOverlayState>,
}

impl OverlayFactory for TestOverlayFactory {
    fn trail_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(TestOverlayBackend {
            state: Arc::clone(&self.state),
        })
    }

    fn hint_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(TestOverlayBackend {
            state: Arc::clone(&self.state),
        })
    }
}

#[derive(Default)]
struct TestRightClickBackend {
    clicks: AtomicUsize,
}

impl RightClickBackend for TestRightClickBackend {
    fn send_right_click(&self) {
        self.clicks.fetch_add(1, Ordering::SeqCst);
    }
}

struct TestCursorProvider {
    position: Mutex<Option<(f32, f32)>>,
}

impl TestCursorProvider {
    fn new(pos: (f32, f32)) -> Self {
        Self {
            position: Mutex::new(Some(pos)),
        }
    }

    fn set_position(&self, pos: (f32, f32)) {
        if let Ok(mut guard) = self.position.lock() {
            *guard = Some(pos);
        }
    }
}

impl CursorPositionProvider for TestCursorProvider {
    fn cursor_position(&self) -> Option<(f32, f32)> {
        self.position.lock().ok().and_then(|guard| *guard)
    }
}

#[test]
fn start_stop_installs_and_uninstalls_once() {
    let (backend, handle) = MockHookBackend::new();
    let mut service = MouseGestureService::new_with_backend(Box::new(backend));

    service.start();
    service.start();
    service.stop();
    service.stop();

    assert_eq!(handle.install_count(), 1);
    assert_eq!(handle.uninstall_count(), 1);
}

#[test]
fn disabling_config_stops_worker_and_blocks_hook_events() {
    let (backend, handle) = MockHookBackend::new();
    let mut service = MouseGestureService::new_with_backend(Box::new(backend));
    let mut config = MouseGestureConfig::default();

    config.enabled = true;
    service.update_config(config.clone());
    assert!(service.is_running());
    assert!(handle.emit(HookEvent::RButtonDown));

    config.enabled = false;
    service.update_config(config);

    assert!(!service.is_running());
    assert!(!handle.emit(HookEvent::RButtonDown));
}

#[test]
fn cancel_event_clears_overlays_and_does_not_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_state = Arc::new(TestOverlayState::default());
    let overlay_factory = Arc::new(TestOverlayFactory {
        state: Arc::clone(&overlay_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend),
        cursor_provider,
    );

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.cancel_behavior = CancelBehavior::DoNothing;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(20));
    let clears_before = overlay_state.trail_clears.load(Ordering::SeqCst);
    let hides_before = overlay_state.hint_hides.load(Ordering::SeqCst);

    assert!(handle.emit(HookEvent::Cancel));
    sleep(Duration::from_millis(20));

    let clears_after = overlay_state.trail_clears.load(Ordering::SeqCst);
    let hides_after = overlay_state.hint_hides.load(Ordering::SeqCst);
    assert!(clears_after > clears_before);
    assert!(hides_after > hides_before);
    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 0);

    service.stop();
}

#[test]
fn no_match_pass_through_click_sends_right_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_factory = Arc::new(TestOverlayFactory {
        state: Arc::new(TestOverlayState::default()),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend),
        Arc::clone(&cursor_provider),
    );

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.no_match_behavior = NoMatchBehavior::PassThroughClick;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    assert!(handle.emit(HookEvent::RButtonUp));
    sleep(Duration::from_millis(20));

    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 1);

    service.stop();
}

#[test]
fn no_match_noop_does_not_send_right_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_factory = Arc::new(TestOverlayFactory {
        state: Arc::new(TestOverlayState::default()),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend),
        Arc::clone(&cursor_provider),
    );

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.no_match_behavior = NoMatchBehavior::DoNothing;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    assert!(handle.emit(HookEvent::RButtonUp));
    sleep(Duration::from_millis(20));

    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 0);

    service.stop();
}
