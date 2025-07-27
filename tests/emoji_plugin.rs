use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::emoji::EmojiPlugin;

#[test]
fn search_returns_clipboard_action() {
    let plugin = EmojiPlugin::default();
    let results = plugin.search("emoji smile");
    assert!(results.iter().any(|a| a.action.starts_with("clipboard:")));
}
