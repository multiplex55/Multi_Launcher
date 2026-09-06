use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::sysinfo::SysInfoPlugin;

#[test]
fn cold_info_search_does_not_return_fake_metrics() {
    let plugin = SysInfoPlugin::default();
    let results = plugin.search("info");
    assert!(
        results.is_empty(),
        "cold cache must not fabricate zero-valued metrics"
    );
}

#[test]
fn cold_cpu_search_does_not_return_fake_metric() {
    let plugin = SysInfoPlugin::default();
    let results = plugin.search("info cpu");
    assert!(
        results.is_empty(),
        "cold cache must not fabricate zero-valued metrics"
    );
}

#[test]
fn cold_mem_search_does_not_return_fake_metric() {
    let plugin = SysInfoPlugin::default();
    let results = plugin.search("info mem");
    assert!(
        results.is_empty(),
        "cold cache must not fabricate zero-valued metrics"
    );
}

#[test]
fn cold_disk_search_does_not_return_fake_metric() {
    let plugin = SysInfoPlugin::default();
    let results = plugin.search("info disk");
    assert!(
        results.is_empty(),
        "cold cache must not fabricate zero-valued metrics"
    );
}

#[test]
fn search_cpu_list_returns_action() {
    let plugin = SysInfoPlugin::default();
    let results = plugin.search("info cpu list 5");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "sysinfo:cpu_list:5");
}
