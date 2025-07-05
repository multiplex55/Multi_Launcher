use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[test]
fn queued_visibility_applies_when_context_available() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));

    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    handle_visibility_trigger(&trigger, &visibility);

    assert_eq!(visibility.load(Ordering::SeqCst), true);
}

