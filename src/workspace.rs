#[cfg(any(test, target_os = "windows"))]
use once_cell::sync::Lazy;
#[cfg(any(test, target_os = "windows"))]
use regex::Regex;

#[cfg(any(test, target_os = "windows"))]
static HOTKEY_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:(?:Ctrl|Alt|Shift|Win)\+)?(?:(?:Ctrl|Alt|Shift|Win)\+)?(?:(?:Ctrl|Alt|Shift|Win)\+)?(?:(?:Ctrl|Alt|Shift|Win)\+)?(?:F(?:[1-9]|1[0-2]|1[3-9]|2[0-4])|[A-Z]|[0-9]|NUMPAD[0-9]|NUMPAD(?:MULTIPLY|ADD|SEPARATOR|SUBTRACT|DOT|DIVIDE)|UP|DOWN|LEFT|RIGHT|BACKSPACE|TAB|ENTER|PAUSE|CAPSLOCK|ESCAPE|SPACE|PAGEUP|PAGEDOWN|END|HOME|INSERT|DELETE|OEM_(?:PLUS|COMMA|MINUS|PERIOD|[1-7])|PRINTSCREEN|SCROLLLOCK|NUMLOCK|LEFT(?:SHIFT|CTRL|ALT)|RIGHT(?:SHIFT|CTRL|ALT))$").unwrap()
});

#[cfg(any(test, target_os = "windows"))]
pub fn is_valid_key_combo(input: &str) -> bool {
    HOTKEY_REGEX.is_match(input)
}
