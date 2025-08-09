use crate::common::slug::slugify;
use crate::gui::LauncherApp;
use crate::plugin::Plugin;
use crate::plugins::note::{load_notes, save_note, Note, NotePlugin};
use eframe::egui::{self, Color32};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

pub struct NotePanel {
    pub open: bool,
    note: Note,
    link_search: String,
    preview_mode: bool,
    markdown_cache: CommonMarkCache,
}

impl NotePanel {
    pub fn from_note(note: Note) -> Self {
        Self {
            open: true,
            note,
            link_search: String::new(),
            preview_mode: true,
            markdown_cache: CommonMarkCache::default(),
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
                let content_id = egui::Id::new("note_content");
                let total_available = ui.available_size();
                let footer_reserved_height = 90.0; // space for buttons and metadata
                let scrollable_height = total_available.y - footer_reserved_height;
                let resp = egui::ScrollArea::vertical()
                    .id_source(content_id)
                    .show_viewport(ui, |ui, _viewport| {
                        ui.set_min_height(scrollable_height);
                        if self.preview_mode {
                            CommonMarkViewer::new("note_content").show(
                                ui,
                                &mut self.markdown_cache,
                                &self.note.content,
                            );
                            None
                        } else {
                            Some(
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.note.content)
                                        .id_source(content_id)
                                        .desired_width(f32::INFINITY)
                                        .frame(true)
                                        .lock_focus(true)
                                        .desired_rows(10),
                                ),
                            )
                        }
                    });
                if !self.preview_mode {
                    if let Some(resp) = resp.inner {
                        resp.context_menu(|ui| {
                            ui.set_min_width(200.0);
                            ui.label("Insert link:");
                            ui.text_edit_singleline(&mut self.link_search);
                            let plugin = NotePlugin::default();
                            let results = plugin.search(&format!("note open {}", self.link_search));
                            for action in results.into_iter().take(10) {
                                let title = action.label.clone();
                                if ui.button(&title).clicked() {
                                    let insert = format!("[[{title}]]");
                                    let mut state = egui::widgets::text_edit::TextEditState::load(
                                        ui.ctx(),
                                        resp.id,
                                    )
                                    .unwrap_or_default();
                                    let idx = state
                                        .cursor
                                        .char_range()
                                        .map(|r| r.primary.index)
                                        .unwrap_or_else(|| self.note.content.chars().count());
                                    self.note.content.insert_str(idx, &insert);
                                    state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::one(egui::text::CCursor::new(
                                            idx + insert.chars().count(),
                                        )),
                                    ));
                                    state.store(ui.ctx(), resp.id);
                                    self.link_search.clear();
                                    ui.close_menu();
                                }
                            }
                        });
                        if resp.clicked() {
                            resp.request_focus();
                        }
                        if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let modifiers = ctx.input(|i| i.modifiers);
                            ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                        }
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save_now = true;
                    }
                    if self.preview_mode {
                        if ui.button("Edit").clicked() {
                            self.preview_mode = false;
                            ui.ctx().memory_mut(|m| m.request_focus(content_id));
                        }
                    } else if ui.button("Render").clicked() {
                        self.preview_mode = true;
                    }
                });
                ui.separator();
                let tags = extract_tags(&self.note.content);
                if !tags.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Tags:");
                        for t in tags {
                            ui.monospace(format!("#{t}"));
                        }
                    });
                }
                let wiki = extract_wiki_links(&self.note.content);
                let links = extract_links(&self.note.content);
                if !wiki.is_empty() || !links.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Links:");
                        for l in wiki {
                            show_wiki_link(ui, app, &l);
                        }
                        for l in links {
                            ui.hyperlink(l);
                        }
                    });
                }
            });
        if save_now {
            self.note.tags = extract_tags(&self.note.content);
            self.note.links = extract_wiki_links(&self.note.content)
                .into_iter()
                .map(|l| slugify(&l))
                .collect();
            if let Some(first) = self.note.content.lines().next() {
                if let Some(t) = first.strip_prefix("# ") {
                    self.note.title = t.to_string();
                }
            }
            if let Err(e) = save_note(&mut self.note) {
                app.set_error(format!("Failed to save note: {e}"));
            } else {
                app.search();
                app.focus_input();
            }
        }
        self.open = open;
    }
}

pub fn show_wiki_link(ui: &mut egui::Ui, app: &mut LauncherApp, l: &str) -> egui::Response {
    // Display wiki style links with brackets and allow Ctrl+click to
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
    if resp.clicked() && ui.ctx().input(|i| i.modifiers.ctrl) {
        app.open_note_panel(&slug, None);
    }
    resp
}

fn extract_tags(content: &str) -> Vec<String> {
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
    let mut tags: Vec<String> = TAG_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

pub fn extract_links(content: &str) -> Vec<String> {
    static LINK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"([a-zA-Z][a-zA-Z0-9+.-]*://\S+|www\.\S+)").unwrap());
    LINK_RE
        .find_iter(content)
        .filter_map(|m| {
            let raw = m.as_str();
            let url = if raw.starts_with("www.") {
                format!("https://{raw}")
            } else {
                raw.to_string()
            };
            Url::parse(&url)
                .ok()
                .filter(|u| u.scheme() == "https")
                .map(|_| raw.to_string())
        })
        .collect()
}

fn extract_wiki_links(content: &str) -> Vec<String> {
    static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
    WIKI_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
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

    #[test]
    fn ctrl_click_opens_linked_note() {
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
        input.modifiers.ctrl = true;
        input.events.push(egui::Event::PointerMoved(pos));
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::CTRL,
        });
        input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::CTRL,
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
    fn regular_click_does_not_navigate() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                rect = show_wiki_link(ui, &mut app, "Another Note").rect;
            });
        });

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
                show_wiki_link(ui, &mut app, "Another Note");
            });
        });

        assert!(app.note_panels.is_empty());
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
        let content = "visit http://example.com and http://exa%mple.com also https://rust-lang.org and www.example.com and www.exa%mple.com";
        let links = extract_links(content);
        assert_eq!(
            links,
            vec![
                "https://rust-lang.org".to_string(),
                "www.example.com".to_string(),
            ]
        );
    }
}
