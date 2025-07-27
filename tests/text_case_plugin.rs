use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::text_case::TextCasePlugin;

#[test]
fn converts_text_cases() {
    let plugin = TextCasePlugin;
    let results = plugin.search("case Rust Test");
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].label, "RUST TEST");
    assert_eq!(results[0].action, "clipboard:RUST TEST");
    assert_eq!(results[1].label, "rust test");
    assert_eq!(results[1].action, "clipboard:rust test");
    assert_eq!(results[2].label, "Rust Test");
    assert_eq!(results[2].action, "clipboard:Rust Test");
    assert_eq!(results[3].label, "rust_test");
    assert_eq!(results[3].action, "clipboard:rust_test");
}
