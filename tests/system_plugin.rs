use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::system::SystemPlugin;

#[test]
fn search_shutdown_returns_action() {
    let plugin = SystemPlugin;
    let results = plugin.search("sys shutdown");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "system:shutdown");
}

#[test]
fn search_shutdown_has_metadata() {
    let plugin = SystemPlugin;
    let results = plugin.search("sys shutdown");
    assert!(results[0].preview_text.is_some());
    assert!(results[0].risk_level.is_some());
    assert!(results[0].icon.is_some());
}
