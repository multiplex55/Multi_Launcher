use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::handle_visibility_trigger;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

#[test]
fn visibility_flag_toggles_on_trigger() {
    let trigger = HotkeyTrigger::new(Hotkey::default());
    let visibility = Arc::new(AtomicBool::new(false));

    // simulate hotkey press
    *trigger.open.lock().unwrap() = true;

    handle_visibility_trigger(&trigger, &visibility);

    assert_eq!(visibility.load(Ordering::SeqCst), true);
}
