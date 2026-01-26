use multi_launcher::mouse_gestures::overlay::{HintOverlay, OverlayBackend, TrailOverlay};
use multi_launcher::mouse_gestures::service::{
    worker_loop_with_overlays, HookEvent, MockHookBackend, MouseGestureConfig, MouseGestureService,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Default)]
struct CountingOverlayBackend {
    draw_calls: Arc<AtomicUsize>,
    hint_calls: Arc<AtomicUsize>,
}

impl CountingOverlayBackend {
    fn draw_calls(&self) -> usize {
        self.draw_calls.load(Ordering::SeqCst)
    }
}

impl OverlayBackend for CountingOverlayBackend {
    fn draw_trail_segment(
        &mut self,
        _from: (f32, f32),
        _to: (f32, f32),
        _color: [u8; 4],
        _width: f32,
    ) {
        self.draw_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn show_hint(&mut self, _text: &str, _position: (f32, f32)) {
        self.hint_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn hide_hint(&mut self) {}
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
fn draw_updates_are_independent_from_recognition_interval() {
    let mut config = MouseGestureConfig::default();
    config.draw_interval_ms = 5;
    config.draw_min_distance_px = 2.0;
    config.recognition_interval_ms = 200;
    config.trail_start_move_px = 0.0;
    config.show_trail = true;
    config.show_hint = false;

    let (event_tx, event_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let position = Arc::new(Mutex::new((0.0_f32, 0.0_f32)));
    let position_provider = {
        let position = Arc::clone(&position);
        move || position.lock().ok().copied()
    };

    let draw_backend = CountingOverlayBackend::default();
    let draw_calls = draw_backend.draw_calls.clone();
    let hint_backend = CountingOverlayBackend::default();
    let trail_overlay = TrailOverlay::new(
        draw_backend,
        config.show_trail,
        config.trail_color,
        config.trail_width,
        config.trail_start_move_px,
    );
    let hint_overlay = HintOverlay::new(hint_backend, config.show_hint, config.hint_offset);

    let recognition_calls = Arc::new(AtomicUsize::new(0));
    let recognition_counter = Arc::clone(&recognition_calls);

    let join = thread::spawn(move || {
        worker_loop_with_overlays(
            config,
            None,
            event_rx,
            stop_rx,
            trail_overlay,
            hint_overlay,
            position_provider,
            Some(move |_, _| {
                recognition_counter.fetch_add(1, Ordering::SeqCst);
            }),
        );
    });

    event_tx.send(HookEvent::RButtonDown).unwrap();
    for step in 1..6 {
        {
            let mut guard = position.lock().unwrap();
            *guard = (step as f32 * 3.0, 0.0);
        }
        thread::sleep(Duration::from_millis(6));
    }

    stop_tx.send(()).unwrap();
    join.join().unwrap();

    let draw_count = draw_calls.load(Ordering::SeqCst);
    let recognition_count = recognition_calls.load(Ordering::SeqCst);
    assert!(
        draw_count >= 2,
        "expected frequent draw updates, got {draw_count}"
    );
    assert!(
        draw_count > recognition_count,
        "expected draw updates even with long recognition interval (draw={draw_count}, recognition={recognition_count})"
    );
}

#[test]
fn recognition_rate_unchanged_when_draw_interval_changes() {
    fn run_recognition_count(draw_interval_ms: u64) -> usize {
        let mut config = MouseGestureConfig::default();
        config.draw_interval_ms = draw_interval_ms;
        config.draw_min_distance_px = 2.0;
        config.recognition_interval_ms = 20;
        config.trail_start_move_px = 0.0;
        config.show_trail = false;
        config.show_hint = false;

        let (event_tx, event_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let position = Arc::new(Mutex::new((0.0_f32, 0.0_f32)));
        let position_provider = {
            let position = Arc::clone(&position);
            move || position.lock().ok().copied()
        };

        let trail_overlay = TrailOverlay::new(
            CountingOverlayBackend::default(),
            config.show_trail,
            config.trail_color,
            config.trail_width,
            config.trail_start_move_px,
        );
        let hint_overlay =
            HintOverlay::new(CountingOverlayBackend::default(), config.show_hint, config.hint_offset);

        let recognition_calls = Arc::new(AtomicUsize::new(0));
        let recognition_counter = Arc::clone(&recognition_calls);

        let join = thread::spawn(move || {
            worker_loop_with_overlays(
                config,
                None,
                event_rx,
                stop_rx,
                trail_overlay,
                hint_overlay,
                position_provider,
                Some(move |_, _| {
                    recognition_counter.fetch_add(1, Ordering::SeqCst);
                }),
            );
        });

        event_tx.send(HookEvent::RButtonDown).unwrap();
        for step in 1..8 {
            {
                let mut guard = position.lock().unwrap();
                *guard = (step as f32 * 3.0, 0.0);
            }
            thread::sleep(Duration::from_millis(10));
        }

        stop_tx.send(()).unwrap();
        join.join().unwrap();

        recognition_calls.load(Ordering::SeqCst)
    }

    let fast_draw = run_recognition_count(5);
    let slow_draw = run_recognition_count(30);

    let difference = fast_draw.max(slow_draw) - fast_draw.min(slow_draw);
    assert!(
        difference <= 1,
        "expected recognition rate to stay stable (fast={fast_draw}, slow={slow_draw})"
    );
}
