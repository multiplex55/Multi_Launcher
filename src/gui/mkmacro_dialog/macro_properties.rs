use super::MkMacroDialog;
use super::key_capture::{apply_captured_hotkey, captured_chord, chord_hotkey, hotkey_name};
use eframe::egui;

fn clear_hotkey(hotkey: &mut Option<crate::mkmacro::MkHotkey>) -> bool {
    hotkey.take().is_some()
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
    let mut changed = false;
    let mut capture = None;
    let mut clear = false;
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
