use super::{MkMacroDialog, navigation};
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
        || ctx.is_using_pointer()
        || ctx.memory(|m| m.any_popup_open())
        || ctx.is_context_menu_open()
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
            ui.horizontal(|ui| {
                ui.label(if mode == SearchMode::Replace {
                    "Find text"
                } else {
                    "Search"
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .id(query_id())
                        .desired_width(f32::INFINITY),
                );
                if state.focus_query {
                    response.request_focus();
                    state.focus_query = false;
                }
                if response.changed() {
                    state.preview = None;
                    state.error = None;
                }
            });
            if mode == SearchMode::Replace {
                ui.horizontal(|ui| {
                    ui.label("Replace with");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut state.replacement)
                                .id(replacement_id())
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        state.preview = None;
                        state.error = None;
                    }
                });
                ui.weak("Exact, case-sensitive text. Only editable fields are replaced.");
                if ui.button("Preview replacements").clicked() {
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
                if let Some(preview) = &mut state.preview {
                    let stale = preview.revision != d.draft_revision()
                        || d.selected_macro_id != Some(preview.plan.macro_id);
                    ui.label(format!("{} affected fields", preview.plan.edits.len()));
                    if stale {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "The draft or selected macro changed. Preview again before applying.",
                        );
                    }
                    if let Some(error) = &preview.all_error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
                    egui::ScrollArea::vertical()
                        .id_source("mkmacro_replace_preview")
                        .max_height(320.0)
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
                    ui.horizontal(|ui| {
                        let can_apply = !stale && !preview.plan.edits.is_empty();
                        if ui
                            .add_enabled(can_apply, egui::Button::new("Replace Current"))
                            .on_hover_text("Apply the selected field change from the preview")
                            .clicked()
                        {
                            apply = Some(true);
                        }
                        if ui
                            .add_enabled(
                                can_apply && preview.all_error.is_none(),
                                egui::Button::new("Replace All"),
                            )
                            .clicked()
                        {
                            apply = Some(false);
                        }
                    });
                }
            } else {
                ui.weak("Search types, details, annotations, variables, text, and Call targets.");
                let matches = state
                    .filter
                    .matches(&rows, &state.query, mode == SearchMode::Jump);
                let position = matches
                    .iter()
                    .position(|&index| Some(rows[index].step_id) == d.selection.primary);
                ui.horizontal(|ui| {
                    ui.label(match position {
                        Some(index) => format!("{} of {} results", index + 1, matches.len()),
                        None => format!("{} results", matches.len()),
                    });
                    let mut direction = None;
                    if ui
                        .add_enabled(!matches.is_empty(), egui::Button::new("Previous"))
                        .clicked()
                    {
                        direction = Some(false);
                    }
                    if ui
                        .add_enabled(!matches.is_empty(), egui::Button::new("Next"))
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
                egui::ScrollArea::vertical()
                    .id_source("mkmacro_search_results")
                    .max_height(320.0)
                    .show_rows(ui, 24.0, matches.len(), |ui, range| {
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
            if let Some(error) = &state.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            if ui
                .button(if mode == SearchMode::Replace {
                    "Cancel"
                } else {
                    "Close"
                })
                .clicked()
                || (owns_keyboard(ctx, d.selected_macro_id, true)
                    && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)))
            {
                close = true;
            }
        });
    if let Some(current) = apply {
        if let Some(preview) = &state.preview {
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
}
