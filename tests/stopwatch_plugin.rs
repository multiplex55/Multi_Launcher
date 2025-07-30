use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::stopwatch::{start_stopwatch_named, stop_stopwatch, StopwatchPlugin};

#[test]
fn stopwatch_start_action() {
    let plugin = StopwatchPlugin::default();
    let actions = plugin.search("sw start test");
    assert_eq!(actions[0].action, "stopwatch:start:test");
}

#[test]
fn stopwatch_list_contains_started() {
    let id = start_stopwatch_named(Some("test".into()));
    let plugin = StopwatchPlugin::default();
    let actions = plugin.search("sw list");
    assert!(actions
        .iter()
        .any(|a| a.action == format!("stopwatch:show:{id}")));
    stop_stopwatch(id);
}
