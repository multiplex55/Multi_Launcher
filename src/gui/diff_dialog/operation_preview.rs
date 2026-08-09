//! Non-persisted confirmation state for folder mutations.
use crate::diff::file_ops::{CopyPlan, DeletePlan, EntryType};
use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PreviewPlan {
    Copy(CopyPlan),
    Delete(DeletePlan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OperationPreview {
    pub view_id: u64,
    pub plan: PreviewPlan,
    pub planning_error: Option<String>,
}

impl OperationPreview {
    pub fn show(&mut self, ctx: &egui::Context, operation_active: bool) -> PreviewResponse {
        let mut response = PreviewResponse::Keep;
        egui::Window::new("Confirm folder operation")
            .id(egui::Id::new(("folder-operation-preview", self.view_id)))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(error) = &self.planning_error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                let executable = match &self.plan {
                    PreviewPlan::Copy(plan) => {
                        ui.heading(match plan.direction {
                            crate::diff::file_ops::CopyDirection::LeftToRight => "Copy →",
                            crate::diff::file_ops::CopyDirection::RightToLeft => "← Copy",
                        });
                        ui.label(format!(
                            "Files: {}  Directories: {}  Overwrites: {}  Skipped: {}  Conflicts: {}  Validation errors: {}",
                            plan.totals.files_copied, plan.totals.directories_created,
                            plan.totals.overwrites, plan.totals.skips, plan.totals.conflicts,
                            plan.totals.errors
                        ));
                        details(ui, "Overwrites", plan.copies.iter().filter(|x| x.overwrite)
                            .map(|x| (x.relative.display().to_string(), "existing destination will be replaced".to_owned())));
                        details(ui, "Conflicts", plan.conflicts.iter()
                            .map(|x| (x.relative.display().to_string(), x.message.clone())));
                        details(ui, "Skipped", plan.skipped.iter()
                            .map(|x| (x.relative.display().to_string(), x.reason.clone())));
                        details(ui, "Validation errors", plan.errors.iter().map(|x| (
                            x.relative.as_ref().map_or_else(|| "root".into(), |p| p.display().to_string()),
                            x.message.clone(),
                        )));
                        !plan.has_fatal_errors()
                    }
                    PreviewPlan::Delete(plan) => {
                        ui.heading(format!("Delete {} (Recycle Bin)", plan.side));
                        let files = plan.items.iter().filter(|x| x.expected.kind == EntryType::File).count();
                        let directories = plan.items.len() - files;
                        ui.label(format!("Files: {files}  Directories: {directories}"));
                        details(ui, "Affected paths", plan.items.iter().map(|x| (
                            x.relative.display().to_string(), "move to Recycle Bin".into(),
                        )));
                        details(ui, "Validation errors", plan.errors.iter().map(|x| (
                            x.relative.as_ref().map_or_else(|| "root".into(), |p| p.display().to_string()),
                            x.message.clone(),
                        )));
                        plan.errors.iter().all(|x| !x.fatal)
                    }
                } && self.planning_error.is_none();
                ui.horizontal(|ui| {
                    if ui.add_enabled(executable && !operation_active, egui::Button::new("Execute")).clicked() {
                        response = PreviewResponse::Execute;
                    }
                    if ui.button("Cancel").clicked() { response = PreviewResponse::Cancel; }
                });
            });
        response
    }
}

fn details(ui: &mut egui::Ui, title: &str, items: impl Iterator<Item = (String, String)>) {
    let items: Vec<_> = items.collect();
    if !items.is_empty() {
        ui.collapsing(format!("{title} ({})", items.len()), |ui| {
            for (path, explanation) in items {
                ui.label(format!("{} — {}", path, explanation));
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewResponse {
    Keep,
    Execute,
    Cancel,
}
