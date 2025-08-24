use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::windows::WindowsPlugin;

#[test]
fn search_lists_windows() {
    let plugin = WindowsPlugin;
    let results = plugin.search("win");
    assert!(results.iter().any(|a| a.action.starts_with("window:switch:")));
    assert!(results.iter().any(|a| a.action.starts_with("window:close:")));
}
