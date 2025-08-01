use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::convert_panel::ConvertPanelPlugin;

#[test]
fn search_conv_prefix() {
    let plugin = ConvertPanelPlugin;
    let results = plugin.search("conv");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "convert:panel");
}

#[test]
fn search_convert_prefix() {
    let plugin = ConvertPanelPlugin;
    let results = plugin.search("convert");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, "convert:panel");
}

