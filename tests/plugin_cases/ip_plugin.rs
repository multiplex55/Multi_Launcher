use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::ip::IpPlugin;

#[test]
fn non_ip_search_is_empty_without_starting_public_lookup() {
    let plugin = IpPlugin::default();
    let results = plugin.search("unrelated");
    assert!(results.is_empty());
}
