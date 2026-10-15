use crate::common::entity_ref::EntityRef;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use crate::plugins::note::load_notes;
use crate::plugins::todo::{TODO_FILE, TodoEntry, load_todos, replace_todos};
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
    pending_clear_confirm: bool,
    load_error: Option<String>,
    #[cfg(test)]
    test_window_close_requested: bool,
}

impl TodoDialog {
    pub fn open(&mut self) {
        self.open_from(TODO_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.text.clear();
        self.priority = 0;
        self.tags.clear();
        self.filter.clear();
        self.pending_clear_confirm = false;
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(TODO_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_todos(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_entries(&mut self, path: &str, candidate: Vec<TodoEntry>) -> anyhow::Result<()> {
        match replace_todos(path, candidate) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save(&mut self, app: &mut LauncherApp, focus: bool, candidate: Vec<TodoEntry>) -> bool {
        if let Err(e) = self.commit_entries(TODO_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save todos: {e}"));
            false
        } else {
            app.search();
            if focus {
                app.focus_input();
            }
            true
        }
    }

    pub fn filtered_indices(entries: &[TodoEntry], filter: &str) -> Vec<usize> {
        let mut filter = filter.trim().to_lowercase();
        let mut negative = false;
        if let Some(stripped) = filter.strip_prefix('!') {
            negative = true;
            filter = stripped.to_string();
        }
        entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if filter.is_empty() {
                    true
                } else if filter.starts_with('#') {
                    let tag = filter.trim_start_matches('#');
                    let has_tag = e.tags.iter().any(|t| t.to_lowercase().contains(tag));
                    if negative { !has_tag } else { has_tag }
                } else {
                    let text_match = e.text.to_lowercase().contains(&filter)
                        || e.tags.iter().any(|t| t.to_lowercase().contains(&filter));
                    if negative { !text_match } else { text_match }
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn add_todo(&mut self) -> bool {
        if self.text.trim().is_empty() {
            return false;
        }
        let tag_list: Vec<String> = self
            .tags
            .trim()
            .trim_end_matches(',')
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.to_owned())
            .collect();
        tracing::debug!("Adding todo: '{}' tags={:?}", self.text, tag_list);
        self.entries.push(TodoEntry {
            id: String::new(),
            text: self.text.clone(),
            done: false,
            priority: self.priority,
            tags: tag_list,
            entity_refs: Vec::<EntityRef>::new(),
        });
        self.text.clear();
        self.priority = 0;
        if !self.persist_tags {
            self.tags.clear();
        }
        true
    }

    fn confirm_clear_completed(&mut self, app: &mut LauncherApp, confirmed: bool) {
        if !self.pending_clear_confirm {
            return;
        }
        if confirmed {
            let mut candidate = self.entries.clone();
            let original_len = candidate.len();
            candidate.retain(|e| !e.done);
            if candidate.len() != original_len {
                self.save(app, false, candidate);
            }
        }
        self.pending_clear_confirm = false;
    }

    pub fn test_set_text(&mut self, text: &str) {
        self.text = text.to_owned();
    }

    pub fn test_set_tags(&mut self, tags: &str) {
        self.tags = tags.to_owned();
    }

    pub fn test_entries(&self) -> &Vec<TodoEntry> {
        &self.entries
    }

    pub fn test_add_todo(&mut self) -> bool {
        self.add_todo()
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        #[cfg(test)]
        {
            self.test_window_close_requested = false;
        }
        if !self.open {
            return;
        }
        let pinned = app.pinned_panels.contains(&crate::gui::Panel::TodoDialog);
        let mut window_open = self.open;
        let mut save_now = false;
        let mut add_now = false;
        let mut clear_confirmed: Option<bool> = None;
        let mut candidate_entries = self.entries.clone();
        egui::Window::new("Todos")
            .open(&mut window_open)
            .resizable(true)
            .default_size((320.0, 200.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                if let Some(error) = &self.load_error {
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Todos are read-only because loading failed",
                        Some(error),
                    );
                    return;
                }
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                let area_height = ui.available_height();
                let mut remove: Option<usize> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.checkbox(&mut self.persist_tags, "Persist Tags");
                    let field_width = (ui.available_width() - 72.0).clamp(80.0, 240.0);
                    egui::Grid::new("todo_add_grid")
                        .num_columns(2)
                        .spacing([4.0, 2.0])
                        .striped(false)
                        .show(ui, |ui| {
                            let text_label = ui.label(egui::RichText::new("New Todo").strong());
                            let text_resp = ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.text)
                                        .desired_width(field_width),
                                )
                                .labelled_by(text_label.id);
                            ui.end_row();

                            let tags_label = ui.label("Tags");
                            let tags_resp = ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.tags)
                                        .desired_width(field_width),
                                )
                                .labelled_by(tags_label.id);
                            ui.end_row();

                            let priority_label = ui.label("Priority");
                            let (prio_resp, add_resp) = ui
                                .horizontal_wrapped(|ui| {
                                    (
                                        ui.add(
                                            egui::DragValue::new(&mut self.priority).range(0..=255),
                                        ),
                                        ui.button("Add"),
                                    )
                                })
                                .inner;
                            let prio_resp = prio_resp.labelled_by(priority_label.id);
                            let add_resp =
                                with_button_emphasis(ui, add_resp, ButtonEmphasis::Primary);

                            if text_resp.changed() {
                                tracing::debug!("Todo text updated: '{}'", self.text);
                            }
                            if tags_resp.changed() {
                                tracing::debug!("Todo tags updated: '{}'", self.tags);
                            }

                            add_now |= add_resp.clicked();
                            if (text_resp.lost_focus()
                                || tags_resp.lost_focus()
                                || prio_resp.lost_focus())
                                && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let modifiers = ctx.input(|i| i.modifiers);
                                ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                                tracing::debug!(
                                    "Enter pressed in TodoDialog fields: text='{}', tags='{}'",
                                    self.text,
                                    self.tags
                                );
                                add_now = true;
                            }
                            ui.end_row();
                        });
                    ui.horizontal_wrapped(|ui| {
                        let clear_button = ui.button("Clear Completed");
                        let clear_button =
                            with_button_emphasis(ui, clear_button, ButtonEmphasis::Destructive);
                        if clear_button.clicked() {
                            self.pending_clear_confirm = true;
                        }
                    });
                    if self.pending_clear_confirm {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Clear completed todos?");
                            let confirm = ui.button("Confirm");
                            let confirm =
                                with_button_emphasis(ui, confirm, ButtonEmphasis::Destructive);
                            if confirm.clicked() {
                                clear_confirmed = Some(true);
                            }
                            if ui.button("Cancel").clicked() {
                                clear_confirmed = Some(false);
                            }
                        });
                    }
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let filter_label = ui.label("Filter");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .desired_width(ui.available_width().min(240.0).max(72.0)),
                        )
                        .labelled_by(filter_label.id);
                    });
                    compact_section_heading(ui, "Todo list", None);
                    let indices = Self::filtered_indices(&candidate_entries, &self.filter);
                    egui::ScrollArea::both()
                        .max_height(area_height)
                        .show(ui, |ui| {
                            for idx in indices {
                                ui.horizontal(|ui| {
                                    let entry = &mut candidate_entries[idx];
                                    if ui.checkbox(&mut entry.done, "").changed() {
                                        save_now = true;
                                    }
                                    ui.label(entry.text.replace('\n', " "));
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut entry.priority)
                                                .range(0..=255),
                                        )
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
                                    let remove_btn = ui.button("Remove");
                                    let remove_btn = with_button_emphasis(
                                        ui,
                                        remove_btn,
                                        ButtonEmphasis::Destructive,
                                    );
                                    remove_btn.context_menu(|ui| {
                                        ui.label("Link note");
                                        for note in
                                            load_notes().unwrap_or_default().into_iter().take(8)
                                        {
                                            if ui
                                                .button(format!(
                                                    "@note:{} {}",
                                                    note.slug, note.title
                                                ))
                                                .clicked()
                                            {
                                                if !entry
                                                    .text
                                                    .contains(&format!("@note:{}", note.slug))
                                                {
                                                    entry
                                                        .text
                                                        .push_str(&format!(" @note:{}", note.slug));
                                                }
                                                entry.entity_refs.push(
                                                    crate::common::entity_ref::EntityRef::new(
                                                        crate::common::entity_ref::EntityKind::Note,
                                                        note.slug,
                                                        Some(note.title),
                                                    ),
                                                );
                                                save_now = true;
                                                ui.close();
                                            }
                                        }
                                    });
                                    if remove_btn.clicked() {
                                        remove = Some(idx);
                                    }
                                });
                            }
                        });
                });
                if let Some(idx) = remove {
                    candidate_entries.remove(idx);
                    save_now = true;
                }
            });

        #[cfg(test)]
        {
            self.test_window_close_requested = !window_open;
        }
        self.open = window_open || pinned;

        if self.open
            && self.load_error.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
            && !app.todo_view_dialog.open
        {
            let modifiers = ctx.input(|i| i.modifiers);
            ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
            tracing::debug!(
                "Enter pressed in TodoDialog: text='{}', tags='{}'",
                self.text,
                self.tags
            );
            add_now = true;
        }

        if add_now {
            if self.text.trim().is_empty() {
                tracing::debug!("Enter pressed but todo text empty; ignoring");
            } else {
                let tag_list = self
                    .tags
                    .trim()
                    .trim_end_matches(',')
                    .split(',')
                    .map(str::trim)
                    .filter(|tag| !tag.is_empty())
                    .map(str::to_owned)
                    .collect();
                candidate_entries.push(TodoEntry {
                    id: String::new(),
                    text: self.text.clone(),
                    done: false,
                    priority: self.priority,
                    tags: tag_list,
                    entity_refs: Vec::new(),
                });
                save_now = true;
            }
        }
        if let Some(confirmed) = clear_confirmed {
            if confirmed {
                let original_len = candidate_entries.len();
                candidate_entries.retain(|entry| !entry.done);
                save_now |= candidate_entries.len() != original_len;
            }
            self.pending_clear_confirm = false;
        }
        if save_now {
            if self.save(app, false, candidate_entries) {
                self.text.clear();
                self.priority = 0;
                if !self.persist_tags {
                    self.tags.clear();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn todo_entry(id: &str, text: &str, tags: &[&str]) -> TodoEntry {
        TodoEntry {
            id: id.to_owned(),
            text: text.to_owned(),
            done: false,
            priority: 2,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            entity_refs: Vec::new(),
        }
    }

    fn write_todos(path: &std::path::Path, entries: &[TodoEntry]) {
        std::fs::write(path, serde_json::to_vec_pretty(entries).unwrap()).unwrap();
    }

    fn window_close_button_bounds(output: &egui::FullOutput) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .find(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some("Close window")
            })
            .and_then(|(_, node)| node.bounds())
            .map(|bounds| {
                egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                )
            })
    }

    fn render_todo_frame(
        ctx: &egui::Context,
        dialog: &mut TodoDialog,
        app: &mut LauncherApp,
        events: Vec<egui::Event>,
    ) -> (Option<egui::Rect>, bool) {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        );
        let close_button = window_close_button_bounds(&output);
        let close_requested = dialog.test_window_close_requested;
        output.drop_without_applying_deltas();
        (close_button, close_requested)
    }

    fn click_todo_window_x(
        ctx: &egui::Context,
        dialog: &mut TodoDialog,
        app: &mut LauncherApp,
        rect: egui::Rect,
    ) -> bool {
        let point = rect.center();
        let _ = render_todo_frame(
            ctx,
            dialog,
            app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_todo_frame(
            ctx,
            dialog,
            app,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .1
    }

    #[test]
    fn enter_adds_todo_with_filter_focus() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut dlg = TodoDialog::default();
        dlg.open();
        dlg.text = "task".into();
        dlg.filter = "something".into();

        ctx.begin_pass(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();

        assert_eq!(dlg.entries.len(), 1);
        assert_eq!(dlg.entries[0].text, "task");
    }

    #[test]
    fn enter_adds_todo_with_tags() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut dlg = TodoDialog::default();
        dlg.open();
        dlg.text = "tagged".into();
        dlg.tags = "a, b".into();

        ctx.begin_pass(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        dlg.ui(&ctx, &mut app);
        ctx.end_pass().drop_without_applying_deltas();

        assert_eq!(dlg.entries.len(), 1);
        assert_eq!(dlg.entries[0].text, "tagged");
        assert_eq!(dlg.entries[0].tags, vec!["a", "b"]);
    }

    #[test]
    fn clear_completed_requires_confirmation() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut dlg = TodoDialog::default();
        dlg.entries = vec![
            TodoEntry {
                id: String::new(),
                text: "done".into(),
                done: true,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
            TodoEntry {
                id: String::new(),
                text: "pending".into(),
                done: false,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
        ];
        dlg.pending_clear_confirm = true;

        dlg.confirm_clear_completed(&mut app, false);

        assert_eq!(dlg.entries.len(), 2);
        assert!(dlg.entries.iter().any(|e| e.done));
    }

    #[test]
    fn clear_completed_after_confirmation_saves() {
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut dlg = TodoDialog::default();
        dlg.entries = vec![
            TodoEntry {
                id: String::new(),
                text: "done".into(),
                done: true,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
            TodoEntry {
                id: String::new(),
                text: "pending".into(),
                done: false,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
        ];
        dlg.pending_clear_confirm = true;

        dlg.confirm_clear_completed(&mut app, true);

        assert_eq!(dlg.entries.len(), 1);
        assert!(!dlg.entries[0].done);
        let saved = load_todos(TODO_FILE).unwrap_or_default();
        assert_eq!(saved.len(), 1);
        assert!(!saved[0].done);
    }

    #[test]
    fn invalid_reload_and_commit_keep_last_good_and_read_only() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("todo.json");
        let initial = vec![TodoEntry {
            id: "saved".into(),
            text: "saved".into(),
            done: false,
            priority: 1,
            tags: Vec::new(),
            entity_refs: Vec::new(),
        }];
        crate::plugins::todo::save_todos(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = TodoDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid todos";
        std::fs::write(&path, invalid).unwrap();
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), Vec::new())
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn pinned_open_preserves_drafts_and_reload_waits_for_a_real_close() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("todo.json");
        let path_text = path.to_string_lossy().into_owned();
        let initial = vec![todo_entry("initial-id", "initial task", &["saved"])];
        write_todos(&path, &initial);

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_pixels_per_point(1.0);
        let mut app = new_app(&ctx);
        let mut dialog = TodoDialog::default();
        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);

        dialog.text = "unsaved task".into();
        dialog.priority = 8;
        dialog.tags = "draft, tags".into();
        dialog.filter = "draft filter".into();
        dialog.persist_tags = true;
        dialog.pending_clear_confirm = true;
        let replacement = vec![todo_entry("replacement-id", "replacement task", &["new"])];
        write_todos(&path, &replacement);

        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.text, "unsaved task");
        assert_eq!(dialog.priority, 8);
        assert_eq!(dialog.tags, "draft, tags");
        assert_eq!(dialog.filter, "draft filter");
        assert!(dialog.persist_tags);
        assert!(dialog.pending_clear_confirm);

        app.pinned_panels.push(crate::gui::Panel::TodoDialog);
        let _ = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new());
        let (window_x, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new());
        let window_x = window_x.expect("the native window X has accessible bounds");
        assert!(
            click_todo_window_x(&ctx, &mut dialog, &mut app, window_x),
            "the simulated click reached the native X and closed its local window state"
        );
        assert!(dialog.open, "a pinned window X retains the dialog");
        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.text, "unsaved task");
        assert_eq!(dialog.tags, "draft, tags");
        assert_eq!(dialog.filter, "draft filter");
        assert!(dialog.pending_clear_confirm);

        app.pinned_panels.clear();
        let (window_x, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new());
        let window_x = window_x.expect("the native X remains available after unpinning");
        assert!(click_todo_window_x(&ctx, &mut dialog, &mut app, window_x));
        assert!(!dialog.open, "an unpinned window X ends the dialog session");

        dialog.ensure_open_from(&path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, replacement);
        assert!(dialog.text.is_empty());
        assert_eq!(dialog.priority, 0);
        assert!(dialog.tags.is_empty());
        assert!(dialog.filter.is_empty());
        assert!(!dialog.pending_clear_confirm);
        assert!(
            dialog.persist_tags,
            "open keeps the existing tag preference"
        );

        let invalid = b"malformed todo fixture";
        std::fs::write(&path, invalid).unwrap();
        dialog.open = false;
        dialog.ensure_open_from(&path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, replacement);
        assert!(dialog.load_error.is_some());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
}
