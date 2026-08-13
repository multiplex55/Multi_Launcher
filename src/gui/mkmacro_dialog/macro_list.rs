use super::MkMacroDialog;
pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    ui.heading("Macros");
    ui.text_edit_singleline(&mut d.search);
    for m in &d.draft.macros {
        if (d.search.is_empty() || m.name.to_lowercase().contains(&d.search.to_lowercase()))
            && ui
                .selectable_label(d.selected_macro == Some(m.id), &m.name)
                .clicked()
        {
            d.selected_macro = Some(m.id);
            d.selection.clear();
        }
    }
}
