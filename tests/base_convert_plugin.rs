use multi_launcher::plugin::Plugin;
use multi_launcher::plugins::base_convert::BaseConvertPlugin;

#[test]
fn bin_to_hex() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 1010 bin to hex");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "1010 bin = a hex");
    assert_eq!(results[0].action, "clipboard:a");
}

#[test]
fn bin_to_oct() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 111 bin to oct");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "111 bin = 7 oct");
    assert_eq!(results[0].action, "clipboard:7");
}

#[test]
fn text_to_bin() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv \"A\" text to bin");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "A text = 01000001 bin");
    assert_eq!(results[0].action, "clipboard:01000001");
}

#[test]
fn text_to_hex() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv \"hi\" text to hex");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "hi text = 6869 hex");
    assert_eq!(results[0].action, "clipboard:6869");
}

#[test]
fn hex_to_text() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 41 hex to text");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "41 hex = A text");
    assert_eq!(results[0].action, "clipboard:A");
}

#[test]
fn hex_to_bin() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv ff hex to bin");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "ff hex = 11111111 bin");
    assert_eq!(results[0].action, "clipboard:11111111");
}

#[test]
fn dec_to_bin() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 10 dec to bin");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "10 dec = 1010 bin");
    assert_eq!(results[0].action, "clipboard:1010");
}

#[test]
fn dec_to_hex() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 15 dec to hex");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "15 dec = f hex");
    assert_eq!(results[0].action, "clipboard:f");
}

#[test]
fn dec_to_oct() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 8 dec to oct");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].label, "8 dec = 10 oct");
    assert_eq!(results[0].action, "clipboard:10");
}

#[test]
fn handles_empty_query() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv");
    assert!(results.is_empty());
}

#[test]
fn handles_invalid_tokens() {
    let plugin = BaseConvertPlugin;
    let results = plugin.search("conv 123 bin hex");
    assert!(results.is_empty());
}
