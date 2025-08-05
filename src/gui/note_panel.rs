use crate::gui::LauncherApp;
use crate::plugins::note::{save_note, Note};
use eframe::egui;
use once_cell::sync::Lazy;
use regex::Regex;
use slug::slugify;

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
                            let resp = ui.link(&l);
                            if resp.clicked() && ui.ctx().input(|i| i.modifiers.ctrl) {
                                let slug = slugify(&l);
                                app.open_note_panel(&slug);
                            }
                        }
                        for l in links {
                            ui.hyperlink(l);
                        }
                    });
                }
            });
        if save_now {
            self.note.tags = extract_tags(&self.note.content);
            self.note.links = extract_links(&self.note.content);
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

fn extract_tags(content: &str) -> Vec<String> {
    static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#([A-Za-z0-9_]+)").unwrap());
    TAG_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

fn extract_links(content: &str) -> Vec<String> {
    static LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"https?://\S+").unwrap());
    LINK_RE
        .find_iter(content)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn extract_wiki_links(content: &str) -> Vec<String> {
    static WIKI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
    WIKI_RE
        .captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}
