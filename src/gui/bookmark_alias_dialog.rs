use crate::gui::LauncherApp;
use crate::plugins::bookmarks::{set_alias, BOOKMARKS_FILE, load_bookmarks};
use eframe::egui;

pub struct BookmarkAliasDialog {
    pub open: bool,
    url: String,
    alias: String,
}

impl Default for BookmarkAliasDialog {
    fn default() -> Self {
        Self { open: false, url: String::new(), alias: String::new() }
    }
}

impl BookmarkAliasDialog {
    pub fn open(&mut self, url: &str) {
        self.url = url.to_string();
        if let Ok(list) = load_bookmarks(BOOKMARKS_FILE) {
            if let Some(entry) = list.into_iter().find(|b| b.url == self.url) {
                if let Some(a) = entry.alias {
                    self.alias = a;
                } else {
                    self.alias.clear();
                }
            }
        }
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        egui::Window::new("Set Bookmark Alias")
            .open(&mut self.open)
            .show(ctx, |ui| {
                ui.label(&self.url);
                ui.text_edit_singleline(&mut self.alias);

                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Err(e) = set_alias(BOOKMARKS_FILE, &self.url, &self.alias) {
                        app.error = Some(format!("Failed to save alias: {e}"));
                    } else {
                        close = true;
                        app.search();
                        app.focus_input();
                    }
                    let modifiers = ctx.input(|i| i.modifiers);
                    ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Enter));
                }

                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        if let Err(e) = set_alias(BOOKMARKS_FILE, &self.url, &self.alias) {
                            app.error = Some(format!("Failed to save alias: {e}"));
                        } else {
                            close = true;
                            app.search();
                            app.focus_input();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.open = false;
        }
    }
}
