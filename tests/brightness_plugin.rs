use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::brightness::BrightnessPlugin;

#[test]
fn search_set_numeric() {
    let plugin = BrightnessPlugin;
    let results = plugin.search("bright 50");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "brightness:set:50");
}

#[test]
fn search_plain_bright() {
    let plugin = BrightnessPlugin;
    let results = plugin.search("bright");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "brightness:dialog");
}
