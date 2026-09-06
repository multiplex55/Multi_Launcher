use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::network::NetworkPlugin;
use std::{thread, time::Duration};

#[test]
fn search_returns_actions() {
    let plugin = NetworkPlugin::default();
    thread::sleep(Duration::from_millis(10));
    let results = plugin.search("net");
    assert!(!results.is_empty());
    assert!(results[0].label.contains("AvgRx"));
}

#[test]
fn search_no_panic_with_empty_history() {
    let plugin = NetworkPlugin::default();
    plugin.clear_history();
    thread::sleep(Duration::from_millis(10));
    plugin.search("net");
}
