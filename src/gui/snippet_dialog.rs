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
    InvalidEditorState,
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
    body_edit_session: u64,
    alias: String,
    text: String,
    hide_contents: bool,
    body_revealed: bool,
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

fn body_editor_id_source(session: u64) -> (&'static str, u64) {
    ("snippet_body_editor", session)
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
        self.open_from(SNIPPETS_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.reset_editor();
        self.filter.clear();
        self.inline_error = None;
        self.pending_removal = None;
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(SNIPPETS_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    pub fn end_session(&mut self) {
        self.open = false;
        self.reset_editor();
        self.filter.clear();
        self.load_error = None;
        self.inline_error = None;
        self.pending_removal = None;
    }

    pub fn open_edit(&mut self, alias: &str) {
        self.open_edit_from(SNIPPETS_FILE, alias);
    }

    fn open_edit_from(&mut self, path: &str, alias: &str) {
        self.pending_removal = None;
        self.inline_error = None;
        if self.load_from(path).is_err() {
            self.reset_editor();
            self.filter.clear();
            self.open = true;
            return;
        }
        self.filter.clear();
        if let Some(pos) = self.entries.iter().position(|e| e.alias == alias) {
            self.begin_existing(pos);
        } else {
            self.begin_new(alias);
        }
    }

    fn begin_existing(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index).cloned() else {
            return false;
        };
        self.body_edit_session = self.body_edit_session.wrapping_add(1);
        self.pending_removal = None;
        self.inline_error = None;
        self.edit_idx = Some(index);
        self.alias = entry.alias;
        self.text = entry.text;
        self.hide_contents = entry.hide_contents;
        self.body_revealed = !entry.hide_contents;
        self.open = true;
        true
    }

    fn begin_new(&mut self, alias: &str) {
        self.body_edit_session = self.body_edit_session.wrapping_add(1);
        self.pending_removal = None;
        self.inline_error = None;
        self.edit_idx = Some(self.entries.len());
        self.alias = alias.to_owned();
        self.text.clear();
        self.hide_contents = false;
        self.body_revealed = true;
        self.open = true;
    }

    fn reset_editor(&mut self) {
        self.edit_idx = None;
        self.alias.clear();
        self.text.clear();
        self.hide_contents = false;
        self.body_revealed = false;
    }

    fn apply_window_open(&mut self, open: bool) {
        if open {
            self.open = true;
        } else {
            self.end_session();
        }
    }

    fn cancel_editor(&mut self) {
        self.reset_editor();
        self.inline_error = None;
        self.pending_removal = None;
    }

    fn reveal_contents(&mut self) {
        if self.edit_idx.is_some() {
            self.body_revealed = true;
        }
    }

    fn set_hide_contents(&mut self, hide_contents: bool) {
        if self.hide_contents == hide_contents {
            return;
        }
        self.hide_contents = hide_contents;
        let editing_existing = self
            .edit_idx
            .is_some_and(|index| index < self.entries.len());
        if editing_existing && hide_contents {
            self.body_revealed = false;
        }
    }

    fn editor_candidate(
        &self,
    ) -> Result<(Vec<SnippetEntry>, Vec<SnippetEntry>, AliasValidation), &'static str> {
        let Some(index) = self.edit_idx else {
            return Err("No snippet is being edited.");
        };
        let expected = self.entries.clone();
        let mut candidate = expected.clone();
        let alias_validation = if index == candidate.len() {
            candidate.push(SnippetEntry {
                alias: self.alias.clone(),
                text: self.text.clone(),
                hide_contents: self.hide_contents,
            });
            AliasValidation {
                edited_index: None,
                alias: self.alias.clone(),
            }
        } else if let Some(entry) = candidate.get_mut(index) {
            entry.alias = self.alias.clone();
            entry.text = self.text.clone();
            entry.hide_contents = self.hide_contents;
            AliasValidation {
                edited_index: Some(index),
                alias: self.alias.clone(),
            }
        } else {
            return Err("The edited snippet no longer exists.");
        };
        Ok((expected, candidate, alias_validation))
    }

    fn save_editor(&mut self, path: &str) -> Result<(), CommitFailure> {
        let (expected, candidate, alias_validation) = match self.editor_candidate() {
            Ok(candidate) => candidate,
            Err(error) => {
                self.inline_error = Some(error.to_owned());
                return Err(CommitFailure::Rejected(CommitRejection::InvalidEditorState));
            }
        };
        self.commit_candidate(path, &expected, candidate, Some(alias_validation))?;
        self.reset_editor();
        Ok(())
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
                    CommitRejection::InvalidEditorState => {
                        "The editor is no longer available.".to_owned()
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
            self.end_session();
            return;
        }
        let (window_open, save_request, confirm_removal) = self.show_window(ctx, || {
            app.report_error_message("ui operation", "Both fields required");
        });

        if save_request {
            match self.save_editor(SNIPPETS_FILE) {
                Ok(()) => Self::finish_success(app, "Saved snippet"),
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
        self.apply_window_open(window_open);
    }

    fn show_window(
        &mut self,
        ctx: &egui::Context,
        mut report_required_fields: impl FnMut(),
    ) -> (bool, bool, bool) {
        let mut window_open = self.open;
        let mut close = false;
        let mut save_request = false;
        let mut confirm_removal = false;
        egui::Window::new("Snippets")
            .default_size((600.0, 500.0))
            .min_width(160.0)
            .min_height(160.0)
            .resizable(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                // Resize follows content in egui. Bound both axes here so overflow
                // scrolls inside the user's chosen size instead of enlarging it.
                egui::ScrollArea::both()
                    .id_source("snippet_dialog_contents")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                // Only controls set the minimum scrollable width, never the body.
                ui.set_min_width(280.0);
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
                if self.edit_idx.is_some() {
                    ui.horizontal(|ui| {
                        ui.label("Alias");
                        let width = ui.available_width().max(80.0);
                        ui.add_sized(
                            [width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(&mut self.alias)
                                .desired_width(f32::INFINITY),
                        );
                    });
                    let mut hide_contents = self.hide_contents;
                    if ui
                        .checkbox(&mut hide_contents, "Hide contents")
                        .on_hover_text(
                            "Hides previews only; saved text and copied clipboard/history remain plaintext.",
                        )
                        .changed()
                    {
                        self.set_hide_contents(hide_contents);
                    }
                    ui.label("Text");
                    let action_height = ui.spacing().interact_size.y
                        + ui.spacing().button_padding.y * 2.0
                        + ui.spacing().item_spacing.y;
                    let editor_height = (ui.available_height() - action_height).max(120.0);
                    let font_id = egui::TextStyle::Body.resolve(ui.style());
                    let line_height = ui.fonts(|fonts| fonts.row_height(&font_id));
                    let desired_rows =
                        (editor_height / line_height).floor().clamp(4.0, 48.0) as usize;
                    if self.body_revealed {
                        egui::ScrollArea::both()
                            .id_source("snippet_dialog_body")
                            .auto_shrink([false, false])
                            .max_height(editor_height)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.text)
                                        .id_source(body_editor_id_source(
                                            self.body_edit_session,
                                        ))
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(desired_rows),
                                );
                            });
                    } else {
                        ui.label("Contents hidden");
                        if ui.button("Reveal to Edit").clicked() {
                            self.reveal_contents();
                        }
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            if self.alias.trim().is_empty() || self.text.trim().is_empty() {
                                report_required_fields();
                            } else {
                                save_request = true;
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.cancel_editor();
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
                            .add_sized(
                                [filter_width, ui.spacing().interact_size.y],
                                egui::TextEdit::singleline(&mut self.filter)
                                    .desired_width(f32::INFINITY),
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
                        egui::ScrollArea::both()
                            .id_source("snippet_dialog_list")
                            .auto_shrink([false, false])
                            .max_height(list_height)
                            .show(ui, |ui| {
                                for idx in matching_indices {
                                    let entry = self.entries[idx].clone();
                                    ui.horizontal(|ui| {
                                        if entry.hide_contents {
                                            ui.small("Hidden")
                                                .on_hover_text("Contents are hidden in previews.");
                                        }
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
                                                self.begin_existing(idx);
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
                        self.begin_new("");
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                }
                });
            });
        if close {
            window_open = false;
        }
        (window_open, save_request, confirm_removal)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AliasValidation, CommitFailure, CommitRejection, SnippetDialog, body_editor_id_source,
        matches_snippet_filter, matching_snippet_indices, single_line_alias,
    };
    use crate::plugins::snippets::{SnippetEntry, load_snippets, save_snippets};
    use eframe::egui;

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.to_string(),
            text: text.to_string(),
            hide_contents: false,
        }
    }

    fn dialog_frame(
        ctx: &egui::Context,
        dialog: &mut SnippetDialog,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                dialog.show_window(ctx, || panic!("unexpected field validation"));
            },
        )
    }

    fn dialog_rect(ctx: &egui::Context) -> egui::Rect {
        ctx.memory(|memory| memory.area_rect(egui::Id::new("Snippets")).unwrap())
    }

    #[test]
    fn snippet_dialog_size_is_independent_of_contents_and_filter() {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut dialog = SnippetDialog {
            open: true,
            entries: vec![snippet("short", "body")],
            ..Default::default()
        };
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let size = dialog_rect(&ctx).size();
        assert!(size.x < 650.0 && size.y < 550.0, "{size:?}");

        dialog.entries = vec![snippet(&"alias".repeat(2000), &"body λ\n".repeat(2000))];
        for _ in 0..20 {
            let output = dialog_frame(&ctx, &mut dialog, Vec::new());
            assert_eq!(dialog_rect(&ctx).size(), size);
            for shape in &output.shapes {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && matches!(text.galley.job.text.as_str(), "Edit" | "Remove")
                {
                    assert!(text.pos.x + text.galley.size().x < dialog_rect(&ctx).right());
                }
            }
            assert!(output.viewport_output.values().all(|viewport| {
                !viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_)))
            }));
        }
        dialog.filter = "no match".into();
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
        dialog.filter.clear();
        dialog.entries[0].hide_contents = true;
        let output = dialog_frame(&ctx, &mut dialog, Vec::new());
        assert_eq!(dialog_rect(&ctx).size(), size);
        assert!(output.shapes.iter().all(|shape| {
            !matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.job.text.contains("body λ"))
        }));
        dialog.begin_existing(0);
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
        dialog.reveal_contents();
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
    }

    #[test]
    fn snippet_dialog_user_resize_and_scrolling_preserve_window_bounds() {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut dialog = SnippetDialog {
            open: true,
            entries: (0..80)
                .map(|i| snippet(&format!("row-{i}"), "body"))
                .collect(),
            ..Default::default()
        };
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let original = dialog_rect(&ctx);
        let corner = original.max - egui::vec2(2.0, 2.0);
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(corner),
                egui::Event::PointerButton {
                    pos: corner,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let resized_corner = original.min + egui::vec2(240.0, 260.0);
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![egui::Event::PointerMoved(resized_corner)],
        );
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![egui::Event::PointerButton {
                pos: resized_corner,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let resized = dialog_rect(&ctx);
        assert!(
            resized.width() < 280.0 && resized.height() < 300.0,
            "{resized:?}"
        );

        let text_position = |output: &egui::FullOutput, prefix: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) if text.galley.job.text.starts_with(prefix) => {
                        Some(text.pos)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no visible text starting with {prefix}"))
        };
        let before = dialog_frame(&ctx, &mut dialog, Vec::new());
        let filter = text_position(&before, "Filter");
        let count = text_position(&before, "80 of 80 snippets");
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(filter),
                egui::Event::Scroll(egui::vec2(-100.0, 0.0)),
            ],
        );
        let after = dialog_frame(&ctx, &mut dialog, Vec::new());
        assert!(text_position(&after, "80 of 80 snippets").x < count.x);
        assert_eq!(dialog_rect(&ctx), resized);

        let before = dialog_frame(&ctx, &mut dialog, Vec::new());
        let first_visible_row = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => text
                        .galley
                        .job
                        .text
                        .strip_prefix("row-")
                        .and_then(|text| text.split_once(':'))
                        .and_then(|(index, _)| index.parse::<usize>().ok()),
                    _ => None,
                })
                .min()
                .expect("at least one list row remains visible")
        };
        assert_eq!(first_visible_row(&before), 0);
        let first_row_position = text_position(&before, "row-0:");
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(resized.center()),
                egui::Event::Scroll(egui::vec2(0.0, -100.0)),
            ],
        );
        let after = dialog_frame(&ctx, &mut dialog, Vec::new());
        // Smooth scrolling can move part of a row before it leaves the clip rect.
        assert!(
            first_visible_row(&after) > 0
                || text_position(&after, "row-0:").y < first_row_position.y,
            "list did not scroll vertically"
        );
        assert_eq!(dialog_rect(&ctx), resized);
        dialog.end_session();
        dialog.open = true;
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        assert_eq!(dialog_rect(&ctx), resized);
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
    fn editor_initializes_saved_privacy_and_new_drafts_safely() {
        let mut hidden = snippet("masked", "private\nline");
        hidden.hide_contents = true;
        let dialog_entries = vec![hidden.clone(), snippet("visible", "public")];
        let mut dialog = SnippetDialog {
            entries: dialog_entries,
            ..SnippetDialog::default()
        };

        assert!(dialog.begin_existing(0));
        assert!(dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, hidden.text);

        assert!(dialog.begin_existing(1));
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);

        dialog.begin_new("new");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert!(dialog.text.is_empty());

        dialog.set_hide_contents(true);
        assert!(dialog.hide_contents);
        assert!(
            dialog.body_revealed,
            "new drafts remain editable while masked"
        );
    }

    #[test]
    fn command_edit_path_uses_concealed_state_for_existing_masked_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut hidden = snippet("masked", "private λ\nbody");
        hidden.hide_contents = true;
        save_snippets(
            path.to_str().unwrap(),
            &[hidden.clone(), snippet("visible", "public")],
        )
        .unwrap();
        let mut dialog = SnippetDialog::default();

        dialog.open_edit_from(path.to_str().unwrap(), "masked");
        assert!(dialog.open);
        assert!(dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, hidden.text);

        dialog.open_edit_from(path.to_str().unwrap(), "visible");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert_eq!(dialog.text, "public");

        dialog.open_edit_from(path.to_str().unwrap(), "new alias");
        assert_eq!(dialog.alias, "new alias");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert!(dialog.text.is_empty());
    }

    #[test]
    fn body_editor_undo_state_is_isolated_between_edit_sessions() {
        use std::cell::Cell;

        let mut masked = snippet("private", "confidential λ\nsecond line");
        masked.hide_contents = true;
        let mut dialog = SnippetDialog {
            entries: vec![masked, snippet("public", "public body")],
            ..SnippetDialog::default()
        };
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);
        dialog.reveal_contents();

        let context = egui::Context::default();
        let editor_bounds = Cell::new(egui::Rect::NOTHING);
        let frame = |text: &mut String, session: u64, time: f64, events: Vec<egui::Event>| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .expect("root viewport is present in RawInput")
                .inner_rect = input.screen_rect;
            context.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(text)
                            .id_source(body_editor_id_source(session))
                            .desired_width(300.0)
                            .desired_rows(4),
                    );
                    editor_bounds.set(response.rect);
                });
            })
        };

        let first_session = dialog.body_edit_session;
        let _ = frame(&mut dialog.text, first_session, 0.0, Vec::new());
        let point = editor_bounds.get().left_top() + egui::vec2(12.0, 12.0);
        let pointer_button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = frame(
            &mut dialog.text,
            first_session,
            0.1,
            vec![egui::Event::PointerMoved(point)],
        );
        let _ = frame(
            &mut dialog.text,
            first_session,
            0.2,
            vec![pointer_button(true)],
        );
        let _ = frame(
            &mut dialog.text,
            first_session,
            0.3,
            vec![pointer_button(false)],
        );
        let _ = frame(
            &mut dialog.text,
            first_session,
            0.4,
            vec![egui::Event::Text(" edited".into())],
        );
        assert_ne!(dialog.text, "confidential λ\nsecond line");

        dialog.set_hide_contents(false);
        dialog.set_hide_contents(true);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.body_edit_session, first_session);
        dialog.reveal_contents();
        assert_eq!(dialog.body_edit_session, first_session);
        dialog.cancel_editor();
        assert!(dialog.begin_existing(1));
        let second_session = dialog.body_edit_session;
        assert_ne!(second_session, first_session);
        assert_eq!(dialog.text, "public body");

        let _ = frame(&mut dialog.text, second_session, 0.5, Vec::new());
        let point = editor_bounds.get().left_top() + egui::vec2(12.0, 12.0);
        let pointer_button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let _ = frame(
            &mut dialog.text,
            second_session,
            0.6,
            vec![egui::Event::PointerMoved(point)],
        );
        let _ = frame(
            &mut dialog.text,
            second_session,
            0.7,
            vec![pointer_button(true)],
        );
        let _ = frame(
            &mut dialog.text,
            second_session,
            0.8,
            vec![pointer_button(false)],
        );
        let _ = frame(
            &mut dialog.text,
            second_session,
            0.9,
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            }],
        );
        assert_eq!(dialog.text, "public body");
    }

    #[test]
    fn new_masked_draft_persists_plaintext_and_resets_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        save_snippets(path.to_str().unwrap(), &[]).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        dialog.begin_new("masked");
        let body = "λ first line\n  second 中文 🚀\n";
        dialog.text = body.to_owned();
        dialog.set_hide_contents(true);
        assert!(dialog.body_revealed);

        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].text, body);
        assert!(candidate[0].hide_contents);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap()[0].text, body);
        assert!(load_snippets(path.to_str().unwrap()).unwrap()[0].hide_contents);
        assert!(dialog.text.is_empty());
        assert!(!dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn alias_and_flag_only_save_preserves_masked_multiline_unicode_body() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let body = "original λ\n  multiline 中文 🚀\n";
        let mut original = snippet("old alias", body);
        original.hide_contents = true;
        let initial = vec![original];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, body);

        dialog.alias = "new alias".to_owned();
        dialog.set_hide_contents(false);
        assert!(
            !dialog.body_revealed,
            "unmasking does not reveal the current session"
        );
        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].text, body);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].alias, "new alias");
        assert_eq!(saved[0].text, body);
        assert!(!saved[0].hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
    }

    #[test]
    fn hiding_existing_draft_conceals_but_preserves_edits_until_reveal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("visible", "original body")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.text = "draft λ\nbody".to_owned();
        dialog.set_hide_contents(true);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, "draft λ\nbody");
        dialog.set_hide_contents(false);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, "draft λ\nbody");

        dialog.reveal_contents();
        assert!(dialog.body_revealed);
        dialog.text = "revealed replacement 中文".to_owned();
        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].text, "revealed replacement 中文");
        assert!(!saved[0].hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn cancel_and_close_clear_revealed_draft_immediately() {
        let mut dialog = SnippetDialog {
            entries: vec![snippet("private", "body")],
            ..SnippetDialog::default()
        };
        dialog.reveal_contents();
        assert!(!dialog.body_revealed, "reveal requires an active editor");
        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "unsaved reveal".to_owned();
        dialog.cancel_editor();
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
        assert!(dialog.edit_idx.is_none());

        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "another unsaved reveal".to_owned();
        dialog.apply_window_open(false);
        assert!(!dialog.open);
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn failed_editor_save_preserves_revealed_session_draft() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = snippet("masked", "original body");
        original.hide_contents = true;
        save_snippets(path.to_str().unwrap(), &[original]).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.open_edit_from(path.to_str().unwrap(), "masked");
        dialog.reveal_contents();
        dialog.alias = "changed alias".to_owned();
        dialog.text = "unsaved revised λ\nbody".to_owned();
        dialog.set_hide_contents(false);
        let draft = dialog.text.clone();
        let invalid = b"external malformed update";
        std::fs::write(&path, invalid).unwrap();

        assert!(matches!(
            dialog.save_editor(path.to_str().unwrap()),
            Err(CommitFailure::Persistence(_))
        ));
        assert_eq!(dialog.edit_idx, Some(0));
        assert_eq!(dialog.alias, "changed alias");
        assert_eq!(dialog.text, draft);
        assert!(dialog.body_revealed);
        assert!(!dialog.hide_contents);
        assert!(dialog.load_error.is_some());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn ensure_open_is_idempotent_and_reopen_reloads_after_session_end() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = snippet("masked", "original body");
        original.hide_contents = true;
        save_snippets(path.to_str().unwrap(), &[original]).unwrap();
        let mut dialog = SnippetDialog::default();

        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "unsaved draft".to_owned();
        let external = vec![snippet("masked", "external update")];
        save_snippets(path.to_str().unwrap(), &external).unwrap();

        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.open);
        assert_eq!(dialog.text, "unsaved draft");
        assert!(dialog.body_revealed);
        assert_eq!(dialog.entries[0].text, "original body");

        dialog.end_session();
        assert!(dialog.text.is_empty());
        assert!(!dialog.body_revealed);
        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.open);
        assert!(dialog.edit_idx.is_none());
        assert!(dialog.text.is_empty());
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.entries, external);
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
