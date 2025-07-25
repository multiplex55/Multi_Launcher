use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timer::{
    cancel_timer, load_saved_alarms, reset_alarms_loaded, TimerEntry, TimerPlugin, ACTIVE_TIMERS,
    ALARMS_FILE,
};
use once_cell::sync::Lazy;
use std::sync::Mutex;
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
        list.insert(1, TimerEntry {
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
        });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer cancel");
    assert!(results
        .iter()
        .any(|a| a.action.starts_with("timer:cancel:")));
    // clear list
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn search_list_lists_timers() {
    let _lock = TEST_MUTEX.lock().unwrap();
    {
        let mut list = ACTIVE_TIMERS.lock().unwrap();
        list.insert(2, TimerEntry {
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
        });
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
        list.insert(3, TimerEntry {
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
        });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer rm");
    assert!(results
        .iter()
        .any(|a| a.action.starts_with("timer:cancel:")));
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
        list.insert(11, TimerEntry {
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
        });
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
        list.insert(12, TimerEntry {
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
        });
    }
    let plugin = TimerPlugin;
    let results = plugin.search("timer resume");
    assert!(results.iter().any(|a| a.action == "timer:resume:12"));
    ACTIVE_TIMERS.lock().unwrap().clear();
}

#[test]
fn take_finished_returns_messages() {
    let _lock = TEST_MUTEX.lock().unwrap();
    use multi_launcher::plugins::timer::{take_finished_messages, FINISHED_MESSAGES};
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
        .values()
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
    let results = plugin.search("timer add 1:30");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("timer:start:1:30"));
}

#[test]
fn format_ts_invalid_timestamp() {
    use multi_launcher::plugins::timer::format_ts;
    use chrono::{Local, TimeZone};
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
fn pause_resume_does_not_grow_heap() {
    use multi_launcher::plugins::timer::{
        cancel_timer, heap_len, pause_timer, resume_timer, start_timer, ACTIVE_TIMERS,
    };
    let _lock = TEST_MUTEX.lock().unwrap();
    ACTIVE_TIMERS.lock().unwrap().clear();

    start_timer(Duration::from_secs(3600), "None".into());
    let id = ACTIVE_TIMERS.lock().unwrap().keys().cloned().next().unwrap();
    assert_eq!(heap_len(), 1);

    for _ in 0..5 {
        pause_timer(id);
        assert_eq!(heap_len(), 0);
        resume_timer(id);
        assert_eq!(heap_len(), 1);
    }

    cancel_timer(id);
    assert_eq!(heap_len(), 0);
}

#[test]
fn parse_hhmm_with_day_offset() {
    use multi_launcher::plugins::timer::parse_hhmm;
    use chrono::{Local, Duration as ChronoDuration};
    let (h, m, date) = parse_hhmm("2d 07:30").unwrap();
    assert_eq!((h, m), (7, 30));
    let expected = Local::now().date_naive() + ChronoDuration::days(2);
    assert_eq!(date.unwrap(), expected);
}

#[test]
fn parse_hhmm_with_absolute_date() {
    use multi_launcher::plugins::timer::parse_hhmm;
    use chrono::NaiveDate;
    let (h, m, date) = parse_hhmm("2024-01-31 05:15").unwrap();
    assert_eq!((h, m), (5, 15));
    assert_eq!(date.unwrap(), NaiveDate::from_ymd_opt(2024, 1, 31).unwrap());
}

#[test]
fn start_alarm_multi_day() {
    use multi_launcher::plugins::timer::{
        cancel_timer, start_alarm_named, ACTIVE_TIMERS,
    };
    use chrono::{Local, Duration as ChronoDuration, Timelike};
    let _lock = TEST_MUTEX.lock().unwrap();
    ACTIVE_TIMERS.lock().unwrap().clear();

    let now = Local::now();
    let date = now.date_naive() + ChronoDuration::days(2);
    let hour = now.hour();
    let minute = now.minute();

    start_alarm_named(hour, minute, Some(date), Some("test".into()), "None".into());
    let (id, remaining) = {
        let list = ACTIVE_TIMERS.lock().unwrap();
        assert_eq!(list.len(), 1);
        let t = list.values().next().unwrap();
        (t.id, t.deadline.saturating_duration_since(std::time::Instant::now()).as_secs())
    };

    let expected = (date.and_hms_opt(hour, minute, 0).unwrap() - now.naive_local()).num_seconds() as u64;
    assert!((remaining as i64 - expected as i64).abs() <= 2);
    cancel_timer(id);
}
