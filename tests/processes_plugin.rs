use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::processes::ProcessesPlugin;

#[test]
fn search_returns_process_actions() {
    let plugin = ProcessesPlugin;
    let results = plugin.search("ps");
    assert!(results.iter().any(|a| a.action.starts_with("process:switch:")));
    assert!(results.iter().any(|a| a.action.starts_with("process:kill:")));
}
