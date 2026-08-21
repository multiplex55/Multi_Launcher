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
    pub disabled_reason: Option<String>,
}
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
    ToolbarState {
        run,
        pause: runtime == RuntimeState::Running,
        resume: runtime == RuntimeState::Paused,
        stop: matches!(runtime, RuntimeState::Running | RuntimeState::Paused),
        run_from: run && selected_rows == 1,
        run_selected: run && selected_rows != 0,
        disabled_reason,
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
    let state = state(dialog);
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            let result = dialog.save();
            report(dialog, result);
        }
        if ui
            .add_enabled(
                dialog.selected_macro().is_some(),
                eframe::egui::Button::new("+ Action"),
            )
            .clicked()
        {
            dialog.action_catalog_visible = true;
        }
        if !state.run {
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
        if ui
            .add_enabled(state.pause, eframe::egui::Button::new("Pause"))
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::pause());
        }
        if ui
            .add_enabled(state.resume, eframe::egui::Button::new("Resume"))
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::resume());
        }
        if ui
            .add_enabled(state.stop, eframe::egui::Button::new("Stop"))
            .clicked()
        {
            report(dialog, crate::mkmacro::runtime::stop());
        }
        if ui
            .add_enabled(state.run_from, eframe::egui::Button::new("Run From Here"))
            .clicked()
        {
            if let Some(id) = dialog.selection.ids.iter().next().copied() {
                let result = dialog.run_from_step(id);
                report(dialog, result)
            }
        }
        if ui
            .add_enabled(
                state.run_selected,
                eframe::egui::Button::new("Run Selected"),
            )
            .clicked()
        {
            let result = dialog.run_selected_steps();
            report(dialog, result);
        }
        if dialog.dirty {
            ui.label("Unsaved changes");
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
            .is_some_and(|s| s.state != RecorderRuntimeState::Idle);
        if ui
            .add_enabled(
                !recorder_active && dialog.selected_macro().is_some(),
                eframe::egui::Button::new("Record"),
            )
            .clicked()
        {
            if let Some(id) = dialog.selected_macro_id {
                report(
                    dialog,
                    crate::mkmacro::runtime::record(id, dialog.recorder_options.clone()),
                );
            }
        }
        ui.menu_button("Record Options", |ui| {
            ui.set_enabled(!recorder_active);
            ui.checkbox(&mut dialog.recorder_options.record_keyboard, "Keyboard");
            ui.checkbox(
                &mut dialog.recorder_options.record_mouse_buttons,
                "Mouse buttons",
            );
            ui.checkbox(
                &mut dialog.recorder_options.record_mouse_wheel,
                "Mouse wheel",
            );
            ui.label("Input");
            ui.checkbox(
                &mut dialog.recorder_options.record_injected_input,
                "Record injected input",
            );
            ui.separator();
            ui.label("Mouse movement");
            for (mode, label) in [
                (MovementMode::Off, "Off"),
                (MovementMode::ClicksOnly, "Clicks Only"),
                (MovementMode::SampledMovement, "Sampled Movement"),
                (MovementMode::DetailedMovement, "Detailed Movement"),
            ] {
                let help = match mode {
                    MovementMode::SampledMovement => {
                        "Editable default: samples and simplifies the mouse path."
                    }
                    MovementMode::DetailedMovement => {
                        "High-fidelity option: retains every distinct mouse position."
                    }
                    _ => "Does not record standalone mouse movement.",
                };
                ui.radio_value(&mut dialog.recorder_options.movement_mode, mode, label)
                    .on_hover_text(help);
            }
            ui.separator();
            ui.label("Window context");
            ui.checkbox(&mut dialog.recorder_options.record_window_context, "Record active/target windows")
                .on_hover_text("Captures active and target windows and generates Activate Window actions.");
            ui.small("Uses client-relative coordinates when reliable metadata is available; otherwise falls back to Screen.");
            let sampled = dialog.recorder_options.movement_mode == MovementMode::SampledMovement;
            ui.add_enabled(
                sampled,
                eframe::egui::Slider::new(
                    &mut dialog.recorder_options.movement_distance_px,
                    1..=500,
                )
                .text("Sample distance (px)"),
            );
            ui.add_enabled(
                sampled,
                eframe::egui::Slider::new(
                    &mut dialog.recorder_options.movement_interval_ms,
                    1..=5000,
                )
                .text("Sample interval (ms)"),
            );
        });
    });
    if let Some(rec) = crate::mkmacro::runtime::recorder_snapshot()
        .filter(|s| s.state != RecorderRuntimeState::Idle)
    {
        ui.horizontal(|ui| {
            let secs=rec.elapsed.as_secs(); ui.label(format!("● Recording {:02}:{:02} — {} raw events — ~{} actions",secs/60,secs%60,rec.raw_event_count,rec.estimated_action_count));
            if rec.dropped_event_count>0 { ui.colored_label(eframe::egui::Color32::YELLOW,format!("{} events dropped",rec.dropped_event_count)); }
            match rec.state {
                RecorderRuntimeState::Recording => if ui.button("Pause Recording").clicked(){report(dialog,crate::mkmacro::runtime::record_pause())},
                RecorderRuntimeState::Paused => if ui.button("Resume Recording").clicked(){report(dialog,crate::mkmacro::runtime::record_resume())},
                _ => {}
            }
            if rec.state!=RecorderRuntimeState::Stopping && ui.button("Stop Recording").clicked() {
                match crate::mkmacro::runtime::record_stop() {
                    Err(e)=>dialog.command_error=Some(e.to_string()),
                    Ok(result)=> {
                        if dialog.apply_recording(result.macro_id, &result.generated_steps).is_ok() {
                            if result.dropped_event_count>0 { dialog.command_error=Some(format!("Recording completed with {} dropped events",result.dropped_event_count)); }
                        } else {
                            dialog.pending_recording=Some((result.macro_id,result.generated_steps));
                            dialog.command_error=Some("Recording target was deleted; captured actions were preserved for recovery".into());
                        }
                    }
                }
            }
        });
    }
    if let Some(error) = &dialog.command_error {
        ui.colored_label(eframe::egui::Color32::RED, error);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_controls_follow_eligibility_and_selection() {
        let s = decide(RuntimeState::Idle, None, 1);
        assert!(s.run && s.run_from && s.run_selected);
        assert!(!s.pause && !s.resume && !s.stop);
        let blocked = decide(RuntimeState::Idle, Some("macro is disabled".into()), 1);
        assert!(!blocked.run);
        assert_eq!(
            blocked.disabled_reason.as_deref(),
            Some("macro is disabled")
        );
    }
    #[test]
    fn running_and_paused_controls_are_deterministic() {
        let running = decide(RuntimeState::Running, None, 1);
        assert!(running.pause && running.stop);
        assert!(!running.run && !running.resume);
        let paused = decide(RuntimeState::Paused, None, 1);
        assert!(paused.resume && paused.stop);
        assert!(!paused.run && !paused.pause);
    }
}
