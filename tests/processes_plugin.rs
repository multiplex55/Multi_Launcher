use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::processes::ProcessesPlugin;

#[test]
fn commands_preserve_process_query_prefixes() {
    let plugin = ProcessesPlugin::default();
    let actions: Vec<_> = plugin
        .commands()
        .into_iter()
        .map(|action| action.action)
        .collect();
    assert_eq!(actions, ["query:ps ", "query:psk ", "query:pss "]);
}

#[test]
fn unrelated_query_returns_immediately_without_results() {
    let plugin = ProcessesPlugin::default();
    assert!(plugin.search("unrelated").is_empty());
}
