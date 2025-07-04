use multi_launcher::hotkey::{Hotkey, HotkeyTrigger};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "linux")]
fn thread_count() -> usize {
    std::fs::read_dir("/proc/self/task").unwrap().count()
}

#[cfg(not(target_os = "linux"))]
fn thread_count() -> usize {
    // best effort for other platforms
    0
}

#[test]
fn restarting_listener_cleans_threads() {
    let trigger = Arc::new(HotkeyTrigger::new(Hotkey::default()));
    let triggers = vec![trigger];

    let base = thread_count();
    let mut listener = HotkeyTrigger::start_listener(triggers.clone(), "test1");
    thread::sleep(Duration::from_millis(50));
    listener.stop();
    thread::sleep(Duration::from_millis(50));
    let after_first = thread_count();
    assert_eq!(after_first, base, "listener thread should exit");

    let mut listener = HotkeyTrigger::start_listener(triggers, "test2");
    thread::sleep(Duration::from_millis(50));
    listener.stop();
    thread::sleep(Duration::from_millis(50));
    let after_second = thread_count();
    assert_eq!(after_second, base, "restarted listener should not leak threads");
}
