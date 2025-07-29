use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::random::RandomPlugin;

#[test]
fn generate_number() {
    let plugin = RandomPlugin::from_seed(1);
    let results = plugin.search("rand number 10");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "7");
    assert_eq!(results[0].action, "clipboard:7");
}

#[test]
fn generate_password() {
    let plugin = RandomPlugin::from_seed(1);
    let results = plugin.search("rand pw 8");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "0zsMbNLx");
    assert_eq!(results[0].action, "clipboard:0zsMbNLx");
}
