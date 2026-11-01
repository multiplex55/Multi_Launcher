use crate::unicode_inspector::{
    Inspection, InspectionError, MAX_INPUT_BYTES, MAX_SCALAR_ROWS, ScalarObservation, inspect,
};
use eframe::egui::{self, Align, Context, Id, Layout, ScrollArea, TextEdit, Ui, Vec2, Window};

use super::utility_dialog_size::{
    UtilityDialogSizeWindowExt, UtilityWindowSizeDefault, UtilityWindowSizeRecipe,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InspectorStatus {
    ClipboardUnavailable,
    InputLimit,
}

/// Session-only Unicode inspector state. Raw text deliberately has no Debug or
/// serialization implementation and is cleared whenever the dialog closes.
pub struct UnicodeInspectorDialogState {
    pub open: bool,
    accepted_source: String,
    edit_buffer: String,
    inspection: Inspection,
    selected_scalar: Option<usize>,
    draft_matches_inspection: bool,
    status: Option<InspectorStatus>,
    session_generation: u64,
    inspection_revision: u64,
}

impl Default for UnicodeInspectorDialogState {
    fn default() -> Self {
        Self {
            open: false,
            accepted_source: String::new(),
            edit_buffer: String::new(),
            inspection: inspect("").expect("empty Unicode source is valid"),
            selected_scalar: None,
            draft_matches_inspection: true,
            status: None,
            session_generation: 0,
            inspection_revision: 0,
        }
    }
}

impl UnicodeInspectorDialogState {
    pub fn open_session(&mut self) {
        if !self.open {
            self.clear_session_data();
            self.open = true;
        }
    }

    pub fn close_session(&mut self) {
        self.open = false;
        self.clear_session_data();
    }

    fn clear_session_data(&mut self) {
        self.bump_editor_identity();
        self.bump_inspection_revision();
        self.accepted_source.clear();
        self.edit_buffer.clear();
        self.inspection = inspect("").expect("empty Unicode source is valid");
        self.selected_scalar = None;
        self.draft_matches_inspection = true;
        self.status = None;
    }

    fn clear_source(&mut self) {
        self.clear_session_data();
        self.open = true;
    }

    fn paste_from(&mut self, read_clipboard: impl FnOnce() -> Result<String, ()>) {
        match read_clipboard() {
            Ok(candidate) => self.accept_candidate(candidate),
            Err(()) => self.status = Some(InspectorStatus::ClipboardUnavailable),
        }
    }

    fn inspect_draft(&mut self) {
        self.accept_candidate(self.edit_buffer.clone());
    }

    fn accept_candidate(&mut self, candidate: String) {
        match inspect(&candidate) {
            Ok(inspection) => {
                self.accepted_source = candidate.clone();
                self.edit_buffer = candidate;
                self.inspection = inspection;
                self.selected_scalar = None;
                self.draft_matches_inspection = true;
                self.status = None;
                self.bump_inspection_revision();
            }
            Err(error) => self.status = Some(status_for_inspection_error(error)),
        }
    }

    fn draft_changed(&mut self) {
        self.selected_scalar = None;
        if self.edit_buffer.len() > MAX_INPUT_BYTES
            || self.edit_buffer.chars().take(MAX_SCALAR_ROWS + 1).count() > MAX_SCALAR_ROWS
        {
            self.edit_buffer = self.accepted_source.clone();
            self.draft_matches_inspection = true;
            self.status = Some(InspectorStatus::InputLimit);
            self.bump_editor_identity();
        } else {
            self.draft_matches_inspection = false;
            self.status = None;
        }
    }

    fn bump_editor_identity(&mut self) {
        self.session_generation = self.session_generation.wrapping_add(1).max(1);
    }

    fn bump_inspection_revision(&mut self) {
        self.inspection_revision = self.inspection_revision.wrapping_add(1).max(1);
    }

    fn selected_scalar_text(&self) -> Option<String> {
        if !self.draft_matches_inspection {
            return None;
        }
        let row = self.inspection.rows().get(self.selected_scalar?)?;
        let character = self.accepted_source.chars().nth(row.scalar_index())?;
        (character == row.character()).then(|| character.to_string())
    }

    pub(crate) fn show(
        &mut self,
        ctx: &Context,
        handle_actions: bool,
        mut read_clipboard: impl FnMut() -> Result<String, ()>,
    ) {
        if !self.open {
            return;
        }

        let window_id = Id::new("unicode_inspector_dialog");
        let mut open = self.open;
        Window::new("Unicode Inspector")
            .id(window_id)
            .collapsible(false)
            .resizable(true)
            .min_size(Vec2::new(520.0, 380.0))
            .show_utility(
                ctx,
                "unicode-inspector",
                window_id,
                Some(&mut open),
                UtilityWindowSizeRecipe::new(
                    UtilityWindowSizeDefault::outer(Vec2::new(820.0, 620.0)),
                    Vec2::new(520.0, 380.0),
                    [None, None],
                ),
                |ui| self.contents(ui, ctx, handle_actions, &mut read_clipboard),
            );

        if open {
            self.open = true;
        } else {
            self.close_session();
        }
    }

    fn contents(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        handle_actions: bool,
        read_clipboard: &mut impl FnMut() -> Result<String, ()>,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Paste from Clipboard").clicked() && handle_actions {
                self.paste_from(|| read_clipboard());
            }
            if ui.button("Inspect Source").clicked() && handle_actions {
                self.inspect_draft();
            }
            if ui.button("Clear").clicked() && handle_actions {
                self.clear_source();
            }
            if ui.button("Copy Accepted Source").clicked() && handle_actions {
                ctx.copy_text(self.accepted_source.clone());
            }
            let selected_text = self.selected_scalar_text();
            if ui
                .add_enabled(
                    selected_text.is_some(),
                    egui::Button::new("Copy Selected Scalar"),
                )
                .clicked()
                && handle_actions
                && let Some(text) = selected_text
            {
                ctx.copy_text(text);
            }
        });
        ui.small(
            "Pasting inside the editor may normalize line endings. Use Paste from Clipboard to preserve exact CRLF text.",
        );

        let edit = ui.add(
            TextEdit::multiline(&mut self.edit_buffer)
                .id_source(Id::new((
                    "unicode_inspector_source",
                    self.session_generation,
                )))
                .interactive(handle_actions)
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .hint_text("Enter text to inspect"),
        );
        if edit.changed() {
            self.draft_changed();
        }

        if let Some(status) = self.status {
            ui.colored_label(egui::Color32::YELLOW, status.message());
        }

        if self.draft_matches_inspection {
            self.show_observations(ui, handle_actions);
        } else {
            ui.small(
                "The last valid source is retained. Select Inspect Source to analyze this draft.",
            );
        }
    }

    fn show_observations(&mut self, ui: &mut Ui, handle_actions: bool) {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} UTF-8 bytes", self.inspection.byte_count()));
            ui.separator();
            ui.label(format!(
                "{} Unicode scalars",
                self.inspection.scalar_count()
            ));
            ui.separator();
            ui.label(format!(
                "{} grapheme clusters",
                self.inspection.grapheme_count()
            ));
            ui.separator();
            ui.label(format!(
                "{} UTF-16 units",
                self.inspection.utf16_unit_count()
            ));
        });

        if self.inspection.rows().is_empty() {
            ui.label("No Unicode scalars in the accepted source.");
            return;
        }

        let row_height = ui.text_style_height(&egui::TextStyle::Body);
        let rows = self.inspection.rows();
        let selected = &mut self.selected_scalar;
        ScrollArea::vertical()
            .id_salt(("unicode_inspector_rows", self.inspection_revision))
            .show_rows(ui, row_height, rows.len(), |ui, range| {
                for index in range {
                    let row = &rows[index];
                    let label = format!(
                        "{}   U+{:04X}   {}:{}   byte {}   {}",
                        row.scalar_index(),
                        row.code_point(),
                        row.line(),
                        row.column(),
                        row.byte_offset(),
                        row.escaped(),
                    );
                    if ui
                        .selectable_label(*selected == Some(index), label)
                        .clicked()
                        && handle_actions
                    {
                        *selected = Some(index);
                    }
                }
            });

        if let Some(index) = self.selected_scalar
            && let Some(row) = self.inspection.rows().get(index)
        {
            ui.separator();
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.strong(format!("Selected scalar U+{:04X}", row.code_point()));
                ui.label(row.escaped());
            });
            show_scalar_details(ui, row);
        }
    }

    #[cfg(test)]
    pub(crate) fn accept_for_test(&mut self, text: String) {
        self.accept_candidate(text);
    }

    #[cfg(test)]
    fn select_for_test(&mut self, index: usize) {
        self.selected_scalar = Some(index);
    }

    #[cfg(test)]
    pub(crate) fn source_for_test(&self) -> &str {
        &self.accepted_source
    }

    #[cfg(test)]
    fn inspection_for_test(&self) -> &Inspection {
        &self.inspection
    }

    #[cfg(test)]
    fn editor_id_for_test(&self) -> Id {
        Id::new(("unicode_inspector_source", self.session_generation))
    }
}

impl InspectorStatus {
    fn message(self) -> &'static str {
        match self {
            Self::ClipboardUnavailable => {
                "Clipboard text could not be read. The previous source is retained."
            }
            Self::InputLimit => {
                "Input exceeds the 256 KiB or 65,536 scalar limit. The previous source is retained."
            }
        }
    }
}

fn status_for_inspection_error(error: InspectionError) -> InspectorStatus {
    match error {
        InspectionError::InputTooManyBytes { .. } | InspectionError::InputTooManyScalars { .. } => {
            InspectorStatus::InputLimit
        }
    }
}

fn show_scalar_details(ui: &mut Ui, row: &ScalarObservation) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Scalar index: {}", row.scalar_index()));
        ui.label(format!("Byte offset: {}", row.byte_offset()));
        ui.label(format!("Line: {}, column: {}", row.line(), row.column()));
        ui.label(format!("Category: {}", row.category_abbreviation()));
    });

    let name = row
        .unicode_name()
        .map(|name| name.to_string())
        .unwrap_or_else(|| "No assigned Unicode name".to_owned());
    ui.label(format!("Name: {name}"));

    let utf8 = row
        .utf8_encoding()
        .bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    let utf16 = row
        .utf16_encoding()
        .units()
        .iter()
        .map(|unit| format!("{unit:04X}"))
        .collect::<Vec<_>>()
        .join(" ");
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("UTF-8: {utf8}"));
        ui.label(format!("UTF-16: {utf16}"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("Whitespace: {}", row.is_whitespace()));
        ui.label(format!("Control: {}", row.is_control()));
        ui.label(format!("Combining: {}", row.is_combining()));
        ui.label(format!("Default ignorable: {}", row.is_default_ignorable()));
        ui.label(format!("Bidi control: {}", row.is_bidi_control()));
    });
    ui.small(crate::unicode_inspector::DATA_VERSION_NOTE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_inspector_session_is_empty_on_open_reopen_and_close() {
        let mut state = UnicodeInspectorDialogState::default();
        assert!(!state.open);
        assert_eq!(state.source_for_test(), "");

        state.open_session();
        let empty_session_revision = state.inspection_revision;
        state.accept_for_test("session text".into());
        let accepted_revision = state.inspection_revision;
        assert_ne!(accepted_revision, empty_session_revision);
        state.select_for_test(0);
        state.open_session();
        assert_eq!(state.source_for_test(), "session text");
        assert_eq!(state.selected_scalar, Some(0));
        assert_eq!(state.inspection_revision, accepted_revision);

        state.close_session();
        assert!(!state.open);
        assert_eq!(state.source_for_test(), "");
        assert!(state.inspection_for_test().rows().is_empty());
        assert_eq!(state.selected_scalar, None);

        state.open_session();
        assert_eq!(state.source_for_test(), "");
        assert!(state.inspection_for_test().rows().is_empty());
        assert_ne!(state.inspection_revision, accepted_revision);
    }

    #[test]
    fn unicode_inspector_close_reopen_retires_old_editor_undo_identity() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        state.accept_for_test("private undo history".into());
        let old_editor_id = state.editor_id_for_test();
        state.close_session();
        state.open_session();

        assert_ne!(state.editor_id_for_test(), old_editor_id);
        assert_eq!(state.source_for_test(), "");
        assert!(state.edit_buffer.is_empty());
    }

    #[test]
    fn unicode_inspector_explicit_paste_reads_once_and_keeps_exact_crlf() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        let mut reads = 0;
        state.paste_from(|| {
            reads += 1;
            Ok("A\r\nB".to_owned())
        });

        assert_eq!(reads, 1);
        assert_eq!(state.source_for_test(), "A\r\nB");
        assert_eq!(state.inspection_for_test().rows().len(), 4);
        assert_eq!(state.inspection_for_test().rows()[2].line(), 1);
    }

    #[test]
    fn unicode_inspector_paste_failure_and_limits_preserve_accepted_model_and_selection() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        state.accept_for_test("A雪".into());
        state.select_for_test(1);
        let accepted_rows = state.inspection_for_test().rows().len();

        let mut reads = 0;
        state.paste_from(|| {
            reads += 1;
            Err(())
        });
        assert_eq!(reads, 1);
        assert_eq!(state.source_for_test(), "A雪");
        assert_eq!(state.inspection_for_test().rows().len(), accepted_rows);
        assert_eq!(state.selected_scalar, Some(1));
        assert_eq!(state.status, Some(InspectorStatus::ClipboardUnavailable));

        state.status = None;
        let too_many_bytes = "x".repeat(MAX_INPUT_BYTES + 1);
        state.paste_from(|| Ok(too_many_bytes));
        assert_eq!(state.source_for_test(), "A雪");
        assert_eq!(state.inspection_for_test().rows().len(), accepted_rows);
        assert_eq!(state.selected_scalar, Some(1));
        assert_eq!(state.status, Some(InspectorStatus::InputLimit));

        let too_many_rows = "x".repeat(MAX_SCALAR_ROWS + 1);
        state.paste_from(|| Ok(too_many_rows));
        assert_eq!(state.source_for_test(), "A雪");
        assert_eq!(state.selected_scalar, Some(1));
        assert_eq!(state.status, Some(InspectorStatus::InputLimit));
    }

    #[test]
    fn unicode_inspector_oversized_editor_change_is_rejected_without_truncation() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        state.accept_for_test("accepted".into());
        state.select_for_test(0);
        let old_editor_id = state.editor_id_for_test();
        state.edit_buffer = "x".repeat(MAX_INPUT_BYTES + 1);
        state.draft_changed();

        assert_eq!(state.edit_buffer, "accepted");
        assert_eq!(state.source_for_test(), "accepted");
        assert_eq!(state.inspection_for_test().scalar_count(), 8);
        assert_eq!(state.selected_scalar, None);
        assert_eq!(state.status, Some(InspectorStatus::InputLimit));
        assert_ne!(state.editor_id_for_test(), old_editor_id);
    }

    #[test]
    fn unicode_inspector_copy_uses_original_scalar_and_rejects_stale_selection() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        state.accept_for_test("é\u{202e}".into());
        state.select_for_test(0);
        assert_eq!(state.selected_scalar_text().as_deref(), Some("é"));
        state.select_for_test(1);
        assert_eq!(state.selected_scalar_text().as_deref(), Some("\u{202e}"));

        state.draft_changed();
        assert_eq!(state.selected_scalar, None);
        assert_eq!(state.selected_scalar_text(), None);
        assert_eq!(state.source_for_test(), "é\u{202e}");
    }

    #[test]
    fn unicode_inspector_clear_wipes_source_rows_selection_and_status() {
        let mut state = UnicodeInspectorDialogState::default();
        state.open_session();
        state.accept_for_test("secret\r\ntext".into());
        state.select_for_test(1);
        state.status = Some(InspectorStatus::ClipboardUnavailable);
        state.clear_source();

        assert!(state.open);
        assert_eq!(state.source_for_test(), "");
        assert!(state.inspection_for_test().rows().is_empty());
        assert_eq!(state.selected_scalar, None);
        assert_eq!(state.status, None);
    }
}
