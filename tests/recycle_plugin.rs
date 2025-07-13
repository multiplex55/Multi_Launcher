use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::recycle::RecyclePlugin;

#[test]
fn search_returns_action() {
    let plugin = RecyclePlugin;
    let results = plugin.search("rec");
    if cfg!(target_os = "windows") {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "Clean Recycle Bin");
        assert_eq!(results[0].action, "recycle:clean");
    } else {
        assert!(results.is_empty());
    }
}
