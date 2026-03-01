use std::time::{Duration, Instant};

use anyhow::anyhow;
use multi_launcher::actions::screenshot::{
    is_cancel_or_timeout, wait_for_new_clipboard_image, ClipboardSnapshot, ScreenshotCaptureError,
};

#[test]
fn cancel_classifier_detects_expected_errors() {
    let cancelled = anyhow!(ScreenshotCaptureError::RegionSelectionCancelled);
    let timed_out = anyhow!(ScreenshotCaptureError::RegionSelectionTimedOut);
    let other = anyhow!("some other failure");

    assert!(is_cancel_or_timeout(&cancelled));
    assert!(is_cancel_or_timeout(&timed_out));
    assert!(!is_cancel_or_timeout(&other));
}

#[test]
fn wait_for_new_clipboard_image_detects_changed_snapshot() {
    let old = Some(ClipboardSnapshot {
        width: 1,
        height: 1,
        bytes: vec![0, 0, 0, 255],
    });
    let mut calls = 0;
    let img = wait_for_new_clipboard_image(
        || {
            calls += 1;
            if calls == 1 {
                old.clone()
            } else {
                Some(ClipboardSnapshot {
                    width: 1,
                    height: 1,
                    bytes: vec![255, 255, 255, 255],
                })
            }
        },
        old,
        Duration::from_millis(20),
        Duration::from_millis(0),
    )
    .expect("new image should be returned");

    assert_eq!(img.bytes, vec![255, 255, 255, 255]);
}

#[test]
fn wait_for_new_clipboard_image_times_out_quickly_with_cancel_error() {
    let start = Instant::now();
    let err = wait_for_new_clipboard_image(
        || None,
        None,
        Duration::from_millis(3),
        Duration::from_millis(0),
    )
    .expect_err("expected cancellation error when no new image appears");

    assert!(is_cancel_or_timeout(&err));
    assert!(start.elapsed() < Duration::from_millis(100));
}
