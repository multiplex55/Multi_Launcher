use super::MkMacroDialog;
pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    if d.selection.ids.len() == 1 {
        ui.separator();
        ui.label("Select an executable action to edit its typed fields.");
    }
}
