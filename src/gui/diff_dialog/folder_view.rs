//! Folder tree presentation. Filtering is a projection of retained rows and
//! sorting is intentionally performed within each parent rather than flattening.
use crate::diff::model::{DiffStatus, FolderCompareState, FolderDisplayFilter};
use eframe::egui;
use std::path::{Path, PathBuf};

pub(super) fn show(ui: &mut egui::Ui, state: &mut FolderCompareState) {
    ui.heading("Folder comparison");
    ui.horizontal(|ui| {
        ui.label("Show retained results:");
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
    });
    ui.horizontal(|ui| {
        ui.label("Find path (display only):");
        ui.text_edit_singleline(&mut state.path_filter);
    });
    ui.horizontal(|ui| {
        ui.label("Scan include/exclude rules (requires Rescan):");
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut String::new()).hint_text(".git/, target/, *.tmp"),
        );
    });
    ui.label(
        "Status is shown with text and a symbol; metadata matches await content verification.",
    );
    ui.separator();
    let rows = ordered_visible(state);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for p in rows {
            let status = &state.content_statuses[&p];
            let depth = p.components().count().saturating_sub(1);
            ui.horizontal(|ui| {
                ui.add_space(depth as f32 * 14.0);
                let symbol = match status {
                    DiffStatus::Identical => "✓",
                    DiffStatus::Modified => "≠",
                    DiffStatus::LeftOnly => "←",
                    DiffStatus::RightOnly => "→",
                    DiffStatus::Error => "!",
                };
                if ui
                    .selectable_label(
                        state.selected_relative_path.as_ref() == Some(&p),
                        format!("{symbol}  {} — {}", p.display(), status_text(status)),
                    )
                    .clicked()
                {
                    state.selected_relative_path = Some(p.clone());
                    state.scroll_anchor = Some(p)
                }
            });
        }
    });
}
fn status_text(s: &DiffStatus) -> &'static str {
    match s {
        DiffStatus::Identical => "Identical (content checked)",
        DiffStatus::Modified => "Different",
        DiffStatus::LeftOnly => "Left only",
        DiffStatus::RightOnly => "Right only",
        DiffStatus::Error => "Error / unreadable",
    }
}
/// Stable depth-first ordering; only children sharing a parent are sorted.
pub(crate) fn ordered_visible(state: &FolderCompareState) -> Vec<PathBuf> {
    let q = state.path_filter.to_lowercase();
    let mut v: Vec<_> = state
        .content_statuses
        .iter()
        .filter(|(p, s)| {
            p.to_string_lossy().to_lowercase().contains(&q)
                && match state.display_filter {
                    FolderDisplayFilter::All => true,
                    FolderDisplayFilter::Changed => **s != DiffStatus::Identical,
                    FolderDisplayFilter::Identical => **s == DiffStatus::Identical,
                    FolderDisplayFilter::OneSided => {
                        matches!(s, DiffStatus::LeftOnly | DiffStatus::RightOnly)
                    }
                }
        })
        .map(|(p, _)| p.clone())
        .collect();
    v.sort_by(|a, b| {
        let ap = a.parent().unwrap_or(Path::new(""));
        let bp = b.parent().unwrap_or(Path::new(""));
        ap.cmp(bp).then_with(|| a.file_name().cmp(&b.file_name()))
    });
    if state.sort.descending {
        v.reverse()
    }
    v
}
/// Navigation wraps at both ends and follows the current retained projection.
pub(crate) fn adjacent_difference(
    rows: &[PathBuf],
    statuses: &std::collections::BTreeMap<PathBuf, DiffStatus>,
    current: Option<&Path>,
    forward: bool,
) -> Option<PathBuf> {
    let d: Vec<_> = rows
        .iter()
        .filter(|p| statuses.get(*p) != Some(&DiffStatus::Identical))
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
