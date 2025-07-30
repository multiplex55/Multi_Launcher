use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::processes::ProcessesPlugin;

#[test]
fn prefix_ps_returns_both_actions() {
    let plugin = ProcessesPlugin;
    let results = plugin.search("ps");
    assert!(results.iter().any(|a| a.action.starts_with("process:switch:")));
    assert!(results.iter().any(|a| a.action.starts_with("process:kill:")));
}

#[test]
fn prefix_psk_returns_only_kill() {
    let plugin = ProcessesPlugin;
    let results = plugin.search("psk");
    assert!(!results.is_empty());
    assert!(results.iter().all(|a| a.action.starts_with("process:kill:")));
    assert!(!results.iter().any(|a| a.action.starts_with("process:switch:")));
}

#[test]
fn prefix_pss_returns_only_switch() {
    let plugin = ProcessesPlugin;
    let results = plugin.search("pss");
    assert!(!results.is_empty());
    assert!(results.iter().all(|a| a.action.starts_with("process:switch:")));
    assert!(!results.iter().any(|a| a.action.starts_with("process:kill:")));
}
