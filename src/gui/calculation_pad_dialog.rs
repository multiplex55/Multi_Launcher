use crate::calculation_pad::{
    CalculationPadSession, DocumentId, LineOutcome, PadDocumentSummary, PadStoreError,
    PendingTransition, SessionError, TransitionDecision, TransitionOutcome,
};
use crate::platform::app_data::AppDataRoot;
use eframe::egui;

pub(crate) struct CalculationPadDialog {
    pub(crate) open: bool,
    focus_source: bool,
    session: CalculationPadSession,
    open_chooser: bool,
    saved_documents: Vec<PadDocumentSummary>,
    save_as_open: bool,
    save_as_name: String,
    error: Option<String>,
    conflict_document: Option<DocumentId>,
    notice: Option<String>,
    #[cfg(test)]
    test_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_copy_all_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_line_copy_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_discard_rect: Option<egui::Rect>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CalculationPadDialogEffect {
    CopyText(String),
}

#[derive(Debug)]
enum DialogIntent {
    New,
    OpenChooser,
    RefreshDocuments,
    Open(DocumentId),
    OpenSaveAs,
    HideChooser,
    CancelSaveAs,
    Save,
    SaveAs(String),
    Decision(TransitionDecision),
}

impl CalculationPadDialog {
    pub(crate) fn new(root: &AppDataRoot) -> Self {
        Self {
            open: false,
            focus_source: false,
            session: CalculationPadSession::new(root),
            open_chooser: false,
            saved_documents: Vec::new(),
            save_as_open: false,
            save_as_name: String::new(),
            error: None,
            conflict_document: None,
            notice: None,
            #[cfg(test)]
            test_editor_id: None,
            #[cfg(test)]
            test_editor_rect: None,
            #[cfg(test)]
            test_copy_all_rect: None,
            #[cfg(test)]
            test_line_copy_rects: Vec::new(),
            #[cfg(test)]
            test_window_rect: None,
            #[cfg(test)]
            test_discard_rect: None,
        }
    }

    pub(crate) fn open(&mut self) {
        self.ensure_session_open();
        self.open = true;
        self.focus_source = true;
    }

    pub(crate) fn ensure_open(&mut self) {
        let was_open = self.open;
        self.ensure_session_open();
        self.open = true;
        if !was_open {
            self.focus_source = true;
        }
    }

    fn ensure_session_open(&mut self) {
        if self.session.source().is_none() && self.session.pending_transition().is_none() {
            match self.session.request_new() {
                Ok(_) => {}
                Err(error) => self.set_error(error),
            }
        }
    }

    pub(crate) fn request_close(&mut self) {
        if self.session.pending_transition().is_some() {
            self.open = true;
            return;
        }
        match self.session.request_close() {
            Ok(TransitionOutcome::Applied) => {
                self.open = self.session.source().is_some();
                if !self.open {
                    self.reset_transient_ui();
                } else {
                    self.focus_source = true;
                }
            }
            Ok(TransitionOutcome::DecisionRequired) => self.open = true,
            Ok(TransitionOutcome::Cancelled) => self.open = true,
            Err(error) => {
                self.set_error(error);
                self.open = true;
            }
        }
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.error = None;
                self.notice = Some("Copied to the clipboard.".into());
            }
            Err(error) => {
                self.conflict_document = None;
                self.error = Some(format!("Clipboard copy failed: {error}"));
                self.notice = None;
            }
        }
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
    ) -> Option<CalculationPadDialogEffect> {
        #[cfg(test)]
        {
            self.test_window_rect = None;
            self.test_discard_rect = None;
        }
        if !self.open {
            return None;
        }

        let mut window_open = self.open;
        let mut authorized_pass = false;
        let mut edited_source = None;
        let mut staged_save_as_name = self.save_as_name.clone();
        let mut source_focus_requested = false;
        let mut source_editor_id = None;
        let mut editor_history_action = None;
        let mut intent = None;
        let mut effect = None;
        let content_rect = ctx.content_rect();
        let min_size = egui::vec2(420.0, 300.0).min(content_rect.size());
        let output = egui::Window::new("Calculation Pad")
            .id(egui::Id::new("calculation-pad-window"))
            .open(&mut window_open)
            .resizable(true)
            .default_size(egui::vec2(900.0, 650.0))
            .min_size(min_size)
            .max_size(content_rect.size())
            .show(ctx, |ui| {
                authorized_pass =
                    first_ui_pass && !ctx.will_discard() && ui.is_visible() && !ui.is_sizing_pass();
                ui.add_enabled_ui(authorized_pass, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("calculation-pad-window-content")
                        .max_height(content_rect.height())
                        .show(ui, |ui| {
                            let editor_id = ui.make_persistent_id("calculation-pad-source");
                            source_editor_id = Some(editor_id);
                            self.clear_text_edit_undoer(ctx, editor_id);
                            if authorized_pass && self.session.pending_transition().is_none() {
                                editor_history_action =
                                    self.take_editor_history_shortcut(ui, editor_id);
                            }
                            let source = self.session.source().unwrap_or_default().to_owned();
                            let evaluation = self.session.evaluation().cloned();
                            self.contents_ui(
                                ui,
                                editor_id,
                                source,
                                evaluation.as_ref(),
                                authorized_pass,
                                &mut staged_save_as_name,
                                &mut source_focus_requested,
                                &mut edited_source,
                                &mut intent,
                                &mut effect,
                            );
                        });
                });
            });

        #[cfg(test)]
        {
            self.test_window_rect = output.as_ref().map(|output| output.response.rect);
        }
        #[cfg(not(test))]
        let _ = output;

        let effects_authorized = authorized_pass && !ctx.will_discard();
        if effects_authorized {
            if staged_save_as_name != self.save_as_name {
                self.save_as_name = staged_save_as_name;
            }
            if source_focus_requested {
                if let Some(editor_id) = source_editor_id {
                    ctx.memory_mut(|memory| memory.request_focus(editor_id));
                    ctx.request_repaint();
                }
                self.focus_source = false;
            }
            match editor_history_action {
                Some(true) => {
                    self.session.undo();
                    ctx.request_repaint();
                }
                Some(false) => {
                    self.session.redo();
                    ctx.request_repaint();
                }
                None => {}
            }
            if let Some(source) = edited_source {
                if let Err(error) = self.session.replace_source(source) {
                    self.set_error(error);
                } else {
                    self.notice = None;
                }
            }
            if let Some(intent) = intent {
                self.apply_intent(intent);
            }

            if !window_open && self.open {
                self.request_close();
            }
        }

        effects_authorized.then_some(effect).flatten()
    }

    fn contents_ui(
        &mut self,
        ui: &mut egui::Ui,
        editor_id: egui::Id,
        mut source: String,
        evaluation: Option<&crate::calculation_pad::SessionEvaluation>,
        authorized_pass: bool,
        staged_save_as_name: &mut String,
        source_focus_requested: &mut bool,
        edited_source: &mut Option<String>,
        intent: &mut Option<DialogIntent>,
        effect: &mut Option<CalculationPadDialogEffect>,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                *intent = Some(DialogIntent::New);
            }
            if ui.button("Open").clicked() {
                *intent = Some(DialogIntent::OpenChooser);
            }
            if ui
                .add_enabled(
                    self.session.pending_transition().is_none(),
                    egui::Button::new("Save"),
                )
                .clicked()
            {
                *intent = Some(DialogIntent::Save);
            }
            if ui.button("Save As…").clicked() {
                *intent = Some(DialogIntent::OpenSaveAs);
            }
            ui.separator();
            let title = self.session.current_name().unwrap_or("Untitled");
            ui.label(title);
            if self.session.is_dirty() {
                ui.colored_label(ui.visuals().warn_fg_color, "Unsaved changes");
            }
        });

        if let Some(error) = self.error.as_deref() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().error_fg_color, error);
                if let Some(id) = self.conflict_document
                    && self.session.pending_transition().is_none()
                    && ui.button("Reload saved version…").clicked()
                {
                    *intent = Some(DialogIntent::Open(id));
                }
                if self.conflict_document.is_some() {
                    ui.label("Save As keeps this draft; Keep Editing cancels the close request.");
                }
            });
        } else if let Some(notice) = self.notice.as_deref() {
            ui.label(notice);
        }

        if self.open_chooser {
            self.document_chooser_ui(ui, intent);
        }
        if self.save_as_open {
            Self::save_as_ui(ui, staged_save_as_name, intent);
        }

        egui::CollapsingHeader::new("Calculation Pad grammar")
            .id_salt("calculation-pad-grammar")
            .show(ui, |ui| {
                ui.label("One expression or name = expression per line; # starts a comment.");
                ui.label("Use earlier names or rN physical-line references (blank lines count).");
                ui.label("Conversions: 6 ft 2 in to cm; exact unsigned bases: 9007199254740993 dec to hex.");
                ui.label("Scalar math supports +, −, ×, ÷, ^, parentheses, named functions and constants.");
                ui.label("Units use the shared catalog; values are not combined with unit algebra.");
            });

        if let Some(pending) = self.session.pending_transition() {
            self.pending_transition_ui(ui, pending, intent);
        }

        ui.separator();
        let available_height = ui.available_height().max(120.0);
        let available_width = ui.available_width();
        if available_width >= 680.0 {
            ui.columns(2, |columns| {
                self.editor_ui(
                    &mut columns[0],
                    editor_id,
                    &mut source,
                    available_height,
                    authorized_pass,
                    source_focus_requested,
                    edited_source,
                );
                self.results_ui(&mut columns[1], evaluation, available_height, effect);
            });
        } else {
            let half_height = (available_height * 0.48).max(90.0);
            self.editor_ui(
                ui,
                editor_id,
                &mut source,
                half_height,
                authorized_pass,
                source_focus_requested,
                edited_source,
            );
            self.results_ui(
                ui,
                evaluation,
                (available_height - half_height).max(90.0),
                effect,
            );
        }
    }

    fn editor_ui(
        &mut self,
        ui: &mut egui::Ui,
        editor_id: egui::Id,
        source: &mut String,
        max_height: f32,
        authorized_pass: bool,
        source_focus_requested: &mut bool,
        edited_source: &mut Option<String>,
    ) {
        ui.heading("Source");
        let response = ui
            .add_enabled_ui(self.session.pending_transition().is_none(), |ui| {
                egui::ScrollArea::both()
                    .id_salt("calculation-pad-editor-scroll")
                    .max_height(max_height)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(source)
                                .id(editor_id)
                                .desired_rows(12)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .hint_text("total = 2 + 3\nr1 * 4"),
                        )
                    })
                    .inner
            })
            .inner;

        #[cfg(test)]
        {
            self.test_editor_id = Some(editor_id);
            self.test_editor_rect = Some(response.rect);
        }

        if response.changed() {
            *edited_source = Some(source.clone());
        }
        if authorized_pass && self.focus_source && self.session.pending_transition().is_none() {
            *source_focus_requested = true;
        }
    }

    fn results_ui(
        &mut self,
        ui: &mut egui::Ui,
        evaluation: Option<&crate::calculation_pad::SessionEvaluation>,
        max_height: f32,
        effect: &mut Option<CalculationPadDialogEffect>,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Results");
            let summary = evaluation.map(copy_all_results).unwrap_or_default();
            let response = ui.button("Copy all");
            #[cfg(test)]
            {
                self.test_copy_all_rect = Some(response.rect);
            }
            if response.clicked() {
                *effect = Some(CalculationPadDialogEffect::CopyText(summary));
            }
        });

        #[cfg(test)]
        self.test_line_copy_rects.clear();

        egui::ScrollArea::vertical()
            .id_salt("calculation-pad-results")
            .max_height(max_height)
            .show(ui, |ui| {
                let Some(evaluation) = evaluation else {
                    ui.label("Open the pad to start a calculation.");
                    return;
                };
                if let Some(diagnostic) = evaluation.diagnostic.as_ref() {
                    ui.colored_label(ui.visuals().error_fg_color, diagnostic.to_string());
                }
                for line in &evaluation.lines {
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(format!("r{}", line.line_index + 1));
                            if let Some(name) = line.assignment_name.as_deref() {
                                ui.strong(name);
                            }
                            if !line.dependency_indices.is_empty() {
                                let refs = line
                                    .dependency_indices
                                    .iter()
                                    .map(|index| format!("r{}", index + 1))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                ui.weak(format!("uses {refs}"));
                            }
                            match &line.outcome {
                                LineOutcome::Blank => {
                                    ui.weak("blank");
                                }
                                LineOutcome::Comment => {
                                    ui.weak("comment");
                                }
                                LineOutcome::Value(value) => {
                                    let formatted = value
                                        .formatted_result()
                                        .unwrap_or_else(|| "unavailable".into());
                                    ui.label(&formatted);
                                    if value.is_approximate() {
                                        ui.weak("approx.");
                                    }
                                    let response = ui.small_button("Copy");
                                    #[cfg(test)]
                                    self.test_line_copy_rects
                                        .push((line.line_index, response.rect));
                                    if response.clicked() {
                                        *effect =
                                            Some(CalculationPadDialogEffect::CopyText(formatted));
                                    }
                                }
                                LineOutcome::Error(error) => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        error.to_string(),
                                    );
                                }
                            }
                        });
                    });
                }
            });
    }

    fn document_chooser_ui(&mut self, ui: &mut egui::Ui, intent: &mut Option<DialogIntent>) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.strong("Saved Calculation Pads");
                if ui.small_button("Refresh").clicked() {
                    *intent = Some(DialogIntent::RefreshDocuments);
                }
                if ui.small_button("Hide").clicked() {
                    *intent = Some(DialogIntent::HideChooser);
                }
            });
            if self.saved_documents.is_empty() {
                ui.label("No saved pads yet.");
            }
            egui::ScrollArea::vertical()
                .id_salt("calculation-pad-document-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for document in &self.saved_documents {
                        let text = format!(
                            "{}  ·  {} lines  ·  {} bytes",
                            document.name, document.line_count, document.source_bytes
                        );
                        if ui.selectable_label(false, text).clicked() {
                            *intent = Some(DialogIntent::Open(document.id));
                        }
                    }
                });
        });
    }

    fn save_as_ui(ui: &mut egui::Ui, save_as_name: &mut String, intent: &mut Option<DialogIntent>) {
        ui.group(|ui| {
            ui.strong("Save Calculation Pad As");
            ui.horizontal_wrapped(|ui| {
                ui.label("Document name");
                ui.text_edit_singleline(save_as_name);
                if ui.button("Save Copy").clicked() {
                    *intent = Some(DialogIntent::SaveAs(save_as_name.clone()));
                }
                if ui.button("Cancel").clicked() {
                    *intent = Some(DialogIntent::CancelSaveAs);
                }
            });
        });
    }

    fn pending_transition_ui(
        &mut self,
        ui: &mut egui::Ui,
        transition: PendingTransition,
        intent: &mut Option<DialogIntent>,
    ) {
        let description = match transition {
            PendingTransition::New => "Save changes before creating a new pad?",
            PendingTransition::Open(_) => "Save changes before opening the selected pad?",
            PendingTransition::Close => "Save changes before closing the Calculation Pad?",
        };
        ui.group(|ui| {
            ui.strong("Unsaved changes");
            ui.label(description);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Save));
                }
                let discard = ui.button("Discard");
                #[cfg(test)]
                {
                    self.test_discard_rect = Some(discard.rect);
                }
                if discard.clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Discard));
                }
                if ui.button("Keep Editing").clicked() {
                    *intent = Some(DialogIntent::Decision(TransitionDecision::Cancel));
                }
            });
        });
    }

    fn apply_intent(&mut self, intent: DialogIntent) {
        match intent {
            DialogIntent::New => match self.session.request_new() {
                Ok(TransitionOutcome::Applied) => self.after_transition_applied(),
                Ok(TransitionOutcome::DecisionRequired) => self.clear_feedback(),
                Ok(TransitionOutcome::Cancelled) => self.clear_feedback(),
                Err(error) => self.set_error(error),
            },
            DialogIntent::OpenChooser => {
                self.open_chooser = true;
                self.load_saved_documents();
            }
            DialogIntent::RefreshDocuments => self.load_saved_documents(),
            DialogIntent::Open(id) => match self.session.request_open(id) {
                Ok(TransitionOutcome::Applied) => {
                    self.open_chooser = false;
                    self.after_transition_applied();
                }
                Ok(TransitionOutcome::DecisionRequired) => {
                    self.open_chooser = false;
                    self.clear_feedback();
                }
                Ok(TransitionOutcome::Cancelled) => self.clear_feedback(),
                Err(error) => self.set_error(error),
            },
            DialogIntent::OpenSaveAs => {
                self.save_as_open = true;
                self.save_as_name = self.session.current_name().unwrap_or_default().to_owned();
            }
            DialogIntent::HideChooser => self.open_chooser = false,
            DialogIntent::CancelSaveAs => self.save_as_open = false,
            DialogIntent::Save => self.save_current(),
            DialogIntent::SaveAs(name) => self.save_as(&name),
            DialogIntent::Decision(decision) => self.decide_pending(decision),
        }
    }

    fn load_saved_documents(&mut self) {
        match self.session.list_saved_documents() {
            Ok(documents) => {
                self.saved_documents = documents;
                self.error = None;
                self.conflict_document = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn save_current(&mut self) {
        match self.session.save() {
            Ok(()) => {
                self.clear_feedback();
                self.notice = Some("Calculation Pad saved.".into());
            }
            Err(SessionError::SaveAsRequired) => {
                self.save_as_open = true;
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn save_as(&mut self, name: &str) {
        match self.session.save_as(name) {
            Ok(()) => {
                self.save_as_open = false;
                self.save_as_name.clear();
                self.clear_feedback();
                self.notice = Some("Calculation Pad saved as a new document.".into());
                if self.session.pending_transition().is_some() {
                    self.decide_pending(TransitionDecision::Save);
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn decide_pending(&mut self, decision: TransitionDecision) {
        match self.session.resolve_pending(decision) {
            Ok(TransitionOutcome::Applied) => self.after_transition_applied(),
            Ok(TransitionOutcome::DecisionRequired) => self.clear_feedback(),
            Ok(TransitionOutcome::Cancelled) => {
                self.open = true;
                self.focus_source = true;
                self.clear_feedback();
            }
            Err(SessionError::SaveAsRequired) if decision == TransitionDecision::Save => {
                self.save_as_open = true;
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    fn after_transition_applied(&mut self) {
        self.open = self.session.source().is_some();
        self.focus_source = self.open;
        self.open_chooser = false;
        self.save_as_open = false;
        self.clear_feedback();
    }

    fn clear_feedback(&mut self) {
        self.error = None;
        self.conflict_document = None;
        self.notice = None;
    }

    fn reset_transient_ui(&mut self) {
        self.open_chooser = false;
        self.saved_documents.clear();
        self.save_as_open = false;
        self.save_as_name.clear();
        self.clear_feedback();
    }

    fn set_error(&mut self, error: SessionError) {
        self.conflict_document = match &error {
            SessionError::Store(PadStoreError::DiskConflict(id)) => Some(*id),
            _ => None,
        };
        self.error = Some(error.to_string());
        self.notice = None;
    }

    fn clear_text_edit_undoer(&self, ctx: &egui::Context, id: egui::Id) {
        if let Some(mut state) = egui::widgets::text_edit::TextEditState::load(ctx, id) {
            state.clear_undoer();
            state.store(ctx, id);
        }
    }

    fn take_editor_history_shortcut(&self, ui: &mut egui::Ui, id: egui::Id) -> Option<bool> {
        if !ui.memory(|memory| memory.has_focus(id)) {
            return None;
        }
        ui.input_mut(|input| {
            if input.consume_key(
                egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                egui::Key::Z,
            ) {
                Some(false)
            } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y) {
                Some(false)
            } else {
                input
                    .consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
                    .then_some(true)
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn test_replace_source(&mut self, source: &str) {
        self.session.replace_source(source).unwrap();
    }

    #[cfg(test)]
    pub(crate) fn test_resolve_pending(&mut self, decision: TransitionDecision) {
        self.decide_pending(decision);
    }

    #[cfg(test)]
    pub(crate) fn test_source(&self) -> Option<&str> {
        self.session.source()
    }

    #[cfg(test)]
    pub(crate) fn test_pending_transition(&self) -> Option<PendingTransition> {
        self.session.pending_transition()
    }

    #[cfg(test)]
    pub(crate) fn test_was_rendered(&self) -> bool {
        self.test_window_rect.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_editor_has_focus(&self, ctx: &egui::Context) -> bool {
        self.test_editor_id
            .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
    }

    #[cfg(test)]
    pub(crate) fn test_focus_source_pending(&self) -> bool {
        self.focus_source
    }

    #[cfg(test)]
    pub(crate) fn test_can_undo(&self) -> bool {
        self.session.can_undo()
    }

    #[cfg(test)]
    fn test_apply_save_as(&mut self, name: &str) {
        self.save_as(name);
    }

    #[cfg(test)]
    fn test_open_document(&mut self, id: DocumentId) {
        self.apply_intent(DialogIntent::Open(id));
    }
}

fn copy_all_results(evaluation: &crate::calculation_pad::SessionEvaluation) -> String {
    let mut output = String::new();
    if let Some(diagnostic) = &evaluation.diagnostic {
        output.push_str(&format!("Document error: {diagnostic}\n"));
    }
    for line in &evaluation.lines {
        let number = line.line_index + 1;
        let name = line
            .assignment_name
            .as_deref()
            .map(|name| format!(" {name}"))
            .unwrap_or_default();
        match &line.outcome {
            LineOutcome::Blank => output.push_str(&format!("r{number}{name}: (blank)\n")),
            LineOutcome::Comment => output.push_str(&format!("r{number}{name}: (comment)\n")),
            LineOutcome::Value(value) => output.push_str(&format!(
                "r{number}{name} = {}\n",
                value
                    .formatted_result()
                    .unwrap_or_else(|| "unavailable".into())
            )),
            LineOutcome::Error(error) => {
                output.push_str(&format!("r{number}{name}: error: {error}\n"));
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{CalculationPadDialog, CalculationPadDialogEffect, DialogIntent, copy_all_results};
    use crate::calculation_pad::{
        CalculationPadSession, CalculationValue, LineOutcome, PadDocumentStore, PendingTransition,
        TransitionDecision,
    };
    use crate::platform::app_data::AppDataRoot;
    use eframe::egui;
    use std::path::Path;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, AppDataRoot, CalculationPadDialog) {
        let directory = tempfile::tempdir().expect("create isolated pad data root");
        let root = AppDataRoot::from_path(directory.path().to_path_buf());
        let dialog = CalculationPadDialog::new(&root);
        (directory, root, dialog)
    }

    fn input(screen_size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, screen_size)),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<CalculationPadDialogEffect> {
        let mut effect = None;
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            effect = dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0);
        });
        output.drop_without_applying_deltas();
        effect
    }

    fn frame_requesting_discard(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (Option<CalculationPadDialogEffect>, bool) {
        let mut effect = None;
        let mut first_ui_pass_seen = false;
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            if first_ui_pass {
                first_ui_pass_seen = true;
                ui.ctx()
                    .request_discard("Calculation Pad discarded-pass regression");
            }
            effect = dialog.show(ui.ctx(), first_ui_pass);
        });
        output.drop_without_applying_deltas();
        (effect, first_ui_pass_seen)
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn click_events(point: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn click(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        size: egui::Vec2,
        point: egui::Pos2,
    ) -> Option<CalculationPadDialogEffect> {
        frame(dialog, ctx, size, click_events(point, true));
        frame(dialog, ctx, size, click_events(point, false))
    }

    fn value(session: &mut CalculationPadSession, line: usize) -> f64 {
        match &session.evaluation().unwrap().lines[line].outcome {
            LineOutcome::Value(CalculationValue::Scalar(value)) => *value,
            other => panic!("expected scalar result, got {other:?}"),
        }
    }

    #[test]
    fn blank_save_opens_save_as_without_reporting_a_saved_document() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();

        dialog.apply_intent(DialogIntent::Save);

        assert!(dialog.save_as_open);
        assert_eq!(dialog.notice, None);
        assert!(dialog.session.list_saved_documents().unwrap().is_empty());
    }

    #[test]
    fn discarded_first_ui_pass_does_not_apply_editor_text_or_consume_focus() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("initial = 1");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        dialog.open();

        let (effect, saw_first_ui_pass) = frame_requesting_discard(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("discarded edit".into()),
            ],
        );
        assert!(saw_first_ui_pass, "the regression requested a real discard");
        assert_eq!(effect, None);
        assert_eq!(dialog.test_source(), Some("initial = 1"));
        assert!(
            dialog.focus_source,
            "discarded pass must retain pending focus"
        );

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        assert!(ctx.memory(|memory| memory.has_focus(editor_id)));
        assert_eq!(dialog.test_source(), Some("initial = 1"));
    }

    #[test]
    fn editor_updates_dependencies_errors_and_session_owned_keyboard_undo_redo() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        dialog.open();
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![egui::Event::Text("amount = 2\namount * 3".into())],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount * 3".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![key(egui::Key::Z, egui::Modifiers::COMMAND)],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        frame(
            &mut dialog,
            &ctx,
            size,
            vec![key(egui::Key::Y, egui::Modifiers::COMMAND)],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount / 0".into()),
            ],
        );
        assert!(matches!(
            dialog.session.evaluation().unwrap().lines[1].outcome,
            LineOutcome::Error(_)
        ));
    }

    #[test]
    fn keyboard_undo_survives_modifier_release_later_in_the_same_frame() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        dialog.test_replace_source("amount = 5\namount * 3");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog.test_editor_id.expect("editor is a real TextEdit");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::Z, egui::Modifiers::COMMAND),
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 6.0);
    }

    #[test]
    fn explicit_copy_buttons_return_single_result_and_source_order_summary() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("total = 2 + 3\nr1 * 4\n1 / 0");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        frame(&mut dialog, &ctx, size, Vec::new());
        for _ in 0..2 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let line_copy = dialog
            .test_line_copy_rects
            .iter()
            .find(|(line, _)| *line == 1)
            .map(|(_, rect)| *rect)
            .expect("a result row exposes an explicit copy button");
        assert_eq!(
            click(&mut dialog, &ctx, size, line_copy.center()),
            Some(CalculationPadDialogEffect::CopyText("20".into()))
        );

        let all_copy = dialog
            .test_copy_all_rect
            .expect("results toolbar exposes explicit Copy all");
        let Some(CalculationPadDialogEffect::CopyText(summary)) =
            click(&mut dialog, &ctx, size, all_copy.center())
        else {
            panic!("Copy all returns a clipboard effect only after an explicit click");
        };
        assert!(summary.find("r1 total = 5").unwrap() < summary.find("r2 = 20").unwrap());
        assert!(summary.contains("r3: error:"));
        assert_eq!(dialog.test_source(), Some("total = 2 + 3\nr1 * 4\n1 / 0"));
    }

    #[test]
    fn saved_document_reopens_and_dirty_close_write_conflict_keeps_the_draft() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 4\namount * 2");
        dialog.test_apply_save_as("Estimate");
        assert!(!dialog.session.is_dirty());
        let saved_id = dialog.session.current_id().unwrap();
        dialog.request_close();
        assert!(!dialog.open);

        dialog.open();
        dialog.test_open_document(saved_id);
        assert_eq!(dialog.test_source(), Some("amount = 4\namount * 2"));
        assert_eq!(dialog.session.current_name(), Some("Estimate"));

        dialog.test_replace_source("amount = 9\namount * 2");
        let directory = PadDocumentStore::new(&root).directory().to_path_buf();
        let path = directory.join(format!("{:032x}.json", saved_id.as_u128()));
        let mut external = std::fs::read(&path).unwrap();
        external.push(b' ');
        std::fs::write(path, external).unwrap();

        dialog.request_close();
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        dialog.test_resolve_pending(TransitionDecision::Save);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|text| text.contains("changed"))
        );

        dialog.test_resolve_pending(TransitionDecision::Cancel);
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(dialog.open);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
    }

    #[test]
    fn clicking_discard_in_pending_close_closes_the_rendered_dialog() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("draft = 7");
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        dialog.request_close();
        assert!(dialog.open);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        for _ in 0..2 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let discard = dialog
            .test_discard_rect
            .expect("pending close renders an actual Discard button");

        assert_eq!(click(&mut dialog, &ctx, size, discard.center()), None);
        assert!(!dialog.open, "Discard applies the pending Close transition");
        assert_eq!(dialog.test_source(), None);
        assert_eq!(dialog.test_pending_transition(), None);

        assert_eq!(frame(&mut dialog, &ctx, size, Vec::new()), None);
        assert!(
            !dialog.test_was_rendered(),
            "the next UI frame does not resurrect the closed Calculation Pad"
        );
    }

    #[test]
    fn constrained_window_keeps_editor_and_result_controls_inside_the_viewport() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("answer = 42\nanswer");
        let ctx = egui::Context::default();
        let size = egui::vec2(500.0, 340.0);
        for _ in 0..4 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let window = dialog
            .test_window_rect
            .expect("dialog has real window geometry");
        let editor = dialog.test_editor_rect.expect("source TextEdit is visible");
        let copy = dialog.test_copy_all_rect.expect("copy remains reachable");
        assert!(screen.contains_rect(window));
        assert!(editor.is_positive());
        assert!(copy.is_positive());
        assert!(screen.contains(editor.center()));
        assert!(screen.contains(copy.center()));
        assert!(window.intersects(editor));
        assert!(window.intersects(copy));
    }

    #[test]
    fn all_results_summary_preserves_physical_line_numbers_and_comments() {
        let mut evaluation = crate::calculation_pad::CalculationPadSession::with_store(
            PadDocumentStore::new(&AppDataRoot::from_path(Path::new("unused-root"))),
        );
        evaluation.replace_source("# note\n3\nr2 + 1").unwrap();
        let summary = copy_all_results(evaluation.evaluation().unwrap());
        assert!(summary.contains("r1: (comment)"));
        assert!(summary.contains("r2 = 3"));
        assert!(summary.contains("r3 = 4"));
    }
}
