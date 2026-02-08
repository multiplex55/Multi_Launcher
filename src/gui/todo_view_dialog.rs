use crate::common::entity_ref::{EntityKind, EntityRef};
use crate::gui::LauncherApp;
use crate::plugins::note::load_notes;
use crate::plugins::todo::{load_todos, save_todos, TodoEntry, TODO_FILE};
use eframe::egui;

const TODO_VIEW_SIZE: egui::Vec2 = egui::vec2(360.0, 260.0);
const TODO_VIEW_LIST_HEIGHT: f32 = 170.0;

pub fn todo_view_layout_sizes() -> (egui::Vec2, f32) {
    (TODO_VIEW_SIZE, TODO_VIEW_LIST_HEIGHT)
}

pub fn todo_view_window_constraints() -> (egui::Vec2, egui::Vec2) {
    (TODO_VIEW_SIZE, TODO_VIEW_SIZE)
}

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

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = load_todos(TODO_FILE).unwrap_or_default();
        if let Some(e) = self.entries.get(idx) {
            self.editing_idx = Some(idx);
            self.editing_text = e.text.clone();
            self.editing_priority = e.priority;
            self.editing_tags = e.tags.join(", ");
        } else {
            self.editing_idx = None;
        }
        self.open = true;
        self.filter.clear();
        self.sort_by_priority = true;
    }

    fn link_note_to_todo(entry: &mut TodoEntry, note_slug: &str, note_title: &str) {
        let token = format!("@note:{note_slug}");
        if !entry.text.contains(&token) {
            if !entry.text.ends_with(' ') && !entry.text.is_empty() {
                entry.text.push(' ');
            }
            entry.text.push_str(&token);
        }
        if !entry
            .entity_refs
            .iter()
            .any(|r| matches!(r.kind, EntityKind::Note) && r.id == note_slug)
        {
            entry.entity_refs.push(EntityRef::new(
                EntityKind::Note,
                note_slug.to_string(),
                Some(note_title.to_string()),
            ));
        }
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_todos(TODO_FILE, &self.entries) {
            app.set_error(format!("Failed to save todos: {e}"));
        } else {
            app.search();
            app.focus_input();
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let (window_size, list_height) = todo_view_layout_sizes();
        let (min_size, max_size) = todo_view_window_constraints();
        let mut close = false;
        let mut save_now = false;
        egui::Window::new("View Todos")
            .open(&mut self.open)
            .resizable(false)
            .default_size(window_size)
            .min_size(min_size)
            .max_size(max_size)
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
                // Keep horizontal overflow for long todo text without wrapping.
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_height(list_height)
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
                                        ui.label("Link note:");
                                        ui.menu_button("Select existing note", |ui| {
                                            for note in load_notes()
                                                .unwrap_or_default()
                                                .into_iter()
                                                .take(12)
                                            {
                                                if ui
                                                    .button(format!(
                                                        "{} ({})",
                                                        note.title, note.slug
                                                    ))
                                                    .clicked()
                                                {
                                                    if !self
                                                        .editing_text
                                                        .contains(&format!("@note:{}", note.slug))
                                                    {
                                                        if !self.editing_text.is_empty() {
                                                            self.editing_text.push(' ');
                                                        }
                                                        self.editing_text.push_str(&format!(
                                                            "@note:{}",
                                                            note.slug
                                                        ));
                                                    }
                                                    ui.close_menu();
                                                }
                                            }
                                        });
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
                                                for token in self.editing_text.split_whitespace() {
                                                    if let Some(slug) = token.strip_prefix("@note:")
                                                    {
                                                        if !slug.trim().is_empty() {
                                                            Self::link_note_to_todo(
                                                                e,
                                                                slug.trim(),
                                                                slug.trim(),
                                                            );
                                                        }
                                                    }
                                                }
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
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut entry.done, "").changed() {
                                        save_now = true;
                                    }
                                    let resp = ui.add(
                                        egui::Label::new(entry.text.replace('\n', " ")).wrap(false),
                                    );
                                    let idx_copy = idx;
                                    resp.clone().context_menu(|ui: &mut egui::Ui| {
                                        if ui.button("Edit Todo").clicked() {
                                            self.editing_idx = Some(idx_copy);
                                            self.editing_text = entry.text.clone();
                                            self.editing_priority = entry.priority;
                                            self.editing_tags = entry.tags.join(", ");
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        ui.label("Link note");
                                        for note in
                                            load_notes().unwrap_or_default().into_iter().take(10)
                                        {
                                            if ui
                                                .button(format!(
                                                    "@note:{} {}",
                                                    note.slug, note.title
                                                ))
                                                .clicked()
                                            {
                                                Self::link_note_to_todo(
                                                    entry,
                                                    &note.slug,
                                                    &note.title,
                                                );
                                                save_now = true;
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                    ui.add(
                                        egui::Label::new(format!("p{}", entry.priority))
                                            .wrap(false),
                                    );
                                    if !entry.tags.is_empty() {
                                        ui.add(
                                            egui::Label::new(format!(
                                                "#{:?}",
                                                entry.tags.join(", ")
                                            ))
                                            .wrap(false),
                                        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_view_layout_sizes_constants() {
        let (window_size, list_height) = todo_view_layout_sizes();
        let (min_size, max_size) = todo_view_window_constraints();
        assert_eq!(window_size, TODO_VIEW_SIZE);
        assert_eq!(list_height, TODO_VIEW_LIST_HEIGHT);
        assert_eq!(min_size, max_size);
        assert_eq!(window_size, min_size);
        assert!(list_height < window_size.y);
    }
}
