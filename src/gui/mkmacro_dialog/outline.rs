use super::{MkMacroDialog, navigation};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct OutlineState {
    pub open: bool,
    show_all: bool,
    query: String,
    filter: navigation::RowFilter,
    last_matches: Option<Arc<Vec<usize>>>,
    last_show_all: bool,
    visible: Arc<Vec<usize>>,
}

pub(super) fn closed_rail_width(ui: &eframe::egui::Ui) -> f32 {
    let style = ui.style().clone();
    let button = style.button_style(
        &eframe::egui::widget_style::Classes::default(),
        eframe::egui::widget_style::WidgetState::Inactive,
    );
    let font_id = super::step_table::effective_font_id(ui, button.text_style.font_id.clone());
    let text_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap("◀".to_owned(), font_id, button.text_style.color)
            .size()
            .x
    });
    let frame_width = if ui.visuals().button_frame {
        button.frame.total_margin().sum().x
    } else {
        0.0
    };

    30.0_f32.max(text_width + frame_width)
}

pub(super) fn show(ui: &mut eframe::egui::Ui, d: &mut MkMacroDialog) {
    if !d.navigation.outline.open {
        let _rail_rect = ui.available_rect_before_wrap();
        let response = ui.button("◀").on_hover_text("Show Outline");
        #[cfg(test)]
        {
            d.last_outline_closed_button_geometry = Some((_rail_rect, response.rect));
        }
        if response.clicked() {
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
        let row_height = navigation::selectable_row_height(ui);
        eframe::egui::ScrollArea::vertical()
            .id_salt("mkmacro_outline")
            .show_rows(ui, row_height, state.visible.len(), |ui, range| {
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
