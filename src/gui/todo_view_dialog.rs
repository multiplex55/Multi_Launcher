use crate::common::entity_ref::{EntityKind, EntityRef};
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, semantic_status, with_button_emphasis,
};
use crate::plugins::note::load_notes;
use crate::plugins::todo::{TODO_FILE, TodoEntry, load_todos, replace_todos};
use eframe::egui;
use std::collections::HashMap;

const TODO_VIEW_SIZE: egui::Vec2 = egui::vec2(360.0, 260.0);
const TODO_VIEW_LIST_HEIGHT: f32 = 170.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum TodoSortMode {
    #[default]
    Priority,
    Name,
}

pub fn todo_view_layout_sizes() -> (egui::Vec2, f32) {
    (TODO_VIEW_SIZE, TODO_VIEW_LIST_HEIGHT)
}

pub fn todo_view_window_constraints() -> (egui::Vec2, egui::Vec2) {
    (TODO_VIEW_SIZE, TODO_VIEW_SIZE)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct TodoViewScrollLayout {
    id: egui::Id,
    inner_rect: egui::Rect,
    clip_rect: egui::Rect,
    content_size: egui::Vec2,
    offset: egui::Vec2,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct TodoViewLayout {
    window_rect: egui::Rect,
    toolbar_rect: egui::Rect,
    filter_rect: egui::Rect,
    list: Option<TodoViewScrollLayout>,
    edit_fields: Vec<egui::Rect>,
    edit_save_rect: egui::Rect,
    edit_cancel_rect: egui::Rect,
    close_rect: egui::Rect,
    window_close_requested: bool,
    close_clicked: bool,
    edit_cancel_clicked: bool,
    visible: bool,
    sizing_pass: bool,
}

fn parse_note_token(token: &str) -> Option<(String, Option<String>)> {
    let slug = token.strip_prefix("@note:")?;
    let slug = slug.trim_matches(|c: char| ",.;)".contains(c));
    if slug.is_empty() {
        return None;
    }
    if let Some((base, anchor)) = slug.split_once('#') {
        if base.trim().is_empty() {
            None
        } else {
            Some((base.trim().to_string(), Some(anchor.trim().to_string())))
        }
    } else {
        Some((slug.to_string(), None))
    }
}

fn extract_anchor_snippet(content: &str, anchor: Option<&str>) -> String {
    let compact = content.replace('\n', " ");
    if let Some(anchor) = anchor {
        let low = compact.to_lowercase();
        let needle = anchor.to_lowercase();
        if let Some(i) = low.find(&needle) {
            let start = i.saturating_sub(40);
            let end = (i + needle.len() + 40).min(compact.len());
            return compact[start..end].to_string();
        }
    }
    compact.chars().take(100).collect()
}

fn note_has_anchor(content: &str, anchor: &str) -> bool {
    content.to_lowercase().contains(&anchor.to_lowercase())
}

#[derive(Default)]
pub struct TodoViewDialog {
    pub open: bool,
    entries: Vec<TodoEntry>,
    filter: String,
    sort_mode: TodoSortMode,
    editing_idx: Option<usize>,
    editing_text: String,
    editing_priority: u8,
    editing_tags: String,
    load_error: Option<String>,
    #[cfg(test)]
    test_row_response: Option<egui::Response>,
    #[cfg(test)]
    test_window_close_requested: bool,
    #[cfg(test)]
    test_layout: Option<TodoViewLayout>,
}

impl TodoViewDialog {
    pub fn open(&mut self) {
        self.open_from(TODO_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.filter.clear();
        self.sort_mode = TodoSortMode::Priority;
        self.editing_idx = None;
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(TODO_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    pub fn open_edit(&mut self, idx: usize) {
        if self.load_from(TODO_FILE).is_err() {
            self.editing_idx = None;
            self.open = true;
            return;
        }
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
        self.sort_mode = TodoSortMode::Priority;
    }

    fn sorted_todo_indices(
        entries: &[TodoEntry],
        filter: &str,
        sort_mode: TodoSortMode,
    ) -> Vec<usize> {
        let filter = filter.trim().to_lowercase();
        let mut indices: Vec<usize> = entries
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

        indices.sort_by(|a, b| {
            let entry_a = &entries[*a];
            let entry_b = &entries[*b];
            match sort_mode {
                TodoSortMode::Priority => entry_b
                    .priority
                    .cmp(&entry_a.priority)
                    .then_with(|| {
                        entry_a
                            .text
                            .to_lowercase()
                            .cmp(&entry_b.text.to_lowercase())
                    })
                    .then_with(|| a.cmp(b)),
                TodoSortMode::Name => entry_a
                    .text
                    .to_lowercase()
                    .cmp(&entry_b.text.to_lowercase())
                    .then_with(|| entry_b.priority.cmp(&entry_a.priority))
                    .then_with(|| a.cmp(b)),
            }
        });

        indices
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

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<TodoEntry>) -> bool {
        if let Err(e) = self.commit_entries(TODO_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save todos: {e}"));
            false
        } else {
            app.search();
            app.focus_input();
            true
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        #[cfg(test)]
        {
            self.test_row_response = None;
            self.test_window_close_requested = false;
            self.test_layout = None;
        }
        if !self.open {
            return;
        }
        let pinned = app
            .pinned_panels
            .contains(&crate::gui::Panel::TodoViewDialog);
        let (window_size, list_height) = todo_view_layout_sizes();
        let (min_size, max_size) = todo_view_window_constraints();
        let mut window_open = self.open;
        let mut close = false;
        let mut save_now = false;
        let mut candidate_entries = self.entries.clone();
        #[cfg(test)]
        let window_id = egui::Id::new("View Todos");
        #[cfg(test)]
        let mut toolbar_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut filter_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut list_layout = None;
        #[cfg(test)]
        let mut edit_fields = Vec::new();
        #[cfg(test)]
        let mut edit_cancel_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut edit_save_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut close_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut close_clicked = false;
        #[cfg(test)]
        let mut edit_cancel_clicked = false;
        #[cfg(test)]
        let mut visible = false;
        #[cfg(test)]
        let mut sizing_pass = false;
        let window_output = egui::Window::new("View Todos")
            .open(&mut window_open)
            .resizable(false)
            .default_size(window_size)
            .min_size(min_size)
            .max_size(max_size)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    visible = ui.is_visible();
                    sizing_pass = ui.is_sizing_pass();
                }
                if let Some(error) = &self.load_error {
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Todos are read-only because loading failed",
                        Some(error),
                    );
                    let close_button = ui.button("Close");
                    #[cfg(test)]
                    {
                        close_rect = close_button.rect;
                        close_clicked |= close_button.clicked();
                    }
                    if close_button.clicked() {
                        close = true;
                    }
                    return;
                }
                let toolbar = ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_label("Sort")
                        .selected_text(match self.sort_mode {
                            TodoSortMode::Priority => "Priority",
                            TodoSortMode::Name => "Name",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.sort_mode,
                                TodoSortMode::Priority,
                                "Priority",
                            );
                            ui.selectable_value(
                                &mut self.sort_mode,
                                TodoSortMode::Name,
                                "Name",
                            );
                        });
                    let filter_label = ui.label("Filter");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .desired_width(ui.available_width().min(180.0).max(72.0)),
                    )
                    .labelled_by(filter_label.id)
                });
                #[cfg(test)]
                {
                    toolbar_rect = toolbar.response.rect;
                    filter_rect = toolbar.inner.rect;
                }
                #[cfg(not(test))]
                let _ = toolbar;
                ui.separator();
                compact_section_heading(ui, "Tasks", None);
                let footer_height = todo_view_footer_reserved_height(ui);
                let actual_list_height = (ui.available_height() - footer_height)
                    .max(0.0)
                    .min(list_height);
                let indices =
                    Self::sorted_todo_indices(&candidate_entries, &self.filter, self.sort_mode);
                let note_titles: HashMap<String, String> = load_notes()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|n| (n.slug, n.title))
                    .collect();
                #[cfg(test)]
                let mut list_clip_rect = egui::Rect::NOTHING;
                // Keep horizontal overflow for long todo text without wrapping.
                let list_output = egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(actual_list_height)
                    .show(ui, |ui| {
                        #[cfg(test)]
                        {
                            list_clip_rect = ui.clip_rect();
                        }
                        for idx in indices {
                            if Some(idx) == self.editing_idx {
                                ui.vertical(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        let label = ui.label("Text:");
                                        let field = ui.add(
                                            egui::TextEdit::singleline(&mut self.editing_text)
                                                .desired_width(
                                                    ui.available_width().min(260.0).max(72.0),
                                                ),
                                        ).labelled_by(label.id);
                                        #[cfg(test)]
                                        edit_fields.push(field.rect);
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        let label = ui.label("Priority:");
                                        ui.add(
                                            egui::DragValue::new(&mut self.editing_priority)
                                                .range(0..=255),
                                        )
                                        .labelled_by(label.id);
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        let label = ui.label("Tags:");
                                        let field = ui.add(
                                            egui::TextEdit::singleline(&mut self.editing_tags)
                                                .desired_width(
                                                    ui.available_width().min(260.0).max(72.0),
                                                ),
                                        ).labelled_by(label.id);
                                        #[cfg(test)]
                                        edit_fields.push(field.rect);
                                    });
                                    ui.horizontal_wrapped(|ui| {
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
                                                    ui.close();
                                                }
                                            }
                                        });
                                    });
                                    ui.horizontal_wrapped(|ui| {
                                        let save_button = ui.button("Save");
                                        #[cfg(test)]
                                        {
                                            edit_save_rect = save_button.rect;
                                        }
                                        let save_button = with_button_emphasis(
                                            ui,
                                            save_button,
                                            ButtonEmphasis::Primary,
                                        );
                                        if save_button.clicked() {
                                            let tags: Vec<String> = self
                                                .editing_tags
                                                .split(',')
                                                .map(|t| t.trim())
                                                .filter(|t| !t.is_empty())
                                                .map(|t| t.to_string())
                                                .collect();
                                            if let Some(e) = candidate_entries.get_mut(idx) {
                                                e.text = self.editing_text.clone();
                                                e.priority = self.editing_priority;
                                                e.tags = tags;
                                                for token in self.editing_text.split_whitespace() {
                                                    if let Some(slug) = token.strip_prefix("@note:")
                                                        && !slug.trim().is_empty() {
                                                            Self::link_note_to_todo(
                                                                e,
                                                                slug.trim(),
                                                                slug.trim(),
                                                            );
                                                        }
                                                }
                                            }
                                            save_now = true;
                                        }
                                        let cancel_button = ui.button("Cancel");
                                        #[cfg(test)]
                                        {
                                            edit_cancel_rect = cancel_button.rect;
                                            edit_cancel_clicked |= cancel_button.clicked();
                                        }
                                        if cancel_button.clicked() {
                                            self.editing_idx = None;
                                        }
                                    });
                                });
                            } else {
                                let entry = &mut candidate_entries[idx];
                                ui.push_id(("todo_view_row", idx), |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.checkbox(&mut entry.done, "").changed() {
                                            save_now = true;
                                        }
                                        let text_for_render = entry.text.replace('\n', " ");
                                        let row_contents = ui.horizontal_wrapped(|ui| {
                                            let mut child_response: Option<egui::Response> = None;
                                            for token in text_for_render.split_whitespace() {
                                                let response = if let Some((slug, anchor)) =
                                                    parse_note_token(token)
                                                {
                                                    let label = note_titles
                                                        .get(&slug)
                                                        .cloned()
                                                        .unwrap_or_else(|| slug.to_string());
                                                    let link_label = if let Some(anchor) = anchor.as_ref() {
                                                        format!("{label}#{anchor}")
                                                    } else {
                                                        label
                                                    };
                                                    let response = ui.link(link_label);
                                                    if response.clicked() {
                                                        app.open_note_panel(&slug, None);
                                                    }
                                                    response
                                                } else {
                                                    ui.label(token)
                                                };
                                                child_response = Some(match child_response.take() {
                                                    Some(combined) => combined.union(response),
                                                    None => response,
                                                });
                                            }
                                            child_response
                                        });
                                        let mut resp = row_contents
                                            .response
                                            .interact(egui::Sense::click());
                                        if let Some(child_response) = row_contents.inner {
                                            resp = resp.union(child_response);
                                        }
                                        #[cfg(test)]
                                        {
                                            self.test_row_response = Some(resp.clone());
                                        }
                                        let idx_copy = idx;
                                        resp.clone().context_menu(|ui: &mut egui::Ui| {
                                            if ui.button("Edit Todo").clicked() {
                                                self.editing_idx = Some(idx_copy);
                                                self.editing_text = entry.text.clone();
                                                self.editing_priority = entry.priority;
                                                self.editing_tags = entry.tags.join(", ");
                                                ui.close();
                                            }
                                            ui.separator();
                                            ui.label("Link note");
                                            for note in load_notes()
                                                .unwrap_or_default()
                                                .into_iter()
                                                .take(10)
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
                                                    ui.close();
                                                }
                                            }
                                        });
                                        ui.add(
                                            egui::Label::new(format!("p{}", entry.priority))
                                                .wrap_mode(egui::TextWrapMode::Extend),
                                        );
                                        if !entry.tags.is_empty() {
                                            ui.add(
                                                egui::Label::new(format!(
                                                    "#{:?}",
                                                    entry.tags.join(", ")
                                                ))
                                                .wrap_mode(egui::TextWrapMode::Extend),
                                            );
                                        }
                                    });
                                    let linked_tokens: Vec<(String, Option<String>)> = entry
                                        .text
                                        .split_whitespace()
                                        .filter_map(parse_note_token)
                                        .collect();
                                    if !linked_tokens.is_empty() {
                                        ui.group(|ui| {
                                            ui.label("Attached notes");
                                            for (slug, anchor) in linked_tokens {
                                                let note = load_notes()
                                                    .unwrap_or_default()
                                                    .into_iter()
                                                    .find(|n| n.slug == slug);
                                                let title = note_titles
                                                    .get(&slug)
                                                    .cloned()
                                                    .unwrap_or_else(|| slug.clone());
                                                let snippet = note
                                                    .as_ref()
                                                    .map(|n| extract_anchor_snippet(&n.content, anchor.as_deref()))
                                                    .unwrap_or_else(|| "note not found".to_string());
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.strong(&title);
                                                    ui.small(&snippet);
                                                    if ui
                                                        .button("Open at anchor")
                                                        .on_hover_text("Open note at referenced anchor")
                                                        .clicked()
                                                    {
                                                        app.open_note_panel(&slug, None);
                                                        if let Some(anchor) = anchor.as_deref()
                                                            && let Some(n) = note.as_ref()
                                                                && !note_has_anchor(&n.content, anchor) {
                                                                    app.report_error_message("ui operation", format!(
                                                                        "Anchor '{}' not found in note '{}'; opened note top.",
                                                                        anchor, title
                                                                    ));
                                                                }
                                                    }
                                                });
                                            }
                                        });
                                    }
                                });
                            }
                        }
                    });
                #[cfg(test)]
                {
                    list_layout = Some(TodoViewScrollLayout {
                        id: list_output.id,
                        inner_rect: list_output.inner_rect,
                        clip_rect: list_clip_rect,
                        content_size: list_output.content_size,
                        offset: list_output.state.offset,
                    });
                }
                #[cfg(not(test))]
                let _ = list_output;
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let close_button = ui.button("Close");
                    #[cfg(test)]
                    {
                        close_rect = close_button.rect;
                        close_clicked |= close_button.clicked();
                    }
                    if close_button.clicked() {
                        close = true;
                    }
                });
            });
        #[cfg(test)]
        {
            self.test_window_close_requested = !window_open;
            self.test_layout = window_output.as_ref().map(|window| TodoViewLayout {
                window_rect: ctx
                    .memory(|memory| memory.area_rect(window_id))
                    .unwrap_or(window.response.rect),
                toolbar_rect,
                filter_rect,
                list: list_layout,
                edit_fields,
                edit_save_rect,
                edit_cancel_rect,
                close_rect,
                window_close_requested: !window_open,
                close_clicked,
                edit_cancel_clicked,
                visible,
                sizing_pass,
            });
        }
        #[cfg(not(test))]
        let _ = window_output;
        if close && !pinned {
            window_open = false;
        }
        self.open = window_open || pinned;
        if save_now {
            if self.save(app, candidate_entries) {
                self.editing_idx = None;
            }
        }
    }
}

fn todo_view_footer_reserved_height(ui: &mut egui::Ui) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .clone()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + native_button_style.frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = button_height + 2.0 * spacing.item_spacing.y + separator_height;
    let pixels_per_point = ui.pixels_per_point().max(f32::EPSILON);
    (height * pixels_per_point).ceil() / pixels_per_point
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        use std::sync::{Arc, atomic::AtomicBool};

        let mut settings = crate::settings::Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn render_todo_frame(
        ctx: &egui::Context,
        dialog: &mut TodoViewDialog,
        app: &mut crate::gui::LauncherApp,
        events: Vec<egui::Event>,
        time: f64,
    ) -> (Option<TodoViewLayout>, Option<egui::Rect>) {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| dialog.ui(ui.ctx(), app),
        );
        let close_window_rect = accesskit_button_bounds(&output, "Close window");
        let layout = dialog.test_layout.clone();
        output.drop_without_applying_deltas();
        (layout, close_window_rect)
    }

    fn accesskit_button_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
        output
            .platform_output
            .accesskit_update
            .as_ref()?
            .nodes
            .iter()
            .filter(|(_, node)| {
                node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
            })
            .find_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ))
            })
    }

    fn scroll_todo_view_control_into_view(
        ctx: &egui::Context,
        list: TodoViewScrollLayout,
        target_rect: egui::Rect,
    ) -> f32 {
        let max_offset = (list.content_size.y - list.clip_rect.height()).max(0.0);
        let target_offset = (list.offset.y + target_rect.center().y - list.clip_rect.center().y)
            .clamp(0.0, max_offset);
        let mut state = egui::scroll_area::State::load(ctx, list.id)
            .expect("the rendered TodoView list has registered native scroll state");
        state.offset.y = target_offset;
        state.store(ctx, list.id);
        target_offset
    }

    fn assert_todo_view_control_visible(
        list: TodoViewScrollLayout,
        control_name: &str,
        control_rect: egui::Rect,
    ) {
        assert!(
            list.clip_rect.contains_rect(control_rect),
            "{control_name} must be reachable inside the actual list viewport: id={:?}, clip={:?}, inner_rect={:?}, control={control_rect:?}, offset={:?}, content_size={:?}",
            list.id,
            list.clip_rect,
            list.inner_rect,
            list.offset,
            list.content_size,
        );
    }

    fn click_todo_view_point(
        ctx: &egui::Context,
        dialog: &mut TodoViewDialog,
        app: &mut crate::gui::LauncherApp,
        point: egui::Pos2,
        time: f64,
    ) -> (Option<TodoViewLayout>, Option<egui::Rect>) {
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
            time,
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
            time + 0.1,
        )
    }

    fn todo_entry(text: &str, priority: u8, tags: &[&str]) -> TodoEntry {
        TodoEntry {
            id: format!("id-{text}"),
            text: text.to_string(),
            done: false,
            priority,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            entity_refs: vec![],
        }
    }

    fn write_todos(path: &std::path::Path, entries: &[TodoEntry]) {
        std::fs::write(path, serde_json::to_vec_pretty(entries).unwrap()).unwrap();
    }

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

    #[test]
    fn snippet_extraction_prefers_anchor_context() {
        let content = "alpha beta gamma important-anchor delta epsilon";
        let snippet = extract_anchor_snippet(content, Some("important-anchor"));
        assert!(snippet.contains("important-anchor"));
    }

    #[test]
    fn anchor_detection_found_vs_missing() {
        let content = "# heading-one\nbody";
        assert!(note_has_anchor(content, "heading-one"));
        assert!(!note_has_anchor(content, "missing-anchor"));
    }

    #[test]
    fn parse_note_token_handles_anchor() {
        assert_eq!(
            parse_note_token("@note:plan#phase-1"),
            Some(("plan".to_string(), Some("phase-1".to_string())))
        );
    }

    #[test]
    fn sorted_indices_priority_ordering_with_ties() {
        let entries = vec![
            todo_entry("beta", 3, &[]),
            todo_entry("alpha", 3, &[]),
            todo_entry("gamma", 9, &[]),
            todo_entry("alpha", 3, &[]),
        ];

        let indices = TodoViewDialog::sorted_todo_indices(&entries, "", TodoSortMode::Priority);
        assert_eq!(indices, vec![2, 1, 3, 0]);
    }

    #[test]
    fn sorted_indices_name_ordering_with_priority_tiebreak() {
        let entries = vec![
            todo_entry("zeta", 2, &[]),
            todo_entry("Alpha", 1, &[]),
            todo_entry("alpha", 7, &[]),
            todo_entry("beta", 9, &[]),
        ];

        let indices = TodoViewDialog::sorted_todo_indices(&entries, "", TodoSortMode::Name);
        assert_eq!(indices, vec![2, 1, 3, 0]);
    }

    #[test]
    fn sorted_indices_name_mode_is_stable_when_name_and_priority_match() {
        let entries = vec![
            todo_entry("same", 4, &[]),
            todo_entry("same", 4, &[]),
            todo_entry("same", 4, &[]),
        ];

        let indices = TodoViewDialog::sorted_todo_indices(&entries, "", TodoSortMode::Name);
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn sorted_indices_filter_and_sort_interaction() {
        let entries = vec![
            todo_entry("alpha", 2, &["home"]),
            todo_entry("bravo", 5, &["work"]),
            todo_entry("charlie", 7, &["work-urgent"]),
            todo_entry("work note", 1, &["misc"]),
        ];

        let by_text = TodoViewDialog::sorted_todo_indices(&entries, "work", TodoSortMode::Name);
        assert_eq!(by_text, vec![1, 2, 3]);

        let by_tag = TodoViewDialog::sorted_todo_indices(&entries, "#work", TodoSortMode::Priority);
        assert_eq!(by_tag, vec![2, 1]);
    }

    #[test]
    fn invalid_reload_and_commit_keep_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("todo.json");
        let initial = vec![todo_entry("saved", 1, &[])];
        crate::plugins::todo::save_todos(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = TodoViewDialog::default();
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
    fn pinned_open_preserves_draft_and_reload_occurs_only_after_close() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("todo.json");
        let path_text = path.to_string_lossy().into_owned();
        let initial = vec![todo_entry("initial task", 3, &["initial"])];
        write_todos(&path, &initial);

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_pixels_per_point(1.0);
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = test_app(&ctx, directory.path());
        let mut dialog = TodoViewDialog::default();
        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);

        dialog.filter = "initial".into();
        dialog.sort_mode = TodoSortMode::Name;
        dialog.editing_idx = Some(0);
        dialog.editing_text = "unsaved edit".into();
        dialog.editing_priority = 9;
        dialog.editing_tags = "draft, tags".into();
        let replacement = vec![todo_entry("replacement task", 7, &["new"])];
        write_todos(&path, &replacement);

        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.filter, "initial");
        assert_eq!(dialog.sort_mode, TodoSortMode::Name);
        assert_eq!(dialog.editing_idx, Some(0));
        assert_eq!(dialog.editing_text, "unsaved edit");
        assert_eq!(dialog.editing_priority, 9);
        assert_eq!(dialog.editing_tags, "draft, tags");

        app.pinned_panels.push(crate::gui::Panel::TodoViewDialog);
        let _ = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.0);
        let (_, window_close) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.1);
        let window_close = window_close.expect("the native window X has accessible bounds");
        let (x_close_layout, _) =
            click_todo_view_point(&ctx, &mut dialog, &mut app, window_close.center(), 1.2);
        assert!(
            x_close_layout
                .expect("the window remains rendered while pinned")
                .window_close_requested,
            "the simulated click reached the native window X"
        );
        assert!(dialog.open, "pinned window X retains the dialog");
        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, initial);
        assert_eq!(dialog.filter, "initial");
        assert_eq!(dialog.editing_text, "unsaved edit");

        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.4);
        let layout = layout.expect("the edit form remains rendered");
        let initial_list = layout
            .list
            .expect("the edit form is inside the native task ScrollArea");
        let scroll_id = initial_list.id;
        let target_offset =
            scroll_todo_view_control_into_view(&ctx, initial_list, layout.edit_cancel_rect);
        let _ = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.5);
        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.6);
        let layout = layout.expect("the scrolled edit form remains rendered");
        let list = layout
            .list
            .expect("the scrolled edit form remains in the task ScrollArea");
        assert_eq!(list.id, scroll_id, "the edit form keeps its ScrollArea ID");
        assert!(
            (list.offset.y - target_offset).abs() <= 1.0,
            "the native list reaches the legal target offset: expected={target_offset}, actual={:?}, content_size={:?}, clip={:?}",
            list.offset,
            list.content_size,
            list.clip_rect,
        );
        assert_todo_view_control_visible(list, "Save", layout.edit_save_rect);
        assert_todo_view_control_visible(list, "Cancel", layout.edit_cancel_rect);
        let cancel_rect = layout.edit_cancel_rect;
        let (cancelled_layout, _) =
            click_todo_view_point(&ctx, &mut dialog, &mut app, cancel_rect.center(), 1.7);
        assert!(
            cancelled_layout
                .expect("the dialog remains rendered after edit Cancel")
                .edit_cancel_clicked
        );
        assert_eq!(dialog.editing_idx, None);
        assert!(dialog.open, "edit Cancel does not close a pinned view");

        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.9);
        let close_rect = layout.expect("the pinned view renders Close").close_rect;
        let (pinned_close_layout, _) =
            click_todo_view_point(&ctx, &mut dialog, &mut app, close_rect.center(), 2.0);
        assert!(
            pinned_close_layout
                .expect("the pinned view remains rendered")
                .close_clicked,
            "the simulated click reached the ordinary Close button"
        );
        assert!(dialog.open, "ordinary Close is refused while pinned");

        app.pinned_panels.clear();
        let (_, window_close) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 2.2);
        let unpinned_close = window_close.expect("the native window X remains available");
        let (unpinned_close_layout, _) =
            click_todo_view_point(&ctx, &mut dialog, &mut app, unpinned_close.center(), 2.3);
        assert!(
            unpinned_close_layout
                .expect("the window is still available for the close frame")
                .window_close_requested
        );
        assert!(!dialog.open, "unpinned window X ends the view session");

        dialog.ensure_open_from(&path_text);
        assert_eq!(dialog.entries, replacement);
        assert!(dialog.filter.is_empty());
        assert_eq!(dialog.sort_mode, TodoSortMode::Priority);
        assert_eq!(dialog.editing_idx, None);

        let invalid = b"malformed todo fixture";
        std::fs::write(&path, invalid).unwrap();
        dialog.open = false;
        dialog.ensure_open_from(&path_text);
        assert!(dialog.open);
        assert_eq!(dialog.entries, replacement);
        assert!(dialog.load_error.is_some());
        assert_eq!(std::fs::read(path).unwrap(), invalid);

        app.pinned_panels.push(crate::gui::Panel::TodoViewDialog);
        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 2.5);
        let close_rect = layout
            .expect("the read-only error view still renders Close")
            .close_rect;
        let (pinned_error_close, _) =
            click_todo_view_point(&ctx, &mut dialog, &mut app, close_rect.center(), 2.6);
        assert!(
            pinned_error_close
                .expect("the pinned error view remains rendered")
                .close_clicked
        );
        assert!(dialog.open, "read-only error Close is refused while pinned");
    }

    #[test]
    fn fixed_view_bounds_long_filter_edit_form_list_and_close_footer() {
        let directory = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut app = test_app(&ctx, directory.path());
        let long_filter = "task-filter-".repeat(6);
        let entries = (0..8)
            .map(|index| {
                todo_entry(
                    &format!(
                        "task {index} {long_filter} {}",
                        "wrapped note text ".repeat(5)
                    ),
                    3,
                    &["long-tag-value"],
                )
            })
            .collect::<Vec<_>>();
        let mut dialog = TodoViewDialog {
            open: true,
            entries,
            filter: long_filter.clone(),
            editing_idx: Some(0),
            editing_text: format!("edited task {long_filter} {}", "long value ".repeat(8)),
            editing_priority: 11,
            editing_tags: format!("tag-one, tag-two, {}", "long tag ".repeat(8)),
            ..Default::default()
        };

        let (first_layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.0);
        let first_scroll_id = first_layout
            .and_then(|layout| layout.list)
            .map(|list| list.id);
        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.1);
        let initial_layout = layout.expect("View Todos renders a fixed window");
        let initial_list = initial_layout
            .list
            .expect("the task body has a ScrollArea before scrolling");
        let target_controls = initial_layout
            .edit_save_rect
            .union(initial_layout.edit_cancel_rect);
        let target_offset = scroll_todo_view_control_into_view(&ctx, initial_list, target_controls);
        let _ = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.2);
        let (layout, _) = render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.3);
        let layout = layout.expect("View Todos renders a fixed window after scrolling");
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
        assert!(layout.visible);
        assert!(!layout.sizing_pass);
        assert!(screen_rect.contains_rect(layout.window_rect));
        assert!(layout.window_rect.contains_rect(layout.toolbar_rect));
        assert!(layout.window_rect.contains_rect(layout.filter_rect));
        assert!(layout.filter_rect.width() <= 180.5);
        assert!(layout.edit_save_rect.is_positive());
        assert!(layout.edit_cancel_rect.is_positive());
        assert_eq!(
            layout.edit_fields.len(),
            2,
            "the edit text and tags use bounded fields"
        );
        let list = layout.list.expect("the task body has a ScrollArea");
        assert_eq!(
            Some(list.id),
            first_scroll_id,
            "the list keeps its widget ID"
        );
        assert!(list.inner_rect.height() <= TODO_VIEW_LIST_HEIGHT + 0.5);
        assert!(list.clip_rect.is_positive());
        assert!(layout.window_rect.contains_rect(list.inner_rect));
        assert!(layout.window_rect.contains_rect(list.clip_rect));
        assert!(list.clip_rect.max.y <= layout.close_rect.min.y);
        assert!(
            (list.offset.y - target_offset).abs() <= 1.0,
            "the native list reaches the legal edit-control offset: expected={target_offset}, actual={:?}, content_size={:?}, clip={:?}",
            list.offset,
            list.content_size,
            list.clip_rect,
        );
        assert_todo_view_control_visible(list, "Save", layout.edit_save_rect);
        assert_todo_view_control_visible(list, "Cancel", layout.edit_cancel_rect);
        assert!(layout.window_rect.contains_rect(layout.close_rect));
        assert!(
            list.content_size.y > list.clip_rect.height(),
            "long rows use the bounded list viewport"
        );
        assert!(list.offset.y >= 0.0);
        assert!(!layout.window_close_requested);
    }

    #[test]
    fn todo_row_context_menu_opens_on_secondary_click() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, workspace.root());
        let mut dialog = TodoViewDialog {
            open: true,
            entries: vec![todo_entry("Task with @note:linked", 1, &[])],
            ..Default::default()
        };
        render_todo_frame(&ctx, &mut dialog, &mut app, Vec::new(), 1.0);
        let response = dialog
            .test_row_response
            .as_ref()
            .expect("the todo row was rendered");
        assert!(response.sense.senses_click());
        let pointer = response.rect.center();

        render_todo_frame(
            &ctx,
            &mut dialog,
            &mut app,
            vec![egui::Event::PointerMoved(pointer)],
            1.1,
        );
        render_todo_frame(
            &ctx,
            &mut dialog,
            &mut app,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            1.2,
        );
        render_todo_frame(
            &ctx,
            &mut dialog,
            &mut app,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Secondary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            1.3,
        );
        let row_response = dialog
            .test_row_response
            .as_ref()
            .expect("the todo row remains rendered after the click");
        assert!(
            row_response.context_menu_opened(),
            "the row's secondary-click menu opens on its wrapped text: id={:?}, rect={:?}, sense={:?}, hovered={}, contains={}, secondary={}, any={}",
            row_response.id,
            row_response.rect,
            row_response.sense,
            row_response.hovered(),
            row_response.contains_pointer(),
            row_response.secondary_clicked(),
            egui::Popup::is_any_open(&ctx),
        );
        assert!(egui::Popup::is_any_open(&ctx));
        drop(dialog);
        drop(app);
        drop(workspace);
    }
}
