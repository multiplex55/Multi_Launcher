use crate::actions::Action;
use crate::gui::{ActivationSource, LauncherApp};
use crate::plugins::note::{
    Note, append_note_content, delete_template, get_template, list_templates, note_backlink_count,
    note_cache_snapshot_with_version, note_version, reload_templates, save_note, save_note_content,
    save_template, template_path, validate_template_name,
};
use crate::plugins::todo::{TODO_FILE, load_todos_or_last_good};
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

fn display_title(note: &Note) -> &str {
    note.alias.as_deref().unwrap_or(&note.title)
}

fn short_preview(content: &str) -> String {
    const LIMIT: usize = 120;

    let mut preview = String::with_capacity(LIMIT + 3);
    let mut char_count = 0;
    let mut pending_space = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("# ") || trimmed.starts_with("Alias:") {
            continue;
        }
        if !preview.is_empty() {
            pending_space = true;
        }

        for character in line.chars() {
            if character.is_whitespace() {
                pending_space |= !preview.is_empty();
                continue;
            }
            if pending_space {
                if char_count == LIMIT {
                    if preview.ends_with(' ') {
                        preview.pop();
                    }
                    preview.push('…');
                    return preview;
                }
                preview.push(' ');
                char_count += 1;
                pending_space = false;
            }
            if char_count == LIMIT {
                preview.push('…');
                return preview;
            }
            preview.push(character);
            char_count += 1;
        }
    }
    preview
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

fn build_search_index(entries: &[Note]) -> Vec<String> {
    entries
        .iter()
        .map(|note| {
            let mut text = note.content.to_lowercase();
            if let Some(alias) = &note.alias {
                text.push('\n');
                text.push_str(&alias.to_lowercase());
            }
            for alias in &note.aliases {
                text.push('\n');
                text.push_str(&alias.to_lowercase());
            }
            text.push('\n');
            text.push_str(&note.slug.to_lowercase());
            for tag in &note.tags {
                text.push('\n');
                text.push_str(&tag.to_lowercase());
            }
            text
        })
        .collect()
}

fn build_row_metadata(
    entries: &[Note],
    backlinks_enabled: bool,
    task_lists_enabled: bool,
) -> anyhow::Result<Vec<NoteRowMetadata>> {
    entries
        .iter()
        .enumerate()
        .map(|(original_index, note)| {
            let mut meta = vec![format!(
                "slug: {}",
                if note.slug.is_empty() {
                    "unsaved"
                } else {
                    &note.slug
                }
            )];
            if !note.tags.is_empty() {
                meta.push(format!("tags: {}", note.tags.join(", ")));
            }
            if backlinks_enabled && !note.slug.is_empty() {
                meta.push(format!("{} backlinks", note_backlink_count(&note.slug)?));
            }
            if task_lists_enabled {
                let count = checkbox_count(&note.content);
                if count > 0 {
                    meta.push(format!("{count} checkboxes"));
                }
            }
            Ok(NoteRowMetadata {
                original_index,
                display_title: display_title(note).to_owned(),
                meta: meta.join(" · "),
                preview: short_preview(&note.content),
            })
        })
        .collect()
}

fn note_action(label: impl Into<String>, action: impl Into<String>) -> Action {
    Action {
        label: label.into(),
        desc: "Note".into(),
        action: action.into(),
        args: None,
    }
}

fn wrap_links_note_action(slug: &str) -> Action {
    note_action("Wrap links in note", format!("note:meta:wrap-links:{slug}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NoteRowMetadata {
    original_index: usize,
    display_title: String,
    meta: String,
    preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetadataKey {
    entries_generation: u64,
    note_revision: u64,
    backlinks_enabled: bool,
    task_lists_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionKey {
    entries_generation: u64,
    raw_search: String,
    filter: String,
}

#[derive(Clone, Debug)]
struct RefreshRetry {
    note_revision: u64,
    backlinks_enabled: bool,
    task_lists_enabled: bool,
    deadline: f64,
}

fn update_filtered_projection(
    search: &str,
    index: &[String],
    entries_generation: u64,
    filtered_indices: &mut Vec<usize>,
    projection_key: &mut Option<ProjectionKey>,
) -> bool {
    let raw_search_changed = projection_key
        .as_ref()
        .is_none_or(|key| key.raw_search != search);
    let generation_changed = projection_key
        .as_ref()
        .is_none_or(|key| key.entries_generation != entries_generation);
    if !raw_search_changed && !generation_changed {
        return false;
    }

    let filter = if raw_search_changed {
        search.to_lowercase()
    } else {
        projection_key
            .as_ref()
            .map(|key| key.filter.clone())
            .unwrap_or_default()
    };
    let filter_changed = projection_key
        .as_ref()
        .is_none_or(|key| key.filter != filter);
    if filter_changed || generation_changed {
        *filtered_indices = index
            .iter()
            .enumerate()
            .filter_map(|(index, text)| {
                (filter.is_empty() || text.contains(&filter)).then_some(index)
            })
            .collect();
    }
    *projection_key = Some(ProjectionKey {
        entries_generation,
        raw_search: search.to_owned(),
        filter,
    });
    filter_changed || generation_changed
}

#[derive(Default)]
pub struct NotesDialog {
    pub open: bool,
    entries: Vec<Note>,
    index: Vec<String>,
    entries_revision: Option<u64>,
    refresh_requested: bool,
    entries_generation: u64,
    row_metadata: Vec<NoteRowMetadata>,
    metadata_key: Option<MetadataKey>,
    filtered_indices: Vec<usize>,
    projection_key: Option<ProjectionKey>,
    refresh_retry: Option<RefreshRetry>,
    edit_idx: Option<usize>,
    text: String,
    search: String,
    template_manager: TemplateManagerState,
    #[cfg(test)]
    last_rendered_indices: Vec<usize>,
    #[cfg(test)]
    test_note_snapshot_calls: u64,
    #[cfg(test)]
    test_metadata_rebuilds: u64,
    #[cfg(test)]
    test_projection_rebuilds: u64,
    #[cfg(test)]
    test_preview_builds: u64,
    #[cfg(test)]
    test_fail_next_snapshot: bool,
    #[cfg(test)]
    test_race_after_candidate: bool,
}

enum PendingNoteSave {
    New(Note),
    Existing { identity: String, content: String },
    Append { identity: String, suffix: String },
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
    fn capture_note_snapshot(&mut self) -> anyhow::Result<(u64, Vec<Note>)> {
        #[cfg(test)]
        {
            self.test_note_snapshot_calls = self.test_note_snapshot_calls.saturating_add(1);
            if std::mem::take(&mut self.test_fail_next_snapshot) {
                anyhow::bail!("injected Quick Notes snapshot failure");
            }
        }
        note_cache_snapshot_with_version()
    }

    fn install_candidate(
        &mut self,
        revision: u64,
        entries: Vec<Note>,
        index: Vec<String>,
        metadata: Vec<NoteRowMetadata>,
        settings: Option<(bool, bool)>,
    ) {
        self.entries = entries;
        self.index = index;
        self.entries_revision = Some(revision);
        self.refresh_requested = false;
        self.entries_generation = self.entries_generation.wrapping_add(1).max(1);
        self.row_metadata = metadata;
        self.metadata_key = settings.map(|(backlinks_enabled, task_lists_enabled)| MetadataKey {
            entries_generation: self.entries_generation,
            note_revision: revision,
            backlinks_enabled,
            task_lists_enabled,
        });
        self.filtered_indices.clear();
        self.projection_key = None;
        self.refresh_retry = None;
        #[cfg(test)]
        if self.metadata_key.is_some() {
            self.test_metadata_rebuilds = self.test_metadata_rebuilds.saturating_add(1);
            self.test_preview_builds = self
                .test_preview_builds
                .saturating_add(self.row_metadata.len().try_into().unwrap_or(u64::MAX));
        }
    }

    fn mark_retry(
        &mut self,
        ctx: &egui::Context,
        revision: u64,
        backlinks_enabled: bool,
        task_lists_enabled: bool,
    ) {
        const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(1);
        let now = ctx.input(|input| input.time);
        let deadline = now + RETRY_AFTER.as_secs_f64();
        self.refresh_retry = Some(RefreshRetry {
            note_revision: revision,
            backlinks_enabled,
            task_lists_enabled,
            deadline,
        });
        ctx.request_repaint_after(RETRY_AFTER);
    }

    fn retry_wait_remaining(
        &self,
        revision: u64,
        backlinks_enabled: bool,
        task_lists_enabled: bool,
        now: f64,
    ) -> Option<std::time::Duration> {
        let retry = self.refresh_retry.as_ref()?;
        if retry.note_revision != revision
            || retry.backlinks_enabled != backlinks_enabled
            || retry.task_lists_enabled != task_lists_enabled
            || now >= retry.deadline
        {
            return None;
        }
        Some(std::time::Duration::from_secs_f64(retry.deadline - now))
    }

    fn maybe_refresh_derived(
        &mut self,
        ctx: &egui::Context,
        settings: &crate::settings::NoteSettings,
    ) {
        if self.edit_idx.is_some() {
            return;
        }

        let backlinks_enabled = settings.backlinks_enabled;
        let task_lists_enabled = settings.task_lists_enabled;
        let observed_revision = note_version();
        let metadata_matches = self.metadata_key.as_ref().is_some_and(|key| {
            key.entries_generation == self.entries_generation
                && key.note_revision == observed_revision
                && key.backlinks_enabled == backlinks_enabled
                && key.task_lists_enabled == task_lists_enabled
        });
        let entries_changed =
            self.refresh_requested || self.entries_revision != Some(observed_revision);
        if entries_changed || !metadata_matches {
            let now = ctx.input(|input| input.time);
            if let Some(wait) = self.retry_wait_remaining(
                observed_revision,
                backlinks_enabled,
                task_lists_enabled,
                now,
            ) {
                ctx.request_repaint_after(wait);
                return;
            }
        }

        if entries_changed {
            let (revision, entries) = match self.capture_note_snapshot() {
                Ok(snapshot) => snapshot,
                Err(_) => {
                    self.mark_retry(
                        ctx,
                        observed_revision,
                        backlinks_enabled,
                        task_lists_enabled,
                    );
                    return;
                }
            };
            let index = build_search_index(&entries);
            let metadata = match build_row_metadata(&entries, backlinks_enabled, task_lists_enabled)
            {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.mark_retry(
                        ctx,
                        observed_revision,
                        backlinks_enabled,
                        task_lists_enabled,
                    );
                    return;
                }
            };
            #[cfg(test)]
            let forced_race = std::mem::take(&mut self.test_race_after_candidate);
            #[cfg(not(test))]
            let forced_race = false;
            if note_version() != revision || forced_race {
                self.mark_retry(ctx, note_version(), backlinks_enabled, task_lists_enabled);
                return;
            }
            self.install_candidate(
                revision,
                entries,
                index,
                metadata,
                Some((backlinks_enabled, task_lists_enabled)),
            );
        } else if !metadata_matches {
            let metadata =
                match build_row_metadata(&self.entries, backlinks_enabled, task_lists_enabled) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        self.mark_retry(
                            ctx,
                            observed_revision,
                            backlinks_enabled,
                            task_lists_enabled,
                        );
                        return;
                    }
                };
            #[cfg(test)]
            let forced_race = std::mem::take(&mut self.test_race_after_candidate);
            #[cfg(not(test))]
            let forced_race = false;
            if note_version() != observed_revision || forced_race {
                self.mark_retry(ctx, note_version(), backlinks_enabled, task_lists_enabled);
                return;
            }
            self.row_metadata = metadata;
            self.metadata_key = Some(MetadataKey {
                entries_generation: self.entries_generation,
                note_revision: observed_revision,
                backlinks_enabled,
                task_lists_enabled,
            });
            self.refresh_retry = None;
            #[cfg(test)]
            {
                self.test_metadata_rebuilds = self.test_metadata_rebuilds.saturating_add(1);
                self.test_preview_builds = self
                    .test_preview_builds
                    .saturating_add(self.row_metadata.len().try_into().unwrap_or(u64::MAX));
            }
        }

        self.rebuild_projection_if_needed();
    }

    fn rebuild_projection_if_needed(&mut self) {
        if update_filtered_projection(
            &self.search,
            &self.index,
            self.entries_generation,
            &mut self.filtered_indices,
            &mut self.projection_key,
        ) {
            #[cfg(test)]
            {
                self.test_projection_rebuilds = self.test_projection_rebuilds.saturating_add(1);
            }
        }
    }

    fn rebuild_index(&mut self) {
        self.index = build_search_index(&self.entries);
        self.entries_generation = self.entries_generation.wrapping_add(1).max(1);
        self.row_metadata.clear();
        self.metadata_key = None;
        self.filtered_indices.clear();
        self.projection_key = None;
    }

    fn ensure_entries_for_edit(&mut self) {
        if self.entries_revision.is_some() {
            return;
        }
        let Ok((revision, entries)) = self.capture_note_snapshot() else {
            return;
        };
        let index = build_search_index(&entries);
        if note_version() == revision {
            self.install_candidate(revision, entries, index, Vec::new(), None);
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.edit_idx = None;
        self.text.clear();
        self.search.clear();
        if self.template_manager.open {
            self.template_manager.refresh();
        }
    }

    pub(crate) fn refresh_entries_from_notes(&mut self) {
        if self.edit_idx.is_none() {
            self.refresh_requested = true;
        }
    }

    pub fn open_edit(&mut self, idx: usize) {
        self.ensure_entries_for_edit();
        if idx < self.entries.len() {
            self.text = self.entries[idx].content.clone();
        } else {
            self.text.clear();
        }
        self.edit_idx = Some(idx);
        self.open = true;
    }

    fn save_note_draft(&mut self, pending: PendingNoteSave, app: &mut LauncherApp) -> bool {
        let result = match pending {
            PendingNoteSave::New(mut note) => {
                save_note(&mut note, true).map(|saved| saved.then_some(note))
            }
            PendingNoteSave::Existing { identity, content } => {
                save_note_content(&identity, &content)
            }
            PendingNoteSave::Append { identity, suffix } => append_note_content(&identity, &suffix),
        };
        match result {
            Ok(Some(_)) => {
                if self.edit_idx.is_none() {
                    self.refresh_entries_from_notes();
                }
                app.search();
                app.focus_input();
                true
            }
            Ok(None) => {
                app.report_error_message("ui operation", "The note no longer exists");
                false
            }
            Err(e) => {
                app.report_error_message("ui operation", format!("Failed to save note: {e}"));
                false
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        #[cfg(test)]
        self.last_rendered_indices.clear();
        if !self.open {
            return;
        }
        self.maybe_refresh_derived(ctx, &app.note_settings);
        let mut close = false;
        let mut save_edit = false;
        let mut note_to_save: Option<PendingNoteSave> = None;
        let mut rebuild_idx = false;
        let mut refresh_entries = false;
        let mut wrap_links_slug: Option<String> = None;
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
                                if idx >= self.entries.len() {
                                    let title =
                                        self.text.lines().next().unwrap_or("untitled").to_string();
                                    note_to_save = Some(PendingNoteSave::New(Note {
                                        title,
                                        path: std::path::PathBuf::new(),
                                        content: self.text.clone(),
                                        tags: Vec::new(),
                                        links: Vec::new(),
                                        slug: String::new(),
                                        alias: None,
                                        aliases: Vec::new(),
                                        entity_refs: Vec::new(),
                                    }));
                                } else if let Some(existing) = self.entries.get(idx) {
                                    note_to_save = Some(PendingNoteSave::Existing {
                                        identity: existing.slug.clone(),
                                        content: self.text.clone(),
                                    });
                                }
                                save_edit = true;
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
                    if update_filtered_projection(
                        &self.search,
                        &self.index,
                        self.entries_generation,
                        &mut self.filtered_indices,
                        &mut self.projection_key,
                    ) {
                        #[cfg(test)]
                        {
                            self.test_projection_rebuilds =
                                self.test_projection_rebuilds.saturating_add(1);
                        }
                    }
                    let mut remove: Option<usize> = None;
                    let area_height = ui.available_height();
                    let mut rows_timer = crate::performance::MetricTimer::start(
                        crate::performance::Metric::QuickNotesRowsBuilt,
                    );
                    rows_timer.set_work_units(0);
                    egui::ScrollArea::both()
                        .max_height(area_height)
                        .show(ui, |ui| {
                            for projection_index in 0..self.filtered_indices.len() {
                                let idx = self.filtered_indices[projection_index];
                                let Some(entry) = self.entries.get(idx) else {
                                    continue;
                                };
                                let Some(row) = self
                                    .row_metadata
                                    .get(idx)
                                    .filter(|row| row.original_index == idx)
                                else {
                                    continue;
                                };
                                ui.vertical(|ui| {
                                    let preview = row.preview.as_str();
                                    let resp = ui
                                        .horizontal(|ui| {
                                            ui.strong(&row.display_title);
                                            ui.small(&row.meta);
                                        })
                                        .response
                                        .on_hover_ui(|ui| {
                                            if preview.is_empty() {
                                                ui.label(&entry.content);
                                            } else {
                                                ui.label(preview);
                                            }
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
                                        ui.menu_button("Meta", |ui| {
                                            if ui.button("Wrap links in note").clicked() {
                                                wrap_links_slug = Some(entry.slug.clone());
                                                ui.close_menu();
                                            }
                                        });
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
                                        for todo in
                                            load_todos_or_last_good(TODO_FILE).into_iter().take(8)
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
                                                if let Some(target) = self.entries.get(idx_copy) {
                                                    note_to_save = Some(PendingNoteSave::Append {
                                                        identity: target.slug.clone(),
                                                        suffix: format!("\n@todo:{todo_id}"),
                                                    });
                                                }
                                                ui.close_menu();
                                            }
                                        }
                                    });
                                });
                                rows_timer.add_work_units(1);
                                #[cfg(test)]
                                self.last_rendered_indices.push(idx);
                                ui.separator();
                            }
                        });
                    drop(rows_timer);
                    if let Some(idx) = remove {
                        self.entries.remove(idx);
                        rebuild_idx = true;
                    }
                }
            });
        if let Some(slug) = wrap_links_slug {
            app.activate_action(wrap_links_note_action(&slug), None, ActivationSource::Click);
        }
        if refresh_entries {
            self.refresh_entries_from_notes();
        }
        if rebuild_idx {
            self.rebuild_index();
        }
        if let Some(note) = note_to_save {
            if self.save_note_draft(note, app) && save_edit {
                self.edit_idx = None;
                self.text.clear();
                self.refresh_entries_from_notes();
            }
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
        NotesDialog, PendingNoteSave, checkbox_count, format_note_timestamp,
        insert_at_char_boundary, note_action, short_preview,
    };
    use crate::gui::{LauncherApp, NotePanel};
    use crate::plugins::note::{Note, load_notes, save_note, save_notes};
    use crate::{
        plugin::PluginManager,
        settings::{NoteViewMode, Settings},
    };
    use chrono::{Local, TimeZone};
    use eframe::egui;
    use once_cell::sync::Lazy;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use tempfile::tempdir;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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

    fn new_isolated_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut settings = Settings::default();
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
            settings,
            None,
            None,
            Some(std::collections::HashSet::new()),
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn render_notes_frame(
        ctx: &egui::Context,
        dialog: &mut NotesDialog,
        app: &mut LauncherApp,
        time: f64,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        );
    }

    fn note(title: &str, slug: &str, content: &str) -> Note {
        Note {
            title: title.into(),
            path: Default::default(),
            content: content.into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    fn setup() -> (
        tempfile::TempDir,
        std::path::PathBuf,
        egui::Context,
        LauncherApp,
    ) {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        save_notes(&[]).unwrap();
        let ctx = egui::Context::default();
        let app = new_app(&ctx);
        (dir, notes_dir, ctx, app)
    }

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_quick_notes_production_rows() {
        use crate::performance::{Metric, workloads};
        use std::cell::{Cell, RefCell};

        let workspace = workloads::IsolatedWorkspace::new();
        std::fs::write(crate::plugins::todo::TODO_FILE, b"[]")
            .expect("write isolated todo snapshot");
        let root = workspace.root();
        let mut settings = Settings::default();
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;

        for count in workloads::selected_sizes(&[100, 1_000, 5_000]) {
            let fixture = workloads::note_fixture(0x5155_4943_4b4e_4f54, count);
            let context = egui::Context::default();
            let mut app = LauncherApp::new(
                &context,
                Arc::new(Vec::new()),
                0,
                PluginManager::new_inert_for_test(),
                root.join("actions.json").to_string_lossy().into_owned(),
                root.join("settings.json").to_string_lossy().into_owned(),
                settings.clone(),
                None,
                None,
                Some(std::collections::HashSet::new()),
                None,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            );
            // Launcher construction initializes the isolated on-disk note cache.
            // Publish after construction so the dialog's real open/edit path sees
            // the deterministic in-memory fixture rather than that empty startup
            // snapshot.
            let cache_guard =
                crate::plugins::note::publish_note_cache_for_test(fixture.values.clone());
            let mut dialog = NotesDialog::default();
            assert_eq!(
                crate::plugins::note::note_cache_snapshot().len(),
                count,
                "launcher construction preserves the fixture note snapshot"
            );
            dialog.open();
            render_notes_frame(&context, &mut dialog, &mut app, 1.0);
            assert_eq!(dialog.entries.len(), count);

            let edit_index = count / 2;
            dialog.open_edit(edit_index);
            assert_eq!(dialog.edit_idx, Some(edit_index));
            assert!(
                dialog.text == fixture.values[edit_index].content,
                "editing resolves to the stable source note identity"
            );
            dialog.edit_idx = None;
            dialog.text.clear();

            for (scenario, filter, expected_indices) in [
                (
                    "empty-filter",
                    String::new(),
                    (0..count).collect::<Vec<_>>(),
                ),
                (
                    "sparse-filter",
                    format!("track-a-note-{edit_index:05}"),
                    vec![edit_index],
                ),
            ] {
                dialog.search = filter.clone();
                let input_slot = RefCell::new(None);
                let frame_index = Cell::new(0_usize);
                let (timing, _frame_output) = workloads::measure(
                    workloads::UI_WARMUPS,
                    |_, _| {
                        let frame = frame_index.get();
                        frame_index.set(frame + 1);
                        *input_slot.borrow_mut() = Some(egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(960.0, 640.0),
                            )),
                            time: Some(1.0 + frame as f64 / 60.0),
                            ..Default::default()
                        });
                    },
                    || {
                        let input = input_slot
                            .borrow_mut()
                            .take()
                            .expect("setup creates deterministic raw input");
                        context.run(input, |ctx| dialog.ui(ctx, &mut app))
                    },
                );
                let metrics =
                    workloads::metrics_for(&[Metric::QuickNotesRowsBuilt, Metric::NoteSnapshot]);
                assert_eq!(metrics.len(), 2);
                assert_eq!(metrics[0].metric, Metric::NoteSnapshot);
                assert_eq!(metrics[0].calls, 0, "warm browsing takes no note snapshots");
                assert_eq!(metrics[1].metric, Metric::QuickNotesRowsBuilt);
                assert_eq!(metrics[1].calls, workloads::SAMPLE_COUNT as u64);
                assert!(!dialog.last_rendered_indices.is_empty());
                assert!(
                    metrics[1].work_units
                        == (dialog.last_rendered_indices.len() * workloads::SAMPLE_COUNT) as u64,
                    "row counter tracks actual Quick Notes widget construction"
                );
                assert!(
                    dialog
                        .last_rendered_indices
                        .windows(2)
                        .all(|pair| pair[0] < pair[1])
                );
                assert!(
                    dialog
                        .last_rendered_indices
                        .iter()
                        .all(|index| expected_indices.binary_search(index).is_ok())
                );
                assert_eq!(dialog.edit_idx, None);
                assert_eq!(
                    dialog.entries[edit_index].slug,
                    fixture.values[edit_index].slug
                );

                let mut signature = workloads::StableSignature::new(
                    0,
                    "quick-notes-projection",
                    dialog.last_rendered_indices.len(),
                );
                for index in &dialog.last_rendered_indices {
                    let note = &dialog.entries[*index];
                    signature.number(*index as u64);
                    signature.bytes(note.slug.as_bytes());
                    signature.bytes(note.title.as_bytes());
                    signature.bytes(note.alias.as_deref().unwrap_or_default().as_bytes());
                }
                workloads::emit_summary(
                    &format!("quick-notes-{count}-{scenario}"),
                    "production NotesDialog::ui; headless egui debug-test CPU; in-memory notes",
                    fixture
                        .summary
                        .with_output_signatures(Some(signature.finish()), None),
                    timing,
                    &metrics,
                );
            }
            drop(dialog);
            drop(app);
            drop(cache_guard);
        }
        drop(workspace);
    }

    #[test]
    fn wrap_links_context_action_uses_expected_meta_route_for_selected_slug() {
        let action = super::wrap_links_note_action("alpha");

        assert_eq!(action.label, "Wrap links in note");
        assert_eq!(action.desc, "Note");
        assert_eq!(action.action, "note:meta:wrap-links:alpha");
    }

    #[test]
    fn quick_notes_wrap_links_saves_closed_note_and_refreshes_entries() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let mut alpha = note("Alpha", "alpha", "visit https://example.com");
        save_note(&mut alpha, true).unwrap();
        app.notes_dialog.open();
        app.notes_dialog.search = "example".into();

        let mut notes_dialog = std::mem::take(&mut app.notes_dialog);
        render_notes_frame(&ctx, &mut notes_dialog, &mut app, 1.0);
        app.notes_dialog = notes_dialog;
        let original_revision = crate::plugins::note::note_version();
        assert_eq!(app.notes_dialog.entries_revision, Some(original_revision));
        assert_eq!(app.notes_dialog.entries.len(), 1);
        assert_eq!(
            app.notes_dialog.entries[0].content,
            "# Alpha\n\nvisit https://example.com"
        );
        assert!(app.notes_dialog.metadata_key.is_some());
        assert_eq!(app.notes_dialog.filtered_indices, vec![0]);

        let mut notes_dialog = std::mem::take(&mut app.notes_dialog);
        assert!(!app.notes_dialog.open);

        app.wrap_note_plain_links("alpha");
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 0);
        assert_eq!(
            notes_dialog.entries[0].content, "# Alpha\n\nvisit https://example.com",
            "the detached dialog retains its last-good presentation until it is rendered again"
        );
        assert!(crate::plugins::note::note_version() > original_revision);

        render_notes_frame(&ctx, &mut notes_dialog, &mut app, 1.1);
        app.notes_dialog = notes_dialog;

        let saved = load_notes().unwrap().remove(0);
        assert!(
            saved
                .content
                .contains("[https://example.com](https://example.com)")
        );
        assert!(
            app.notes_dialog.entries[0]
                .content
                .contains("[https://example.com](https://example.com)")
        );
        assert_eq!(
            app.notes_dialog.entries_revision,
            Some(crate::plugins::note::note_version())
        );
        assert_eq!(app.notes_dialog.search, "example");
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 0);

        drop(app);
        drop(workspace);
    }

    #[test]
    fn quick_notes_wrap_links_mutates_open_unsaved_content_from_memory() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _notes_dir, _ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "disk https://disk.example");
        save_note(&mut alpha, true).unwrap();
        let mut panel = NotePanel::from_note(alpha);
        panel.replace_content_after_external_mutation("memory https://memory.example".into());
        app.note_panels.push(panel);

        app.wrap_note_plain_links("alpha");

        assert!(
            app.note_panels[0]
                .note_content()
                .contains("[https://memory.example](https://memory.example)")
        );
        assert!(!app.note_panels[0].note_content().contains("disk.example"));
        assert!(
            load_notes().unwrap()[0]
                .content
                .contains("[https://memory.example](https://memory.example)")
        );
    }

    #[test]
    fn quick_notes_wrap_links_noop_preserves_content_and_modification_state() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _notes_dir, _ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "already [link](https://example.com)");
        save_note(&mut alpha, true).unwrap();
        let mut panel = NotePanel::from_note(alpha);
        panel.replace_content_after_external_mutation("already [link](https://example.com)".into());
        let modified_before = panel.test_last_edit_at_secs();
        app.note_panels.push(panel);

        app.wrap_note_plain_links("alpha");

        assert_eq!(
            app.note_panels[0].note_content(),
            "already [link](https://example.com)"
        );
        assert_eq!(app.note_panels[0].test_last_edit_at_secs(), modified_before);
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 0);
    }

    #[test]
    fn quick_notes_wrap_links_preserves_open_panel_and_view_mode() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _notes_dir, _ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "visit https://example.com");
        save_note(&mut alpha, true).unwrap();
        let mut panel = NotePanel::from_note(alpha);
        panel.open = true;
        panel.test_set_view_mode(NoteViewMode::Split);
        app.note_panels.push(panel);

        app.wrap_note_plain_links("alpha");

        assert_eq!(app.note_panels.len(), 1);
        assert!(app.note_panels[0].open);
        assert_eq!(app.note_panels[0].test_view_mode(), NoteViewMode::Split);
    }

    #[test]
    fn stale_quick_notes_edit_retains_a_later_added_note() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _notes_dir, ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "# Alpha\n\noriginal");
        save_note(&mut alpha, true).unwrap();
        let mut dialog = NotesDialog::default();
        dialog.open();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);

        let mut beta = note("Beta", "beta", "# Beta\n\nadded later");
        save_note(&mut beta, true).unwrap();
        assert!(dialog.save_note_draft(
            PendingNoteSave::Existing {
                identity: "alpha".into(),
                content: "# Alpha\n\nedited".into(),
            },
            &mut app,
        ));
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.0);

        assert_eq!(dialog.entries.len(), 2);
        assert!(dialog.entries.iter().any(|note| note.slug == "beta"));
        assert!(load_notes().unwrap().iter().any(|note| note.slug == "beta"));
    }

    #[test]
    fn failed_quick_notes_edit_retains_dialog_and_committed_note() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (dir, notes_dir, ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "# Alpha\n\noriginal");
        save_note(&mut alpha, true).unwrap();
        let mut dialog = NotesDialog::default();
        dialog.open();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);
        dialog.open_edit(0);
        dialog.text = "unsaved draft retained on failure".into();
        let entries_before = dialog.entries.clone();
        let blocker = dir.path().join("not-a-directory");
        std::fs::write(&blocker, "block note directory creation").unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &blocker) };

        let saved = dialog.save_note_draft(
            PendingNoteSave::Existing {
                identity: "alpha".into(),
                content: "# Alpha\n\nshould fail".into(),
            },
            &mut app,
        );
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };

        assert!(!saved);
        assert!(dialog.open);
        assert_eq!(dialog.edit_idx, Some(0));
        assert_eq!(dialog.text, "unsaved draft retained on failure");
        assert_eq!(dialog.entries, entries_before);
        assert_eq!(load_notes().unwrap()[0].content, "# Alpha\n\noriginal");
    }

    #[test]
    fn existing_context_action_routes_are_unchanged() {
        assert_eq!(
            note_action("Open note", "note:open:alpha").action,
            "note:open:alpha"
        );
        assert_eq!(
            note_action("Edit note", "note:edit:alpha").action,
            "note:edit:alpha"
        );
        assert_eq!(
            note_action("Remove note", "note:remove:alpha").action,
            "note:remove:alpha"
        );
        assert_eq!(
            format!("@todo:{} {}", "todo-1", "Ship it"),
            "@todo:todo-1 Ship it"
        );
    }

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
    fn preview_matches_normalized_reference_and_unicode_boundaries() {
        fn eager_reference(content: &str) -> String {
            let normalized = content
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
            if normalized.chars().count() > 120 {
                format!("{}…", normalized.chars().take(120).collect::<String>())
            } else {
                normalized
            }
        }

        for content in [
            "\n\u{2003}\t\r\nhello",
            "## retained heading\r\nAliases: retained metadata\r\n\r\nBody\ttext\nnext line",
            "  # skipped\nAlias: skipped\n  Body  \t text \r\n",
            &"x".repeat(119),
            &"x".repeat(120),
            &"x".repeat(121),
            &"東京".repeat(60),
            &"東京".repeat(61),
            &format!("{}\nAlias: a very long skipped prefix", "z".repeat(121)),
        ] {
            assert_eq!(
                short_preview(content),
                eager_reference(content),
                "{content:?}"
            );
        }
        assert_eq!(short_preview(&"x".repeat(120)).chars().count(), 120);
        assert_eq!(short_preview(&"x".repeat(121)).chars().count(), 121);
        assert_eq!(short_preview("\n\u{2003}hello"), "hello");
    }

    #[test]
    fn quick_notes_reuses_metadata_and_keeps_sparse_original_indices() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let _outer_cache = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        let _fixture_cache = crate::plugins::note::publish_note_cache_for_test(vec![
            note("Alpha", "alpha", "# Alpha\n\n[[Target]]\n- [ ] checkbox"),
            note("Beta", "beta", "# Beta\n\nordinary body"),
            note("Target", "target", "# Target\n\nunique-needle"),
        ]);
        let mut dialog = NotesDialog::default();
        dialog.open();
        dialog.open_edit(2);
        assert_eq!(dialog.text, "# Target\n\nunique-needle");
        dialog.edit_idx = None;
        dialog.text.clear();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 1);
        assert_eq!(dialog.test_projection_rebuilds, 1);
        assert_eq!(dialog.test_preview_builds, 3);
        assert!(dialog.row_metadata[0].meta.contains("1 checkboxes"));
        assert!(dialog.row_metadata[2].meta.contains("1 backlinks"));

        render_notes_frame(&ctx, &mut dialog, &mut app, 1.1);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 1);
        assert_eq!(dialog.test_projection_rebuilds, 1);
        assert_eq!(dialog.test_preview_builds, 3);

        dialog.search = "unique-needle".into();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.2);
        assert_eq!(dialog.filtered_indices, vec![2]);
        assert_eq!(dialog.last_rendered_indices, vec![2]);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 1);
        assert_eq!(dialog.test_projection_rebuilds, 2);

        app.note_settings.backlinks_enabled = false;
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.3);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 2);
        assert_eq!(dialog.test_projection_rebuilds, 2);
        assert!(!dialog.row_metadata[0].meta.contains("backlinks"));

        app.note_settings.task_lists_enabled = false;
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.4);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 3);
        assert!(!dialog.row_metadata[0].meta.contains("checkboxes"));

        app.note_settings.backlinks_enabled = true;
        app.note_settings.task_lists_enabled = true;
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.5);
        assert_eq!(dialog.test_note_snapshot_calls, 1);
        assert_eq!(dialog.test_metadata_rebuilds, 4);
        assert!(dialog.row_metadata[2].meta.contains("1 backlinks"));
        assert!(dialog.row_metadata[0].meta.contains("1 checkboxes"));

        dialog.open_edit(2);
        assert_eq!(dialog.text, "# Target\n\nunique-needle");
        assert_eq!(dialog.edit_idx, Some(2));
        drop(dialog);
        drop(app);
        drop(_fixture_cache);
        drop(_outer_cache);
        drop(workspace);
    }

    #[test]
    fn quick_notes_defers_drafts_and_keeps_last_good_candidate_until_recovery() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let _outer_cache = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        let _fixture_cache = crate::plugins::note::publish_note_cache_for_test(vec![note(
            "Original",
            "original",
            "# Original\n\ncommitted body",
        )]);
        let mut dialog = NotesDialog::default();
        dialog.open();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);

        let initial_entries = dialog.entries.clone();
        let initial_index = dialog.index.clone();
        let initial_metadata = dialog.row_metadata.clone();
        let initial_projection = dialog.filtered_indices.clone();
        let initial_metadata_key = dialog.metadata_key.clone();
        let initial_projection_key = dialog.projection_key.clone();
        dialog.refresh_entries_from_notes();
        dialog.test_race_after_candidate = true;
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.1);
        assert_eq!(dialog.entries, initial_entries);
        assert_eq!(dialog.index, initial_index);
        assert_eq!(dialog.row_metadata, initial_metadata);
        assert_eq!(dialog.filtered_indices, initial_projection);
        assert_eq!(dialog.metadata_key, initial_metadata_key);
        assert_eq!(dialog.projection_key, initial_projection_key);
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.5);
        assert_eq!(dialog.test_note_snapshot_calls, 2);
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.2);

        let original_metadata = dialog.row_metadata.clone();
        let original_index = dialog.index.clone();
        let original_projection = dialog.filtered_indices.clone();
        let original_metadata_key = dialog.metadata_key.clone();
        let original_projection_key = dialog.projection_key.clone();

        dialog.open_edit(dialog.entries.len());
        dialog.text = "unsaved draft".into();
        let _external_draft = crate::plugins::note::publish_note_cache_for_test(vec![
            note("Original", "original", "# Original\n\nexternal update"),
            note("Added", "added", "# Added\n\nexternal addition"),
        ]);
        dialog.refresh_entries_from_notes();
        assert_eq!(
            dialog.entries.len(),
            1,
            "a new-note sentinel keeps its old index"
        );
        assert_eq!(dialog.edit_idx, Some(1));
        assert_eq!(dialog.text, "unsaved draft");
        dialog.edit_idx = None; // cancel; the next browse frame may publish the pending cache.
        dialog.text.clear();

        dialog.test_fail_next_snapshot = true;
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.4);
        assert_eq!(dialog.entries.len(), 1);
        assert_eq!(dialog.entries[0].content, "# Original\n\ncommitted body");
        assert_eq!(dialog.index, original_index);
        assert_eq!(dialog.row_metadata, original_metadata);
        assert_eq!(dialog.filtered_indices, original_projection);
        assert_eq!(dialog.metadata_key, original_metadata_key);
        assert_eq!(dialog.projection_key, original_projection_key);

        render_notes_frame(&ctx, &mut dialog, &mut app, 3.0);
        assert_eq!(
            dialog.entries.len(),
            1,
            "failed snapshots use a bounded retry"
        );
        assert_eq!(dialog.test_note_snapshot_calls, 4);

        let _newer_cache = crate::plugins::note::publish_note_cache_for_test(vec![
            note(
                "Original",
                "original",
                "# Original\n\nnewest committed body",
            ),
            note("Added", "added", "# Added\n\nexternal addition"),
        ]);
        dialog.test_race_after_candidate = true;
        render_notes_frame(&ctx, &mut dialog, &mut app, 3.1);
        assert_eq!(
            dialog.entries.len(),
            1,
            "a raced candidate is not partially installed"
        );
        assert_eq!(dialog.row_metadata, original_metadata);
        assert_eq!(dialog.metadata_key, original_metadata_key);
        assert_eq!(dialog.projection_key, original_projection_key);

        render_notes_frame(&ctx, &mut dialog, &mut app, 4.2);
        assert_eq!(dialog.entries.len(), 2);
        assert_eq!(
            dialog.entries[0].content,
            "# Original\n\nnewest committed body"
        );
        assert_eq!(dialog.entries[1].slug, "added");
        assert_eq!(dialog.test_metadata_rebuilds, 3);

        drop(dialog);
        drop(app);
        drop(_newer_cache);
        drop(_external_draft);
        drop(_fixture_cache);
        drop(_outer_cache);
        drop(workspace);
    }

    #[test]
    fn quick_notes_publishes_a_successful_edit_after_the_editor_closes() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let _outer_cache = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        let mut alpha = note("Alpha", "alpha", "# Alpha\n\noriginal");
        save_note(&mut alpha, true).unwrap();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let mut dialog = NotesDialog::default();
        dialog.open();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);

        dialog.open_edit(0);
        let edited_content = "# Alpha\n\ncommitted draft".to_string();
        dialog.test_fail_next_snapshot = true;
        assert!(dialog.save_note_draft(
            PendingNoteSave::Existing {
                identity: "alpha".into(),
                content: edited_content.clone(),
            },
            &mut app,
        ));
        assert_eq!(dialog.entries[0].content, "# Alpha\n\noriginal");
        assert_eq!(dialog.edit_idx, Some(0));

        dialog.edit_idx = None;
        dialog.text.clear();
        dialog.refresh_entries_from_notes();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.1);
        assert_eq!(
            dialog.edit_idx, None,
            "a committed disk write closes the editor"
        );
        assert_eq!(dialog.entries[0].content, "# Alpha\n\noriginal");
        assert_eq!(load_notes().unwrap()[0].content, edited_content);
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.5);
        assert_eq!(dialog.entries[0].content, "# Alpha\n\noriginal");
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.2);
        assert_eq!(dialog.entries[0].content, edited_content);
        assert_eq!(load_notes().unwrap()[0].content, edited_content);

        drop(dialog);
        drop(app);
        drop(_outer_cache);
        drop(workspace);
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
