use super::MkMacroDialog;
use super::key_capture::{apply_captured_hotkey, captured_chord, chord_hotkey, hotkey_name};
pub(crate) use super::window_matcher_editor::{MatcherEditorOutcome, matcher_ui};
use super::window_picker::{MatcherDestination, MatcherEditRequest};
use crate::gui::theme::widgets::{self, StatusTone};
use crate::mkmacro::hotkeys::{HotkeyDiagnostic, HotkeyDiagnosticSeverity};
use crate::mkmacro::{MkHotkeyScope, MkMacroFolder, MkWindowMatcher};
use eframe::egui;

fn folder_choices(folders: &[MkMacroFolder]) -> Vec<(Option<u64>, String)> {
    std::iter::once((None, "Unfiled".to_owned()))
        .chain(
            folders
                .iter()
                .map(|folder| (Some(folder.id), folder.name.clone())),
        )
        .collect()
}

// Resolve dangling references for presentation only; a user choice applies the change.
fn current_folder_choice(
    choices: &[(Option<u64>, String)],
    folder_id: Option<u64>,
) -> (Option<u64>, &str) {
    choices
        .iter()
        .find(|(id, _)| *id == folder_id)
        .map(|(id, name)| (*id, name.as_str()))
        .unwrap_or((None, "Unfiled"))
}

// Keep severity classification independent from the active palette.
fn hotkey_diagnostic_tone(severity: HotkeyDiagnosticSeverity) -> StatusTone {
    match severity {
        HotkeyDiagnosticSeverity::Error => StatusTone::Error,
        HotkeyDiagnosticSeverity::Warning => StatusTone::Warning,
    }
}

fn has_recording_toggle_conflict(diagnostics: &[HotkeyDiagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.severity == HotkeyDiagnosticSeverity::Error
            && diagnostic.message == "hotkey conflicts with the recording toggle"
    })
}

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
    widgets::labeled_form_row(ui, "Record Toggle", |ui| {
        let label = hotkey_name(&d.draft.settings.record_toggle_hotkey);
        let response = ui.button(if d.record_hotkey_capture {
            "Press a key…"
        } else {
            &label
        });
        if response.clicked() {
            d.record_hotkey_capture = true;
        }
        response
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
    let conflicts = crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[]);
    if has_recording_toggle_conflict(&conflicts) {
        widgets::semantic_status(
            ui,
            StatusTone::Error,
            "hotkey conflicts with an enabled macro",
            None,
        );
    }
    ui.separator();
    let capturing = d.hotkey_capture;
    let mut changed = false;
    let mut capture = None;
    let mut clear = false;
    let mut picker_original = None;
    let folders = folder_choices(&d.draft.folders);
    let mut folder_change = None;
    let Some(m) = d.selected_macro_mut() else {
        widgets::secondary_help(ui, "Select a macro to edit its properties.");
        return;
    };
    let macro_id = m.id;
    widgets::compact_section_heading(
        ui,
        "Macro Properties",
        Some("Edit identity, hotkey, scope, and playback behavior."),
    );
    changed |=
        widgets::labeled_form_row(ui, "Name", |ui| ui.text_edit_singleline(&mut m.name)).changed();
    changed |= widgets::labeled_form_row(ui, "Description", |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut m.description)
                .desired_rows(2)
                .hint_text("Description"),
        )
    })
    .changed();
    changed |= ui.checkbox(&mut m.enabled, "Enabled").changed();
    let (selected_folder, folder_label) = current_folder_choice(&folders, m.folder_id);
    widgets::labeled_form_row(ui, "Folder", |ui| {
        egui::ComboBox::from_id_salt(("macro_properties_folder", macro_id))
            .selected_text(folder_label)
            .show_ui(ui, |ui| {
                for (folder_id, name) in &folders {
                    if ui
                        .selectable_label(selected_folder == *folder_id, name)
                        .clicked()
                    {
                        folder_change = Some(*folder_id);
                    }
                }
            })
            .response
    });
    widgets::labeled_form_row(ui, "Hotkey", |ui| {
        let label = m
            .hotkey
            .as_ref()
            .map(hotkey_name)
            .unwrap_or_else(|| "None".into());
        ui.horizontal(|ui| {
            let response = ui.button(if capturing { "Press a key…" } else { &label });
            if response.clicked() {
                capture = Some(true);
            }
            if m.hotkey.is_some() && ui.small_button("Clear").clicked() {
                clear = true;
            }
            response
        })
        .inner
    });
    let mut scope_enabled = matches!(m.hotkey_scope, MkHotkeyScope::ActiveWindow(_));
    if widgets::labeled_form_row(ui, "Hotkey scope", |ui| {
        ui.checkbox(
            &mut scope_enabled,
            "Only activate this hotkey when this window is active",
        )
    })
    .changed()
    {
        changed |= set_hotkey_scope_enabled(&mut m.hotkey_scope, scope_enabled);
    }
    if matches!(m.hotkey_scope, MkHotkeyScope::ActiveWindow(_)) {
        let mut matcher_ui_changed = false;
        changed |= edit_active_matcher(&mut m.hotkey_scope, |matcher| {
            let outcome = matcher_ui(ui, matcher);
            matcher_ui_changed |= outcome.changed;
            if outcome.pick_window {
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
                    .range(0..=60_000)
                    .suffix(" ms random delay"),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut m.playback.random_offset_px)
                    .range(0..=10_000)
                    .suffix(" px random offset"),
            )
            .changed();
    });
    let _ = m;
    super::signature_editor::show(ui, d, macro_id);
    let m = d
        .draft
        .macros
        .iter_mut()
        .find(|m| m.id == macro_id)
        .expect("selected macro remains present while rendering properties");
    if clear {
        changed |= clear_hotkey(&mut m.hotkey);
    }
    let _ = m;
    if let Some(folder_id) = folder_change {
        d.move_macro_to_folder(macro_id, folder_id);
    }
    if let Some(original) = picker_original {
        d.window_picker.open(MatcherEditRequest {
            destination: MatcherDestination::MacroHotkey { macro_id },
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
    for diagnostic in crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[])
        .into_iter()
        .filter(|x| Some(x.macro_id) == d.selected_macro_id)
    {
        widgets::semantic_status(
            ui,
            hotkey_diagnostic_tone(diagnostic.severity),
            &diagnostic.message,
            None,
        );
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
    fn folder_choices_start_with_unfiled_and_preserve_document_order() {
        let folders = vec![
            MkMacroFolder {
                id: 42,
                name: "Zulu".into(),
            },
            MkMacroFolder {
                id: 7,
                name: "Alpha".into(),
            },
            MkMacroFolder {
                id: 19,
                name: "Zulu".into(),
            },
        ];
        assert_eq!(
            folder_choices(&folders),
            vec![
                (None, "Unfiled".into()),
                (Some(42), "Zulu".into()),
                (Some(7), "Alpha".into()),
                (Some(19), "Zulu".into()),
            ]
        );
        assert_eq!(folder_choices(&[]), vec![(None, "Unfiled".into())]);
    }

    #[test]
    fn folder_choice_resolves_existing_and_dangling_references() {
        let choices = folder_choices(&[MkMacroFolder {
            id: 42,
            name: "Utilities".into(),
        }]);
        assert_eq!(
            current_folder_choice(&choices, Some(42)),
            (Some(42), "Utilities")
        );
        assert_eq!(current_folder_choice(&choices, None), (None, "Unfiled"));
        assert_eq!(
            current_folder_choice(&choices, Some(999)),
            (None, "Unfiled")
        );
    }

    #[test]
    fn hotkey_severity_maps_to_semantic_error_and_warning_tones() {
        assert_eq!(
            hotkey_diagnostic_tone(HotkeyDiagnosticSeverity::Error),
            StatusTone::Error
        );
        assert_eq!(
            hotkey_diagnostic_tone(HotkeyDiagnosticSeverity::Warning),
            StatusTone::Warning
        );
    }

    #[test]
    fn recorder_summary_requires_a_recording_toggle_error() {
        let mut diagnostics = vec![
            HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Warning,
                macro_id: 1,
                message: "Multiple window-specific macros share this hotkey.".into(),
            },
            HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: 2,
                message: "hotkey conflicts with launcher".into(),
            },
            HotkeyDiagnostic {
                severity: HotkeyDiagnosticSeverity::Error,
                macro_id: 3,
                message: "Malformed hotkey".into(),
            },
        ];
        assert!(!has_recording_toggle_conflict(&[]));
        assert!(!has_recording_toggle_conflict(&diagnostics));
        diagnostics.push(HotkeyDiagnostic {
            severity: HotkeyDiagnosticSeverity::Warning,
            macro_id: 4,
            message: "hotkey conflicts with the recording toggle".into(),
        });
        assert!(!has_recording_toggle_conflict(&diagnostics));
        diagnostics.last_mut().unwrap().severity = HotkeyDiagnosticSeverity::Error;
        assert!(has_recording_toggle_conflict(&diagnostics));
    }
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
