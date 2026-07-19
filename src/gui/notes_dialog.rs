use crate::actions::Action;
use crate::gui::{ActivationSource, LauncherApp};
use crate::plugins::note::{
    Note, delete_template, get_template, list_templates, load_notes, note_backlinks,
    note_cache_snapshot, reload_templates, save_notes, save_template, template_path,
    validate_template_name,
};
use crate::plugins::todo::{TODO_FILE, load_todos};
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

fn cached_notes_or_load() -> Vec<Note> {
    let snapshot = note_cache_snapshot();
    if snapshot.is_empty() {
        load_notes().unwrap_or_default()
    } else {
        snapshot
    }
}

fn display_title(note: &Note) -> &str {
    note.alias.as_deref().unwrap_or(&note.title)
}

fn short_preview(content: &str) -> String {
    let preview = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("# ") && !trimmed.starts_with("Alias:")
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if preview.chars().count() > 120 {
        format!("{}…", preview.chars().take(120).collect::<String>())
    } else {
        preview
    }
}

fn checkbox_count(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- [ ] ")
                || trimmed.starts_with("- [x] ")
                || trimmed.starts_with("- [X] ")
        })
        .count()
}

fn note_action(label: impl Into<String>, action: impl Into<String>) -> Action {
    Action {
        label: label.into(),
        desc: "Note".into(),
        action: action.into(),
        args: None,
    }
}

#[derive(Default)]
pub struct NotesDialog {
    pub open: bool,
    entries: Vec<Note>,
    index: Vec<String>,
    edit_idx: Option<usize>,
    text: String,
    search: String,
    template_manager: TemplateManagerState,
}

#[derive(Default)]
struct TemplateManagerState {
    open: bool,
    templates: Vec<String>,
    selected: Option<String>,
    name: String,
    content: String,
    pending_delete: Option<String>,
}

impl TemplateManagerState {
    fn open(&mut self) {
        self.open = true;
        self.refresh();
    }

    fn refresh(&mut self) {
        let _ = reload_templates();
        self.templates = list_templates().unwrap_or_default();
        if let Some(selected) = self.selected.clone() {
            if self.templates.iter().any(|name| name == &selected) {
                self.load_for_edit(&selected);
            } else {
                self.clear_editor();
            }
        }
    }

    fn clear_editor(&mut self) {
        self.selected = None;
        self.name.clear();
        self.content.clear();
    }

    fn load_for_edit(&mut self, name: &str) {
        self.selected = Some(name.to_string());
        self.name = name.to_string();
        self.content = get_template(name).unwrap_or_default();
    }

    fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }

        let mut refresh = false;
        let mut open = self.open;
        egui::Window::new("Note Templates")
            .open(&mut open)
            .resizable(true)
            .default_size((520.0, 360.0))
            .min_width(320.0)
            .min_height(220.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("New Template").clicked() {
                        self.clear_editor();
                    }
                    if ui.button("Refresh").clicked() {
                        refresh = true;
                    }
                });
                ui.separator();
                ui.columns(2, |columns| {
                    columns[0].heading("Templates");
                    egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                        for name in self.templates.clone() {
                            ui.horizontal(|ui| {
                                let selected = self.selected.as_deref() == Some(name.as_str());
                                if ui.selectable_label(selected, &name).clicked() {
                                    self.load_for_edit(&name);
                                }
                                if ui.small_button("Open").clicked() {
                                    match template_path(&name)
                                        .and_then(|path| open::that(path).map_err(Into::into))
                                    {
                                        Ok(()) => {}
                                        Err(e) => app.report_error_message(
                                            "ui operation",
                                            format!("Failed to open template: {e}"),
                                        ),
                                    }
                                }
                                if ui.small_button("Delete").clicked() {
                                    self.pending_delete = Some(name.clone());
                                }
                            });
                        }
                    });

                    columns[1].heading(if self.selected.is_some() {
                        "Edit Template"
                    } else {
                        "Create Template"
                    });
                    columns[1].label("Name");
                    columns[1].add(
                        egui::TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY),
                    );
                    columns[1].label("Content");
                    columns[1].add(
                        egui::TextEdit::multiline(&mut self.content)
                            .desired_width(f32::INFINITY)
                            .desired_rows(10),
                    );
                    columns[1].horizontal(|ui| {
                        if ui.button("Save Template").clicked() {
                            match validate_template_name(&self.name).and_then(|name| {
                                save_template(name, &self.content).map(|_| name.to_string())
                            }) {
                                Ok(saved_name) => {
                                    self.selected = Some(saved_name);
                                    refresh = true;
                                    app.search();
                                }
                                Err(e) => app.report_error_message(
                                    "ui operation",
                                    format!("Failed to save template: {e}"),
                                ),
                            }
                        }
                        if ui.button("Clear").clicked() {
                            self.clear_editor();
                        }
                    });
                });
            });
        self.open = open;

        if let Some(name) = self.pending_delete.clone() {
            let mut confirm_open = true;
            egui::Window::new("Delete Template?")
                .open(&mut confirm_open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Delete template '{name}'?"));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            match delete_template(&name) {
                                Ok(()) => {
                                    if self.selected.as_deref() == Some(name.as_str()) {
                                        self.clear_editor();
                                    }
                                    self.pending_delete = None;
                                    refresh = true;
                                    app.search();
                                }
                                Err(e) => app.report_error_message(
                                    "ui operation",
                                    format!("Failed to delete template: {e}"),
                                ),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_delete = None;
                        }
                    });
                });
            if !confirm_open {
                self.pending_delete = None;
            }
        }

        if refresh {
            self.refresh();
        }
    }
}

impl NotesDialog {
    pub fn open(&mut self) {
        self.entries = cached_notes_or_load();
        self.rebuild_index();
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
        self.search.clear();
        if self.template_manager.open {
            self.template_manager.refresh();
        }
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.entries = cached_notes_or_load();
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
                for alias in &n.aliases {
                    txt.push('\n');
                    txt.push_str(&alias.to_lowercase());
                }
                txt.push('\n');
                txt.push_str(&n.slug.to_lowercase());
                for tag in &n.tags {
                    txt.push('\n');
                    txt.push_str(&tag.to_lowercase());
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
        let mut refresh_entries = false;
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
                                        aliases: Vec::new(),
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
                        if app.note_settings.templates_enabled && ui.button("Templates").clicked() {
                            self.template_manager.open();
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
                                ui.vertical(|ui| {
                                    let title = display_title(&entry);
                                    let slug = if entry.slug.is_empty() {
                                        "unsaved"
                                    } else {
                                        &entry.slug
                                    };
                                    let mut meta = vec![format!("slug: {slug}")];
                                    if !entry.tags.is_empty() {
                                        meta.push(format!("tags: {}", entry.tags.join(", ")));
                                    }
                                    if app.note_settings.backlinks_enabled && !entry.slug.is_empty()
                                    {
                                        meta.push(format!(
                                            "{} backlinks",
                                            note_backlinks(&entry.slug).len()
                                        ));
                                    }
                                    if app.note_settings.task_lists_enabled {
                                        let count = checkbox_count(&entry.content);
                                        if count > 0 {
                                            meta.push(format!("{count} checkboxes"));
                                        }
                                    }
                                    let preview = short_preview(&entry.content);
                                    let resp = ui
                                        .horizontal(|ui| {
                                            ui.strong(title);
                                            ui.small(meta.join(" · "));
                                        })
                                        .response
                                        .on_hover_text(if preview.is_empty() {
                                            entry.content.clone()
                                        } else {
                                            preview.clone()
                                        });
                                    if !preview.is_empty() {
                                        ui.small(preview);
                                    }
                                    let idx_copy = idx;
                                    resp.clone().context_menu(|ui| {
                                        if ui.button("Open").clicked() {
                                            app.open_note_panel(&entry.slug, None);
                                            ui.close_menu();
                                        }
                                        if ui.button("Edit").clicked() {
                                            self.edit_idx = Some(idx_copy);
                                            self.text = entry.content.clone();
                                            ui.close_menu();
                                        }
                                        if ui.button("Open externally").clicked() {
                                            if let Err(e) = open::that(&entry.path) {
                                                app.report_error_message(
                                                    "ui operation",
                                                    format!("Failed to open note externally: {e}"),
                                                );
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Copy link").clicked() {
                                            let link = format!("[[{}]]", entry.slug);
                                            if let Err(e) =
                                                crate::actions::clipboard::set_text(&link)
                                            {
                                                app.report_error_message(
                                                    "ui operation",
                                                    format!("Failed to copy note link: {e}"),
                                                );
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Copy slug").clicked() {
                                            if let Err(e) =
                                                crate::actions::clipboard::set_text(&entry.slug)
                                            {
                                                app.report_error_message(
                                                    "ui operation",
                                                    format!("Failed to copy note slug: {e}"),
                                                );
                                            }
                                            ui.close_menu();
                                        }
                                        if app.note_settings.aliases_enabled
                                            && ui.button("Manage aliases").clicked()
                                        {
                                            app.open_note_panel(&entry.slug, None);
                                            ui.close_menu();
                                        }
                                        if app.note_settings.templates_enabled
                                            && ui.button("Create note from template").clicked()
                                        {
                                            app.activate_action(
                                                note_action("New note", "query:note templates"),
                                                None,
                                                ActivationSource::Click,
                                            );
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove Note").clicked() {
                                            if entry.slug.is_empty() {
                                                remove = Some(idx_copy);
                                            } else {
                                                app.activate_action(
                                                    note_action(
                                                        "Remove note",
                                                        format!("note:remove:{}", entry.slug),
                                                    ),
                                                    None,
                                                    ActivationSource::Click,
                                                );
                                                if !app.require_confirm_destructive {
                                                    refresh_entries = true;
                                                }
                                            }
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
                                });
                                ui.separator();
                            }
                        });
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        rebuild_idx = true;
                        save_now = true;
                    }
                }
            });
        if refresh_entries {
            self.entries = cached_notes_or_load();
            rebuild_idx = true;
        }
        if rebuild_idx {
            self.rebuild_index();
        }
        if save_now {
            self.save(app);
        }
        if close {
            self.open = false;
        }
        if app.note_settings.templates_enabled {
            self.template_manager.ui(ctx, app);
        } else {
            self.template_manager.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        checkbox_count, format_note_timestamp, insert_at_char_boundary, note_action, short_preview,
    };
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

    #[test]
    fn preview_omits_title_and_alias_metadata() {
        let preview = short_preview("# Title\nAlias: Primary\n\nBody text\nwith spacing");

        assert_eq!(preview, "Body text with spacing");
    }

    #[test]
    fn checkbox_count_counts_task_list_rows() {
        let content = "- [ ] open\n- [x] done\n- [X] upper\nnot a task";

        assert_eq!(checkbox_count(content), 3);
    }

    #[test]
    fn remove_note_context_action_uses_existing_remove_route() {
        let action = note_action("Remove note", "note:remove:alpha");

        assert_eq!(action.action, "note:remove:alpha");
        assert_eq!(action.desc, "Note");
    }

    #[test]
    fn open_note_context_action_uses_existing_open_route() {
        let action = note_action("Open note", "note:open:alpha");

        assert_eq!(action.action, "note:open:alpha");
        assert_eq!(action.desc, "Note");
    }
}
