use multi_launcher::plugins::mouse_gestures::engine::{
    direction_sequence, dtw_distance, meets_min_track_len, parse_gesture, preprocess_points,
    preprocess_points_for_directions, serialize_gesture, track_length, Point, PreprocessConfig,
    Vector,
};
use multi_launcher::plugins::mouse_gestures::settings::MouseGesturePluginSettings;

fn approx_eq(left: f32, right: f32, tolerance: f32) -> bool {
    (left - right).abs() <= tolerance
}

#[test]
fn parse_serialize_round_trip_with_name_and_whitespace() {
    let input = "  Swipe : 0,0 | 10,0 | 10,10 ";
    let gesture = parse_gesture(input).expect("parse should succeed");
    assert_eq!(gesture.name.as_deref(), Some("Swipe"));
    assert_eq!(
        gesture.points,
        vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 10.0, y: 10.0 }
        ]
    );

    let serialized = serialize_gesture(&gesture);
    assert_eq!(serialized, "Swipe:0,0|10,0|10,10");
    let reparsed = parse_gesture(&serialized).expect("parse should succeed");
    assert_eq!(gesture, reparsed);
}

#[test]
fn parse_serialize_round_trip_without_name() {
    let input = "0,0|5,0|5,5";
    let gesture = parse_gesture(input).expect("parse should succeed");
    assert_eq!(gesture.name, None);
    assert_eq!(
        gesture.points,
        vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 5.0, y: 0.0 },
            Point { x: 5.0, y: 5.0 }
        ]
    );

    let serialized = serialize_gesture(&gesture);
    assert_eq!(serialized, input);
}

#[test]
fn track_length_and_min_track_len_gate() {
    let points = vec![
        Point { x: 0.0, y: 0.0 },
        Point { x: 3.0, y: 4.0 },
        Point { x: 6.0, y: 4.0 },
    ];

    let length = track_length(&points);
    assert!(approx_eq(length, 8.0, 0.001));
    assert!(meets_min_track_len(&points, 7.5));
    assert!(!meets_min_track_len(&points, 9.0));
}

#[test]
fn preprocess_respects_min_track_len() {
    let points = vec![
        Point { x: 0.0, y: 0.0 },
        Point { x: 1.0, y: 0.0 },
    ];
    let config = PreprocessConfig {
        sample_count: 4,
        smoothing_window: 1,
        min_track_len: 2.0,
    };
    let result = preprocess_points(&points, &config);
    assert!(result.is_err());
}

#[test]
fn dtw_invariants() {
    let base = vec![
        Vector { x: 1.0, y: 0.0 },
        Vector { x: 1.0, y: 0.0 },
        Vector { x: 1.0, y: 0.0 },
    ];
    let identical = dtw_distance(&base, &base);
    assert!(identical <= 0.01);

    let reversed = vec![
        Vector { x: -1.0, y: 0.0 },
        Vector { x: -1.0, y: 0.0 },
        Vector { x: -1.0, y: 0.0 },
    ];
    let reversed_distance = dtw_distance(&base, &reversed);
    assert!(reversed_distance > 1.5);

    let perturbed = vec![
        Vector { x: 0.98, y: 0.1 },
        Vector { x: 1.0, y: 0.0 },
        Vector { x: 0.95, y: -0.05 },
    ];
    let perturbed_distance = dtw_distance(&base, &perturbed);
    assert!(perturbed_distance < 0.2);

    let unrelated = vec![
        Vector { x: 0.0, y: 1.0 },
        Vector { x: 0.0, y: 1.0 },
        Vector { x: 0.0, y: 1.0 },
    ];
    let unrelated_distance = dtw_distance(&base, &unrelated);
    assert!(unrelated_distance > 0.9);
}

#[test]
fn preprocess_for_directions_smooths_jittery_input() {
    let points = vec![
        Point { x: 0.0, y: 0.0 },
        Point { x: 10.0, y: 5.0 },
        Point { x: 20.0, y: -5.0 },
        Point { x: 30.0, y: 5.0 },
        Point { x: 40.0, y: -5.0 },
        Point { x: 50.0, y: 0.0 },
    ];

    let mut raw_settings = MouseGesturePluginSettings::default();
    raw_settings.segment_threshold_px = 0.0;
    raw_settings.direction_tolerance_deg = 0.0;
    let raw_dirs = direction_sequence(&points, &raw_settings);

    let mut settings = MouseGesturePluginSettings::default();
    settings.sampling_enabled = true;
    settings.smoothing_enabled = true;
    settings.segment_threshold_px = 0.0;
    settings.direction_tolerance_deg = 0.0;

    let processed = preprocess_points_for_directions(&points, &settings);
    let processed_dirs = direction_sequence(&processed, &settings);

    assert!(!raw_dirs.is_empty());
    assert!(!processed_dirs.is_empty());
    assert_ne!(raw_dirs, processed_dirs);
    assert!(processed_dirs.len() < raw_dirs.len());
}
