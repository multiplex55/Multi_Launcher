//! Save-dialog glue only; report construction lives in `crate::diff`.
use crate::diff::folder_export::{FolderExportFormat, FolderExportSnapshot};
use crate::diff::model::{FolderCompareState, TextViewModel};
use crate::diff::text_export::{TextExportFormat, TextExportSnapshot};
use eframe::egui;

pub(super) fn folder_menu(ui: &mut egui::Ui, state: &FolderCompareState) {
    ui.menu_button("Export…", |ui| {
        for (label, format, extension) in [
            ("CSV", FolderExportFormat::Csv, "csv"),
            ("HTML", FolderExportFormat::Html, "html"),
            ("Plain text", FolderExportFormat::PlainText, "txt"),
        ] {
            ui.menu_button(label, |ui| {
                if ui.button("Complete model").clicked() {
                    save_folder(state, format, extension, false);
                    ui.close_menu();
                }
                if ui.button("Current filter").clicked() {
                    save_folder(state, format, extension, true);
                    ui.close_menu();
                }
            });
        }
    });
}
fn save_folder(
    state: &FolderCompareState,
    format: FolderExportFormat,
    extension: &str,
    filtered: bool,
) {
    // Clone all scalar metadata before opening the native (blocking) dialog.
    let snapshot = if filtered {
        FolderExportSnapshot::filtered(
            &state.model,
            state.display_filter.clone(),
            &state.path_filter,
            state.sort.descending,
        )
    } else {
        FolderExportSnapshot::complete(&state.model)
    };
    if let Some(path) = rfd::FileDialog::new()
        .add_filter(extension, &[extension])
        .set_file_name(format!("folder-comparison.{extension}"))
        .save_file()
        && let Err(e) = std::fs::write(&path, snapshot.render(format))
    {
        tracing::error!("could not export {}: {e}", path.display());
    }
}
pub(super) fn text_menu(ui: &mut egui::Ui, model: &TextViewModel) {
    ui.menu_button("Export…", |ui| {
        for (label, format, ext) in [
            ("Unified diff", TextExportFormat::UnifiedDiff, "diff"),
            ("Summary", TextExportFormat::PlainTextSummary, "txt"),
            (
                "HTML side-by-side",
                TextExportFormat::HtmlSideBySide,
                "html",
            ),
        ] {
            if ui.button(label).clicked() {
                let left = model
                    .left_path
                    .as_ref()
                    .map_or_else(|| "left".into(), |p| p.to_string_lossy().into_owned());
                let right = model
                    .right_path
                    .as_ref()
                    .map_or_else(|| "right".into(), |p| p.to_string_lossy().into_owned());
                let snapshot = TextExportSnapshot::text(
                    left,
                    right,
                    model.left.source(),
                    model.right.source(),
                );
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(ext, &[ext])
                    .set_file_name(format!("comparison.{ext}"))
                    .save_file()
                    && let Err(e) = std::fs::write(&path, snapshot.render(format))
                {
                    tracing::error!("could not export {}: {e}", path.display());
                }
                ui.close_menu();
            }
        }
    });
}
