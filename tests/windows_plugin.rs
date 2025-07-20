use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::windows::WindowsPlugin;

#[test]
fn search_lists_windows() {
    let plugin = WindowsPlugin;
    let results = plugin.search("win");
    if cfg!(target_os = "windows") {
        assert!(results.iter().any(|a| a.action.starts_with("window:switch:")));
        assert!(results.iter().any(|a| a.action.starts_with("window:close:")));
    } else {
        assert!(results.is_empty());
    }
}
