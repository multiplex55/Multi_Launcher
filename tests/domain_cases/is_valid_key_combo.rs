#![cfg(windows)]
use multi_launcher::workspace::is_valid_key_combo;

#[test]
fn valid_key_combos() {
    assert!(is_valid_key_combo("Ctrl+Alt+F5"));
    assert!(is_valid_key_combo("F24"));
}

#[test]
fn invalid_key_combos() {
    assert!(!is_valid_key_combo("Ctrl+Shift"));
    assert!(!is_valid_key_combo("Foo"));
}
