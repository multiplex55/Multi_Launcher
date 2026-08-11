//! Folder tree presentation. Every view operation is a pure projection of retained data.
use crate::diff::folder_compare::{
    EntryKind, FolderEntry, FolderProjectionRow, FolderStatus, expand_ancestors,
    project_folder_rows,
};
use crate::diff::model::{FolderCompareState, FolderDisplayFilter, FolderSortState};
use crate::diff::settings::FolderSortColumn;
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
    RequestMutation(MutationKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MutationKind {
    CopyRight,
    CopyLeft,
    DeleteLeft,
    DeleteRight,
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
                let removed = state.selected_paths.remove(&path);
                if !removed {
                    state.selected_paths.insert(path.clone());
                }
                // Ctrl-click changes membership without moving the range anchor.
                // Only replace the primary selection when it no longer belongs to
                // the selection (or when this is the first selected row).
                if state
                    .primary_selection
                    .as_ref()
                    .is_none_or(|primary| !state.selected_paths.contains(primary))
                {
                    state.primary_selection = if removed {
                        state.selected_paths.iter().next().cloned()
                    } else {
                        Some(path)
                    };
                }
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
    ui.label("Folder comparison");
    let root_row = egui::vec2(ui.available_width().max(0.0), 22.0);
    ui.allocate_ui(root_row, |ui| {
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(format!(
                    "{} ↔ {}",
                    state.left_root.display(),
                    state.right_root.display()
                ));
            });
    });
    let mut action = FolderViewAction::Noop;
    let command_height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), command_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            super::export::folder_menu(ui, state);
            ui.menu_button("View", |ui| {
                for (label, filter) in [
                    ("All", FolderDisplayFilter::All),
                    ("Differences", FolderDisplayFilter::Differences),
                    ("Identical", FolderDisplayFilter::Identical),
                    ("Left only", FolderDisplayFilter::LeftOnly),
                    ("Right only", FolderDisplayFilter::RightOnly),
                    ("Left newer", FolderDisplayFilter::LeftNewer),
                    ("Right newer", FolderDisplayFilter::RightNewer),
                    ("Errors", FolderDisplayFilter::Errors),
                    ("Left changes", FolderDisplayFilter::LeftChanges),
                    ("Right changes", FolderDisplayFilter::RightChanges),
                    ("Orphans", FolderDisplayFilter::Orphans),
                    ("All changes", FolderDisplayFilter::Changes),
                ] {
                    if ui
                        .selectable_label(state.display_filter == filter, label)
                        .clicked()
                    {
                        state.display_filter = filter;
                    }
                }
            });
            if ui.button("Folder Rules…").clicked() {
                super::folder_rules_dialog::open(state);
            }
            let required = state.rescan_required();
            ui.label(format!(
                "Rules: {}",
                if state.applied_scan_rules == Default::default() {
                    "Default"
                } else {
                    "Custom"
                }
            ));
            if required {
                ui.colored_label(egui::Color32::YELLOW, "Rescan required");
            }
            if ui
                .add_enabled(required, egui::Button::new("Rescan"))
                .clicked()
            {
                action = FolderViewAction::RequestRescan;
            }
        },
    );
    if super::folder_rules_dialog::show(ui.ctx(), state) {
        action = FolderViewAction::RequestRescan;
    }
    ui.horizontal(|ui| {
        ui.label("Find path:");
        ui.text_edit_singleline(&mut state.path_filter);
    });
    mutation_buttons(ui, state, runtime, &mut action);
    let scans_done = state.left_scan_complete && state.right_scan_complete;
    ui.label(format!(
        "Scanned entries: left {} ({}) / right {} ({})",
        runtime.left_visited,
        complete(state.left_scan_complete),
        runtime.right_visited,
        complete(state.right_scan_complete)
    ));
    status_summary(ui, state);
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
    super::folder_table::show(
        ui,
        state,
        &paths,
        &projected,
        runtime.mutation_active(),
        &mut action,
    );
    action
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FolderStatusCounts {
    pub total: usize,
    pub same: usize,
    pub different: usize,
    pub left_only: usize,
    pub right_only: usize,
    pub left_newer: usize,
    pub right_newer: usize,
    pub errors: usize,
}
pub(crate) fn status_counts(state: &FolderCompareState) -> FolderStatusCounts {
    let mut c = FolderStatusCounts::default();
    for entry in state.model.entries.values() {
        c.total += 1;
        match entry.effective_status {
            FolderStatus::Identical => c.same += 1,
            FolderStatus::LeftOnly => c.left_only += 1,
            FolderStatus::RightOnly => c.right_only += 1,
            FolderStatus::LeftNewer => c.left_newer += 1,
            FolderStatus::RightNewer => c.right_newer += 1,
            FolderStatus::Unreadable | FolderStatus::Error => c.errors += 1,
            FolderStatus::Different | FolderStatus::PendingContentComparison => {}
        }
        if entry.effective_status.is_different() {
            c.different += 1;
        }
    }
    c
}
fn status_summary(ui: &mut egui::Ui, state: &mut FolderCompareState) {
    let c = status_counts(state);
    ui.horizontal_wrapped(|ui| {
        for (label, count, filter) in [
            ("Total", c.total, FolderDisplayFilter::All),
            ("Same", c.same, FolderDisplayFilter::Identical),
            ("Different", c.different, FolderDisplayFilter::Differences),
            ("Left only", c.left_only, FolderDisplayFilter::LeftOnly),
            ("Right only", c.right_only, FolderDisplayFilter::RightOnly),
            ("Left newer", c.left_newer, FolderDisplayFilter::LeftNewer),
            (
                "Right newer",
                c.right_newer,
                FolderDisplayFilter::RightNewer,
            ),
            ("Errors", c.errors, FolderDisplayFilter::Errors),
        ] {
            if ui.small_button(format!("{label}: {count}")).clicked() {
                state.display_filter = filter;
            }
        }
    });
}

pub(crate) fn exclude_exact(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
pub(crate) fn exclude_extension(path: &Path) -> Option<String> {
    path.extension()
        .filter(|x| !x.is_empty())
        .map(|x| format!("*.{}", x.to_string_lossy()))
}
pub(crate) fn exclude_subtree(path: &Path) -> String {
    format!(
        "{}/",
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
    )
}
fn append_excludes(state: &mut FolderCompareState, patterns: impl IntoIterator<Item = String>) {
    let mut existing: BTreeSet<String> = state
        .draft_rules
        .exclude_rules
        .lines()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
        .collect();
    existing.extend(patterns);
    state.draft_rules.exclude_rules = existing.into_iter().collect::<Vec<_>>().join("\n");
}

fn applicable(state: &FolderCompareState, left: bool) -> bool {
    !state.selected_paths.is_empty()
        && state.selected_paths.iter().any(|path| {
            state.model.entries.values().any(|entry| {
                &entry.relative_path == path
                    && if left {
                        entry.left.is_some()
                    } else {
                        entry.right.is_some()
                    }
            })
        })
}

fn mutation_buttons(
    ui: &mut egui::Ui,
    state: &FolderCompareState,
    runtime: &crate::diff::folder_runtime::FolderRuntime,
    action: &mut FolderViewAction,
) {
    ui.horizontal(|ui| {
        for (label, kind, left) in [
            ("Copy →", MutationKind::CopyRight, true),
            ("← Copy", MutationKind::CopyLeft, false),
            ("Delete Left…", MutationKind::DeleteLeft, true),
            ("Delete Right…", MutationKind::DeleteRight, false),
        ] {
            if ui
                .add_enabled(
                    !runtime.mutation_active() && applicable(state, left),
                    egui::Button::new(label),
                )
                .clicked()
            {
                *action = FolderViewAction::RequestMutation(kind);
            }
        }
    });
}

pub(super) fn mutation_menu(
    ui: &mut egui::Ui,
    state: &mut FolderCompareState,
    operation_active: bool,
    action: &mut FolderViewAction,
) {
    for (label, kind, left) in [
        ("Copy →", MutationKind::CopyRight, true),
        ("← Copy", MutationKind::CopyLeft, false),
        ("Delete Left…", MutationKind::DeleteLeft, true),
        ("Delete Right…", MutationKind::DeleteRight, false),
    ] {
        if ui
            .add_enabled(
                !operation_active && applicable(state, left),
                egui::Button::new(label),
            )
            .clicked()
        {
            *action = FolderViewAction::RequestMutation(kind);
            ui.close_menu();
        }
    }
    ui.separator();
    if ui.button("Exclude exact relative path").clicked() {
        append_excludes(
            state,
            state
                .selected_paths
                .iter()
                .map(|p| exclude_exact(p))
                .collect::<Vec<_>>(),
        );
        ui.close_menu();
    }
    let extensions: Vec<_> = state
        .selected_paths
        .iter()
        .filter_map(|p| exclude_extension(p))
        .collect();
    if ui
        .add_enabled(
            !extensions.is_empty(),
            egui::Button::new("Exclude matching extension"),
        )
        .clicked()
    {
        append_excludes(state, extensions);
        ui.close_menu();
    }
    let folders: Vec<_> = state
        .selected_paths
        .iter()
        .filter(|path| {
            state
                .model
                .entries
                .values()
                .any(|e| &e.relative_path == *path && is_directory(e))
        })
        .map(|p| exclude_subtree(p))
        .collect();
    if ui
        .add_enabled(
            !folders.is_empty(),
            egui::Button::new("Exclude selected folder subtree"),
        )
        .clicked()
    {
        append_excludes(state, folders);
        ui.close_menu();
    }
    if ui.button("Edit filters…").clicked() {
        super::folder_rules_dialog::open(state);
        ui.close_menu();
    }
}

pub(super) fn is_directory(entry: &FolderEntry) -> bool {
    entry
        .left
        .as_ref()
        .or(entry.right.as_ref())
        .and_then(|side| side.metadata.as_ref())
        .is_some_and(|metadata| metadata.kind == EntryKind::Directory)
}

/// Converts activation into a workspace action without deriving operation paths
/// from the relative/display path.
pub(super) fn activate_entry(
    state: &mut FolderCompareState,
    entry: &FolderEntry,
) -> FolderViewAction {
    if is_directory(entry) {
        if !state.expanded_nodes.remove(&entry.relative_path) {
            state.expanded_nodes.insert(entry.relative_path.clone());
        }
        FolderViewAction::Noop
    } else {
        FolderViewAction::OpenChild {
            relative_path: entry.relative_path.clone(),
            left: entry.left.as_ref().map(|side| side.path.clone()),
            right: entry.right.as_ref().map(|side| side.path.clone()),
        }
    }
}
pub(super) fn side_details(side: Option<&crate::diff::folder_compare::EntrySide>) -> String {
    let Some(metadata) = side.and_then(|s| s.metadata.as_ref()) else {
        return "—".into();
    };
    let modified = metadata
        .modified
        .map(|t| {
            chrono::DateTime::<chrono::Local>::from(t)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "—".into());
    format!("{} / {}", format_size(metadata.size), modified)
}
pub(super) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
pub(super) fn format_modified(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
fn complete(value: bool) -> &'static str {
    if value { "complete" } else { "running" }
}
pub(super) fn status_label(status: FolderStatus) -> &'static str {
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
    let aligned = state
        .model
        .with_alignment_overrides(&state.alignment_overrides);
    project_folder_rows(
        &aligned,
        state.display_filter.clone(),
        &state.path_filter,
        &state.expanded_nodes,
        state.sort.clone(),
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
    use crate::diff::folder_compare::{EntryMetadata, EntrySide, FolderEntry};
    use std::time::SystemTime;

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
    fn every_individual_and_combined_status_filter_projects_without_mutating_model() {
        use crate::diff::model::FolderStatusFilter as C;
        let mut s = state(&[
            ("same", FolderStatus::Identical),
            ("lo", FolderStatus::LeftOnly),
            ("ro", FolderStatus::RightOnly),
            ("ln", FolderStatus::LeftNewer),
            ("rn", FolderStatus::RightNewer),
            ("bad", FolderStatus::Error),
        ]);
        let original = s.model.clone();
        for (filter, expected) in [
            (FolderDisplayFilter::Identical, 1),
            (FolderDisplayFilter::Differences, 5),
            (FolderDisplayFilter::LeftOnly, 1),
            (FolderDisplayFilter::RightOnly, 1),
            (FolderDisplayFilter::LeftNewer, 1),
            (FolderDisplayFilter::RightNewer, 1),
            (FolderDisplayFilter::Errors, 1),
            (FolderDisplayFilter::LeftChanges, 2),
            (FolderDisplayFilter::RightChanges, 2),
            (FolderDisplayFilter::Orphans, 2),
            (FolderDisplayFilter::Changes, 5),
        ] {
            s.display_filter = filter;
            assert_eq!(paths(&s).len(), expected);
        }
        s.display_filter =
            FolderDisplayFilter::Combined([C::LeftOnly, C::Errors].into_iter().collect());
        assert_eq!(paths(&s).len(), 2);
        assert_eq!(s.model, original);
    }

    #[test]
    fn exclude_patterns_cover_file_extension_and_folder() {
        assert_eq!(exclude_exact(Path::new("src/main.rs")), "src/main.rs");
        assert_eq!(
            exclude_extension(Path::new("src/main.rs")).as_deref(),
            Some("*.rs")
        );
        assert_eq!(exclude_extension(Path::new("README")), None);
        assert_eq!(exclude_subtree(Path::new("target/debug")), "target/debug/");
    }

    #[test]
    fn summary_counts_complete_model_not_projection() {
        let mut s = state(&[
            ("same", FolderStatus::Identical),
            ("lo", FolderStatus::LeftOnly),
            ("err", FolderStatus::Unreadable),
        ]);
        s.display_filter = FolderDisplayFilter::Identical;
        assert_eq!(paths(&s).len(), 1);
        assert_eq!(
            status_counts(&s),
            FolderStatusCounts {
                total: 3,
                same: 1,
                different: 2,
                left_only: 1,
                errors: 1,
                ..Default::default()
            }
        );
    }

    fn side(path: &str, kind: EntryKind) -> EntrySide {
        EntrySide {
            path: path.into(),
            metadata: Some(EntryMetadata {
                kind,
                size: 0,
                modified: Some(SystemTime::UNIX_EPOCH),
                identity: None,
            }),
            error: None,
        }
    }

    fn entry(
        relative: &str,
        left: Option<&str>,
        right: Option<&str>,
        kind: EntryKind,
    ) -> FolderEntry {
        FolderEntry {
            relative_path: relative.into(),
            left: left.map(|path| side(path, kind)),
            right: right.map(|path| side(path, kind)),
            metadata_status: FolderStatus::Different,
            effective_status: FolderStatus::Different,
            content_checked: false,
        }
    }

    #[test]
    fn child_actions_use_stored_paired_and_one_sided_paths() {
        for (left, right) in [
            (Some("/actual-left/name"), Some("/actual-right/name")),
            (Some("/actual-left/only"), None),
            (None, Some("/actual-right/only")),
        ] {
            let mut state = FolderCompareState::default();
            let entry = entry("display/name", left, right, EntryKind::File);
            assert_eq!(
                activate_entry(&mut state, &entry),
                FolderViewAction::OpenChild {
                    relative_path: "display/name".into(),
                    left: left.map(Into::into),
                    right: right.map(Into::into),
                }
            );
        }
    }

    #[test]
    fn every_raw_sort_column_works_both_directions_with_stable_missing_ties() {
        fn rich(path: &str, size: u64, modified: u64, status: FolderStatus) -> FolderEntry {
            let mut e = entry(path, Some(path), Some(path), EntryKind::File);
            for side in [&mut e.left, &mut e.right] {
                let m = side.as_mut().unwrap().metadata.as_mut().unwrap();
                m.size = size;
                m.modified =
                    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(modified));
            }
            e.effective_status = status;
            e
        }
        let mut s = FolderCompareState::default();
        s.model
            .entries
            .insert("b".into(), rich("b", 20, 200, FolderStatus::RightOnly));
        s.model
            .entries
            .insert("a".into(), rich("a", 10, 100, FolderStatus::Identical));
        s.model.entries.insert(
            "missing".into(),
            FolderEntry {
                relative_path: "missing".into(),
                left: None,
                right: None,
                metadata_status: FolderStatus::Error,
                effective_status: FolderStatus::Error,
                content_checked: false,
            },
        );
        let original = s.model.clone();
        for column in [
            FolderSortColumn::Path,
            FolderSortColumn::Status,
            FolderSortColumn::LeftSize,
            FolderSortColumn::RightSize,
            FolderSortColumn::LeftModified,
            FolderSortColumn::RightModified,
        ] {
            s.sort = FolderSortState {
                column,
                descending: false,
            };
            let asc = paths(&s);
            s.sort.descending = true;
            let desc = paths(&s);
            assert_eq!(asc.len(), 3);
            assert_eq!(desc.len(), 3);
            assert_ne!(asc, desc, "{column:?}");
        }
        assert_eq!(
            s.model, original,
            "sorting must only reorder projection rows"
        );
        s.sort = FolderSortState {
            column: FolderSortColumn::LeftSize,
            descending: false,
        };
        assert_eq!(paths(&s)[0], PathBuf::from("missing"));
    }

    #[test]
    fn directory_activation_only_toggles_expansion() {
        let mut state = FolderCompareState::default();
        let entry = entry(
            "directory",
            Some("/left/directory"),
            None,
            EntryKind::Directory,
        );
        assert_eq!(activate_entry(&mut state, &entry), FolderViewAction::Noop);
        assert!(state.expanded_nodes.contains(Path::new("directory")));
        assert_eq!(activate_entry(&mut state, &entry), FolderViewAction::Noop);
        assert!(!state.expanded_nodes.contains(Path::new("directory")));
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
        assert_eq!(s.primary_selection, Some("b".into()));
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
