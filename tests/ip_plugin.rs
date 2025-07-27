use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::ip::IpPlugin;

#[test]
fn search_returns_addresses() {
    let plugin = IpPlugin;
    let results = plugin.search("ip");
    if cfg!(target_os = "windows") {
        assert!(!results.is_empty());
    } else {
        assert!(results.is_empty());
    }
}
