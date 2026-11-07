//! Save-dialog glue only. Report construction lives in `crate::diff`.
use super::effects::{PickerOwner, ViewEffect};
use crate::diff::folder_export::{FolderExportFormat, FolderExportSnapshot};
use crate::diff::model::{FolderCompareState, TextViewModel};
use crate::diff::text_export::{TextExportFormat, TextExportSnapshot};
use eframe::egui;

enum Report {
    Folder(FolderExportSnapshot, FolderExportFormat),
    Text(TextExportSnapshot, TextExportFormat),
}

pub(super) struct ExportRequest {
    extension: &'static str,
    file_stem: &'static str,
    report: Report,
}

impl ExportRequest {
    pub(super) fn save(self, owner: PickerOwner) -> Result<(), String> {
        let picker = owner.dialog().map_err(str::to_owned)?;
        let Some(path) = picker
            .add_filter(self.extension, &[self.extension])
            .set_file_name(format!("{}.{}", self.file_stem, self.extension))
            .save_file()
        else {
            return Ok(());
        };
        let contents = match self.report {
            Report::Folder(snapshot, format) => snapshot.render(format),
            Report::Text(snapshot, format) => snapshot.render(format),
        };
        std::fs::write(&path, contents)
            .map_err(|error| format!("Could not export {}: {error}", path.display()))
    }
}

pub(super) fn folder_menu(
    ui: &mut egui::Ui,
    state: &FolderCompareState,
    effects: &mut Vec<ViewEffect>,
) {
    ui.menu_button("Export…", |ui| {
        for (label, format, extension) in [
            ("CSV", FolderExportFormat::Csv, "csv"),
            ("HTML", FolderExportFormat::Html, "html"),
            ("Plain text", FolderExportFormat::PlainText, "txt"),
        ] {
            ui.menu_button(label, |ui| {
                if ui.button("Complete model").clicked() {
                    let snapshot = FolderExportSnapshot::complete(&state.model);
                    effects.push(ViewEffect::Export(ExportRequest {
                        extension,
                        file_stem: "folder-comparison",
                        report: Report::Folder(snapshot, format),
                    }));
                    ui.close();
                }
                if ui.button("Current filter").clicked() {
                    let snapshot = FolderExportSnapshot::filtered(
                        &state.model,
                        state.display_filter.clone(),
                        &state.path_filter,
                        state.sort.descending,
                    );
                    effects.push(ViewEffect::Export(ExportRequest {
                        extension,
                        file_stem: "folder-comparison",
                        report: Report::Folder(snapshot, format),
                    }));
                    ui.close();
                }
            });
        }
    });
}

pub(super) fn text_menu(ui: &mut egui::Ui, model: &TextViewModel, effects: &mut Vec<ViewEffect>) {
    ui.menu_button("Export…", |ui| {
        for (label, format, extension) in [
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
                effects.push(ViewEffect::Export(ExportRequest {
                    extension,
                    file_stem: "comparison",
                    report: Report::Text(snapshot, format),
                }));
                ui.close();
            }
        }
    });
}
