use crate::diff::model::{DiffSide, TextViewModel};
use crate::diff::text_compare::{ChangeImportance, DiffRowKind, NavigationDirection};
use eframe::egui::{self, Color32, RichText};

pub fn show(ui: &mut egui::Ui, workspace: u64, view: u64, model: &mut TextViewModel) {
    model.poll();
    shortcuts(ui, model);
    ui.horizontal_wrapped(|ui| {
        ui.button("Rules…").on_hover_text("Comparison rules");
        ui.label("Differences:");
        let mut ignore = model.rules.ignore_all_whitespace;
        let all_changed = ui.selectable_value(&mut ignore, false, "All").changed();
        let important_changed = ui
            .selectable_value(&mut ignore, true, "Important")
            .changed();
        if all_changed || important_changed {
            model.set_ignore_all_whitespace(ignore);
        }
        if ui.button("⏮ First").clicked() {
            model.navigate(NavigationDirection::First)
        }
        if ui.button("◀ Previous (F7)").clicked() {
            model.navigate(NavigationDirection::Previous)
        }
        if ui.button("Next (F8) ▶").clicked() {
            model.navigate(NavigationDirection::Next)
        }
        if ui.button("Last ⏭").clicked() {
            model.navigate(NavigationDirection::Last)
        }
        ui.checkbox(&mut model.wrap, "Wrap")
            .on_hover_text("Keep both aligned cells at the larger measured row height");
        ui.checkbox(&mut model.syntax, "Syntax");
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
    });
    ui.separator();
    let available = ui.available_width();
    let comparison_height = ui.available_height();
    model.splitter = crate::diff::model::validated_splitter(model.splitter);
    let left_width = (available - 8.0) * model.splitter;
    let pending = model.pending_scroll_row;
    let mut scroll_accepted = pending.is_none();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_width, comparison_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| scroll_accepted &= pane(ui, workspace, view, DiffSide::Left, model, pending),
        );
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(8.0, comparison_height), egui::Sense::drag());
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().widgets.inactive.bg_fill);
        if response.dragged() {
            model.splitter = crate::diff::model::validated_splitter(
                model.splitter + response.drag_delta().x / available,
            );
        }
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), comparison_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| scroll_accepted &= pane(ui, workspace, view, DiffSide::Right, model, pending),
        );
    });
    if scroll_accepted {
        model.pending_scroll_row = None;
    }
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
    let rows = &c.rows;
    let row_height = if model.wrap { 34.0 } else { 20.0 };
    let mut scroll = egui::ScrollArea::vertical().id_source((workspace, view, side, "diff_scroll"));
    if let Some(row_index) = pending {
        scroll = scroll.vertical_scroll_offset(row_index as f32 * row_height);
    }
    // Defer model mutation until after the scroll callback releases its
    // immutable borrow of the retained comparison.
    let mut selected_row = None;
    scroll.show_rows(ui, row_height, rows.len(), |ui, range| {
        for i in range {
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
                        let id = egui::Id::new((
                            workspace,
                            view,
                            side,
                            row.id,
                            c.navigation.row_to_hunk[i],
                        ));
                        ui.push_id(id, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(text).monospace()).wrap(model.wrap),
                            )
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
