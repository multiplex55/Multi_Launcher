//! Pure recognition state for radial item shortcuts and hotstrings.
//!
//! Native hooks remain owned by `hotkey::launcher_invocation`; this module is
//! deliberately only the bounded, deterministic decision core used by that
//! existing listener.

use super::invocation::InputProvenance;
use super::model::{CellId, ClickGesture, MenuId, RadialDocument, TriggerScope};
use crate::hotkey::launcher_invocation::{KeyEvent, KeyTransition, vk_from_key};
use crate::hotkey::{Hotkey, parse_hotkey};
use std::collections::HashSet;

pub const HOTSTRING_SUFFIX_CAPACITY: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemInputTrigger {
    Shortcut(Hotkey),
    Hotstring { text: String, case_sensitive: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInputBinding {
    pub menu_id: MenuId,
    pub cell_id: CellId,
    pub gesture: ClickGesture,
    pub scope: TriggerScope,
    pub trigger: ItemInputTrigger,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInputMatch {
    pub binding: ItemInputBinding,
    pub primary_key: Option<u32>,
    pub provenance: InputProvenance,
    /// Only suppress the current hook event when it is the primary down. A
    /// modifier-first completion must remain balanced in the foreground app.
    pub consume_current: bool,
}

/// Compiles persisted item inputs into exact runtime identities. Global
/// entries require the feature opt-in and are dropped when they can co-fire
/// with an application-owned launcher/MkMacro reservation.
pub fn compile_item_inputs(
    document: &RadialDocument,
    allow_global: bool,
    reserved: &[Hotkey],
) -> Vec<ItemInputBinding> {
    let mut result = Vec::new();
    for menu in &document.menus {
        for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
            for shortcut in &cell.shortcuts {
                let Some(chord) = parse_hotkey(&shortcut.chord) else {
                    continue;
                };
                if shortcut.scope == TriggerScope::Global
                    && (!allow_global || reserved.iter().any(|other| can_cofire(&chord, other)))
                {
                    continue;
                }
                result.push(ItemInputBinding {
                    menu_id: menu.id.clone(),
                    cell_id: cell.id.clone(),
                    gesture: shortcut.gesture,
                    scope: shortcut.scope,
                    trigger: ItemInputTrigger::Shortcut(chord),
                });
            }
            for hotstring in &cell.hotstrings {
                if hotstring.scope == TriggerScope::Global && !allow_global {
                    continue;
                }
                result.push(ItemInputBinding {
                    menu_id: menu.id.clone(),
                    cell_id: cell.id.clone(),
                    gesture: hotstring.gesture,
                    scope: hotstring.scope,
                    trigger: ItemInputTrigger::Hotstring {
                        text: hotstring.text.clone(),
                        case_sensitive: hotstring.case_sensitive,
                    },
                });
            }
        }
    }
    result
}

pub fn can_cofire(left: &Hotkey, right: &Hotkey) -> bool {
    if left.key != right.key {
        return false;
    }
    let bare_caps = |hotkey: &Hotkey| {
        hotkey.key == crate::hotkey::Key::CapsLock
            && !hotkey.ctrl
            && !hotkey.shift
            && !hotkey.alt
            && !hotkey.alt_gr
            && !hotkey.win
    };
    (!bare_caps(left) && !bare_caps(right)) || (bare_caps(left) && bare_caps(right))
}

#[derive(Default)]
pub struct ItemInputRecognizer {
    down: HashSet<u32>,
    shortcut_latches: Vec<bool>,
    local_suffix: String,
    global_suffix: String,
    focus_token: Option<isize>,
    session_epoch: u64,
}

impl ItemInputRecognizer {
    pub fn clear(&mut self) {
        self.down.clear();
        self.shortcut_latches.clear();
        self.local_suffix.clear();
        self.global_suffix.clear();
    }

    pub fn transition(&mut self, focus_token: isize, session_epoch: u64, exclusive: bool) {
        if exclusive || self.focus_token != Some(focus_token) || self.session_epoch != session_epoch
        {
            self.clear();
        }
        self.focus_token = Some(focus_token);
        self.session_epoch = session_epoch;
    }

    pub fn observe_owned_cycle_event(&mut self, event: KeyEvent) {
        if event.provenance != InputProvenance::Physical {
            return;
        }
        match event.transition {
            KeyTransition::Up => {
                self.down.remove(&event.vk);
                self.shortcut_latches.fill(false);
            }
            KeyTransition::Down | KeyTransition::Repeat => {
                self.down.insert(event.vk);
            }
        }
    }

    pub fn process(
        &mut self,
        bindings: &[ItemInputBinding],
        active_menu: Option<&MenuId>,
        event: KeyEvent,
    ) -> Option<ItemInputMatch> {
        // Item inputs never accept injected keystrokes. This prevents all
        // app-owned SendInput paths from feeding the suffix buffer while also
        // avoiding accidental Talon/AHK expansion loops.
        if event.provenance != InputProvenance::Physical {
            return None;
        }
        if self.shortcut_latches.len() != bindings.len() {
            self.shortcut_latches = vec![false; bindings.len()];
        }
        if event.transition == KeyTransition::Up {
            self.down.remove(&event.vk);
        } else {
            self.down.insert(event.vk);
        }

        let eligible_scope = |binding: &ItemInputBinding| {
            binding.scope == TriggerScope::Global
                || active_menu.is_some_and(|menu| menu == &binding.menu_id)
        };
        let mut matched = None;
        for (index, binding) in bindings.iter().enumerate() {
            let ItemInputTrigger::Shortcut(chord) = &binding.trigger else {
                continue;
            };
            let eligible = eligible_scope(binding) && chord_is_down(&self.down, chord);
            if eligible && !self.shortcut_latches[index] && event.transition == KeyTransition::Down
            {
                matched = Some(ItemInputMatch {
                    binding: binding.clone(),
                    primary_key: vk_from_key(chord.key),
                    provenance: event.provenance,
                    consume_current: vk_from_key(chord.key) == Some(event.vk),
                });
            }
            self.shortcut_latches[index] = eligible;
        }
        if matched.is_some() {
            return matched;
        }

        let has_local_hotstrings = active_menu.is_some_and(|menu| {
            bindings.iter().any(|binding| {
                binding.scope == TriggerScope::MenuLocal
                    && &binding.menu_id == menu
                    && matches!(&binding.trigger, ItemInputTrigger::Hotstring { .. })
            })
        });
        let has_global_hotstrings = bindings.iter().any(|binding| {
            binding.scope == TriggerScope::Global
                && matches!(&binding.trigger, ItemInputTrigger::Hotstring { .. })
        });
        if !has_local_hotstrings {
            self.local_suffix.clear();
        }
        if !has_global_hotstrings {
            self.global_suffix.clear();
        }
        if !has_local_hotstrings && !has_global_hotstrings {
            return None;
        }

        if event.transition != KeyTransition::Down || has_command_modifier(&self.down) {
            return None;
        }
        if event.vk == 0x08 {
            if has_local_hotstrings {
                self.local_suffix.pop();
            }
            if has_global_hotstrings {
                self.global_suffix.pop();
            }
            return None;
        }
        if matches!(event.vk, 0x09 | 0x0D | 0x1B) {
            self.local_suffix.clear();
            self.global_suffix.clear();
            return None;
        }
        let Some(character) = printable_ascii(
            event.vk,
            self.down.contains(&0xA0) || self.down.contains(&0xA1),
        ) else {
            return None;
        };
        if has_local_hotstrings {
            push_suffix(&mut self.local_suffix, character);
        }
        if has_global_hotstrings {
            push_suffix(&mut self.global_suffix, character);
        }
        for binding in bindings {
            let ItemInputTrigger::Hotstring {
                text,
                case_sensitive,
            } = &binding.trigger
            else {
                continue;
            };
            if !eligible_scope(binding) {
                continue;
            }
            let suffix = if binding.scope == TriggerScope::Global {
                &self.global_suffix
            } else {
                &self.local_suffix
            };
            let matches = if *case_sensitive {
                suffix.ends_with(text)
            } else {
                suffix
                    .to_ascii_lowercase()
                    .ends_with(&text.to_ascii_lowercase())
            };
            if matches {
                if binding.scope == TriggerScope::Global {
                    self.global_suffix.clear();
                } else {
                    self.local_suffix.clear();
                }
                return Some(ItemInputMatch {
                    binding: binding.clone(),
                    primary_key: None,
                    provenance: event.provenance,
                    consume_current: false,
                });
            }
        }
        None
    }
}

fn push_suffix(suffix: &mut String, character: char) {
    suffix.push(character);
    if suffix.len() > HOTSTRING_SUFFIX_CAPACITY {
        let excess = suffix.len() - HOTSTRING_SUFFIX_CAPACITY;
        suffix.drain(..excess);
    }
}

fn chord_is_down(down: &HashSet<u32>, hotkey: &Hotkey) -> bool {
    let Some(primary) = vk_from_key(hotkey.key) else {
        return false;
    };
    if !down.contains(&primary) {
        return false;
    }
    let ctrl = down.contains(&0xA2) || down.contains(&0xA3);
    let shift = down.contains(&0xA0) || down.contains(&0xA1);
    let alt = down.contains(&0xA4) || down.contains(&0xA5);
    let win = down.contains(&0x5B) || down.contains(&0x5C);
    if hotkey.key == crate::hotkey::Key::CapsLock
        && !hotkey.ctrl
        && !hotkey.shift
        && !hotkey.alt
        && !hotkey.alt_gr
        && !hotkey.win
    {
        return !ctrl && !shift && !alt && !win;
    }
    (!hotkey.ctrl || ctrl)
        && (!hotkey.shift || shift)
        && (!hotkey.alt || alt)
        && (!hotkey.alt_gr || down.contains(&0xA5))
        && (!hotkey.win || win)
}

fn has_command_modifier(down: &HashSet<u32>) -> bool {
    down.contains(&0xA2)
        || down.contains(&0xA3)
        || down.contains(&0xA4)
        || down.contains(&0xA5)
        || down.contains(&0x5B)
        || down.contains(&0x5C)
}

fn printable_ascii(vk: u32, shift: bool) -> Option<char> {
    match vk {
        0x41..=0x5A => Some(if shift {
            char::from_u32(vk)?
        } else {
            char::from_u32(vk + 32)?
        }),
        0x30..=0x39 => {
            const SHIFTED_DIGITS: &[u8; 10] = b")!@#$%^&*(";
            if shift {
                Some(SHIFTED_DIGITS[(vk - 0x30) as usize] as char)
            } else {
                char::from_u32(vk)
            }
        }
        0x20 => Some(' '),
        0xBA => Some(if shift { ':' } else { ';' }),
        0xBB => Some(if shift { '+' } else { '=' }),
        0xBC => Some(if shift { '<' } else { ',' }),
        0xBD => Some(if shift { '_' } else { '-' }),
        0xBE => Some(if shift { '>' } else { '.' }),
        0xBF => Some(if shift { '?' } else { '/' }),
        0xC0 => Some(if shift { '~' } else { '`' }),
        0xDB => Some(if shift { '{' } else { '[' }),
        0xDC => Some(if shift { '|' } else { '\\' }),
        0xDD => Some(if shift { '}' } else { ']' }),
        0xDE => Some(if shift { '"' } else { '\'' }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::invocation::Timestamp;

    fn event(vk: u32, transition: KeyTransition, provenance: InputProvenance) -> KeyEvent {
        KeyEvent {
            vk,
            transition,
            at: Timestamp::default(),
            provenance,
        }
    }

    fn binding(gesture: ClickGesture, trigger: ItemInputTrigger) -> ItemInputBinding {
        ItemInputBinding {
            menu_id: MenuId::new("menu"),
            cell_id: CellId::new("cell"),
            gesture,
            scope: TriggerScope::MenuLocal,
            trigger,
        }
    }

    #[test]
    fn all_five_shortcut_variants_preserve_the_configured_gesture() {
        for gesture in [
            ClickGesture::Primary,
            ClickGesture::Secondary,
            ClickGesture::CtrlPrimary,
            ClickGesture::ShiftPrimary,
            ClickGesture::AltPrimary,
        ] {
            let bindings = vec![binding(
                gesture,
                ItemInputTrigger::Shortcut(parse_hotkey("Ctrl+K").unwrap()),
            )];
            let mut recognizer = ItemInputRecognizer::default();
            recognizer.transition(1, 1, false);
            recognizer.process(
                &bindings,
                Some(&MenuId::new("menu")),
                event(0xA2, KeyTransition::Down, InputProvenance::Physical),
            );
            let matched = recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x4B, KeyTransition::Down, InputProvenance::Physical),
                )
                .unwrap();
            assert_eq!(matched.binding.gesture, gesture);
        }
    }

    #[test]
    fn all_five_hotstring_variants_preserve_the_configured_gesture() {
        for gesture in [
            ClickGesture::Primary,
            ClickGesture::Secondary,
            ClickGesture::CtrlPrimary,
            ClickGesture::ShiftPrimary,
            ClickGesture::AltPrimary,
        ] {
            let bindings = vec![binding(
                gesture,
                ItemInputTrigger::Hotstring {
                    text: "go".into(),
                    case_sensitive: false,
                },
            )];
            let mut recognizer = ItemInputRecognizer::default();
            recognizer.transition(1, 1, false);
            assert!(
                recognizer
                    .process(
                        &bindings,
                        Some(&MenuId::new("menu")),
                        event(0x47, KeyTransition::Down, InputProvenance::Physical),
                    )
                    .is_none()
            );
            let matched = recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x4F, KeyTransition::Down, InputProvenance::Physical),
                )
                .unwrap();
            assert_eq!(matched.binding.gesture, gesture);
            assert!(matched.primary_key.is_none());
        }
    }

    #[test]
    fn local_scope_focus_session_and_injection_bound_the_suffix() {
        let bindings = vec![binding(
            ClickGesture::Primary,
            ItemInputTrigger::Hotstring {
                text: "abc".into(),
                case_sensitive: false,
            },
        )];
        let mut recognizer = ItemInputRecognizer::default();
        recognizer.transition(1, 1, false);
        for vk in [0x41, 0x42] {
            assert!(
                recognizer
                    .process(
                        &bindings,
                        Some(&MenuId::new("other")),
                        event(vk, KeyTransition::Down, InputProvenance::Physical)
                    )
                    .is_none()
            );
        }
        assert!(
            recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x43, KeyTransition::Down, InputProvenance::SelfInjected)
                )
                .is_none()
        );
        assert!(
            recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x43, KeyTransition::Down, InputProvenance::ExternalInjected)
                )
                .is_none()
        );
        recognizer.transition(2, 1, false);
        for vk in [0x41, 0x42, 0x43] {
            let result = recognizer.process(
                &bindings,
                Some(&MenuId::new("menu")),
                event(vk, KeyTransition::Down, InputProvenance::Physical),
            );
            if vk == 0x43 {
                assert!(result.is_some())
            } else {
                assert!(result.is_none())
            }
        }
        recognizer.transition(2, 2, false);
        assert!(
            recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x43, KeyTransition::Down, InputProvenance::Physical)
                )
                .is_none()
        );
        recognizer.transition(2, 2, true);
        assert!(recognizer.local_suffix.is_empty());
        assert!(recognizer.global_suffix.len() <= HOTSTRING_SUFFIX_CAPACITY);
    }

    #[test]
    fn external_typing_without_eligible_hotstrings_is_not_retained_or_consumed() {
        let local = binding(
            ClickGesture::Primary,
            ItemInputTrigger::Hotstring {
                text: "secret".into(),
                case_sensitive: false,
            },
        );
        let mut recognizer = ItemInputRecognizer::default();
        for vk in [0x53, 0x45, 0x43, 0x52, 0x45, 0x54] {
            assert!(
                recognizer
                    .process(
                        &[local.clone()],
                        None,
                        event(vk, KeyTransition::Down, InputProvenance::Physical)
                    )
                    .is_none()
            );
        }
        assert!(recognizer.local_suffix.is_empty());
        assert!(recognizer.global_suffix.is_empty());
        assert!(
            recognizer
                .process(
                    &[local],
                    Some(&MenuId::new("menu")),
                    event(0x54, KeyTransition::Down, InputProvenance::Physical)
                )
                .is_none()
        );
    }

    #[test]
    fn relinquishing_local_scope_clears_local_history_without_seeding_global_history() {
        let local = binding(
            ClickGesture::Primary,
            ItemInputTrigger::Hotstring {
                text: "ab".into(),
                case_sensitive: false,
            },
        );
        let mut global = local.clone();
        global.scope = TriggerScope::Global;
        global.cell_id = CellId::new("global");
        global.trigger = ItemInputTrigger::Hotstring {
            text: "abc".into(),
            case_sensitive: false,
        };
        let mut recognizer = ItemInputRecognizer::default();
        recognizer.process(
            &[local.clone()],
            Some(&MenuId::new("menu")),
            event(0x41, KeyTransition::Down, InputProvenance::Physical),
        );
        recognizer.process(
            &[local.clone()],
            None,
            event(0x42, KeyTransition::Down, InputProvenance::Physical),
        );
        assert!(recognizer.local_suffix.is_empty());
        assert!(
            recognizer
                .process(
                    &[global],
                    None,
                    event(0x43, KeyTransition::Down, InputProvenance::Physical)
                )
                .is_none()
        );
    }

    #[test]
    fn alt_gr_does_not_feed_hotstrings_and_exact_shortcuts_still_match() {
        let bindings = vec![binding(
            ClickGesture::AltPrimary,
            ItemInputTrigger::Shortcut(parse_hotkey("AltGr+K").unwrap()),
        )];
        let mut recognizer = ItemInputRecognizer::default();
        recognizer.transition(-10, 1, false);
        recognizer.process(
            &bindings,
            Some(&MenuId::new("menu")),
            event(0xA5, KeyTransition::Down, InputProvenance::Physical),
        );
        let matched = recognizer
            .process(
                &bindings,
                Some(&MenuId::new("menu")),
                event(0x4B, KeyTransition::Down, InputProvenance::Physical),
            )
            .unwrap();
        assert_eq!(matched.binding.gesture, ClickGesture::AltPrimary);
    }

    #[test]
    fn primary_first_shortcut_claim_does_not_suppress_a_delivered_key_cycle() {
        let bindings = vec![binding(
            ClickGesture::Primary,
            ItemInputTrigger::Shortcut(parse_hotkey("Ctrl+K").unwrap()),
        )];
        let mut recognizer = ItemInputRecognizer::default();
        recognizer.transition(1, 1, false);
        assert!(
            recognizer
                .process(
                    &bindings,
                    Some(&MenuId::new("menu")),
                    event(0x4B, KeyTransition::Down, InputProvenance::Physical),
                )
                .is_none()
        );
        let matched = recognizer
            .process(
                &bindings,
                Some(&MenuId::new("menu")),
                event(0xA2, KeyTransition::Down, InputProvenance::Physical),
            )
            .unwrap();
        assert_eq!(matched.primary_key, Some(0x4B));
        assert!(!matched.consume_current);
    }

    #[test]
    fn global_inputs_require_opt_in_and_respect_owned_hotkey_reservations() {
        let mut document = RadialDocument::starter();
        let cell = &mut document.menus[0].rings[0].cells[0];
        cell.shortcuts.push(super::super::model::ItemShortcut {
            id: super::super::model::ShortcutId::new("global"),
            chord: "Ctrl+K".into(),
            gesture: ClickGesture::Secondary,
            scope: TriggerScope::Global,
        });
        cell.hotstrings.push(super::super::model::ItemHotstring {
            id: super::super::model::HotstringId::new("global-text"),
            text: "launch".into(),
            gesture: ClickGesture::Primary,
            case_sensitive: false,
            scope: TriggerScope::Global,
        });
        assert!(compile_item_inputs(&document, false, &[]).is_empty());
        let reserved = [parse_hotkey("Ctrl+K").unwrap()];
        let compiled = compile_item_inputs(&document, true, &reserved);
        assert_eq!(compiled.len(), 1);
        assert!(matches!(
            compiled[0].trigger,
            ItemInputTrigger::Hotstring { .. }
        ));
    }
}
