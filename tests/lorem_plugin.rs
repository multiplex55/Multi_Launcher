use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::lorem::LoremPlugin;

#[test]
fn generate_words() {
    let plugin = LoremPlugin;
    let results = plugin.search("lorem w 5");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("clipboard:"));
}

#[test]
fn generate_sentences() {
    let plugin = LoremPlugin;
    let results = plugin.search("lorem s 2");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("clipboard:"));
}

#[test]
fn generate_paragraphs() {
    let plugin = LoremPlugin;
    let results = plugin.search("lorem p 1");
    assert_eq!(results.len(), 1);
    assert!(results[0].action.starts_with("clipboard:"));
}
