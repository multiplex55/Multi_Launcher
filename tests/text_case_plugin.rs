use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::text_case::TextCasePlugin;

#[test]
fn converts_text_cases() {
    let plugin = TextCasePlugin;
    let results = plugin.search("case Rust Test");
    assert_eq!(results.len(), 26);
    // original cases
    assert_eq!(results[0].label, "RUST TEST");
    assert_eq!(results[0].action, "clipboard:RUST TEST");
    assert_eq!(results[1].label, "rust test");
    assert_eq!(results[1].action, "clipboard:rust test");
    assert_eq!(results[2].label, "Rust Test");
    assert_eq!(results[2].action, "clipboard:Rust Test");
    assert_eq!(results[5].label, "rust_test");
    assert_eq!(results[5].action, "clipboard:rust_test");
    assert_eq!(results[3].label, "rustTest"); // camelCase
    assert_eq!(results[4].label, "RustTest"); // PascalCase
    assert_eq!(results[6].label, "RUST_TEST"); // SCREAMING_SNAKE_CASE
    assert_eq!(results[7].label, "rust-test"); // kebab-case
    assert_eq!(results[9].label, "rust.test"); // dot.case
    assert_eq!(results[10].label, "RuSt TeSt"); // Alternating case
    assert_eq!(results[11].label, "rUsT tEsT"); // Mocking SpongeBob
    assert_eq!(results[12].label, "rUST tEST"); // Inverse case
    assert_eq!(results[13].label, "tseT tsuR"); // Backwards case
    assert_eq!(results[14].label, "RT"); // Acronym
    assert_eq!(results[15].label, "R. T."); // Initial Caps
    assert_eq!(results[17].label, "Rust test"); // Sentence case
    assert_eq!(results[18].label, "UnVzdCBUZXN0"); // Base64
    assert_eq!(results[19].label, "527573742054657374"); // Hex
    assert_eq!(results[21].label, "Ehfg Grfg"); // ROT13
    assert_eq!(results[22].label, "rust 👏 test"); // Clap case
    assert_eq!(results[24].label, "R-u-s-t T-e-s-t"); // Custom delimiter
}

#[test]
fn converts_specific_cases() {
    let plugin = TextCasePlugin;
    let hex = plugin.search("case hex Rust");
    assert_eq!(hex.len(), 1);
    assert_eq!(hex[0].label, "52757374");

    let bin = plugin.search("case binary A");
    assert_eq!(bin.len(), 1);
    assert_eq!(bin[0].label, "01000001");
}

