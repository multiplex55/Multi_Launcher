use crate::gui::LauncherApp;
use crate::plugins::todo::{load_todos, save_todos, TodoEntry, TODO_FILE};
use eframe::egui;

#[derive(Default)]
pub struct TodoDialog {
    pub open: bool,
    entries: Vec<TodoEntry>,
    text: String,
    priority: u8,
    tags: String,
}

impl TodoDialog {
    pub fn open(&mut self) {
        self.entries = load_todos(TODO_FILE).unwrap_or_default();
        self.open = true;
        self.text.clear();
        self.priority = 0;
        self.tags.clear();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_todos(TODO_FILE, &self.entries) {
            app.error = Some(format!("Failed to save todos: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open { return; }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("Todos")
            .open(&mut self.open)
            .resizable(true)
            .default_size((360.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("New");
                    ui.text_edit_singleline(&mut self.text);
                    ui.label("Priority");
                    if ui
                        .add(egui::DragValue::new(&mut self.priority).clamp_range(0..=255))
                        .changed()
                    {
                        // no action
                    }
                    ui.label("Tags");
                    ui.text_edit_singleline(&mut self.tags);
                    if ui.button("Add").clicked() {
                        if !self.text.trim().is_empty() {
                            let tag_list: Vec<String> = self
                                .tags
                                .split(',')
                                .map(|t| t.trim())
                                .filter(|t| !t.is_empty())
                                .map(|t| t.to_string())
                                .collect();
                            self.entries.push(TodoEntry {
                                text: self.text.clone(),
                                done: false,
                                priority: self.priority,
                                tags: tag_list,
                            });
                            self.text.clear();
                            self.priority = 0;
                            self.tags.clear();
                            save_now = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Clear Completed").clicked() {
                        self.entries.retain(|e| !e.done);
                        save_now = true;
                    }
                    if ui.button("Close").clicked() { close = true; }
                });
                ui.separator();
                let mut remove: Option<usize> = None;
                let area_height = ui.available_height();
                egui::ScrollArea::both().max_height(area_height).show(ui, |ui| {
                    for idx in 0..self.entries.len() {
                        ui.horizontal(|ui| {
                            let entry = &mut self.entries[idx];
                            if ui.checkbox(&mut entry.done, "").changed() {
                                save_now = true;
                            }
                            ui.label(entry.text.replace('\n', " "));
                            ui.add(egui::DragValue::new(&mut entry.priority).clamp_range(0..=255));
                            let mut tag_str = entry.tags.join(", ");
                            if ui.text_edit_singleline(&mut tag_str).changed() {
                                entry.tags = tag_str
                                    .split(',')
                                    .map(|t| t.trim())
                                    .filter(|t| !t.is_empty())
                                    .map(|t| t.to_string())
                                    .collect();
                                save_now = true;
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
            });
        if save_now { self.save(app); }
        if close { self.open = false; }
    }
}
