//! Pure settings validation and presentation shared by the settings UI and main.

use super::item_input::can_cofire;
use super::model::{RadialDocument, RadialFeatureSettings, TriggerScope};
use crate::hotkey::parse_hotkey;

pub const MIN_HOLD_THRESHOLD_MS: u64 = 100;
pub const MAX_HOLD_THRESHOLD_MS: u64 = 2_000;
pub const MIN_TOOLTIP_DELAY_MS: u64 = 0;
pub const MAX_TOOLTIP_DELAY_MS: u64 = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadialSettingsIssue {
    pub field: &'static str,
    pub message: String,
}

/// Publication invariant independent of runtime enablement. A configured
/// settings override must never be made dangling by authoring or package
/// publication, even while the radial feature is temporarily disabled.
pub fn validate_publication_default(
    settings: &RadialFeatureSettings,
    document: &RadialDocument,
) -> Result<(), RadialSettingsIssue> {
    let Some(default_menu) = settings.default_menu_id.as_ref() else {
        return Ok(());
    };
    if document.menus.iter().any(|menu| &menu.id == default_menu) {
        Ok(())
    } else {
        Err(RadialSettingsIssue {
            field: "default_menu_id",
            message: format!(
                "Cannot publish radial menus because configured default `{default_menu}` would be missing"
            ),
        })
    }
}

pub fn validate(
    settings: &RadialFeatureSettings,
    document: &RadialDocument,
    reserved: &[(String, String)],
) -> Vec<RadialSettingsIssue> {
    let mut issues = Vec::new();
    if !settings.enabled {
        return issues;
    }
    if !(MIN_HOLD_THRESHOLD_MS..=MAX_HOLD_THRESHOLD_MS).contains(&settings.hold_threshold_ms) {
        issues.push(RadialSettingsIssue {
            field: "hold_threshold_ms",
            message: format!(
                "Hold threshold must be between {MIN_HOLD_THRESHOLD_MS} and {MAX_HOLD_THRESHOLD_MS} ms"
            ),
        });
    }
    if !(MIN_TOOLTIP_DELAY_MS..=MAX_TOOLTIP_DELAY_MS).contains(&settings.tooltip_delay_ms) {
        issues.push(RadialSettingsIssue {
            field: "tooltip_delay_ms",
            message: format!(
                "Tooltip delay must be between {MIN_TOOLTIP_DELAY_MS} and {MAX_TOOLTIP_DELAY_MS} ms"
            ),
        });
    }

    let default_menu = settings.effective_default_menu_id(document);
    if !document.menus.iter().any(|menu| menu.id == default_menu) {
        issues.push(RadialSettingsIssue {
            field: "default_menu_id",
            message: format!("Default radial menu `{default_menu}` does not exist"),
        });
    }

    let reserved = reserved
        .iter()
        .filter_map(|(owner, chord)| parse_hotkey(chord).map(|hotkey| (owner, hotkey)))
        .collect::<Vec<_>>();
    let mut radial = Vec::new();
    for trigger in document
        .custom_triggers
        .iter()
        .filter(|trigger| trigger.scope == TriggerScope::Global)
    {
        if let Some(hotkey) = parse_hotkey(&trigger.chord) {
            radial.push((
                format!("radial trigger {}", trigger.id.as_str()),
                trigger.chord.as_str(),
                hotkey,
            ));
        }
    }
    if settings.global_item_inputs {
        for (menu, cell, shortcut) in document
            .menus
            .iter()
            .flat_map(|menu| {
                menu.rings
                    .iter()
                    .flat_map(move |ring| ring.cells.iter().map(move |cell| (menu, cell)))
            })
            .flat_map(|(menu, cell)| {
                cell.shortcuts
                    .iter()
                    .filter(|shortcut| shortcut.scope == TriggerScope::Global)
                    .map(move |shortcut| (menu, cell, shortcut))
            })
        {
            if let Some(hotkey) = parse_hotkey(&shortcut.chord) {
                radial.push((
                    format!("radial item {} / {}", menu.id.as_str(), cell.id.as_str()),
                    shortcut.chord.as_str(),
                    hotkey,
                ));
            }
        }
    }

    for (owner, chord, hotkey) in &radial {
        for (reserved_owner, reserved_hotkey) in &reserved {
            if can_cofire(hotkey, reserved_hotkey) {
                issues.push(RadialSettingsIssue {
                    field: "input_scope",
                    message: format!("{owner} hotkey `{chord}` can co-fire with {reserved_owner}"),
                });
            }
        }
    }
    for left in 0..radial.len() {
        for right in (left + 1)..radial.len() {
            if can_cofire(&radial[left].2, &radial[right].2) {
                issues.push(RadialSettingsIssue {
                    field: "input_scope",
                    message: format!(
                        "{} hotkey `{}` can co-fire with {}",
                        radial[left].0, radial[left].1, radial[right].0
                    ),
                });
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::{
        ClickGesture, ItemShortcut, MenuId, ShortcutId, TriggerDefinition, TriggerId,
    };

    #[test]
    fn validates_threshold_default_menu_and_global_cofire() {
        let mut document = RadialDocument::starter();
        let mut settings = RadialFeatureSettings::default();
        settings.hold_threshold_ms = 99;
        settings.default_menu_id = Some(MenuId::new("missing"));
        settings.global_item_inputs = true;
        document.custom_triggers.push(TriggerDefinition {
            id: TriggerId::new("direct"),
            chord: "Ctrl+R".into(),
            menu_id: document.default_menu_id.clone(),
            scope: TriggerScope::Global,
        });
        document.menus[0].rings[0].cells[0]
            .shortcuts
            .push(ItemShortcut {
                id: ShortcutId::new("item"),
                chord: "Ctrl+Shift+R".into(),
                gesture: ClickGesture::Primary,
                scope: TriggerScope::Global,
            });
        let issues = validate(
            &settings,
            &document,
            &[("launcher toggle".into(), "Alt+R".into())],
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "hold_threshold_ms")
        );
        assert!(issues.iter().any(|issue| issue.field == "default_menu_id"));
        assert!(
            issues
                .iter()
                .filter(|issue| issue.field == "input_scope")
                .count()
                >= 2
        );
    }

    #[test]
    fn authoring_and_package_publication_never_dangle_configured_default_when_disabled() {
        let mut settings = RadialFeatureSettings::default();
        settings.enabled = false;
        settings.default_menu_id = Some(MenuId::new("configured"));
        let document = RadialDocument::starter();
        assert!(validate(&settings, &document, &[]).is_empty());
        assert!(validate_publication_default(&settings, &document).is_err());
        let package_candidate = document.clone();
        assert!(validate_publication_default(&settings, &package_candidate).is_err());

        let mut candidate = document;
        let mut configured = candidate.menus[0].clone();
        configured.id = MenuId::new("configured");
        candidate.menus.push(configured);
        assert_eq!(validate_publication_default(&settings, &candidate), Ok(()));
    }
}
