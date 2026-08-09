use crate::diff::model::{DiffView, DiffWorkspace};
use crate::diff::query::DiffOpenPayload;
use eframe::egui;

#[derive(Debug, Default)]
pub struct DiffDialogState {
    pub open: bool,
    pub workspace: DiffWorkspace,
}

impl DiffDialogState {
    pub fn open_payload(&mut self, payload: DiffOpenPayload) -> Result<(), String> {
        self.open = true;
        self.workspace = DiffWorkspace::default();
        self.workspace.open_invocation(payload.left, payload.right)
    }
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Diff")
            .id(egui::Id::new(("diff_window", self.workspace.workspace_id)))
            .open(&mut open)
            .default_size([900.0, 650.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let left = ui.add(
                        egui::TextEdit::singleline(&mut self.workspace.left_visible)
                            .id(egui::Id::new((self.workspace.workspace_id, "left")))
                            .hint_text("Left file or folder"),
                    );
                    if self.workspace.focus_left_requested {
                        left.request_focus();
                        self.workspace.focus_left_requested = false;
                    }
                    if ui.button("Browse…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            self.workspace.left_visible = p.display().to_string();
                        }
                    }
                    ui.label("↔");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.workspace.right_visible)
                            .id(egui::Id::new((self.workspace.workspace_id, "right")))
                            .hint_text("Right file or folder"),
                    );
                    if ui.button("Browse…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            self.workspace.right_visible = p.display().to_string();
                        }
                    }
                    if ui.button("Compare").clicked() {
                        let _ = self.workspace.open_paths(
                            self.workspace.left_visible.clone(),
                            self.workspace.right_visible.clone(),
                        );
                    }
                });
                if let Some(error) = &self.workspace.error {
                    ui.colored_label(egui::Color32::RED, error);
                }
                if self.workspace.navigation_stack.len() > 0 && ui.button("← Back").clicked() {
                    self.workspace.back();
                }
                ui.separator();
                match &self.workspace.current_view.view {
                    DiffView::Start => {
                        ui.label("Choose two files or two folders to compare.");
                    }
                    DiffView::TextCompare(s) => {
                        ui.heading(match s.kind {
                            crate::diff::model::FileComparisonKind::Text => "Text comparison",
                            crate::diff::model::FileComparisonKind::Binary => "Binary files",
                        });
                        ui.label(format!(
                            "Left: {}",
                            s.left
                                .as_ref()
                                .map_or("(missing)".into(), |p| p.display().to_string())
                        ));
                        ui.label(format!(
                            "Right: {}",
                            s.right
                                .as_ref()
                                .map_or("(missing)".into(), |p| p.display().to_string())
                        ));
                    }
                    DiffView::FolderCompare(s) => {
                        ui.heading("Folder comparison");
                        ui.label(format!("{} computed entries", s.content_statuses.len()));
                    }
                }
            });
        self.open = open;
    }
}
