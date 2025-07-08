use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::wikipedia::WikipediaPlugin;

#[test]
fn search_returns_action() {
    let plugin = WikipediaPlugin;
    let results = plugin.search("wiki space");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "https://en.wikipedia.org/wiki/Special:Search?search=space");
}
