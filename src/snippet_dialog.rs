use crate::gui::LauncherApp;
use crate::plugins::snippets::{load_snippets, save_snippets, SnippetEntry, SNIPPETS_FILE};
use eframe::egui;

#[derive(Default)]
pub struct SnippetDialog {
    pub open: bool,
    entries: Vec<SnippetEntry>,
    edit_idx: Option<usize>,
    alias: String,
    text: String,
}

impl SnippetDialog {
    pub fn open(&mut self) {
        self.entries = load_snippets(SNIPPETS_FILE).unwrap_or_default();
        self.open = true;
        self.edit_idx = None;
        self.alias.clear();
        self.text.clear();
    }

    pub fn open_edit(&mut self, alias: &str) {
        self.entries = load_snippets(SNIPPETS_FILE).unwrap_or_default();
        if let Some(pos) = self.entries.iter().position(|e| e.alias == alias) {
            self.edit_idx = Some(pos);
            self.alias = alias.to_string();
            self.text = self.entries[pos].text.clone();
        } else {
            self.edit_idx = Some(self.entries.len());
            self.alias = alias.to_string();
            self.text.clear();
        }
        self.open = true;
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_snippets(SNIPPETS_FILE, &self.entries) {
            app.error = Some(format!("Failed to save snippets: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Snippets")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Alias");
                        ui.text_edit_singleline(&mut self.alias);
                    });
                    ui.label("Text");
                    ui.text_edit_multiline(&mut self.text);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.alias.trim().is_empty() || self.text.trim().is_empty() {
                                app.error = Some("Both fields required".into());
                            } else {
                                if idx == self.entries.len() {
                                    self.entries.push(SnippetEntry { alias: self.alias.clone(), text: self.text.clone() });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.alias = self.alias.clone();
                                    e.text = self.text.clone();
                                }
                                self.edit_idx = None;
                                self.alias.clear();
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
                    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for idx in 0..self.entries.len() {
                            let entry = self.entries[idx].clone();
                            let resp = ui.label(format!("{}: {}", entry.alias, entry.text.replace('\n', " ")));
                            resp.context_menu(|ui| {
                                if ui.button("Edit").clicked() {
                                    self.edit_idx = Some(idx);
                                    self.alias = entry.alias.clone();
                                    self.text = entry.text.clone();
                                    ui.close_menu();
                                }
                                if ui.button("Remove").clicked() {
                                    remove = Some(idx);
                                    ui.close_menu();
                                }
                            });
                        }
                    });
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        save_now = true;
                    }
                    if ui.button("Add Snippet").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.alias.clear();
                        self.text.clear();
                    }
                    if ui.button("Close").clicked() { close = true; }
                }
            });
        if save_now { self.save(app); }
        if close { self.open = false; }
    }
}

