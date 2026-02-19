use crate::actions::screenshot::{capture, Mode as ScreenshotMode};
use crate::common::slug::slugify;
use crate::gui::LauncherApp;
use crate::plugin::Plugin;
use crate::plugins::note::{
    assets_dir, available_tags, image_files, note_cache_snapshot, note_version, resolve_note_query,
    save_note, Note, NoteExternalOpen, NotePlugin, NoteTarget,
};
use crate::plugins::todo::{load_todos, todo_version, TODO_FILE};
use eframe::egui::{self, popup, Color32, FontId, Key};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_toast::{Toast, ToastKind, ToastOptions};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use image::imageops::FilterType;
use once_cell::sync::Lazy;
use regex::Regex;
use rfd::FileDialog;
use std::collections::HashMap;
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
    note_slug: Option<String>,
    todo_id: Option<String>,
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
    image_search: String,
    tag_search: String,
    preview_mode: bool,
    markdown_cache: CommonMarkCache,
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
    #[cfg(test)]
    heavy_recompute_count: usize,
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
    fn details_toggle_label(&self) -> &'static str {
        if self.show_metadata {
            "Hide Details"
        } else {
            "Show Details"
        }
    }

    pub fn from_note(note: Note) -> Self {
        let mut panel = Self {
            open: true,
            note,
            link_search: String::new(),
            image_search: String::new(),
            tag_search: String::new(),
            preview_mode: true,
            markdown_cache: CommonMarkCache::default(),
            image_cache: HashMap::new(),
            overwrite_prompt: false,
            show_open_with_menu: false,
            show_metadata: true,
            tags_expanded: false,
            links_expanded: false,
            backlink_tab: BacklinkTab::LinkedTodos,
            backlink_page: 0,
            pending_selection: None,
            link_dialog_open: false,
            link_text: String::new(),
            link_url: String::new(),
            focus_textedit_next_frame: false,
            last_textedit_id: None,
            derived: NoteDerivedView::default(),
            fast_derived_dirty: true,
            heavy_recompute_requested: true,
            last_edit_at_secs: None,
            last_notes_version: 0,
            last_todo_revision: 0,
            #[cfg(test)]
            heavy_recompute_count: 0,
        };
        panel.refresh_fast_derived();
        panel.refresh_heavy_derived(true);
        panel
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

    fn refresh_heavy_derived(&mut self, force: bool) {
        let current_notes_version = note_version();
        let current_todo_revision = todo_version();
        if !force
            && self.last_notes_version == current_notes_version
            && self.last_todo_revision == current_todo_revision
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

        let notes = note_cache_snapshot();
        self.derived.backlink_rows_linked_todos =
            backlink_rows_for_note(&self.note.slug, BacklinkTab::LinkedTodos, &todos, &notes);
        self.derived.backlink_rows_related_notes =
            backlink_rows_for_note(&self.note.slug, BacklinkTab::RelatedNotes, &todos, &notes);
        self.derived.backlink_rows_mentions =
            backlink_rows_for_note(&self.note.slug, BacklinkTab::Mentions, &todos, &notes);

        self.last_notes_version = current_notes_version;
        self.last_todo_revision = current_todo_revision;
        self.heavy_recompute_requested = false;
        #[cfg(test)]
        {
            self.heavy_recompute_count += 1;
        }
    }

    fn mark_content_changed(&mut self, now_secs: f64) {
        self.fast_derived_dirty = true;
        self.heavy_recompute_requested = true;
        self.last_edit_at_secs = Some(now_secs);
    }

    fn maybe_refresh_heavy_derived(&mut self, ctx: &egui::Context) {
        let notes_changed = self.last_notes_version != note_version();
        let todos_changed = self.last_todo_revision != todo_version();
        let debounce_elapsed = self
            .last_edit_at_secs
            .map(|t| ctx.input(|i| i.time - t) >= HEAVY_RECOMPUTE_IDLE_DEBOUNCE.as_secs_f64())
            .unwrap_or(false);
        if notes_changed || todos_changed || debounce_elapsed {
            self.refresh_heavy_derived(false);
            return;
        }

        if self.heavy_recompute_requested {
            ctx.request_repaint_after(HEAVY_RECOMPUTE_IDLE_DEBOUNCE);
        }
    }

    pub fn note_slug(&self) -> &str {
        &self.note.slug
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
                    if self.preview_mode {
                        if ui.button("Edit").clicked() {
                            self.preview_mode = false;
                            // Defer focus until after the TextEdit has been created; requesting
                            // focus for an ID that doesn't exist in the current frame can trip
                            // AccessKit assertions (focused node missing from node tree).
                            self.focus_textedit_next_frame = true;
                        }
                    } else if ui.button("Render").clicked() {
                        self.preview_mode = true;
                        if let Some(id) = self.last_textedit_id {
                            ui.ctx().memory_mut(|m| m.surrender_focus(id));
                        }
                    }
                    if ui.button(self.details_toggle_label()).clicked() {
                        self.show_metadata = !self.show_metadata;
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
                });
                if self.fast_derived_dirty {
                    self.refresh_fast_derived();
                }
                self.maybe_refresh_heavy_derived(ctx);
                if self.show_metadata && !self.derived.tags.is_empty() {
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
                        .into_iter()
                        .map(|(label, url)| LinkKind::Url(label, url)),
                );
                if self.show_metadata && !all_links.is_empty() {
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
                if self.show_metadata {
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
                        for (idx, row) in rows[page_start..page_end].iter().enumerate() {
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
                                if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                {
                                    if let Some(slug) = &row.note_slug {
                                        app.open_note_panel(slug, None);
                                    }
                                }
                                ui.horizontal_wrapped(|ui| {
                                    ui.small(format!("[{}]", row.type_badge));
                                    ui.small(format!("updated {}", row.updated));
                                });
                                ui.small(&row.snippet);
                            });
                            ui.separator();
                        }
                        if total_pages > 1 {
                            ui.horizontal(|ui| {
                                if ui.button("Prev").clicked() && self.backlink_page > 0 {
                                    self.backlink_page -= 1;
                                }
                                ui.small(format!(
                                    "Page {}/{}",
                                    self.backlink_page + 1,
                                    total_pages
                                ));
                                if ui.button("Next").clicked()
                                    && self.backlink_page + 1 < total_pages
                                {
                                    self.backlink_page += 1;
                                }
                            });
                        }
                    }
                    ui.separator();
                }
                let remaining = ui.available_height();
                let resp = egui::ScrollArea::vertical()
                    .id_source(scroll_id_source)
                    .max_height(remaining)
                    .show(ui, |ui| {
                        if self.preview_mode {
                            let mut last = 0usize;
                            let content_clone = self.note.content.clone();
                            let mut modified = false;
                            for cap in IMAGE_RE.captures_iter(&content_clone) {
                                let m = cap.get(0).unwrap();
                                let range = m.range();
                                let before = &content_clone[last..range.start];
                                if !before.is_empty() {
                                    if self.render_segment(ui, app, before, last) {
                                        modified = true;
                                        break;
                                    }
                                }
                                let alt = cap.get(1).unwrap().as_str();
                                let target = cap.get(2).unwrap().as_str();
                                let (rel, width) = if let Some((p, w)) = target.split_once('|') {
                                    (p, w.parse::<f32>().ok())
                                } else {
                                    (target, None)
                                };
                                let full = if let Some(stripped) = rel.strip_prefix("assets/") {
                                    assets_dir().join(stripped)
                                } else {
                                    std::path::PathBuf::from(rel)
                                };
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
                                            egui::ColorImage::from_rgba_unmultiplied(
                                                size,
                                                rgba.as_raw(),
                                            ),
                                            egui::TextureOptions::LINEAR,
                                        );
                                        self.image_cache.insert(full.clone(), tex.clone());
                                        tex
                                    } else {
                                        last = range.end;
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
                                            let repl =
                                                format!("![{alt}]({rel}|{:.0})", new_w.round());
                                            self.note.content.replace_range(range.clone(), &repl);
                                            self.markdown_cache.clear_scrollable();
                                            modified = true;
                                            break;
                                        }
                                    }
                                    response.context_menu(|ui| {
                                        let mut w = width.unwrap_or(display.x);
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut w)
                                                    .clamp_range(20.0..=4096.0),
                                            )
                                            .changed()
                                        {
                                            let repl = format!("![{alt}]({rel}|{:.0})", w.round());
                                            self.note.content.replace_range(range.clone(), &repl);
                                            self.markdown_cache.clear_scrollable();
                                            modified = true;
                                        }
                                        if ui.button("Reset size").clicked() {
                                            let repl = format!("![{alt}]({rel})");
                                            self.note.content.replace_range(range.clone(), &repl);
                                            self.markdown_cache.clear_scrollable();
                                            modified = true;
                                            ui.close_menu();
                                        }
                                    });
                                }
                                last = range.end;
                            }
                            if !modified {
                                let rest = &content_clone[last..];
                                if !rest.is_empty() {
                                    if self.render_segment(ui, app, rest, last) {
                                        modified = true;
                                    }
                                }
                            }
                            if modified {
                                self.markdown_cache.clear_scrollable();
                                self.mark_content_changed(ctx.input(|i| i.time));
                            }
                            None
                        } else {
                            Some(
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.note.content)
                                        .id_source(text_id_source)
                                        .desired_width(f32::INFINITY)
                                        .font(FontId::monospace(app.note_font_size))
                                        .frame(true)
                                        .lock_focus(true)
                                        .desired_rows(10),
                                ),
                            )
                        }
                    });
                if !self.preview_mode {
                    if let Some(resp) = resp.inner {
                        if resp.changed() {
                            self.mark_content_changed(ctx.input(|i| i.time));
                        }
                        let first_edit_frame = self.last_textedit_id.is_none();
                        self.last_textedit_id = Some(resp.id);
                        if self.focus_textedit_next_frame || first_edit_frame {
                            resp.request_focus();
                            self.focus_textedit_next_frame = false;
                        }
                        if !resp.secondary_clicked() {
                            let state = egui::widgets::text_edit::TextEditState::load(ctx, resp.id)
                                .unwrap_or_default();
                            if let Some(range) = state.cursor.char_range() {
                                let [min, max] = range.sorted();
                                if min.index != max.index {
                                    self.pending_selection = Some((min.index, max.index));
                                } else {
                                    self.pending_selection = None;
                                }
                            } else {
                                self.pending_selection = None;
                            }
                        }
                        resp.context_menu(|ui| {
                            let ctx2 = ui.ctx().clone();
                            self.build_textedit_menu(ui, &ctx2, resp.id, app);
                        });
                        if resp.has_focus()
                            && ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Period))
                        {
                            let pos = resp.rect.left_top();
                            popup::show_tooltip_at(
                                ctx,
                                egui::Id::new("note_ctx_menu"),
                                Some(pos),
                                |ui| {
                                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                                        self.build_textedit_menu(ui, ctx, resp.id, app);
                                    });
                                },
                            );
                        }
                        if resp.has_focus() && app.vim_mode {
                            let mut state =
                                egui::widgets::text_edit::TextEditState::load(ctx, resp.id)
                                    .unwrap_or_default();
                            let total_chars = self.note.content.chars().count();
                            let mut idx = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or(0)
                                .min(total_chars);
                            let mut moved = false;

                            if ctx.input(|i| i.key_pressed(egui::Key::H)) {
                                ctx.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::H)
                                });
                                idx = idx.saturating_sub(1);
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(egui::Key::L)) {
                                ctx.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::L)
                                });
                                idx = (idx + 1).min(total_chars);
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(egui::Key::J)) {
                                ctx.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::J)
                                });
                                let byte_idx = char_to_byte_index(&self.note.content, idx);
                                if let Some(pos) = self.note.content[byte_idx..].find('\n') {
                                    let new_byte = byte_idx + pos + 1;
                                    idx = byte_to_char_index(&self.note.content, new_byte);
                                } else {
                                    idx = total_chars;
                                }
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(egui::Key::K)) {
                                ctx.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::K)
                                });
                                let byte_idx = char_to_byte_index(&self.note.content, idx);
                                if let Some(pos) = self.note.content[..byte_idx].rfind('\n') {
                                    idx = byte_to_char_index(&self.note.content, pos);
                                } else {
                                    idx = 0;
                                }
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(egui::Key::Y)) {
                                ctx.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::Y)
                                });
                                let byte_idx = char_to_byte_index(&self.note.content, idx);
                                let start_byte = self.note.content[..byte_idx]
                                    .rfind('\n')
                                    .map(|p| p + 1)
                                    .unwrap_or(0);
                                let end_byte = self.note.content[byte_idx..]
                                    .find('\n')
                                    .map(|p| byte_idx + p)
                                    .unwrap_or_else(|| self.note.content.len());
                                ctx.output_mut(|o| {
                                    o.copied_text =
                                        self.note.content[start_byte..end_byte].to_string();
                                });
                            }

                            if moved {
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(
                                        egui::text::CCursor::new(idx),
                                    )));
                                state.store(ctx, resp.id);
                            }
                        }
                        if resp.clicked() {
                            resp.request_focus();
                        }
                        if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let modifiers = ctx.input(|i| i.modifiers);
                            ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                        }
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
                                app.set_error(format!("Failed to save note: {e}"));
                            } else {
                                self.refresh_fast_derived();
                                self.refresh_heavy_derived(true);
                                self.finish_save(app);
                                self.overwrite_prompt = false;
                            }
                        }
                        if ui.button("Save as New").clicked() {
                            self.note.slug.clear();
                            self.note.path = std::path::PathBuf::new();
                            if let Err(e) = save_note(&mut self.note, true) {
                                app.set_error(format!("Failed to save note: {e}"));
                            } else {
                                self.refresh_fast_derived();
                                self.refresh_heavy_derived(true);
                                self.finish_save(app);
                                self.overwrite_prompt = false;
                            }
                        }
                    });
                });
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
                self.refresh_heavy_derived(true);
                self.finish_save(app);
            }
            Ok(false) => {
                self.overwrite_prompt = true;
            }
            Err(e) => {
                app.set_error(format!("Failed to save note: {e}"));
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

    fn render_segment(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        segment: &str,
        start: usize,
    ) -> bool {
        let mut modified = false;
        let mut offset = start;
        let mut buf_offset = offset;
        let mut buffer = String::new();
        let old_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in segment.lines() {
            if line.starts_with("- [ ]") || line.starts_with("- [x]") || line.starts_with("- [X]") {
                if !buffer.is_empty() {
                    ui.scope(|ui| {
                        ui.style_mut().override_font_id =
                            Some(FontId::proportional(app.note_font_size));
                        let processed = preprocess_note_links(
                            &buffer,
                            &self.note.slug,
                            &self.derived.todo_label_map,
                        );
                        CommonMarkViewer::new(format!("note_seg_{}", buf_offset)).show(
                            ui,
                            &mut self.markdown_cache,
                            &processed,
                        );
                        handle_markdown_links(ui, app);
                    });
                    buffer.clear();
                }
                let checked = line.as_bytes()[3] == b'x' || line.as_bytes()[3] == b'X';
                let mut state = checked;
                ui.horizontal(|ui| {
                    let resp = ui.checkbox(&mut state, "");
                    if resp.changed() {
                        let repl = if state { "- [x]" } else { "- [ ]" };
                        self.note.content.replace_range(offset..offset + 5, repl);
                        self.markdown_cache.clear_scrollable();
                        modified = true;
                    }
                    ui.scope(|ui| {
                        ui.style_mut().override_font_id =
                            Some(FontId::proportional(app.note_font_size));
                        let rest = preprocess_note_links(
                            line.get(6..).unwrap_or(""),
                            &self.note.slug,
                            &self.derived.todo_label_map,
                        );
                        CommonMarkViewer::new(format!("note_seg_{}", offset)).show(
                            ui,
                            &mut self.markdown_cache,
                            &rest,
                        );
                        handle_markdown_links(ui, app);
                    });
                });
                offset += line.len() + 1;
                buf_offset = offset;
            } else {
                if !buffer.is_empty() {
                    buffer.push('\n');
                }
                buffer.push_str(line);
                offset += line.len() + 1;
            }
        }
        if !buffer.is_empty() {
            ui.scope(|ui| {
                ui.style_mut().override_font_id = Some(FontId::proportional(app.note_font_size));
                let processed =
                    preprocess_note_links(&buffer, &self.note.slug, &self.derived.todo_label_map);
                CommonMarkViewer::new(format!("note_seg_{}", buf_offset)).show(
                    ui,
                    &mut self.markdown_cache,
                    &processed,
                );
                handle_markdown_links(ui, app);
            });
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
        });

        ui.menu_button("Insert link", |ui| {
            ui.set_min_width(200.0);
            ui.label("Insert link:");
            ui.text_edit_singleline(&mut self.link_search);
            let plugin = NotePlugin::default();
            let results = plugin.search(&format!("note open {}", self.link_search));
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for action in &results {
                        let title = action.label.clone();
                        if slugify(&title) == self.note.slug {
                            continue;
                        }
                        if ui.button(&title).clicked() {
                            let insert = format!("[[{title}]]");
                            let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id)
                                .unwrap_or_default();
                            let idx = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx_byte = char_to_byte_index(&self.note.content, idx);
                            self.note.content.insert_str(idx_byte, &insert);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(idx + insert.chars().count()),
                                )));
                            state.store(ctx, id);
                            self.link_search.clear();
                            ui.close_menu();
                        }
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
                            app.set_error(format!("Failed to copy image: {e}"));
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
                                app.set_error(format!("Failed to save screenshot: {e}"));
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
                    Err(e) => app.set_error(format!("Screenshot failed: {e}")),
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
            insert_tag_menu(ui, ctx, id, &mut self.note.content, &mut self.tag_search);
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
        if let Some((start, end)) = self.pending_selection.take() {
            let (start_byte, end_byte) = char_range_to_byte_range(&self.note.content, start, end);
            self.note
                .content
                .replace_range(start_byte..end_byte, &insert);
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            let cursor = start + insert.chars().count();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            state.store(ctx, id);
        } else {
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            let idx = state
                .cursor
                .char_range()
                .map(|r| r.primary.index)
                .unwrap_or_else(|| self.note.content.chars().count());
            let idx_byte = char_to_byte_index(&self.note.content, idx);
            self.note.content.insert_str(idx_byte, &insert);
            let cursor = idx + insert.chars().count();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            state.store(ctx, id);
        }
        self.link_dialog_open = false;
        self.link_text.clear();
        self.link_url.clear();
    }

    fn open_external(&self, app: &mut LauncherApp, choice: NoteExternalOpen) {
        let path = self.note.path.clone();
        if let Err(e) = spawn_external(&path, choice) {
            app.set_error(format!("Failed to open note externally: {e}"));
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
                app.set_error(format!(
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
                app.set_error(format!("Broken note link: [[{target}]]"));
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
) {
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

fn backlink_rows_for_note(
    current_slug: &str,
    tab: BacklinkTab,
    todos: &[crate::plugins::todo::TodoEntry],
    notes: &[Note],
) -> Vec<BacklinkRow> {
    let mut rows = Vec::new();
    for todo in todos {
        let token = format!("@note:{current_slug}");
        if todo.text.contains(&token) {
            if matches!(tab, BacklinkTab::LinkedTodos | BacklinkTab::Mentions) {
                rows.push(BacklinkRow {
                    title: todo.text.clone(),
                    type_badge: "Todo".to_string(),
                    updated: "n/a".to_string(),
                    snippet: extract_snippet_around(&todo.text, &token),
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
        let token = format!("[[{current_slug}");
        let mention = format!("@note:{current_slug}");
        if note.links.iter().any(|l| l == current_slug) {
            if tab == BacklinkTab::RelatedNotes {
                rows.push(BacklinkRow {
                    title: note.title.clone(),
                    type_badge: "Note".to_string(),
                    updated: format_note_updated(note),
                    snippet: extract_snippet_around(&note.content, &token),
                    note_slug: Some(note.slug.clone()),
                    todo_id: None,
                });
            }
        } else if note.content.contains(&mention) && tab == BacklinkTab::Mentions {
            rows.push(BacklinkRow {
                title: note.title.clone(),
                type_badge: "Note".to_string(),
                updated: format_note_updated(note),
                snippet: extract_snippet_around(&note.content, &mention),
                note_slug: Some(note.slug.clone()),
                todo_id: None,
            });
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
    use std::sync::{atomic::AtomicBool, Arc};

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
            entity_refs: Vec::new(),
        }
    }

    fn render_panel_and_dump_shapes(
        ctx: &egui::Context,
        panel: &mut NotePanel,
        app: &mut LauncherApp,
    ) -> String {
        let output = ctx.run(Default::default(), |ctx| {
            panel.ui(ctx, app);
        });
        format!("{:?}", output.shapes)
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
            entity_refs: Vec::new(),
        };
        let mut panel = NotePanel::from_note(note);
        panel.preview_mode = false;
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

        let current = "central";
        let notes = vec![
            Note {
                title: "related".into(),
                path: std::path::PathBuf::new(),
                content: "[[central]] body".into(),
                tags: Vec::new(),
                links: vec!["central".into()],
                slug: "related".into(),
                alias: None,
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

        let linked = backlink_rows_for_note(current, BacklinkTab::LinkedTodos, &todos, &notes);
        let related = backlink_rows_for_note(current, BacklinkTab::RelatedNotes, &todos, &notes);
        let mentions = backlink_rows_for_note(current, BacklinkTab::Mentions, &todos, &notes);

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
        panel.preview_mode = false;

        let shown = render_panel_and_dump_shapes(&ctx, &mut panel, &mut app);
        assert!(shown.contains("Tags:"));
        assert!(shown.contains("Links:"));
        assert!(shown.contains("Backlinks"));
        assert!(shown.contains("Body visible always"));

        panel.show_metadata = false;
        let hidden = render_panel_and_dump_shapes(&ctx, &mut panel, &mut app);
        assert!(!hidden.contains("Tags:"));
        assert!(!hidden.contains("Links:"));
        assert!(!hidden.contains("Backlinks"));
        assert!(hidden.contains("Body visible always"));
    }

    #[test]
    fn toggle_button_label_reflects_state() {
        let mut panel = NotePanel::from_note(empty_note("body"));
        assert_eq!(panel.details_toggle_label(), "Hide Details");
        panel.show_metadata = false;
        assert_eq!(panel.details_toggle_label(), "Show Details");
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
    fn save_recomputes_derived_and_updates_links() {
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        std::env::set_var("ML_NOTES_DIR", dir.path());

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
            std::env::set_var("ML_NOTES_DIR", p);
        } else {
            std::env::remove_var("ML_NOTES_DIR");
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
        std::env::set_var("ML_NOTES_DIR", dir.path());

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
            std::env::set_var("ML_NOTES_DIR", p);
        } else {
            std::env::remove_var("ML_NOTES_DIR");
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
    fn note_scheme_link_opens_panel() {
        use crate::plugins::note::Note;
        use std::path::PathBuf;
        use tempfile::tempdir;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        std::env::set_var("ML_NOTES_DIR", dir.path());
        let note = Note {
            title: "Title".into(),
            path: PathBuf::new(),
            content: String::from("dummy"),
            tags: Vec::new(),
            links: Vec::new(),
            slug: String::new(),
            alias: None,
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
            std::env::set_var("ML_NOTES_DIR", p);
        } else {
            std::env::remove_var("ML_NOTES_DIR");
        }
        let _ = crate::plugins::note::refresh_cache();
        assert_eq!(app.note_panels.len(), 1);
        assert_eq!(slugify(&app.note_panels[0].note.title), "linked-note");
    }
}
