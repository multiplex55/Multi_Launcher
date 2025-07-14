use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timer::{
    TimerPlugin, ACTIVE_TIMERS, TimerEntry, ALARMS_FILE, load_saved_alarms,
    reset_alarms_loaded, cancel_timer,
};
use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use std::time::{Duration, Instant, SystemTime};
use tempfile::tempdir;

static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[test]
fn search_timer_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "timer:dialog:timer");
}

#[test]
fn search_alarm_dialog() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("alarm");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "timer:dialog:alarm");
}

#[test]
fn search_timer_returns_start_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer 1s");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:"));
}

#[test]
fn search_named_timer() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer 1s tea");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1s|"));
}

#[test]
fn search_cancel_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    // manually insert an active timer
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry {
            id: 1,
            label: "test".into(),
            deadline: Instant::now() + Duration::from_secs(10),
            cancel: Arc::new(AtomicBool::new(false)),
            persist: false,
            end_ts: 0,
        });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer cancel");
    assert!(results.iter().any(|a| a.action.starts_with("timer:cancel:")));
    // clear list
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_list_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.push(TimerEntry {
            id: 2,
            label: "demo".into(),
            deadline: Instant::now() + Duration::from_secs(20),
            cancel: Arc::new(AtomicBool::new(false)),
            persist: false,
            end_ts: 0,
        });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer list");
    assert!(results.iter().any(|a| a.action.starts_with("timer:show:")));
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn take_finished_returns_messages() {
    let _lock = TEST_MUTEX.lock().unwrap();
    use multi_launcher::plugins::timer::{FINISHED_MESSAGES, take_finished_messages};
    FINISHED_MESSAGES.lock().unwrap().push("done".to_string());
    let msgs = take_finished_messages();
    assert_eq!(msgs, vec!["done".to_string()]);
    assert!(take_finished_messages().is_empty());
}

#[test]
fn load_saved_alarms_is_idempotent() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let json = format!("[{{\"label\":\"demo\",\"end_ts\":{}}}]", now + 3600);
    std::fs::write(ALARMS_FILE, json).unwrap();

    reset_alarms_loaded();
    ACTIVE_TIMERS.lock().unwrap().clear();

    load_saved_alarms();
    let first = ACTIVE_TIMERS.lock().unwrap().len();
    load_saved_alarms();
    let second = ACTIVE_TIMERS.lock().unwrap().len();

    assert_eq!(first, 1);
    assert_eq!(second, 1);

    let ids: Vec<u64> = ACTIVE_TIMERS
        .lock()
        .unwrap()
        .iter()
        .map(|t| t.id)
        .collect();
    for id in ids {
        cancel_timer(id);
    }
}

#[test]
fn parse_duration_colon_formats() {
    use multi_launcher::plugins::timer::parse_duration;
    let d = parse_duration("1:02").unwrap();
    assert_eq!(d, Duration::from_secs(62));
    let d = parse_duration("1:02:03").unwrap();
    assert_eq!(d, Duration::from_secs(3723));
}

#[test]
fn search_timer_hms_format() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer 1:30");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1:30"));
}

