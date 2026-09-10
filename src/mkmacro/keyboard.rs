//! Authoritative physical-key names and Windows virtual-key conversion.
//!
//! Unicode text remains an `MkAction::Text` concern. This module models keys
//! which Windows can address through virtual-key/scan-code input.

use super::MkKey;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowsKeyMetadata {
    pub virtual_key: u16,
    pub scan_code: u16,
    pub extended: bool,
}

pub fn is_modifier(key: &MkKey) -> bool {
    matches!(
        key,
        MkKey::Control
            | MkKey::LeftControl
            | MkKey::RightControl
            | MkKey::Alt
            | MkKey::LeftAlt
            | MkKey::RightAlt
            | MkKey::Shift
            | MkKey::LeftShift
            | MkKey::RightShift
            | MkKey::Meta
            | MkKey::LeftMeta
            | MkKey::RightMeta
    )
}

pub fn display_name(key: &MkKey) -> String {
    match key {
        MkKey::Character(value) => value.to_uppercase(),
        MkKey::Control => "Ctrl".into(),
        MkKey::LeftControl => "Left Ctrl".into(),
        MkKey::RightControl => "Right Ctrl".into(),
        MkKey::Alt => "Alt".into(),
        MkKey::LeftAlt => "Left Alt".into(),
        MkKey::RightAlt => "Right Alt".into(),
        MkKey::Shift => "Shift".into(),
        MkKey::LeftShift => "Left Shift".into(),
        MkKey::RightShift => "Right Shift".into(),
        MkKey::Meta => "Win".into(),
        MkKey::LeftMeta => "Left Win".into(),
        MkKey::RightMeta => "Right Win".into(),
        MkKey::Function(number) => format!("F{number}"),
        MkKey::Numpad(number) => format!("Numpad {number}"),
        MkKey::PageUp => "Page Up".into(),
        MkKey::PageDown => "Page Down".into(),
        MkKey::CapsLock => "Caps Lock".into(),
        MkKey::NumLock => "Num Lock".into(),
        MkKey::ScrollLock => "Scroll Lock".into(),
        MkKey::PrintScreen => "Print Screen".into(),
        MkKey::PauseBreak => "Pause / Break".into(),
        MkKey::NumpadMultiply => "Numpad *".into(),
        MkKey::NumpadAdd => "Numpad +".into(),
        MkKey::NumpadSeparator => "Numpad Separator".into(),
        MkKey::NumpadSubtract => "Numpad -".into(),
        MkKey::NumpadDecimal => "Numpad .".into(),
        MkKey::NumpadDivide => "Numpad /".into(),
        MkKey::OemSemicolon => "; (OEM Semicolon)".into(),
        MkKey::OemEquals => "= (OEM Equals)".into(),
        MkKey::OemComma => ", (OEM Comma)".into(),
        MkKey::OemMinus => "- (OEM Minus)".into(),
        MkKey::OemPeriod => ". (OEM Period)".into(),
        MkKey::OemSlash => "/ (OEM Slash)".into(),
        MkKey::OemBacktick => "` (OEM Backtick)".into(),
        MkKey::OemLeftBracket => "[ (OEM Left Bracket)".into(),
        MkKey::OemBackslash => "\\ (OEM Backslash)".into(),
        MkKey::OemRightBracket => "] (OEM Right Bracket)".into(),
        MkKey::OemQuote => "' (OEM Quote)".into(),
        MkKey::Oem102 => "OEM 102".into(),
        MkKey::BrowserBack => "Browser Back".into(),
        MkKey::BrowserForward => "Browser Forward".into(),
        MkKey::BrowserRefresh => "Browser Refresh".into(),
        MkKey::BrowserStop => "Browser Stop".into(),
        MkKey::BrowserSearch => "Browser Search".into(),
        MkKey::BrowserFavorites => "Browser Favorites".into(),
        MkKey::BrowserHome => "Browser Home".into(),
        MkKey::VolumeMute => "Volume Mute".into(),
        MkKey::VolumeDown => "Volume Down".into(),
        MkKey::VolumeUp => "Volume Up".into(),
        MkKey::MediaNext => "Media Next".into(),
        MkKey::MediaPrevious => "Media Previous".into(),
        MkKey::MediaStop => "Media Stop".into(),
        MkKey::MediaPlayPause => "Media Play / Pause".into(),
        MkKey::LaunchMail => "Launch Mail".into(),
        MkKey::LaunchMediaSelect => "Launch Media Select".into(),
        MkKey::LaunchApp1 => "Launch App 1".into(),
        MkKey::LaunchApp2 => "Launch App 2".into(),
        MkKey::RawVirtualKey { vk, scan_code, .. } if *scan_code != 0 => {
            format!("VK 0x{vk:02X} / Scan 0x{scan_code:02X}")
        }
        MkKey::RawVirtualKey { vk, .. } => format!("VK 0x{vk:02X}"),
        other => format!("{other:?}"),
    }
}

pub fn windows_key_metadata(key: &MkKey) -> Option<WindowsKeyMetadata> {
    let (virtual_key, scan_code, extended) = match key {
        MkKey::Character(s) if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => {
            (s.as_bytes()[0].to_ascii_uppercase() as u16, 0, false)
        }
        MkKey::Enter => (0x0D, 0, false),
        MkKey::Tab => (0x09, 0, false),
        MkKey::Escape => (0x1B, 0, false),
        MkKey::Space => (0x20, 0, false),
        MkKey::Backspace => (0x08, 0, false),
        MkKey::Delete => (0x2E, 0, true),
        MkKey::Insert => (0x2D, 0, true),
        MkKey::Up => (0x26, 0, true),
        MkKey::Down => (0x28, 0, true),
        MkKey::Left => (0x25, 0, true),
        MkKey::Right => (0x27, 0, true),
        MkKey::Home => (0x24, 0, true),
        MkKey::End => (0x23, 0, true),
        MkKey::PageUp => (0x21, 0, true),
        MkKey::PageDown => (0x22, 0, true),
        MkKey::CapsLock => (0x14, 0, false),
        MkKey::NumLock => (0x90, 0, true),
        MkKey::ScrollLock => (0x91, 0, false),
        MkKey::PrintScreen => (0x2C, 0, true),
        MkKey::PauseBreak => (0x13, 0, false),
        MkKey::Control => (0x11, 0, false),
        MkKey::LeftControl => (0xA2, 0, false),
        MkKey::RightControl => (0xA3, 0, true),
        MkKey::Alt => (0x12, 0, false),
        MkKey::LeftAlt => (0xA4, 0, false),
        MkKey::RightAlt => (0xA5, 0, true),
        MkKey::Shift => (0x10, 0, false),
        MkKey::LeftShift => (0xA0, 0, false),
        MkKey::RightShift => (0xA1, 0, false),
        MkKey::Meta | MkKey::LeftMeta => (0x5B, 0, true),
        MkKey::RightMeta => (0x5C, 0, true),
        MkKey::Function(number @ 1..=24) => (0x6F + u16::from(*number), 0, false),
        MkKey::Numpad(number @ 0..=9) => (0x60 + u16::from(*number), 0, false),
        MkKey::NumpadMultiply => (0x6A, 0, false),
        MkKey::NumpadAdd => (0x6B, 0, false),
        MkKey::NumpadSeparator => (0x6C, 0, false),
        MkKey::NumpadSubtract => (0x6D, 0, false),
        MkKey::NumpadDecimal => (0x6E, 0, false),
        MkKey::NumpadDivide => (0x6F, 0, true),
        MkKey::OemSemicolon => (0xBA, 0, false),
        MkKey::OemEquals => (0xBB, 0, false),
        MkKey::OemComma => (0xBC, 0, false),
        MkKey::OemMinus => (0xBD, 0, false),
        MkKey::OemPeriod => (0xBE, 0, false),
        MkKey::OemSlash => (0xBF, 0, false),
        MkKey::OemBacktick => (0xC0, 0, false),
        MkKey::OemLeftBracket => (0xDB, 0, false),
        MkKey::OemBackslash => (0xDC, 0, false),
        MkKey::OemRightBracket => (0xDD, 0, false),
        MkKey::OemQuote => (0xDE, 0, false),
        MkKey::Oem102 => (0xE2, 0, false),
        MkKey::BrowserBack => (0xA6, 0, true),
        MkKey::BrowserForward => (0xA7, 0, true),
        MkKey::BrowserRefresh => (0xA8, 0, true),
        MkKey::BrowserStop => (0xA9, 0, true),
        MkKey::BrowserSearch => (0xAA, 0, true),
        MkKey::BrowserFavorites => (0xAB, 0, true),
        MkKey::BrowserHome => (0xAC, 0, true),
        MkKey::VolumeMute => (0xAD, 0, true),
        MkKey::VolumeDown => (0xAE, 0, true),
        MkKey::VolumeUp => (0xAF, 0, true),
        MkKey::MediaNext => (0xB0, 0, true),
        MkKey::MediaPrevious => (0xB1, 0, true),
        MkKey::MediaStop => (0xB2, 0, true),
        MkKey::MediaPlayPause => (0xB3, 0, true),
        MkKey::LaunchMail => (0xB4, 0, true),
        MkKey::LaunchMediaSelect => (0xB5, 0, true),
        MkKey::LaunchApp1 => (0xB6, 0, true),
        MkKey::LaunchApp2 => (0xB7, 0, true),
        MkKey::RawVirtualKey {
            vk,
            scan_code,
            extended,
        } => (*vk, *scan_code, *extended),
        _ => return None,
    };
    Some(WindowsKeyMetadata {
        virtual_key,
        scan_code,
        extended,
    })
}

pub fn virtual_key(key: &MkKey) -> Option<u16> {
    windows_key_metadata(key).map(|metadata| metadata.virtual_key)
}

pub fn key_validation_error(key: &MkKey) -> Option<&'static str> {
    match key {
        MkKey::Character(value)
            if value.len() != 1 || !value.as_bytes()[0].is_ascii_alphanumeric() =>
        {
            Some("physical Character keys must be one ASCII letter or digit; use Text for Unicode")
        }
        MkKey::Function(number) if !(1..=24).contains(number) => {
            Some("Windows physical function keys are limited to F1 through F24")
        }
        MkKey::Numpad(number) if *number > 9 => Some("numpad digit must be between 0 and 9"),
        MkKey::RawVirtualKey {
            vk: 0,
            scan_code: 0,
            ..
        } => Some("raw Windows key requires a virtual key or scan code"),
        _ => None,
    }
}

pub fn mk_key_from_windows_event(vk: u32, scan_code: u32, extended: bool) -> MkKey {
    let raw = || MkKey::RawVirtualKey {
        vk: vk as u16,
        scan_code: scan_code as u16,
        extended,
    };
    let named = match vk {
        0x08 => MkKey::Backspace,
        0x09 => MkKey::Tab,
        0x0D => MkKey::Enter,
        0x10 => {
            if scan_code == 0x36 {
                MkKey::RightShift
            } else {
                MkKey::LeftShift
            }
        }
        0x11 => {
            if extended {
                MkKey::RightControl
            } else {
                MkKey::LeftControl
            }
        }
        0x12 => {
            if extended {
                MkKey::RightAlt
            } else {
                MkKey::LeftAlt
            }
        }
        0x13 => MkKey::PauseBreak,
        0x14 => MkKey::CapsLock,
        0x1B => MkKey::Escape,
        0x20 => MkKey::Space,
        0x21 => MkKey::PageUp,
        0x22 => MkKey::PageDown,
        0x23 => MkKey::End,
        0x24 => MkKey::Home,
        0x25 => MkKey::Left,
        0x26 => MkKey::Up,
        0x27 => MkKey::Right,
        0x28 => MkKey::Down,
        0x2C => MkKey::PrintScreen,
        0x2D => MkKey::Insert,
        0x2E => MkKey::Delete,
        0x5B => MkKey::LeftMeta,
        0x5C => MkKey::RightMeta,
        0x60..=0x69 => MkKey::Numpad((vk - 0x60) as u8),
        0x6A => MkKey::NumpadMultiply,
        0x6B => MkKey::NumpadAdd,
        0x6C => MkKey::NumpadSeparator,
        0x6D => MkKey::NumpadSubtract,
        0x6E => MkKey::NumpadDecimal,
        0x6F => MkKey::NumpadDivide,
        0x70..=0x87 => MkKey::Function((vk - 0x6F) as u8),
        0x90 => MkKey::NumLock,
        0x91 => MkKey::ScrollLock,
        0xA0 => MkKey::LeftShift,
        0xA1 => MkKey::RightShift,
        0xA2 => MkKey::LeftControl,
        0xA3 => MkKey::RightControl,
        0xA4 => MkKey::LeftAlt,
        0xA5 => MkKey::RightAlt,
        0xA6 => MkKey::BrowserBack,
        0xA7 => MkKey::BrowserForward,
        0xA8 => MkKey::BrowserRefresh,
        0xA9 => MkKey::BrowserStop,
        0xAA => MkKey::BrowserSearch,
        0xAB => MkKey::BrowserFavorites,
        0xAC => MkKey::BrowserHome,
        0xAD => MkKey::VolumeMute,
        0xAE => MkKey::VolumeDown,
        0xAF => MkKey::VolumeUp,
        0xB0 => MkKey::MediaNext,
        0xB1 => MkKey::MediaPrevious,
        0xB2 => MkKey::MediaStop,
        0xB3 => MkKey::MediaPlayPause,
        0xB4 => MkKey::LaunchMail,
        0xB5 => MkKey::LaunchMediaSelect,
        0xB6 => MkKey::LaunchApp1,
        0xB7 => MkKey::LaunchApp2,
        0xBA => MkKey::OemSemicolon,
        0xBB => MkKey::OemEquals,
        0xBC => MkKey::OemComma,
        0xBD => MkKey::OemMinus,
        0xBE => MkKey::OemPeriod,
        0xBF => MkKey::OemSlash,
        0xC0 => MkKey::OemBacktick,
        0xDB => MkKey::OemLeftBracket,
        0xDC => MkKey::OemBackslash,
        0xDD => MkKey::OemRightBracket,
        0xDE => MkKey::OemQuote,
        0xE2 => MkKey::Oem102,
        value if (0x30..=0x39).contains(&value) || (0x41..=0x5A).contains(&value) => {
            MkKey::Character(char::from_u32(value).unwrap().to_string())
        }
        _ => raw(),
    };
    // Scan codes are resolved by SendInput for named keys, but the extended
    // bit distinguishes physically different keys that share a VK (notably
    // main/numpad Enter and navigation/numpad navigation).
    if windows_key_metadata(&named).is_some_and(|metadata| metadata.extended == extended) {
        named
    } else {
        raw()
    }
}

/// Removes repeated physical keys without changing the first-authored order.
pub fn dedupe_chord(keys: &mut Vec<MkKey>) {
    let mut seen = HashSet::new();
    keys.retain(|key| seen.insert(key.clone()));
}

/// Complete named inventory offered by the authoring picker. F25-F35 are
/// intentionally absent because Windows SendInput has no matching VK values.
pub fn key_inventory() -> Vec<MkKey> {
    let mut keys = ('A'..='Z')
        .chain('0'..='9')
        .map(|c| MkKey::Character(c.to_string()))
        .collect::<Vec<_>>();
    keys.extend([
        MkKey::Control,
        MkKey::LeftControl,
        MkKey::RightControl,
        MkKey::Alt,
        MkKey::LeftAlt,
        MkKey::RightAlt,
        MkKey::Shift,
        MkKey::LeftShift,
        MkKey::RightShift,
        MkKey::Meta,
        MkKey::LeftMeta,
        MkKey::RightMeta,
        MkKey::Enter,
        MkKey::Tab,
        MkKey::Escape,
        MkKey::Space,
        MkKey::Backspace,
        MkKey::Delete,
        MkKey::Insert,
        MkKey::Up,
        MkKey::Down,
        MkKey::Left,
        MkKey::Right,
        MkKey::Home,
        MkKey::End,
        MkKey::PageUp,
        MkKey::PageDown,
        MkKey::CapsLock,
        MkKey::NumLock,
        MkKey::ScrollLock,
        MkKey::PrintScreen,
        MkKey::PauseBreak,
    ]);
    keys.extend((1..=24).map(MkKey::Function));
    keys.extend((0..=9).map(MkKey::Numpad));
    keys.extend([
        MkKey::NumpadMultiply,
        MkKey::NumpadAdd,
        MkKey::NumpadSeparator,
        MkKey::NumpadSubtract,
        MkKey::NumpadDecimal,
        MkKey::NumpadDivide,
        MkKey::OemSemicolon,
        MkKey::OemEquals,
        MkKey::OemComma,
        MkKey::OemMinus,
        MkKey::OemPeriod,
        MkKey::OemSlash,
        MkKey::OemBacktick,
        MkKey::OemLeftBracket,
        MkKey::OemBackslash,
        MkKey::OemRightBracket,
        MkKey::OemQuote,
        MkKey::Oem102,
        MkKey::BrowserBack,
        MkKey::BrowserForward,
        MkKey::BrowserRefresh,
        MkKey::BrowserStop,
        MkKey::BrowserSearch,
        MkKey::BrowserFavorites,
        MkKey::BrowserHome,
        MkKey::VolumeMute,
        MkKey::VolumeDown,
        MkKey::VolumeUp,
        MkKey::MediaNext,
        MkKey::MediaPrevious,
        MkKey::MediaStop,
        MkKey::MediaPlayPause,
        MkKey::LaunchMail,
        MkKey::LaunchMediaSelect,
        MkKey::LaunchApp1,
        MkKey::LaunchApp2,
    ]);
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::input::{
        InputSink, KEYEVENTF_EXTENDEDKEY_, KEYEVENTF_KEYUP_, KEYEVENTF_SCANCODE_,
        MKMACRO_EXTRA_INFO, RawInputEvent, Win32InputBackend,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn named_inventory_round_trips_and_every_entry_has_windows_metadata() {
        let inventory = key_inventory();
        assert!(!inventory.is_empty());
        assert_eq!(
            inventory
                .iter()
                .filter(|key| **key == MkKey::Insert)
                .count(),
            1
        );
        assert!(inventory.contains(&MkKey::Function(1)));
        assert!(inventory.contains(&MkKey::Function(24)));
        assert!(!inventory.contains(&MkKey::Function(25)));

        for key in inventory {
            let json = serde_json::to_string(&key).unwrap();
            assert_eq!(serde_json::from_str::<MkKey>(&json).unwrap(), key, "{json}");
            assert!(
                windows_key_metadata(&key).is_some(),
                "missing metadata for {key:?}"
            );
            assert!(
                key_validation_error(&key).is_none(),
                "invalid inventory key {key:?}"
            );
        }
    }

    #[test]
    fn raw_key_and_ordered_hotkey_serde_preserve_physical_identity() {
        let raw = MkKey::RawVirtualKey {
            vk: 0xE8,
            scan_code: 0x56,
            extended: true,
        };
        let keys = vec![
            MkKey::LeftControl,
            MkKey::RightShift,
            MkKey::Character("S".into()),
            raw.clone(),
        ];
        let json = serde_json::to_string(&keys).unwrap();
        assert_eq!(serde_json::from_str::<Vec<MkKey>>(&json).unwrap(), keys);
        let action = super::super::MkAction::Hotkey(keys.clone());
        let action_json = serde_json::to_string(&action).unwrap();
        assert_eq!(
            serde_json::from_str::<super::super::MkAction>(&action_json).unwrap(),
            action
        );
        assert_eq!(
            windows_key_metadata(&raw),
            Some(WindowsKeyMetadata {
                virtual_key: 0xE8,
                scan_code: 0x56,
                extended: true,
            })
        );
    }

    #[test]
    fn every_named_key_has_the_exact_windows_mapping() {
        for value in b'A'..=b'Z' {
            assert_eq!(
                virtual_key(&MkKey::Character(char::from(value).to_string())),
                Some(u16::from(value))
            );
        }
        for value in b'0'..=b'9' {
            assert_eq!(
                virtual_key(&MkKey::Character(char::from(value).to_string())),
                Some(u16::from(value))
            );
        }
        for number in 1..=24 {
            assert_eq!(
                virtual_key(&MkKey::Function(number)),
                Some(0x6F + u16::from(number))
            );
        }
        for number in 0..=9 {
            assert_eq!(
                virtual_key(&MkKey::Numpad(number)),
                Some(0x60 + u16::from(number))
            );
        }
        let cases = [
            (MkKey::Control, 0x11, false),
            (MkKey::LeftControl, 0xA2, false),
            (MkKey::RightControl, 0xA3, true),
            (MkKey::Shift, 0x10, false),
            (MkKey::LeftShift, 0xA0, false),
            (MkKey::RightShift, 0xA1, false),
            (MkKey::Alt, 0x12, false),
            (MkKey::LeftAlt, 0xA4, false),
            (MkKey::RightAlt, 0xA5, true),
            (MkKey::Meta, 0x5B, true),
            (MkKey::LeftMeta, 0x5B, true),
            (MkKey::RightMeta, 0x5C, true),
            (MkKey::Enter, 0x0D, false),
            (MkKey::Tab, 0x09, false),
            (MkKey::Escape, 0x1B, false),
            (MkKey::Space, 0x20, false),
            (MkKey::Backspace, 0x08, false),
            (MkKey::Insert, 0x2D, true),
            (MkKey::Delete, 0x2E, true),
            (MkKey::Up, 0x26, true),
            (MkKey::Down, 0x28, true),
            (MkKey::Left, 0x25, true),
            (MkKey::Right, 0x27, true),
            (MkKey::Home, 0x24, true),
            (MkKey::End, 0x23, true),
            (MkKey::PageUp, 0x21, true),
            (MkKey::PageDown, 0x22, true),
            (MkKey::PrintScreen, 0x2C, true),
            (MkKey::PauseBreak, 0x13, false),
            (MkKey::CapsLock, 0x14, false),
            (MkKey::NumLock, 0x90, true),
            (MkKey::ScrollLock, 0x91, false),
            (MkKey::NumpadMultiply, 0x6A, false),
            (MkKey::NumpadAdd, 0x6B, false),
            (MkKey::NumpadSeparator, 0x6C, false),
            (MkKey::NumpadSubtract, 0x6D, false),
            (MkKey::NumpadDecimal, 0x6E, false),
            (MkKey::NumpadDivide, 0x6F, true),
            (MkKey::OemSemicolon, 0xBA, false),
            (MkKey::OemEquals, 0xBB, false),
            (MkKey::OemComma, 0xBC, false),
            (MkKey::OemMinus, 0xBD, false),
            (MkKey::OemPeriod, 0xBE, false),
            (MkKey::OemSlash, 0xBF, false),
            (MkKey::OemBacktick, 0xC0, false),
            (MkKey::OemLeftBracket, 0xDB, false),
            (MkKey::OemBackslash, 0xDC, false),
            (MkKey::OemRightBracket, 0xDD, false),
            (MkKey::OemQuote, 0xDE, false),
            (MkKey::Oem102, 0xE2, false),
            (MkKey::BrowserBack, 0xA6, true),
            (MkKey::BrowserForward, 0xA7, true),
            (MkKey::BrowserRefresh, 0xA8, true),
            (MkKey::BrowserStop, 0xA9, true),
            (MkKey::BrowserSearch, 0xAA, true),
            (MkKey::BrowserFavorites, 0xAB, true),
            (MkKey::BrowserHome, 0xAC, true),
            (MkKey::VolumeMute, 0xAD, true),
            (MkKey::VolumeDown, 0xAE, true),
            (MkKey::VolumeUp, 0xAF, true),
            (MkKey::MediaNext, 0xB0, true),
            (MkKey::MediaPrevious, 0xB1, true),
            (MkKey::MediaStop, 0xB2, true),
            (MkKey::MediaPlayPause, 0xB3, true),
            (MkKey::LaunchMail, 0xB4, true),
            (MkKey::LaunchMediaSelect, 0xB5, true),
            (MkKey::LaunchApp1, 0xB6, true),
            (MkKey::LaunchApp2, 0xB7, true),
        ];
        for (key, virtual_key, extended) in cases {
            let metadata = windows_key_metadata(&key).unwrap();
            assert_eq!(metadata.virtual_key, virtual_key, "{key:?}");
            assert_eq!(metadata.extended, extended, "{key:?}");
        }
    }

    #[test]
    fn event_conversion_preserves_sided_modifiers_and_unknown_scan_metadata() {
        assert_eq!(
            mk_key_from_windows_event(0x10, 0x2A, false),
            MkKey::LeftShift
        );
        assert_eq!(
            mk_key_from_windows_event(0x10, 0x36, false),
            MkKey::RightShift
        );
        assert_eq!(
            mk_key_from_windows_event(0x11, 0x1D, true),
            MkKey::RightControl
        );
        assert_eq!(mk_key_from_windows_event(0x12, 0x38, true), MkKey::RightAlt);
        assert_eq!(
            mk_key_from_windows_event(0xE8, 0x56, true),
            MkKey::RawVirtualKey {
                vk: 0xE8,
                scan_code: 0x56,
                extended: true,
            }
        );
    }

    #[test]
    fn canonical_extended_forms_remain_named_keys() {
        let cases = [
            (0x0D, 0x1C, false, MkKey::Enter),
            (0x2D, 0x52, true, MkKey::Insert),
            (0x2E, 0x53, true, MkKey::Delete),
            (0x24, 0x47, true, MkKey::Home),
            (0x23, 0x4F, true, MkKey::End),
            (0x21, 0x49, true, MkKey::PageUp),
            (0x22, 0x51, true, MkKey::PageDown),
            (0x25, 0x4B, true, MkKey::Left),
            (0x27, 0x4D, true, MkKey::Right),
            (0x26, 0x48, true, MkKey::Up),
            (0x28, 0x50, true, MkKey::Down),
        ];
        for (vk, scan_code, extended, expected) in cases {
            assert_eq!(mk_key_from_windows_event(vk, scan_code, extended), expected);
        }
    }

    #[derive(Clone, Default)]
    struct RecordingInputSink(Arc<Mutex<Vec<RawInputEvent>>>);
    impl InputSink for RecordingInputSink {
        fn send(&self, events: &[RawInputEvent]) -> Result<usize, String> {
            self.0.lock().unwrap().extend_from_slice(events);
            Ok(events.len())
        }
    }

    #[test]
    fn noncanonical_keypad_forms_round_trip_to_exact_send_input_metadata() {
        let cases = [
            (0x0D, 0x1C, true),
            (0x2D, 0x52, false),
            (0x2E, 0x53, false),
            (0x24, 0x47, false),
            (0x23, 0x4F, false),
            (0x21, 0x49, false),
            (0x22, 0x51, false),
            (0x25, 0x4B, false),
            (0x27, 0x4D, false),
            (0x26, 0x48, false),
            (0x28, 0x50, false),
            (0x0C, 0x4C, false),
        ];
        for (vk, scan_code, extended) in cases {
            let key = mk_key_from_windows_event(vk, scan_code, extended);
            assert_eq!(
                key,
                MkKey::RawVirtualKey {
                    vk: vk as u16,
                    scan_code: scan_code as u16,
                    extended,
                }
            );
            assert_eq!(
                windows_key_metadata(&key),
                Some(WindowsKeyMetadata {
                    virtual_key: vk as u16,
                    scan_code: scan_code as u16,
                    extended,
                })
            );

            let sink = RecordingInputSink::default();
            Win32InputBackend::with_sink(sink.clone())
                .key_press(&key)
                .unwrap();
            let extended_flag = if extended { KEYEVENTF_EXTENDEDKEY_ } else { 0 };
            assert_eq!(
                *sink.0.lock().unwrap(),
                [
                    RawInputEvent::Keyboard {
                        vk: vk as u16,
                        scan: scan_code as u16,
                        flags: KEYEVENTF_SCANCODE_ | extended_flag,
                        extra: MKMACRO_EXTRA_INFO,
                    },
                    RawInputEvent::Keyboard {
                        vk: vk as u16,
                        scan: scan_code as u16,
                        flags: KEYEVENTF_SCANCODE_ | extended_flag | KEYEVENTF_KEYUP_,
                        extra: MKMACRO_EXTRA_INFO,
                    },
                ]
            );
        }
    }

    #[test]
    fn invalid_physical_keys_are_rejected_instead_of_becoming_text() {
        assert!(key_validation_error(&MkKey::Character("é".into())).is_some());
        assert!(key_validation_error(&MkKey::Character("AB".into())).is_some());
        assert!(key_validation_error(&MkKey::Function(25)).is_some());
        assert!(key_validation_error(&MkKey::Numpad(10)).is_some());
        assert!(
            key_validation_error(&MkKey::RawVirtualKey {
                vk: 0,
                scan_code: 0,
                extended: false,
            })
            .is_some()
        );
    }
}
