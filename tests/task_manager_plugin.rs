use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::task_manager::TaskManagerPlugin;

#[test]
fn search_returns_action() {
    let plugin = TaskManagerPlugin;
    let results = plugin.search("tm");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "shell:taskmgr");
}

#[test]
fn search_has_metadata() {
    let plugin = TaskManagerPlugin;
    let results = plugin.search("tm");
    assert!(results[0].preview_text.is_some());
    assert!(results[0].risk_level.is_some());
    assert!(results[0].icon.is_some());
}
