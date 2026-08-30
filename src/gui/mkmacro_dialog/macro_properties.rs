use super::MkMacroDialog;
use super::key_capture::{apply_captured_hotkey, captured_chord, chord_hotkey, hotkey_name};
use super::window_picker::{MatcherDestination, MatcherEditRequest};
use crate::mkmacro::{MkHotkeyScope, MkWindowMatcher};
use eframe::egui;

fn clear_hotkey(hotkey: &mut Option<crate::mkmacro::MkHotkey>) -> bool {
    hotkey.take().is_some()
}

fn set_hotkey_scope_enabled(scope: &mut MkHotkeyScope, enabled: bool) -> bool {
    let next = if enabled {
        match scope {
            MkHotkeyScope::AnyWindow => MkHotkeyScope::ActiveWindow(MkWindowMatcher::default()),
            MkHotkeyScope::ActiveWindow(_) => return false,
        }
    } else {
        MkHotkeyScope::AnyWindow
    };
    if *scope == next {
        false
    } else {
        *scope = next;
        true
    }
}

fn edit_active_matcher(scope: &mut MkHotkeyScope, edit: impl FnOnce(&mut MkWindowMatcher)) -> bool {
    let MkHotkeyScope::ActiveWindow(matcher) = scope else {
        return false;
    };
    let before = matcher.clone();
    edit(matcher);
    *matcher != before
}

pub(super) fn show(ui: &mut egui::Ui, d: &mut MkMacroDialog) {
    ui.horizontal(|ui| {
        ui.label("Record Toggle:");
        let label = hotkey_name(&d.draft.settings.record_toggle_hotkey);
        if ui
            .button(if d.record_hotkey_capture {
                "Press a key…"
            } else {
                &label
            })
            .clicked()
        {
            d.record_hotkey_capture = true;
        }
    });
    if d.record_hotkey_capture {
        if let Some(chord) = ui.input(captured_chord) {
            d.record_hotkey_capture = false;
            if let Some(hotkey) = chord_hotkey(chord) {
                if crate::mkmacro::hotkeys::compile_hotkey(&hotkey).is_some()
                    && d.draft.settings.record_toggle_hotkey != hotkey
                {
                    d.draft.settings.record_toggle_hotkey = hotkey;
                    d.mark_dirty();
                }
            }
        }
    }
    let control = crate::mkmacro::hotkeys::canonical_hotkey(&d.draft.settings.record_toggle_hotkey);
    let conflicts =
        crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[("mkmacro record toggle", &control)]);
    if !conflicts.is_empty() {
        ui.colored_label(
            egui::Color32::YELLOW,
            "hotkey conflicts with an enabled macro",
        );
    }
    ui.separator();
    let capturing = d.hotkey_capture;
    let selected_macro_id = d.selected_macro_id;
    let mut changed = false;
    let mut capture = None;
    let mut clear = false;
    let mut picker_original = None;
    let Some(m) = d.selected_macro_mut() else {
        ui.label("Select a macro");
        return;
    };
    ui.heading("Macro Properties");
    changed |= ui.text_edit_singleline(&mut m.name).changed();
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut m.description)
                .desired_rows(2)
                .hint_text("Description"),
        )
        .changed();
    changed |= ui.checkbox(&mut m.enabled, "Enabled").changed();
    ui.horizontal(|ui| {
        ui.label("Hotkey:");
        let label = m
            .hotkey
            .as_ref()
            .map(hotkey_name)
            .unwrap_or_else(|| "None".into());
        if ui
            .button(if capturing { "Press a key…" } else { &label })
            .clicked()
        {
            capture = Some(true);
        }
        if m.hotkey.is_some() && ui.small_button("Clear").clicked() {
            clear = true;
        }
    });
    let mut scope_enabled = matches!(m.hotkey_scope, MkHotkeyScope::ActiveWindow(_));
    if ui
        .checkbox(
            &mut scope_enabled,
            "Only activate this hotkey when this window is active",
        )
        .changed()
    {
        changed |= set_hotkey_scope_enabled(&mut m.hotkey_scope, scope_enabled);
    }
    if matches!(m.hotkey_scope, MkHotkeyScope::ActiveWindow(_)) {
        let mut matcher_ui_changed = false;
        changed |= edit_active_matcher(&mut m.hotkey_scope, |matcher| {
            let (fields_changed, picked) =
                super::action_editor::matcher_ui_with_change(ui, matcher);
            matcher_ui_changed |= fields_changed;
            if picked {
                picker_original = Some(matcher.clone());
            }
        });
        changed |= matcher_ui_changed;
    }
    egui::CollapsingHeader::new("Playback Settings").show(ui, |ui| {
        changed |= ui
            .add(egui::Slider::new(&mut m.playback.speed_percent, 1..=1000).text("Speed %"))
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut m.playback.random_delay_ms)
                    .clamp_range(0..=60_000)
                    .suffix(" ms random delay"),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut m.playback.random_offset_px)
                    .clamp_range(0..=10_000)
                    .suffix(" px random offset"),
            )
            .changed();
    });
    if clear {
        changed |= clear_hotkey(&mut m.hotkey);
    }
    let _ = m;
    if let (Some(macro_id), Some(original)) = (selected_macro_id, picker_original) {
        d.window_picker.open(MatcherEditRequest {
            destination: MatcherDestination::MacroHotkeyScope { macro_id },
            original,
        });
    }
    if capture == Some(true) {
        d.hotkey_capture = true;
    }
    if capturing {
        if let Some(result) = ui.input(captured_chord) {
            d.hotkey_capture = false;
            if let Some(m) = d.selected_macro_mut() {
                changed |= apply_captured_hotkey(&mut m.hotkey, result);
            }
        }
    }
    if let Some(w) =
        crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[("mkmacro record toggle", &control)])
            .into_iter()
            .find(|x| Some(x.macro_id) == d.selected_macro_id)
    {
        ui.colored_label(egui::Color32::YELLOW, w.message);
    }
    if changed {
        d.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkHotkey, MkKey};

    #[test]
    fn enabling_scope_creates_a_default_matcher() {
        let mut scope = MkHotkeyScope::AnyWindow;
        assert!(set_hotkey_scope_enabled(&mut scope, true));
        assert_eq!(
            scope,
            MkHotkeyScope::ActiveWindow(MkWindowMatcher::default())
        );
    }

    #[test]
    fn enabling_scope_again_preserves_existing_matcher() {
        let matcher = MkWindowMatcher {
            process: Some("editor.exe".into()),
            ..Default::default()
        };
        let mut scope = MkHotkeyScope::ActiveWindow(matcher.clone());
        assert!(!set_hotkey_scope_enabled(&mut scope, true));
        assert_eq!(scope, MkHotkeyScope::ActiveWindow(matcher));
    }

    #[test]
    fn disabling_scope_produces_any_window() {
        let mut scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher {
            title: Some("Editor".into()),
            ..Default::default()
        });
        assert!(set_hotkey_scope_enabled(&mut scope, false));
        assert_eq!(scope, MkHotkeyScope::AnyWindow);
    }

    #[test]
    fn matcher_edits_report_a_document_change() {
        let mut scope = MkHotkeyScope::ActiveWindow(MkWindowMatcher::default());
        assert!(edit_active_matcher(&mut scope, |matcher| {
            matcher.title = Some("Editor".into());
        }));
        assert!(!edit_active_matcher(&mut scope, |matcher| {
            matcher.title = Some("Editor".into());
        }));
    }

    #[test]
    fn clear_removes_an_assigned_hotkey_and_reports_a_change() {
        let mut hotkey = Some(MkHotkey {
            key: MkKey::Delete,
            modifiers: vec![],
        });
        assert!(clear_hotkey(&mut hotkey));
        assert!(hotkey.is_none());
        assert!(!clear_hotkey(&mut hotkey));
    }
}
