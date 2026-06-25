use crate::actions::screenshot::{capture, Mode as ScreenshotMode};
use crate::common::slug::slugify;
use crate::gui::LauncherApp;
use crate::notes_markdown::{
    analyze_markdown, task_list::toggle_task_marker, MarkdownAnalysis, MarkdownCallout,
    MarkdownHeading, MarkdownSection, MarkdownTaskItem,
};
use crate::plugins::note::{
    append_note, assets_dir, available_tags, extract_aliases, image_files, load_notes,
    note_alias_map_snapshot, note_cache_snapshot, note_link_menu_targets_snapshot, note_version,
    resolve_note_query, save_note, Note, NoteExternalOpen, NoteLinkMenuTarget, NoteTarget,
};
use crate::plugins::todo::{load_todos, todo_version, TODO_FILE};
use crate::settings::{NoteSettings, NoteViewMode};
use eframe::egui::{self, popup, Color32, FontId, Key};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_toast::{Toast, ToastKind, ToastOptions};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use image::imageops::FilterType;
use once_cell::sync::Lazy;
use regex::Regex;
use rfd::FileDialog;
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use url::Url;

const BACKLINK_PAGE_SIZE: usize = 12;
const HEAVY_RECOMPUTE_IDLE_DEBOUNCE: Duration = Duration::from_millis(250);
const NOTE_LINK_CONTEXT_MENU_RESULT_LIMIT: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BacklinkTab {
    LinkedTodos,
    RelatedNotes,
    Mentions,
}

impl BacklinkTab {
    fn label(self) -> &'static str {
        match self {
            BacklinkTab::LinkedTodos => "Linked Todos",
            BacklinkTab::RelatedNotes => "Related Notes",
            BacklinkTab::Mentions => "Mentions",
        }
    }
}

#[derive(Clone)]
struct BacklinkRow {
    title: String,
    type_badge: String,
    updated: String,
    snippet: String,
    reason: String,
    note_slug: Option<String>,
    todo_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NoteOutlineRow {
    level: u8,
    title: String,
    normalized_anchor: String,
    line_index: usize,
    char_index: usize,
    collapsible: bool,
    collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkMenuResultsKey {
    notes_version: u64,
    current_slug: String,
    query: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkMenuResult {
    slug: String,
    display_title: String,
    search_text: String,
}

static IMAGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());
static TODO_TOKEN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@todo:([A-Za-z0-9_-]+)").unwrap());

fn clamp_char_index(s: &str, char_index: usize) -> usize {
    char_index.min(s.chars().count())
}

fn char_to_byte_index(s: &str, char_index: usize) -> usize {
    let clamped = clamp_char_index(s, char_index);
    s.char_indices()
        .nth(clamped)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| s.len())
}

fn byte_to_char_index(s: &str, byte_index: usize) -> usize {
    let mut clamped = byte_index.min(s.len());
    while clamped > 0 && !s.is_char_boundary(clamped) {
        clamped -= 1;
    }
    s[..clamped].chars().count()
}

fn char_range_to_byte_range(s: &str, start: usize, end: usize) -> (usize, usize) {
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    (char_to_byte_index(s, start), char_to_byte_index(s, end))
}

fn callout_insert_source(kind: &str) -> String {
    format!("> [!{}] Title\n> Body\n", kind.to_ascii_uppercase())
}

fn insert_callout_at_char(content: &str, char_index: usize, kind: &str) -> (String, usize) {
    let byte_index = char_to_byte_index(content, char_index);
    let insert = callout_insert_source(kind);
    let mut updated = String::with_capacity(content.len() + insert.len());
    updated.push_str(&content[..byte_index]);
    updated.push_str(&insert);
    updated.push_str(&content[byte_index..]);
    (updated, char_index + insert.chars().count())
}

fn alias_metadata_line(alias: &str) -> String {
    format!("Alias: {}", alias.trim())
}

fn alias_metadata_key(alias: &str) -> String {
    alias.trim().to_lowercase()
}

fn aliases_metadata_line(aliases: &[String]) -> String {
    if aliases.len() == 1 {
        alias_metadata_line(&aliases[0])
    } else {
        format!("Aliases: {}", aliases.join(", "))
    }
}

fn alias_metadata_indexes_and_values(lines: &[&str]) -> (Vec<usize>, Vec<String>) {
    let mut indexes = Vec::new();
    let mut aliases = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if let Some(alias) = trimmed.strip_prefix("Alias:") {
            indexes.push(idx);
            let alias = alias.trim();
            let key = alias_metadata_key(alias);
            if !alias.is_empty()
                && !aliases
                    .iter()
                    .any(|a: &String| alias_metadata_key(a) == key)
            {
                aliases.push(alias.to_string());
            }
        } else if let Some(alias_list) = trimmed.strip_prefix("Aliases:") {
            indexes.push(idx);
            for alias in alias_list
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
            {
                let key = alias_metadata_key(alias);
                if !aliases
                    .iter()
                    .any(|a: &String| alias_metadata_key(a) == key)
                {
                    aliases.push(alias.to_string());
                }
            }
        }
    }
    (indexes, aliases)
}

pub(crate) fn add_alias_metadata(content: &str, alias: &str) -> String {
    let alias = alias.trim();
    if alias.is_empty() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (indexes, mut aliases) = alias_metadata_indexes_and_values(&line_refs);
    let key = alias_metadata_key(alias);
    if aliases.iter().any(|a| alias_metadata_key(a) == key) {
        return content.to_string();
    }
    aliases.push(alias.to_string());
    if let Some(first_idx) = indexes.first().copied() {
        lines[first_idx] = aliases_metadata_line(&aliases);
        let index_set: HashSet<usize> = indexes.into_iter().skip(1).collect();
        lines = lines
            .into_iter()
            .enumerate()
            .filter_map(|(idx, line)| (!index_set.contains(&idx)).then_some(line))
            .collect();
    } else {
        let insert_at = usize::from(lines.first().is_some_and(|line| line.starts_with("# ")));
        lines.insert(insert_at, alias_metadata_line(alias));
    }
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn remove_alias_metadata(content: &str, alias: &str) -> String {
    let alias = alias.trim();
    if alias.is_empty() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (indexes, aliases) = alias_metadata_indexes_and_values(&line_refs);
    let remove_key = alias_metadata_key(alias);
    let aliases: Vec<String> = aliases
        .into_iter()
        .filter(|a| alias_metadata_key(a) != remove_key)
        .collect();
    if indexes.is_empty() {
        return content.to_string();
    }
    let index_set: HashSet<usize> = indexes.iter().copied().skip(1).collect();
    if let Some(first_idx) = indexes.first().copied() {
        if aliases.is_empty() {
            lines = lines
                .into_iter()
                .enumerate()
                .filter_map(|(idx, line)| (!indexes.contains(&idx)).then_some(line))
                .collect();
        } else {
            lines[first_idx] = aliases_metadata_line(&aliases);
            lines = lines
                .into_iter()
                .enumerate()
                .filter_map(|(idx, line)| (!index_set.contains(&idx)).then_some(line))
                .collect();
        }
    }
    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn rename_alias_metadata(content: &str, old_alias: &str, new_alias: &str) -> String {
    let new_alias = new_alias.trim();
    if new_alias.is_empty() {
        remove_alias_metadata(content, old_alias)
    } else {
        add_alias_metadata(&remove_alias_metadata(content, old_alias), new_alias)
    }
}

fn wrap_char_range_in_callout(
    content: &str,
    start_char: usize,
    end_char: usize,
    kind: &str,
) -> (String, std::ops::Range<usize>) {
    let (start_byte, end_byte) = char_range_to_byte_range(content, start_char, end_char);
    let start_char = clamp_char_index(content, start_char.min(end_char));
    let selected = &content[start_byte..end_byte];
    let mut quoted = String::new();
    for (idx, line) in selected.split('\n').enumerate() {
        if idx > 0 {
            quoted.push('\n');
        }
        quoted.push_str("> ");
        quoted.push_str(line);
    }
    let replacement = format!("> [!{}] Title\n{quoted}", kind.to_ascii_uppercase());
    let mut updated =
        String::with_capacity(content.len() - (end_byte - start_byte) + replacement.len());
    updated.push_str(&content[..start_byte]);
    updated.push_str(&replacement);
    updated.push_str(&content[end_byte..]);
    let body_start = start_char
        + format!("> [!{}] Title\n> ", kind.to_ascii_uppercase())
            .chars()
            .count();
    let body_end = body_start + quoted.chars().count().saturating_sub("> ".chars().count());
    (updated, body_start..body_end)
}

fn preprocess_note_links(
    content: &str,
    current_slug: &str,
    todo_labels: &HashMap<String, String>,
) -> String {
    static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
    let mut out = WIKI_RE
        .replace_all(content, |caps: &regex::Captures| {
            let text = &caps[1];
            let target = text.split('|').next().unwrap_or(text).trim();
            let slug = slugify(target);
            if slug == current_slug {
                caps[0].to_string()
            } else {
                format!("[{text}](note://{slug})")
            }
        })
        .to_string();

    out = TODO_TOKEN_RE
        .replace_all(&out, |caps: &regex::Captures| {
            let id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let label = todo_labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.to_string());
            format!("[{label}](todo://{id})")
        })
        .to_string();
    out
}

fn preprocess_preview_markdown(
    content: &str,
    current_slug: &str,
    todo_labels: &HashMap<String, String>,
) -> String {
    preprocess_note_links(content, current_slug, todo_labels)
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedImageTarget {
    rel: String,
    full: PathBuf,
    width: Option<f32>,
}

fn parse_note_image_target(target: &str) -> ParsedImageTarget {
    let (rel, width) = if let Some((path, width)) = target.split_once('|') {
        (path.trim(), width.trim().parse::<f32>().ok())
    } else {
        (target.trim(), None)
    };
    let full = if let Some(stripped) = rel.strip_prefix("assets/") {
        assets_dir().join(stripped)
    } else {
        PathBuf::from(rel)
    };
    ParsedImageTarget {
        rel: rel.to_string(),
        full,
        width,
    }
}

fn handle_markdown_links(ui: &egui::Ui, app: &mut LauncherApp) {
    if let Some(mut open_url) = ui.ctx().output_mut(|o| o.open_url.take()) {
        if let Ok(url) = Url::parse(&open_url.url) {
            if url.scheme() == "note" {
                if let Some(slug) = url.host_str() {
                    app.open_note_panel(slug, None);
                }
            } else if url.scheme() == "todo" {
                if let Some(todo_id) = url.host_str() {
                    let todos = load_todos(TODO_FILE).unwrap_or_default();
                    if let Some((idx, _)) = todos.iter().enumerate().find(|(_, t)| t.id == todo_id)
                    {
                        app.todo_view_dialog.open_edit(idx);
                    } else {
                        app.todo_view_dialog.open();
                    }
                }
            } else {
                ui.ctx().open_url(open_url);
            }
        } else {
            if open_url.url.starts_with("www.") {
                open_url.url = format!("https://{}", open_url.url);
            }
            ui.ctx().open_url(open_url);
        }
    }
}

pub struct NotePanel {
    pub open: bool,
    note: Note,
    link_search: String,
    link_menu_targets: Vec<LinkMenuResult>,
    link_menu_targets_version: Option<u64>,
    link_menu_results: Vec<LinkMenuResult>,
    link_menu_results_key: Option<LinkMenuResultsKey>,
    #[cfg(test)]
    link_menu_target_refresh_count: usize,
    #[cfg(test)]
    link_menu_result_refresh_count: usize,
    image_search: String,
    tag_search: String,
    view_mode: NoteViewMode,
    markdown_cache: CommonMarkCache,
    markdown_analysis: Option<MarkdownAnalysis>,
    markdown_analysis_source_hash: Option<u64>,
    collapsed_sections: HashSet<String>,
    ui_state_error_reported: bool,
    preview_render_error_reported: bool,
    outline_open: bool,
    outline_width: f32,
    outline_filter: String,
    selected_outline_heading: Option<String>,
    pending_scroll_target: Option<String>,
    image_cache: HashMap<std::path::PathBuf, egui::TextureHandle>,
    overwrite_prompt: bool,
    show_open_with_menu: bool,
    show_metadata: bool,
    tags_expanded: bool,
    links_expanded: bool,
    backlink_tab: BacklinkTab,
    backlink_page: usize,
    pending_selection: Option<(usize, usize)>,
    link_dialog_open: bool,
    link_text: String,
    link_url: String,
    link_new_dialog_open: bool,
    link_new_name: String,
    new_alias: String,
    alias_rename_inputs: HashMap<String, String>,

    // Focus management: avoid requesting focus on an ID that does not correspond to
    // an existing widget in the current frame. This prevents AccessKit from seeing
    // a focused node that is not present in the accessibility tree.
    focus_textedit_next_frame: bool,
    last_textedit_id: Option<egui::Id>,
    derived: NoteDerivedView,
    fast_derived_dirty: bool,
    heavy_recompute_requested: bool,
    last_edit_at_secs: Option<f64>,
    last_notes_version: u64,
    last_todo_revision: u64,
    last_backlink_content_hash: Option<u64>,
    last_alias_map_hash: u64,
    #[cfg(test)]
    heavy_recompute_count: usize,
    #[cfg(test)]
    last_ui_sections: NotePanelUiSections,
    #[cfg(test)]
    metadata_details_render_count: usize,
    #[cfg(test)]
    backlinks_render_count: usize,
}

#[cfg(test)]
#[derive(Default, Clone, Copy)]
struct NotePanelUiSections {
    tags_visible: bool,
    links_visible: bool,
    backlinks_visible: bool,
    content_visible: bool,
}

#[derive(Default, Clone)]
struct NoteDerivedView {
    tags: Vec<String>,
    wiki_links: Vec<String>,
    external_links: Vec<(String, String)>,
    backlink_rows_linked_todos: Vec<BacklinkRow>,
    backlink_rows_related_notes: Vec<BacklinkRow>,
    backlink_rows_mentions: Vec<BacklinkRow>,
    todo_label_map: HashMap<String, String>,
}

impl NotePanel {
    fn section_key_for_slug(note_slug: &str, section: &MarkdownSection) -> String {
        format!(
            "{}::{}::{}::{}",
            note_slug,
            section.heading.normalized_anchor,
            section.heading.title,
            section.heading.line_index
        )
    }

    fn section_key(&self, section: &MarkdownSection) -> String {
        Self::section_key_for_slug(&self.note.slug, section)
    }

    fn is_main_title_section(&self, section: &MarkdownSection) -> bool {
        section.heading.level == 1
            && section.heading.line_index == 0
            && section.heading.title == self.note.title
    }

    fn collapsible_sections(&mut self, enabled: bool) -> Vec<MarkdownSection> {
        if !enabled {
            return Vec::new();
        }
        self.markdown_analysis().sections.clone()
    }

    fn collapsed_body_ranges(
        note_slug: &str,
        sections: &[MarkdownSection],
        collapsed_sections: &HashSet<String>,
        enabled: bool,
    ) -> Vec<std::ops::Range<usize>> {
        if !enabled {
            return Vec::new();
        }
        sections
            .iter()
            .filter(|section| {
                collapsed_sections.contains(&Self::section_key_for_slug(note_slug, section))
            })
            .map(|section| section.body_byte_range.clone())
            .collect()
    }

    fn range_is_hidden(range: &std::ops::Range<usize>, hidden: &[std::ops::Range<usize>]) -> bool {
        hidden
            .iter()
            .any(|hidden| range.start >= hidden.start && range.end <= hidden.end)
    }

    fn outline_rows_from_headings(
        note_slug: &str,
        content: &str,
        headings: &[MarkdownHeading],
        sections: &[MarkdownSection],
        collapsed_sections: &HashSet<String>,
        max_outline_depth: usize,
        collapsible_sections_enabled: bool,
        filter: &str,
    ) -> Vec<NoteOutlineRow> {
        let max_depth = max_outline_depth.clamp(1, 6) as u8;
        let filter = filter.trim().to_lowercase();
        headings
            .iter()
            .filter(|heading| heading.level <= max_depth)
            .filter(|heading| filter.is_empty() || heading.title.to_lowercase().contains(&filter))
            .map(|heading| {
                let section = sections.iter().find(|section| {
                    section.heading.line_index == heading.line_index
                        && section.heading.normalized_anchor == heading.normalized_anchor
                });
                let (collapsible, collapsed) = section
                    .map(|section| {
                        let key = Self::section_key_for_slug(note_slug, section);
                        (
                            collapsible_sections_enabled,
                            collapsed_sections.contains(&key),
                        )
                    })
                    .unwrap_or((false, false));
                NoteOutlineRow {
                    level: heading.level,
                    title: heading.title.clone(),
                    normalized_anchor: heading.normalized_anchor.clone(),
                    line_index: heading.line_index,
                    char_index: byte_to_char_index(content, heading.byte_range.start),
                    collapsible,
                    collapsed,
                }
            })
            .collect()
    }

    fn details_toggle_label(&self) -> &'static str {
        if self.show_metadata {
            "Hide Details"
        } else {
            "Show Details"
        }
    }

    fn set_show_metadata(&mut self, app: &mut LauncherApp, show_metadata: bool) {
        if self.show_metadata == show_metadata {
            return;
        }
        self.show_metadata = show_metadata;
        self.persist_details_visibility(app);
    }

    fn persist_details_visibility(&self, app: &mut LauncherApp) {
        match crate::settings::Settings::load(&app.settings_path) {
            Ok(mut settings) => {
                if settings.note_show_details == self.show_metadata {
                    return;
                }
                settings.note_show_details = self.show_metadata;
                if let Err(err) = settings.save(&app.settings_path) {
                    app.report_error(
                        "ui operation",
                        format!("Failed to save note detail visibility setting: {err}"),
                    );
                } else {
                    app.note_show_details = self.show_metadata;
                }
            }
            Err(err) => {
                app.report_error(
                    "ui operation",
                    format!("Failed to load settings for note detail visibility: {err}"),
                );
            }
        }
    }

    pub fn from_note(note: Note) -> Self {
        Self::from_note_with_details(note, true)
    }

    pub fn from_note_with_details(note: Note, show_details: bool) -> Self {
        Self::from_note_with_details_and_view_mode(note, show_details, NoteViewMode::Preview)
    }

    pub fn from_note_with_details_and_settings(
        note: Note,
        show_details: bool,
        settings: &NoteSettings,
    ) -> Self {
        let view_mode = if settings.rich_markdown_enabled {
            settings.effective_default_view_mode()
        } else {
            NoteViewMode::Edit
        };
        let mut panel = Self::from_note_with_details_and_view_mode_and_backlink_setting(
            note,
            show_details,
            view_mode,
            settings.backlinks_enabled,
        );
        panel.outline_open = settings.outline_sidebar_default_open;
        panel
    }

    fn from_note_with_details_and_view_mode(
        note: Note,
        show_details: bool,
        view_mode: NoteViewMode,
    ) -> Self {
        Self::from_note_with_details_and_view_mode_and_backlink_setting(
            note,
            show_details,
            view_mode,
            true,
        )
    }

    fn from_note_with_details_and_view_mode_and_backlink_setting(
        note: Note,
        show_details: bool,
        view_mode: NoteViewMode,
        backlinks_enabled: bool,
    ) -> Self {
        let mut panel = Self {
            open: true,
            note,
            link_search: String::new(),
            link_menu_targets: Vec::new(),
            link_menu_targets_version: None,
            link_menu_results: Vec::new(),
            link_menu_results_key: None,
            #[cfg(test)]
            link_menu_target_refresh_count: 0,
            #[cfg(test)]
            link_menu_result_refresh_count: 0,
            image_search: String::new(),
            tag_search: String::new(),
            view_mode,
            markdown_cache: CommonMarkCache::default(),
            markdown_analysis: None,
            markdown_analysis_source_hash: None,
            collapsed_sections: HashSet::new(),
            ui_state_error_reported: false,
            preview_render_error_reported: false,
            outline_open: false,
            outline_width: 180.0,
            outline_filter: String::new(),
            selected_outline_heading: None,
            pending_scroll_target: None,
            image_cache: HashMap::new(),
            overwrite_prompt: false,
            show_open_with_menu: false,
            show_metadata: show_details,
            tags_expanded: false,
            links_expanded: false,
            backlink_tab: BacklinkTab::LinkedTodos,
            backlink_page: 0,
            pending_selection: None,
            link_dialog_open: false,
            link_text: String::new(),
            link_url: String::new(),
            link_new_dialog_open: false,
            link_new_name: String::new(),
            new_alias: String::new(),
            alias_rename_inputs: HashMap::new(),
            focus_textedit_next_frame: false,
            last_textedit_id: None,
            derived: NoteDerivedView::default(),
            fast_derived_dirty: true,
            heavy_recompute_requested: true,
            last_edit_at_secs: None,
            last_notes_version: 0,
            last_todo_revision: 0,
            last_backlink_content_hash: None,
            last_alias_map_hash: 0,
            #[cfg(test)]
            heavy_recompute_count: 0,
            #[cfg(test)]
            last_ui_sections: NotePanelUiSections::default(),
            #[cfg(test)]
            metadata_details_render_count: 0,
            #[cfg(test)]
            backlinks_render_count: 0,
        };
        panel.refresh_fast_derived();
        panel.refresh_heavy_derived(true, backlinks_enabled);
        panel
    }

    pub(crate) fn load_collapsed_sections_state(&mut self, app: &mut LauncherApp) {
        if !app.note_settings.collapsed_sections_persist {
            return;
        }
        let path = crate::note_ui_state::path_for_settings(Path::new(&app.settings_path));
        match crate::note_ui_state::load(&path) {
            Ok(state) => {
                self.collapsed_sections = state.collapsed_sections_for(&self.note.slug);
            }
            Err(err) => self.report_ui_state_error_once(
                app,
                format!(
                    "Failed to load note UI state from {}: {err}",
                    path.display()
                ),
            ),
        }
    }

    fn persist_collapsed_sections_state(&mut self, app: &mut LauncherApp) {
        if !app.note_settings.collapsed_sections_persist {
            return;
        }
        let path = crate::note_ui_state::path_for_settings(Path::new(&app.settings_path));
        let mut state = match crate::note_ui_state::load(&path) {
            Ok(state) => state,
            Err(err) => {
                self.report_ui_state_error_once(
                    app,
                    format!(
                        "Failed to load note UI state from {}: {err}",
                        path.display()
                    ),
                );
                return;
            }
        };
        state.set_collapsed_sections(
            self.note.slug.clone(),
            self.collapsed_sections.iter().cloned(),
        );
        if let Err(err) = crate::note_ui_state::save(&path, &state) {
            self.report_ui_state_error_once(
                app,
                format!("Failed to save note UI state to {}: {err}", path.display()),
            );
        }
    }

    fn report_ui_state_error_once(&mut self, app: &mut LauncherApp, message: String) {
        if self.ui_state_error_reported {
            return;
        }
        self.ui_state_error_reported = true;
        app.report_error("note UI state", message);
    }

    fn backlink_rows_for_active_tab(&self) -> &[BacklinkRow] {
        match self.backlink_tab {
            BacklinkTab::LinkedTodos => &self.derived.backlink_rows_linked_todos,
            BacklinkTab::RelatedNotes => &self.derived.backlink_rows_related_notes,
            BacklinkTab::Mentions => &self.derived.backlink_rows_mentions,
        }
    }

    fn refresh_fast_derived(&mut self) {
        self.derived.tags = extract_tags(&self.note.content);
        self.derived.wiki_links = extract_wiki_links(&self.note.content)
            .into_iter()
            .filter(|l| slugify(l) != self.note.slug)
            .collect();
        self.derived.external_links = extract_links(&self.note.content);
        self.fast_derived_dirty = false;
    }

    fn sync_aliases_from_content(&mut self) {
        self.note.aliases = extract_aliases(&self.note.content);
        self.note.alias = self.note.aliases.first().cloned();
        self.fast_derived_dirty = true;
        self.heavy_recompute_requested = true;
    }

    fn save_alias_metadata_change(&mut self, app: &mut LauncherApp) {
        self.sync_aliases_from_content();
        let mut note = self.note.clone();
        match save_note(&mut note, true) {
            Ok(true) => {
                self.note = note;
                self.sync_aliases_from_content();
            }
            Ok(false) => self.overwrite_prompt = true,
            Err(err) => app.report_error("note save", format!("Failed to save aliases: {err}")),
        }
    }

    fn alias_collision_warning(&self, alias: &str) -> Option<String> {
        let alias = alias.trim();
        if alias.is_empty() {
            return None;
        }
        let alias_map = note_alias_map_snapshot();
        let Some(slugs) = alias_map.get(&alias.to_lowercase()) else {
            return None;
        };
        let conflicting_slugs: HashSet<&str> = slugs
            .iter()
            .map(String::as_str)
            .filter(|slug| *slug != self.note.slug)
            .collect();
        if conflicting_slugs.is_empty() {
            return None;
        }
        let labels = note_cache_snapshot()
            .into_iter()
            .filter(|note| conflicting_slugs.contains(note.slug.as_str()))
            .map(|note| note_display_with_secondary(&note))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("Alias \"{alias}\" is already used by {labels}"))
    }

    fn warn_alias_collision(&self, app: &mut LauncherApp, alias: &str) {
        if let Some(message) = self.alias_collision_warning(alias) {
            app.add_toast(Toast {
                text: message.into(),
                kind: ToastKind::Warning,
                options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),
            });
        }
    }

    fn refresh_heavy_derived(&mut self, force: bool, backlinks_enabled: bool) {
        let current_notes_version = note_version();
        let current_todo_revision = todo_version();
        let current_content_hash = self.content_hash();
        let notes = note_cache_snapshot();
        let current_alias_map_hash = alias_map_hash(&notes);

        if !backlinks_enabled {
            self.last_notes_version = current_notes_version;
            self.last_todo_revision = current_todo_revision;
            self.last_backlink_content_hash = Some(current_content_hash);
            self.last_alias_map_hash = current_alias_map_hash;
            self.heavy_recompute_requested = false;
            return;
        }

        if !force
            && self.last_notes_version == current_notes_version
            && self.last_todo_revision == current_todo_revision
            && self.last_backlink_content_hash == Some(current_content_hash)
            && self.last_alias_map_hash == current_alias_map_hash
        {
            self.heavy_recompute_requested = false;
            return;
        }

        let todos = load_todos(TODO_FILE).unwrap_or_default();
        self.derived.todo_label_map = todos
            .iter()
            .filter(|t| !t.id.is_empty())
            .map(|t| (t.id.clone(), t.text.clone()))
            .collect::<HashMap<_, _>>();

        self.derived.backlink_rows_linked_todos =
            backlink_rows_for_note(&self.note, BacklinkTab::LinkedTodos, &todos, &notes);
        self.derived.backlink_rows_related_notes =
            backlink_rows_for_note(&self.note, BacklinkTab::RelatedNotes, &todos, &notes);
        self.derived.backlink_rows_mentions =
            backlink_rows_for_note(&self.note, BacklinkTab::Mentions, &todos, &notes);

        self.last_notes_version = current_notes_version;
        self.last_todo_revision = current_todo_revision;
        self.last_backlink_content_hash = Some(current_content_hash);
        self.last_alias_map_hash = current_alias_map_hash;
        self.heavy_recompute_requested = false;
        #[cfg(test)]
        {
            self.heavy_recompute_count += 1;
        }
    }

    fn content_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.note.content.hash(&mut hasher);
        hasher.finish()
    }

    fn markdown_analysis(&mut self) -> &MarkdownAnalysis {
        let source_hash = self.content_hash();
        if self.markdown_analysis_source_hash != Some(source_hash) {
            self.markdown_analysis = Some(analyze_markdown(&self.note.content));
            self.markdown_analysis_source_hash = Some(source_hash);
        }
        self.markdown_analysis
            .as_ref()
            .expect("markdown analysis should be cached after recompute")
    }

    fn invalidate_markdown_analysis(&mut self) {
        self.markdown_analysis = None;
        self.markdown_analysis_source_hash = None;
    }

    fn mark_content_changed(&mut self, now_secs: f64) {
        self.invalidate_markdown_analysis();
        self.markdown_cache.clear_scrollable();
        self.fast_derived_dirty = true;
        self.heavy_recompute_requested = true;
        self.last_edit_at_secs = Some(now_secs);
    }

    fn toggle_rendered_checkbox_marker(
        &mut self,
        task_item: &MarkdownTaskItem,
        now_secs: f64,
    ) -> bool {
        if let Some(updated) =
            toggle_task_marker(&self.note.content, task_item.marker_byte_range.clone())
        {
            self.note.content = updated;
            self.mark_content_changed(now_secs);
            true
        } else {
            false
        }
    }

    fn maybe_refresh_heavy_derived(&mut self, ctx: &egui::Context, backlinks_enabled: bool) {
        let notes_changed = self.last_notes_version != note_version();
        let todos_changed = self.last_todo_revision != todo_version();
        let alias_changed = self.last_alias_map_hash != alias_map_hash(&note_cache_snapshot());
        let content_changed = self.last_backlink_content_hash != Some(self.content_hash());
        let debounce_elapsed = self
            .last_edit_at_secs
            .map(|t| ctx.input(|i| i.time - t) >= HEAVY_RECOMPUTE_IDLE_DEBOUNCE.as_secs_f64())
            .unwrap_or(false);
        if notes_changed || todos_changed || alias_changed || (content_changed && debounce_elapsed)
        {
            self.refresh_heavy_derived(false, backlinks_enabled);
            return;
        }

        if self.heavy_recompute_requested {
            ctx.request_repaint_after(HEAVY_RECOMPUTE_IDLE_DEBOUNCE);
        }
    }

    fn refresh_link_menu_targets_if_needed(&mut self) {
        let current_version = note_version();
        if self.link_menu_targets_version == Some(current_version) {
            return;
        }

        self.link_menu_targets = note_link_menu_targets_snapshot()
            .into_iter()
            .map(|target: NoteLinkMenuTarget| LinkMenuResult {
                display_title: target.display_title().to_string(),
                search_text: target.search_text(),
                slug: target.slug,
            })
            .collect();
        self.link_menu_targets_version = Some(current_version);
        self.invalidate_link_menu_results();
        #[cfg(test)]
        {
            self.link_menu_target_refresh_count += 1;
        }
    }

    fn refresh_link_menu_results_if_needed(&mut self) {
        self.refresh_link_menu_targets_if_needed();

        let current_version = note_version();
        let query = self.link_search.trim().to_lowercase();
        let key = LinkMenuResultsKey {
            notes_version: current_version,
            current_slug: self.note.slug.clone(),
            query: query.clone(),
        };
        if self.link_menu_results_key.as_ref() == Some(&key) {
            return;
        }

        let mut results: Vec<LinkMenuResult> = self
            .link_menu_targets
            .iter()
            .filter(|target| target.slug != self.note.slug)
            .cloned()
            .collect();

        if query.is_empty() {
            results.sort_by(|a, b| {
                a.display_title
                    .to_lowercase()
                    .cmp(&b.display_title.to_lowercase())
                    .then_with(|| a.slug.cmp(&b.slug))
            });
        } else {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, LinkMenuResult)> = results
                .into_iter()
                .filter_map(|target| {
                    matcher
                        .fuzzy_match(&target.search_text, &query)
                        .map(|score| (score, target))
                })
                .collect();
            scored.sort_by(|(score_a, a), (score_b, b)| {
                score_b
                    .cmp(score_a)
                    .then_with(|| {
                        a.display_title
                            .to_lowercase()
                            .cmp(&b.display_title.to_lowercase())
                    })
                    .then_with(|| a.slug.cmp(&b.slug))
            });
            results = scored.into_iter().map(|(_, target)| target).collect();
        }

        results.truncate(NOTE_LINK_CONTEXT_MENU_RESULT_LIMIT);
        self.link_menu_results = results;
        self.link_menu_results_key = Some(key);
        #[cfg(test)]
        {
            self.link_menu_result_refresh_count += 1;
        }
    }

    fn link_menu_results_snapshot(&mut self) -> Vec<LinkMenuResult> {
        self.refresh_link_menu_results_if_needed();
        self.link_menu_results.clone()
    }

    fn invalidate_link_menu_results(&mut self) {
        self.link_menu_results_key = None;
    }

    pub fn note_slug(&self) -> &str {
        &self.note.slug
    }

    pub fn note_content(&self) -> &str {
        &self.note.content
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut save_now = false;
        let screen_rect = ctx.available_rect();
        let max_width = screen_rect.width().min(800.0);
        let max_height = screen_rect.height().min(600.0);
        // NOTE: `egui::TextEditState` cursor indices are char-based and we also mutate `self` inside the
        // window closure (save, open externally, etc.). Don't capture borrows of `self.note.slug` in
        // the closure environment - keep IDs based on an owned clone instead.
        let slug = self.note.slug.clone();
        let content_id = egui::Id::new(("note_content", slug.clone()));
        let scroll_id_source = ("note_scroll", slug.clone());
        let text_id_source = ("note_text", slug);

        egui::Window::new(self.note.title.clone())
            .open(&mut open)
            .resizable(true)
            .default_size(app.note_panel_default_size)
            .min_width(200.0)
            .min_height(150.0)
            .max_width(max_width)
            .max_height(max_height)
            .movable(true)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    self.last_ui_sections = NotePanelUiSections::default();
                }
                if ui
                    .ctx()
                    .input(|i| i.modifiers.ctrl && i.key_pressed(Key::Equals))
                {
                    app.note_font_size += 1.0;
                }
                if ui
                    .ctx()
                    .input(|i| i.modifiers.ctrl && i.key_pressed(Key::Minus))
                {
                    app.note_font_size = (app.note_font_size - 1.0).max(8.0);
                }
                if self.render_toolbar(ui, app) {
                    save_now = true;
                }
                if !app.note_settings.outline_sidebar_enabled {
                    self.outline_open = false;
                    self.outline_filter.clear();
                    self.selected_outline_heading = None;
                    self.pending_scroll_target = None;
                }
                if self.fast_derived_dirty {
                    self.refresh_fast_derived();
                }
                self.maybe_refresh_heavy_derived(ctx, app.note_settings.backlinks_enabled);
                self.render_outline(ui, app);
                let remaining = ui.available_height();
                #[cfg(test)]
                {
                    self.last_ui_sections.content_visible = true;
                }
                match self.view_mode {
                    NoteViewMode::Edit => {
                        let resp = egui::ScrollArea::vertical()
                            .id_source(scroll_id_source)
                            .max_height(remaining)
                            .show(ui, |ui| self.render_editor(ui, app, text_id_source.clone()))
                            .inner;
                        self.handle_editor_response(resp, ctx, app, true);
                    }
                    NoteViewMode::Preview => {
                        egui::ScrollArea::vertical()
                            .id_source(scroll_id_source)
                            .max_height(remaining)
                            .show(ui, |ui| {
                                let _ = self.markdown_analysis();
                                self.render_preview(ui, app, ctx);
                            });
                    }
                    NoteViewMode::Split => {
                        self.render_split(ui, app, ctx);
                    }
                }
            });

        // If the panel is closing, ensure we don't leave egui focus on a widget
        // that will no longer exist this frame. This avoids AccessKit panics
        // about focused nodes missing from the accessibility tree.
        if !open {
            if let Some(id) = self.last_textedit_id {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
        }

        if self.link_dialog_open {
            let mut open_link = true;
            egui::Window::new("Insert Link")
                .collapsible(false)
                .resizable(false)
                .open(&mut open_link)
                .show(ctx, |ui| {
                    ui.label("Text:");
                    ui.text_edit_singleline(&mut self.link_text);
                    ui.label("URL:");
                    ui.text_edit_singleline(&mut self.link_url);
                    ui.horizontal(|ui| {
                        if ui.button("Insert").clicked() {
                            let id = self.last_textedit_id.unwrap_or(content_id);
                            self.insert_link(ctx, id);
                            // Return focus to the editor after insertion.
                            self.focus_textedit_next_frame = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.link_text.clear();
                            self.link_url.clear();
                            self.link_dialog_open = false;
                        }
                    });
                });
            self.link_dialog_open &= open_link;
        }
        if self.link_new_dialog_open {
            let mut open_link_new = true;
            egui::Window::new("Link new note")
                .collapsible(false)
                .resizable(false)
                .open(&mut open_link_new)
                .show(ctx, |ui| {
                    ui.label("New note name:");
                    ui.text_edit_singleline(&mut self.link_new_name);
                    let is_empty = self.link_new_name.trim().is_empty();
                    if is_empty {
                        ui.colored_label(Color32::YELLOW, "Name is required");
                    }
                    ui.horizontal(|ui| {
                        let id = self.last_textedit_id.unwrap_or(content_id);
                        let confirm = ui.add_enabled(!is_empty, egui::Button::new("Link"));
                        if confirm.clicked() {
                            self.insert_or_create_note_link(ctx, id, app);
                            self.focus_textedit_next_frame = true;
                        }
                        if ui.button("Cancel").clicked() {
                            self.link_new_name.clear();
                            self.link_new_dialog_open = false;
                        }
                    });
                });
            self.link_new_dialog_open &= open_link_new;
        }
        if save_now || (!open && app.note_save_on_close) {
            self.save(app);
            if self.overwrite_prompt {
                open = true;
            }
        }
        self.open = open;
        if self.overwrite_prompt {
            egui::Window::new("Note exists")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("A note with this name already exists.");
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            if let Err(e) = save_note(&mut self.note, true) {
                                app.report_error(
                                    "ui operation",
                                    format!("Failed to save note: {e}"),
                                );
                            } else {
                                self.refresh_fast_derived();
                                self.refresh_heavy_derived(
                                    true,
                                    app.note_settings.backlinks_enabled,
                                );
                                self.finish_save(app);
                                self.overwrite_prompt = false;
                            }
                        }
                        if ui.button("Save as New").clicked() {
                            self.note.slug.clear();
                            self.note.path = std::path::PathBuf::new();
                            if let Err(e) = save_note(&mut self.note, true) {
                                app.report_error(
                                    "ui operation",
                                    format!("Failed to save note: {e}"),
                                );
                            } else {
                                self.refresh_fast_derived();
                                self.refresh_heavy_derived(
                                    true,
                                    app.note_settings.backlinks_enabled,
                                );
                                self.finish_save(app);
                                self.overwrite_prompt = false;
                            }
                        }
                    });
                });
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) -> bool {
        let mut save_now = false;
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                save_now = true;
            }
            let open_resp = ui.button("Open Externally");
            let popup_id = open_resp.id.with("open_with_menu");
            if open_resp.clicked() {
                match app.note_external_open {
                    NoteExternalOpen::Powershell => {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Powershell);
                    }
                    NoteExternalOpen::Notepad => {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Notepad);
                    }
                    NoteExternalOpen::Wezterm => {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Wezterm);
                    }
                    NoteExternalOpen::Neither => {
                        self.show_open_with_menu = true;
                        ui.memory_mut(|m| m.open_popup(popup_id));
                    }
                }
            }
            if self.show_open_with_menu {
                let mut close = false;
                if popup::popup_below_widget(ui, popup_id, &open_resp, |ui| {
                    if ui.button("Powershell").clicked() {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Powershell);
                        close = true;
                    }
                    if ui.button("WezTerm").clicked() {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Wezterm);
                        close = true;
                    }
                    if ui.button("Notepad").clicked() {
                        self.save(app);
                        self.open_external(app, NoteExternalOpen::Notepad);
                        close = true;
                    }
                })
                .is_none()
                {
                    close = true;
                }
                if close {
                    ui.memory_mut(|m| m.close_popup());
                    self.show_open_with_menu = false;
                }
            }
            if !app.note_settings.rich_markdown_enabled
                && matches!(self.view_mode, NoteViewMode::Preview | NoteViewMode::Split)
            {
                self.view_mode = NoteViewMode::Edit;
            }
            if !app.note_settings.can_use_split() && matches!(self.view_mode, NoteViewMode::Split) {
                self.view_mode = if app.note_settings.rich_markdown_enabled {
                    NoteViewMode::Preview
                } else {
                    NoteViewMode::Edit
                };
            }
            if matches!(self.view_mode, NoteViewMode::Preview | NoteViewMode::Split)
                && ui.button("Edit").clicked()
            {
                self.view_mode = NoteViewMode::Edit;
                self.focus_textedit_next_frame = true;
            }
            if !matches!(self.view_mode, NoteViewMode::Preview) && ui.button("Render").clicked() {
                self.view_mode = NoteViewMode::Preview;
                if let Some(id) = self.last_textedit_id {
                    ui.ctx().memory_mut(|m| m.surrender_focus(id));
                }
            }
            if app.note_settings.can_use_split()
                && !matches!(self.view_mode, NoteViewMode::Split)
                && ui.button("Split").clicked()
            {
                self.view_mode = NoteViewMode::Split;
            }
            if ui.button(self.details_toggle_label()).clicked() {
                self.set_show_metadata(app, !self.show_metadata);
                let was_focused = self
                    .last_textedit_id
                    .map(|id| ui.ctx().memory(|m| m.has_focus(id)))
                    .unwrap_or(false);
                if was_focused {
                    self.focus_textedit_next_frame = true;
                }
            }
            ui.separator();
            if ui.button("A-").clicked() {
                app.note_font_size = (app.note_font_size - 1.0).max(8.0);
            }
            if ui.button("A+").clicked() {
                app.note_font_size += 1.0;
            }
            let sections =
                self.collapsible_sections(app.note_settings.collapsible_sections_enabled);
            if !sections.is_empty() {
                ui.separator();
                if ui.button("Expand all").clicked() {
                    self.collapsed_sections.clear();
                }
                if ui.button("Collapse all").clicked() {
                    let keys = sections
                        .iter()
                        .filter(|section| !self.is_main_title_section(section))
                        .map(|section| self.section_key(section))
                        .collect();
                    self.collapsed_sections = keys;
                }
            } else if !app.note_settings.collapsible_sections_enabled {
                self.collapsed_sections.clear();
            }
            if app.note_settings.outline_sidebar_enabled {
                ui.separator();
                if ui.button("Toggle outline").clicked() {
                    self.outline_open = !self.outline_open;
                }
            }
        });
        save_now
    }

    fn render_outline(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        if !self.show_metadata {
            return;
        }
        self.render_metadata_details(ui, app);
        if app.note_settings.outline_sidebar_enabled {
            if self.outline_open {
                let analysis = self.markdown_analysis().clone();
                let rows = Self::outline_rows_from_headings(
                    &self.note.slug,
                    &self.note.content,
                    &analysis.headings,
                    &analysis.sections,
                    &self.collapsed_sections,
                    app.note_settings.max_outline_depth,
                    app.note_settings.collapsible_sections_enabled,
                    &self.outline_filter,
                );
                if !rows.is_empty() || !self.outline_filter.is_empty() {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Outline");
                        ui.add(
                            egui::DragValue::new(&mut self.outline_width)
                                .clamp_range(120.0..=360.0)
                                .speed(4.0)
                                .prefix("width "),
                        );
                    });
                    ui.set_max_width(self.outline_width);
                    ui.text_edit_singleline(&mut self.outline_filter);
                    for row in rows {
                        ui.horizontal(|ui| {
                            ui.add_space((row.level.saturating_sub(1) as f32) * 12.0);
                            if app.note_settings.collapsible_sections_enabled {
                                let marker = if row.collapsible {
                                    if row.collapsed {
                                        "▶"
                                    } else {
                                        "▼"
                                    }
                                } else {
                                    "•"
                                };
                                ui.small(marker);
                            }
                            let selected = self.selected_outline_heading.as_deref()
                                == Some(row.normalized_anchor.as_str());
                            if ui.selectable_label(selected, &row.title).clicked() {
                                self.selected_outline_heading = Some(row.normalized_anchor.clone());
                                self.pending_scroll_target = Some(row.normalized_anchor.clone());
                                match self.view_mode {
                                    NoteViewMode::Edit => {
                                        self.move_editor_cursor_to(row.char_index, ui.ctx());
                                    }
                                    NoteViewMode::Split => {
                                        let editor_focused = self
                                            .last_textedit_id
                                            .map(|id| ui.ctx().memory(|m| m.has_focus(id)))
                                            .unwrap_or(false);
                                        if editor_focused {
                                            self.move_editor_cursor_to(row.char_index, ui.ctx());
                                        }
                                    }
                                    NoteViewMode::Preview => {}
                                }
                            }
                        });
                    }
                }
            }
        } else {
            self.outline_open = false;
            self.outline_filter.clear();
            self.selected_outline_heading = None;
            self.pending_scroll_target = None;
        }
        if app.note_settings.backlinks_enabled {
            self.render_backlinks(ui, app);
        }
    }

    fn move_editor_cursor_to(&mut self, char_index: usize, ctx: &egui::Context) {
        let id = self
            .last_textedit_id
            .unwrap_or_else(|| egui::Id::new(("note_text", self.note.slug.clone())));
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(char_index),
            )));
        state.store(ctx, id);
        self.pending_selection = None;
        self.focus_textedit_next_frame = true;
    }

    fn render_metadata_details(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        #[cfg(test)]
        {
            self.metadata_details_render_count += 1;
        }
        if !self.derived.tags.is_empty() {
            #[cfg(test)]
            {
                self.last_ui_sections.tags_visible = true;
            }
            let was_focused = self
                .last_textedit_id
                .map(|id| ui.ctx().memory(|m| m.has_focus(id)))
                .unwrap_or(false);
            let tag_count = self.derived.tags.len();
            ui.horizontal_wrapped(|ui| {
                ui.label("Tags:");
                let threshold = app.note_more_limit;
                let show_all = self.tags_expanded || tag_count <= threshold;
                let limit = if show_all { tag_count } else { threshold };
                for t in self.derived.tags.iter().take(limit) {
                    if ui.link(format!("#{t}")).clicked() {
                        app.filter_notes_by_tag(t);
                    }
                }
                if tag_count > threshold {
                    let label = if self.tags_expanded {
                        "collapse"
                    } else {
                        "... (more)"
                    };
                    if ui.button(label).clicked() {
                        self.tags_expanded = !self.tags_expanded;
                        if was_focused {
                            self.focus_textedit_next_frame = true;
                        }
                    }
                }
            });
        }

        if app.note_settings.aliases_enabled {
            ui.horizontal_wrapped(|ui| {
                ui.label("Aliases:");
                if self.note.aliases.is_empty() {
                    ui.small("None");
                }
                for alias in self.note.aliases.clone() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(&alias);
                            ui.small(format!("{} · {}", self.note.title, self.note.slug));
                            if ui.small_button("Remove").clicked() {
                                self.note.content =
                                    remove_alias_metadata(&self.note.content, &alias);
                                self.alias_rename_inputs.remove(&alias);
                                self.save_alias_metadata_change(app);
                            }
                        });
                        let mut input = self
                            .alias_rename_inputs
                            .entry(alias.clone())
                            .or_insert_with(|| alias.clone())
                            .clone();
                        let mut rename_to = None;
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut input);
                            if ui.small_button("Rename").clicked() {
                                rename_to = Some(input.trim().to_string());
                            }
                        });
                        if let Some(renamed) = rename_to {
                            self.warn_alias_collision(app, &renamed);
                            self.note.content =
                                rename_alias_metadata(&self.note.content, &alias, &renamed);
                            self.alias_rename_inputs.remove(&alias);
                            self.save_alias_metadata_change(app);
                        } else {
                            self.alias_rename_inputs.insert(alias.clone(), input);
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Add alias:");
                ui.text_edit_singleline(&mut self.new_alias);
                if ui.button("Add").clicked() {
                    let alias = self.new_alias.trim().to_string();
                    if !alias.is_empty() {
                        self.warn_alias_collision(app, &alias);
                        self.note.content = add_alias_metadata(&self.note.content, &alias);
                        self.new_alias.clear();
                        self.save_alias_metadata_change(app);
                    }
                }
            });
        }

        enum LinkKind {
            Wiki(String),
            Url(String, String),
        }
        let mut all_links: Vec<LinkKind> = Vec::new();
        all_links.extend(self.derived.wiki_links.iter().cloned().map(LinkKind::Wiki));
        all_links.extend(
            self.derived
                .external_links
                .iter()
                .cloned()
                .map(|(label, url)| LinkKind::Url(label, url)),
        );
        if all_links.is_empty() {
            return;
        }
        #[cfg(test)]
        {
            self.last_ui_sections.links_visible = true;
        }
        let was_focused = self
            .last_textedit_id
            .map(|id| ui.ctx().memory(|m| m.has_focus(id)))
            .unwrap_or(false);
        ui.horizontal_wrapped(|ui| {
            ui.label("Links:");
            let threshold = app.note_more_limit;
            let total = all_links.len();
            let show_all = self.links_expanded || total <= threshold;
            let limit = if show_all { total } else { threshold };
            for l in all_links.iter().take(limit) {
                match l {
                    LinkKind::Wiki(s) => {
                        let _ = show_wiki_link(ui, app, s);
                    }
                    LinkKind::Url(label, url) => {
                        let _ = ui.hyperlink_to(label, url);
                    }
                }
            }
            if total > threshold {
                let label = if self.links_expanded {
                    "collapse"
                } else {
                    "... (more)"
                };
                if ui.button(label).clicked() {
                    self.links_expanded = !self.links_expanded;
                    if was_focused {
                        self.focus_textedit_next_frame = true;
                    }
                }
            }
        });
    }

    fn render_backlinks(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        #[cfg(test)]
        {
            self.backlinks_render_count += 1;
        }
        #[cfg(test)]
        {
            self.last_ui_sections.backlinks_visible = true;
        }
        ui.separator();
        ui.label("Backlinks");
        ui.horizontal(|ui| {
            for tab in [
                BacklinkTab::LinkedTodos,
                BacklinkTab::RelatedNotes,
                BacklinkTab::Mentions,
            ] {
                if ui
                    .selectable_label(self.backlink_tab == tab, tab.label())
                    .clicked()
                {
                    self.backlink_tab = tab;
                    self.backlink_page = 0;
                }
            }
        });
        let rows = self.backlink_rows_for_active_tab();
        let total_pages = (rows.len() + BACKLINK_PAGE_SIZE - 1) / BACKLINK_PAGE_SIZE;
        let page_start = self.backlink_page * BACKLINK_PAGE_SIZE;
        let page_end = (page_start + BACKLINK_PAGE_SIZE).min(rows.len());
        if rows.is_empty() {
            ui.small("No backlinks in this category.");
        } else {
            let page_rows: Vec<BacklinkRow> = rows[page_start..page_end].to_vec();
            for (idx, row) in page_rows.iter().enumerate() {
                ui.push_id(("backlink_row", idx, page_start), |ui| {
                    let resp = ui.selectable_label(false, &row.title);
                    if resp.clicked() {
                        if let Some(slug) = &row.note_slug {
                            app.open_note_panel(slug, None);
                        } else if let Some(todo_id) = &row.todo_id {
                            let todos = load_todos(TODO_FILE).unwrap_or_default();
                            if let Some((todo_idx, _)) =
                                todos.iter().enumerate().find(|(_, t)| &t.id == todo_id)
                            {
                                app.todo_view_dialog.open_edit(todo_idx);
                            } else {
                                app.todo_view_dialog.open();
                            }
                        }
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(slug) = &row.note_slug {
                            app.open_note_panel(slug, None);
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.small(format!("[{}]", row.type_badge));
                        ui.small(format!("updated {}", row.updated));
                        ui.small(format!("reason: {}", row.reason));
                    });
                    ui.small(&row.snippet);
                    if ui.button("Open").clicked() {
                        if let Some(slug) = &row.note_slug {
                            app.open_note_panel(slug, None);
                        } else if let Some(todo_id) = &row.todo_id {
                            let todos = load_todos(TODO_FILE).unwrap_or_default();
                            if let Some((todo_idx, _)) =
                                todos.iter().enumerate().find(|(_, t)| &t.id == todo_id)
                            {
                                app.todo_view_dialog.open_edit(todo_idx);
                            } else {
                                app.todo_view_dialog.open();
                            }
                        }
                    }
                });
                ui.separator();
            }
            if total_pages > 1 {
                ui.horizontal(|ui| {
                    if ui.button("Prev").clicked() && self.backlink_page > 0 {
                        self.backlink_page -= 1;
                    }
                    ui.small(format!("Page {}/{}", self.backlink_page + 1, total_pages));
                    if ui.button("Next").clicked() && self.backlink_page + 1 < total_pages {
                        self.backlink_page += 1;
                    }
                });
            }
        }
        ui.separator();
    }

    fn render_editor(
        &mut self,
        ui: &mut egui::Ui,
        app: &LauncherApp,
        text_id_source: (&'static str, String),
    ) -> egui::Response {
        ui.add(
            egui::TextEdit::multiline(&mut self.note.content)
                .id_source(text_id_source)
                .desired_width(f32::INFINITY)
                .font(FontId::monospace(app.note_font_size))
                .frame(true)
                .lock_focus(true)
                .desired_rows(10),
        )
    }

    fn render_preview(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, ctx: &egui::Context) {
        self.render_preview_document(ui, app, ctx);
    }

    fn render_preview_document(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
    ) {
        if app.note_settings.collapsible_sections_enabled {
            let sections = self.collapsible_sections(true);
            if !sections.is_empty() {
                self.render_collapsible_preview(ui, app, ctx, &sections);
                return;
            }
        } else {
            self.collapsed_sections.clear();
        }
        if let Some(target) = self.pending_scroll_target.clone() {
            if let Some(heading) = self
                .markdown_analysis()
                .headings
                .iter()
                .find(|heading| heading.normalized_anchor == target)
                .cloned()
            {
                if heading.byte_range.start > 0 {
                    self.render_preview_range(ui, app, ctx, 0..heading.byte_range.start);
                }
                let resp = ui.scope(|ui| {
                    let heading_text = self.note.content[heading.byte_range.clone()].to_string();
                    self.show_markdown_fragment(ui, app, &heading_text, heading.byte_range.start);
                });
                ui.scroll_to_rect(resp.response.rect, Some(egui::Align::Center));
                self.pending_scroll_target = None;
                if heading.byte_range.end < self.note.content.len() {
                    self.render_preview_range(
                        ui,
                        app,
                        ctx,
                        heading.byte_range.end..self.note.content.len(),
                    );
                }
                return;
            }
        }
        self.render_preview_range(ui, app, ctx, 0..self.note.content.len());
    }

    fn render_preview_range(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        render_range: std::ops::Range<usize>,
    ) -> bool {
        if app.note_settings.callouts_enabled {
            let callouts: Vec<MarkdownCallout> = self
                .markdown_analysis()
                .callouts
                .iter()
                .filter(|callout| {
                    callout.byte_range.start >= render_range.start
                        && callout.byte_range.end <= render_range.end
                })
                .cloned()
                .collect();
            if !callouts.is_empty() {
                let mut modified = false;
                let mut cursor = render_range.start;
                for callout in callouts {
                    if cursor < callout.byte_range.start
                        && self.render_preview_range_plain(
                            ui,
                            app,
                            ctx,
                            cursor..callout.byte_range.start,
                        )
                    {
                        modified = true;
                        break;
                    }
                    self.render_callout(ui, app, &callout);
                    cursor = callout.byte_range.end;
                }
                if !modified
                    && cursor < render_range.end
                    && self.render_preview_range_plain(ui, app, ctx, cursor..render_range.end)
                {
                    modified = true;
                }
                return modified;
            }
        }
        self.render_preview_range_plain(ui, app, ctx, render_range)
    }

    fn render_preview_range_plain(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        render_range: std::ops::Range<usize>,
    ) -> bool {
        let mut last = 0usize;
        let content_clone = self.note.content[render_range.clone()].to_string();
        let mut modified = false;
        for cap in IMAGE_RE.captures_iter(&content_clone) {
            let m = cap.get(0).unwrap();
            let local_range = m.range();
            let range =
                (render_range.start + local_range.start)..(render_range.start + local_range.end);
            let before = &content_clone[last..local_range.start];
            if !before.is_empty()
                && self.render_segment(ui, app, before, render_range.start + last, ctx)
            {
                modified = true;
                break;
            }
            let alt = cap.get(1).unwrap().as_str();
            let target = cap.get(2).unwrap().as_str();
            let parsed = parse_note_image_target(target);
            let rel = parsed.rel.as_str();
            let full = parsed.full;
            let width = parsed.width;
            if app.note_images_as_links {
                let label = if alt.is_empty() { rel } else { alt };
                if ui.link(label).clicked() {
                    app.open_image_panel(&full);
                }
            } else {
                let tex = if let Some(t) = self.image_cache.get(&full) {
                    t.clone()
                } else if let Ok(mut img) = image::open(&full) {
                    if img.width() > 512 || img.height() > 512 {
                        img = img.resize(512, 512, FilterType::Triangle);
                    }
                    let size = [img.width() as usize, img.height() as usize];
                    let rgba = img.to_rgba8();
                    let tex = ui.ctx().load_texture(
                        full.to_string_lossy().to_string(),
                        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
                        egui::TextureOptions::LINEAR,
                    );
                    self.image_cache.insert(full.clone(), tex.clone());
                    tex
                } else {
                    last = local_range.end;
                    continue;
                };
                let mut display = tex.size_vec2();
                if let Some(w) = width {
                    display *= w / display.x;
                }
                let response = ui.add(
                    egui::Image::new(&tex)
                        .fit_to_exact_size(display)
                        .sense(egui::Sense::click()),
                );
                if response.clicked() {
                    app.open_image_panel(&full);
                }
                if response.hovered() {
                    let scroll = ui.ctx().input(|i| {
                        if i.modifiers.ctrl {
                            i.raw_scroll_delta.y
                        } else {
                            0.0
                        }
                    });
                    if scroll != 0.0 {
                        let new_w = (display.x + scroll).clamp(20.0, 4096.0);
                        let repl = format!("![{alt}]({rel}|{:.0})", new_w.round());
                        self.note.content.replace_range(range.clone(), &repl);
                        modified = true;
                        break;
                    }
                }
                response.context_menu(|ui| {
                    let mut w = width.unwrap_or(display.x);
                    if ui
                        .add(egui::DragValue::new(&mut w).clamp_range(20.0..=4096.0))
                        .changed()
                    {
                        let repl = format!("![{alt}]({rel}|{:.0})", w.round());
                        self.note.content.replace_range(range.clone(), &repl);
                        modified = true;
                    }
                    if ui.button("Reset size").clicked() {
                        let repl = format!("![{alt}]({rel})");
                        self.note.content.replace_range(range.clone(), &repl);
                        modified = true;
                        ui.close_menu();
                    }
                });
            }
            last = local_range.end;
        }
        if !modified {
            let rest = &content_clone[last..];
            if !rest.is_empty()
                && self.render_segment(ui, app, rest, render_range.start + last, ctx)
            {
                modified = true;
            }
        }
        if modified {
            self.mark_content_changed(ctx.input(|i| i.time));
        }
        modified
    }

    fn render_callout(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        callout: &MarkdownCallout,
    ) {
        let visuals = ui.visuals().clone();
        let fill = visuals.faint_bg_color;
        let stroke = egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
        egui::Frame::none()
            .fill(fill)
            .stroke(stroke)
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(callout.kind.to_ascii_uppercase());
                        if !callout.title.is_empty() {
                            ui.label(
                                egui::RichText::new(&callout.title)
                                    .color(visuals.strong_text_color()),
                            );
                        }
                    });
                    if !callout.body.is_empty() {
                        ui.add_space(2.0);
                        self.show_markdown_fragment(
                            ui,
                            app,
                            &callout.body,
                            format!("callout_{}", callout.byte_range.start),
                        );
                    }
                });
            });
    }

    fn render_collapsible_preview(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        sections: &[MarkdownSection],
    ) {
        let hidden = Self::collapsed_body_ranges(
            &self.note.slug,
            sections,
            &self.collapsed_sections,
            app.note_settings.collapsible_sections_enabled,
        );
        let mut cursor = 0usize;
        for section in sections {
            let heading_range = section.heading.byte_range.clone();
            if Self::range_is_hidden(&heading_range, &hidden) {
                continue;
            }
            if cursor < heading_range.start {
                self.render_visible_preview_range(
                    ui,
                    app,
                    ctx,
                    cursor..heading_range.start,
                    &hidden,
                );
            }
            let key = self.section_key(section);
            let collapsed = self.collapsed_sections.contains(&key);
            let resp = ui.horizontal_top(|ui| {
                ui.add_space((section.heading.level.saturating_sub(1) as f32) * 10.0);
                let label = if collapsed { "▶" } else { "▼" };
                if ui.small_button(label).clicked() {
                    if collapsed {
                        self.collapsed_sections.remove(&key);
                    } else {
                        self.collapsed_sections.insert(key.clone());
                    }
                    self.persist_collapsed_sections_state(app);
                }
                let heading = self.note.content[heading_range.clone()].to_string();
                self.show_markdown_fragment(ui, app, &heading, heading_range.start);
            });
            if self.pending_scroll_target.as_deref()
                == Some(section.heading.normalized_anchor.as_str())
            {
                ui.scroll_to_rect(resp.response.rect, Some(egui::Align::Center));
                self.pending_scroll_target = None;
            }
            cursor = heading_range.end;
        }
        if cursor < self.note.content.len() {
            self.render_visible_preview_range(
                ui,
                app,
                ctx,
                cursor..self.note.content.len(),
                &hidden,
            );
        }
    }

    fn render_visible_preview_range(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        range: std::ops::Range<usize>,
        hidden: &[std::ops::Range<usize>],
    ) {
        let mut cursor = range.start;
        for hidden_range in hidden
            .iter()
            .filter(|hidden| hidden.end > range.start && hidden.start < range.end)
        {
            if cursor < hidden_range.start.min(range.end) {
                self.render_preview_range(ui, app, ctx, cursor..hidden_range.start.min(range.end));
            }
            cursor = cursor.max(hidden_range.end.min(range.end));
        }
        if cursor < range.end {
            self.render_preview_range(ui, app, ctx, cursor..range.end);
        }
    }

    fn render_split(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, ctx: &egui::Context) {
        let slug = self.note.slug.clone();
        let editor_id_source = ("note_split_text", slug.clone());
        let editor_scroll_id = ("note_split_editor_scroll", slug.clone());
        let preview_scroll_id = ("note_split_preview_scroll", slug);
        let height = ui.available_height();

        ui.horizontal(|ui| {
            let pane_width =
                ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(120.0);
            let editor_resp = ui
                .allocate_ui_with_layout(
                    egui::vec2(pane_width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(pane_width);
                        egui::ScrollArea::vertical()
                            .id_source(editor_scroll_id)
                            .max_height(height)
                            .show(ui, |ui| self.render_editor(ui, app, editor_id_source))
                            .inner
                    },
                )
                .inner;
            self.handle_editor_response(editor_resp, ctx, app, false);

            ui.separator();

            ui.allocate_ui_with_layout(
                egui::vec2(pane_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(pane_width);
                    egui::ScrollArea::vertical()
                        .id_source(preview_scroll_id)
                        .max_height(height)
                        .show(ui, |ui| {
                            let _ = self.markdown_analysis();
                            self.render_preview(ui, app, ctx);
                        });
                },
            );
        });
    }

    fn handle_editor_response(
        &mut self,
        resp: egui::Response,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        request_initial_focus: bool,
    ) {
        if resp.changed() {
            self.mark_content_changed(ctx.input(|i| i.time));
        }
        let first_edit_frame = self.last_textedit_id.is_none();
        self.last_textedit_id = Some(resp.id);
        if self.focus_textedit_next_frame || (request_initial_focus && first_edit_frame) {
            resp.request_focus();
            self.focus_textedit_next_frame = false;
        }
        if !resp.secondary_clicked() {
            let state =
                egui::widgets::text_edit::TextEditState::load(ctx, resp.id).unwrap_or_default();
            if let Some(range) = state.cursor.char_range() {
                let [min, max] = range.sorted();
                self.pending_selection = (min.index != max.index).then_some((min.index, max.index));
            } else {
                self.pending_selection = None;
            }
        }
        resp.context_menu(|ui| {
            let ctx2 = ui.ctx().clone();
            self.build_textedit_menu(ui, &ctx2, resp.id, app);
        });
        if resp.has_focus() && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Period)) {
            let pos = resp.rect.left_top();
            popup::show_tooltip_at(ctx, egui::Id::new("note_ctx_menu"), Some(pos), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    self.build_textedit_menu(ui, ctx, resp.id, app);
                });
            });
        }
        if resp.has_focus() && app.vim_mode {
            self.handle_vim_keys(ctx, resp.id);
        }
        if resp.clicked() {
            resp.request_focus();
        }
        if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            let modifiers = ctx.input(|i| i.modifiers);
            ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
        }
    }

    fn handle_vim_keys(&mut self, ctx: &egui::Context, id: egui::Id) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let total_chars = self.note.content.chars().count();
        let mut idx = state
            .cursor
            .char_range()
            .map(|r| r.primary.index)
            .unwrap_or(0)
            .min(total_chars);
        let mut moved = false;

        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::H));
            idx = idx.saturating_sub(1);
            moved = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::L)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::L));
            idx = (idx + 1).min(total_chars);
            moved = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::J)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::J));
            let byte_idx = char_to_byte_index(&self.note.content, idx);
            if let Some(pos) = self.note.content[byte_idx..].find('\n') {
                idx = byte_to_char_index(&self.note.content, byte_idx + pos + 1);
            } else {
                idx = total_chars;
            }
            moved = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::K)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::K));
            let byte_idx = char_to_byte_index(&self.note.content, idx);
            idx = self.note.content[..byte_idx]
                .rfind('\n')
                .map(|pos| byte_to_char_index(&self.note.content, pos))
                .unwrap_or(0);
            moved = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Y)) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Y));
            let byte_idx = char_to_byte_index(&self.note.content, idx);
            let start_byte = self.note.content[..byte_idx]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let end_byte = self.note.content[byte_idx..]
                .find('\n')
                .map(|p| byte_idx + p)
                .unwrap_or_else(|| self.note.content.len());
            ctx.output_mut(|o| o.copied_text = self.note.content[start_byte..end_byte].to_string());
        }

        if moved {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(idx),
                )));
            state.store(ctx, id);
        }
    }

    /// Persist the current note to disk and update UI state.
    ///
    /// This is invoked when the user clicks the **Save** button or when the
    /// panel closes while [`Settings::note_save_on_close`](crate::settings::Settings::note_save_on_close)
    /// is `true`. Close events include pressing `Esc`, clicking the window's
    /// close button, or any programmatic request to close the panel.
    pub(super) fn save(&mut self, app: &mut LauncherApp) {
        self.note.tags = extract_tags(&self.note.content);
        self.note.links = extract_wiki_links(&self.note.content)
            .into_iter()
            .map(|l| slugify(&l))
            .filter(|l| l != &self.note.slug)
            .collect();
        self.fast_derived_dirty = true;
        self.heavy_recompute_requested = true;
        if let Some(first) = self.note.content.lines().next() {
            if let Some(t) = first.strip_prefix("# ") {
                self.note.title = t.to_string();
            }
        }
        match save_note(&mut self.note, app.note_always_overwrite) {
            Ok(true) => {
                self.refresh_fast_derived();
                self.refresh_heavy_derived(true, app.note_settings.backlinks_enabled);
                self.finish_save(app);
                self.link_menu_targets_version = None;
                self.invalidate_link_menu_results();
            }
            Ok(false) => {
                self.overwrite_prompt = true;
            }
            Err(e) => {
                app.report_error("ui operation", format!("Failed to save note: {e}"));
            }
        }
    }

    fn finish_save(&self, app: &mut LauncherApp) {
        app.search();
        app.focus_input();
        if app.enable_toasts {
            app.add_toast(Toast {
                text: format!("Saved note {}", self.note.title).into(),
                kind: ToastKind::Success,
                options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),
            });
        }
    }

    fn show_markdown_fragment(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        fragment: &str,
        cache_id: impl std::fmt::Display,
    ) {
        if fragment.is_empty() {
            return;
        }
        ui.scope(|ui| {
            ui.style_mut().override_font_id = Some(FontId::proportional(app.note_font_size));
            let processed = preprocess_preview_markdown(
                fragment,
                &self.note.slug,
                &self.derived.todo_label_map,
            );
            let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                CommonMarkViewer::new(format!("note_seg_{}", cache_id)).show(
                    ui,
                    &mut self.markdown_cache,
                    &processed,
                );
            }));
            match render_result {
                Ok(()) => handle_markdown_links(ui, app),
                Err(_) => self.render_preview_fallback(ui, app, fragment),
            }
        });
    }

    fn render_preview_fallback(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        fragment: &str,
    ) {
        if !self.preview_render_error_reported {
            app.report_error_message(
                "note preview",
                "Markdown preview rendering failed; showing read-only source text instead.",
            );
            self.preview_render_error_reported = true;
        }
        let mut fallback = fragment.to_string();
        ui.add(
            egui::TextEdit::multiline(&mut fallback)
                .desired_width(f32::INFINITY)
                .font(FontId::monospace(app.note_font_size))
                .interactive(false),
        );
    }

    fn render_task_item(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        task_item: &MarkdownTaskItem,
        interactive: bool,
        ctx: &egui::Context,
    ) -> bool {
        let mut modified = false;
        ui.horizontal_top(|ui| {
            ui.add_space(task_item.indent as f32 * 8.0);
            let mut state = task_item.checked;
            let resp = ui.add_enabled(interactive, egui::Checkbox::without_text(&mut state));
            if interactive && resp.changed() {
                modified = self.toggle_rendered_checkbox_marker(task_item, ctx.input(|i| i.time));
            }
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                self.show_markdown_fragment(
                    ui,
                    app,
                    &task_item.text,
                    format!("task_{}", task_item.marker_byte_range.start),
                );
            });
        });
        modified
    }

    fn render_segment(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        segment: &str,
        start: usize,
        ctx: &egui::Context,
    ) -> bool {
        if !app.note_settings.task_lists_enabled {
            self.show_markdown_fragment(ui, app, segment, start);
            return false;
        }

        let segment_end = start + segment.len();
        let task_items: Vec<MarkdownTaskItem> = self
            .markdown_analysis()
            .task_items
            .iter()
            .filter(|item| {
                item.line_byte_range.start >= start && item.line_byte_range.end <= segment_end
            })
            .cloned()
            .collect();

        if task_items.is_empty() {
            self.show_markdown_fragment(ui, app, segment, start);
            return false;
        }

        let mut modified = false;
        let mut cursor = start;
        let old_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.0;
        for task_item in task_items {
            if task_item.line_byte_range.start > cursor {
                let before = self.note.content[cursor..task_item.line_byte_range.start].to_string();
                self.show_markdown_fragment(ui, app, &before, cursor);
            }
            if self.render_task_item(
                ui,
                app,
                &task_item,
                app.note_settings.interactive_checkboxes_enabled,
                ctx,
            ) {
                modified = true;
                break;
            }
            cursor = task_item.line_byte_range.end;
            if self.note.content.as_bytes().get(cursor) == Some(&b'\n') {
                cursor += 1;
            }
        }
        if !modified && cursor < segment_end {
            let rest = self.note.content[cursor..segment_end].to_string();
            self.show_markdown_fragment(ui, app, &rest, cursor);
        }
        ui.spacing_mut().item_spacing = old_spacing;
        modified
    }

    fn build_textedit_menu(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        id: egui::Id,
        app: &mut LauncherApp,
    ) {
        if self.pending_selection.is_none() {
            let state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            if let Some(range) = state.cursor.char_range() {
                let [min, max] = range.sorted();
                if min.index != max.index {
                    self.pending_selection = Some((min.index, max.index));
                }
            }
        }

        ui.menu_button("Markdown", |ui| {
            if ui.button("Add Checkbox").clicked() {
                let mut state =
                    egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                let idx = state
                    .cursor
                    .char_range()
                    .map(|r| r.primary.index)
                    .unwrap_or_else(|| self.note.content.chars().count());
                let idx_byte = char_to_byte_index(&self.note.content, idx);
                self.note.content.insert_str(idx_byte, "- [ ] ");
                self.mark_content_changed(ctx.input(|i| i.time));
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(idx + 6),
                    )));
                state.store(ctx, id);
                ui.close_menu();
            }
            if ui.button("Insert Link...").clicked() {
                if let Some((start, end)) = self.pending_selection {
                    let (start, end) = char_range_to_byte_range(&self.note.content, start, end);
                    self.link_text = self.note.content[start..end].to_string();
                } else {
                    self.link_text.clear();
                }
                self.link_dialog_open = true;
                ui.close_menu();
            }
            if ui.button("Bold Selection").clicked() {
                self.wrap_selection(ctx, id, "**", "**");
                ui.close_menu();
            }
            if ui.button("Italic Selection").clicked() {
                self.wrap_selection(ctx, id, "*", "*");
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Insert NOTE callout").clicked() {
                self.insert_callout(ctx, id, "note");
                ui.close_menu();
            }
            if ui.button("Insert WARNING callout").clicked() {
                self.insert_callout(ctx, id, "warning");
                ui.close_menu();
            }
            if ui.button("Insert TODO callout").clicked() {
                self.insert_callout(ctx, id, "todo");
                ui.close_menu();
            }
            if ui.button("Wrap selection in callout").clicked() {
                self.wrap_selection_in_callout(ctx, id, "note");
                ui.close_menu();
            }
        });

        ui.menu_button("Insert link", |ui| {
            ui.set_min_width(200.0);
            ui.horizontal(|ui| {
                ui.label("Insert link:");
                if ui.button("Link new").clicked() {
                    self.link_new_dialog_open = true;
                    self.link_new_name.clear();
                    ui.close_menu();
                }
            });
            ui.text_edit_singleline(&mut self.link_search);
            let results = self.link_menu_results_snapshot();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    if results.is_empty() {
                        ui.label("No matching notes");
                    }
                    for target in &results {
                        let title = target.display_title.clone();
                        if ui.button(&title).clicked() {
                            let insert = format!("[[{title}]]");
                            self.insert_text_at_cursor_or_selection(ctx, id, &insert);
                            self.link_search.clear();
                            self.invalidate_link_menu_results();
                            ui.close_menu();
                        }
                    }
                    if results.len() == NOTE_LINK_CONTEXT_MENU_RESULT_LIMIT {
                        ui.small("Showing first 50 matches. Type to narrow results.");
                    }
                });
        });

        ui.menu_button("Insert image", |ui| {
            ui.set_min_width(200.0);
            if ui.button("Upload...").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                    .pick_file()
                {
                    if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                        let dest = assets_dir().join(fname);
                        if let Err(e) = std::fs::copy(&path, &dest) {
                            app.report_error("ui operation", format!("Failed to copy image: {e}"));
                        } else {
                            let insert = format!("![{0}](assets/{0})", fname);
                            let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
                                .unwrap_or_default();
                            let idx = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx_byte = char_to_byte_index(&self.note.content, idx);
                            self.note.content.insert_str(idx_byte, &insert);
                            self.mark_content_changed(ctx.input(|i| i.time));
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(idx + insert.chars().count()),
                                )));
                            state.store(ctx, id);
                            self.image_search.clear();
                            ui.close_menu();
                        }
                    }
                }
            }
            if ui.button("Screenshot...").clicked() {
                match capture(ScreenshotMode::Region, true) {
                    Ok(path) => {
                        if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                            let dest = assets_dir().join(fname);
                            let result = std::fs::rename(&path, &dest).or_else(|_| {
                                std::fs::copy(&path, &dest)
                                    .map(|_| std::fs::remove_file(&path).unwrap_or(()))
                            });
                            if let Err(e) = result {
                                app.report_error(
                                    "ui operation",
                                    format!("Failed to save screenshot: {e}"),
                                );
                            } else {
                                let insert = format!("![{0}](assets/{0})", fname);
                                let mut state =
                                    egui::widgets::text_edit::TextEditState::load(ctx, id)
                                        .unwrap_or_default();
                                let idx = state
                                    .cursor
                                    .char_range()
                                    .map(|r| r.primary.index)
                                    .unwrap_or_else(|| self.note.content.chars().count());
                                let idx_byte = char_to_byte_index(&self.note.content, idx);
                                self.note.content.insert_str(idx_byte, &insert);
                                self.mark_content_changed(ctx.input(|i| i.time));
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(
                                        egui::text::CCursor::new(idx + insert.chars().count()),
                                    )));
                                state.store(ctx, id);
                                self.image_search.clear();
                                ui.close_menu();
                            }
                        }
                    }
                    Err(e) => app.report_error("ui operation", format!("Screenshot failed: {e}")),
                }
            }
            ui.label("Insert image:");
            ui.text_edit_singleline(&mut self.image_search);
            let matcher = SkimMatcherV2::default();
            let filter = self.image_search.to_lowercase();
            let images = image_files();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for img in images.into_iter().filter(|name| {
                        filter.is_empty()
                            || matcher.fuzzy_match(&name.to_lowercase(), &filter).is_some()
                    }) {
                        if ui.button(&img).clicked() {
                            let insert = format!("![{0}](assets/{0})", img);
                            let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
                                .unwrap_or_default();
                            let idx = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx_byte = char_to_byte_index(&self.note.content, idx);
                            self.note.content.insert_str(idx_byte, &insert);
                            self.mark_content_changed(ctx.input(|i| i.time));
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(idx + insert.chars().count()),
                                )));
                            state.store(ctx, id);
                            self.image_search.clear();
                            ui.close_menu();
                        }
                    }
                });
        });

        ui.menu_button("Link todo", |ui| {
            ui.label("Select existing todo");
            for todo in load_todos(TODO_FILE)
                .unwrap_or_default()
                .into_iter()
                .take(12)
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
                    let token = format!("@todo:{todo_id}");
                    let mut state =
                        egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                    let idx = state
                        .cursor
                        .char_range()
                        .map(|r| r.primary.index)
                        .unwrap_or_else(|| self.note.content.chars().count());
                    let idx_byte = char_to_byte_index(&self.note.content, idx);
                    let ends_with_ws = self.note.content[..idx_byte]
                        .chars()
                        .last()
                        .map(|c| c.is_whitespace())
                        .unwrap_or(true);
                    let insert = if idx_byte == 0 || ends_with_ws {
                        token.clone()
                    } else {
                        format!(" {token}")
                    };
                    self.note.content.insert_str(idx_byte, &insert);
                    self.mark_content_changed(ctx.input(|i| i.time));
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(idx + insert.chars().count()),
                        )));
                    state.store(ctx, id);
                    ui.close_menu();
                }
            }
        });

        ui.menu_button("Insert tag", |ui| {
            if insert_tag_menu(ui, ctx, id, &mut self.note.content, &mut self.tag_search) {
                self.mark_content_changed(ctx.input(|i| i.time));
            }
        });
    }

    pub fn wrap_selection(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        start_marker: &str,
        end_marker: &str,
    ) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let mut range = state.cursor.char_range().and_then(|r| {
            let [min, max] = r.sorted();
            if min.index != max.index {
                Some((min.index, max.index))
            } else {
                None
            }
        });

        if range.is_none() {
            range = self.pending_selection.take();
        } else {
            self.pending_selection = None;
        }

        if let Some((start, end)) = range {
            let (start_byte, end_byte) = char_range_to_byte_range(&self.note.content, start, end);
            self.note.content.insert_str(end_byte, end_marker);
            self.note.content.insert_str(start_byte, start_marker);
            self.mark_content_changed(ctx.input(|i| i.time));
            let new_start = start + start_marker.chars().count();
            let new_end = end + start_marker.chars().count();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(new_start),
                    egui::text::CCursor::new(new_end),
                )));
            state.store(ctx, id);
        }
    }

    fn insert_callout(&mut self, ctx: &egui::Context, id: egui::Id, kind: &str) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let idx = state
            .cursor
            .char_range()
            .map(|r| r.primary.index)
            .unwrap_or_else(|| self.note.content.chars().count());
        let (updated, cursor) = insert_callout_at_char(&self.note.content, idx, kind);
        self.note.content = updated;
        self.mark_content_changed(ctx.input(|i| i.time));
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        state.store(ctx, id);
    }

    fn wrap_selection_in_callout(&mut self, ctx: &egui::Context, id: egui::Id, kind: &str) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let mut range = state.cursor.char_range().and_then(|r| {
            let [min, max] = r.sorted();
            (min.index != max.index).then_some((min.index, max.index))
        });
        if range.is_none() {
            range = self.pending_selection.take();
        } else {
            self.pending_selection = None;
        }
        if let Some((start, end)) = range {
            let (updated, selected_range) =
                wrap_char_range_in_callout(&self.note.content, start, end, kind);
            self.note.content = updated;
            self.mark_content_changed(ctx.input(|i| i.time));
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(selected_range.start),
                    egui::text::CCursor::new(selected_range.end),
                )));
            state.store(ctx, id);
        }
    }

    pub fn insert_link(&mut self, ctx: &egui::Context, id: egui::Id) {
        let text = if self.link_text.is_empty() {
            if let Some((start, end)) = self.pending_selection {
                let (start, end) = char_range_to_byte_range(&self.note.content, start, end);
                self.note.content[start..end].to_string()
            } else {
                String::new()
            }
        } else {
            self.link_text.clone()
        };
        let insert = format!("[{text}]({})", self.link_url);
        self.insert_text_at_cursor_or_selection(ctx, id, &insert);
        self.link_dialog_open = false;
        self.link_text.clear();
        self.link_url.clear();
    }

    fn insert_text_at_cursor_or_selection(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        insert: &str,
    ) {
        if let Some((start, end)) = self.pending_selection.take() {
            let (start_byte, end_byte) = char_range_to_byte_range(&self.note.content, start, end);
            self.note
                .content
                .replace_range(start_byte..end_byte, insert);
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            let cursor = start + insert.chars().count();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            state.store(ctx, id);
            self.mark_content_changed(ctx.input(|i| i.time));
            return;
        }

        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let idx = state
            .cursor
            .char_range()
            .map(|r| r.primary.index)
            .unwrap_or_else(|| self.note.content.chars().count());
        let idx_byte = char_to_byte_index(&self.note.content, idx);
        self.note.content.insert_str(idx_byte, insert);
        let cursor = idx + insert.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        state.store(ctx, id);
        self.mark_content_changed(ctx.input(|i| i.time));
    }

    pub fn set_link_new_name(&mut self, name: impl Into<String>) {
        self.link_new_name = name.into();
    }

    pub fn insert_or_create_note_link(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        app: &mut LauncherApp,
    ) {
        let normalized_name = self.link_new_name.trim().to_string();
        if normalized_name.is_empty() {
            return;
        }

        let mut target_slug = slugify(&normalized_name);
        let mut link_target = normalized_name.to_string();

        match resolve_note_query(&normalized_name) {
            NoteTarget::Resolved(slug) => {
                target_slug = slug.clone();
                if let Some(note) = load_notes()
                    .ok()
                    .and_then(|notes| notes.into_iter().find(|n| n.slug == slug))
                {
                    link_target = note.title;
                } else {
                    link_target = slug;
                }
            }
            NoteTarget::Ambiguous(slugs) => {
                if let Some(slug) = slugs.into_iter().next() {
                    target_slug = slug.clone();
                    link_target = slug;
                }
            }
            NoteTarget::Broken => {
                if let Err(err) = append_note(&normalized_name, "") {
                    app.report_error(
                        "ui operation",
                        format!("Failed to create note while linking: {err}"),
                    );
                    return;
                }
                self.link_menu_targets_version = None;
                self.invalidate_link_menu_results();
                target_slug = slugify(&normalized_name);
                link_target = target_slug.clone();
            }
        }

        if target_slug == self.note.slug {
            self.link_new_dialog_open = false;
            self.link_new_name.clear();
            return;
        }

        let insert = format!("[[{link_target}]]");
        self.insert_text_at_cursor_or_selection(ctx, id, &insert);
        self.link_new_dialog_open = false;
        self.link_new_name.clear();
    }

    fn open_external(&self, app: &mut LauncherApp, choice: NoteExternalOpen) {
        let path = self.note.path.clone();
        if let Err(e) = spawn_external(&path, choice) {
            app.report_error(
                "ui operation",
                format!("Failed to open note externally: {e}"),
            );
        }
    }
}

pub fn spawn_external(path: &Path, choice: NoteExternalOpen) -> std::io::Result<()> {
    match choice {
        NoteExternalOpen::Powershell => {
            let (mut cmd, _cmd_str) = build_nvim_command(path);
            cmd.spawn().map(|_| ())
        }
        NoteExternalOpen::Wezterm => {
            let (mut cmd, _cmd_str) = build_wezterm_command(path);
            match cmd.spawn() {
                Ok(_) => Ok(()),
                Err(_) => {
                    let (mut cmd, _cmd_str) = build_nvim_command(path);
                    cmd.spawn().map(|_| ())
                }
            }
        }
        NoteExternalOpen::Notepad => Command::new("notepad.exe").arg(path).spawn().map(|_| ()),
        NoteExternalOpen::Neither => Ok(()),
    }
}

pub fn show_wiki_link(ui: &mut egui::Ui, app: &mut LauncherApp, l: &str) -> egui::Response {
    let text = format!("[[{l}]]");
    let target = l.split('|').next().unwrap_or(l).trim();
    match resolve_note_query(target) {
        NoteTarget::Resolved(slug) => {
            let resp = ui.link(text);
            if resp.clicked() {
                app.open_note_panel(&slug, None);
            }
            resp
        }
        NoteTarget::Ambiguous(slugs) => {
            let label = format!("{text} (ambiguous)");
            let resp = ui.add(
                egui::Label::new(egui::RichText::new(label).color(Color32::YELLOW))
                    .sense(egui::Sense::click()),
            );
            if resp.clicked() {
                app.report_error("ui operation", format!(
                    "Ambiguous link [[{target}]]; use [[slug:<slug>]] or [[path:<file.md>]]. Candidates: {}",
                    slugs.join(", ")
                ));
            }
            resp
        }
        NoteTarget::Broken => {
            let slug = slugify(target);
            let resp = ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("{text} (missing)")).color(Color32::RED),
                )
                .sense(egui::Sense::click()),
            );
            if resp.clicked() {
                app.report_error("ui operation", format!("Broken note link: [[{target}]]"));
                app.open_note_panel(&slug, None);
            }
            resp
        }
    }
}

fn insert_tag_menu(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    id: egui::Id,
    content: &mut String,
    search: &mut String,
) -> bool {
    let mut inserted = false;
    ui.set_min_width(200.0);
    ui.label("Insert tag:");
    ui.text_edit_singleline(search);
    let matcher = SkimMatcherV2::default();
    let filter = search.to_lowercase();
    let tags = available_tags();
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            for tag in tags.into_iter().filter(|t| {
                filter.is_empty() || matcher.fuzzy_match(&t.to_lowercase(), &filter).is_some()
            }) {
                if ui.button(format!("#{tag}")).clicked() {
                    let insert = format!("#{tag}");
                    let mut state =
                        egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                    let idx = state
                        .cursor
                        .char_range()
                        .map(|r| r.primary.index)
                        .unwrap_or_else(|| content.chars().count());
                    let idx_byte = char_to_byte_index(content, idx);
                    content.insert_str(idx_byte, &insert);
                    inserted = true;
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(idx + insert.chars().count()),
                        )));
                    state.store(ctx, id);
                    search.clear();
                    ui.close_menu();
                }
            }
        });
    inserted
}

fn detect_shell() -> PathBuf {
    let ps7_path = env::var("ML_PWSH7_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe"));
    if ps7_path.exists() {
        return ps7_path;
    }
    let has_powershell = env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|p| p.join("powershell.exe").exists()))
        .unwrap_or(false);
    if has_powershell {
        PathBuf::from("powershell.exe")
    } else {
        PathBuf::from("cmd.exe")
    }
}

pub fn build_nvim_command(note_path: &Path) -> (Command, String) {
    let shell = detect_shell();
    let mut cmd = Command::new(&shell);
    if shell
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("cmd.exe"))
        .unwrap_or(false)
    {
        cmd.arg("/C").arg("nvim").arg(note_path);
    } else {
        cmd.arg("-NoLogo")
            .arg("-NoExit")
            .arg("-Command")
            .arg(format!("nvim {}", note_path.display()));
    }
    let cmd_str = format!("{:?}", cmd);
    (cmd, cmd_str)
}

pub fn build_wezterm_command(note_path: &Path) -> (Command, String) {
    let mut cmd = Command::new("wezterm");
    cmd.arg("start").arg("--").arg("nvim").arg(note_path);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let cmd_str = format!("{:?}", cmd);
    (cmd, cmd_str)
}

fn extract_tags(content: &str) -> Vec<String> {
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
    let mut tags: Vec<String> = Vec::new();
    let mut in_code = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        for cap in TAG_RE.captures_iter(line) {
            tags.push(cap[1].to_lowercase());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

pub fn extract_links(content: &str) -> Vec<(String, String)> {
    static MARKDOWN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());
    static LINK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://\S+|www\.\S+)").unwrap());

    let mut links: Vec<(String, String)> = Vec::new();

    for cap in MARKDOWN_RE.captures_iter(content) {
        let label = cap[1].to_string();
        let raw = cap[2].to_string();
        let url = if raw.starts_with("www.") {
            format!("https://{raw}")
        } else {
            raw.clone()
        };
        if Url::parse(&url)
            .ok()
            .filter(|u| u.scheme() == "https")
            .is_some()
        {
            links.push((label, url));
        }
    }

    let stripped = MARKDOWN_RE.replace_all(content, "");
    links.extend(LINK_RE.find_iter(&stripped).filter_map(|m| {
        let raw = m.as_str();
        let url = if raw.starts_with("www.") {
            format!("https://{raw}")
        } else {
            raw.to_string()
        };
        Url::parse(&url)
            .ok()
            .filter(|u| u.scheme() == "https")
            .map(|_| (raw.to_string(), url))
    }));

    links.sort();
    links.dedup();
    links
}

fn extract_snippet_around(content: &str, needle: &str) -> String {
    const WINDOW: usize = 44;
    let compact = content.replace('\n', " ");
    if compact.is_empty() {
        return String::new();
    }
    let lower = compact.to_lowercase();
    let needle_lower = needle.to_lowercase();
    if let Some(pos) = lower.find(&needle_lower) {
        let start = pos.saturating_sub(WINDOW);
        let end = (pos + needle_lower.len() + WINDOW).min(compact.len());
        let mut out = compact[start..end].trim().to_string();
        if start > 0 {
            out = format!("…{out}");
        }
        if end < compact.len() {
            out.push('…');
        }
        out
    } else {
        compact.chars().take(90).collect()
    }
}

fn format_note_updated(note: &Note) -> String {
    std::fs::metadata(&note.path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            chrono::DateTime::<chrono::Local>::from(t)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn alias_map_hash(notes: &[Note]) -> u64 {
    let mut aliases: Vec<(&str, &str)> = notes
        .iter()
        .filter_map(|note| {
            note.alias
                .as_deref()
                .map(|alias| (alias, note.slug.as_str()))
        })
        .collect();
    aliases.sort_unstable();
    let mut hasher = DefaultHasher::new();
    aliases.hash(&mut hasher);
    hasher.finish()
}

fn note_display_with_secondary(note: &Note) -> String {
    if let Some(alias) = note.alias.as_deref().filter(|a| !a.trim().is_empty()) {
        format!("{alias} ({} · {})", note.title, note.slug)
    } else {
        format!("{} ({})", note.title, note.slug)
    }
}

fn content_without_fenced_code(content: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            out.push('\n');
            continue;
        }
        if !in_code {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn note_reference_needles(current: &Note) -> Vec<(String, String)> {
    let mut needles = vec![
        (format!("[[{}]]", current.title), "wiki title".to_string()),
        (format!("[[{}]]", current.slug), "wiki slug".to_string()),
        (
            format!("link://note/{}", current.slug),
            "link id".to_string(),
        ),
        (
            format!("@note:{}", current.slug),
            "note mention".to_string(),
        ),
    ];
    if let Some(alias) = current.alias.as_deref().filter(|a| !a.trim().is_empty()) {
        needles.push((format!("[[{}]]", alias.trim()), "wiki alias".to_string()));
    }
    needles.sort();
    needles.dedup();
    needles
}

fn backlink_rows_for_note(
    current_note: &Note,
    tab: BacklinkTab,
    todos: &[crate::plugins::todo::TodoEntry],
    notes: &[Note],
) -> Vec<BacklinkRow> {
    let mut rows = Vec::new();
    let current_slug = current_note.slug.as_str();
    let needles = note_reference_needles(current_note);
    let current_searchable = content_without_fenced_code(&current_note.content);

    for todo in todos {
        let matched = todo
            .entity_refs
            .iter()
            .any(|r| r.kind == crate::common::entity_ref::EntityKind::Note && r.id == current_slug)
            .then(|| {
                (
                    format!("@note:{current_slug}"),
                    "todo linked to note".to_string(),
                )
            })
            .or_else(|| {
                needles
                    .iter()
                    .find(|(needle, _)| todo.text.contains(needle))
                    .cloned()
            });
        let current_note_mentions_todo = !todo.id.is_empty()
            && (current_searchable.contains(&format!("@todo:{}", todo.id))
                || current_note.entity_refs.iter().any(|r| {
                    r.kind == crate::common::entity_ref::EntityKind::Todo && r.id == todo.id
                }));
        let matched = matched.or_else(|| {
            current_note_mentions_todo.then(|| {
                (
                    format!("@todo:{}", todo.id),
                    "note mentions todo".to_string(),
                )
            })
        });
        if let Some((needle, reason)) = matched {
            if matches!(tab, BacklinkTab::LinkedTodos | BacklinkTab::Mentions) {
                rows.push(BacklinkRow {
                    title: todo.text.clone(),
                    type_badge: "Todo".to_string(),
                    updated: "n/a".to_string(),
                    snippet: extract_snippet_around(&todo.text, &needle),
                    reason,
                    note_slug: None,
                    todo_id: Some(todo.id.clone()),
                });
            }
        }
    }

    for note in notes {
        if note.slug == current_slug {
            continue;
        }
        let searchable = content_without_fenced_code(&note.content);
        let matched = note
            .links
            .iter()
            .any(|l| l == current_slug)
            .then(|| (format!("[[{current_slug}"), "wiki link".to_string()))
            .or_else(|| {
                note.entity_refs
                    .iter()
                    .any(|r| {
                        r.kind == crate::common::entity_ref::EntityKind::Note
                            && r.id == current_slug
                    })
                    .then(|| {
                        (
                            format!("@note:{current_slug}"),
                            "entity reference".to_string(),
                        )
                    })
            })
            .or_else(|| {
                needles
                    .iter()
                    .find(|(needle, _)| searchable.contains(needle))
                    .cloned()
            });
        if let Some((needle, reason)) = matched {
            let is_mention = reason.contains("mention") || reason.contains("entity");
            if (tab == BacklinkTab::RelatedNotes && !is_mention)
                || (tab == BacklinkTab::Mentions && is_mention)
            {
                rows.push(BacklinkRow {
                    title: note.title.clone(),
                    type_badge: "Note".to_string(),
                    updated: format_note_updated(note),
                    snippet: extract_snippet_around(&searchable, &needle),
                    reason,
                    note_slug: Some(note.slug.clone()),
                    todo_id: None,
                });
            }
        }
    }
    rows
}

fn extract_wiki_links(content: &str) -> Vec<String> {
    static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
    let mut links: Vec<String> = WIKI_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect();
    links.sort();
    links.dedup();
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{plugin::PluginManager, settings::Settings};
    use eframe::egui;
    use std::{
        fs,
        sync::{atomic::AtomicBool, Arc, Mutex, MutexGuard},
    };
    use tempfile::{tempdir, TempDir};

    static NOTES_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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

    fn empty_note(content: &str) -> Note {
        Note {
            title: String::new(),
            path: std::path::PathBuf::new(),
            content: content.to_string(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn insert_callout_at_char_preserves_unicode_boundaries() {
        let content = "αβ\nemoji 😀 end";
        let insert_at = "αβ\nemoji 😀".chars().count();

        let (updated, cursor) = insert_callout_at_char(content, insert_at, "warning");

        assert_eq!(updated, "αβ\nemoji 😀> [!WARNING] Title\n> Body\n end");
        assert_eq!(
            cursor,
            insert_at + "> [!WARNING] Title\n> Body\n".chars().count()
        );
        assert!(updated.is_char_boundary(char_to_byte_index(&updated, cursor)));
    }

    #[test]
    fn wrap_char_range_in_callout_preserves_unicode_selection_boundaries() {
        let content = "Intro\néclair 😀\nありがとう\nDone";
        let start = "Intro\n".chars().count();
        let end = "Intro\néclair 😀\nありがとう".chars().count();

        let (updated, selected_range) = wrap_char_range_in_callout(content, start, end, "todo");

        assert_eq!(
            updated,
            "Intro\n> [!TODO] Title\n> éclair 😀\n> ありがとう\nDone"
        );
        assert_eq!(
            &updated[char_range_to_byte_range(&updated, selected_range.start, selected_range.end).0
                ..char_range_to_byte_range(&updated, selected_range.start, selected_range.end).1],
            "éclair 😀\n> ありがとう"
        );
    }

    struct TempNotesDir {
        _guard: MutexGuard<'static, ()>,
        dir: TempDir,
        previous_notes_dir: Option<String>,
    }

    impl TempNotesDir {
        fn new() -> Self {
            let guard = NOTES_ENV_LOCK.lock().expect("notes env lock");
            let previous_notes_dir = std::env::var("ML_NOTES_DIR").ok();
            let dir = tempdir().expect("temp notes dir");
            unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };
            Self {
                _guard: guard,
                dir,
                previous_notes_dir,
            }
        }

        fn write_note(&self, file_name: &str, content: &str) {
            fs::write(self.dir.path().join(file_name), content).expect("write note markdown");
        }

        fn refresh_cache(&self) {
            crate::plugins::note::refresh_cache().unwrap();
        }
    }

    impl Drop for TempNotesDir {
        fn drop(&mut self) {
            if let Some(previous_notes_dir) = &self.previous_notes_dir {
                unsafe { std::env::set_var("ML_NOTES_DIR", previous_notes_dir) };
            } else {
                unsafe { std::env::remove_var("ML_NOTES_DIR") };
            }
            crate::plugins::note::refresh_cache().unwrap();
        }
    }

    fn note_with_slug(title: &str, slug: &str) -> Note {
        Note {
            title: title.into(),
            path: std::path::PathBuf::new(),
            content: format!("# {title}\n\nBody"),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    fn render_panel_once(ctx: &egui::Context, panel: &mut NotePanel, app: &mut LauncherApp) {
        let _ = ctx.run(Default::default(), |ctx| {
            panel.ui(ctx, app);
        });
    }

    #[test]
    fn outline_state_is_hidden_and_pending_navigation_ignored_when_disabled() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.outline_sidebar_enabled = false;
        let mut panel = NotePanel::from_note(empty_note("# One\n\n## Two"));
        panel.outline_open = true;
        panel.outline_filter = "two".into();
        panel.selected_outline_heading = Some("two".into());
        panel.pending_scroll_target = Some("two".into());

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(!panel.outline_open);
        assert!(panel.outline_filter.is_empty());
        assert!(panel.selected_outline_heading.is_none());
        assert!(panel.pending_scroll_target.is_none());
    }

    #[test]
    fn outline_rows_respect_max_depth_filter() {
        let content = "# One\n\n## Two\n\n### Three\n\n#### Four\n";
        let analysis = analyze_markdown(content);
        let rows = NotePanel::outline_rows_from_headings(
            "note",
            content,
            &analysis.headings,
            &analysis.sections,
            &HashSet::new(),
            2,
            false,
            "",
        );

        let titles = rows
            .iter()
            .map(|row| row.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(titles, vec!["One", "Two"]);
        assert!(rows.iter().all(|row| row.level <= 2));
    }

    #[test]
    fn link_menu_targets_are_cached_by_note_version() {
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nAlpha body");
        notes_dir.write_note("beta.md", "# Beta\n\nBeta body");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(note_with_slug("Current", "current"));
        panel.refresh_link_menu_results_if_needed();
        let target_refresh_count = panel.link_menu_target_refresh_count;

        panel.refresh_link_menu_results_if_needed();

        assert_eq!(panel.link_menu_target_refresh_count, target_refresh_count);
    }

    #[test]
    fn link_menu_search_reuses_targets_but_refreshes_results() {
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nAlpha body");
        notes_dir.write_note("beta.md", "# Beta\n\nBeta body");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(note_with_slug("Current", "current"));
        panel.refresh_link_menu_results_if_needed();
        let target_refresh_count = panel.link_menu_target_refresh_count;
        let result_refresh_count = panel.link_menu_result_refresh_count;

        panel.link_search = "alp".into();
        panel.refresh_link_menu_results_if_needed();

        assert_eq!(panel.link_menu_target_refresh_count, target_refresh_count);
        assert!(panel.link_menu_result_refresh_count > result_refresh_count);
        assert!(panel
            .link_menu_results
            .iter()
            .any(|result| result.display_title == "Alpha"));
        assert!(!panel
            .link_menu_results
            .iter()
            .any(|result| result.display_title == "Beta"));
    }

    #[test]
    fn link_menu_excludes_current_note_by_slug() {
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nAlpha body");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(note_with_slug("Alpha", "alpha"));
        let results = panel.link_menu_results_snapshot();

        assert!(!results.iter().any(|result| result.slug == "alpha"));
    }

    #[test]
    fn link_menu_targets_refresh_when_note_version_changes() {
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nAlpha body");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(note_with_slug("Current", "current"));
        let results = panel.link_menu_results_snapshot();
        assert!(results.iter().any(|result| result.display_title == "Alpha"));

        notes_dir.write_note("beta.md", "# Beta\n\nBeta body");
        notes_dir.refresh_cache();
        let results = panel.link_menu_results_snapshot();

        assert!(results.iter().any(|result| result.display_title == "Beta"));
    }

    #[test]
    fn section_keys_include_slug_anchor_title_and_line_tiebreaker() {
        let content = "# Title\n\n## Repeat\nBody\n\n## Repeat\nBody 2";
        let analysis = analyze_markdown(content);
        let first = &analysis.sections[1];
        let second = &analysis.sections[2];

        let first_key = NotePanel::section_key_for_slug("note-a", first);
        let second_key = NotePanel::section_key_for_slug("note-a", second);
        let other_note_key = NotePanel::section_key_for_slug("note-b", first);

        assert_ne!(first_key, second_key);
        assert_ne!(first_key, other_note_key);
        assert!(first_key.contains("note-a::repeat::Repeat::2"));
        assert!(second_key.contains("note-a::repeat-1::Repeat::5"));
    }

    #[test]
    fn disabled_collapsible_sections_keeps_all_ranges_visible() {
        let content = "# Title\n\n## Child\nHidden when enabled";
        let analysis = analyze_markdown(content);
        let child = &analysis.sections[1];
        let key = NotePanel::section_key_for_slug("note", child);
        let collapsed = HashSet::from([key]);

        let disabled_ranges =
            NotePanel::collapsed_body_ranges("note", &analysis.sections, &collapsed, false);
        let enabled_ranges =
            NotePanel::collapsed_body_ranges("note", &analysis.sections, &collapsed, true);

        assert!(disabled_ranges.is_empty());
        assert_eq!(enabled_ranges, vec![child.body_byte_range.clone()]);
    }

    #[test]
    fn wrap_selection_preserves_range() {
        let ctx = egui::Context::default();
        let mut panel = NotePanel::from_note(empty_note("hello world"));
        let id = egui::Id::new("note_content");
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(5),
            )));
        state.store(&ctx, id);
        panel.wrap_selection(&ctx, id, "**", "**");
        assert_eq!(panel.note.content, "**hello** world");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap();
        let [min, max] = range.sorted();
        assert_eq!((min.index, max.index), (2, 7));
        assert!(panel.pending_selection.is_none());
    }

    #[test]
    fn insert_link_replaces_selection() {
        let ctx = egui::Context::default();
        let mut panel = NotePanel::from_note(empty_note("hello world"));
        let id = egui::Id::new("note_content");
        panel.pending_selection = Some((6, 11));
        panel.link_url = "http://example.com".to_string();
        panel.insert_link(&ctx, id);
        assert_eq!(panel.note.content, "hello [world](http://example.com)");
        assert!(panel.pending_selection.is_none());
    }

    #[test]
    fn programmatic_selection_insertion_marks_derived_dirty_and_preserves_cursor() {
        let ctx = egui::Context::default();
        let mut panel = NotePanel::from_note(empty_note("hello world"));
        let id = egui::Id::new("note_content");
        panel.pending_selection = Some((6, 11));
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);

        panel.insert_text_at_cursor_or_selection(&ctx, id, "[[Other]]");

        assert_eq!(panel.note.content, "hello [[Other]]");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let idx = state.cursor.char_range().unwrap().primary.index;
        assert_eq!(idx, "hello [[Other]]".chars().count());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert!(panel.last_edit_at_secs.is_some());

        panel.refresh_fast_derived();
        assert_eq!(panel.derived.wiki_links, vec!["Other".to_string()]);
        assert!(!panel.fast_derived_dirty);
    }

    #[test]
    fn programmatic_cursor_insertion_marks_derived_dirty_and_preserves_cursor() {
        let ctx = egui::Context::default();
        let mut panel = NotePanel::from_note(empty_note("hello "));
        let id = egui::Id::new("note_content");
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(6),
            )));
        state.store(&ctx, id);
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);

        panel.insert_text_at_cursor_or_selection(&ctx, id, "[[Other]]");

        assert_eq!(panel.note.content, "hello [[Other]]");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let idx = state.cursor.char_range().unwrap().primary.index;
        assert_eq!(idx, "hello [[Other]]".chars().count());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert!(panel.last_edit_at_secs.is_some());

        panel.refresh_fast_derived();
        assert_eq!(panel.derived.wiki_links, vec!["Other".to_string()]);
        assert!(!panel.fast_derived_dirty);
    }

    #[test]
    fn insert_or_create_note_link_creates_and_inserts_at_cursor() {
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("hello "));
        panel.note.slug = "current-note".into();
        panel.link_new_name = "Brand New Note".into();
        let id = egui::Id::new("note_content");
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };
        let _ = crate::plugins::note::refresh_cache();

        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(6),
            )));
        state.store(&ctx, id);

        panel.insert_or_create_note_link(&ctx, id, &mut app);

        assert_eq!(panel.note.content, "hello [[brand-new-note]]");
        let notes = crate::plugins::note::load_notes().unwrap();
        assert!(notes.iter().any(|n| n.slug == "brand-new-note"));

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
        let _ = crate::plugins::note::refresh_cache();
    }

    #[test]
    fn insert_or_create_note_link_reuses_existing_note() {
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("hello "));
        panel.note.slug = "current-note".into();
        panel.link_new_name = "existing note".into();
        let id = egui::Id::new("note_content");
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };
        crate::plugins::note::append_note("Existing Note", "body").unwrap();
        let _ = crate::plugins::note::refresh_cache();

        panel.insert_or_create_note_link(&ctx, id, &mut app);

        assert_eq!(panel.note.content, "hello [[Existing Note]]");
        let notes = crate::plugins::note::load_notes().unwrap();
        assert_eq!(
            notes.iter().filter(|n| n.slug == "existing-note").count(),
            1
        );

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
        let _ = crate::plugins::note::refresh_cache();
    }

    #[test]
    fn insert_or_create_note_link_replaces_selection_and_sets_cursor() {
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("hello world"));
        panel.note.slug = "current-note".into();
        panel.pending_selection = Some((6, 11));
        panel.link_new_name = "Linked Note".into();
        let id = egui::Id::new("note_content");
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };
        let _ = crate::plugins::note::refresh_cache();

        panel.insert_or_create_note_link(&ctx, id, &mut app);

        assert_eq!(panel.note.content, "hello [[linked-note]]");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let idx = state.cursor.char_range().unwrap().primary.index;
        assert_eq!(idx, "hello [[linked-note]]".chars().count());

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
        let _ = crate::plugins::note::refresh_cache();
    }

    #[test]
    fn insert_or_create_note_link_empty_name_does_not_mutate_content() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("hello world"));
        panel.link_new_name = "   ".into();
        let id = egui::Id::new("note_content");

        panel.insert_or_create_note_link(&ctx, id, &mut app);

        assert_eq!(panel.note.content, "hello world");
    }

    #[test]
    fn formatting_wraps_current_selection() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("note_content");
        let mut panel = NotePanel::from_note(empty_note("hello world"));

        // Bold selection
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(6),
                egui::text::CCursor::new(11),
            )));
        state.store(&ctx, id);
        panel.wrap_selection(&ctx, id, "**", "**");
        assert_eq!(panel.note.content, "hello **world**");

        // Italic selection
        panel.note.content = "hello world".to_string();
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(5),
            )));
        state.store(&ctx, id);
        panel.wrap_selection(&ctx, id, "*", "*");
        assert_eq!(panel.note.content, "*hello* world");
    }

    #[test]
    fn click_opens_linked_note() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = show_wiki_link(ui, &mut app, "Second Note").rect;
            });
        });
        assert!(app.note_panels.is_empty());

        let pos = rect.center();
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show_wiki_link(ui, &mut app, "Second Note");
            });
        });

        assert_eq!(app.note_panels.len(), 1);
        assert_eq!(slugify(&app.note_panels[0].note.title), "second-note");
    }

    #[test]
    fn enter_in_note_panel_inserts_newline_without_query_execution() {
        use crate::actions::Action;
        use crate::plugins::note::Note;
        use std::path::PathBuf;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        app.query = "initial".into();
        app.results = vec![Action {
            label: "test".into(),
            desc: String::new(),
            action: "query:changed".into(),
            args: None,
        }];
        app.selected = Some(0);

        let note = Note {
            title: "Title".into(),
            path: PathBuf::new(),
            content: String::from("line1"),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        panel.view_mode = NoteViewMode::Edit;
        app.note_panels.push(panel);

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |_ui| {
                let mut panel = app.note_panels.remove(0);
                panel.ui(ctx, &mut app);
                app.note_panels.insert(0, panel);
            });
        });

        let mut input = egui::RawInput::default();
        // Keep this test independent from exact widget Y-positioning. The editor
        // is auto-focused on first edit frame, so Enter + text should append a
        // newline even if surrounding UI above the editor changes.
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
        input.events.push(egui::Event::Text("\n".into()));
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |_ui| {
                let mut panel = app.note_panels.remove(0);
                panel.ui(ctx, &mut app);
                app.note_panels.insert(0, panel);
            });
        });

        assert_eq!(app.query, "initial");
        assert_eq!(app.note_panels[0].note.content, "line1\n");
    }

    #[test]
    fn extract_links_filters_invalid() {
        let content = "visit http://example.com and http://exa%mple.com also [Rust](https://rust-lang.org) and https://rust-lang.org and https://rust-lang.org and www.example.com and www.example.com and www.exa%mple.com";
        let links = extract_links(content);
        assert_eq!(
            links,
            vec![
                ("Rust".to_string(), "https://rust-lang.org".to_string()),
                (
                    "https://rust-lang.org".to_string(),
                    "https://rust-lang.org".to_string(),
                ),
                (
                    "www.example.com".to_string(),
                    "https://www.example.com".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn handle_markdown_links_promotes_www() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ctx.output_mut(|o| {
                    o.open_url = Some(egui::OpenUrl::same_tab("www.example.com"));
                });
                handle_markdown_links(ui, &mut app);
            });
        });
        assert_eq!(
            output.platform_output.open_url.unwrap().url,
            "https://www.example.com"
        );
    }

    #[test]
    fn extract_wiki_links_dedupes() {
        let content = "links [[alpha]] and [[alpha]] and [[beta]]";
        let links = extract_wiki_links(content);
        assert_eq!(links, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn preprocess_wiki_links_rewrites() {
        let content = "See [[Target Note]]";
        let processed = preprocess_note_links(content, "current-note", &HashMap::new());
        assert_eq!(processed, "See [Target Note](note://target-note)");
    }

    #[test]
    fn preprocess_wiki_links_preserves_alias_text_and_uses_target_slug() {
        let content = "See [[Target Note|friendly label]] and [[  Spaced Note  ]]";
        let processed = preprocess_note_links(content, "current-note", &HashMap::new());
        assert_eq!(
            processed,
            "See [Target Note|friendly label](note://target-note) and [  Spaced Note  ](note://spaced-note)"
        );
    }

    #[test]
    fn preprocess_wiki_links_skips_self() {
        let content = "See [[Target Note]]";
        let processed = preprocess_note_links(content, "target-note", &HashMap::new());
        assert_eq!(processed, content);
    }

    #[test]
    fn snippet_extraction_is_deterministic() {
        let content = "one two three target-fragment four five six seven";
        let a = extract_snippet_around(content, "target-fragment");
        let b = extract_snippet_around(content, "target-fragment");
        assert_eq!(a, b);
        assert!(a.contains("target-fragment"));
    }

    #[test]
    fn backlinks_grouping_splits_categories() {
        use crate::common::entity_ref::EntityRef;
        use crate::plugins::todo::TodoEntry;

        let current = Note {
            title: "central".into(),
            path: std::path::PathBuf::new(),
            content: String::new(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "central".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let notes = vec![
            Note {
                title: "related".into(),
                path: std::path::PathBuf::new(),
                content: "[[central]] body".into(),
                tags: Vec::new(),
                links: vec!["central".into()],
                slug: "related".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
            Note {
                title: "mention".into(),
                path: std::path::PathBuf::new(),
                content: "see @note:central soon".into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: "mention".into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs: Vec::new(),
            },
        ];
        let todos = vec![TodoEntry {
            id: "t1".into(),
            text: "do thing @note:central".into(),
            done: false,
            priority: 1,
            tags: Vec::new(),
            entity_refs: vec![EntityRef::new(
                crate::common::entity_ref::EntityKind::Note,
                "central",
                None,
            )],
        }];

        let linked = backlink_rows_for_note(&current, BacklinkTab::LinkedTodos, &todos, &notes);
        let related = backlink_rows_for_note(&current, BacklinkTab::RelatedNotes, &todos, &notes);
        let mentions = backlink_rows_for_note(&current, BacklinkTab::Mentions, &todos, &notes);

        assert_eq!(linked.len(), 1);
        assert_eq!(related.len(), 1);
        assert!(mentions.len() >= 1);
    }

    #[test]
    fn toggle_hides_metadata_sections_in_ui() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note(
            "#tag [[linked-note]] https://example.com\n\nBody visible always",
        ));
        panel.view_mode = NoteViewMode::Edit;

        render_panel_once(&ctx, &mut panel, &mut app);
        assert!(panel.last_ui_sections.tags_visible);
        assert!(panel.last_ui_sections.links_visible);
        assert!(panel.last_ui_sections.backlinks_visible);
        assert!(panel.last_ui_sections.content_visible);

        panel.show_metadata = false;
        render_panel_once(&ctx, &mut panel, &mut app);
        assert!(!panel.last_ui_sections.tags_visible);
        assert!(!panel.last_ui_sections.links_visible);
        assert!(!panel.last_ui_sections.backlinks_visible);
        assert!(panel.last_ui_sections.content_visible);
    }

    #[test]
    fn backlinks_hidden_when_disabled() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.backlinks_enabled = false;
        let mut panel = NotePanel::from_note(empty_note(
            "#tag [[linked-note]] https://example.com

Body visible always",
        ));
        panel.view_mode = NoteViewMode::Edit;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.tags_visible);
        assert!(panel.last_ui_sections.links_visible);
        assert!(!panel.last_ui_sections.backlinks_visible);
        assert!(panel.last_ui_sections.content_visible);
        assert_eq!(panel.backlinks_render_count, 0);
    }

    #[test]
    fn backlink_rows_ignore_fenced_code_links() {
        let current = Note {
            title: "Central Note".into(),
            path: std::path::PathBuf::new(),
            content: String::new(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "central-note".into(),
            alias: Some("Hub".into()),
            aliases: vec!["Hub".into()],
            entity_refs: Vec::new(),
        };
        let coded = Note {
            title: "coded".into(),
            path: std::path::PathBuf::new(),
            content: "```
[[Central Note]]
[[central-note]]
[[Hub]]
link://note/central-note
@note:central-note
```"
            .into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "coded".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let linked = Note {
            title: "linked".into(),
            path: std::path::PathBuf::new(),
            content: "See [[Hub]] and link://note/central-note".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "linked".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let notes = vec![current, coded, linked];

        let related = backlink_rows_for_note(&notes[0], BacklinkTab::RelatedNotes, &[], &notes);
        let titles: Vec<_> = related.iter().map(|row| row.title.as_str()).collect();

        assert_eq!(titles, vec!["linked"]);
        assert!(related
            .iter()
            .any(|row| row.reason == "link id" || row.reason == "wiki alias"));
    }

    #[test]
    fn split_view_renders_details_and_backlinks_once_per_panel() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note(
            "#tag [[linked-note]] https://example.com\n\nBody visible in split",
        ));
        panel.view_mode = NoteViewMode::Split;
        app.note_settings.split_view_enabled = true;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.tags_visible);
        assert!(panel.last_ui_sections.links_visible);
        assert!(panel.last_ui_sections.backlinks_visible);
        assert!(panel.last_ui_sections.content_visible);
        assert_eq!(panel.metadata_details_render_count, 1);
        assert_eq!(panel.backlinks_render_count, 1);
    }

    #[test]
    fn initializes_view_mode_from_note_settings_default() {
        let mut settings = NoteSettings::default();
        settings.default_view_mode = NoteViewMode::Edit;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert_eq!(panel.view_mode, NoteViewMode::Edit);
    }

    #[test]
    fn split_default_falls_back_to_preview_when_split_disabled() {
        let mut settings = NoteSettings::default();
        settings.default_view_mode = NoteViewMode::Split;
        settings.split_view_enabled = false;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert_eq!(panel.view_mode, NoteViewMode::Preview);
    }

    #[test]
    fn split_default_falls_back_to_edit_when_rich_markdown_disabled() {
        let mut settings = NoteSettings::default();
        settings.default_view_mode = NoteViewMode::Split;
        settings.rich_markdown_enabled = false;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert_eq!(panel.view_mode, NoteViewMode::Edit);
    }

    #[test]
    fn split_mode_falls_back_when_disabled_at_render_time() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.split_view_enabled = false;
        let mut panel = NotePanel::from_note(empty_note("body"));
        panel.view_mode = NoteViewMode::Split;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert_eq!(panel.view_mode, NoteViewMode::Preview);
    }

    #[test]
    fn split_mode_falls_back_to_edit_when_rich_markdown_disabled_at_render_time() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.rich_markdown_enabled = false;
        let mut panel = NotePanel::from_note(empty_note("body"));
        panel.view_mode = NoteViewMode::Split;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert_eq!(panel.view_mode, NoteViewMode::Edit);
    }

    #[test]
    fn split_mode_does_not_request_initial_editor_focus() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("body"));
        panel.view_mode = NoteViewMode::Split;

        render_panel_once(&ctx, &mut panel, &mut app);

        let editor_has_focus = panel
            .last_textedit_id
            .map(|id| ctx.memory(|m| m.has_focus(id)))
            .unwrap_or(false);
        assert!(!editor_has_focus);
    }

    #[test]
    fn constructor_with_hidden_details_shows_show_details_label() {
        let panel = NotePanel::from_note_with_details(empty_note("body"), false);
        assert_eq!(panel.details_toggle_label(), "Show Details");
    }

    #[test]
    fn constructor_with_visible_details_shows_hide_details_label() {
        let panel = NotePanel::from_note_with_details(empty_note("body"), true);
        assert_eq!(panel.details_toggle_label(), "Hide Details");
    }

    #[test]
    fn toggle_preserves_tab_and_pagination_state() {
        let mut panel = NotePanel::from_note(empty_note("body"));
        panel.backlink_tab = BacklinkTab::Mentions;
        panel.backlink_page = 2;

        panel.show_metadata = false;
        panel.show_metadata = true;

        assert_eq!(panel.backlink_tab, BacklinkTab::Mentions);
        assert_eq!(panel.backlink_page, 2);
    }

    #[test]
    fn toggle_persists_note_detail_visibility_once_per_change() {
        use std::{fs, thread, time::Duration};
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");

        let mut settings = Settings::default();
        settings.note_show_details = false;
        settings
            .save(settings_path.to_str().expect("settings path"))
            .expect("write settings");
        app.settings_path = settings_path.to_string_lossy().to_string();

        let mut panel = NotePanel::from_note_with_details(empty_note("body"), false);
        panel.set_show_metadata(&mut app, true);

        let persisted = Settings::load(app.settings_path.as_str()).expect("load settings");
        assert!(persisted.note_show_details);

        let first_modified = fs::metadata(&settings_path)
            .expect("metadata")
            .modified()
            .expect("modified time");
        thread::sleep(Duration::from_millis(20));
        panel.set_show_metadata(&mut app, true);
        let second_modified = fs::metadata(&settings_path)
            .expect("metadata")
            .modified()
            .expect("modified time");

        assert_eq!(first_modified, second_modified);
    }

    #[test]
    fn derived_metadata_is_reused_without_save() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let note = Note {
            title: "Title".into(),
            path: std::path::PathBuf::new(),
            content: "# Title

Body with [[Other]]"
                .into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "title".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        let initial = panel.heavy_recompute_count;
        let _ = ctx.run(Default::default(), |ctx| {
            panel.ui(ctx, &mut app);
        });
        let _ = ctx.run(Default::default(), |ctx| {
            panel.ui(ctx, &mut app);
        });
        assert_eq!(panel.heavy_recompute_count, initial);
    }

    #[test]
    fn edits_do_not_trigger_heavy_recompute_every_frame() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let note = Note {
            title: "Title".into(),
            path: std::path::PathBuf::new(),
            content: "# Title\n\nBody".into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "title".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        let initial = panel.heavy_recompute_count;
        panel.mark_content_changed(f64::MAX);

        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |ctx| {
                panel.ui(ctx, &mut app);
            });
        }

        assert_eq!(panel.heavy_recompute_count, initial);
    }

    #[test]
    fn markdown_analysis_is_reused_until_content_changes() {
        let mut panel = NotePanel::from_note(empty_note("# Title\n\n- [ ] item"));
        let first_hash = {
            let analysis = panel.markdown_analysis();
            assert_eq!(analysis.headings.len(), 1);
            assert_eq!(analysis.task_items.len(), 1);
            panel.markdown_analysis_source_hash
        };

        let second_hash = {
            let analysis = panel.markdown_analysis();
            assert_eq!(analysis.headings.len(), 1);
            panel.markdown_analysis_source_hash
        };

        assert_eq!(first_hash, second_hash);

        panel.mark_content_changed(1.0);

        assert!(panel.markdown_analysis.is_none());
        assert!(panel.markdown_analysis_source_hash.is_none());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(1.0));
    }

    #[test]
    fn rendered_checkbox_toggle_marks_content_changed_and_invalidates_analysis() {
        let mut panel = NotePanel::from_note(empty_note("- [ ] item"));
        assert_eq!(panel.markdown_analysis().task_items.len(), 1);
        assert!(panel.markdown_analysis.is_some());
        assert!(panel.markdown_analysis_source_hash.is_some());
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);

        let item = panel.markdown_analysis().task_items[0].clone();
        panel.toggle_rendered_checkbox_marker(&item, 2.0);

        assert_eq!(panel.note.content, "- [x] item");
        assert!(panel.markdown_analysis.is_none());
        assert!(panel.markdown_analysis_source_hash.is_none());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(2.0));

        let analysis = panel.markdown_analysis();
        assert_eq!(analysis.task_items.len(), 1);
        assert!(analysis.task_items[0].checked);
    }

    #[test]
    fn disabled_task_lists_use_normal_markdown_without_cached_task_analysis() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.task_lists_enabled = false;
        let mut panel = NotePanel::from_note(empty_note("- [ ] item"));
        panel.markdown_analysis = None;
        panel.markdown_analysis_source_hash = None;

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!panel.render_segment(ui, &mut app, "- [ ] item", 0, ctx));
            });
        });

        assert_eq!(panel.note.content, "- [ ] item");
        assert!(panel.markdown_analysis.is_none());
        assert!(panel.markdown_analysis_source_hash.is_none());
    }

    #[test]
    fn disabled_interactive_checkboxes_do_not_mutate_during_preview_render() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.task_lists_enabled = true;
        app.note_settings.interactive_checkboxes_enabled = false;
        let mut panel =
            NotePanel::from_note(empty_note("* [ ] item with [[Link]] #tag @todo:missing"));

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!panel.render_segment(
                    ui,
                    &mut app,
                    "* [ ] item with [[Link]] #tag @todo:missing",
                    0,
                    ctx,
                ));
            });
        });

        assert_eq!(
            panel.note.content,
            "* [ ] item with [[Link]] #tag @todo:missing"
        );
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);
        assert!(panel.markdown_analysis.is_some());
    }

    #[test]
    fn save_recomputes_derived_and_updates_links() {
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        let note = Note {
            title: "Source".into(),
            path: std::path::PathBuf::new(),
            content: "# Source

[[alpha]]"
                .into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        let before = panel.heavy_recompute_count;
        panel.note.content = "# Source

[[beta]]"
            .into();
        panel.fast_derived_dirty = true;
        panel.heavy_recompute_requested = true;
        panel.save(&mut app);

        assert!(panel.heavy_recompute_count > before);
        assert_eq!(panel.note.links, vec!["beta".to_string()]);

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn save_invalidates_backlink_rows_when_slug_changes() {
        use std::fs;
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        fs::write(
            dir.path().join("alpha.md"),
            "# Alpha

body",
        )
        .unwrap();
        fs::write(
            dir.path().join("other.md"),
            "# Other

[[alpha]]",
        )
        .unwrap();
        let _ = crate::plugins::note::refresh_cache();

        let note = crate::plugins::note::note_cache_snapshot()
            .into_iter()
            .find(|n| n.slug == "alpha")
            .expect("alpha note should exist in cache");
        let mut panel = NotePanel::from_note(note);
        assert_eq!(panel.derived.backlink_rows_related_notes.len(), 1);

        panel.note.slug.clear();
        panel.note.content = "# Beta

body"
            .into();
        panel.save(&mut app);

        assert_eq!(panel.note.slug, "beta");
        assert!(panel.derived.backlink_rows_related_notes.is_empty());

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn preprocess_uses_injected_todo_label_map() {
        let mut labels = HashMap::new();
        labels.insert("abc".to_string(), "Readable Label".to_string());
        let processed = preprocess_note_links("ref @todo:abc", "current", &labels);
        assert_eq!(processed, "ref [Readable Label](todo://abc)");
    }

    #[test]
    fn preprocess_todo_labels_fall_back_to_id_and_ignore_invalid_tokens() {
        let mut labels = HashMap::new();
        labels.insert("abc-123".to_string(), "Readable Todo".to_string());
        let processed = preprocess_note_links(
            "mapped @todo:abc-123 fallback @todo:missing ignored @todo:not.valid",
            "current",
            &labels,
        );
        assert_eq!(
            processed,
            "mapped [Readable Todo](todo://abc-123) fallback [missing](todo://missing) ignored [not](todo://not).valid"
        );
    }

    #[test]
    fn preprocess_preview_markdown_uses_same_link_and_todo_mapping() {
        let mut labels = HashMap::new();
        labels.insert("build".to_string(), "Build launcher".to_string());

        assert_eq!(
            preprocess_preview_markdown("[[Other Note]] @todo:build", "current", &labels),
            preprocess_note_links("[[Other Note]] @todo:build", "current", &labels)
        );
    }

    #[test]
    fn parse_note_image_target_handles_assets_and_width_suffixes() {
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        let parsed = parse_note_image_target("assets/image.png|320");
        assert_eq!(parsed.rel, "assets/image.png");
        assert_eq!(parsed.full, dir.path().join("assets").join("image.png"));
        assert_eq!(parsed.width, Some(320.0));

        let parsed_no_width = parse_note_image_target("relative/image.png");
        assert_eq!(parsed_no_width.rel, "relative/image.png");
        assert_eq!(parsed_no_width.full, PathBuf::from("relative/image.png"));
        assert_eq!(parsed_no_width.width, None);

        let parsed_bad_width = parse_note_image_target("assets/image.png|wide");
        assert_eq!(parsed_bad_width.rel, "assets/image.png");
        assert_eq!(parsed_bad_width.width, None);

        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn preview_preprocess_preserves_markdown_blocks_and_raw_html() {
        let mut labels = HashMap::new();
        labels.insert("build".to_string(), "Build launcher".to_string());
        let source = concat!(
            "# Heading\n\n",
            "- bullet\n",
            "1. number\n",
            "- [ ] task\n\n",
            "> [!NOTE]\n",
            "> callout\n\n",
            "```rust\nfn main() {}\n```\n\n",
            "`inline` <span>raw</span>\n\n",
            "| a | b |\n| - | - |\n| c | d |\n\n",
            "---\n\n",
            "![Alt](assets/pic.png|120)\n",
            "[external](https://example.com) [[Other Note]] @todo:build\n"
        );
        let processed = preprocess_preview_markdown(source, "current", &labels);

        assert!(processed.contains("# Heading"));
        assert!(processed.contains("- bullet"));
        assert!(processed.contains("1. number"));
        assert!(processed.contains("- [ ] task"));
        assert!(processed.contains("> [!NOTE]\n> callout"));
        assert!(processed.contains("```rust\nfn main() {}\n```"));
        assert!(processed.contains("`inline` <span>raw</span>"));
        assert!(processed.contains("| a | b |"));
        assert!(processed.contains("---"));
        assert!(processed.contains("![Alt](assets/pic.png|120)"));
        assert!(processed.contains("[external](https://example.com)"));
        assert!(processed.contains("[Other Note](note://other-note)"));
        assert!(processed.contains("[Build launcher](todo://build)"));
    }

    #[test]
    fn preview_preprocess_keeps_current_note_wiki_link_literal() {
        let labels = HashMap::new();
        let processed = preprocess_preview_markdown("[[Current Note]]", "current-note", &labels);
        assert_eq!(processed, "[[Current Note]]");
    }

    #[test]
    fn add_alias_metadata_inserts_unicode_alias_after_title() {
        let content = "# Café\n\nBody\n";
        let updated = add_alias_metadata(content, "東京 🦀");
        assert_eq!(updated, "# Café\nAlias: 東京 🦀\n\nBody\n");
    }

    #[test]
    fn add_alias_metadata_extends_existing_unicode_aliases() {
        let content = "# Café\nAliases: 東京, naïve\n\nBody";
        let updated = add_alias_metadata(content, "🦀 Crab");
        assert_eq!(updated, "# Café\nAliases: 東京, naïve, 🦀 Crab\n\nBody");
    }

    #[test]
    fn remove_alias_metadata_removes_unicode_alias_without_touching_others() {
        let content = "# Café\nAliases: 東京, naïve, 🦀 Crab\n\nBody\n";
        let updated = remove_alias_metadata(content, "NAÏVE");
        assert_eq!(updated, "# Café\nAliases: 東京, 🦀 Crab\n\nBody\n");
    }

    #[test]
    fn rename_alias_metadata_renames_unicode_alias() {
        let content = "# Café\nAlias: 東京\n\nBody";
        let updated = rename_alias_metadata(content, "東京", "京都 🦀");
        assert_eq!(updated, "# Café\nAlias: 京都 🦀\n\nBody");
    }

    #[test]
    fn note_scheme_link_opens_panel() {
        use crate::plugins::note::Note;
        use std::path::PathBuf;
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };
        let note = Note {
            title: "Title".into(),
            path: PathBuf::new(),
            content: String::from("dummy"),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.output_mut(|o| {
                o.open_url = Some(egui::OpenUrl::same_tab("note://linked-note"));
            });
            panel.ui(ctx, &mut app);
        });
        drop(dir);
        if let Some(p) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", p) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
        let _ = crate::plugins::note::refresh_cache();
        assert_eq!(app.note_panels.len(), 1);
        assert_eq!(slugify(&app.note_panels[0].note.title), "linked-note");
    }
}
