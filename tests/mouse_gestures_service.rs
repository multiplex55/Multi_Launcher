use multi_launcher::gui::MouseGestureEvent;
use multi_launcher::mouse_gestures::{
    MockMouseHookBackend, MouseGestureEventSink, MouseGestureService,
};
use multi_launcher::plugins::mouse_gestures::db::{
    MouseGestureBinding, MouseGestureDb, MouseGestureProfile,
};
use multi_launcher::plugins::mouse_gestures::engine::Point;
use multi_launcher::plugins::mouse_gestures::settings::MouseGesturePluginSettings;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingSink {
    events: Arc<Mutex<Vec<MouseGestureEvent>>>,
}

impl RecordingSink {
    fn events(&self) -> Vec<MouseGestureEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl MouseGestureEventSink for RecordingSink {
    fn dispatch(&self, event: MouseGestureEvent) {
        if let Ok(mut guard) = self.events.lock() {
            guard.push(event);
        }
    }
}

#[test]
fn mouse_gesture_service_start_stop_idempotent() {
    let backend = Arc::new(MockMouseHookBackend::default());
    let sink = Arc::new(RecordingSink::default());
    let service = MouseGestureService::new_with_backend_and_sink(backend.clone(), sink);

    service.start();
    service.start();
    assert_eq!(backend.start_count(), 1);

    service.stop();
    service.stop();
    assert_eq!(backend.stop_count(), 1);
}

#[test]
fn mouse_gesture_service_passthrough_for_short_track() {
    let backend = Arc::new(MockMouseHookBackend::default());
    let sink = Arc::new(RecordingSink::default());
    let service = MouseGestureService::new_with_backend_and_sink(backend.clone(), sink);

    let mut settings = MouseGesturePluginSettings::default();
    settings.enabled = true;
    settings.min_track_len = 50.0;
    service.update_settings(settings);
    service.start();

    let points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 0.0 }];
    let outcome = backend.simulate_track(points);

    assert!(!outcome.matched);
    assert!(outcome.passthrough_click);
    assert_eq!(backend.passthrough_clicks(), 1);
}

#[test]
fn mouse_gesture_service_dispatches_event_on_match() {
    let backend = Arc::new(MockMouseHookBackend::default());
    let sink = Arc::new(RecordingSink::default());
    let service = MouseGestureService::new_with_backend_and_sink(backend.clone(), sink.clone());

    let mut db = MouseGestureDb::default();
    db.bindings = HashMap::from([("gesture-1".to_string(), "SwipeRight:0,0|100,0".to_string())]);
    db.profiles.push(MouseGestureProfile {
        id: "default".to_string(),
        label: "Default".to_string(),
        enabled: true,
        priority: 0,
        rules: Vec::new(),
        bindings: vec![MouseGestureBinding {
            gesture_id: "gesture-1".to_string(),
            label: "Calc".to_string(),
            action: "query:calc".to_string(),
            args: Some("1+1".to_string()),
            priority: 0,
            enabled: true,
        }],
    });
    service.update_db(db);

    let mut settings = MouseGesturePluginSettings::default();
    settings.enabled = true;
    settings.max_distance = 1.0;
    settings.min_track_len = 1.0;
    settings.sampling_enabled = false;
    settings.smoothing_enabled = false;
    service.update_settings(settings);
    service.start();

    let points = vec![Point { x: 0.0, y: 0.0 }, Point { x: 100.0, y: 0.0 }];
    let outcome = backend.simulate_track(points);

    assert!(outcome.matched);
    let events = sink.events();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.gesture_id, "gesture-1");
    assert_eq!(event.gesture_name.as_deref(), Some("SwipeRight"));
    assert_eq!(event.profile_id, "default");
    assert_eq!(event.profile_label, "Default");
    assert_eq!(event.action_payload, "query:calc");
    assert_eq!(event.action_args.as_deref(), Some("1+1"));
    assert!(event.distance.is_finite());
}
