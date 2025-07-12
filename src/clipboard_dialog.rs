use crate::gui::LauncherApp;
use crate::plugins::clipboard::{load_history, save_history, CLIPBOARD_FILE};
use eframe::egui;
use std::collections::VecDeque;

#[derive(Default)]
pub struct ClipboardDialog {
    pub open: bool,
    entries: Vec<String>,
    edit_idx: Option<usize>,
    text: String,
}

impl ClipboardDialog {
    pub fn open(&mut self) {
        self.entries = load_history(CLIPBOARD_FILE).unwrap_or_default().into();
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = load_history(CLIPBOARD_FILE).unwrap_or_default().into();
        if idx < self.entries.len() {
            self.text = self.entries[idx].clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        let history: VecDeque<String> = self.entries.clone().into();
        if let Err(e) = save_history(CLIPBOARD_FILE, &history) {
            app.error = Some(format!("Failed to save clipboard history: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Clipboard History")
            .open(&mut self.open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                if let Some(idx) = self.edit_idx {
                    ui.label("Text");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.text)
                            .desired_rows(5)
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.text.trim().is_empty() {
                                app.error = Some("Text required".into());
                            } else {
                                if idx < self.entries.len() {
                                    self.entries[idx] = self.text.clone();
                                }
                                self.edit_idx = None;
                                self.text.clear();
                                save_now = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                        }
                    });
                } else {
                    let mut remove: Option<usize> = None;
                    let area_height = ui.available_height();
                    egui::ScrollArea::both().max_height(area_height).show(ui, |ui| {
                        for idx in 0..self.entries.len() {
                            let entry = self.entries[idx].clone();
                            ui.horizontal(|ui| {
                                let resp = ui.label(entry.replace('\n', " "));
                                let idx_copy = idx;
                                resp.clone().context_menu(|ui| {
                                    if ui.button("Edit Entry").clicked() {
                                        self.edit_idx = Some(idx_copy);
                                        self.text = self.entries[idx_copy].clone();
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove Entry").clicked() {
                                        remove = Some(idx_copy);
                                        ui.close_menu();
                                    }
                                });
                                if ui.button("Edit").clicked() {
                                    self.edit_idx = Some(idx);
                                    self.text = entry.clone();
                                }
                                if ui.button("Remove").clicked() {
                                    remove = Some(idx);
                                }
                            });
                        }
                    });
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        save_now = true;
                    }
                    if ui.button("Close").clicked() { close = true; }
                }
            });
        if save_now { self.save(app); }
        if close { self.open = false; }
    }
}
