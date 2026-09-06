use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::media::MediaPlugin;

#[test]
fn search_play_returns_action() {
    let plugin = MediaPlugin;
    let results = plugin.search("media play");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "media:play");
}

#[test]
fn search_pause_returns_action() {
    let plugin = MediaPlugin;
    let results = plugin.search("media pause");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "media:pause");
}

#[test]
fn search_next_returns_action() {
    let plugin = MediaPlugin;
    let results = plugin.search("media next");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "media:next");
}

#[test]
fn search_prev_returns_action() {
    let plugin = MediaPlugin;
    let results = plugin.search("media prev");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "media:prev");
}

#[test]
fn search_plain_lists_all() {
    let plugin = MediaPlugin;
    let results = plugin.search("media");
    assert_eq!(results.len(), 4);
    assert!(results.iter().any(|a| a.action == "media:play"));
    assert!(results.iter().any(|a| a.action == "media:pause"));
    assert!(results.iter().any(|a| a.action == "media:next"));
    assert!(results.iter().any(|a| a.action == "media:prev"));
}
