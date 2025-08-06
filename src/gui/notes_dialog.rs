use crate::gui::LauncherApp;
use crate::plugins::note::{load_notes, save_notes, Note};
use eframe::egui;

#[derive(Default)]
pub struct NotesDialog {
    pub open: bool,
    entries: Vec<Note>,
    edit_idx: Option<usize>,
    text: String,
}

impl NotesDialog {
    pub fn open(&mut self) {
        self.entries = load_notes().unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = load_notes().unwrap_or_default();
        if idx < self.entries.len() {
            self.text = self.entries[idx].content.clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_notes(&self.entries) {
            app.set_error(format!("Failed to save notes: {e}"));
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
                    egui::ScrollArea::vertical()
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            let resp = ui.add(
                                egui::TextEdit::multiline(&mut self.text)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(10),
                            );
                            if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let modifiers = ctx.input(|i| i.modifiers);
                                ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.text.trim().is_empty() {
                                app.set_error("Text required".into());
                            } else {
                                if idx == self.entries.len() {
                                    let title =
                                        self.text.lines().next().unwrap_or("untitled").to_string();
                                    self.entries.push(Note {
                                        title,
                                        path: std::path::PathBuf::new(),
                                        content: self.text.clone(),
                                        tags: Vec::new(),
                                        links: Vec::new(),
                                        slug: String::new(),
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.content = self.text.clone();
                                    e.title =
                                        self.text.lines().next().unwrap_or(&e.title).to_string();
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
                                    let resp = ui.label(entry.content.replace('\n', " "));
                                    let idx_copy = idx;
                                    resp.clone().context_menu(|ui| {
                                        if ui.button("Edit Note").clicked() {
                                            self.edit_idx = Some(idx_copy);
                                            self.text = entry.content.clone();
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove Note").clicked() {
                                            remove = Some(idx_copy);
                                            ui.close_menu();
                                        }
                                    });
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.text = entry.content.clone();
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
