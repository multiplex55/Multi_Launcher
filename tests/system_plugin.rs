use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::system::SystemPlugin;

#[test]
fn search_shutdown_returns_action() {
    let plugin = SystemPlugin;
    let results = plugin.search("sys shutdown");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "system:shutdown");
}
