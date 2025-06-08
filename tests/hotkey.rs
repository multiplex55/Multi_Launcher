use multi_launcher::hotkey::{parse_hotkey, Hotkey, HotkeyTrigger};
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
fn parse_shift_escape() {
    let hk = parse_hotkey("Shift+Escape").expect("should parse shift+escape");
    assert_eq!(hk.key, Key::Escape);
    assert!(!hk.ctrl && hk.shift && !hk.alt);
}

#[test]
fn parse_invalid_hotkey() {
    assert!(parse_hotkey("Ctrl+Foo").is_none());
    assert!(parse_hotkey("Ctrl+Shift").is_none());
}

#[test]
fn trigger_take() {
    let hk = Hotkey::default();
    let trigger = HotkeyTrigger::new(hk);
    {
        let mut open = trigger.open.lock().unwrap();
        *open = true;
    }
    assert!(trigger.take());
    assert!(!trigger.take());
}
