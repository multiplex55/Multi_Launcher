use super::MkMacroDialog;
use super::key_capture::{apply_captured_hotkey, captured_chord, hotkey_name};
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
    let mut visual_order = 0;
    show_with_jump_mode(ui, d, None, &mut visual_order);
}

pub(super) fn show_with_jump_mode(
    ui: &mut egui::Ui,
    d: &mut MkMacroDialog,
    jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
) {
    let before = d.mkmacro_properties_preferences.clone();
    ui.horizontal_wrapped(|ui| {
        ui.strong("Macro Properties");
        ui.checkbox(
            &mut d.mkmacro_properties_preferences.master_open,
            "Show sections",
        );
        if ui.small_button("Expand All").clicked() {
            d.mkmacro_properties_preferences.expand_all();
        }
        if ui.small_button("Collapse All").clicked() {
            d.mkmacro_properties_preferences.collapse_all();
        }
        ui.checkbox(
            &mut d.mkmacro_properties_preferences.focus_steps,
            "Focus Steps",
        );
    });

    let focus_steps = d.mkmacro_properties_preferences.focus_steps;
    let master_open = d.mkmacro_properties_preferences.master_open;
    if focus_steps || !master_open {
        show_compact_summary(ui, d);
    } else {
        if let Some(macro_id) = d.selected_macro_id {
            d.mkmacro_properties_preferences.identity_open = property_group(
                ui,
                "identity",
                "Identity",
                d.mkmacro_properties_preferences.identity_open,
                jump_mode_scope,
                visual_order,
                |ui, order| show_identity(ui, d, jump_mode_scope, order),
            );
            d.mkmacro_properties_preferences.activation_open = property_group(
                ui,
                "activation",
                "Activation",
                d.mkmacro_properties_preferences.activation_open,
                jump_mode_scope,
                visual_order,
                |ui, _| show_activation(ui, d),
            );
            d.mkmacro_properties_preferences.playback_open = property_group(
                ui,
                "playback",
                "Playback",
                d.mkmacro_properties_preferences.playback_open,
                jump_mode_scope,
                visual_order,
                |ui, order| show_playback(ui, d, jump_mode_scope, order),
            );
            d.mkmacro_properties_preferences.reusable_signature_open = property_group(
                ui,
                "reusable_signature",
                "Reusable Signature",
                d.mkmacro_properties_preferences.reusable_signature_open,
                jump_mode_scope,
                visual_order,
                |ui, _| super::signature_editor::show(ui, d, macro_id),
            );
        } else {
            widgets::secondary_help(ui, "Select a macro to edit its properties.");
        }
        d.mkmacro_properties_preferences.recording_open = property_group(
            ui,
            "recording",
            "Recording",
            d.mkmacro_properties_preferences.recording_open,
            jump_mode_scope,
            visual_order,
            |ui, _| show_recording(ui, d),
        );
    }

    let activation_visible = !focus_steps
        && master_open
        && d.selected_macro_id.is_some()
        && d.mkmacro_properties_preferences.activation_open;
    let recording_visible =
        !focus_steps && master_open && d.mkmacro_properties_preferences.recording_open;
    if !activation_visible {
        d.hotkey_capture = false;
    }
    if !recording_visible {
        d.cancel_property_record_hotkey_capture();
    }
    if before != d.mkmacro_properties_preferences {
        d.queue_macro_properties_preferences();
    }
}

fn property_group(
    ui: &mut egui::Ui,
    id: &'static str,
    title: &str,
    mut open: bool,
    jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
    body: impl FnOnce(&mut egui::Ui, &mut u64),
) -> bool {
    ui.push_id(("mkmacro_property_group", id), |ui| {
        let arrow = if open { "▼" } else { "▶" };
        let response = ui
            .scope_builder(
                egui::UiBuilder::new().id(egui::Id::new(("mkmacro_property_group", id))),
                |ui| ui.selectable_label(open, format!("{arrow} {title}")),
            )
            .inner;
        super::jump_mode::register_target(
            ui,
            jump_mode_scope,
            &response,
            format!("properties/group/{id}"),
            title.to_owned(),
            crate::gui::jump_mode::TargetKind::Selectable,
            visual_order,
        );
        if response.clicked() {
            open = !open;
        }
        if open {
            ui.indent(("mkmacro_property_body", id), |ui| body(ui, visual_order));
        }
    });
    open
}

fn show_compact_summary(ui: &mut egui::Ui, d: &MkMacroDialog) {
    if let Some(macro_) = d.selected_macro() {
        ui.horizontal_wrapped(|ui| {
            ui.strong(&macro_.name);
            ui.label(if macro_.enabled {
                "Enabled"
            } else {
                "Disabled"
            });
            if let Some(folder) = macro_
                .folder_id
                .and_then(|id| d.draft.folders.iter().find(|folder| folder.id == id))
            {
                ui.label(format!("Folder: {}", folder.name));
            }
            if let Some(hotkey) = &macro_.hotkey {
                ui.label(format!("Hotkey: {}", hotkey_name(hotkey)));
            }
        });
    } else {
        widgets::secondary_help(ui, "No macro selected.");
    }
}

fn show_identity(
    ui: &mut egui::Ui,
    d: &mut MkMacroDialog,
    jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
) {
    let folders = folder_choices(&d.draft.folders);
    let mut folder_change = None;
    let Some(macro_) = d.selected_macro_mut() else {
        return;
    };
    let macro_id = macro_.id;
    let mut changed = false;
    let name = widgets::labeled_form_row(ui, "Name", |ui| {
        ui.scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(("mkmacro_property_name", macro_id))),
            |ui| ui.text_edit_singleline(&mut macro_.name),
        )
        .inner
    });
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &name,
        format!("properties/{macro_id}/name"),
        "Name",
        crate::gui::jump_mode::TargetKind::TextEntry,
        visual_order,
    );
    changed |= name.changed();

    let description = widgets::labeled_form_row(ui, "Description", |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut macro_.description)
                .id(egui::Id::new(("mkmacro_property_description", macro_id)))
                .desired_rows(2)
                .hint_text("Description"),
        )
    });
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &description,
        format!("properties/{macro_id}/description"),
        "Description",
        crate::gui::jump_mode::TargetKind::TextEntry,
        visual_order,
    );
    changed |= description.changed();

    let enabled = ui
        .scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(("mkmacro_property_enabled", macro_id))),
            |ui| ui.checkbox(&mut macro_.enabled, "Enabled"),
        )
        .inner;
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &enabled,
        format!("properties/{macro_id}/enabled"),
        "Enabled",
        crate::gui::jump_mode::TargetKind::Checkbox,
        visual_order,
    );
    changed |= enabled.changed();

    let (selected_folder, folder_label) = current_folder_choice(&folders, macro_.folder_id);
    let popup_scope = jump_mode_scope.map(|scope| scope.combo_popup_scope("properties/folder"));
    let folder = widgets::labeled_form_row(ui, "Folder", |ui| {
        ui.scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(("mkmacro_property_folder", macro_id))),
            |ui| {
                egui::ComboBox::from_id_salt(("macro_properties_folder", macro_id))
                    .selected_text(folder_label)
                    .show_ui(ui, |ui| {
                        for (folder_id, name) in &folders {
                            let option_id = folder_id
                                .map(|id| egui::Id::new(("mkmacro_folder_option", macro_id, id)))
                                .unwrap_or_else(|| {
                                    egui::Id::new(("mkmacro_folder_option", macro_id, "unfiled"))
                                });
                            let response = ui
                                .scope_builder(egui::UiBuilder::new().id(option_id), |ui| {
                                    ui.selectable_label(selected_folder == *folder_id, name)
                                })
                                .inner;
                            super::jump_mode::register_target(
                                ui,
                                popup_scope.as_ref(),
                                &response,
                                format!(
                                    "folder/{}",
                                    folder_id
                                        .map_or_else(|| "unfiled".to_owned(), |id| id.to_string())
                                ),
                                name.clone(),
                                crate::gui::jump_mode::TargetKind::Selectable,
                                visual_order,
                            );
                            if response.clicked() {
                                folder_change = Some(*folder_id);
                            }
                        }
                    })
                    .response
            },
        )
        .inner
    });
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &folder,
        "properties/folder",
        "Folder",
        crate::gui::jump_mode::TargetKind::ComboBox,
        visual_order,
    );
    if changed {
        d.mark_dirty();
    }
    if let Some(folder_id) = folder_change {
        d.move_macro_to_folder(macro_id, folder_id);
    }
}

fn show_activation(ui: &mut egui::Ui, d: &mut MkMacroDialog) {
    let capturing = d.hotkey_capture;
    let mut changed = false;
    let mut capture = false;
    let mut clear = false;
    let mut picker_original = None;
    let Some(macro_) = d.selected_macro_mut() else {
        return;
    };
    let macro_id = macro_.id;
    widgets::labeled_form_row(ui, "Hotkey", |ui| {
        let label = macro_
            .hotkey
            .as_ref()
            .map(hotkey_name)
            .unwrap_or_else(|| "None".into());
        ui.horizontal(|ui| {
            let response = ui.button(if capturing { "Press a key…" } else { &label });
            if response.clicked() {
                capture = true;
            }
            if macro_.hotkey.is_some() && ui.small_button("Clear").clicked() {
                clear = true;
            }
            response
        })
        .inner
    });
    let mut scope_enabled = matches!(macro_.hotkey_scope, MkHotkeyScope::ActiveWindow(_));
    if widgets::labeled_form_row(ui, "Hotkey scope", |ui| {
        ui.checkbox(
            &mut scope_enabled,
            "Only activate this hotkey when this window is active",
        )
    })
    .changed()
    {
        changed |= set_hotkey_scope_enabled(&mut macro_.hotkey_scope, scope_enabled);
    }
    if matches!(macro_.hotkey_scope, MkHotkeyScope::ActiveWindow(_)) {
        let mut matcher_ui_changed = false;
        changed |= edit_active_matcher(&mut macro_.hotkey_scope, |matcher| {
            let outcome = matcher_ui(ui, matcher);
            matcher_ui_changed |= outcome.changed;
            if outcome.pick_window {
                picker_original = Some(matcher.clone());
            }
        });
        changed |= matcher_ui_changed;
    }
    if clear {
        changed |= clear_hotkey(&mut macro_.hotkey);
    }
    let _ = macro_;
    if let Some(original) = picker_original {
        d.window_picker.open(MatcherEditRequest {
            destination: MatcherDestination::MacroHotkey { macro_id },
            original,
        });
    }
    if capture {
        d.hotkey_capture = true;
    }
    if capturing && let Some(result) = ui.input(captured_chord) {
        d.hotkey_capture = false;
        if let Some(macro_) = d.selected_macro_mut() {
            changed |= apply_captured_hotkey(&mut macro_.hotkey, result);
        }
    }
    for diagnostic in crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[])
        .into_iter()
        .filter(|diagnostic| Some(diagnostic.macro_id) == d.selected_macro_id)
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

fn show_playback(
    ui: &mut egui::Ui,
    d: &mut MkMacroDialog,
    jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
) {
    let Some(macro_) = d.selected_macro_mut() else {
        return;
    };
    let macro_id = macro_.id;
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        let speed_slider = ui
            .scope_builder(
                egui::UiBuilder::new().id(egui::Id::new(("mkmacro_speed_slider", macro_id))),
                |ui| {
                    ui.add(
                        egui::Slider::new(&mut macro_.playback.speed_percent, 1..=1000)
                            .text("Speed %")
                            .show_value(false),
                    )
                },
            )
            .inner;
        super::jump_mode::register_target(
            ui,
            jump_mode_scope,
            &speed_slider,
            format!("properties/{macro_id}/speed-slider"),
            "Speed",
            crate::gui::jump_mode::TargetKind::Slider,
            visual_order,
        );
        changed |= speed_slider.changed();

        let speed_value = ui
            .scope_builder(
                egui::UiBuilder::new().id(egui::Id::new(("mkmacro_speed_value", macro_id))),
                |ui| {
                    ui.add(
                        egui::DragValue::new(&mut macro_.playback.speed_percent)
                            .range(1..=1000)
                            .suffix("%"),
                    )
                },
            )
            .inner;
        super::jump_mode::register_target(
            ui,
            jump_mode_scope,
            &speed_value,
            format!("properties/{macro_id}/speed-value"),
            "Speed percentage",
            crate::gui::jump_mode::TargetKind::TextEntry,
            visual_order,
        );
        changed |= speed_value.changed();
    });

    let random_delay = ui
        .scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(("mkmacro_random_delay", macro_id))),
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut macro_.playback.random_delay_ms)
                        .range(0..=60_000)
                        .suffix(" ms random delay"),
                )
            },
        )
        .inner;
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &random_delay,
        format!("properties/{macro_id}/random-delay"),
        "Random delay",
        crate::gui::jump_mode::TargetKind::TextEntry,
        visual_order,
    );
    changed |= random_delay.changed();

    let random_offset = ui
        .scope_builder(
            egui::UiBuilder::new().id(egui::Id::new(("mkmacro_random_offset", macro_id))),
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut macro_.playback.random_offset_px)
                        .range(0..=10_000)
                        .suffix(" px random offset"),
                )
            },
        )
        .inner;
    super::jump_mode::register_target(
        ui,
        jump_mode_scope,
        &random_offset,
        format!("properties/{macro_id}/random-offset"),
        "Random offset",
        crate::gui::jump_mode::TargetKind::TextEntry,
        visual_order,
    );
    changed |= random_offset.changed();
    if changed {
        d.mark_dirty();
    }
}

fn show_recording(ui: &mut egui::Ui, d: &mut MkMacroDialog) {
    widgets::labeled_form_row(ui, "Record Toggle", |ui| {
        let label = hotkey_name(&d.draft.settings.record_toggle_hotkey);
        let response = ui.button(if d.record_hotkey_capture {
            "Press a key…"
        } else {
            &label
        });
        if response.clicked() {
            d.start_property_record_hotkey_capture();
        }
        response
    });
    let conflicts = crate::mkmacro::hotkeys::validate_hotkeys(&d.draft, &[]);
    if has_recording_toggle_conflict(&conflicts) {
        widgets::semantic_status(
            ui,
            StatusTone::Error,
            "hotkey conflicts with an enabled macro",
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkHotkey, MkKey};

    fn test_dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = crate::mkmacro::MkMacroStore::open(directory.path()).unwrap();
        (directory, MkMacroDialog::new(std::sync::Arc::new(store)))
    }

    fn input(screen_rect: egui::Rect, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen_rect),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn render_properties(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        screen_rect: egui::Rect,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run_ui(input(screen_rect, events), |context| {
            egui::CentralPanel::default().show(context, |ui| show(ui, dialog));
        })
    }

    fn render_contents(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        screen_rect: egui::Rect,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        context.run_ui(input(screen_rect, events), |context| {
            egui::CentralPanel::default().show(context, |ui| dialog.show_contents(ui));
        })
    }

    fn accessible_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find_map(|(_, node)| {
                if node.label() != Some(label) {
                    return None;
                }
                let bounds = node.bounds()?;
                Some(egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ))
            })
    }

    fn click(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        screen_rect: egui::Rect,
        point: egui::Pos2,
    ) {
        let pressed = render_properties(
            context,
            dialog,
            screen_rect,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        pressed.drop_without_applying_deltas();
        let released = render_properties(
            context,
            dialog,
            screen_rect,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        released.drop_without_applying_deltas();
    }

    fn click_contents(
        context: &egui::Context,
        dialog: &mut MkMacroDialog,
        screen_rect: egui::Rect,
        point: egui::Pos2,
    ) {
        let pressed = render_contents(
            context,
            dialog,
            screen_rect,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        pressed.drop_without_applying_deltas();
        let released = render_contents(
            context,
            dialog,
            screen_rect,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            }],
        );
        released.drop_without_applying_deltas();
    }

    #[test]
    fn property_controls_persist_presentation_without_mutating_macro_or_capture_controls() {
        let (_directory, mut dialog) = test_dialog();
        dialog.create_macro();
        dialog.dirty = false;
        let before_draft = dialog.draft.clone();
        let before_revision = dialog.draft_revision();
        let before_selected = dialog.selected_macro_id;
        let context = egui::Context::default();
        context.enable_accesskit();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 560.0));

        let initial = render_properties(&context, &mut dialog, screen_rect, Vec::new());
        let expand_bounds = accessible_bounds(&initial, "Expand All")
            .expect("Expand All is available while properties are collapsed");
        initial.drop_without_applying_deltas();
        click(&context, &mut dialog, screen_rect, expand_bounds.center());
        assert!(dialog.mkmacro_properties_preferences.master_open);
        assert!(dialog.mkmacro_properties_preferences.playback_open);
        assert!(
            dialog
                .mkmacro_properties_preferences
                .reusable_signature_open
        );

        let expanded = render_properties(&context, &mut dialog, screen_rect, Vec::new());
        let playback_bounds = accessible_bounds(&expanded, "▼ Playback")
            .expect("the expanded Playback group can be collapsed independently");
        expanded.drop_without_applying_deltas();
        click(&context, &mut dialog, screen_rect, playback_bounds.center());
        assert!(!dialog.mkmacro_properties_preferences.playback_open);

        dialog.hotkey_capture = true;
        dialog.start_property_record_hotkey_capture();
        dialog.pause_record_hotkey_capture = true;
        dialog.marker_record_hotkey_capture = true;
        let expanded = render_properties(&context, &mut dialog, screen_rect, Vec::new());
        let focus_bounds = accessible_bounds(&expanded, "Focus Steps")
            .expect("Focus Steps remains available after expanding properties");
        expanded.drop_without_applying_deltas();
        click(&context, &mut dialog, screen_rect, focus_bounds.center());

        assert!(dialog.mkmacro_properties_preferences.focus_steps);
        assert!(dialog.mkmacro_properties_preferences.master_open);
        assert!(dialog.mkmacro_properties_preferences.activation_open);
        assert!(!dialog.hotkey_capture);
        assert!(!dialog.record_hotkey_capture);
        assert!(dialog.pause_record_hotkey_capture);
        assert!(dialog.marker_record_hotkey_capture);
        let focused = render_properties(&context, &mut dialog, screen_rect, Vec::new());
        let focus_bounds = accessible_bounds(&focused, "Focus Steps")
            .expect("Focus Steps can be turned off again");
        focused.drop_without_applying_deltas();
        click(&context, &mut dialog, screen_rect, focus_bounds.center());

        assert!(!dialog.mkmacro_properties_preferences.focus_steps);
        assert!(dialog.mkmacro_properties_preferences.master_open);
        assert!(!dialog.mkmacro_properties_preferences.playback_open);
        let restored_groups = render_properties(&context, &mut dialog, screen_rect, Vec::new());
        assert!(accessible_bounds(&restored_groups, "▶ Playback").is_some());
        restored_groups.drop_without_applying_deltas();
        assert_eq!(dialog.draft, before_draft);
        assert_eq!(dialog.draft_revision(), before_revision);
        assert_eq!(dialog.selected_macro_id, before_selected);
        assert!(!dialog.dirty);
        assert_eq!(
            dialog.take_macro_properties_preferences_for_persist(),
            Some(dialog.mkmacro_properties_preferences.clone())
        );
        assert_eq!(dialog.take_macro_properties_preferences_for_persist(), None);
    }

    #[test]
    fn recording_toggle_remains_available_without_a_selected_macro() {
        let (_directory, mut dialog) = test_dialog();
        dialog.mkmacro_properties_preferences.master_open = true;
        let context = egui::Context::default();
        context.enable_accesskit();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 560.0));
        let record_label = hotkey_name(&dialog.draft.settings.record_toggle_hotkey);

        let output = render_contents(&context, &mut dialog, screen_rect, Vec::new());
        let record_bounds = accessible_bounds(&output, &record_label)
            .expect("the document-wide Record Toggle is shown without a selected macro");
        output.drop_without_applying_deltas();
        click_contents(&context, &mut dialog, screen_rect, record_bounds.center());

        let captured = render_contents(
            &context,
            &mut dialog,
            screen_rect,
            vec![egui::Event::Key {
                key: egui::Key::F8,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        captured.drop_without_applying_deltas();

        assert!(dialog.selected_macro_id.is_none());
        assert!(!dialog.record_hotkey_capture);
        assert_eq!(
            hotkey_name(&dialog.draft.settings.record_toggle_hotkey),
            "F8"
        );
        assert!(dialog.dirty);
        assert_eq!(dialog.draft_revision(), 1);
    }

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
