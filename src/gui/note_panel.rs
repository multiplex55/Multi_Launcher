use crate::actions::screenshot::{capture, Mode as ScreenshotMode};
use crate::common::slug::slugify;
use crate::gui::LauncherApp;
use crate::plugin::Plugin;
use crate::plugins::note::{
    assets_dir, available_tags, image_files, load_notes, save_note, Note, NoteExternalOpen,
    NotePlugin,
};
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
};
use url::Url;

static IMAGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap());

fn preprocess_note_links(content: &str, current_slug: &str) -> String {
    static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
    WIKI_RE
        .replace_all(content, |caps: &regex::Captures| {
            let text = &caps[1];
            let slug = slugify(text);
            if slug == current_slug {
                caps[0].to_string()
            } else {
                format!("[{text}](note://{slug})")
            }
        })
        .to_string()
}

fn handle_markdown_links(ui: &egui::Ui, app: &mut LauncherApp) {
    if let Some(mut open_url) = ui.ctx().output_mut(|o| o.open_url.take()) {
        if let Ok(url) = Url::parse(&open_url.url) {
            if url.scheme() == "note" {
                if let Some(slug) = url.host_str() {
                    app.open_note_panel(slug, None);
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
    tags_expanded: bool,
    links_expanded: bool,
    pending_selection: Option<(usize, usize)>, // byte offsets
    link_dialog_open: bool,
    link_text: String,
    link_url: String,
}

impl NotePanel {
    pub fn from_note(note: Note) -> Self {
        Self {
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
            tags_expanded: false,
            links_expanded: false,
            pending_selection: None,
            link_dialog_open: false,
            link_text: String::new(),
            link_url: String::new(),
        }
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
        let content_id = egui::Id::new("note_content");
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
                            ui.ctx().memory_mut(|m| m.request_focus(content_id));
                        }
                    } else if ui.button("Render").clicked() {
                        self.preview_mode = true;
                    }
                    ui.separator();
                    if ui.button("A-").clicked() {
                        app.note_font_size = (app.note_font_size - 1.0).max(8.0);
                    }
                    if ui.button("A+").clicked() {
                        app.note_font_size += 1.0;
                    }
                });
                let tags = extract_tags(&self.note.content);
                if !tags.is_empty() {
                    let was_focused = ui.ctx().memory(|m| m.has_focus(content_id));
                    let tag_count = tags.len();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Tags:");
                        let threshold = app.note_more_limit;
                        let show_all = self.tags_expanded || tag_count <= threshold;
                        let limit = if show_all { tag_count } else { threshold };
                        for t in tags.iter().take(limit) {
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
                                    ui.ctx().memory_mut(|m| m.request_focus(content_id));
                                }
                            }
                        }
                    });
                }
                let wiki = extract_wiki_links(&self.note.content)
                    .into_iter()
                    .filter(|l| slugify(l) != self.note.slug)
                    .collect::<Vec<_>>();
                let links = extract_links(&self.note.content);
                enum LinkKind {
                    Wiki(String),
                    Url(String, String),
                }
                let mut all_links: Vec<LinkKind> = Vec::new();
                all_links.extend(wiki.into_iter().map(LinkKind::Wiki));
                all_links.extend(
                    links
                        .into_iter()
                        .map(|(label, url)| LinkKind::Url(label, url)),
                );
                if !all_links.is_empty() {
                    let was_focused = ui.ctx().memory(|m| m.has_focus(content_id));
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
                                    ui.ctx().memory_mut(|m| m.request_focus(content_id));
                                }
                            }
                        }
                    });
                }
                ui.separator();
                let remaining = ui.available_height();
                let resp = egui::ScrollArea::vertical()
                    .id_source(content_id)
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
                            }
                            None
                        } else {
                            Some(
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.note.content)
                                        .id_source(content_id)
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
                        if !resp.secondary_clicked() {
                            let state = egui::widgets::text_edit::TextEditState::load(ctx, resp.id)
                                .unwrap_or_default();
                            if let Some(range) = state.cursor.char_range() {
                                let [min, max] = range.sorted();
                                if min.index != max.index {
                                    self.pending_selection =
                                        char_range_to_byte_range(&self.note.content, range);
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
                            let mut idx = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or(0);
                            let mut moved = false;
                            if ctx.input(|i| i.key_pressed(Key::H)) {
                                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::H));
                                idx = idx.saturating_sub(1);
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(Key::L)) {
                                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::L));
                                idx = (idx + 1).min(self.note.content.chars().count());
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(Key::J)) {
                                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::J));
                                let after =
                                    self.note.content.chars().skip(idx).position(|c| c == '\n');
                                idx = after
                                    .map(|pos| idx + pos + 1)
                                    .unwrap_or_else(|| self.note.content.chars().count());
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(Key::K)) {
                                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::K));
                                idx = self
                                    .note
                                    .content
                                    .chars()
                                    .take(idx)
                                    .rposition(|c| c == '\n')
                                    .unwrap_or(0);
                                moved = true;
                            }
                            if ctx.input(|i| i.key_pressed(Key::Y)) {
                                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Y));
                                let start_char = self
                                    .note
                                    .content
                                    .chars()
                                    .take(idx)
                                    .rposition(|c| c == '\n')
                                    .map(|p| p + 1)
                                    .unwrap_or(0);
                                let end_char = self
                                    .note
                                    .content
                                    .chars()
                                    .skip(idx)
                                    .position(|c| c == '\n')
                                    .map(|p| idx + p)
                                    .unwrap_or_else(|| self.note.content.chars().count());
                                let start = char_to_byte_index(&self.note.content, start_char);
                                let end = char_to_byte_index(&self.note.content, end_char);
                                ctx.output_mut(|o| {
                                    o.copied_text = self.note.content[start..end].to_string();
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
                            self.insert_link(ctx, content_id);
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
        if let Some(first) = self.note.content.lines().next() {
            if let Some(t) = first.strip_prefix("# ") {
                self.note.title = t.to_string();
            }
        }
        match save_note(&mut self.note, app.note_always_overwrite) {
            Ok(true) => {
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
                        let processed = preprocess_note_links(&buffer, &self.note.slug);
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
                        let rest =
                            preprocess_note_links(line.get(6..).unwrap_or(""), &self.note.slug);
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
                let processed = preprocess_note_links(&buffer, &self.note.slug);
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
                self.pending_selection =
                    char_range_to_byte_range(&self.note.content, range).or(self.pending_selection);
            }
        }

        ui.menu_button("Markdown", |ui| {
            if ui.button("Add Checkbox").clicked() {
                let mut state =
                    egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                let idx_chars = state
                    .cursor
                    .char_range()
                    .map(|r| r.primary.index)
                    .unwrap_or_else(|| self.note.content.chars().count());
                let idx = char_to_byte_index(&self.note.content, idx_chars);
                self.note.content.insert_str(idx, "- [ ] ");
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(byte_to_char_index(
                            &self.note.content,
                            idx + "- [ ] ".len(),
                        )),
                    )));
                state.store(ctx, id);
                ui.close_menu();
            }
            if ui.button("Insert Link...").clicked() {
                if let Some((start, end)) = self.pending_selection {
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
                            let idx_chars = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx = char_to_byte_index(&self.note.content, idx_chars);
                            self.note.content.insert_str(idx, &insert);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(byte_to_char_index(
                                        &self.note.content,
                                        idx + insert.len(),
                                    )),
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
                            let idx_chars = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx = char_to_byte_index(&self.note.content, idx_chars);
                            self.note.content.insert_str(idx, &insert);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(byte_to_char_index(
                                        &self.note.content,
                                        idx + insert.len(),
                                    )),
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
                                let idx_chars = state
                                    .cursor
                                    .char_range()
                                    .map(|r| r.primary.index)
                                    .unwrap_or_else(|| self.note.content.chars().count());
                                let idx = char_to_byte_index(&self.note.content, idx_chars);
                                self.note.content.insert_str(idx, &insert);
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(
                                        egui::text::CCursor::new(byte_to_char_index(
                                            &self.note.content,
                                            idx + insert.len(),
                                        )),
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
                            let idx_chars = state
                                .cursor
                                .char_range()
                                .map(|r| r.primary.index)
                                .unwrap_or_else(|| self.note.content.chars().count());
                            let idx = char_to_byte_index(&self.note.content, idx_chars);
                            self.note.content.insert_str(idx, &insert);
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(byte_to_char_index(
                                        &self.note.content,
                                        idx + insert.len(),
                                    )),
                                )));
                            state.store(ctx, id);
                            self.image_search.clear();
                            ui.close_menu();
                        }
                    }
                });
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
        let mut range = state
            .cursor
            .char_range()
            .and_then(|r| char_range_to_byte_range(&self.note.content, r));

        if range.is_none() {
            range = self.pending_selection.take();
        } else {
            self.pending_selection = None;
        }

        if let Some((start, end)) = range {
            self.note.content.insert_str(end, end_marker);
            self.note.content.insert_str(start, start_marker);
            let new_start = start + start_marker.len();
            let new_end = end + start_marker.len();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(byte_to_char_index(&self.note.content, new_start)),
                    egui::text::CCursor::new(byte_to_char_index(&self.note.content, new_end)),
                )));
            state.store(ctx, id);
        }
    }

    pub fn insert_link(&mut self, ctx: &egui::Context, id: egui::Id) {
        let text = if self.link_text.is_empty() {
            if let Some((start, end)) = self.pending_selection {
                self.note.content[start..end].to_string()
            } else {
                String::new()
            }
        } else {
            self.link_text.clone()
        };
        let insert = format!("[{text}]({})", self.link_url);
        if let Some((start, end)) = self.pending_selection.take() {
            self.note.content.replace_range(start..end, &insert);
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            let cursor = byte_to_char_index(&self.note.content, start + insert.len());
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor),
                )));
            state.store(ctx, id);
        } else {
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            let idx_chars = state
                .cursor
                .char_range()
                .map(|r| r.primary.index)
                .unwrap_or_else(|| self.note.content.chars().count());
            let idx = char_to_byte_index(&self.note.content, idx_chars);
            self.note.content.insert_str(idx, &insert);
            let cursor = byte_to_char_index(&self.note.content, idx + insert.len());
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
    // Display wiki style links with brackets and allow clicking to
    // navigate to the referenced note. Missing targets are colored red.
    let slug = slugify(l);
    let exists = load_notes()
        .ok()
        .map(|notes| notes.iter().any(|n| n.slug == slug))
        .unwrap_or(false);
    let text = format!("[[{l}]]");
    let resp = if exists {
        ui.link(text)
    } else {
        ui.add(
            egui::Label::new(egui::RichText::new(text).color(Color32::RED))
                .sense(egui::Sense::click()),
        )
    };
    if resp.clicked() {
        app.open_note_panel(&slug, None);
    }
    resp
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
                    let idx_chars = state
                        .cursor
                        .char_range()
                        .map(|r| r.primary.index)
                        .unwrap_or_else(|| content.chars().count());
                    let idx = char_to_byte_index(content, idx_chars);
                    content.insert_str(idx, &insert);
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(byte_to_char_index(
                                content,
                                idx + insert.len(),
                            )),
                        )));
                    state.store(ctx, id);
                    search.clear();
                    ui.close_menu();
                }
            }
        });
}

fn char_to_byte_index(text: &str, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    match text.char_indices().nth(char_idx) {
        Some((idx, _)) => idx,
        None => text.len(),
    }
}

fn byte_to_char_index(text: &str, byte_idx: usize) -> usize {
    let clamped = byte_idx.min(text.len());
    text[..clamped].chars().count()
}

fn char_range_to_byte_range(text: &str, range: egui::text::CCursorRange) -> Option<(usize, usize)> {
    let [min, max] = range.sorted();
    if min.index == max.index {
        None
    } else {
        Some((
            char_to_byte_index(text, min.index),
            char_to_byte_index(text, max.index),
        ))
    }
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
        }
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
    fn wrap_selection_handles_multibyte_content() {
        let ctx = egui::Context::default();
        let mut panel = NotePanel::from_note(empty_note("café ☕ note"));
        let id = egui::Id::new("note_content");
        let mut state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap_or_default();
        let selection =
            egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(4));
        state.cursor.set_char_range(Some(selection));
        state.store(&ctx, id);
        panel.pending_selection = char_range_to_byte_range(&panel.note.content, selection);

        panel.wrap_selection(&ctx, id, "**", "**");
        assert_eq!(panel.note.content, "**café** ☕ note");
        let state = egui::widgets::text_edit::TextEditState::load(&ctx, id).unwrap();
        let range = state.cursor.char_range().unwrap().sorted();
        assert_eq!((range[0].index, range[1].index), (2, 6));
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
        let pos = egui::pos2(200.0, 100.0);
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
        let processed = preprocess_note_links(content, "current-note");
        assert_eq!(processed, "See [Target Note](note://target-note)");
    }

    #[test]
    fn preprocess_wiki_links_skips_self() {
        let content = "See [[Target Note]]";
        let processed = preprocess_note_links(content, "target-note");
        assert_eq!(processed, content);
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
