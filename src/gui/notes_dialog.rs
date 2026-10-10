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

#[derive(Clone, Debug, PartialEq)]
struct NotesGeometryKey {
    entries_generation: u64,
    metadata_generation: u64,
    projection_generation: u64,
    viewport_width: u32,
    pixels_per_point: u32,
    title_font: egui::FontId,
    small_font: egui::FontId,
    wrap: Option<bool>,
    spacing_x: u32,
    spacing_y: u32,
    interact_width: u32,
    interact_height: u32,
}

#[derive(Clone, Debug)]
struct NoteRowGeometry {
    original_index: usize,
    identity: String,
    top: f32,
    header_height: f32,
    header_width: f32,
    body_width: f32,
    preview_height: Option<f32>,
    body_height: f32,
    separator_top: f32,
    visual_bottom: f32,
    width_before: f32,
    width_after: f32,
}

struct NotesGeometry {
    key: NotesGeometryKey,
    rows: Vec<NoteRowGeometry>,
    row_by_identity: std::collections::HashMap<String, usize>,
    content_size: egui::Vec2,
    font_atlas: std::sync::Arc<egui::mutex::Mutex<egui::epaint::TextureAtlas>>,
    #[cfg(test)]
    measured_rows: usize,
    #[cfg(test)]
    rebuild_nanos: u128,
}

impl NotesGeometry {
    fn matches(
        &self,
        key: &NotesGeometryKey,
        font_atlas: &std::sync::Arc<egui::mutex::Mutex<egui::epaint::TextureAtlas>>,
    ) -> bool {
        self.key == *key && std::sync::Arc::ptr_eq(&self.font_atlas, font_atlas)
    }

    fn anchor_at(&self, offset_y: f32) -> Option<(&str, f32)> {
        let row = self
            .rows
            .iter()
            .find(|row| row.visual_bottom > offset_y)
            .or_else(|| self.rows.last())?;
        Some((&row.identity, (offset_y - row.top).max(0.0)))
    }
}

#[derive(Clone)]
struct NoteMenuOwner {
    identity: String,
    original_index: usize,
    response: egui::Response,
}

fn geometry_key(
    entries_generation: u64,
    metadata_generation: u64,
    projection_generation: u64,
    viewport_width: f32,
    pixels_per_point: f32,
    style: &egui::Style,
) -> NotesGeometryKey {
    let spacing = &style.spacing;
    NotesGeometryKey {
        entries_generation,
        metadata_generation,
        projection_generation,
        viewport_width: viewport_width.to_bits(),
        pixels_per_point: pixels_per_point.to_bits(),
        title_font: style.override_text_style.as_ref().map_or_else(
            || egui::FontSelection::Default.resolve(style),
            |text_style| text_style.resolve(style),
        ),
        small_font: egui::TextStyle::Small.resolve(style),
        wrap: style.wrap,
        spacing_x: spacing.item_spacing.x.to_bits(),
        spacing_y: spacing.item_spacing.y.to_bits(),
        interact_width: spacing.interact_size.x.to_bits(),
        interact_height: spacing.interact_size.y.to_bits(),
    }
}

fn galley_size(
    ctx: &egui::Context,
    style: &egui::Style,
    text: egui::WidgetText,
    wrap: bool,
    width: f32,
    valign: egui::Align,
) -> egui::Vec2 {
    text.into_galley_impl(
        ctx,
        style,
        wrap,
        width,
        egui::FontSelection::Default,
        valign,
    )
    .size()
}

fn measure_notes_geometry(
    ctx: &egui::Context,
    style: &egui::Style,
    viewport_width: f32,
    entries: &[Note],
    metadata: &[NoteRowMetadata],
    filtered_indices: &[usize],
    entries_generation: u64,
    metadata_generation: u64,
    projection_generation: u64,
) -> NotesGeometry {
    #[cfg(test)]
    let started = std::time::Instant::now();
    let font_atlas = ctx.fonts(|fonts| fonts.texture_atlas());
    let key = geometry_key(
        entries_generation,
        metadata_generation,
        projection_generation,
        viewport_width,
        ctx.pixels_per_point(),
        style,
    );
    let spacing_x = style.spacing.item_spacing.x;
    let spacing_y = style.spacing.item_spacing.y;
    let header_wrap = style.wrap.unwrap_or(false);
    let preview_wrap = style.wrap.unwrap_or(true);
    let mut prefix_width = viewport_width.max(0.0);
    let mut top = 0.0;
    let mut content_height = 0.0;
    let mut rows = Vec::with_capacity(filtered_indices.len());
    let mut row_by_identity = std::collections::HashMap::with_capacity(filtered_indices.len());

    for &original_index in filtered_indices {
        let (Some(entry), Some(metadata)) = (
            entries.get(original_index),
            metadata
                .get(original_index)
                .filter(|metadata| metadata.original_index == original_index),
        ) else {
            continue;
        };

        let width_before = prefix_width;
        let title_size = galley_size(
            ctx,
            style,
            egui::RichText::new(&metadata.display_title).strong().into(),
            header_wrap,
            width_before,
            egui::Align::Center,
        );
        let meta_width = (width_before - title_size.x - spacing_x).max(0.0);
        let meta_size = galley_size(
            ctx,
            style,
            egui::RichText::new(&metadata.meta).small().into(),
            header_wrap,
            meta_width,
            egui::Align::Center,
        );
        let header_width = title_size.x + spacing_x + meta_size.x;
        let header_height = style
            .spacing
            .interact_size
            .y
            .max(title_size.y)
            .max(meta_size.y);
        prefix_width = prefix_width.max(header_width);

        let preview_size = if metadata.preview.is_empty() {
            None
        } else {
            Some(galley_size(
                ctx,
                style,
                egui::RichText::new(&metadata.preview).small().into(),
                preview_wrap,
                prefix_width,
                egui::Align::Min,
            ))
        };
        let body_height = if let Some(preview_size) = preview_size {
            header_height + spacing_y + preview_size.y
        } else {
            header_height
        };
        let body_width = preview_size.map_or(header_width, |preview_size| {
            header_width.max(preview_size.x)
        });
        if let Some(preview_size) = preview_size {
            prefix_width = prefix_width.max(preview_size.x);
        }
        let separator_top = top + body_height + spacing_y;
        content_height = separator_top + 6.0;
        let identity = if entry.slug.is_empty() {
            format!("#unsaved:{original_index}")
        } else {
            entry.slug.clone()
        };
        row_by_identity.insert(identity.clone(), rows.len());
        rows.push(NoteRowGeometry {
            original_index,
            identity,
            top,
            header_height,
            header_width,
            body_width,
            preview_height: preview_size.map(|size| size.y),
            body_height,
            separator_top,
            visual_bottom: content_height,
            width_before,
            width_after: prefix_width,
        });
        top = content_height + spacing_y;
    }

    #[cfg(test)]
    let rebuild_nanos = started.elapsed().as_nanos();
    NotesGeometry {
        key,
        rows,
        row_by_identity,
        content_size: if filtered_indices.is_empty() {
            egui::Vec2::ZERO
        } else {
            egui::vec2(prefix_width, content_height)
        },
        font_atlas,
        #[cfg(test)]
        measured_rows: filtered_indices.len(),
        #[cfg(test)]
        rebuild_nanos,
    }
}

fn remap_scroll_anchor(
    old: &NotesGeometry,
    new: &NotesGeometry,
    offset_y: f32,
) -> Option<(f32, f32)> {
    let (anchor_identity, intra_row_offset) = old.anchor_at(offset_y)?;
    let anchor_position = old.row_by_identity.get(anchor_identity).copied()?;

    for distance in 0..old.rows.len() {
        let candidates = [
            (anchor_position.checked_add(distance), distance == 0),
            (anchor_position.checked_sub(distance), false),
        ];
        for (candidate, is_anchor) in candidates {
            let Some(candidate) = candidate.filter(|index| *index < old.rows.len()) else {
                continue;
            };
            let old_row = &old.rows[candidate];
            let Some(&new_position) = new.row_by_identity.get(&old_row.identity) else {
                continue;
            };
            let new_row = &new.rows[new_position];
            let offset = if is_anchor {
                intra_row_offset.min(new_row.visual_bottom - new_row.top)
            } else {
                0.0
            };
            return Some((new_row.top, offset));
        }
    }
    None
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
    projection_generation: u64,
    metadata_generation: u64,
    row_metadata: Vec<NoteRowMetadata>,
    metadata_key: Option<MetadataKey>,
    filtered_indices: Vec<usize>,
    projection_key: Option<ProjectionKey>,
    notes_geometry: Option<NotesGeometry>,
    menu_owner: Option<NoteMenuOwner>,
    refresh_retry: Option<RefreshRetry>,
    edit_idx: Option<usize>,
    text: String,
    search: String,
    template_manager: TemplateManagerState,
    #[cfg(test)]
    last_rendered_indices: Vec<usize>,
    #[cfg(test)]
    last_rendered_row_rects: Vec<(usize, RenderedNoteRowRects)>,
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
    #[cfg(test)]
    test_geometry_rebuilds: u64,
    #[cfg(test)]
    test_geometry_rows_measured: u64,
    #[cfg(test)]
    test_last_geometry_rebuild_nanos: u128,
    #[cfg(test)]
    test_scroll_area_id: Option<egui::Id>,
    #[cfg(test)]
    test_scroll_area_offset: egui::Vec2,
    #[cfg(test)]
    test_scroll_area_inner_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_scroll_area_content_size: egui::Vec2,
    #[cfg(test)]
    test_content_origin: egui::Pos2,
    #[cfg(test)]
    test_menu_edit_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_menu_remove_rect: Option<egui::Rect>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct RenderedNoteRowRects {
    outer: egui::Rect,
    header: egui::Rect,
    title: egui::Rect,
    meta: egui::Rect,
    preview: Option<egui::Rect>,
    separator: egui::Rect,
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
        self.metadata_generation = self.metadata_generation.wrapping_add(1).max(1);
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
            self.metadata_generation = self.metadata_generation.wrapping_add(1).max(1);
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
        if self.refresh_projection() {
            #[cfg(test)]
            {
                self.test_projection_rebuilds = self.test_projection_rebuilds.saturating_add(1);
            }
        }
    }

    fn refresh_projection(&mut self) -> bool {
        let changed = update_filtered_projection(
            &self.search,
            &self.index,
            self.entries_generation,
            &mut self.filtered_indices,
            &mut self.projection_key,
        );
        if changed {
            self.projection_generation = self.projection_generation.wrapping_add(1).max(1);
        }
        changed
    }

    fn rebuild_index(&mut self) {
        self.index = build_search_index(&self.entries);
        self.entries_generation = self.entries_generation.wrapping_add(1).max(1);
        self.row_metadata.clear();
        self.metadata_key = None;
        self.filtered_indices.clear();
        self.projection_key = None;
    }

    fn ensure_notes_geometry(
        &mut self,
        ctx: &egui::Context,
        style: &egui::Style,
        viewport_width: f32,
    ) -> Option<NotesGeometry> {
        let key = geometry_key(
            self.entries_generation,
            self.metadata_generation,
            self.projection_generation,
            viewport_width,
            ctx.pixels_per_point(),
            style,
        );
        let font_atlas = ctx.fonts(|fonts| fonts.texture_atlas());
        if self
            .notes_geometry
            .as_ref()
            .is_some_and(|geometry| geometry.matches(&key, &font_atlas))
        {
            return None;
        }
        let geometry = measure_notes_geometry(
            ctx,
            style,
            viewport_width,
            &self.entries,
            &self.row_metadata,
            &self.filtered_indices,
            self.entries_generation,
            self.metadata_generation,
            self.projection_generation,
        );
        #[cfg(test)]
        {
            self.test_geometry_rebuilds = self.test_geometry_rebuilds.saturating_add(1);
            self.test_geometry_rows_measured = self
                .test_geometry_rows_measured
                .saturating_add(geometry.measured_rows.try_into().unwrap_or(u64::MAX));
            self.test_last_geometry_rebuild_nanos = geometry.rebuild_nanos;
        }
        self.notes_geometry.replace(geometry)
    }

    fn close_menu_owner(&mut self) {
        if let Some(owner) = self.menu_owner.take()
            && owner.response.context_menu_opened()
        {
            owner.response.context_menu(|ui| ui.close_menu());
        }
    }

    fn reconcile_menu_owner(&mut self) {
        let Some(owner) = self.menu_owner.as_ref() else {
            return;
        };
        if !owner.response.context_menu_opened() {
            self.menu_owner = None;
            return;
        }
        let row = self.notes_geometry.as_ref().and_then(|geometry| {
            geometry
                .row_by_identity
                .get(&owner.identity)
                .and_then(|position| geometry.rows.get(*position))
        });
        if let Some(row) = row {
            if let Some(owner) = self.menu_owner.as_mut() {
                owner.original_index = row.original_index;
            }
        } else {
            self.close_menu_owner();
        }
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
        #[cfg(test)]
        self.last_rendered_row_rects.clear();
        #[cfg(test)]
        {
            self.test_menu_edit_rect = None;
            self.test_menu_remove_rect = None;
        }
        if !self.open {
            self.close_menu_owner();
            return;
        }
        if self.edit_idx.is_some() {
            self.close_menu_owner();
        }
        self.maybe_refresh_derived(ctx, &app.note_settings);
        let mut close = false;
        let mut save_edit = false;
        let mut note_to_save: Option<PendingNoteSave> = None;
        let mut rebuild_idx = false;
        let mut refresh_entries = false;
        let mut wrap_links_slug: Option<String> = None;
        let mut window_open = self.open;
        egui::Window::new("Quick Notes")
            .open(&mut window_open)
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
                        self.projection_generation =
                            self.projection_generation.wrapping_add(1).max(1);
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
                    let output = egui::ScrollArea::both()
                        .max_height(area_height)
                        .show_viewport(ui, |ui, viewport| {
                            let old_geometry = self.ensure_notes_geometry(
                                ctx,
                                ui.style(),
                                viewport.width(),
                            );
                            self.reconcile_menu_owner();
                            let content_origin = ui.min_rect().min;
                            let Some(geometry) = self.notes_geometry.as_ref() else {
                                return;
                            };
                            #[cfg(test)]
                            {
                                self.test_content_origin = content_origin;
                            }

                            if let Some(old_geometry) = old_geometry.as_ref()
                                && let Some((top, intra_row_offset)) = remap_scroll_anchor(
                                    old_geometry,
                                    geometry,
                                    viewport.min.y,
                                )
                            {
                                let spacing = ui.spacing().item_spacing;
                                let target = egui::Rect::from_min_size(
                                    content_origin
                                        + egui::vec2(
                                            viewport.min.x + spacing.x,
                                            top + intra_row_offset + spacing.y,
                                        ),
                                    egui::Vec2::splat(1.0),
                                );
                                ui.scroll_to_rect(target, Some(egui::Align::Min));
                            }

                            let first_visible = geometry
                                .rows
                                .partition_point(|row| row.visual_bottom <= viewport.min.y)
                                .saturating_sub(2);
                            let after_visible = geometry
                                .rows
                                .partition_point(|row| row.top <= viewport.max.y)
                                .saturating_add(2)
                                .min(geometry.rows.len());
                            let mut render_positions =
                                (first_visible..after_visible).collect::<Vec<_>>();
                            if let Some(owner) = self
                                .menu_owner
                                .as_ref()
                                .filter(|owner| owner.response.context_menu_opened())
                                && let Some(&position) =
                                    geometry.row_by_identity.get(&owner.identity)
                                && !render_positions.contains(&position)
                            {
                                render_positions.push(position);
                            }
                            render_positions.sort_unstable();
                            render_positions.dedup();
                            let content_size = geometry.content_size;
                            let render_rows = render_positions
                                .into_iter()
                                .filter_map(|position| geometry.rows.get(position).cloned())
                                .collect::<Vec<_>>();

                            for row_geometry in render_rows {
                                let idx = row_geometry.original_index;
                                let identity = row_geometry.identity.as_str();
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
                                let row_rect = egui::Rect::from_min_size(
                                    content_origin + egui::vec2(0.0, row_geometry.top),
                                    egui::vec2(
                                        row_geometry.width_before,
                                        row_geometry.body_height,
                                    ),
                                );
                                let separator_rect = egui::Rect::from_min_size(
                                    content_origin
                                        + egui::vec2(0.0, row_geometry.separator_top),
                                    egui::vec2(row_geometry.width_after, 6.0),
                                );
                                let mut open_menu_response = None;
                                let _allocated_row = ui.push_id(("quick-notes-row", identity), |ui| {
                                    ui.allocate_ui_at_rect(row_rect, |ui| {
                                        ui.vertical(|ui| {
                                            let preview = row.preview.as_str();
                                            let header = ui.horizontal(|ui| {
                                                let title = ui.strong(&row.display_title);
                                                let meta = ui.small(&row.meta);
                                                (title.rect, meta.rect)
                                            });
                                            let header_response = header.response;
                                            let (title_rect, meta_rect) = header.inner;
                                            let response = header_response.clone().on_hover_ui(|ui| {
                                                if preview.is_empty() {
                                                    ui.label(&entry.content);
                                                } else {
                                                    ui.label(preview);
                                                }
                                            });
                                            let preview_rect = if preview.is_empty() {
                                                None
                                            } else {
                                                Some(ui.small(preview).rect)
                                            };
                                            let idx_copy = idx;
                                            response.clone().context_menu(|ui| {
                                                    if ui.button("Open").clicked() {
                                                        app.open_note_panel(&entry.slug, None);
                                                        ui.close_menu();
                                                    }
                                                    let edit_button = ui.button("Edit");
                                                    #[cfg(test)]
                                                    {
                                                        self.test_menu_edit_rect =
                                                            Some(edit_button.rect);
                                                    }
                                                    if edit_button.clicked() {
                                                        self.edit_idx = Some(idx_copy);
                                                        self.text = entry.content.clone();
                                                        ui.close_menu();
                                                    }
                                                    if ui.button("Open externally").clicked() {
                                                        if let Err(e) = open::that(&entry.path) {
                                                            app.report_error_message(
                                                                "ui operation",
                                                                format!(
                                                                    "Failed to open note externally: {e}"
                                                                ),
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
                                                        if let Err(e) = crate::actions::clipboard::set_text(&entry.slug) {
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
                                                    let remove_button = ui.button("Remove Note");
                                                    #[cfg(test)]
                                                    {
                                                        self.test_menu_remove_rect =
                                                            Some(remove_button.rect);
                                                    }
                                                    if remove_button.clicked() {
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
                                                    for todo in load_todos_or_last_good(TODO_FILE)
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
                                            if response.context_menu_opened() {
                                                open_menu_response = Some(response.clone());
                                            }
                                            (
                                                header_response.rect,
                                                title_rect,
                                                meta_rect,
                                                preview_rect,
                                            )
                                        })
                                    })
                                });
                                #[cfg(test)]
                                let actual_row = _allocated_row.inner.inner;
                                #[cfg(test)]
                                let actual_outer_rect = actual_row.response.rect;
                                #[cfg(test)]
                                let (
                                    actual_header_rect,
                                    actual_title_rect,
                                    actual_meta_rect,
                                    actual_preview_rect,
                                ) = actual_row.inner;
                                let _separator_response = ui
                                    .push_id(("quick-notes-separator", identity), |ui| {
                                        ui.allocate_ui_at_rect(separator_rect, |ui| ui.separator())
                                    })
                                    .inner;
                                #[cfg(test)]
                                let separator_response = _separator_response.inner;
                                if let Some(response) = open_menu_response {
                                    if let Some(owner) = self
                                        .menu_owner
                                        .as_mut()
                                        .filter(|owner| owner.identity == identity)
                                    {
                                        owner.original_index = idx;
                                        owner.response = response;
                                    } else {
                                        self.menu_owner = Some(NoteMenuOwner {
                                            identity: identity.to_owned(),
                                            original_index: idx,
                                            response,
                                        });
                                    }
                                }
                                rows_timer.add_work_units(1);
                                #[cfg(test)]
                                {
                                    self.last_rendered_indices.push(idx);
                                    self.last_rendered_row_rects.push((
                                        idx,
                                        RenderedNoteRowRects {
                                            outer: actual_outer_rect,
                                            header: actual_header_rect,
                                            title: actual_title_rect,
                                            meta: actual_meta_rect,
                                            preview: actual_preview_rect,
                                            separator: separator_response.rect,
                                        },
                                    ));
                                }
                            }
                            ui.expand_to_include_rect(egui::Rect::from_min_size(
                                content_origin,
                                content_size,
                            ));
                        });
                    #[cfg(test)]
                    {
                        self.test_scroll_area_id = Some(output.id);
                        self.test_scroll_area_offset = output.state.offset;
                        self.test_scroll_area_inner_rect = Some(output.inner_rect);
                        self.test_scroll_area_content_size = output.content_size;
                    }
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
            window_open = false;
        }
        if !window_open {
            self.close_menu_owner();
        }
        self.open = window_open;
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
        NotesDialog, PendingNoteSave, RenderedNoteRowRects, checkbox_count, format_note_timestamp,
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

    #[derive(Debug)]
    struct EagerNoteRowRects {
        outer: egui::Rect,
        header: egui::Rect,
        preview: Option<egui::Rect>,
        separator: egui::Rect,
    }

    #[derive(Debug)]
    struct EagerNotesGeometry {
        rows: Vec<EagerNoteRowRects>,
        content_size: egui::Vec2,
        viewport: egui::Rect,
        content_origin: egui::Pos2,
        estimated_rows: Vec<super::NoteRowGeometry>,
        estimated_content_size: egui::Vec2,
    }

    // Independent copy of the pre-virtualization row composition. Keep this
    // actual-widget oracle separate from the cached geometry estimator so the
    // virtualization tests can catch egui layout changes and estimate drift.
    fn eager_notes_geometry(
        ctx: &egui::Context,
        entries: &[Note],
        screen_width: f32,
    ) -> EagerNotesGeometry {
        let metadata = super::build_row_metadata(entries, false, false).unwrap();
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(screen_width, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let output = egui::ScrollArea::both().max_height(520.0).show_viewport(
                        ui,
                        |ui, viewport| {
                            let content_origin = ui.min_rect().min;
                            let all_indices = (0..entries.len()).collect::<Vec<_>>();
                            let estimated = super::measure_notes_geometry(
                                ctx,
                                ui.style(),
                                viewport.width(),
                                entries,
                                &metadata,
                                &all_indices,
                                1,
                                1,
                                1,
                            );
                            let mut rows = Vec::with_capacity(entries.len());
                            for row in &metadata {
                                let outer = ui.vertical(|ui| {
                                    let header = ui
                                        .horizontal(|ui| {
                                            ui.strong(&row.display_title);
                                            ui.small(&row.meta);
                                        })
                                        .response
                                        .rect;
                                    let preview = if row.preview.is_empty() {
                                        None
                                    } else {
                                        Some(ui.small(&row.preview).rect)
                                    };
                                    (header, preview)
                                });
                                let separator = ui.separator().rect;
                                rows.push(EagerNoteRowRects {
                                    outer: outer.response.rect,
                                    header: outer.inner.0,
                                    preview: outer.inner.1,
                                    separator,
                                });
                            }
                            (rows, content_origin, estimated)
                        },
                    );
                    result = Some(EagerNotesGeometry {
                        rows: output.inner.0,
                        content_size: output.content_size,
                        viewport: output.inner_rect,
                        content_origin: output.inner.1,
                        estimated_rows: output.inner.2.rows,
                        estimated_content_size: output.inner.2.content_size,
                    });
                });
            },
        );
        result.expect("the eager geometry oracle runs its central panel")
    }

    #[test]
    fn quick_notes_eager_geometry_oracle_records_real_variable_rows() {
        let entries = vec![
            note("Short title", "short", &"short preview words ".repeat(30)),
            note(
                &"W".repeat(120),
                "wide",
                &format!("{}\nsecond line", "wide preview words ".repeat(30)),
            ),
            note("", "blank", ""),
            note(&"Wrapped\n".repeat(20), "multiline", ""),
        ];
        let ctx = egui::Context::default();
        let geometry = eager_notes_geometry(&ctx, &entries, 420.0);

        assert_eq!(geometry.rows.len(), entries.len());
        assert!(geometry.rows[1].outer.width() > geometry.viewport.width());
        assert!(geometry.rows[0].preview.unwrap().height() > 10.0);
        assert!(geometry.rows[2].outer.min.y > geometry.rows[1].separator.min.y);
        assert!(geometry.rows[2].preview.is_none());
        assert!(geometry.rows[3].outer.height() > geometry.rows[0].outer.height());
        assert!(
            geometry.content_size.x >= geometry.rows[1].outer.right() - geometry.content_origin.x
        );
        assert!(
            geometry.content_size.y
                >= geometry.rows[3].separator.bottom() - geometry.content_origin.y
        );
        assert_eq!(geometry.estimated_rows.len(), geometry.rows.len());
        for (actual, estimate) in geometry.rows.iter().zip(&geometry.estimated_rows) {
            assert!(
                (actual.outer.min.y - geometry.content_origin.y - estimate.top).abs() < 0.2,
                "row top differs from actual egui layout: {actual:?} vs {estimate:?}"
            );
            assert!(
                (actual.outer.height() - estimate.body_height).abs() < 0.2,
                "row height differs from actual egui layout: {actual:?} vs {estimate:?}"
            );
            assert!(
                (actual.outer.width() - estimate.body_width).abs() < 0.2,
                "row body width differs from actual egui layout: {actual:?} vs {estimate:?}"
            );
            assert!((actual.header.width() - estimate.header_width).abs() < 0.2);
            assert!(
                (actual.separator.min.y - geometry.content_origin.y - estimate.separator_top).abs()
                    < 0.2,
                "separator differs from actual egui layout: {actual:?} vs {estimate:?}"
            );
            assert!((actual.separator.width() - estimate.width_after).abs() < 0.2);
            match (actual.preview, estimate.preview_height) {
                (Some(preview), Some(height)) => assert!((preview.height() - height).abs() < 0.2),
                (None, None) => {}
                mismatch => panic!("actual and estimated preview presence differ: {mismatch:?}"),
            }
        }
        assert!((geometry.content_size.x - geometry.estimated_content_size.x).abs() < 0.2);
        assert!((geometry.content_size.y - geometry.estimated_content_size.y).abs() < 0.2);

        let wrapped_ctx = egui::Context::default();
        let mut style = (*wrapped_ctx.style()).clone();
        style.wrap = Some(true);
        wrapped_ctx.set_style(style);
        let wrapped = eager_notes_geometry(&wrapped_ctx, &entries, 280.0);
        assert!(wrapped.rows[1].outer.height() > geometry.rows[1].outer.height());
        assert!(
            wrapped.rows[1].preview.unwrap().height() > geometry.rows[1].preview.unwrap().height()
        );
        for (actual, estimate) in wrapped.rows.iter().zip(&wrapped.estimated_rows) {
            assert!((actual.outer.min.y - wrapped.content_origin.y - estimate.top).abs() < 0.2);
            assert!((actual.outer.height() - estimate.body_height).abs() < 0.2);
            assert!((actual.outer.width() - estimate.body_width).abs() < 0.2);
            assert!((actual.header.width() - estimate.header_width).abs() < 0.2);
            assert!(
                (actual.separator.min.y - wrapped.content_origin.y - estimate.separator_top).abs()
                    < 0.2
            );
            match (actual.preview, estimate.preview_height) {
                (Some(preview), Some(height)) => assert!((preview.height() - height).abs() < 0.2),
                (None, None) => {}
                mismatch => panic!("actual and estimated wrapped preview differ: {mismatch:?}"),
            }
        }
        assert!((wrapped.content_size.x - wrapped.estimated_content_size.x).abs() < 0.2);
        assert!((wrapped.content_size.y - wrapped.estimated_content_size.y).abs() < 0.2);
    }

    #[test]
    fn quick_notes_browsing_virtualizes_rows_and_reuses_geometry() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let notes = (0..240)
            .map(|index| {
                let content = if index == 173 {
                    "# Task\n\n- [ ] keep original index".to_owned()
                } else if index == 0 {
                    "A long prefix preview which remains bounded and can wrap when Quick Notes is narrow. ".repeat(8)
                } else {
                    "A short body used to exercise bounded viewport rows.".to_owned()
                };
                let title = match index {
                    0 => "W".repeat(120),
                    1 => "Multiline\n".repeat(8),
                    _ => format!("Note {index:03}"),
                };
                note(
                    &title,
                    &format!("browse-{index:03}"),
                    &content,
                )
            })
            .collect::<Vec<_>>();
        let _cache = crate::plugins::note::publish_note_cache_for_test(notes.clone());
        let mut dialog = NotesDialog::default();
        dialog.open();

        for frame in 0..5 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.0 + frame as f64 / 60.0);
        }
        assert_eq!(dialog.filtered_indices.len(), 240);
        assert_eq!(dialog.notes_geometry.as_ref().unwrap().rows.len(), 240);
        assert!(
            dialog.notes_geometry.as_ref().unwrap().rows[0].width_after
                > dialog.test_scroll_area_inner_rect.unwrap().width(),
            "an offscreen unwrapped title contributes to the full horizontal extent"
        );
        assert_eq!(
            dialog.notes_geometry.as_ref().unwrap().rows[1].width_before,
            dialog.notes_geometry.as_ref().unwrap().rows[0].width_after,
            "the wide first header expands later rows in the same ordered layout"
        );
        assert!(
            dialog.last_rendered_indices.len() < dialog.filtered_indices.len(),
            "only visible rows, overscan, and an open menu owner should build widgets"
        );
        assert!(
            dialog
                .last_rendered_indices
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        let geometry_rebuilds = dialog.test_geometry_rebuilds;
        let geometry_rows_measured = dialog.test_geometry_rows_measured;
        for frame in 5..9 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.0 + frame as f64 / 60.0);
        }
        assert_eq!(dialog.test_geometry_rebuilds, geometry_rebuilds);
        assert_eq!(dialog.test_geometry_rows_measured, geometry_rows_measured);
        assert!(dialog.last_rendered_indices.len() < 32);
        let geometry = dialog.notes_geometry.as_ref().unwrap();
        for (index, actual) in &dialog.last_rendered_row_rects {
            let row = geometry
                .rows
                .iter()
                .find(|row| row.original_index == *index)
                .expect("every rendered row has cached geometry");
            let expected_top = dialog.test_content_origin.y + row.top;
            assert!((actual.outer.min.x - dialog.test_content_origin.x).abs() < 0.5);
            assert!((actual.outer.min.y - expected_top).abs() < 0.5);
            assert!((actual.outer.height() - row.body_height).abs() < 0.5);
            assert!((actual.outer.width() - row.body_width).abs() < 0.5);
            assert!((actual.header.min.y - expected_top).abs() < 0.5);
            assert!((actual.header.height() - row.header_height).abs() < 0.5);
            assert!((actual.header.width() - row.header_width).abs() < 0.5);
            match (actual.preview, row.preview_height) {
                (Some(preview), Some(height)) => {
                    let expected_preview_top =
                        expected_top + row.header_height + ctx.style().spacing.item_spacing.y;
                    assert!((preview.min.y - expected_preview_top).abs() < 0.5);
                    assert!((preview.height() - height).abs() < 0.5);
                }
                (None, None) => {}
                mismatch => panic!("actual and cached preview presence differ: {mismatch:?}"),
            }
            assert!(
                (actual.separator.min.y - (dialog.test_content_origin.y + row.separator_top)).abs()
                    < 0.5
            );
            assert!((actual.separator.width() - row.width_after).abs() < 0.5);
            assert!((actual.separator.height() - 6.0).abs() < 0.5);
        }
        assert!(
            (dialog.test_scroll_area_content_size.x - geometry.content_size.x).abs() < 1.0,
            "full horizontal scroll extent includes offscreen wide rows"
        );
        assert!(
            (dialog.test_scroll_area_content_size.y - geometry.content_size.y).abs() < 1.0,
            "full vertical scroll extent includes unbuilt rows"
        );

        for (target, time) in [(0, 1.2), (120, 1.22), (239, 1.24)] {
            let geometry = dialog.notes_geometry.as_ref().unwrap();
            let inner = dialog.test_scroll_area_inner_rect.unwrap();
            let max_y = (dialog.test_scroll_area_content_size.y - inner.height()).max(0.0);
            let y = if target == 239 {
                max_y
            } else {
                geometry.rows[target].top
            };
            let max_x = (dialog.test_scroll_area_content_size.x - inner.width()).max(0.0);
            let x = if target == 120 { max_x.min(24.0) } else { 0.0 };
            set_notes_scroll_offset(&ctx, &dialog, egui::vec2(x, y));
            render_notes_frame(&ctx, &mut dialog, &mut app, time);
            let inner = dialog.test_scroll_area_inner_rect.unwrap();
            let visible = rendered_note_rect(&dialog, target);
            assert!(
                visible.header.intersects(inner),
                "target {target} header should be visible at first/middle/bottom scroll offsets"
            );
            assert!(dialog.last_rendered_indices.contains(&target));
            if target == 120 {
                assert!(dialog.test_scroll_area_offset.x > 0.0);
            }
            if target == 239 {
                assert!(
                    (dialog.test_scroll_area_offset.y - max_y).abs() < 1.0,
                    "the last row scrolls to the actual vertical maximum"
                );
                assert!(
                    visible.separator.bottom() <= inner.bottom() + 1.0,
                    "the final separator is reachable at the bottom scroll offset"
                );
            }
        }

        let geometry = dialog.notes_geometry.as_ref().unwrap();
        let inner = dialog.test_scroll_area_inner_rect.unwrap();
        let max_x = (dialog.test_scroll_area_content_size.x - inner.width()).max(0.0);
        assert!(
            max_x > 24.0,
            "wide offscreen title creates horizontal scroll extent"
        );
        set_notes_scroll_offset(
            &ctx,
            &dialog,
            egui::vec2(24.0, geometry.rows[120].top + 2.0),
        );
        for frame in 0..24 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.25 + frame as f64 / 60.0);
        }
        assert!(dialog.test_scroll_area_offset.x > 0.0);
        let saved_anchor_offset = dialog.test_scroll_area_offset.y;
        let saved_horizontal_offset = dialog.test_scroll_area_offset.x;
        let (anchor_identity, intra_row) = dialog
            .notes_geometry
            .as_ref()
            .unwrap()
            .anchor_at(saved_anchor_offset)
            .expect("middle viewport has an anchor note");
        let anchor_identity = anchor_identity.to_owned();
        let old_notes = notes.clone();
        let mut inserted_notes = old_notes.clone();
        inserted_notes.insert(0, note("Inserted before", "browse-before", "body"));
        let _inserted_cache = crate::plugins::note::publish_note_cache_for_test(inserted_notes);
        for frame in 0..30 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.65 + frame as f64 / 60.0);
        }
        let geometry = dialog.notes_geometry.as_ref().unwrap();
        let new_anchor_index = geometry.row_by_identity[&anchor_identity];
        let remapped = &geometry.rows[new_anchor_index];
        assert_eq!(remapped.original_index, 121);
        assert!((remapped.top - geometry.rows[121].top).abs() < 0.1);
        assert!(dialog.last_rendered_indices.contains(&121));
        assert!(
            (dialog.test_scroll_area_offset.x - saved_horizontal_offset).abs() < 1.0,
            "horizontal offset is preserved across insertion"
        );
        assert!(
            (dialog.test_scroll_area_offset.y - (remapped.top + intra_row)).abs() < 1.0,
            "the same note identity and intra-row position survive insertion"
        );
        assert!(
            rendered_note_rect(&dialog, 121)
                .header
                .intersects(dialog.test_scroll_area_inner_rect.unwrap())
        );
        dialog.search = "browse-173".into();
        for frame in 0..24 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 2.16 + frame as f64 / 60.0);
        }
        assert_eq!(dialog.filtered_indices, vec![174]);
        assert_eq!(dialog.last_rendered_indices, vec![174]);

        let sparse_row = rendered_note_rect(&dialog, 174);
        let sparse_inner = dialog.test_scroll_area_inner_rect.unwrap();
        assert!(
            sparse_inner.contains_rect(sparse_row.header),
            "settled sparse header is inside the viewport: header={:?}, inner={sparse_inner:?}",
            sparse_row.header
        );
        let header_pointer = header_gap_point(&sparse_row);
        assert!(sparse_inner.contains(header_pointer));
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            header_pointer,
            egui::PointerButton::Secondary,
            2.56,
        );
        assert!(
            ctx.is_context_menu_open(),
            "sparse original row opens its real menu"
        );
        let edit_rect = dialog.test_menu_edit_rect.expect("Edit button is rendered");
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            edit_rect.center(),
            egui::PointerButton::Primary,
            2.62,
        );
        assert_eq!(dialog.edit_idx, Some(174));
        assert_eq!(dialog.text, "# Task\n\n- [ ] keep original index");
        dialog.edit_idx = None;
        dialog.text.clear();
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.66);

        crate::gui::set_execute_action_hook(Some(Box::new(|_| Ok(()))));
        app.require_confirm_destructive = false;
        struct ResetExecuteHook;
        impl Drop for ResetExecuteHook {
            fn drop(&mut self) {
                crate::gui::set_execute_action_hook(None);
            }
        }
        let _reset_execute_hook = ResetExecuteHook;
        let header_pointer = header_gap_point(&rendered_note_rect(&dialog, 174));
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            header_pointer,
            egui::PointerButton::Secondary,
            2.68,
        );
        let remove_rect = dialog
            .test_menu_remove_rect
            .expect("Remove button is rendered");
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            remove_rect.center(),
            egui::PointerButton::Primary,
            2.72,
        );
        assert_eq!(
            app.test_last_activation
                .as_ref()
                .map(|(action, source)| (action.action.as_str(), *source)),
            Some((
                "note:remove:browse-173",
                crate::gui::ActivationSource::Click
            )),
            "the sparse original row dispatches its own Remove action"
        );

        let prior_metadata_generation = dialog.metadata_generation;
        let prior_geometry_rebuilds = dialog.test_geometry_rebuilds;
        app.note_settings.task_lists_enabled = !app.note_settings.task_lists_enabled;
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.78);
        assert!(dialog.metadata_generation > prior_metadata_generation);
        assert!(dialog.test_geometry_rebuilds > prior_geometry_rebuilds);

        let mut style = (*ctx.style()).clone();
        style.override_font_id = Some(egui::FontId::new(18.0, egui::FontFamily::Proportional));
        style.override_text_style = Some(egui::TextStyle::Heading);
        ctx.set_style(style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.80);
        let font_style_rebuilds = dialog.test_geometry_rebuilds;
        style.override_text_style = Some(egui::TextStyle::Monospace);
        ctx.set_style(style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.82);
        assert!(
            dialog.test_geometry_rebuilds > font_style_rebuilds,
            "effective title text style invalidates geometry even with override_font_id"
        );
        let wrap_rebuilds = dialog.test_geometry_rebuilds;
        style.wrap = Some(true);
        ctx.set_style(style);
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.84);
        assert!(dialog.test_geometry_rebuilds > wrap_rebuilds);
        let row = dialog
            .notes_geometry
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.original_index == 174)
            .unwrap();
        let actual = rendered_note_rect(&dialog, 174);
        assert!((actual.outer.height() - row.body_height).abs() < 0.5);
        assert!((actual.outer.width() - row.body_width).abs() < 0.5);
        assert!((actual.header.height() - row.header_height).abs() < 0.5);
        assert!((actual.header.width() - row.header_width).abs() < 0.5);

        let pixels_rebuilds = dialog.test_geometry_rebuilds;
        ctx.set_pixels_per_point(1.5);
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.86);
        assert!(dialog.test_geometry_rebuilds > pixels_rebuilds);

        let prior_viewport_width = dialog.notes_geometry.as_ref().unwrap().key.viewport_width;
        let prior_resize_rebuilds = dialog.test_geometry_rebuilds;
        render_notes_frame_at(&ctx, &mut dialog, &mut app, 2.88, 300.0, 640.0);
        assert!(dialog.test_geometry_rebuilds > prior_resize_rebuilds);
        assert_ne!(
            dialog.notes_geometry.as_ref().unwrap().key.viewport_width,
            prior_viewport_width,
            "the changed real window viewport width invalidates measured geometry"
        );

        dialog.search = "no-result-matches".into();
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.90);
        assert!(dialog.filtered_indices.is_empty());
        assert!(dialog.last_rendered_indices.is_empty());
        assert!(dialog.notes_geometry.as_ref().unwrap().rows.is_empty());
        assert!(dialog.test_scroll_area_offset.x.abs() < 0.5);
        assert!(dialog.test_scroll_area_offset.y.abs() < 0.5);
        dialog.search = "browse-001".into();
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.92);
        assert_eq!(dialog.filtered_indices, vec![2]);
        assert!(dialog.test_scroll_area_offset.x.abs() < 0.5);
        assert!(dialog.test_scroll_area_offset.y.abs() < 0.5);

        drop(dialog);
        drop(app);
        drop(_inserted_cache);
        drop(_cache);
        drop(workspace);
    }

    #[test]
    fn quick_notes_scroll_anchor_prefers_identity_then_nearby_survivor() {
        let ctx = egui::Context::default();
        let original = vec![
            note("Alpha", "alpha", "short"),
            note("Beta", "beta", "body"),
            note("Gamma", "gamma", "body"),
        ];
        let metadata = super::build_row_metadata(&original, false, false).unwrap();
        let indices = vec![0, 1, 2];
        let inserted = vec![
            note("Before", "before", "body"),
            original[0].clone(),
            original[1].clone(),
            original[2].clone(),
        ];
        let inserted_metadata = super::build_row_metadata(&inserted, false, false).unwrap();
        let inserted_indices = vec![0, 1, 2, 3];
        let without_beta = vec![
            inserted[0].clone(),
            inserted[1].clone(),
            inserted[3].clone(),
        ];
        let without_beta_metadata = super::build_row_metadata(&without_beta, false, false).unwrap();
        let without_beta_indices = vec![0, 1, 2];
        let mut geometries = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let old = super::measure_notes_geometry(
                ctx,
                &ctx.style(),
                320.0,
                &original,
                &metadata,
                &indices,
                1,
                1,
                1,
            );
            let new = super::measure_notes_geometry(
                ctx,
                &ctx.style(),
                320.0,
                &inserted,
                &inserted_metadata,
                &inserted_indices,
                2,
                2,
                2,
            );
            let replacement = super::measure_notes_geometry(
                ctx,
                &ctx.style(),
                320.0,
                &without_beta,
                &without_beta_metadata,
                &without_beta_indices,
                3,
                3,
                3,
            );
            geometries = Some((old, new, replacement));
        });
        let (old, new, replacement) = geometries.expect("measurements run in an egui frame");
        let beta_top = old.rows[1].top;
        let (anchor_top, intra_row) = super::remap_scroll_anchor(&old, &new, beta_top + 4.0)
            .expect("the same stable note identity survives insertion");
        assert!((anchor_top - new.rows[2].top).abs() < 0.1);
        assert!((intra_row - 4.0).abs() < 0.1);

        let (anchor_top, intra_row) =
            super::remap_scroll_anchor(&old, &replacement, beta_top + 4.0)
                .expect("a nearby old-order note survives");
        assert!((anchor_top - replacement.rows[2].top).abs() < 0.1);
        assert_eq!(intra_row, 0.0);
    }

    #[test]
    fn quick_notes_popup_owner_stays_with_identity_offscreen_then_closes_on_removal() {
        use crate::performance::workloads;

        let workspace = workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let notes = (0..80)
            .map(|index| {
                note(
                    &format!("Popup note {index:03}"),
                    &format!("popup-{index:03}"),
                    "small body",
                )
            })
            .collect::<Vec<_>>();
        let cache = crate::plugins::note::publish_note_cache_for_test(notes.clone());
        let mut dialog = NotesDialog::default();
        dialog.open();
        for frame in 0..5 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.0 + frame as f64 / 60.0);
        }

        let row_rects = rendered_note_rect(&dialog, 0);
        let header_rect = row_rects.header;
        let header_pointer = header_gap_point(&row_rects);
        assert!(
            header_rect.contains(header_pointer)
                && dialog
                    .test_scroll_area_inner_rect
                    .is_some_and(|inner| inner.contains(header_pointer)),
            "first popup owner gap is visible: header={header_rect:?}, pointer={header_pointer:?}, inner={:?}",
            dialog.test_scroll_area_inner_rect
        );
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            header_pointer,
            egui::PointerButton::Secondary,
            1.1,
        );
        assert!(
            dialog
                .menu_owner
                .as_ref()
                .is_some_and(|owner| owner.response.context_menu_opened()),
            "a real secondary click in header padding opens the row-owned context menu; header={header_rect:?}, pointer={header_pointer:?}, inner={:?}, rendered={:?}",
            dialog.test_scroll_area_inner_rect,
            dialog.last_rendered_indices
        );
        assert!(ctx.is_context_menu_open());
        assert_eq!(
            dialog
                .menu_owner
                .as_ref()
                .map(|owner| owner.identity.as_str()),
            Some("popup-000")
        );

        let far_row_top = dialog.notes_geometry.as_ref().unwrap().rows[30].top;
        set_notes_scroll_offset(&ctx, &dialog, egui::vec2(0.0, far_row_top));
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.14);
        assert!(ctx.is_context_menu_open());
        assert_eq!(dialog.menu_owner.as_ref().unwrap().original_index, 0);
        assert!(dialog.last_rendered_indices.contains(&0));
        assert!(dialog.last_rendered_indices.len() < 32);

        let mut inserted = notes.clone();
        inserted.insert(0, note("Before owner", "popup-before", "body"));
        let inserted_cache = crate::plugins::note::publish_note_cache_for_test(inserted.clone());
        for frame in 0..4 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.16 + frame as f64 / 60.0);
        }
        assert!(ctx.is_context_menu_open());
        assert_eq!(dialog.menu_owner.as_ref().unwrap().identity, "popup-000");
        assert_eq!(dialog.menu_owner.as_ref().unwrap().original_index, 1);
        assert!(dialog.last_rendered_indices.contains(&1));
        assert!(dialog.last_rendered_indices.len() < 32);

        inserted.remove(1);
        let removed_cache = crate::plugins::note::publish_note_cache_for_test(inserted);
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.24);
        assert!(dialog.menu_owner.is_none());
        assert!(
            !ctx.is_context_menu_open(),
            "removed owner closes its own menu"
        );

        drop(dialog);
        drop(app);
        drop(removed_cache);
        drop(inserted_cache);
        drop(cache);
        drop(workspace);
    }

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
        render_notes_frame_at(ctx, dialog, app, time, 960.0, 640.0);
    }

    fn render_notes_frame_at(
        ctx: &egui::Context,
        dialog: &mut NotesDialog,
        app: &mut LauncherApp,
        time: f64,
        width: f32,
        height: f32,
    ) {
        render_notes_frame_with_events(ctx, dialog, app, time, width, height, Vec::new());
    }

    fn render_notes_frame_with_events(
        ctx: &egui::Context,
        dialog: &mut NotesDialog,
        app: &mut LauncherApp,
        time: f64,
        width: f32,
        height: f32,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        );
    }

    fn set_notes_scroll_offset(ctx: &egui::Context, dialog: &NotesDialog, offset: egui::Vec2) {
        let id = dialog
            .test_scroll_area_id
            .expect("NotesDialog scroll area rendered");
        let mut state = egui::scroll_area::State::load(ctx, id).unwrap_or_default();
        state.offset = offset;
        state.store(ctx, id);
    }

    fn rendered_note_rect(dialog: &NotesDialog, original_index: usize) -> RenderedNoteRowRects {
        dialog
            .last_rendered_row_rects
            .iter()
            .find_map(|(index, rects)| (*index == original_index).then_some(*rects))
            .expect("requested original note index was built")
    }

    fn header_gap_point(row: &RenderedNoteRowRects) -> egui::Pos2 {
        let y = row.header.center().y;
        [
            egui::pos2((row.title.right() + row.meta.left()) * 0.5, y),
            egui::pos2(row.header.max.x - 0.5, y),
            egui::pos2(row.meta.max.x + 0.5, y),
            egui::pos2(row.header.min.x + 0.5, y),
        ]
        .into_iter()
        .find(|point| {
            row.header.contains(*point) && !row.title.contains(*point) && !row.meta.contains(*point)
        })
        .expect("header layout has interactive padding or spacing outside its text labels")
    }

    fn click_notes_at(
        ctx: &egui::Context,
        dialog: &mut NotesDialog,
        app: &mut LauncherApp,
        point: egui::Pos2,
        button: egui::PointerButton,
        time: f64,
    ) {
        render_notes_frame_with_events(
            ctx,
            dialog,
            app,
            time - 0.01,
            960.0,
            640.0,
            vec![egui::Event::PointerMoved(point)],
        );
        render_notes_frame_with_events(
            ctx,
            dialog,
            app,
            time,
            960.0,
            640.0,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_notes_frame_with_events(
            ctx,
            dialog,
            app,
            time + 0.02,
            960.0,
            640.0,
            vec![egui::Event::PointerButton {
                pos: point,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
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

            let mut baseline_viewport_height = None;
            for (scenario_index, (scenario, filter, expected_indices, screen_height)) in [
                (
                    "empty-filter",
                    String::new(),
                    (0..count).collect::<Vec<_>>(),
                    640.0,
                ),
                (
                    "sparse-filter",
                    format!("track-a-note-{edit_index:05}"),
                    vec![edit_index],
                    640.0,
                ),
                (
                    "empty-filter-small-viewport",
                    String::new(),
                    (0..count).collect::<Vec<_>>(),
                    180.0,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                dialog.search = filter.clone();
                let scenario_base_time = 2.0 + scenario_index as f64 * 5.0;
                let cold_rebuilds_before = dialog.test_geometry_rebuilds;
                let cold_rows_before = dialog.test_geometry_rows_measured;
                // Treat this as a cold geometry observation; construction and
                // settling stay outside the fixed timed workload protocol.
                dialog.notes_geometry = None;
                for frame in 0..7 {
                    render_notes_frame_at(
                        &context,
                        &mut dialog,
                        &mut app,
                        scenario_base_time + frame as f64 / 60.0,
                        960.0,
                        screen_height,
                    );
                }
                assert_eq!(dialog.filtered_indices, expected_indices);
                let cold_rebuild_count = dialog.test_geometry_rebuilds - cold_rebuilds_before;
                let cold_rows_measured = dialog.test_geometry_rows_measured - cold_rows_before;
                let last_cold_rebuild_nanos = dialog.test_last_geometry_rebuild_nanos;
                assert!(cold_rebuild_count > 0);
                let viewport_height = dialog
                    .test_scroll_area_inner_rect
                    .expect("real Quick Notes scroll area rendered")
                    .height();
                if scenario_index == 0 {
                    baseline_viewport_height = Some(viewport_height);
                } else if scenario == "empty-filter-small-viewport" {
                    assert!(
                        (viewport_height - baseline_viewport_height.unwrap()).abs() > 20.0,
                        "small viewport scenario must actually change ScrollArea height"
                    );
                }
                let warm_rebuilds_before = dialog.test_geometry_rebuilds;
                let warm_rows_before = dialog.test_geometry_rows_measured;
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
                                egui::vec2(960.0, screen_height),
                            )),
                            time: Some(scenario_base_time + 1.0 + frame as f64 / 60.0),
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
                let warm_rebuild_count = dialog.test_geometry_rebuilds - warm_rebuilds_before;
                let warm_rows_measured = dialog.test_geometry_rows_measured - warm_rows_before;
                assert_eq!(warm_rebuild_count, 0, "warm scrolling reuses row geometry");
                assert_eq!(
                    warm_rows_measured, 0,
                    "warm scrolling does not remeasure rows"
                );
                let metrics =
                    workloads::metrics_for(&[Metric::QuickNotesRowsBuilt, Metric::NoteSnapshot]);
                assert_eq!(metrics.len(), 2);
                assert_eq!(metrics[0].metric, Metric::NoteSnapshot);
                assert_eq!(metrics[0].calls, 0, "warm browsing takes no note snapshots");
                assert_eq!(metrics[1].metric, Metric::QuickNotesRowsBuilt);
                assert_eq!(metrics[1].calls, workloads::SAMPLE_COUNT as u64);
                assert!(!dialog.last_rendered_indices.is_empty());
                assert!(dialog.last_rendered_indices.len() < 64);
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
                assert_eq!(
                    dialog.filtered_indices, expected_indices,
                    "the complete search projection remains available independently of viewport rows"
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
                let mut projection_signature = workloads::StableSignature::new(
                    0,
                    "quick-notes-complete-projection",
                    dialog.filtered_indices.len(),
                );
                for index in &dialog.filtered_indices {
                    let note = &dialog.entries[*index];
                    projection_signature.number(*index as u64);
                    projection_signature.bytes(note.slug.as_bytes());
                    projection_signature.bytes(note.title.as_bytes());
                }
                workloads::emit_summary_with_notes_geometry(
                    &format!("quick-notes-{count}-{scenario}"),
                    &format!(
                        "production NotesDialog::ui; headless egui debug-test CPU; in-memory notes; screen height {}pt",
                        screen_height
                    ),
                    fixture
                        .summary
                        .with_output_signatures(Some(signature.finish()), None),
                    timing,
                    &metrics,
                    Some(workloads::NotesGeometrySummary {
                        cold_rebuild_count,
                        cold_rows_measured,
                        last_cold_rebuild_nanos: last_cold_rebuild_nanos.min(u64::MAX as u128)
                            as u64,
                        warm_rebuild_count,
                        warm_rows_measured,
                        projection_count: dialog.filtered_indices.len(),
                        projection_signature: projection_signature.finish(),
                        viewport_height_points: viewport_height,
                    }),
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
