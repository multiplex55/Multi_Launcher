use multi_launcher::actions::Action;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::text_case::TextCasePlugin;

fn action_view(action: &Action) -> (&str, &str, &str) {
    (&action.label, &action.desc, &action.action)
}

#[test]
fn default_case_rust_test_results_are_frozen_in_order() {
    let plugin = TextCasePlugin;
    let results = plugin.search("case Rust Test");

    let expected = [
        ("RUST TEST", "Text Case-Uppercase", "clipboard:RUST TEST"),
        ("rust test", "Text Case-Lowercase", "clipboard:rust test"),
        ("Rust Test", "Text Case-Capitalized", "clipboard:Rust Test"),
        ("rustTest", "Text Case-Camel", "clipboard:rustTest"),
        ("RustTest", "Text Case-Pascal", "clipboard:RustTest"),
        ("rust_test", "Text Case-Snake", "clipboard:rust_test"),
        ("RUST_TEST", "Text Case-Screaming", "clipboard:RUST_TEST"),
        ("rust-test", "Text Case-Kebab", "clipboard:rust-test"),
        ("Rust-Test", "Text Case-Train", "clipboard:Rust-Test"),
        ("rust.test", "Text Case-Dot", "clipboard:rust.test"),
        ("RuSt TeSt", "Text Case-Alternating", "clipboard:RuSt TeSt"),
        ("rUsT tEsT", "Text Case-Mocking", "clipboard:rUsT tEsT"),
        ("rUST tEST", "Text Case-Inverse", "clipboard:rUST tEST"),
        ("tseT tsuR", "Text Case-Backwards", "clipboard:tseT tsuR"),
        ("RT", "Text Case-Acronym", "clipboard:RT"),
        ("R. T.", "Text Case-Initials", "clipboard:R. T."),
        ("Rust Test", "Text Case-Title", "clipboard:Rust Test"),
        ("Rust test", "Text Case-Sentence", "clipboard:Rust test"),
        ("UnVzdCBUZXN0", "Text Case-Base64", "clipboard:UnVzdCBUZXN0"),
        (
            "527573742054657374",
            "Text Case-Hex",
            "clipboard:527573742054657374",
        ),
        (
            "01010010 01110101 01110011 01110100 00100000 01010100 01100101 01110011 01110100",
            "Text Case-Binary",
            "clipboard:01010010 01110101 01110011 01110100 00100000 01010100 01100101 01110011 01110100",
        ),
        ("Ehfg Grfg", "Text Case-ROT13", "clipboard:Ehfg Grfg"),
        ("rust 👏 test", "Text Case-Clap", "clipboard:rust 👏 test"),
        ("Rust Test", "Text Case-Emoji", "clipboard:Rust Test"),
        ("R-u-s-t T-e-s-t", "Text Case-Custom", "clipboard:R-u-s-t T-e-s-t"),
        (
            ".-. ..- ... - / - . ... -",
            "Text Case-Morse",
            "clipboard:.-. ..- ... - / - . ... -",
        ),
    ];

    assert_eq!(results.len(), 26);
    for (index, expected_action) in expected.iter().enumerate() {
        assert_eq!(
            action_view(&results[index]),
            *expected_action,
            "unexpected result at index {index}"
        );
    }
}

#[test]
fn specific_operation_queries_return_only_requested_operation() {
    let plugin = TextCasePlugin;
    let cases = [
        (
            "case upper Rust Test",
            ("RUST TEST", "Text Case-Uppercase", "clipboard:RUST TEST"),
        ),
        (
            "case title use the following API",
            (
                "Use the Following Api",
                "Text Case-Title",
                "clipboard:Use the Following Api",
            ),
        ),
        (
            "case screaming Rust Test",
            ("RUST_TEST", "Text Case-Screaming", "clipboard:RUST_TEST"),
        ),
        (
            "case base64 Rust",
            ("UnVzdA==", "Text Case-Base64", "clipboard:UnVzdA=="),
        ),
        (
            "case hex Rust",
            ("52757374", "Text Case-Hex", "clipboard:52757374"),
        ),
        (
            "case binary A",
            ("01000001", "Text Case-Binary", "clipboard:01000001"),
        ),
    ];

    for (query, expected) in cases {
        let results = plugin.search(query);
        assert_eq!(results.len(), 1, "{query}");
        assert_eq!(action_view(&results[0]), expected, "{query}");
    }
}

#[test]
fn text_case_behavior_regressions() {
    let plugin = TextCasePlugin;
    let cases = [
        ("case", Vec::<(&str, &str)>::new()),
        (
            "case     Rust     Test  ",
            vec![("Text Case-Snake", "rust_test")],
        ),
        (
            "case hello, world!",
            vec![("Text Case-Title", "Hello, World!")],
        ),
        ("case rUsT TeSt", vec![("Text Case-Inverse", "RuSt tEsT")]),
        ("case café мир", vec![("Text Case-Uppercase", "CAFÉ МИР")]),
        (
            "case title the lord of the rings and to mars",
            vec![("Text Case-Title", "The Lord of the Rings and to Mars")],
        ),
    ];

    for (query, expected_matches) in cases {
        let results = plugin.search(query);
        if expected_matches.is_empty() {
            assert!(results.is_empty(), "{query}");
            continue;
        }
        for (desc, label) in expected_matches {
            let result = results
                .iter()
                .find(|action| action.desc == desc)
                .unwrap_or_else(|| panic!("missing {desc} for {query}"));
            assert_eq!(result.label, label, "{query}");
            assert_eq!(result.action, format!("clipboard:{label}"), "{query}");
        }
    }
}
