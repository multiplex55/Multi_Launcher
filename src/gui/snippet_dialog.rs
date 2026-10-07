use super::{LauncherApp, push_toast};
use crate::plugins::snippets::{
    SNIPPETS_FILE, SnippetEntry, load_snippets, snippet_preview_text, update_snippets,
};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRemoval {
    index: usize,
    expected_entry: SnippetEntry,
    expected_entries: Vec<SnippetEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AliasValidation {
    edited_index: Option<usize>,
    alias: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommitRejection {
    StaleSnapshot,
    DuplicateAlias,
}

#[derive(Debug)]
enum CommitFailure {
    Rejected(CommitRejection),
    Persistence(anyhow::Error),
}

#[derive(Default)]
pub struct SnippetDialog {
    pub open: bool,
    entries: Vec<SnippetEntry>,
    edit_idx: Option<usize>,
    alias: String,
    text: String,
    filter: String,
    load_error: Option<String>,
    inline_error: Option<String>,
    pending_removal: Option<PendingRemoval>,
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

fn alias_is_duplicate(snippets: &[SnippetEntry], validation: &AliasValidation) -> bool {
    if validation.edited_index.is_some_and(|index| {
        snippets
            .get(index)
            .is_some_and(|entry| entry.alias == validation.alias)
    }) {
        return false;
    }

    snippets.iter().enumerate().any(|(index, entry)| {
        Some(index) != validation.edited_index && entry.alias == validation.alias
    })
}

fn update_snapshot(
    path: &str,
    expected: &[SnippetEntry],
    candidate: Vec<SnippetEntry>,
    alias_validation: Option<AliasValidation>,
) -> Result<Vec<SnippetEntry>, CommitFailure> {
    let mut rejection = None;
    let committed = update_snippets(path, |current| {
        if current.as_slice() != expected {
            rejection = Some(CommitRejection::StaleSnapshot);
            return Ok(false);
        }
        if let Some(validation) = &alias_validation
            && alias_is_duplicate(current, validation)
        {
            rejection = Some(CommitRejection::DuplicateAlias);
            return Ok(false);
        }

        *current = candidate;
        Ok(true)
    })
    .map_err(CommitFailure::Persistence)?;

    if let Some(rejection) = rejection {
        Err(CommitFailure::Rejected(rejection))
    } else {
        Ok(committed)
    }
}

fn is_load_failure(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<crate::common::persistence::PersistenceError>(),
        Some(
            crate::common::persistence::PersistenceError::Read { .. }
                | crate::common::persistence::PersistenceError::MalformedJson { .. }
        )
    )
}

impl SnippetDialog {
    pub fn open(&mut self) {
        let _ = self.load_from(SNIPPETS_FILE);
        self.open = true;
        self.edit_idx = None;
        self.alias.clear();
        self.text.clear();
        self.filter.clear();
        self.inline_error = None;
        self.pending_removal = None;
    }

    pub fn open_edit(&mut self, alias: &str) {
        self.pending_removal = None;
        self.inline_error = None;
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
        self.pending_removal = None;
        self.inline_error = None;
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

    fn commit_candidate(
        &mut self,
        path: &str,
        expected: &[SnippetEntry],
        candidate: Vec<SnippetEntry>,
        alias_validation: Option<AliasValidation>,
    ) -> Result<(), CommitFailure> {
        match update_snapshot(path, expected, candidate, alias_validation) {
            Ok(committed) => {
                self.entries = committed;
                self.load_error = None;
                self.inline_error = None;
                Ok(())
            }
            Err(CommitFailure::Rejected(rejection)) => {
                self.inline_error = Some(match &rejection {
                    CommitRejection::StaleSnapshot => {
                        "Snippets changed since this view was loaded. Reload and try again."
                            .to_owned()
                    }
                    CommitRejection::DuplicateAlias => {
                        "An entry with this exact alias already exists.".to_owned()
                    }
                });
                Err(CommitFailure::Rejected(rejection))
            }
            Err(CommitFailure::Persistence(error)) => {
                let error_message = error.to_string();
                if is_load_failure(&error) {
                    self.load_error = Some(error_message.clone());
                }
                self.inline_error = Some(format!("Failed to save snippets: {error_message}"));
                Err(CommitFailure::Persistence(error))
            }
        }
    }

    fn request_removal(&mut self, index: usize) {
        let Some(expected_entry) = self.entries.get(index).cloned() else {
            return;
        };
        self.pending_removal = Some(PendingRemoval {
            index,
            expected_entry,
            expected_entries: self.entries.clone(),
        });
        self.inline_error = None;
    }

    fn cancel_removal(&mut self) {
        self.pending_removal = None;
        self.inline_error = None;
    }

    fn filter_changed(&mut self) {
        self.pending_removal = None;
        self.inline_error = None;
    }

    fn confirm_removal(&mut self, path: &str) -> Result<bool, CommitFailure> {
        let Some(pending) = self.pending_removal.clone() else {
            return Ok(false);
        };
        if pending.expected_entries.get(pending.index) != Some(&pending.expected_entry) {
            self.pending_removal = None;
            let rejection = CommitRejection::StaleSnapshot;
            self.inline_error = Some(
                "Snippets changed since this view was loaded. Reload and try again.".to_owned(),
            );
            return Err(CommitFailure::Rejected(rejection));
        }

        let mut candidate = pending.expected_entries.clone();
        candidate.remove(pending.index);
        match self.commit_candidate(path, &pending.expected_entries, candidate, None) {
            Ok(()) => {
                self.pending_removal = None;
                Ok(true)
            }
            Err(error @ CommitFailure::Rejected(CommitRejection::StaleSnapshot)) => {
                self.pending_removal = None;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    fn finish_success(app: &mut LauncherApp, message: &str) {
        if app.enable_toasts {
            push_toast(
                &mut app.toasts,
                Toast {
                    text: message.into(),
                    kind: ToastKind::Success,
                    options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),
                },
            );
        }
        app.search();
        app.focus_input();
    }

    fn report_commit_failure(app: &mut LauncherApp, failure: &CommitFailure) {
        if let CommitFailure::Persistence(error) = failure {
            app.report_error_message("ui operation", format!("Failed to save snippets: {error}"));
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            self.pending_removal = None;
            return;
        }
        let mut window_open = self.open;
        let mut close = false;
        let mut save_request = None;
        let mut confirm_removal = false;
        egui::Window::new("Snippets")
            .default_size((600.0, 500.0))
            .min_width(380.0)
            .min_height(320.0)
            .resizable(true)
            .open(&mut window_open)
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
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(editor_height)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.text)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(desired_rows),
                            );
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.alias.trim().is_empty() || self.text.trim().is_empty() {
                                app.report_error_message("ui operation", "Both fields required");
                            } else {
                                let expected = self.entries.clone();
                                let mut candidate = expected.clone();
                                let edited_index = if idx == candidate.len() {
                                    candidate.push(SnippetEntry {
                                        alias: self.alias.clone(),
                                        text: self.text.clone(),
                                        hide_contents: false,
                                    });
                                    None
                                } else if let Some(entry) = candidate.get_mut(idx) {
                                    entry.alias = self.alias.clone();
                                    entry.text = self.text.clone();
                                    Some(idx)
                                } else {
                                    self.inline_error =
                                        Some("The edited snippet no longer exists.".to_owned());
                                    None
                                };
                                if idx <= expected.len() {
                                    save_request = Some((
                                        expected,
                                        candidate,
                                        Some(AliasValidation {
                                            edited_index,
                                            alias: self.alias.clone(),
                                        }),
                                    ));
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.edit_idx = None;
                            self.inline_error = None;
                            self.pending_removal = None;
                        }
                    });
                    if let Some(error) = &self.inline_error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        let clear_filter_width = 116.0;
                        let filter_spacing = ui.spacing().item_spacing.x;
                        let filter_width =
                            (ui.available_width() - clear_filter_width - filter_spacing).max(80.0);
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut self.filter)
                                    .desired_width(filter_width),
                            )
                            .changed()
                        {
                            self.filter_changed();
                        }
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
                            self.filter_changed();
                        }
                    });
                    let matching_indices = matching_snippet_indices(&self.entries, &self.filter);
                    ui.label(format!(
                        "{} of {} snippets",
                        matching_indices.len(),
                        self.entries.len()
                    ));
                    if let Some(error) = &self.inline_error {
                        ui.colored_label(egui::Color32::RED, error);
                    }
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
                                        let edit_width = 80.0;
                                        let remove_width = 72.0;
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

                                        let confirming = self
                                            .pending_removal
                                            .as_ref()
                                            .is_some_and(|pending| pending.index == idx);
                                        if confirming {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(edit_width, row_height),
                                                    egui::Button::new("Confirm"),
                                                )
                                                .clicked()
                                            {
                                                confirm_removal = true;
                                            }
                                            if ui
                                                .add_sized(
                                                    egui::vec2(remove_width, row_height),
                                                    egui::Button::new("Cancel"),
                                                )
                                                .clicked()
                                            {
                                                self.cancel_removal();
                                            }
                                        } else {
                                            if ui
                                                .add_sized(
                                                    egui::vec2(edit_width, row_height),
                                                    egui::Button::new("Edit"),
                                                )
                                                .clicked()
                                            {
                                                self.pending_removal = None;
                                                self.inline_error = None;
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
                                                self.request_removal(idx);
                                            }
                                        }
                                    });
                                }
                            });
                    }
                    if ui.button("Add Snippet").clicked() {
                        self.pending_removal = None;
                        self.inline_error = None;
                        self.edit_idx = Some(self.entries.len());
                        self.alias.clear();
                        self.text.clear();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
            });

        self.open = window_open;
        if let Some((expected, candidate, alias_validation)) = save_request {
            match self.commit_candidate(SNIPPETS_FILE, &expected, candidate, alias_validation) {
                Ok(()) => {
                    Self::finish_success(app, "Saved snippet");
                    self.edit_idx = None;
                    self.alias.clear();
                    self.text.clear();
                }
                Err(failure) => Self::report_commit_failure(app, &failure),
            }
        }
        if confirm_removal {
            match self.confirm_removal(SNIPPETS_FILE) {
                Ok(true) => Self::finish_success(app, "Removed snippet"),
                Ok(false) => {}
                Err(failure) => Self::report_commit_failure(app, &failure),
            }
        }
        if close {
            self.open = false;
        }
        if !self.open {
            self.pending_removal = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AliasValidation, CommitFailure, CommitRejection, SnippetDialog, matches_snippet_filter,
        matching_snippet_indices, single_line_alias,
    };
    use crate::plugins::snippets::{SnippetEntry, load_snippets, save_snippets};

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
    fn remove_request_and_cancel_do_not_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let entries = vec![snippet("first", "value")];
        save_snippets(path.to_str().unwrap(), &entries).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        dialog.request_removal(0);
        assert_eq!(dialog.pending_removal.as_ref().unwrap().index, 0);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

        dialog.cancel_removal();
        assert!(dialog.pending_removal.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), entries);
    }

    #[test]
    fn confirm_removes_the_selected_duplicate_row_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let entries = vec![
            snippet("same", "first body"),
            snippet("same", "second body"),
        ];
        save_snippets(path.to_str().unwrap(), &entries).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        dialog.request_removal(1);
        assert!(dialog.confirm_removal(path.to_str().unwrap()).unwrap());

        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            vec![entries[0].clone()]
        );
        assert!(dialog.pending_removal.is_none());
    }

    #[test]
    fn filter_changes_and_new_targets_invalidate_pending_confirmation() {
        let mut dialog = SnippetDialog {
            entries: vec![snippet("first", "one"), snippet("second", "two")],
            ..SnippetDialog::default()
        };

        dialog.request_removal(0);
        dialog.filter_changed();
        assert!(dialog.pending_removal.is_none());

        dialog.request_removal(0);
        dialog.request_removal(1);
        let pending = dialog.pending_removal.as_ref().unwrap();
        assert_eq!(pending.index, 1);
        assert_eq!(pending.expected_entry.alias, "second");
    }

    #[test]
    fn stale_removal_confirmation_keeps_external_update_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("first", "one")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        dialog.request_removal(0);

        let mut externally_updated = initial.clone();
        externally_updated.push(snippet("external", "append"));
        save_snippets(path.to_str().unwrap(), &externally_updated).unwrap();
        let external_bytes = std::fs::read(&path).unwrap();

        assert!(matches!(
            dialog.confirm_removal(path.to_str().unwrap()),
            Err(CommitFailure::Rejected(CommitRejection::StaleSnapshot))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external_bytes);
        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            externally_updated
        );
        assert!(dialog.pending_removal.is_none());
        assert!(dialog.inline_error.is_some());
    }

    #[test]
    fn exact_alias_validation_rejects_create_and_rename_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("existing", "one"), snippet("other", "two")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut created = initial.clone();
        created.push(snippet("existing", "new"));
        assert!(matches!(
            dialog.commit_candidate(
                path.to_str().unwrap(),
                &initial,
                created,
                Some(AliasValidation {
                    edited_index: None,
                    alias: "existing".to_owned(),
                }),
            ),
            Err(CommitFailure::Rejected(CommitRejection::DuplicateAlias))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert!(dialog.load_error.is_none());
        assert!(dialog.inline_error.is_some());

        let mut renamed = initial.clone();
        renamed[1].alias = "existing".to_owned();
        assert!(matches!(
            dialog.commit_candidate(
                path.to_str().unwrap(),
                &initial,
                renamed,
                Some(AliasValidation {
                    edited_index: Some(1),
                    alias: "existing".to_owned(),
                }),
            ),
            Err(CommitFailure::Rejected(CommitRejection::DuplicateAlias))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(dialog.entries, initial);
    }

    #[test]
    fn alias_validation_is_case_sensitive_and_preserves_untrimmed_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("Foo", "one")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut candidate = initial.clone();
        candidate.push(snippet("foo", "lowercase"));
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &initial,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: None,
                    alias: "foo".to_owned(),
                }),
            )
            .unwrap();

        let expected = candidate;
        let mut candidate = expected.clone();
        candidate.push(snippet(" Foo ", "spaced"));
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &expected,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: None,
                    alias: " Foo ".to_owned(),
                }),
            )
            .unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), candidate);
    }

    #[test]
    fn unchanged_alias_is_allowed_for_historical_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![
            snippet("duplicate", "first"),
            snippet("duplicate", "second"),
        ];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut candidate = initial.clone();
        candidate[0].text = "updated first".to_owned();
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &initial,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: Some(0),
                    alias: "duplicate".to_owned(),
                }),
            )
            .unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), candidate);
    }

    #[test]
    fn external_append_body_and_privacy_changes_reject_stale_snapshot() {
        for external_change in ["append", "body", "privacy"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("snippets.json");
            let initial = vec![snippet("saved", "value")];
            save_snippets(path.to_str().unwrap(), &initial).unwrap();
            let mut dialog = SnippetDialog::default();
            dialog.load_from(path.to_str().unwrap()).unwrap();

            let mut externally_updated = initial.clone();
            match external_change {
                "append" => externally_updated.push(snippet("external", "append")),
                "body" => externally_updated[0].text = "external body".to_owned(),
                "privacy" => externally_updated[0].hide_contents = true,
                _ => unreachable!(),
            }
            save_snippets(path.to_str().unwrap(), &externally_updated).unwrap();
            let external_bytes = std::fs::read(&path).unwrap();
            let mut candidate = initial.clone();
            candidate[0].text = "draft update".to_owned();

            assert!(matches!(
                dialog.commit_candidate(path.to_str().unwrap(), &initial, candidate, None),
                Err(CommitFailure::Rejected(CommitRejection::StaleSnapshot))
            ));
            assert_eq!(
                std::fs::read(&path).unwrap(),
                external_bytes,
                "{external_change}"
            );
            assert_eq!(
                load_snippets(path.to_str().unwrap()).unwrap(),
                externally_updated,
                "{external_change}"
            );
            assert_eq!(dialog.entries, initial, "{external_change}");
            assert!(dialog.load_error.is_none(), "{external_change}");
        }
    }

    #[test]
    fn malformed_reload_and_commit_keep_dialog_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("saved", "value")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid snippets";
        std::fs::write(&path, invalid).unwrap();

        assert!(
            dialog
                .commit_candidate(
                    path.to_str().unwrap(),
                    &initial,
                    vec![snippet("lost", "value")],
                    Some(AliasValidation {
                        edited_index: None,
                        alias: "lost".to_owned(),
                    }),
                )
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
        assert!(dialog.load_error.is_some());
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
    }
}
