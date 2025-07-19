use crate::gui::LauncherApp;
use crate::plugins::todo::{load_todos, save_todos, TodoEntry, TODO_FILE};
use eframe::egui;

#[derive(Default)]
pub struct TodoViewDialog {
    pub open: bool,
    entries: Vec<TodoEntry>,
    filter: String,
    sort_by_priority: bool,
    editing_idx: Option<usize>,
    editing_text: String,
    editing_priority: u8,
    editing_tags: String,
}

impl TodoViewDialog {
    pub fn open(&mut self) {
        self.entries = load_todos(TODO_FILE).unwrap_or_default();
        self.open = true;
        self.filter.clear();
        self.sort_by_priority = true;
        self.editing_idx = None;
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
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("View Todos")
            .open(&mut self.open)
            .resizable(true)
            .default_size((320.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.sort_by_priority, "Sort by priority");
                    ui.label("Filter");
                    ui.text_edit_singleline(&mut self.filter);
                });
                ui.separator();
                let filter = self.filter.trim().to_lowercase();
                let mut indices: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        if filter.is_empty() {
                            true
                        } else if filter.starts_with('#') {
                            let tag = filter.trim_start_matches('#');
                            e.tags.iter().any(|t| t.to_lowercase().contains(tag))
                        } else {
                            e.text.to_lowercase().contains(&filter)
                                || e.tags.iter().any(|t| t.to_lowercase().contains(&filter))
                        }
                    })
                    .map(|(i, _)| i)
                    .collect();
                if self.sort_by_priority {
                    indices
                        .sort_by(|a, b| self.entries[*b].priority.cmp(&self.entries[*a].priority));
                }
                let area_height = ui.available_height();
                egui::ScrollArea::both()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        for idx in indices {
                            if Some(idx) == self.editing_idx {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Text:");
                                        ui.text_edit_singleline(&mut self.editing_text);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Priority:");
                                        ui.add(
                                            egui::DragValue::new(&mut self.editing_priority)
                                                .clamp_range(0..=255),
                                        );
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Tags:");
                                        ui.text_edit_singleline(&mut self.editing_tags);
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.button("Save").clicked() {
                                            let tags: Vec<String> = self
                                                .editing_tags
                                                .split(',')
                                                .map(|t| t.trim())
                                                .filter(|t| !t.is_empty())
                                                .map(|t| t.to_string())
                                                .collect();
                                            if let Some(e) = self.entries.get_mut(idx) {
                                                e.text = self.editing_text.clone();
                                                e.priority = self.editing_priority;
                                                e.tags = tags;
                                            }
                                            self.editing_idx = None;
                                            save_now = true;
                                        }
                                        if ui.button("Cancel").clicked() {
                                            self.editing_idx = None;
                                        }
                                    });
                                });
                            } else {
                                let entry = &mut self.entries[idx];
                                ui.horizontal_wrapped(|ui| {
                                    if ui.checkbox(&mut entry.done, "").changed() {
                                        save_now = true;
                                    }
                                    ui.label(entry.text.replace('\n', " "));
                                    ui.label(format!("p{}", entry.priority));
                                    if !entry.tags.is_empty() {
                                        ui.label(format!("#{:?}", entry.tags.join(", ")));
                                    }
                                    if ui.button("Edit").clicked() {
                                        self.editing_idx = Some(idx);
                                        self.editing_text = entry.text.clone();
                                        self.editing_priority = entry.priority;
                                        self.editing_tags = entry.tags.join(", ");
                                    }
                                });
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
    }
}
