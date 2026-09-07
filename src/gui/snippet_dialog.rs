use super::{LauncherApp, push_toast};
use crate::plugins::snippets::{SNIPPETS_FILE, SnippetEntry, load_snippets, replace_snippets};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Default)]
pub struct SnippetDialog {
    pub open: bool,
    entries: Vec<SnippetEntry>,
    edit_idx: Option<usize>,
    alias: String,
    text: String,
    filter: String,
    load_error: Option<String>,
}

fn matches_snippet_filter(entry: &SnippetEntry, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }

    let lowered_filter = filter.to_lowercase();
    entry.alias.to_lowercase().contains(&lowered_filter)
        || entry.text.to_lowercase().contains(&lowered_filter)
}

impl SnippetDialog {
    pub fn open(&mut self) {
        let _ = self.load_from(SNIPPETS_FILE);
        self.open = true;
        self.edit_idx = None;
        self.alias.clear();
        self.text.clear();
        self.filter.clear();
    }

    pub fn open_edit(&mut self, alias: &str) {
        if self.load_from(SNIPPETS_FILE).is_err() {
            self.edit_idx = None;
            self.open = true;
            return;
        }
        self.filter.clear();
        if let Some(pos) = self.entries.iter().position(|e| e.alias == alias) {
            self.edit_idx = Some(pos);
            self.alias = alias.to_string();
            self.text = self.entries[pos].text.clone();
        } else {
            self.edit_idx = Some(self.entries.len());
            self.alias = alias.to_string();
            self.text.clear();
        }
        self.open = true;
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        match load_snippets(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_entries(&mut self, path: &str, candidate: Vec<SnippetEntry>) -> anyhow::Result<()> {
        match replace_snippets(path, candidate) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn save(&mut self, app: &mut LauncherApp, candidate: Vec<SnippetEntry>) -> bool {
        if let Err(e) = self.commit_entries(SNIPPETS_FILE, candidate) {
            app.report_error_message("ui operation", format!("Failed to save snippets: {e}"));
            false
        } else {
            if app.enable_toasts {
                push_toast(
                    &mut app.toasts,
                    Toast {
                        text: "Saved snippet".into(),
                        kind: ToastKind::Success,
                        options: ToastOptions::default()
                            .duration_in_seconds(app.toast_duration as f64),
                    },
                );
            }
            app.search();
            app.focus_input();
            true
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut close = false;
        let mut save_candidate = None;
        egui::Window::new("Snippets")
            .open(&mut self.open)
            .show(ctx, |ui| {
                if let Some(error) = &self.load_error {
                    ui.colored_label(
                        egui::Color32::RED,
                        format!("Snippets are read-only because loading failed: {error}"),
                    );
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                    return;
                }
                if let Some(idx) = self.edit_idx {
                    ui.horizontal(|ui| {
                        ui.label("Alias");
                        ui.text_edit_singleline(&mut self.alias);
                    });
                    ui.label("Text");
                    ui.text_edit_multiline(&mut self.text);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.alias.trim().is_empty() || self.text.trim().is_empty() {
                                app.report_error_message("ui operation", "Both fields required");
                            } else {
                                let mut candidate = self.entries.clone();
                                if idx == candidate.len() {
                                    candidate.push(SnippetEntry {
                                        alias: self.alias.clone(),
                                        text: self.text.clone(),
                                    });
                                } else if let Some(e) = candidate.get_mut(idx) {
                                    e.alias = self.alias.clone();
                                    e.text = self.text.clone();
                                }
                                save_candidate = Some(candidate);
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                        }
                    });
                } else {
                    let mut remove: Option<usize> = None;
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        ui.add(egui::TextEdit::singleline(&mut self.filter));
                    });
                    let mut shown_count = 0usize;
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            for idx in 0..self.entries.len() {
                                let entry = self.entries[idx].clone();
                                if !matches_snippet_filter(&entry, &self.filter) {
                                    continue;
                                }
                                shown_count += 1;
                                ui.horizontal(|ui| {
                                    ui.label(format!(
                                        "{}: {}",
                                        entry.alias,
                                        entry.text.replace('\n', " ")
                                    ));
                                    if ui.button("Edit").clicked() {
                                        self.edit_idx = Some(idx);
                                        self.alias = entry.alias.clone();
                                        self.text = entry.text.clone();
                                    }
                                    if ui.button("Remove").clicked() {
                                        remove = Some(idx);
                                    }
                                });
                            }
                        });
                    if shown_count == 0 {
                        ui.label("No snippets match filter");
                    }
                    if let Some(idx) = remove {
                        let mut candidate = self.entries.clone();
                        candidate.remove(idx);
                        save_candidate = Some(candidate);
                    }
                    if ui.button("Add Snippet").clicked() {
                        self.edit_idx = Some(self.entries.len());
                        self.alias.clear();
                        self.text.clear();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });
        if let Some(candidate) = save_candidate
            && self.save(app, candidate)
        {
            self.edit_idx = None;
            self.alias.clear();
            self.text.clear();
        }
        if close {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SnippetDialog, matches_snippet_filter};
    use crate::plugins::snippets::{SnippetEntry, save_snippets};

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let entry = snippet("greet", "hello world");
        assert!(matches_snippet_filter(&entry, ""));
        assert!(matches_snippet_filter(&entry, "   "));
    }

    #[test]
    fn alias_only_match() {
        let entry = snippet("git-status", "show status");
        assert!(matches_snippet_filter(&entry, "status"));
    }

    #[test]
    fn text_only_match() {
        let entry = snippet("gs", "Git Status Output");
        assert!(matches_snippet_filter(&entry, "output"));
    }

    #[test]
    fn case_insensitive_matching() {
        let entry = snippet("DockerUp", "Start Containers");
        assert!(matches_snippet_filter(&entry, "docker"));
        assert!(matches_snippet_filter(&entry, "CONTAINERS"));
    }

    #[test]
    fn non_match_behavior() {
        let entry = snippet("deploy", "release production");
        assert!(!matches_snippet_filter(&entry, "staging"));
    }

    #[test]
    fn invalid_reload_and_commit_keep_dialog_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("saved", "value")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid snippets";
        std::fs::write(&path, invalid).unwrap();

        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
        assert!(
            dialog
                .commit_entries(path.to_str().unwrap(), vec![snippet("lost", "value")])
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(path).unwrap(), invalid);
        assert!(dialog.load_error.is_some());
    }
}
