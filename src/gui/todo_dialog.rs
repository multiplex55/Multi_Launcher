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
    filter: String,
    pub persist_tags: bool,
}

impl TodoDialog {
    pub fn open(&mut self) {
        self.entries = load_todos(TODO_FILE).unwrap_or_default();
        self.open = true;
        self.text.clear();
        self.priority = 0;
        self.tags.clear();
        self.filter.clear();
    }

    fn save(&mut self, app: &mut LauncherApp, focus: bool) {
        if let Err(e) = save_todos(TODO_FILE, &self.entries) {
            app.error = Some(format!("Failed to save todos: {e}"));
        } else {
            app.search();
            if focus {
                app.focus_input();
            }
        }
    }

    pub fn filtered_indices(entries: &[TodoEntry], filter: &str) -> Vec<usize> {
        let filter = filter.trim().to_lowercase();
        entries
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
            .collect()
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut save_now = false;
        egui::Window::new("Todos")
            .open(&mut self.open)
            .resizable(true)
            .default_size((320.0, 200.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                ui.checkbox(&mut self.persist_tags, "Persist Tags");
                egui::Grid::new("todo_add_grid")
                    .num_columns(2)
                    .spacing([4.0, 2.0])
                    .striped(false)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("New Todo").strong());
                        let text_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.text)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        ui.label("Tags");
                        let tags_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.tags)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        ui.label("Priority");
                        let (prio_resp, add_resp) = ui
                            .horizontal(|ui| {
                                (
                                    ui.add(
                                        egui::DragValue::new(&mut self.priority)
                                            .clamp_range(0..=255),
                                    ),
                                    ui.button("Add"),
                                )
                            })
                            .inner;
                        let mut add_clicked = add_resp.clicked();
                        if !add_clicked
                            && (text_resp.has_focus()
                                || tags_resp.has_focus()
                                || prio_resp.has_focus())
                            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            add_clicked = true;
                            let modifiers = ctx.input(|i| i.modifiers);
                            ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                        }

                        if add_clicked {
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
                                if !self.persist_tags {
                                    self.tags.clear();
                                }
                                save_now = true;
                            }
                        }
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    if ui.button("Clear Completed").clicked() {
                        self.entries.retain(|e| !e.done);
                        save_now = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Filter");
                    ui.text_edit_singleline(&mut self.filter);
                });
                let mut remove: Option<usize> = None;
                let area_height = ui.available_height();
                let indices = Self::filtered_indices(&self.entries, &self.filter);
                egui::ScrollArea::both()
                    .max_height(area_height)
                    .show(ui, |ui| {
                        for idx in indices {
                            ui.horizontal(|ui| {
                                let entry = &mut self.entries[idx];
                                if ui.checkbox(&mut entry.done, "").changed() {
                                    save_now = true;
                                }
                                ui.label(entry.text.replace('\n', " "));
                                ui.add(
                                    egui::DragValue::new(&mut entry.priority).clamp_range(0..=255),
                                );
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
        if save_now {
            self.save(app, false);
        }
    }
}
