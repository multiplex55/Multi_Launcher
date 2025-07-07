use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::processes::ProcessesPlugin;

#[test]
fn search_returns_processes() {
    let plugin = ProcessesPlugin;
    let results = plugin.search("ps");
    assert!(!results.is_empty());
}
