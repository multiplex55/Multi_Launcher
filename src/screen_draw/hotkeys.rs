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

/// Resolves canvas-local shortcuts. This is deliberately called only by the
/// focused Drawing canvas; Ghost has no input surface and therefore cannot
/// consume these keys.
pub(crate) fn local_shortcut(
    settings: &ScreenDrawSettings,
    virtual_key: u32,
    modifiers: u32,
    text_editing: bool,
) -> Option<LocalShortcutAction> {
    if text_editing {
        return None;
    }
    let ctrl = modifiers & MOD_CONTROL_VALUE != 0;
    let shift = modifiers & MOD_SHIFT_VALUE != 0;
    if ctrl && virtual_key == b'Z' as u32 {
        return Some(if shift {
            LocalShortcutAction::Redo
        } else {
            LocalShortcutAction::Undo
        });
    }
    if ctrl && virtual_key == b'Y' as u32 {
        return Some(LocalShortcutAction::Redo);
    }
    for (tool, chord) in &settings.tool_hotkeys {
        if chord_matches(chord, virtual_key, modifiers) {
            return Some(LocalShortcutAction::Tool(*tool));
        }
    }
    if settings
        .increase_thickness_hotkey
        .as_ref()
        .is_some_and(|chord| chord_matches(chord, virtual_key, modifiers))
    {
        return Some(LocalShortcutAction::IncreaseThickness);
    }
    if settings
        .decrease_thickness_hotkey
        .as_ref()
        .is_some_and(|chord| chord_matches(chord, virtual_key, modifiers))
    {
        return Some(LocalShortcutAction::DecreaseThickness);
    }
    for (index, chord) in settings.quick_color_hotkeys.iter().enumerate() {
        if chord
            .as_ref()
            .is_some_and(|chord| chord_matches(chord, virtual_key, modifiers))
        {
            return Some(LocalShortcutAction::Color(settings.palette[index]));
        }
    }
    None
}

fn chord_matches(chord: &HotkeyChord, virtual_key: u32, modifiers: u32) -> bool {
    let text = chord.as_str().trim();
    if text == "[" || text == "]" {
        return modifiers & (MOD_CONTROL_VALUE | MOD_ALT_VALUE | MOD_WIN_VALUE) == 0
            && virtual_key == if text == "[" { 0xDB } else { 0xDD };
    }
    let Ok(native) = to_native_hotkey(chord) else {
        return false;
    };
    let expected = native.modifiers & !MOD_NOREPEAT_VALUE;
    modifiers & (MOD_ALT_VALUE | MOD_CONTROL_VALUE | MOD_SHIFT_VALUE | MOD_WIN_VALUE) == expected
        && virtual_key == native.virtual_key
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
        assert_eq!(
            local_shortcut(&settings, b'G' as u32, 0, false),
            Some(LocalShortcutAction::Tool(ScreenDrawTool::FadingInk))
        );
        assert_eq!(
            local_shortcut(&settings, b'V' as u32, 0, false),
            Some(LocalShortcutAction::Tool(ScreenDrawTool::Eyedropper))
        );
        assert_eq!(
            local_shortcut(&settings, 0xDB, 0, false),
            Some(LocalShortcutAction::DecreaseThickness)
        );
        assert_eq!(
            local_shortcut(&settings, b'Z' as u32, MOD_CONTROL_VALUE, false),
            Some(LocalShortcutAction::Undo)
        );
        assert_eq!(
            local_shortcut(&settings, b'1' as u32, 0, false),
            Some(LocalShortcutAction::Color(settings.palette[0]))
        );
    }

    #[test]
    fn text_editing_disables_all_local_shortcuts() {
        let settings = ScreenDrawSettings::default();
        assert_eq!(local_shortcut(&settings, b'P' as u32, 0, true), None);
        assert_eq!(
            local_shortcut(&settings, b'Z' as u32, MOD_CONTROL_VALUE, true),
            None
        );
    }
}
