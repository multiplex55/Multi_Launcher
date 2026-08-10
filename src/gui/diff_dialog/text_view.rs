use crate::diff::model::{DiffSide, TextViewModel};
use crate::diff::text_compare::{
    ChangeImportance, DiffRowKind, FindScope, NavigationDirection, RowProjectionMode,
};
use eframe::egui::{self, Color32, RichText};

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
    let comparison_height = ui.available_height().max(0.0);
    model.splitter = crate::diff::model::validated_splitter(model.splitter);
    let (left_width, splitter_width, right_width) =
        comparison_dimensions(available, comparison_height, model.splitter);
    let pending = model.pending_scroll_row;
    let mut scroll_accepted = pending.is_none();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_width, comparison_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| scroll_accepted &= pane(ui, workspace, view, DiffSide::Left, model, pending),
        );
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
        ui.allocate_ui_with_layout(
            egui::vec2(right_width, comparison_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| scroll_accepted &= pane(ui, workspace, view, DiffSide::Right, model, pending),
        );
    });
    if scroll_accepted {
        model.pending_scroll_row = None;
    }
    super::rules_dialog::show(ui.ctx(), view, model);
}

fn comparison_dimensions(width: f32, height: f32, splitter: f32) -> (f32, f32, f32) {
    let width = width.max(0.0);
    let _height = height.max(0.0);
    let splitter_width = 8.0_f32.min(width);
    let pane_width = (width - splitter_width).max(0.0);
    let left = pane_width * splitter.clamp(0.0, 1.0);
    (left, splitter_width, (pane_width - left).max(0.0))
}
fn pane(
    ui: &mut egui::Ui,
    workspace: u64,
    view: u64,
    side: DiffSide,
    model: &mut TextViewModel,
    pending: Option<usize>,
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
    let row_height = if model.wrap { 34.0 } else { 20.0 };
    let axis_id = if model.wrap {
        "vertical"
    } else {
        "horizontal_vertical"
    };
    let (stored_x, stored_y) = model.scroll.offsets(side);
    let mut scroll = if model.wrap {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    }
    .id_source((workspace, view, side, axis_id))
    .vertical_scroll_offset(stored_y);
    if !model.wrap {
        scroll = scroll.horizontal_scroll_offset(stored_x);
    }
    if let Some(row_index) = pending {
        let visual = projected
            .binary_search(&row_index)
            .unwrap_or_else(|i| i.min(projected.len().saturating_sub(1)));
        scroll = scroll.vertical_scroll_offset(visual as f32 * row_height);
    }
    // Defer model mutation until after the scroll callback releases its
    // immutable borrow of the retained comparison.
    let mut selected_row = None;
    let output = scroll.show_rows(ui, row_height, projected.len(), |ui, range| {
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
                ui.set_min_height(row_height);
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
                            job.append(&fragment.text, 0.0, format);
                        }
                        let id = egui::Id::new((
                            workspace,
                            view,
                            side,
                            row.id,
                            c.navigation.row_to_hunk[i],
                        ));
                        ui.push_id(id, |ui| {
                            ui.add(egui::Label::new(job).wrap(model.wrap))
                                .on_hover_text(format!("{semantic} line"));
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
    });
    let new_x = output.state.offset.x;
    let new_y = output.state.offset.y;
    let focus_id = egui::Id::new((workspace, view, side, axis_id, "focus"));
    let pane_response = ui.interact(output.inner_rect, focus_id, egui::Sense::click_and_drag());
    if pane_response.clicked() {
        pane_response.request_focus();
    }
    let offset_changed =
        (new_y - stored_y).abs() > 0.25 || (!model.wrap && (new_x - stored_x).abs() > 0.25);
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
        model
            .scroll
            .drive(side, new_x, new_y, row_height, model.wrap);
        let visual = (new_y / row_height.max(1.0)) as usize;
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
    true
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
    fn minified_json_long_line_is_clipped_to_pane_and_scrolls_horizontally() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(screen);
        let measured = Rc::new(RefCell::new(None));
        let captured = measured.clone();
        context.run(input, |ctx| {
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
