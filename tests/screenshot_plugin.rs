use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::screenshot::ScreenshotPlugin;

#[test]
fn search_lists_screenshot_actions() {
    let plugin = ScreenshotPlugin;
    let results = plugin.search("ss");
    assert!(!results.is_empty());
    let prefixes = [
        "screenshot:window",
        "screenshot:region",
        "screenshot:desktop",
        "screenshot:window_clip",
        "screenshot:region_clip",
        "screenshot:desktop_clip",
    ];
    for prefix in prefixes.iter() {
        assert!(results.iter().any(|a| a.action == *prefix), "missing action {}", prefix);
    }
}
