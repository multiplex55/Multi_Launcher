use crate::diff::model::TextCompareOrigin;
use crate::diff::model::{DiffSide, TextViewModel};
use crate::diff::text_compare::{
    ChangeImportance, DiffRowKind, FindScope, NavigationDirection, RowProjectionMode,
};
use crate::gui::theme::widgets::{StatusTone, semantic_status};
use eframe::egui::{self, Color32, RichText};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextViewAction {
    CompareSelectionWithClipboard { view_id: u64 },
}

fn equality_status_labels(
    raw_equal: bool,
    effectively_equal: bool,
) -> (&'static str, &'static str) {
    (
        if raw_equal {
            "Raw: identical"
        } else {
            "Raw: different"
        },
        if effectively_equal {
            "Effective: equal under current rules and ignored ranges"
        } else {
            "Effective: different under current rules and ignored ranges"
        },
    )
}

pub fn show(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    model: &mut TextViewModel,
) -> Option<TextViewAction> {
    model.poll();
    shortcuts(ui, model);
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        if ui.button("◀ Previous (F7)").clicked() {
            model.navigate(NavigationDirection::Previous)
        }
        if ui.button("Next (F8) ▶").clicked() {
            model.navigate(NavigationDirection::Next)
        }
        let merge = model.comparison.is_some() && !model.external_conflict.iter().any(|x| *x);
        if ui
            .add_enabled(merge, egui::Button::new("Copy →"))
            .on_hover_text("Copy current difference left-to-right (Ctrl+Alt+Right)")
            .clicked()
            && let Err(e) = model.copy_hunk(DiffSide::Left)
        {
            model.right_error = Some(e)
        }
        if ui
            .add_enabled(merge, egui::Button::new("← Copy"))
            .on_hover_text(
                "Copy current difference right-to-left (Ctrl+Alt+Left; Alt+Left is Back)",
            )
            .clicked()
            && let Err(e) = model.copy_hunk(DiffSide::Right)
        {
            model.left_error = Some(e)
        }
        if ui.button("Find").clicked() {
            model.find_open = true;
        }
        if ui
            .add_enabled(
                model.has_fresh_source_selection(),
                egui::Button::new("Compare selected lines with Clipboard"),
            )
            .clicked()
        {
            action = Some(TextViewAction::CompareSelectionWithClipboard { view_id: view });
        }
        if ui.button("Previous bookmark").clicked()
            && let Err(error) = model.navigate_bookmark(false)
        {
            model.annotation_notice = Some(error);
        }
        if ui.button("Next bookmark").clicked()
            && let Err(error) = model.navigate_bookmark(true)
        {
            model.annotation_notice = Some(error);
        }
        let bookmark_label = model
            .source_line_selection
            .map(|selection| match selection.side {
                DiffSide::Left => ("Left", selection.focus_line + 1),
                DiffSide::Right => ("Right", selection.focus_line + 1),
            });
        if ui
            .add_enabled(
                model.has_fresh_source_selection(),
                egui::Button::new("Toggle bookmark"),
            )
            .clicked()
        {
            let selected = bookmark_label;
            match model.toggle_bookmark_at_selection() {
                Ok(added) => {
                    if let Some((side, line)) = selected {
                        model.annotation_notice = Some(format!(
                            "{} bookmark on {side} line {line}.",
                            if added { "Added" } else { "Removed" }
                        ));
                    }
                }
                Err(error) => model.annotation_notice = Some(error),
            }
        }
        ui.menu_button("Ignored lines", |ui| {
            let selected = model.source_line_selection.is_some();
            if ui
                .add_enabled(selected, egui::Button::new("Ignore selected lines"))
                .clicked()
            {
                if let Err(error) = model.ignore_selected_lines() {
                    model.annotation_notice = Some(error);
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    selected,
                    egui::Button::new("Restore selected ignored lines"),
                )
                .clicked()
            {
                if let Err(error) = model.restore_selected_ignored_lines() {
                    model.annotation_notice = Some(error);
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    !model.left.ignored_line_ranges().is_empty()
                        || !model.right.ignored_line_ranges().is_empty(),
                    egui::Button::new("Clear ignored ranges"),
                )
                .clicked()
            {
                model.clear_ignored_ranges();
                ui.close();
            }
        });
        let mut sync = model.scroll.sync_vertical && model.scroll.sync_horizontal;
        if ui.checkbox(&mut sync, "Sync Scroll").changed() {
            model.scroll.set_sync(sync, sync);
        }
        ui.menu_button("View", |ui| {
            if ui.checkbox(&mut model.wrap, "Wrap").changed() {
                model.scroll.wrapped = model.wrap;
            }
            let mut vertical = model.scroll.sync_vertical;
            let mut horizontal = model.scroll.sync_horizontal;
            let changed = ui.checkbox(&mut vertical, "Sync Vertical").changed()
                | ui.checkbox(&mut horizontal, "Sync Horizontal").changed();
            if changed {
                model.scroll.set_sync(vertical, horizontal);
            }
            ui.add_enabled(
                model.large_file_tier.syntax_enabled(),
                egui::Checkbox::new(&mut model.syntax, "Syntax"),
            );
            ui.checkbox(&mut model.visible_whitespace, "Visible Whitespace");
            ui.add_enabled_ui(model.visible_whitespace, |ui| {
                ui.checkbox(&mut model.visible_line_endings, "Line Endings");
            });
            ui.checkbox(&mut model.text_details_open, "Text Details");
            let mut projection_mode = model.projection_mode;
            let mut projection_context = model.projection_context;
            let mut projection_changed = ui
                .selectable_value(&mut projection_mode, RowProjectionMode::All, "All rows")
                .changed()
                | ui.selectable_value(
                    &mut projection_mode,
                    RowProjectionMode::DifferencesOnly,
                    "Differences only",
                )
                .changed()
                | ui.selectable_value(
                    &mut projection_mode,
                    RowProjectionMode::IdenticalOnly,
                    "Identical only",
                )
                .changed();
            if projection_mode == RowProjectionMode::DifferencesOnly {
                projection_changed |= ui
                    .add(
                        egui::Slider::new(
                            &mut projection_context,
                            0..=crate::diff::text_compare::MAX_ROW_PROJECTION_CONTEXT,
                        )
                        .text("Context lines"),
                    )
                    .changed();
            }
            if projection_changed {
                model.set_row_projection(projection_mode, projection_context);
            }
            let mut ignore = model.rules.ignore_all_whitespace;
            if ui
                .selectable_value(&mut ignore, false, "All differences")
                .changed()
                || ui
                    .selectable_value(&mut ignore, true, "Important differences")
                    .changed()
            {
                model.set_ignore_all_whitespace(ignore);
            }
        });
        ui.menu_button("More", |ui| {
            super::export::text_menu(ui, model);
            if ui.button("Rules…").clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new(("diff-rules", view)),
                        super::rules_dialog::RulesDialogState::new(&model.rules),
                    )
                });
                ui.close();
            }
            if ui.button("First").clicked() {
                model.navigate(NavigationDirection::First);
            }
            if ui.button("Last").clicked() {
                model.navigate(NavigationDirection::Last);
            }
            if ui.button("Previous Intraline Difference").clicked() {
                model.navigate_intraline(NavigationDirection::Previous);
            }
            if ui.button("Next Intraline Difference").clicked() {
                model.navigate_intraline(NavigationDirection::Next);
            }
            if ui
                .add_enabled(
                    model.left.can_undo() || model.right.can_undo(),
                    egui::Button::new("Undo"),
                )
                .clicked()
            {
                model.undo();
            }
            if ui
                .add_enabled(
                    model.left.can_redo() || model.right.can_redo(),
                    egui::Button::new("Redo"),
                )
                .clicked()
            {
                model.redo();
            }
            let save_left = if model.origin == TextCompareOrigin::Memory {
                "Save Left As…"
            } else {
                "Save Left"
            };
            let save_right = if model.origin == TextCompareOrigin::Memory {
                "Save Right As…"
            } else {
                "Save Right"
            };
            if ui.button(save_left).clicked() {
                save_side(model, DiffSide::Left);
            }
            if ui.button(save_right).clicked() {
                save_side(model, DiffSide::Right);
            }
            if ui.button("Save All Modified").clicked() {
                if model.left.is_dirty() {
                    save_side(model, DiffSide::Left);
                }
                if model.right.is_dirty() {
                    save_side(model, DiffSide::Right);
                }
            }
        });
    });
    if model.find_open {
        ui.horizontal_wrapped(|ui| {
            ui.label("Find:");
            let find_width = ui.available_width().max(ui.spacing().interact_size.x);
            let changed = ui
                .add(egui::TextEdit::singleline(&mut model.find_query).desired_width(find_width))
                .changed();
            egui::ComboBox::from_id_salt((view, "find-scope"))
                .selected_text(format!("{:?}", model.find_scope))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut model.find_scope, FindScope::Both, "Both panes");
                    ui.selectable_value(
                        &mut model.find_scope,
                        FindScope::CurrentSide,
                        "Current pane",
                    );
                    ui.selectable_value(&mut model.find_scope, FindScope::Left, "Left");
                    ui.selectable_value(&mut model.find_scope, FindScope::Right, "Right");
                });
            let changed = ui
                .checkbox(&mut model.find_case_sensitive, "Case sensitive")
                .changed()
                | ui.checkbox(&mut model.find_projection_only, "Visible projection only")
                    .changed()
                | changed;
            if changed {
                model.refresh_find();
            }
            if ui.button("Previous").clicked() {
                model.navigate_find(false);
            }
            if ui.button("Next").clicked() {
                model.navigate_find(true);
            }
            ui.label(format!("{} matches", model.find_matches.len()));
        });
    }
    ui.separator();
    let number = model.comparison.as_ref().and_then(|c| {
        model
            .current_row
            .and_then(|r| c.difference_number(r, false))
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(number.map_or("Difference —/—".into(), |(n, t)| {
            format!("Difference {n}/{t}")
        }));
        side_status(
            ui,
            "Left",
            &model.left,
            model.external_conflict[0],
            model.left_error.as_deref(),
        );
        side_status(
            ui,
            "Right",
            &model.right,
            model.external_conflict[1],
            model.right_error.as_deref(),
        );
        if model.recalculating {
            ui.label("⏳ Recalculating… (showing last valid alignment)");
        }
        ui.label(model.large_file_tier.explanation());
        if model.comparison_is_fresh()
            && let Some(comparison) = model.comparison.as_ref()
        {
            let (raw_equality, effective_equality) =
                equality_status_labels(comparison.raw_equal, comparison.equal_under_rules);
            ui.horizontal_wrapped(|ui| {
                ui.strong(raw_equality);
                ui.label(effective_equality);
            });
            if let Some(selection) = model.source_line_selection {
                let (source_id, revision) = match selection.side {
                    DiffSide::Left => (model.left.source_id(), model.left.revision),
                    DiffSide::Right => (model.right.source_id(), model.right.revision),
                };
                if source_id == selection.source_id && revision == selection.revision {
                    let projection = model.projected_rows();
                    let visible_lines: std::collections::HashSet<_> = projection
                        .iter()
                        .filter_map(|row| {
                            crate::diff::model::visual_to_source(
                                &comparison.rows[*row],
                                selection.side,
                            )
                        })
                        .collect();
                    let hidden =
                        (selection.start..selection.end).any(|line| !visible_lines.contains(&line));
                    ui.label(format!(
                        "{} lines {}–{} selected{}",
                        if selection.side == DiffSide::Left {
                            "Left"
                        } else {
                            "Right"
                        },
                        selection.start + 1,
                        selection.end,
                        if hidden {
                            " (some selected lines are hidden by the current row filter)"
                        } else {
                            ""
                        }
                    ));
                }
            }
        }
        if let Some(notice) = model.annotation_notice.as_deref() {
            semantic_status(ui, StatusTone::Info, notice, None);
        }
        if let Some(notice) = model.projection_notice.as_deref() {
            semantic_status(ui, StatusTone::Info, notice, None);
        }
        if let Some(error) = model.alignment_error.as_deref() {
            semantic_status(ui, StatusTone::Warning, error, None);
        }
    });
    ui.separator();
    let available = ui.available_width().max(0.0);
    let total_height = ui.available_height().max(0.0);
    let details_height = if model.text_details_open {
        model
            .text_details_height
            .clamp(72.0, (total_height * 0.45).max(72.0))
    } else {
        0.0
    };
    let comparison_height = (total_height - details_height).max(0.0);
    model.splitter = crate::diff::model::validated_splitter(model.splitter);
    let (left_width, splitter_width, right_width) =
        comparison_dimensions(available, comparison_height, model.splitter);
    prepare_row_measurements(ui, model, left_width, right_width);
    let pending_request = model.pending_scroll_request();
    let pending = pending_request.map(|request| request.row());
    let pending_measurement_key = model.row_measurements.key.clone();
    let mut scroll_accepted = pending_request.is_some();
    ui.horizontal(|ui| {
        // `comparison_dimensions` accounts for every pixel; no implicit item
        // spacing may be inserted around the dedicated splitter.
        ui.spacing_mut().item_spacing.x = 0.0;
        scroll_accepted &= exact_pane(ui, egui::vec2(left_width, comparison_height), |ui| {
            pane(ui, workspace, view, DiffSide::Left, model, pending)
        });
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(splitter_width, comparison_height),
            egui::Sense::drag(),
        );
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().widgets.inactive.bg_fill);
        if let Some(c) = &model.comparison {
            let pixels = rect.height().max(1.0) as usize;
            for pixel in 0..pixels {
                let a = pixel * c.rows.len() / pixels;
                let b = ((pixel + 1) * c.rows.len() / pixels).min(c.rows.len());
                let color = c.rows[a..b]
                    .iter()
                    .find_map(|r| match (r.kind, r.importance) {
                        (DiffRowKind::Ignored, _) => Some(Color32::from_rgb(85, 110, 145)),
                        (_, ChangeImportance::Unimportant) => Some(Color32::YELLOW),
                        (DiffRowKind::Modified, _) => Some(Color32::from_rgb(240, 180, 40)),
                        (DiffRowKind::Deleted, _) => Some(Color32::RED),
                        (DiffRowKind::Inserted, _) => Some(Color32::GREEN),
                        _ => None,
                    });
                if let Some(color) = color {
                    let y = rect.top() + pixel as f32;
                    ui.painter().line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        egui::Stroke::new(1.0_f32, color),
                    );
                }
            }
            if let Some(row) = model.current_row {
                let y = rect.top() + rect.height() * row as f32 / c.rows.len().max(1) as f32;
                ui.painter().line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(2.0_f32, Color32::WHITE),
                );
            }
        }
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            model.request_overview_scroll(pos.y - rect.top(), rect.height());
        }
        if response.dragged() {
            model.splitter = crate::diff::model::validated_splitter(
                model.splitter + response.drag_delta().x / available,
            );
        }
        scroll_accepted &= exact_pane(ui, egui::vec2(right_width, comparison_height), |ui| {
            pane(ui, workspace, view, DiffSide::Right, model, pending)
        });
    });
    if let (Some(request), Some(measurement_key)) =
        (pending_request, pending_measurement_key.as_ref())
        && scroll_accepted
    {
        model.accept_pending_scroll_request(request, measurement_key);
    }
    if model.text_details_open {
        text_details(ui, model, details_height);
    }
    super::rules_dialog::show(ui.ctx(), view, model);
    action
}

pub(super) fn save_side(model: &mut TextViewModel, side: DiffSide) -> bool {
    let name = match side {
        DiffSide::Left => "left.txt",
        DiffSide::Right => "right.txt",
    };
    save_side_with_picker(model, side, || {
        rfd::FileDialog::new()
            .add_filter("Text files", &["txt"])
            .set_file_name(name)
            .save_file()
    })
}

fn save_side_with_picker(
    model: &mut TextViewModel,
    side: DiffSide,
    pick_destination: impl FnOnce() -> Option<std::path::PathBuf>,
) -> bool {
    if model.origin == TextCompareOrigin::Memory {
        pick_destination().is_some_and(|path| model.save_as_memory(side, &path))
    } else {
        model.save(side)
    }
}

const TEXT_INSET: f32 = 72.0;

fn monospace_font(ui: &egui::Ui) -> egui::FontId {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .cloned()
        .unwrap_or_else(|| egui::FontId::monospace(14.0))
}

fn minimum_visual_row_height(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    ui.ctx()
        .fonts_mut(|fonts| fonts.row_height(font))
        .max(crate::diff::model::MIN_VISUAL_LINE_HEIGHT)
}

fn monospace_glyph_width(ui: &egui::Ui, font: &egui::FontId) -> f32 {
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap("0".to_owned(), font.clone(), ui.visuals().text_color())
            .size()
            .x
    })
}

fn prepare_row_measurements(
    ui: &egui::Ui,
    model: &mut TextViewModel,
    left_pane: f32,
    right_pane: f32,
) {
    let Some(c) = model.comparison.as_ref() else {
        model.row_measurements = Default::default();
        return;
    };
    let font = monospace_font(ui);
    let inset = TEXT_INSET * (font.size / 14.0);
    let row_height = minimum_visual_row_height(ui, &font);
    let left_width = (left_pane - inset).max(1.0);
    let right_width = (right_pane - inset).max(1.0);
    let projected = crate::diff::text_compare::row_projection(
        c,
        model.projection_mode,
        model.projection_context,
    );
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    ui.visuals().dark_mode.hash(&mut identity);
    font.family.hash(&mut identity);
    font.size.to_bits().hash(&mut identity);
    row_height.to_bits().hash(&mut identity);
    ui.ctx().pixels_per_point().to_bits().hash(&mut identity);
    let key = crate::diff::model::RowMeasureKey {
        left_source_id: model.left.source_id(),
        right_source_id: model.right.source_id(),
        left_revision: model.left.revision,
        right_revision: model.right.revision,
        comparison_revision: c.rules_revision,
        annotation_revision: c.annotation_revision,
        left_text_width_bits: left_width.to_bits(),
        right_text_width_bits: right_width.to_bits(),
        font_theme: identity.finish(),
        text_style: font.size.to_bits() as u64,
        display_config: ((model.syntax as u64) << 3)
            | ((model.visible_whitespace as u64) << 2)
            | ((model.visible_line_endings as u64) << 1)
            | model.large_file_tier.syntax_enabled() as u64,
        wrap: model.wrap,
        projection_mode: match model.projection_mode {
            RowProjectionMode::All => 0,
            RowProjectionMode::DifferencesOnly => 1,
            RowProjectionMode::IdenticalOnly => 2,
        },
        projection_context: model.projection_context,
    };
    if model.row_measurements.key.as_ref() == Some(&key) {
        return;
    }
    let left_source = model.left.source();
    let right_source = model.right.source();
    let heights: Vec<f32> = projected
        .iter()
        .map(|&comparison_row| {
            let row = &c.rows[comparison_row];
            let measure = |side: DiffSide, width: f32| {
                let source = if side == DiffSide::Left {
                    left_source
                } else {
                    right_source
                };
                let line = crate::diff::model::visual_to_source(row, side)
                    .and_then(|n| source.lines().nth(n))
                    .unwrap_or("");
                let displayed = if model.visible_whitespace {
                    crate::diff::text_compare::visible_whitespace(line, model.visible_line_endings)
                } else {
                    line.to_owned()
                };
                if !model.wrap {
                    return row_height;
                }
                let mut job = egui::text::LayoutJob::simple(
                    displayed.replace('\t', "    "),
                    font.clone(),
                    ui.visuals().text_color(),
                    width,
                );
                job.wrap.max_width = width;
                ui.ctx().fonts_mut(|fonts| fonts.layout_job(job).size().y)
            };
            measure(DiffSide::Left, left_width).max(measure(DiffSide::Right, right_width))
        })
        .collect();
    model
        .row_measurements
        .rebuild(key, projected, c.rows.len(), heights);
}

fn projected_measurement_for_row(
    comparison_row: usize,
    projected: &[usize],
    measurements: &crate::diff::model::RowMeasurementCache,
) -> Option<usize> {
    if measurements.key.is_none() {
        return None;
    }
    let projected_row = projected.binary_search(&comparison_row).ok()?;
    (measurements.projected_to_comparison.get(projected_row) == Some(&comparison_row)
        && measurements
            .comparison_to_projected
            .get(comparison_row)
            .and_then(|row| *row)
            == Some(projected_row)
        && measurements.heights.get(projected_row).is_some())
    .then_some(projected_row)
}

/// Allocates the pane before laying out any of its content. The child can grow
/// a scroll canvas, but its viewport and paint clip remain exactly `size`.
fn exact_pane<R>(ui: &mut egui::Ui, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    add(&mut child)
}

fn comparison_dimensions(width: f32, height: f32, splitter: f32) -> (f32, f32, f32) {
    let width = width.max(0.0);
    let _height = height.max(0.0);
    let splitter_width = 8.0_f32.min(width);
    let pane_width = (width - splitter_width).max(0.0);
    let left = pane_width * splitter.clamp(0.0, 1.0);
    (left, splitter_width, (pane_width - left).max(0.0))
}

/// Conservative geometry used to size the unwrapped virtual canvas before a
/// galley is painted. Tabs use the same four-column stops as the text pane.
pub(super) fn unwrapped_content_width(text: &str, glyph_width: f32, gutter_width: f32) -> f32 {
    let mut columns = 0usize;
    for character in text.chars() {
        if character == '\t' {
            columns += 4 - columns % 4;
        } else {
            columns += 1;
        }
    }
    gutter_width + columns as f32 * glyph_width.max(0.0)
}

fn horizontal_scroll_range(pane_width: f32, content_width: f32) -> f32 {
    (content_width - pane_width.max(0.0)).max(0.0)
}
fn pane(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    side: DiffSide,
    model: &mut TextViewModel,
    pending: Option<usize>,
) -> bool {
    if model.wrap {
        render_wrapped_pane(ui, workspace, view, side, model, pending)
    } else {
        render_unwrapped_pane(ui, workspace, view, side, model, pending)
    }
}

fn render_unwrapped_pane(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    side: DiffSide,
    model: &mut TextViewModel,
    pending: Option<usize>,
) -> bool {
    render_pane_contents(ui, workspace, view, side, model, pending, false)
}

fn render_wrapped_pane(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    side: DiffSide,
    model: &mut TextViewModel,
    pending: Option<usize>,
) -> bool {
    render_pane_contents(ui, workspace, view, side, model, pending, true)
}

fn render_pane_contents(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    side: DiffSide,
    model: &mut TextViewModel,
    pending: Option<usize>,
    wrapped: bool,
) -> bool {
    let Some(c) = model.comparison.as_ref() else {
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
        return false;
    };
    let source = match side {
        DiffSide::Left => model.left.source(),
        DiffSide::Right => model.right.source(),
    };
    let ignored_ranges = match side {
        DiffSide::Left => model.left.ignored_line_ranges().to_vec(),
        DiffSide::Right => model.right.ignored_line_ranges().to_vec(),
    };
    let projected = crate::diff::text_compare::row_projection(
        c,
        model.projection_mode,
        model.projection_context,
    );
    let rows = &c.rows;
    let font = monospace_font(ui);
    let row_height = minimum_visual_row_height(ui, &font);
    let glyph_width = monospace_glyph_width(ui, &font);
    let gutter_width = TEXT_INSET * (font.size / 14.0);
    let measured_heights = model.row_measurements.heights.clone();
    let measured_offsets = model.row_measurements.offsets.clone();
    let measurement_cache = model.row_measurements.clone();
    let axis_id = if wrapped {
        "vertical"
    } else {
        "horizontal_vertical"
    };
    let (stored_x, stored_y) = model.scroll.offsets(side);
    let mut scroll = if wrapped {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    }
    .auto_shrink([false, false])
    .id_salt((workspace, view, side, axis_id))
    .vertical_scroll_offset(stored_y);
    if !wrapped {
        scroll = scroll.horizontal_scroll_offset(stored_x);
    }
    let pending_visual =
        pending.and_then(|row| projected_measurement_for_row(row, &projected, &measurement_cache));
    let scroll_target_accepted = pending.is_none() || pending_visual.is_some();
    if pending.is_some() && model.row_measurements.key.is_none() {
        ui.ctx().request_repaint();
    }
    if let Some(visual) = pending_visual {
        let offset = if wrapped {
            measurement_cache.offset_for_row(visual) as f32
        } else {
            visual as f32 * row_height
        };
        scroll = scroll.vertical_scroll_offset(offset);
    }
    // Defer model mutation until after the scroll callback releases its
    // immutable borrow of the retained comparison.
    let mut selected_row = None;
    let mut selection_action: Option<(usize, bool)> = None;
    let mut begin_alignment_action = None;
    let mut alignment_action: Option<usize> = None;
    let mut remove_anchor = None;
    let mut render_range = |ui: &mut egui::Ui, range: std::ops::Range<usize>| {
        if !wrapped {
            let canvas_width = source
                .lines()
                .map(|line| unwrapped_content_width(line, glyph_width, gutter_width))
                .fold(ui.available_width(), f32::max);
            ui.set_min_width(canvas_width);
        }
        for projected_i in range {
            let i = projected[projected_i];
            let row = &rows[i];
            let current = model.current_row == Some(i);
            let line = crate::diff::model::visual_to_source(row, side);
            let selected = line.is_some_and(|line| {
                model.source_line_selection.is_some_and(|selection| {
                    selection.side == side
                        && selection.source_id
                            == if side == DiffSide::Left {
                                model.left.source_id()
                            } else {
                                model.right.source_id()
                            }
                        && selection.revision
                            == if side == DiffSide::Left {
                                model.left.revision
                            } else {
                                model.right.revision
                            }
                        && (selection.start..selection.end).contains(&line)
                })
            });
            let bg = match (row.kind, row.importance) {
                (DiffRowKind::Ignored, _) => Color32::from_rgb(35, 49, 69),
                (_, ChangeImportance::Unimportant) => Color32::from_rgb(70, 65, 35),
                (DiffRowKind::Equal, _) => Color32::TRANSPARENT,
                (DiffRowKind::Inserted, _) => Color32::from_rgb(28, 70, 42),
                (DiffRowKind::Deleted, _) => Color32::from_rgb(80, 35, 35),
                (DiffRowKind::Modified, _) => Color32::from_rgb(65, 55, 25),
            };
            let fill = if selected {
                Color32::from_rgb(30, 75, 105)
            } else if current {
                bg.gamma_multiply(1.5)
            } else {
                bg
            };
            let frame = egui::Frame::NONE.fill(fill);
            frame.show(ui, |ui| {
                ui.set_min_height(if wrapped {
                    measured_heights
                        .get(projected_i)
                        .copied()
                        .unwrap_or(row_height)
                } else {
                    row_height
                });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(line.map_or("·".into(), |n| (n + 1).to_string()))
                            .monospace()
                            .weak(),
                    );
                    let semantic = match row.kind {
                        DiffRowKind::Equal => "equal",
                        DiffRowKind::Inserted => "inserted",
                        DiffRowKind::Deleted => "deleted",
                        DiffRowKind::Modified => "modified",
                        DiffRowKind::Ignored => "ignored source line",
                    };
                    ui.label(
                        RichText::new(match side {
                            DiffSide::Left => "L",
                            DiffSide::Right => "R",
                        })
                        .strong(),
                    )
                    .on_hover_text(semantic);
                    if let Some(n) = line {
                        let bookmarked = match side {
                            DiffSide::Left => model.left.is_bookmarked(n),
                            DiffSide::Right => model.right.is_bookmarked(n),
                        };
                        if bookmarked {
                            ui.label(RichText::new("★").color(Color32::YELLOW))
                                .on_hover_text("Source-line bookmark");
                        }
                        if crate::diff::text_compare::line_is_ignored(&ignored_ranges, n) {
                            ui.label(RichText::new("⊘").color(Color32::LIGHT_BLUE))
                                .on_hover_text("Ignored in the effective comparison");
                        }
                        let text = source.lines().nth(n).unwrap_or("");
                        let ranges = match side {
                            DiffSide::Left => &row.left_ranges,
                            DiffSide::Right => &row.right_ranges,
                        };
                        let language = crate::diff::syntax::language_for_path(match side {
                            DiffSide::Left => model.left_path.as_deref(),
                            DiffSide::Right => model.right_path.as_deref(),
                        });
                        let syntax = if model.syntax && model.large_file_tier.syntax_enabled() {
                            model
                                .syntax_cache
                                .line(
                                    crate::diff::syntax::HighlightKey {
                                        revision: match side {
                                            DiffSide::Left => model.left.revision,
                                            DiffSide::Right => model.right.revision,
                                        },
                                        language,
                                        theme: "base16-ocean.dark".into(),
                                        line: n,
                                    },
                                    text,
                                )
                                .to_vec()
                        } else {
                            vec![]
                        };
                        let changed: Vec<_> =
                            ranges.iter().map(|r| (r.byte_start, r.byte_end)).collect();
                        let fragments =
                            crate::diff::syntax::render_fragments(text, &syntax, &changed);
                        let mut job = egui::text::LayoutJob::default();
                        for fragment in fragments {
                            let mut format = egui::TextFormat {
                                font_id: font.clone(),
                                color: fragment.rgb.map_or(ui.visuals().text_color(), |c| {
                                    Color32::from_rgb(c[0], c[1], c[2])
                                }),
                                ..Default::default()
                            };
                            if fragment.changed {
                                format.background =
                                    Color32::from_rgba_unmultiplied(255, 190, 40, 75);
                                format.underline =
                                    egui::Stroke::new(1.5_f32, Color32::from_rgb(255, 210, 80));
                            }
                            let displayed = if model.visible_whitespace {
                                crate::diff::text_compare::visible_whitespace(&fragment.text, false)
                            } else {
                                fragment.text
                            };
                            job.append(&displayed, 0.0, format);
                        }
                        if model.visible_whitespace && model.visible_line_endings {
                            job.append(
                                "¶",
                                0.0,
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color: Color32::LIGHT_BLUE,
                                    ..Default::default()
                                },
                            );
                        }
                        let id = egui::Id::new((
                            workspace,
                            view,
                            side,
                            row.id,
                            c.navigation.row_to_hunk[i],
                        ));
                        ui.push_id(id, |ui| {
                            // Both the job and the widget reject wrapping. This is
                            // essential: the galley must define the horizontal
                            // canvas instead of negotiating a wider parent pane.
                            if !wrapped {
                                job.wrap.max_width = f32::INFINITY;
                            } else {
                                // Force breaking even for uninterrupted tokens;
                                // wrapped content never negotiates a wider pane.
                                job.wrap.max_width = ui.available_width().max(1.0);
                            }
                            let label = ui
                                .add(egui::Label::new(job).wrap_mode(if wrapped {
                                    egui::TextWrapMode::Wrap
                                } else {
                                    egui::TextWrapMode::Extend
                                }))
                                .on_hover_text(format!("{semantic} line"));
                            let response = ui.interact(
                                label.rect,
                                label.id.with("source-line-selection"),
                                egui::Sense::click_and_drag(),
                            );
                            if response.clicked() {
                                selection_action = Some((n, ui.input(|i| i.modifiers.shift)));
                            } else if response.drag_started() {
                                selection_action = Some((n, false));
                            } else if model.source_line_selection.is_some()
                                && ui.input(|input| {
                                    input.pointer.primary_down()
                                        && input
                                            .pointer
                                            .interact_pos()
                                            .is_some_and(|pointer| response.rect.contains(pointer))
                                })
                            {
                                selection_action = Some((n, true));
                            }
                            if model.manual_alignments.iter().any(|a| match side {
                                DiffSide::Left => a.left_line == n,
                                DiffSide::Right => a.right_line == n,
                            }) {
                                ui.label(RichText::new("⚓").color(Color32::LIGHT_BLUE));
                            }
                            response.context_menu(|ui| {
                                if let Some(pending) = model.pending_alignment {
                                    let source_is_current = match pending.side {
                                        DiffSide::Left => {
                                            pending.source_id == model.left.source_id()
                                                && pending.revision == model.left.revision
                                        }
                                        DiffSide::Right => {
                                            pending.source_id == model.right.source_id()
                                                && pending.revision == model.right.revision
                                        }
                                    };
                                    if source_is_current
                                        && pending.side != side
                                        && ui.button("Preview / confirm alignment").clicked()
                                    {
                                        alignment_action = Some(n);
                                        ui.close();
                                    }
                                } else if ui.button("Align With…").clicked() {
                                    begin_alignment_action = Some(n);
                                    ui.close();
                                }
                                if let Some(anchor) =
                                    model
                                        .manual_alignments
                                        .iter()
                                        .copied()
                                        .find(|a| match side {
                                            DiffSide::Left => a.left_line == n,
                                            DiffSide::Right => a.right_line == n,
                                        })
                                    && ui.button("Remove alignment").clicked()
                                {
                                    remove_anchor = Some(anchor);
                                    ui.close();
                                }
                            });
                        });
                    } else if row.kind == DiffRowKind::Ignored {
                        ui.label(RichText::new("ignored line on other side").weak())
                            .on_hover_text("This row is excluded from navigation and hunk copy");
                    } else if ui
                        .button("＋ Insert line")
                        .on_hover_text("Missing on this side; insert real source text")
                        .clicked()
                    {
                        selected_row = Some(i);
                    }
                });
            });
        }
    };
    let output = if wrapped {
        scroll.show_viewport(ui, |ui, viewport| {
            let range = measurement_cache.visible_range(viewport.top(), viewport.height(), 4);
            let before = measured_offsets.get(range.start).copied().unwrap_or(0.0);
            let after = measurement_cache.total_height()
                - measured_offsets.get(range.end).copied().unwrap_or(0.0);
            ui.add_space(before as f32);
            render_range(ui, range);
            ui.add_space(after.max(0.0) as f32);
        })
    } else {
        scroll.show_rows(ui, row_height, projected.len(), |ui, range| {
            render_range(ui, range)
        })
    };
    let new_x = output.state.offset.x;
    let new_y = output.state.offset.y;
    let focus_id = egui::Id::new((workspace, view, side, axis_id, "focus"));
    let pane_response = ui.interact(output.inner_rect, focus_id, egui::Sense::click_and_drag());
    if pane_response.clicked() {
        pane_response.request_focus();
    }
    if pane_response.has_focus()
        && let Some((delta, extend)) = ui.input_mut(|input| {
            for (key, delta) in [(egui::Key::ArrowUp, -1), (egui::Key::ArrowDown, 1)] {
                if input.consume_key(egui::Modifiers::SHIFT, key) {
                    return Some((delta, true));
                }
                if input.consume_key(egui::Modifiers::NONE, key) {
                    return Some((delta, false));
                }
            }
            None
        })
    {
        model.move_source_selection(side, delta, extend);
    }
    let offset_changed =
        (new_y - stored_y).abs() > 0.25 || (!wrapped && (new_x - stored_x).abs() > 0.25);
    let directly_interacting = (pane_response.hovered() || pane_response.has_focus())
        && ui.input(|input| {
            input.pointer.any_down()
                || input.smooth_scroll_delta != egui::Vec2::ZERO
                || input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::PageUp
                                | egui::Key::PageDown
                                | egui::Key::Home
                                | egui::Key::End,
                            pressed: true,
                            ..
                        }
                    )
                })
        });
    if offset_changed || directly_interacting {
        if wrapped {
            model
                .scroll
                .drive_measured(side, new_x, new_y, &model.row_measurements);
        } else {
            model.scroll.drive(side, new_x, new_y, row_height, false);
        }
        let visual = if wrapped {
            model.row_measurements.row_at_offset(new_y as f64).0
        } else {
            (new_y / row_height.max(1.0)) as usize
        };
        if let Some(&aligned) = projected.get(visual.min(projected.len().saturating_sub(1))) {
            model.scroll.aligned_row = aligned;
        }
        model.active_side = side;
        ui.ctx().request_repaint();
    }
    if let Some(row) = selected_row {
        model.active_side = side;
        model.set_current_row(Some(row));
    }
    if let Some((line, extend)) = selection_action {
        model.select_source_line(side, line, extend);
    }
    if let Some(line) = begin_alignment_action {
        model.begin_manual_alignment(side, line);
    }
    if let Some(target_line) = alignment_action {
        if let Some(pending) = model.pending_alignment.take() {
            let source_is_current = match pending.side {
                DiffSide::Left => {
                    pending.source_id == model.left.source_id()
                        && pending.revision == model.left.revision
                }
                DiffSide::Right => {
                    pending.source_id == model.right.source_id()
                        && pending.revision == model.right.revision
                }
            };
            if !source_is_current || model.source_line_is_ignored(side, target_line) {
                model.alignment_error = Some(
                    "Alignment was cancelled because a source changed or an endpoint is ignored."
                        .into(),
                );
                return true;
            }
            let anchor = crate::diff::text_compare::ManualTextAlignment {
                left_line: if pending.side == DiffSide::Left {
                    pending.source_line
                } else {
                    target_line
                },
                right_line: if pending.side == DiffSide::Right {
                    pending.source_line
                } else {
                    target_line
                },
            };
            if let Err(error) = model.add_manual_alignment(anchor) {
                model.alignment_error = Some(error);
            }
        }
    }
    if let Some(anchor) = remove_anchor {
        model.remove_manual_alignment(anchor);
    }
    scroll_target_accepted
}

fn text_details(ui: &mut egui::Ui, model: &mut TextViewModel, height: f32) {
    let chrome_top = ui.cursor().min.y;
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.strong("Text Details");
        ui.add(
            egui::DragValue::new(&mut model.text_details_height)
                .range(72.0..=400.0)
                .suffix(" px"),
        );
    });
    let content_height = (height - (ui.cursor().min.y - chrome_top)).max(1.0);
    let row = model
        .current_row
        .and_then(|i| model.comparison.as_ref()?.rows.get(i));
    let value = |side| {
        row.and_then(|r| crate::diff::model::visual_to_source(r, side))
            .map(|n| {
                let source = if side == DiffSide::Left {
                    model.left.source()
                } else {
                    model.right.source()
                };
                source.lines().nth(n).unwrap_or("").to_owned()
            })
            .unwrap_or_default()
    };
    ui.allocate_ui(egui::vec2(ui.available_width(), content_height), |ui| {
        ui.columns(2, |columns| {
            for (column, side) in columns.iter_mut().zip([DiffSide::Left, DiffSide::Right]) {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(column, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(value(side)).monospace())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            }
        });
    });
}
fn side_status(
    ui: &mut egui::Ui,
    name: &str,
    d: &crate::diff::text_file::TextDocument,
    conflict: bool,
    error: Option<&str>,
) {
    let state = if conflict {
        "conflict"
    } else if d.read_only {
        "read-only"
    } else if d.is_dirty() {
        "dirty"
    } else {
        "saved"
    };
    ui.label(format!("{name}: {state}"));
    if let Some(e) = error {
        semantic_status(ui, StatusTone::Error, e, None);
    }
}
fn shortcuts(ui: &egui::Ui, m: &mut TextViewModel) {
    let mut save_active_side = None;
    let mut save_all = false;
    ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::F) {
            m.find_open = true;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::F7) {
            m.navigate(NavigationDirection::Previous)
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::F8) {
            m.navigate(NavigationDirection::Next)
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Z) {
            m.undo();
        }
        if i.consume_key(
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
            egui::Key::Z,
        ) {
            m.redo();
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
            save_active_side = Some(m.active_side);
        }
        if i.consume_key(
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
            egui::Key::S,
        ) {
            save_all = true;
        }
    });
    if let Some(side) = save_active_side {
        save_side(m, side);
    }
    if save_all {
        if m.left.is_dirty() {
            save_side(m, DiffSide::Left);
        }
        if m.right.is_dirty() {
            save_side(m, DiffSide::Right);
        }
    }

    ui.input_mut(|i| {
        let copy_modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            ..Default::default()
        };
        if i.consume_key(copy_modifiers, egui::Key::ArrowRight)
            && let Err(error) = m.copy_hunk(DiffSide::Left)
        {
            m.right_error = Some(error);
        }
        if i.consume_key(copy_modifiers, egui::Key::ArrowLeft)
            && let Err(error) = m.copy_hunk(DiffSide::Right)
        {
            m.left_error = Some(error);
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num1) {
            m.active_side = DiffSide::Left;
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num2) {
            m.active_side = DiffSide::Right;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, fs, rc::Rc};

    fn wait_for_test_comparison(model: &mut TextViewModel) {
        for _ in 0..500 {
            model.poll();
            if model.comparison.is_some() && !model.recalculating {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(model.comparison.is_some(), "text comparison completed");
    }

    fn render_text_view_frame(
        context: &egui::Context,
        model: &mut TextViewModel,
        size: egui::Vec2,
    ) {
        render_text_view_frame_at_native_scale(context, model, size, 1.0);
    }

    fn render_text_view_frame_at_native_scale(
        context: &egui::Context,
        model: &mut TextViewModel,
        size: egui::Vec2,
        pixels_per_point: f32,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("default raw input includes the root viewport")
            .native_pixels_per_point = Some(pixels_per_point);
        context
            .run_ui(input, |ctx| {
                assert_eq!(ctx.viewport_rect().size(), size);
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, 7, 11, model);
                });
            })
            .drop_without_applying_deltas();
    }

    fn accessible_widget_bounds(
        output: &egui::FullOutput,
        role: egui::accesskit::Role,
        label: Option<&str>,
        value: Option<&str>,
    ) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find_map(|(_, node)| {
                (node.role() == role
                    && label.is_none_or(|expected| node.label() == Some(expected))
                    && value.is_none_or(|expected| node.value() == Some(expected)))
                .then(|| node.bounds())
                .flatten()
                .map(|bounds| {
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    )
                })
            })
    }

    #[test]
    fn diff_workspace_chrome_scaled_text_controls_remain_reachable() {
        let context = egui::Context::default();
        let theme = crate::settings::ThemeSettings::default();
        crate::gui::theme::publish_committed(&context, &theme);
        context.enable_accesskit();
        context.style_mut_of(context.theme(), |style| {
            style.animation_time = 0.0;
            style.override_font_id = Some(egui::FontId::proportional(24.0));
        });
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 720.0));
        let mut model = TextViewModel::from_test_text(
            "left text\nneedle left".to_owned(),
            "right text\nneedle right".to_owned(),
        );
        model.find_open = true;
        model.text_details_open = true;
        let mut settled_viewport = None;
        let mut settled_controls = [None; 6];

        for _ in 0..3 {
            let mut viewport = None;
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        viewport = Some(ui.clip_rect());
                        show(ui, 1, 2, &mut model);
                    });
                },
            );
            let controls = [
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some("◀ Previous (F7)"),
                    None,
                ),
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some("Next (F8) ▶"),
                    None,
                ),
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some("Find"),
                    None,
                ),
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some("Previous"),
                    None,
                ),
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some("Next"),
                    None,
                ),
                accessible_widget_bounds(&output, egui::accesskit::Role::TextInput, None, Some("")),
            ];
            output.drop_without_applying_deltas();
            settled_viewport = viewport.or(settled_viewport);
            for (settled, observed) in settled_controls.iter_mut().zip(controls) {
                *settled = observed.or(*settled);
            }
        }

        let viewport = settled_viewport.expect("CentralPanel supplied a native clip rect");
        for control in settled_controls {
            let bounds = control.expect("scaled text toolbar control is accessible");
            assert!(bounds.is_positive(), "{bounds:?}");
            assert!(
                viewport.contains_rect(bounds),
                "{bounds:?} outside {viewport:?}"
            );
        }
        assert_eq!(model.find_query, "");
        assert!(!model.left.is_dirty());
        assert!(!model.right.is_dirty());
        assert!(!model.saved_filesystem_mutation);
        assert_eq!(
            f32::from_bits(model.row_measurements.key.as_ref().unwrap().text_style as u32),
            14.0,
            "diff content keeps its installed Mono role despite an unrelated style override"
        );
    }

    #[test]
    fn ignored_lines_controls_render_with_a_source_coordinate_selection() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut model = TextViewModel::from_test_text(
            "first\nsecond\nthird".into(),
            "first\nsecond\nthird".into(),
        );
        assert!(model.select_source_line(DiffSide::Left, 1, false));
        assert!(model.move_source_selection(DiffSide::Left, 1, true));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, 7, 11, &mut model));
            },
        );
        assert!(
            accessible_widget_bounds(
                &output,
                egui::accesskit::Role::Button,
                Some("Ignored lines"),
                None,
            )
            .is_some()
        );
        for label in [
            "Compare selected lines with Clipboard",
            "Previous bookmark",
            "Next bookmark",
            "Toggle bookmark",
        ] {
            assert!(
                accessible_widget_bounds(
                    &output,
                    egui::accesskit::Role::Button,
                    Some(label),
                    None,
                )
                .is_some(),
                "{label} remains reachable"
            );
        }
        assert_eq!(
            model.source_line_selection.unwrap().start..model.source_line_selection.unwrap().end,
            1..3
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn scroll_targets_require_an_exact_projected_measurement_row() {
        let mut measurements = crate::diff::model::RowMeasurementCache::default();
        measurements.rebuild(
            crate::diff::model::RowMeasureKey {
                left_source_id: 1,
                right_source_id: 2,
                left_revision: 3,
                right_revision: 4,
                comparison_revision: 5,
                annotation_revision: 6,
                left_text_width_bits: 7,
                right_text_width_bits: 8,
                font_theme: 9,
                text_style: 10,
                display_config: 11,
                wrap: true,
                projection_mode: 0,
                projection_context: 3,
            },
            vec![2, 5],
            7,
            [20.0, 24.0],
        );

        assert_eq!(
            projected_measurement_for_row(5, &[2, 5], &measurements),
            Some(1)
        );
        assert_eq!(
            projected_measurement_for_row(4, &[2, 5], &measurements),
            None,
            "a missing target cannot be reported as its insertion neighbor"
        );
        assert_eq!(
            projected_measurement_for_row(1, &[2, 5], &measurements),
            None
        );
    }

    #[test]
    fn ignored_lines_raw_identical_status_can_have_an_effective_difference() {
        let compiled = crate::diff::text_compare::CompiledRules::compile(
            &crate::diff::text_compare::TextComparisonRules::default(),
        )
        .unwrap();
        let comparison = crate::diff::text_compare::compare_with_annotations(
            "same\nignored\nend",
            "same\nignored\nend",
            1,
            2,
            0,
            0,
            &compiled,
            100,
            &[],
            &[crate::diff::text_compare::IgnoredLineRange { start: 1, end: 2 }],
            &[],
            1,
        )
        .unwrap();

        assert!(comparison.raw_equal);
        assert!(!comparison.equal_under_rules);
        assert_eq!(
            equality_status_labels(comparison.raw_equal, comparison.equal_under_rules),
            (
                "Raw: identical",
                "Effective: different under current rules and ignored ranges"
            )
        );
        assert_eq!(
            equality_status_labels(false, true),
            (
                "Raw: different",
                "Effective: equal under current rules and ignored ranges"
            )
        );
    }

    #[test]
    fn diff_text_rows_measure_and_paint_with_the_effective_monospace_scale() {
        let left = format!("short row\n{}\nlast", "left token ".repeat(4));
        let right = format!("short row\n{}\nlast", "right token ".repeat(4));

        for wrap in [false, true] {
            let context = egui::Context::default();
            let mut model = TextViewModel::from_test_text(left.clone(), right.clone());
            model.wrap = wrap;
            model.scroll.wrapped = wrap;
            let mut keys = Vec::new();
            let mut heights_at_scale = Vec::new();
            let mut expected_row_heights = Vec::new();

            for overall_scale in [0.75, 1.0, 2.0] {
                let mut theme = crate::settings::ThemeSettings::default();
                theme.preferences.typography_scale = overall_scale;
                crate::gui::theme::publish_committed(&context, &theme);
                render_text_view_frame(&context, &mut model, egui::vec2(420.0, 620.0));
                let style = context.style_of(context.theme());
                expected_row_heights.push(
                    context
                        .fonts_mut(|fonts| {
                            fonts.row_height(&style.text_styles[&egui::TextStyle::Monospace])
                        })
                        .max(crate::diff::model::MIN_VISUAL_LINE_HEIGHT),
                );

                let key = model
                    .row_measurements
                    .key
                    .as_ref()
                    .expect("the comparison has a measured font key");
                let font_size = f32::from_bits(key.text_style as u32);
                assert!(
                    (font_size - 14.0 * overall_scale).abs() < 0.01,
                    "diff uses the installed 14 pt monospace role at scale {overall_scale}, actual {font_size}"
                );
                assert!(
                    model
                        .row_measurements
                        .heights
                        .iter()
                        .all(|height| *height >= crate::diff::model::MIN_VISUAL_LINE_HEIGHT),
                    "all measured rows retain the established minimum height"
                );
                assert!(!model.row_measurements.heights.is_empty());
                keys.push(key.clone());
                heights_at_scale.push(model.row_measurements.heights.clone());
            }

            assert_ne!(
                keys[0], keys[1],
                "75% and 100% fonts have distinct cache keys"
            );
            assert_ne!(
                keys[1], keys[2],
                "100% and 200% fonts have distinct cache keys"
            );
            if wrap {
                let tallest = |heights: &[f32]| heights.iter().copied().fold(0.0, f32::max);
                assert!(tallest(&heights_at_scale[2]) > tallest(&heights_at_scale[1]));
                assert!(tallest(&heights_at_scale[1]) > tallest(&heights_at_scale[0]));
            } else {
                assert!(
                    heights_at_scale[0].iter().all(|height| (*height
                        - crate::diff::model::MIN_VISUAL_LINE_HEIGHT)
                        .abs()
                        < 0.01),
                    "the 75% unwrapped rows retain the historical 20 pt minimum"
                );
                assert!(
                    heights_at_scale[1]
                        .iter()
                        .all(|height| (*height - expected_row_heights[1]).abs() < 0.01),
                    "100% unwrapped rows use the actual monospace galley height with the historical minimum"
                );
                assert!(
                    heights_at_scale[2]
                        .iter()
                        .all(|height| *height > crate::diff::model::MIN_VISUAL_LINE_HEIGHT),
                    "200% unwrapped row height follows the effective font"
                );

                let warm_key = model.row_measurements.key.clone();
                model.row_measurements.set_height(0, 173.0);
                render_text_view_frame(&context, &mut model, egui::vec2(420.0, 620.0));
                assert_eq!(model.row_measurements.key, warm_key);
                assert_eq!(model.row_measurements.heights[0], 173.0);

                let mut theme = crate::settings::ThemeSettings::default();
                theme.preferences.typography_scale = 1.0;
                crate::gui::theme::publish_committed(&context, &theme);
                render_text_view_frame(&context, &mut model, egui::vec2(420.0, 620.0));
                assert_ne!(model.row_measurements.key, warm_key);
                assert!(model.row_measurements.heights[0] < 173.0);

                let base_key = model.row_measurements.key.clone();
                model.row_measurements.set_height(0, 173.0);
                let mut style = (*context.style_of(context.theme())).clone();
                style.spacing.extra_text_line_spacing += 5.0;
                context.set_style_of(context.theme(), style);
                render_text_view_frame(&context, &mut model, egui::vec2(420.0, 620.0));
                assert_eq!(model.row_measurements.key, base_key);
                assert_eq!(
                    model.row_measurements.heights[0], 173.0,
                    "a line-spacing change that does not affect explicit fragment galleys reuses cached rows"
                );

                let spacing_key = model.row_measurements.key.clone();
                let mut style = (*context.style_of(context.theme())).clone();
                style
                    .text_styles
                    .get_mut(&egui::TextStyle::Monospace)
                    .unwrap()
                    .family = egui::FontFamily::Proportional;
                context.set_style_of(context.theme(), style);
                render_text_view_frame(&context, &mut model, egui::vec2(420.0, 620.0));
                assert_ne!(model.row_measurements.key, spacing_key);
                assert!(model.row_measurements.heights[0] < 173.0);

                let family_key = model
                    .row_measurements
                    .key
                    .clone()
                    .expect("the family change follows a measured comparison");
                let logical_widths = (
                    family_key.left_text_width_bits,
                    family_key.right_text_width_bits,
                );
                render_text_view_frame_at_native_scale(
                    &context,
                    &mut model,
                    egui::vec2(420.0, 620.0),
                    1.5,
                );
                assert!((context.pixels_per_point() - 1.5).abs() < 0.001);
                assert_ne!(model.row_measurements.key.as_ref(), Some(&family_key));
                let dpi_key = model
                    .row_measurements
                    .key
                    .clone()
                    .expect("native DPI changes a measured cache identity");
                assert_eq!(
                    (dpi_key.left_text_width_bits, dpi_key.right_text_width_bits,),
                    logical_widths,
                    "native DPI changes the cache identity without changing logical pane widths"
                );
                model.row_measurements.set_height(0, 173.0);
                render_text_view_frame_at_native_scale(
                    &context,
                    &mut model,
                    egui::vec2(420.0, 620.0),
                    1.5,
                );
                assert_eq!(model.row_measurements.key.as_ref(), Some(&dpi_key));
                assert_eq!(model.row_measurements.heights[0], 173.0);
            }
        }
    }

    #[test]
    fn comparison_allocations_are_non_negative_and_bounded() {
        for (width, height) in [
            (400.0, 250.0),
            (900.0, 650.0),
            (1600.0, 1000.0),
            (3.0, -2.0),
        ] {
            let (left, splitter, right) = comparison_dimensions(width, height, 0.5);
            assert!(left >= 0.0 && splitter >= 0.0 && right >= 0.0);
            assert!(left + splitter + right <= width.max(0.0));
            assert!(height.max(0.0) >= 0.0);
        }
    }

    #[test]
    fn pane_widths_are_exact_across_ratios_and_workspace_sizes() {
        for width in [4.0, 640.0, 8192.0] {
            for ratio in [0.15, 0.50, 0.85] {
                let (left, splitter, right) = comparison_dimensions(width, 480.0, ratio);
                assert_eq!(splitter, width.min(8.0));
                assert!((left + splitter + right - width).abs() < f32::EPSILON * width.max(1.0));
                let available = (width - splitter).max(0.0);
                assert!((left - available * ratio).abs() < 0.001);
                assert!((right - available * (1.0 - ratio)).abs() < 0.001);
            }
        }
    }

    #[test]
    fn unwrapped_content_geometry_handles_extreme_lines_and_real_world_tokens() {
        let pane = 300.0;
        let cases = [
            "x".repeat(10),
            "x".repeat(1_000),
            "x".repeat(100_000),
            "x".repeat(500_000),
            format!(r#"{{\"payload\":\"{}\"}}"#, "x".repeat(100_000)),
            format!(
                "https://example.invalid/{}?token={}",
                "path/".repeat(200),
                "a".repeat(4_000)
            ),
            "uninterrupted_token".repeat(1_000),
            "\tlet\tvalue\t=\t".repeat(1_000),
            format!(
                "fn highlighted() {{ {} }}",
                "Some(value).unwrap();".repeat(1_000)
            ),
        ];
        for (index, line) in cases.iter().enumerate() {
            let content = unwrapped_content_width(line, 8.5, 72.0);
            assert!(content >= 72.0 + line.chars().count() as f32 * 8.5);
            if index > 0 {
                assert!(content > pane);
                assert!(horizontal_scroll_range(pane, content) > 0.0);
            }
            assert_eq!(pane, 300.0, "content must not alter its pane");
        }
    }

    #[test]
    fn either_or_both_long_sides_only_expand_their_content_canvas() {
        let pane = 420.0;
        for (left, right) in [
            ("x".repeat(10_000), "short".into()),
            ("short".into(), "x".repeat(10_000)),
            ("x".repeat(10_000), "y".repeat(20_000)),
        ] {
            let left_content = unwrapped_content_width(&left, 8.5, 72.0);
            let right_content = unwrapped_content_width(&right, 8.5, 72.0);
            assert_eq!(pane, 420.0);
            assert_eq!(pane, 420.0);
            assert_eq!(
                horizontal_scroll_range(pane, left_content) > 0.0,
                left.len() > 5
            );
            assert_eq!(
                horizontal_scroll_range(pane, right_content) > 0.0,
                right.len() > 5
            );
        }
    }

    #[test]
    fn minified_json_long_line_is_clipped_to_pane_and_scrolls_horizontally() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        let measured = Rc::new(RefCell::new(None));
        let captured = measured.clone();
        context
            .run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.allocate_ui(egui::vec2(300.0, 200.0), |ui| {
                        let json = format!(r#"{{"payload":"{}"}}"#, "x".repeat(100_000));
                        let output = egui::ScrollArea::both()
                            .id_salt(("long-line-regression", "left", "xy"))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(json).wrap_mode(egui::TextWrapMode::Extend),
                                );
                            });
                        *captured.borrow_mut() = Some((output.inner_rect, output.content_size));
                    });
                });
            })
            .drop_without_applying_deltas();
        let (pane, content) = measured.borrow().expect("scroll area was rendered");
        assert_eq!(screen.size(), egui::vec2(800.0, 600.0));
        assert!(pane.width() <= 300.0, "pane must not widen its parent");
        assert!(
            pane.height() <= 200.0,
            "pane must remain vertically bounded"
        );
        assert!(
            content.x > pane.width(),
            "horizontal scrolling is available"
        );
        assert!(content.y < 50.0, "unwrapped JSON remains one visual line");
    }

    #[test]
    fn memory_text_save_as_cancel_preserves_origin_source_and_dirty_revision() {
        let mut model = TextViewModel::from_test_text("left".into(), "right".into());
        model.left.replace_source("edited\r\nleft".into()).unwrap();
        let source = model.left.source().to_owned();
        let revision = model.left.revision;
        let saved_revision = model.left.saved_revision;
        let error = model.left_error.clone();
        assert!(!save_side_with_picker(&mut model, DiffSide::Left, || None));
        assert_eq!(model.origin, TextCompareOrigin::Memory);
        assert_eq!(model.left_path, None);
        assert_eq!(model.left.source(), source);
        assert_eq!(model.left.revision, revision);
        assert_eq!(model.left.saved_revision, saved_revision);
        assert_eq!(model.left_error, error);
        assert!(model.left.is_dirty());
    }

    #[test]
    fn text_save_shortcut_finishes_before_hunk_copy_shortcut() {
        let directory = tempfile::tempdir().unwrap();
        let left_path = directory.path().join("left.txt");
        let right_path = directory.path().join("right.txt");
        fs::write(&left_path, "before").unwrap();
        fs::write(&right_path, "right").unwrap();
        let mut model = TextViewModel::load(
            &crate::diff::model::TextCompareState {
                left: Some(left_path.clone()),
                right: Some(right_path),
                relative_path: None,
                origin: TextCompareOrigin::FileBacked,
            },
            &Default::default(),
        )
        .unwrap();
        model
            .left
            .replace_source("unsaved before copy".into())
            .unwrap();
        model.schedule_compare();
        wait_for_test_comparison(&mut model);
        let row = model
            .comparison
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| {
                matches!(
                    row.kind,
                    crate::diff::text_compare::DiffRowKind::Modified
                        | crate::diff::text_compare::DiffRowKind::Deleted
                        | crate::diff::text_compare::DiffRowKind::Inserted
                )
            })
            .unwrap();
        model.current_row = Some(row);

        let key_event = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let context = egui::Context::default();
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    focused: true,
                    events: vec![
                        key_event(egui::Key::S, egui::Modifiers::CTRL),
                        key_event(
                            egui::Key::ArrowLeft,
                            egui::Modifiers {
                                alt: true,
                                ctrl: true,
                                ..Default::default()
                            },
                        ),
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| show(ui, 1, 2, &mut model));
                },
            )
            .drop_without_applying_deltas();

        assert_eq!(fs::read(&left_path).unwrap(), b"unsaved before copy");
        assert_eq!(model.left.source(), "right");
        assert!(model.left.is_dirty());
    }
}
