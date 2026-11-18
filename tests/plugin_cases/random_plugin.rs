use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::random::RandomPlugin;

#[test]
fn generate_number() {
    let plugin = RandomPlugin::from_seed(1);
    let results = plugin.search("rand number 10");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "10");
    assert_eq!(results[0].action, "clipboard:10");
}

#[test]
fn password_search_is_options_only_and_preserves_rng() {
    let plugin = RandomPlugin::from_seed(1);
    for query in ["rand pw 8", "random password 8"] {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "Generate and copy password");
        assert_eq!(results[0].desc, "Password length: 8");
        assert_eq!(results[0].action, "password:copy:8");
        assert!(results[0].args.is_none());
    }
    for query in [
        "rand pw 0",
        "rand pw 7",
        "rand pw 257",
        "rand pw +8",
        "rand pw 8 extra",
    ] {
        assert!(plugin.search(query).is_empty());
    }
    assert_eq!(plugin.search("rand pw 256")[0].action, "password:copy:256");
    assert!(
        plugin
            .commands()
            .iter()
            .any(|item| item.action == "query:rand pw ")
    );
    let untouched = RandomPlugin::from_seed(1);
    assert_eq!(
        plugin.search("rand dice")[0].action,
        untouched.search("rand dice")[0].action
    );
    assert_eq!(
        plugin.search("random number 100")[0].action,
        untouched.search("rand number 100")[0].action
    );
}
