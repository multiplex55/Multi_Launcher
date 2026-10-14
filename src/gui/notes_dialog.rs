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

#[cfg(test)]
mod c2f_observation {
    use std::cell::RefCell;
    use std::time::Instant;

    #[derive(Clone, Copy, Debug, Default)]
    pub(super) struct Snapshot {
        pub phase_ns: [u64; 6],
        pub snapshot_calls: u64,
        pub snapshot_successes: u64,
        pub snapshot_errors: u64,
        pub snapshot_entries: u64,
        pub index_build_calls: u64,
        pub index_entries: u64,
        pub index_utf8_bytes: u64,
        pub metadata_build_calls: u64,
        pub metadata_entries: u64,
        pub metadata_output_utf8_bytes: u64,
        pub metadata_backlink_queries: u64,
        pub metadata_checkbox_lines: u64,
        pub metadata_preview_builds: u64,
        pub projection_calls: u64,
        pub projection_entries_scanned: u64,
        pub projection_indices_materialized: u64,
        pub geometry_ensure_calls: u64,
        pub geometry_rebuilds: u64,
        pub geometry_rows_measured: u64,
        pub geometry_identity_entries: u64,
        pub geometry_overlap_old_bytes: u64,
        pub geometry_overlap_new_bytes: u64,
        pub geometry_overlap_old_entries: u64,
        pub geometry_overlap_new_entries: u64,
        pub geometry_overlap_old_rows_vec_bytes: u64,
        pub geometry_overlap_old_identity_bytes: u64,
        pub geometry_overlap_old_map_key_bytes: u64,
        pub geometry_overlap_new_rows_vec_bytes: u64,
        pub geometry_overlap_new_identity_bytes: u64,
        pub geometry_overlap_new_map_key_bytes: u64,
        pub painted_rows: u64,
    }

    #[derive(Clone, Copy)]
    #[repr(usize)]
    pub(super) enum Phase {
        Snapshot = 0,
        SearchIndex = 1,
        Metadata = 2,
        Projection = 3,
        EnsureGeometry = 4,
        MeasureGeometry = 5,
    }

    thread_local! {
        static ACTIVE: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
    }

    pub(super) struct Scope {
        previous: Option<Snapshot>,
        finished: bool,
    }

    impl Scope {
        pub(super) fn begin() -> Self {
            let previous = ACTIVE.with(|active| active.replace(Some(Snapshot::default())));
            Self {
                previous,
                finished: false,
            }
        }

        pub(super) fn finish(&mut self) -> Snapshot {
            let snapshot = ACTIVE.with(|active| {
                let snapshot = active.borrow().unwrap_or_default();
                active.replace(self.previous.take());
                snapshot
            });
            self.finished = true;
            snapshot
        }
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if !self.finished {
                ACTIVE.with(|active| {
                    active.replace(self.previous.take());
                });
            }
        }
    }

    pub(super) struct Timer {
        phase: Phase,
        started: Option<Instant>,
    }

    impl Timer {
        pub(super) fn start(phase: Phase) -> Self {
            let started = active().then(Instant::now);
            Self { phase, started }
        }
    }

    impl Drop for Timer {
        fn drop(&mut self) {
            let Some(started) = self.started else {
                return;
            };
            let nanos = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            with_snapshot(|snapshot| {
                let slot = &mut snapshot.phase_ns[self.phase as usize];
                *slot = slot.saturating_add(nanos);
            });
        }
    }

    pub(super) fn active() -> bool {
        ACTIVE.with(|active| active.borrow().is_some())
    }

    pub(super) fn with_snapshot(mut update: impl FnMut(&mut Snapshot)) {
        ACTIVE.with(|active| {
            if let Some(snapshot) = active.borrow_mut().as_mut() {
                update(snapshot);
            }
        });
    }
}

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
    let mut count = 0;
    #[cfg(test)]
    let observing = c2f_observation::active();
    #[cfg(test)]
    let mut visited_lines = 0_u64;
    for line in content.lines() {
        #[cfg(test)]
        if observing {
            visited_lines = visited_lines.saturating_add(1);
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
        {
            count += 1;
        }
    }
    #[cfg(test)]
    if observing {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.metadata_checkbox_lines = snapshot
                .metadata_checkbox_lines
                .saturating_add(visited_lines);
        });
    }
    count
}

fn build_search_index(entries: &[Note]) -> Vec<String> {
    #[cfg(test)]
    let observing = c2f_observation::active();
    #[cfg(test)]
    let _phase = c2f_observation::Timer::start(c2f_observation::Phase::SearchIndex);
    #[cfg(test)]
    let mut output_bytes = 0_u64;
    let output = entries
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
            #[cfg(test)]
            if observing {
                output_bytes = output_bytes.saturating_add(text.len() as u64);
            }
            text
        })
        .collect::<Vec<_>>();
    #[cfg(test)]
    if observing {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.index_build_calls = snapshot.index_build_calls.saturating_add(1);
            snapshot.index_entries = snapshot.index_entries.saturating_add(output.len() as u64);
            snapshot.index_utf8_bytes = snapshot.index_utf8_bytes.saturating_add(output_bytes);
        });
    }
    output
}

fn build_row_metadata(
    entries: &[Note],
    backlinks_enabled: bool,
    task_lists_enabled: bool,
) -> anyhow::Result<Vec<NoteRowMetadata>> {
    #[cfg(test)]
    let observing = c2f_observation::active();
    #[cfg(test)]
    let _phase = c2f_observation::Timer::start(c2f_observation::Phase::Metadata);
    #[cfg(test)]
    if observing {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.metadata_build_calls = snapshot.metadata_build_calls.saturating_add(1);
        });
    }
    #[cfg(test)]
    let mut output_bytes = 0_u64;
    #[cfg(test)]
    let mut backlink_queries = 0_u64;
    let output = entries
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
                #[cfg(test)]
                if observing {
                    backlink_queries = backlink_queries.saturating_add(1);
                }
                meta.push(format!("{} backlinks", note_backlink_count(&note.slug)?));
            }
            if task_lists_enabled {
                let count = checkbox_count(&note.content);
                if count > 0 {
                    meta.push(format!("{count} checkboxes"));
                }
            }
            let row = NoteRowMetadata {
                original_index,
                display_title: display_title(note).to_owned(),
                meta: meta.join(" · "),
                preview: short_preview(&note.content),
            };
            #[cfg(test)]
            if observing {
                output_bytes = output_bytes
                    .saturating_add(row.display_title.len() as u64)
                    .saturating_add(row.meta.len() as u64)
                    .saturating_add(row.preview.len() as u64);
            }
            Ok(row)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    #[cfg(test)]
    if observing {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.metadata_entries = snapshot
                .metadata_entries
                .saturating_add(output.len() as u64);
            snapshot.metadata_output_utf8_bytes = snapshot
                .metadata_output_utf8_bytes
                .saturating_add(output_bytes);
            snapshot.metadata_backlink_queries = snapshot
                .metadata_backlink_queries
                .saturating_add(backlink_queries);
            snapshot.metadata_preview_builds = snapshot
                .metadata_preview_builds
                .saturating_add(output.len() as u64);
        });
    }
    Ok(output)
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
    font_revision: u64,
    viewport_width: u32,
    pixels_per_point: u32,
    title_font: egui::FontId,
    small_font: egui::FontId,
    wrap_mode: Option<egui::TextWrapMode>,
    extra_text_line_spacing_bits: u32,
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
    #[cfg(test)]
    measured_rows: usize,
    #[cfg(test)]
    rebuild_nanos: u128,
    #[cfg(test)]
    row_identity_capacity_bytes: u64,
    #[cfg(test)]
    map_key_capacity_bytes: u64,
}

impl NotesGeometry {
    fn matches(&self, key: &NotesGeometryKey) -> bool {
        self.key == *key
    }

    fn anchor_at(&self, offset_y: f32) -> Option<(&str, f32)> {
        let row = self
            .rows
            .iter()
            .find(|row| row.visual_bottom > offset_y)
            .or_else(|| self.rows.last())?;
        Some((&row.identity, (offset_y - row.top).max(0.0)))
    }

    #[cfg(test)]
    fn partial_capacity_bytes(&self) -> u64 {
        (self.rows.capacity() * std::mem::size_of::<NoteRowGeometry>())
            .saturating_add(self.row_identity_capacity_bytes as usize)
            .saturating_add(self.map_key_capacity_bytes as usize)
            .try_into()
            .unwrap_or(u64::MAX)
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
    font_revision: u64,
    viewport_width: f32,
    pixels_per_point: f32,
    style: &egui::Style,
) -> NotesGeometryKey {
    let spacing = &style.spacing;
    NotesGeometryKey {
        entries_generation,
        metadata_generation,
        projection_generation,
        font_revision,
        viewport_width: viewport_width.to_bits(),
        pixels_per_point: pixels_per_point.to_bits(),
        title_font: egui::FontSelection::Default.resolve(style),
        small_font: style
            .override_font_id
            .clone()
            .unwrap_or_else(|| egui::TextStyle::Small.resolve(style)),
        wrap_mode: style.wrap_mode,
        extra_text_line_spacing_bits: spacing.extra_text_line_spacing.to_bits(),
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
    wrap_mode: egui::TextWrapMode,
    width: f32,
    valign: egui::Align,
) -> egui::Vec2 {
    text.into_galley_impl(
        ctx,
        style,
        egui::text::TextWrapping::from_wrap_mode_and_width(wrap_mode, width),
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
    let key = geometry_key(
        entries_generation,
        metadata_generation,
        projection_generation,
        super::font_revision::active_revision(ctx),
        viewport_width,
        ctx.pixels_per_point(),
        style,
    );
    let spacing_x = style.spacing.item_spacing.x;
    let spacing_y = style.spacing.item_spacing.y;
    let header_wrap = style.wrap_mode.unwrap_or(egui::TextWrapMode::Extend);
    let preview_wrap = style.wrap_mode.unwrap_or(egui::TextWrapMode::Wrap);
    let mut prefix_width = viewport_width.max(0.0);
    let mut top = 0.0;
    let mut content_height = 0.0;
    let mut rows = Vec::with_capacity(filtered_indices.len());
    let mut row_by_identity = std::collections::HashMap::with_capacity(filtered_indices.len());
    #[cfg(test)]
    let mut row_identity_capacity_bytes = 0_u64;
    #[cfg(test)]
    let mut map_key_capacity_bytes = 0_u64;

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
        let map_key = identity.clone();
        #[cfg(test)]
        {
            row_identity_capacity_bytes =
                row_identity_capacity_bytes.saturating_add(identity.capacity() as u64);
            map_key_capacity_bytes =
                map_key_capacity_bytes.saturating_add(map_key.capacity() as u64);
        }
        row_by_identity.insert(map_key, rows.len());
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
        #[cfg(test)]
        measured_rows: filtered_indices.len(),
        #[cfg(test)]
        rebuild_nanos,
        #[cfg(test)]
        row_identity_capacity_bytes,
        #[cfg(test)]
        map_key_capacity_bytes,
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
    #[cfg(test)]
    let observing = c2f_observation::active();
    #[cfg(test)]
    let _phase = c2f_observation::Timer::start(c2f_observation::Phase::Projection);
    #[cfg(test)]
    if observing {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.projection_calls = snapshot.projection_calls.saturating_add(1);
        });
    }
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
        #[cfg(test)]
        if observing {
            c2f_observation::with_snapshot(|snapshot| {
                snapshot.projection_entries_scanned = snapshot
                    .projection_entries_scanned
                    .saturating_add(index.len() as u64);
            });
        }
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
    #[cfg(test)]
    if observing && (filter_changed || generation_changed) {
        c2f_observation::with_snapshot(|snapshot| {
            snapshot.projection_indices_materialized = snapshot
                .projection_indices_materialized
                .saturating_add(filtered_indices.len() as u64);
        });
    }
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
    last_rendered_header_ids: Vec<(String, egui::Id)>,
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
        let _phase = c2f_observation::Timer::start(c2f_observation::Phase::Snapshot);
        #[cfg(test)]
        {
            self.test_note_snapshot_calls = self.test_note_snapshot_calls.saturating_add(1);
            if c2f_observation::active() {
                c2f_observation::with_snapshot(|snapshot| {
                    snapshot.snapshot_calls = snapshot.snapshot_calls.saturating_add(1);
                });
            }
            if std::mem::take(&mut self.test_fail_next_snapshot) {
                #[cfg(test)]
                if c2f_observation::active() {
                    c2f_observation::with_snapshot(|snapshot| {
                        snapshot.snapshot_errors = snapshot.snapshot_errors.saturating_add(1);
                    });
                }
                anyhow::bail!("injected Quick Notes snapshot failure");
            }
        }
        let result = note_cache_snapshot_with_version();
        #[cfg(test)]
        if c2f_observation::active() {
            c2f_observation::with_snapshot(|snapshot| match &result {
                Ok((_, entries)) => {
                    snapshot.snapshot_successes = snapshot.snapshot_successes.saturating_add(1);
                    snapshot.snapshot_entries = snapshot
                        .snapshot_entries
                        .saturating_add(entries.len() as u64);
                }
                Err(_) => {
                    snapshot.snapshot_errors = snapshot.snapshot_errors.saturating_add(1);
                }
            });
        }
        result
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
        #[cfg(test)]
        let observing = c2f_observation::active();
        #[cfg(test)]
        let _phase = c2f_observation::Timer::start(c2f_observation::Phase::EnsureGeometry);
        #[cfg(test)]
        if observing {
            c2f_observation::with_snapshot(|snapshot| {
                snapshot.geometry_ensure_calls = snapshot.geometry_ensure_calls.saturating_add(1);
            });
        }
        let key = geometry_key(
            self.entries_generation,
            self.metadata_generation,
            self.projection_generation,
            super::font_revision::active_revision(ctx),
            viewport_width,
            ctx.pixels_per_point(),
            style,
        );
        if self
            .notes_geometry
            .as_ref()
            .is_some_and(|geometry| geometry.matches(&key))
        {
            return None;
        }
        let mut track_c_timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::NotesGeometryCold,
        );
        track_c_timer.set_work_units(self.filtered_indices.len());
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
            if observing {
                c2f_observation::with_snapshot(|snapshot| {
                    snapshot.phase_ns[c2f_observation::Phase::MeasureGeometry as usize] = snapshot
                        .phase_ns[c2f_observation::Phase::MeasureGeometry as usize]
                        .saturating_add(geometry.rebuild_nanos.min(u64::MAX as u128) as u64);
                    snapshot.geometry_rebuilds = snapshot.geometry_rebuilds.saturating_add(1);
                    snapshot.geometry_rows_measured = snapshot
                        .geometry_rows_measured
                        .saturating_add(geometry.rows.len() as u64);
                    snapshot.geometry_identity_entries = snapshot
                        .geometry_identity_entries
                        .saturating_add(geometry.row_by_identity.len() as u64);
                });
            }
        }
        let result = self.notes_geometry.replace(geometry);
        #[cfg(test)]
        if observing {
            let new_capacity = self
                .notes_geometry
                .as_ref()
                .map_or(0, NotesGeometry::partial_capacity_bytes);
            let old_capacity = result
                .as_ref()
                .map_or(0, NotesGeometry::partial_capacity_bytes);
            let old_entries = result
                .as_ref()
                .map_or(0, |geometry| geometry.row_by_identity.len() as u64);
            let new_entries = self
                .notes_geometry
                .as_ref()
                .map_or(0, |geometry| geometry.row_by_identity.len() as u64);
            let old_parts = result.as_ref().map_or([0; 3], |geometry| {
                [
                    (geometry.rows.capacity() * std::mem::size_of::<NoteRowGeometry>()) as u64,
                    geometry.row_identity_capacity_bytes,
                    geometry.map_key_capacity_bytes,
                ]
            });
            let new_parts = self.notes_geometry.as_ref().map_or([0; 3], |geometry| {
                [
                    (geometry.rows.capacity() * std::mem::size_of::<NoteRowGeometry>()) as u64,
                    geometry.row_identity_capacity_bytes,
                    geometry.map_key_capacity_bytes,
                ]
            });
            c2f_observation::with_snapshot(|snapshot| {
                snapshot.geometry_overlap_old_bytes = old_capacity;
                snapshot.geometry_overlap_new_bytes = new_capacity;
                snapshot.geometry_overlap_old_entries = old_entries;
                snapshot.geometry_overlap_new_entries = new_entries;
                snapshot.geometry_overlap_old_rows_vec_bytes = old_parts[0];
                snapshot.geometry_overlap_old_identity_bytes = old_parts[1];
                snapshot.geometry_overlap_old_map_key_bytes = old_parts[2];
                snapshot.geometry_overlap_new_rows_vec_bytes = new_parts[0];
                snapshot.geometry_overlap_new_identity_bytes = new_parts[1];
                snapshot.geometry_overlap_new_map_key_bytes = new_parts[2];
            });
        }
        track_c_timer.finish(crate::performance::track_c::Outcome::Completed);
        result
    }

    fn close_menu_owner(&mut self) {
        if let Some(owner) = self.menu_owner.take()
            && owner.response.context_menu_opened()
        {
            owner.response.context_menu(|ui| ui.close());
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
        self.last_rendered_header_ids.clear();
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
                                output.cursor_range.map(|range| range.primary.index.0);

                            let mut insert_timestamp = false;
                            let mut insert_idx = None;
                            resp.context_menu(|ui| {
                                if ui.button("Insert timestamp").clicked() {
                                    insert_timestamp = true;
                                    insert_idx = caret_char_idx;
                                    ui.close();
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
                    let _output = egui::ScrollArea::both()
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
                                #[cfg(test)]
                                let mut row_header_response_id = None;
                                let row_id = ui.id().with(("quick-notes-row", identity));
                                let _allocated_row = ui.scope_builder(
                                    egui::UiBuilder::new().id(row_id).max_rect(row_rect),
                                    |ui| {
                                        ui.vertical(|ui| {
                                            let preview = row.preview.as_str();
                                            let header = ui.horizontal(|ui| {
                                                let title = ui.strong(&row.display_title);
                                                let meta = ui.small(&row.meta);
                                                (title, meta)
                                            });
                                            let (title_response, meta_response) = header.inner;
                                            let title_rect = title_response.rect;
                                            let meta_rect = meta_response.rect;
                                            let header_response = header
                                                .response
                                                .interact(egui::Sense::click())
                                                .union(title_response)
                                                .union(meta_response);
                                            #[cfg(test)]
                                            {
                                                row_header_response_id = Some(header_response.id);
                                            }
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
                                                        ui.close();
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
                                                        ui.close();
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
                                                        ui.close();
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
                                                        ui.close();
                                                    }
                                                    if ui.button("Copy slug").clicked() {
                                                        if let Err(e) = crate::actions::clipboard::set_text(&entry.slug) {
                                                            app.report_error_message(
                                                                "ui operation",
                                                                format!("Failed to copy note slug: {e}"),
                                                            );
                                                        }
                                                        ui.close();
                                                    }
                                                    if app.note_settings.aliases_enabled
                                                        && ui.button("Manage aliases").clicked()
                                                    {
                                                        app.open_note_panel(&entry.slug, None);
                                                        ui.close();
                                                    }
                                                    if app.note_settings.templates_enabled
                                                        && ui.button("Create note from template").clicked()
                                                    {
                                                        app.activate_action(
                                                            note_action("New note", "query:note templates"),
                                                            None,
                                                            ActivationSource::Click,
                                                        );
                                                        ui.close();
                                                    }
                                                    ui.menu_button("Meta", |ui| {
                                                        if ui.button("Wrap links in note").clicked() {
                                                            wrap_links_slug = Some(entry.slug.clone());
                                                            ui.close();
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
                                                        ui.close();
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
                                                            ui.close();
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
                                    },
                                );
                                #[cfg(test)]
                                let actual_row = _allocated_row.inner;
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
                                        ui.scope_builder(egui::UiBuilder::new().max_rect(separator_rect), |ui| ui.separator())
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
                                    if c2f_observation::active() {
                                        c2f_observation::with_snapshot(|snapshot| {
                                            snapshot.painted_rows =
                                                snapshot.painted_rows.saturating_add(1);
                                        });
                                    }
                                    self.last_rendered_indices.push(idx);
                                    self.last_rendered_header_ids
                                        .push((
                                            identity.to_owned(),
                                            row_header_response_id
                                                .expect("the note row built a header response"),
                                        ));
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
                        self.test_scroll_area_id = Some(_output.id);
                        self.test_scroll_area_offset = _output.state.offset;
                        self.test_scroll_area_inner_rect = Some(_output.inner_rect);
                        self.test_scroll_area_content_size = _output.content_size;
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

    #[derive(Debug)]
    struct EagerNoteRowRects {
        original_index: usize,
        outer: egui::Rect,
        header: egui::Rect,
        title: egui::Rect,
        meta: egui::Rect,
        preview: Option<egui::Rect>,
        separator: egui::Rect,
    }

    struct EagerNotesRowsLayout {
        rows: Vec<EagerNoteRowRects>,
        estimated: super::NotesGeometry,
        content_origin: egui::Pos2,
        bounds: egui::Rect,
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

    fn eager_notes_rows(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        entries: &[Note],
        metadata: &[super::NoteRowMetadata],
        filtered_indices: &[usize],
        viewport_width: f32,
    ) -> EagerNotesRowsLayout {
        let content_origin = ui.min_rect().min;
        let estimated = super::measure_notes_geometry(
            ctx,
            ui.style(),
            viewport_width,
            entries,
            metadata,
            filtered_indices,
            1,
            1,
            1,
        );
        let mut rows = Vec::with_capacity(filtered_indices.len());
        let mut bounds = egui::Rect::NOTHING;
        for &original_index in filtered_indices {
            let Some(row) = metadata
                .get(original_index)
                .filter(|row| row.original_index == original_index)
            else {
                continue;
            };
            let outer = ui.vertical(|ui| {
                let header = ui.horizontal(|ui| {
                    let title = ui.strong(&row.display_title);
                    let meta = ui.small(&row.meta);
                    (title.rect, meta.rect)
                });
                let preview = if row.preview.is_empty() {
                    None
                } else {
                    Some(ui.small(&row.preview).rect)
                };
                (
                    header.response.rect,
                    header.inner.0,
                    header.inner.1,
                    preview,
                )
            });
            let separator = ui.separator().rect;
            let (header, title, meta, preview) = outer.inner;
            for rect in [outer.response.rect, header, title, meta, separator] {
                bounds = bounds.union(rect);
            }
            if let Some(preview) = preview {
                bounds = bounds.union(preview);
            }
            rows.push(EagerNoteRowRects {
                original_index,
                outer: outer.response.rect,
                header,
                title,
                meta,
                preview,
                separator,
            });
        }
        EagerNotesRowsLayout {
            rows,
            estimated,
            content_origin,
            bounds,
        }
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
        let all_indices = (0..entries.len()).collect::<Vec<_>>();
        let mut result = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(screen_width, 800.0),
                )),
                ..Default::default()
            },
            |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    let output = egui::ScrollArea::both().max_height(520.0).show_viewport(
                        ui,
                        |ui, viewport| {
                            let layout = eager_notes_rows(
                                &frame_context,
                                ui,
                                entries,
                                &metadata,
                                &all_indices,
                                viewport.width(),
                            );
                            (layout.rows, layout.content_origin, layout.estimated)
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
        output.drop_without_applying_deltas();
        result.expect("the eager geometry oracle runs its central panel")
    }

    fn eager_notes_geometry_for_projection(
        ctx: &egui::Context,
        entries: &[Note],
        filtered_indices: &[usize],
        viewport_size: egui::Vec2,
        style: &egui::Style,
        backlinks_enabled: bool,
        task_lists_enabled: bool,
    ) -> EagerNotesGeometry {
        let metadata = super::build_row_metadata(entries, backlinks_enabled, task_lists_enabled)
            .expect("synthetic note metadata builds in the isolated fixture");
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let content_rect = viewport;
        ctx.set_style_of(ctx.theme(), style.clone());
        let layer = egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("track-c-notes-eager-oracle"),
        );
        let mut result = None;
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |root_ui| {
                let mut ui = egui::Ui::new(
                    root_ui.ctx().clone(),
                    egui::Id::new("track-c-notes-eager-ui"),
                    egui::UiBuilder::new()
                        .layer_id(layer)
                        .max_rect(content_rect),
                );
                ui.set_clip_rect(viewport);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_style(style.clone());
                    let layout = eager_notes_rows(
                        ctx,
                        ui,
                        entries,
                        &metadata,
                        filtered_indices,
                        viewport.width(),
                    );
                    let content_size = if layout.rows.is_empty() {
                        egui::Vec2::ZERO
                    } else {
                        egui::vec2(
                            viewport
                                .width()
                                .max(layout.bounds.right() - layout.content_origin.x),
                            (layout.bounds.bottom() - layout.content_origin.y).max(0.0),
                        )
                    };
                    result = Some(EagerNotesGeometry {
                        rows: layout.rows,
                        content_size,
                        viewport,
                        content_origin: layout.content_origin,
                        estimated_rows: layout.estimated.rows,
                        estimated_content_size: layout.estimated.content_size,
                    });
                });
            },
        );
        output.drop_without_applying_deltas();
        result.expect("the projection-aware eager note oracle runs its row composition")
    }

    fn track_c_expected_note_indices(entries: &[Note], filter: &str) -> Vec<usize> {
        let filter = filter.to_lowercase();
        entries
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                let mut searchable = note.content.to_lowercase();
                if let Some(alias) = &note.alias {
                    searchable.push('\n');
                    searchable.push_str(&alias.to_lowercase());
                }
                for alias in &note.aliases {
                    searchable.push('\n');
                    searchable.push_str(&alias.to_lowercase());
                }
                searchable.push('\n');
                searchable.push_str(&note.slug.to_lowercase());
                for tag in &note.tags {
                    searchable.push('\n');
                    searchable.push_str(&tag.to_lowercase());
                }
                (filter.is_empty() || searchable.contains(&filter)).then_some(index)
            })
            .collect()
    }

    struct TrackCQuickNotesState {
        dialog: NotesDialog,
        app: LauncherApp,
        ctx: egui::Context,
        expected_notes: Vec<Note>,
        expected_metadata: Vec<super::NoteRowMetadata>,
        expected_indices: Vec<usize>,
        eager: EagerNotesGeometry,
        screen_width: f32,
        screen_height: f32,
        actual_scroll_area_width: f32,
        wide_scroll_area_width: Option<f32>,
        wide_geometry_width: Option<u32>,
        wide_rebuilds: Option<u64>,
        rebuilds_before: u64,
        rows_measured_before: u64,
        cold: bool,
        frame_index: usize,
        _cache_guard: crate::plugins::note::NoteCachePublicationGuard,
    }

    fn track_c_prepare_quick_notes_state(
        root: &std::path::Path,
        notes: &[Note],
        filter: &str,
        screen_width: f32,
        screen_height: f32,
        cold: bool,
        transition_from_width: Option<f32>,
    ) -> TrackCQuickNotesState {
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, root);
        let cache_guard = crate::plugins::note::publish_note_cache_for_test(notes.to_vec());
        let (published_revision, canonical_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("synthetic note cache snapshot is healthy");
        assert_eq!(canonical_notes.len(), notes.len());
        assert_eq!(
            canonical_notes
                .iter()
                .map(|note| &note.slug)
                .collect::<Vec<_>>(),
            notes.iter().map(|note| &note.slug).collect::<Vec<_>>(),
            "cache publication retains the fixture's ordered note identities"
        );
        let mut dialog = NotesDialog::default();
        dialog.open();
        dialog.search = filter.to_owned();
        if let Some(wide_width) = transition_from_width {
            for frame in 0..7 {
                render_notes_frame_at(
                    &ctx,
                    &mut dialog,
                    &mut app,
                    1.0 + frame as f64 / 60.0,
                    wide_width,
                    screen_height,
                );
            }
        }
        let wide_viewport = transition_from_width.map(|_| {
            dialog
                .test_scroll_area_inner_rect
                .expect("the wide Quick Notes ScrollArea rendered")
        });
        let wide_geometry_width = transition_from_width.map(|_| {
            dialog
                .notes_geometry
                .as_ref()
                .expect("wide Quick Notes geometry is installed")
                .key
                .viewport_width
        });
        let wide_rebuilds = transition_from_width.map(|_| dialog.test_geometry_rebuilds);
        let first_final_frame = if transition_from_width.is_some() {
            7
        } else {
            0
        };
        for frame in first_final_frame..first_final_frame + 7 {
            render_notes_frame_at(
                &ctx,
                &mut dialog,
                &mut app,
                1.0 + frame as f64 / 60.0,
                screen_width,
                screen_height,
            );
        }

        assert_eq!(
            dialog.entries, canonical_notes,
            "Quick Notes loaded the complete canonical published snapshot"
        );
        assert_eq!(dialog.entries_revision, Some(published_revision));
        let backlinks_enabled = app.note_settings.backlinks_enabled;
        let task_lists_enabled = app.note_settings.task_lists_enabled;
        let expected_metadata =
            super::build_row_metadata(&canonical_notes, backlinks_enabled, task_lists_enabled)
                .expect("synthetic note metadata builds in the isolated fixture");
        assert_eq!(dialog.row_metadata, expected_metadata);
        let expected_indices = track_c_expected_note_indices(&canonical_notes, filter);
        assert_eq!(dialog.filtered_indices, expected_indices);
        let viewport = dialog
            .test_scroll_area_inner_rect
            .expect("the actual NotesDialog ScrollArea rendered");
        assert!(viewport.width() > 0.0 && viewport.height() > 0.0);
        if let Some(wide_viewport) = wide_viewport {
            assert!(
                viewport.width() + 0.5 < wide_viewport.width(),
                "the real 300-point screen constrains the NotesDialog viewport: wide={} narrow={}",
                wide_viewport.width(),
                viewport.width()
            );
            let geometry_width = dialog
                .notes_geometry
                .as_ref()
                .expect("narrow Quick Notes geometry is installed")
                .key
                .viewport_width;
            assert_ne!(geometry_width, wide_geometry_width.unwrap());
            assert!(dialog.test_geometry_rebuilds > wide_rebuilds.unwrap());
        }
        let style = ctx.style_of(ctx.theme()).as_ref().clone();
        let oracle_ctx = egui::Context::default();
        let eager = eager_notes_geometry_for_projection(
            &oracle_ctx,
            &canonical_notes,
            &expected_indices,
            viewport.size(),
            &style,
            backlinks_enabled,
            task_lists_enabled,
        );
        assert_quick_notes_geometry_matches_eager(&dialog, &eager, &expected_indices);

        let rebuilds_before = dialog.test_geometry_rebuilds;
        let rows_measured_before = dialog.test_geometry_rows_measured;
        if cold {
            dialog.notes_geometry = None;
        }
        crate::performance::track_c::reset();
        TrackCQuickNotesState {
            dialog,
            app,
            ctx,
            expected_notes: canonical_notes,
            expected_metadata,
            expected_indices,
            eager,
            screen_width,
            screen_height,
            actual_scroll_area_width: viewport.width(),
            wide_scroll_area_width: wide_viewport.map(|rect| rect.width()),
            wide_geometry_width,
            wide_rebuilds,
            rebuilds_before,
            rows_measured_before,
            cold,
            frame_index: if transition_from_width.is_some() {
                14
            } else {
                7
            },
            _cache_guard: cache_guard,
        }
    }

    fn assert_quick_notes_geometry_matches_eager(
        dialog: &NotesDialog,
        eager: &EagerNotesGeometry,
        expected_indices: &[usize],
    ) {
        let geometry = dialog
            .notes_geometry
            .as_ref()
            .expect("the production NotesDialog geometry is installed");
        assert_eq!(geometry.rows.len(), expected_indices.len());
        assert_eq!(eager.rows.len(), expected_indices.len());
        assert_eq!(geometry.rows.len(), eager.estimated_rows.len());
        assert!(
            (geometry.content_size.x - eager.estimated_content_size.x).abs() <= 0.5
                && (geometry.content_size.y - eager.estimated_content_size.y).abs() <= 0.5,
            "production and eager content extents differ: actual={:?} estimated={:?}",
            geometry.content_size,
            eager.estimated_content_size
        );
        for (position, ((actual, estimated), expected_index)) in geometry
            .rows
            .iter()
            .zip(&eager.rows)
            .zip(expected_indices)
            .enumerate()
        {
            assert_eq!(actual.original_index, *expected_index);
            assert_eq!(estimated.original_index, *expected_index);
            assert_eq!(actual.identity, dialog.entries[*expected_index].slug);
            let expected_top = estimated.outer.min.y - eager.content_origin.y;
            assert!(
                (actual.top - expected_top).abs() <= 0.5,
                "row {position} top differs from eager widget: geometry={} eager={expected_top}",
                actual.top
            );
            assert!(
                (actual.body_height - estimated.outer.height()).abs() <= 0.5,
                "row {position} body height differs: geometry={} eager={}",
                actual.body_height,
                estimated.outer.height()
            );
            assert!(
                (actual.body_width - estimated.outer.width()).abs() <= 1.0,
                "row {position} body width differs: geometry={} eager={}",
                actual.body_width,
                estimated.outer.width()
            );
            assert!(
                (actual.header_height - estimated.header.height()).abs() <= 0.5,
                "row {position} header height differs: geometry={} eager={}",
                actual.header_height,
                estimated.header.height()
            );
            assert!(
                (actual.header_width - estimated.header.width()).abs() <= 1.0,
                "row {position} header width differs: geometry={} eager={}",
                actual.header_width,
                estimated.header.width()
            );
            let expected_separator_top = estimated.separator.min.y - eager.content_origin.y;
            assert!(
                (actual.separator_top - expected_separator_top).abs() <= 0.5,
                "row {position} separator top differs: geometry={} eager={expected_separator_top}",
                actual.separator_top
            );
            assert!(
                (actual.width_after - estimated.separator.width()).abs() <= 1.0,
                "row {position} trailing width differs: geometry={} eager={}",
                actual.width_after,
                estimated.separator.width()
            );
            let expected_width_before = if position == 0 {
                eager.viewport.width()
            } else {
                eager.rows[position - 1].separator.width()
            };
            assert!(
                (actual.width_before - expected_width_before).abs() <= 1.0,
                "row {position} leading width differs: geometry={} eager={expected_width_before}",
                actual.width_before
            );
            match (actual.preview_height, estimated.preview) {
                (Some(actual_height), Some(preview)) => assert!(
                    (actual_height - preview.height()).abs() <= 0.5,
                    "row {position} preview height differs: geometry={actual_height} eager={}",
                    preview.height()
                ),
                (None, None) => {}
                mismatch => panic!("row {position} preview presence differs: {mismatch:?}"),
            }
        }
        assert!(
            (eager.content_size.x - eager.estimated_content_size.x).abs() <= 1.0
                && (eager.content_size.y - eager.estimated_content_size.y).abs() <= 1.0,
            "complete eager note extent differs from the geometry estimate: actual={:?} estimated={:?}",
            eager.content_size,
            eager.estimated_content_size
        );
    }

    fn track_c_validate_quick_notes_state(
        state: &TrackCQuickNotesState,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        use crate::performance::track_c_workloads::{
            OwnerObservation, StableSignature, note_fixture_identity,
        };

        let dialog = &state.dialog;
        assert!(dialog.open);
        assert_eq!(dialog.entries, state.expected_notes);
        assert_eq!(dialog.row_metadata, state.expected_metadata);
        assert_eq!(dialog.filtered_indices, state.expected_indices);
        assert_eq!(
            track_c_expected_note_indices(&state.expected_notes, &dialog.search),
            state.expected_indices,
            "the complete original-index projection matches the independent filter"
        );
        assert_quick_notes_geometry_matches_eager(dialog, &state.eager, &state.expected_indices);
        assert!(
            (dialog.test_scroll_area_content_size.x - state.eager.content_size.x).abs() <= 1.0
                && (dialog.test_scroll_area_content_size.y - state.eager.content_size.y).abs()
                    <= 1.0,
            "the real Quick Notes ScrollArea exposes the independent eager full extent: actual={:?} eager={:?}",
            dialog.test_scroll_area_content_size,
            state.eager.content_size
        );

        let rebuilds = dialog
            .test_geometry_rebuilds
            .saturating_sub(state.rebuilds_before);
        let rows_measured = dialog
            .test_geometry_rows_measured
            .saturating_sub(state.rows_measured_before);
        if state.cold {
            assert_eq!(rebuilds, 1, "cold Quick Notes geometry rebuilds once");
            assert_eq!(rows_measured, state.expected_indices.len() as u64);
        } else {
            assert_eq!(rebuilds, 0, "warm Quick Notes geometry is reused");
            assert_eq!(rows_measured, 0, "warm Quick Notes measures no full rows");
        }

        let viewport = dialog
            .test_scroll_area_inner_rect
            .expect("the production Quick Notes ScrollArea rendered");
        assert!(
            (viewport.width() - state.actual_scroll_area_width).abs() <= 0.5,
            "actual ScrollArea row width remains stable: setup={} measured={}",
            state.actual_scroll_area_width,
            viewport.width()
        );
        if let (Some(wide_width), Some(wide_key), Some(wide_rebuilds)) = (
            state.wide_scroll_area_width,
            state.wide_geometry_width,
            state.wide_rebuilds,
        ) {
            assert!(state.actual_scroll_area_width + 0.5 < wide_width);
            let narrow_key = dialog
                .notes_geometry
                .as_ref()
                .expect("narrow geometry remains installed")
                .key
                .viewport_width;
            assert_ne!(narrow_key, wide_key);
            assert!(dialog.test_geometry_rebuilds > wide_rebuilds);
        }
        let content_origin = dialog.test_content_origin;
        let eager_rows = &state.eager.rows;
        let first_visible = eager_rows.partition_point(|row| {
            content_origin.y + row.separator.bottom() - state.eager.content_origin.y
                <= viewport.min.y
        });
        let after_visible = eager_rows.partition_point(|row| {
            content_origin.y + row.outer.top() - state.eager.content_origin.y <= viewport.max.y
        });
        let first = first_visible.saturating_sub(2);
        let after = after_visible.saturating_add(2).min(eager_rows.len());
        let expected_visible_indices = eager_rows[first..after]
            .iter()
            .map(|row| row.original_index)
            .collect::<Vec<_>>();
        assert_eq!(
            dialog.last_rendered_indices, expected_visible_indices,
            "NotesDialog builds exactly the eager visible range plus its two-row overscan"
        );
        assert!(!dialog.last_rendered_indices.is_empty());
        assert!(dialog.last_rendered_indices.len() < 64);
        assert!(
            dialog
                .last_rendered_indices
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        assert_eq!(
            dialog.last_rendered_row_rects.len(),
            expected_visible_indices.len()
        );
        for (actual_index, actual) in &dialog.last_rendered_row_rects {
            let position = state
                .expected_indices
                .binary_search(actual_index)
                .expect("rendered original index remains in the full projection");
            let eager = &state.eager.rows[position];
            assert_eq!(*actual_index, eager.original_index);
            let offset = content_origin - state.eager.content_origin;
            let compare_rect = |label: &str, actual: egui::Rect, expected: egui::Rect| {
                let expected = expected.translate(offset);
                assert!(
                    actual.min.distance(expected.min) <= 1.0
                        && actual.max.distance(expected.max) <= 1.0,
                    "rendered note {actual_index} {label} differs from eager widget: actual={actual:?} eager={expected:?}"
                );
            };
            compare_rect("outer", actual.outer, eager.outer);
            compare_rect("header", actual.header, eager.header);
            compare_rect("title", actual.title, eager.title);
            compare_rect("meta", actual.meta, eager.meta);
            compare_rect("separator", actual.separator, eager.separator);
            match (actual.preview, eager.preview) {
                (Some(actual), Some(expected)) => compare_rect("preview", actual, expected),
                (None, None) => {}
                mismatch => panic!("rendered note {actual_index} preview differs: {mismatch:?}"),
            }
        }

        let geometry = dialog.notes_geometry.as_ref().unwrap();
        let mut structure = StableSignature::new(
            0x435F_5155_4943_4B4E,
            "track-c-quick-notes-complete-geometry",
            geometry.rows.len(),
        );
        structure.number(note_fixture_identity(&dialog.entries));
        structure.number(dialog.search.len() as u64);
        structure.bytes(dialog.search.as_bytes());
        structure.number(dialog.entries_generation);
        structure.number(dialog.metadata_generation);
        structure.number(dialog.projection_generation);
        structure.number(geometry.key.viewport_width as u64);
        structure.number(geometry.key.pixels_per_point as u64);
        structure.number(geometry.rows.len() as u64);
        for (index, row) in geometry.rows.iter().enumerate() {
            structure.number(row.original_index as u64);
            structure.number(row.identity.len() as u64);
            structure.bytes(row.identity.as_bytes());
            assert_eq!(geometry.row_by_identity.get(&row.identity), Some(&index));
            for value in [
                row.top,
                row.header_height,
                row.header_width,
                row.body_width,
                row.preview_height.unwrap_or(-1.0),
                row.body_height,
                row.separator_top,
                row.visual_bottom,
                row.width_before,
                row.width_after,
            ] {
                structure.number(u64::from(value.to_bits()));
            }
        }
        for value in [geometry.content_size.x, geometry.content_size.y] {
            structure.number(u64::from(value.to_bits()));
        }
        for (index, metadata) in dialog.row_metadata.iter().enumerate() {
            structure.number(index as u64);
            structure.number(metadata.original_index as u64);
            for value in [
                metadata.display_title.as_str(),
                metadata.meta.as_str(),
                metadata.preview.as_str(),
            ] {
                structure.number(value.len() as u64);
                structure.bytes(value.as_bytes());
            }
        }
        structure.number(state.expected_indices.len() as u64);
        for index in &state.expected_indices {
            structure.number(*index as u64);
        }
        let structural_signature = structure.finish();

        let offset = dialog.test_scroll_area_offset;
        let content_size = dialog.test_scroll_area_content_size;
        let mut viewport_signature = StableSignature::new(
            0x435F_5155_4943_4B4E,
            "track-c-quick-notes-visible-viewport",
            dialog.last_rendered_row_rects.len(),
        );
        for value in [
            viewport.min.x,
            viewport.min.y,
            viewport.max.x,
            viewport.max.y,
            content_origin.x,
            content_origin.y,
            content_size.x,
            content_size.y,
            offset.x,
            offset.y,
            state.screen_width,
            state.screen_height,
        ] {
            viewport_signature.number(u64::from(value.to_bits()));
        }
        if let (Some(width), Some(key), Some(rebuilds)) = (
            state.wide_scroll_area_width,
            state.wide_geometry_width,
            state.wide_rebuilds,
        ) {
            viewport_signature.number(1);
            viewport_signature.number(u64::from(width.to_bits()));
            viewport_signature.number(u64::from(state.actual_scroll_area_width.to_bits()));
            viewport_signature.number(key as u64);
            viewport_signature.number(rebuilds);
        }
        for (index, rects) in &dialog.last_rendered_row_rects {
            viewport_signature.number(*index as u64);
            for rect in [
                Some(rects.outer),
                Some(rects.header),
                Some(rects.title),
                Some(rects.meta),
                rects.preview,
                Some(rects.separator),
            ]
            .into_iter()
            {
                if let Some(rect) = rect {
                    viewport_signature.number(1);
                    for value in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
                        viewport_signature.number(u64::from(value.to_bits()));
                    }
                } else {
                    viewport_signature.number(0);
                }
            }
        }
        let viewport_receipt = viewport_signature.finish();
        let revisions = vec![
            dialog.entries_revision.unwrap_or_default(),
            dialog.entries_generation,
            dialog.metadata_generation,
            dialog.projection_generation,
        ];
        let mut complete = StableSignature::new(
            0x435F_5155_4943_4B4F,
            "track-c-quick-notes-output-state",
            dialog.last_rendered_indices.len(),
        );
        complete.number(structural_signature);
        complete.number(viewport_receipt);
        for revision in &revisions {
            complete.number(*revision);
        }
        let output_identity = complete.finish();
        OwnerObservation {
            output_identity,
            structural_signature,
            revision_receipts: revisions,
            viewport_receipt: Some(viewport_receipt),
            work_units: rows_measured,
            work_counters: vec![
                rebuilds,
                rows_measured,
                dialog.last_rendered_indices.len() as u64,
            ],
        }
    }

    fn track_c_notes_settings_label(scenario: &str) -> &'static str {
        match scenario {
            "empty" => "notes_empty_backlinks_on_tasks_on",
            "sparse" => "notes_sparse_backlinks_on_tasks_on",
            "long_unicode" => "notes_long_unicode_backlinks_on_tasks_on",
            "horizontal_empty" => "notes_empty_backlinks_on_tasks_on",
            _ => unreachable!("the owner uses documented Quick Notes scenarios"),
        }
    }

    fn track_c_notes_fixture_signature(notes: &[Note], filter: &str, screen_height: f32) -> u64 {
        use crate::performance::track_c_workloads::{StableSignature, note_fixture_identity};

        let settings = Settings::default().note;
        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B4E,
            "track-c-quick-notes-input",
            notes.len(),
        );
        signature.number(note_fixture_identity(notes));
        signature.number(filter.len() as u64);
        signature.bytes(filter.as_bytes());
        signature.number(u64::from(screen_height.to_bits()));
        signature.number(settings.backlinks_enabled as u64);
        signature.number(settings.task_lists_enabled as u64);
        signature.finish()
    }

    fn track_c_notes_width_transition_fixture_signature(
        notes: &[Note],
        from_width: f32,
        to_width: f32,
        screen_height: f32,
    ) -> u64 {
        use crate::performance::track_c_workloads::{StableSignature, note_fixture_identity};

        let settings = Settings::default().note;
        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B4E,
            "track-c-quick-notes-width-transition-input",
            notes.len(),
        );
        signature.number(note_fixture_identity(notes));
        signature.bytes(b"empty-filter-horizontal-cold-warm-pair");
        signature.number(u64::from(from_width.to_bits()));
        signature.number(u64::from(to_width.to_bits()));
        signature.number(u64::from(screen_height.to_bits()));
        signature.number(settings.backlinks_enabled as u64);
        signature.number(settings.task_lists_enabled as u64);
        signature.finish()
    }

    fn emit_track_c_notes_width_receipt(
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        fixture_signature: u64,
        count: usize,
        cold_type: &str,
        widths_milli: &[u64],
    ) {
        assert_eq!(
            widths_milli.len(),
            crate::performance::track_c_workloads::MEASURED_SAMPLES
        );
        let values = widths_milli
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        eprintln!(
            "TRACK_C_WIDTH_RECEIPT schema=1 owner=run_ui+NotesDialog::ui source={} profile={} mode={:?} telemetry_enabled={} fixture=quick_notes_note_fixture fixture_signature={:016x} count={} viewport=screen_300x640_transition_960x640 settings=notes_empty_backlinks_on_tasks_on cold_type={} scenario=quick_notes_empty_horizontal_width_transition transition=screen_960x640_to_300x640 actual_scroll_area_width_milli=[{}] samples={}",
            source,
            profile,
            mode,
            crate::performance::enabled(),
            fixture_signature,
            count,
            cold_type,
            values,
            widths_milli.len(),
        );
    }

    #[test]
    fn track_c_oracle_quick_notes_dialog_matches_frozen_eager_geometry() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, note_fixture};

        let _test_lock = TEST_MUTEX.lock().unwrap();
        let workspace = TrackCWorkspace::new();
        let fixture = note_fixture(100);
        let scenarios = [
            ("empty", ""),
            ("sparse", "track-a-note-00050"),
            ("long_unicode", "layout-segment-λ"),
        ];
        for (scenario, filter) in scenarios {
            for screen_height in [640.0, 180.0] {
                for cold in [true, false] {
                    let mut state = track_c_prepare_quick_notes_state(
                        workspace.root(),
                        &fixture.values,
                        filter,
                        960.0,
                        screen_height,
                        cold,
                        None,
                    );
                    assert_eq!(state.dialog.search, filter, "scenario {scenario}");
                    let frame = state.frame_index;
                    state.frame_index += 1;
                    let _ = run_track_c_notes_frame(
                        &state.ctx,
                        &mut state.dialog,
                        &mut state.app,
                        2.0 + frame as f64 / 60.0,
                        state.screen_width,
                        state.screen_height,
                    );
                    let _ = track_c_validate_quick_notes_state(&state);
                    drop(state);
                }
            }
        }
        drop(workspace);
    }

    #[test]
    fn track_c_oracle_quick_notes_horizontal_width_transition_matches_frozen_eager_geometry() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, note_fixture};

        let _test_lock = TEST_MUTEX.lock().unwrap();
        let workspace = TrackCWorkspace::new();
        let fixture = note_fixture(100);
        let mut widths = Vec::with_capacity(2);
        for cold in [true, false] {
            let mut state = track_c_prepare_quick_notes_state(
                workspace.root(),
                &fixture.values,
                "",
                300.0,
                640.0,
                cold,
                Some(960.0),
            );
            assert!(state.actual_scroll_area_width < state.wide_scroll_area_width.unwrap());
            widths.push(
                state
                    .dialog
                    .test_scroll_area_inner_rect
                    .expect("the measured Quick Notes ScrollArea rendered")
                    .width(),
            );
            let frame = state.frame_index;
            state.frame_index += 1;
            let _ = run_track_c_notes_frame(
                &state.ctx,
                &mut state.dialog,
                &mut state.app,
                2.0 + frame as f64 / 60.0,
                state.screen_width,
                state.screen_height,
            );
            let _ = track_c_validate_quick_notes_state(&state);
            drop(state);
        }
        assert!((widths[0] - widths[1]).abs() <= 0.5);
        drop(workspace);
    }

    #[test]
    #[ignore = "opt-in Track C owner benchmark; isolated synthetic note fixture only"]
    fn track_c_benchmark_quick_notes_dialog_geometry() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, ReportMetadata, TestFrameOutput, TrackCWorkspace, emit_samples,
            measure_owner, note_fixture,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C mode");
        let _test_lock = TEST_MUTEX.lock().unwrap();
        let workspace = TrackCWorkspace::new();
        for count in mode.note_sizes() {
            let fixture = note_fixture(*count);
            for (scenario, filter) in [
                ("empty", ""),
                ("sparse", "track-a-note-00050"),
                ("long_unicode", "layout-segment-λ"),
            ] {
                for screen_height in [640.0, 180.0] {
                    let fixture_signature =
                        track_c_notes_fixture_signature(&fixture.values, filter, screen_height);
                    for cold in [true, false] {
                        let cold_type = if cold {
                            "geometry_cold_warm_fonts"
                        } else {
                            "geometry_warm_warm_fonts"
                        };
                        let samples = measure_owner(
                            || {
                                track_c_prepare_quick_notes_state(
                                    workspace.root(),
                                    &fixture.values,
                                    filter,
                                    960.0,
                                    screen_height,
                                    cold,
                                    None,
                                )
                            },
                            |state| {
                                let frame = state.frame_index;
                                state.frame_index += 1;
                                TestFrameOutput::from(run_track_c_notes_frame(
                                    &state.ctx,
                                    &mut state.dialog,
                                    &mut state.app,
                                    2.0 + frame as f64 / 60.0,
                                    state.screen_width,
                                    state.screen_height,
                                ))
                            },
                            |state, _frame_output| track_c_validate_quick_notes_state(state),
                        );
                        emit_samples(
                            ReportMetadata {
                                owner: "run_ui+NotesDialog::ui",
                                fixture_name: "quick_notes_note_fixture",
                                fixture_signature,
                                item_count: *count,
                                viewport: if screen_height == 640.0 {
                                    "screen_960x640"
                                } else {
                                    "screen_960x180"
                                },
                                scale_milli: 1_000,
                                font_state: "active_font_revision",
                                settings: track_c_notes_settings_label(scenario),
                                cold_type,
                                mode,
                            },
                            &samples,
                        );
                    }
                }
            }
            let from_width = 960.0;
            let to_width = 300.0;
            let screen_height = 640.0;
            let fixture_signature = track_c_notes_width_transition_fixture_signature(
                &fixture.values,
                from_width,
                to_width,
                screen_height,
            );
            for cold in [true, false] {
                let cold_type = if cold {
                    "geometry_cold_warm_fonts"
                } else {
                    "geometry_warm_warm_fonts"
                };
                let mut width_receipts_milli = Vec::with_capacity(
                    crate::performance::track_c_workloads::WARMUPS
                        + crate::performance::track_c_workloads::MEASURED_SAMPLES,
                );
                let samples = measure_owner(
                    || {
                        track_c_prepare_quick_notes_state(
                            workspace.root(),
                            &fixture.values,
                            "",
                            to_width,
                            screen_height,
                            cold,
                            Some(from_width),
                        )
                    },
                    |state| {
                        let frame = state.frame_index;
                        state.frame_index += 1;
                        TestFrameOutput::from(run_track_c_notes_frame(
                            &state.ctx,
                            &mut state.dialog,
                            &mut state.app,
                            2.0 + frame as f64 / 60.0,
                            state.screen_width,
                            state.screen_height,
                        ))
                    },
                    |state, _frame_output| {
                        let observation = track_c_validate_quick_notes_state(state);
                        let measured_width = state
                            .dialog
                            .test_scroll_area_inner_rect
                            .expect("the measured Quick Notes ScrollArea rendered")
                            .width();
                        width_receipts_milli.push((measured_width * 1_000.0).round() as u64);
                        observation
                    },
                );
                assert_eq!(
                    width_receipts_milli.len(),
                    crate::performance::track_c_workloads::WARMUPS
                        + crate::performance::track_c_workloads::MEASURED_SAMPLES,
                );
                let measured_widths =
                    &width_receipts_milli[crate::performance::track_c_workloads::WARMUPS..];
                assert_eq!(measured_widths.len(), samples.len());
                assert!(measured_widths.iter().all(|width| *width > 0));
                let min_width = measured_widths.iter().min().copied().unwrap();
                let max_width = measured_widths.iter().max().copied().unwrap();
                assert!(
                    max_width.saturating_sub(min_width) <= 1_000,
                    "narrow actual ScrollArea width is stable within one point"
                );
                emit_samples(
                    ReportMetadata {
                        owner: "run_ui+NotesDialog::ui",
                        fixture_name: "quick_notes_note_fixture",
                        fixture_signature,
                        item_count: *count,
                        viewport: "screen_300x640_transition_960x640",
                        scale_milli: 1_000,
                        font_state: "active_font_revision",
                        settings: track_c_notes_settings_label("horizontal_empty"),
                        cold_type,
                        mode,
                    },
                    &samples,
                );
                emit_track_c_notes_width_receipt(
                    mode,
                    fixture_signature,
                    *count,
                    cold_type,
                    measured_widths,
                );
            }
        }
        drop(workspace);
    }

    fn run_track_c_notes_frame(
        ctx: &egui::Context,
        dialog: &mut NotesDialog,
        app: &mut LauncherApp,
        time: f64,
        width: f32,
        height: f32,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                time: Some(time),
                ..Default::default()
            },
            |ctx| dialog.ui(ctx, app),
        )
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
        let mut style = (*wrapped_ctx.style_of(wrapped_ctx.theme())).clone();
        style.wrap_mode = Some(egui::TextWrapMode::Wrap);
        wrapped_ctx.set_style_of(wrapped_ctx.theme(), style);
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
        let large_geometry_capacity = dialog
            .notes_geometry
            .as_ref()
            .unwrap()
            .partial_capacity_bytes();
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
                    let expected_preview_top = expected_top
                        + row.header_height
                        + ctx.style_of(ctx.theme()).spacing.item_spacing.y;
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
        let sparse_geometry = dialog.notes_geometry.as_ref().unwrap();
        assert_eq!(sparse_geometry.rows.len(), 1);
        assert_eq!(sparse_geometry.row_by_identity.len(), 1);
        assert!(
            sparse_geometry.partial_capacity_bytes() < large_geometry_capacity,
            "the filtered geometry and identity map shrink with the one-row projection"
        );
        let sparse_rebuilds = dialog.test_geometry_rebuilds;
        let sparse_rows_measured = dialog.test_geometry_rows_measured;
        let sparse_capacity = sparse_geometry.partial_capacity_bytes();
        for frame in 0..4 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 2.545 + frame as f64 * 0.002);
        }
        assert_eq!(dialog.test_geometry_rebuilds, sparse_rebuilds);
        assert_eq!(dialog.test_geometry_rows_measured, sparse_rows_measured);
        let sparse_geometry = dialog.notes_geometry.as_ref().unwrap();
        assert_eq!(sparse_geometry.rows.len(), 1);
        assert_eq!(sparse_geometry.row_by_identity.len(), 1);
        assert_eq!(sparse_geometry.partial_capacity_bytes(), sparse_capacity);

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
            egui::Popup::is_any_open(&ctx),
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

        let mut style = (*ctx.style_of(ctx.theme())).clone();
        style.override_font_id = Some(egui::FontId::new(18.0, egui::FontFamily::Proportional));
        style.override_text_style = Some(egui::TextStyle::Heading);
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.80);
        let font_style_rebuilds = dialog.test_geometry_rebuilds;
        style.override_text_style = Some(egui::TextStyle::Monospace);
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.82);
        assert_eq!(
            dialog.test_geometry_rebuilds, font_style_rebuilds,
            "override_font_id takes precedence over text-style overrides for title and small RichText"
        );
        let effective_override_font = style.override_font_id.clone().unwrap();
        let geometry = dialog.notes_geometry.as_ref().unwrap();
        assert_eq!(geometry.key.title_font, effective_override_font);
        assert_eq!(geometry.key.small_font, effective_override_font);

        style.override_font_id = Some(egui::FontId::new(21.0, egui::FontFamily::Proportional));
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.83);
        assert!(dialog.test_geometry_rebuilds > font_style_rebuilds);
        let wrap_rebuilds = dialog.test_geometry_rebuilds;
        style.wrap_mode = Some(egui::TextWrapMode::Wrap);
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.84);
        assert!(dialog.test_geometry_rebuilds > wrap_rebuilds);
        let wrap_rebuilds = dialog.test_geometry_rebuilds;
        style.spacing.extra_text_line_spacing = 3.0;
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.85);
        assert!(dialog.test_geometry_rebuilds > wrap_rebuilds);
        let spacing_rebuilds = dialog.test_geometry_rebuilds;
        style.wrap_mode = Some(egui::TextWrapMode::Truncate);
        ctx.set_style_of(ctx.theme(), style.clone());
        render_notes_frame(&ctx, &mut dialog, &mut app, 2.855);
        assert!(dialog.test_geometry_rebuilds > spacing_rebuilds);
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
        let expected_indices = dialog.filtered_indices.clone();
        let viewport_size = dialog
            .test_scroll_area_inner_rect
            .expect("the Notes viewport remains measured")
            .size();
        let eager = eager_notes_geometry_for_projection(
            &egui::Context::default(),
            &dialog.entries,
            &expected_indices,
            viewport_size,
            &style,
            app.note_settings.backlinks_enabled,
            app.note_settings.task_lists_enabled,
        );
        assert_quick_notes_geometry_matches_eager(&dialog, &eager, &expected_indices);
        assert_eq!(
            dialog
                .notes_geometry
                .as_ref()
                .unwrap()
                .key
                .extra_text_line_spacing_bits,
            3.0_f32.to_bits()
        );
        assert_eq!(
            dialog.notes_geometry.as_ref().unwrap().key.title_font,
            style.override_font_id.clone().unwrap()
        );
        assert_eq!(
            dialog.notes_geometry.as_ref().unwrap().key.small_font,
            style.override_font_id.clone().unwrap()
        );

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
    fn quick_notes_geometry_follows_consumer_font_activation() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, workspace.root());
        let entries = vec![
            note(
                "Font activation title",
                "font-activation",
                "A preview with enough text to exercise line layout under the active font.",
            ),
            note("Second title", "font-second", "Second preview row."),
        ];
        let _cache = crate::plugins::note::publish_note_cache_for_test(entries.clone());
        let mut dialog = NotesDialog::default();
        dialog.open();

        let mut changed_definitions = egui::FontDefinitions::default();
        let proportional = changed_definitions
            .families
            .get_mut(&egui::FontFamily::Proportional)
            .expect("default proportional family exists");
        proportional.push(proportional[0].clone());
        let default_definitions = egui::FontDefinitions::default();
        let run_with_installs =
            |dialog: &mut NotesDialog,
             app: &mut LauncherApp,
             time: f64,
             installs: &[egui::FontDefinitions]| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(960.0, 640.0),
                        )),
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| {
                        dialog.ui(ctx, app);
                        for definitions in installs {
                            crate::gui::font_revision::install(ctx, definitions.clone());
                        }
                    },
                )
                .drop_without_applying_deltas();
            };

        for frame in 0..4 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.0 + frame as f64 / 60.0);
        }
        assert_eq!(dialog.filtered_indices, vec![0, 1]);
        let initial_rebuilds = dialog.test_geometry_rebuilds;
        let initial_rows_measured = dialog.test_geometry_rows_measured;
        let initial_geometry = dialog.notes_geometry.as_ref().unwrap();
        assert_eq!(initial_geometry.key.font_revision, 0);
        assert_eq!(initial_rebuilds, 1);

        run_with_installs(&mut dialog, &mut app, 1.1, &[default_definitions.clone()]);
        assert_eq!(crate::gui::font_revision::active_revision(&ctx), 0);
        assert_eq!(dialog.test_geometry_rebuilds, initial_rebuilds);
        assert_eq!(dialog.test_geometry_rows_measured, initial_rows_measured);

        run_with_installs(
            &mut dialog,
            &mut app,
            1.12,
            &[changed_definitions.clone(), default_definitions],
        );
        assert_eq!(crate::gui::font_revision::active_revision(&ctx), 0);
        assert_eq!(dialog.test_geometry_rebuilds, initial_rebuilds);
        assert_eq!(dialog.test_geometry_rows_measured, initial_rows_measured);
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.14);
        assert_eq!(crate::gui::font_revision::active_revision(&ctx), 0);
        assert_eq!(dialog.test_geometry_rebuilds, initial_rebuilds);

        run_with_installs(&mut dialog, &mut app, 1.16, &[changed_definitions.clone()]);
        assert_eq!(crate::gui::font_revision::active_revision(&ctx), 0);
        assert_eq!(dialog.test_geometry_rebuilds, initial_rebuilds);
        assert_eq!(
            dialog.notes_geometry.as_ref().unwrap().key.font_revision,
            0,
            "the in-pass font request does not invalidate already measured geometry"
        );

        render_notes_frame(&ctx, &mut dialog, &mut app, 1.18);
        let active_font_revision = crate::gui::font_revision::active_revision(&ctx);
        assert!(active_font_revision > 0);
        assert_eq!(dialog.test_geometry_rebuilds, initial_rebuilds + 1);
        assert_eq!(
            dialog.test_geometry_rows_measured - initial_rows_measured,
            entries.len() as u64
        );
        assert_eq!(
            dialog.notes_geometry.as_ref().unwrap().key.font_revision,
            active_font_revision
        );
        let active_rebuilds = dialog.test_geometry_rebuilds;
        let active_rows_measured = dialog.test_geometry_rows_measured;
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.20);
        assert_eq!(dialog.test_geometry_rebuilds, active_rebuilds);
        assert_eq!(dialog.test_geometry_rows_measured, active_rows_measured);

        let oracle_ctx = egui::Context::default();
        crate::gui::font_revision::install(&oracle_ctx, changed_definitions);
        let viewport_size = dialog
            .test_scroll_area_inner_rect
            .expect("the active Notes viewport is measured")
            .size();
        let eager = eager_notes_geometry_for_projection(
            &oracle_ctx,
            &dialog.entries,
            &dialog.filtered_indices,
            viewport_size,
            ctx.style_of(ctx.theme()).as_ref(),
            app.note_settings.backlinks_enabled,
            app.note_settings.task_lists_enabled,
        );
        assert_quick_notes_geometry_matches_eager(&dialog, &eager, &dialog.filtered_indices);

        drop(dialog);
        drop(app);
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
        ctx.run_ui(egui::RawInput::default(), |ctx| {
            let old = super::measure_notes_geometry(
                ctx,
                &ctx.style_of(ctx.theme()),
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
                &ctx.style_of(ctx.theme()),
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
                &ctx.style_of(ctx.theme()),
                320.0,
                &without_beta,
                &without_beta_metadata,
                &without_beta_indices,
                3,
                3,
                3,
            );
            geometries = Some((old, new, replacement));
        })
        .drop_without_applying_deltas();
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
        let original_owner_header_id = rendered_note_header_id(&dialog, "popup-000");
        assert_ne!(
            original_owner_header_id,
            rendered_note_header_id(&dialog, "popup-001"),
            "distinct note identities have distinct header widget IDs"
        );

        let row_rects = rendered_note_rect(&dialog, 0);
        let header_rect = row_rects.header;
        let title_pointer = row_rects.title.center();
        let header_pointer = header_gap_point(&row_rects);
        assert!(
            header_rect.contains(header_pointer)
                && dialog
                    .test_scroll_area_inner_rect
                    .is_some_and(|inner| inner.contains(header_pointer)),
            "first popup owner gap is visible: header={header_rect:?}, pointer={header_pointer:?}, inner={:?}",
            dialog.test_scroll_area_inner_rect
        );
        assert!(row_rects.title.contains(title_pointer));
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            title_pointer,
            egui::PointerButton::Secondary,
            1.08,
        );
        assert!(
            dialog
                .menu_owner
                .as_ref()
                .is_some_and(|owner| owner.response.context_menu_opened()),
            "a real secondary click on the title text opens the row-owned context menu"
        );
        assert_eq!(
            dialog.menu_owner.as_ref().unwrap().response.id,
            original_owner_header_id
        );
        render_notes_frame_with_events(
            &ctx,
            &mut dialog,
            &mut app,
            1.12,
            960.0,
            640.0,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "Escape closes the title-owned context menu before testing header padding"
        );
        click_notes_at(
            &ctx,
            &mut dialog,
            &mut app,
            header_pointer,
            egui::PointerButton::Secondary,
            1.2,
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
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(
            dialog
                .menu_owner
                .as_ref()
                .map(|owner| owner.identity.as_str()),
            Some("popup-000")
        );
        assert_eq!(
            dialog.menu_owner.as_ref().unwrap().response.id,
            original_owner_header_id,
            "the menu response has the row's stable header ID"
        );

        let far_row_top = dialog.notes_geometry.as_ref().unwrap().rows[30].top;
        set_notes_scroll_offset(&ctx, &dialog, egui::vec2(0.0, far_row_top));
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.24);
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(dialog.menu_owner.as_ref().unwrap().original_index, 0);
        assert_eq!(
            rendered_note_header_id(&dialog, "popup-000"),
            original_owner_header_id,
            "scrolling keeps the offscreen owner's header ID"
        );
        assert!(dialog.last_rendered_indices.contains(&0));
        assert!(dialog.last_rendered_indices.len() < 32);

        let mut inserted = notes.clone();
        inserted.insert(0, note("Before owner", "popup-before", "body"));
        let inserted_cache = crate::plugins::note::publish_note_cache_for_test(inserted.clone());
        for frame in 0..4 {
            render_notes_frame(&ctx, &mut dialog, &mut app, 1.26 + frame as f64 / 60.0);
        }
        assert!(egui::Popup::is_any_open(&ctx));
        assert_eq!(dialog.menu_owner.as_ref().unwrap().identity, "popup-000");
        assert_eq!(dialog.menu_owner.as_ref().unwrap().original_index, 1);
        assert_eq!(
            rendered_note_header_id(&dialog, "popup-000"),
            original_owner_header_id,
            "inserting a preceding note does not retarget the popup widget ID"
        );
        assert!(dialog.last_rendered_indices.contains(&1));
        assert!(dialog.last_rendered_indices.len() < 32);

        inserted.remove(1);
        let removed_cache = crate::plugins::note::publish_note_cache_for_test(inserted);
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.34);
        assert!(dialog.menu_owner.is_none());
        assert!(
            !egui::Popup::is_any_open(&ctx),
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
        ctx.run_ui(
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
        )
        .drop_without_applying_deltas();
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

    fn rendered_note_header_id(dialog: &NotesDialog, identity: &str) -> egui::Id {
        dialog
            .last_rendered_header_ids
            .iter()
            .find_map(|(rendered_identity, id)| (rendered_identity == identity).then_some(*id))
            .expect("requested note header was built")
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
        crate::performance::workloads::IsolatedWorkspace,
        egui::Context,
        LauncherApp,
    ) {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        save_notes(&[]).unwrap();
        let ctx = egui::Context::default();
        let app = new_isolated_app(&ctx, workspace.root());
        (workspace, ctx, app)
    }

    struct NotesDirOverride {
        previous: Option<std::ffi::OsString>,
    }

    impl NotesDirOverride {
        fn new(path: &std::path::Path) -> Self {
            let previous = std::env::var_os("ML_NOTES_DIR");
            unsafe { std::env::set_var("ML_NOTES_DIR", path) };
            Self { previous }
        }
    }

    impl Drop for NotesDirOverride {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = self.previous.take() {
                    std::env::set_var("ML_NOTES_DIR", previous);
                } else {
                    std::env::remove_var("ML_NOTES_DIR");
                }
            }
        }
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
                        context.run_ui(input, |ctx| dialog.ui(ctx, &mut app))
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
        let (_workspace, _ctx, mut app) = setup();
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
        let (_workspace, _ctx, mut app) = setup();
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
        let (_workspace, _ctx, mut app) = setup();
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
        let (_workspace, ctx, mut app) = setup();
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
        let (workspace, ctx, mut app) = setup();
        let mut alpha = note("Alpha", "alpha", "# Alpha\n\noriginal");
        save_note(&mut alpha, true).unwrap();
        let mut dialog = NotesDialog::default();
        dialog.open();
        render_notes_frame(&ctx, &mut dialog, &mut app, 1.0);
        dialog.open_edit(0);
        dialog.text = "unsaved draft retained on failure".into();
        let entries_before = dialog.entries.clone();
        let blocker = workspace.root().join("not-a-directory");
        std::fs::write(&blocker, "block note directory creation").unwrap();
        let notes_dir_override = NotesDirOverride::new(&blocker);

        let saved = dialog.save_note_draft(
            PendingNoteSave::Existing {
                identity: "alpha".into(),
                content: "# Alpha\n\nshould fail".into(),
            },
            &mut app,
        );
        drop(notes_dir_override);

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

    fn track_c2f_string_capacity_bytes(notes: &[Note]) -> u64 {
        notes
            .iter()
            .map(|note| {
                let mut bytes = note.title.capacity()
                    + note.path.capacity()
                    + note.content.capacity()
                    + note.slug.capacity()
                    + note.alias.as_ref().map_or(0, String::capacity);
                bytes += note.tags.iter().map(String::capacity).sum::<usize>();
                bytes += note.links.iter().map(String::capacity).sum::<usize>();
                bytes += note.aliases.iter().map(String::capacity).sum::<usize>();
                for entity in &note.entity_refs {
                    bytes += entity.id.capacity();
                    bytes += entity.title.as_ref().map_or(0, String::capacity);
                }
                bytes as u64
            })
            .sum()
    }

    fn track_c2f_nested_vector_capacity_bytes(notes: &[Note]) -> u64 {
        notes
            .iter()
            .map(|note| {
                note.tags.capacity() * std::mem::size_of::<String>()
                    + note.links.capacity() * std::mem::size_of::<String>()
                    + note.aliases.capacity() * std::mem::size_of::<String>()
                    + note.entity_refs.capacity()
                        * std::mem::size_of::<crate::common::entity_ref::EntityRef>()
            })
            .sum::<usize>() as u64
    }

    fn track_c2f_capacity_receipt(dialog: &NotesDialog) -> TrackC2FCapacityReceipt {
        let index_string_capacity = dialog.index.iter().map(String::capacity).sum::<usize>();
        let metadata_string_capacity = dialog
            .row_metadata
            .iter()
            .map(|row| row.display_title.capacity() + row.meta.capacity() + row.preview.capacity())
            .sum::<usize>();
        let geometry = dialog.notes_geometry.as_ref();
        TrackC2FCapacityReceipt([
            (dialog.entries.capacity() * std::mem::size_of::<Note>()) as u64,
            track_c2f_string_capacity_bytes(&dialog.entries),
            track_c2f_nested_vector_capacity_bytes(&dialog.entries),
            (dialog.index.capacity() * std::mem::size_of::<String>()) as u64,
            index_string_capacity as u64,
            (dialog.row_metadata.capacity() * std::mem::size_of::<super::NoteRowMetadata>()) as u64,
            metadata_string_capacity as u64,
            (dialog.filtered_indices.capacity() * std::mem::size_of::<usize>()) as u64,
            geometry.map_or(0, |geometry| {
                (geometry.rows.capacity() * std::mem::size_of::<super::NoteRowGeometry>()) as u64
            }),
            geometry.map_or(0, |geometry| geometry.row_identity_capacity_bytes),
            geometry.map_or(0, |geometry| geometry.map_key_capacity_bytes),
            geometry.map_or(0, |geometry| geometry.row_by_identity.len() as u64),
        ])
    }

    fn track_c2f_note_identity(notes: &[Note]) -> u64 {
        crate::performance::track_c_workloads::note_fixture_identity(notes)
    }

    fn track_c2f_signature_string(
        signature: &mut crate::performance::track_c_workloads::StableSignature,
        value: &str,
    ) {
        signature.number(value.len() as u64);
        signature.bytes(value.as_bytes());
    }

    fn track_c2f_fixture_signature(
        raw_fixture: &[Note],
        initial_notes: &[Note],
        expected_notes: &[Note],
        recipe: TrackC2FRecipe,
        screen_width: f32,
        screen_height: f32,
    ) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-input-fixture",
            raw_fixture.len(),
        );
        track_c2f_signature_string(&mut signature, recipe.token());
        signature.number(track_c2f_note_identity(raw_fixture));
        signature.number(track_c2f_note_identity(initial_notes));
        signature.number(track_c2f_note_identity(expected_notes));
        track_c2f_signature_string(&mut signature, recipe.initial_query());
        track_c2f_signature_string(&mut signature, recipe.measured_query());
        let initial_width = if recipe == TrackC2FRecipe::Width960To300 {
            960.0
        } else {
            screen_width
        };
        signature.number(u64::from(initial_width.to_bits()));
        signature.number(u64::from(screen_width.to_bits()));
        signature.number(u64::from(screen_height.to_bits()));
        signature.number(1); // initial backlinks enabled
        signature.number((recipe != TrackC2FRecipe::MetadataSettingsOnly) as u64);
        signature.number(1); // initial task lists enabled
        signature.number(1); // final task lists remain enabled
        signature.finish()
    }

    fn track_c2f_input_values(fixture: &[Note], recipe: TrackC2FRecipe) -> Vec<Note> {
        let mut notes = fixture.to_vec();
        if recipe == TrackC2FRecipe::EarlyWideUnicode {
            let first = notes.first_mut().expect("synthetic fixture is nonempty");
            first.alias = None;
            first.aliases.clear();
            first.content = first
                .content
                .lines()
                .filter(|line| {
                    let trimmed = line.trim_start();
                    !trimmed.starts_with("Alias:") && !trimmed.starts_with("Aliases:")
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        notes
    }

    fn track_c2f_expected_values(initial: &[Note], recipe: TrackC2FRecipe) -> Vec<Note> {
        let mut notes = initial.to_vec();
        match recipe {
            TrackC2FRecipe::RevisionFinalPreview => {
                let last = notes.last_mut().expect("synthetic fixture is nonempty");
                last.content = format!("C2F changed final preview 東京 🙂\r\n{}", last.content);
            }
            TrackC2FRecipe::EarlyWideUnicode => {
                let first = notes.first_mut().expect("synthetic fixture is nonempty");
                let title = "東京🙂W".repeat(256);
                first.title = title.clone();
                let (_, remainder) = first
                    .content
                    .split_once('\n')
                    .expect("synthetic first note starts with a Markdown heading");
                first.content = format!("# {title}\n{remainder}");
            }
            _ => {}
        }
        notes
    }

    fn track_c2f_geometry_identity(geometry: &super::NotesGeometry) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-notes-geometry",
            geometry.rows.len(),
        );
        for value in [
            geometry.key.viewport_width as u64,
            geometry.key.pixels_per_point as u64,
            geometry.key.spacing_x as u64,
            geometry.key.spacing_y as u64,
            geometry.key.interact_width as u64,
            geometry.key.interact_height as u64,
        ] {
            signature.number(value);
        }
        track_c2f_signature_string(&mut signature, &format!("{:?}", geometry.key.title_font));
        track_c2f_signature_string(&mut signature, &format!("{:?}", geometry.key.small_font));
        match geometry.key.wrap_mode {
            Some(value) => {
                signature.number(1);
                track_c2f_signature_string(&mut signature, &format!("{value:?}"));
            }
            None => signature.number(0),
        }
        for row in &geometry.rows {
            signature.number(row.original_index as u64);
            track_c2f_signature_string(&mut signature, &row.identity);
            for value in [
                row.top,
                row.header_height,
                row.header_width,
                row.body_width,
                row.preview_height.unwrap_or(-1.0),
                row.body_height,
                row.separator_top,
                row.visual_bottom,
                row.width_before,
                row.width_after,
            ] {
                signature.number(u64::from(value.to_bits()));
            }
        }
        signature.number(geometry.content_size.x.to_bits() as u64);
        signature.number(geometry.content_size.y.to_bits() as u64);
        let mut map_entries = geometry.row_by_identity.iter().collect::<Vec<_>>();
        map_entries.sort_by(|left, right| left.0.cmp(right.0));
        signature.number(map_entries.len() as u64);
        for (identity, row_index) in map_entries {
            track_c2f_signature_string(&mut signature, identity);
            signature.number(*row_index as u64);
        }
        signature.finish()
    }

    fn track_c2f_metadata_identity(metadata: &[super::NoteRowMetadata]) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-note-row-metadata",
            metadata.len(),
        );
        for row in metadata {
            signature.number(row.original_index as u64);
            track_c2f_signature_string(&mut signature, &row.display_title);
            track_c2f_signature_string(&mut signature, &row.meta);
            track_c2f_signature_string(&mut signature, &row.preview);
        }
        signature.finish()
    }

    fn track_c2f_index_identity(index: &[String]) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature =
            StableSignature::new(0x435F_5155_4943_4B46, "track-c2f-search-index", index.len());
        for text in index {
            track_c2f_signature_string(&mut signature, text);
        }
        signature.finish()
    }

    fn track_c2f_projection_identity(indices: &[usize]) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-filtered-projection",
            indices.len(),
        );
        for index in indices {
            signature.number(*index as u64);
        }
        signature.finish()
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TrackC2FRecipe {
        RevisionFinalPreview,
        MetadataSettingsOnly,
        SearchSparse,
        SearchCaseOnly,
        GeometryCold,
        WarmRepeat,
        EarlyWideUnicode,
        Width960To300,
        FontRevisionReplacement,
    }

    impl TrackC2FRecipe {
        fn token(self) -> &'static str {
            match self {
                Self::RevisionFinalPreview => "revision_final_preview",
                Self::MetadataSettingsOnly => "metadata_settings_only",
                Self::SearchSparse => "search_sparse",
                Self::SearchCaseOnly => "search_case_only",
                Self::GeometryCold => "geometry_cold",
                Self::WarmRepeat => "warm_repeat",
                Self::EarlyWideUnicode => "early_wide_unicode",
                Self::Width960To300 => "width_960_to_300",
                Self::FontRevisionReplacement => "font_revision_replacement",
            }
        }

        fn initial_query(self) -> &'static str {
            match self {
                Self::SearchCaseOnly => "track-a-note-00050",
                _ => "",
            }
        }

        fn measured_query(self) -> &'static str {
            match self {
                Self::SearchSparse => "track-a-note-00050",
                Self::SearchCaseOnly => "TRACK-A-NOTE-00050",
                _ => self.initial_query(),
            }
        }

        fn screen_width(self) -> f32 {
            match self {
                Self::Width960To300 => 300.0,
                _ => 960.0,
            }
        }
    }

    #[derive(Clone, Debug)]
    struct TrackC2FCapacityReceipt([u64; 12]);

    #[derive(Clone, Debug)]
    struct TrackC2FSample {
        elapsed_ns: u64,
        observation: super::c2f_observation::Snapshot,
        snapshot_estimated_clone_bytes: u64,
        snapshot_metric_lock_wait_ns: u64,
        snapshot_metric_hold_ns: u64,
        fixture_id: u64,
        canonical_id: u64,
        index_id: u64,
        metadata_id: u64,
        projection_id: u64,
        projection_selected_indices: u64,
        geometry_id: u64,
        viewport_id: u64,
        complete_output_id: u64,
        revision_receipt: [u64; 10],
        settings_receipt: [u64; 4],
        viewport_receipt: [i64; 8],
        width_receipt: [u64; 4],
        visible_indices: Vec<usize>,
        capacity_before: TrackC2FCapacityReceipt,
        capacity_after: TrackC2FCapacityReceipt,
        geometry_overlap: [u64; 8],
    }

    struct TrackC2FState {
        app: LauncherApp,
        dialog: NotesDialog,
        ctx: egui::Context,
        expected_notes: Vec<Note>,
        expected_index: Vec<String>,
        expected_metadata: Vec<super::NoteRowMetadata>,
        expected_indices: Vec<usize>,
        eager: EagerNotesGeometry,
        recipe: TrackC2FRecipe,
        screen_width: f32,
        screen_height: f32,
        wide_scroll_width: Option<f32>,
        wide_geometry_width: Option<u32>,
        initial_second_width_before: Option<f32>,
        target_viewport: egui::Vec2,
        fixture_id: u64,
        expected_revision: u64,
        current_revision_before: u64,
        dialog_revision_before: u64,
        initial_backlinks: bool,
        expected_backlinks: bool,
        initial_tasks: bool,
        initial_row_key_width_bits: u32,
        initial_scroll_area_width_milli: u64,
        capacity_before: TrackC2FCapacityReceipt,
        cache_guard: Option<crate::plugins::note::NoteCachePublicationGuard>,
        observer: Option<super::c2f_observation::Scope>,
    }

    fn track_c2f_reference_search_index(entries: &[Note]) -> Vec<String> {
        let mut output = Vec::with_capacity(entries.len());
        for entry in entries {
            let mut searchable = String::new();
            searchable.push_str(&entry.content.to_lowercase());
            if let Some(alias) = entry.alias.as_deref() {
                searchable.push('\n');
                searchable.push_str(&alias.to_lowercase());
            }
            for alias in &entry.aliases {
                searchable.push('\n');
                searchable.push_str(&alias.to_lowercase());
            }
            searchable.push('\n');
            searchable.push_str(&entry.slug.to_lowercase());
            for tag in &entry.tags {
                searchable.push('\n');
                searchable.push_str(&tag.to_lowercase());
            }
            output.push(searchable);
        }
        output
    }

    fn track_c2f_reference_checkbox_count(content: &str) -> usize {
        let mut matches = 0;
        for line in content.lines() {
            let line = line.trim_start();
            if line.starts_with("- [ ] ")
                || line.starts_with("- [x] ")
                || line.starts_with("- [X] ")
            {
                matches += 1;
            }
        }
        matches
    }

    fn track_c2f_reference_preview(content: &str) -> String {
        const LIMIT: usize = 120;
        let mut result = String::new();
        let mut characters = 0;
        let mut pending_space = false;
        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("# ") || trimmed.starts_with("Alias:") {
                continue;
            }
            if !result.is_empty() {
                pending_space = true;
            }
            for character in line.chars() {
                if character.is_whitespace() {
                    pending_space |= !result.is_empty();
                    continue;
                }
                if pending_space {
                    if characters == LIMIT {
                        if result.ends_with(' ') {
                            result.pop();
                        }
                        result.push('…');
                        return result;
                    }
                    result.push(' ');
                    characters += 1;
                    pending_space = false;
                }
                if characters == LIMIT {
                    result.push('…');
                    return result;
                }
                result.push(character);
                characters += 1;
            }
        }
        result
    }

    fn track_c2f_reference_metadata(
        notes: &[Note],
        backlinks_enabled: bool,
        task_lists_enabled: bool,
    ) -> Vec<super::NoteRowMetadata> {
        use std::collections::{HashMap, HashSet};

        let mut sources_by_target: HashMap<&str, HashSet<&str>> = HashMap::new();
        for source in notes {
            for target in &source.links {
                if source.slug != *target {
                    sources_by_target
                        .entry(target.as_str())
                        .or_default()
                        .insert(source.slug.as_str());
                }
            }
        }
        notes
            .iter()
            .enumerate()
            .map(|(original_index, note)| {
                let mut fields = vec![format!(
                    "slug: {}",
                    if note.slug.is_empty() {
                        "unsaved"
                    } else {
                        &note.slug
                    }
                )];
                if !note.tags.is_empty() {
                    fields.push(format!("tags: {}", note.tags.join(", ")));
                }
                if backlinks_enabled && !note.slug.is_empty() {
                    let count = sources_by_target
                        .get(note.slug.as_str())
                        .map_or(0, HashSet::len);
                    fields.push(format!("{count} backlinks"));
                }
                if task_lists_enabled {
                    let count = track_c2f_reference_checkbox_count(&note.content);
                    if count != 0 {
                        fields.push(format!("{count} checkboxes"));
                    }
                }
                super::NoteRowMetadata {
                    original_index,
                    display_title: note.alias.as_deref().unwrap_or(&note.title).to_owned(),
                    meta: fields.join(" · "),
                    preview: track_c2f_reference_preview(&note.content),
                }
            })
            .collect()
    }

    fn track_c2f_reference_indices(entries: &[Note], raw_search: &str) -> Vec<usize> {
        let needle = raw_search.to_lowercase();
        track_c2f_reference_search_index(entries)
            .into_iter()
            .enumerate()
            .filter_map(|(index, text)| {
                (needle.is_empty() || text.contains(&needle)).then_some(index)
            })
            .collect()
    }

    fn track_c2f_prepare_state(
        root: &std::path::Path,
        raw_fixture: &[Note],
        recipe: TrackC2FRecipe,
        screen_height: f32,
    ) -> TrackC2FState {
        let ctx = egui::Context::default();
        let mut app = new_isolated_app(&ctx, root);
        app.note_settings.backlinks_enabled = true;
        app.note_settings.task_lists_enabled = true;

        let initial_input = track_c2f_input_values(raw_fixture, recipe);
        let mut cache_guard = Some(crate::plugins::note::publish_note_cache_for_test(
            initial_input,
        ));
        let (initial_revision, initial_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("the owned synthetic cache is healthy");
        assert_eq!(initial_notes.len(), raw_fixture.len());

        let mut dialog = NotesDialog::default();
        dialog.open();
        dialog.search = recipe.initial_query().to_owned();
        for frame in 0..8 {
            render_notes_frame_at(
                &ctx,
                &mut dialog,
                &mut app,
                1.0 + frame as f64 / 60.0,
                960.0,
                screen_height,
            );
        }
        assert_eq!(dialog.entries, initial_notes);
        assert_eq!(dialog.entries_revision, Some(initial_revision));
        let wide_rect = dialog
            .test_scroll_area_inner_rect
            .expect("the settled Quick Notes ScrollArea has a real viewport");
        let wide_geometry_width = dialog
            .notes_geometry
            .as_ref()
            .expect("the initial geometry is settled")
            .key
            .viewport_width;
        let initial_second_width_before = (recipe == TrackC2FRecipe::EarlyWideUnicode).then(|| {
            dialog
                .notes_geometry
                .as_ref()
                .and_then(|geometry| geometry.rows.get(1))
                .expect("the wide-title fixture has a following row")
                .width_before
        });
        let mut target_viewport = wide_rect.size();

        if recipe == TrackC2FRecipe::Width960To300 {
            let calibration_ctx = egui::Context::default();
            let mut calibration_app = new_isolated_app(&calibration_ctx, root);
            calibration_app.note_settings.clone_from(&app.note_settings);
            let mut calibration_dialog = NotesDialog::default();
            calibration_dialog.open();
            for frame in 0..8 {
                render_notes_frame_at(
                    &calibration_ctx,
                    &mut calibration_dialog,
                    &mut calibration_app,
                    1.0 + frame as f64 / 60.0,
                    960.0,
                    screen_height,
                );
            }
            render_notes_frame_at(
                &calibration_ctx,
                &mut calibration_dialog,
                &mut calibration_app,
                2.0,
                300.0,
                screen_height,
            );
            let narrow_rect = calibration_dialog
                .test_scroll_area_inner_rect
                .expect("the separate calibration app rendered a narrow ScrollArea");
            assert!(narrow_rect.width() + 0.5 < wide_rect.width());
            let (calibrated_revision, calibrated_notes) =
                crate::plugins::note::note_cache_snapshot_with_version()
                    .expect("the calibration app did not poison the synthetic note cache");
            assert_eq!(calibrated_revision, initial_revision);
            assert_eq!(calibrated_notes, initial_notes);
            assert_eq!(crate::plugins::note::note_version(), initial_revision);
            target_viewport = narrow_rect.size();
            drop(calibration_dialog);
            drop(calibration_app);
        }

        let dialog_revision_before = dialog.entries_revision.unwrap_or_default();
        let expected_input = track_c2f_expected_values(&initial_notes, recipe);
        let changed = expected_input != initial_notes;
        let (expected_revision, expected_notes) = if changed {
            drop(cache_guard.take());
            cache_guard = Some(crate::plugins::note::publish_note_cache_for_test(
                expected_input,
            ));
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("the changed owned synthetic cache is healthy")
        } else {
            (initial_revision, initial_notes.clone())
        };
        let expected_backlinks = recipe != TrackC2FRecipe::MetadataSettingsOnly;
        let initial_tasks = app.note_settings.task_lists_enabled;
        let expected_index = track_c2f_reference_search_index(&expected_notes);
        let expected_metadata =
            track_c2f_reference_metadata(&expected_notes, expected_backlinks, initial_tasks);
        let expected_indices =
            track_c2f_reference_indices(&expected_notes, recipe.measured_query());
        let style = ctx.style_of(ctx.theme()).as_ref().clone();
        let oracle_ctx = egui::Context::default();
        let eager = eager_notes_geometry_for_projection(
            &oracle_ctx,
            &expected_notes,
            &expected_indices,
            target_viewport,
            &style,
            expected_backlinks,
            initial_tasks,
        );
        let fixture_id = track_c2f_fixture_signature(
            raw_fixture,
            &initial_notes,
            &expected_notes,
            recipe,
            recipe.screen_width(),
            screen_height,
        );
        let current_revision_before = crate::plugins::note::note_version();
        assert_eq!(current_revision_before, expected_revision);
        let capacity_before = track_c2f_capacity_receipt(&dialog);

        TrackC2FState {
            app,
            dialog,
            ctx,
            expected_notes,
            expected_index,
            expected_metadata,
            expected_indices,
            eager,
            recipe,
            screen_width: recipe.screen_width(),
            screen_height,
            wide_scroll_width: (recipe == TrackC2FRecipe::Width960To300)
                .then_some(wide_rect.width()),
            wide_geometry_width: (recipe == TrackC2FRecipe::Width960To300)
                .then_some(wide_geometry_width),
            initial_second_width_before,
            target_viewport,
            fixture_id,
            expected_revision,
            current_revision_before,
            dialog_revision_before,
            initial_backlinks: true,
            expected_backlinks,
            initial_tasks,
            initial_row_key_width_bits: wide_geometry_width,
            initial_scroll_area_width_milli: (wide_rect.width() * 1_000.0).round() as u64,
            capacity_before,
            cache_guard,
            observer: None,
        }
    }

    fn track_c2f_run_owner(state: &mut TrackC2FState, frame: usize) -> egui::FullOutput {
        match state.recipe {
            TrackC2FRecipe::MetadataSettingsOnly => {
                state.app.note_settings.backlinks_enabled = false;
            }
            TrackC2FRecipe::SearchSparse | TrackC2FRecipe::SearchCaseOnly => {
                state.dialog.search = state.recipe.measured_query().to_owned();
            }
            TrackC2FRecipe::GeometryCold => {
                drop(state.dialog.notes_geometry.take());
            }
            TrackC2FRecipe::RevisionFinalPreview
            | TrackC2FRecipe::WarmRepeat
            | TrackC2FRecipe::EarlyWideUnicode
            | TrackC2FRecipe::Width960To300
            | TrackC2FRecipe::FontRevisionReplacement => {}
        }
        run_track_c_notes_frame(
            &state.ctx,
            &mut state.dialog,
            &mut state.app,
            3.0 + frame as f64 / 60.0,
            state.screen_width,
            state.screen_height,
        )
    }

    fn track_c2f_viewport_receipt(dialog: &NotesDialog) -> ([i64; 8], Vec<usize>, u64) {
        let inner = dialog
            .test_scroll_area_inner_rect
            .expect("the measured ScrollArea has a viewport");
        let content = dialog.test_scroll_area_content_size;
        let offset = dialog.test_scroll_area_offset;
        let key_width = dialog
            .notes_geometry
            .as_ref()
            .expect("the measured geometry is installed")
            .key
            .viewport_width;
        let milli = |value: f32| (value * 1_000.0).round() as i64;
        let receipt = [
            key_width as i64,
            milli(inner.width()),
            milli(inner.height()),
            milli(content.x),
            milli(content.y),
            milli(offset.x),
            milli(offset.y),
            dialog.last_rendered_indices.len() as i64,
        ];
        let visible = dialog.last_rendered_indices.clone();
        let mut signature = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-visible-viewport",
            visible.len(),
        );
        for value in receipt {
            signature.number(value as u64);
        }
        for (index, rects) in &dialog.last_rendered_row_rects {
            signature.number(*index as u64);
            for actual_rect in [
                Some(rects.outer),
                Some(rects.header),
                Some(rects.title),
                Some(rects.meta),
                rects.preview,
                Some(rects.separator),
            ] {
                match actual_rect {
                    Some(rect) => {
                        signature.number(1);
                        for value in [rect.min.x, rect.min.y, rect.max.x, rect.max.y] {
                            signature.number(value.to_bits() as u64);
                        }
                    }
                    None => signature.number(0),
                }
            }
        }
        (receipt, visible, signature.finish())
    }

    fn track_c2f_assert_actual_widgets_match_eager(
        dialog: &NotesDialog,
        eager: &EagerNotesGeometry,
        expected_indices: &[usize],
    ) {
        assert!(
            (dialog.test_scroll_area_content_size.x - eager.content_size.x).abs() <= 1.0
                && (dialog.test_scroll_area_content_size.y - eager.content_size.y).abs() <= 1.0,
            "the actual Quick Notes ScrollArea exposes the eager full extent: actual={:?} eager={:?}",
            dialog.test_scroll_area_content_size,
            eager.content_size
        );

        let viewport = dialog
            .test_scroll_area_inner_rect
            .expect("the production Quick Notes ScrollArea rendered");
        let eager_rows = &eager.rows;
        let content_origin = dialog.test_content_origin;
        let first_visible = eager_rows.partition_point(|row| {
            content_origin.y + row.separator.bottom() - eager.content_origin.y <= viewport.min.y
        });
        let after_visible = eager_rows.partition_point(|row| {
            content_origin.y + row.outer.top() - eager.content_origin.y <= viewport.max.y
        });
        let first = first_visible.saturating_sub(2);
        let after = after_visible.saturating_add(2).min(eager_rows.len());
        let expected_visible_indices = eager_rows[first..after]
            .iter()
            .map(|row| row.original_index)
            .collect::<Vec<_>>();
        assert_eq!(
            dialog.last_rendered_indices, expected_visible_indices,
            "the actual dialog renders exactly the eager visible range plus two overscan rows"
        );
        assert!(!dialog.last_rendered_indices.is_empty());
        assert!(
            dialog.last_rendered_indices.len() < 64,
            "the actual dialog builds a bounded set of visible rows"
        );
        assert!(
            dialog
                .last_rendered_indices
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
        assert_eq!(
            dialog.last_rendered_row_rects.len(),
            expected_visible_indices.len(),
            "each built row has a captured set of actual widget rectangles"
        );

        for (actual_index, actual) in &dialog.last_rendered_row_rects {
            let position = expected_indices
                .binary_search(actual_index)
                .expect("rendered original index remains in the full projection");
            let expected = &eager.rows[position];
            assert_eq!(*actual_index, expected.original_index);
            let offset = content_origin - eager.content_origin;
            let compare_rect = |label: &str, actual: egui::Rect, expected: egui::Rect| {
                let expected = expected.translate(offset);
                assert!(
                    actual.min.distance(expected.min) <= 1.0
                        && actual.max.distance(expected.max) <= 1.0,
                    "rendered note {actual_index} {label} differs from eager widget: actual={actual:?} eager={expected:?}"
                );
            };
            compare_rect("outer", actual.outer, expected.outer);
            compare_rect("header", actual.header, expected.header);
            compare_rect("title", actual.title, expected.title);
            compare_rect("meta", actual.meta, expected.meta);
            compare_rect("separator", actual.separator, expected.separator);
            match (actual.preview, expected.preview) {
                (Some(actual), Some(expected)) => compare_rect("preview", actual, expected),
                (None, None) => {}
                mismatch => panic!("rendered note {actual_index} preview differs: {mismatch:?}"),
            }
        }
    }

    fn track_c2f_validate_sample(
        state: &mut TrackC2FState,
        elapsed_ns: u64,
        observation: super::c2f_observation::Snapshot,
        note_snapshot_metric: crate::performance::MetricSnapshot,
    ) -> TrackC2FSample {
        use crate::performance::track_c_workloads::StableSignature;

        let dialog = &state.dialog;
        assert!(dialog.open);
        assert_eq!(dialog.entries, state.expected_notes);
        assert_eq!(dialog.entries_revision, Some(state.expected_revision));
        assert_eq!(dialog.index, state.expected_index);
        assert_eq!(dialog.row_metadata, state.expected_metadata);
        assert_eq!(dialog.filtered_indices, state.expected_indices);
        assert_eq!(
            track_c2f_reference_indices(&dialog.entries, &dialog.search),
            state.expected_indices
        );
        assert_eq!(dialog.search, state.recipe.measured_query());
        assert_eq!(
            state.app.note_settings.backlinks_enabled,
            state.expected_backlinks
        );
        assert_eq!(
            state.app.note_settings.task_lists_enabled,
            state.initial_tasks
        );
        assert_eq!(state.app.note_settings.task_lists_enabled, true);
        let (global_revision, global_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("the current synthetic note cache remains healthy");
        assert_eq!(global_revision, state.expected_revision);
        assert_eq!(global_notes, state.expected_notes);
        assert_eq!(
            crate::plugins::note::note_version(),
            state.expected_revision
        );
        assert_eq!(
            dialog.entries_revision,
            Some(crate::plugins::note::note_version())
        );
        assert_eq!(
            dialog.row_metadata,
            track_c2f_reference_metadata(
                &state.expected_notes,
                state.expected_backlinks,
                state.initial_tasks,
            )
        );
        let geometry = dialog
            .notes_geometry
            .as_ref()
            .expect("geometry is installed");
        assert_quick_notes_geometry_matches_eager(dialog, &state.eager, &state.expected_indices);
        track_c2f_assert_actual_widgets_match_eager(dialog, &state.eager, &state.expected_indices);
        assert_eq!(geometry.rows.len(), state.expected_indices.len());
        assert_eq!(geometry.row_by_identity.len(), geometry.rows.len());
        for (index, row) in geometry.rows.iter().enumerate() {
            assert_eq!(row.original_index, state.expected_indices[index]);
            assert_eq!(geometry.row_by_identity.get(&row.identity), Some(&index));
        }
        assert_eq!(
            observation.painted_rows,
            dialog.last_rendered_indices.len() as u64
        );
        if state.recipe == TrackC2FRecipe::Width960To300 {
            let actual_width = dialog.test_scroll_area_inner_rect.unwrap().width();
            assert!(actual_width + 0.5 < state.wide_scroll_width.unwrap());
            assert_ne!(
                geometry.key.viewport_width,
                state.wide_geometry_width.unwrap()
            );
            assert!((actual_width - state.target_viewport.x).abs() < 1.0);
        } else if matches!(
            state.recipe,
            TrackC2FRecipe::WarmRepeat
                | TrackC2FRecipe::SearchCaseOnly
                | TrackC2FRecipe::GeometryCold
        ) {
            assert_eq!(
                geometry.key.viewport_width,
                state.initial_row_key_width_bits
            );
            assert_eq!(
                (dialog.test_scroll_area_inner_rect.unwrap().width() * 1_000.0).round() as u64,
                state.initial_scroll_area_width_milli
            );
        }
        if state.recipe == TrackC2FRecipe::EarlyWideUnicode {
            let first = &geometry.rows[0];
            let later = geometry
                .rows
                .get(1)
                .expect("the fixture contains later rows");
            assert!(first.width_after > first.width_before);
            assert!(later.width_before > state.initial_second_width_before.unwrap() + 0.5);
            assert!(state.expected_notes[0].title.starts_with("東京🙂W"));
        }
        if state.recipe == TrackC2FRecipe::RevisionFinalPreview {
            assert!(
                state
                    .expected_notes
                    .last()
                    .unwrap()
                    .content
                    .starts_with("C2F changed final preview 東京 🙂\r\n")
            );
        }

        let capacity_after = track_c2f_capacity_receipt(dialog);
        let (viewport_receipt, visible_indices, viewport_id) = track_c2f_viewport_receipt(dialog);
        let width_receipt = [
            u64::from(state.initial_row_key_width_bits),
            u64::from(geometry.key.viewport_width),
            state.initial_scroll_area_width_milli,
            (dialog.test_scroll_area_inner_rect.unwrap().width() * 1_000.0).round() as u64,
        ];
        if state.recipe == TrackC2FRecipe::Width960To300 {
            assert_ne!(width_receipt[0], width_receipt[1]);
            assert!(width_receipt[3] < width_receipt[2]);
        }
        let canonical_id = track_c2f_note_identity(&state.expected_notes);
        let index_id = track_c2f_index_identity(&state.expected_index);
        let metadata_id = track_c2f_metadata_identity(&state.expected_metadata);
        let projection_id = track_c2f_projection_identity(&state.expected_indices);
        let geometry_id = track_c2f_geometry_identity(geometry);
        let mut complete = StableSignature::new(
            0x435F_5155_4943_4B46,
            "track-c2f-complete-owner-state",
            dialog.entries.len(),
        );
        for value in [
            canonical_id,
            index_id,
            metadata_id,
            projection_id,
            geometry_id,
            viewport_id,
            state.app.note_settings.backlinks_enabled as u64,
            state.app.note_settings.task_lists_enabled as u64,
            state.dialog.edit_idx.map_or(u64::MAX, |index| index as u64),
            state.dialog.refresh_requested as u64,
            state.dialog.refresh_retry.is_some() as u64,
            state.dialog.menu_owner.is_some() as u64,
        ] {
            complete.number(value);
        }
        track_c2f_signature_string(&mut complete, &dialog.search);
        let settings_receipt = [
            state.initial_backlinks as u64,
            state.expected_backlinks as u64,
            state.initial_tasks as u64,
            state.initial_tasks as u64,
        ];
        let revision_receipt = [
            state.dialog_revision_before,
            state.current_revision_before,
            crate::plugins::note::note_version(),
            dialog.entries_revision.unwrap_or_default(),
            dialog.entries_generation,
            dialog.metadata_generation,
            dialog.projection_generation,
            geometry.key.entries_generation,
            geometry.key.metadata_generation,
            geometry.key.projection_generation,
        ];
        let complete_output_id = complete.finish();
        let geometry_overlap = [
            observation.geometry_overlap_old_rows_vec_bytes,
            observation.geometry_overlap_old_identity_bytes,
            observation.geometry_overlap_old_map_key_bytes,
            observation.geometry_overlap_old_entries,
            observation.geometry_overlap_new_rows_vec_bytes,
            observation.geometry_overlap_new_identity_bytes,
            observation.geometry_overlap_new_map_key_bytes,
            observation.geometry_overlap_new_entries,
        ];
        assert_eq!(
            observation.geometry_rebuilds,
            match state.recipe {
                TrackC2FRecipe::RevisionFinalPreview
                | TrackC2FRecipe::MetadataSettingsOnly
                | TrackC2FRecipe::SearchSparse
                | TrackC2FRecipe::GeometryCold
                | TrackC2FRecipe::EarlyWideUnicode
                | TrackC2FRecipe::Width960To300
                | TrackC2FRecipe::FontRevisionReplacement => 1,
                _ => 0,
            }
        );
        assert_eq!(
            observation.snapshot_successes,
            u64::from(matches!(
                state.recipe,
                TrackC2FRecipe::RevisionFinalPreview | TrackC2FRecipe::EarlyWideUnicode
            ))
        );
        assert_eq!(
            observation.index_build_calls,
            u64::from(matches!(
                state.recipe,
                TrackC2FRecipe::RevisionFinalPreview | TrackC2FRecipe::EarlyWideUnicode
            ))
        );
        assert_eq!(
            observation.metadata_build_calls,
            u64::from(matches!(
                state.recipe,
                TrackC2FRecipe::RevisionFinalPreview
                    | TrackC2FRecipe::EarlyWideUnicode
                    | TrackC2FRecipe::MetadataSettingsOnly
            )),
            "unexpected metadata build work for recipe {}",
            state.recipe.token()
        );
        if matches!(
            state.recipe,
            TrackC2FRecipe::WarmRepeat | TrackC2FRecipe::SearchCaseOnly
        ) {
            assert_eq!(observation.snapshot_calls, 0);
            assert_eq!(observation.snapshot_successes, 0);
            assert_eq!(observation.snapshot_errors, 0);
            assert_eq!(observation.snapshot_entries, 0);
            assert_eq!(observation.index_build_calls, 0);
            assert_eq!(observation.index_entries, 0);
            assert_eq!(observation.index_utf8_bytes, 0);
            assert_eq!(
                observation.metadata_build_calls,
                0,
                "warm recipe {} must not rebuild metadata",
                state.recipe.token()
            );
            assert_eq!(observation.metadata_entries, 0);
            assert_eq!(observation.metadata_output_utf8_bytes, 0);
            assert_eq!(observation.metadata_backlink_queries, 0);
            assert_eq!(observation.metadata_checkbox_lines, 0);
            assert_eq!(observation.metadata_preview_builds, 0);
            assert_eq!(observation.projection_entries_scanned, 0);
            assert_eq!(observation.projection_indices_materialized, 0);
            assert_eq!(observation.geometry_rebuilds, 0);
            assert_eq!(observation.geometry_rows_measured, 0);
            assert_eq!(observation.geometry_identity_entries, 0);
        }

        TrackC2FSample {
            elapsed_ns,
            snapshot_estimated_clone_bytes: note_snapshot_metric.work_units,
            snapshot_metric_lock_wait_ns: note_snapshot_metric.lock_wait_nanos_total,
            snapshot_metric_hold_ns: note_snapshot_metric.elapsed_nanos_total,
            fixture_id: state.fixture_id,
            canonical_id,
            index_id,
            metadata_id,
            projection_id,
            projection_selected_indices: state.expected_indices.len() as u64,
            geometry_id,
            viewport_id,
            complete_output_id,
            revision_receipt,
            settings_receipt,
            viewport_receipt,
            width_receipt,
            visible_indices,
            capacity_before: state.capacity_before.clone(),
            capacity_after,
            geometry_overlap,
            observation,
        }
    }

    fn track_c2f_sample(state: &mut TrackC2FState, frame: usize) -> TrackC2FSample {
        crate::performance::reset_metrics();
        state.observer = Some(super::c2f_observation::Scope::begin());
        let started = std::time::Instant::now();
        let output = track_c2f_run_owner(state, frame);
        output.drop_without_applying_deltas();
        let elapsed_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let observation = state
            .observer
            .take()
            .expect("observer spans exactly one owner sample")
            .finish();
        let note_snapshot_metric = crate::performance::snapshot_metrics()
            [crate::performance::Metric::NoteSnapshot as usize];
        track_c2f_validate_sample(state, elapsed_ns, observation, note_snapshot_metric)
    }

    fn track_c2f_validate_benchmark_args(
        telemetry_enabled: bool,
        profile: &str,
        mode: Option<&str>,
        source: &str,
    ) -> Result<crate::performance::track_c_workloads::BenchmarkMode, &'static str> {
        use crate::performance::track_c_workloads::BenchmarkMode;

        if !telemetry_enabled {
            return Err("telemetry must be enabled");
        }
        if profile != "fast-dev" {
            return Err("profile must be fast-dev");
        }
        let mode = mode.ok_or("benchmark mode must be explicit")?;
        let mode = BenchmarkMode::parse(mode).map_err(|_| "invalid benchmark mode")?;
        if source == "UNCOMMITTED_SOURCE" {
            if mode != BenchmarkMode::Small {
                return Err("uncommitted source is eligible only for Small mode");
            }
        } else if source.len() != 40 || !source.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("source must be a 40-character hexadecimal commit");
        }
        Ok(mode)
    }

    fn track_c2f_emit(
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        recipe: TrackC2FRecipe,
        count: usize,
        height: f32,
        samples: &[TrackC2FSample],
    ) {
        assert_eq!(
            samples.len(),
            crate::performance::track_c_workloads::MEASURED_SAMPLES
        );
        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_owned());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_owned());
        for token in [&source, &profile, recipe.token()] {
            assert!(!token.chars().any(char::is_whitespace));
        }
        let times = |select: fn(&TrackC2FSample) -> u64| {
            samples
                .iter()
                .map(select)
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let work = |select: fn(&TrackC2FSample) -> u64| {
            samples
                .iter()
                .map(|sample| select(sample))
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let ids = |select: fn(&TrackC2FSample) -> u64| {
            samples
                .iter()
                .map(|sample| format!("{:016x}", select(sample)))
                .collect::<Vec<_>>()
                .join(",")
        };
        let tuple_u64 = |select: fn(&TrackC2FSample) -> Vec<u64>| {
            samples
                .iter()
                .map(|sample| {
                    select(sample)
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(":")
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let tuple_i64 = |select: fn(&TrackC2FSample) -> Vec<i64>| {
            samples
                .iter()
                .map(|sample| {
                    select(sample)
                        .iter()
                        .map(i64::to_string)
                        .collect::<Vec<_>>()
                        .join(":")
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        let residual_ns = samples
            .iter()
            .map(|sample| {
                sample.elapsed_ns.saturating_sub(
                    sample.observation.phase_ns[0]
                        .saturating_add(sample.observation.phase_ns[1])
                        .saturating_add(sample.observation.phase_ns[2])
                        .saturating_add(sample.observation.phase_ns[3])
                        .saturating_add(sample.observation.phase_ns[4]),
                )
            })
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mode = match mode {
            crate::performance::track_c_workloads::BenchmarkMode::Small => "small",
            crate::performance::track_c_workloads::BenchmarkMode::Full => "full",
        };
        let settings = if recipe == TrackC2FRecipe::MetadataSettingsOnly {
            "backlinks_off_tasks_on"
        } else {
            "backlinks_on_tasks_on"
        };
        let viewport = match (recipe, height as u32) {
            (TrackC2FRecipe::Width960To300, _) => "screen_300x640_transition_960x640",
            (_, 180) => "screen_960x180",
            _ => "screen_960x640",
        };
        let visible_indices = samples
            .iter()
            .map(|sample| {
                sample
                    .visible_indices
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(":")
            })
            .collect::<Vec<_>>()
            .join(";");
        let fields = vec![
            "schema=1".to_owned(),
            "stage=quick_notes_refresh_characterization".to_owned(),
            "owner=run_ui+NotesDialog::ui".to_owned(),
            format!("source={source}"),
            format!("profile={profile}"),
            format!("mode={mode}"),
            format!("telemetry_enabled={}", crate::performance::enabled()),
            "observer_enabled=true".to_owned(),
            "warmups=5".to_owned(),
            "samples=20".to_owned(),
            "exclusions=0".to_owned(),
            "fixture=quick_notes_note_fixture".to_owned(),
            format!("recipe={}", recipe.token()),
            format!("count={count}"),
            format!(
                "screen_width_milli={}",
                (recipe.screen_width() * 1_000.0) as u64
            ),
            format!("screen_height_milli={}", (height * 1_000.0) as u64),
            format!("viewport={viewport}"),
            format!("settings={settings}"),
            format!("fixture_signature={:016x}", samples[0].fixture_id),
            format!("fixture_ids=[{}]", ids(|s| s.fixture_id)),
            format!("canonical_ids=[{}]", ids(|s| s.canonical_id)),
            format!("search_index_ids=[{}]", ids(|s| s.index_id)),
            format!("metadata_ids=[{}]", ids(|s| s.metadata_id)),
            format!("projection_ids=[{}]", ids(|s| s.projection_id)),
            format!("geometry_ids=[{}]", ids(|s| s.geometry_id)),
            format!("viewport_ids=[{}]", ids(|s| s.viewport_id)),
            format!("complete_output_ids=[{}]", ids(|s| s.complete_output_id)),
            format!("elapsed_ns=[{}]", times(|s| s.elapsed_ns)),
            format!("snapshot_ns=[{}]", times(|s| s.observation.phase_ns[0])),
            format!("search_index_ns=[{}]", times(|s| s.observation.phase_ns[1])),
            format!(
                "metadata_build_ns=[{}]",
                times(|s| s.observation.phase_ns[2])
            ),
            format!(
                "filtered_projection_ns=[{}]",
                times(|s| s.observation.phase_ns[3])
            ),
            format!(
                "ensure_geometry_ns=[{}]",
                times(|s| s.observation.phase_ns[4])
            ),
            format!(
                "measure_geometry_ns=[{}]",
                times(|s| s.observation.phase_ns[5])
            ),
            format!("owner_residual_ns=[{residual_ns}]"),
            format!(
                "snapshot_calls=[{}]",
                work(|s| s.observation.snapshot_calls)
            ),
            format!(
                "snapshot_successes=[{}]",
                work(|s| s.observation.snapshot_successes)
            ),
            format!(
                "snapshot_errors=[{}]",
                work(|s| s.observation.snapshot_errors)
            ),
            format!(
                "snapshot_note_entries=[{}]",
                work(|s| s.observation.snapshot_entries)
            ),
            format!(
                "snapshot_estimated_clone_bytes=[{}]",
                work(|s| s.snapshot_estimated_clone_bytes)
            ),
            format!(
                "snapshot_metric_lock_wait_ns=[{}]",
                work(|s| s.snapshot_metric_lock_wait_ns)
            ),
            format!(
                "snapshot_metric_hold_ns=[{}]",
                work(|s| s.snapshot_metric_hold_ns)
            ),
            format!(
                "search_index_build_calls=[{}]",
                work(|s| s.observation.index_build_calls)
            ),
            format!(
                "search_index_entries=[{}]",
                work(|s| s.observation.index_entries)
            ),
            format!(
                "search_index_utf8_bytes=[{}]",
                work(|s| s.observation.index_utf8_bytes)
            ),
            format!(
                "metadata_build_calls=[{}]",
                work(|s| s.observation.metadata_build_calls)
            ),
            format!(
                "metadata_entries=[{}]",
                work(|s| s.observation.metadata_entries)
            ),
            format!(
                "metadata_output_utf8_bytes=[{}]",
                work(|s| s.observation.metadata_output_utf8_bytes)
            ),
            format!(
                "metadata_backlink_queries=[{}]",
                work(|s| s.observation.metadata_backlink_queries)
            ),
            format!(
                "metadata_checkbox_lines_visited=[{}]",
                work(|s| s.observation.metadata_checkbox_lines)
            ),
            format!(
                "metadata_preview_builds=[{}]",
                work(|s| s.observation.metadata_preview_builds)
            ),
            format!(
                "projection_calls=[{}]",
                work(|s| s.observation.projection_calls)
            ),
            format!(
                "projection_entries_scanned=[{}]",
                work(|s| s.observation.projection_entries_scanned)
            ),
            format!(
                "projection_indices_materialized=[{}]",
                work(|s| s.observation.projection_indices_materialized)
            ),
            format!(
                "projection_selected_indices=[{}]",
                work(|s| s.projection_selected_indices)
            ),
            format!(
                "geometry_ensure_calls=[{}]",
                work(|s| s.observation.geometry_ensure_calls)
            ),
            format!(
                "geometry_rebuilds=[{}]",
                work(|s| s.observation.geometry_rebuilds)
            ),
            format!(
                "geometry_rows_measured=[{}]",
                work(|s| s.observation.geometry_rows_measured)
            ),
            format!(
                "geometry_identity_entries=[{}]",
                work(|s| s.observation.geometry_identity_entries)
            ),
            format!("painted_rows=[{}]", work(|s| s.observation.painted_rows)),
            format!(
                "revision_receipts=[{}]",
                tuple_u64(|s| s.revision_receipt.to_vec())
            ),
            format!(
                "settings_receipts=[{}]",
                tuple_u64(|s| s.settings_receipt.to_vec())
            ),
            format!(
                "viewport_receipts=[{}]",
                tuple_i64(|s| s.viewport_receipt.to_vec())
            ),
            format!(
                "width_receipts=[{}]",
                tuple_u64(|s| s.width_receipt.to_vec())
            ),
            format!("visible_original_indices=[{visible_indices}]"),
            format!(
                "capacity_before=[{}]",
                tuple_u64(|s| s.capacity_before.0.to_vec())
            ),
            format!(
                "capacity_after=[{}]",
                tuple_u64(|s| s.capacity_after.0.to_vec())
            ),
            format!(
                "geometry_overlap=[{}]",
                tuple_u64(|s| s.geometry_overlap.to_vec())
            ),
        ];
        assert!(
            fields
                .iter()
                .all(|field| !field.chars().any(char::is_whitespace))
        );
        eprintln!("TRACK_C2F_NOTES_RESULT {}", fields.join(" "));
    }

    #[test]
    fn track_c2f_oracle_quick_notes_recipe_contract() {
        use crate::performance::track_c_workloads::{TrackCWorkspace, note_fixture};

        let _lock = TEST_MUTEX.lock().unwrap();
        let workspace = TrackCWorkspace::new();
        assert_eq!(
            track_c2f_validate_benchmark_args(
                true,
                "fast-dev",
                Some("small"),
                "UNCOMMITTED_SOURCE",
            ),
            Ok(crate::performance::track_c_workloads::BenchmarkMode::Small)
        );
        assert_eq!(
            track_c2f_validate_benchmark_args(
                true,
                "fast-dev",
                Some("full"),
                "0123456789abcdef0123456789abcdef01234567",
            ),
            Ok(crate::performance::track_c_workloads::BenchmarkMode::Full)
        );
        assert!(
            track_c2f_validate_benchmark_args(
                false,
                "fast-dev",
                Some("small"),
                "UNCOMMITTED_SOURCE",
            )
            .is_err()
        );
        assert!(
            track_c2f_validate_benchmark_args(
                true,
                "unspecified-profile",
                Some("small"),
                "UNCOMMITTED_SOURCE",
            )
            .is_err()
        );
        assert!(
            track_c2f_validate_benchmark_args(true, "fast-dev", None, "UNCOMMITTED_SOURCE",)
                .is_err()
        );
        assert!(
            track_c2f_validate_benchmark_args(
                true,
                "fast-dev",
                Some("large"),
                "UNCOMMITTED_SOURCE",
            )
            .is_err()
        );
        assert!(track_c2f_validate_benchmark_args(
            true,
            "fast-dev",
            Some("full"),
            "UNCOMMITTED_SOURCE",
        )
        .is_err());
        assert!(
            track_c2f_validate_benchmark_args(true, "fast-dev", Some("small"), "not-a-commit",)
                .is_err()
        );

        let mut text = format!("# Ignored\r\nAlias: Hidden\r\n{}   tail", "界".repeat(120));
        text.push_str("\r\n- [ ] final task");
        let preview = track_c2f_reference_preview(&text);
        assert_eq!(preview, format!("{}…", "界".repeat(120)));
        assert_eq!(preview.chars().count(), 121);
        assert!(text.contains("\r\n"));
        assert_eq!(track_c2f_reference_checkbox_count(&text), 1);
        assert_eq!(short_preview(&text), preview);
        let preview_note = note("Preview boundary", "preview-boundary", &text);
        let preview_metadata = super::build_row_metadata(&[preview_note], false, true).unwrap();
        assert_eq!(preview_metadata[0].preview, preview);

        let space_boundary = format!(
            "# Ignored\r\nAlias: Hidden\r\n{} z\r\n- [ ] task",
            "界".repeat(119)
        );
        let expected_space_boundary = format!("{} …", "界".repeat(119));
        assert_eq!(
            track_c2f_reference_preview(&space_boundary),
            expected_space_boundary
        );
        assert_eq!(short_preview(&space_boundary), expected_space_boundary);
        assert_eq!(expected_space_boundary.chars().nth(119), Some(' '));
        assert_eq!(expected_space_boundary.chars().nth(120), Some('…'));

        for recipe in [
            TrackC2FRecipe::RevisionFinalPreview,
            TrackC2FRecipe::MetadataSettingsOnly,
            TrackC2FRecipe::SearchSparse,
            TrackC2FRecipe::SearchCaseOnly,
            TrackC2FRecipe::GeometryCold,
            TrackC2FRecipe::WarmRepeat,
            TrackC2FRecipe::EarlyWideUnicode,
            TrackC2FRecipe::Width960To300,
        ] {
            let fixture = note_fixture(100);
            for height in if matches!(
                recipe,
                TrackC2FRecipe::GeometryCold | TrackC2FRecipe::WarmRepeat
            ) {
                vec![640.0, 180.0]
            } else {
                vec![640.0]
            } {
                let mut state =
                    track_c2f_prepare_state(workspace.root(), &fixture.values, recipe, height);
                let sample = track_c2f_sample(&mut state, 0);
                assert_ne!(sample.complete_output_id, 0);
                assert_eq!(sample.fixture_id, state.fixture_id);
                drop(state);
            }
        }

        let fixture = note_fixture(100);
        let mut atlas_state = track_c2f_prepare_state(
            workspace.root(),
            &fixture.values,
            TrackC2FRecipe::FontRevisionReplacement,
            640.0,
        );
        let old_font_revision = atlas_state
            .dialog
            .notes_geometry
            .as_ref()
            .expect("setup settled the old geometry")
            .key
            .font_revision;
        let old_geometry_key = atlas_state
            .dialog
            .notes_geometry
            .as_ref()
            .expect("setup settled the old geometry")
            .key
            .clone();
        let mut definitions = egui::FontDefinitions::default();
        let duplicate = definitions
            .font_data
            .get("Ubuntu-Light")
            .expect("egui default font definitions contain Ubuntu-Light")
            .clone();
        definitions
            .font_data
            .insert("C2F-unused-atlas-rejection".to_owned(), duplicate);
        crate::gui::font_revision::install(&atlas_state.ctx, definitions.clone());

        let oracle_ctx = egui::Context::default();
        crate::gui::font_revision::install(&oracle_ctx, definitions);
        let style = atlas_state
            .ctx
            .style_of(atlas_state.ctx.theme())
            .as_ref()
            .clone();
        atlas_state.eager = eager_notes_geometry_for_projection(
            &oracle_ctx,
            &atlas_state.expected_notes,
            &atlas_state.expected_indices,
            atlas_state.target_viewport,
            &style,
            atlas_state.expected_backlinks,
            atlas_state.initial_tasks,
        );
        let atlas_sample = track_c2f_sample(&mut atlas_state, 99);
        assert_eq!(atlas_sample.observation.geometry_rebuilds, 1);
        assert_eq!(atlas_sample.observation.geometry_rows_measured, 100);
        let installed_geometry = atlas_state
            .dialog
            .notes_geometry
            .as_ref()
            .expect("geometry was rebuilt against the active font revision");
        assert_ne!(installed_geometry.key.font_revision, old_font_revision);
        let mut expected_geometry_key = old_geometry_key;
        expected_geometry_key.font_revision =
            crate::gui::font_revision::active_revision(&atlas_state.ctx);
        assert_eq!(installed_geometry.key, expected_geometry_key);
        drop(atlas_state);
    }

    #[test]
    #[ignore = "opt-in C2-F Quick Notes characterization; owned synthetic fixtures only"]
    fn track_c2f_benchmark_quick_notes_characterization() {
        use crate::performance::track_c_workloads::{BenchmarkMode, TrackCWorkspace, note_fixture};

        let source = std::env::var("ML_TRACK_C_SOURCE_SHA")
            .unwrap_or_else(|_| "UNCOMMITTED_SOURCE".to_owned());
        let profile = std::env::var("ML_TRACK_C_PROFILE")
            .unwrap_or_else(|_| "unspecified-profile".to_owned());
        let mode_argument = std::env::var("ML_TRACK_C_BENCH_MODE").ok();
        let mode = track_c2f_validate_benchmark_args(
            crate::performance::enabled(),
            &profile,
            mode_argument.as_deref(),
            &source,
        )
        .unwrap_or_else(|error| panic!("invalid C2-F benchmark invocation: {error}"));
        let _lock = TEST_MUTEX.lock().unwrap();
        let workspace = TrackCWorkspace::new();
        for count in mode.note_sizes() {
            let fixture = note_fixture(*count);
            for recipe in [
                TrackC2FRecipe::RevisionFinalPreview,
                TrackC2FRecipe::MetadataSettingsOnly,
                TrackC2FRecipe::SearchSparse,
                TrackC2FRecipe::SearchCaseOnly,
                TrackC2FRecipe::GeometryCold,
                TrackC2FRecipe::WarmRepeat,
                TrackC2FRecipe::EarlyWideUnicode,
                TrackC2FRecipe::Width960To300,
            ] {
                let heights: &[f32] = if matches!(
                    recipe,
                    TrackC2FRecipe::GeometryCold | TrackC2FRecipe::WarmRepeat
                ) {
                    &[640.0, 180.0]
                } else {
                    &[640.0]
                };
                for height in heights {
                    let mut warmups =
                        Vec::with_capacity(crate::performance::track_c_workloads::WARMUPS);
                    for frame in 0..crate::performance::track_c_workloads::WARMUPS {
                        let mut state = track_c2f_prepare_state(
                            workspace.root(),
                            &fixture.values,
                            recipe,
                            *height,
                        );
                        let sample = track_c2f_sample(&mut state, frame);
                        warmups.push(sample);
                        drop(state);
                    }
                    assert_eq!(
                        warmups.len(),
                        crate::performance::track_c_workloads::WARMUPS
                    );
                    let mut samples =
                        Vec::with_capacity(crate::performance::track_c_workloads::MEASURED_SAMPLES);
                    for frame in 0..crate::performance::track_c_workloads::MEASURED_SAMPLES {
                        let mut state = track_c2f_prepare_state(
                            workspace.root(),
                            &fixture.values,
                            recipe,
                            *height,
                        );
                        let sample = track_c2f_sample(&mut state, frame);
                        samples.push(sample);
                        drop(state);
                    }
                    track_c2f_emit(mode, recipe, *count, *height, &samples);
                }
            }
        }
        drop(workspace);
    }
}
