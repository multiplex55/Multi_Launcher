use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use multi_launcher::visibility::start_hotkey_poller;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn poller_detects_trigger_without_consuming() {
    let trigger = Arc::new(HotkeyTrigger::new(Hotkey::default()));
    let mut poller = start_hotkey_poller(trigger.clone());
    *trigger.open.lock().unwrap() = true;
    for _ in 0..10 {
        if poller.ready() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(poller.ready());
    assert!(trigger.take());
    poller.stop();
}
