use super::MkMacroDialog;
pub const SIDEBAR_WIDTH: f32 = 220.0;
pub(super) fn show_empty(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    ui.vertical_centered(|ui| {
        ui.heading("Mouse/Keyboard Macros");
        ui.label("Create reusable keyboard, mouse, window, and automation workflows.");
        if ui
            .add_sized([180.0, 36.0], eframe::egui::Button::new("+ Create Macro"))
            .clicked()
        {
            d.create_macro();
        }
        ui.add_enabled(false, eframe::egui::Button::new("Record New Macro"))
            .on_disabled_hover_text("Macro recording integration is coming soon.");
    });
}
pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    ui.heading("Macros");
    ui.horizontal(|ui| {
        if ui.button("New").clicked() {
            d.create_macro();
        }
        let selected = d.selected_macro().is_some();
        if ui
            .add_enabled(selected, eframe::egui::Button::new("Duplicate"))
            .clicked()
        {
            d.duplicate_selected_macro();
        }
        if ui
            .add_enabled(selected, eframe::egui::Button::new("Delete"))
            .clicked()
        {
            d.request_delete_selected_macro();
        }
    });
    ui.add(eframe::egui::TextEdit::singleline(&mut d.search).hint_text("Search"));
    let mut clicked = None;
    eframe::egui::ScrollArea::vertical().show(ui, |ui| {
        for m in &d.draft.macros {
            if (d.search.is_empty() || m.name.to_lowercase().contains(&d.search.to_lowercase()))
                && ui
                    .selectable_label(d.selected_macro_id == Some(m.id), &m.name)
                    .clicked()
            {
                clicked = Some(m.id);
            }
        }
    });
    if let Some(id) = clicked {
        d.selected_macro_id = Some(id);
        d.selection.clear();
    }
}
