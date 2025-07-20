use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::network::NetworkPlugin;

#[test]
fn search_returns_actions() {
    let plugin = NetworkPlugin;
    let results = plugin.search("net");
    assert!(!results.is_empty());
}
