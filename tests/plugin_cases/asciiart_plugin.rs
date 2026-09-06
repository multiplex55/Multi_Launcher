use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::asciiart::AsciiArtPlugin;

#[test]
fn search_returns_multiline_action() {
    let plugin = AsciiArtPlugin::default();
    let results = plugin.search("ascii hi");
    assert_eq!(results.len(), 1);
    assert!(results[0].label.contains('\n'));
}
