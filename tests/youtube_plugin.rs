use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::youtube::YoutubePlugin;

#[test]
fn search_returns_action() {
    let plugin = YoutubePlugin;
    let results = plugin.search("yt rust");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].action,
        "https://www.youtube.com/results?search_query=rust"
    );
}

#[test]
fn search_encodes_spaces() {
    let plugin = YoutubePlugin;
    let results = plugin.search("yt rust lang");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].action,
        "https://www.youtube.com/results?search_query=rust%20lang"
    );
}

#[test]
fn search_encodes_special_chars() {
    let plugin = YoutubePlugin;
    let results = plugin.search("yt rust & borrow");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].action,
        "https://www.youtube.com/results?search_query=rust%20%26%20borrow"
    );
}
