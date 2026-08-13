//! Macro-hotkey validation and stable ID mapping.
use super::{MkHotkey, MkKey, MkMacroDocument};
use std::collections::{BTreeMap, HashMap};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyDiagnostic {
    pub macro_id: u64,
    pub message: String,
}
fn key_name(k: &MkKey) -> String {
    match k {
        MkKey::Character(s) => s.to_ascii_uppercase(),
        x => format!("{x:?}").to_ascii_uppercase(),
    }
}
pub fn canonical_hotkey(h: &MkHotkey) -> String {
    let mut mods = h.modifiers.iter().map(key_name).collect::<Vec<_>>();
    mods.sort();
    mods.dedup();
    mods.push(key_name(&h.key));
    mods.join("+")
}
pub fn validate_hotkeys(doc: &MkMacroDocument, reserved: &[(&str, &str)]) -> Vec<HotkeyDiagnostic> {
    let mut seen = HashMap::new();
    let reserved = reserved
        .iter()
        .map(|(n, h)| (h.to_ascii_uppercase().replace(' ', ""), *n))
        .collect::<HashMap<_, _>>();
    let mut out = Vec::new();
    for m in doc.macros.iter().filter(|m| m.enabled) {
        let Some(h) = &m.hotkey else { continue };
        let c = canonical_hotkey(h);
        if let Some(other) = seen.insert(c.clone(), m.id) {
            out.push(HotkeyDiagnostic {
                macro_id: m.id,
                message: format!("hotkey duplicates macro {other}"),
            })
        }
        if let Some(name) = reserved.get(&c) {
            out.push(HotkeyDiagnostic {
                macro_id: m.id,
                message: format!("hotkey conflicts with {name}"),
            })
        }
    }
    out
}
/// Rebuilt after every store update; IDs are deterministic and disabled macros are absent.
pub fn rebuild_hotkey_map(doc: &MkMacroDocument, first_registration_id: i32) -> BTreeMap<i32, u64> {
    let mut ids = doc
        .macros
        .iter()
        .filter(|m| m.enabled && m.hotkey.is_some())
        .map(|m| m.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.into_iter()
        .enumerate()
        .map(|(i, id)| (first_registration_id + i as i32, id))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkMacro, MkPlayback};
    fn mac(id: u64, on: bool) -> MkMacro {
        MkMacro {
            id,
            name: id.to_string(),
            description: String::new(),
            enabled: on,
            hotkey: Some(MkHotkey {
                key: MkKey::Character("K".into()),
                modifiers: vec![MkKey::Control],
            }),
            playback: MkPlayback::default(),
            steps: vec![],
        }
    }
    #[test]
    fn duplicates_disabled_and_stable_reload() {
        let d = MkMacroDocument {
            schema_version: 1,
            macros: vec![mac(9, true), mac(2, true), mac(1, false)],
        };
        assert_eq!(validate_hotkeys(&d, &[]).len(), 1);
        assert_eq!(
            rebuild_hotkey_map(&d, 100)
                .into_values()
                .collect::<Vec<_>>(),
            vec![2, 9]
        );
    }
    #[test]
    fn reserved_conflict() {
        let d = MkMacroDocument {
            schema_version: 1,
            macros: vec![mac(1, true)],
        };
        assert_eq!(
            validate_hotkeys(&d, &[("emergency stop", "CONTROL+K")]).len(),
            1
        )
    }
}
