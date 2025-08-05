use crate::common::slug::slugify;
use crate::gui::LauncherApp;
use crate::plugins::note::{save_note, Note};
use eframe::egui;
use once_cell::sync::Lazy;
use regex::Regex;
use url::Url;

#[derive(Clone)]
pub struct NotePanel {
    pub open: bool,
    note: Note,
}

impl NotePanel {
    pub fn from_note(note: Note) -> Self {
        Self { open: true, note }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut save_now = false;
        egui::Window::new(self.note.title.clone())
            .open(&mut open)
            .resizable(true)
            .default_size((420.0, 320.0))
            .min_width(200.0)
            .min_height(150.0)
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut self.note.content)
                        .desired_width(f32::INFINITY)
                        .desired_rows(15),
                );
                if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let modifiers = ctx.input(|i| i.modifiers);
                    ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                }
                if ui.button("Save").clicked() {
                    save_now = true;
                }
                ui.separator();
                let tags = extract_tags(&self.note.content);
                if !tags.is_empty() {
                    ui.horizontal(|ui| {
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
            if let Err(e) = save_note(&self.note) {
                app.set_error(format!("Failed to save note: {e}"));
            } else {
                app.search();
                app.focus_input();
            }
        }
        self.open = open;
    }
}

fn show_wiki_link(ui: &mut egui::Ui, app: &mut LauncherApp, l: &str) -> egui::Response {
    // Display wiki style links with brackets and allow Ctrl+click to
    // navigate to the referenced note.
    let resp = ui.link(format!("[[{l}]]"));
    if resp.clicked() && ui.ctx().input(|i| i.modifiers.ctrl) {
        let slug = slugify(l);
        app.open_note_panel(&slug, None);
    }
    resp
}

fn extract_tags(content: &str) -> Vec<String> {
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
    TAG_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

fn extract_links(content: &str) -> Vec<String> {
    static LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(https://\S+|www\.\S+)").unwrap());
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
            Vec::new(),
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
