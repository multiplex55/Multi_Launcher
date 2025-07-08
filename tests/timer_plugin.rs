use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timer::{TimerPlugin, ACTIVE_TIMERS, TimerEntry};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn search_timer_returns_start_action() {
    let plugin = TimerPlugin;
    let results = plugin.search("timer 1s");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:"));
}

#[test]
fn search_named_timer() {
    let plugin = TimerPlugin;
    let results = plugin.search("timer 1s tea");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1s|"));
}

#[test]
fn search_cancel_lists_timers() {
    // manually insert an active timer
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry { id: 1, label: "test".into(), cancel: Arc::new(AtomicBool::new(false)) });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer cancel");
    assert!(results.iter().any(|a| a.action.starts_with("timer:cancel:")));
    // clear list
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_list_lists_timers() {
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry { id: 2, label: "demo".into(), cancel: Arc::new(AtomicBool::new(false)) });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer list");
    assert!(results.iter().any(|a| a.action.starts_with("timer:show:")));
    ACTIVE_TIMERS.lock().unwrap().clear();
}

