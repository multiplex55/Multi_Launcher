//! Bounded, virtualized viewport for Folder Compare results.
use super::folder_view::{self, FolderViewAction, SelectionGesture};
use crate::diff::folder_compare::{FolderEntry, FolderProjectionRow};
use crate::diff::model::{FolderCompareState, FolderSortState};
use crate::diff::settings::{FolderColumnWidthsV1, FolderSortColumn};
use eframe::egui;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Every result has this height. Keeping it fixed makes the scroll offset map
/// directly to an index and prevents paths or metadata from growing a row.
pub(crate) const FOLDER_ROW_HEIGHT: f32 = 24.0;
/// The header is fixed above (and is therefore not part of) vertical scrolling.
const FOLDER_HEADER_HEIGHT: f32 = 26.0;
const OVERSCAN_ROWS: usize = 4;
const GAP: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct TableLayout {
    widths: [f32; 8],
    total_width: f32,
}

fn table_layout(widths: FolderColumnWidthsV1) -> TableLayout {
    let widths = widths.validated().as_array();
    TableLayout {
        widths,
        total_width: widths.iter().sum::<f32>() + GAP * 7.0,
    }
}

/// Returns the half-open range requiring widgets for a virtual canvas.
pub(crate) fn visible_row_range(
    vertical_offset: f32,
    viewport_height: f32,
    total_rows: usize,
    overscan: usize,
) -> Range<usize> {
    if total_rows == 0 || viewport_height <= 0.0 || !viewport_height.is_finite() {
        return 0..0;
    }
    let offset = if vertical_offset.is_finite() {
        vertical_offset.max(0.0)
    } else {
        0.0
    };
    let first = (offset / FOLDER_ROW_HEIGHT).floor() as usize;
    if first >= total_rows {
        return total_rows..total_rows;
    }
    let visible_end = ((offset + viewport_height) / FOLDER_ROW_HEIGHT).ceil() as usize;
    first.saturating_sub(overscan)..visible_end.saturating_add(overscan).min(total_rows)
}

pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut FolderCompareState,
    paths: &[PathBuf],
    rows: &[FolderProjectionRow],
    operation_active: bool,
    action: &mut FolderViewAction,
) {
    // Allocate exactly the post-control remainder once. Neither scroll canvas
    // dimension can affect the parent Ui or the Diff window.
    let available = ui.available_size().max(egui::Vec2::ZERO);
    let (outer, _) = ui.allocate_exact_size(available, egui::Sense::hover());
    let mut viewport_ui = ui.child_ui(outer, egui::Layout::top_down(egui::Align::Min));
    viewport_ui.set_clip_rect(viewport_ui.clip_rect().intersect(outer));

    state.column_widths = state.column_widths.validated();
    let layout = table_layout(state.column_widths);
    let aligned_model = state
        .model
        .with_alignment_overrides(&state.alignment_overrides);
    let entry_keys: HashMap<PathBuf, String> = aligned_model
        .entries
        .iter()
        .map(|(key, entry)| (entry.relative_path.clone(), key.clone()))
        .collect();
    let anchor_index = state
        .scroll_anchor
        .as_ref()
        .and_then(|anchor| rows.iter().position(|row| &row.path == anchor));
    let data_height = (available.y - FOLDER_HEADER_HEIGHT).max(0.0);

    egui::ScrollArea::horizontal()
        .id_source("folder-results-horizontal")
        .auto_shrink([false, false])
        .show(&mut viewport_ui, |ui| {
            ui.set_min_width(layout.total_width);
            render_header(ui, layout, state, available.x);
            let mut scroll = egui::ScrollArea::vertical()
                .id_source("folder-results-vertical")
                .auto_shrink([false, false])
                .max_height(data_height);
            if let Some(index) = anchor_index {
                let centered = index as f32 * FOLDER_ROW_HEIGHT
                    - (data_height - FOLDER_ROW_HEIGHT).max(0.0) / 2.0;
                scroll = scroll.vertical_scroll_offset(centered.max(0.0));
            }
            scroll.show_viewport(ui, |ui, visible| {
                let total_height = rows.len() as f32 * FOLDER_ROW_HEIGHT;
                let (_, canvas) = ui.allocate_space(egui::vec2(layout.total_width, total_height));
                let range = visible_row_range(
                    visible.min.y - canvas.min.y,
                    visible.height(),
                    rows.len(),
                    OVERSCAN_ROWS,
                );
                for index in range {
                    let row_rect = egui::Rect::from_min_size(
                        canvas.min + egui::vec2(0.0, index as f32 * FOLDER_ROW_HEIGHT),
                        egui::vec2(layout.total_width, FOLDER_ROW_HEIGHT),
                    );
                    render_row(
                        ui,
                        row_rect,
                        layout,
                        state,
                        &aligned_model,
                        paths,
                        &rows[index],
                        entry_keys.get(&rows[index].path),
                        operation_active,
                        action,
                        index,
                    );
                }
            });
        });
}

fn cell_rect(row: egui::Rect, layout: TableLayout, column: usize) -> egui::Rect {
    let x = row.min.x + layout.widths[..column].iter().sum::<f32>() + GAP * column as f32;
    egui::Rect::from_min_size(
        egui::pos2(x, row.min.y),
        egui::vec2(layout.widths[column], row.height()),
    )
}

fn render_header(
    ui: &mut egui::Ui,
    layout: TableLayout,
    state: &mut FolderCompareState,
    viewport: f32,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(layout.total_width, FOLDER_HEADER_HEIGHT),
        egui::Sense::hover(),
    );
    for (column, (title, sort_column)) in [
        ("Left path", Some(FolderSortColumn::Path)),
        ("Left size", Some(FolderSortColumn::LeftSize)),
        ("Left modified", Some(FolderSortColumn::LeftModified)),
        ("Status", Some(FolderSortColumn::Status)),
        ("Right path", Some(FolderSortColumn::Path)),
        ("Right size", Some(FolderSortColumn::RightSize)),
        ("Right modified", Some(FolderSortColumn::RightModified)),
        ("", None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut cell = ui.child_ui(
            cell_rect(rect, layout, column),
            egui::Layout::left_to_right(egui::Align::Center),
        );
        if column == 7 {
            if cell.small_button("Reset Columns").clicked() {
                state.column_widths = FolderColumnWidthsV1::for_viewport(viewport).validated();
            }
            continue;
        }
        let active = sort_column == Some(state.sort.column);
        let indicator = if active {
            if state.sort.descending {
                " ▼"
            } else {
                " ▲"
            }
        } else {
            ""
        };
        if cell
            .add(
                egui::Button::new(egui::RichText::new(format!("{title}{indicator}")).strong())
                    .frame(false),
            )
            .clicked()
        {
            if let Some(column) = sort_column {
                if state.sort.column == column {
                    state.sort.descending = !state.sort.descending;
                } else {
                    state.sort = FolderSortState {
                        column,
                        descending: false,
                    };
                }
            }
        }
        if column < 7 {
            let separator = egui::Rect::from_center_size(
                egui::pos2(cell.max_rect().right() + GAP / 2.0, rect.center().y),
                egui::vec2(8.0, rect.height()),
            );
            let response = ui.interact(
                separator,
                ui.id().with(("folder-column-separator", column)),
                egui::Sense::drag(),
            );
            if response.dragged() {
                state
                    .column_widths
                    .set(column, layout.widths[column] + response.drag_delta().x);
            }
            response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        }
    }
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    layout: TableLayout,
    state: &mut FolderCompareState,
    aligned_model: &crate::diff::folder_compare::FolderModel,
    paths: &[PathBuf],
    row: &FolderProjectionRow,
    entry_key: Option<&String>,
    operation_active: bool,
    action: &mut FolderViewAction,
    index: usize,
) {
    let Some(entry) = entry_key
        .and_then(|key| aligned_model.entries.get(key))
        .cloned()
    else {
        return;
    };
    if index % 2 == 1 {
        ui.painter()
            .rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    }
    render_path_cell(
        ui,
        cell_rect(rect, layout, 0),
        state,
        paths,
        row,
        &entry,
        true,
        operation_active,
        action,
    );
    label_cell(
        ui,
        cell_rect(rect, layout, 1),
        entry
            .left
            .as_ref()
            .and_then(|s| s.metadata.as_ref())
            .map(|m| folder_view::format_size(m.size))
            .unwrap_or_else(|| "—".into()),
    );
    label_cell(
        ui,
        cell_rect(rect, layout, 2),
        entry
            .left
            .as_ref()
            .and_then(|s| s.metadata.as_ref())
            .and_then(|m| m.modified)
            .map(folder_view::format_modified)
            .unwrap_or_else(|| "—".into()),
    );
    label_cell(
        ui,
        cell_rect(rect, layout, 3),
        folder_view::status_label(entry.effective_status),
    );
    render_path_cell(
        ui,
        cell_rect(rect, layout, 4),
        state,
        paths,
        row,
        &entry,
        false,
        operation_active,
        action,
    );
    label_cell(
        ui,
        cell_rect(rect, layout, 5),
        entry
            .right
            .as_ref()
            .and_then(|s| s.metadata.as_ref())
            .map(|m| folder_view::format_size(m.size))
            .unwrap_or_else(|| "—".into()),
    );
    label_cell(
        ui,
        cell_rect(rect, layout, 6),
        entry
            .right
            .as_ref()
            .and_then(|s| s.metadata.as_ref())
            .and_then(|m| m.modified)
            .map(folder_view::format_modified)
            .unwrap_or_else(|| "—".into()),
    );
    let mut cell = ui.child_ui(
        cell_rect(rect, layout, 7),
        egui::Layout::left_to_right(egui::Align::Center),
    );
    if cell
        .small_button(if folder_view::is_directory(&entry) {
            "Toggle"
        } else {
            "Open"
        })
        .clicked()
    {
        *action = folder_view::activate_entry(state, &entry);
    }
}

fn label_cell(ui: &mut egui::Ui, rect: egui::Rect, text: impl Into<egui::WidgetText>) {
    let mut cell = ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center));
    cell.add(egui::Label::new(text).wrap(false));
}

#[allow(clippy::too_many_arguments)]
fn render_path_cell(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    state: &mut FolderCompareState,
    paths: &[PathBuf],
    row: &FolderProjectionRow,
    entry: &FolderEntry,
    left: bool,
    operation_active: bool,
    action: &mut FolderViewAction,
) {
    let mut cell = ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center));
    if left {
        cell.add_space(row.depth as f32 * 14.0);
        if row.has_children {
            let open = state.expanded_nodes.contains(&row.path);
            if cell.small_button(if open { "▼" } else { "▶" }).clicked() {
                if open {
                    state.expanded_nodes.remove(&row.path);
                } else {
                    state.expanded_nodes.insert(row.path.clone());
                }
            }
        } else {
            cell.add_space(22.0);
        }
    }
    let present = if left {
        entry.left.is_some()
    } else {
        entry.right.is_some()
    };
    let mapping = state
        .alignment_overrides
        .iter()
        .find(|m| m.left_relative == row.path);
    let side_relative = mapping
        .map(|m| {
            if left {
                &m.left_relative
            } else {
                &m.right_relative
            }
        })
        .cloned()
        .unwrap_or_else(|| row.path.clone());
    let text = if present {
        format!(
            "{}{}",
            if mapping.is_some() { "⛓ " } else { "" },
            side_relative.display()
        )
    } else if mapping.is_some() {
        format!("⚠ missing: {}", side_relative.display())
    } else {
        String::new()
    };
    cell.style_mut().wrap = Some(false);
    let response = cell.add(egui::SelectableLabel::new(
        state.selected_paths.contains(&row.path),
        text,
    ));
    response.clone().context_menu(|ui| {
        if present && ui.button("Align With…").clicked() {
            state.pending_alignment = Some((left, side_relative.clone()));
            ui.close_menu();
        }
        if let Some(index) = state
            .alignment_overrides
            .iter()
            .position(|m| m.left_relative == row.path)
        {
            if ui.button("Remove alignment override").clicked() {
                state.alignment_overrides.remove(index);
                state.pending_alignment = None;
                ui.close_menu();
            }
        }
        if let Some((source_left, source)) = state.pending_alignment.clone() {
            if source_left != left
                && present
                && ui
                    .button(format!("Confirm alignment with {}", source.display()))
                    .clicked()
            {
                let value = if source_left {
                    crate::diff::folder_compare::FolderAlignmentOverride {
                        left_relative: source,
                        right_relative: side_relative.clone(),
                    }
                } else {
                    crate::diff::folder_compare::FolderAlignmentOverride {
                        left_relative: side_relative.clone(),
                        right_relative: source,
                    }
                };
                let mut candidate = state.alignment_overrides.clone();
                candidate.push(value);
                if crate::diff::folder_compare::validate_alignment_overrides(&candidate).is_ok() {
                    state.alignment_overrides = candidate;
                }
                state.pending_alignment = None;
                ui.close_menu();
            }
        }
        ui.separator();
        folder_view::mutation_menu(ui, state, operation_active, action);
    });
    if response.clicked() {
        let modifiers = cell.input(|i| i.modifiers);
        folder_view::apply_selection(
            state,
            paths,
            Some(Path::new(&row.path)),
            if modifiers.shift {
                SelectionGesture::Range
            } else if modifiers.command {
                SelectionGesture::Toggle
            } else {
                SelectionGesture::Click
            },
        );
    }
    if response.double_clicked() {
        *action = folder_view::activate_entry(state, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_ranges_cover_edges_and_clamp_overscan() {
        assert_eq!(visible_row_range(0.0, 100.0, 0, 4), 0..0);
        assert_eq!(visible_row_range(0.0, 0.0, 100, 4), 0..0);
        assert_eq!(visible_row_range(0.0, 48.0, 100, 4), 0..6);
        assert_eq!(visible_row_range(240.0, 48.0, 100, 4), 6..16);
        assert_eq!(visible_row_range(2352.0, 48.0, 100, 4), 94..100);
        assert_eq!(visible_row_range(0.0, 24.0, 2, 50), 0..2);
    }

    #[test]
    fn ten_thousand_rows_still_render_only_viewport_and_overscan() {
        let range = visible_row_range(5_000.0, 360.0, 10_000, OVERSCAN_ROWS);
        assert!(range.len() <= 15 + OVERSCAN_ROWS * 2 + 1);
        assert!(range.end < 10_000);
    }

    #[test]
    fn taller_viewport_reveals_more_without_changing_model_size() {
        let rows = 10_000;
        let short = visible_row_range(0.0, 360.0, rows, OVERSCAN_ROWS);
        let tall = visible_row_range(0.0, 720.0, rows, OVERSCAN_ROWS);
        assert!(tall.len() > short.len());
        assert_eq!(rows, 10_000);
    }

    #[test]
    fn layout_regressions_are_bounded_and_row_work_is_virtualized() {
        for count in [1, 15, 100, 10_000] {
            for path in ["short", &"x".repeat(260), "a/b/c/d/e/f/g/h/i/j"] {
                let rows: Vec<_> = (0..count)
                    .map(|i| FolderProjectionRow {
                        path: format!("{path}/{i}").into(),
                        depth: i % 10,
                        has_children: false,
                    })
                    .collect();
                for width in [500.0, 900.0, 1_600.0] {
                    let layout = table_layout(FolderColumnWidthsV1::for_viewport(width));
                    assert!(layout.total_width >= width);
                    assert_eq!(layout.widths[0], layout.widths[4]);
                    assert!(layout.widths[0] >= 190.0);
                }
                let rendered = visible_row_range(1_000.0, 360.0, count, OVERSCAN_ROWS).len();
                assert!(rendered <= 24);
            }
        }
        let narrow = table_layout(FolderColumnWidthsV1::for_viewport(100.0));
        let wide = table_layout(FolderColumnWidthsV1::for_viewport(1600.0));
        assert!(narrow.total_width > 100.0); // inner overflow, never parent growth
        assert!(wide.total_width >= 1600.0);
    }

    #[test]
    fn projected_offscreen_anchor_resolves_without_rendering_row() {
        let rows: Vec<_> = (0..10_000)
            .map(|i| FolderProjectionRow {
                path: format!("row-{i}").into(),
                depth: 0,
                has_children: false,
            })
            .collect();
        let anchor: PathBuf = "row-9000".into();
        let index = rows.iter().position(|row| row.path == anchor).unwrap();
        let range = visible_row_range(
            index as f32 * FOLDER_ROW_HEIGHT,
            360.0,
            rows.len(),
            OVERSCAN_ROWS,
        );
        assert!(range.contains(&index));
        assert!(!visible_row_range(0.0, 360.0, rows.len(), OVERSCAN_ROWS).contains(&index));
    }

    #[test]
    fn offscreen_selection_remains_state_without_a_row_widget() {
        let paths: Vec<PathBuf> = (0..10_000).map(|i| format!("row-{i}").into()).collect();
        let mut state = FolderCompareState::default();
        folder_view::apply_selection(
            &mut state,
            &paths,
            Some(Path::new("row-9000")),
            SelectionGesture::Click,
        );
        let widgets = visible_row_range(0.0, 360.0, paths.len(), OVERSCAN_ROWS);
        assert!(!widgets.contains(&9_000));
        assert!(state.selected_paths.contains(Path::new("row-9000")));
        assert_eq!(state.primary_selection, Some("row-9000".into()));
        assert_eq!(state.scroll_anchor, Some("row-9000".into()));
    }
}
