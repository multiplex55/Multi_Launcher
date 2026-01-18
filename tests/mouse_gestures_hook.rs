#[cfg(windows)]
use multi_launcher::mouse_gestures::{should_ignore_event, MG_PASSTHROUGH_MARK};

#[cfg(windows)]
#[test]
fn passthrough_mark_is_ignored() {
    assert!(should_ignore_event(0, MG_PASSTHROUGH_MARK));
}

#[cfg(windows)]
#[test]
fn normal_event_is_not_ignored() {
    assert!(!should_ignore_event(0, 0));
}
