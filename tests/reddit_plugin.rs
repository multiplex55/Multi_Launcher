use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::reddit::RedditPlugin;

#[test]
fn search_returns_action() {
    let plugin = RedditPlugin;
    let results = plugin.search("red cats");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "https://www.reddit.com/search/?q=cats");
}
