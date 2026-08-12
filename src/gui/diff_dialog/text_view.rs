use crate::diff::model::{DiffSide, TextViewModel};
use crate::diff::text_compare::{
    ChangeImportance, DiffRowKind, FindScope, NavigationDirection, RowProjectionMode,
};
use eframe::egui::{self, Color32, RichText};
use std::hash::{Hash, Hasher};

pub fn show(ui: &mut egui::Ui, workspace: u64, view: u64, model: &mut TextViewModel) {
    model.poll();
    shortcuts(ui, model);
    let command_height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), command_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
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
            {
                if let Err(e) = model.copy_hunk(DiffSide::Left) {
                    model.right_error = Some(e)
                }
            }
            if ui
                .add_enabled(merge, egui::Button::new("← Copy"))
                .on_hover_text(
                    "Copy current difference right-to-left (Ctrl+Alt+Left; Alt+Left is Back)",
                )
                .clicked()
            {
                if let Err(e) = model.copy_hunk(DiffSide::Right) {
                    model.left_error = Some(e)
                }
            }
            if ui.button("Find").clicked() {
                model.find_open = true;
            }
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
                ui.selectable_value(
                    &mut model.projection_mode,
                    RowProjectionMode::All,
                    "All rows",
                );
                ui.selectable_value(
                    &mut model.projection_mode,
                    RowProjectionMode::DifferencesOnly,
                    "Differences only",
                );
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
                    ui.close_menu();
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
                if ui.button("Save Left").clicked() {
                    model.save(DiffSide::Left);
                }
                if ui.button("Save Right").clicked() {
                    model.save(DiffSide::Right);
                }
                if ui.button("Save All Modified").clicked() {
                    if model.left.is_dirty() {
                        model.save(DiffSide::Left);
                    }
                    if model.right.is_dirty() {
                        model.save(DiffSide::Right);
                    }
                }
            });
        },
    );
    if model.find_open {
        ui.horizontal(|ui| {
            ui.label("Find:");
            let changed = ui.text_edit_singleline(&mut model.find_query).changed();
            egui::ComboBox::from_id_source((view, "find-scope"))
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
    egui::ScrollArea::horizontal()
        .max_height(command_height)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
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
            });
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
    let pending = model.pending_scroll_row;
    let mut scroll_accepted = pending.is_none();
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
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                model.request_overview_scroll(pos.y - rect.top(), rect.height());
            }
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
    if scroll_accepted {
        model.pending_scroll_row = None;
    }
    if model.text_details_open {
        text_details(ui, model, details_height);
    }
    super::rules_dialog::show(ui.ctx(), view, model);
}

const TEXT_INSET: f32 = 72.0;

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
    let left_width = (left_pane - TEXT_INSET).max(1.0);
    let right_width = (right_pane - TEXT_INSET).max(1.0);
    let projected = crate::diff::text_compare::row_projection(
        c,
        model.projection_mode,
        model.projection_context,
    );
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    ui.visuals().dark_mode.hash(&mut identity);
    egui::FontId::monospace(14.0).family.hash(&mut identity);
    let key = crate::diff::model::RowMeasureKey {
        left_revision: model.left.revision,
        right_revision: model.right.revision,
        comparison_revision: c.rules_revision,
        left_text_width_bits: left_width.to_bits(),
        right_text_width_bits: right_width.to_bits(),
        font_theme: identity.finish(),
        text_style: 14.0f32.to_bits() as u64,
        display_config: ((model.syntax as u64) << 3)
            | ((model.visible_whitespace as u64) << 2)
            | ((model.visible_line_endings as u64) << 1)
            | model.large_file_tier.syntax_enabled() as u64,
        wrap: model.wrap,
        projection_mode: match model.projection_mode {
            RowProjectionMode::All => 0,
            RowProjectionMode::DifferencesOnly => 1,
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
                    return crate::diff::model::MIN_VISUAL_LINE_HEIGHT;
                }
                let mut job = egui::text::LayoutJob::simple(
                    displayed.replace('\t', "    "),
                    egui::FontId::monospace(14.0),
                    ui.visuals().text_color(),
                    width,
                );
                job.wrap.max_width = width;
                ui.fonts(|fonts| fonts.layout_job(job).size().y)
            };
            measure(DiffSide::Left, left_width).max(measure(DiffSide::Right, right_width))
        })
        .collect();
    model
        .row_measurements
        .rebuild(key, projected, c.rows.len(), heights);
}

/// Allocates the pane before laying out any of its content. The child can grow
/// a scroll canvas, but its viewport and paint clip remain exactly `size`.
fn exact_pane<R>(ui: &mut egui::Ui, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut child = ui.child_ui(rect, egui::Layout::top_down(egui::Align::Min));
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
    let projected = crate::diff::text_compare::row_projection(
        c,
        model.projection_mode,
        model.projection_context,
    );
    let rows = &c.rows;
    let row_height = crate::diff::model::MIN_VISUAL_LINE_HEIGHT;
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
    .id_source((workspace, view, side, axis_id))
    .vertical_scroll_offset(stored_y);
    if !wrapped {
        scroll = scroll.horizontal_scroll_offset(stored_x);
    }
    if let Some(row_index) = pending {
        let visual = projected
            .binary_search(&row_index)
            .unwrap_or_else(|i| i.min(projected.len().saturating_sub(1)));
        let offset = if wrapped {
            model.row_measurements.offset_for_row(visual) as f32
        } else {
            visual as f32 * row_height
        };
        scroll = scroll.vertical_scroll_offset(offset);
    }
    // Defer model mutation until after the scroll callback releases its
    // immutable borrow of the retained comparison.
    let mut selected_row = None;
    let mut alignment_action: Option<(DiffSide, usize)> = None;
    let mut remove_anchor = None;
    let mut render_range = |ui: &mut egui::Ui, range: std::ops::Range<usize>| {
        if !wrapped {
            let canvas_width = source
                .lines()
                .map(|line| unwrapped_content_width(line, 8.5, 72.0))
                .fold(ui.available_width(), f32::max);
            ui.set_min_width(canvas_width);
        }
        for projected_i in range {
            let i = projected[projected_i];
            let row = &rows[i];
            let current = model.current_row == Some(i);
            let bg = match (row.kind, row.importance) {
                (_, ChangeImportance::Unimportant) => Color32::from_rgb(70, 65, 35),
                (DiffRowKind::Equal, _) => Color32::TRANSPARENT,
                (DiffRowKind::Inserted, _) => Color32::from_rgb(28, 70, 42),
                (DiffRowKind::Deleted, _) => Color32::from_rgb(80, 35, 35),
                (DiffRowKind::Modified, _) => Color32::from_rgb(65, 55, 25),
            };
            let frame = egui::Frame::none().fill(if current { bg.gamma_multiply(1.5) } else { bg });
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
                    let line = crate::diff::model::visual_to_source(row, side);
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
                                font_id: egui::FontId::monospace(14.0),
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
                                    font_id: egui::FontId::monospace(14.0),
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
                            let response = ui
                                .add(egui::Label::new(job).wrap(wrapped))
                                .on_hover_text(format!("{semantic} line"));
                            if model.manual_alignments.iter().any(|a| match side {
                                DiffSide::Left => a.left_line == n,
                                DiffSide::Right => a.right_line == n,
                            }) {
                                ui.label(RichText::new("⚓").color(Color32::LIGHT_BLUE));
                            }
                            response.context_menu(|ui| {
                                if let Some((origin_side, _origin_line)) = model.pending_alignment {
                                    if origin_side != side
                                        && ui.button("Preview / confirm alignment").clicked()
                                    {
                                        alignment_action = Some(match side {
                                            DiffSide::Left => (DiffSide::Left, n),
                                            DiffSide::Right => (DiffSide::Right, n),
                                        });
                                        ui.close_menu();
                                    }
                                } else if ui.button("Align With…").clicked() {
                                    model.pending_alignment = Some((side, n));
                                    ui.close_menu();
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
                                {
                                    if ui.button("Remove alignment").clicked() {
                                        remove_anchor = Some(anchor);
                                        ui.close_menu();
                                    }
                                }
                            });
                        });
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
    let offset_changed =
        (new_y - stored_y).abs() > 0.25 || (!wrapped && (new_x - stored_x).abs() > 0.25);
    let directly_interacting = (pane_response.hovered() || pane_response.has_focus())
        && ui.input(|input| {
            input.pointer.any_down()
                || input.raw_scroll_delta != egui::Vec2::ZERO
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
    if let Some((target_side, target_line)) = alignment_action {
        if let Some((origin_side, origin_line)) = model.pending_alignment.take() {
            let anchor = crate::diff::text_compare::ManualTextAlignment {
                left_line: if origin_side == DiffSide::Left {
                    origin_line
                } else {
                    target_line
                },
                right_line: if origin_side == DiffSide::Right {
                    origin_line
                } else {
                    target_line
                },
            };
            if let Err(error) = model.add_manual_alignment(anchor) {
                model.alignment_error = Some(error);
            }
        }
        let _ = target_side;
    }
    if let Some(anchor) = remove_anchor {
        model.remove_manual_alignment(anchor);
    }
    true
}

fn text_details(ui: &mut egui::Ui, model: &mut TextViewModel, height: f32) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.strong("Text Details");
        ui.add(
            egui::DragValue::new(&mut model.text_details_height)
                .clamp_range(72.0..=400.0)
                .suffix(" px"),
        );
    });
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
    ui.allocate_ui(
        egui::vec2(ui.available_width(), (height - 30.0).max(1.0)),
        |ui| {
            ui.columns(2, |columns| {
                for (column, side) in columns.iter_mut().zip([DiffSide::Left, DiffSide::Right]) {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(column, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(value(side)).monospace())
                                    .wrap(false),
                            );
                        });
                }
            });
        },
    );
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
        ui.colored_label(Color32::RED, e);
    }
}
fn shortcuts(ui: &egui::Ui, m: &mut TextViewModel) {
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
            m.save(m.active_side);
        }
        if i.consume_key(
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
            egui::Key::S,
        ) {
            if m.left.is_dirty() {
                m.save(DiffSide::Left);
            }
            if m.right.is_dirty() {
                m.save(DiffSide::Right);
            }
        }
        let copy_modifiers = egui::Modifiers {
            alt: true,
            ctrl: true,
            ..Default::default()
        };
        if i.consume_key(copy_modifiers, egui::Key::ArrowRight) {
            if let Err(error) = m.copy_hunk(DiffSide::Left) {
                m.right_error = Some(error);
            }
        }
        if i.consume_key(copy_modifiers, egui::Key::ArrowLeft) {
            if let Err(error) = m.copy_hunk(DiffSide::Right) {
                m.left_error = Some(error);
            }
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
    use std::{cell::RefCell, rc::Rc};

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
        let _ = context.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(300.0, 200.0), |ui| {
                    let json = format!(r#"{{"payload":"{}"}}"#, "x".repeat(100_000));
                    let output = egui::ScrollArea::both()
                        .id_source(("long-line-regression", "left", "xy"))
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(json).wrap(false));
                        });
                    *captured.borrow_mut() = Some((output.inner_rect, output.content_size));
                });
            });
        });
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
}
