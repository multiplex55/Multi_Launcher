use super::{MkMacroDialog, navigation};
use std::sync::Arc;

pub(super) struct OutlineState {
    pub open: bool,
    show_all: bool,
    query: String,
    filter: navigation::RowFilter,
    last_matches: Option<Arc<Vec<usize>>>,
    last_show_all: bool,
    visible: Arc<Vec<usize>>,
}
impl Default for OutlineState {
    fn default() -> Self {
        Self {
            open: false,
            show_all: false,
            query: String::new(),
            filter: Default::default(),
            last_matches: None,
            last_show_all: false,
            visible: Arc::default(),
        }
    }
}

pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    if !d.navigation.outline.open {
        if ui.button("◀").on_hover_text("Show Outline").clicked() {
            d.navigation.outline.open = true;
        }
        return;
    }
    ui.horizontal(|ui| {
        ui.strong("Outline");
        if ui
            .small_button("▶")
            .on_hover_text("Collapse Outline")
            .clicked()
        {
            d.navigation.outline.open = false;
        }
    });
    let rows = navigation::rows(d);
    let mut target = None;
    let blocked = d.action_editor.draft.is_some() || super::step_table::table_modal_open(d);
    ui.add_enabled_ui(!blocked, |ui| {
        let state = &mut d.navigation.outline;
        ui.checkbox(&mut state.show_all, "Show all steps");
        ui.add(eframe::egui::TextEdit::singleline(&mut state.query).hint_text("Filter outline"));
        let matches = state.filter.matches(&rows, &state.query, true);
        if state
            .last_matches
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &matches))
            || state.last_show_all != state.show_all
        {
            state.visible = Arc::new(
                matches
                    .iter()
                    .copied()
                    .filter(|&index| state.show_all || rows[index].outline_node)
                    .collect(),
            );
            state.last_matches = Some(matches);
            state.last_show_all = state.show_all;
        }
        eframe::egui::ScrollArea::vertical()
            .id_source("mkmacro_outline")
            .show_rows(ui, 24.0, state.visible.len(), |ui, range| {
                for index in range {
                    let row = &rows[state.visible[index]];
                    ui.horizontal(|ui| {
                        ui.add_space((row.depth as f32 * 12.0).min(ui.available_width() / 2.0));
                        if ui
                            .add(row.selectable(ui, d.selection.primary == Some(row.step_id)))
                            .on_hover_text(format!("{}\n{}", row.details, row.comment))
                            .clicked()
                        {
                            target = Some((row.macro_id, row.step_id));
                        }
                    });
                }
            });
    });
    if let Some((mid, id)) = target {
        navigation::navigate(d, mid, id);
        ui.ctx().request_repaint();
    }
}
