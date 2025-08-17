use multi_launcher::actions::Action;
use multi_launcher::history::HistoryEntry;
use multi_launcher::history::{append_history, clear_history, get_history};
use tempfile::tempdir;

#[test]
fn clear_history_empties_file() {
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();

    let entry = HistoryEntry {
        query: "test".into(),
        query_lc: String::new(),
        action: Action {
            label: "l".into(),
            desc: "".into(),
            action: "run".into(),
            args: None,
        },
    };
    append_history(entry, 10).unwrap();
    assert!(!get_history().is_empty());

    clear_history().unwrap();
    assert!(get_history().is_empty());

    let content = std::fs::read_to_string(dir.path().join("history.json")).unwrap();
    assert_eq!(content.trim(), "[]");
}

#[test]
fn plugin_clear_action() {
    use multi_launcher::plugin::Plugin;
    use multi_launcher::plugins::history::HistoryPlugin;

    let plugin = HistoryPlugin;
    let results = plugin.search("hi clear");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "history:clear");
}
