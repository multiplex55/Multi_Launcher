use multi_launcher::gui::register_event_sender;
use multi_launcher::mouse_gestures::db::{
    BindingEntry, BindingKind, GestureDb, GestureEntry, SCHEMA_VERSION,
};
use multi_launcher::mouse_gestures::engine::DirMode;
use multi_launcher::mouse_gestures::overlay::OverlayBackend;
use multi_launcher::mouse_gestures::service::{
    set_draw_mode_active, should_ignore_window_title, CancelBehavior, CursorPositionProvider,
    HookEvent, MockHookBackend, MouseGestureConfig, MouseGestureService, NoMatchBehavior,
    OverlayFactory, RightClickBackend,
};
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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
    fn draw_trail_segment(
        &mut self,
        _from: (f32, f32),
        _to: (f32, f32),
        _color: [u8; 4],
        _width: f32,
    ) {
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
struct HintRecordingState {
    hints: Mutex<Vec<String>>,
}

#[derive(Clone)]
struct RecordingHintBackend {
    state: Arc<HintRecordingState>,
}

impl OverlayBackend for RecordingHintBackend {
    fn draw_trail_segment(
        &mut self,
        _from: (f32, f32),
        _to: (f32, f32),
        _color: [u8; 4],
        _width: f32,
    ) {
    }

    fn clear_trail(&mut self) {}

    fn show_hint(&mut self, text: &str, _position: (f32, f32)) {
        if let Ok(mut guard) = self.state.hints.lock() {
            guard.push(text.to_string());
        }
    }

    fn hide_hint(&mut self) {}
}

#[derive(Clone)]
struct HintRecordingFactory {
    state: Arc<HintRecordingState>,
}

impl OverlayFactory for HintRecordingFactory {
    fn trail_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(TestOverlayBackend {
            state: Arc::new(TestOverlayState::default()),
        })
    }

    fn hint_backend(&self) -> Box<dyn OverlayBackend> {
        Box::new(RecordingHintBackend {
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

fn wait_for_hint(state: &HintRecordingState, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    loop {
        if let Ok(guard) = state.hints.lock() {
            if let Some(last) = guard.last() {
                return Some(last.clone());
            }
        }
        if start.elapsed() >= timeout {
            return None;
        }
        sleep(Duration::from_millis(5));
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
fn should_ignore_window_title_handles_matches() {
    let ignore = vec!["Notepad".to_string(), "  firefox  ".to_string()];
    assert!(should_ignore_window_title(&ignore, "Notepad"));
    assert!(should_ignore_window_title(&ignore, "Mozilla Firefox"));
    assert!(should_ignore_window_title(
        &ignore,
        "FIREFOX - Private Browsing"
    ));
    assert!(!should_ignore_window_title(&ignore, "Terminal"));
}

#[test]
fn should_ignore_window_title_skips_empty_entries() {
    let ignore = vec!["".to_string(), "   ".to_string()];
    assert!(!should_ignore_window_title(&ignore, "Notepad"));
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
fn draw_mode_gate_suppresses_gesture_down_move_up_paths() {
    let _guard = TEST_MUTEX.lock().expect("test mutex");
    set_draw_mode_active(false);

    let (backend, handle) = MockHookBackend::new();
    let overlay_state = Arc::new(TestOverlayState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(TestOverlayFactory {
        state: Arc::clone(&overlay_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((10.0, 10.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    service.update_config(config);

    set_draw_mode_active(true);
    assert!(handle.emit(HookEvent::RButtonDown));
    cursor_provider.set_position((120.0, 120.0));
    sleep(Duration::from_millis(80));
    assert!(handle.emit(HookEvent::RButtonUp));
    sleep(Duration::from_millis(80));

    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 0);
    assert_eq!(overlay_state.trail_clears.load(Ordering::SeqCst), 0);

    set_draw_mode_active(false);
    service.stop();
}

#[test]
fn cancel_event_clears_overlays_and_does_not_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_state = Arc::new(TestOverlayState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(TestOverlayFactory {
        state: Arc::clone(&overlay_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        cursor_provider_trait,
    );

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.cancel_behavior = CancelBehavior::DoNothing;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(50));
    let clears_before = overlay_state.trail_clears.load(Ordering::SeqCst);
    let hides_before = overlay_state.hint_hides.load(Ordering::SeqCst);

    assert!(handle.emit(HookEvent::Cancel));
    sleep(Duration::from_millis(50));

    let clears_after = overlay_state.trail_clears.load(Ordering::SeqCst);
    let hides_after = overlay_state.hint_hides.load(Ordering::SeqCst);
    assert!(clears_after > clears_before);
    assert!(hides_after >= hides_before);
    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 0);

    service.stop();
}

#[test]
fn no_match_pass_through_click_sends_right_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(TestOverlayFactory {
        state: Arc::new(TestOverlayState::default()),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
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
    sleep(Duration::from_millis(50));

    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 1);

    service.stop();
}

#[test]
fn no_match_noop_does_not_send_right_click() {
    let (backend, handle) = MockHookBackend::new();
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(TestOverlayFactory {
        state: Arc::new(TestOverlayState::default()),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
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
    sleep(Duration::from_millis(50));

    assert_eq!(click_backend.clicks.load(Ordering::SeqCst), 0);

    service.stop();
}

#[test]
fn hint_text_includes_best_guess_and_match_type() {
    let _guard = TEST_MUTEX.lock().expect("test mutex");
    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "RU".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Open Browser".into(),
                kind: BindingKind::Execute,
                action: "stopwatch:show:1".into(),
                args: None,
                enabled: true,
            }],
        }],
    };
    service.update_db(Some(Arc::new(Mutex::new(db))));

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    let deadline = Instant::now() + Duration::from_millis(500);
    let expected =
        "R\nWheel: cycle • 1-9: select • Release: run • Esc: cancel\nClosest: Open Browser [prefix]";

    let last = loop {
        {
            let hints = hint_state.hints.lock().expect("lock hints");
            if let Some(last) = hints.last() {
                if last == expected {
                    break last.clone();
                }
            }
        }

        if Instant::now() >= deadline {
            let hints = hint_state.hints.lock().expect("lock hints");
            panic!("hint text: {:?}", hints.last());
        }
        sleep(Duration::from_millis(10));
    };

    assert_eq!(last, expected);

    service.stop();
}

#[test]
fn practice_mode_suppresses_execute_action() {
    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "R".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![BindingEntry {
                label: "Open Browser".into(),
                kind: BindingKind::Execute,
                action: "stopwatch:show:1".into(),
                args: None,
                enabled: true,
            }],
        }],
    };
    service.update_db(Some(Arc::new(Mutex::new(db))));

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.practice_mode = true;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config);

    let (tx, rx) = std::sync::mpsc::channel();
    register_event_sender(tx);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    assert!(handle.emit(HookEvent::RButtonUp));
    sleep(Duration::from_millis(20));

    assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());

    service.stop();
}

#[test]
fn cheat_sheet_overlay_shows_after_delay_without_tokens() {
    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![
            GestureEntry {
                label: "Open Browser".into(),
                tokens: "R".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Open Browser".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                }],
            },
            GestureEntry {
                label: "Close Window".into(),
                tokens: "L".into(),
                dir_mode: DirMode::Four,
                stroke: Vec::new(),
                enabled: true,
                bindings: vec![BindingEntry {
                    label: "Close Window".into(),
                    kind: BindingKind::Execute,
                    action: "window:close".into(),
                    args: None,
                    enabled: true,
                }],
            },
        ],
    };
    service.update_db(Some(Arc::new(Mutex::new(db))));

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.deadzone_px = 100.0;
    config.trail_interval_ms = 10;
    config.recognition_interval_ms = 10;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(300));

    let hints = hint_state.hints.lock().expect("lock hints");
    let last = hints.last().expect("hint text");
    assert!(last.contains("Cheat sheet"));
    assert!(last.contains("Open Browser"));

    service.stop();
}

#[test]
fn selection_persists_across_gesture_sessions() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().expect("tempdir");
    std::env::set_current_dir(dir.path()).expect("set current dir");

    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "R".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![
                BindingEntry {
                    label: "Primary".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                },
                BindingEntry {
                    label: "Secondary".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:2".into(),
                    args: None,
                    enabled: true,
                },
            ],
        }],
    };
    service.update_db(Some(Arc::new(Mutex::new(db.clone()))));

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config.clone());

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    sleep(Duration::from_millis(20));
    assert!(handle.emit(HookEvent::SelectBinding(1)));
    sleep(Duration::from_millis(10));
    assert!(handle.emit(HookEvent::RButtonUp));
    sleep(Duration::from_millis(20));

    service.stop();

    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );
    service.update_db(Some(Arc::new(Mutex::new(db))));
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    sleep(Duration::from_millis(50));

    let hint_text = wait_for_hint(&hint_state, Duration::from_millis(500)).expect("hint text");
    assert!(hint_text.contains("Secondary"));

    service.stop();
}

#[test]
fn numeric_selection_updates_hint_text() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().expect("tempdir");
    std::env::set_current_dir(dir.path()).expect("set current dir");

    let (backend, handle) = MockHookBackend::new();
    let hint_state = Arc::new(HintRecordingState::default());
    let overlay_factory: Arc<dyn OverlayFactory> = Arc::new(HintRecordingFactory {
        state: Arc::clone(&hint_state),
    });
    let click_backend = Arc::new(TestRightClickBackend::default());
    let click_backend_trait: Arc<dyn RightClickBackend> = click_backend.clone();
    let cursor_provider = Arc::new(TestCursorProvider::new((0.0, 0.0)));
    let cursor_provider_trait: Arc<dyn CursorPositionProvider> = cursor_provider.clone();

    let mut service = MouseGestureService::new_with_backend_and_overlays(
        Box::new(backend),
        overlay_factory,
        Arc::clone(&click_backend_trait),
        Arc::clone(&cursor_provider_trait),
    );

    let db = GestureDb {
        schema_version: SCHEMA_VERSION,
        gestures: vec![GestureEntry {
            label: "Open Browser".into(),
            tokens: "R".into(),
            dir_mode: DirMode::Four,
            stroke: Vec::new(),
            enabled: true,
            bindings: vec![
                BindingEntry {
                    label: "First".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:1".into(),
                    args: None,
                    enabled: true,
                },
                BindingEntry {
                    label: "Second".into(),
                    kind: BindingKind::Execute,
                    action: "stopwatch:show:2".into(),
                    args: None,
                    enabled: true,
                },
            ],
        }],
    };
    service.update_db(Some(Arc::new(Mutex::new(db))));

    let mut config = MouseGestureConfig::default();
    config.enabled = true;
    config.threshold_px = 1.0;
    config.deadzone_px = 0.1;
    config.trail_interval_ms = 1;
    config.recognition_interval_ms = 1;
    service.update_config(config);

    assert!(handle.emit(HookEvent::RButtonDown));
    sleep(Duration::from_millis(5));
    cursor_provider.set_position((50.0, 0.0));
    sleep(Duration::from_millis(20));

    assert!(handle.emit(HookEvent::SelectBinding(1)));
    sleep(Duration::from_millis(10));

    let last = wait_for_hint(&hint_state, Duration::from_millis(500)).expect("hint text");
    let first_line = last.lines().next().expect("first line");
    assert!(first_line.contains("Second"));

    service.stop();
}
