use multi_launcher::hotkey::{parse_hotkey, HotkeyTrigger, process_test_events};
use multi_launcher::visibility::handle_visibility_trigger;
use rdev::{EventType, Key};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[test]
fn launcher_and_quit_hotkeys_toggle_flags() {
    let launcher_hotkey = parse_hotkey("F2").unwrap();
    let quit_hotkey = parse_hotkey("Ctrl+Q").unwrap();

    let launcher_trigger = Arc::new(HotkeyTrigger::new(launcher_hotkey));
    let quit_trigger = Arc::new(HotkeyTrigger::new(quit_hotkey));

    let triggers = vec![launcher_trigger.clone(), quit_trigger.clone()];

    let events = vec![
        EventType::KeyPress(Key::F2),
        EventType::KeyRelease(Key::F2),
        EventType::KeyPress(Key::ControlLeft),
        EventType::KeyPress(Key::KeyQ),
        EventType::KeyRelease(Key::KeyQ),
        EventType::KeyRelease(Key::ControlLeft),
    ];

    process_test_events(&triggers, &events);

    assert!(launcher_trigger.take());
    assert!(quit_trigger.take());
}

#[test]
fn zero_key_events_toggle_visibility() {
    let zero_hotkey = parse_hotkey("0").unwrap();
    let trigger = Arc::new(HotkeyTrigger::new(zero_hotkey));

    let triggers = vec![trigger.clone()];
    let events = vec![
        EventType::KeyPress(Key::Num0),
        EventType::KeyRelease(Key::Num0),
    ];

    process_test_events(&triggers, &events);

    let visibility = Arc::new(AtomicBool::new(false));

    handle_visibility_trigger(&trigger, &visibility);
    assert_eq!(visibility.load(Ordering::SeqCst), true);
}
