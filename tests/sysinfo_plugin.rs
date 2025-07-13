use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::sysinfo::SysInfoPlugin;

#[test]
fn search_info_returns_actions() {
    let plugin = SysInfoPlugin;
    let results = plugin.search("info");
    assert!(!results.is_empty());
}

#[test]
fn search_cpu_returns_action() {
    let plugin = SysInfoPlugin;
    let results = plugin.search("info cpu");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_mem_returns_action() {
    let plugin = SysInfoPlugin;
    let results = plugin.search("info mem");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_disk_returns_action() {
    let plugin = SysInfoPlugin;
    let results = plugin.search("info disk");
    assert_eq!(results.len(), 1);
}
