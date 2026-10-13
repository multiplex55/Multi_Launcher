use crate::actions::screenshot::{Mode as ScreenshotMode, capture};
use crate::common::slug::slugify;
use crate::file_search::actions::{
    ExplorerAction, execute_explorer_action, resolve_explorer_action,
};
use crate::gui::LauncherApp;
use crate::notes_markdown::{
    MarkdownAnalysis, MarkdownCallout, MarkdownHeading, MarkdownSection, MarkdownTaskItem,
    PlainLinkConversionReport, analyze_markdown, convert_plain_links,
    task_list::toggle_task_marker,
};
use crate::plugins::note::{
    Note, NoteExternalOpen, NoteLinkMenuTarget, NoteTarget, append_note, assets_dir,
    available_tags, extract_aliases, image_files, load_notes, note_alias_conflict_display_labels,
    note_cache_snapshot, note_cache_snapshot_with_version,
    note_link_menu_targets_snapshot_with_version, note_version, resolve_note_query, save_note,
    save_note_image_asset,
};
use crate::plugins::todo::{TODO_FILE, load_todos_or_last_good, todo_version};
use crate::process::configure_background_command;
use crate::settings::{NoteSettings, NoteViewMode};
use chrono::{DateTime, Local, TimeZone};
use eframe::egui::{self, Color32, FontId, Key, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_toast::{Toast, ToastKind, ToastOptions};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use image::{RgbaImage, imageops::FilterType};
use once_cell::sync::Lazy;
use regex::Regex;
use rfd::FileDialog;
use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use url::Url;

const BACKLINK_PAGE_SIZE: usize = 12;
const HEAVY_RECOMPUTE_IDLE_DEBOUNCE: Duration = Duration::from_millis(250);
const HEAVY_RECOMPUTE_RETRY_DELAY: Duration = Duration::from_secs(1);
const NOTE_LINK_CONTEXT_MENU_RESULT_LIMIT: usize = 50;

// C1-B attribution is compiled only into test builds and is active only while
// a test owns a `NoteRefreshObservationScope`. It intentionally does not feed
// the existing Track C collector or alter frozen C0/C1-A result artifacts.
#[cfg(test)]
#[derive(Clone, Copy)]
enum NoteRefreshStringClass {
    Needles = 0,
    FencedContent = 1,
    Snippet = 2,
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum NoteRefreshDurationKind {
    Refresh,
    Snapshot,
    TodoLoad,
    LabelMapBuild,
    FusedTraversal,
    DerivedInstall,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NoteRefreshFusedTraversalObservation {
    calls: u64,
    todo_visits: u64,
    note_visits: u64,
    elapsed_nanos: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NoteRefreshObservation {
    refresh_calls: u64,
    successful_recomputes: u64,
    failed_snapshots: u64,
    snapshot_calls: u64,
    snapshot_successes: u64,
    todo_load_calls: u64,
    label_map_visits: u64,
    fused_traversal: NoteRefreshFusedTraversalObservation,
    fenced_content_calls: u64,
    fenced_input_bytes: u64,
    fenced_output_bytes: u64,
    materialized_string_counts: [u64; 3],
    materialized_string_len_bytes: [u64; 3],
    materialized_string_capacity_bytes: [u64; 3],
    refresh_elapsed_nanos: u64,
    snapshot_elapsed_nanos: u64,
    todo_load_elapsed_nanos: u64,
    label_map_build_elapsed_nanos: u64,
    derived_install_elapsed_nanos: u64,
}

#[cfg(test)]
#[derive(Clone)]
struct NoteRefreshObserverHandle(std::rc::Rc<std::cell::RefCell<NoteRefreshObservation>>);

#[cfg(test)]
impl NoteRefreshObserverHandle {
    fn current() -> Option<Self> {
        NOTE_REFRESH_OBSERVER.with(|observer| observer.borrow().clone())
    }

    fn record(&self, update: impl FnOnce(&mut NoteRefreshObservation)) {
        update(&mut self.0.borrow_mut());
    }

    fn record_materialized_string(&self, class: NoteRefreshStringClass, value: &String) {
        self.record(|observation| {
            let index = class as usize;
            observation.materialized_string_counts[index] =
                observation.materialized_string_counts[index].saturating_add(1);
            observation.materialized_string_len_bytes[index] =
                observation.materialized_string_len_bytes[index].saturating_add(value.len() as u64);
            observation.materialized_string_capacity_bytes[index] = observation
                .materialized_string_capacity_bytes[index]
                .saturating_add(value.capacity() as u64);
        });
    }

    fn add_elapsed(&self, kind: NoteRefreshDurationKind, elapsed_nanos: u64) {
        self.record(|observation| match kind {
            NoteRefreshDurationKind::Refresh => {
                observation.refresh_elapsed_nanos = observation
                    .refresh_elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
            NoteRefreshDurationKind::Snapshot => {
                observation.snapshot_elapsed_nanos = observation
                    .snapshot_elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
            NoteRefreshDurationKind::TodoLoad => {
                observation.todo_load_elapsed_nanos = observation
                    .todo_load_elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
            NoteRefreshDurationKind::LabelMapBuild => {
                observation.label_map_build_elapsed_nanos = observation
                    .label_map_build_elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
            NoteRefreshDurationKind::FusedTraversal => {
                observation.fused_traversal.elapsed_nanos = observation
                    .fused_traversal
                    .elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
            NoteRefreshDurationKind::DerivedInstall => {
                observation.derived_install_elapsed_nanos = observation
                    .derived_install_elapsed_nanos
                    .saturating_add(elapsed_nanos);
            }
        });
    }
}

#[cfg(test)]
struct NoteRefreshObservationTimer {
    observer: Option<NoteRefreshObserverHandle>,
    kind: NoteRefreshDurationKind,
    started: Option<Instant>,
}

#[cfg(test)]
impl NoteRefreshObservationTimer {
    fn start(observer: Option<NoteRefreshObserverHandle>, kind: NoteRefreshDurationKind) -> Self {
        let started = observer.as_ref().map(|_| Instant::now());
        Self {
            observer,
            kind,
            started,
        }
    }
}

#[cfg(test)]
impl Drop for NoteRefreshObservationTimer {
    fn drop(&mut self) {
        if let (Some(observer), Some(started)) = (&self.observer, self.started) {
            observer.add_elapsed(
                self.kind,
                started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            );
        }
    }
}

#[cfg(test)]
thread_local! {
    static NOTE_REFRESH_OBSERVER: std::cell::RefCell<Option<NoteRefreshObserverHandle>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
struct NoteRefreshObservationScope {
    handle: NoteRefreshObserverHandle,
    previous: Option<NoteRefreshObserverHandle>,
}

#[cfg(test)]
impl NoteRefreshObservationScope {
    fn new() -> Self {
        let handle = NoteRefreshObserverHandle(std::rc::Rc::new(std::cell::RefCell::new(
            NoteRefreshObservation::default(),
        )));
        let previous = NOTE_REFRESH_OBSERVER
            .with(|observer| std::mem::replace(&mut *observer.borrow_mut(), Some(handle.clone())));
        Self { handle, previous }
    }

    fn snapshot(&self) -> NoteRefreshObservation {
        self.handle.0.borrow().clone()
    }

    fn reset(&self) {
        *self.handle.0.borrow_mut() = NoteRefreshObservation::default();
    }
}

#[cfg(test)]
impl Drop for NoteRefreshObservationScope {
    fn drop(&mut self) {
        NOTE_REFRESH_OBSERVER.with(|observer| {
            *observer.borrow_mut() = self.previous.take();
        });
    }
}

#[derive(Clone, Copy)]
struct HeavyRefreshRetry {
    notes_revision: u64,
    todo_revision: u64,
    backlinks_enabled: bool,
    retry_at: Instant,
}

#[derive(Debug)]
pub(super) struct ClipboardRgbaData {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardImagePasteOutcome {
    NotHandled,
    Handled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextInsertionKind {
    ReplacedSelection,
    InsertedAtCaret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextInsertionOutcome {
    kind: TextInsertionKind,
    resulting_char_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NotePasteRequest {
    has_egui_paste: bool,
    has_egui_shortcut: bool,
    has_windows_shortcut: bool,
}

/// Stateful edge detector for the native Windows paste chord.
///
/// The policy is a V rising edge while Ctrl is already down. Pressing Ctrl after
/// V is held is therefore not a new chord. Sampling must happen every frame,
/// including while the editor is unfocused, so a held V cannot become an edge
/// merely because focus moved to the editor.
#[derive(Default, Debug)]
struct NativePasteDetector {
    v_was_down: bool,
}

impl NativePasteDetector {
    fn update(&mut self, ctrl_down: bool, v_down: bool) -> bool {
        let edge = ctrl_down && v_down && !self.v_was_down;
        self.v_was_down = v_down;
        edge
    }

    fn sample(&mut self) -> bool {
        let (ctrl_down, v_down) = native_paste_key_state();
        self.update(ctrl_down, v_down)
    }
}

#[cfg(target_os = "windows")]
fn native_paste_key_state() -> (bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_V,
    };
    // SAFETY: GetAsyncKeyState only observes process-global keyboard state and
    // accepts the two valid virtual-key constants supplied here.
    unsafe {
        let down = |key: VIRTUAL_KEY| (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0;
        (down(VK_CONTROL), down(VK_V))
    }
}

#[cfg(not(target_os = "windows"))]
fn native_paste_key_state() -> (bool, bool) {
    (false, false)
}

fn is_note_paste_shortcut(event: &egui::Event) -> bool {
    matches!(
        event,
        egui::Event::Key {
            key: Key::V,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } if !modifiers.alt
            && (modifiers.command || (!cfg!(target_os = "macos") && modifiers.ctrl))
    )
}

fn detect_note_paste_request(
    events: &[egui::Event],
    has_windows_shortcut: bool,
) -> Option<NotePasteRequest> {
    let request = NotePasteRequest {
        has_egui_paste: events
            .iter()
            .any(|event| matches!(event, egui::Event::Paste(_))),
        has_egui_shortcut: events.iter().any(is_note_paste_shortcut),
        has_windows_shortcut,
    };
    (request.has_egui_paste || request.has_egui_shortcut || request.has_windows_shortcut)
        .then_some(request)
}

fn consume_note_paste_request(events: &mut Vec<egui::Event>, request: NotePasteRequest) {
    events.retain(|event| {
        !(request.has_egui_paste && matches!(event, egui::Event::Paste(_)))
            && !(request.has_egui_shortcut && is_note_paste_shortcut(event))
    });
}

fn rgba_image_from_clipboard_data(
    width: usize,
    height: usize,
    bytes: Vec<u8>,
) -> anyhow::Result<RgbaImage> {
    let width =
        u32::try_from(width).map_err(|_| anyhow::anyhow!("clipboard image width is too large"))?;
    let height = u32::try_from(height)
        .map_err(|_| anyhow::anyhow!("clipboard image height is too large"))?;
    if width == 0 || height == 0 {
        anyhow::bail!("clipboard image has zero width or height");
    }
    let expected = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| anyhow::anyhow!("clipboard image dimensions overflow"))?;
    if bytes.len() != expected {
        anyhow::bail!(
            "clipboard image contains {} RGBA bytes; expected {expected}",
            bytes.len()
        );
    }
    RgbaImage::from_raw(width, height, bytes)
        .ok_or_else(|| anyhow::anyhow!("clipboard image RGBA data is malformed"))
}

fn note_image_markdown(filename: &str) -> String {
    format!("![{filename}](assets/{filename})")
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SplitPaneWidths {
    editor: f32,
    preview: f32,
    separator: f32,
}

fn split_pane_widths(total_width: f32, split_ratio: f32, separator_width: f32) -> SplitPaneWidths {
    let separator = separator_width.max(0.0);
    let usable_width = (total_width.max(0.0) - separator).max(0.0);
    let editor = usable_width * split_ratio.clamp(0.2, 0.8);
    SplitPaneWidths {
        editor,
        preview: usable_width - editor,
        separator,
    }
}

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

fn heading_display_text(heading: &MarkdownHeading) -> &str {
    heading.title.as_str()
}

fn task_display_text(task_item: &MarkdownTaskItem) -> &str {
    task_item.text.as_str()
}

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

fn selected_char_range(state: &egui::widgets::text_edit::TextEditState) -> Option<(usize, usize)> {
    state.cursor.char_range().and_then(|range| {
        let range = range.as_sorted_char_range();
        (range.start != range.end).then_some((range.start.0, range.end.0))
    })
}

fn normalize_char_range(range: (usize, usize), char_count: usize) -> Option<(usize, usize)> {
    let (start, end) = if range.0 <= range.1 {
        range
    } else {
        (range.1, range.0)
    };
    let normalized = (start.min(char_count), end.min(char_count));
    (normalized.0 != normalized.1).then_some(normalized)
}

/// Right-clicks on either selection boundary are considered inside. Egui can
/// report the caret at either edge, and neither edge should discard a selection.
fn caret_is_within_range(caret: usize, range: (usize, usize)) -> bool {
    range.0 <= caret && caret <= range.1
}

fn restore_char_range(state: &mut egui::widgets::text_edit::TextEditState, range: (usize, usize)) {
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(range.0),
            egui::text::CCursor::new(range.1),
        )));
}

fn collapse_cursor(state: &mut egui::widgets::text_edit::TextEditState, index: usize) {
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(index),
        )));
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum MarkdownLinkAction {
    NoteReference(String),
    TodoReference(String),
    Filesystem(ExplorerAction),
    ExternalUrl(String),
    InvalidNoteLink(String),
    InvalidFilesystemUrl(String),
}

/// Classifies a clicked markdown target and performs click-time filesystem
/// validation, but deliberately has no UI or process-launching side effects.
fn classify_markdown_link(target: &str) -> MarkdownLinkAction {
    match Url::parse(target) {
        Ok(url) => match url.scheme() {
            "note" => url.host_str().map_or_else(
                || MarkdownLinkAction::InvalidNoteLink("missing note slug".to_string()),
                |slug| MarkdownLinkAction::NoteReference(slug.to_string()),
            ),
            "todo" => url.host_str().map_or_else(
                || MarkdownLinkAction::InvalidNoteLink("missing todo id".to_string()),
                |id| MarkdownLinkAction::TodoReference(id.to_string()),
            ),
            "file" => {
                let path = match url.to_file_path() {
                    Ok(path) => path,
                    Err(()) => {
                        return MarkdownLinkAction::InvalidFilesystemUrl(format!(
                            "cannot convert file URL to a local path: {target}"
                        ));
                    }
                };
                match resolve_explorer_action(&path) {
                    Ok(
                        action @ (ExplorerAction::OpenDirectory(_) | ExplorerAction::RevealFile(_)),
                    ) => MarkdownLinkAction::Filesystem(action),
                    Ok(ExplorerAction::Unsupported { reason, .. }) => {
                        MarkdownLinkAction::InvalidFilesystemUrl(format!(
                            "cannot open {}: {reason}",
                            path.display()
                        ))
                    }
                    Err(err) => MarkdownLinkAction::InvalidFilesystemUrl(err.to_string()),
                }
            }
            _ => MarkdownLinkAction::ExternalUrl(target.to_string()),
        },
        Err(err) if target.starts_with("file:") => {
            MarkdownLinkAction::InvalidFilesystemUrl(format!("invalid file URL: {err}"))
        }
        Err(err) if target.starts_with("note:") || target.starts_with("todo:") => {
            MarkdownLinkAction::InvalidNoteLink(format!("invalid note link: {err}"))
        }
        Err(_) if target.starts_with("www.") => {
            MarkdownLinkAction::ExternalUrl(format!("https://{target}"))
        }
        Err(_) => MarkdownLinkAction::ExternalUrl(target.to_string()),
    }
}

fn handle_markdown_links(ui: &egui::Ui, app: &mut LauncherApp) {
    let open_url = ui.ctx().output_mut(|output| {
        let command_index = output
            .commands
            .iter()
            .rposition(|command| matches!(command, egui::OutputCommand::OpenUrl(_)))?;
        match output.commands.remove(command_index) {
            egui::OutputCommand::OpenUrl(open_url) => Some(open_url),
            _ => unreachable!("the selected output command was checked as an OpenUrl"),
        }
    });
    if let Some(mut open_url) = open_url {
        match classify_markdown_link(&open_url.url) {
            MarkdownLinkAction::NoteReference(slug) => app.open_note_panel(&slug, None),
            MarkdownLinkAction::TodoReference(todo_id) => open_todo_reference(app, &todo_id),
            MarkdownLinkAction::Filesystem(action) => {
                if let Err(err) = execute_explorer_action(action) {
                    app.report_error("filesystem link", err);
                }
            }
            MarkdownLinkAction::ExternalUrl(url) => {
                open_url.url = url;
                ui.ctx().open_url(open_url);
            }
            MarkdownLinkAction::InvalidNoteLink(err) => app.report_error("note link", err),
            MarkdownLinkAction::InvalidFilesystemUrl(err) => {
                app.report_error("filesystem link", err);
            }
        }
    }
}

fn open_todo_reference(app: &mut LauncherApp, todo_id: &str) {
    let todos = load_todos_or_last_good(TODO_FILE);
    if let Some((idx, _)) = todos.iter().enumerate().find(|(_, t)| t.id == todo_id) {
        app.todo_view_dialog.open_edit(idx);
    } else {
        app.todo_view_dialog.open();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPlainLinkConversion {
    source: String,
    report: PlainLinkConversionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlainLinkConfirmOutcome {
    Applied,
    Refreshed,
    NoCandidates,
}

pub struct NotePanel {
    pub open: bool,
    note: Note,
    saved_content_baseline: String,
    discard_unsaved_prompt: bool,
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
    split_ratio: f32,
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
    pending_cursor_position: Option<usize>,
    link_dialog_open: bool,
    link_text: String,
    link_url: String,
    link_new_dialog_open: bool,
    link_new_name: String,
    pending_plain_link_conversion: Option<PendingPlainLinkConversion>,
    new_alias: String,
    alias_rename_inputs: HashMap<String, String>,
    native_paste_detector: NativePasteDetector,

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
    last_backlinks_enabled: Option<bool>,
    heavy_refresh_retry: Option<HeavyRefreshRetry>,
    #[cfg(test)]
    heavy_recompute_count: usize,
    #[cfg(test)]
    fail_next_heavy_snapshot: bool,
    #[cfg(test)]
    fail_next_link_menu_snapshot: bool,
    #[cfg(test)]
    last_ui_sections: NotePanelUiSections,
    #[cfg(test)]
    last_content_rect: Option<egui::Rect>,
    #[cfg(test)]
    last_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    last_split_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    last_split_preview_rect: Option<egui::Rect>,
    #[cfg(test)]
    last_outline_rect: Option<egui::Rect>,
    #[cfg(test)]
    last_preview_heading_rects: Vec<egui::Rect>,
    #[cfg(test)]
    last_preview_fallback_rect: Option<egui::Rect>,
    #[cfg(test)]
    metadata_details_render_count: usize,
    #[cfg(test)]
    backlinks_render_count: usize,
    /// Render-path observations only; they never participate in paste routing.
    #[cfg(test)]
    last_paste_interceptor_id: Option<egui::Id>,
    #[cfg(test)]
    last_paste_outcome: Option<ClipboardImagePasteOutcome>,
    #[cfg(test)]
    last_paste_request: Option<NotePasteRequest>,
}

#[cfg(test)]
#[derive(Default, Clone, Copy)]
struct NotePanelUiSections {
    tags_visible: bool,
    links_visible: bool,
    backlinks_visible: bool,
    content_visible: bool,
    outline_visible: bool,
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

#[cfg(test)]
mod alias_warning_test_observer {
    use std::cell::RefCell;
    use std::time::Instant;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct Stats {
        pub lookup_calls: u64,
        pub formatting_calls: u64,
        pub formatting_nanos: u64,
        pub output_bytes: u64,
    }

    thread_local! {
        static STATS: RefCell<Option<Stats>> = RefCell::new(None);
    }

    pub(super) struct Scope;

    impl Scope {
        pub(super) fn begin() -> Self {
            STATS.with(|stats| {
                let mut stats = stats.borrow_mut();
                assert!(stats.is_none(), "alias warning observer is already active");
                *stats = Some(Stats::default());
            });
            Self
        }

        pub(super) fn snapshot(&self) -> Stats {
            STATS.with(|stats| {
                stats
                    .borrow()
                    .as_ref()
                    .copied()
                    .expect("alias warning observer remains active")
            })
        }
    }

    pub(super) fn lookup_called() {
        STATS.with(|stats| {
            if let Some(stats) = stats.borrow_mut().as_mut() {
                stats.lookup_calls = stats.lookup_calls.saturating_add(1);
            }
        });
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            STATS.with(|stats| {
                stats.borrow_mut().take();
            });
        }
    }

    pub(super) fn formatting_started() -> Option<Instant> {
        STATS.with(|stats| stats.borrow().is_some().then(Instant::now))
    }

    pub(super) fn formatting_finished(started: Option<Instant>, output_bytes: usize) {
        let Some(started) = started else {
            return;
        };
        let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        STATS.with(|stats| {
            if let Some(stats) = stats.borrow_mut().as_mut() {
                stats.formatting_calls = stats.formatting_calls.saturating_add(1);
                stats.formatting_nanos = stats.formatting_nanos.saturating_add(elapsed);
                stats.output_bytes = stats
                    .output_bytes
                    .saturating_add(output_bytes.min(u64::MAX as usize) as u64);
            }
        });
    }
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
        match crate::settings::Settings::update(&app.settings_path, |settings| {
            settings.note_show_details = self.show_metadata;
            Ok(())
        }) {
            Ok(_) => app.note_show_details = self.show_metadata,
            Err(err) => {
                app.report_error(
                    "ui operation",
                    format!("Failed to save note detail visibility setting: {err}"),
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
        let saved_content_baseline = note.content.clone();
        let mut panel = Self {
            open: true,
            note,
            saved_content_baseline,
            discard_unsaved_prompt: false,
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
            split_ratio: 0.5,
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
            pending_cursor_position: None,
            link_dialog_open: false,
            link_text: String::new(),
            link_url: String::new(),
            link_new_dialog_open: false,
            link_new_name: String::new(),
            pending_plain_link_conversion: None,
            new_alias: String::new(),
            alias_rename_inputs: HashMap::new(),
            native_paste_detector: NativePasteDetector::default(),
            focus_textedit_next_frame: false,
            last_textedit_id: None,
            derived: NoteDerivedView::default(),
            fast_derived_dirty: true,
            heavy_recompute_requested: true,
            last_edit_at_secs: None,
            last_notes_version: 0,
            last_todo_revision: 0,
            last_backlinks_enabled: None,
            heavy_refresh_retry: None,
            #[cfg(test)]
            heavy_recompute_count: 0,
            #[cfg(test)]
            fail_next_heavy_snapshot: false,
            #[cfg(test)]
            fail_next_link_menu_snapshot: false,
            #[cfg(test)]
            last_ui_sections: NotePanelUiSections::default(),
            #[cfg(test)]
            last_content_rect: None,
            #[cfg(test)]
            last_editor_rect: None,
            #[cfg(test)]
            last_split_editor_rect: None,
            #[cfg(test)]
            last_split_preview_rect: None,
            #[cfg(test)]
            last_outline_rect: None,
            #[cfg(test)]
            last_preview_heading_rects: Vec::new(),
            #[cfg(test)]
            last_preview_fallback_rect: None,
            #[cfg(test)]
            metadata_details_render_count: 0,
            #[cfg(test)]
            backlinks_render_count: 0,
            #[cfg(test)]
            last_paste_interceptor_id: None,
            #[cfg(test)]
            last_paste_outcome: None,
            #[cfg(test)]
            last_paste_request: None,
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
        let slug = self.note.slug.clone();
        let collapsed = self.collapsed_sections.iter().cloned().collect::<Vec<_>>();
        if let Err(err) = crate::note_ui_state::update(&path, |state| {
            state.set_collapsed_sections(slug, collapsed);
        }) {
            self.report_ui_state_error_once(
                app,
                format!("Failed to save note UI state to {}: {err}", path.display()),
            );
        }
    }

    fn expand_all_collapsible_sections(&mut self, app: &mut LauncherApp) {
        self.collapsed_sections.clear();
        self.persist_collapsed_sections_state(app);
    }

    fn collapse_all_collapsible_sections(
        &mut self,
        app: &mut LauncherApp,
        sections: &[MarkdownSection],
    ) {
        let keys = sections
            .iter()
            .filter(|section| !self.is_main_title_section(section))
            .map(|section| self.section_key(section))
            .collect();
        self.collapsed_sections = keys;
        self.persist_collapsed_sections_state(app);
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
        self.heavy_refresh_retry = None;
    }

    fn save_alias_metadata_change(&mut self, app: &mut LauncherApp) {
        self.sync_aliases_from_content();
        let mut note = self.note.clone();
        match save_note(&mut note, true) {
            Ok(true) => {
                self.note = note;
                self.sync_aliases_from_content();
                self.mark_current_content_saved();
            }
            Ok(false) => self.overwrite_prompt = true,
            Err(err) => app.report_error("note save", format!("Failed to save aliases: {err}")),
        }
    }

    fn alias_collision_warning(&self, alias: &str) -> anyhow::Result<Option<String>> {
        #[cfg(test)]
        alias_warning_test_observer::lookup_called();
        let alias = alias.trim();
        if alias.is_empty() {
            return Ok(None);
        }
        let (_, Some(labels)) = note_alias_conflict_display_labels(alias, &self.note.slug)? else {
            return Ok(None);
        };
        #[cfg(test)]
        let formatting_started = alias_warning_test_observer::formatting_started();
        let labels = labels.join(", ");
        let message = format!("Alias \"{alias}\" is already used by {labels}");
        #[cfg(test)]
        alias_warning_test_observer::formatting_finished(formatting_started, message.len());
        Ok(Some(message))
    }

    fn warn_alias_collision(&self, app: &mut LauncherApp, alias: &str) {
        match self.alias_collision_warning(alias) {
            Ok(Some(message)) => app.add_toast(Toast {
                text: message.into(),
                kind: ToastKind::Warning,
                options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),

                style: Default::default(),
            }),
            Ok(None) => {}
            Err(err) => app.report_error(
                "note alias warning",
                format!("Could not check for conflicting note aliases: {err}"),
            ),
        }
    }

    fn refresh_heavy_derived(&mut self, force: bool, backlinks_enabled: bool) -> bool {
        #[cfg(test)]
        let refresh_observer = {
            let observer = NoteRefreshObserverHandle::current();
            if let Some(observer) = &observer {
                observer.record(|observation| {
                    observation.refresh_calls = observation.refresh_calls.saturating_add(1);
                });
            }
            observer
        };
        #[cfg(test)]
        let _refresh_observation_timer = NoteRefreshObservationTimer::start(
            refresh_observer.clone(),
            NoteRefreshDurationKind::Refresh,
        );

        let current_notes_version = note_version();
        let current_todo_revision = todo_version();
        let settings_changed = self.last_backlinks_enabled != Some(backlinks_enabled);
        let refresh_needed = force
            || settings_changed
            || self.last_notes_version != current_notes_version
            || self.last_todo_revision != current_todo_revision
            || self.heavy_recompute_requested
            || self.heavy_refresh_retry.is_some();
        if !refresh_needed {
            return true;
        }

        if !backlinks_enabled {
            self.last_notes_version = current_notes_version;
            self.last_todo_revision = current_todo_revision;
            self.last_backlinks_enabled = Some(false);
            self.heavy_recompute_requested = false;
            self.heavy_refresh_retry = None;
            return true;
        }

        let mut relationship_timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::NoteRelationshipRefresh,
        );

        #[cfg(test)]
        let snapshot = {
            if let Some(observer) = &refresh_observer {
                observer.record(|observation| {
                    observation.snapshot_calls = observation.snapshot_calls.saturating_add(1);
                });
            }
            let _snapshot_observation_timer = NoteRefreshObservationTimer::start(
                refresh_observer.clone(),
                NoteRefreshDurationKind::Snapshot,
            );
            let snapshot = if std::mem::take(&mut self.fail_next_heavy_snapshot) {
                Err(anyhow::anyhow!("injected note snapshot failure"))
            } else {
                note_cache_snapshot_with_version()
            };
            if let Some(observer) = &refresh_observer {
                observer.record(|observation| {
                    if snapshot.is_ok() {
                        observation.snapshot_successes =
                            observation.snapshot_successes.saturating_add(1);
                    } else {
                        observation.failed_snapshots =
                            observation.failed_snapshots.saturating_add(1);
                    }
                });
            }
            snapshot
        };
        #[cfg(not(test))]
        let snapshot = note_cache_snapshot_with_version();

        let (snapshot_notes_revision, notes) = match snapshot {
            Ok(snapshot) => snapshot,
            Err(_) => {
                relationship_timer.finish(crate::performance::track_c::Outcome::Error);
                self.heavy_refresh_retry = Some(HeavyRefreshRetry {
                    notes_revision: current_notes_version,
                    todo_revision: current_todo_revision,
                    backlinks_enabled,
                    retry_at: Instant::now() + HEAVY_RECOMPUTE_RETRY_DELAY,
                });
                return false;
            }
        };

        let _recompute_timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::NoteHeavyRecompute);
        #[cfg(test)]
        {
            if let Some(observer) = &refresh_observer {
                observer.record(|observation| {
                    observation.todo_load_calls = observation.todo_load_calls.saturating_add(1);
                });
            }
        }
        #[cfg(test)]
        let todo_load_observation_timer = NoteRefreshObservationTimer::start(
            refresh_observer.clone(),
            NoteRefreshDurationKind::TodoLoad,
        );
        let todos = load_todos_or_last_good(TODO_FILE);
        #[cfg(test)]
        drop(todo_load_observation_timer);

        #[cfg(test)]
        let label_map_observation_timer = NoteRefreshObservationTimer::start(
            refresh_observer.clone(),
            NoteRefreshDurationKind::LabelMapBuild,
        );
        #[cfg(test)]
        let label_map_observer = refresh_observer.clone();
        let todo_label_map = todos
            .iter()
            .filter(|todo| {
                #[cfg(test)]
                if let Some(observer) = &label_map_observer {
                    observer.record(|observation| {
                        observation.label_map_visits =
                            observation.label_map_visits.saturating_add(1);
                    });
                }
                !todo.id.is_empty()
            })
            .map(|todo| (todo.id.clone(), todo.text.clone()))
            .collect::<HashMap<_, _>>();
        #[cfg(test)]
        drop(label_map_observation_timer);

        let mut mentions_timer = crate::performance::track_c::Timer::start(
            crate::performance::track_c::Phase::NoteMentionsScan,
        );
        let prepared_rows = backlink_rows_for_note(&self.note, &todos, &notes);
        mentions_timer.set_work_units(todos.len().saturating_add(notes.len()));
        mentions_timer.finish(crate::performance::track_c::Outcome::Completed);

        #[cfg(test)]
        let derived_install_observation_timer = NoteRefreshObservationTimer::start(
            refresh_observer.clone(),
            NoteRefreshDurationKind::DerivedInstall,
        );
        self.derived.todo_label_map = todo_label_map;
        self.derived.backlink_rows_linked_todos = prepared_rows.linked_todos;
        self.derived.backlink_rows_related_notes = prepared_rows.related_notes;
        self.derived.backlink_rows_mentions = prepared_rows.mentions;

        // The note revision came with this exact snapshot. Todo revisions are
        // sampled before loading so a concurrent write can only leave an older
        // applied revision and cause a safe follow-up recompute.
        self.last_notes_version = snapshot_notes_revision;
        self.last_todo_revision = current_todo_revision;
        self.last_backlinks_enabled = Some(true);
        self.heavy_recompute_requested = false;
        self.heavy_refresh_retry = None;
        #[cfg(test)]
        drop(derived_install_observation_timer);
        relationship_timer.set_work_units(todos.len().saturating_add(notes.len()));
        relationship_timer.finish(crate::performance::track_c::Outcome::Completed);
        #[cfg(test)]
        {
            self.heavy_recompute_count += 1;
            if let Some(observer) = &refresh_observer {
                observer.record(|observation| {
                    observation.successful_recomputes =
                        observation.successful_recomputes.saturating_add(1);
                });
            }
        }
        true
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
        self.heavy_refresh_retry = None;
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
        let mut refresh_check_timer =
            crate::performance::MetricTimer::start(crate::performance::Metric::NoteRefreshCheck);
        refresh_check_timer.set_work_units(0);
        let current_notes_version = note_version();
        let current_todo_revision = todo_version();
        let notes_changed = self.last_notes_version != current_notes_version;
        let todos_changed = self.last_todo_revision != current_todo_revision;
        let settings_changed = self.last_backlinks_enabled != Some(backlinks_enabled);
        let now_secs = ctx.input(|i| i.time);
        let debounce_remaining = self.last_edit_at_secs.map(|edited_at| {
            let remaining = edited_at + HEAVY_RECOMPUTE_IDLE_DEBOUNCE.as_secs_f64() - now_secs;
            if remaining.is_finite() {
                Duration::from_secs_f64(
                    remaining.clamp(0.0, HEAVY_RECOMPUTE_IDLE_DEBOUNCE.as_secs_f64()),
                )
            } else {
                HEAVY_RECOMPUTE_IDLE_DEBOUNCE
            }
        });
        let local_edit_due = self.heavy_recompute_requested
            && debounce_remaining
                .map(|remaining| remaining.is_zero())
                .unwrap_or(true);
        let refresh_needed = notes_changed
            || todos_changed
            || settings_changed
            || local_edit_due
            || self.heavy_refresh_retry.is_some();

        if refresh_needed {
            let retry_matches_current_inputs = self.heavy_refresh_retry.is_some_and(|retry| {
                retry.notes_revision == current_notes_version
                    && retry.todo_revision == current_todo_revision
                    && retry.backlinks_enabled == backlinks_enabled
            });
            if retry_matches_current_inputs && let Some(retry) = self.heavy_refresh_retry {
                let retry_remaining = retry.retry_at.saturating_duration_since(Instant::now());
                if !retry_remaining.is_zero() {
                    drop(refresh_check_timer);
                    self.request_heavy_refresh_retry(ctx);
                    return;
                }
            }

            refresh_check_timer.set_work_units(1);
            drop(refresh_check_timer);
            if !self.refresh_heavy_derived(false, backlinks_enabled) {
                self.request_heavy_refresh_retry(ctx);
            }
            return;
        }

        drop(refresh_check_timer);
        if self.heavy_recompute_requested
            && let Some(debounce_remaining) = debounce_remaining
            && !debounce_remaining.is_zero()
        {
            ctx.request_repaint_after(debounce_remaining);
        }
    }

    fn request_heavy_refresh_retry(&self, ctx: &egui::Context) {
        if let Some(retry) = self.heavy_refresh_retry {
            let retry_remaining = retry.retry_at.saturating_duration_since(Instant::now());
            ctx.request_repaint_after(retry_remaining);
        }
    }

    fn refresh_link_menu_targets_if_needed(&mut self) -> Option<u64> {
        let current_version = note_version();
        if self.link_menu_targets_version == Some(current_version) {
            return Some(current_version);
        }

        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_link_menu_snapshot) {
            return None;
        }

        let (target_revision, targets) = match note_link_menu_targets_snapshot_with_version() {
            Ok(snapshot) => snapshot,
            Err(_) => return None,
        };
        self.link_menu_targets = targets
            .into_iter()
            .map(|target: NoteLinkMenuTarget| LinkMenuResult {
                display_title: target.display_title().to_string(),
                search_text: target.search_text(),
                slug: target.slug,
            })
            .collect();
        self.link_menu_targets_version = Some(target_revision);
        self.invalidate_link_menu_results();
        #[cfg(test)]
        {
            self.link_menu_target_refresh_count += 1;
        }
        Some(target_revision)
    }

    fn refresh_link_menu_results_if_needed(&mut self) {
        let Some(current_version) = self.refresh_link_menu_targets_if_needed() else {
            return;
        };
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

    pub(super) fn note_close_observation(&self) -> super::query_observation::NoteCloseSoleNote {
        use super::query_observation::{NoteCloseSoleNote, Q11_NOTE_MARKER, Q11_NOTE_SLUG};
        NoteCloseSoleNote {
            slug_digest: self
                .note
                .slug
                .bytes()
                .fold(0xcbf29ce484222325, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                }),
            fixture_slug: self.note.slug == Q11_NOTE_SLUG,
            fixture_marker: self.note.content.contains(Q11_NOTE_MARKER),
            pending_discard: self.discard_unsaved_prompt,
            rendered_discard: None,
        }
    }

    pub(crate) fn note_content_clone_for_mutation(&self) -> Note {
        self.note.clone()
    }

    #[cfg(test)]
    pub(crate) fn test_view_mode(&self) -> NoteViewMode {
        self.view_mode
    }

    #[cfg(test)]
    pub(crate) fn test_set_view_mode(&mut self, view_mode: NoteViewMode) {
        self.set_view_mode(view_mode);
    }

    fn set_view_mode(&mut self, view_mode: NoteViewMode) {
        if self.view_mode != view_mode {
            self.view_mode = view_mode;
            // Edit and Split live in different Ui namespaces. Until the new
            // editor renders there is no valid ID for that variant.
            self.last_textedit_id = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_last_edit_at_secs(&self) -> Option<f64> {
        self.last_edit_at_secs
    }

    pub(crate) fn has_unsaved_changes(&self) -> bool {
        self.note.content != self.saved_content_baseline
    }

    fn mark_current_content_saved(&mut self) {
        self.saved_content_baseline.clone_from(&self.note.content);
    }

    pub(crate) fn request_close(&mut self, app: &mut LauncherApp) {
        if app.note_save_on_close {
            self.save(app);
            if !self.overwrite_prompt {
                self.open = false;
            }
        } else if !app.note_confirm_discard_unsaved_changes || !self.has_unsaved_changes() {
            self.open = false;
            self.discard_unsaved_prompt = false;
        } else {
            self.open = true;
            self.discard_unsaved_prompt = true;
        }
    }

    fn keep_editing(&mut self) {
        self.discard_unsaved_prompt = false;
        self.open = true;
        self.focus_textedit_next_frame = true;
    }

    fn discard_changes(&mut self) {
        self.discard_unsaved_prompt = false;
        self.open = false;
    }

    pub(crate) fn replace_content_after_saved_external_mutation(
        &mut self,
        content: String,
        now_secs: f64,
    ) {
        self.replace_content_from_mutation(content, now_secs);
        self.mark_current_content_saved();
    }

    pub(crate) fn replace_content_from_mutation(&mut self, content: String, now_secs: f64) {
        if self.note.content == content {
            return;
        }

        self.note.content = content;
        self.mark_content_changed(now_secs);
    }

    fn plain_link_candidate_count(report: &PlainLinkConversionReport) -> usize {
        report.web_links + report.directories + report.files
    }

    fn start_plain_link_conversion(&mut self) -> bool {
        let source = self.note.content.clone();
        let report = convert_plain_links(&source);
        if Self::plain_link_candidate_count(&report) == 0 {
            self.pending_plain_link_conversion = None;
            false
        } else {
            self.pending_plain_link_conversion =
                Some(PendingPlainLinkConversion { source, report });
            true
        }
    }

    fn cancel_plain_link_conversion(&mut self) {
        self.pending_plain_link_conversion = None;
        self.focus_textedit_next_frame = true;
    }

    fn confirm_plain_link_conversion(&mut self, now_secs: f64) -> PlainLinkConfirmOutcome {
        let Some(pending) = self.pending_plain_link_conversion.as_ref() else {
            return PlainLinkConfirmOutcome::NoCandidates;
        };
        if self.note.content != pending.source {
            let source = self.note.content.clone();
            let report = convert_plain_links(&source);
            if Self::plain_link_candidate_count(&report) == 0 {
                self.pending_plain_link_conversion = None;
                self.focus_textedit_next_frame = true;
                return PlainLinkConfirmOutcome::NoCandidates;
            }
            self.pending_plain_link_conversion =
                Some(PendingPlainLinkConversion { source, report });
            return PlainLinkConfirmOutcome::Refreshed;
        }

        let transformed = self
            .pending_plain_link_conversion
            .take()
            .expect("pending conversion checked above")
            .report
            .content;
        self.replace_content_from_mutation(transformed, now_secs);
        self.focus_textedit_next_frame = true;
        PlainLinkConfirmOutcome::Applied
    }

    fn show_no_plain_links_toast(app: &mut LauncherApp) {
        app.add_toast(Toast {
            text: "No convertible plain links found.".into(),
            kind: ToastKind::Info,
            options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),

            style: Default::default(),
        });
    }

    pub(crate) fn replace_content_after_external_mutation(&mut self, content: String) {
        self.replace_content_from_mutation(content, 0.0);
    }

    pub(crate) fn replace_note_after_external_mutation(&mut self, mut note: Note) {
        let content = std::mem::take(&mut note.content);
        self.note = note;
        self.replace_content_from_mutation(content, 0.0);
    }

    pub(crate) fn invalidate_note_derived_data_after_external_mutation(&mut self) {
        self.invalidate_markdown_analysis();
        self.invalidate_link_menu_results();
        self.link_menu_targets_version = None;
        self.fast_derived_dirty = true;
        self.heavy_recompute_requested = true;
        self.heavy_refresh_retry = None;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        self.ui_with_note_close_observation(ctx, app, None);
    }

    pub(super) fn ui_with_note_close_observation(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        mut observed_discard: Option<
            &mut Option<super::query_observation::NoteCloseRenderedDiscard>,
        >,
    ) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut save_now = false;
        let screen_rect = ctx.content_rect();
        let margin = 24.0;
        let max_width = (screen_rect.width() - margin).max(200.0);
        let max_height = (screen_rect.height() - margin).max(150.0);
        // NOTE: `egui::TextEditState` cursor indices are char-based and we also mutate `self` inside the
        // window closure (save, open externally, etc.). Don't capture borrows of `self.note.slug` in
        // the closure environment - keep IDs based on an owned clone instead.
        let slug = self.note.slug.clone();
        let window_id = egui::Id::new(("note_panel_window", self.note.slug.clone()));
        let scroll_id_source = ("note_scroll", slug.clone());
        let text_id_source = ("note_text", slug);

        egui::Window::new(self.note.title.clone())
            .id(window_id)
            .open(&mut open)
            .resizable(true)
            .default_size(app.note_panel_default_size)
            .min_size(egui::vec2(200.0, 150.0))
            .max_width(max_width)
            .max_height(max_height)
            .movable(true)
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    self.last_ui_sections = NotePanelUiSections::default();
                    self.last_content_rect = None;
                    self.last_editor_rect = None;
                    self.last_split_editor_rect = None;
                    self.last_split_preview_rect = None;
                    self.last_outline_rect = None;
                    self.last_preview_heading_rects.clear();
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
                if self.show_metadata {
                    self.render_metadata_details_section(ui, app);
                }
                self.render_note_body_row(
                    ui,
                    app,
                    ctx,
                    scroll_id_source.clone(),
                    text_id_source.clone(),
                );
            });

        // If the panel is closing, ensure we don't leave egui focus on a widget
        // that will no longer exist this frame. This avoids AccessKit panics
        // about focused nodes missing from the accessibility tree.

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
                            if let Some(id) = self.last_textedit_id {
                                self.insert_link(ctx, id);
                                // Return focus to the editor after insertion.
                                self.focus_textedit_next_frame = true;
                            }
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
                        let confirm = ui.add_enabled(
                            !is_empty && self.last_textedit_id.is_some(),
                            egui::Button::new("Link"),
                        );
                        if confirm.clicked() {
                            if let Some(id) = self.last_textedit_id {
                                self.insert_or_create_note_link(ctx, id, app);
                                self.focus_textedit_next_frame = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.link_new_name.clear();
                            self.link_new_dialog_open = false;
                        }
                    });
                });
            self.link_new_dialog_open &= open_link_new;
        }
        if self.pending_plain_link_conversion.is_some() {
            let mut dialog_open = true;
            let mut convert = false;
            let mut cancel = false;
            egui::Window::new("Convert plain links?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut dialog_open)
                .show(ctx, |ui| {
                    let report = &self
                        .pending_plain_link_conversion
                        .as_ref()
                        .expect("dialog requires pending conversion")
                        .report;
                    egui::Grid::new("plain_link_conversion_counts").show(ui, |ui| {
                        ui.label("Web links:");
                        ui.label(report.web_links.to_string());
                        ui.end_row();
                        ui.label("Directories:");
                        ui.label(report.directories.to_string());
                        ui.end_row();
                        ui.label("Files:");
                        ui.label(report.files.to_string());
                        ui.end_row();
                        ui.label("Already linked/protected candidates:");
                        ui.label(report.skipped_existing.to_string());
                        ui.end_row();
                        ui.label("Skipped/invalid paths:");
                        ui.label(report.skipped_invalid_paths.to_string());
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        convert = ui.button("Convert").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if cancel || !dialog_open {
                self.cancel_plain_link_conversion();
            } else if convert
                && self.confirm_plain_link_conversion(ctx.input(|i| i.time))
                    == PlainLinkConfirmOutcome::NoCandidates
            {
                Self::show_no_plain_links_toast(app);
            }
        }
        if save_now {
            self.save(app);
        }
        if !open {
            self.request_close(app);
        }

        if self.discard_unsaved_prompt {
            let mut prompt_open = true;
            let mut discard = false;
            let mut keep_editing = false;
            egui::Window::new("Discard unsaved changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .open(&mut prompt_open)
                .show(ctx, |ui| {
                    ui.label("This note has changes that have not been saved.");
                    ui.horizontal(|ui| {
                        let response = ui.button("Discard Changes");
                        if let Some(observed) = observed_discard.as_deref_mut() {
                            *observed = super::query_observation::observe_note_discard_response(
                                ui,
                                &response,
                                &self.note.slug,
                            );
                        }
                        discard = response.clicked();
                        keep_editing = ui.button("Keep Editing").clicked();
                    });
                });
            if discard {
                self.discard_changes();
            } else if keep_editing || !prompt_open {
                self.keep_editing();
            }
        }

        if !self.open
            && let Some(id) = self.last_textedit_id
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
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
                                self.mark_current_content_saved();
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
                                self.mark_current_content_saved();
                                self.finish_save(app);
                                self.overwrite_prompt = false;
                            }
                        }
                    });
                });
        }
        self.request_heavy_refresh_retry(ctx);
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
                        egui::Popup::open_id(ui.ctx(), popup_id);
                    }
                }
            }
            if self.show_open_with_menu {
                let mut close = false;
                if egui::Popup::new(popup_id, ui.ctx().clone(), &open_resp, open_resp.layer_id)
                    .open_memory(None)
                    .show(|ui| {
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
                    egui::Popup::close_id(ui.ctx(), popup_id);
                    self.show_open_with_menu = false;
                }
            }
            if !app.note_settings.rich_markdown_enabled
                && matches!(self.view_mode, NoteViewMode::Preview | NoteViewMode::Split)
            {
                self.set_view_mode(NoteViewMode::Edit);
            }
            if !app.note_settings.can_use_split() && matches!(self.view_mode, NoteViewMode::Split) {
                let view_mode = if app.note_settings.rich_markdown_enabled {
                    NoteViewMode::Preview
                } else {
                    NoteViewMode::Edit
                };
                self.set_view_mode(view_mode);
            }
            if matches!(self.view_mode, NoteViewMode::Preview | NoteViewMode::Split)
                && ui.button("Edit").clicked()
            {
                self.set_view_mode(NoteViewMode::Edit);
                self.focus_textedit_next_frame = true;
            }
            if !matches!(self.view_mode, NoteViewMode::Preview) && ui.button("Render").clicked() {
                let previous_editor_id = self.last_textedit_id;
                self.set_view_mode(NoteViewMode::Preview);
                if let Some(id) = previous_editor_id {
                    ui.ctx().memory_mut(|m| m.surrender_focus(id));
                }
            }
            if app.note_settings.can_use_split()
                && !matches!(self.view_mode, NoteViewMode::Split)
                && ui.button("Split").clicked()
            {
                self.set_view_mode(NoteViewMode::Split);
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
                    self.expand_all_collapsible_sections(app);
                }
                if ui.button("Collapse all").clicked() {
                    self.collapse_all_collapsible_sections(app, &sections);
                }
            } else if !app.note_settings.collapsible_sections_enabled {
                self.collapsed_sections.clear();
            }
            if app.note_settings.outline_sidebar_enabled {
                ui.separator();
                let outline_label = if self.outline_open {
                    "Hide Outline"
                } else {
                    "Show Outline"
                };

                if ui.button(outline_label).clicked() {
                    self.outline_open = !self.outline_open;
                }
            }
        });
        save_now
    }

    fn render_metadata_details_section(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        self.render_metadata_details(ui, app);
        if app.note_settings.backlinks_enabled {
            self.render_backlinks(ui, app);
        }
    }

    fn render_outline_sidebar(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        outline_size: egui::Vec2,
    ) {
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
        #[cfg(test)]
        {
            self.last_ui_sections.outline_visible = true;
        }

        self.outline_width = self.outline_width.clamp(120.0, 360.0);
        let outline_width = self.outline_width.min(outline_size.x.max(0.0));
        let outline_scroll_id = ("note_outline_scroll", self.note.slug.clone());

        ui.vertical(|ui| {
            ui.set_width(outline_width);
            egui::ScrollArea::vertical()
                .id_salt(outline_scroll_id)
                .max_height(outline_size.y)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(outline_width);
                    ui.horizontal(|ui| {
                        ui.label("Outline");
                        ui.add(
                            egui::DragValue::new(&mut self.outline_width)
                                .range(120.0..=360.0)
                                .speed(4.0)
                                .prefix("width "),
                        );
                    });
                    self.outline_width = self.outline_width.clamp(120.0, 360.0);
                    ui.text_edit_singleline(&mut self.outline_filter);
                    if rows.is_empty() {
                        if self.outline_filter.is_empty() {
                            ui.label("No headings in this note.");
                        } else {
                            ui.label("No headings match the filter.");
                        }
                    }
                    for row in rows {
                        ui.horizontal(|ui| {
                            ui.add_space((row.level.saturating_sub(1) as f32) * 12.0);
                            if app.note_settings.collapsible_sections_enabled {
                                let marker = if row.collapsible {
                                    if row.collapsed { "▶" } else { "▼" }
                                } else {
                                    "•"
                                };
                                ui.small(marker);
                            }
                            let selected = self.selected_outline_heading.as_deref()
                                == Some(row.normalized_anchor.as_str());
                            if ui.selectable_label(selected, &row.title).clicked() {
                                self.handle_outline_row_click(&row, ui.ctx());
                            }
                        });
                    }
                });
        });
    }

    fn render_note_body_row(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        scroll_id_source: (&'static str, String),
        text_id_source: (&'static str, String),
    ) {
        let body_width = ui.available_width().max(0.0);
        let body_height = ui.available_height();
        let has_outline = app.note_settings.outline_sidebar_enabled && self.outline_open;

        if !has_outline {
            let body_size = egui::vec2(body_width, body_height);
            let content = ui.allocate_ui_with_layout(
                body_size,
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(body_width);
                    self.render_note_content_area(
                        ui,
                        app,
                        ctx,
                        scroll_id_source,
                        text_id_source,
                        body_size,
                    );
                },
            );

            #[cfg(test)]
            {
                self.last_content_rect = Some(egui::Rect::from_min_size(
                    content.response.rect.min,
                    body_size,
                ));
            }

            #[cfg(not(test))]
            {
                let _ = &content;
            }

            return;
        }

        ui.horizontal_top(|ui| {
            let outline_separator_width = 6.0 + ui.spacing().item_spacing.x * 2.0;
            let content_width;
            {
                self.outline_width = self.outline_width.clamp(120.0, 360.0);
                let outline_budget = (body_width - outline_separator_width).max(0.0);
                let outline_width = self.outline_width.min(outline_budget).min(body_width);
                content_width = (body_width - outline_width - outline_separator_width).max(0.0);
                let outline = ui.allocate_ui_with_layout(
                    egui::vec2(outline_width, body_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(outline_width);
                        self.render_outline_sidebar(
                            ui,
                            app,
                            egui::vec2(outline_width, body_height),
                        );
                    },
                );
                #[cfg(test)]
                {
                    self.last_outline_rect = Some(egui::Rect::from_min_size(
                        outline.response.rect.min,
                        egui::vec2(outline_width, body_height),
                    ));
                }
                #[cfg(not(test))]
                {
                    let _ = &outline;
                }

                ui.separator();
            }

            let content = ui.allocate_ui_with_layout(
                egui::vec2(content_width, body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(content_width);
                    self.render_note_content_area(
                        ui,
                        app,
                        ctx,
                        scroll_id_source,
                        text_id_source,
                        egui::vec2(content_width, body_height),
                    );
                },
            );
            #[cfg(test)]
            {
                let content_min = if has_outline {
                    self.last_outline_rect
                        .map_or(content.response.rect.min, |rect| {
                            egui::pos2(
                                rect.min.x + rect.width() + outline_separator_width,
                                rect.min.y,
                            )
                        })
                } else {
                    content.response.rect.min
                };
                self.last_content_rect = Some(egui::Rect::from_min_size(
                    content_min,
                    egui::vec2(content_width, body_height),
                ));
            }
            #[cfg(not(test))]
            {
                let _ = &content;
            }
        });
    }

    fn render_note_content_area(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        scroll_id_source: (&'static str, String),
        text_id_source: (&'static str, String),
        available_size: egui::Vec2,
    ) {
        let available_size = egui::vec2(available_size.x.max(0.0), available_size.y.max(0.0));
        #[cfg(test)]
        {
            self.last_ui_sections.content_visible = true;
        }
        match self.view_mode {
            NoteViewMode::Edit => {
                let resp = self.render_bounded_editor(
                    ui,
                    app,
                    ctx,
                    text_id_source.clone(),
                    available_size,
                );
                self.handle_editor_response(resp, ctx, app, true);
            }
            NoteViewMode::Preview => {
                // Keep the native edge detector synchronized even though Preview
                // never performs clipboard inspection.
                let _ = self.native_paste_detector.sample();
                egui::ScrollArea::vertical()
                    .id_salt(scroll_id_source)
                    .max_height(available_size.y)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(available_size.x);
                        ui.set_min_width(available_size.x);
                        ui.set_max_width(available_size.x);

                        let _ = self.markdown_analysis();
                        self.render_preview(ui, app, ctx);
                    });
            }
            NoteViewMode::Split => {
                self.render_split(ui, app, ctx, available_size);
            }
        }
    }

    fn intercept_clipboard_image_paste_with<Native, Lookup, Save, LookupError, SaveError>(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        editor_id: egui::Id,
        mut native_edge: Native,
        mut lookup: Lookup,
        mut save: Save,
    ) -> ClipboardImagePasteOutcome
    where
        Native: FnMut() -> bool,
        Lookup: FnMut() -> Result<Option<ClipboardRgbaData>, LookupError>,
        Save: FnMut(&RgbaImage) -> Result<String, SaveError>,
        LookupError: std::fmt::Display,
        SaveError: std::fmt::Display,
    {
        #[cfg(test)]
        {
            self.last_paste_interceptor_id = Some(editor_id);
        }
        let outcome = (|| {
            // Always sample to keep the detector synchronized across focus and
            // mode changes; clipboard access remains gated below.
            let has_windows_shortcut = native_edge();
            if has_windows_shortcut {
                tracing::trace!(?editor_id, "native Ctrl+V rising edge detected");
            }
            if !matches!(self.view_mode, NoteViewMode::Edit | NoteViewMode::Split) {
                return ClipboardImagePasteOutcome::NotHandled;
            }
            if !ctx.memory(|memory| memory.has_focus(editor_id)) {
                return ClipboardImagePasteOutcome::NotHandled;
            }
            tracing::trace!(
                ?editor_id,
                "exact note editor has focus; inspecting clipboard request"
            );
            let Some(request) =
                ctx.input(|input| detect_note_paste_request(&input.events, has_windows_shortcut))
            else {
                return ClipboardImagePasteOutcome::NotHandled;
            };
            tracing::trace!(
                ?editor_id,
                has_egui_paste = request.has_egui_paste,
                has_egui_shortcut = request.has_egui_shortcut,
                has_windows_shortcut = request.has_windows_shortcut,
                "note image paste request detected"
            );
            #[cfg(test)]
            {
                self.last_paste_request = Some(request);
            }

            let data = match lookup() {
                Ok(Some(data)) => data,
                Ok(None) => {
                    tracing::trace!(
                        ?editor_id,
                        "clipboard contains no image; leaving paste request for TextEdit"
                    );
                    return ClipboardImagePasteOutcome::NotHandled;
                }
                Err(error) => {
                    tracing::warn!(stage = "clipboard_image_read_decode", error = %error, "note image paste failed");
                    app.report_error("clipboard image paste", error);
                    return ClipboardImagePasteOutcome::NotHandled;
                }
            };

            tracing::debug!(
                width = data.width,
                height = data.height,
                "clipboard image received"
            );

            let image = match rgba_image_from_clipboard_data(data.width, data.height, data.bytes) {
                Ok(image) => image,
                Err(error) => {
                    ctx.input_mut(|input| consume_note_paste_request(&mut input.events, request));
                    tracing::warn!(stage = "rgba_validation", error = %error, "note image paste failed");
                    app.report_error(
                        "clipboard image paste",
                        format!("invalid clipboard RGBA image data: {error}"),
                    );
                    return ClipboardImagePasteOutcome::Handled;
                }
            };
            let filename = match save(&image) {
                Ok(filename) => filename,
                Err(error) => {
                    ctx.input_mut(|input| consume_note_paste_request(&mut input.events, request));
                    tracing::warn!(stage = "png_asset_saving", error = %error, "note image paste failed");
                    app.report_error(
                        "clipboard image paste",
                        format!("could not persist clipboard image asset or PNG: {error}"),
                    );
                    return ClipboardImagePasteOutcome::Handled;
                }
            };
            tracing::debug!(asset_path = %format!("assets/{filename}"), "note image asset saved");
            let markdown = note_image_markdown(&filename);
            ctx.input_mut(|input| consume_note_paste_request(&mut input.events, request));
            let insertion = self.insert_text_at_cursor_or_selection(ctx, editor_id, &markdown);
            tracing::trace!(
                ?editor_id,
                insertion_kind = ?insertion.kind,
                resulting_char_index = insertion.resulting_char_index,
                "note image markdown inserted"
            );
            ClipboardImagePasteOutcome::Handled
        })();
        #[cfg(test)]
        {
            self.last_paste_outcome = Some(outcome);
        }
        outcome
    }

    fn handle_outline_row_click(&mut self, row: &NoteOutlineRow, ctx: &egui::Context) {
        self.selected_outline_heading = Some(row.normalized_anchor.clone());
        self.pending_scroll_target = Some(row.normalized_anchor.clone());

        match self.view_mode {
            NoteViewMode::Edit => {
                self.move_editor_cursor_to(row.char_index, ctx);
            }
            NoteViewMode::Preview => {}
            NoteViewMode::Split => {
                let editor_focused = self
                    .last_textedit_id
                    .map(|id| ctx.memory(|m| m.has_focus(id)))
                    .unwrap_or(false);
                if editor_focused {
                    self.move_editor_cursor_to(row.char_index, ctx);
                }
            }
        }
    }

    fn move_editor_cursor_to(&mut self, char_index: usize, ctx: &egui::Context) {
        if let Some(id) = self.last_textedit_id {
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(char_index),
                )));
            state.store(ctx, id);
            self.pending_cursor_position = None;
        } else {
            self.pending_cursor_position = Some(char_index);
        }
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
        let total_pages = rows.len().div_ceil(BACKLINK_PAGE_SIZE);
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
                            let todos = load_todos_or_last_good(TODO_FILE);
                            if let Some((todo_idx, _)) =
                                todos.iter().enumerate().find(|(_, t)| &t.id == todo_id)
                            {
                                app.todo_view_dialog.open_edit(todo_idx);
                            } else {
                                app.todo_view_dialog.open();
                            }
                        }
                    }
                    if resp.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && let Some(slug) = &row.note_slug
                    {
                        app.open_note_panel(slug, None);
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
                            let todos = load_todos_or_last_good(TODO_FILE);
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

    fn render_bounded_editor(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        text_id_source: impl std::hash::Hash + std::fmt::Debug,
        available_size: egui::Vec2,
    ) -> egui::Response {
        let native_edge = self.native_paste_detector.sample();
        self.render_bounded_editor_with_paste(
            ui,
            app,
            ctx,
            text_id_source,
            available_size,
            move || native_edge,
            crate::gui::note_clipboard::read_clipboard_image,
            save_note_image_asset,
        )
    }

    fn render_bounded_editor_with_paste<Native, Lookup, Save, LookupError, SaveError>(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        text_id_source: impl std::hash::Hash + std::fmt::Debug,
        available_size: egui::Vec2,
        mut native_edge: Native,
        mut lookup: Lookup,
        mut save: Save,
    ) -> egui::Response
    where
        Native: FnMut() -> bool,
        Lookup: FnMut() -> Result<Option<ClipboardRgbaData>, LookupError>,
        Save: FnMut(&RgbaImage) -> Result<String, SaveError>,
        LookupError: std::fmt::Display,
        SaveError: std::fmt::Display,
    {
        #[cfg(test)]
        {
            self.last_paste_interceptor_id = None;
            self.last_paste_outcome = None;
            self.last_paste_request = None;
        }
        let available_size = egui::vec2(available_size.x.max(0.0), available_size.y.max(0.0));

        egui::ScrollArea::vertical()
            .max_width(available_size.x)
            .max_height(available_size.y)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(available_size.x);
                ui.set_min_width(available_size.x);
                ui.set_max_width(available_size.x);
                let editor_id = ui.make_persistent_id(text_id_source);
                if self.focus_textedit_next_frame && ui.is_enabled() {
                    // Focus before processing this frame's keyboard/clipboard events.
                    ctx.memory_mut(|memory| memory.request_focus(editor_id));
                }
                self.intercept_clipboard_image_paste_with(
                    ctx,
                    app,
                    editor_id,
                    &mut native_edge,
                    &mut lookup,
                    &mut save,
                );
                self.render_editor(ui, app, editor_id, Some(available_size))
            })
            .inner
    }

    fn render_editor(
        &mut self,
        ui: &mut egui::Ui,
        app: &LauncherApp,
        editor_id: egui::Id,
        desired_size: Option<egui::Vec2>,
    ) -> egui::Response {
        let desired_width = desired_size.map_or(f32::INFINITY, |size| size.x);
        let text_edit = egui::TextEdit::multiline(&mut self.note.content)
            .id(editor_id)
            .desired_width(desired_width)
            .font(FontId::monospace(app.note_font_size))
            .lock_focus(true);
        let response = if let Some(size) = desired_size {
            ui.add_sized(size, text_edit)
        } else {
            ui.add(text_edit.desired_rows(10))
        };
        debug_assert_eq!(response.id, editor_id);
        #[cfg(test)]
        {
            self.last_editor_rect = Some(response.rect);
        }
        response
    }

    fn render_preview(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, ctx: &egui::Context) {
        let preview_width = ui.available_width().max(0.0);
        ui.set_min_width(preview_width);
        ui.set_max_width(preview_width);
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
        if let Some(target) = self.pending_scroll_target.clone()
            && let Some(heading) = self
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
                let max_preview_width = ui.available_width().max(1.0);
                if display.x > max_preview_width {
                    display *= max_preview_width / display.x;
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
                            i.smooth_scroll_delta.y
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
                        .add(egui::DragValue::new(&mut w).range(20.0..=4096.0))
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
                        ui.close();
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
        let stroke = egui::Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color);
        egui::Frame::NONE
            .fill(fill)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
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
            let collapsed = self.collapsed_sections.contains(&self.section_key(section));
            let resp = self.render_collapsible_heading_row(ui, app, section, collapsed);
            if self.pending_scroll_target.as_deref()
                == Some(section.heading.normalized_anchor.as_str())
            {
                ui.scroll_to_rect(resp.rect, Some(egui::Align::Center));
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

    fn render_collapsible_heading_row(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        section: &MarkdownSection,
        collapsed: bool,
    ) -> egui::Response {
        let key = self.section_key(section);
        let resp = ui
            .horizontal_top(|ui| {
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

                let size = match section.heading.level {
                    1 => app.note_font_size * 1.45,
                    2 => app.note_font_size * 1.30,
                    3 => app.note_font_size * 1.15,
                    _ => app.note_font_size,
                };
                ui.label(
                    egui::RichText::new(heading_display_text(&section.heading))
                        .size(size)
                        .strong(),
                );
            })
            .response;
        #[cfg(test)]
        {
            self.last_preview_heading_rects.push(resp.rect);
        }
        resp
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

    fn render_split(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        ctx: &egui::Context,
        available_size: egui::Vec2,
    ) {
        let slug = self.note.slug.clone();
        let editor_id_source = ("note_split_text", slug.clone());
        let preview_scroll_id = ("note_split_preview_scroll", slug);
        let total_width = available_size.x.max(0.0);
        let total_height = available_size.y.max(0.0);
        let widths = split_pane_widths(
            total_width,
            self.split_ratio,
            6.0 + ui.spacing().item_spacing.x * 2.0,
        );
        self.split_ratio = self.split_ratio.clamp(0.2, 0.8);
        let editor_width = widths.editor;
        let preview_width = widths.preview;

        ui.horizontal(|ui| {
            let editor_pane = ui.allocate_ui_with_layout(
                egui::vec2(editor_width, total_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(editor_width);
                    let editor_size = egui::vec2(editor_width, total_height);
                    self.render_bounded_editor(ui, app, ctx, editor_id_source, editor_size)
                },
            );
            #[cfg(test)]
            {
                self.last_split_editor_rect = Some(egui::Rect::from_min_size(
                    editor_pane.response.rect.min,
                    egui::vec2(editor_width, total_height),
                ));
            }
            let editor_resp = editor_pane.inner;
            self.handle_editor_response(editor_resp, ctx, app, false);

            ui.separator();

            let preview_pane = ui.allocate_ui_with_layout(
                egui::vec2(preview_width, total_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(preview_width);
                    egui::ScrollArea::vertical()
                        .id_salt(preview_scroll_id)
                        .max_width(preview_width)
                        .max_height(total_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(preview_width);
                            ui.set_min_width(preview_width);
                            ui.set_max_width(preview_width);

                            let _ = self.markdown_analysis();
                            self.render_preview(ui, app, ctx);
                        });
                },
            );
            #[cfg(not(test))]
            {
                let _ = &preview_pane;
            }
            #[cfg(test)]
            {
                let preview_min = self
                    .last_split_editor_rect
                    .map_or(preview_pane.response.rect.min, |rect| {
                        egui::pos2(rect.min.x + rect.width() + widths.separator, rect.min.y)
                    });
                self.last_split_preview_rect = Some(egui::Rect::from_min_size(
                    preview_min,
                    egui::vec2(preview_width, total_height),
                ));
            }
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
        if let Some(char_index) = self.pending_cursor_position {
            let mut pending_state =
                egui::widgets::text_edit::TextEditState::load(ctx, resp.id).unwrap_or_default();
            pending_state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(char_index),
                )));
            pending_state.store(ctx, resp.id);
            self.pending_cursor_position = None;
        }
        if request_initial_focus && first_edit_frame {
            self.focus_textedit_next_frame = true;
        }
        // egui lays out a new Window invisibly with disabled widgets first.
        // Keep the focus request until the editor can actually receive input.
        if self.focus_textedit_next_frame && resp.enabled() {
            resp.request_focus();
            self.focus_textedit_next_frame = false;
        }
        let mut state =
            egui::widgets::text_edit::TextEditState::load(ctx, resp.id).unwrap_or_default();
        if !resp.secondary_clicked() {
            self.pending_selection = selected_char_range(&state)
                .and_then(|range| normalize_char_range(range, self.note.content.chars().count()));
        } else {
            match self
                .pending_selection
                .and_then(|range| normalize_char_range(range, self.note.content.chars().count()))
            {
                Some(cached) => {
                    let caret = state.cursor.char_range().map(|range| range.primary.index.0);
                    if caret.is_some_and(|caret| caret_is_within_range(caret, cached)) {
                        restore_char_range(&mut state, cached);
                        state.store(ctx, resp.id);
                        self.pending_selection = Some(cached);
                    } else {
                        self.pending_selection = None;
                    }
                }
                None => self.pending_selection = None,
            }
        }
        resp.context_menu(|ui| {
            let ctx2 = ui.ctx().clone();
            self.build_textedit_menu(ui, &ctx2, resp.id, app);
        });
        if resp.has_focus() && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Period)) {
            // The right-click restoration path may have consumed `state` while
            // storing it. Reloading also snapshots the authoritative live state
            // immediately before the keyboard menu is opened.
            let keyboard_state =
                egui::widgets::text_edit::TextEditState::load(ctx, resp.id).unwrap_or_default();
            if let Some(range) = selected_char_range(&keyboard_state)
                .and_then(|range| normalize_char_range(range, self.note.content.chars().count()))
            {
                self.pending_selection = Some(range);
            }
            let pos = resp.rect.left_top();
            let _ = egui::Tooltip::always_open(
                ctx.clone(),
                resp.layer_id,
                egui::Id::new("note_ctx_menu"),
                egui::PopupAnchor::Position(pos),
            )
            .show(|ui| {
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
            .map(|r| r.primary.index.0)
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
            ctx.output_mut(|output| {
                output.commands.push(egui::OutputCommand::CopyText(
                    self.note.content[start_byte..end_byte].to_string(),
                ));
            });
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
        self.heavy_refresh_retry = None;
        if let Some(first) = self.note.content.lines().next()
            && let Some(t) = first.strip_prefix("# ")
        {
            self.note.title = t.to_string();
        }
        match save_note(&mut self.note, app.note_always_overwrite) {
            Ok(true) => {
                self.refresh_fast_derived();
                self.refresh_heavy_derived(true, app.note_settings.backlinks_enabled);
                self.mark_current_content_saved();
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

                style: Default::default(),
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
        let fragment_width = ui.available_width().max(0.0);
        ui.scope(|ui| {
            ui.set_min_width(fragment_width);
            ui.set_max_width(fragment_width);
            ui.style_mut().override_font_id = Some(FontId::proportional(app.note_font_size));
            let processed = preprocess_preview_markdown(
                fragment,
                &self.note.slug,
                &self.derived.todo_label_map,
            );
            let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                egui::ScrollArea::horizontal()
                    .max_width(fragment_width)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_min_width(fragment_width);
                        ui.set_max_width(fragment_width);
                        ui.push_id(("note_seg", cache_id.to_string()), |ui| {
                            CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &processed);
                        });
                    });
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
        let fallback_width = ui.available_width().max(0.0);
        ui.set_min_width(fallback_width);
        ui.set_max_width(fallback_width);
        let response = egui::ScrollArea::horizontal()
            .max_width(fallback_width)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add_sized(
                    egui::vec2(fallback_width, 0.0),
                    egui::TextEdit::multiline(&mut fallback)
                        .desired_width(fallback_width)
                        .font(FontId::monospace(app.note_font_size))
                        .interactive(false),
                )
            })
            .inner;
        #[cfg(not(test))]
        {
            let _ = &response;
        }
        #[cfg(test)]
        {
            self.last_preview_fallback_rect = Some(response.rect);
        }
    }

    fn render_inline_note_text(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, text: &str) {
        let font = FontId::proportional(app.note_font_size);
        let mut cursor = 0;

        while cursor < text.len() {
            let remaining = &text[cursor..];
            let next_wiki = remaining.find("[[");
            let next_markdown = remaining.find('[').filter(|idx| {
                let byte_idx = cursor + idx;
                byte_idx == 0 || text.as_bytes().get(byte_idx - 1) != Some(&b'!')
            });
            let next_todo = remaining.find("@todo:");

            let Some(next) = [next_wiki, next_markdown, next_todo]
                .into_iter()
                .flatten()
                .min()
            else {
                ui.label(RichText::new(remaining).font(font.clone()));
                break;
            };

            if next > 0 {
                ui.label(RichText::new(&remaining[..next]).font(font.clone()));
                cursor += next;
                continue;
            }

            if let Some(rest) = text[cursor..].strip_prefix("[[")
                && let Some(end) = rest.find("]]")
            {
                let link_text = &rest[..end];
                let (target, label) = link_text
                    .split_once('|')
                    .map(|(target, alias)| (target.trim(), alias.trim()))
                    .unwrap_or_else(|| (link_text.trim(), link_text.trim()));
                let resp = ui.link(RichText::new(label).font(font.clone()));
                if resp.clicked() {
                    match resolve_note_query(target) {
                            NoteTarget::Resolved(slug) => app.open_note_panel(&slug, None),
                            NoteTarget::Ambiguous(slugs) => app.report_error("ui operation", format!(
                                "Ambiguous link [[{target}]]; use [[slug:<slug>]] or [[path:<file.md>]]. Candidates: {}",
                                slugs.join(", ")
                            )),
                            NoteTarget::Broken => {
                                app.report_error(
                                    "ui operation",
                                    format!("Broken note link: [[{target}]]"),
                                );
                                app.open_note_panel(&slugify(target), None);
                            }
                        }
                }
                cursor += 2 + end + 2;
                continue;
            }

            if text[cursor..].starts_with('[')
                && let Some(label_end) = text[cursor + 1..].find("](")
            {
                let label_end = cursor + 1 + label_end;
                let url_start = label_end + 2;
                if let Some(url_end_offset) = text[url_start..].find(')') {
                    let url_end = url_start + url_end_offset;
                    let label = &text[cursor + 1..label_end];
                    let url = &text[url_start..url_end];
                    if url.starts_with("http://") || url.starts_with("https://") {
                        ui.scope(|ui| {
                            ui.style_mut().override_font_id = Some(font.clone());
                            ui.hyperlink_to(label, url);
                        });
                        cursor = url_end + 1;
                        continue;
                    }
                }
            }

            if let Some(rest) = text[cursor..].strip_prefix("@todo:") {
                let id_len = rest
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
                    .count();
                if id_len > 0 {
                    let todo_id = &rest[..id_len];
                    let resp =
                        ui.link(RichText::new(format!("@todo:{todo_id}")).font(font.clone()));
                    if resp.clicked() {
                        open_todo_reference(app, todo_id);
                    }
                    cursor += "@todo:".len() + id_len;
                    continue;
                }
            }

            let ch = text[cursor..].chars().next().unwrap();
            ui.label(RichText::new(ch.to_string()).font(font.clone()));
            cursor += ch.len_utf8();
        }
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
            ui.add_space(4.0);
            self.render_inline_note_text(ui, app, task_display_text(task_item));
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
        ui.menu_button("Markdown", |ui| {
            if ui.button("Add Checkbox").clicked() {
                let mut state =
                    egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                let idx = state
                    .cursor
                    .char_range()
                    .map(|r| r.primary.index.0)
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
                ui.close();
            }
            if ui.button("Insert Timestamp").clicked() {
                let timestamp = format_note_editor_timestamp_now();
                self.insert_text_at_cursor_or_selection(ctx, id, &timestamp);
                ui.close();
            }
            if ui.button("Insert Link...").clicked() {
                if let Some((start, end)) = self.resolve_selection(ctx, id) {
                    let (start, end) = char_range_to_byte_range(&self.note.content, start, end);
                    self.link_text = self.note.content[start..end].to_string();
                } else {
                    self.link_text.clear();
                }
                self.link_dialog_open = true;
                ui.close();
            }
            if ui.button("Convert plain links").clicked() {
                if !self.start_plain_link_conversion() {
                    Self::show_no_plain_links_toast(app);
                }
                ui.close();
            }
            if ui.button("Bold Selection").clicked() {
                self.wrap_selection(ctx, id, "**", "**");
                ui.close();
            }
            if ui.button("Italic Selection").clicked() {
                self.wrap_selection(ctx, id, "*", "*");
                ui.close();
            }
            ui.separator();
            if ui.button("Insert NOTE callout").clicked() {
                self.insert_callout(ctx, id, "note");
                ui.close();
            }
            if ui.button("Insert WARNING callout").clicked() {
                self.insert_callout(ctx, id, "warning");
                ui.close();
            }
            if ui.button("Insert TODO callout").clicked() {
                self.insert_callout(ctx, id, "todo");
                ui.close();
            }
            if ui.button("Wrap selection in callout").clicked() {
                self.wrap_selection_in_callout(ctx, id, "note");
                ui.close();
            }
        });

        ui.menu_button("Insert link", |ui| {
            ui.set_min_width(200.0);
            ui.horizontal(|ui| {
                ui.label("Insert link:");
                if ui.button("Link new").clicked() {
                    self.link_new_dialog_open = true;
                    self.link_new_name.clear();
                    ui.close();
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
                            ui.close();
                        }
                    }
                    if results.len() == NOTE_LINK_CONTEXT_MENU_RESULT_LIMIT {
                        ui.small("Showing first 50 matches. Type to narrow results.");
                    }
                });
        });

        ui.menu_button("Insert image", |ui| {
            ui.set_min_width(200.0);
            if ui.button("Upload...").clicked()
                && let Some(path) = FileDialog::new()
                    .add_filter("Image", &["png", "jpg", "jpeg", "gif", "bmp", "webp"])
                    .pick_file()
                && let Some(fname) = path.file_name().and_then(|s| s.to_str())
            {
                let dest = assets_dir().join(fname);
                if let Err(e) = std::fs::copy(&path, &dest) {
                    app.report_error("ui operation", format!("Failed to copy image: {e}"));
                } else {
                    let insert = format!("![{0}](assets/{0})", fname);
                    self.insert_text_at_cursor_or_selection(ctx, id, &insert);
                    self.image_search.clear();
                    ui.close();
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
                                self.insert_text_at_cursor_or_selection(ctx, id, &insert);
                                self.image_search.clear();
                                ui.close();
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
                            self.insert_text_at_cursor_or_selection(ctx, id, &insert);
                            self.image_search.clear();
                            ui.close();
                        }
                    }
                });
        });

        ui.menu_button("Link todo", |ui| {
            ui.label("Select existing todo");
            for todo in load_todos_or_last_good(TODO_FILE).into_iter().take(12) {
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
                        .map(|r| r.primary.index.0)
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
                    ui.close();
                }
            }
        });

        ui.menu_button("Insert tag", |ui| {
            if let Some(tag) = insert_tag_menu(ui, &mut self.tag_search) {
                self.insert_text_at_cursor_or_selection(ctx, id, &tag);
            }
        });
    }

    fn resolve_selection(&self, ctx: &egui::Context, id: egui::Id) -> Option<(usize, usize)> {
        let char_count = self.note.content.chars().count();
        let state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        selected_char_range(&state)
            .and_then(|range| normalize_char_range(range, char_count))
            .or_else(|| {
                self.pending_selection
                    .and_then(|range| normalize_char_range(range, char_count))
            })
    }

    pub fn wrap_selection(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        start_marker: &str,
        end_marker: &str,
    ) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        if let Some((start, end)) = self.resolve_selection(ctx, id) {
            let (start_byte, end_byte) = char_range_to_byte_range(&self.note.content, start, end);
            self.note.content.insert_str(end_byte, end_marker);
            self.note.content.insert_str(start_byte, start_marker);
            self.mark_content_changed(ctx.input(|i| i.time));
            let new_start = start + start_marker.chars().count();
            let new_end = end + start_marker.chars().count();
            restore_char_range(&mut state, (new_start, new_end));
            state.store(ctx, id);
            self.pending_selection = Some((new_start, new_end));
        }
    }

    fn insert_callout(&mut self, ctx: &egui::Context, id: egui::Id, kind: &str) {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let idx = state
            .cursor
            .char_range()
            .map(|r| r.primary.index.0)
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
        if let Some((start, end)) = self.resolve_selection(ctx, id) {
            let (updated, selected_range) =
                wrap_char_range_in_callout(&self.note.content, start, end, kind);
            self.note.content = updated;
            self.mark_content_changed(ctx.input(|i| i.time));
            let selected = (selected_range.start, selected_range.end);
            restore_char_range(&mut state, selected);
            state.store(ctx, id);
            self.pending_selection = Some(selected);
        }
    }

    pub fn insert_link(&mut self, ctx: &egui::Context, id: egui::Id) {
        let text = if self.link_text.is_empty() {
            if let Some((start, end)) = self.resolve_selection(ctx, id) {
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
    ) -> TextInsertionOutcome {
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        if let Some((start, end)) = self.resolve_selection(ctx, id) {
            let (start_byte, end_byte) = char_range_to_byte_range(&self.note.content, start, end);
            self.note
                .content
                .replace_range(start_byte..end_byte, insert);
            let cursor = start + insert.chars().count();
            collapse_cursor(&mut state, cursor);
            state.store(ctx, id);
            self.pending_selection = None;
            self.mark_content_changed(ctx.input(|i| i.time));
            return TextInsertionOutcome {
                kind: TextInsertionKind::ReplacedSelection,
                resulting_char_index: cursor,
            };
        }

        let idx = state
            .cursor
            .char_range()
            .map(|r| r.primary.index.0)
            .unwrap_or_else(|| self.note.content.chars().count())
            .min(self.note.content.chars().count());
        let idx_byte = char_to_byte_index(&self.note.content, idx);
        self.note.content.insert_str(idx_byte, insert);
        let cursor = idx + insert.chars().count();
        collapse_cursor(&mut state, cursor);
        state.store(ctx, id);
        self.pending_selection = None;
        self.mark_content_changed(ctx.input(|i| i.time));
        TextInsertionOutcome {
            kind: TextInsertionKind::InsertedAtCaret,
            resulting_char_index: cursor,
        }
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

fn insert_tag_menu(ui: &mut egui::Ui, search: &mut String) -> Option<String> {
    let mut inserted = None;
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
                    inserted = Some(insert);
                    search.clear();
                    ui.close();
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
    configure_background_command(&mut cmd);
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
    #[cfg(test)]
    let observer = NoteRefreshObserverHandle::current();
    let compact = content.replace('\n', " ");
    #[cfg(test)]
    if let Some(observer) = &observer {
        observer.record_materialized_string(NoteRefreshStringClass::Snippet, &compact);
    }
    if compact.is_empty() {
        let output = String::new();
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record_materialized_string(NoteRefreshStringClass::Snippet, &output);
        }
        return output;
    }
    let lower = compact.to_lowercase();
    #[cfg(test)]
    if let Some(observer) = &observer {
        observer.record_materialized_string(NoteRefreshStringClass::Snippet, &lower);
    }
    let needle_lower = needle.to_lowercase();
    #[cfg(test)]
    if let Some(observer) = &observer {
        observer.record_materialized_string(NoteRefreshStringClass::Snippet, &needle_lower);
    }
    let output = if let Some(pos) = lower.find(&needle_lower) {
        let start = pos.saturating_sub(WINDOW);
        let end = (pos + needle_lower.len() + WINDOW).min(compact.len());
        let mut out = compact[start..end].trim().to_string();
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record_materialized_string(NoteRefreshStringClass::Snippet, &out);
        }
        if start > 0 {
            out = format!("…{out}");
            #[cfg(test)]
            if let Some(observer) = &observer {
                observer.record_materialized_string(NoteRefreshStringClass::Snippet, &out);
            }
        }
        if end < compact.len() {
            out.push('…');
        }
        out
    } else {
        let output = compact.chars().take(90).collect::<String>();
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record_materialized_string(NoteRefreshStringClass::Snippet, &output);
        }
        output
    };
    output
}

fn format_note_editor_timestamp<Tz>(timestamp: DateTime<Tz>) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    format!(
        "{}.{:03}",
        timestamp.format("%Y%m%d-%H%M%S"),
        timestamp.timestamp_subsec_millis()
    )
}

fn format_note_editor_timestamp_now() -> String {
    format_note_editor_timestamp(Local::now())
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

fn content_without_fenced_code(content: &str) -> String {
    #[cfg(test)]
    let observer = NoteRefreshObserverHandle::current();
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
    #[cfg(test)]
    if let Some(observer) = &observer {
        observer.record(|observation| {
            observation.fenced_content_calls = observation.fenced_content_calls.saturating_add(1);
            observation.fenced_input_bytes = observation
                .fenced_input_bytes
                .saturating_add(content.len() as u64);
            observation.fenced_output_bytes = observation
                .fenced_output_bytes
                .saturating_add(out.len() as u64);
        });
        observer.record_materialized_string(NoteRefreshStringClass::FencedContent, &out);
    }
    out
}

fn note_reference_needles(current: &Note) -> Vec<(String, String)> {
    #[cfg(test)]
    let observer = NoteRefreshObserverHandle::current();
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
    #[cfg(test)]
    if let Some(observer) = &observer {
        for (needle, reason) in &needles {
            observer.record_materialized_string(NoteRefreshStringClass::Needles, needle);
            observer.record_materialized_string(NoteRefreshStringClass::Needles, reason);
        }
    }
    if let Some(alias) = current.alias.as_deref().filter(|a| !a.trim().is_empty()) {
        let alias_needle = format!("[[{}]]", alias.trim());
        let alias_reason = "wiki alias".to_string();
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record_materialized_string(NoteRefreshStringClass::Needles, &alias_needle);
            observer.record_materialized_string(NoteRefreshStringClass::Needles, &alias_reason);
        }
        needles.push((alias_needle, alias_reason));
    }
    needles.sort();
    needles.dedup();
    needles
}

struct PreparedBacklinkRows {
    linked_todos: Vec<BacklinkRow>,
    related_notes: Vec<BacklinkRow>,
    mentions: Vec<BacklinkRow>,
}

fn backlink_rows_for_note(
    current_note: &Note,
    todos: &[crate::plugins::todo::TodoEntry],
    notes: &[Note],
) -> PreparedBacklinkRows {
    #[cfg(test)]
    let observer = NoteRefreshObserverHandle::current();
    #[cfg(test)]
    if let Some(observer) = &observer {
        observer.record(|observation| {
            observation.fused_traversal.calls = observation.fused_traversal.calls.saturating_add(1);
        });
    }
    #[cfg(test)]
    let _fused_observation_timer = NoteRefreshObservationTimer::start(
        observer.clone(),
        NoteRefreshDurationKind::FusedTraversal,
    );

    let mut prepared = PreparedBacklinkRows {
        linked_todos: Vec::new(),
        related_notes: Vec::new(),
        mentions: Vec::new(),
    };
    let current_slug = current_note.slug.as_str();
    let needles = note_reference_needles(current_note);
    let current_searchable = content_without_fenced_code(&current_note.content);

    for todo in todos {
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record(|observation| {
                observation.fused_traversal.todo_visits =
                    observation.fused_traversal.todo_visits.saturating_add(1);
            });
        }
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
            let row = BacklinkRow {
                title: todo.text.clone(),
                type_badge: "Todo".to_string(),
                updated: "n/a".to_string(),
                snippet: extract_snippet_around(&todo.text, &needle),
                reason,
                note_slug: None,
                todo_id: Some(todo.id.clone()),
            };
            // Todo relationships intentionally appear in both categories.
            prepared.linked_todos.push(row.clone());
            prepared.mentions.push(row);
        }
    }

    for note in notes {
        #[cfg(test)]
        if let Some(observer) = &observer {
            observer.record(|observation| {
                observation.fused_traversal.note_visits =
                    observation.fused_traversal.note_visits.saturating_add(1);
            });
        }
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
            let row = BacklinkRow {
                title: note.title.clone(),
                type_badge: "Note".to_string(),
                updated: format_note_updated(note),
                snippet: extract_snippet_around(&searchable, &needle),
                reason,
                note_slug: Some(note.slug.clone()),
                todo_id: None,
            };
            if is_mention {
                prepared.mentions.push(row);
            } else {
                prepared.related_notes.push(row);
            }
        }
    }
    prepared
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
    use chrono::{FixedOffset, Timelike};
    use eframe::egui;
    use std::{
        fs,
        sync::{Arc, Mutex, MutexGuard, atomic::AtomicBool},
    };
    use tempfile::{TempDir, tempdir};

    static NOTES_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn derived_workload_signature(panel: &NotePanel) -> u64 {
        let mut signature = crate::performance::workloads::StableSignature::new(
            0,
            "note-derived-output",
            panel.derived.backlink_rows_linked_todos.len()
                + panel.derived.backlink_rows_related_notes.len()
                + panel.derived.backlink_rows_mentions.len(),
        );
        for value in &panel.derived.tags {
            signature.bytes(value.as_bytes());
        }
        for value in &panel.derived.wiki_links {
            signature.bytes(value.as_bytes());
        }
        for (url, label) in &panel.derived.external_links {
            signature.bytes(url.as_bytes());
            signature.bytes(label.as_bytes());
        }
        for rows in [
            &panel.derived.backlink_rows_linked_todos,
            &panel.derived.backlink_rows_related_notes,
            &panel.derived.backlink_rows_mentions,
        ] {
            for row in rows {
                signature.bytes(row.title.as_bytes());
                signature.bytes(row.type_badge.as_bytes());
                signature.bytes(row.updated.as_bytes());
                signature.bytes(row.snippet.as_bytes());
                signature.bytes(row.reason.as_bytes());
                signature.bytes(row.note_slug.as_deref().unwrap_or_default().as_bytes());
                signature.bytes(row.todo_id.as_deref().unwrap_or_default().as_bytes());
            }
        }
        let mut todos = panel.derived.todo_label_map.iter().collect::<Vec<_>>();
        todos.sort_unstable_by(|left, right| left.0.cmp(right.0));
        for (id, label) in todos {
            signature.bytes(id.as_bytes());
            signature.bytes(label.as_bytes());
        }
        signature.finish()
    }

    // Frozen test-local copy of the eager Track C relationship oracle. Keep
    // this independent from `backlink_rows_for_note` so a later fused pass
    // cannot silently become its own expected result.
    fn track_c_eager_fenced_content(content: &str) -> String {
        let mut output = String::new();
        let mut in_fence = false;
        for line in content.lines() {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                output.push('\n');
            } else if !in_fence {
                output.push_str(line);
                output.push('\n');
            } else {
                output.push('\n');
            }
        }
        output
    }

    fn track_c_eager_needles(current: &Note) -> Vec<(String, String)> {
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
        if let Some(alias) = current
            .alias
            .as_deref()
            .filter(|alias| !alias.trim().is_empty())
        {
            needles.push((format!("[[{}]]", alias.trim()), "wiki alias".to_string()));
        }
        needles.sort();
        needles.dedup();
        needles
    }

    fn track_c_eager_snippet(content: &str, needle: &str) -> String {
        const WINDOW: usize = 44;
        let compact = content.replace('\n', " ");
        if compact.is_empty() {
            return String::new();
        }
        let lower = compact.to_lowercase();
        let needle_lower = needle.to_lowercase();
        if let Some(position) = lower.find(&needle_lower) {
            let start = position.saturating_sub(WINDOW);
            let end = (position + needle_lower.len() + WINDOW).min(compact.len());
            let mut output = compact[start..end].trim().to_string();
            if start > 0 {
                output = format!("…{output}");
            }
            if end < compact.len() {
                output.push('…');
            }
            output
        } else {
            compact.chars().take(90).collect()
        }
    }

    fn track_c_eager_note_rows(
        current: &Note,
        tab: BacklinkTab,
        todos: &[crate::plugins::todo::TodoEntry],
        notes: &[Note],
    ) -> Vec<BacklinkRow> {
        use crate::common::entity_ref::EntityKind;

        let mut rows = Vec::new();
        let needles = track_c_eager_needles(current);
        let current_searchable = track_c_eager_fenced_content(&current.content);
        for todo in todos {
            let matched = todo
                .entity_refs
                .iter()
                .any(|reference| reference.kind == EntityKind::Note && reference.id == current.slug)
                .then(|| {
                    (
                        format!("@note:{}", current.slug),
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
                    || current.entity_refs.iter().any(|reference| {
                        reference.kind == EntityKind::Todo && reference.id == todo.id
                    }));
            let matched = matched.or_else(|| {
                current_note_mentions_todo.then(|| {
                    (
                        format!("@todo:{}", todo.id),
                        "note mentions todo".to_string(),
                    )
                })
            });
            if let Some((needle, reason)) = matched
                && matches!(tab, BacklinkTab::LinkedTodos | BacklinkTab::Mentions)
            {
                rows.push(BacklinkRow {
                    title: todo.text.clone(),
                    type_badge: "Todo".into(),
                    updated: "n/a".into(),
                    snippet: track_c_eager_snippet(&todo.text, &needle),
                    reason,
                    note_slug: None,
                    todo_id: Some(todo.id.clone()),
                });
            }
        }

        for note in notes {
            if note.slug == current.slug {
                continue;
            }
            let searchable = track_c_eager_fenced_content(&note.content);
            let matched = note
                .links
                .iter()
                .any(|link| link == &current.slug)
                .then(|| (format!("[[{}", current.slug), "wiki link".into()))
                .or_else(|| {
                    note.entity_refs
                        .iter()
                        .any(|reference| {
                            reference.kind == EntityKind::Note && reference.id == current.slug
                        })
                        .then(|| {
                            (
                                format!("@note:{}", current.slug),
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
                    let updated = std::fs::metadata(&note.path)
                        .ok()
                        .and_then(|metadata| metadata.modified().ok())
                        .map(|modified| {
                            chrono::DateTime::<chrono::Local>::from(modified)
                                .format("%Y-%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    rows.push(BacklinkRow {
                        title: note.title.clone(),
                        type_badge: "Note".into(),
                        updated,
                        snippet: track_c_eager_snippet(&searchable, &needle),
                        reason,
                        note_slug: Some(note.slug.clone()),
                        todo_id: None,
                    });
                }
            }
        }
        rows
    }

    // C1-C experiment only: retain source-position postings and exact fenced
    // prose, then materialize rows against the current draft on every replay.
    // This intentionally remains a full scan for sources without a canonical
    // or typed posting; it is not a reverse-text index.
    struct TrackCPreparedNoteSource {
        slug: String,
        title: String,
        path: PathBuf,
        fenced_searchable: String,
    }

    struct TrackCPreparedNoteProjection {
        sources: Vec<TrackCPreparedNoteSource>,
        canonical_links: HashMap<String, Vec<usize>>,
        typed_note_refs: HashMap<String, Vec<usize>>,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct TrackCPreparedProjectionFootprint {
        text_logical_bytes: usize,
        text_capacity_bytes: usize,
        metadata_logical_bytes: usize,
        metadata_string_capacity_bytes: usize,
        source_slot_bytes: usize,
        posting_key_logical_bytes: usize,
        posting_key_capacity_bytes: usize,
        posting_positions: usize,
        posting_position_capacity: usize,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct TrackCPreparedProjectionWork {
        todo_visits: usize,
        note_visits: usize,
        canonical_posting_entries: usize,
        typed_posting_entries: usize,
        fallback_visits: usize,
        fallback_input_bytes: usize,
        snippet_calls: usize,
        metadata_reads: usize,
        linked_todo_rows: usize,
        related_note_rows: usize,
        mention_rows: usize,
    }

    struct TrackCPreparedProjectionReplay {
        rows: [Vec<BacklinkRow>; 3],
        todo_label_map: HashMap<String, String>,
        work: TrackCPreparedProjectionWork,
    }

    impl TrackCPreparedNoteProjection {
        fn prepare(notes: &[Note]) -> Self {
            use crate::common::entity_ref::EntityKind;

            let mut projection = Self {
                sources: Vec::with_capacity(notes.len()),
                canonical_links: HashMap::new(),
                typed_note_refs: HashMap::new(),
            };
            for (position, note) in notes.iter().enumerate() {
                projection.sources.push(TrackCPreparedNoteSource {
                    slug: note.slug.clone(),
                    title: note.title.clone(),
                    path: note.path.clone(),
                    fenced_searchable: content_without_fenced_code(&note.content),
                });
                for target in &note.links {
                    projection
                        .canonical_links
                        .entry(target.clone())
                        .or_default()
                        .push(position);
                }
                for reference in &note.entity_refs {
                    if reference.kind == EntityKind::Note {
                        projection
                            .typed_note_refs
                            .entry(reference.id.clone())
                            .or_default()
                            .push(position);
                    }
                }
            }
            projection
        }

        fn footprint(&self) -> TrackCPreparedProjectionFootprint {
            let mut footprint = TrackCPreparedProjectionFootprint {
                source_slot_bytes: self
                    .sources
                    .capacity()
                    .saturating_mul(std::mem::size_of::<TrackCPreparedNoteSource>()),
                ..TrackCPreparedProjectionFootprint::default()
            };
            for source in &self.sources {
                footprint.text_logical_bytes = footprint
                    .text_logical_bytes
                    .saturating_add(source.fenced_searchable.len());
                footprint.text_capacity_bytes = footprint
                    .text_capacity_bytes
                    .saturating_add(source.fenced_searchable.capacity());
                // Paths are lossy UTF-8 display bytes. PathBuf capacity and
                // HashMap bucket/control storage are intentionally excluded.
                let path_bytes = source.path.to_string_lossy().len();
                footprint.metadata_logical_bytes = footprint
                    .metadata_logical_bytes
                    .saturating_add(source.slug.len())
                    .saturating_add(source.title.len())
                    .saturating_add(path_bytes);
                footprint.metadata_string_capacity_bytes = footprint
                    .metadata_string_capacity_bytes
                    .saturating_add(source.slug.capacity())
                    .saturating_add(source.title.capacity());
            }
            for (key, positions) in self
                .canonical_links
                .iter()
                .chain(self.typed_note_refs.iter())
            {
                footprint.posting_key_logical_bytes = footprint
                    .posting_key_logical_bytes
                    .saturating_add(key.len());
                footprint.posting_key_capacity_bytes = footprint
                    .posting_key_capacity_bytes
                    .saturating_add(key.capacity());
                footprint.posting_positions =
                    footprint.posting_positions.saturating_add(positions.len());
                footprint.posting_position_capacity = footprint
                    .posting_position_capacity
                    .saturating_add(positions.capacity());
            }
            footprint
        }

        fn replay(
            &self,
            current: &Note,
            todos: &[crate::plugins::todo::TodoEntry],
        ) -> TrackCPreparedProjectionReplay {
            use crate::common::entity_ref::EntityKind;

            let mut rows: [Vec<BacklinkRow>; 3] = std::array::from_fn(|_| Vec::new());
            let mut work = TrackCPreparedProjectionWork {
                todo_visits: todos.len(),
                note_visits: self.sources.len(),
                ..TrackCPreparedProjectionWork::default()
            };
            let needles = note_reference_needles(current);
            let current_searchable = content_without_fenced_code(&current.content);

            for todo in todos {
                let matched = todo
                    .entity_refs
                    .iter()
                    .any(|reference| {
                        reference.kind == EntityKind::Note && reference.id == current.slug
                    })
                    .then(|| {
                        (
                            format!("@note:{}", current.slug),
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
                        || current.entity_refs.iter().any(|reference| {
                            reference.kind == EntityKind::Todo && reference.id == todo.id
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
                    let row = BacklinkRow {
                        title: todo.text.clone(),
                        type_badge: "Todo".into(),
                        updated: "n/a".into(),
                        snippet: extract_snippet_around(&todo.text, &needle),
                        reason,
                        note_slug: None,
                        todo_id: Some(todo.id.clone()),
                    };
                    work.snippet_calls = work.snippet_calls.saturating_add(1);
                    rows[0].push(row.clone());
                    rows[2].push(row);
                }
            }

            let canonical_positions = self.canonical_links.get(&current.slug);
            let typed_positions = self.typed_note_refs.get(&current.slug);
            work.canonical_posting_entries = canonical_positions.map_or(0, Vec::len);
            work.typed_posting_entries = typed_positions.map_or(0, Vec::len);
            for (position, source) in self.sources.iter().enumerate() {
                if source.slug == current.slug {
                    continue;
                }
                let canonical_match = canonical_positions
                    .is_some_and(|positions| positions.binary_search(&position).is_ok());
                let typed_match = !canonical_match
                    && typed_positions
                        .is_some_and(|positions| positions.binary_search(&position).is_ok());
                let matched = if canonical_match {
                    Some((format!("[[{}", current.slug), "wiki link".to_string()))
                } else if typed_match {
                    Some((
                        format!("@note:{}", current.slug),
                        "entity reference".to_string(),
                    ))
                } else {
                    work.fallback_visits = work.fallback_visits.saturating_add(1);
                    work.fallback_input_bytes = work
                        .fallback_input_bytes
                        .saturating_add(source.fenced_searchable.len());
                    needles
                        .iter()
                        .find(|(needle, _)| source.fenced_searchable.contains(needle))
                        .cloned()
                };
                if let Some((needle, reason)) = matched {
                    let is_mention = reason.contains("mention") || reason.contains("entity");
                    let row = BacklinkRow {
                        title: source.title.clone(),
                        type_badge: "Note".into(),
                        updated: track_c_prepared_updated(&source.path),
                        snippet: extract_snippet_around(&source.fenced_searchable, &needle),
                        reason,
                        note_slug: Some(source.slug.clone()),
                        todo_id: None,
                    };
                    work.metadata_reads = work.metadata_reads.saturating_add(1);
                    work.snippet_calls = work.snippet_calls.saturating_add(1);
                    if is_mention {
                        rows[2].push(row);
                    } else {
                        rows[1].push(row);
                    }
                }
            }

            work.linked_todo_rows = rows[0].len();
            work.related_note_rows = rows[1].len();
            work.mention_rows = rows[2].len();
            let todo_label_map = todos
                .iter()
                .filter(|todo| !todo.id.is_empty())
                .map(|todo| (todo.id.clone(), todo.text.clone()))
                .collect();
            TrackCPreparedProjectionReplay {
                rows,
                todo_label_map,
                work,
            }
        }
    }

    fn track_c_prepared_updated(path: &Path) -> String {
        std::fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|modified| {
                chrono::DateTime::<chrono::Local>::from(modified)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn track_c_prepared_rows_signature(
        rows: &[Vec<BacklinkRow>; 3],
    ) -> Vec<Vec<TrackCBacklinkRowIdentity>> {
        rows.iter()
            .map(|category| backlink_rows_signature(category))
            .collect::<Vec<_>>()
    }

    struct TrackC1CObserverPause {
        previous: Option<NoteRefreshObserverHandle>,
    }

    impl TrackC1CObserverPause {
        fn new() -> Self {
            let previous = NOTE_REFRESH_OBSERVER
                .with(|observer| std::mem::replace(&mut *observer.borrow_mut(), None));
            Self { previous }
        }
    }

    impl Drop for TrackC1CObserverPause {
        fn drop(&mut self) {
            NOTE_REFRESH_OBSERVER.with(|observer| {
                *observer.borrow_mut() = self.previous.take();
            });
        }
    }

    struct TrackC1CProjectionBundle {
        cache: crate::plugins::note::NoteCache,
        projection: TrackCPreparedNoteProjection,
    }

    impl TrackC1CProjectionBundle {
        fn new(notes: Vec<Note>) -> Self {
            let cache = crate::plugins::note::build_note_cache_candidate_for_test(notes);
            let projection = TrackCPreparedNoteProjection::prepare(&cache.notes);
            Self { cache, projection }
        }
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct TrackC1CSample {
        current_owner_ns: u64,
        projection_owner_ns: u64,
        projection_prepare_ns: u64,
        current_repeat5_ns: u64,
        projection_repeat5_ns: u64,
        equal_current_publish_ns: u64,
        equal_projection_publish_ns: u64,
        changed_current_publish_ns: u64,
        changed_projection_publish_ns: u64,
        unchanged_gate_ns: u64,
        canonical_posting_entries: u64,
        typed_posting_entries: u64,
        fallback_visits: u64,
        fallback_input_bytes: u64,
        linked_todo_rows: u64,
        related_note_rows: u64,
        mention_rows: u64,
        snippet_calls: u64,
        metadata_reads: u64,
        complete_output_id: u64,
        structural_output_id: u64,
        note_revision: u64,
        todo_revision: u64,
        equal_break_even_k: Option<u64>,
        changed_break_even_k: Option<u64>,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct TrackC1CProjectionOwnerReceipt {
        note_revision: u64,
        todo_revision: u64,
        structural_output_id: u64,
        complete_output_id: u64,
        work: TrackCPreparedProjectionWork,
        elapsed_ns: u64,
    }

    fn track_c1c_refresh_prepared_owner(
        state: &mut TrackC1ANoteUiOwnerState,
        projection: &TrackCPreparedNoteProjection,
    ) -> TrackC1CProjectionOwnerReceipt {
        use crate::performance::{Metric, MetricTimer, track_c};

        let _current_note_revision = note_version();
        let current_todo_revision = todo_version();
        let mut relationship_timer = track_c::Timer::start(track_c::Phase::NoteRelationshipRefresh);
        let (snapshot_note_revision, snapshot_notes) =
            note_cache_snapshot_with_version().expect("candidate owner takes a healthy snapshot");
        let _recompute_timer = MetricTimer::start(Metric::NoteHeavyRecompute);
        let todos = load_todos_or_last_good(TODO_FILE);
        let mut scan_timer = track_c::Timer::start(track_c::Phase::NoteMentionsScan);
        let replay = projection.replay(&state.panel.note, &todos);
        let work = replay.work;
        scan_timer.set_work_units(snapshot_notes.len().saturating_add(todos.len()));
        scan_timer.finish(track_c::Outcome::Completed);

        let [linked_todos, related_notes, mentions] = replay.rows;
        state.panel.derived.backlink_rows_linked_todos = linked_todos;
        state.panel.derived.backlink_rows_related_notes = related_notes;
        state.panel.derived.backlink_rows_mentions = mentions;
        state.panel.derived.todo_label_map = replay.todo_label_map;
        state.panel.last_notes_version = snapshot_note_revision;
        state.panel.last_todo_revision = current_todo_revision;
        state.panel.last_backlinks_enabled = Some(true);
        state.panel.heavy_recompute_requested = false;
        state.panel.heavy_refresh_retry = None;
        state.panel.heavy_recompute_count = state.panel.heavy_recompute_count.saturating_add(1);

        relationship_timer.set_work_units(snapshot_notes.len().saturating_add(todos.len()));
        relationship_timer.finish(track_c::Outcome::Completed);
        drop(_recompute_timer);
        drop(snapshot_notes);
        drop(todos);
        TrackC1CProjectionOwnerReceipt {
            note_revision: snapshot_note_revision,
            todo_revision: current_todo_revision,
            work,
            ..TrackC1CProjectionOwnerReceipt::default()
        }
    }

    fn track_c1c_validate_panel(
        state: &TrackC1ANoteUiOwnerState,
    ) -> TrackC1CProjectionOwnerReceipt {
        assert_eq!(
            track_c1a_panel_rows(&state.panel),
            state.expected_rows,
            "all seven row fields and category order match the frozen owner oracle"
        );
        assert_eq!(state.panel.derived.todo_label_map, state.expected_labels);
        assert_eq!(state.panel.last_notes_version, state.expected_note_revision);
        assert_eq!(state.panel.last_todo_revision, state.expected_todo_revision);
        assert_eq!(note_version(), state.expected_note_revision);
        assert_eq!(todo_version(), state.expected_todo_revision);
        assert_eq!(state.panel.note.content, state.draft_content);
        assert_eq!(state.panel.saved_content_baseline, state.original_content);
        assert!(!state.panel.heavy_recompute_requested);
        assert!(state.panel.heavy_refresh_retry.is_none());
        let structural_output_id = track_c_backlink_rows_identity(
            &track_c1a_panel_rows(&state.panel),
            &state.panel.derived.todo_label_map,
        );
        let mut complete = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_3143_4E4F_5554,
            "track-c-c1c-note-complete-state",
            8,
        );
        complete.number(structural_output_id);
        complete.number(state.panel.last_notes_version);
        complete.number(state.panel.last_todo_revision);
        complete.number(state.panel.note.slug.len() as u64);
        complete.bytes(state.panel.note.slug.as_bytes());
        complete.number(state.panel.note.content.len() as u64);
        complete.bytes(state.panel.note.content.as_bytes());
        complete.number(state.panel.saved_content_baseline.len() as u64);
        complete.bytes(state.panel.saved_content_baseline.as_bytes());
        complete.number(state.panel.heavy_recompute_requested as u64);
        complete.number((state.panel.last_backlinks_enabled == Some(true)) as u64);
        TrackC1CProjectionOwnerReceipt {
            note_revision: state.panel.last_notes_version,
            todo_revision: state.panel.last_todo_revision,
            structural_output_id,
            complete_output_id: complete.finish(),
            ..TrackC1CProjectionOwnerReceipt::default()
        }
    }

    fn track_c1c_assert_projection_matches_frozen(
        projection: &TrackCPreparedNoteProjection,
        current: &Note,
        todos: &[crate::plugins::todo::TodoEntry],
        canonical_notes: &[Note],
    ) -> TrackCPreparedProjectionWork {
        let replay = projection.replay(current, todos);
        let expected = [
            BacklinkTab::LinkedTodos,
            BacklinkTab::RelatedNotes,
            BacklinkTab::Mentions,
        ]
        .into_iter()
        .map(|tab| track_c_eager_note_rows(current, tab, todos, canonical_notes))
        .collect::<Vec<_>>();
        assert_eq!(
            track_c_prepared_rows_signature(&replay.rows),
            expected
                .iter()
                .map(|rows| backlink_rows_signature(rows))
                .collect::<Vec<_>>(),
            "prepared replay matches every frozen row field and ordered category"
        );
        assert_eq!(replay.todo_label_map, track_c1a_labels_for(todos));
        replay.work
    }

    fn track_c1c_run_current_owner(
        state: &mut TrackC1ANoteUiOwnerState,
    ) -> TrackC1CProjectionOwnerReceipt {
        use crate::performance::track_c;

        track_c::reset();
        crate::performance::reset_metrics();
        let started = Instant::now();
        let completed = state.panel.refresh_heavy_derived(true, true);
        let elapsed_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        assert!(
            completed,
            "current fused owner completes the controlled snapshot"
        );
        let mut receipt = track_c1c_validate_panel(state);
        receipt.elapsed_ns = elapsed_ns;
        track_c1c_assert_owner_phase_receipts(
            state.expected_notes.len(),
            state.expected_todos.len(),
        );
        receipt
    }

    fn track_c1c_run_projection_owner(
        state: &mut TrackC1ANoteUiOwnerState,
        projection: &TrackCPreparedNoteProjection,
    ) -> TrackC1CProjectionOwnerReceipt {
        use crate::performance::track_c;

        track_c::reset();
        crate::performance::reset_metrics();
        let started = Instant::now();
        let owner_receipt = track_c1c_refresh_prepared_owner(state, projection);
        let elapsed_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let mut receipt = track_c1c_validate_panel(state);
        assert_eq!(owner_receipt.note_revision, receipt.note_revision);
        assert_eq!(owner_receipt.todo_revision, receipt.todo_revision);
        receipt.work = owner_receipt.work;
        receipt.elapsed_ns = elapsed_ns;
        track_c1c_assert_owner_phase_receipts(
            state.expected_notes.len(),
            state.expected_todos.len(),
        );
        receipt
    }

    fn track_c1c_assert_owner_phase_receipts(note_count: usize, todo_count: usize) {
        use crate::performance::track_c::{self, Phase};

        if !crate::performance::enabled() {
            return;
        }
        let expected_work = note_count.saturating_add(todo_count) as u64;
        let phases = track_c::snapshot();
        for phase in [Phase::NoteRelationshipRefresh, Phase::NoteMentionsScan] {
            let stats = phases
                .iter()
                .find(|(candidate, _)| *candidate == phase)
                .expect("Track C phase has fixed cardinality")
                .1;
            assert_eq!(stats.calls, 1);
            assert_eq!(stats.work_units, expected_work);
        }
    }

    fn track_c1c_assert_unchanged_gate_has_no_heavy_work() {
        use crate::performance::{Metric, snapshot_metrics, track_c};

        assert_eq!(snapshot_metrics()[Metric::NoteSnapshot as usize].calls, 0);
        for phase in [
            track_c::Phase::NoteRelationshipRefresh,
            track_c::Phase::NoteMentionsScan,
        ] {
            let stats = track_c::snapshot()
                .into_iter()
                .find(|(candidate, _)| *candidate == phase)
                .expect("Track C phase has fixed cardinality")
                .1;
            assert_eq!(stats.calls, 0);
            assert_eq!(stats.work_units, 0);
        }
    }

    fn track_c1c_break_even_k(
        current_publish_ns: u64,
        projection_publish_ns: u64,
        current_owner_ns: u64,
        projection_owner_ns: u64,
    ) -> Option<u64> {
        let per_refresh_saving = current_owner_ns.checked_sub(projection_owner_ns)?;
        if per_refresh_saving == 0 {
            return None;
        }
        let extra_publish = projection_publish_ns.saturating_sub(current_publish_ns);
        Some(extra_publish.div_ceil(per_refresh_saving))
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct TrackC1CLocalPublicationReceipt {
        equal_current_ns: u64,
        equal_projection_ns: u64,
        changed_current_ns: u64,
        changed_projection_ns: u64,
        equal_retained_logical_bytes: usize,
        equal_retained_capacity_bytes: usize,
        changed_old_logical_bytes: usize,
        changed_old_capacity_bytes: usize,
        changed_peak_logical_bytes: usize,
        changed_peak_capacity_bytes: usize,
    }

    fn track_c1c_measure_local_publication(
        canonical_notes: &[Note],
        current: &Note,
        todos: &[crate::plugins::todo::TodoEntry],
    ) -> TrackC1CLocalPublicationReceipt {
        use crate::plugins::note::build_note_cache_candidate_for_test;

        assert!(canonical_notes.len() > 1);
        let mut previous_notes = canonical_notes.to_vec();
        previous_notes[1].title = format!("Previous {}", previous_notes[1].title);

        // Prepare the old published generations outside the transaction
        // timers. Both changed candidates below target this sample's exact
        // canonical replay snapshot, rather than an unrelated edit.
        let equal_current_cache = build_note_cache_candidate_for_test(canonical_notes.to_vec());
        let equal_projection_bundle = TrackC1CProjectionBundle::new(canonical_notes.to_vec());
        let mut changed_current_cache = build_note_cache_candidate_for_test(previous_notes.clone());
        let mut changed_projection_bundle = TrackC1CProjectionBundle::new(previous_notes);
        let equal_footprint = equal_projection_bundle.projection.footprint();
        let changed_old_footprint = changed_projection_bundle.projection.footprint();

        // Cloning test inputs is setup, not canonical construction or local
        // publication work measured below.
        let equal_current_input = canonical_notes.to_vec();
        let current_started = Instant::now();
        let equal_candidate = build_note_cache_candidate_for_test(equal_current_input);
        let current_equal = equal_current_cache.notes == equal_candidate.notes;
        drop(equal_candidate);
        let equal_current_ns = current_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        assert!(current_equal, "same canonical input takes the equal branch");

        // The proposed owner checks canonical equality before preparing an
        // index. An equal candidate is discarded and retains the old
        // projection, so this timed branch intentionally does not build one.
        let equal_projection_input = canonical_notes.to_vec();
        let projection_started = Instant::now();
        let equal_projection_cache = build_note_cache_candidate_for_test(equal_projection_input);
        let projection_equal = equal_projection_bundle.cache.notes == equal_projection_cache.notes;
        drop(equal_projection_cache);
        let equal_projection_ns = projection_started
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        assert!(
            projection_equal,
            "projection candidate uses the same equal canonical input and branch"
        );

        let changed_current_input = canonical_notes.to_vec();
        let current_started = Instant::now();
        let changed_candidate = build_note_cache_candidate_for_test(changed_current_input);
        let current_changed = changed_current_cache.notes != changed_candidate.notes;
        if current_changed {
            let previous = std::mem::replace(&mut changed_current_cache, changed_candidate);
            drop(previous);
        } else {
            drop(changed_candidate);
        }
        let changed_current_ns = current_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        assert!(
            current_changed,
            "changed canonical input takes replacement branch"
        );

        let changed_projection_input = canonical_notes.to_vec();
        let projection_started = Instant::now();
        let changed_projection_cache =
            build_note_cache_candidate_for_test(changed_projection_input);
        let projection_changed =
            changed_projection_bundle.cache.notes != changed_projection_cache.notes;
        if projection_changed {
            let projection = TrackCPreparedNoteProjection::prepare(&changed_projection_cache.notes);
            let replacement = TrackC1CProjectionBundle {
                cache: changed_projection_cache,
                projection,
            };
            let previous = std::mem::replace(&mut changed_projection_bundle, replacement);
            drop(previous);
        } else {
            drop(changed_projection_cache);
        }
        let changed_projection_ns = projection_started
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        assert!(
            projection_changed,
            "changed projection candidate takes the matching replacement branch"
        );
        assert_eq!(changed_current_cache.notes, canonical_notes);
        assert_eq!(changed_projection_bundle.cache.notes, canonical_notes);
        assert_eq!(equal_projection_bundle.cache.notes, canonical_notes);
        let _ = track_c1c_assert_projection_matches_frozen(
            &equal_projection_bundle.projection,
            current,
            todos,
            canonical_notes,
        );
        let _ = track_c1c_assert_projection_matches_frozen(
            &changed_projection_bundle.projection,
            current,
            todos,
            canonical_notes,
        );

        let new_footprint = changed_projection_bundle.projection.footprint();
        TrackC1CLocalPublicationReceipt {
            equal_current_ns,
            equal_projection_ns,
            changed_current_ns,
            changed_projection_ns,
            // Equal rejection retains one projection generation because the
            // approved owner checks equality before preparing the candidate.
            equal_retained_logical_bytes: projection_logical_bytes(equal_footprint),
            equal_retained_capacity_bytes: projection_capacity_bytes(equal_footprint),
            // During changed replacement the old and new projection
            // generations overlap until the local install drops the old one.
            changed_old_logical_bytes: projection_logical_bytes(changed_old_footprint),
            changed_old_capacity_bytes: projection_capacity_bytes(changed_old_footprint),
            changed_peak_logical_bytes: projection_logical_bytes(changed_old_footprint)
                .saturating_add(projection_logical_bytes(new_footprint)),
            changed_peak_capacity_bytes: projection_capacity_bytes(changed_old_footprint)
                .saturating_add(projection_capacity_bytes(new_footprint)),
        }
    }

    fn projection_logical_bytes(footprint: TrackCPreparedProjectionFootprint) -> usize {
        footprint
            .text_logical_bytes
            .saturating_add(footprint.metadata_logical_bytes)
            .saturating_add(footprint.source_slot_bytes)
            .saturating_add(footprint.posting_key_logical_bytes)
            .saturating_add(
                footprint
                    .posting_positions
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
    }

    fn projection_capacity_bytes(footprint: TrackCPreparedProjectionFootprint) -> usize {
        footprint
            .text_capacity_bytes
            .saturating_add(footprint.metadata_string_capacity_bytes)
            .saturating_add(footprint.source_slot_bytes)
            .saturating_add(footprint.posting_key_capacity_bytes)
            .saturating_add(
                footprint
                    .posting_position_capacity
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
    }

    fn track_c1c_emit_samples(
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        item_count: usize,
        transition: TrackC1ANoteTransition,
        fixture_signature: u64,
        samples: &[TrackC1CSample],
        footprint: TrackCPreparedProjectionFootprint,
        equal_retained_logical_bytes: usize,
        equal_retained_capacity_bytes: usize,
        changed_old_logical_bytes: usize,
        changed_old_capacity_bytes: usize,
        changed_peak_logical_bytes: usize,
        changed_peak_capacity_bytes: usize,
    ) {
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, WARMUPS};
        assert_eq!(samples.len(), MEASURED_SAMPLES);
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        assert!(!source.bytes().any(|byte| byte.is_ascii_whitespace()));
        assert!(!profile.bytes().any(|byte| byte.is_ascii_whitespace()));
        let values =
            |select: fn(&TrackC1CSample) -> u64| track_c1a_join_u64(samples.iter().map(select));
        let equal_current_one = samples
            .iter()
            .map(|sample| {
                sample
                    .equal_current_publish_ns
                    .saturating_add(sample.current_owner_ns)
            })
            .collect::<Vec<_>>();
        let equal_projection_one = samples
            .iter()
            .map(|sample| {
                sample
                    .equal_projection_publish_ns
                    .saturating_add(sample.projection_owner_ns)
            })
            .collect::<Vec<_>>();
        let equal_current_five = samples
            .iter()
            .map(|sample| {
                sample
                    .equal_current_publish_ns
                    .saturating_add(sample.current_repeat5_ns)
            })
            .collect::<Vec<_>>();
        let equal_projection_five = samples
            .iter()
            .map(|sample| {
                sample
                    .equal_projection_publish_ns
                    .saturating_add(sample.projection_repeat5_ns)
            })
            .collect::<Vec<_>>();
        let changed_current_one = samples
            .iter()
            .map(|sample| {
                sample
                    .changed_current_publish_ns
                    .saturating_add(sample.current_owner_ns)
            })
            .collect::<Vec<_>>();
        let changed_projection_one = samples
            .iter()
            .map(|sample| {
                sample
                    .changed_projection_publish_ns
                    .saturating_add(sample.projection_owner_ns)
            })
            .collect::<Vec<_>>();
        let changed_current_five = samples
            .iter()
            .map(|sample| {
                sample
                    .changed_current_publish_ns
                    .saturating_add(sample.current_repeat5_ns)
            })
            .collect::<Vec<_>>();
        let changed_projection_five = samples
            .iter()
            .map(|sample| {
                sample
                    .changed_projection_publish_ns
                    .saturating_add(sample.projection_repeat5_ns)
            })
            .collect::<Vec<_>>();
        let join = |items: &[u64]| track_c1a_join_u64(items.iter().copied());
        let mut line = format!(
            "C1C_NOTE_RESULT schema=1 stage=prepared_projection owner=NotePanel_refresh_heavy_derived_vs_prepared_projection_replay source={source} profile={profile} mode={} telemetry_enabled={} observer_enabled=false fixture=synthetic_note_panel_refresh fixture_signature={fixture_signature:016x} transition={} count={item_count} todo_count=12 viewport=960x640 settings=backlinks_enabled_inert cold_type={} warmups={WARMUPS} samples={MEASURED_SAMPLES} exclusions=0 repeat_k=5 repeat_scope=forced_invalidated_refresh_not_idle publication_totals_applicability=standalone_model_not_transition_latency publication_model=local_cache_simulation_without_global_lock_or_revision integrated_owner=false break_even_scope=local_simulation memory_scope=projection_only_lossy_utf8_paths_map_buckets_excluded allocator_and_process_ram=not_measured text_logical_bytes={} text_capacity_bytes={} metadata_logical_bytes={} metadata_string_capacity_bytes={} source_slot_bytes={} posting_key_logical_bytes={} posting_key_capacity_bytes={} posting_positions={} posting_position_capacity={} equal_projection_retained_logical_bytes={equal_retained_logical_bytes} equal_projection_retained_capacity_bytes={equal_retained_capacity_bytes} changed_old_projection_logical_bytes={changed_old_logical_bytes} changed_old_projection_capacity_bytes={changed_old_capacity_bytes} changed_two_generation_peak_logical_bytes={changed_peak_logical_bytes} changed_two_generation_peak_capacity_bytes={changed_peak_capacity_bytes}",
            match mode {
                crate::performance::track_c_workloads::BenchmarkMode::Small => "small",
                crate::performance::track_c_workloads::BenchmarkMode::Full => "full",
            },
            crate::performance::enabled(),
            transition.report_label(),
            transition.cold_type(),
            footprint.text_logical_bytes,
            footprint.text_capacity_bytes,
            footprint.metadata_logical_bytes,
            footprint.metadata_string_capacity_bytes,
            footprint.source_slot_bytes,
            footprint.posting_key_logical_bytes,
            footprint.posting_key_capacity_bytes,
            footprint.posting_positions,
            footprint.posting_position_capacity,
        );
        use std::fmt::Write as _;
        write!(
            line,
            " current_owner_ns=[{}] projection_owner_ns=[{}] projection_prepare_ns=[{}] current_repeat5_ns=[{}] projection_repeat5_ns=[{}] unchanged_gate_ns=[{}] equal_current_publish_ns=[{}] equal_projection_publish_ns=[{}] changed_current_publish_ns=[{}] changed_projection_publish_ns=[{}] equal_current_plus1_ns=[{}] equal_projection_plus1_ns=[{}] equal_current_plus5_ns=[{}] equal_projection_plus5_ns=[{}] changed_current_plus1_ns=[{}] changed_projection_plus1_ns=[{}] changed_current_plus5_ns=[{}] changed_projection_plus5_ns=[{}] equal_break_even_k=[{}] equal_break_even_finite=[{}] changed_break_even_k=[{}] changed_break_even_finite=[{}] canonical_posting_entries=[{}] typed_posting_entries=[{}] fallback_visits=[{}] fallback_input_bytes=[{}] linked_todo_rows=[{}] related_note_rows=[{}] mention_rows=[{}] snippet_calls=[{}] metadata_reads=[{}] complete_output_ids=[{}] structural_output_ids=[{}] note_revisions=[{}] todo_revisions=[{}]",
            values(|sample| sample.current_owner_ns),
            values(|sample| sample.projection_owner_ns),
            values(|sample| sample.projection_prepare_ns),
            values(|sample| sample.current_repeat5_ns),
            values(|sample| sample.projection_repeat5_ns),
            values(|sample| sample.unchanged_gate_ns),
            values(|sample| sample.equal_current_publish_ns),
            values(|sample| sample.equal_projection_publish_ns),
            values(|sample| sample.changed_current_publish_ns),
            values(|sample| sample.changed_projection_publish_ns),
            join(&equal_current_one),
            join(&equal_projection_one),
            join(&equal_current_five),
            join(&equal_projection_five),
            join(&changed_current_one),
            join(&changed_projection_one),
            join(&changed_current_five),
            join(&changed_projection_five),
            track_c1a_join_u64(samples.iter().map(|sample| sample.equal_break_even_k.unwrap_or(0))),
            track_c1a_join_u64(samples.iter().map(|sample| sample.equal_break_even_k.is_some() as u64)),
            track_c1a_join_u64(samples.iter().map(|sample| sample.changed_break_even_k.unwrap_or(0))),
            track_c1a_join_u64(samples.iter().map(|sample| sample.changed_break_even_k.is_some() as u64)),
            values(|sample| sample.canonical_posting_entries),
            values(|sample| sample.typed_posting_entries),
            values(|sample| sample.fallback_visits),
            values(|sample| sample.fallback_input_bytes),
            values(|sample| sample.linked_todo_rows),
            values(|sample| sample.related_note_rows),
            values(|sample| sample.mention_rows),
            values(|sample| sample.snippet_calls),
            values(|sample| sample.metadata_reads),
            track_c1a_join_hex(samples.iter().map(|sample| sample.complete_output_id)),
            track_c1a_join_hex(samples.iter().map(|sample| sample.structural_output_id)),
            values(|sample| sample.note_revision),
            values(|sample| sample.todo_revision),
        )
        .expect("append compact C1-C numeric record");
        println!("{line}");
    }

    fn track_c_backlink_rows_identity(
        rows: &[Vec<(
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        )>],
        labels: &HashMap<String, String>,
    ) -> u64 {
        let mut signature = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_4E4F_5445,
            "track-c-backlink-output",
            rows.iter().map(Vec::len).sum::<usize>(),
        );
        let string = |signature: &mut crate::performance::track_c_workloads::StableSignature,
                      value: &str| {
            signature.number(value.len() as u64);
            signature.bytes(value.as_bytes());
        };
        for category in rows {
            signature.number(category.len() as u64);
            for row in category {
                string(&mut signature, &row.0);
                string(&mut signature, &row.1);
                string(&mut signature, &row.2);
                string(&mut signature, &row.3);
                string(&mut signature, &row.4);
                for value in [&row.5, &row.6] {
                    if let Some(value) = value {
                        signature.number(1);
                        string(&mut signature, value);
                    } else {
                        signature.number(0);
                    }
                }
            }
        }
        let mut sorted_labels = labels.iter().collect::<Vec<_>>();
        sorted_labels.sort_unstable_by(|left, right| left.0.cmp(right.0));
        for (id, label) in sorted_labels {
            string(&mut signature, id);
            string(&mut signature, label);
        }
        signature.finish()
    }

    type TrackCBacklinkRowIdentity = (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    );

    struct TrackCNoteOwnerState {
        context: egui::Context,
        panel: NotePanel,
        _cache_guard: crate::plugins::note::NoteCachePublicationGuard,
        expected_rows: Vec<Vec<TrackCBacklinkRowIdentity>>,
        expected_labels: HashMap<String, String>,
        expected_note_count: usize,
        expected_todo_count: usize,
        expected_note_revision: u64,
        expected_todo_revision: u64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TrackC1ANoteTransition {
        LocalDraft,
        ExternalNoteRevision,
        PersistedTodoRevision,
    }

    impl TrackC1ANoteTransition {
        fn report_label(self) -> &'static str {
            match self {
                Self::LocalDraft => "local-draft-debounce",
                Self::ExternalNoteRevision => "external-note-revision",
                Self::PersistedTodoRevision => "persisted-todo-revision",
            }
        }

        fn cold_type(self) -> &'static str {
            match self {
                Self::LocalDraft => "debounce-due-refresh",
                Self::ExternalNoteRevision => "external-note-revision-bypass",
                Self::PersistedTodoRevision => "todo-revision-bypass",
            }
        }
    }

    struct TrackC1ATodoSnapshotGuard {
        path: String,
        original: Vec<crate::plugins::todo::TodoEntry>,
    }

    impl TrackC1ATodoSnapshotGuard {
        fn publish(path: &str, replacement: &[crate::plugins::todo::TodoEntry]) -> Self {
            let original = crate::plugins::todo::TODO_DATA
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            crate::plugins::todo::replace_todos(path, replacement.to_vec())
                .expect("publish synthetic TODO state in the owned workspace");
            Self {
                path: path.to_string(),
                original,
            }
        }
    }

    impl Drop for TrackC1ATodoSnapshotGuard {
        fn drop(&mut self) {
            let _ =
                crate::plugins::todo::replace_todos(&self.path, std::mem::take(&mut self.original));
        }
    }

    struct TrackC1ANoteUiOwnerState {
        // Remove observation before dropping owners; app, panel, caches, and
        // their paths are torn down before the owned CWD is restored.
        _observer_scope: NoteRefreshObservationScope,
        panel: NotePanel,
        app: LauncherApp,
        _note_cache_guard: crate::plugins::note::NoteCachePublicationGuard,
        _todo_snapshot_guard: TrackC1ATodoSnapshotGuard,
        context: egui::Context,
        expected_notes: Vec<Note>,
        expected_todos: Vec<crate::plugins::todo::TodoEntry>,
        expected_rows: Vec<Vec<TrackCBacklinkRowIdentity>>,
        expected_labels: HashMap<String, String>,
        expected_note_revision: u64,
        expected_todo_revision: u64,
        initial_rows: Vec<Vec<TrackCBacklinkRowIdentity>>,
        original_content: String,
        draft_content: String,
        transition: TrackC1ANoteTransition,
        fixture_signature: u64,
        item_count: usize,
        _workspace: crate::performance::track_c_workloads::TrackCWorkspace,
        _notes_env_guard: MutexGuard<'static, ()>,
    }

    #[derive(Clone, Debug)]
    struct TrackC1ANoteSampleDetails {
        observation: NoteRefreshObservation,
        snapshot_metric: crate::performance::MetricSnapshot,
    }

    impl TrackC1ANoteUiOwnerState {
        fn new(item_count: usize, transition: TrackC1ANoteTransition) -> Self {
            use crate::performance::track_c_workloads::TrackCWorkspace;

            // Match the existing test lock order: TrackCWorkspace owns the
            // workload/CWD lock, then the note-environment lock serializes
            // TempNotesDir users while the app and workspace are alive.
            let workspace = TrackCWorkspace::new();
            let notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes env lock");
            let (fixture, original_todos) = track_c_note_inputs(item_count);
            let mut signature_notes = fixture.values.clone();
            let mut signature_todos = original_todos.clone();

            if transition == TrackC1ANoteTransition::ExternalNoteRevision {
                write_track_c_note_files(&workspace, &fixture.values);
            }

            let context = egui::Context::default();
            let mut app = track_c1a_make_inert_app(&context, &workspace);

            // Finish app construction before publishing the owner fixture.
            // The explicit inert manager prevents PluginEditor metadata
            // discovery from registering default plugins and refreshing the
            // global note cache; publish the controlled stores last so the
            // panel and eager oracle observe the same inputs.
            let note_cache_guard =
                crate::plugins::note::publish_note_cache_for_test(fixture.values.clone());
            let todo_snapshot_guard =
                TrackC1ATodoSnapshotGuard::publish(TODO_FILE, &original_todos);

            if transition == TrackC1ANoteTransition::ExternalNoteRevision {
                crate::plugins::note::refresh_cache()
                    .expect("load the owned initial Markdown fixture");
            }

            let (initial_note_revision, initial_notes) =
                crate::plugins::note::note_cache_snapshot_with_version()
                    .expect("initial synthetic note cache snapshot is healthy");
            let current = initial_notes
                .iter()
                .find(|note| note.slug == "synthetic-note-00000")
                .expect("fixture contains the current note")
                .clone();

            let mut panel = NotePanel::from_note(current);
            let (panel_note_revision, panel_notes) =
                crate::plugins::note::note_cache_snapshot_with_version()
                    .expect("panel initialization leaves the controlled note cache available");
            assert_eq!(panel_note_revision, initial_note_revision);
            assert_eq!(panel_notes, initial_notes);
            let saved_original_content = panel.note.content.clone();
            let baseline_rows = track_c1a_rows_for(&panel.note, &original_todos, &initial_notes);
            let baseline_labels = track_c1a_labels_for(&original_todos);
            assert_eq!(
                track_c1a_panel_rows(&panel),
                baseline_rows,
                "initial actual panel rows match the frozen eager reference"
            );
            assert_eq!(panel.derived.todo_label_map, baseline_labels);
            assert_eq!(panel.last_notes_version, initial_note_revision);
            assert_eq!(panel.last_todo_revision, todo_version());

            // Settle the real dialog caller before arming the observer or
            // editing. The retained FullOutput is dropped outside any sample.
            let baseline_output = track_c1a_run_frame(&context, &mut panel, &mut app, 9.0);
            drop(baseline_output);
            let initial_rows = track_c1a_panel_rows(&panel);

            let observer_scope = NoteRefreshObservationScope::new();
            let draft_content = format!(
                "{}\n\nDraft-only relation @todo:todo-11",
                saved_original_content
            );
            panel.replace_content_from_mutation(draft_content.clone(), 10.0);
            let predeadline_output = track_c1a_run_frame(&context, &mut panel, &mut app, 10.1);
            drop(predeadline_output);
            let predeadline_observation = observer_scope.snapshot();
            assert_eq!(
                predeadline_observation,
                NoteRefreshObservation::default(),
                "the pre-deadline actual UI frame does no heavy refresh work"
            );
            assert_eq!(track_c1a_panel_rows(&panel), initial_rows);
            assert_eq!(panel.derived.todo_label_map, baseline_labels);
            assert_eq!(panel.last_notes_version, initial_note_revision);
            assert_eq!(panel.last_todo_revision, todo_version());
            assert_eq!(panel.saved_content_baseline, saved_original_content);
            assert_eq!(panel.note.content, draft_content);
            assert!(panel.heavy_recompute_requested);

            let mut expected_notes = initial_notes;
            let mut expected_todos = original_todos.clone();
            match transition {
                TrackC1ANoteTransition::LocalDraft => {}
                TrackC1ANoteTransition::ExternalNoteRevision => {
                    let changed = &mut signature_notes[1];
                    let replacement =
                        track_c1a_external_revision_content(&changed.slug, &changed.content);
                    changed.title = replacement
                        .lines()
                        .next()
                        .and_then(|line| line.strip_prefix("# "))
                        .expect("external fixture retains a Markdown title")
                        .to_string();
                    changed.content = replacement.clone();
                    let path = workspace.notes_dir().join(format!("{}.md", changed.slug));
                    std::fs::write(path, replacement)
                        .expect("persist one external note edit in the owned workspace");
                    crate::plugins::note::refresh_cache()
                        .expect("publish the externally changed owned note files");
                    let (revision, notes) =
                        crate::plugins::note::note_cache_snapshot_with_version()
                            .expect("refreshed synthetic note cache snapshot is healthy");
                    assert_ne!(revision, initial_note_revision);
                    expected_notes = notes;
                }
                TrackC1ANoteTransition::PersistedTodoRevision => {
                    signature_todos[0].text =
                        "Persisted TODO revision for @note:synthetic-note-00000".into();
                    signature_todos[0].tags.push("persisted-revision".into());
                    expected_todos =
                        crate::plugins::todo::replace_todos(TODO_FILE, signature_todos.clone())
                            .expect("publish a changed TODO snapshot in the owned workspace");
                }
            }

            let (expected_note_revision, published_notes) =
                crate::plugins::note::note_cache_snapshot_with_version()
                    .expect("expected synthetic note snapshot is healthy");
            if transition != TrackC1ANoteTransition::ExternalNoteRevision {
                expected_notes = published_notes;
            }
            let expected_todo_revision = todo_version();
            assert_eq!(
                expected_notes.len(),
                item_count,
                "each scenario keeps the complete synthetic note catalog"
            );
            if transition != TrackC1ANoteTransition::PersistedTodoRevision {
                expected_todos = original_todos;
            }
            let expected_rows = track_c1a_rows_for(&panel.note, &expected_todos, &expected_notes);
            let expected_labels = track_c1a_labels_for(&expected_todos);
            assert_ne!(
                expected_rows, initial_rows,
                "each transition must change the actual derived output"
            );

            let fixture_signature = track_c1a_scenario_identity(
                &signature_notes,
                &signature_todos,
                transition,
                &draft_content,
            );
            observer_scope.reset();
            crate::performance::track_c::reset();
            crate::performance::reset_metrics();

            Self {
                _observer_scope: observer_scope,
                panel,
                app,
                _note_cache_guard: note_cache_guard,
                _todo_snapshot_guard: todo_snapshot_guard,
                context,
                expected_notes,
                expected_todos,
                expected_rows,
                expected_labels,
                expected_note_revision,
                expected_todo_revision,
                initial_rows,
                original_content: saved_original_content,
                draft_content,
                transition,
                fixture_signature,
                item_count,
                _workspace: workspace,
                _notes_env_guard: notes_env_guard,
            }
        }

        fn owner_time(&self) -> f64 {
            match self.transition {
                TrackC1ANoteTransition::LocalDraft => 10.26,
                TrackC1ANoteTransition::ExternalNoteRevision
                | TrackC1ANoteTransition::PersistedTodoRevision => 10.2,
            }
        }
    }

    fn track_c1a_make_inert_app(
        context: &egui::Context,
        workspace: &crate::performance::track_c_workloads::TrackCWorkspace,
    ) -> LauncherApp {
        let mut settings = Settings::default();
        settings.enable_toasts = false;
        settings.show_inline_errors = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enabled_plugins = Some(HashSet::new());
        settings.enabled_capabilities = Some(HashMap::new());
        settings.plugin_dirs = Some(Vec::new());
        settings.index_paths = Some(Vec::new());
        settings.note.backlinks_enabled = true;
        settings.note.collapsed_sections_persist = false;

        let mut app = LauncherApp::new(
            context,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            workspace.actions_path().to_string_lossy().into_owned(),
            workspace.settings_path().to_string_lossy().into_owned(),
            settings,
            Some(Vec::new()),
            Some(Vec::new()),
            Some(HashSet::new()),
            Some(HashMap::new()),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        app.watchers.clear();
        while app.rx.try_recv().is_ok() {}
        app.folder_aliases.clear();
        app.folder_aliases_lc.clear();
        app.bookmark_aliases.clear();
        app.bookmark_aliases_lc.clear();
        app.usage.clear();
        app.enable_toasts = false;
        app.show_inline_errors = false;
        app.show_error_toasts = false;
        app.dashboard_enabled = false;
        app.note_settings.backlinks_enabled = true;
        app.note_settings.collapsed_sections_persist = false;
        app
    }

    fn write_track_c_note_files(
        workspace: &crate::performance::track_c_workloads::TrackCWorkspace,
        notes: &[Note],
    ) {
        let notes_dir = workspace.notes_dir();
        std::fs::create_dir_all(&notes_dir).expect("owned note directory exists");
        for note in notes {
            std::fs::write(notes_dir.join(format!("{}.md", note.slug)), &note.content)
                .expect("write synthetic note document");
        }
    }

    fn track_c1a_external_revision_content(slug: &str, content: &str) -> String {
        format!("# External revision {slug}\n{content}")
    }

    fn track_c1a_rows_for(
        current: &Note,
        todos: &[crate::plugins::todo::TodoEntry],
        notes: &[Note],
    ) -> Vec<Vec<TrackCBacklinkRowIdentity>> {
        [
            BacklinkTab::LinkedTodos,
            BacklinkTab::RelatedNotes,
            BacklinkTab::Mentions,
        ]
        .into_iter()
        .map(|tab| backlink_rows_signature(&track_c_eager_note_rows(current, tab, todos, notes)))
        .collect()
    }

    fn track_c1a_panel_rows(panel: &NotePanel) -> Vec<Vec<TrackCBacklinkRowIdentity>> {
        [
            &panel.derived.backlink_rows_linked_todos,
            &panel.derived.backlink_rows_related_notes,
            &panel.derived.backlink_rows_mentions,
        ]
        .into_iter()
        .map(|rows| backlink_rows_signature(rows))
        .collect()
    }

    fn track_c1a_labels_for(todos: &[crate::plugins::todo::TodoEntry]) -> HashMap<String, String> {
        todos
            .iter()
            .filter(|todo| !todo.id.is_empty())
            .map(|todo| (todo.id.clone(), todo.text.clone()))
            .collect()
    }

    fn track_c1a_run_frame(
        context: &egui::Context,
        panel: &mut NotePanel,
        app: &mut LauncherApp,
        time: f64,
    ) -> egui::FullOutput {
        context.run_ui(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 640.0),
                )),
                ..Default::default()
            },
            |context| panel.ui(context, app),
        )
    }

    fn track_c1a_scenario_identity(
        notes: &[Note],
        todos: &[crate::plugins::todo::TodoEntry],
        transition: TrackC1ANoteTransition,
        draft_content: &str,
    ) -> u64 {
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_3141_4E4F_5445,
            "track-c-c1a-note-refresh-input",
            notes.len().saturating_add(todos.len()),
        );
        signature.number(track_c_note_and_todo_fixture_identity(notes, todos));
        signature.number(transition.report_label().len() as u64);
        signature.bytes(transition.report_label().as_bytes());
        signature.number(draft_content.len() as u64);
        signature.bytes(draft_content.as_bytes());
        signature.finish()
    }

    fn track_c1a_validate_owner(
        state: &mut TrackC1ANoteUiOwnerState,
        output: &egui::FullOutput,
    ) -> (
        crate::performance::track_c_workloads::OwnerObservation,
        TrackC1ANoteSampleDetails,
    ) {
        use crate::performance::{Metric, track_c};

        // Retaining the complete egui output until after these assertions
        // keeps its destruction and all expected-output work outside timing.
        let _output_pixels_per_point = output.pixels_per_point;
        assert_eq!(state.expected_notes.len(), state.item_count);
        assert_eq!(
            track_c1a_panel_rows(&state.panel),
            state.expected_rows,
            "every row field, category, and order matches the frozen eager oracle"
        );
        assert_eq!(state.panel.derived.todo_label_map, state.expected_labels);
        assert_eq!(state.panel.last_notes_version, state.expected_note_revision);
        assert_eq!(state.panel.last_todo_revision, state.expected_todo_revision);
        assert_eq!(
            crate::plugins::note::note_version(),
            state.expected_note_revision
        );
        assert_eq!(todo_version(), state.expected_todo_revision);
        assert_eq!(state.panel.last_backlinks_enabled, Some(true));
        assert_eq!(state.panel.note.content, state.draft_content);
        assert_eq!(state.panel.saved_content_baseline, state.original_content);
        assert!(!state.panel.heavy_recompute_requested);
        assert!(state.panel.heavy_refresh_retry.is_none());

        let observation = state._observer_scope.snapshot();
        assert_eq!(observation.refresh_calls, 1);
        assert_eq!(observation.successful_recomputes, 1);
        assert_eq!(observation.failed_snapshots, 0);
        assert_eq!(observation.snapshot_calls, 1);
        assert_eq!(observation.snapshot_successes, 1);
        assert_eq!(observation.todo_load_calls, 1);
        assert_eq!(
            observation.label_map_visits,
            state.expected_todos.len() as u64
        );
        assert_eq!(observation.fused_traversal.calls, 1);
        assert_eq!(
            observation.fused_traversal.todo_visits,
            state.expected_todos.len() as u64
        );
        assert_eq!(
            observation.fused_traversal.note_visits,
            state.expected_notes.len() as u64
        );
        assert_eq!(
            observation.fenced_content_calls,
            state.expected_notes.len() as u64
        );

        let structure =
            track_c_backlink_rows_identity(&state.expected_rows, &state.expected_labels);
        assert_ne!(state.expected_rows, state.initial_rows);
        let mut complete = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_3141_4E4F_5554,
            "track-c-c1a-note-complete-state",
            8,
        );
        complete.number(structure);
        complete.number(state.expected_note_revision);
        complete.number(state.expected_todo_revision);
        complete.number(state.original_content.len() as u64);
        complete.bytes(state.original_content.as_bytes());
        complete.number(state.draft_content.len() as u64);
        complete.bytes(state.draft_content.as_bytes());
        complete.number(state.panel.heavy_recompute_requested as u64);
        complete.number((state.panel.last_backlinks_enabled == Some(true)) as u64);
        let output_identity = complete.finish();

        let snapshot_metric = crate::performance::snapshot_metrics()[Metric::NoteSnapshot as usize];
        if crate::performance::enabled() {
            assert_eq!(snapshot_metric.calls, 1);
            // This historical metric records snapshot samples but does not
            // record successful outcomes; observer.snapshot_successes above
            // is the success receipt for this owner.
            assert_eq!(snapshot_metric.completed, 0);
        }
        let note_relationship = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == track_c::Phase::NoteRelationshipRefresh)
            .expect("note relationship phase is fixed-cardinality")
            .1;
        let note_mentions = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == track_c::Phase::NoteMentionsScan)
            .expect("note mentions phase is fixed-cardinality")
            .1;
        if crate::performance::enabled() {
            assert_eq!(note_relationship.calls, 1);
            assert_eq!(note_mentions.calls, 1);
            let expected_visits = state
                .expected_notes
                .len()
                .saturating_add(state.expected_todos.len())
                as u64;
            assert_eq!(note_relationship.work_units, expected_visits);
            assert_eq!(note_mentions.work_units, expected_visits);
        }
        let work_units = note_relationship.work_units;

        let details = TrackC1ANoteSampleDetails {
            observation,
            snapshot_metric,
        };
        state._observer_scope.reset();
        track_c::reset();
        crate::performance::reset_metrics();
        let idle_time = state.owner_time() + 0.01;
        let idle_output =
            track_c1a_run_frame(&state.context, &mut state.panel, &mut state.app, idle_time);
        drop(idle_output);
        assert_eq!(
            state._observer_scope.snapshot(),
            NoteRefreshObservation::default(),
            "the next idle actual UI frame performs no heavy refresh work"
        );
        assert_eq!(
            crate::performance::snapshot_metrics()[Metric::NoteSnapshot as usize].calls,
            0,
            "the idle frame takes no note snapshot"
        );
        let owner_observation = crate::performance::track_c_workloads::OwnerObservation {
            output_identity,
            structural_signature: structure,
            revision_receipts: vec![state.expected_note_revision, state.expected_todo_revision],
            viewport_receipt: None,
            work_units,
            work_counters: vec![
                details.observation.refresh_calls,
                details.observation.snapshot_calls,
                details.observation.successful_recomputes,
                details.observation.todo_load_calls,
                details.observation.label_map_visits,
                work_units,
            ],
        };
        (owner_observation, details)
    }

    fn track_c1a_fixture_signature(item_count: usize, transition: TrackC1ANoteTransition) -> u64 {
        let (fixture, todos) = track_c_note_inputs(item_count);
        let mut signature_notes = fixture.values;
        let mut signature_todos = todos;
        match transition {
            TrackC1ANoteTransition::LocalDraft => {}
            TrackC1ANoteTransition::ExternalNoteRevision => {
                let note = &mut signature_notes[1];
                let revised = track_c1a_external_revision_content(&note.slug, &note.content);
                note.title = revised
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("# "))
                    .expect("external fixture retains a Markdown title")
                    .to_string();
                note.content = revised;
            }
            TrackC1ANoteTransition::PersistedTodoRevision => {
                signature_todos[0].text =
                    "Persisted TODO revision for @note:synthetic-note-00000".into();
                signature_todos[0].tags.push("persisted-revision".into());
            }
        }
        let draft_content = format!(
            "{}\n\nDraft-only relation @todo:todo-11",
            signature_notes[0].content
        );
        track_c1a_scenario_identity(
            &signature_notes,
            &signature_todos,
            transition,
            &draft_content,
        )
    }

    fn track_c1a_join_u64(values: impl IntoIterator<Item = u64>) -> String {
        values
            .into_iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn track_c1a_join_hex(values: impl IntoIterator<Item = u64>) -> String {
        values
            .into_iter()
            .map(|value| format!("{value:016x}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn track_c1b_emit_samples(
        mode: crate::performance::track_c_workloads::BenchmarkMode,
        item_count: usize,
        transition: TrackC1ANoteTransition,
        fixture_signature: u64,
        samples: &[crate::performance::track_c_workloads::SampleRecord],
        details: &[TrackC1ANoteSampleDetails],
    ) {
        use crate::performance::track_c_workloads::{MEASURED_SAMPLES, WARMUPS};

        assert_eq!(samples.len(), MEASURED_SAMPLES);
        assert_eq!(details.len(), MEASURED_SAMPLES);
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        assert!(!source.bytes().any(|byte| byte.is_ascii_whitespace()));
        assert!(!profile.bytes().any(|byte| byte.is_ascii_whitespace()));
        let elapsed = samples
            .iter()
            .map(|sample| sample.elapsed_nanos)
            .collect::<Vec<_>>();
        let frame_p50 = crate::performance::track_c_workloads::nearest_rank(&elapsed, 50);
        let frame_p95 = crate::performance::track_c_workloads::nearest_rank(&elapsed, 95);
        let frame_max = elapsed.iter().copied().max().unwrap_or_default();
        let mut line = format!(
            "C1B_NOTE_RESULT schema=1 stage=fused owner=egui_Context_run+NotePanel_ui source={source} profile={profile} mode={mode:?} telemetry_enabled={} observer_enabled=true fixture=synthetic_note_panel_refresh fixture_signature={fixture_signature:016x} transition={} count={item_count} todo_count=12 viewport=960x640 settings=backlinks_enabled_inert cold_type={} warmups={WARMUPS} samples={MEASURED_SAMPLES} exclusions=0 frame_timer=Context_run+NotePanel_ui attributed_owner=NotePanel_refresh_heavy_derived frame_elapsed_ns=[{}] frame_p50_ns={frame_p50} frame_p95_ns={frame_p95} frame_max_ns={frame_max} complete_output_state_ids=[{}] structural_output_ids=[{}] note_revisions=[{}] todo_revisions=[{}] work_units=[{}] work_counters=[{}]",
            crate::performance::enabled(),
            transition.report_label(),
            transition.cold_type(),
            track_c1a_join_u64(elapsed),
            track_c1a_join_hex(samples.iter().map(|sample| sample.output_identity)),
            track_c1a_join_hex(samples.iter().map(|sample| sample.structural_signature)),
            track_c1a_join_u64(samples.iter().map(|sample| sample.revision_receipts[0])),
            track_c1a_join_u64(samples.iter().map(|sample| sample.revision_receipts[1])),
            track_c1a_join_u64(samples.iter().map(|sample| sample.work_units)),
            samples
                .iter()
                .map(|sample| track_c1a_join_u64(sample.work_counters.iter().copied()))
                .collect::<Vec<_>>()
                .join(";"),
        );
        let append = |line: &mut String, name: &str, values: String| {
            use std::fmt::Write as _;
            write!(line, " {name}=[{values}]").expect("append numeric C1B field");
        };
        let observer_values = |select: fn(&NoteRefreshObservation) -> u64| {
            track_c1a_join_u64(details.iter().map(|detail| select(&detail.observation)))
        };
        append(
            &mut line,
            "refresh_calls",
            observer_values(|o| o.refresh_calls),
        );
        append(
            &mut line,
            "refresh_successes",
            observer_values(|o| o.successful_recomputes),
        );
        append(
            &mut line,
            "failed_snapshots",
            observer_values(|o| o.failed_snapshots),
        );
        append(
            &mut line,
            "snapshot_calls",
            observer_values(|o| o.snapshot_calls),
        );
        append(
            &mut line,
            "snapshot_successes",
            observer_values(|o| o.snapshot_successes),
        );
        append(
            &mut line,
            "todo_load_calls",
            observer_values(|o| o.todo_load_calls),
        );
        append(
            &mut line,
            "label_map_visits",
            observer_values(|o| o.label_map_visits),
        );
        append(
            &mut line,
            "fence_calls",
            observer_values(|o| o.fenced_content_calls),
        );
        append(
            &mut line,
            "fence_input_bytes",
            observer_values(|o| o.fenced_input_bytes),
        );
        append(
            &mut line,
            "fence_output_bytes",
            observer_values(|o| o.fenced_output_bytes),
        );
        append(
            &mut line,
            "refresh_ns",
            observer_values(|o| o.refresh_elapsed_nanos),
        );
        append(
            &mut line,
            "snapshot_ns",
            observer_values(|o| o.snapshot_elapsed_nanos),
        );
        append(
            &mut line,
            "todo_load_ns",
            observer_values(|o| o.todo_load_elapsed_nanos),
        );
        append(
            &mut line,
            "label_map_build_ns",
            observer_values(|o| o.label_map_build_elapsed_nanos),
        );
        append(
            &mut line,
            "derived_install_ns",
            observer_values(|o| o.derived_install_elapsed_nanos),
        );
        append(
            &mut line,
            "fused_calls",
            observer_values(|o| o.fused_traversal.calls),
        );
        append(
            &mut line,
            "fused_todo_visits",
            observer_values(|o| o.fused_traversal.todo_visits),
        );
        append(
            &mut line,
            "fused_note_visits",
            observer_values(|o| o.fused_traversal.note_visits),
        );
        append(
            &mut line,
            "fused_ns",
            observer_values(|o| o.fused_traversal.elapsed_nanos),
        );
        for (index, name) in ["needles", "fenced", "snippet"].into_iter().enumerate() {
            append(
                &mut line,
                &format!("string_{name}_count"),
                track_c1a_join_u64(
                    details
                        .iter()
                        .map(|d| d.observation.materialized_string_counts[index]),
                ),
            );
            append(
                &mut line,
                &format!("string_{name}_len_bytes"),
                track_c1a_join_u64(
                    details
                        .iter()
                        .map(|d| d.observation.materialized_string_len_bytes[index]),
                ),
            );
            append(
                &mut line,
                &format!("string_{name}_capacity_bytes"),
                track_c1a_join_u64(
                    details
                        .iter()
                        .map(|d| d.observation.materialized_string_capacity_bytes[index]),
                ),
            );
        }
        append(
            &mut line,
            "snapshot_metric_calls",
            track_c1a_join_u64(details.iter().map(|d| d.snapshot_metric.calls)),
        );
        append(
            &mut line,
            "snapshot_metric_estimated_clone_bytes",
            track_c1a_join_u64(details.iter().map(|d| d.snapshot_metric.work_units)),
        );
        append(
            &mut line,
            "snapshot_metric_elapsed_ns",
            track_c1a_join_u64(
                details
                    .iter()
                    .map(|d| d.snapshot_metric.elapsed_nanos_total),
            ),
        );
        append(
            &mut line,
            "snapshot_metric_lock_wait_ns",
            track_c1a_join_u64(
                details
                    .iter()
                    .map(|d| d.snapshot_metric.lock_wait_nanos_total),
            ),
        );
        append(
            &mut line,
            "snapshot_metric_completed",
            track_c1a_join_u64(details.iter().map(|d| d.snapshot_metric.completed)),
        );
        append(
            &mut line,
            "snapshot_metric_errors",
            track_c1a_join_u64(details.iter().map(|d| d.snapshot_metric.errors)),
        );
        append(
            &mut line,
            "snapshot_metric_abandoned",
            track_c1a_join_u64(details.iter().map(|d| d.snapshot_metric.abandoned)),
        );
        println!("{line}");
    }

    #[test]
    fn track_c_c1a_open_note_panel_refresh_matches_frozen_rows_and_actual_work() {
        for transition in [
            TrackC1ANoteTransition::LocalDraft,
            TrackC1ANoteTransition::ExternalNoteRevision,
            TrackC1ANoteTransition::PersistedTodoRevision,
        ] {
            let mut state = TrackC1ANoteUiOwnerState::new(100, transition);
            let time = state.owner_time();
            let output =
                track_c1a_run_frame(&state.context, &mut state.panel, &mut state.app, time);
            let (observation, details) = track_c1a_validate_owner(&mut state, &output);
            drop(output);
            assert_ne!(observation.output_identity, 0);
            assert_ne!(observation.structural_signature, 0);
            assert_eq!(details.observation.fenced_content_calls, 100);
            assert_eq!(details.observation.fused_traversal.calls, 1);
            assert_eq!(details.observation.fused_traversal.todo_visits, 12);
            assert_eq!(details.observation.fused_traversal.note_visits, 100);
        }
    }

    #[test]
    fn track_c_c1b_note_save_on_close_persists_and_refreshes_backlinks() {
        use crate::performance::track_c_workloads::TrackCWorkspace;
        use crate::plugins::todo::TodoEntry;

        let workspace = TrackCWorkspace::new();
        let _notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes env lock");
        let _cache_guard = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        std::fs::create_dir_all(workspace.notes_dir()).expect("create owned notes directory");
        let initial_content = "# Current\n\nOriginal body";
        std::fs::write(workspace.notes_dir().join("current.md"), initial_content)
            .expect("write owned current note");
        std::fs::write(
            workspace.notes_dir().join("other.md"),
            "# Other\n\n[[Current]]",
        )
        .expect("write owned related note");
        crate::plugins::note::refresh_cache().expect("publish owned note files");
        let (initial_revision, initial_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("owned initial cache is healthy");
        let current = initial_notes
            .iter()
            .find(|note| note.slug == "current")
            .expect("current note loaded from the owned workspace")
            .clone();
        assert_eq!(current.path, workspace.notes_dir().join("current.md"));

        let todos = vec![TodoEntry {
            id: "todo-close-save".into(),
            text: "Close-save relationship target".into(),
            done: false,
            priority: 0,
            tags: Vec::new(),
            entity_refs: Vec::new(),
        }];
        let _todo_guard = TrackC1ATodoSnapshotGuard::publish(TODO_FILE, &todos);
        let context = egui::Context::default();
        let mut app = track_c1a_make_inert_app(&context, &workspace);
        app.note_save_on_close = true;
        app.note_always_overwrite = true;
        let draft = format!("{}\n\n@todo:todo-close-save", current.content);
        let mut panel = NotePanel::from_note(current);
        panel.replace_content_from_mutation(draft.clone(), 10.0);
        app.push_note_panel(panel);

        let observer = NoteRefreshObservationScope::new();
        assert!(
            app.close_front_dialog(),
            "the close owner handles the panel"
        );
        let close_observation = observer.snapshot();
        assert_eq!(close_observation.refresh_calls, 1);
        assert_eq!(close_observation.successful_recomputes, 1);
        assert_eq!(close_observation.snapshot_successes, 1);
        assert_eq!(close_observation.fused_traversal.calls, 1);
        assert_eq!(close_observation.fused_traversal.todo_visits, 1);
        assert_eq!(close_observation.fused_traversal.note_visits, 2);
        assert_eq!(close_observation.fenced_content_calls, 2);
        drop(observer);

        let saved_path = workspace.notes_dir().join("current.md");
        assert_eq!(std::fs::read_to_string(&saved_path).unwrap(), draft);
        let (saved_revision, saved_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("save publishes a healthy current note snapshot");
        assert_ne!(saved_revision, initial_revision);
        let saved_current = saved_notes
            .iter()
            .find(|note| note.slug == "current")
            .expect("saved note remains in the canonical cache");
        assert_eq!(saved_current.content, draft);

        let expected_rows = [
            BacklinkTab::LinkedTodos,
            BacklinkTab::RelatedNotes,
            BacklinkTab::Mentions,
        ]
        .into_iter()
        .map(|tab| {
            backlink_rows_signature(&track_c_eager_note_rows(
                saved_current,
                tab,
                &todos,
                &saved_notes,
            ))
        })
        .collect::<Vec<_>>();
        let mut refreshed_panel = NotePanel::from_note(saved_current.clone());
        refreshed_panel.last_backlinks_enabled = None;
        refreshed_panel.maybe_refresh_heavy_derived(&context, true);
        let actual_rows = [
            &refreshed_panel.derived.backlink_rows_linked_todos,
            &refreshed_panel.derived.backlink_rows_related_notes,
            &refreshed_panel.derived.backlink_rows_mentions,
        ]
        .into_iter()
        .map(|rows| backlink_rows_signature(rows))
        .collect::<Vec<_>>();
        assert_eq!(actual_rows, expected_rows);
        assert!(
            refreshed_panel
                .derived
                .backlink_rows_linked_todos
                .iter()
                .any(|row| row.todo_id.as_deref() == Some("todo-close-save")),
            "saved outgoing TODO reference appears in the refreshed derived rows"
        );
    }

    struct TrackC3FScannerReleaseGuard(Option<std::sync::mpsc::SyncSender<()>>);

    impl TrackC3FScannerReleaseGuard {
        fn release(&mut self) -> Result<(), std::sync::mpsc::TrySendError<()>> {
            self.0.take().map_or(Ok(()), |release| release.try_send(()))
        }
    }

    impl Drop for TrackC3FScannerReleaseGuard {
        fn drop(&mut self) {
            if let Some(release) = self.0.take() {
                let _ = release.try_send(());
            }
        }
    }

    #[test]
    fn track_c_c3f_note_save_while_initial_index_scan_is_blocked() {
        use crate::common::entity_ref::{EntityKind, EntityRef};
        use crate::indexer::coordinator::{IndexConfig, IndexCoordinator};
        use crate::performance::track_c_watcher_lifecycle::{
            HandlerKind, HandlerOwnedWorkspace, HandlerReleaseProbe,
        };
        use crate::plugins::todo::TodoEntry;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        const DEADLINE: Duration = Duration::from_secs(10);

        // Acquire the owned CWD/environment before note guards or any app,
        // watcher, or coordinator that can retain these paths.
        let mut workspace = HandlerOwnedWorkspace::new();
        let _notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes env lock");
        let _note_cache_guard = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        let notes_dir = workspace.root().join("notes");
        std::fs::create_dir_all(&notes_dir).expect("create owned notes directory");
        let initial_content = "# Current\n\nOriginal body";
        std::fs::write(notes_dir.join("current.md"), initial_content)
            .expect("write owned current note");
        std::fs::write(
            notes_dir.join("other.md"),
            "# Other\n\n[[Current]]\n@note:current",
        )
        .expect("write owned related and mentioning note");
        let actions_path = workspace.root().join("actions.json");
        std::fs::write(&actions_path, b"[]").expect("write owned Actions watcher target");
        crate::plugins::note::refresh_cache().expect("publish owned note files");
        let (initial_revision, initial_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("owned initial cache is healthy");
        let current = initial_notes
            .iter()
            .find(|note| note.slug == "current")
            .expect("current note loaded from the owned workspace")
            .clone();
        assert_eq!(current.path, notes_dir.join("current.md"));

        let todos = vec![TodoEntry {
            id: "todo-close-save".into(),
            text: "Synthetic TODO linked to Current".into(),
            done: false,
            priority: 0,
            tags: Vec::new(),
            entity_refs: vec![EntityRef::new(
                EntityKind::Note,
                "current",
                Some("Current".into()),
            )],
        }];
        let _todo_guard = TrackC1ATodoSnapshotGuard::publish(TODO_FILE, &todos);

        let context = egui::Context::default();
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.enable_toasts = false;
        settings.show_error_toasts = false;
        settings.dashboard.enabled = false;
        settings.enabled_plugins = Some(HashSet::new());
        settings.enabled_capabilities = Some(HashMap::new());
        settings.plugin_dirs = Some(Vec::new());
        settings.index_paths = Some(Vec::new());
        settings.note.backlinks_enabled = true;
        settings.note.collapsed_sections_persist = false;
        let settings_path = workspace.root().join("settings.json");

        let release_probe = HandlerReleaseProbe::begin(workspace.root());
        workspace.callbacks_may_use_paths();
        let mut app = LauncherApp::new(
            &context,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            actions_path.to_string_lossy().into_owned(),
            settings_path.to_string_lossy().into_owned(),
            settings,
            Some(Vec::new()),
            Some(Vec::new()),
            Some(HashSet::new()),
            Some(HashMap::new()),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        app.note_save_on_close = true;
        app.note_always_overwrite = true;

        let index_root = workspace.root().join("blocked-initial-index");
        std::fs::create_dir_all(&index_root).expect("create owned index root");
        let config = IndexConfig::new(vec![index_root.to_string_lossy().into_owned()], Some(1));
        let expected_indexed = crate::actions::Action {
            label: "Released indexed action".into(),
            desc: "Synthetic scanner result".into(),
            action: "index:released-action".into(),
            args: Some("after-release".into()),
        };
        let scanner_config = config.clone();
        let scanner_action = expected_indexed.clone();
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let release_rx = Arc::new(Mutex::new(release_rx));
        let coordinator = IndexCoordinator::with_test_scanner(move |actual_config| {
            if actual_config != &scanner_config {
                return Err("initial scanner received an unexpected config".to_owned());
            }
            entered_tx
                .send(())
                .map_err(|_| "initial scan owner left before scanner entry".to_owned())?;
            release_rx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv_timeout(DEADLINE)
                .map_err(|error| format!("initial scan release deadline elapsed: {error}"))?;
            Ok(vec![scanner_action.clone()])
        })
        .expect("channel-gated initial coordinator starts");
        // Keep the release guard newer than the coordinator and app so an
        // unwind opens the scanner gate before either owner is dropped.
        let mut scanner_release = TrackC3FScannerReleaseGuard(Some(release_tx));
        app.install_test_index_coordinator(coordinator);
        app.request_index_config(config.clone());
        let generation = app
            .indexing
            .expected_generation
            .expect("the requested initial scan is accepted");
        assert_eq!(app.indexing.desired, config);

        entered_rx
            .recv_timeout(DEADLINE)
            .expect("the actual initial scanner reaches its release gate");
        let blocked_actions = Arc::clone(&app.actions);
        assert!(blocked_actions.is_empty());
        assert!(Arc::ptr_eq(&app.actions, &blocked_actions));

        let draft = format!("{initial_content}\n\n@todo:todo-close-save");
        let mut panel = NotePanel::from_note(current);
        panel.replace_content_from_mutation(draft.clone(), 10.0);
        app.push_note_panel(panel);
        assert!(
            app.close_front_dialog(),
            "the note panel is closed and saved"
        );

        let saved_path = notes_dir.join("current.md");
        assert_eq!(std::fs::read_to_string(&saved_path).unwrap(), draft);
        let (saved_revision, saved_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("save publishes a healthy current note snapshot");
        assert_ne!(saved_revision, initial_revision);
        let saved_current = saved_notes
            .iter()
            .find(|note| note.slug == "current")
            .expect("saved note remains in the canonical cache");
        assert_eq!(saved_current.content, draft);
        assert_eq!(
            saved_current.entity_refs,
            vec![EntityRef::new(EntityKind::Todo, "todo-close-save", None,)]
        );
        let saved_other = saved_notes
            .iter()
            .find(|note| note.slug == "other")
            .expect("the related note remains in the canonical cache");
        assert!(saved_other.links.iter().any(|link| link == "current"));
        assert!(
            saved_other.entity_refs.iter().any(|reference| {
                reference.kind == EntityKind::Note && reference.id == "current"
            })
        );

        let tabs = [
            BacklinkTab::LinkedTodos,
            BacklinkTab::RelatedNotes,
            BacklinkTab::Mentions,
        ];
        let expected_rows = tabs
            .into_iter()
            .map(|tab| {
                backlink_rows_signature(&track_c_eager_note_rows(
                    saved_current,
                    tab,
                    &todos,
                    &saved_notes,
                ))
            })
            .collect::<Vec<_>>();
        assert!(expected_rows.iter().all(|rows| !rows.is_empty()));
        let mut refreshed_panel = NotePanel::from_note(saved_current.clone());
        refreshed_panel.last_backlinks_enabled = None;
        refreshed_panel.maybe_refresh_heavy_derived(&context, true);
        let actual_rows = [
            &refreshed_panel.derived.backlink_rows_linked_todos,
            &refreshed_panel.derived.backlink_rows_related_notes,
            &refreshed_panel.derived.backlink_rows_mentions,
        ]
        .into_iter()
        .map(|rows| backlink_rows_signature(rows))
        .collect::<Vec<_>>();
        assert_eq!(actual_rows, expected_rows);

        assert!(Arc::ptr_eq(&app.actions, &blocked_actions));
        assert_eq!(app.indexing.expected_generation, Some(generation));
        assert_eq!(app.indexing.desired, config);

        scanner_release
            .release()
            .expect("the gated initial scanner is released exactly once");
        let completion = app
            .wait_for_index_completion_for_test(generation)
            .expect("the accepted scan completes after release");
        assert_eq!(completion.generation(), generation);
        assert_eq!(completion.config(), &config);
        assert_eq!(
            completion
                .outcome()
                .as_ref()
                .expect("scanner succeeds")
                .as_slice(),
            std::slice::from_ref(&expected_indexed)
        );

        let deadline = Instant::now() + DEADLINE;
        while app.actions.as_slice() != std::slice::from_ref(&expected_indexed) {
            app.process_watch_events();
            assert!(
                Instant::now() < deadline,
                "the app processes its original IndexReady envelope before the deadline"
            );
            std::thread::yield_now();
        }
        assert_eq!(app.indexing.expected_generation, Some(generation));
        assert_eq!(app.indexing.desired, config);
        assert_eq!(
            app.actions.as_slice(),
            std::slice::from_ref(&expected_indexed)
        );
        assert_eq!(app.custom_len, 0);
        assert!(matches!(
            app.wait_for_index_completion_for_test(generation),
            Err(crate::indexer::coordinator::CoordinatorError::ResultAlreadyAcknowledged(
                acknowledged_generation
            )) if acknowledged_generation == generation
        ));

        drop(app);
        let mut release_report = release_probe.wait(Duration::from_secs(5));
        let expected_handlers = [
            HandlerKind::ClipboardModify,
            HandlerKind::GuiActions,
            HandlerKind::GuiNotes,
            HandlerKind::GuiTodos,
            HandlerKind::JsonDashboard,
        ]
        .to_vec();
        release_report.attempts.sort_by_key(|kind| kind.token());
        release_report.created.sort_by_key(|kind| kind.token());
        release_report.released.sort_by_key(|kind| kind.token());
        let exact_inventory = release_report.attempts == expected_handlers
            && release_report.created == expected_handlers
            && release_report.released == expected_handlers;
        release_report.complete &= exact_inventory;
        assert!(
            release_report.complete && !release_report.overflowed,
            "all five fixture-owned watcher callback closures release before workspace teardown"
        );
        workspace.callbacks_released(&release_report);
    }

    #[test]
    #[ignore = "opt-in C1-B fused note refresh attribution; synthetic owned workspace only"]
    fn track_c_c1b_benchmark_open_note_panel_refresh_attribution() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, MEASURED_SAMPLES, WARMUPS, measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C benchmark mode");
        let mut scenarios = mode
            .note_sizes()
            .iter()
            .map(|count| (*count, TrackC1ANoteTransition::LocalDraft))
            .collect::<Vec<_>>();
        let persisted_count = if mode == BenchmarkMode::Small {
            100
        } else {
            1_000
        };
        scenarios.extend([
            (
                persisted_count,
                TrackC1ANoteTransition::ExternalNoteRevision,
            ),
            (
                persisted_count,
                TrackC1ANoteTransition::PersistedTodoRevision,
            ),
        ]);

        for (item_count, transition) in scenarios {
            let fixture_signature = track_c1a_fixture_signature(item_count, transition);
            let validation_index = std::cell::Cell::new(0_usize);
            let details = std::cell::RefCell::new(Vec::with_capacity(MEASURED_SAMPLES));
            let samples = measure_owner(
                || {
                    let state = TrackC1ANoteUiOwnerState::new(item_count, transition);
                    assert_eq!(state.fixture_signature, fixture_signature);
                    state
                },
                |state| {
                    let time = state.owner_time();
                    track_c1a_run_frame(&state.context, &mut state.panel, &mut state.app, time)
                },
                |state, output| {
                    let current_index = validation_index.get();
                    validation_index.set(current_index + 1);
                    let (observation, detail) = track_c1a_validate_owner(state, output);
                    if current_index >= WARMUPS {
                        details.borrow_mut().push(detail);
                    }
                    observation
                },
            );
            let details = details.into_inner();
            assert_eq!(validation_index.get(), WARMUPS + MEASURED_SAMPLES);
            track_c1b_emit_samples(
                mode,
                item_count,
                transition,
                fixture_signature,
                &samples,
                &details,
            );
        }
    }

    fn track_c_note_inputs(
        count: usize,
    ) -> (
        crate::performance::workloads::Fixture<Note>,
        Vec<crate::plugins::todo::TodoEntry>,
    ) {
        use crate::common::entity_ref::{EntityKind, EntityRef};
        use crate::plugins::todo::TodoEntry;

        let mut fixture = crate::performance::track_c_workloads::note_fixture(count);
        let current_slug = "synthetic-note-00000";
        if count > 1 {
            let note = &mut fixture.values[1];
            // Preserve the seeded multi-kilobyte body and place a real Unicode
            // snippet inside the selected 44-byte window at valid byte
            // boundaries. The known invalid-boundary case is separate below.
            let prefix = format!("[[{current_slug}]]\n東京{}", "u".repeat(35));
            note.content = format!("{prefix}\n{}", note.content);
        }
        if count > 2 {
            let fenced_only = &mut fixture.values[2];
            // This row contains the current-note mention needle only inside a
            // fenced block. Keep wiki syntax out of this case: NoteCache
            // intentionally resolves wiki links from raw content before the
            // panel's prose/fence filtering boundary.
            fenced_only
                .content
                .push_str(&format!("\n```text\n@note:{current_slug}\n```\n"));
        }
        fixture.summary.signature =
            crate::performance::track_c_workloads::note_fixture_identity(&fixture.values);
        let todos = (0..12)
            .map(|index| {
                let linked = index % 3 == 0;
                TodoEntry {
                    id: format!("todo-{index:02}"),
                    text: if linked {
                        format!("Synthetic task {index:02} for @note:{current_slug}")
                    } else {
                        format!("Synthetic unrelated task {index:02}")
                    },
                    done: index % 4 == 0,
                    priority: (index % 5) as u8,
                    tags: vec![format!("work-{:02}", index % 3)],
                    entity_refs: linked
                        .then(|| {
                            vec![EntityRef::new(
                                EntityKind::Note,
                                current_slug,
                                Some("Synthetic Unicode title 東京".into()),
                            )]
                        })
                        .unwrap_or_default(),
                }
            })
            .collect();
        (fixture, todos)
    }

    fn track_c_note_and_todo_fixture_identity(
        notes: &[Note],
        todos: &[crate::plugins::todo::TodoEntry],
    ) -> u64 {
        use crate::common::entity_ref::EntityKind;
        use crate::performance::track_c_workloads::StableSignature;

        let mut signature = StableSignature::new(
            0x435F_4E4F_5445,
            "track-c-note-and-todo-relations",
            notes.len().saturating_add(todos.len()),
        );
        signature.number(crate::performance::track_c_workloads::note_fixture_identity(notes));
        signature.number(todos.len() as u64);
        for todo in todos {
            signature.number(todo.id.len() as u64);
            signature.bytes(todo.id.as_bytes());
            signature.number(todo.text.len() as u64);
            signature.bytes(todo.text.as_bytes());
            signature.number(todo.done as u64);
            signature.number(todo.priority as u64);
            signature.number(todo.tags.len() as u64);
            for tag in &todo.tags {
                signature.number(tag.len() as u64);
                signature.bytes(tag.as_bytes());
            }
            signature.number(todo.entity_refs.len() as u64);
            for reference in &todo.entity_refs {
                signature.number(match reference.kind {
                    EntityKind::Note => 0,
                    EntityKind::Todo => 1,
                    EntityKind::Event => 2,
                });
                signature.number(reference.id.len() as u64);
                signature.bytes(reference.id.as_bytes());
                if let Some(title) = &reference.title {
                    signature.number(1);
                    signature.number(title.len() as u64);
                    signature.bytes(title.as_bytes());
                } else {
                    signature.number(0);
                }
            }
        }
        signature.finish()
    }

    fn track_c_make_note_owner_state(
        notes: &[Note],
        todos: &[crate::plugins::todo::TodoEntry],
    ) -> TrackCNoteOwnerState {
        let cache_guard = crate::plugins::note::publish_note_cache_for_test(notes.to_vec());
        let (expected_note_revision, snapshot_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("synthetic note cache snapshot is healthy");
        assert_eq!(snapshot_notes.len(), notes.len());
        assert_eq!(
            snapshot_notes
                .iter()
                .map(|note| &note.slug)
                .collect::<Vec<_>>(),
            notes.iter().map(|note| &note.slug).collect::<Vec<_>>(),
            "cache publication retains the fixture's ordered note identities"
        );
        let mut panel = NotePanel::from_note(snapshot_notes[0].clone());
        // Force one revision/settings-driven owner recompute regardless of
        // whether this fixture happens to match a process's original cache.
        panel.last_backlinks_enabled = None;
        let expected_todo_revision = crate::plugins::todo::todo_version();
        let current = &snapshot_notes[0];
        let expected_rows = [
            BacklinkTab::LinkedTodos,
            BacklinkTab::RelatedNotes,
            BacklinkTab::Mentions,
        ]
        .into_iter()
        .map(|tab| {
            backlink_rows_signature(&track_c_eager_note_rows(
                current,
                tab,
                todos,
                &snapshot_notes,
            ))
        })
        .collect::<Vec<_>>();
        let expected_labels = todos
            .iter()
            .filter(|todo| !todo.id.is_empty())
            .map(|todo| (todo.id.clone(), todo.text.clone()))
            .collect::<HashMap<_, _>>();
        crate::performance::track_c::reset();
        TrackCNoteOwnerState {
            context: egui::Context::default(),
            panel,
            _cache_guard: cache_guard,
            expected_rows,
            expected_labels,
            expected_note_count: snapshot_notes.len(),
            expected_todo_count: todos.len(),
            expected_note_revision,
            expected_todo_revision,
        }
    }

    fn track_c_validate_note_owner(
        state: &TrackCNoteOwnerState,
        require_metrics: bool,
    ) -> crate::performance::track_c_workloads::OwnerObservation {
        use crate::performance::track_c::{self, Phase};
        use crate::performance::track_c_workloads::OwnerObservation;

        let actual_rows = [
            &state.panel.derived.backlink_rows_linked_todos,
            &state.panel.derived.backlink_rows_related_notes,
            &state.panel.derived.backlink_rows_mentions,
        ]
        .into_iter()
        .map(|rows| backlink_rows_signature(rows))
        .collect::<Vec<_>>();
        assert_eq!(
            actual_rows, state.expected_rows,
            "all row fields/order match frozen eager reference"
        );
        assert_eq!(state.panel.derived.todo_label_map, state.expected_labels);
        assert_eq!(state.panel.last_notes_version, state.expected_note_revision);
        assert_eq!(state.panel.last_todo_revision, state.expected_todo_revision);

        let structure = track_c_backlink_rows_identity(&actual_rows, &state.expected_labels);
        let mut complete = crate::performance::track_c_workloads::StableSignature::new(
            0x435F_4E4F_5445,
            "track-c-note-complete-state",
            3,
        );
        complete.number(structure);
        complete.number(state.expected_note_revision);
        complete.number(state.expected_todo_revision);
        let refresh = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == Phase::NoteRelationshipRefresh)
            .expect("note relationship phase is fixed-cardinality")
            .1;
        let mentions = track_c::snapshot()
            .into_iter()
            .find(|(phase, _)| *phase == Phase::NoteMentionsScan)
            .expect("note mentions phase is fixed-cardinality")
            .1;
        if require_metrics {
            assert_eq!(refresh.calls, 1, "actual heavy-refresh owner ran once");
            assert_eq!(refresh.completed, 1);
            assert_eq!(mentions.calls, 1);
            let expected_visits = state
                .expected_note_count
                .saturating_add(state.expected_todo_count) as u64;
            assert_eq!(refresh.work_units, expected_visits);
            assert_eq!(mentions.work_units, expected_visits);
        }
        OwnerObservation {
            output_identity: complete.finish(),
            structural_signature: structure,
            revision_receipts: vec![state.expected_note_revision, state.expected_todo_revision],
            viewport_receipt: None,
            work_units: refresh.work_units,
            work_counters: vec![
                refresh.calls,
                refresh.work_units,
                mentions.calls,
                mentions.work_units,
            ],
        }
    }

    #[test]
    fn track_c_oracle_note_refresh_matches_frozen_full_rows_and_revisions() {
        use crate::performance::track_c_workloads::TrackCWorkspace;

        let workspace = TrackCWorkspace::new();
        let (fixture, todos) = track_c_note_inputs(100);
        std::fs::write(
            crate::plugins::todo::TODO_FILE,
            serde_json::to_vec(&todos).expect("serialize synthetic todo data"),
        )
        .expect("write isolated todo data");
        let mut state = track_c_make_note_owner_state(&fixture.values, &todos);
        state
            .panel
            .maybe_refresh_heavy_derived(&state.context, true);
        let observation = track_c_validate_note_owner(&state, false);
        assert!(observation.output_identity != 0);
        assert_eq!(observation.revision_receipts.len(), 2);
        assert!(
            state.expected_rows[1].iter().any(|row| {
                row.5.as_deref() == Some("synthetic-note-00001") && row.3.contains("東京")
            }),
            "the real related-note snippet includes Unicode body text"
        );
        assert!(
            state.expected_rows.iter().flatten().all(|row| {
                row.5.as_deref() != Some("synthetic-note-00002")
                    && row.5.as_deref() != Some("synthetic-note-00003")
            }),
            "fenced-only and unrelated rows remain excluded"
        );
        assert!(fixture.values[0].content.len() > 4_000);
        assert!(fixture.values[28].content.len() > 32_000);
        let fixture_signature = track_c_note_and_todo_fixture_identity(&fixture.values, &todos);
        let mut changed_todos = todos.clone();
        changed_todos[0].tags.push("changed-input-field".into());
        assert_ne!(
            fixture_signature,
            track_c_note_and_todo_fixture_identity(&fixture.values, &changed_todos),
            "the report identity includes complete todo inputs"
        );
        drop(state);
        drop(workspace);
    }

    #[test]
    fn prepared_note_projection_matches_frozen_eager_categories_and_fresh_metadata() {
        use crate::common::entity_ref::{EntityKind, EntityRef};
        use crate::performance::track_c_workloads::TrackCWorkspace;
        use crate::plugins::todo::TodoEntry;

        let workspace = TrackCWorkspace::new();
        let _notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes env lock");
        let notes_dir = workspace.notes_dir();
        std::fs::create_dir_all(&notes_dir).expect("create owned notes directory");
        let note = |slug: &str, title: &str, content: &str| Note {
            title: title.into(),
            path: notes_dir.join(format!("{slug}.md")),
            content: content.into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        };

        let current = note("central", "Central", "# Central\n");
        let fenced_wiki = note(
            "fenced-wiki",
            "Fenced canonical wiki",
            "```md\n[[Central]]\n```\n",
        );
        let mut typed = note("typed-source", "Typed reference", "Typed only");
        typed.entity_refs = vec![EntityRef::new(EntityKind::Note, "central", None)];
        let suffix = note(
            "suffix-source",
            "Prefix substring",
            "A literal @note:central-suffix occurrence",
        );
        let duplicate = note(
            "duplicate-source",
            "Duplicate source",
            "An alias mention [[Fresh Alias]]",
        );
        let fenced_prose = note(
            "fenced-prose",
            "Fenced prose only",
            "```text\n@note:central\n```\n",
        );
        let fresh_alias = note(
            "fresh-alias-source",
            "Fresh draft alias",
            "The new draft needle [[Fresh Alias]]",
        );
        let source_notes = vec![
            current.clone(),
            fenced_wiki.clone(),
            typed,
            suffix,
            duplicate.clone(),
            duplicate,
            fenced_prose,
            fresh_alias,
        ];
        std::fs::write(&fenced_wiki.path, &fenced_wiki.content)
            .expect("write the metadata fixture under the owned workspace");
        let _cache_guard = crate::plugins::note::publish_note_cache_for_test(source_notes.clone());
        let (note_revision, canonical_notes) =
            crate::plugins::note::note_cache_snapshot_with_version()
                .expect("canonical projection fixture snapshot is healthy");
        assert_eq!(canonical_notes.len(), source_notes.len());
        assert_eq!(canonical_notes[4], canonical_notes[5]);

        let mut draft = canonical_notes[0].clone();
        draft.title = "Fresh Title".into();
        draft.alias = Some("Fresh Alias".into());
        draft.content = "# Fresh Title\n\n@todo:draft-outgoing".into();
        let todos = vec![
            TodoEntry {
                id: "todo-typed".into(),
                text: "TODO with a competing [[Fresh Title]] needle".into(),
                done: false,
                priority: 0,
                tags: Vec::new(),
                entity_refs: vec![EntityRef::new(EntityKind::Note, "central", None)],
            },
            TodoEntry {
                id: "todo-needle".into(),
                text: "TODO for [[Fresh Alias]]".into(),
                done: false,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
            TodoEntry {
                id: "draft-outgoing".into(),
                text: "TODO mentioned by the unsaved draft".into(),
                done: false,
                priority: 0,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            },
        ];
        let projection = TrackCPreparedNoteProjection::prepare(&canonical_notes);

        let assert_matches_frozen = |projection: &TrackCPreparedNoteProjection| {
            let replay = projection.replay(&draft, &todos);
            let expected = [
                BacklinkTab::LinkedTodos,
                BacklinkTab::RelatedNotes,
                BacklinkTab::Mentions,
            ]
            .into_iter()
            .map(|tab| track_c_eager_note_rows(&draft, tab, &todos, &canonical_notes))
            .collect::<Vec<_>>();
            assert_eq!(
                track_c_prepared_rows_signature(&replay.rows),
                expected
                    .iter()
                    .map(|rows| backlink_rows_signature(rows))
                    .collect::<Vec<_>>(),
                "all seven row fields and ordered categories match the frozen eager oracle"
            );
            assert_eq!(
                replay.todo_label_map,
                track_c1a_labels_for(&todos),
                "TODO label projection matches the current input"
            );
            assert_eq!(replay.work.todo_visits, todos.len());
            assert_eq!(replay.work.note_visits, canonical_notes.len());
            replay
        };

        let first = assert_matches_frozen(&projection);
        assert_eq!(
            first.rows[1]
                .iter()
                .filter_map(|row| row.note_slug.as_deref())
                .collect::<Vec<_>>(),
            [
                "fenced-wiki",
                "duplicate-source",
                "duplicate-source",
                "fresh-alias-source"
            ],
            "raw fenced wiki metadata wins and duplicate source positions/order survive"
        );
        assert!(
            first.rows[1]
                .iter()
                .all(|row| row.note_slug.as_deref() != Some("fenced-prose"))
        );
        assert!(first.rows[2].iter().any(|row| {
            row.note_slug.as_deref() == Some("suffix-source") && row.reason == "note mention"
        }));
        assert!(first.rows[2].iter().any(|row| {
            row.todo_id.as_deref() == Some("draft-outgoing") && row.reason == "note mentions todo"
        }));
        assert_eq!(
            first.rows[0]
                .iter()
                .map(|row| row.todo_id.as_deref().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["todo-typed", "todo-needle", "draft-outgoing"]
        );

        let with_metadata = first.rows[1]
            .iter()
            .find(|row| row.note_slug.as_deref() == Some("fenced-wiki"))
            .expect("canonical wiki row is present")
            .updated
            .clone();
        assert_ne!(with_metadata, "unknown");
        std::fs::remove_file(&fenced_wiki.path)
            .expect("remove only the owned metadata fixture file");
        let after_file_removal = assert_matches_frozen(&projection);
        let without_metadata = after_file_removal.rows[1]
            .iter()
            .find(|row| row.note_slug.as_deref() == Some("fenced-wiki"))
            .expect("canonical wiki row remains after metadata disappears")
            .updated
            .clone();
        assert_eq!(without_metadata, "unknown");
        assert_eq!(note_revision, crate::plugins::note::note_version());
    }

    #[test]
    #[ignore = "opt-in C1-C prepared projection viability benchmark; isolated synthetic notes only"]
    fn track_c_c1c_benchmark_prepared_note_projection_viability() {
        use crate::performance::track_c;
        use crate::performance::track_c_workloads::{BenchmarkMode, MEASURED_SAMPLES, WARMUPS};
        use crate::plugins::note::build_note_cache_candidate_for_test;

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C benchmark mode");
        let mut scenarios = mode
            .note_sizes()
            .iter()
            .map(|count| (*count, TrackC1ANoteTransition::LocalDraft))
            .collect::<Vec<_>>();
        let persisted_count = if mode == BenchmarkMode::Small {
            100
        } else {
            1_000
        };
        scenarios.extend([
            (
                persisted_count,
                TrackC1ANoteTransition::ExternalNoteRevision,
            ),
            (
                persisted_count,
                TrackC1ANoteTransition::PersistedTodoRevision,
            ),
        ]);

        for (item_count, transition) in scenarios {
            let fixture_signature = track_c1a_fixture_signature(item_count, transition);
            let mut samples = Vec::with_capacity(MEASURED_SAMPLES);
            let mut memory_summary = None;
            for iteration in 0..WARMUPS + MEASURED_SAMPLES {
                let mut state = TrackC1ANoteUiOwnerState::new(item_count, transition);
                assert_eq!(state.fixture_signature, fixture_signature);

                // Exercise and validate the actual dialog caller outside the
                // direct owner timing; it also settles the same canonical
                // snapshot, draft, and inert settings used below.
                let frame_time = state.owner_time();
                let frame = track_c1a_run_frame(
                    &state.context,
                    &mut state.panel,
                    &mut state.app,
                    frame_time,
                );
                let _ = track_c1a_validate_owner(&mut state, &frame);
                drop(frame);

                // The C1-A observer is disabled for both owner paths and all
                // projection preparation/publication timings. Track C's
                // bounded metric timers remain identically enabled.
                let _observer_pause = TrackC1CObserverPause::new();
                track_c::reset();
                crate::performance::reset_metrics();
                let (snapshot_revision, canonical_notes) =
                    crate::plugins::note::note_cache_snapshot_with_version()
                        .expect("C1-C sample uses a healthy canonical snapshot");
                assert_eq!(snapshot_revision, state.expected_note_revision);
                assert_eq!(canonical_notes, state.expected_notes);
                assert_eq!(todo_version(), state.expected_todo_revision);

                let projection_cache = build_note_cache_candidate_for_test(canonical_notes.clone());
                assert_eq!(projection_cache.notes, canonical_notes);
                let preparation_started = Instant::now();
                let projection = TrackCPreparedNoteProjection::prepare(&projection_cache.notes);
                let projection_prepare_ns = preparation_started
                    .elapsed()
                    .as_nanos()
                    .min(u64::MAX as u128) as u64;
                let footprint = projection.footprint();
                let projection_bundle = TrackC1CProjectionBundle {
                    cache: projection_cache,
                    projection,
                };
                let projection = &projection_bundle.projection;

                // Alternate first-run order, then alternate each member of
                // the five forced-refresh repeats to reduce systematic warm
                // cache ordering bias. Every complete output is checked after
                // its individual timer stops.
                let (current_first, projection_first) = if iteration % 2 == 0 {
                    (
                        track_c1c_run_current_owner(&mut state),
                        track_c1c_run_projection_owner(&mut state, projection),
                    )
                } else {
                    let projected = track_c1c_run_projection_owner(&mut state, projection);
                    let current = track_c1c_run_current_owner(&mut state);
                    (current, projected)
                };
                assert_eq!(
                    current_first.complete_output_id, projection_first.complete_output_id,
                    "fused and prepared owners produce the same complete output state"
                );
                assert_eq!(
                    current_first.structural_output_id,
                    projection_first.structural_output_id
                );

                let mut current_repeat5_ns = 0_u64;
                let mut projection_repeat5_ns = 0_u64;
                let mut replay_work = projection_first.work;
                for repeat in 0..5 {
                    let (current, projected) = if (repeat + iteration) % 2 == 0 {
                        (
                            track_c1c_run_current_owner(&mut state),
                            track_c1c_run_projection_owner(&mut state, projection),
                        )
                    } else {
                        let projected = track_c1c_run_projection_owner(&mut state, projection);
                        let current = track_c1c_run_current_owner(&mut state);
                        (current, projected)
                    };
                    assert_eq!(current.complete_output_id, current_first.complete_output_id);
                    assert_eq!(
                        projected.complete_output_id,
                        current_first.complete_output_id
                    );
                    current_repeat5_ns = current_repeat5_ns.saturating_add(current.elapsed_ns);
                    projection_repeat5_ns =
                        projection_repeat5_ns.saturating_add(projected.elapsed_ns);
                    replay_work = projected.work;
                }
                assert_eq!(replay_work.note_visits, canonical_notes.len());
                assert_eq!(replay_work.todo_visits, state.expected_todos.len());
                assert_eq!(replay_work.linked_todo_rows, state.expected_rows[0].len());
                assert_eq!(replay_work.related_note_rows, state.expected_rows[1].len());
                assert_eq!(replay_work.mention_rows, state.expected_rows[2].len());
                assert_eq!(
                    replay_work.snippet_calls,
                    replay_work
                        .related_note_rows
                        .saturating_add(replay_work.mention_rows),
                    "TODO rows are cloned into Mentions, but each snippet is constructed once"
                );

                // Measure the existing cheap unchanged panel gate separately;
                // it takes no canonical snapshot or relationship traversal.
                track_c::reset();
                crate::performance::reset_metrics();
                let unchanged_started = Instant::now();
                state
                    .panel
                    .maybe_refresh_heavy_derived(&state.context, true);
                let unchanged_gate_ns =
                    unchanged_started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                let unchanged_receipt = track_c1c_validate_panel(&state);
                assert_eq!(
                    unchanged_receipt.complete_output_id,
                    current_first.complete_output_id
                );
                track_c1c_assert_unchanged_gate_has_no_heavy_work();

                // Equal/changed canonical transaction measurements are a
                // standalone local publication model, not costs charged to
                // this draft/TODO transition's refresh latency.
                let publication = track_c1c_measure_local_publication(
                    &canonical_notes,
                    &state.panel.note,
                    &state.expected_todos,
                );
                let current_id = current_first.complete_output_id;
                let memory_now = (
                    footprint,
                    publication.equal_retained_logical_bytes,
                    publication.equal_retained_capacity_bytes,
                    publication.changed_old_logical_bytes,
                    publication.changed_old_capacity_bytes,
                    publication.changed_peak_logical_bytes,
                    publication.changed_peak_capacity_bytes,
                );
                if let Some(previous) = memory_summary {
                    assert_eq!(
                        previous, memory_now,
                        "the same synthetic fixture has stable logical/capacity receipts"
                    );
                } else {
                    memory_summary = Some(memory_now);
                }

                if iteration >= WARMUPS {
                    let equal_break_even_k = track_c1c_break_even_k(
                        publication.equal_current_ns,
                        publication.equal_projection_ns,
                        current_first.elapsed_ns,
                        projection_first.elapsed_ns,
                    );
                    let changed_break_even_k = track_c1c_break_even_k(
                        publication.changed_current_ns,
                        publication.changed_projection_ns,
                        current_first.elapsed_ns,
                        projection_first.elapsed_ns,
                    );
                    samples.push(TrackC1CSample {
                        current_owner_ns: current_first.elapsed_ns,
                        projection_owner_ns: projection_first.elapsed_ns,
                        projection_prepare_ns,
                        current_repeat5_ns,
                        projection_repeat5_ns,
                        equal_current_publish_ns: publication.equal_current_ns,
                        equal_projection_publish_ns: publication.equal_projection_ns,
                        changed_current_publish_ns: publication.changed_current_ns,
                        changed_projection_publish_ns: publication.changed_projection_ns,
                        unchanged_gate_ns,
                        canonical_posting_entries: replay_work
                            .canonical_posting_entries
                            .min(u64::MAX as usize)
                            as u64,
                        typed_posting_entries: replay_work
                            .typed_posting_entries
                            .min(u64::MAX as usize)
                            as u64,
                        fallback_visits: replay_work.fallback_visits.min(u64::MAX as usize) as u64,
                        fallback_input_bytes: replay_work
                            .fallback_input_bytes
                            .min(u64::MAX as usize)
                            as u64,
                        linked_todo_rows: replay_work.linked_todo_rows.min(u64::MAX as usize)
                            as u64,
                        related_note_rows: replay_work.related_note_rows.min(u64::MAX as usize)
                            as u64,
                        mention_rows: replay_work.mention_rows.min(u64::MAX as usize) as u64,
                        snippet_calls: replay_work.snippet_calls.min(u64::MAX as usize) as u64,
                        metadata_reads: replay_work.metadata_reads.min(u64::MAX as usize) as u64,
                        complete_output_id: current_id,
                        structural_output_id: current_first.structural_output_id,
                        note_revision: current_first.note_revision,
                        todo_revision: current_first.todo_revision,
                        equal_break_even_k,
                        changed_break_even_k,
                    });
                }
            }

            let (
                footprint,
                equal_retained_logical_bytes,
                equal_retained_capacity_bytes,
                changed_old_logical_bytes,
                changed_old_capacity_bytes,
                changed_peak_logical_bytes,
                changed_peak_capacity_bytes,
            ) = memory_summary.expect("warmups and samples produced a memory receipt");
            track_c1c_emit_samples(
                mode,
                item_count,
                transition,
                fixture_signature,
                &samples,
                footprint,
                equal_retained_logical_bytes,
                equal_retained_capacity_bytes,
                changed_old_logical_bytes,
                changed_old_capacity_bytes,
                changed_peak_logical_bytes,
                changed_peak_capacity_bytes,
            );
        }
    }

    #[test]
    fn track_c_oracle_records_existing_utf8_snippet_boundary_panic() {
        let content = format!("界{}@note:synthetic-note-00000", "a".repeat(43));
        let result = std::panic::catch_unwind(|| {
            extract_snippet_around(&content, "@note:synthetic-note-00000")
        });
        assert!(
            result.is_err(),
            "this input places WINDOW inside 界's UTF-8 bytes"
        );
    }

    #[test]
    #[ignore = "opt-in Track C owner benchmark; isolated synthetic notes only"]
    fn track_c_benchmark_note_panel_heavy_refresh_owner() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, ReportMetadata, TrackCWorkspace, emit_samples, measure_owner,
        };

        assert!(crate::performance::enabled());
        let mode = BenchmarkMode::from_process_env().expect("valid Track C mode");
        let workspace = TrackCWorkspace::new();
        for count in mode.note_sizes() {
            let (fixture, todos) = track_c_note_inputs(*count);
            std::fs::write(
                crate::plugins::todo::TODO_FILE,
                serde_json::to_vec(&todos).expect("serialize synthetic todo data"),
            )
            .expect("write isolated todo data");
            let fixture_signature = track_c_note_and_todo_fixture_identity(&fixture.values, &todos);
            let samples = measure_owner(
                || track_c_make_note_owner_state(&fixture.values, &todos),
                |state| {
                    state
                        .panel
                        .maybe_refresh_heavy_derived(&state.context, true)
                },
                |state, ()| track_c_validate_note_owner(state, true),
            );
            emit_samples(
                ReportMetadata {
                    owner: "NotePanel::maybe_refresh_heavy_derived",
                    fixture_name: "synthetic-note-and-todo-relations",
                    fixture_signature,
                    item_count: *count,
                    viewport: "not-rendered",
                    scale_milli: 1_000,
                    font_state: "not-applicable",
                    settings: "backlinks-enabled-inert",
                    cold_type: "note-cache-revision-refresh",
                    mode,
                },
                &samples,
            );
        }
        drop(workspace);
    }

    #[test]
    fn note_editor_timestamp_formats_milliseconds_exactly() {
        let offset = FixedOffset::east_opt(5 * 60 * 60 + 30 * 60).expect("valid fixed offset");
        let timestamp = offset
            .with_ymd_and_hms(2026, 8, 8, 20, 25, 12)
            .single()
            .expect("valid fixed datetime")
            .with_nanosecond(347_000_000)
            .expect("valid nanoseconds");

        assert_eq!(
            format_note_editor_timestamp(timestamp),
            "20260808-202512.347"
        );
    }

    #[test]
    fn note_editor_timestamp_zero_pads_single_digit_milliseconds() {
        let timestamp = FixedOffset::west_opt(7 * 60 * 60)
            .expect("valid fixed offset")
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("valid fixed datetime")
            .with_nanosecond(7_000_000)
            .expect("valid nanoseconds");

        assert_eq!(
            format_note_editor_timestamp(timestamp),
            "20260102-030405.007"
        );
    }

    #[test]
    fn note_editor_timestamp_zero_pads_zero_milliseconds() {
        let timestamp = FixedOffset::east_opt(0)
            .expect("valid fixed offset")
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("valid fixed datetime")
            .with_nanosecond(0)
            .expect("valid nanoseconds");

        assert_eq!(
            format_note_editor_timestamp(timestamp),
            "20260102-030405.000"
        );
    }

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

    #[test]
    #[ignore = "opt-in Track A workload benchmark; set MULTI_LAUNCHER_PERF=1 before the process"]
    fn track_a_benchmark_note_refresh_check_owner() {
        use crate::performance::{Metric, workloads};
        use std::cell::Cell;

        let workspace = workloads::IsolatedWorkspace::new();
        std::fs::write(crate::plugins::todo::TODO_FILE, b"[]").expect("write isolated todo state");

        for count in workloads::selected_sizes(&[100, 1_000, 5_000]) {
            let fixture = workloads::note_fixture(0x4e4f_5445_5f4d_3042, count);
            let _cache_guard =
                crate::plugins::note::publish_note_cache_for_test(fixture.values.clone());
            let context = egui::Context::default();
            let mut panel = NotePanel::from_note(fixture.values[0].clone());
            let initial_derived = derived_workload_signature(&panel);
            let initial_recomputes = panel.heavy_recompute_count;

            let (idle_timing, _idle_output) = workloads::measure(
                workloads::UI_WARMUPS,
                |_, _| {},
                || panel.maybe_refresh_heavy_derived(&context, true),
            );
            let idle_metrics = workloads::metrics_for(&[
                Metric::NoteRefreshCheck,
                Metric::NoteSnapshot,
                Metric::NoteAliasHash,
                Metric::NoteHeavyRecompute,
            ]);
            assert_eq!(derived_workload_signature(&panel), initial_derived);
            assert_eq!(panel.heavy_recompute_count, initial_recomputes);
            assert_eq!(idle_metrics[0].work_units, 0);
            assert_eq!(idle_metrics[1].calls, 0);
            assert_eq!(idle_metrics[2].calls, 0);
            assert_eq!(idle_metrics[3].calls, 0);
            workloads::emit_summary(
                &format!("note-{count}-idle-refresh-check"),
                "production NotePanel::maybe_refresh_heavy_derived; headless derived-only CPU",
                fixture
                    .summary
                    .with_output_signatures(Some(initial_derived), None),
                idle_timing,
                &idle_metrics,
            );

            let frame_index = Cell::new(0_usize);
            let (draft_timing, _draft_output) = workloads::measure_with_state(
                &mut panel,
                workloads::UI_WARMUPS,
                |panel, _, _| {
                    let frame = frame_index.get();
                    frame_index.set(frame + 1);
                    let now = 10.0 + frame as f64;
                    panel
                        .note
                        .content
                        .push_str(&format!("\nDraft marker {frame:02}"));
                    panel.mark_content_changed(now);
                    context
                        .run_ui(
                            egui::RawInput {
                                time: Some(now + 1.0),
                                ..Default::default()
                            },
                            |_| {},
                        )
                        .drop_without_applying_deltas();
                },
                |panel| panel.maybe_refresh_heavy_derived(&context, true),
            );
            let draft_metrics = workloads::metrics_for(&[
                Metric::NoteRefreshCheck,
                Metric::NoteSnapshot,
                Metric::NoteAliasHash,
                Metric::NoteHeavyRecompute,
            ]);
            let draft_derived = derived_workload_signature(&panel);
            assert_eq!(draft_derived, initial_derived);
            assert_eq!(
                panel.heavy_recompute_count,
                initial_recomputes + workloads::UI_WARMUPS + workloads::SAMPLE_COUNT
            );
            assert_eq!(draft_metrics[0].work_units, workloads::SAMPLE_COUNT as u64);
            assert_eq!(draft_metrics[3].calls, workloads::SAMPLE_COUNT as u64);
            workloads::emit_summary(
                &format!("note-{count}-draft-after-debounce"),
                "production NotePanel::maybe_refresh_heavy_derived; synthetic draft rearmed outside timed interval",
                fixture
                    .summary
                    .with_output_signatures(Some(draft_derived), None),
                draft_timing,
                &draft_metrics,
            );
        }
        drop(workspace);
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

    fn backlink_rows_signature(
        rows: &[BacklinkRow],
    ) -> Vec<(
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> {
        rows.iter()
            .map(|row| {
                (
                    row.title.clone(),
                    row.type_badge.clone(),
                    row.updated.clone(),
                    row.snippet.clone(),
                    row.reason.clone(),
                    row.note_slug.clone(),
                    row.todo_id.clone(),
                )
            })
            .collect()
    }

    fn related_note_row_identity_text(panel: &NotePanel) -> Vec<(String, String, String)> {
        let mut rows = panel
            .derived
            .backlink_rows_related_notes
            .iter()
            .map(|row| {
                (
                    row.note_slug.clone().unwrap_or_default(),
                    row.title.clone(),
                    row.reason.clone(),
                )
            })
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows
    }

    fn mentioned_note_row_identity_text(panel: &NotePanel) -> Vec<(String, String, String)> {
        let mut rows = panel
            .derived
            .backlink_rows_mentions
            .iter()
            .filter_map(|row| {
                row.note_slug
                    .as_ref()
                    .map(|slug| (slug.clone(), row.title.clone(), row.reason.clone()))
            })
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows
    }

    fn run_heavy_refresh_at(panel: &mut NotePanel, ctx: &egui::Context, time: f64) {
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn note_close_observation_uses_real_discard_response_and_counts_unrendered_preview_notes() {
        use super::super::query_observation::{
            NoteCloseRenderFrame, NoteCloseWidgetRole, Q11_NOTE_SLUG,
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = new_app(&ctx);
        app.query = "preserved ordinary query".into();
        app.note_save_on_close = false;
        app.note_confirm_discard_unsaved_changes = true;
        let mut note = empty_note("# radial acceptance q11");
        note.slug = Q11_NOTE_SLUG.into();
        note.title = "Owned Q11".into();
        let mut panel = NotePanel::from_note(note);
        panel.replace_content_from_mutation(
            "# radial acceptance q11\nunsaved owned text".into(),
            1.0,
        );
        panel.request_close(&mut app);
        let before = panel.note_close_observation();
        assert!(before.fixture_slug && before.fixture_marker && before.pending_discard);
        assert!(
            before.rendered_discard.is_none(),
            "pending state is not a rendered widget receipt"
        );
        let original_content = panel.note_content().to_owned();
        let mut other = NotePanel::from_note(empty_note("# radial acceptance q11\nother note"));
        other.note.slug = "another-note".into();
        other.view_mode = NoteViewMode::Preview;
        assert!(!other.note_close_observation().fixture_slug);
        let mut observed = None;
        for _ in 0..4 {
            observed = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    panel.ui_with_note_close_observation(ctx, &mut app, Some(&mut observed));
                    let mut frame = NoteCloseRenderFrame::new(ctx);
                    frame.observe_panel(&panel, observed.clone());
                    let snapshot = frame.snapshot(std::slice::from_ref(&panel)).unwrap();
                    assert_eq!(snapshot.client_size, [900, 650]);
                    assert_eq!(snapshot.open_note_count, 1);
                    assert_eq!(snapshot.sole_note.unwrap().rendered_discard, observed);
                    let unrendered = NoteCloseRenderFrame::new(ctx)
                        .snapshot(std::slice::from_ref(&panel))
                        .unwrap();
                    assert!(unrendered.sole_note.unwrap().rendered_discard.is_none());
                },
            );
            if let Some(widget) = &observed {
                let nodes = &output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes;
                let (_, node) = nodes
                    .iter()
                    .find(|(id, _)| *id == egui::accesskit::NodeId(widget.widget_id))
                    .unwrap();
                assert_eq!(node.role(), egui::accesskit::Role::Button);
                assert_eq!(node.label(), Some("Discard Changes"));
                let bounds = node.bounds().unwrap();
                assert_eq!(
                    widget.bounds,
                    [
                        bounds.x0.floor() as i32,
                        bounds.y0.floor() as i32,
                        bounds.x1.ceil() as i32,
                        bounds.y1.ceil() as i32
                    ]
                );
                assert_eq!(widget.role, NoteCloseWidgetRole::DiscardChanges);
                assert_eq!(widget.owner_slug_digest, before.slug_digest);
            }
        }
        let widget = observed.expect("the actual normal confirmation must render");
        assert!(widget.widget_id != 0 && widget.enabled && widget.visible && widget.fully_visible);
        assert!(widget.bounds[0] >= widget.clip[0] && widget.bounds[1] >= widget.clip[1]);
        assert!(widget.bounds[2] <= widget.clip[2] && widget.bounds[3] <= widget.clip[3]);
        assert_eq!(panel.note_close_observation(), before);
        assert_eq!(panel.note_content(), original_content);
        assert!(panel.open && panel.has_unsaved_changes());
        assert_eq!(app.query, "preserved ordinary query");
        assert!(!app.note_save_on_close && app.note_confirm_discard_unsaved_changes);
        let mut panels = [panel, other];
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
                ..Default::default()
            },
            |ctx| {
                let mut frame = NoteCloseRenderFrame::new(ctx);
                frame.observe_panel(&panels[0], Some(widget.clone()));
                let ambiguous = frame.snapshot(&panels).unwrap();
                assert_eq!(
                    ambiguous.open_note_count, 2,
                    "preview/unrendered bodies still count as open panels"
                );
                assert!(ambiguous.sole_note.is_none());
                panels[1].open = false;
                let current = NoteCloseRenderFrame::new(ctx).snapshot(&panels).unwrap();
                assert_eq!(current.open_note_count, 1);
                assert!(
                    current.sole_note.unwrap().rendered_discard.is_none(),
                    "a prior-frame widget cannot be reused"
                );
                let mut replaced = NoteCloseRenderFrame::new(ctx);
                replaced.observe_panel(&panels[0], Some(widget.clone()));
                let mut replacement = empty_note("# radial acceptance q11");
                replacement.slug = "replacement-note".into();
                panels[0] = NotePanel::from_note(replacement);
                let snapshot = replaced.snapshot(&panels).unwrap();
                let sole = snapshot.sole_note.unwrap();
                assert!(!sole.fixture_slug && sole.fixture_marker);
                assert!(
                    sole.rendered_discard.is_none(),
                    "replacement must not inherit the prior panel's Response"
                );
                panels[0].open = false;
                let empty = NoteCloseRenderFrame::new(ctx).snapshot(&panels).unwrap();
                assert_eq!(empty.open_note_count, 0);
                assert!(empty.sole_note.is_none());
                for panel in &mut panels {
                    let mut note = empty_note("# radial acceptance q11");
                    note.slug = Q11_NOTE_SLUG.into();
                    *panel = NotePanel::from_note(note);
                }
                let duplicates = NoteCloseRenderFrame::new(ctx).snapshot(&panels).unwrap();
                assert_eq!(
                    duplicates.open_note_count, 2,
                    "duplicate canonical notes are still ambiguous"
                );
                assert!(duplicates.sole_note.is_none());
                panels[1].open = false;
                panels[0].replace_content_from_mutation(
                    "other content without the Q11 marker".into(),
                    2.0,
                );
                let sole = NoteCloseRenderFrame::new(ctx)
                    .snapshot(&panels)
                    .unwrap()
                    .sole_note
                    .unwrap();
                assert!(sole.fixture_slug && !sole.fixture_marker);
            },
        )
        .drop_without_applying_deltas();
    }

    #[test]
    fn render_editor_uses_the_supplied_explicit_id() {
        let ctx = egui::Context::default();
        let app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("body"));
        let editor_id = egui::Id::new("explicit-note-editor");
        let mut response_id = None;

        ctx.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                response_id = Some(panel.render_editor(ui, &app, editor_id, None).id);
            });
        })
        .drop_without_applying_deltas();

        assert_eq!(response_id, Some(editor_id));
    }

    #[test]
    fn note_window_initial_focus_survives_invisible_layout_without_stealing_later_focus() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("# Focus fixture"));
        panel.view_mode = NoteViewMode::Edit;
        let query_id = egui::Id::new("focus-fixture-query");
        let mut query = String::new();
        for frame in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            };
            ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.add(egui::TextEdit::singleline(&mut query).id(query_id));
                    // ROOT's initial show may focus its query after the note's sizing pass.
                    if frame != 2 {
                        response.request_focus();
                    }
                });
                panel.ui(ctx, &mut app);
            })
            .drop_without_applying_deltas();
            if frame == 0 {
                assert!(panel.focus_textedit_next_frame);
            } else if frame < 3 {
                let editor = panel.last_textedit_id.expect("note editor rendered");
                assert!(ctx.memory(|memory| memory.has_focus(editor)));
                assert!(!panel.focus_textedit_next_frame);
            } else {
                assert!(ctx.memory(|memory| memory.has_focus(query_id)));
            }
        }
    }

    #[test]
    fn bounded_editor_image_paste_uses_real_widget_id_and_consumes_both_request_forms() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("α猫ω"));
        panel.view_mode = NoteViewMode::Edit;
        let source = ("render-path-image-editor", "known-note");
        let size = egui::vec2(320.0, 140.0);
        let mut first_id = None;

        let output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = panel.render_bounded_editor_with_paste(
                    ui,
                    &mut app,
                    &frame_context,
                    source,
                    size,
                    || false,
                    || Ok::<_, String>(None),
                    |_| -> Result<String, String> { panic!("frame-one saver must not run") },
                );
                first_id = Some(response.id);
                response.request_focus();
                store_cursor(&frame_context, response.id, 2, 1);
                panel.handle_editor_response(response, &frame_context, &mut app, false);
            });
        });
        output.drop_without_applying_deltas();
        let first_id = first_id.unwrap();
        assert_eq!(panel.last_textedit_id, Some(first_id));

        panel.fast_derived_dirty = false;
        panel.heavy_recompute_requested = false;
        panel.last_edit_at_secs = None;
        panel.pending_selection = Some((1, 2));
        let lookups = std::cell::Cell::new(0);
        let saves = std::cell::Cell::new(0);
        let rgba = vec![1, 2, 3, 255, 5, 6, 7, 128];
        let filename = "deterministic-image.png";
        let markdown = note_image_markdown(filename);
        let mut second_id = None;
        let mut events_after_interceptor = None;
        let mut input = egui::RawInput {
            time: Some(88.25),
            events: vec![
                egui::Event::Text("unrelated".into()),
                egui::Event::Paste("text fallback".into()),
                key_event(egui::Modifiers::CTRL),
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
            ],
            ..Default::default()
        };
        // A text event would deliberately alter the editor; keep it unrelated to
        // text editing while still proving paste filtering is selective.
        input.events[0] = egui::Event::PointerGone;
        let output = ctx.run_ui(input, |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = panel.render_bounded_editor_with_paste(
                    ui,
                    &mut app,
                    &frame_context,
                    source,
                    size,
                    || false,
                    || {
                        lookups.set(lookups.get() + 1);
                        Ok::<_, String>(Some(ClipboardRgbaData {
                            width: 2,
                            height: 1,
                            bytes: rgba.clone(),
                        }))
                    },
                    |image| {
                        saves.set(saves.get() + 1);
                        assert_eq!(image.dimensions(), (2, 1));
                        assert_eq!(image.as_raw(), &rgba);
                        Ok::<_, String>(filename.into())
                    },
                );
                events_after_interceptor = Some(input_events(&frame_context));
                second_id = Some(response.id);
                panel.handle_editor_response(response, &frame_context, &mut app, false);
            });
        });
        output.drop_without_applying_deltas();

        let second_id = second_id.unwrap();
        assert_eq!((lookups.get(), saves.get()), (1, 1));
        assert_eq!(first_id, second_id);
        assert_eq!(panel.last_textedit_id, Some(second_id));
        assert_eq!(panel.last_paste_interceptor_id, Some(second_id));
        assert_eq!(
            panel.last_paste_outcome,
            Some(ClipboardImagePasteOutcome::Handled)
        );
        assert_eq!(
            panel.last_paste_request,
            Some(NotePasteRequest {
                has_egui_paste: true,
                has_egui_shortcut: true,
                has_windows_shortcut: false,
            })
        );
        assert!(ctx.memory(|memory| memory.has_focus(second_id)));
        assert_eq!(
            events_after_interceptor.unwrap(),
            vec![
                egui::Event::PointerGone,
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
            ]
        );
        assert_eq!(panel.note.content, format!("α{markdown}ω"));
        assert!(!panel.note.content.contains("text fallback"));
        let range = egui::widgets::text_edit::TextEditState::load(&ctx, second_id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.primary.index.0, 1 + markdown.chars().count());
        assert_eq!(range.primary.index.0, range.secondary.index.0);
        assert!(panel.pending_selection.is_none());
        assert!(panel.fast_derived_dirty && panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(88.25));
    }

    #[test]
    fn bounded_editor_leaves_native_text_paste_for_textedit() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("ab"));
        panel.view_mode = NoteViewMode::Edit;
        let source = "render-path-text-editor";
        let size = egui::vec2(320.0, 140.0);
        let mut id = None;
        let output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = panel.render_bounded_editor_with_paste(
                    ui,
                    &mut app,
                    &frame_context,
                    source,
                    size,
                    || false,
                    || Ok::<_, String>(None),
                    |_| -> Result<String, String> { panic!("saver must not run") },
                );
                id = Some(response.id);
                response.request_focus();
                store_cursor(&frame_context, response.id, 1, 1);
                panel.handle_editor_response(response, &frame_context, &mut app, false);
            });
        });
        output.drop_without_applying_deltas();
        panel.fast_derived_dirty = false;
        panel.heavy_recompute_requested = false;
        panel.last_edit_at_secs = None;
        let lookups = std::cell::Cell::new(0);
        let mut response_changed = false;
        let mut event_seen_before_textedit = None;
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(99.5),
                events: vec![egui::Event::Paste("hello".into())],
                ..Default::default()
            },
            |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    let response = panel.render_bounded_editor_with_paste(
                        ui,
                        &mut app,
                        &frame_context,
                        source,
                        size,
                        || false,
                        || {
                            lookups.set(lookups.get() + 1);
                            Ok::<_, String>(None)
                        },
                        |_| -> Result<String, String> { panic!("saver must not run") },
                    );
                    event_seen_before_textedit = Some(input_events(&frame_context));
                    response_changed = response.changed();
                    assert_eq!(response.id, id.unwrap());
                    panel.handle_editor_response(response, &frame_context, &mut app, false);
                });
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(lookups.get(), 1);
        assert_eq!(
            panel.last_paste_outcome,
            Some(ClipboardImagePasteOutcome::NotHandled)
        );
        assert_eq!(
            event_seen_before_textedit,
            Some(vec![egui::Event::Paste("hello".into())])
        );
        assert!(response_changed);
        assert_eq!(panel.note.content, "ahellob");
        assert!(!panel.note.content.contains("assets/"));
        assert!(panel.fast_derived_dirty && panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(99.5));
        assert_eq!(panel.last_paste_interceptor_id, id);
    }

    #[test]
    fn edit_and_split_editor_sources_produce_distinct_stable_final_ids() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("body"));
        panel.note.slug = "same-slug".into();
        let mut first = None;
        let mut second = None;

        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    let size = egui::vec2(300.0, 120.0);
                    let edit = panel.render_bounded_editor(
                        ui,
                        &mut app,
                        &frame_context,
                        ("note_text", "same-slug"),
                        size,
                    );
                    let split = panel.render_bounded_editor(
                        ui,
                        &mut app,
                        &frame_context,
                        ("note_split_text", "same-slug"),
                        size,
                    );
                    if first.is_none() {
                        first = Some((edit.id, split.id));
                    } else {
                        second = Some((edit.id, split.id));
                    }
                });
            });
            output.drop_without_applying_deltas();
        }

        assert_eq!(first, second);
        let (edit_id, split_id) = first.unwrap();
        assert_ne!(edit_id, split_id);
    }

    #[test]
    fn pending_cursor_and_later_insertions_use_the_rendered_editor_state() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("abcdef"));
        panel.move_editor_cursor_to(3, &ctx);
        assert!(panel.last_textedit_id.is_none());
        assert_eq!(panel.pending_cursor_position, Some(3));
        let editor_id = egui::Id::new("actual-rendered-editor");

        let output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = panel.render_editor(ui, &app, editor_id, None);
                panel.handle_editor_response(response, &frame_context, &mut app, false);
            });
        });
        output.drop_without_applying_deltas();

        assert_eq!(panel.last_textedit_id, Some(editor_id));
        assert!(panel.pending_cursor_position.is_none());
        let cursor = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(cursor.primary.index.0, 3);

        panel.insert_text_at_cursor_or_selection(&ctx, panel.last_textedit_id.unwrap(), "!");
        assert_eq!(panel.note.content, "abc!def");
        let cursor = egui::widgets::text_edit::TextEditState::load(&ctx, editor_id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(cursor.primary.index.0, 4);
    }

    #[derive(Default)]
    struct PasteInputBuilder {
        events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    }

    impl PasteInputBuilder {
        fn native(mut self, text: &str) -> Self {
            self.events.push(egui::Event::Paste(text.into()));
            self
        }

        fn key(mut self, modifiers: egui::Modifiers) -> Self {
            self.modifiers = modifiers;
            self.events.push(egui::Event::Key {
                key: Key::V,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            });
            self
        }

        fn unrelated(mut self, text: &str) -> Self {
            self.events.push(egui::Event::Text(text.into()));
            self
        }

        fn begin(self, ctx: &egui::Context, focused: egui::Id) {
            let modifiers = self.modifiers;
            ctx.begin_pass(egui::RawInput {
                time: Some(123.456),
                events: self.events,
                ..Default::default()
            });
            // egui 0.36 removed RawInput.modifiers. Preserve the old frame
            // state directly while keeping this event fixture unchanged.
            ctx.input_mut(|input| input.modifiers = modifiers);
            ctx.memory_mut(|memory| memory.request_focus(focused));
        }
    }

    fn key_event(modifiers: egui::Modifiers) -> egui::Event {
        PasteInputBuilder::default()
            .key(modifiers)
            .events
            .pop()
            .unwrap()
    }

    fn input_events(ctx: &egui::Context) -> Vec<egui::Event> {
        ctx.input(|input| input.events.clone())
    }

    fn store_cursor(ctx: &egui::Context, id: egui::Id, primary: usize, secondary: usize) {
        use egui::text::{CCursor, CCursorRange};
        let mut state = egui::widgets::text_edit::TextEditState::default();
        state.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(primary),
            secondary: CCursor::new(secondary),
            h_pos: None,
        }));
        state.store(ctx, id);
    }

    #[test]
    fn paste_request_detection_accepts_native_ctrl_command_and_deduplicates_combined_input() {
        let paste = egui::Event::Paste("clipboard text".into());
        let ctrl = key_event(egui::Modifiers::CTRL);
        let command = key_event(egui::Modifiers::COMMAND);
        assert_eq!(
            detect_note_paste_request(&[paste.clone()], false).unwrap(),
            NotePasteRequest {
                has_egui_paste: true,
                has_egui_shortcut: false,
                has_windows_shortcut: false
            }
        );
        assert!(detect_note_paste_request(&[ctrl.clone()], false).is_some());
        assert!(detect_note_paste_request(&[command], false).is_some());

        let mut events = vec![
            egui::Event::Text("before".into()),
            paste,
            egui::Event::Text("middle".into()),
            ctrl,
            egui::Event::Text("after".into()),
        ];
        let request = detect_note_paste_request(&events, false).unwrap();
        assert_eq!(
            request,
            NotePasteRequest {
                has_egui_paste: true,
                has_egui_shortcut: true,
                has_windows_shortcut: false
            }
        );
        consume_note_paste_request(&mut events, request);
        assert_eq!(
            events,
            vec![
                egui::Event::Text("before".into()),
                egui::Event::Text("middle".into()),
                egui::Event::Text("after".into())
            ]
        );
    }

    #[test]
    fn paste_request_detection_rejects_plain_alt_and_repeated_v() {
        assert!(detect_note_paste_request(&[key_event(egui::Modifiers::NONE)], false).is_none());
        assert!(detect_note_paste_request(&[key_event(egui::Modifiers::ALT)], false).is_none());
        let mut repeated = key_event(egui::Modifiers::CTRL);
        if let egui::Event::Key { repeat, .. } = &mut repeated {
            *repeat = true;
        }
        assert!(detect_note_paste_request(&[repeated], false).is_none());
    }

    #[test]
    fn native_detector_uses_v_rising_edges_and_requires_ctrl_already_down() {
        let mut detector = NativePasteDetector::default();
        // Initial press, held frames, release, and a second press yield two edges.
        assert!(!detector.update(true, false));
        assert!(detector.update(true, true));
        assert!(!detector.update(true, true));
        assert!(!detector.update(true, false));
        assert!(detector.update(true, true));

        // V without Ctrl still updates/rearms state. Our explicit chord policy is
        // that adding Ctrl while V remains held is not a paste edge.
        assert!(!detector.update(false, false));
        assert!(!detector.update(false, true));
        assert!(!detector.update(true, true));
        assert!(!detector.update(false, false));
        assert!(detector.update(true, true));
    }

    #[test]
    fn merged_native_and_egui_request_is_single_and_consumes_only_egui_sources() {
        let paste = egui::Event::Paste("hello".into());
        let key = key_event(egui::Modifiers::CTRL);
        let mut events = vec![paste, key];
        let request = detect_note_paste_request(&events, true).unwrap();
        assert_eq!(
            request,
            NotePasteRequest {
                has_egui_paste: true,
                has_egui_shortcut: true,
                has_windows_shortcut: true,
            }
        );
        consume_note_paste_request(&mut events, request);
        assert!(events.is_empty());
        assert_eq!(
            detect_note_paste_request(&[], true),
            Some(NotePasteRequest {
                has_egui_paste: false,
                has_egui_shortcut: false,
                has_windows_shortcut: true,
            })
        );
    }

    #[test]
    fn clipboard_rgba_validation_and_markdown_are_independent() {
        let image = rgba_image_from_clipboard_data(1, 2, vec![255; 8]).unwrap();
        assert_eq!(image.dimensions(), (1, 2));
        assert!(rgba_image_from_clipboard_data(0, 2, vec![]).is_err());
        assert!(rgba_image_from_clipboard_data(2, 2, vec![0; 15]).is_err());
        assert_eq!(
            note_image_markdown("note-image-20260825-162145.123.png"),
            "![note-image-20260825-162145.123.png](assets/note-image-20260825-162145.123.png)"
        );
    }

    #[test]
    fn image_paste_replaces_real_unicode_selection_consumes_input_and_marks_dirty() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("note-editor-multiline-stable");
        PasteInputBuilder::default()
            .unrelated("before")
            .native("clipboard text")
            .unrelated("after")
            .begin(&ctx, id);
        store_cursor(&ctx, id, 2, 1);
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("α猫ω"));
        panel.view_mode = NoteViewMode::Edit;
        let saves = std::cell::Cell::new(0);
        let outcome = panel.intercept_clipboard_image_paste_with(
            &ctx,
            &mut app,
            id,
            || false,
            || {
                Ok::<_, String>(Some(ClipboardRgbaData {
                    width: 2,
                    height: 1,
                    bytes: vec![1, 2, 3, 255, 5, 6, 7, 128],
                }))
            },
            |image| {
                saves.set(saves.get() + 1);
                assert_eq!(image.dimensions(), (2, 1));
                assert_eq!(image.as_raw(), &vec![1, 2, 3, 255, 5, 6, 7, 128]);
                Ok::<_, String>("note-image-20260825-162145.123.png".into())
            },
        );
        let markdown = note_image_markdown("note-image-20260825-162145.123.png");
        assert_eq!(outcome, ClipboardImagePasteOutcome::Handled);
        assert_eq!(saves.get(), 1);
        assert_eq!(panel.note.content, format!("α{markdown}ω"));
        assert_eq!(
            input_events(&ctx),
            vec![
                egui::Event::Text("before".into()),
                egui::Event::Text("after".into())
            ]
        );
        assert!(panel.fast_derived_dirty && panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(123.456));
        assert!(panel.pending_selection.is_none());
        let range = egui::widgets::text_edit::TextEditState::load(&ctx, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(range.primary.index.0, 1 + markdown.chars().count());
        assert_eq!(range.primary.index.0, range.secondary.index.0);
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn image_paste_at_caret_inserts_in_middle_and_native_text_never_reaches_editor() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("caret-editor");
        PasteInputBuilder::default()
            .native("text that must not appear")
            .key(egui::Modifiers::CTRL)
            .begin(&ctx, id);
        store_cursor(&ctx, id, 1, 1);
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("ab"));
        panel.view_mode = NoteViewMode::Edit;
        let lookups = std::cell::Cell::new(0);
        let saves = std::cell::Cell::new(0);
        let outcome = panel.intercept_clipboard_image_paste_with(
            &ctx,
            &mut app,
            id,
            || false,
            || {
                lookups.set(lookups.get() + 1);
                Ok::<_, String>(Some(ClipboardRgbaData {
                    width: 1,
                    height: 1,
                    bytes: vec![9, 8, 7, 255],
                }))
            },
            |_| {
                saves.set(saves.get() + 1);
                Ok::<_, String>("note-image-20260826-010203.004.png".into())
            },
        );
        let markdown = note_image_markdown("note-image-20260826-010203.004.png");
        assert_eq!(outcome, ClipboardImagePasteOutcome::Handled);
        assert_eq!((lookups.get(), saves.get()), (1, 1));
        assert_eq!(panel.note.content, format!("a{markdown}b"));
        assert!(input_events(&ctx).is_empty());
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn text_only_paste_is_completely_untouched_and_egui_can_process_it() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("text-editor");
        PasteInputBuilder::default()
            .native("ordinary text")
            .begin(&ctx, id);
        store_cursor(&ctx, id, 1, 1);
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("ab"));
        panel.view_mode = NoteViewMode::Edit;
        panel.pending_selection = Some((1, 1));
        let before_state = egui::widgets::text_edit::TextEditState::load(&ctx, id)
            .unwrap()
            .cursor
            .char_range();
        let before = (
            panel.note.content.clone(),
            panel.pending_selection,
            panel.fast_derived_dirty,
            panel.heavy_recompute_requested,
            panel.last_edit_at_secs,
        );
        let outcome = panel.intercept_clipboard_image_paste_with(
            &ctx,
            &mut app,
            id,
            || false,
            || Ok::<_, String>(None),
            |_| -> Result<String, String> { panic!("save must not run") },
        );
        assert_eq!(outcome, ClipboardImagePasteOutcome::NotHandled);
        assert_eq!(
            input_events(&ctx),
            vec![egui::Event::Paste("ordinary text".into())]
        );
        assert_eq!(
            (
                panel.note.content.clone(),
                panel.pending_selection,
                panel.fast_derived_dirty,
                panel.heavy_recompute_requested,
                panel.last_edit_at_secs
            ),
            before
        );
        assert_eq!(
            egui::widgets::text_edit::TextEditState::load(&ctx, id)
                .unwrap()
                .cursor
                .char_range(),
            before_state
        );
        let mut root_ui = egui::Ui::new(
            ctx.clone(),
            egui::Id::new("text-only-paste-root"),
            egui::UiBuilder::new().max_rect(ctx.content_rect()),
        );
        let _panel_response = egui::CentralPanel::default().show(&mut root_ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut panel.note.content).id(id));
        });
        assert_eq!(panel.note.content, "aordinary textb");
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn clipboard_lookup_is_excluded_by_each_non_editor_focus_or_view_mode() {
        for (index, mode, focus_editor) in [
            (0, NoteViewMode::Edit, false),
            (1, NoteViewMode::Preview, true),
            (2, NoteViewMode::Edit, false),
        ] {
            let ctx = egui::Context::default();
            let id = egui::Id::new(("editor", index));
            let focused = if focus_editor {
                id
            } else {
                egui::Id::new(("toolbar-or-link-dialog", index))
            };
            PasteInputBuilder::default()
                .unrelated("x")
                .native("ordinary text")
                .key(egui::Modifiers::COMMAND)
                .begin(&ctx, focused);
            let original = input_events(&ctx);
            let mut app = new_app(&ctx);
            let mut panel = NotePanel::from_note(empty_note("unchanged"));
            panel.view_mode = mode;
            let lookups = std::cell::Cell::new(0);
            let saves = std::cell::Cell::new(0);
            let outcome = panel.intercept_clipboard_image_paste_with(
                &ctx,
                &mut app,
                id,
                || false,
                || {
                    lookups.set(lookups.get() + 1);
                    Ok::<_, String>(None)
                },
                |_| {
                    saves.set(saves.get() + 1);
                    Ok::<_, String>(String::new())
                },
            );
            assert_eq!(outcome, ClipboardImagePasteOutcome::NotHandled);
            assert_eq!((lookups.get(), saves.get()), (0, 0));
            assert_eq!(input_events(&ctx), original);
            assert_eq!(panel.note.content, "unchanged");
            ctx.end_pass().drop_without_applying_deltas();
        }
    }

    #[test]
    fn focused_split_editor_routes_keyboard_image_paste() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("ab"));
        panel.view_mode = NoteViewMode::Split;
        let source = "render-path-split-editor";
        let size = egui::vec2(320.0, 140.0);
        let mut first_id = None;

        let output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                let response = panel.render_bounded_editor_with_paste(
                    ui,
                    &mut app,
                    &frame_context,
                    source,
                    size,
                    || false,
                    || Ok::<_, String>(None),
                    |_| -> Result<String, String> { panic!("frame-one saver must not run") },
                );
                first_id = Some(response.id);
                response.request_focus();
                store_cursor(&frame_context, response.id, 1, 1);
                panel.handle_editor_response(response, &frame_context, &mut app, false);
            });
        });
        output.drop_without_applying_deltas();

        let lookups = std::cell::Cell::new(0);
        let saves = std::cell::Cell::new(0);
        let mut second_id = None;
        let output = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    key_event(egui::Modifiers::COMMAND),
                    egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    let response = panel.render_bounded_editor_with_paste(
                        ui,
                        &mut app,
                        &frame_context,
                        source,
                        size,
                        || false,
                        || {
                            lookups.set(lookups.get() + 1);
                            Ok::<_, String>(Some(ClipboardRgbaData {
                                width: 1,
                                height: 1,
                                bytes: vec![10, 20, 30, 255],
                            }))
                        },
                        |image| {
                            saves.set(saves.get() + 1);
                            assert_eq!(image.as_raw(), &vec![10, 20, 30, 255]);
                            Ok::<_, String>("split-image.png".into())
                        },
                    );
                    second_id = Some(response.id);
                    panel.handle_editor_response(response, &frame_context, &mut app, false);
                });
            },
        );
        output.drop_without_applying_deltas();

        assert_eq!((lookups.get(), saves.get()), (1, 1));
        assert_eq!(first_id, second_id);
        assert_eq!(panel.last_paste_interceptor_id, second_id);
        assert_eq!(
            panel.last_paste_request,
            Some(NotePasteRequest {
                has_egui_paste: false,
                has_egui_shortcut: true,
                has_windows_shortcut: false,
            })
        );
        assert_eq!(
            panel.note.content,
            format!("a{}b", note_image_markdown("split-image.png"))
        );
    }

    #[test]
    fn image_decode_and_save_failures_preserve_state_but_consume_image_paste() {
        for save_fails in [false, true] {
            let ctx = egui::Context::default();
            let id = egui::Id::new(("failure-editor", save_fails));
            PasteInputBuilder::default()
                .native("fallback text")
                .key(egui::Modifiers::CTRL)
                .begin(&ctx, id);
            store_cursor(&ctx, id, 2, 1);
            let mut app = new_app(&ctx);
            app.show_inline_errors = true;
            let mut panel = NotePanel::from_note(empty_note("α猫ω"));
            panel.view_mode = NoteViewMode::Edit;
            panel.pending_selection = Some((1, 2));
            let outcome = panel.intercept_clipboard_image_paste_with(
                &ctx,
                &mut app,
                id,
                || false,
                || {
                    Ok::<_, String>(Some(ClipboardRgbaData {
                        width: 1,
                        height: 1,
                        bytes: if save_fails {
                            vec![1, 2, 3, 4]
                        } else {
                            vec![1, 2, 3]
                        },
                    }))
                },
                |_| {
                    if save_fails {
                        Err::<String, String>("disk full".into())
                    } else {
                        Ok::<_, String>("unused.png".into())
                    }
                },
            );
            assert_eq!(outcome, ClipboardImagePasteOutcome::Handled);
            assert!(input_events(&ctx).is_empty());
            assert_eq!(panel.note.content, "α猫ω");
            assert_eq!(panel.pending_selection, Some((1, 2)));
            assert!(!panel.fast_derived_dirty && !panel.heavy_recompute_requested);
            assert_eq!(panel.last_edit_at_secs, None);
            let range = egui::widgets::text_edit::TextEditState::load(&ctx, id)
                .unwrap()
                .cursor
                .char_range()
                .unwrap();
            assert_eq!((range.primary.index.0, range.secondary.index.0), (2, 1));
            let error = app.error.as_deref().unwrap();
            assert!(
                error.contains(if save_fails {
                    "persist clipboard image"
                } else {
                    "invalid clipboard RGBA"
                }),
                "{error}"
            );
            ctx.end_pass().drop_without_applying_deltas();
        }
    }

    #[test]
    fn clipboard_read_failure_reports_error_without_consuming_text_paste() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("read-failure-editor");
        PasteInputBuilder::default()
            .native("ordinary text")
            .begin(&ctx, id);
        let mut app = new_app(&ctx);
        app.show_inline_errors = true;
        let mut panel = NotePanel::from_note(empty_note("unchanged"));
        panel.view_mode = NoteViewMode::Edit;
        let outcome = panel.intercept_clipboard_image_paste_with(
            &ctx,
            &mut app,
            id,
            || false,
            || Err::<Option<ClipboardRgbaData>, _>("could not open the clipboard"),
            |_| Ok::<_, String>(String::new()),
        );
        assert_eq!(outcome, ClipboardImagePasteOutcome::NotHandled);
        assert_eq!(
            input_events(&ctx),
            vec![egui::Event::Paste("ordinary text".into())]
        );
        assert_eq!(panel.note.content, "unchanged");
        assert!(
            app.error
                .as_deref()
                .unwrap()
                .contains("could not open the clipboard")
        );
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn image_paste_can_use_real_transactional_writer_in_an_injected_directory() {
        use crate::plugins::note::save_note_image_asset_in_dir_at;
        let ctx = egui::Context::default();
        let id = egui::Id::new("writer-integration-editor");
        PasteInputBuilder::default()
            .native("ignored")
            .begin(&ctx, id);
        store_cursor(&ctx, id, 0, 0);
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note(""));
        panel.view_mode = NoteViewMode::Edit;
        let temp = tempdir().unwrap();
        let assets = temp.path().join("assets");
        let timestamp = Local
            .with_ymd_and_hms(2026, 8, 26, 1, 2, 3)
            .single()
            .unwrap()
            + chrono::Duration::milliseconds(4);
        let outcome = panel.intercept_clipboard_image_paste_with(
            &ctx,
            &mut app,
            id,
            || false,
            || {
                Ok::<_, String>(Some(ClipboardRgbaData {
                    width: 2,
                    height: 1,
                    bytes: vec![1, 2, 3, 4, 5, 6, 7, 8],
                }))
            },
            |image| save_note_image_asset_in_dir_at(image, &assets, timestamp),
        );
        assert_eq!(outcome, ClipboardImagePasteOutcome::Handled);
        let filename = "note-image-20260826-010203.004.png";
        assert_eq!(panel.note.content, note_image_markdown(filename));
        let decoded = image::open(assets.join(filename)).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.as_raw(), &vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(fs::read_dir(&assets).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("staging")
        }));
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn plain_link_conversion_starts_from_entire_unsaved_buffer() {
        let mut panel = NotePanel::from_note(empty_note("before https://example.com after"));
        panel.pending_selection = Some((0, 6));

        assert!(panel.start_plain_link_conversion());
        let pending = panel.pending_plain_link_conversion.as_ref().unwrap();
        assert_eq!(pending.source, "before https://example.com after");
        assert_eq!(pending.report.web_links, 1);
        assert_eq!(pending.report.directories, 0);
        assert_eq!(pending.report.files, 0);
        assert_eq!(
            pending.report.content,
            "before [example](https://example.com) after"
        );
    }

    #[test]
    fn plain_link_conversion_noop_has_no_dialog_or_mutation_and_can_notify() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("[linked](https://example.com)"));
        let before_edit = panel.last_edit_at_secs;

        assert!(!panel.start_plain_link_conversion());
        NotePanel::show_no_plain_links_toast(&mut app);

        assert!(panel.pending_plain_link_conversion.is_none());
        assert_eq!(panel.note.content, "[linked](https://example.com)");
        assert_eq!(panel.last_edit_at_secs, before_edit);
        assert!(
            app.test_toast_messages
                .iter()
                .any(|message| message == "No convertible plain links found.")
        );
    }

    #[test]
    fn cancelling_plain_link_conversion_only_clears_transient_state() {
        let mut panel = NotePanel::from_note(empty_note("https://example.com"));
        assert!(panel.start_plain_link_conversion());
        let before = panel.note.clone();
        let before_edit = panel.last_edit_at_secs;

        panel.cancel_plain_link_conversion();

        assert_eq!(panel.note.content, before.content);
        assert_eq!(panel.note.path, before.path);
        assert_eq!(panel.last_edit_at_secs, before_edit);
        assert!(panel.pending_plain_link_conversion.is_none());
        assert!(panel.focus_textedit_next_frame);
    }

    #[test]
    fn confirming_plain_link_conversion_uses_mutation_invalidation_without_saving() {
        let temp = tempdir().unwrap();
        let note_path = temp.path().join("not-written.md");
        fs::write(&note_path, "persisted content").unwrap();
        let mut note = empty_note("# Title\n\nhttps://example.com");
        note.path = note_path.clone();
        let mut panel = NotePanel::from_note(note);
        let _ = panel.markdown_analysis();
        panel.fast_derived_dirty = false;
        panel.heavy_recompute_requested = false;
        assert!(panel.start_plain_link_conversion());

        assert_eq!(
            panel.confirm_plain_link_conversion(42.0),
            PlainLinkConfirmOutcome::Applied
        );

        assert_eq!(
            panel.note.content,
            "# Title\n\n[example](https://example.com)"
        );
        assert_eq!(panel.note.path, note_path);
        assert_eq!(panel.last_edit_at_secs, Some(42.0));
        assert!(panel.markdown_analysis.is_none());
        assert!(panel.markdown_analysis_source_hash.is_none());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert!(panel.focus_textedit_next_frame);
        assert_eq!(
            fs::read_to_string(&panel.note.path).unwrap(),
            "persisted content"
        );
    }

    #[test]
    fn stale_plain_link_confirmation_refreshes_then_requires_second_click() {
        let mut panel = NotePanel::from_note(empty_note("https://one.example"));
        assert!(panel.start_plain_link_conversion());
        panel.note.content.push_str(" and https://two.example");

        assert_eq!(
            panel.confirm_plain_link_conversion(1.0),
            PlainLinkConfirmOutcome::Refreshed
        );
        assert_eq!(
            panel.note.content,
            "https://one.example and https://two.example"
        );
        let refreshed = panel.pending_plain_link_conversion.as_ref().unwrap();
        assert_eq!(refreshed.source, panel.note.content);
        assert_eq!(refreshed.report.web_links, 2);
        assert_eq!(panel.last_edit_at_secs, None);

        assert_eq!(
            panel.confirm_plain_link_conversion(2.0),
            PlainLinkConfirmOutcome::Applied
        );
        assert_eq!(
            panel.note.content,
            "[one](https://one.example) and [two](https://two.example)"
        );
        assert_eq!(panel.last_edit_at_secs, Some(2.0));
    }

    #[test]
    fn stale_plain_link_confirmation_closes_when_candidates_were_removed() {
        let mut panel = NotePanel::from_note(empty_note("https://example.com"));
        assert!(panel.start_plain_link_conversion());
        panel.note.content = "intervening edit with no links".into();

        assert_eq!(
            panel.confirm_plain_link_conversion(3.0),
            PlainLinkConfirmOutcome::NoCandidates
        );
        assert_eq!(panel.note.content, "intervening edit with no links");
        assert!(panel.pending_plain_link_conversion.is_none());
        assert_eq!(panel.last_edit_at_secs, None);
    }

    #[test]
    fn converted_plain_links_are_idempotent() {
        let mut panel = NotePanel::from_note(empty_note("https://example.com"));
        assert!(panel.start_plain_link_conversion());
        assert_eq!(
            panel.confirm_plain_link_conversion(4.0),
            PlainLinkConfirmOutcome::Applied
        );
        let converted = panel.note.content.clone();
        let edit_time = panel.last_edit_at_secs;

        assert!(!panel.start_plain_link_conversion());
        assert_eq!(panel.note.content, converted);
        assert_eq!(panel.last_edit_at_secs, edit_time);
    }

    #[test]
    fn heading_display_text_uses_heading_title_without_markdown_marker() {
        let content = "## Vendors to buy from or look at";
        let analysis = analyze_markdown(content);
        let heading = analysis.headings.first().expect("heading should parse");
        let display_text = heading_display_text(heading);

        assert_eq!(display_text, "Vendors to buy from or look at");
        assert!(!display_text.contains("##"));
    }

    #[test]
    fn task_display_text_uses_normalized_task_text_without_marker_or_bullet() {
        let content = "- [ ] adsfasdf";
        let task_item = analyze_markdown(content).task_items.remove(0);
        let display_text = task_display_text(&task_item);

        assert_eq!(display_text, "adsfasdf");
        assert!(!display_text.contains("[ ]"));
        assert!(!display_text.starts_with('-'));
        assert!(!display_text.starts_with('\n'));
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
        ctx.run_ui(Default::default(), |ctx| {
            panel.ui(ctx, app);
        })
        .drop_without_applying_deltas();
    }

    fn render_note_body_once(
        ctx: &egui::Context,
        panel: &mut NotePanel,
        app: &mut LauncherApp,
        size: egui::Vec2,
    ) {
        let slug = panel.note.slug.clone();
        let scroll_id_source = ("note_scroll", slug.clone());
        let text_id_source = ("note_text", slug);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };

        let output = ctx.run_ui(raw_input, |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(size.x);
                    ui.set_height(size.y);
                    panel.render_note_body_row(
                        ui,
                        app,
                        &frame_context,
                        scroll_id_source.clone(),
                        text_id_source.clone(),
                    );
                });
            });
        });
        output.drop_without_applying_deltas();
    }

    fn assert_split_panes_share_content_width(
        content_rect: egui::Rect,
        editor_rect: egui::Rect,
        preview_rect: egui::Rect,
    ) {
        let split_gap = (preview_rect.min.x - editor_rect.max.x).max(0.0);
        assert!(
            editor_rect.min.x >= content_rect.min.x - 1.0,
            "split editor should start within the existing content body, content={content_rect:?}, editor={editor_rect:?}"
        );
        assert!(
            preview_rect.max.x <= content_rect.max.x + 1.0,
            "split preview should end within the existing content body, content={content_rect:?}, preview={preview_rect:?}"
        );
        assert!(
            editor_rect.width() + split_gap + preview_rect.width() <= content_rect.width() + 1.0,
            "split panes plus their separator/spacing should fit within the content width, content={content_rect:?}, editor={editor_rect:?}, preview={preview_rect:?}, gap={split_gap}"
        );
    }

    fn assert_outline_and_content_share_body_width(
        body_width: f32,
        outline_rect: egui::Rect,
        content_rect: egui::Rect,
    ) {
        let outline_gap = (content_rect.min.x - outline_rect.max.x).max(0.0);
        assert!(
            content_rect.min.x >= outline_rect.max.x,
            "outline-visible content should start after the outline width plus separator budget, outline={outline_rect:?}, content={content_rect:?}"
        );
        assert!(
            outline_rect.width() + outline_gap + content_rect.width() <= body_width + 1.0,
            "outline plus separator/spacing plus content should share the existing note body width, outline={outline_rect:?}, content={content_rect:?}, gap={outline_gap}, body_width={body_width}"
        );
    }

    #[test]
    fn markdown_title_change_keeps_note_window_rect() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (420.0, 320.0);

        let mut panel = NotePanel::from_note(note_with_slug("Original Title", "stable-note"));
        let window_id = egui::Id::new(("note_panel_window", panel.note.slug.clone()));
        let raw_input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };

        ctx.run_ui(raw_input(), |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();
        let original_rect = ctx
            .memory(|memory| memory.area_rect(window_id))
            .expect("note window rect should be remembered after rendering");

        panel.note.title = "Renamed From Markdown".into();
        panel.note.content = "# Renamed From Markdown\n\nBody".into();
        app.note_panel_default_size = (760.0, 560.0);

        ctx.run_ui(raw_input(), |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();
        let renamed_rect = ctx
            .memory(|memory| memory.area_rect(window_id))
            .expect("note window rect should still be remembered after title changes");

        assert_eq!(renamed_rect, original_rect);
    }

    #[test]
    fn edit_mode_editor_height_tracks_tall_content_pane() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (720.0, 520.0);

        let mut panel = NotePanel::from_note(empty_note("Short note body"));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Edit;

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(raw_input, |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();

        let content_rect = panel
            .last_content_rect
            .expect("content pane should be allocated in edit mode");
        let editor_rect = panel
            .last_editor_rect
            .expect("editor should be allocated in edit mode");

        assert!(
            content_rect.height() > 300.0,
            "content pane should receive the window body height, got {content_rect:?}"
        );
        assert!(
            editor_rect.height() > 300.0,
            "editor should grow with the tall content pane instead of staying near the old desired_rows(10) height, got {editor_rect:?}"
        );
        assert!(
            (content_rect.height() - editor_rect.height()).abs() < 8.0,
            "editor should closely match the content pane height, content={content_rect:?}, editor={editor_rect:?}"
        );
    }

    #[test]
    fn edit_mode_long_note_does_not_expand_outer_window() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (500.0, 360.0);

        let content = (0..300)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut panel = NotePanel::from_note(empty_note(&content));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Edit;

        let window_id = egui::Id::new(("note_panel_window", panel.note.slug.clone()));
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 900.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(raw_input, |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();

        let rect = ctx
            .memory(|memory| memory.area_rect(window_id))
            .expect("note panel window rect should be remembered after rendering");

        assert!(
            rect.height() <= app.note_panel_default_size.1 + 120.0,
            "long edit content should not expand the outer window from content height, rect={rect:?}"
        );
        assert!(
            rect.height() < 700.0,
            "long edit content should not expand the outer window near the screen height, rect={rect:?}"
        );
        assert!(
            rect.width() <= app.note_panel_default_size.0 + 80.0,
            "long edit content or lines should not expand the outer window width, rect={rect:?}"
        );
    }

    #[test]
    fn short_and_long_notes_use_similar_outer_window_size() {
        fn rendered_note_panel_rect(content: &str, slug: &str) -> egui::Rect {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.note_panel_default_size = (500.0, 360.0);

            let mut note = empty_note(content);
            note.slug = slug.into();
            let mut panel = NotePanel::from_note(note);
            panel.show_metadata = false;
            panel.view_mode = NoteViewMode::Edit;

            let window_id = egui::Id::new(("note_panel_window", panel.note.slug.clone()));
            let raw_input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..Default::default()
            };
            ctx.run_ui(raw_input, |ctx| {
                panel.ui(ctx, &mut app);
            })
            .drop_without_applying_deltas();

            ctx.memory(|memory| memory.area_rect(window_id))
                .expect("note panel window rect should be remembered after rendering")
        }

        let short_rect = rendered_note_panel_rect("short note", "short-note");
        let long_content = (0..300)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let long_rect = rendered_note_panel_rect(&long_content, "long-note");

        assert!((short_rect.width() - long_rect.width()).abs() <= 40.0);
        assert!((short_rect.height() - long_rect.height()).abs() <= 60.0);
    }

    #[test]
    fn split_panes_fill_allocated_body_height() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (760.0, 540.0);
        let original_default_size = app.note_panel_default_size;
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;
        app.note_settings.outline_sidebar_enabled = true;

        let mut panel = NotePanel::from_note(empty_note(
            "# Split body\n\n## Preview heading\n\nShort split-mode body.",
        ));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.outline_open = true;

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(raw_input, |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();

        let content_rect = panel
            .last_content_rect
            .expect("content body pane should be allocated");
        let editor_rect = panel
            .last_split_editor_rect
            .expect("split editor pane should be allocated");
        let preview_rect = panel
            .last_split_preview_rect
            .expect("split preview pane should be allocated");

        assert!(
            (editor_rect.height() - preview_rect.height()).abs() < 4.0,
            "split editor and preview panes should have similar heights, editor={editor_rect:?}, preview={preview_rect:?}"
        );
        assert!(
            (content_rect.height() - editor_rect.height()).abs() < 8.0,
            "split editor pane should closely match content body height, content={content_rect:?}, editor={editor_rect:?}"
        );
        assert!(
            (content_rect.height() - preview_rect.height()).abs() < 8.0,
            "split preview pane should closely match content body height, content={content_rect:?}, preview={preview_rect:?}"
        );
        assert_eq!(
            app.note_panel_default_size, original_default_size,
            "split rendering must use bounded layout and must not write the measured window size back to the global note panel default"
        );
    }

    #[test]
    fn split_view_does_not_auto_expand_note_window_width() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (500.0, 420.0);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;

        let mut panel = NotePanel::from_note(empty_note("# Split width\n\nBody"));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.outline_open = false;

        render_note_body_once(&ctx, &mut panel, &mut app, egui::vec2(500.0, 420.0));

        let content_rect = panel
            .last_content_rect
            .expect("split content body pane should be allocated");
        let editor_rect = panel
            .last_split_editor_rect
            .expect("split editor pane should be allocated");
        let preview_rect = panel
            .last_split_preview_rect
            .expect("split preview pane should be allocated");
        assert!(
            panel.last_outline_rect.is_none(),
            "closed outline should not allocate an outline pane in no-outline split view"
        );
        assert!(
            content_rect.width() <= app.note_panel_default_size.0 + 1.0,
            "split view should keep the content near the configured note width instead of expanding toward the screen width, content={content_rect:?}"
        );
        assert!(
            content_rect.width() < 700.0,
            "split view content should not approach a large screen width, content={content_rect:?}"
        );
        assert_split_panes_share_content_width(content_rect, editor_rect, preview_rect);
    }

    #[test]
    fn split_view_allows_narrow_dialog_without_forcing_pane_minimums() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (220.0, 420.0);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;

        let mut panel = NotePanel::from_note(empty_note("# Narrow split\n\nBody"));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.outline_open = false;

        render_note_body_once(&ctx, &mut panel, &mut app, egui::vec2(220.0, 420.0));

        let content_rect = panel
            .last_content_rect
            .expect("narrow split content body pane should be allocated");
        let editor_rect = panel
            .last_split_editor_rect
            .expect("narrow split editor pane should be allocated");
        let preview_rect = panel
            .last_split_preview_rect
            .expect("narrow split preview pane should be allocated");
        assert!(
            panel.last_outline_rect.is_none(),
            "closed outline should not allocate an outline pane in narrow no-outline split view"
        );
        assert!(
            content_rect.width() <= app.note_panel_default_size.0 + 1.0,
            "split view should allow a narrow configured dialog width, content={content_rect:?}"
        );
        assert!(
            editor_rect.width() > 0.0 && preview_rect.width() > 0.0,
            "narrow split panes should remain positive, editor={editor_rect:?}, preview={preview_rect:?}"
        );
        assert_split_panes_share_content_width(content_rect, editor_rect, preview_rect);
        assert!(
            content_rect.width() < 240.0,
            "narrow split content should remain too narrow to fit two old 120px minimum panes, content={content_rect:?}"
        );
    }

    #[test]
    fn outline_and_split_view_share_existing_dialog_width() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (640.0, 480.0);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;
        app.note_settings.outline_sidebar_enabled = true;

        let mut panel = NotePanel::from_note(empty_note(
            "# Outline split\n\n## First\n\nBody\n\n## Second\n\nMore body",
        ));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.outline_open = true;
        panel.outline_width = 220.0;

        render_note_body_once(&ctx, &mut panel, &mut app, egui::vec2(640.0, 480.0));

        let outline_rect = panel
            .last_outline_rect
            .expect("outline pane should be allocated with split view");
        let content_rect = panel
            .last_content_rect
            .expect("content pane should be allocated beside outline");
        let editor_rect = panel
            .last_split_editor_rect
            .expect("split editor pane should be allocated beside outline");
        let preview_rect = panel
            .last_split_preview_rect
            .expect("split preview pane should be allocated beside outline");
        let body_width = app.note_panel_default_size.0;

        assert!(
            outline_rect.width() <= 220.0 + 1.0,
            "outline should not exceed configured width, outline={outline_rect:?}"
        );
        assert_outline_and_content_share_body_width(body_width, outline_rect, content_rect);
        assert_split_panes_share_content_width(content_rect, editor_rect, preview_rect);
    }

    #[test]
    fn split_preview_long_unbroken_content_does_not_expand_dialog() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (500.0, 420.0);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;

        let long_unbroken = "x".repeat(10_000);
        let mut panel =
            NotePanel::from_note(empty_note(&format!("# Long preview\n\n{long_unbroken}")));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.outline_open = false;

        render_note_body_once(&ctx, &mut panel, &mut app, egui::vec2(500.0, 420.0));

        let content_rect = panel
            .last_content_rect
            .expect("long preview content body pane should be allocated");
        let editor_rect = panel
            .last_split_editor_rect
            .expect("long preview split editor pane should be allocated");
        let preview_rect = panel
            .last_split_preview_rect
            .expect("long preview split pane should be allocated");

        assert!(
            panel.last_outline_rect.is_none(),
            "closed outline should not allocate an outline pane with long split preview content"
        );
        assert!(
            content_rect.width() <= app.note_panel_default_size.0 + 1.0,
            "long unbroken preview content should not widen the dialog content rect, content={content_rect:?}"
        );
        assert_split_panes_share_content_width(content_rect, editor_rect, preview_rect);
        assert!(
            preview_rect.width() <= content_rect.width() + 1.0,
            "long unbroken preview content should remain bounded by the content rect, content={content_rect:?}, preview={preview_rect:?}"
        );
        assert!(
            preview_rect.width() < 700.0,
            "long unbroken preview content should not expand toward a large screen width, preview={preview_rect:?}"
        );
    }

    #[test]
    fn note_panel_default_size_is_only_runtime_window_default() {
        let source = include_str!("note_panel.rs");
        let tests_start = source
            .find("mod tests")
            .expect("note_panel.rs should contain test module");
        let production_source = &source[..tests_start];

        assert_eq!(
            production_source
                .matches("app.note_panel_default_size")
                .count(),
            1,
            "production note panel code should only read app.note_panel_default_size for Window::default_size"
        );
        assert!(
            production_source.contains(".default_size(app.note_panel_default_size)"),
            "production note panel code should pass app.note_panel_default_size to egui::Window::default_size"
        );
        assert!(
            !production_source.contains("app.note_panel_default_size ="),
            "ordinary note panel rendering must not write measured sizes back to app.note_panel_default_size"
        );
    }

    #[test]
    fn split_ratio_is_clamped_during_split_render() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_panel_default_size = (760.0, 540.0);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.split_view_enabled = true;

        let mut panel = NotePanel::from_note(empty_note("# Split ratio\n\nBody"));
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Split;
        panel.split_ratio = 1.5;

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(raw_input, |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();

        assert_eq!(panel.split_ratio, 0.8);
    }

    #[test]
    fn split_preview_fallback_keeps_long_unbroken_text_inside_preview_pane() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note(""));
        let long_unbroken_preview = "x".repeat(10_000);
        let pane_width = 240.0;
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };

        ctx.run_ui(raw_input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(pane_width, 300.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(pane_width);
                        panel.render_preview_fallback(ui, &mut app, &long_unbroken_preview);
                    },
                );
            });
        })
        .drop_without_applying_deltas();

        let fallback_rect = panel
            .last_preview_fallback_rect
            .expect("fallback TextEdit should be rendered");
        assert!(
            fallback_rect.width() <= pane_width + 1.0,
            "fallback text edit should stay within the preview pane width instead of expanding the dialog, pane_width={pane_width}, fallback={fallback_rect:?}"
        );
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
        assert!(!panel.last_ui_sections.outline_visible);
    }

    #[test]
    fn outline_can_render_while_details_are_hidden() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.outline_sidebar_enabled = true;
        let mut panel = NotePanel::from_note(empty_note("# One\n\n## Two"));
        panel.show_metadata = false;
        panel.outline_open = true;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.outline_visible);
        assert!(panel.last_ui_sections.content_visible);
        assert!(!panel.last_ui_sections.tags_visible);
        assert!(!panel.last_ui_sections.links_visible);

        let outline_rect = panel
            .last_outline_rect
            .expect("outline pane should be allocated while open");
        let content_rect = panel
            .last_content_rect
            .expect("content pane should be allocated while outline is open");
        assert!(
            outline_rect.is_positive() && outline_rect.width() > 1.0 && outline_rect.height() > 1.0,
            "outline pane should have meaningful size, got {outline_rect:?}"
        );
        assert!(
            content_rect.is_positive() && content_rect.width() > 1.0 && content_rect.height() > 1.0,
            "content pane should have meaningful size, got {content_rect:?}"
        );
    }

    #[test]
    fn details_and_outline_can_render_together() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.outline_sidebar_enabled = true;
        let mut panel = NotePanel::from_note(empty_note(
            "# One\n\n#tag [[linked-note]] https://example.com\n\n## Two",
        ));
        panel.show_metadata = true;
        panel.outline_open = true;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.tags_visible);
        assert!(panel.last_ui_sections.links_visible);
        assert!(panel.last_ui_sections.backlinks_visible);
        assert!(panel.last_ui_sections.outline_visible);
        assert!(panel.last_ui_sections.content_visible);
        assert!(
            panel
                .last_outline_rect
                .expect("outline pane should be allocated while open")
                .is_positive()
        );
        assert!(
            panel
                .last_content_rect
                .expect("content pane should be allocated while outline is open")
                .is_positive()
        );
    }

    #[test]
    fn outline_sidebar_renders_empty_state_when_note_has_no_headings() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("Body without headings"));
        panel.outline_open = true;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.outline_visible);
        assert!(panel.last_ui_sections.content_visible);
    }

    #[test]
    fn outline_sidebar_renders_filtered_empty_state_when_no_headings_match() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut panel = NotePanel::from_note(empty_note("# One\n\n## Two"));
        panel.outline_open = true;
        panel.outline_filter = "missing".into();

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(panel.last_ui_sections.outline_visible);
        assert!(panel.last_ui_sections.content_visible);
    }

    #[test]
    fn preview_sections_do_not_use_viewport_height_as_spacing_without_outline() {
        const FPV_LOCATIONS_MARKDOWN: &str = r#"# FPV Drone Flying Locations

[[FPV Random Contracts to fly in areas]]

# Georgia

## West of Warner Robins

Some text.

## Perry

Some text.
"#;

        fn render_heading_gap_at_height(height: f32) -> f32 {
            let ctx = egui::Context::default();
            let mut app = new_app(&ctx);
            app.note_settings.rich_markdown_enabled = true;
            app.note_settings.collapsible_sections_enabled = true;
            app.note_settings.outline_sidebar_enabled = true;

            let mut note = empty_note(FPV_LOCATIONS_MARKDOWN);
            note.title = "FPV Drone Flying Locations".into();
            note.slug = "fpv-drone-flying-locations".into();

            let mut panel = NotePanel::from_note(note);
            panel.outline_open = false;
            panel.show_metadata = false;
            panel.view_mode = NoteViewMode::Preview;
            panel.last_preview_heading_rects.clear();

            render_note_body_once(&ctx, &mut panel, &mut app, egui::vec2(640.0, height));

            assert!(
                panel.last_outline_rect.is_none(),
                "outline should stay closed while outline support is enabled"
            );
            assert!(
                panel.last_preview_heading_rects.len() >= 2,
                "expected at least two recorded preview heading rects at height {height}, got {:?}",
                panel.last_preview_heading_rects
            );

            let first = panel.last_preview_heading_rects[0];
            let second = panel.last_preview_heading_rects[1];
            assert!(
                second.min.y > first.max.y,
                "second heading should appear below the first at height {height}: first={first:?}, second={second:?}"
            );

            let gap = second.min.y - first.max.y;
            assert!(
                gap < 120.0,
                "heading gap should stay bounded and not consume the preview viewport height at height {height}: gap={gap}, first={first:?}, second={second:?}"
            );
            gap
        }

        let gap_at_320_height = render_heading_gap_at_height(320.0);
        let gap_at_800_height = render_heading_gap_at_height(800.0);

        assert!(
            gap_at_800_height <= gap_at_320_height + 20.0,
            "heading gap should not grow with viewport height: gap_at_320_height={gap_at_320_height}, gap_at_800_height={gap_at_800_height}"
        );
    }

    #[test]
    fn horizontal_markdown_fragments_shrink_vertically_with_long_links() {
        const LONG_LINK_MARKDOWN: &str = r#"# First

https://www.google.com/maps/place/very-long-unbroken-url-or-query-string-that-keeps-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going-and-going...

# Second

More text.
"#;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.collapsible_sections_enabled = true;

        let mut note = empty_note(LONG_LINK_MARKDOWN);
        note.title = "Long link preview".into();
        note.slug = "long-link-preview".into();
        let mut panel = NotePanel::from_note(note);
        panel.view_mode = NoteViewMode::Preview;
        panel.show_metadata = false;
        panel.outline_open = false;
        panel.last_preview_heading_rects.clear();

        let note_body_size = egui::vec2(500.0, 700.0);
        render_note_body_once(&ctx, &mut panel, &mut app, note_body_size);

        let content_rect = panel
            .last_content_rect
            .expect("preview content body pane should be allocated");
        assert!(
            content_rect.width() <= note_body_size.x + 1.0,
            "long unbroken URL should not expand the bounded note body width, body_size={note_body_size:?}, content={content_rect:?}"
        );
        assert!(
            panel.last_preview_heading_rects.len() >= 2,
            "expected at least two recorded preview heading rects, got {:?}",
            panel.last_preview_heading_rects
        );

        let first = panel.last_preview_heading_rects[0];
        let second = panel.last_preview_heading_rects[1];
        let gap = second.min.y - first.max.y;
        assert!(
            gap < 160.0,
            "long-link markdown fragment should not consume the vertical viewport; gap was {gap}"
        );
    }

    #[test]
    fn preview_headings_stack_vertically_with_outline_body_row() {
        const FPV_MARKDOWN_SAMPLE: &str = r#"# fpv
#fpv #drone
# Training
[[fpv-training]]
# Youtube channel
[Alec Searle YT](https://www.youtube.com/@alec_searle/videos)
[[fpv-drone-buying]]
[[fpv-random-contracts-to-fly-in-areas]]
# Locations
[[fpv-drone-flying-locations]]
"#;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.note_settings.rich_markdown_enabled = true;
        app.note_settings.collapsible_sections_enabled = true;
        app.note_settings.outline_sidebar_enabled = true;

        let mut note = empty_note(FPV_MARKDOWN_SAMPLE);
        note.title = "fpv".into();
        note.slug = "fpv".into();
        let mut panel = NotePanel::from_note(note);
        panel.show_metadata = false;
        panel.view_mode = NoteViewMode::Preview;
        panel.outline_open = true;

        render_panel_once(&ctx, &mut panel, &mut app);

        assert!(
            panel.last_outline_rect.is_some(),
            "outline pane should be allocated"
        );
        assert!(
            panel.last_content_rect.is_some(),
            "content pane should be allocated"
        );
        assert!(
            panel.last_preview_heading_rects.len() >= 4,
            "expected at least four recorded preview heading rects, got {:?}",
            panel.last_preview_heading_rects
        );
        for pair in panel.last_preview_heading_rects.windows(2) {
            assert!(
                pair[0].min.y < pair[1].min.y,
                "heading rect y positions should strictly increase: {:?}",
                panel.last_preview_heading_rects
            );
        }

        let first_y = panel.last_preview_heading_rects[0].min.y;
        assert!(
            panel
                .last_preview_heading_rects
                .iter()
                .any(|rect| (rect.min.y - first_y).abs() > 1.0),
            "heading rect y positions should not all share approximately the same min.y: {:?}",
            panel.last_preview_heading_rects
        );
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
    fn empty_link_menu_targets_are_cached_at_their_captured_revision() {
        let notes_dir = TempNotesDir::new();
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(note_with_slug("Current", "current"));
        panel.refresh_link_menu_results_if_needed();
        let target_refresh_count = panel.link_menu_target_refresh_count;
        let captured_revision = panel
            .link_menu_targets_version
            .expect("empty targets still have a cache revision");
        assert!(panel.link_menu_targets.is_empty());
        assert_eq!(captured_revision, note_version());
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("results should be keyed after target capture")
                .notes_version,
            captured_revision
        );

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
        assert!(
            panel
                .link_menu_results
                .iter()
                .any(|result| result.display_title == "Alpha")
        );
        assert!(
            !panel
                .link_menu_results
                .iter()
                .any(|result| result.display_title == "Beta")
        );
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
        assert_eq!(panel.link_menu_targets_version, Some(note_version()));
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("link menu results should be cached")
                .notes_version,
            panel.link_menu_targets_version.expect("targets are cached")
        );

        notes_dir.write_note("beta.md", "# Beta\n\nBeta body");
        notes_dir.refresh_cache();
        let results = panel.link_menu_results_snapshot();

        assert!(results.iter().any(|result| result.display_title == "Beta"));
        assert_eq!(panel.link_menu_targets_version, Some(note_version()));
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("link menu results should be cached")
                .notes_version,
            panel.link_menu_targets_version.expect("targets are cached")
        );
    }

    #[test]
    fn link_menu_snapshot_failure_retains_populated_cache_until_recovery() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("current.md", "# Current\n\nCurrent body");
        notes_dir.write_note("before.md", "# Before\n\nBefore body");
        notes_dir.refresh_cache();

        let current = note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("current note should be cached");
        let mut panel = NotePanel::from_note(current);
        let previous_results = panel.link_menu_results_snapshot();
        let previous_targets = panel.link_menu_targets.clone();
        let previous_version = panel
            .link_menu_targets_version
            .expect("initial targets should have a revision");
        let previous_key = panel
            .link_menu_results_key
            .clone()
            .expect("initial results should have a key");

        notes_dir.write_note("before.md", "# Before Updated\n\nUpdated body");
        notes_dir.write_note("after.md", "# After\n\nAfter body");
        notes_dir.refresh_cache();
        let changed_version = note_version();
        assert_ne!(changed_version, previous_version);

        panel.fail_next_link_menu_snapshot = true;
        assert_eq!(panel.link_menu_results_snapshot(), previous_results);
        assert_eq!(panel.link_menu_targets, previous_targets);
        assert_eq!(panel.link_menu_targets_version, Some(previous_version));
        assert_eq!(panel.link_menu_results_key, Some(previous_key));

        let recovered_results = panel.link_menu_results_snapshot();
        let target_slugs = panel
            .link_menu_targets
            .iter()
            .map(|target| target.slug.as_str())
            .collect::<HashSet<_>>();
        assert!(target_slugs.contains("after"));
        assert!(target_slugs.contains("before"));
        assert_eq!(panel.link_menu_targets_version, Some(changed_version));
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("recovered results should be keyed")
                .notes_version,
            changed_version
        );
        assert!(
            recovered_results
                .iter()
                .any(|result| result.slug == "after")
        );
        assert!(
            recovered_results.iter().any(|result| {
                result.slug == "before" && result.display_title == "Before Updated"
            })
        );
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

    fn app_with_note_ui_state_path(ctx: &egui::Context, dir: &TempDir) -> LauncherApp {
        let mut app = new_app(ctx);
        let settings_path = dir.path().join("settings.json");
        app.settings_path = settings_path.to_string_lossy().to_string();
        app.note_settings.collapsible_sections_enabled = true;
        app.note_settings.collapsed_sections_persist = true;
        app
    }

    fn note_with_collapsible_sections() -> Note {
        Note {
            title: "Project".into(),
            path: std::path::PathBuf::new(),
            content: "# Project\n\nIntro\n\n## Alpha\n\nA\n\n## Beta\n\nB\n\n# Appendix\n\nC"
                .into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: "project".into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    #[test]
    fn expand_all_clears_runtime_and_persisted_collapsed_sections() {
        let ctx = egui::Context::default();
        let dir = tempdir().expect("tempdir");
        let mut app = app_with_note_ui_state_path(&ctx, &dir);
        let mut panel = NotePanel::from_note(note_with_collapsible_sections());
        let sections = panel.collapsible_sections(true);
        panel.collapse_all_collapsible_sections(&mut app, &sections);
        assert!(!panel.collapsed_sections.is_empty());

        panel.expand_all_collapsible_sections(&mut app);

        assert!(panel.collapsed_sections.is_empty());
        let state_path = crate::note_ui_state::path_for_settings(Path::new(&app.settings_path));
        let state = crate::note_ui_state::load(&state_path).expect("load note ui state");
        assert!(state.collapsed_sections_for("project").is_empty());
    }

    #[test]
    fn collapse_all_persists_non_main_title_sections_only() {
        let ctx = egui::Context::default();
        let dir = tempdir().expect("tempdir");
        let mut app = app_with_note_ui_state_path(&ctx, &dir);
        let mut panel = NotePanel::from_note(note_with_collapsible_sections());
        let sections = panel.collapsible_sections(true);

        panel.collapse_all_collapsible_sections(&mut app, &sections);

        let expected = sections
            .iter()
            .filter(|section| !panel.is_main_title_section(section))
            .map(|section| panel.section_key(section))
            .collect::<HashSet<_>>();
        let main_title_key = panel.section_key(&sections[0]);
        assert_eq!(panel.collapsed_sections, expected);
        assert!(!panel.collapsed_sections.contains(&main_title_key));

        let state_path = crate::note_ui_state::path_for_settings(Path::new(&app.settings_path));
        let state = crate::note_ui_state::load(&state_path).expect("load note ui state");
        assert_eq!(state.collapsed_sections_for("project"), expected);
    }

    #[test]
    fn collapse_all_updates_runtime_state_without_persistence_when_disabled() {
        let ctx = egui::Context::default();
        let dir = tempdir().expect("tempdir");
        let mut app = app_with_note_ui_state_path(&ctx, &dir);
        app.note_settings.collapsed_sections_persist = false;
        let mut panel = NotePanel::from_note(note_with_collapsible_sections());
        let sections = panel.collapsible_sections(true);

        panel.collapse_all_collapsible_sections(&mut app, &sections);

        assert!(!panel.collapsed_sections.is_empty());
        let state_path = crate::note_ui_state::path_for_settings(Path::new(&app.settings_path));
        assert!(
            !state_path.exists(),
            "disabled persistence should not create note UI state"
        );
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
        let [min, max] = range.sorted_cursors();
        assert_eq!((min.index.0, max.index.0), (2, 7));
        assert_eq!(panel.pending_selection, Some((2, 7)));
    }

    #[test]
    fn character_range_policy_sorts_clamps_and_rejects_empty_ranges() {
        assert_eq!(normalize_char_range((8, 2), 10), Some((2, 8)));
        assert_eq!(normalize_char_range((3, 3), 10), None);
        assert_eq!(normalize_char_range((2, 99), 5), Some((2, 5)));
        assert_eq!(normalize_char_range((8, 99), 5), None);
    }

    #[test]
    fn right_click_range_boundaries_are_inclusive() {
        let range = (2, 5);
        assert!(caret_is_within_range(2, range));
        assert!(caret_is_within_range(5, range));
        assert!(!caret_is_within_range(1, range));
        assert!(!caret_is_within_range(6, range));
    }

    #[test]
    fn collapsed_live_cursor_does_not_override_cached_selection() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("selection_resolver");
        let mut panel = NotePanel::from_note(empty_note("alpha βeta 😀 gamma"));
        panel.pending_selection = Some((6, 12));
        let mut state = egui::widgets::text_edit::TextEditState::default();
        collapse_cursor(&mut state, 0);
        state.store(&ctx, id);

        assert_eq!(panel.resolve_selection(&ctx, id), Some((6, 12)));
    }

    #[test]
    fn unicode_stale_selection_replacement_is_safe() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("unicode_replace");
        let mut panel = NotePanel::from_note(empty_note("alpha\nβeta 😀\ngamma"));
        panel.pending_selection = Some((6, 99));

        let outcome = panel.insert_text_at_cursor_or_selection(&ctx, id, "replacement");

        assert_eq!(panel.note.content, "alpha\nreplacement");
        assert!(panel.pending_selection.is_none());
        assert_eq!(
            outcome,
            TextInsertionOutcome {
                kind: TextInsertionKind::ReplacedSelection,
                resulting_char_index: 17,
            }
        );
    }

    #[test]
    fn timestamp_insertion_at_caret_preserves_surrounding_text_and_collapses_cursor() {
        const TIMESTAMP: &str = "20260808-202512.347";
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            time: Some(42.25),
            ..Default::default()
        });
        let id = egui::Id::new("timestamp_at_caret");
        let mut panel = NotePanel::from_note(empty_note("Hello world"));
        let mut state = egui::widgets::text_edit::TextEditState::default();
        collapse_cursor(&mut state, 6);
        state.store(&ctx, id);
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, None);

        let outcome = panel.insert_text_at_cursor_or_selection(&ctx, id, TIMESTAMP);

        assert_eq!(panel.note.content, "Hello 20260808-202512.347world");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap();
        assert_eq!(range.primary.index.0, range.secondary.index.0);
        assert_eq!(range.primary.index.0, 6 + TIMESTAMP.chars().count());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(42.25));
        assert_eq!(
            outcome,
            TextInsertionOutcome {
                kind: TextInsertionKind::InsertedAtCaret,
                resulting_char_index: 6 + TIMESTAMP.chars().count(),
            }
        );
        ctx.end_pass().drop_without_applying_deltas();
    }

    #[test]
    fn timestamp_replaces_pending_selection_and_clears_it() {
        const TIMESTAMP: &str = "20260808-202512.347";
        let ctx = egui::Context::default();
        let id = egui::Id::new("timestamp_selection");
        let mut panel = NotePanel::from_note(empty_note("Before selected text after"));
        panel.pending_selection = Some((7, 20));

        panel.insert_text_at_cursor_or_selection(&ctx, id, TIMESTAMP);

        assert_eq!(panel.note.content, "Before 20260808-202512.347 after");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap();
        assert_eq!(range.primary.index.0, range.secondary.index.0);
        assert_eq!(range.primary.index.0, 7 + TIMESTAMP.chars().count());
        assert!(panel.pending_selection.is_none());
    }

    #[test]
    fn timestamp_replacement_uses_character_indices_for_unicode_content() {
        const TIMESTAMP: &str = "20260808-202512.347";
        let ctx = egui::Context::default();
        let id = egui::Id::new("timestamp_unicode_selection");
        let mut panel = NotePanel::from_note(empty_note("alpha 😀 βeta"));
        panel.pending_selection = Some((6, 9));

        panel.insert_text_at_cursor_or_selection(&ctx, id, TIMESTAMP);

        assert_eq!(panel.note.content, "alpha 20260808-202512.347eta");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap();
        assert_eq!(range.primary.index.0, range.secondary.index.0);
        assert_eq!(range.primary.index.0, 6 + TIMESTAMP.chars().count());
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
        let idx = state.cursor.char_range().unwrap().primary.index.0;
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
        let idx = state.cursor.char_range().unwrap().primary.index.0;
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
        let idx = state.cursor.char_range().unwrap().primary.index.0;
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
        ctx.run_ui(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = show_wiki_link(ui, &mut app, "Second Note").rect;
            });
        })
        .drop_without_applying_deltas();
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

        ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show_wiki_link(ui, &mut app, "Second Note");
            });
        })
        .drop_without_applying_deltas();

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

        let output = ctx.run_ui(Default::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |_ui| {
                let mut panel = app.note_panels.remove(0);
                panel.ui(&frame_context, &mut app);
                app.note_panels.insert(0, panel);
            });
        });
        output.drop_without_applying_deltas();

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

        let output = ctx.run_ui(input, |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |_ui| {
                let mut panel = app.note_panels.remove(0);
                panel.ui(&frame_context, &mut app);
                app.note_panels.insert(0, panel);
            });
        });
        output.drop_without_applying_deltas();

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
        let output =
            ctx.run_ui(Default::default(), |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    frame_context.output_mut(|output| {
                        output.commands.push(egui::OutputCommand::OpenUrl(
                            egui::OpenUrl::same_tab("www.example.com"),
                        ));
                    });
                    handle_markdown_links(ui, &mut app);
                });
            });
        assert_eq!(
            output
                .platform_output
                .commands
                .iter()
                .find_map(|command| match command {
                    egui::OutputCommand::OpenUrl(open_url) => Some(open_url.url.as_str()),
                    _ => None,
                })
                .expect("external markdown links retain an OpenUrl output command"),
            "https://www.example.com"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn classify_markdown_link_references_and_external_urls() {
        assert_eq!(
            classify_markdown_link("note://abc"),
            MarkdownLinkAction::NoteReference("abc".into())
        );
        assert_eq!(
            classify_markdown_link("todo://abc"),
            MarkdownLinkAction::TodoReference("abc".into())
        );
        for url in ["http://example.com", "https://example.com"] {
            assert_eq!(
                classify_markdown_link(url),
                MarkdownLinkAction::ExternalUrl(url.into())
            );
        }
    }

    #[test]
    fn file_urls_resolve_to_existing_explorer_actions_without_external_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("note.txt");
        std::fs::write(&file, "note").unwrap();

        let directory_url = Url::from_directory_path(temp.path()).unwrap().to_string();
        let file_url = Url::from_file_path(&file).unwrap().to_string();
        assert_eq!(
            classify_markdown_link(&directory_url),
            MarkdownLinkAction::Filesystem(ExplorerAction::OpenDirectory(
                temp.path().to_path_buf()
            ))
        );
        assert_eq!(
            classify_markdown_link(&file_url),
            MarkdownLinkAction::Filesystem(ExplorerAction::RevealFile(file.clone()))
        );
        assert_eq!(
            resolve_explorer_action(temp.path()).unwrap(),
            ExplorerAction::OpenDirectory(temp.path().to_path_buf())
        );
        assert_eq!(
            resolve_explorer_action(&file).unwrap(),
            ExplorerAction::RevealFile(file)
        );
        assert!(!matches!(
            classify_markdown_link(&file_url),
            MarkdownLinkAction::ExternalUrl(_)
        ));
    }

    fn assert_filesystem_link_error_is_not_forwarded(target: &str) {
        assert!(matches!(
            classify_markdown_link(target),
            MarkdownLinkAction::InvalidFilesystemUrl(_)
        ));

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_inline_errors = true;
        let output =
            ctx.run_ui(Default::default(), |root_ui| {
                let frame_context = root_ui.ctx().clone();
                egui::CentralPanel::default().show(root_ui, |ui| {
                    frame_context.output_mut(|output| {
                        output.commands.push(egui::OutputCommand::OpenUrl(
                            egui::OpenUrl::same_tab(target),
                        ));
                    });
                    handle_markdown_links(ui, &mut app);
                });
            });
        assert!(app.error.is_some());
        assert!(
            !output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::OpenUrl(_)))
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn deleted_file_url_reports_error_without_external_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("deleted.txt");
        std::fs::write(&file, "note").unwrap();
        let file_url = Url::from_file_path(&file).unwrap().to_string();
        std::fs::remove_file(file).unwrap();

        assert_filesystem_link_error_is_not_forwarded(&file_url);
    }

    #[test]
    fn non_convertible_file_url_reports_error_without_external_fallback() {
        let file_url = "file:relative";
        assert!(Url::parse(file_url).is_ok());
        assert_filesystem_link_error_is_not_forwarded(file_url);
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

        let prepared = backlink_rows_for_note(&current, &todos, &notes);

        assert_eq!(prepared.linked_todos.len(), 1);
        assert_eq!(prepared.related_notes.len(), 1);
        assert!(!prepared.mentions.is_empty());
    }

    #[test]
    fn prepared_backlink_rows_match_frozen_eager_categories_and_order() {
        use crate::common::entity_ref::{EntityKind, EntityRef};
        use crate::plugins::todo::TodoEntry;

        let make_note =
            |title: &str, slug: &str, content: &str, entity_refs: Vec<EntityRef>| Note {
                title: title.into(),
                path: std::path::PathBuf::new(),
                content: content.into(),
                tags: Vec::new(),
                links: Vec::new(),
                slug: slug.into(),
                alias: None,
                aliases: Vec::new(),
                entity_refs,
            };
        let make_todo = |id: &str, text: &str, entity_refs: Vec<EntityRef>| TodoEntry {
            id: id.into(),
            text: text.into(),
            done: false,
            priority: 0,
            tags: Vec::new(),
            entity_refs,
        };
        let note_ref = || EntityRef::new(EntityKind::Note, "central-note", None);
        let compare = |current: &Note, todos: &[TodoEntry], notes: &[Note]| {
            let prepared = backlink_rows_for_note(current, todos, notes);
            let actual = [
                backlink_rows_signature(&prepared.linked_todos),
                backlink_rows_signature(&prepared.related_notes),
                backlink_rows_signature(&prepared.mentions),
            ];
            let expected = [
                BacklinkTab::LinkedTodos,
                BacklinkTab::RelatedNotes,
                BacklinkTab::Mentions,
            ]
            .into_iter()
            .map(|tab| {
                backlink_rows_signature(&track_c_eager_note_rows(current, tab, todos, notes))
            })
            .collect::<Vec<_>>();
            assert_eq!(actual.as_slice(), expected.as_slice());
            prepared
        };

        let empty_current = make_note("Current", "central-note", "# Current", Vec::new());
        let empty_prepared = compare(&empty_current, &[], &[]);
        assert!(empty_prepared.linked_todos.is_empty());
        let self_only = vec![empty_current.clone()];
        let self_prepared = compare(&empty_current, &[], &self_only);
        assert!(self_prepared.related_notes.is_empty());
        assert!(self_prepared.mentions.is_empty());

        let current = make_note(
            "Central Note",
            "central-note",
            "# Central Note\nAlias: Primary Alias\nAliases: Secondary Alias\n\n@todo:todo-draft",
            vec![EntityRef::new(EntityKind::Todo, "todo-typed", None)],
        );
        let raw_notes = vec![
            current.clone(),
            make_note(
                "Primary Link",
                "primary-link",
                "# Primary Link\n\n**nested [[Primary Alias]] and [[Secondary Alias]], duplicate [[Primary Alias]]**",
                Vec::new(),
            ),
            make_note(
                "Wiki Wins",
                "wiki-wins",
                "# Wiki Wins\n\n[[Central Note]] @note:central-note",
                vec![note_ref()],
            ),
            make_note(
                "Entity Wins",
                "entity-wins",
                "# Entity Wins\n\n@note:central-note [[Unknown Target]]",
                vec![note_ref()],
            ),
            make_note(
                "Text Mention",
                "text-mention",
                &format!(
                    "# Text Mention\n\nUnicode 東京 before @note:central-note {}",
                    "z".repeat(60)
                ),
                Vec::new(),
            ),
            make_note(
                "Fenced Wiki",
                "fenced-wiki",
                "# Fenced Wiki\n\n```md\n[[Central Note]]\n```",
                Vec::new(),
            ),
            make_note(
                "Fenced Prose",
                "fenced-prose",
                "# Fenced Prose\n\n```md\n@note:central-note\n```",
                Vec::new(),
            ),
            make_note(
                "Ambiguous Source",
                "ambiguous-source",
                "# Ambiguous Source\n\n[[Ambiguous Alias]]",
                Vec::new(),
            ),
            make_note(
                "Collision One",
                "collision-one",
                "# Collision One\nAlias: Ambiguous Alias",
                Vec::new(),
            ),
            make_note(
                "Collision Two",
                "collision-two",
                "# Collision Two\nAlias: Ambiguous Alias",
                Vec::new(),
            ),
        ];
        let mut todos = vec![
            make_todo(
                "todo-overlap",
                "Both [[Central Note]] and @note:central-note",
                vec![note_ref()],
            ),
            make_todo(
                "todo-needle-order",
                "Both [[Central Note]] and @note:central-note",
                Vec::new(),
            ),
            make_todo("todo-draft", "Draft-only todo", Vec::new()),
            make_todo("todo-typed", "Typed outgoing todo", Vec::new()),
            make_todo(
                "",
                "Empty identity with persisted reference",
                vec![note_ref()],
            ),
            make_todo("unrelated", "No relationship", Vec::new()),
        ];
        todos.push(make_todo(
            "todo-overlap",
            "Second source with duplicate TODO ID",
            vec![note_ref()],
        ));
        todos.push(todos[1].clone());

        let base_cache = crate::plugins::note::publish_note_cache_for_test(raw_notes.clone());
        let (_, canonical_notes) = crate::plugins::note::note_cache_snapshot_with_version()
            .expect("canonical fixture publication is healthy");
        let canonical_current = &canonical_notes[0];
        assert_eq!(canonical_current.alias.as_deref(), Some("Primary Alias"));
        assert!(
            canonical_current
                .aliases
                .iter()
                .any(|alias| alias.as_str() == "Secondary Alias")
        );
        let prepared = compare(canonical_current, &todos, &canonical_notes);
        let linked = backlink_rows_signature(&prepared.linked_todos);
        let mentions = backlink_rows_signature(&prepared.mentions);
        let related = backlink_rows_signature(&prepared.related_notes);
        let ordered_todo_rows = |rows: &[BacklinkRow]| {
            rows.iter()
                .map(|row| (row.title.clone(), row.todo_id.clone()))
                .collect::<Vec<_>>()
        };
        let expected_todo_order = [
            (
                "Both [[Central Note]] and @note:central-note",
                Some("todo-overlap"),
            ),
            (
                "Both [[Central Note]] and @note:central-note",
                Some("todo-needle-order"),
            ),
            ("Draft-only todo", Some("todo-draft")),
            ("Typed outgoing todo", Some("todo-typed")),
            ("Empty identity with persisted reference", Some("")),
            ("Second source with duplicate TODO ID", Some("todo-overlap")),
            (
                "Both [[Central Note]] and @note:central-note",
                Some("todo-needle-order"),
            ),
        ]
        .into_iter()
        .map(|(title, id)| (title.to_owned(), id.map(str::to_owned)))
        .collect::<Vec<_>>();
        assert_eq!(
            ordered_todo_rows(&prepared.linked_todos),
            expected_todo_order
        );
        assert_eq!(
            ordered_todo_rows(&prepared.mentions[..linked.len()]),
            expected_todo_order,
            "Mentions retains the complete ordered TODO prefix"
        );
        assert_eq!(
            prepared
                .related_notes
                .iter()
                .map(|row| row.note_slug.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec!["primary-link", "wiki-wins", "fenced-wiki"]
        );
        assert_eq!(
            prepared
                .mentions
                .iter()
                .filter_map(|row| row.note_slug.as_deref())
                .collect::<Vec<_>>(),
            vec!["entity-wins", "text-mention"]
        );
        assert!(
            prepared
                .related_notes
                .iter()
                .find(|row| row.note_slug.as_deref() == Some("fenced-wiki"))
                .is_some_and(|row| row.reason == "wiki link"),
            "raw canonical wiki links retain precedence even when their source text is fenced"
        );
        assert_eq!(prepared.linked_todos[0].reason, "todo linked to note");
        assert_eq!(prepared.linked_todos[1].reason, "note mention");
        assert_eq!(prepared.linked_todos[2].reason, "note mentions todo");
        assert_eq!(prepared.linked_todos[4].todo_id.as_deref(), Some(""));
        assert_eq!(prepared.related_notes[1].reason, "wiki link");
        assert_eq!(prepared.mentions[7].reason, "entity reference");
        assert_eq!(prepared.mentions[8].reason, "note mention");
        assert!(prepared.mentions.iter().all(|row| {
            row.note_slug.as_deref() != Some("fenced-prose")
                && row.note_slug.as_deref() != Some("ambiguous-source")
        }));
        assert!(
            prepared
                .mentions
                .iter()
                .find(|row| row.note_slug.as_deref() == Some("text-mention"))
                .is_some_and(|row| row.snippet.contains("東京"))
        );
        assert_eq!(linked.as_slice(), &mentions[..linked.len()]);
        assert_ne!(
            linked[0], linked[5],
            "distinct source TODOs with the same ID retain their separate complete rows"
        );
        assert_eq!(
            linked[1], linked[6],
            "an exact repeated source TODO retains its duplicate complete row"
        );
        assert_eq!(
            linked.len(),
            7,
            "matched todos keep source order and no dedup"
        );
        assert_eq!(related.len(), 3);
        assert_eq!(mentions.len(), 9);

        let mut transitioned_notes = raw_notes;
        transitioned_notes[0].content =
            "# Central Note\nAlias: Secondary Alias\nAliases: Primary Alias\n\n@todo:todo-draft"
                .into();
        let transitioned_cache =
            crate::plugins::note::publish_note_cache_for_test(transitioned_notes);
        let (_, transitioned) = crate::plugins::note::note_cache_snapshot_with_version()
            .expect("primary/secondary alias transition publishes canonically");
        let transitioned_current = &transitioned[0];
        assert_eq!(
            transitioned_current.alias.as_deref(),
            Some("Secondary Alias")
        );
        assert!(
            transitioned_current
                .aliases
                .iter()
                .any(|alias| alias.as_str() == "Primary Alias")
        );
        let transitioned_rows = compare(transitioned_current, &todos, &transitioned);
        assert_eq!(
            backlink_rows_signature(&transitioned_rows.linked_todos),
            linked
        );
        assert_eq!(
            backlink_rows_signature(&transitioned_rows.related_notes),
            related
        );
        assert_eq!(
            backlink_rows_signature(&transitioned_rows.mentions),
            mentions
        );
        drop(transitioned_cache);
        drop(base_cache);
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
    fn backlinks_disabled_constructor_skips_initial_backlink_rows() {
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("current.md", "# Current\n\nBody");
        notes_dir.write_note("other.md", "# Other\n\n[[Current]]");
        notes_dir.refresh_cache();

        let note = crate::plugins::note::note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("current note should exist in cache");

        let mut settings = NoteSettings::default();
        settings.backlinks_enabled = false;
        let panel = NotePanel::from_note_with_details_and_settings(note, true, &settings);

        assert!(panel.derived.backlink_rows_linked_todos.is_empty());
        assert!(panel.derived.backlink_rows_related_notes.is_empty());
        assert!(panel.derived.backlink_rows_mentions.is_empty());
        assert_eq!(panel.heavy_recompute_count, 0);
    }

    #[test]
    fn backlink_setting_transitions_rebuild_only_when_enabled_and_preserve_rows() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("current.md", "# Current\n\nBody");
        notes_dir.write_note("other.md", "# Other\n\n[[Current]]");
        notes_dir.refresh_cache();

        let current = note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("current note should exist in cache");
        let mut panel = NotePanel::from_note_with_details_and_view_mode_and_backlink_setting(
            current,
            true,
            NoteViewMode::Preview,
            false,
        );
        assert_eq!(panel.heavy_recompute_count, 0);
        assert!(panel.derived.backlink_rows_related_notes.is_empty());

        let ctx = egui::Context::default();
        let refresh_with_setting = |panel: &mut NotePanel, enabled| {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(1.0),
                    ..Default::default()
                },
                |ctx| panel.maybe_refresh_heavy_derived(ctx, enabled),
            )
            .drop_without_applying_deltas();
        };

        refresh_with_setting(&mut panel, true);
        assert_eq!(panel.heavy_recompute_count, 1);
        assert_eq!(panel.last_backlinks_enabled, Some(true));
        assert_eq!(panel.derived.backlink_rows_related_notes.len(), 1);
        let enabled_rows = backlink_rows_signature(&panel.derived.backlink_rows_related_notes);

        refresh_with_setting(&mut panel, false);
        assert_eq!(panel.heavy_recompute_count, 1);
        assert_eq!(panel.last_backlinks_enabled, Some(false));

        refresh_with_setting(&mut panel, true);
        assert_eq!(panel.heavy_recompute_count, 2);
        assert_eq!(panel.last_backlinks_enabled, Some(true));
        assert_eq!(
            backlink_rows_signature(&panel.derived.backlink_rows_related_notes),
            enabled_rows
        );
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

        let prepared = backlink_rows_for_note(&notes[0], &[], &notes);
        let titles: Vec<_> = prepared
            .related_notes
            .iter()
            .map(|row| row.title.as_str())
            .collect();

        assert_eq!(titles, vec!["linked"]);
        assert!(
            prepared
                .related_notes
                .iter()
                .any(|row| row.reason == "link id" || row.reason == "wiki alias")
        );
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
    fn initializes_default_view_mode_to_preview_from_default_settings() {
        let settings = NoteSettings::default();

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert_eq!(settings.default_view_mode, NoteViewMode::Preview);
        assert_eq!(panel.view_mode, NoteViewMode::Preview);
    }

    #[test]
    fn initializes_default_view_mode_to_split_when_enabled() {
        let mut settings = NoteSettings::default();
        settings.default_view_mode = NoteViewMode::Split;
        settings.split_view_enabled = true;
        settings.rich_markdown_enabled = true;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert_eq!(panel.view_mode, NoteViewMode::Split);
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
    fn outline_opens_by_default_when_enabled_in_settings() {
        let mut settings = NoteSettings::default();
        settings.outline_sidebar_default_open = true;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert!(panel.outline_open);
    }

    #[test]
    fn outline_stays_closed_by_default_when_disabled_in_settings() {
        let mut settings = NoteSettings::default();
        settings.outline_sidebar_default_open = false;

        let panel =
            NotePanel::from_note_with_details_and_settings(empty_note("body"), true, &settings);

        assert!(!panel.outline_open);
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
        ctx.run_ui(Default::default(), |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();
        ctx.run_ui(Default::default(), |ctx| {
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();
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
            ctx.run_ui(Default::default(), |ctx| {
                panel.ui(ctx, &mut app);
            })
            .drop_without_applying_deltas();
        }

        assert_eq!(panel.heavy_recompute_count, initial);
    }

    #[test]
    fn unchanged_heavy_check_skips_note_snapshot_alias_hash_and_recompute() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        let ctx = egui::Context::default();
        crate::performance::reset_metrics();

        panel.maybe_refresh_heavy_derived(&ctx, true);

        assert_eq!(panel.heavy_recompute_count, initial_recomputes);
        if crate::performance::enabled() {
            let metrics = crate::performance::snapshot_metrics();
            assert_eq!(
                metrics[crate::performance::Metric::NoteSnapshot as usize].calls,
                0
            );
            assert_eq!(
                metrics[crate::performance::Metric::NoteAliasHash as usize].calls,
                0
            );
            assert_eq!(
                metrics[crate::performance::Metric::NoteHeavyRecompute as usize].calls,
                0
            );
        }
    }

    #[test]
    fn pending_external_mutation_without_edit_time_refreshes_immediately() {
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        panel.invalidate_note_derived_data_after_external_mutation();

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();

        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert!(!panel.heavy_recompute_requested);
    }

    #[test]
    fn todo_revision_change_bypasses_local_edit_debounce() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        panel.mark_content_changed(f64::MAX);
        panel.last_todo_revision = todo_version().wrapping_add(1);

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();

        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert!(!panel.heavy_recompute_requested);
    }

    #[test]
    fn persisted_todo_text_and_note_reference_refresh_linked_rows() {
        use crate::common::entity_ref::{EntityKind, EntityRef};
        use crate::plugins::todo::{TodoEntry, replace_todos};

        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = workspace.root().join("notes");
        fs::write(
            notes_dir.join("current.md"),
            "# Current\n\nCurrent note body",
        )
        .expect("write isolated current note");
        crate::plugins::note::refresh_cache().expect("publish current note");
        let current = note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("current note should be cached");
        let mut panel = NotePanel::from_note(current);
        let ctx = egui::Context::default();
        run_heavy_refresh_at(&mut panel, &ctx, 0.0);
        assert!(panel.derived.backlink_rows_linked_todos.is_empty());

        let todo_id = "persisted-todo";
        let previous_revision = todo_version();
        replace_todos(
            crate::plugins::todo::TODO_FILE,
            vec![TodoEntry {
                id: todo_id.into(),
                text: "Unlinked saved label".into(),
                done: false,
                priority: 1,
                tags: Vec::new(),
                entity_refs: Vec::new(),
            }],
        )
        .expect("persist an initially unlinked todo");
        assert_ne!(todo_version(), previous_revision);
        run_heavy_refresh_at(&mut panel, &ctx, 0.1);
        assert!(panel.derived.backlink_rows_linked_todos.is_empty());
        assert_eq!(
            panel
                .derived
                .todo_label_map
                .get(todo_id)
                .map(String::as_str),
            Some("Unlinked saved label")
        );

        let previous_revision = todo_version();
        replace_todos(
            crate::plugins::todo::TODO_FILE,
            vec![TodoEntry {
                id: todo_id.into(),
                text: "Readable linked todo label".into(),
                done: false,
                priority: 1,
                tags: vec!["release".into()],
                entity_refs: vec![EntityRef::new(EntityKind::Note, "current", None)],
            }],
        )
        .expect("persist edited todo text and note reference");
        assert_ne!(todo_version(), previous_revision);
        run_heavy_refresh_at(&mut panel, &ctx, 0.2);

        let linked = &panel.derived.backlink_rows_linked_todos;
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].todo_id.as_deref(), Some(todo_id));
        assert_eq!(linked[0].title, "Readable linked todo label");
        assert_eq!(linked[0].type_badge, "Todo");
        assert_eq!(linked[0].reason, "todo linked to note");
        assert_eq!(
            panel
                .derived
                .todo_label_map
                .get(todo_id)
                .map(String::as_str),
            Some("Readable linked todo label")
        );

        let previous_revision = todo_version();
        replace_todos(
            crate::plugins::todo::TODO_FILE,
            vec![TodoEntry {
                id: todo_id.into(),
                text: "Reference removed".into(),
                done: false,
                priority: 1,
                tags: vec!["release".into()],
                entity_refs: Vec::new(),
            }],
        )
        .expect("persist todo reference removal");
        assert_ne!(todo_version(), previous_revision);
        run_heavy_refresh_at(&mut panel, &ctx, 0.3);
        assert!(panel.derived.backlink_rows_linked_todos.is_empty());
        assert_eq!(
            panel
                .derived
                .todo_label_map
                .get(todo_id)
                .map(String::as_str),
            Some("Reference removed")
        );
    }

    #[test]
    fn persisted_note_revision_bypasses_local_edit_debounce() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nBody");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(empty_note("# Current\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        panel.mark_content_changed(f64::MAX);
        notes_dir.write_note("beta.md", "# Beta\n\nBody");
        notes_dir.refresh_cache();

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();

        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert_eq!(panel.last_notes_version, note_version());
        assert!(!panel.heavy_recompute_requested);
    }

    #[test]
    fn open_unsaved_panel_tracks_external_note_and_alias_transitions() {
        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = workspace.root().join("notes");
        let write_note = |file_name: &str, content: &str| {
            fs::write(notes_dir.join(file_name), content).expect("write isolated note");
        };
        write_note(
            "current.md",
            "# Current\nAlias: Primary Alias\nAliases: Secondary Alias\n\nSaved body",
        );
        write_note("primary-source.md", "# Primary Source\n\n[[Primary Alias]]");
        write_note(
            "secondary-source.md",
            "# Secondary Source\n\n[[Secondary Alias]]",
        );
        write_note("mention-source.md", "# Mention Source\n\n@note:current");
        crate::plugins::note::refresh_cache().expect("publish initial notes");

        let current = note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("current note should be cached");
        let mut panel = NotePanel::from_note(current);
        let ctx = egui::Context::default();
        run_heavy_refresh_at(&mut panel, &ctx, 0.0);
        assert_eq!(
            related_note_row_identity_text(&panel),
            vec![
                (
                    "primary-source".into(),
                    "Primary Source".into(),
                    "wiki link".into(),
                ),
                (
                    "secondary-source".into(),
                    "Secondary Source".into(),
                    "wiki link".into(),
                ),
            ]
        );
        assert_eq!(
            mentioned_note_row_identity_text(&panel),
            vec![(
                "mention-source".into(),
                "Mention Source".into(),
                "entity reference".into(),
            )]
        );

        panel.note.content.push_str("\nUnsaved panel draft");
        panel.mark_content_changed(0.01);
        assert!(panel.has_unsaved_changes());

        let refresh_after_external_change = |panel: &mut NotePanel, time: f64| {
            let previous_revision = note_version();
            crate::plugins::note::refresh_cache().expect("refresh externally changed notes");
            assert_ne!(note_version(), previous_revision);
            run_heavy_refresh_at(panel, &ctx, time);
            assert_eq!(panel.last_notes_version, note_version());
            assert!(panel.has_unsaved_changes());
            assert!(panel.note.content.contains("Unsaved panel draft"));
        };

        write_note(
            "created-source.md",
            "# Created Source\n\n[[Secondary Alias]]",
        );
        refresh_after_external_change(&mut panel, 0.1);
        assert_eq!(
            related_note_row_identity_text(&panel),
            vec![
                (
                    "created-source".into(),
                    "Created Source".into(),
                    "wiki link".into(),
                ),
                (
                    "primary-source".into(),
                    "Primary Source".into(),
                    "wiki link".into(),
                ),
                (
                    "secondary-source".into(),
                    "Secondary Source".into(),
                    "wiki link".into(),
                ),
            ]
        );
        let initial_menu = panel.link_menu_results_snapshot();
        assert!(
            initial_menu
                .iter()
                .any(|result| result.slug == "created-source")
        );
        assert_eq!(panel.link_menu_targets_version, Some(note_version()));
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("initial menu results should be keyed")
                .notes_version,
            panel
                .link_menu_targets_version
                .expect("targets are current")
        );

        write_note(
            "current.md",
            "# Current\nAlias: Primary Renamed\nAliases: Secondary Renamed\n\nSaved body",
        );
        write_note(
            "primary-source.md",
            "# Primary Source\n\n[[Primary Renamed]]",
        );
        write_note(
            "secondary-source.md",
            "# Secondary Source\n\n[[Secondary Renamed]]",
        );
        write_note(
            "created-source.md",
            "# Created Source\n\n[[Secondary Renamed]]",
        );
        refresh_after_external_change(&mut panel, 0.2);
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![
                ("created-source".into(), "Created Source".into()),
                ("primary-source".into(), "Primary Source".into()),
                ("secondary-source".into(), "Secondary Source".into()),
            ]
        );
        assert_eq!(
            mentioned_note_row_identity_text(&panel),
            vec![(
                "mention-source".into(),
                "Mention Source".into(),
                "entity reference".into(),
            )]
        );
        let _ = panel.link_menu_results_snapshot();
        let current_target = panel
            .link_menu_targets
            .iter()
            .find(|target| target.slug == "current")
            .expect("current target should be present in the target projection");
        assert_eq!(current_target.display_title, "Primary Renamed");
        assert!(current_target.search_text.contains("Secondary Renamed"));
        assert_eq!(panel.link_menu_targets_version, Some(note_version()));

        write_note(
            "current.md",
            "# Current\nAlias: Secondary Renamed\n\nSaved body",
        );
        refresh_after_external_change(&mut panel, 0.3);
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![
                ("created-source".into(), "Created Source".into()),
                ("secondary-source".into(), "Secondary Source".into()),
            ]
        );

        write_note(
            "collision.md",
            "# Collision\nAlias: Secondary Renamed\n\nCompeting target",
        );
        refresh_after_external_change(&mut panel, 0.4);
        assert!(related_note_row_identity_text(&panel).is_empty());
        let collision_results = panel.link_menu_results_snapshot();
        assert!(
            collision_results
                .iter()
                .any(|result| result.slug == "collision")
        );
        assert_eq!(panel.link_menu_targets_version, Some(note_version()));
        assert_eq!(
            panel
                .link_menu_results_key
                .as_ref()
                .expect("collision menu results should be keyed")
                .notes_version,
            panel
                .link_menu_targets_version
                .expect("targets are current")
        );

        fs::remove_file(notes_dir.join("collision.md")).expect("remove alias collision");
        refresh_after_external_change(&mut panel, 0.5);
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![
                ("created-source".into(), "Created Source".into()),
                ("secondary-source".into(), "Secondary Source".into()),
            ]
        );
        assert!(
            panel
                .link_menu_results_snapshot()
                .iter()
                .all(|result| result.slug != "collision")
        );

        write_note(
            "mention-source.md",
            "# Mention Source Edited\n\n@note:other",
        );
        refresh_after_external_change(&mut panel, 0.55);
        assert!(mentioned_note_row_identity_text(&panel).is_empty());
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![
                ("created-source".into(), "Created Source".into()),
                ("secondary-source".into(), "Secondary Source".into()),
            ]
        );

        fs::rename(
            notes_dir.join("secondary-source.md"),
            notes_dir.join("renamed-secondary-source.md"),
        )
        .expect("rename linked note");
        refresh_after_external_change(&mut panel, 0.6);
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![
                ("created-source".into(), "Created Source".into()),
                ("renamed-secondary-source".into(), "Secondary Source".into(),),
            ]
        );

        fs::remove_file(notes_dir.join("renamed-secondary-source.md"))
            .expect("delete renamed linked note");
        refresh_after_external_change(&mut panel, 0.7);
        assert_eq!(
            related_note_row_identity_text(&panel)
                .into_iter()
                .map(|(slug, title, _)| (slug, title))
                .collect::<Vec<_>>(),
            vec![("created-source".into(), "Created Source".into())]
        );

        write_note("current.md", "# Current\n\nSaved body without aliases");
        refresh_after_external_change(&mut panel, 0.8);
        assert!(related_note_row_identity_text(&panel).is_empty());

        write_note(
            "created-source.md",
            "# Created Source Edited\n\n[[current]]",
        );
        refresh_after_external_change(&mut panel, 0.9);
        assert_eq!(
            related_note_row_identity_text(&panel),
            vec![(
                "created-source".into(),
                "Created Source Edited".into(),
                "wiki link".into(),
            )]
        );
        fs::remove_file(notes_dir.join("created-source.md")).expect("delete edited linked note");
        refresh_after_external_change(&mut panel, 1.0);
        assert!(related_note_row_identity_text(&panel).is_empty());
    }

    #[test]
    fn changed_note_revision_and_setting_bypass_retry_deadline() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let notes_dir = TempNotesDir::new();
        notes_dir.write_note("alpha.md", "# Alpha\n\nBody");
        notes_dir.refresh_cache();

        let mut panel = NotePanel::from_note(empty_note("# Current\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        let initial_notes_revision = panel.last_notes_version;
        panel.heavy_refresh_retry = Some(HeavyRefreshRetry {
            notes_revision: initial_notes_revision,
            todo_revision: panel.last_todo_revision,
            backlinks_enabled: true,
            retry_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
        });

        notes_dir.write_note("beta.md", "# Beta\n\nBody");
        notes_dir.refresh_cache();

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert!(panel.heavy_refresh_retry.is_none());

        panel.heavy_refresh_retry = Some(HeavyRefreshRetry {
            notes_revision: panel.last_notes_version,
            todo_revision: panel.last_todo_revision,
            backlinks_enabled: true,
            retry_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
        });
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.1),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, false),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert_eq!(panel.last_backlinks_enabled, Some(false));
        assert!(panel.heavy_refresh_retry.is_none());
    }

    #[test]
    fn failed_heavy_snapshot_retains_rows_and_retries_after_cooldown() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        let initial_signature = derived_workload_signature(&panel);
        let initial_notes_revision = panel.last_notes_version;
        let initial_todo_revision = panel.last_todo_revision;
        let initial_applied_setting = panel.last_backlinks_enabled;
        let initial_recomputes = panel.heavy_recompute_count;
        panel.fail_next_heavy_snapshot = true;

        assert!(!panel.refresh_heavy_derived(true, true));

        assert_eq!(derived_workload_signature(&panel), initial_signature);
        assert_eq!(panel.last_notes_version, initial_notes_revision);
        assert_eq!(panel.last_todo_revision, initial_todo_revision);
        assert_eq!(panel.last_backlinks_enabled, initial_applied_setting);
        assert!(!panel.heavy_recompute_requested);
        assert!(panel.heavy_refresh_retry.is_some());
        assert_eq!(panel.heavy_recompute_count, initial_recomputes);

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.1),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes);

        panel
            .heavy_refresh_retry
            .as_mut()
            .expect("force failure should schedule a retry")
            .retry_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
        ctx.run_ui(
            egui::RawInput {
                time: Some(0.2),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert!(!panel.heavy_recompute_requested);
        assert!(panel.heavy_refresh_retry.is_none());
    }

    #[test]
    fn new_draft_after_failed_refresh_uses_normal_edit_debounce() {
        let _workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        let initial_recomputes = panel.heavy_recompute_count;
        panel.fail_next_heavy_snapshot = true;
        assert!(!panel.refresh_heavy_derived(true, true));
        assert!(panel.heavy_refresh_retry.is_some());

        panel.note.content.push_str("\nDraft");
        panel.mark_content_changed(10.0);
        assert!(panel.heavy_refresh_retry.is_none());

        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                time: Some(10.1),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes);
        assert!(panel.heavy_recompute_requested);

        ctx.run_ui(
            egui::RawInput {
                time: Some(10.251),
                ..Default::default()
            },
            |ctx| panel.maybe_refresh_heavy_derived(ctx, true),
        )
        .drop_without_applying_deltas();
        assert_eq!(panel.heavy_recompute_count, initial_recomputes + 1);
        assert!(!panel.heavy_recompute_requested);
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
    fn programmatic_content_replacement_invalidates_cached_markdown_analysis() {
        let mut panel = NotePanel::from_note(empty_note("# Title\n\n- [ ] item"));
        assert_eq!(panel.markdown_analysis().headings.len(), 1);
        assert!(panel.markdown_analysis.is_some());
        assert!(panel.markdown_analysis_source_hash.is_some());
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);

        panel.replace_content_from_mutation("# Updated\n\nBody".into(), 3.0);

        assert_eq!(panel.note.content, "# Updated\n\nBody");
        assert!(panel.markdown_analysis.is_none());
        assert!(panel.markdown_analysis_source_hash.is_none());
        assert!(panel.fast_derived_dirty);
        assert!(panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, Some(3.0));

        let analysis = panel.markdown_analysis();
        assert_eq!(analysis.headings.len(), 1);
        assert_eq!(analysis.headings[0].title, "Updated");
    }

    #[test]
    fn programmatic_unchanged_content_does_not_dirty_derived_state() {
        let mut panel = NotePanel::from_note(empty_note("# Title\n\nBody"));
        assert_eq!(panel.markdown_analysis().headings.len(), 1);
        let cached_hash = panel.markdown_analysis_source_hash;

        panel.replace_content_from_mutation("# Title\n\nBody".into(), 4.0);

        assert_eq!(panel.note.content, "# Title\n\nBody");
        assert!(panel.markdown_analysis.is_some());
        assert_eq!(panel.markdown_analysis_source_hash, cached_hash);
        assert!(!panel.fast_derived_dirty);
        assert!(!panel.heavy_recompute_requested);
        assert_eq!(panel.last_edit_at_secs, None);
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

        let output = ctx.run_ui(Default::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                assert!(!panel.render_segment(ui, &mut app, "- [ ] item", 0, &frame_context));
            });
        });
        output.drop_without_applying_deltas();

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

        let output = ctx.run_ui(Default::default(), |root_ui| {
            let frame_context = root_ui.ctx().clone();
            egui::CentralPanel::default().show(root_ui, |ui| {
                assert!(!panel.render_segment(
                    ui,
                    &mut app,
                    "* [ ] item with [[Link]] #tag @todo:missing",
                    0,
                    &frame_context,
                ));
            });
        });
        output.drop_without_applying_deltas();

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
        ctx.run_ui(Default::default(), |ctx| {
            ctx.output_mut(|output| {
                output
                    .commands
                    .push(egui::OutputCommand::OpenUrl(egui::OpenUrl::same_tab(
                        "note://linked-note",
                    )));
            });
            panel.ui(ctx, &mut app);
        })
        .drop_without_applying_deltas();
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

    #[test]
    fn persistence_baseline_tracks_exact_content() {
        let mut panel = NotePanel::from_note(empty_note("original"));
        assert!(!panel.has_unsaved_changes());
        panel.replace_content_from_mutation("changed".into(), 1.0);
        assert!(panel.has_unsaved_changes());
        panel.replace_content_from_mutation("original".into(), 2.0);
        assert!(!panel.has_unsaved_changes());
    }

    #[test]
    fn saved_external_mutation_is_clean_but_unsaved_mutation_is_dirty() {
        let mut panel = NotePanel::from_note(empty_note("original"));
        panel.replace_content_after_saved_external_mutation("persisted".into(), 1.0);
        assert!(!panel.has_unsaved_changes());
        panel.replace_content_from_mutation("unpersisted".into(), 2.0);
        assert!(panel.has_unsaved_changes());
    }

    #[test]
    fn discard_prompt_actions_have_explicit_transitions() {
        let mut panel = NotePanel::from_note(empty_note("original"));
        panel.discard_unsaved_prompt = true;
        panel.keep_editing();
        assert!(panel.open);
        assert!(!panel.discard_unsaved_prompt);
        assert!(panel.focus_textedit_next_frame);
        panel.discard_unsaved_prompt = true;
        panel.discard_changes();
        assert!(!panel.open);
        assert!(!panel.discard_unsaved_prompt);
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TrackC1DAliasCase {
        Unique,
        Duplicate,
        Ambiguous,
    }

    impl TrackC1DAliasCase {
        const ALL: [Self; 3] = [Self::Unique, Self::Duplicate, Self::Ambiguous];

        fn label(self) -> &'static str {
            match self {
                Self::Unique => "unique",
                Self::Duplicate => "duplicate",
                Self::Ambiguous => "ambiguous",
            }
        }

        fn query(self) -> &'static str {
            match self {
                Self::Unique => "  C1D unique alias 2026  ",
                Self::Duplicate => "  C1D COLLISION alias 2026  ",
                Self::Ambiguous => "  C1D shared ALIAS 2026  ",
            }
        }
    }

    const TRACK_C1D_CURRENT_SLUG: &str = "current-note-00000";

    fn track_c1d_note(index: usize, title: String, slug: String, aliases: Vec<String>) -> Note {
        Note {
            alias: aliases.first().cloned(),
            aliases,
            title: title.clone(),
            path: PathBuf::from(format!("notes/source-{index:05}.md")),
            content: format!("# {title}\nSynthetic alias characterization source {index}.\n"),
            tags: Vec::new(),
            links: Vec::new(),
            slug,
            entity_refs: Vec::new(),
        }
    }

    fn track_c1d_notes(count: usize, case: TrackC1DAliasCase) -> Vec<Note> {
        assert!(count >= 3);
        let mut notes = crate::performance::track_c_workloads::note_fixture(count).values;
        let set_aliases = |note: &mut Note, primary: &str, secondary: &str| {
            note.alias = Some(primary.to_string());
            note.aliases = vec![primary.to_string(), secondary.to_string()];
        };

        notes[0].slug = TRACK_C1D_CURRENT_SLUG.into();
        match case {
            TrackC1DAliasCase::Unique => {
                set_aliases(&mut notes[0], "Current primary", "C1D unique alias 2026");
            }
            TrackC1DAliasCase::Duplicate => {
                set_aliases(&mut notes[0], "Current primary", "Current only");
                notes[1].slug = "duplicate-target".into();
                notes[2].slug = "duplicate-target".into();
                set_aliases(
                    &mut notes[1],
                    "Primary duplicate first",
                    "C1D Collision Alias 2026",
                );
                set_aliases(
                    &mut notes[2],
                    "Primary duplicate second",
                    "C1D COLLISION ALIAS 2026",
                );
            }
            TrackC1DAliasCase::Ambiguous => {
                set_aliases(&mut notes[0], "Current primary", "Current only");
                notes[1].slug = "ambiguous-target-a".into();
                notes[2].slug = "ambiguous-target-b".into();
                set_aliases(
                    &mut notes[1],
                    "Primary ambiguous A",
                    "C1D Shared Alias 2026",
                );
                set_aliases(
                    &mut notes[2],
                    "Primary ambiguous B",
                    "C1D shared alias 2026",
                );
            }
        }

        notes
    }

    fn track_c1d_alias_map(notes: &[Note]) -> HashMap<String, Vec<String>> {
        let mut aliases = HashMap::<String, Vec<String>>::new();
        for note in notes {
            for alias in &note.aliases {
                let slugs = aliases.entry(alias.to_lowercase()).or_default();
                if !slugs.contains(&note.slug) {
                    slugs.push(note.slug.clone());
                }
            }
        }
        aliases
    }

    fn track_c1d_eager_warning(alias: &str, excluded_slug: &str, notes: &[Note]) -> Option<String> {
        let alias = alias.trim();
        if alias.is_empty() {
            return None;
        }
        let alias_map = track_c1d_alias_map(notes);
        let slugs = alias_map.get(&alias.to_lowercase())?;
        let conflicting_slugs = slugs
            .iter()
            .map(String::as_str)
            .filter(|slug| *slug != excluded_slug)
            .collect::<HashSet<_>>();
        if conflicting_slugs.is_empty() {
            return None;
        }

        let labels = notes
            .iter()
            .filter(|note| conflicting_slugs.contains(note.slug.as_str()))
            .map(|note| {
                if let Some(primary) = note
                    .alias
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    format!("{primary} ({} · {})", note.title, note.slug)
                } else {
                    format!("{} ({})", note.title, note.slug)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("Alias \"{alias}\" is already used by {labels}"))
    }

    fn track_c1d_output_signature(output: Option<&str>) -> u64 {
        let mut signature = crate::performance::workloads::StableSignature::new(
            0,
            "track-c1d-alias-warning-output",
            1,
        );
        match output {
            Some(output) => {
                signature.number(1);
                signature.bytes(output.as_bytes());
            }
            None => signature.number(0),
        }
        signature.finish()
    }

    fn track_c1d_numeric_array(values: &[u64]) -> String {
        values
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn track_c1d_fixture_signature(notes: &[Note], case: TrackC1DAliasCase) -> u64 {
        let mut signature = crate::performance::workloads::StableSignature::new(
            0,
            "track-c1d-alias-warning-fixture",
            notes.len(),
        );
        signature.bytes(case.label().as_bytes());
        signature.bytes(case.query().as_bytes());
        signature.number(crate::performance::track_c_workloads::note_fixture_identity(notes));
        signature.finish()
    }

    fn track_c1d_alias_payload(alias_map: &HashMap<String, Vec<String>>) -> (u64, u64, u64) {
        use std::mem::size_of;

        let slug_entries = alias_map.values().map(Vec::len).sum::<usize>();
        let selected_bytes = alias_map.iter().fold(0usize, |total, (alias, slugs)| {
            total
                .saturating_add(size_of::<String>())
                .saturating_add(alias.len())
                .saturating_add(size_of::<Vec<String>>())
                .saturating_add(
                    slugs
                        .iter()
                        .map(|slug| size_of::<String>().saturating_add(slug.len()))
                        .fold(0usize, usize::saturating_add),
                )
        });
        (
            alias_map.len() as u64,
            slug_entries as u64,
            selected_bytes as u64,
        )
    }

    #[test]
    fn alias_collision_warning_matches_frozen_reference_for_semantic_edges() {
        use crate::performance::track_c_workloads::TrackCWorkspace;

        let workspace = TrackCWorkspace::new();
        let _notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes environment lock");
        let context = egui::Context::default();
        let _app = track_c1a_make_inert_app(&context, &workspace);
        let raw_notes = vec![
            track_c1d_note(
                0,
                "Current Café".into(),
                TRACK_C1D_CURRENT_SLUG.into(),
                vec![
                    "Current primary".into(),
                    "Self only".into(),
                    "Éclair, 東京".into(),
                    "Alias + punctuation!".into(),
                ],
            ),
            track_c1d_note(
                1,
                "Duplicate first".into(),
                "duplicate-source".into(),
                vec!["Duplicate primary one".into(), "éclair, 東京".into()],
            ),
            track_c1d_note(
                2,
                "Duplicate second".into(),
                "duplicate-source".into(),
                vec!["Duplicate primary two".into(), "ÉCLAIR, 東京".into()],
            ),
            track_c1d_note(
                3,
                "Ambiguous A".into(),
                "ambiguous-a".into(),
                vec!["Ambiguous primary A".into(), "Éclair, 東京".into()],
            ),
            track_c1d_note(
                4,
                "Ambiguous B".into(),
                "ambiguous-b".into(),
                vec!["Ambiguous primary B".into(), "éclair, 東京".into()],
            ),
        ];
        let _cache_guard = crate::plugins::note::publish_note_cache_for_test(raw_notes);
        let canonical = crate::plugins::note::note_cache_snapshot();
        let current = canonical
            .iter()
            .find(|note| note.slug == TRACK_C1D_CURRENT_SLUG)
            .expect("synthetic cache includes current note")
            .clone();
        let mut panel = NotePanel::from_note(current);

        let blank_observer = alias_warning_test_observer::Scope::begin();
        let blank_alias_observer = crate::plugins::note::NoteAliasSnapshotTestObserver::begin();
        assert_eq!(panel.alias_collision_warning("  \t ").unwrap(), None);
        assert_eq!(blank_observer.snapshot().lookup_calls, 1);
        assert_eq!(blank_alias_observer.snapshot().calls, 0);
        drop(blank_alias_observer);
        drop(blank_observer);

        let self_observer = alias_warning_test_observer::Scope::begin();
        let self_alias_observer = crate::plugins::note::NoteAliasSnapshotTestObserver::begin();
        let self_query_observer = crate::plugins::note::NoteAliasConflictQueryTestObserver::begin();
        assert_eq!(panel.alias_collision_warning(" Self only ").unwrap(), None);
        let self_stats = self_observer.snapshot();
        assert_eq!(self_stats.lookup_calls, 1);
        assert_eq!(self_stats.formatting_calls, 0);
        let self_alias_stats = self_alias_observer.snapshot();
        assert_eq!(self_alias_stats.calls, 0);
        assert_eq!(self_alias_stats.failures, 0);
        let self_query_stats = self_query_observer.snapshot();
        assert_eq!(self_query_stats.calls, 1);
        assert_eq!(self_query_stats.successes, 1);
        assert_eq!(self_query_stats.candidate_entries, 1);
        assert_eq!(self_query_stats.candidate_slugs_examined, 1);
        assert_eq!(self_query_stats.notes_visited, 0);
        drop(self_query_observer);
        drop(self_alias_observer);
        drop(self_observer);

        let unicode_query = "  ÉCLAIR, 東京  ";
        let unicode_output = panel
            .alias_collision_warning(unicode_query)
            .expect("healthy cache produces a warning result");
        assert_eq!(
            unicode_output,
            track_c1d_eager_warning(unicode_query, TRACK_C1D_CURRENT_SLUG, &canonical)
        );
        assert_eq!(
            unicode_output.as_deref(),
            Some(
                "Alias \"ÉCLAIR, 東京\" is already used by Duplicate primary one (Duplicate first · duplicate-source), Duplicate primary two (Duplicate second · duplicate-source), Ambiguous primary A (Ambiguous A · ambiguous-a), Ambiguous primary B (Ambiguous B · ambiguous-b)"
            )
        );

        let punctuation_query = " Alias + punctuation! ";
        assert_eq!(
            panel.alias_collision_warning(punctuation_query).unwrap(),
            None
        );
        panel.note.slug = "unsaved-current-slug".into();
        let self_slug_is_now_conflicting = panel
            .alias_collision_warning("Self only")
            .expect("healthy cache reports current draft slug conflicts");
        assert_eq!(
            self_slug_is_now_conflicting,
            track_c1d_eager_warning("Self only", "unsaved-current-slug", &canonical)
        );
        assert_eq!(
            self_slug_is_now_conflicting.as_deref(),
            Some(
                "Alias \"Self only\" is already used by Current primary (Current Café · current-note-00000)"
            )
        );
    }

    #[test]
    fn alias_warning_query_failure_is_reported_and_add_rename_saves_continue() {
        use crate::performance::track_c_workloads::TrackCWorkspace;
        use crate::plugins::note::{
            NoteAliasConflictQueryTestObserver, fail_next_note_alias_conflict_query_for_test,
        };

        let workspace = TrackCWorkspace::new();
        let _cache_guard = crate::plugins::note::publish_note_cache_for_test(Vec::new());
        let notes_dir = TempNotesDir::new();
        let context = egui::Context::default();
        let mut app = track_c1a_make_inert_app(&context, &workspace);
        app.show_inline_errors = true;
        app.error = None;
        notes_dir.write_note("current.md", "# Current\nAlias: Original\n\nBody");
        notes_dir.refresh_cache();
        let current = crate::plugins::note::note_cache_snapshot()
            .into_iter()
            .find(|note| note.slug == "current")
            .expect("owned note is present before accepted alias edits");
        let mut panel = NotePanel::from_note(current);

        let add_query_observer = NoteAliasConflictQueryTestObserver::begin();
        {
            let _failure = fail_next_note_alias_conflict_query_for_test();
            panel.warn_alias_collision(&mut app, "Added alias");
        }
        let add_query = add_query_observer.snapshot();
        assert_eq!(add_query.calls, 1);
        assert_eq!(add_query.failures, 1);
        drop(add_query_observer);
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("Could not check for conflicting note aliases")),
            "a failed advisory lookup is surfaced through the app error channel"
        );

        panel.note.content = add_alias_metadata(&panel.note.content, "Added alias");
        panel.save_alias_metadata_change(&mut app);
        let saved_after_add = std::fs::read_to_string(notes_dir.dir.path().join("current.md"))
            .expect("accepted Add continues to persistence after lookup error");
        assert!(saved_after_add.contains("Added alias"));

        app.error = None;
        let rename_query_observer = NoteAliasConflictQueryTestObserver::begin();
        {
            let _failure = fail_next_note_alias_conflict_query_for_test();
            panel.warn_alias_collision(&mut app, "Renamed alias");
        }
        let rename_query = rename_query_observer.snapshot();
        assert_eq!(rename_query.calls, 1);
        assert_eq!(rename_query.failures, 1);
        drop(rename_query_observer);
        panel.note.content =
            rename_alias_metadata(&panel.note.content, "Original", "Renamed alias");
        panel.save_alias_metadata_change(&mut app);
        let saved_after_rename = std::fs::read_to_string(notes_dir.dir.path().join("current.md"))
            .expect("accepted Rename continues to persistence after lookup error");
        assert!(!saved_after_rename.contains("Alias: Original"));
        assert!(saved_after_rename.contains("Renamed alias"));
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("Could not check for conflicting note aliases"))
        );
    }

    #[test]
    #[ignore = "fixed-sample C1-D owner characterization; run through the Track C controlled runner"]
    fn track_c_c1d_benchmark_single_lock_alias_warning_costs() {
        use crate::performance::track_c_workloads::{
            BenchmarkMode, MEASURED_SAMPLES, TrackCWorkspace, WARMUPS,
        };
        use crate::performance::{Metric, reset_metrics, snapshot_metrics};
        use crate::plugins::note::{
            NoteAliasConflictQueryTestObserver, NoteAliasSnapshotTestObserver,
        };
        use std::time::Instant;

        assert!(
            crate::performance::enabled(),
            "the alias warning owner run requires MULTI_LAUNCHER_PERF=1"
        );
        let mode = BenchmarkMode::from_process_env().expect("valid Track C benchmark mode");
        let source =
            std::env::var("ML_TRACK_C_SOURCE_SHA").unwrap_or_else(|_| "UNCOMMITTED_SOURCE".into());
        let profile =
            std::env::var("ML_TRACK_C_PROFILE").unwrap_or_else(|_| "unspecified-profile".into());
        for (name, value) in [("source", source.as_str()), ("profile", profile.as_str())] {
            assert!(
                !value.bytes().any(|byte| byte.is_ascii_whitespace()),
                "Track C {name} metadata must be a single token"
            );
        }

        for count in mode.note_sizes() {
            for case in TrackC1DAliasCase::ALL {
                let mut owner_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut message_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_calls = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_successes = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_failures = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_lock_wait_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut query_lock_hold_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut candidate_entries = Vec::with_capacity(MEASURED_SAMPLES);
                let mut candidate_slugs_examined = Vec::with_capacity(MEASURED_SAMPLES);
                let mut notes_visited = Vec::with_capacity(MEASURED_SAMPLES);
                let mut labels_returned = Vec::with_capacity(MEASURED_SAMPLES);
                let mut label_bytes = Vec::with_capacity(MEASURED_SAMPLES);
                let mut output_bytes = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_revision_receipts = Vec::with_capacity(MEASURED_SAMPLES);
                let mut alias_map_snapshot_calls = Vec::with_capacity(MEASURED_SAMPLES);
                let mut alias_map_snapshot_key_entries = Vec::with_capacity(MEASURED_SAMPLES);
                let mut alias_map_snapshot_slug_entries = Vec::with_capacity(MEASURED_SAMPLES);
                let mut alias_map_selected_clone_bytes = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_snapshot_calls = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_snapshot_entries = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_snapshot_estimated_clone_bytes = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_snapshot_lock_wait_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut note_snapshot_lock_hold_ns = Vec::with_capacity(MEASURED_SAMPLES);
                let mut fixture_signature = None;
                let mut warning_signature = None;

                for sample_index in 0..(WARMUPS + MEASURED_SAMPLES) {
                    // Preserve the established order: owned CWD first, note
                    // environment lock second, inert app before cache publish.
                    let workspace = TrackCWorkspace::new();
                    let _notes_env_guard = NOTES_ENV_LOCK.lock().expect("notes environment lock");
                    let context = egui::Context::default();
                    let _app = track_c1a_make_inert_app(&context, &workspace);
                    let raw_notes = track_c1d_notes(*count, case);
                    let _cache_guard = crate::plugins::note::publish_note_cache_for_test(raw_notes);
                    let (revision, canonical) =
                        crate::plugins::note::note_cache_snapshot_with_version()
                            .expect("controlled alias fixture snapshot is healthy");
                    assert_eq!(canonical.len(), *count);
                    let current = canonical
                        .iter()
                        .find(|note| note.slug == TRACK_C1D_CURRENT_SLUG)
                        .expect("controlled alias fixture has a current note")
                        .clone();
                    let panel = NotePanel::from_note(current);
                    let expected =
                        track_c1d_eager_warning(case.query(), &panel.note.slug, &canonical);
                    match case {
                        TrackC1DAliasCase::Unique => {
                            assert!(expected.is_none(), "self-only alias is not a collision");
                        }
                        TrackC1DAliasCase::Duplicate | TrackC1DAliasCase::Ambiguous => {
                            assert!(expected.is_some(), "collision fixture has other slugs");
                        }
                    }
                    let signature = track_c1d_fixture_signature(&canonical, case);
                    let output_id = track_c1d_output_signature(expected.as_deref());
                    let expected_alias_map = track_c1d_alias_map(&canonical);
                    let actual_alias_map = crate::plugins::note::note_alias_map_snapshot();
                    assert_eq!(actual_alias_map, expected_alias_map);
                    let (expected_key_entries, expected_slug_entries, selected_clone_bytes) =
                        track_c1d_alias_payload(&expected_alias_map);
                    assert!(expected_key_entries > 0);
                    assert!(expected_slug_entries >= expected_key_entries);
                    assert!(selected_clone_bytes > 0);
                    assert_eq!(crate::plugins::note::note_version(), revision);
                    assert_eq!(
                        fixture_signature.get_or_insert(signature),
                        &signature,
                        "fresh owned workspaces preserve the same fixture identity"
                    );
                    assert_eq!(
                        warning_signature.get_or_insert(output_id),
                        &output_id,
                        "the frozen full warning remains stable across samples"
                    );

                    reset_metrics();
                    let formatting_observer = alias_warning_test_observer::Scope::begin();
                    let alias_snapshot_observer = NoteAliasSnapshotTestObserver::begin();
                    let query_observer = NoteAliasConflictQueryTestObserver::begin();
                    let started = Instant::now();
                    let actual = panel
                        .alias_collision_warning(case.query())
                        .expect("controlled cache query succeeds");
                    let elapsed = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                    let observer_stats = formatting_observer.snapshot();
                    let alias_stats = alias_snapshot_observer.snapshot();
                    let query_stats = query_observer.snapshot();
                    drop(alias_snapshot_observer);
                    drop(query_observer);
                    drop(formatting_observer);

                    // All full-output/oracle checks and telemetry collection
                    // occur after the exact owner interval.
                    assert_eq!(actual, expected, "measured output equals frozen warning");
                    assert_eq!(crate::plugins::note::note_version(), revision);
                    assert_eq!(observer_stats.lookup_calls, 1);
                    assert_eq!(
                        alias_stats.calls, 0,
                        "optimized path avoids alias-map snapshots"
                    );
                    assert_eq!(alias_stats.key_entries, 0);
                    assert_eq!(alias_stats.failures, 0, "owned cache lock stays healthy");
                    let expected_format_calls: u64 = if expected.is_some() { 1 } else { 0 };
                    assert_eq!(observer_stats.formatting_calls, expected_format_calls);
                    assert_eq!(
                        observer_stats.output_bytes,
                        expected.as_ref().map_or(0, |message| message.len() as u64)
                    );
                    assert_eq!(query_stats.calls, 1);
                    assert_eq!(query_stats.successes, 1);
                    assert_eq!(query_stats.failures, 0);
                    assert_eq!(query_stats.note_revision, revision);

                    let normalized_query = case.query().trim().to_lowercase();
                    let expected_candidate_entries = expected_alias_map
                        .get(&normalized_query)
                        .map_or(0, |slugs| slugs.len() as u64);
                    let expected_notes_visited = if expected.is_some() { *count as u64 } else { 0 };
                    let expected_labels_returned: u64 = match case {
                        TrackC1DAliasCase::Unique => 0,
                        TrackC1DAliasCase::Duplicate | TrackC1DAliasCase::Ambiguous => 2,
                    };
                    let expected_label_bytes =
                        expected.as_ref().map_or(0, |message| {
                            let prefix =
                                format!("Alias \"{}\" is already used by ", case.query().trim());
                            message.len().saturating_sub(prefix.len()).saturating_sub(
                                expected_labels_returned.saturating_sub(1) as usize * 2,
                            ) as u64
                        });
                    assert_eq!(query_stats.candidate_entries, expected_candidate_entries);
                    assert_eq!(
                        query_stats.candidate_slugs_examined, expected_candidate_entries,
                        "candidate receipt counts actual map-slug iterations, including the current slug"
                    );
                    assert_eq!(query_stats.notes_visited, expected_notes_visited);
                    assert_eq!(query_stats.labels_returned, expected_labels_returned);
                    assert_eq!(query_stats.label_bytes, expected_label_bytes);

                    let metrics = snapshot_metrics();
                    let snapshot = metrics
                        .iter()
                        .find(|metric| metric.metric == Metric::NoteSnapshot)
                        .expect("NoteSnapshot metric is present");
                    assert_eq!(
                        snapshot.calls, 0,
                        "optimized path avoids full-note snapshots"
                    );
                    assert_eq!(snapshot.errors, 0);
                    assert_eq!(snapshot.abandoned, 0);
                    assert_eq!(snapshot.work_units, 0);
                    assert_eq!(snapshot.lock_wait_nanos_total, 0);
                    assert_eq!(snapshot.elapsed_nanos_total, 0);

                    if sample_index < WARMUPS {
                        continue;
                    }

                    owner_ns.push(elapsed);
                    query_ns.push(query_stats.query_nanos);
                    message_ns.push(observer_stats.formatting_nanos);
                    query_calls.push(query_stats.calls);
                    query_successes.push(query_stats.successes);
                    query_failures.push(query_stats.failures);
                    query_lock_wait_ns.push(query_stats.lock_wait_nanos);
                    query_lock_hold_ns.push(query_stats.lock_hold_nanos);
                    candidate_entries.push(query_stats.candidate_entries);
                    candidate_slugs_examined.push(query_stats.candidate_slugs_examined);
                    notes_visited.push(query_stats.notes_visited);
                    labels_returned.push(query_stats.labels_returned);
                    label_bytes.push(query_stats.label_bytes);
                    output_bytes.push(observer_stats.output_bytes);
                    note_revision_receipts.push(query_stats.note_revision);
                    alias_map_snapshot_calls.push(alias_stats.calls);
                    alias_map_snapshot_key_entries.push(alias_stats.key_entries);
                    alias_map_snapshot_slug_entries
                        .push(expected_slug_entries.saturating_mul(alias_stats.calls));
                    alias_map_selected_clone_bytes
                        .push(selected_clone_bytes.saturating_mul(alias_stats.calls));
                    note_snapshot_calls.push(snapshot.calls);
                    note_snapshot_entries.push(snapshot.calls.saturating_mul(*count as u64));
                    note_snapshot_estimated_clone_bytes.push(snapshot.work_units);
                    note_snapshot_lock_wait_ns.push(snapshot.lock_wait_nanos_total);
                    note_snapshot_lock_hold_ns.push(snapshot.elapsed_nanos_total);
                }

                let fixture_signature = fixture_signature.expect("measured fixture signature");
                let warning_signature = warning_signature.expect("measured warning signature");
                let owner_ns = track_c1d_numeric_array(&owner_ns);
                let query_ns = track_c1d_numeric_array(&query_ns);
                let message_ns = track_c1d_numeric_array(&message_ns);
                let query_calls = track_c1d_numeric_array(&query_calls);
                let query_successes = track_c1d_numeric_array(&query_successes);
                let query_failures = track_c1d_numeric_array(&query_failures);
                let query_lock_wait_ns = track_c1d_numeric_array(&query_lock_wait_ns);
                let query_lock_hold_ns = track_c1d_numeric_array(&query_lock_hold_ns);
                let candidate_entries = track_c1d_numeric_array(&candidate_entries);
                let candidate_slugs_examined = track_c1d_numeric_array(&candidate_slugs_examined);
                let notes_visited = track_c1d_numeric_array(&notes_visited);
                let labels_returned = track_c1d_numeric_array(&labels_returned);
                let label_bytes = track_c1d_numeric_array(&label_bytes);
                let output_bytes = track_c1d_numeric_array(&output_bytes);
                let note_revision_receipts = track_c1d_numeric_array(&note_revision_receipts);
                let alias_map_snapshot_calls = track_c1d_numeric_array(&alias_map_snapshot_calls);
                let alias_map_snapshot_key_entries =
                    track_c1d_numeric_array(&alias_map_snapshot_key_entries);
                let alias_map_snapshot_slug_entries =
                    track_c1d_numeric_array(&alias_map_snapshot_slug_entries);
                let alias_map_selected_clone_bytes =
                    track_c1d_numeric_array(&alias_map_selected_clone_bytes);
                let note_snapshot_calls = track_c1d_numeric_array(&note_snapshot_calls);
                let note_snapshot_entries = track_c1d_numeric_array(&note_snapshot_entries);
                let note_snapshot_estimated_clone_bytes =
                    track_c1d_numeric_array(&note_snapshot_estimated_clone_bytes);
                let note_snapshot_lock_wait_ns =
                    track_c1d_numeric_array(&note_snapshot_lock_wait_ns);
                let note_snapshot_lock_hold_ns =
                    track_c1d_numeric_array(&note_snapshot_lock_hold_ns);
                eprintln!(
                    "C1D_ALIAS_QUERY_RESULT schema=1 stage=single_lock_conflict_labels owner=NotePanel_alias_collision_warning source={source} profile={profile} mode={} telemetry_enabled=true observer_enabled=true fixture=synthetic_alias_collision fixture_signature={fixture_signature:016x} warning_signature={warning_signature:016x} case={} count={count} warmups={WARMUPS} samples={MEASURED_SAMPLES} exclusions=0 owner_scope=lookup_join_message_only invocation_scope=accepted_Add_or_Rename_not_keystrokes lock_scope=single_CACHE_generation estimate_scope=selected_logical_payload_not_allocator_or_RSS candidate_scope=map_slug_entries_including_current note_scope=ordered_cache_rows_scanned legacy_error_scope=fallible_query_diagnosed_save_continues owner_ns=[{owner_ns}] query_ns=[{query_ns}] message_ns=[{message_ns}] query_calls=[{query_calls}] query_successes=[{query_successes}] query_failures=[{query_failures}] query_lock_wait_ns=[{query_lock_wait_ns}] query_lock_hold_ns=[{query_lock_hold_ns}] candidate_entries=[{candidate_entries}] candidate_slugs_examined=[{candidate_slugs_examined}] notes_visited=[{notes_visited}] labels_returned=[{labels_returned}] label_bytes=[{label_bytes}] output_bytes=[{output_bytes}] note_revision_receipts=[{note_revision_receipts}] alias_map_snapshot_calls=[{alias_map_snapshot_calls}] alias_map_snapshot_key_entries=[{alias_map_snapshot_key_entries}] alias_map_snapshot_slug_entries=[{alias_map_snapshot_slug_entries}] alias_map_selected_clone_bytes=[{alias_map_selected_clone_bytes}] note_snapshot_calls=[{note_snapshot_calls}] note_snapshot_entries=[{note_snapshot_entries}] note_snapshot_estimated_clone_bytes=[{note_snapshot_estimated_clone_bytes}] note_snapshot_lock_wait_ns=[{note_snapshot_lock_wait_ns}] note_snapshot_lock_hold_ns=[{note_snapshot_lock_hold_ns}]",
                    match mode {
                        BenchmarkMode::Small => "small",
                        BenchmarkMode::Full => "full",
                    },
                    case.label(),
                );
            }
        }
    }
}
