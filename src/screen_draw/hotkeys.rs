use crate::hotkey::{Key, parse_hotkey};

use super::{HotkeyChord, RgbaColor, ScreenDrawSettings, ScreenDrawTool};

/// Platform-neutral representation passed to the session-scoped native
/// hotkey registration boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeHotkey {
    pub modifiers: u32,
    pub virtual_key: u32,
}

const MOD_ALT_VALUE: u32 = 0x0001;
const MOD_CONTROL_VALUE: u32 = 0x0002;
const MOD_SHIFT_VALUE: u32 = 0x0004;
const MOD_WIN_VALUE: u32 = 0x0008;
const MOD_NOREPEAT_VALUE: u32 = 0x4000;

pub(crate) fn to_native_hotkey(chord: &HotkeyChord) -> Result<NativeHotkey, String> {
    let parsed = parse_hotkey(chord.as_str())
        .ok_or_else(|| format!("invalid emergency hotkey '{}'", chord.as_str()))?;
    let mut modifiers = MOD_NOREPEAT_VALUE;
    if parsed.alt {
        modifiers |= MOD_ALT_VALUE;
    }
    if parsed.ctrl {
        modifiers |= MOD_CONTROL_VALUE;
    }
    if parsed.shift {
        modifiers |= MOD_SHIFT_VALUE;
    }
    if parsed.win {
        modifiers |= MOD_WIN_VALUE;
    }
    let virtual_key = key_to_virtual_key(parsed.key)
        .ok_or_else(|| format!("unsupported emergency hotkey '{}'", chord.as_str()))?;
    Ok(NativeHotkey {
        modifiers,
        virtual_key,
    })
}

fn key_to_virtual_key(key: Key) -> Option<u32> {
    let value = match key {
        Key::Space => 0x20,
        Key::Tab => 0x09,
        Key::Return => 0x0D,
        Key::Escape => 0x1B,
        Key::Delete => 0x2E,
        Key::Backspace => 0x08,
        Key::CapsLock => 0x14,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::LeftArrow => 0x25,
        Key::UpArrow => 0x26,
        Key::RightArrow => 0x27,
        Key::DownArrow => 0x28,
        Key::Num0 => b'0' as u32,
        Key::Num1 => b'1' as u32,
        Key::Num2 => b'2' as u32,
        Key::Num3 => b'3' as u32,
        Key::Num4 => b'4' as u32,
        Key::Num5 => b'5' as u32,
        Key::Num6 => b'6' as u32,
        Key::Num7 => b'7' as u32,
        Key::Num8 => b'8' as u32,
        Key::Num9 => b'9' as u32,
        Key::KeyA => b'A' as u32,
        Key::KeyB => b'B' as u32,
        Key::KeyC => b'C' as u32,
        Key::KeyD => b'D' as u32,
        Key::KeyE => b'E' as u32,
        Key::KeyF => b'F' as u32,
        Key::KeyG => b'G' as u32,
        Key::KeyH => b'H' as u32,
        Key::KeyI => b'I' as u32,
        Key::KeyJ => b'J' as u32,
        Key::KeyK => b'K' as u32,
        Key::KeyL => b'L' as u32,
        Key::KeyM => b'M' as u32,
        Key::KeyN => b'N' as u32,
        Key::KeyO => b'O' as u32,
        Key::KeyP => b'P' as u32,
        Key::KeyQ => b'Q' as u32,
        Key::KeyR => b'R' as u32,
        Key::KeyS => b'S' as u32,
        Key::KeyT => b'T' as u32,
        Key::KeyU => b'U' as u32,
        Key::KeyV => b'V' as u32,
        Key::KeyW => b'W' as u32,
        Key::KeyX => b'X' as u32,
        Key::KeyY => b'Y' as u32,
        Key::KeyZ => b'Z' as u32,
        Key::F1 => 0x70,
        Key::F2 => 0x71,
        Key::F3 => 0x72,
        Key::F4 => 0x73,
        Key::F5 => 0x74,
        Key::F6 => 0x75,
        Key::F7 => 0x76,
        Key::F8 => 0x77,
        Key::F9 => 0x78,
        Key::F10 => 0x79,
        Key::F11 => 0x7A,
        Key::F12 => 0x7B,
        Key::F13 => 0x7C,
        Key::F14 => 0x7D,
        Key::F15 => 0x7E,
        Key::F16 => 0x7F,
        Key::F17 => 0x80,
        Key::F18 => 0x81,
        Key::F19 => 0x82,
        Key::F20 => 0x83,
        Key::F21 => 0x84,
        Key::F22 => 0x85,
        Key::F23 => 0x86,
        Key::F24 => 0x87,
        _ => return None,
    };
    Some(value)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LocalShortcutAction {
    Tool(ScreenDrawTool),
    Undo,
    Redo,
    IncreaseThickness,
    DecreaseThickness,
    Color(RgbaColor),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalShortcutKey {
    Standard(Key),
    OpenBracket,
    CloseBracket,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LocalShortcutModifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub win: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalShortcutInput {
    pub key: LocalShortcutKey,
    pub modifiers: LocalShortcutModifiers,
    pub text_editing: bool,
}

impl LocalShortcutInput {
    pub(crate) fn from_native(
        virtual_key: u32,
        modifiers: u32,
        text_editing: bool,
    ) -> Option<Self> {
        Some(Self {
            key: native_virtual_key_to_local_key(virtual_key)?,
            modifiers: LocalShortcutModifiers {
                alt: modifiers & MOD_ALT_VALUE != 0,
                ctrl: modifiers & MOD_CONTROL_VALUE != 0,
                shift: modifiers & MOD_SHIFT_VALUE != 0,
                win: modifiers & MOD_WIN_VALUE != 0,
            },
            text_editing,
        })
    }
}

/// Resolves all focused Screen Draw shortcuts through one platform-neutral
/// boundary. Native canvas and egui toolbar events are adapted before they
/// reach this table.
pub(crate) fn resolve_local_shortcut(
    settings: &ScreenDrawSettings,
    input: LocalShortcutInput,
) -> Option<LocalShortcutAction> {
    if input.text_editing {
        return None;
    }
    if input.modifiers.ctrl && input.key == LocalShortcutKey::Standard(Key::KeyZ) {
        return Some(if input.modifiers.shift {
            LocalShortcutAction::Redo
        } else {
            LocalShortcutAction::Undo
        });
    }
    if input.modifiers.ctrl && input.key == LocalShortcutKey::Standard(Key::KeyY) {
        return Some(LocalShortcutAction::Redo);
    }
    for (tool, chord) in &settings.tool_hotkeys {
        if chord_matches(chord, input.key, input.modifiers) {
            return Some(LocalShortcutAction::Tool(*tool));
        }
    }
    if settings
        .increase_thickness_hotkey
        .as_ref()
        .is_some_and(|chord| chord_matches(chord, input.key, input.modifiers))
    {
        return Some(LocalShortcutAction::IncreaseThickness);
    }
    if settings
        .decrease_thickness_hotkey
        .as_ref()
        .is_some_and(|chord| chord_matches(chord, input.key, input.modifiers))
    {
        return Some(LocalShortcutAction::DecreaseThickness);
    }
    for (index, chord) in settings.quick_color_hotkeys.iter().enumerate() {
        if chord
            .as_ref()
            .is_some_and(|chord| chord_matches(chord, input.key, input.modifiers))
        {
            return Some(LocalShortcutAction::Color(settings.palette[index]));
        }
    }
    None
}

fn chord_matches(
    chord: &HotkeyChord,
    key: LocalShortcutKey,
    modifiers: LocalShortcutModifiers,
) -> bool {
    let text = chord.as_str().trim();
    if text == "[" || text == "]" {
        return !modifiers.ctrl
            && !modifiers.alt
            && !modifiers.win
            && key
                == if text == "[" {
                    LocalShortcutKey::OpenBracket
                } else {
                    LocalShortcutKey::CloseBracket
                };
    }
    let Some(parsed) = parse_hotkey(chord.as_str()) else {
        return false;
    };
    key == LocalShortcutKey::Standard(parsed.key)
        && modifiers
            == (LocalShortcutModifiers {
                alt: parsed.alt,
                ctrl: parsed.ctrl,
                shift: parsed.shift,
                win: parsed.win,
            })
}

fn native_virtual_key_to_local_key(virtual_key: u32) -> Option<LocalShortcutKey> {
    if virtual_key == 0xDB {
        return Some(LocalShortcutKey::OpenBracket);
    }
    if virtual_key == 0xDD {
        return Some(LocalShortcutKey::CloseBracket);
    }
    let key = match virtual_key {
        0x20 => Key::Space,
        0x09 => Key::Tab,
        0x0D => Key::Return,
        0x2E => Key::Delete,
        0x08 => Key::Backspace,
        0x14 => Key::CapsLock,
        0x24 => Key::Home,
        0x23 => Key::End,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x25 => Key::LeftArrow,
        0x26 => Key::UpArrow,
        0x27 => Key::RightArrow,
        0x28 => Key::DownArrow,
        value @ 0x30..=0x39 => digit_key((value - 0x30) as u8)?,
        value @ 0x41..=0x5A => letter_key((value - 0x41) as u8)?,
        value @ 0x70..=0x87 => function_key((value - 0x70 + 1) as u8)?,
        _ => return None,
    };
    Some(LocalShortcutKey::Standard(key))
}

fn digit_key(index: u8) -> Option<Key> {
    Some(match index {
        0 => Key::Num0,
        1 => Key::Num1,
        2 => Key::Num2,
        3 => Key::Num3,
        4 => Key::Num4,
        5 => Key::Num5,
        6 => Key::Num6,
        7 => Key::Num7,
        8 => Key::Num8,
        9 => Key::Num9,
        _ => return None,
    })
}

fn letter_key(index: u8) -> Option<Key> {
    Some(match index {
        0 => Key::KeyA,
        1 => Key::KeyB,
        2 => Key::KeyC,
        3 => Key::KeyD,
        4 => Key::KeyE,
        5 => Key::KeyF,
        6 => Key::KeyG,
        7 => Key::KeyH,
        8 => Key::KeyI,
        9 => Key::KeyJ,
        10 => Key::KeyK,
        11 => Key::KeyL,
        12 => Key::KeyM,
        13 => Key::KeyN,
        14 => Key::KeyO,
        15 => Key::KeyP,
        16 => Key::KeyQ,
        17 => Key::KeyR,
        18 => Key::KeyS,
        19 => Key::KeyT,
        20 => Key::KeyU,
        21 => Key::KeyV,
        22 => Key::KeyW,
        23 => Key::KeyX,
        24 => Key::KeyY,
        25 => Key::KeyZ,
        _ => return None,
    })
}

fn function_key(index: u8) -> Option<Key> {
    Some(match index {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        13 => Key::F13,
        14 => Key::F14,
        15 => Key::F15,
        16 => Key::F16,
        17 => Key::F17,
        18 => Key::F18,
        19 => Key::F19,
        20 => Key::F20,
        21 => Key::F21,
        22 => Key::F22,
        23 => Key::F23,
        24 => Key::F24,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_emergency_chord_maps_to_norepeating_ctrl_shift_f12() {
        let hotkey = to_native_hotkey(&HotkeyChord::from_unchecked("Ctrl+Shift+F12")).unwrap();
        assert_eq!(hotkey.modifiers, 0x4000 | 0x0002 | 0x0004);
        assert_eq!(hotkey.virtual_key, 0x7B);
    }

    #[test]
    fn malformed_or_unsupported_chords_are_rejected_before_registration() {
        assert!(to_native_hotkey(&HotkeyChord::from_unchecked("Ctrl+Nope")).is_err());
    }

    #[test]
    fn local_defaults_cover_tools_history_brackets_and_quick_colors() {
        let settings = ScreenDrawSettings::default();
        let input = |virtual_key, modifiers| {
            LocalShortcutInput::from_native(virtual_key, modifiers, false).unwrap()
        };
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'G' as u32, 0)),
            Some(LocalShortcutAction::Tool(ScreenDrawTool::FadingInk))
        );
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'V' as u32, 0)),
            Some(LocalShortcutAction::Tool(ScreenDrawTool::Eyedropper))
        );
        assert_eq!(
            resolve_local_shortcut(&settings, input(0xDB, 0)),
            Some(LocalShortcutAction::DecreaseThickness)
        );
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'Z' as u32, MOD_CONTROL_VALUE)),
            Some(LocalShortcutAction::Undo)
        );
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'1' as u32, 0)),
            Some(LocalShortcutAction::Color(settings.palette[0]))
        );
        for index in 0..9 {
            assert_eq!(
                resolve_local_shortcut(&settings, input(b'1' as u32 + index as u32, 0)),
                Some(LocalShortcutAction::Color(settings.palette[index]))
            );
        }
    }

    #[test]
    fn text_editing_disables_all_local_shortcuts() {
        let settings = ScreenDrawSettings::default();
        let input = |virtual_key, modifiers| {
            LocalShortcutInput::from_native(virtual_key, modifiers, true).unwrap()
        };
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'P' as u32, 0)),
            None
        );
        assert_eq!(
            resolve_local_shortcut(&settings, input(b'Z' as u32, MOD_CONTROL_VALUE)),
            None
        );
    }

    #[test]
    fn history_variants_and_modifier_matching_are_exact() {
        let settings = ScreenDrawSettings::default();
        let resolve = |virtual_key, modifiers| {
            resolve_local_shortcut(
                &settings,
                LocalShortcutInput::from_native(virtual_key, modifiers, false).unwrap(),
            )
        };
        assert_eq!(
            resolve(b'Z' as u32, MOD_CONTROL_VALUE),
            Some(LocalShortcutAction::Undo)
        );
        assert_eq!(
            resolve(b'Y' as u32, MOD_CONTROL_VALUE),
            Some(LocalShortcutAction::Redo)
        );
        assert_eq!(
            resolve(b'Z' as u32, MOD_CONTROL_VALUE | MOD_SHIFT_VALUE),
            Some(LocalShortcutAction::Redo)
        );
        assert_eq!(resolve(b'P' as u32, MOD_SHIFT_VALUE), None);
    }
}
