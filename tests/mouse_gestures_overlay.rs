use multi_launcher::mouse_gestures::overlay::{HintOverlay, OverlayBackend, TrailOverlay};
use std::sync::{Arc, Mutex};

#[derive(Default, Debug)]
struct RecordedCalls {
    trails: Vec<TrailCall>,
    hints: Vec<HintCall>,
    hides: usize,
    clears: usize,
}

#[derive(Debug, PartialEq)]
struct TrailCall {
    from: (f32, f32),
    to: (f32, f32),
}

#[derive(Debug, PartialEq)]
struct HintCall {
    text: String,
    position: (f32, f32),
}

#[derive(Clone, Default)]
struct RecordingBackend {
    calls: Arc<Mutex<RecordedCalls>>,
}

impl RecordingBackend {
    fn calls(&self) -> Arc<Mutex<RecordedCalls>> {
        Arc::clone(&self.calls)
    }
}

impl OverlayBackend for RecordingBackend {
    fn draw_trail_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        _color: [u8; 4],
        _width: f32,
    ) {
        if let Ok(mut guard) = self.calls.lock() {
            guard.trails.push(TrailCall { from, to });
        }
    }

    fn clear_trail(&mut self) {
        if let Ok(mut guard) = self.calls.lock() {
            guard.clears += 1;
        }
    }

    fn show_hint(&mut self, text: &str, position: (f32, f32)) {
        if let Ok(mut guard) = self.calls.lock() {
            guard.hints.push(HintCall {
                text: text.to_string(),
                position,
            });
        }
    }

    fn hide_hint(&mut self) {
        if let Ok(mut guard) = self.calls.lock() {
            guard.hides += 1;
        }
    }
}

#[test]
fn trail_starts_after_threshold() {
    let backend = RecordingBackend::default();
    let calls = backend.calls();
    let mut overlay = TrailOverlay::new(backend, true, [0, 0, 0, 0], 1.0, 10.0);

    overlay.reset((0.0, 0.0));
    overlay.update_position((5.0, 0.0));
    overlay.update_position((9.0, 0.0));

    {
        let guard = calls.lock().expect("lock calls");
        assert!(guard.trails.is_empty());
        assert_eq!(guard.clears, 1);
    }

    overlay.update_position((12.0, 0.0));
    overlay.update_position((20.0, 0.0));

    let guard = calls.lock().expect("lock calls");
    assert_eq!(
        guard.trails,
        vec![
            TrailCall {
                from: (0.0, 0.0),
                to: (12.0, 0.0)
            },
            TrailCall {
                from: (12.0, 0.0),
                to: (20.0, 0.0)
            }
        ]
    );
    assert_eq!(guard.clears, 1);
}

#[test]
fn hint_updates_on_tokens_and_hides_when_disabled() {
    let backend = RecordingBackend::default();
    let calls = backend.calls();
    let mut overlay = HintOverlay::new(backend, true, (2.0, 3.0));

    overlay.update("LR", Some("Open"), (10.0, 10.0));
    overlay.update("LR", Some("Open"), (10.0, 10.0));
    overlay.update("LRU", Some("Open"), (10.0, 10.0));

    {
        let guard = calls.lock().expect("lock calls");
        assert_eq!(
            guard.hints,
            vec![
                HintCall {
                    text: "LR - Open".to_string(),
                    position: (12.0, 13.0),
                },
                HintCall {
                    text: "LRU - Open".to_string(),
                    position: (12.0, 13.0),
                }
            ]
        );
        assert_eq!(guard.hides, 0);
    }

    overlay.set_enabled(false);

    let guard = calls.lock().expect("lock calls");
    assert_eq!(guard.hides, 1);
}

#[test]
fn trail_draws_incrementally_without_redraws() {
    let backend = RecordingBackend::default();
    let calls = backend.calls();
    let mut overlay = TrailOverlay::new(backend, true, [0, 0, 0, 0], 1.0, 0.0);

    overlay.reset((0.0, 0.0));
    let point_count = 100;
    for idx in 1..=point_count {
        overlay.update_position((idx as f32 * 2.0, 0.0));
    }

    let guard = calls.lock().expect("lock calls");
    assert_eq!(guard.clears, 1);
    assert_eq!(guard.trails.len(), point_count);
}
