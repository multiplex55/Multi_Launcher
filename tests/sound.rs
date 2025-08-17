use multi_launcher::sound::{play_sound, SOUND_NAMES};

#[test]
fn sound_names_contains_expected() {
    assert!(SOUND_NAMES.contains(&"None"));
    assert!(SOUND_NAMES.contains(&"Alarm.wav"));
}

#[cfg(not(target_os = "windows"))]
#[test]
fn play_sound_returns_quickly_and_no_panic() {
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let result_none = std::panic::catch_unwind(|| play_sound("None"));
    assert!(result_none.is_ok());
    assert!(start.elapsed() < Duration::from_millis(100));

    let start_invalid = Instant::now();
    let result_invalid = std::panic::catch_unwind(|| play_sound("invalid"));
    assert!(result_invalid.is_ok());
    assert!(start_invalid.elapsed() < Duration::from_millis(100));
}
