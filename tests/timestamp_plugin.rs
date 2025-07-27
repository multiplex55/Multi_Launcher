use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::timestamp::TimestampPlugin;
use chrono::{Local, TimeZone};

#[test]
fn unix_to_date() {
    let plugin = TimestampPlugin;
    let results = plugin.search("ts 0");
    let expected = Local
        .timestamp_opt(0, 0)
        .single()
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, expected);
    assert_eq!(results[0].action, format!("clipboard:{expected}"));
}

#[test]
fn date_to_unix() {
    let plugin = TimestampPlugin;
    let query = "ts 2024-05-01 12:00";
    let dt = Local
        .with_ymd_and_hms(2024, 5, 1, 12, 0, 0)
        .unwrap();
    let ts = dt.timestamp().to_string();
    let results = plugin.search(query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, ts);
    assert_eq!(results[0].action, format!("clipboard:{}", ts));
}

#[test]
fn ms_to_time() {
    let plugin = TimestampPlugin;
    let results = plugin.search("tsm 3600000");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "01:00:00");
    assert_eq!(results[0].action, "clipboard:01:00:00");
}

#[test]
fn time_to_ms() {
    let plugin = TimestampPlugin;
    let results = plugin.search("tsm 01:00");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "3600000");
    assert_eq!(results[0].action, "clipboard:3600000");
}

#[test]
fn time_with_ms_to_ms() {
    let plugin = TimestampPlugin;
    let results = plugin.search("tsm 01:00:00.500");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "3600500");
    assert_eq!(results[0].action, "clipboard:3600500");
}
