use multi_launcher::mouse_gestures::engine::{DirMode, GestureTracker};

#[test]
fn jitter_under_deadzone_clicks() {
    let mut tracker = GestureTracker::new(DirMode::Four, 10.0, 25.0, 25.0, 10);
    tracker.feed_point((0.0, 0.0), 0);
    tracker.feed_point((3.0, 4.0), 10);
    tracker.feed_point((5.0, 1.0), 20);

    assert!(tracker.tokens().is_empty());
    assert!(tracker.should_click());
}

#[test]
fn cardinal_and_diagonal_tokens_four_mode() {
    let mut tracker = GestureTracker::new(DirMode::Four, 5.0, 25.0, 25.0, 10);
    tracker.feed_point((0.0, 0.0), 0);
    tracker.feed_point((12.0, 0.0), 10);
    tracker.feed_point((12.0, 12.0), 20);
    tracker.feed_point((0.0, 24.0), 30);

    assert_eq!(tracker.tokens_string(), "RDL");
}

#[test]
fn cardinal_and_diagonal_tokens_eight_mode() {
    let mut tracker = GestureTracker::new(DirMode::Eight, 5.0, 25.0, 25.0, 10);
    tracker.feed_point((0.0, 0.0), 0);
    tracker.feed_point((12.0, 0.0), 10);
    tracker.feed_point((12.0, 12.0), 20);
    tracker.feed_point((0.0, 24.0), 30);
    tracker.feed_point((0.0, 12.0), 40);

    assert_eq!(tracker.tokens_string(), "6218");
}

#[test]
fn long_move_repeats_tokens() {
    let mut tracker = GestureTracker::new(DirMode::Four, 3.0, 10.0, 10.0, 10);
    tracker.feed_point((0.0, 0.0), 0);
    tracker.feed_point((4.0, 0.0), 10);
    tracker.feed_point((15.0, 0.0), 20);
    tracker.feed_point((27.0, 0.0), 30);

    assert_eq!(tracker.tokens_string(), "R");
}

#[test]
fn max_tokens_caps_output() {
    let mut tracker = GestureTracker::new(DirMode::Four, 5.0, 25.0, 25.0, 2);
    tracker.feed_point((0.0, 0.0), 0);
    tracker.feed_point((12.0, 0.0), 10);
    tracker.feed_point((12.0, 12.0), 20);
    tracker.feed_point((0.0, 12.0), 30);
    tracker.feed_point((0.0, 0.0), 40);

    assert_eq!(tracker.tokens_string(), "RD");
}
