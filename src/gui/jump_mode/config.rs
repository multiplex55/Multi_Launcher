use std::fmt;

use eframe::egui;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::hotkey::{Key as HotkeyKey, parse_hotkey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpChordError {
    Empty,
    EmptyComponent,
    MissingMainKey,
    MultipleMainKeys,
    DuplicateModifier,
    UnsupportedModifier,
    UnsupportedKey,
    ReservedMainKey,
    PrintableKeyNeedsModifier,
    CtrlAltAmbiguous,
}

impl fmt::Display for JumpChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "enter one supported key chord",
            Self::EmptyComponent => "empty '+' components are not allowed",
            Self::MissingMainKey => "the chord must contain exactly one main key",
            Self::MultipleMainKeys => "the chord must contain exactly one main key",
            Self::DuplicateModifier => "a modifier may appear only once",
            Self::UnsupportedModifier => {
                "AltGr, Windows, and side-specific modifiers are unsupported"
            }
            Self::UnsupportedKey => {
                "supported keys are A-Z, 0-9, Space, Tab, Delete, Home/End, PageUp/PageDown, arrows, and F1-F24"
            }
            Self::ReservedMainKey => "Escape, Enter, and Backspace are reserved",
            Self::PrintableKeyNeedsModifier => "letters, digits, and Space require a modifier",
            Self::CtrlAltAmbiguous => "Ctrl+Alt is ambiguous with AltGr and is unsupported",
        };
        f.write_str(message)
    }
}

impl std::error::Error for JumpChordError {}

/// A validated local chord whose keys and modifiers are representable by egui.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JumpModeChord {
    key: egui::Key,
    ctrl: bool,
    shift: bool,
    alt: bool,
}

impl JumpModeChord {
    pub fn parse(input: &str) -> Result<Self, JumpChordError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(JumpChordError::Empty);
        }

        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut primary = None;

        for component in input.split('+') {
            let token = component.trim();
            if token.is_empty() {
                return Err(JumpChordError::EmptyComponent);
            }
            let upper = token.to_ascii_uppercase();
            match upper.as_str() {
                "CTRL" | "CONTROL" => {
                    if ctrl {
                        return Err(JumpChordError::DuplicateModifier);
                    }
                    ctrl = true;
                }
                "SHIFT" => {
                    if shift {
                        return Err(JumpChordError::DuplicateModifier);
                    }
                    shift = true;
                }
                "ALT" => {
                    if alt {
                        return Err(JumpChordError::DuplicateModifier);
                    }
                    alt = true;
                }
                "ALTGR" | "ALT-GR" | "WIN" | "SUPER" | "WINDOWS" | "CMD" | "COMMAND" | "META"
                | "LEFTCTRL" | "RIGHTCTRL" | "LEFTSHIFT" | "RIGHTSHIFT" | "LEFTALT"
                | "RIGHTALT" => {
                    return Err(JumpChordError::UnsupportedModifier);
                }
                _ => {
                    if primary.is_some() {
                        return Err(JumpChordError::MultipleMainKeys);
                    }
                    let parsed = parse_hotkey(token).ok_or(JumpChordError::UnsupportedKey)?;
                    primary = Some(egui_key(parsed.key).ok_or(JumpChordError::UnsupportedKey)?);
                }
            }
        }

        let key = primary.ok_or(JumpChordError::MissingMainKey)?;
        if matches!(
            key,
            egui::Key::Escape | egui::Key::Enter | egui::Key::Backspace
        ) {
            return Err(JumpChordError::ReservedMainKey);
        }
        if ctrl && alt {
            return Err(JumpChordError::CtrlAltAmbiguous);
        }
        if is_printable_key(key) && !(ctrl || shift || alt) {
            return Err(JumpChordError::PrintableKeyNeedsModifier);
        }

        Ok(Self {
            key,
            ctrl,
            shift,
            alt,
        })
    }

    pub fn key(self) -> egui::Key {
        self.key
    }

    pub fn ctrl(self) -> bool {
        self.ctrl
    }

    pub fn shift(self) -> bool {
        self.shift
    }

    pub fn alt(self) -> bool {
        self.alt
    }

    pub fn canonical_text(self) -> String {
        let mut parts = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        parts.push(egui_key_name(self.key).to_owned());
        parts.join("+")
    }
}

impl Default for JumpModeChord {
    fn default() -> Self {
        Self {
            key: egui::Key::Space,
            ctrl: false,
            shift: true,
            alt: false,
        }
    }
}

impl Serialize for JumpModeChord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical_text())
    }
}

impl<'de> Deserialize<'de> for JumpModeChord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value.as_str().and_then(|text| Self::parse(text).ok()) {
            Some(chord) => Ok(chord),
            None => {
                tracing::warn!(
                    "invalid jump_mode_activation_chord setting; using {}",
                    Self::default().canonical_text()
                );
                Ok(Self::default())
            }
        }
    }
}

fn egui_key(key: HotkeyKey) -> Option<egui::Key> {
    Some(match key {
        HotkeyKey::KeyA => egui::Key::A,
        HotkeyKey::KeyB => egui::Key::B,
        HotkeyKey::KeyC => egui::Key::C,
        HotkeyKey::KeyD => egui::Key::D,
        HotkeyKey::KeyE => egui::Key::E,
        HotkeyKey::KeyF => egui::Key::F,
        HotkeyKey::KeyG => egui::Key::G,
        HotkeyKey::KeyH => egui::Key::H,
        HotkeyKey::KeyI => egui::Key::I,
        HotkeyKey::KeyJ => egui::Key::J,
        HotkeyKey::KeyK => egui::Key::K,
        HotkeyKey::KeyL => egui::Key::L,
        HotkeyKey::KeyM => egui::Key::M,
        HotkeyKey::KeyN => egui::Key::N,
        HotkeyKey::KeyO => egui::Key::O,
        HotkeyKey::KeyP => egui::Key::P,
        HotkeyKey::KeyQ => egui::Key::Q,
        HotkeyKey::KeyR => egui::Key::R,
        HotkeyKey::KeyS => egui::Key::S,
        HotkeyKey::KeyT => egui::Key::T,
        HotkeyKey::KeyU => egui::Key::U,
        HotkeyKey::KeyV => egui::Key::V,
        HotkeyKey::KeyW => egui::Key::W,
        HotkeyKey::KeyX => egui::Key::X,
        HotkeyKey::KeyY => egui::Key::Y,
        HotkeyKey::KeyZ => egui::Key::Z,
        HotkeyKey::Num0 => egui::Key::Num0,
        HotkeyKey::Num1 => egui::Key::Num1,
        HotkeyKey::Num2 => egui::Key::Num2,
        HotkeyKey::Num3 => egui::Key::Num3,
        HotkeyKey::Num4 => egui::Key::Num4,
        HotkeyKey::Num5 => egui::Key::Num5,
        HotkeyKey::Num6 => egui::Key::Num6,
        HotkeyKey::Num7 => egui::Key::Num7,
        HotkeyKey::Num8 => egui::Key::Num8,
        HotkeyKey::Num9 => egui::Key::Num9,
        HotkeyKey::Space => egui::Key::Space,
        HotkeyKey::Tab => egui::Key::Tab,
        HotkeyKey::Delete => egui::Key::Delete,
        HotkeyKey::Home => egui::Key::Home,
        HotkeyKey::End => egui::Key::End,
        HotkeyKey::PageUp => egui::Key::PageUp,
        HotkeyKey::PageDown => egui::Key::PageDown,
        HotkeyKey::LeftArrow => egui::Key::ArrowLeft,
        HotkeyKey::RightArrow => egui::Key::ArrowRight,
        HotkeyKey::UpArrow => egui::Key::ArrowUp,
        HotkeyKey::DownArrow => egui::Key::ArrowDown,
        HotkeyKey::F1 => egui::Key::F1,
        HotkeyKey::F2 => egui::Key::F2,
        HotkeyKey::F3 => egui::Key::F3,
        HotkeyKey::F4 => egui::Key::F4,
        HotkeyKey::F5 => egui::Key::F5,
        HotkeyKey::F6 => egui::Key::F6,
        HotkeyKey::F7 => egui::Key::F7,
        HotkeyKey::F8 => egui::Key::F8,
        HotkeyKey::F9 => egui::Key::F9,
        HotkeyKey::F10 => egui::Key::F10,
        HotkeyKey::F11 => egui::Key::F11,
        HotkeyKey::F12 => egui::Key::F12,
        HotkeyKey::F13 => egui::Key::F13,
        HotkeyKey::F14 => egui::Key::F14,
        HotkeyKey::F15 => egui::Key::F15,
        HotkeyKey::F16 => egui::Key::F16,
        HotkeyKey::F17 => egui::Key::F17,
        HotkeyKey::F18 => egui::Key::F18,
        HotkeyKey::F19 => egui::Key::F19,
        HotkeyKey::F20 => egui::Key::F20,
        HotkeyKey::F21 => egui::Key::F21,
        HotkeyKey::F22 => egui::Key::F22,
        HotkeyKey::F23 => egui::Key::F23,
        HotkeyKey::F24 => egui::Key::F24,
        HotkeyKey::Escape => egui::Key::Escape,
        HotkeyKey::Return => egui::Key::Enter,
        HotkeyKey::Backspace => egui::Key::Backspace,
        _ => return None,
    })
}

fn is_printable_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::A
            | egui::Key::B
            | egui::Key::C
            | egui::Key::D
            | egui::Key::E
            | egui::Key::F
            | egui::Key::G
            | egui::Key::H
            | egui::Key::I
            | egui::Key::J
            | egui::Key::K
            | egui::Key::L
            | egui::Key::M
            | egui::Key::N
            | egui::Key::O
            | egui::Key::P
            | egui::Key::Q
            | egui::Key::R
            | egui::Key::S
            | egui::Key::T
            | egui::Key::U
            | egui::Key::V
            | egui::Key::W
            | egui::Key::X
            | egui::Key::Y
            | egui::Key::Z
            | egui::Key::Num0
            | egui::Key::Num1
            | egui::Key::Num2
            | egui::Key::Num3
            | egui::Key::Num4
            | egui::Key::Num5
            | egui::Key::Num6
            | egui::Key::Num7
            | egui::Key::Num8
            | egui::Key::Num9
            | egui::Key::Space
    )
}

fn egui_key_name(key: egui::Key) -> &'static str {
    match key {
        egui::Key::A => "A",
        egui::Key::B => "B",
        egui::Key::C => "C",
        egui::Key::D => "D",
        egui::Key::E => "E",
        egui::Key::F => "F",
        egui::Key::G => "G",
        egui::Key::H => "H",
        egui::Key::I => "I",
        egui::Key::J => "J",
        egui::Key::K => "K",
        egui::Key::L => "L",
        egui::Key::M => "M",
        egui::Key::N => "N",
        egui::Key::O => "O",
        egui::Key::P => "P",
        egui::Key::Q => "Q",
        egui::Key::R => "R",
        egui::Key::S => "S",
        egui::Key::T => "T",
        egui::Key::U => "U",
        egui::Key::V => "V",
        egui::Key::W => "W",
        egui::Key::X => "X",
        egui::Key::Y => "Y",
        egui::Key::Z => "Z",
        egui::Key::Num0 => "0",
        egui::Key::Num1 => "1",
        egui::Key::Num2 => "2",
        egui::Key::Num3 => "3",
        egui::Key::Num4 => "4",
        egui::Key::Num5 => "5",
        egui::Key::Num6 => "6",
        egui::Key::Num7 => "7",
        egui::Key::Num8 => "8",
        egui::Key::Num9 => "9",
        egui::Key::Space => "Space",
        egui::Key::Tab => "Tab",
        egui::Key::Delete => "Delete",
        egui::Key::Home => "Home",
        egui::Key::End => "End",
        egui::Key::PageUp => "PageUp",
        egui::Key::PageDown => "PageDown",
        egui::Key::ArrowLeft => "Left",
        egui::Key::ArrowRight => "Right",
        egui::Key::ArrowUp => "Up",
        egui::Key::ArrowDown => "Down",
        egui::Key::F1 => "F1",
        egui::Key::F2 => "F2",
        egui::Key::F3 => "F3",
        egui::Key::F4 => "F4",
        egui::Key::F5 => "F5",
        egui::Key::F6 => "F6",
        egui::Key::F7 => "F7",
        egui::Key::F8 => "F8",
        egui::Key::F9 => "F9",
        egui::Key::F10 => "F10",
        egui::Key::F11 => "F11",
        egui::Key::F12 => "F12",
        egui::Key::F13 => "F13",
        egui::Key::F14 => "F14",
        egui::Key::F15 => "F15",
        egui::Key::F16 => "F16",
        egui::Key::F17 => "F17",
        egui::Key::F18 => "F18",
        egui::Key::F19 => "F19",
        egui::Key::F20 => "F20",
        egui::Key::F21 => "F21",
        egui::Key::F22 => "F22",
        egui::Key::F23 => "F23",
        egui::Key::F24 => "F24",
        egui::Key::Escape => "Escape",
        egui::Key::Enter => "Enter",
        egui::Key::Backspace => "Backspace",
        _ => "Unsupported",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HintAlphabetError {
    TooShort,
    TooLong,
    NonAsciiLetter,
    DuplicateLetter(char),
}

impl fmt::Display for HintAlphabetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => f.write_str("alphabet must contain 2 through 26 letters"),
            Self::TooLong => f.write_str("alphabet must contain no more than 26 letters"),
            Self::NonAsciiLetter => f.write_str("alphabet may contain only ASCII letters"),
            Self::DuplicateLetter(letter) => write!(f, "alphabet repeats the letter {letter}"),
        }
    }
}

impl std::error::Error for HintAlphabetError {}

/// Ordered, validated symbols and O(1) rank lookup for one frozen hint map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintAlphabet {
    symbols: Vec<u8>,
    ranks: [u8; 26],
}

impl HintAlphabet {
    pub fn parse(input: &str) -> Result<Self, HintAlphabetError> {
        let trimmed = input.trim();
        let bytes = trimmed.as_bytes();
        if bytes.len() < 2 {
            return Err(HintAlphabetError::TooShort);
        }
        if bytes.len() > 26 {
            return Err(HintAlphabetError::TooLong);
        }

        let mut seen = [false; 26];
        let mut ranks = [u8::MAX; 26];
        let mut symbols = Vec::with_capacity(bytes.len());
        for (rank, byte) in bytes.iter().copied().enumerate() {
            let upper = if byte.is_ascii_lowercase() {
                byte.to_ascii_uppercase()
            } else if byte.is_ascii_uppercase() {
                byte
            } else {
                return Err(HintAlphabetError::NonAsciiLetter);
            };
            let index = usize::from(upper - b'A');
            if seen[index] {
                return Err(HintAlphabetError::DuplicateLetter(char::from(upper)));
            }
            seen[index] = true;
            ranks[index] = rank as u8;
            symbols.push(upper);
        }
        Ok(Self { symbols, ranks })
    }

    pub fn symbols(&self) -> &[u8] {
        &self.symbols
    }

    pub fn rank(&self, symbol: u8) -> Option<u8> {
        let upper = symbol.to_ascii_uppercase();
        if !upper.is_ascii_uppercase() {
            return None;
        }
        let rank = self.ranks[usize::from(upper - b'A')];
        (rank != u8::MAX).then_some(rank)
    }

    pub fn contains(&self, symbol: u8) -> bool {
        self.rank(symbol).is_some()
    }

    pub fn canonical_text(&self) -> String {
        String::from_utf8(self.symbols.clone()).expect("validated ASCII alphabet")
    }
}

impl Default for HintAlphabet {
    fn default() -> Self {
        Self::parse(std::str::from_utf8(super::HINT_ALPHABET).expect("default alphabet is ASCII"))
            .expect("default alphabet is valid")
    }
}

impl Serialize for HintAlphabet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical_text())
    }
}

impl<'de> Deserialize<'de> for HintAlphabet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value.as_str().and_then(|text| Self::parse(text).ok()) {
            Some(alphabet) => Ok(alphabet),
            None => {
                tracing::warn!(
                    "invalid jump_mode_hint_alphabet setting; using {}",
                    Self::default().canonical_text()
                );
                Ok(Self::default())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_configuration_chord_validates_supported_key_and_canonical_modifiers() {
        let default = JumpModeChord::parse("  Shift + Space  ").unwrap();
        assert_eq!(default, JumpModeChord::default());
        assert_eq!(default.canonical_text(), "Shift+Space");

        let chord = JumpModeChord::parse(" control + shift + F8 ").unwrap();
        assert_eq!(chord.key(), egui::Key::F8);
        assert!(chord.ctrl());
        assert!(chord.shift());
        assert!(!chord.alt());
        assert_eq!(chord.canonical_text(), "Ctrl+Shift+F8");

        let navigation = JumpModeChord::parse("Alt+LeftArrow").unwrap();
        assert_eq!(navigation.key(), egui::Key::ArrowLeft);
        assert_eq!(navigation.canonical_text(), "Alt+Left");
        assert_eq!(JumpModeChord::parse("Ctrl+A").unwrap().key(), egui::Key::A);
        for key in [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "Tab", "Delete", "Home", "End",
            "PageUp", "PageDown", "Left", "Right", "Up", "Down",
        ] {
            assert!(
                JumpModeChord::parse(&format!("Ctrl+{key}")).is_ok(),
                "supported key {key} was rejected"
            );
        }
        for letter in 'A'..='Z' {
            assert!(JumpModeChord::parse(&format!("Ctrl+{letter}")).is_ok());
        }
        for function in 1..=24 {
            assert!(JumpModeChord::parse(&format!("Ctrl+F{function}")).is_ok());
        }
    }

    #[test]
    fn jump_configuration_chord_rejects_malformed_ambiguous_and_reserved_input() {
        for invalid in [
            "",
            "Ctrl++A",
            "+A",
            "Ctrl+",
            "Ctrl",
            "Ctrl+A+F8",
            "F1+F2",
            "Unknown",
            "Escape",
            "Return",
            "Enter",
            "Backspace",
            "A",
            "7",
            "Space",
            "AltGr+F8",
            "Alt-GR+F8",
            "Win+F8",
            "Super+F8",
            "LeftCtrl+F8",
            "RightShift+F8",
            "Ctrl+Alt+F8",
            "Ctrl+Ctrl+F8",
            "CapsLock",
            "F25",
            "Numpad1",
            "MediaPlay",
        ] {
            assert!(
                JumpModeChord::parse(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn jump_configuration_alphabet_normalizes_case_and_preserves_order() {
        let alphabet = HintAlphabet::parse("  aSdFjkLq \t").unwrap();
        assert_eq!(alphabet.canonical_text(), "ASDFJKLQ");
        assert_eq!(alphabet.symbols(), b"ASDFJKLQ");
        for (rank, symbol) in b"ASDFJKLQ".iter().copied().enumerate() {
            assert_eq!(alphabet.rank(symbol), Some(rank as u8));
        }
        assert_eq!(alphabet.rank(b'Z'), None);
    }

    #[test]
    fn jump_configuration_alphabet_rejects_invalid_shapes() {
        for invalid in [
            "",
            "A",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZX",
            "AAB",
            "aAa",
            "AB C",
            "A\nB",
            "AÅ",
            "AB1",
        ] {
            assert!(
                HintAlphabet::parse(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn jump_configuration_preferences_serialize_canonically_and_recover_by_field() {
        let chord = JumpModeChord::parse(" control + shift + F8 ").unwrap();
        let alphabet = HintAlphabet::parse("asDfjklq").unwrap();
        let value = serde_json::json!({
            "chord": chord,
            "alphabet": alphabet,
        });
        assert_eq!(value["chord"], "Ctrl+Shift+F8");
        assert_eq!(value["alphabet"], "ASDFJKLQ");

        #[derive(Debug, Deserialize)]
        struct Preferences {
            #[serde(default)]
            chord: JumpModeChord,
            #[serde(default)]
            alphabet: HintAlphabet,
        }

        let decoded: Preferences = serde_json::from_value(serde_json::json!({
            "chord": ["not", "a", "string"],
            "alphabet": "zY",
        }))
        .unwrap();
        assert_eq!(decoded.chord, JumpModeChord::default());
        assert_eq!(decoded.alphabet.canonical_text(), "ZY");

        let decoded: Preferences = serde_json::from_value(serde_json::json!({
            "chord": "Ctrl+F8",
            "alphabet": 23,
        }))
        .unwrap();
        assert_eq!(decoded.chord.canonical_text(), "Ctrl+F8");
        assert_eq!(decoded.alphabet, HintAlphabet::default());
    }
}
