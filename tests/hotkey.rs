use multi_launcher::hotkey::{parse_hotkey, Hotkey};
use rdev::Key;

#[test]
fn parse_simple_f_key() {
    let hk = parse_hotkey("F2").expect("should parse F2");
    assert_eq!(hk.key, Key::F2);
    assert!(!hk.ctrl && !hk.shift && !hk.alt);
}

#[test]
fn parse_combo_hotkey() {
    let hk = parse_hotkey("Ctrl+Shift+Space").expect("should parse combination");
    assert_eq!(hk.key, Key::Space);
    assert!(hk.ctrl && hk.shift && !hk.alt);
}

#[test]
fn parse_invalid_hotkey() {
    assert!(parse_hotkey("Ctrl+Foo").is_none());
    assert!(parse_hotkey("Ctrl+Shift").is_none());
}
