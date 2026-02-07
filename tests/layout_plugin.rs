use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::layout::LayoutPlugin;

#[test]
fn layout_command_has_metadata() {
    let plugin = LayoutPlugin;
    let results = plugin.commands();
    assert!(!results.is_empty());
    let action = &results[0];
    assert!(action.preview_text.is_some());
    assert!(action.risk_level.is_some());
    assert!(action.icon.is_some());
}
