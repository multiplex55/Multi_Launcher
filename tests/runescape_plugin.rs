use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::runescape::RunescapeSearchPlugin;

#[test]
fn rs_search_returns_action() {
    let plugin = RunescapeSearchPlugin;
    let results = plugin.search("rs boots of lightness");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].action,
        "https://runescape.wiki/?search=boots%20of%20lightness"
    );
}

#[test]
fn osrs_search_returns_action() {
    let plugin = RunescapeSearchPlugin;
    let results = plugin.search("osrs agility");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "https://oldschool.runescape.wiki/?search=agility");
}
