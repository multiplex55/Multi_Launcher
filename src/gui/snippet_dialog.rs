use super::{LauncherApp, push_toast};
use crate::plugins::snippets::{
    SNIPPETS_FILE, SnippetEntry, load_snippets, replace_snippets, snippet_preview_text,
};
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

fn matching_snippet_indices(entries: &[SnippetEntry], filter: &str) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| matches_snippet_filter(entry, filter).then_some(index))
        .collect()
}

fn single_line_alias(alias: &str) -> String {
    let mut display = String::with_capacity(alias.len());
    let mut space_pending = false;
    for character in alias.chars() {
        if character.is_whitespace() || character.is_control() {
            space_pending = !display.is_empty();
        } else {
            if space_pending {
                display.push(' ');
                space_pending = false;
            }
            display.push(character);
        }
    }
    display
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
            .default_size((600.0, 500.0))
            .min_width(380.0)
            .min_height(320.0)
            .resizable(true)
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
                    let action_height = ui.spacing().interact_size.y
                        + ui.spacing().button_padding.y * 2.0
                        + ui.spacing().item_spacing.y;
                    let editor_height = (ui.available_height() - action_height).max(120.0);
                    let font_id = egui::TextStyle::Body.resolve(ui.style());
                    let line_height = ui.fonts(|fonts| fonts.row_height(&font_id));
                    let desired_rows =
                        (editor_height / line_height).floor().clamp(4.0, 48.0) as usize;
                    ui.add(
                        egui::TextEdit::multiline(&mut self.text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(desired_rows),
                    );
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
                                        hide_contents: false,
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
                        let clear_filter_width = 116.0;
                        let filter_spacing = ui.spacing().item_spacing.x;
                        let filter_width =
                            (ui.available_width() - clear_filter_width - filter_spacing).max(80.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.filter)
                                .desired_width(filter_width),
                        );
                        if ui
                            .add_enabled(
                                !self.filter.is_empty(),
                                egui::Button::new("Clear Filter").min_size(egui::vec2(
                                    clear_filter_width,
                                    ui.spacing().interact_size.y
                                        + ui.spacing().button_padding.y * 2.0,
                                )),
                            )
                            .clicked()
                        {
                            self.filter.clear();
                        }
                    });
                    let matching_indices = matching_snippet_indices(&self.entries, &self.filter);
                    ui.label(format!(
                        "{} of {} snippets",
                        matching_indices.len(),
                        self.entries.len()
                    ));
                    if matching_indices.is_empty() {
                        ui.label("No snippets match filter");
                    } else {
                        let footer_height = (ui.spacing().interact_size.y
                            + ui.spacing().button_padding.y * 2.0
                            + ui.spacing().item_spacing.y)
                            * 2.0;
                        let list_height = (ui.available_height() - footer_height).max(100.0);
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .max_height(list_height)
                            .show(ui, |ui| {
                                for idx in matching_indices {
                                    let entry = self.entries[idx].clone();
                                    ui.horizontal(|ui| {
                                        let spacing = ui.spacing().item_spacing.x;
                                        let row_height = ui.spacing().interact_size.y
                                            + ui.spacing().button_padding.y * 2.0;
                                        let edit_width = 48.0;
                                        let remove_width = 68.0;
                                        let preview_width = (ui.available_width()
                                            - edit_width
                                            - remove_width
                                            - spacing * 2.0)
                                            .max(0.0);
                                        let preview = format!(
                                            "{}: {}",
                                            single_line_alias(&entry.alias),
                                            snippet_preview_text(&entry)
                                        );
                                        let preview_rect = ui
                                            .allocate_exact_size(
                                                egui::vec2(preview_width, row_height),
                                                egui::Sense::hover(),
                                            )
                                            .0;
                                        let text_color = ui.visuals().text_color();
                                        let mut layout_job =
                                            egui::text::LayoutJob::simple_singleline(
                                                preview,
                                                egui::TextStyle::Body.resolve(ui.style()),
                                                text_color,
                                            );
                                        layout_job.wrap.max_width = preview_width;
                                        layout_job.wrap.max_rows = 1;
                                        layout_job.wrap.break_anywhere = true;
                                        let galley = ui.fonts(|fonts| fonts.layout_job(layout_job));
                                        let text_position = egui::pos2(
                                            preview_rect.left(),
                                            preview_rect.center().y - galley.size().y / 2.0,
                                        );
                                        ui.painter().with_clip_rect(preview_rect).galley(
                                            text_position,
                                            galley,
                                            text_color,
                                        );
                                        if ui
                                            .add_sized(
                                                egui::vec2(edit_width, row_height),
                                                egui::Button::new("Edit"),
                                            )
                                            .clicked()
                                        {
                                            self.edit_idx = Some(idx);
                                            self.alias = entry.alias.clone();
                                            self.text = entry.text.clone();
                                        }
                                        if ui
                                            .add_sized(
                                                egui::vec2(remove_width, row_height),
                                                egui::Button::new("Remove"),
                                            )
                                            .clicked()
                                        {
                                            remove = Some(idx);
                                        }
                                    });
                                }
                            });
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
    use super::{
        SnippetDialog, matches_snippet_filter, matching_snippet_indices, single_line_alias,
    };
    use crate::plugins::snippets::{SnippetEntry, save_snippets};

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.to_string(),
            text: text.to_string(),
            hide_contents: false,
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
    fn matching_count_uses_alias_and_hidden_body_filter_predicate() {
        let mut hidden = snippet("private", "launch-token payload");
        hidden.hide_contents = true;
        let entries = vec![snippet("Deploy", "release"), hidden];

        assert_eq!(matching_snippet_indices(&entries, "").len(), 2);
        assert_eq!(matching_snippet_indices(&entries, "DEPLOY"), vec![0]);
        assert_eq!(matching_snippet_indices(&entries, "launch-token"), vec![1]);
        assert!(matching_snippet_indices(&entries, "no match").is_empty());
    }

    #[test]
    fn alias_preview_normalizes_control_whitespace_without_changing_alias() {
        let entry = snippet("private\nalias", "body");

        assert_eq!(single_line_alias(&entry.alias), "private alias");
        assert_eq!(entry.alias, "private\nalias");
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
