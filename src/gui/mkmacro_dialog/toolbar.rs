use super::MkMacroDialog;
pub(super) fn show(ui: &mut eframe::egui::Ui, dialog: &mut MkMacroDialog) {
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            let _ = dialog.save();
        }
        if let Some(reason) = dialog.playback_block_reason() {
            ui.add_enabled(false, eframe::egui::Button::new("Run"))
                .on_disabled_hover_text(reason);
        } else {
            ui.button("Run");
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
    });
}
