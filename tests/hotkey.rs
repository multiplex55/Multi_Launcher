use multi_launcher::hotkey::{parse_hotkey, Hotkey, HotkeyTrigger};
use multi_launcher::hotkey::Key;

#[test]
fn parse_simple_f_key() {
    let hk = parse_hotkey("F2").expect("should parse F2");
    assert_eq!(hk.key, Key::F2);
    assert!(!hk.ctrl && !hk.shift && !hk.alt && !hk.win);
}

#[test]
fn parse_high_function_keys() {
    let f13 = parse_hotkey("F13").expect("should parse F13");
    assert_eq!(f13.key, Key::F13);
    let f24 = parse_hotkey("F24").expect("should parse F24");
    assert_eq!(f24.key, Key::F24);
}

#[test]
fn parse_combo_hotkey() {
    let hk = parse_hotkey("Ctrl+Shift+Space").expect("should parse combination");
    assert_eq!(hk.key, Key::Space);
    assert!(hk.ctrl && hk.shift && !hk.alt && !hk.win);
}

#[test]
fn parse_shift_escape() {
    let hk = parse_hotkey("Shift+Escape").expect("should parse shift+escape");
    assert_eq!(hk.key, Key::Escape);
    assert!(!hk.ctrl && hk.shift && !hk.alt && !hk.win);
}

#[test]
fn parse_zero_hotkey() {
    let hk = parse_hotkey("0").expect("should parse numeric zero");
    assert_eq!(hk.key, Key::Num0);
    assert!(!hk.ctrl && !hk.shift && !hk.alt && !hk.win);
}

#[test]
fn parse_win_modifier() {
    let hk = parse_hotkey("Win+F3").expect("should parse win modifier");
    assert_eq!(hk.key, Key::F3);
    assert!(hk.win && !hk.ctrl && !hk.shift && !hk.alt);
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
