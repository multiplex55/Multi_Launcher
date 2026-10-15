//! Pure validation for the main-owned launcher hotkey route.

use crate::hotkey::{Hotkey, Key, launcher_invocation, parse_hotkey, runtime};
use crate::mkmacro::{MkMacroDocument, hotkeys::HotkeyDiagnosticSeverity};
use crate::radial::model::{RadialDocument, TriggerScope};
use crate::screen_draw::ScreenDrawSettings;
use crate::settings::Settings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyRouteIssue {
    pub field: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedHotkeyRoutes {
    pub launcher: Hotkey,
    pub quit: Option<Hotkey>,
    pub help: Option<Hotkey>,
    pub screen_draw_launch: Option<Hotkey>,
    pub screen_draw_emergency: Option<Hotkey>,
    pub shared_invocation: bool,
    /// Process-level bindings handled by the polling listener. Empty when
    /// shared launcher invocation owns those bindings through the native hook.
    pub legacy_triggers: Vec<Hotkey>,
}

/// Validates the route selected by the proposed settings without mutating the
/// settings, radial document, macro document, or any active runtime service.
pub fn validate_hotkey_routes(
    settings: &Settings,
    radial_document: &RadialDocument,
    macro_document: Option<&MkMacroDocument>,
) -> Result<ValidatedHotkeyRoutes, Vec<HotkeyRouteIssue>> {
    let mut issues = Vec::new();
    let launcher = parse_configured(
        "launcher",
        settings.hotkey.as_deref().unwrap_or("F2"),
        &mut issues,
    )
    .unwrap_or_else(default_launcher_hotkey);
    let quit = settings
        .quit_hotkey
        .as_deref()
        .and_then(|value| parse_configured("quit", value, &mut issues));
    let help = settings
        .help_hotkey
        .as_deref()
        .and_then(|value| parse_configured("help", value, &mut issues));

    let screen_draw = match settings.plugin_settings.get("screen_draw") {
        Some(value) => match serde_json::from_value::<ScreenDrawSettings>(value.clone()) {
            Ok(screen_draw) => Some(screen_draw),
            Err(error) => {
                issues.push(HotkeyRouteIssue {
                    field: "screen_draw",
                    message: format!("Screen Draw settings could not be read: {error}"),
                });
                None
            }
        },
        None => Some(ScreenDrawSettings::default()),
    };
    let (screen_draw_launch, screen_draw_emergency) = screen_draw
        .map(|screen_draw| {
            let launch = screen_draw.launch_hotkey.as_ref().and_then(|chord| {
                parse_configured("Screen Draw launch", chord.as_str(), &mut issues)
            });
            let emergency = parse_configured(
                "Screen Draw emergency",
                screen_draw.emergency_hotkey.as_str(),
                &mut issues,
            );
            (launch, emergency)
        })
        .unwrap_or((None, None));

    let shared_invocation = settings.radial.enabled && settings.radial.shared_tap_hold;
    let mut process_bindings = vec![("launcher", launcher)];
    process_bindings.extend(quit.map(|hotkey| ("quit", hotkey)));
    process_bindings.extend(help.map(|hotkey| ("help", hotkey)));
    process_bindings.extend(screen_draw_launch.map(|hotkey| ("Screen Draw launch", hotkey)));
    process_bindings.extend(screen_draw_emergency.map(|hotkey| ("Screen Draw emergency", hotkey)));

    for left in 0..process_bindings.len() {
        for right in (left + 1)..process_bindings.len() {
            let (left_name, left_hotkey) = process_bindings[left];
            let (right_name, right_hotkey) = process_bindings[right];
            if hotkeys_can_cofire(left_hotkey, right_hotkey) {
                issues.push(HotkeyRouteIssue {
                    field: "hotkeys",
                    message: format!("{left_name} hotkey can co-fire with {right_name} hotkey"),
                });
            }
        }
    }

    let route_mapper = if shared_invocation {
        "shared input hook"
    } else {
        "legacy polling listener"
    };
    for (name, hotkey) in &process_bindings {
        let supported = if shared_invocation {
            launcher_invocation::vk_from_key(hotkey.key).is_some()
        } else {
            runtime::legacy_virtual_key(hotkey.key).is_some()
        };
        if !supported {
            issues.push(HotkeyRouteIssue {
                field: "hotkeys",
                message: format!(
                    "{name} hotkey primary key {:?} is unsupported by the selected {route_mapper}",
                    hotkey.key
                ),
            });
        }
    }

    let mut reservations = process_bindings
        .iter()
        .map(|(name, hotkey)| (name.to_string(), display_hotkey(*hotkey)))
        .collect::<Vec<_>>();
    if settings.radial.enabled {
        let mut radial_reservations = Vec::new();
        validate_radial_hotkeys(
            settings,
            radial_document,
            &mut radial_reservations,
            &mut issues,
        );
        let radial_issues =
            crate::radial::settings::validate(&settings.radial, radial_document, &reservations);
        issues.extend(radial_issues.into_iter().map(|issue| HotkeyRouteIssue {
            field: issue.field,
            message: issue.message,
        }));
        reservations.extend(radial_reservations);
    }

    if let Some(document) = macro_document {
        let reserved_refs = reservations
            .iter()
            .map(|(name, chord)| (name.as_str(), chord.as_str()))
            .collect::<Vec<_>>();
        for diagnostic in crate::mkmacro::hotkeys::validate_hotkeys(document, &reserved_refs) {
            if diagnostic.severity == HotkeyDiagnosticSeverity::Error
                && diagnostic.message.starts_with("hotkey conflicts with ")
            {
                issues.push(HotkeyRouteIssue {
                    field: "macro_hotkeys",
                    message: format!("macro {} {}", diagnostic.macro_id, diagnostic.message),
                });
            }
        }
    }

    if issues.is_empty() {
        let legacy_triggers = if shared_invocation {
            Vec::new()
        } else {
            process_bindings.iter().map(|(_, hotkey)| *hotkey).collect()
        };
        Ok(ValidatedHotkeyRoutes {
            launcher,
            quit,
            help,
            screen_draw_launch,
            screen_draw_emergency,
            shared_invocation,
            legacy_triggers,
        })
    } else {
        Err(issues)
    }
}

/// Same subset-modifier matching rule used by `HotkeyTrigger`, including the
/// special bare-CapsLock exclusion from modified CapsLock chords.
pub fn hotkeys_can_cofire(left: Hotkey, right: Hotkey) -> bool {
    if left.key != right.key {
        return false;
    }
    let unmodified_caps_lock = |hotkey: Hotkey| {
        hotkey.key == Key::CapsLock
            && !hotkey.ctrl
            && !hotkey.shift
            && !hotkey.alt
            && !hotkey.alt_gr
            && !hotkey.win
    };
    let left_exact_caps_lock = unmodified_caps_lock(left);
    let right_exact_caps_lock = unmodified_caps_lock(right);
    (!left_exact_caps_lock && !right_exact_caps_lock)
        || (left_exact_caps_lock && right_exact_caps_lock)
}

fn parse_configured(
    field: &'static str,
    value: &str,
    issues: &mut Vec<HotkeyRouteIssue>,
) -> Option<Hotkey> {
    match parse_hotkey(value) {
        Some(hotkey) => Some(hotkey),
        None => {
            issues.push(HotkeyRouteIssue {
                field,
                message: format!("{field} hotkey `{value}` is invalid"),
            });
            None
        }
    }
}

fn validate_radial_hotkeys(
    settings: &Settings,
    document: &RadialDocument,
    reservations: &mut Vec<(String, String)>,
    issues: &mut Vec<HotkeyRouteIssue>,
) {
    let mut validate_radial_chord = |name: String, chord: &str| {
        let Some(hotkey) = parse_hotkey(chord) else {
            issues.push(HotkeyRouteIssue {
                field: "radial_hotkeys",
                message: format!("{name} hotkey `{chord}` is invalid"),
            });
            return;
        };
        if launcher_invocation::vk_from_key(hotkey.key).is_none() {
            issues.push(HotkeyRouteIssue {
                field: "radial_hotkeys",
                message: format!(
                    "{name} primary key {:?} is unsupported by the shared input hook",
                    hotkey.key
                ),
            });
        }
        reservations.push((name, display_hotkey(hotkey)));
    };

    for trigger in document
        .custom_triggers
        .iter()
        .filter(|trigger| trigger.scope == TriggerScope::Global)
    {
        validate_radial_chord(
            format!("radial trigger {}", trigger.id.as_str()),
            &trigger.chord,
        );
    }
    if settings.radial.global_item_inputs {
        for shortcut in document
            .menus
            .iter()
            .flat_map(|menu| &menu.rings)
            .flat_map(|ring| &ring.cells)
            .flat_map(|cell| &cell.shortcuts)
            .filter(|shortcut| shortcut.scope == TriggerScope::Global)
        {
            validate_radial_chord(
                format!("radial item shortcut {}", shortcut.id.as_str()),
                &shortcut.chord,
            );
        }
    }
}

fn display_hotkey(hotkey: Hotkey) -> String {
    let mut parts = Vec::new();
    if hotkey.ctrl {
        parts.push("Ctrl".to_string());
    }
    if hotkey.shift {
        parts.push("Shift".to_string());
    }
    if hotkey.alt {
        parts.push("Alt".to_string());
    }
    if hotkey.alt_gr {
        parts.push("AltGr".to_string());
    }
    if hotkey.win {
        parts.push("Win".to_string());
    }
    let key = match hotkey.key {
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::F13 => "F13",
        Key::F14 => "F14",
        Key::F15 => "F15",
        Key::F16 => "F16",
        Key::F17 => "F17",
        Key::F18 => "F18",
        Key::F19 => "F19",
        Key::F20 => "F20",
        Key::F21 => "F21",
        Key::F22 => "F22",
        Key::F23 => "F23",
        Key::F24 => "F24",
        Key::KeyA => "A",
        Key::KeyB => "B",
        Key::KeyC => "C",
        Key::KeyD => "D",
        Key::KeyE => "E",
        Key::KeyF => "F",
        Key::KeyG => "G",
        Key::KeyH => "H",
        Key::KeyI => "I",
        Key::KeyJ => "J",
        Key::KeyK => "K",
        Key::KeyL => "L",
        Key::KeyM => "M",
        Key::KeyN => "N",
        Key::KeyO => "O",
        Key::KeyP => "P",
        Key::KeyQ => "Q",
        Key::KeyR => "R",
        Key::KeyS => "S",
        Key::KeyT => "T",
        Key::KeyU => "U",
        Key::KeyV => "V",
        Key::KeyW => "W",
        Key::KeyX => "X",
        Key::KeyY => "Y",
        Key::KeyZ => "Z",
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::Escape => "Escape",
        Key::Space => "Space",
        Key::Return => "Enter",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::LeftArrow => "Left",
        Key::RightArrow => "Right",
        Key::UpArrow => "Up",
        Key::DownArrow => "Down",
        Key::CapsLock => "CapsLock",
        unsupported => return format!("{}+{:?}", parts.join("+"), unsupported),
    };
    parts.push(key.to_string());
    parts.join("+")
}

fn default_launcher_hotkey() -> Hotkey {
    Hotkey {
        key: Key::F2,
        ctrl: false,
        shift: false,
        alt: false,
        alt_gr: false,
        win: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkHotkey, MkHotkeyScope, MkKey, MkMacro};
    use crate::radial::model::{RadialDocument, TriggerDefinition, TriggerId};

    fn empty_macro() -> MkMacroDocument {
        MkMacroDocument::default()
    }

    #[test]
    fn selected_legacy_route_rejects_keys_the_polling_listener_cannot_map() {
        let mut settings = Settings::default();
        settings.radial.enabled = false;
        settings.hotkey = Some("Ctrl+F13".into());
        let issues =
            validate_hotkey_routes(&settings, &RadialDocument::starter(), None).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue
                .message
                .contains("unsupported by the selected legacy polling listener")
        }));
    }

    #[test]
    fn shared_route_accepts_f13_which_the_native_hook_can_map() {
        let mut settings = Settings::default();
        settings.hotkey = Some("Ctrl+F13".into());
        settings.radial.enabled = true;
        settings.radial.shared_tap_hold = true;
        let routes = validate_hotkey_routes(&settings, &RadialDocument::starter(), None).unwrap();
        assert!(routes.shared_invocation);
        assert!(routes.legacy_triggers.is_empty());
    }

    #[test]
    fn enabled_process_bindings_reject_invalid_values_and_cofire_conflicts() {
        let mut settings = Settings::default();
        settings.hotkey = Some("Ctrl+K".into());
        settings.quit_hotkey = Some("Shift+K".into());
        settings.help_hotkey = Some("NotAKey".into());
        let issues =
            validate_hotkey_routes(&settings, &RadialDocument::starter(), None).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue
                .message
                .contains("launcher hotkey can co-fire with quit hotkey")
        }));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("help hotkey `NotAKey` is invalid"))
        );
    }

    #[test]
    fn bare_caps_lock_preserves_the_legacy_modified_chord_exception() {
        let mut settings = Settings::default();
        settings.hotkey = Some("CapsLock".into());
        settings.quit_hotkey = Some("Ctrl+CapsLock".into());
        assert!(validate_hotkey_routes(&settings, &RadialDocument::starter(), None).is_ok());
    }

    #[test]
    fn screen_draw_and_radial_direct_triggers_are_checked_against_process_routes() {
        let mut settings = Settings::default();
        settings.hotkey = Some("Ctrl+Shift+D".into());
        settings.plugin_settings.insert(
            "screen_draw".into(),
            serde_json::json!({
                "launch_hotkey": "Ctrl+D",
                "emergency_hotkey": "Ctrl+Shift+F12"
            }),
        );
        settings.radial.enabled = true;
        let mut radial = RadialDocument::starter();
        radial.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("launcher-conflict"),
            chord: "Alt+Shift+D".into(),
            menu_id: radial.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        let issues = validate_hotkey_routes(&settings, &radial, None).unwrap_err();
        assert!(issues.iter().any(|issue| {
            issue
                .message
                .contains("launcher hotkey can co-fire with Screen Draw launch hotkey")
        }));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("can co-fire with launcher"))
        );
    }

    #[test]
    fn nonconflicting_global_radial_trigger_and_item_shortcut_validate() {
        let mut settings = Settings::default();
        settings.radial.enabled = true;
        settings.radial.global_item_inputs = true;
        let mut radial = RadialDocument::starter();
        radial.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("independent-trigger"),
            chord: "Ctrl+F3".into(),
            menu_id: radial.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        radial.menus[0].rings[0].cells[0]
            .shortcuts
            .push(crate::radial::model::ItemShortcut {
                id: crate::radial::model::ShortcutId::new("independent-item"),
                chord: "Ctrl+F4".into(),
                gesture: crate::radial::model::ClickGesture::Primary,
                scope: TriggerScope::Global,
            });

        assert!(validate_hotkey_routes(&settings, &radial, None).is_ok());
    }

    #[test]
    fn macro_reservations_use_the_existing_macro_hotkey_validator_without_mutation() {
        let mut settings = Settings::default();
        settings.hotkey = Some("Ctrl+K".into());
        let radial = RadialDocument::starter();
        let original_settings = settings.clone();
        let original_radial = radial.clone();
        let macro_document = MkMacroDocument {
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 5,
                name: "existing macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: Some(MkHotkey {
                    key: MkKey::Character("K".into()),
                    modifiers: vec![MkKey::Control],
                }),
                hotkey_scope: MkHotkeyScope::AnyWindow,
                folder_id: None,
                playback: Default::default(),
                steps: vec![],
            }],
            ..Default::default()
        };
        let issues = validate_hotkey_routes(&settings, &radial, Some(&macro_document)).unwrap_err();
        assert!(issues.iter().any(|issue| issue.field == "macro_hotkeys"));
        assert_eq!(settings.hotkey, original_settings.hotkey);
        assert_eq!(radial, original_radial);
    }

    #[test]
    fn altgr_process_route_conflicts_with_unmodified_macro_hotkey() {
        let mut settings = Settings::default();
        settings.hotkey = Some("AltGr+K".into());
        let macro_document = MkMacroDocument {
            macros: vec![MkMacro {
                signature: Default::default(),
                id: 6,
                name: "plain K macro".into(),
                description: String::new(),
                enabled: true,
                hotkey: Some(MkHotkey {
                    key: MkKey::Character("K".into()),
                    modifiers: vec![],
                }),
                hotkey_scope: MkHotkeyScope::AnyWindow,
                folder_id: None,
                playback: Default::default(),
                steps: vec![],
            }],
            ..Default::default()
        };

        let issues =
            validate_hotkey_routes(&settings, &RadialDocument::starter(), Some(&macro_document))
                .unwrap_err();
        assert!(issues.iter().any(|issue| issue.field == "macro_hotkeys"));
    }

    #[test]
    fn disabling_radial_preserves_invalid_legacy_values_for_later_repair() {
        let mut settings = Settings::default();
        settings.radial.enabled = false;
        settings.radial.hold_threshold_ms = 0;
        settings.radial.default_item_input_scope = TriggerScope::Global;
        settings.radial.global_item_inputs = false;
        assert!(
            validate_hotkey_routes(&settings, &RadialDocument::starter(), Some(&empty_macro()))
                .is_ok()
        );
    }
}
