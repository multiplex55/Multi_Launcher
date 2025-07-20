use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::network::NetworkPlugin;
use std::{thread, time::Duration};

#[test]
fn search_returns_actions() {
    let plugin = NetworkPlugin::default();
    thread::sleep(Duration::from_millis(10));
    let results = plugin.search("net");
    assert!(!results.is_empty());
}
