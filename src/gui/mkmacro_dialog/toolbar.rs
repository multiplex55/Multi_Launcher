use super::MkMacroDialog;
use crate::mkmacro::{MovementMode, RecorderRuntimeState, RuntimeState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolbarState {
    pub run: bool,
    pub pause: bool,
    pub resume: bool,
    pub stop: bool,
    pub run_from: bool,
    pub run_selected: bool,
    pub debug_run: bool,
    pub debug_from: bool,
    pub debug_selected: bool,
    pub disabled_reason: Option<String>,
    pub run_from_reason: Option<String>,
    pub run_selected_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarCommand {
    Run,
    Pause,
    Resume,
    Stop,
    RunFrom,
    RunSelected,
    DebugRun,
    DebugFrom,
    DebugSelected,
}

impl ToolbarState {
    /// Return the enablement computed by [`decide`] for one execution command.
    /// Keeping this lookup pure gives the renderer and tests one source of truth.
    pub fn command_enabled(&self, command: ToolbarCommand) -> bool {
        match command {
            ToolbarCommand::Run => self.run,
            ToolbarCommand::Pause => self.pause,
            ToolbarCommand::Resume => self.resume,
            ToolbarCommand::Stop => self.stop,
            ToolbarCommand::RunFrom => self.run_from,
            ToolbarCommand::RunSelected => self.run_selected,
            ToolbarCommand::DebugRun => self.debug_run,
            ToolbarCommand::DebugFrom => self.debug_from,
            ToolbarCommand::DebugSelected => self.debug_selected,
        }
    }
}

pub const DEBUG_COMMANDS: [ToolbarCommand; 3] = [
    ToolbarCommand::DebugRun,
    ToolbarCommand::DebugFrom,
    ToolbarCommand::DebugSelected,
];

pub fn decide(
    runtime: RuntimeState,
    disabled_reason: Option<String>,
    selected_rows: usize,
) -> ToolbarState {
    let idle = !matches!(
        runtime,
        RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stopping
    );
    let run = idle && disabled_reason.is_none();
    let run_from_reason = if !run {
        disabled_reason.clone()
    } else if selected_rows != 1 {
        Some("Select exactly one step".into())
    } else {
        None
    };
    let run_selected_reason = if !run {
        disabled_reason.clone()
    } else if selected_rows == 0 {
        Some("Select one or more steps".into())
    } else {
        None
    };
    let run_from = run && selected_rows == 1;
    let run_selected = run && selected_rows != 0;
    ToolbarState {
        run,
        pause: runtime == RuntimeState::Running,
        resume: runtime == RuntimeState::Paused,
        stop: matches!(runtime, RuntimeState::Running | RuntimeState::Paused),
        run_from,
        run_selected,
        debug_run: run,
        debug_from: run_from,
        debug_selected: run_selected,
        disabled_reason,
        run_from_reason,
        run_selected_reason,
    }
}
pub fn state(dialog: &MkMacroDialog) -> ToolbarState {
    let runtime = crate::mkmacro::runtime::snapshot()
        .map(|s| s.state)
        .unwrap_or(RuntimeState::Idle);
    decide(
        runtime,
        dialog.playback_block_reason(),
        dialog.selection.ids.len(),
    )
}
fn report(dialog: &mut MkMacroDialog, result: anyhow::Result<()>) {
    if let Err(e) = result {
        dialog.command_error = Some(e.to_string());
    }
}
pub(super) fn show(ui: &mut eframe::egui::Ui, dialog: &mut MkMacroDialog) {
    process_recorder_hotkey_capture(ui.ctx(), dialog);
    if dialog.recording_annotation_open {
        show_recording_annotation(ui.ctx(), dialog);
        return;
    }
    let state = state(dialog);
    ui.horizontal(|ui| {
        if ui
            .button("Refresh checks")
            .on_hover_text("Check external image files and connected monitors")
            .clicked()
        {
            dialog.refresh_environment();
        }
        if ui.button("Save").clicked() {
            let result = dialog.save();
            report(dialog, result);
        }
        ui.add_enabled_ui(
            dialog.action_editor.draft.is_none() && !super::step_table::table_modal_open(dialog),
            |ui| super::package_ui::show_toolbar_menu(ui, dialog),
        );
        ui.add_enabled_ui(
            dialog.action_editor.draft.is_none() && !super::step_table::table_modal_open(dialog),
            |ui| {
                ui.menu_button("Edit steps", |ui| {
                    use super::editor_operations::{self, ClipboardCommand};
                    let selected =
                        dialog.selected_macro().is_some() && !dialog.selection.ids.is_empty();
                    for (label, command, enabled) in [
                        ("Copy  Ctrl+C", ClipboardCommand::Copy, selected),
                        ("Cut  Ctrl+X", ClipboardCommand::Cut, selected),
                        (
                            "Paste  Ctrl+V",
                            ClipboardCommand::Paste,
                            dialog.selected_macro().is_some()
                                && !dialog.editor_state.clipboard.is_empty(),
                        ),
                        ("Duplicate  Ctrl+D", ClipboardCommand::Duplicate, selected),
                    ] {
                        if ui
                            .add_enabled(enabled, eframe::egui::Button::new(label))
                            .clicked()
                        {
                            let result = editor_operations::clipboard(dialog, command);
                            report(dialog, result);
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    for (label, mode) in [
                        ("Find  Ctrl+F", super::search::SearchMode::Find),
                        ("Replace  Ctrl+H", super::search::SearchMode::Replace),
                        ("Jump to Step  Ctrl+G", super::search::SearchMode::Jump),
                    ] {
                        if ui
                            .add_enabled(
                                dialog.selected_macro().is_some(),
                                eframe::egui::Button::new(label),
                            )
                            .clicked()
                        {
                            super::search::open(dialog, mode);
                            ui.close_menu();
                        }
                    }
                    ui.checkbox(&mut dialog.navigation.outline.open, "Show Outline");
                });
            },
        );
        if ui
            .add_enabled(
                dialog.selected_macro().is_some(),
                eframe::egui::Button::new("+ Action"),
            )
            .clicked()
        {
            dialog.action_catalog_visible = true;
        }
        if !state.command_enabled(ToolbarCommand::Run) {
            ui.add_enabled(false, eframe::egui::Button::new("Run"))
                .on_disabled_hover_text(
                    state
                        .disabled_reason
                        .clone()
                        .unwrap_or_else(|| "Playback is active".into()),
                );
        } else if ui.button("Run").clicked() {
            let result = dialog.run_selected_macro();
            report(dialog, result);
        }
        ui.menu_button("Debug", |ui| {
            let [
                debug_run_command,
                debug_from_command,
                debug_selected_command,
            ] = DEBUG_COMMANDS;
            let response = ui.add_enabled(
                state.command_enabled(debug_run_command),
                eframe::egui::Button::new("Debug Run"),
            );
            let response = if let Some(reason) = state.disabled_reason.clone() {
                response.on_disabled_hover_text(reason)
            } else {
                response
            };
            if response.clicked() {
                let result = dialog.debug_selected_macro();
                report(dialog, result);
            }

            ui.separator();

            let response = ui.add_enabled(
                state.command_enabled(debug_from_command),
                eframe::egui::Button::new("Debug From Here"),
            );
            let response = if let Some(reason) = state.run_from_reason.clone() {
                response.on_disabled_hover_text(reason)
            } else {
                response
            };
            if response.clicked() {
                if let Some(id) = dialog.selection.ids.iter().next().copied() {
                    let result = dialog.debug_from_step(id);
                    report(dialog, result);
                }
            }

            let response = ui.add_enabled(
                state.command_enabled(debug_selected_command),
                eframe::egui::Button::new("Debug Selected"),
            );
            let response = if let Some(reason) = state.run_selected_reason.clone() {
                response.on_disabled_hover_text(reason)
            } else {
                response
            };
            if response.clicked() {
                let result = dialog.debug_selected_steps();
                report(dialog, result);
            }
        });
        if ui
            .add_enabled(
                state.command_enabled(ToolbarCommand::Pause),
                eframe::egui::Button::new("Pause"),
            )
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::pause());
        }
        if ui
            .add_enabled(
                state.command_enabled(ToolbarCommand::Resume),
                eframe::egui::Button::new("Resume"),
            )
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::resume());
        }
        if ui
            .add_enabled(
                state.command_enabled(ToolbarCommand::Stop),
                eframe::egui::Button::new("Stop"),
            )
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::stop());
        }
        let response = ui.add_enabled(
            state.command_enabled(ToolbarCommand::RunFrom),
            eframe::egui::Button::new("Run From Here"),
        );
        let response = if let Some(reason) = state.run_from_reason.clone() {
            response.on_disabled_hover_text(reason)
        } else {
            response
        };
        if response.clicked() {
            if let Some(id) = dialog.selection.ids.iter().next().copied() {
                let result = dialog.run_from_step(id);
                report(dialog, result)
            }
        }
        let response = ui.add_enabled(
            state.command_enabled(ToolbarCommand::RunSelected),
            eframe::egui::Button::new("Run Selected"),
        );
        let response = if let Some(reason) = state.run_selected_reason.clone() {
            response.on_disabled_hover_text(reason)
        } else {
            response
        };
        if response.clicked() {
            let result = dialog.run_selected_steps();
            report(dialog, result);
        }
        if dialog.dirty {
            ui.label("Unsaved changes");
        }
        if ui
            .add_enabled(
                dialog.can_undo_recording_insert(),
                eframe::egui::Button::new("Undo Recording Insert"),
            )
            .clicked()
            && let Err(error) = dialog.undo_last_recording_insert()
        {
            dialog.command_error = Some(error);
        }
        if ui
            .add_enabled(
                dialog.can_redo_recording_insert(),
                eframe::egui::Button::new("Redo Recording Insert"),
            )
            .clicked()
            && let Err(error) = dialog.redo_last_recording_insert()
        {
            dialog.command_error = Some(error);
        }
        if dialog.conflict {
            ui.colored_label(
                eframe::egui::Color32::YELLOW,
                "File changed externally; reload or save to overwrite",
            );
        }
        let recorder = crate::mkmacro::runtime::recorder_snapshot();
        let recorder_active = recorder
            .as_ref()
            .is_some_and(|s| s.state != RecorderRuntimeState::Idle)
            || crate::mkmacro::runtime::record_stop_pending();
        let authoring_modal =
            dialog.action_editor.draft.is_some() || super::step_table::table_modal_open(dialog);
        let recording_controls = super::recorder_controller::decide_recording_controls(
            crate::mkmacro::runtime::snapshot()
                .as_deref()
                .map_or(RuntimeState::Idle, |snapshot| snapshot.state),
            if crate::mkmacro::runtime::record_stop_pending() {
                RecorderRuntimeState::Stopping
            } else {
                recorder
                    .as_deref()
                    .map_or(RecorderRuntimeState::Idle, |snapshot| snapshot.state)
            },
            dialog.selected_macro().is_some(),
            dialog.recording_review.is_some(),
            authoring_modal,
        );
        if ui
            .add_enabled(
                recording_controls.record,
                eframe::egui::Button::new("Record"),
            )
            .clicked()
        {
            if let Some(target) = dialog.recording_target() {
                report(
                    dialog,
                    crate::mkmacro::runtime::record_target(
                        target,
                        crate::mkmacro::NormalizationConfig::from(&dialog.draft.settings.recorder),
                    ),
                );
            }
        }
        ui.menu_button("Record Options", |ui| {
            show_record_options(ui, dialog, recorder_active)
        });
    });
    if let Some(rec) = crate::mkmacro::runtime::recorder_snapshot().filter(|s| {
        s.state != RecorderRuntimeState::Idle || crate::mkmacro::runtime::record_stop_pending()
    }) {
        let recorder_state = if crate::mkmacro::runtime::record_stop_pending() {
            RecorderRuntimeState::Stopping
        } else {
            rec.state
        };
        let controls = super::recorder_controller::decide_recording_controls(
            crate::mkmacro::runtime::snapshot()
                .as_deref()
                .map_or(RuntimeState::Idle, |snapshot| snapshot.state),
            recorder_state,
            dialog.selected_macro().is_some(),
            dialog.recording_review.is_some(),
            dialog.action_editor.draft.is_some() || super::step_table::table_modal_open(dialog),
        );
        ui.horizontal(|ui| {
            let secs = rec.elapsed.as_secs();
            let state_label = match recorder_state {
                RecorderRuntimeState::Recording => "Recording",
                RecorderRuntimeState::Paused => "Paused",
                RecorderRuntimeState::Stopping => "Stopping",
                RecorderRuntimeState::Idle => "Idle",
            };
            ui.label(format!(
                "● {state_label} {:02}:{:02} — {} raw events — ~{} actions",
                secs / 60,
                secs % 60,
                rec.raw_event_count,
                rec.estimated_action_count
            ));
            if rec.dropped_event_count > 0 {
                ui.colored_label(
                    eframe::egui::Color32::YELLOW,
                    format!("{} events dropped", rec.dropped_event_count),
                );
            }
            match recorder_state {
                RecorderRuntimeState::Recording => {
                    if ui
                        .add_enabled(controls.pause, eframe::egui::Button::new("Pause Recording"))
                        .clicked()
                    {
                        report(dialog, crate::mkmacro::runtime::record_pause())
                    }
                }
                RecorderRuntimeState::Paused => {
                    if ui
                        .add_enabled(
                            controls.resume,
                            eframe::egui::Button::new("Resume Recording"),
                        )
                        .clicked()
                    {
                        report(dialog, crate::mkmacro::runtime::record_resume())
                    }
                }
                _ => {}
            }
            if ui
                .add_enabled(controls.marker, eframe::egui::Button::new("Marker"))
                .clicked()
            {
                report(dialog, crate::mkmacro::runtime::record_marker());
            }
            if ui
                .add_enabled(controls.annotate, eframe::egui::Button::new("Annotate"))
                .clicked()
            {
                let result = dialog.begin_recording_annotation();
                report(dialog, result);
            }
            if ui
                .add_enabled(controls.stop, eframe::egui::Button::new("Stop Recording"))
                .clicked()
            {
                let result = dialog.stop_recording_for_review();
                report(dialog, result);
            }
        });
    }
    if let Some(error) = &dialog.command_error {
        ui.colored_label(eframe::egui::Color32::RED, error);
    }
    if let Some(status) = crate::mkmacro::runtime::recording_status() {
        ui.colored_label(eframe::egui::Color32::YELLOW, status);
    }
    if let Some(run) = crate::mkmacro::runtime::snapshot().filter(|s| {
        matches!(
            s.state,
            RuntimeState::Running | RuntimeState::Paused | RuntimeState::Failed
        )
    }) {
        let name = run
            .macro_id
            .and_then(|id| dialog.draft.macros.iter().find(|m| m.id == id))
            .map(|m| m.name.as_str())
            .unwrap_or("Unknown macro");
        ui.label(format!(
            "{:?}: {} — step {}/{}",
            run.state,
            name,
            run.completed_steps.saturating_add(1).min(run.total_steps),
            run.total_steps
        ));
    }
    show_recording_annotation(ui.ctx(), dialog);
}

fn show_recording_annotation(ctx: &eframe::egui::Context, dialog: &mut MkMacroDialog) {
    if !dialog.recording_annotation_open {
        return;
    }
    if crate::mkmacro::runtime::recorder_snapshot().is_none_or(|snapshot| {
        matches!(
            snapshot.state,
            RecorderRuntimeState::Idle | RecorderRuntimeState::Stopping
        )
    }) {
        let _ = dialog.finish_recording_annotation(false);
        return;
    }
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    eframe::egui::Window::new("Recording annotation")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Add a note at the current recording position.");
            ui.text_edit_singleline(&mut dialog.recording_annotation_text)
                .request_focus();
            ui.horizontal(|ui| {
                save = ui
                    .add_enabled(
                        !dialog.recording_annotation_text.trim().is_empty(),
                        eframe::egui::Button::new("Add note"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if save || cancel || !open {
        let result = dialog.finish_recording_annotation(save);
        report(dialog, result);
    }
}

fn process_recorder_hotkey_capture(ctx: &eframe::egui::Context, dialog: &mut MkMacroDialog) {
    let pause = if dialog.pause_record_hotkey_capture {
        true
    } else if dialog.marker_record_hotkey_capture {
        false
    } else {
        return;
    };
    let Some(chord) = ctx.input(super::key_capture::captured_chord) else {
        return;
    };
    // The capture owns this physical press for the frame; table/search
    // shortcuts must not also act on it after the capture flag is cleared.
    ctx.input_mut(|input| {
        input
            .events
            .retain(|event| !matches!(event, eframe::egui::Event::Key { pressed: true, .. }));
    });
    dialog.pause_record_hotkey_capture = false;
    dialog.marker_record_hotkey_capture = false;
    let Some(hotkey) = super::key_capture::chord_hotkey(chord) else {
        return;
    };
    let slot = if pause {
        &mut dialog.draft.settings.recorder.pause_resume_hotkey
    } else {
        &mut dialog.draft.settings.recorder.marker_hotkey
    };
    if slot.as_ref() != Some(&hotkey) {
        *slot = Some(hotkey);
        dialog.mark_dirty();
    }
}

fn recorder_control_conflict(dialog: &MkMacroDialog) -> Option<String> {
    let controls = [
        (
            "Record Toggle",
            Some(&dialog.draft.settings.record_toggle_hotkey),
        ),
        (
            "Pause/Resume",
            dialog.draft.settings.recorder.pause_resume_hotkey.as_ref(),
        ),
        (
            "Marker",
            dialog.draft.settings.recorder.marker_hotkey.as_ref(),
        ),
    ];
    for (index, (left_name, left)) in controls.iter().enumerate() {
        let Some(left) = left else { continue };
        let left = crate::mkmacro::hotkeys::canonical_hotkey(left);
        for (right_name, right) in &controls[index + 1..] {
            if right.is_some_and(|right| crate::mkmacro::hotkeys::canonical_hotkey(right) == left) {
                return Some(format!("{left_name} conflicts with {right_name}"));
            }
        }
    }
    None
}

fn show_record_options(ui: &mut eframe::egui::Ui, dialog: &mut MkMacroDialog, active: bool) {
    ui.set_enabled(!active);
    let before = dialog.draft.settings.recorder.clone();
    let pause_capturing = dialog.pause_record_hotkey_capture;
    let marker_capturing = dialog.marker_record_hotkey_capture;
    let toggle_capturing = dialog.record_hotkey_capture;
    let toggle_label = super::key_capture::hotkey_name(&dialog.draft.settings.record_toggle_hotkey);
    let mut capture_pause = false;
    let mut capture_marker = false;
    let mut capture_toggle = false;
    {
        let options = &mut dialog.draft.settings.recorder;
        ui.collapsing("Keyboard", |ui| {
            ui.checkbox(&mut options.record_keyboard, "Keyboard");
        });
        ui.collapsing("Input", |ui| {
            ui.checkbox(&mut options.record_mouse_buttons, "Mouse buttons");
            ui.checkbox(&mut options.record_mouse_wheel, "Mouse wheel");
            ui.checkbox(&mut options.record_injected_input, "Injected input");
        });
        ui.collapsing("Mouse Movement", |ui| {
            for (mode, label) in [
                (MovementMode::Off, "Off"),
                (MovementMode::ClicksOnly, "Clicks Only"),
                (MovementMode::SampledMovement, "Sampled Movement"),
                (MovementMode::DetailedMovement, "Detailed Movement"),
            ] {
                ui.radio_value(&mut options.movement_mode, mode, label);
            }
            let sampled = options.movement_mode == MovementMode::SampledMovement;
            ui.add_enabled(
                sampled,
                eframe::egui::Slider::new(&mut options.movement_distance_px, 1..=500)
                    .text("Distance (px)"),
            );
            ui.add_enabled(
                sampled,
                eframe::egui::Slider::new(&mut options.movement_interval_ms, 1..=5_000)
                    .text("Interval (ms)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.click_max_ms, 1..=10_000)
                    .text("Click max (ms)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.click_distance_px, 0..=100)
                    .text("Click distance (px)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.multi_click_ms, 1..=5_000)
                    .text("Multi-click (ms)"),
            );
        });
        ui.collapsing("Timing", |ui| {
            ui.add(
                eframe::egui::Slider::new(&mut options.minimum_idle_delay_ms, 0..=60_000)
                    .text("Minimum idle delay (ms)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.delay_rounding_ms, 1..=10_000)
                    .text("Delay rounding (ms)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.key_tap_max_ms, 1..=10_000)
                    .text("Key tap max (ms)"),
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.text_run_gap_ms, 1..=60_000)
                    .text("Text run gap (ms)"),
            );
        });
        ui.collapsing("Window Context", |ui| {
            ui.checkbox(
                &mut options.record_window_context,
                "Record active and target windows",
            );
            ui.checkbox(
                &mut options.detect_application_launches,
                "Detect application launches",
            );
            ui.checkbox(
                &mut options.inspect_clicked_controls,
                "Inspect clicked controls",
            );
        });
        ui.collapsing("Smart Cleanup", |ui| {
            ui.checkbox(&mut options.smart_keyboard_cleanup, "Keyboard cleanup");
            ui.checkbox(&mut options.smart_mouse_cleanup, "Mouse cleanup");
            ui.checkbox(&mut options.smart_window_cleanup, "Window cleanup");
            ui.checkbox(
                &mut options.smart_repeated_click_cleanup,
                "Repeated-click cleanup",
            );
            ui.add(
                eframe::egui::Slider::new(&mut options.repeated_click_minimum, 2..=100)
                    .text("Repeat minimum"),
            );
            ui.add(
                eframe::egui::Slider::new(
                    &mut options.repeated_click_interval_tolerance_ms,
                    0..=10_000,
                )
                .text("Interval tolerance (ms)"),
            );
        });
        ui.collapsing("Recorder Hotkeys", |ui| {
            ui.horizontal(|ui| {
                ui.label("Record Toggle:");
                capture_toggle = ui
                    .button(if toggle_capturing {
                        "Press a key…"
                    } else {
                        &toggle_label
                    })
                    .clicked();
            });
            ui.horizontal(|ui| {
                ui.label("Pause/Resume:");
                let label = options
                    .pause_resume_hotkey
                    .as_ref()
                    .map(super::key_capture::hotkey_name)
                    .unwrap_or_else(|| "Not set".into());
                capture_pause = ui
                    .button(if pause_capturing {
                        "Press a key…"
                    } else {
                        &label
                    })
                    .clicked();
                if ui
                    .add_enabled(
                        options.pause_resume_hotkey.is_some(),
                        eframe::egui::Button::new("Clear"),
                    )
                    .clicked()
                {
                    options.pause_resume_hotkey = None;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Marker:");
                let label = options
                    .marker_hotkey
                    .as_ref()
                    .map(super::key_capture::hotkey_name)
                    .unwrap_or_else(|| "Not set".into());
                capture_marker = ui
                    .button(if marker_capturing {
                        "Press a key…"
                    } else {
                        &label
                    })
                    .clicked();
                if ui
                    .add_enabled(
                        options.marker_hotkey.is_some(),
                        eframe::egui::Button::new("Clear"),
                    )
                    .clicked()
                {
                    options.marker_hotkey = None;
                }
            });
        });
        ui.collapsing("Advanced", |ui| {
            ui.checkbox(
                &mut options.capture_text_paste_for_freeze_suggestion,
                "Capture paste for freeze suggestion",
            );
            ui.small("Clipboard observations are transient and clear when Review closes.");
        });
        options.clamp();
    }
    if capture_toggle {
        dialog.record_hotkey_capture = true;
        dialog.pause_record_hotkey_capture = false;
        dialog.marker_record_hotkey_capture = false;
    } else if capture_pause {
        dialog.pause_record_hotkey_capture = true;
        dialog.marker_record_hotkey_capture = false;
    } else if capture_marker {
        dialog.marker_record_hotkey_capture = true;
        dialog.pause_record_hotkey_capture = false;
    }
    if dialog.draft.settings.recorder != before {
        dialog.mark_dirty();
    }
    if let Some(conflict) = recorder_control_conflict(dialog) {
        ui.colored_label(eframe::egui::Color32::RED, conflict);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_controls_follow_eligibility_and_selection() {
        let s = decide(RuntimeState::Idle, None, 1);
        assert!(s.run && s.run_from && s.run_selected);
        assert_eq!(s.debug_run, s.run);
        assert_eq!(s.debug_from, s.run_from);
        assert_eq!(s.debug_selected, s.run_selected);
        assert!(!s.pause && !s.resume && !s.stop);
        let blocked = decide(RuntimeState::Idle, Some("macro is disabled".into()), 1);
        assert!(!blocked.run);
        assert_eq!(blocked.debug_run, blocked.run);
        assert_eq!(blocked.debug_from, blocked.run_from);
        assert_eq!(blocked.debug_selected, blocked.run_selected);
        assert_eq!(
            blocked.disabled_reason.as_deref(),
            Some("macro is disabled")
        );
    }

    #[test]
    fn debug_command_availability_and_identity_are_pure() {
        assert_eq!(
            DEBUG_COMMANDS,
            [
                ToolbarCommand::DebugRun,
                ToolbarCommand::DebugFrom,
                ToolbarCommand::DebugSelected,
            ]
        );

        for runtime in [
            RuntimeState::Running,
            RuntimeState::Paused,
            RuntimeState::Stopping,
        ] {
            let state = decide(runtime, None, 1);
            assert!(
                DEBUG_COMMANDS
                    .into_iter()
                    .all(|command| !state.command_enabled(command))
            );
        }

        let idle_with_one_row = decide(RuntimeState::Idle, None, 1);
        assert!(idle_with_one_row.command_enabled(ToolbarCommand::DebugRun));
        assert!(idle_with_one_row.command_enabled(ToolbarCommand::DebugFrom));
        assert!(idle_with_one_row.command_enabled(ToolbarCommand::DebugSelected));

        let idle_with_no_rows = decide(RuntimeState::Idle, None, 0);
        assert!(idle_with_no_rows.command_enabled(ToolbarCommand::DebugRun));
        assert!(!idle_with_no_rows.command_enabled(ToolbarCommand::DebugFrom));
        assert!(!idle_with_no_rows.command_enabled(ToolbarCommand::DebugSelected));
    }

    #[test]
    fn selection_enablement_cannot_override_a_non_idle_runtime() {
        for runtime in [
            RuntimeState::Running,
            RuntimeState::Paused,
            RuntimeState::Stopping,
        ] {
            for selected_rows in [0, 1, 2] {
                let state = decide(runtime, None, selected_rows);
                assert!(!state.command_enabled(ToolbarCommand::DebugFrom));
                assert!(!state.command_enabled(ToolbarCommand::DebugSelected));
            }
        }
    }

    #[test]
    fn running_and_paused_controls_are_deterministic() {
        let running = decide(RuntimeState::Running, None, 1);
        assert!(running.pause && running.stop);
        assert!(!running.run && !running.resume);
        assert_eq!(running.debug_run, running.run);
        assert_eq!(running.debug_from, running.run_from);
        assert_eq!(running.debug_selected, running.run_selected);
        let paused = decide(RuntimeState::Paused, None, 1);
        assert!(paused.resume && paused.stop);
        assert!(!paused.run && !paused.pause);
        assert_eq!(paused.debug_run, paused.run);
        assert_eq!(paused.debug_from, paused.run_from);
        assert_eq!(paused.debug_selected, paused.run_selected);
    }

    #[test]
    fn debug_availability_is_exactly_normal_availability_for_all_runtime_states() {
        for runtime in [
            RuntimeState::Idle,
            RuntimeState::Running,
            RuntimeState::Paused,
            RuntimeState::Stopping,
            RuntimeState::Completed,
            RuntimeState::Stopped,
            RuntimeState::Failed,
        ] {
            for reason in [
                None,
                Some("fatal validation".into()),
                Some("disabled macro".into()),
            ] {
                for selected_rows in 0..=3 {
                    let state = decide(runtime, reason.clone(), selected_rows);
                    let idle = !matches!(
                        runtime,
                        RuntimeState::Running | RuntimeState::Paused | RuntimeState::Stopping
                    );
                    let expected_run = idle && reason.is_none();
                    assert_eq!(state.debug_run, state.run);
                    assert_eq!(state.debug_from, state.run_from);
                    assert_eq!(state.debug_selected, state.run_selected);
                    assert_eq!(state.run, expected_run);
                    assert_eq!(
                        state.run_from_reason,
                        if !expected_run {
                            reason.clone()
                        } else if selected_rows != 1 {
                            Some("Select exactly one step".into())
                        } else {
                            None
                        }
                    );
                    assert_eq!(
                        state.run_selected_reason,
                        if !expected_run {
                            reason.clone()
                        } else if selected_rows == 0 {
                            Some("Select one or more steps".into())
                        } else {
                            None
                        }
                    );
                }
            }
        }
    }
}
