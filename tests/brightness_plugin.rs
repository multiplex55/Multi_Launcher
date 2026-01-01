use multi_launcher::gui::BRIGHTNESS_QUERIES;
use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::brightness::BrightnessPlugin;
use std::sync::atomic::Ordering;

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

#[test]
fn search_bright_no_hardware_calls() {
    BRIGHTNESS_QUERIES.store(0, Ordering::SeqCst);
    let plugin = BrightnessPlugin;
    let _ = plugin.search("bright");
    assert_eq!(BRIGHTNESS_QUERIES.load(Ordering::SeqCst), 0);
}
