//! Folder tree presentation. Filtering is a projection of the authoritative model.
use crate::diff::folder_compare::FolderStatus;
use crate::diff::model::{FolderCompareState, FolderDisplayFilter};
use eframe::egui;
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

pub(super) fn show(ui: &mut egui::Ui, state: &mut FolderCompareState) -> FolderViewAction {
    ui.heading("Folder comparison");
    ui.label(format!(
        "{} ↔ {}",
        state.left_root.display(),
        state.right_root.display()
    ));
    let mut action = FolderViewAction::Noop;
    ui.horizontal(|ui| {
        for (label, f) in [
            ("All", FolderDisplayFilter::All),
            ("Differences", FolderDisplayFilter::Changed),
            ("Identical", FolderDisplayFilter::Identical),
            ("Left / Right only", FolderDisplayFilter::OneSided),
        ] {
            if ui
                .selectable_label(state.display_filter == f, label)
                .clicked()
            {
                state.display_filter = f
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
    ui.label(format!(
        "Scanned: left {} / right {}",
        complete(state.left_scan_complete),
        complete(state.right_scan_complete)
    ));
    ui.separator();
    let rows = ordered_visible(state);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in rows {
            let entry = state
                .model
                .entries
                .values()
                .find(|e| e.relative_path == p)
                .expect("visible model row");
            let status = entry.effective_status;
            let left = entry.left.as_ref().map(|s| s.path.clone());
            let right = entry.right.as_ref().map(|s| s.path.clone());
            let depth = p.components().count().saturating_sub(1);
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0);
                let selected = state.selected_paths.contains(&p);
                if ui
                    .selectable_label(
                        selected,
                        format!(
                            "{}  {} — {}",
                            symbol(status),
                            p.display(),
                            status_text(status)
                        ),
                    )
                    .clicked()
                {
                    state.selected_paths.clear();
                    state.selected_paths.insert(p.clone());
                    state.primary_selection = Some(p.clone());
                    state.scroll_anchor = Some(p.clone());
                }
                if ui.small_button("Open").clicked() {
                    action = FolderViewAction::OpenChild {
                        relative_path: p.clone(),
                        left,
                        right,
                    };
                }
            });
        }
    });
    action
}
fn complete(value: bool) -> &'static str {
    if value { "complete" } else { "running" }
}
fn symbol(s: FolderStatus) -> &'static str {
    match s {
        FolderStatus::Identical => "✓",
        FolderStatus::Different => "≠",
        FolderStatus::LeftOnly => "←",
        FolderStatus::RightOnly => "→",
        FolderStatus::LeftNewer => "←+",
        FolderStatus::RightNewer => "+→",
        FolderStatus::PendingContentComparison => "…",
        FolderStatus::Unreadable => "?",
        FolderStatus::Error => "!",
    }
}
fn status_text(s: FolderStatus) -> &'static str {
    match s {
        FolderStatus::Identical => "Identical",
        FolderStatus::Different => "Different",
        FolderStatus::LeftOnly => "Left only",
        FolderStatus::RightOnly => "Right only",
        FolderStatus::LeftNewer => "Left newer",
        FolderStatus::RightNewer => "Right newer",
        FolderStatus::PendingContentComparison => "Pending content comparison",
        FolderStatus::Unreadable => "Unreadable",
        FolderStatus::Error => "Error",
    }
}

pub(crate) fn ordered_visible(state: &FolderCompareState) -> Vec<PathBuf> {
    let q = state.path_filter.to_lowercase();
    let mut v: Vec<_> = state
        .model
        .entries
        .values()
        .filter(|e| {
            e.relative_path
                .to_string_lossy()
                .to_lowercase()
                .contains(&q)
                && match state.display_filter {
                    FolderDisplayFilter::All => true,
                    FolderDisplayFilter::Changed => e.effective_status != FolderStatus::Identical,
                    FolderDisplayFilter::Identical => e.effective_status == FolderStatus::Identical,
                    FolderDisplayFilter::OneSided => matches!(
                        e.effective_status,
                        FolderStatus::LeftOnly | FolderStatus::RightOnly
                    ),
                }
        })
        .map(|e| e.relative_path.clone())
        .collect();
    v.sort_by(|a, b| {
        a.parent()
            .unwrap_or(Path::new(""))
            .cmp(b.parent().unwrap_or(Path::new("")))
            .then_with(|| a.file_name().cmp(&b.file_name()))
    });
    if state.sort.descending {
        v.reverse()
    }
    v
}
pub(crate) fn adjacent_difference(
    rows: &[PathBuf],
    state: &FolderCompareState,
    current: Option<&Path>,
    forward: bool,
) -> Option<PathBuf> {
    let d: Vec<_> = rows
        .iter()
        .filter(|p| {
            state
                .model
                .entries
                .values()
                .find(|e| &e.relative_path == *p)
                .is_some_and(|e| e.effective_status != FolderStatus::Identical)
        })
        .collect();
    if d.is_empty() {
        return None;
    }
    let at = current.and_then(|c| d.iter().position(|p| p.as_path() == c));
    Some(if forward {
        d[(at.map_or(0, |x| x + 1)) % d.len()].clone()
    } else {
        d[at.map_or(d.len() - 1, |x| if x == 0 { d.len() - 1 } else { x - 1 })].clone()
    })
}
