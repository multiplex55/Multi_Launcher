use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timer::{ACTIVE_TIMERS, TimerEntry, TimerPlugin};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    let results = plugin.search("timer add 1s");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:"));
}

#[test]
fn search_named_timer() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer add 1s tea");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1s|"));
}

#[test]
fn search_cancel_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    // manually insert an active timer
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            1,
            TimerEntry {
                id: 1,
                label: "test".into(),
                deadline: Instant::now() + Duration::from_secs(10),
                persist: false,
                end_ts: 0,
                start_ts: 0,
                paused: false,
                remaining: Duration::from_secs(10),
                generation: 0,
                sound: "None".into(),
            },
        );
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer cancel");
    assert!(
        results
            .iter()
            .any(|a| a.action.starts_with("timer:cancel:"))
    );
    // clear list
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_list_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            2,
            TimerEntry {
                id: 2,
                label: "demo".into(),
                deadline: Instant::now() + Duration::from_secs(20),
                persist: false,
                end_ts: 0,
                start_ts: 0,
                paused: false,
                remaining: Duration::from_secs(20),
                generation: 0,
                sound: "None".into(),
            },
        );
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer list");
    assert!(results.iter().any(|a| a.action.starts_with("timer:show:")));
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_rm_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            3,
            TimerEntry {
                id: 3,
                label: "remove".into(),
                deadline: Instant::now() + Duration::from_secs(30),
                persist: false,
                end_ts: 0,
                start_ts: 0,
                paused: false,
                remaining: Duration::from_secs(30),
                generation: 0,
                sound: "None".into(),
            },
        );
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer rm");
    assert!(
        results
            .iter()
            .any(|a| a.action.starts_with("timer:cancel:"))
    );
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_pause_timer_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer pause 5");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "timer:pause:5");
}

#[test]
fn search_pause_lists_running_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            11,
            TimerEntry {
                id: 11,
                label: "run".into(),
                deadline: Instant::now() + Duration::from_secs(5),
                persist: false,
                end_ts: 0,
                start_ts: 0,
                paused: false,
                remaining: Duration::from_secs(5),
                generation: 0,
                sound: "None".into(),
            },
        );
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer pause");
    assert!(results.iter().any(|a| a.action == "timer:pause:11"));
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_resume_timer_action() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;
    let results = plugin.search("timer resume 7");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "timer:resume:7");
}

#[test]
fn search_resume_lists_paused_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(
            12,
            TimerEntry {
                id: 12,
                label: "pause".into(),
                deadline: Instant::now() + Duration::from_secs(5),
                persist: false,
                end_ts: 0,
                start_ts: 0,
                paused: true,
                remaining: Duration::from_secs(5),
                generation: 0,
                sound: "None".into(),
            },
        );
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer resume");
    assert!(results.iter().any(|a| a.action == "timer:resume:12"));
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
    let results = plugin.search("timer add 1:30");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1:30"));
}

#[test]
fn format_ts_invalid_timestamp() {
    use chrono::{Local, TimeZone};
    use multi_launcher::plugins::timer::format_ts;
    let invalid_ts = 10_000_000_000_000u64;
    let expected = Local
        .timestamp_opt(0, 0)
        .single()
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    assert_eq!(format_ts(invalid_ts), expected);
}

#[test]
fn search_alarm_keeps_date_prefix_and_optional_name_together() {
    let _lock = TEST_MUTEX.lock().unwrap();
    let plugin = TimerPlugin;

    for (query, expected) in [
        (
            "alarm 2d 07:30 morning meeting",
            "alarm:set:2d 07:30|morning meeting",
        ),
        (
            "alarm 2024-01-31 05:15 early shift",
            "alarm:set:2024-01-31 05:15|early shift",
        ),
        ("alarm 07:30 plain name", "alarm:set:07:30|plain name"),
    ] {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1, "query: {query}");
        assert_eq!(results[0].action, expected);
    }
    assert!(plugin.search("alarm 2d invalid name").is_empty());
}
