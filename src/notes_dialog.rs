use crate::gui::LauncherApp;
use crate::plugins::notes::{load_notes, save_notes, NoteEntry, QUICK_NOTES_FILE};
use chrono::Local;
use eframe::egui;

#[derive(Default)]
pub struct NotesDialog {
    pub open: bool,
    entries: Vec<NoteEntry>,
    edit_idx: Option<usize>,
    text: String,
}

impl NotesDialog {
    pub fn open(&mut self) {
        self.entries = load_notes(QUICK_NOTES_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = load_notes(QUICK_NOTES_FILE).unwrap_or_default();
        if idx < self.entries.len() {
            self.text = self.entries[idx].text.clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_notes(QUICK_NOTES_FILE, &self.entries) {
            app.error = Some(format!("Failed to save notes: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Quick Notes")
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
                            .desired_width(f32::INFINITY)
                            .desired_rows(10),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.text.trim().is_empty() {
                                app.error = Some("Text required".into());
                            } else {
                                if idx == self.entries.len() {
                                    self.entries.push(NoteEntry {
                                        ts: Local::now().timestamp() as u64,
                                        text: self.text.clone(),
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.text = self.text.clone();
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
                    egui::ScrollArea::both()
                        .max_height(area_height)
                        .show(ui, |ui| {
                        for idx in 0..self.entries.len() {
                            let entry = self.entries[idx].clone();
                            ui.horizontal(|ui| {
                                let resp = ui.label(entry.text.replace('\n', " "));
                                let idx_copy = idx;
                                resp.clone().context_menu(|ui| {
                                    if ui.button("Edit Note").clicked() {
                                        self.edit_idx = Some(idx_copy);
                                        self.text = entry.text.clone();
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove Note").clicked() {
                                        remove = Some(idx_copy);
                                        ui.close_menu();
                                    }
                                });
                                if ui.button("Edit").clicked() {
                                    self.edit_idx = Some(idx);
                                    self.text = entry.text.clone();
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
                    if ui.button("Add Note").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.text.clear();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
    }
}
