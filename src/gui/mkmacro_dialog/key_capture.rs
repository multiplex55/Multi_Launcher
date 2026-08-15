//! Shared conversion of egui key events into platform-independent macro keys.

use crate::mkmacro::{MkHotkey, MkKey};
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapturedChord {
    Cancelled,
    /// Modifiers are ordered Control, Alt, Shift, Meta, followed by the primary key.
    Keys(Vec<MkKey>),
}

pub(crate) fn mk_key_from_egui(key: egui::Key) -> Option<MkKey> {
    use egui::Key::*;
    let character = match key {
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        _ => "",
    };
    if !character.is_empty() {
        return Some(MkKey::Character(character.into()));
    }
    Some(match key {
        Enter => MkKey::Enter,
        Tab => MkKey::Tab,
        Escape => MkKey::Escape,
        Space => MkKey::Space,
        Backspace => MkKey::Backspace,
        Delete => MkKey::Delete,
        ArrowUp => MkKey::Up,
        ArrowDown => MkKey::Down,
        ArrowLeft => MkKey::Left,
        ArrowRight => MkKey::Right,
        Home => MkKey::Home,
        End => MkKey::End,
        PageUp => MkKey::PageUp,
        PageDown => MkKey::PageDown,
        F1 => MkKey::Function(1),
        F2 => MkKey::Function(2),
        F3 => MkKey::Function(3),
        F4 => MkKey::Function(4),
        F5 => MkKey::Function(5),
        F6 => MkKey::Function(6),
        F7 => MkKey::Function(7),
        F8 => MkKey::Function(8),
        F9 => MkKey::Function(9),
        F10 => MkKey::Function(10),
        F11 => MkKey::Function(11),
        F12 => MkKey::Function(12),
        F13 => MkKey::Function(13),
        F14 => MkKey::Function(14),
        F15 => MkKey::Function(15),
        F16 => MkKey::Function(16),
        F17 => MkKey::Function(17),
        F18 => MkKey::Function(18),
        F19 => MkKey::Function(19),
        F20 => MkKey::Function(20),
        F21 => MkKey::Function(21),
        F22 => MkKey::Function(22),
        F23 => MkKey::Function(23),
        F24 => MkKey::Function(24),
        F25 => MkKey::Function(25),
        F26 => MkKey::Function(26),
        F27 => MkKey::Function(27),
        F28 => MkKey::Function(28),
        F29 => MkKey::Function(29),
        F30 => MkKey::Function(30),
        F31 => MkKey::Function(31),
        F32 => MkKey::Function(32),
        F33 => MkKey::Function(33),
        F34 => MkKey::Function(34),
        F35 => MkKey::Function(35),
        _ => return None,
    })
}

pub(crate) fn captured_chord(input: &egui::InputState) -> Option<CapturedChord> {
    input.events.iter().find_map(|event| {
        let egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } = event
        else {
            return None;
        };
        if *key == egui::Key::Escape {
            return Some(CapturedChord::Cancelled);
        }
        let primary = mk_key_from_egui(*key)?;
        let mut keys = Vec::with_capacity(5);
        if modifiers.ctrl {
            keys.push(MkKey::Control);
        }
        if modifiers.alt {
            keys.push(MkKey::Alt);
        }
        if modifiers.shift {
            keys.push(MkKey::Shift);
        }
        // `command` aliases Ctrl off macOS; mac_cmd uniquely identifies the Meta/Command key.
        if modifiers.mac_cmd {
            keys.push(MkKey::Meta);
        }
        if !keys.contains(&primary) {
            keys.push(primary);
        }
        Some(CapturedChord::Keys(keys))
    })
}

pub(crate) fn key_name(key: &MkKey) -> String {
    match key {
        MkKey::Character(v) => v.to_uppercase(),
        MkKey::Function(n) => format!("F{n}"),
        MkKey::PageUp => "Page Up".into(),
        MkKey::PageDown => "Page Down".into(),
        MkKey::LeftControl | MkKey::RightControl | MkKey::Control => "Ctrl".into(),
        MkKey::LeftAlt | MkKey::RightAlt | MkKey::Alt => "Alt".into(),
        MkKey::LeftShift | MkKey::RightShift | MkKey::Shift => "Shift".into(),
        MkKey::LeftMeta | MkKey::RightMeta | MkKey::Meta => "Meta".into(),
        other => format!("{other:?}"),
    }
}

pub(crate) fn hotkey_name(h: &MkHotkey) -> String {
    h.modifiers
        .iter()
        .chain(std::iter::once(&h.key))
        .map(key_name)
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_supported_keys_are_explicitly_translated() {
        let letters = [
            A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
        ];
        for (key, expected) in letters.into_iter().zip('A'..='Z') {
            assert_eq!(
                mk_key_from_egui(key),
                Some(MkKey::Character(expected.to_string()))
            );
        }
        let digits = [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
        for (key, expected) in digits.into_iter().zip('0'..='9') {
            assert_eq!(
                mk_key_from_egui(key),
                Some(MkKey::Character(expected.to_string()))
            );
        }
        let named = [
            (Enter, MkKey::Enter),
            (Tab, MkKey::Tab),
            (Space, MkKey::Space),
            (Backspace, MkKey::Backspace),
            (Delete, MkKey::Delete),
            (Escape, MkKey::Escape),
            (Home, MkKey::Home),
            (End, MkKey::End),
            (PageUp, MkKey::PageUp),
            (PageDown, MkKey::PageDown),
            (ArrowUp, MkKey::Up),
            (ArrowDown, MkKey::Down),
            (ArrowLeft, MkKey::Left),
            (ArrowRight, MkKey::Right),
        ];
        for (key, expected) in named {
            assert_eq!(mk_key_from_egui(key), Some(expected));
        }
        let functions = [
            F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19,
            F20, F21, F22, F23, F24, F25, F26, F27, F28, F29, F30, F31, F32, F33, F34, F35,
        ];
        for (number, key) in functions.into_iter().enumerate() {
            assert_eq!(
                mk_key_from_egui(key),
                Some(MkKey::Function(number as u8 + 1))
            );
        }
        assert_eq!(mk_key_from_egui(Insert), None);
    }

    fn capture(key: egui::Key, modifiers: egui::Modifiers) -> CapturedChord {
        let ctx = egui::Context::default();
        ctx.begin_frame(egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        });
        ctx.input(|i| captured_chord(i).unwrap())
    }

    #[test]
    fn chords_have_stable_modifiers_and_special_keys_are_not_clear() {
        assert_eq!(
            capture(
                M,
                egui::Modifiers {
                    ctrl: true,
                    alt: true,
                    ..Default::default()
                }
            ),
            CapturedChord::Keys(vec![
                MkKey::Control,
                MkKey::Alt,
                MkKey::Character("M".into())
            ])
        );
        assert_eq!(
            capture(
                Num2,
                egui::Modifiers {
                    shift: true,
                    ..Default::default()
                }
            ),
            CapturedChord::Keys(vec![MkKey::Shift, MkKey::Character("2".into())])
        );
        assert_eq!(
            capture(
                A,
                egui::Modifiers {
                    mac_cmd: true,
                    command: true,
                    ..Default::default()
                }
            ),
            CapturedChord::Keys(vec![MkKey::Meta, MkKey::Character("A".into())])
        );
        assert_eq!(
            capture(Backspace, Default::default()),
            CapturedChord::Keys(vec![MkKey::Backspace])
        );
        assert_eq!(
            capture(Delete, Default::default()),
            CapturedChord::Keys(vec![MkKey::Delete])
        );
        assert_eq!(
            capture(Escape, Default::default()),
            CapturedChord::Cancelled
        );
    }
    use egui::Key::*;
}
