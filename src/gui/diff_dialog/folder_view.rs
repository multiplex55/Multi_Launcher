//! Folder tree presentation. Every view operation is a pure projection of retained data.
use crate::diff::folder_compare::{
    FolderProjectionRow, FolderStatus, expand_ancestors, project_folder_rows,
};
use crate::diff::model::{FolderCompareState, FolderDisplayFilter};
use eframe::egui;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) enum FolderViewAction {
    #[default]
    Noop,
    OpenChild {
        relative_path: PathBuf,
        left: Option<PathBuf>,
        right: Option<PathBuf>,
    },
    NavigateBack,
    RequestRescan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionGesture {
    Click,
    Toggle,
    Range,
    SelectAll,
    MoveUp,
    MoveDown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DifferenceNavigation {
    First,
    Previous,
    Next,
    Last,
}

pub(crate) fn apply_selection(
    state: &mut FolderCompareState,
    rows: &[PathBuf],
    target: Option<&Path>,
    gesture: SelectionGesture,
) {
    if rows.is_empty() {
        if gesture == SelectionGesture::SelectAll {
            state.selected_paths.clear();
            state.primary_selection = None;
        }
        return;
    }
    let target_index = target.and_then(|p| rows.iter().position(|r| r == p));
    match gesture {
        SelectionGesture::Click => {
            if let Some(i) = target_index {
                state.selected_paths.clear();
                state.selected_paths.insert(rows[i].clone());
                state.primary_selection = Some(rows[i].clone());
            }
        }
        SelectionGesture::Toggle => {
            if let Some(i) = target_index {
                let path = rows[i].clone();
                if !state.selected_paths.remove(&path) {
                    state.selected_paths.insert(path.clone());
                }
                state.primary_selection = state
                    .selected_paths
                    .contains(&path)
                    .then_some(path)
                    .or_else(|| state.selected_paths.iter().next().cloned());
            }
        }
        SelectionGesture::Range => {
            if let Some(end) = target_index {
                let start = state
                    .primary_selection
                    .as_ref()
                    .and_then(|p| rows.iter().position(|r| r == p))
                    .unwrap_or(end);
                state.selected_paths.clear();
                for path in &rows[start.min(end)..=start.max(end)] {
                    state.selected_paths.insert(path.clone());
                }
                state.primary_selection = Some(rows[end].clone());
            }
        }
        SelectionGesture::SelectAll => {
            state.selected_paths = rows.iter().cloned().collect();
            if state
                .primary_selection
                .as_ref()
                .is_none_or(|p| !state.selected_paths.contains(p))
            {
                state.primary_selection = rows.first().cloned();
            }
        }
        SelectionGesture::MoveUp | SelectionGesture::MoveDown => {
            let current = state
                .primary_selection
                .as_ref()
                .and_then(|p| rows.iter().position(|r| r == p));
            let i = match (gesture, current) {
                (SelectionGesture::MoveUp, Some(i)) => i.saturating_sub(1),
                (SelectionGesture::MoveDown, Some(i)) => (i + 1).min(rows.len() - 1),
                (SelectionGesture::MoveUp, None) => rows.len() - 1,
                _ => 0,
            };
            state.selected_paths.clear();
            state.selected_paths.insert(rows[i].clone());
            state.primary_selection = Some(rows[i].clone());
        }
    }
    state.scroll_anchor = state.primary_selection.clone();
}

pub(crate) fn adjacent_difference(
    rows: &[PathBuf],
    state: &FolderCompareState,
    current: Option<&Path>,
    forward: bool,
) -> Option<PathBuf> {
    navigate_difference(
        rows,
        state,
        current,
        if forward {
            DifferenceNavigation::Next
        } else {
            DifferenceNavigation::Previous
        },
    )
}

pub(crate) fn navigate_difference(
    rows: &[PathBuf],
    state: &FolderCompareState,
    current: Option<&Path>,
    nav: DifferenceNavigation,
) -> Option<PathBuf> {
    let differences: Vec<_> = rows
        .iter()
        .filter(|path| {
            state.model.entries.values().any(|e| {
                e.relative_path.as_path() == path.as_path() && e.effective_status.is_different()
            })
        })
        .collect();
    if differences.is_empty() {
        return None;
    }
    let at = current.and_then(|p| differences.iter().position(|r| r.as_path() == p));
    let index = match nav {
        DifferenceNavigation::First => 0,
        DifferenceNavigation::Last => differences.len() - 1,
        DifferenceNavigation::Next => at.map_or(0, |i| (i + 1) % differences.len()),
        DifferenceNavigation::Previous => at.map_or(differences.len() - 1, |i| {
            if i == 0 { differences.len() - 1 } else { i - 1 }
        }),
    };
    Some(differences[index].clone())
}

fn select_navigation_target(state: &mut FolderCompareState, target: PathBuf) {
    expand_ancestors(&state.model, &target, &mut state.expanded_nodes);
    state.selected_paths.clear();
    state.selected_paths.insert(target.clone());
    state.primary_selection = Some(target.clone());
    state.scroll_anchor = Some(target);
}

pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut FolderCompareState,
    runtime: &crate::diff::folder_runtime::FolderRuntime,
) -> FolderViewAction {
    ui.heading("Folder comparison");
    ui.label(format!(
        "{} ↔ {}",
        state.left_root.display(),
        state.right_root.display()
    ));
    let mut action = FolderViewAction::Noop;
    ui.horizontal_wrapped(|ui| {
        for (label, filter) in [
            ("All", FolderDisplayFilter::All),
            ("Differences", FolderDisplayFilter::Differences),
            ("Identical", FolderDisplayFilter::Identical),
            ("Left only", FolderDisplayFilter::LeftOnly),
            ("Right only", FolderDisplayFilter::RightOnly),
            ("Left newer", FolderDisplayFilter::LeftNewer),
            ("Right newer", FolderDisplayFilter::RightNewer),
            ("Errors", FolderDisplayFilter::Errors),
        ] {
            if ui
                .selectable_label(state.display_filter == filter, label)
                .clicked()
            {
                state.display_filter = filter;
            }
        }
        if ui.button("Rescan").clicked() {
            action = FolderViewAction::RequestRescan;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Find path:");
        ui.text_edit_singleline(&mut state.path_filter);
    });
    let scans_done = state.left_scan_complete && state.right_scan_complete;
    ui.label(format!(
        "Scanned entries: left {} ({}) / right {} ({})",
        runtime.left_visited,
        complete(state.left_scan_complete),
        runtime.right_visited,
        complete(state.right_scan_complete)
    ));
    ui.label(format!(
        "Scanning: {}; content checking: {}; compared paths: {}{}",
        if scans_done { "idle" } else { "active" },
        if runtime.is_active() && scans_done {
            "active"
        } else {
            "idle"
        },
        runtime.completed_comparisons,
        if scans_done {
            format!(" / {} total", state.model.entries.len())
        } else {
            String::new()
        }
    ));

    let mut projected = projected_rows(state);
    let mut paths: Vec<_> = projected.iter().map(|r| r.path.clone()).collect();
    let keys = ui.input(|i| {
        (
            i.modifiers.command && i.key_pressed(egui::Key::A),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
        )
    });
    if keys.0 {
        apply_selection(state, &paths, None, SelectionGesture::SelectAll);
    }
    if keys.1 {
        apply_selection(state, &paths, None, SelectionGesture::MoveUp);
    }
    if keys.2 {
        apply_selection(state, &paths, None, SelectionGesture::MoveDown);
    }
    ui.horizontal(|ui| {
        for (label, nav) in [
            ("First", DifferenceNavigation::First),
            ("Previous", DifferenceNavigation::Previous),
            ("Next", DifferenceNavigation::Next),
            ("Last", DifferenceNavigation::Last),
        ] {
            if ui.button(label).clicked() {
                if let Some(target) =
                    navigate_difference(&paths, state, state.primary_selection.as_deref(), nav)
                {
                    select_navigation_target(state, target);
                }
            }
        }
    });
    // Navigation may have expanded a hidden target, so calculate the projection again.
    projected = projected_rows(state);
    paths = projected.iter().map(|r| r.path.clone()).collect();
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("folder-results")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                for heading in [
                    "Left path",
                    "Left size / modified",
                    "Status",
                    "Right path",
                    "Right size / modified",
                    "",
                ] {
                    ui.strong(heading);
                }
                ui.end_row();
                for row in &projected {
                    render_row(ui, state, &paths, row, &mut action);
                }
            });
    });
    action
}

fn render_row(
    ui: &mut egui::Ui,
    state: &mut FolderCompareState,
    paths: &[PathBuf],
    row: &FolderProjectionRow,
    action: &mut FolderViewAction,
) {
    let entry = state
        .model
        .entries
        .values()
        .find(|e| e.relative_path == row.path)
        .expect("projected row")
        .clone();
    let selected = state.selected_paths.contains(&row.path);
    ui.horizontal(|ui| {
        ui.add_space(row.depth as f32 * 14.0);
        if row.has_children {
            let open = state.expanded_nodes.contains(&row.path);
            if ui.small_button(if open { "▼" } else { "▶" }).clicked() {
                if open {
                    state.expanded_nodes.remove(&row.path);
                } else {
                    state.expanded_nodes.insert(row.path.clone());
                }
            }
        } else {
            ui.add_space(22.0);
        }
        let name = entry
            .left
            .as_ref()
            .map(|_| row.path.display().to_string())
            .unwrap_or_default();
        let response = ui.selectable_label(selected, name);
        if response.clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            apply_selection(
                state,
                paths,
                Some(&row.path),
                if modifiers.shift {
                    SelectionGesture::Range
                } else if modifiers.command {
                    SelectionGesture::Toggle
                } else {
                    SelectionGesture::Click
                },
            );
        }
        if state.scroll_anchor.as_ref() == Some(&row.path) {
            response.scroll_to_me(Some(egui::Align::Center));
        }
    });
    ui.label(side_details(entry.left.as_ref()));
    ui.label(status_label(entry.effective_status));
    ui.label(
        entry
            .right
            .as_ref()
            .map(|_| row.path.display().to_string())
            .unwrap_or_default(),
    );
    ui.label(side_details(entry.right.as_ref()));
    if ui.small_button("Open").clicked() {
        *action = FolderViewAction::OpenChild {
            relative_path: row.path.clone(),
            left: entry.left.map(|s| s.path),
            right: entry.right.map(|s| s.path),
        };
    }
    ui.end_row();
}
fn side_details(side: Option<&crate::diff::folder_compare::EntrySide>) -> String {
    let Some(metadata) = side.and_then(|s| s.metadata.as_ref()) else {
        return String::new();
    };
    let modified = metadata
        .modified
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|| "—".into());
    format!("{} B / {}", metadata.size, modified)
}
fn complete(value: bool) -> &'static str {
    if value { "complete" } else { "running" }
}
fn status_label(status: FolderStatus) -> &'static str {
    match status {
        FolderStatus::Identical => "✓ Identical",
        FolderStatus::Different => "≠ Different",
        FolderStatus::LeftOnly => "← Left only",
        FolderStatus::RightOnly => "→ Right only",
        FolderStatus::LeftNewer => "L+ Left newer",
        FolderStatus::RightNewer => "R+ Right newer",
        FolderStatus::PendingContentComparison => "◌ Checking contents",
        FolderStatus::Unreadable | FolderStatus::Error => "! Error",
    }
}
pub(crate) fn projected_rows(state: &FolderCompareState) -> Vec<FolderProjectionRow> {
    project_folder_rows(
        &state.model,
        state.display_filter.clone(),
        &state.path_filter,
        &state.expanded_nodes,
        state.sort.descending,
    )
}
pub(crate) fn ordered_visible(state: &FolderCompareState) -> Vec<PathBuf> {
    projected_rows(state).into_iter().map(|r| r.path).collect()
}

/// A mutation command must call this once and operate on the returned value.
pub(crate) fn selection_snapshot(state: &FolderCompareState) -> BTreeSet<PathBuf> {
    state.selected_paths.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::folder_compare::FolderEntry;

    fn state(items: &[(&str, FolderStatus)]) -> FolderCompareState {
        let mut state = FolderCompareState::default();
        for (path, status) in items {
            state.model.entries.insert(
                (*path).into(),
                FolderEntry {
                    relative_path: PathBuf::from(path),
                    left: None,
                    right: None,
                    metadata_status: *status,
                    effective_status: *status,
                    content_checked: true,
                },
            );
        }
        state
    }
    fn paths(state: &FolderCompareState) -> Vec<PathBuf> {
        ordered_visible(state)
    }

    #[test]
    fn hierarchy_collapse_and_ancestor_expansion() {
        let mut s = state(&[
            ("dir", FolderStatus::Identical),
            ("dir/child", FolderStatus::Different),
            ("dir/child/grand", FolderStatus::LeftOnly),
        ]);
        assert_eq!(paths(&s), vec![PathBuf::from("dir")]);
        s.expanded_nodes.insert("dir".into());
        assert_eq!(
            paths(&s),
            vec![PathBuf::from("dir"), PathBuf::from("dir/child")]
        );
        select_navigation_target(&mut s, "dir/child/grand".into());
        assert!(s.expanded_nodes.contains(Path::new("dir/child")));
        assert_eq!(s.scroll_anchor, Some("dir/child/grand".into()));
        assert_eq!(paths(&s).last(), Some(&PathBuf::from("dir/child/grand")));
    }

    #[test]
    fn every_filter_combines_with_path_filter_without_mutating_model() {
        let items = [
            ("same", FolderStatus::Identical),
            ("different", FolderStatus::Different),
            ("left", FolderStatus::LeftOnly),
            ("right", FolderStatus::RightOnly),
            ("left-new", FolderStatus::LeftNewer),
            ("right-new", FolderStatus::RightNewer),
            ("bad", FolderStatus::Error),
        ];
        let mut s = state(&items);
        let before = s.model.clone();
        for (filter, expected) in [
            (FolderDisplayFilter::All, 7),
            (FolderDisplayFilter::Differences, 6),
            (FolderDisplayFilter::Identical, 1),
            (FolderDisplayFilter::LeftOnly, 1),
            (FolderDisplayFilter::RightOnly, 1),
            (FolderDisplayFilter::LeftNewer, 1),
            (FolderDisplayFilter::RightNewer, 1),
            (FolderDisplayFilter::Errors, 1),
        ] {
            s.display_filter = filter;
            assert_eq!(paths(&s).len(), expected);
        }
        s.display_filter = FolderDisplayFilter::Differences;
        s.path_filter = "new".into();
        assert_eq!(
            paths(&s),
            vec![PathBuf::from("left-new"), PathBuf::from("right-new")]
        );
        assert_eq!(s.model, before); // projections neither alter contents nor revision (scan generation)
    }

    #[test]
    fn selection_gestures_and_keyboard_transitions() {
        let mut s = state(&[]);
        let rows: Vec<PathBuf> = ["a", "b", "c"].into_iter().map(Into::into).collect();
        apply_selection(&mut s, &rows, Some(Path::new("b")), SelectionGesture::Click);
        assert_eq!(s.primary_selection, Some("b".into()));
        apply_selection(
            &mut s,
            &rows,
            Some(Path::new("c")),
            SelectionGesture::Toggle,
        );
        assert_eq!(s.selected_paths.len(), 2);
        apply_selection(&mut s, &rows, Some(Path::new("a")), SelectionGesture::Range);
        assert_eq!(s.selected_paths.len(), 2);
        apply_selection(&mut s, &rows, None, SelectionGesture::SelectAll);
        assert_eq!(s.selected_paths.len(), 3);
        apply_selection(&mut s, &rows, None, SelectionGesture::MoveDown);
        assert_eq!(s.primary_selection, Some("b".into()));
        apply_selection(&mut s, &rows, None, SelectionGesture::MoveUp);
        assert_eq!(s.primary_selection, Some("a".into()));
        assert!(
            s.selected_paths
                .contains(s.primary_selection.as_ref().unwrap())
        );
    }

    #[test]
    fn difference_navigation_endpoints_wrap_and_exclude_hidden_rows() {
        let s = state(&[
            ("a", FolderStatus::Different),
            ("b", FolderStatus::Identical),
            ("c", FolderStatus::LeftOnly),
        ]);
        let visible = vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")];
        assert_eq!(
            navigate_difference(&visible, &s, None, DifferenceNavigation::First),
            Some("a".into())
        );
        assert_eq!(
            navigate_difference(&visible, &s, None, DifferenceNavigation::Last),
            Some("c".into())
        );
        assert_eq!(
            adjacent_difference(&visible, &s, Some(Path::new("c")), true),
            Some("a".into())
        );
        assert_eq!(
            adjacent_difference(&visible, &s, Some(Path::new("a")), false),
            Some("c".into())
        );
        assert_eq!(
            navigate_difference(
                &visible[..2],
                &s,
                Some(Path::new("a")),
                DifferenceNavigation::Next
            ),
            Some("a".into())
        );
        assert_eq!(
            navigate_difference(&[PathBuf::from("b")], &s, None, DifferenceNavigation::Next),
            None
        );
    }
}
