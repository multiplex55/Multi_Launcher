use crate::gui::LauncherApp;
use crate::plugins::note::{load_notes, save_notes, Note};
use crate::plugins::todo::{load_todos, TODO_FILE};
use chrono::{DateTime, Local};
use eframe::egui;

fn format_note_timestamp(dt: DateTime<Local>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_note_timestamp_now() -> String {
    format_note_timestamp(Local::now())
}

fn insert_at_char_boundary(text: &str, idx: usize, insert: &str) -> String {
    let char_count = text.chars().count();
    let char_idx = idx.min(char_count);
    let byte_idx = text
        .char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(text.len());

    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(&text[..byte_idx]);
    out.push_str(insert);
    out.push_str(&text[byte_idx..]);
    out
}

#[derive(Default)]
pub struct NotesDialog {
    pub open: bool,
    entries: Vec<Note>,
    index: Vec<String>,
    edit_idx: Option<usize>,
    text: String,
    search: String,
}

impl NotesDialog {
    pub fn open(&mut self) {
        self.entries = load_notes().unwrap_or_default();
        self.rebuild_index();
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
        self.search.clear();
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = load_notes().unwrap_or_default();
        self.rebuild_index();
        if idx < self.entries.len() {
            self.text = self.entries[idx].content.clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    fn rebuild_index(&mut self) {
        self.index = self
            .entries
            .iter()
            .map(|n| {
                let mut txt = n.content.to_lowercase();
                if let Some(a) = &n.alias {
                    txt.push('\n');
                    txt.push_str(&a.to_lowercase());
                }
                txt
            })
            .collect();
    }

    fn save(&mut self, app: &mut LauncherApp) {
        if let Err(e) = save_notes(&self.entries) {
            app.report_error_message("ui operation", format!("Failed to save notes: {e}"));
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
        let mut rebuild_idx = false;
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
                            let output = egui::TextEdit::multiline(&mut self.text)
                                .desired_width(f32::INFINITY)
                                .desired_rows(10)
                                .show(ui);
                            let resp = output.response.clone();
                            let caret_char_idx =
                                output.cursor_range.map(|range| range.primary.ccursor.index);

                            let mut insert_timestamp = false;
                            let mut insert_idx = None;
                            resp.context_menu(|ui| {
                                if ui.button("Insert timestamp").clicked() {
                                    insert_timestamp = true;
                                    insert_idx = caret_char_idx;
                                    ui.close_menu();
                                }
                            });

                            if insert_timestamp {
                                let ts = format_note_timestamp_now();
                                let idx = insert_idx.unwrap_or(usize::MAX);
                                self.text = insert_at_char_boundary(&self.text, idx, &ts);
                            }

                            if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                                let modifiers = ctx.input(|i| i.modifiers);
                                ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                            }
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.text.trim().is_empty() {
                                app.report_error_message("ui operation", "Text required");
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
                                        alias: None,
                                        entity_refs: Vec::new(),
                                    });
                                } else if let Some(e) = self.entries.get_mut(idx) {
                                    e.content = self.text.clone();
                                    e.title =
                                        self.text.lines().next().unwrap_or(&e.title).to_string();
                                }
                                rebuild_idx = true;
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
                    ui.horizontal(|ui| {
                        if ui.button("Add Note").clicked() {
                            self.edit_idx = Some(self.entries.len());
                            self.text.clear();
                        }
                        if ui.button("Unused Assets").clicked() {
                            app.unused_assets_dialog.open();
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                    ui.label("Search");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search).desired_width(f32::INFINITY),
                    );
                    let filter = self.search.to_lowercase();
                    let mut remove: Option<usize> = None;
                    let area_height = ui.available_height();
                    egui::ScrollArea::both()
                        .max_height(area_height)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                if !filter.is_empty() && !self.index[idx].contains(&filter) {
                                    continue;
                                }
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
                                        ui.separator();
                                        ui.label("Link to todo");
                                        for todo in load_todos(TODO_FILE)
                                            .unwrap_or_default()
                                            .into_iter()
                                            .take(8)
                                        {
                                            let todo_id = if todo.id.is_empty() {
                                                todo.text.clone()
                                            } else {
                                                todo.id.clone()
                                            };
                                            if ui
                                                .button(format!("@todo:{todo_id} {}", todo.text))
                                                .clicked()
                                            {
                                                if let Some(target) = self.entries.get_mut(idx_copy)
                                                {
                                                    target.content.push_str(&format!(
                                                        "
@todo:{todo_id}"
                                                    ));
                                                }
                                                save_now = true;
                                                ui.close_menu();
                                            }
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
                        rebuild_idx = true;
                        save_now = true;
                    }
                }
            });
        if rebuild_idx {
            self.rebuild_index();
        }
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
    use super::{format_note_timestamp, insert_at_char_boundary};
    use chrono::{Local, TimeZone};

    #[test]
    fn insert_in_middle() {
        assert_eq!(
            insert_at_char_boundary("hello world", 5, ","),
            "hello, world"
        );
    }

    #[test]
    fn insert_at_start_and_end() {
        assert_eq!(insert_at_char_boundary("world", 0, "hello "), "hello world");
        assert_eq!(insert_at_char_boundary("hello", 5, " world"), "hello world");
    }

    #[test]
    fn insert_out_of_range_falls_back_to_end() {
        assert_eq!(insert_at_char_boundary("hello", 999, "!"), "hello!");
    }

    #[test]
    fn unicode_safe_char_boundary_handling() {
        assert_eq!(insert_at_char_boundary("a😀b", 2, "-"), "a😀-b");
        assert_eq!(insert_at_char_boundary("éß", 1, "-"), "é-ß");
    }

    #[test]
    fn timestamp_format_is_deterministic() {
        let dt = Local
            .with_ymd_and_hms(2024, 1, 2, 3, 4, 5)
            .single()
            .expect("valid local datetime");
        assert_eq!(format_note_timestamp(dt), "2024-01-02 03:04:05");
    }
}
