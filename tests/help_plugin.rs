use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::help::HelpPlugin;

#[test]
fn search_returns_help_action() {
    let plugin = HelpPlugin;
    let results = plugin.search("help");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "help:show");
}
