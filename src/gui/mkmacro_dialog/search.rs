use super::{MkMacroDialog, navigation};
use crate::gui::theme::widgets::{StatusTone, semantic_status};
use crate::mkmacro::authoring_fields::ReplacementPreview;
use eframe::egui::{self, Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchMode {
    Find,
    Replace,
    Jump,
}

#[derive(Default)]
pub(super) struct SearchState {
    pub mode: Option<SearchMode>,
    query: String,
    replacement: String,
    focus_query: bool,
    filter: navigation::RowFilter,
    preview: Option<PreviewState>,
    error: Option<String>,
}

struct PreviewState {
    revision: u64,
    plan: ReplacementPreview,
    current: usize,
    all_error: Option<String>,
    macro_name: String,
    rows: std::sync::Arc<Vec<navigation::SearchRow>>,
}

fn query_id() -> egui::Id {
    egui::Id::new("mkmacro_search_query")
}
fn replacement_id() -> egui::Id {
    egui::Id::new("mkmacro_replacement_text")
}

fn owns_keyboard(ctx: &egui::Context, macro_id: Option<u64>, search_open: bool) -> bool {
    let focused = ctx.memory(|m| m.focused());
    focused.is_none()
        || macro_id.is_some_and(|mid| focused == Some(navigation::table_id(mid)))
        || (search_open && (focused == Some(query_id()) || focused == Some(replacement_id())))
}

pub(super) fn open(d: &mut MkMacroDialog, mode: SearchMode) {
    d.navigation.search.mode = Some(mode);
    d.navigation.search.focus_query = true;
    d.navigation.search.preview = None;
    d.navigation.search.error = None;
    d.editor_state.drag = None;
}

pub(super) fn close(d: &mut MkMacroDialog) {
    d.navigation.search.mode = None;
    d.navigation.search.preview = None;
    d.navigation.search.error = None;
}

/// Run before table input routing, consuming these keys so Enter/F3 cannot
/// accidentally edit or move the row that search just selected.
pub(super) fn shortcuts(ctx: &egui::Context, d: &mut MkMacroDialog) {
    if d.selected_macro().is_none()
        || d.action_editor.draft.is_some()
        || super::step_table::other_modal_open(d)
        || ctx.egui_is_using_pointer()
        || egui::Popup::is_any_open(ctx)
    {
        return;
    }
    if !owns_keyboard(ctx, d.selected_macro_id, d.navigation.search.mode.is_some()) {
        return;
    }
    let mode = ctx.input_mut(|i| {
        [
            (Key::F, SearchMode::Find),
            (Key::H, SearchMode::Replace),
            (Key::G, SearchMode::Jump),
        ]
        .into_iter()
        .find_map(|(key, mode)| i.consume_key(Modifiers::CTRL, key).then_some(mode))
    });
    if let Some(mode) = mode {
        open(d, mode);
    }
    // An open window routes its own navigation after laying out the query.
    if d.navigation.search.mode.is_none() {
        let direction = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::SHIFT, Key::F3) {
                Some(false)
            } else if i.consume_key(Modifiers::NONE, Key::F3) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(forward) = direction {
            let rows = navigation::rows(d);
            let state = &mut d.navigation.search;
            let matches = state.filter.matches(&rows, &state.query, false);
            if let Some(index) = next_match(&rows, &matches, d.selection.primary, forward) {
                navigation::navigate(d, rows[index].macro_id, rows[index].step_id);
                ctx.request_repaint();
            }
        }
    }
}

fn next_match(
    rows: &[navigation::SearchRow],
    matches: &[usize],
    primary: Option<u64>,
    forward: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let selected = primary.and_then(|id| rows.iter().position(|row| row.step_id == id));
    if forward {
        selected
            .and_then(|selected| matches.iter().find(|&&i| i > selected).copied())
            .or_else(|| matches.first().copied())
    } else {
        selected
            .and_then(|selected| matches.iter().rev().find(|&&i| i < selected).copied())
            .or_else(|| matches.last().copied())
    }
}

fn apply_preview(
    d: &mut MkMacroDialog,
    preview: &PreviewState,
    current: bool,
) -> Result<(), String> {
    if preview.revision != d.draft_revision() || d.selected_macro_id != Some(preview.plan.macro_id)
    {
        return Err("The draft or selected macro changed. Preview replacements again".into());
    }
    let candidate = preview
        .plan
        .candidate(&d.draft, current.then_some(preview.current))?;
    d.draft = candidate;
    d.mark_dirty();
    Ok(())
}

fn native_button_font(ui: &egui::Ui) -> (egui::FontId, egui::Vec2) {
    let style = ui.style().clone();
    let button = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| button.text_style.font_id.clone());
    let frame_margin = if ui.visuals().button_frame {
        button.frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    (font_id, frame_margin)
}

fn wrapped_button_size(ui: &egui::Ui, label: &str, available_width: f32) -> egui::Vec2 {
    let (font_id, frame_margin) = native_button_font(ui);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let text_size = egui::WidgetText::from(label)
        .into_galley(ui, Some(egui::TextWrapMode::Wrap), text_width, font_id)
        .size();
    let spacing = ui.spacing();
    egui::vec2(
        (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width),
        (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y),
    )
}

fn wrapped_button_row_height(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let available_width = ui.available_width().max(1.0);
    let spacing = ui.spacing();
    let mut row_width = 0.0;
    let mut row_height = 0.0_f32;
    let mut total_height = 0.0;
    for label in labels {
        let size = wrapped_button_size(ui, label, available_width);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + size.x > available_width {
            total_height += row_height + spacing.item_spacing.y;
            row_width = size.x;
            row_height = size.y;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += size.x;
            row_height = row_height.max(size.y);
        }
    }
    total_height += row_height;
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (total_height * pixels_per_point).ceil() / pixels_per_point
}

fn footer_reserved_height(ui: &egui::Ui, mode: SearchMode, has_preview: bool) -> f32 {
    let close_label = if mode == SearchMode::Replace {
        "Cancel"
    } else {
        "Close"
    };
    let spacing_y = ui.spacing().item_spacing.y;
    let close_height = wrapped_button_row_height(ui, &[close_label]);
    let height = if mode == SearchMode::Replace && has_preview {
        // Include the ScrollArea-to-actions gap and the actions-to-Cancel gap.
        wrapped_button_row_height(ui, &["Replace Current", "Replace All"])
            + close_height
            + 2.0 * spacing_y
    } else {
        // Include the ScrollArea-to-Close/Cancel gap.
        close_height + spacing_y
    };
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

fn search_body_max_height(ui: &egui::Ui, mode: SearchMode, has_preview: bool) -> f32 {
    (ui.available_height() - footer_reserved_height(ui, mode, has_preview))
        .max(0.0)
        .min(320.0)
}

pub(super) fn show(ctx: &egui::Context, d: &mut MkMacroDialog) {
    let Some(mode) = d.navigation.search.mode else {
        return;
    };
    if d.action_editor.draft.is_some() || super::step_table::other_modal_open(d) {
        return;
    }
    let rows = navigation::rows(d);
    let mut state = std::mem::take(&mut d.navigation.search);
    let mut window_open = true;
    let mut target = None;
    let mut apply = None;
    let mut close = false;
    let title = match mode {
        SearchMode::Find => "Find steps",
        SearchMode::Replace => "Replace step fields",
        SearchMode::Jump => "Jump to step",
    };
    egui::Window::new(title)
        .id(egui::Id::new("mkmacro_step_search"))
        .open(&mut window_open)
        .collapsible(false)
        .default_width(if mode == SearchMode::Replace {
            820.0
        } else {
            550.0
        })
        .show(ctx, |ui| {
            let macro_name = d
                .selected_macro()
                .map_or("No macro selected", |m| m.name.as_str());
            ui.strong(format!("Current macro: {macro_name}"));
            ui.label(if mode == SearchMode::Replace {
                "Find text"
            } else {
                "Search"
            });
            let query_width = ui.available_width().max(1.0);
            let response = ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .id(query_id())
                    .desired_width(query_width),
            );
            if state.focus_query {
                response.request_focus();
                state.focus_query = false;
            }
            if response.changed() {
                state.preview = None;
                state.error = None;
            }
            if mode == SearchMode::Replace {
                ui.label("Replace with");
                let replacement_width = ui.available_width().max(1.0);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut state.replacement)
                            .id(replacement_id())
                            .desired_width(replacement_width),
                    )
                    .changed()
                {
                    state.preview = None;
                    state.error = None;
                }
                ui.weak("Exact, case-sensitive text. Only editable fields are replaced.");
                if ui
                    .add(egui::Button::new("Preview replacements").wrap())
                    .clicked()
                {
                    state.preview = None;
                    state.error = None;
                    match d
                        .selected_macro_id
                        .ok_or_else(|| "Select a macro".to_owned())
                        .and_then(|mid| {
                            ReplacementPreview::prepare(
                                &d.draft,
                                mid,
                                &state.query,
                                &state.replacement,
                            )
                        }) {
                        Ok(plan) => {
                            let all_error = plan.candidate(&d.draft, None).err();
                            state.preview = Some(PreviewState {
                                revision: d.draft_revision(),
                                plan,
                                current: 0,
                                all_error,
                                macro_name: macro_name.to_owned(),
                                rows: rows.clone(),
                            });
                        }
                        Err(error) => state.error = Some(error),
                    }
                }
                if let Some(error) = &state.error {
                    semantic_status(ui, StatusTone::Error, error, None);
                }
                if let Some(preview) = &mut state.preview {
                    let stale = preview.revision != d.draft_revision()
                        || d.selected_macro_id != Some(preview.plan.macro_id);
                    ui.label(format!("{} affected fields", preview.plan.edits.len()));
                    if stale {
                        semantic_status(
                            ui,
                            StatusTone::Warning,
                            "The draft or selected macro changed. Preview again before applying.",
                            None,
                        );
                    }
                    if let Some(error) = &preview.all_error {
                        semantic_status(ui, StatusTone::Error, error, None);
                    }
                    let max_height = search_body_max_height(ui, mode, true);
                    egui::ScrollArea::vertical()
                        .id_salt("mkmacro_replace_preview")
                        .max_height(max_height)
                        .min_scrolled_height(0.0)
                        .show(ui, |ui| {
                            for (index, edit) in preview.plan.edits.iter().enumerate() {
                                let row = preview
                                    .rows
                                    .get(edit.row - 1)
                                    .filter(|row| row.step_id == edit.step_id);
                                let description = row.map_or_else(
                                    || format!("Step {}", edit.row),
                                    |row| {
                                        format!("{} · {} · {}", row.row, row.action_type, row.label)
                                    },
                                );
                                ui.group(|ui| {
                                    if ui
                                        .selectable_label(
                                            preview.current == index,
                                            format!(
                                                "{} · {description} · {}",
                                                preview.macro_name, edit.field
                                            ),
                                        )
                                        .clicked()
                                    {
                                        preview.current = index;
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong("Old:");
                                        ui.label(&edit.old);
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong("New:");
                                        ui.label(&edit.new);
                                    });
                                });
                            }
                        });
                    ui.horizontal_wrapped(|ui| {
                        let can_apply = !stale && !preview.plan.edits.is_empty();
                        if ui
                            .add_enabled(can_apply, egui::Button::new("Replace Current").wrap())
                            .on_hover_text("Apply the selected field change from the preview")
                            .clicked()
                        {
                            apply = Some(true);
                        }
                        if ui
                            .add_enabled(
                                can_apply && preview.all_error.is_none(),
                                egui::Button::new("Replace All").wrap(),
                            )
                            .clicked()
                        {
                            apply = Some(false);
                        }
                    });
                }
            } else {
                ui.weak("Search types, details, annotations, variables, text, and Call targets.");
                if let Some(error) = &state.error {
                    semantic_status(ui, StatusTone::Error, error, None);
                }
                let matches = state
                    .filter
                    .matches(&rows, &state.query, mode == SearchMode::Jump);
                let position = matches
                    .iter()
                    .position(|&index| Some(rows[index].step_id) == d.selection.primary);
                ui.horizontal_wrapped(|ui| {
                    ui.label(match position {
                        Some(index) => format!("{} of {} results", index + 1, matches.len()),
                        None => format!("{} results", matches.len()),
                    });
                    let mut direction = None;
                    if ui
                        .add_enabled(!matches.is_empty(), egui::Button::new("Previous").wrap())
                        .clicked()
                    {
                        direction = Some(false);
                    }
                    if ui
                        .add_enabled(!matches.is_empty(), egui::Button::new("Next").wrap())
                        .clicked()
                    {
                        direction = Some(true);
                    }
                    let keyboard_owned = owns_keyboard(ctx, d.selected_macro_id, true);
                    let key_direction = ui.input_mut(|i| {
                        if !keyboard_owned {
                            return None;
                        }
                        if i.consume_key(Modifiers::SHIFT, Key::F3)
                            || i.consume_key(Modifiers::SHIFT, Key::Enter)
                        {
                            Some(false)
                        } else if i.consume_key(Modifiers::NONE, Key::F3)
                            || i.consume_key(Modifiers::NONE, Key::Enter)
                        {
                            Some(true)
                        } else {
                            None
                        }
                    });
                    if let Some(forward) = direction.or(key_direction) {
                        // Jump with Enter activates its highlighted row; Find
                        // advances past the current source row with wraparound.
                        let index = if mode == SearchMode::Jump && key_direction.is_some() {
                            position
                                .and_then(|index| matches.get(index).copied())
                                .or_else(|| matches.first().copied())
                        } else {
                            next_match(&rows, &matches, d.selection.primary, forward)
                        };
                        if let Some(index) = index {
                            target = Some((rows[index].macro_id, rows[index].step_id));
                            close = mode == SearchMode::Jump;
                        }
                    }
                });
                if state.query.trim().is_empty() && mode == SearchMode::Find {
                    ui.weak("Enter text to find matching steps.");
                }
                let max_height = search_body_max_height(ui, mode, false);
                let row_height = navigation::selectable_row_height(ui);
                egui::ScrollArea::vertical()
                    .id_salt("mkmacro_search_results")
                    .max_height(max_height)
                    .min_scrolled_height(0.0)
                    .show_rows(ui, row_height, matches.len(), |ui, range| {
                        for index in range {
                            let row = &rows[matches[index]];
                            if ui
                                .add(row.selectable(ui, d.selection.primary == Some(row.step_id)))
                                .on_hover_ui(|ui| row.hover(ui))
                                .clicked()
                            {
                                target = Some((row.macro_id, row.step_id));
                                close = mode == SearchMode::Jump;
                            }
                        }
                    });
            }
            let close_label = if mode == SearchMode::Replace {
                "Cancel"
            } else {
                "Close"
            };
            if ui.add(egui::Button::new(close_label).wrap()).clicked()
                || (owns_keyboard(ctx, d.selected_macro_id, true)
                    && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)))
            {
                close = true;
            }
        });
    if let Some(current) = apply
        && let Some(preview) = &state.preview
    {
        match apply_preview(d, preview, current) {
            Ok(()) => {
                if current {
                    let edit = &preview.plan.edits[preview.current];
                    target = Some((preview.plan.macro_id, edit.step_id));
                }
                state.preview = None;
                state.error = None;
                close = true;
            }
            Err(error) => state.error = Some(error),
        }
    }
    if close || !window_open {
        state.mode = None;
        state.preview = None;
        d.navigation.focus_table = true;
    }
    d.navigation.search = state;
    if let Some((mid, id)) = target {
        navigation::navigate(d, mid, id);
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::*;

    fn dialog() -> (tempfile::TempDir, MkMacroDialog) {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut d = MkMacroDialog::new(std::sync::Arc::new(store));
        d.create_macro();
        d.selected_macro_mut().unwrap().steps = (1..=3)
            .map(|id| MkStep {
                id,
                enabled: true,
                breakpoint: false,
                repeat: 1,
                delay_after_ms: 0,
                on_error: Default::default(),
                metadata: MkStepMetadata {
                    label: "old".into(),
                    ..Default::default()
                },
                action: MkAction::Delay(Default::default()),
            })
            .collect();
        d.mark_dirty();
        d.dirty = false;
        (directory, d)
    }

    fn preview(d: &MkMacroDialog) -> PreviewState {
        PreviewState {
            revision: d.draft_revision(),
            plan: ReplacementPreview::prepare(&d.draft, d.selected_macro_id.unwrap(), "old", "new")
                .unwrap(),
            current: 1,
            all_error: None,
            macro_name: d.selected_macro().unwrap().name.clone(),
            rows: navigation::rows(d),
        }
    }

    #[test]
    fn next_previous_wrap_and_empty_matches_have_stable_source_order() {
        let (_directory, d) = dialog();
        let rows = navigation::rows(&d);
        let matched = [0, 2];
        assert_eq!(next_match(&rows, &matched, None, true), Some(0));
        assert_eq!(next_match(&rows, &matched, Some(1), true), Some(2));
        assert_eq!(next_match(&rows, &matched, Some(3), true), Some(0));
        assert_eq!(next_match(&rows, &matched, Some(1), false), Some(2));
        assert_eq!(next_match(&rows, &matched, Some(2), false), Some(0));
        assert_eq!(next_match(&rows, &[], Some(1), true), None);
    }

    #[test]
    fn replace_apply_is_one_revision_and_rejects_stale_preview_or_macro_switch() {
        let (_directory, mut d) = dialog();
        let prepared = preview(&d);
        let revision = d.draft_revision();
        let mid = d.selected_macro_id;
        d.set_selected_macro(None);
        assert!(apply_preview(&mut d, &prepared, false).is_err());
        assert_eq!(d.draft_revision(), revision);
        assert!(!d.dirty);
        d.set_selected_macro(mid);
        apply_preview(&mut d, &prepared, true).unwrap();
        assert_eq!(d.draft_revision(), revision + 1);
        assert!(d.dirty);
        assert_eq!(
            d.selected_macro()
                .unwrap()
                .steps
                .iter()
                .map(|s| s.metadata.label.as_str())
                .collect::<Vec<_>>(),
            ["old", "new", "old"]
        );
        let after = d.draft.clone();
        assert!(apply_preview(&mut d, &prepared, false).is_err());
        assert_eq!(d.draft, after);
        let prepared = preview(&d);
        apply_preview(&mut d, &prepared, false).unwrap();
        assert_eq!(d.draft_revision(), revision + 2);
        assert!(
            d.selected_macro()
                .unwrap()
                .steps
                .iter()
                .all(|s| s.metadata.label == "new")
        );
    }

    #[test]
    fn search_keyboard_ownership_respects_text_fields_and_table_modal_routing() {
        let (_directory, mut d) = dialog();
        let ctx = egui::Context::default();
        let mid = d.selected_macro_id;
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("unrelated_text")));
        assert!(!owns_keyboard(&ctx, mid, false));
        assert!(!owns_keyboard(&ctx, mid, true));
        ctx.memory_mut(|m| m.request_focus(query_id()));
        assert!(!owns_keyboard(&ctx, mid, false));
        assert!(owns_keyboard(&ctx, mid, true));
        open(&mut d, SearchMode::Find);
        assert!(super::super::step_table::table_modal_open(&d));
        ctx.memory_mut(|m| m.request_focus(navigation::table_id(mid.unwrap())));
        assert!(owns_keyboard(&ctx, mid, true));
        assert!(super::super::step_table::table_modal_open(&d));
        d.close_children();
        assert!(!super::super::step_table::table_modal_open(&d));
        assert!(!d.dirty);
    }

    #[test]
    fn native_scaled_metrics_keep_search_and_table_controls_visible_without_mutation() {
        use egui::{accesskit::Role, pos2, vec2};

        fn accessible_rect(bounds: egui::accesskit::Rect) -> egui::Rect {
            egui::Rect::from_min_max(
                pos2(bounds.x0 as f32, bounds.y0 as f32),
                pos2(bounds.x1 as f32, bounds.y1 as f32),
            )
        }

        fn same_rect(left: egui::Rect, right: egui::Rect) -> bool {
            (left.min.x - right.min.x).abs() < 0.5
                && (left.min.y - right.min.y).abs() < 0.5
                && (left.max.x - right.max.x).abs() < 0.5
                && (left.max.y - right.max.y).abs() < 0.5
        }

        let (_directory, mut d) = dialog();
        d.selected_macro_mut().unwrap().steps = (1..=40)
            .map(|id| MkStep {
                id,
                enabled: true,
                breakpoint: false,
                repeat: 12_345,
                delay_after_ms: 1_234_567,
                on_error: Default::default(),
                metadata: Default::default(),
                action: MkAction::Delay(Default::default()),
            })
            .collect();
        d.mark_dirty();
        d.dirty = false;
        let before_draft = d.draft.clone();
        let before_revision = d.draft_revision();
        let before_selection = (
            d.selection.ids.clone(),
            d.selection.anchor,
            d.selection.primary,
        );

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(760.0, 600.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();

        let mut default_metrics = None;
        let default_output = ctx.run_ui(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                default_metrics = Some((
                    navigation::selectable_row_height(ui),
                    super::super::step_table::table_header_row_height(ui),
                    super::super::step_table::table_body_row_height(ui),
                ));
            });
        });
        default_output.drop_without_applying_deltas();

        ctx.style_mut_of(ctx.theme(), |style| {
            style.override_font_id = Some(egui::FontId::new(28.0, egui::FontFamily::Proportional));
        });

        open(&mut d, SearchMode::Find);
        d.navigation.search.query = "delay".into();
        let mut close_bounds = None;
        let mut previous_bounds = None;
        let mut next_bounds = None;
        for _ in 0..4 {
            let output = ctx.run_ui(input(), |ctx| show(ctx, &mut d));
            if let Some(update) = output.platform_output.accesskit_update.as_ref() {
                for (_, node) in &update.nodes {
                    let Some(bounds) = node.bounds().map(accessible_rect) else {
                        continue;
                    };
                    if node.role() != Role::Button {
                        continue;
                    }
                    match node.label() {
                        Some("Close") => close_bounds = Some(bounds),
                        Some("Previous") => previous_bounds = Some(bounds),
                        Some("Next") => next_bounds = Some(bounds),
                        _ => {}
                    }
                }
            }
            output.drop_without_applying_deltas();
        }

        let rows = navigation::rows(&d);
        let mut search_row_geometry = None;
        let row_output = ctx.run_ui(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let row_height = navigation::selectable_row_height(ui);
                egui::ScrollArea::vertical()
                    .id_salt("mkmacro_search_results")
                    .max_height(row_height * 3.0)
                    .min_scrolled_height(0.0)
                    .show_rows(ui, row_height, rows.len(), |ui, range| {
                        for index in range {
                            let response = ui.add(rows[index].selectable(ui, false));
                            if index == 0 {
                                search_row_geometry =
                                    Some((response.rect, ui.clip_rect(), row_height));
                            }
                        }
                    });
            });
        });
        row_output.drop_without_applying_deltas();
        d.navigation.search.mode = None;

        let mut previous_viewport = None;
        let mut table_viewport = None;
        let mut table_metrics = None;
        let mut stable_viewport_observed = false;
        let mut enabled_header_bounds = std::collections::HashMap::new();
        let mut checkbox_bounds = std::collections::HashMap::new();
        let mut spin_button_bounds = std::collections::HashMap::new();
        for _ in 0..4 {
            let mut viewport = None;
            let mut metrics = None;
            let output = ctx.run_ui(input(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    metrics = Some((
                        super::super::step_table::table_header_row_height(ui),
                        super::super::step_table::table_body_row_height(ui),
                    ));
                    viewport = super::super::step_table::show(ui, &mut d);
                });
            });
            if let Some(update) = output.platform_output.accesskit_update.as_ref() {
                for (node_id, node) in &update.nodes {
                    let Some(bounds) = node.bounds().map(accessible_rect) else {
                        continue;
                    };
                    if node.role() == Role::Label
                        && (node.label() == Some("Enabled") || node.value() == Some("Enabled"))
                    {
                        enabled_header_bounds.insert(*node_id, bounds);
                    }
                    match node.role() {
                        Role::CheckBox => {
                            checkbox_bounds.insert(*node_id, bounds);
                        }
                        Role::SpinButton => {
                            spin_button_bounds.insert(*node_id, bounds);
                        }
                        _ => {}
                    }
                }
            }
            if let (Some(previous), Some(current), Some(current_metrics)) =
                (previous_viewport, viewport, metrics)
            {
                stable_viewport_observed = same_rect(previous, current);
                table_viewport = Some(current);
                table_metrics = Some(current_metrics);
            }
            previous_viewport = viewport;
            output.drop_without_applying_deltas();
        }

        let (default_row, default_header, default_body) =
            default_metrics.expect("the default native metrics are measured");
        let (scaled_header, scaled_body) =
            table_metrics.expect("the scaled table viewport settles");
        assert!(
            stable_viewport_observed,
            "the scaled main-table viewport settles"
        );
        let table_viewport = table_viewport.expect("the table has an actual viewport");
        assert!(
            table_viewport.is_positive() && screen_rect.contains_rect(table_viewport),
            "the table viewport {table_viewport:?} fits the actual screen {screen_rect:?}"
        );
        let (search_row, search_clip, scaled_row) =
            search_row_geometry.expect("the first shared search result is rendered");
        assert!(
            scaled_row > default_row,
            "scaled selectable row: {scaled_row} vs {default_row}"
        );
        assert!(
            scaled_header > default_header,
            "scaled table header: {scaled_header} vs {default_header}"
        );
        assert!(
            scaled_body > default_body,
            "scaled table body: {scaled_body} vs {default_body}"
        );
        assert!(
            search_row.height() <= scaled_row + 0.5,
            "native search row height {} fits its measured stride {scaled_row}",
            search_row.height()
        );
        assert!(
            search_clip.contains_rect(search_row),
            "native search row {search_row:?} is inside its actual scroll clip {search_clip:?}"
        );
        assert!(
            enabled_header_bounds.values().any(|bounds| {
                bounds.is_positive()
                    && table_viewport.contains_rect(*bounds)
                    && bounds.height() <= scaled_header + 0.5
            }),
            "the native Enabled header fits its measured row height {scaled_header}: {enabled_header_bounds:?}, {table_viewport:?}"
        );
        for (name, bounds) in [
            ("Close", close_bounds),
            ("Previous", previous_bounds),
            ("Next", next_bounds),
        ] {
            let bounds = bounds.expect("the search window exposes its native footer controls");
            assert!(
                bounds.is_positive() && screen_rect.contains_rect(bounds),
                "search {name} bounds {bounds:?} fit the actual screen {screen_rect:?}"
            );
        }
        assert!(
            checkbox_bounds.values().any(|bounds| {
                bounds.is_positive()
                    && table_viewport.contains_rect(*bounds)
                    && bounds.height() <= scaled_body + 0.5
            }),
            "a native checkbox fits the table viewport and row height {scaled_body}: {checkbox_bounds:?}, {table_viewport:?}"
        );
        assert!(
            spin_button_bounds.values().any(|bounds| {
                bounds.is_positive()
                    && table_viewport.contains_rect(*bounds)
                    && bounds.height() <= scaled_body + 0.5
            }),
            "a native DragValue fits the table viewport and row height {scaled_body}: {spin_button_bounds:?}, {table_viewport:?}"
        );
        assert_eq!(d.draft, before_draft);
        assert_eq!(d.draft_revision(), before_revision);
        assert!(!d.dirty);
        assert_eq!(
            (
                d.selection.ids.clone(),
                d.selection.anchor,
                d.selection.primary,
            ),
            before_selection
        );
    }
}
