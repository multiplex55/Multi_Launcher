use crate::gui::LauncherApp;
use crate::plugins::todo::{load_todos, save_todos, TodoEntry, TODO_FILE};
use eframe::egui;

#[derive(Default)]
pub struct TodoViewDialog {
    pub open: bool,
    entries: Vec<TodoEntry>,
    filter: String,
    sort_by_priority: bool,
}

impl TodoViewDialog {
    pub fn open(&mut self) {
        self.entries = load_todos(TODO_FILE).unwrap_or_default();
        self.open = true;
        self.filter.clear();
        self.sort_by_priority = true;
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
            .default_size((360.0, 240.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.sort_by_priority, "Sort by priority");
                    ui.label("Filter");
                    ui.text_edit_singleline(&mut self.filter);
                });
                ui.separator();
                let filter = self.filter.trim().trim_start_matches('#').to_lowercase();
                let mut indices: Vec<usize> = self
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| {
                        filter.is_empty()
                            || e.tags
                                .iter()
                                .any(|t| t.eq_ignore_ascii_case(&filter))
                    })
                    .map(|(i, _)| i)
                    .collect();
                if self.sort_by_priority {
                    indices.sort_by(|a, b| self.entries[*b].priority.cmp(&self.entries[*a].priority));
                }
                let mut remove: Option<usize> = None;
                let area_height = ui.available_height();
                egui::ScrollArea::both().max_height(area_height).show(ui, |ui| {
                    for idx in indices {
                        let entry = &mut self.entries[idx];
                        ui.horizontal(|ui| {
                            if ui.text_edit_singleline(&mut entry.text).changed() {
                                save_now = true;
                            }
                            if ui
                                .add(egui::DragValue::new(&mut entry.priority).clamp_range(0..=255))
                                .changed()
                            {
                                save_now = true;
                            }
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

