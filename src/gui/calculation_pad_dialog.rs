use super::utility_host::{
    HostIdentity as PadHostIdentity, UtilityHostState, UtilityPresentation as PadHost,
    ViewportPassProvenance,
};
use crate::calculation_pad::{
    CalculationPadSession, DocumentId, DocumentIncarnation, EXAMPLE_GROUPS,
    InspectorSourceSnapshot, LineId, LineOutcome, PadDocumentSummary, PadRecoverySnapshot,
    PadRecoveryStore, PadRecoveryWriteFailure, PadRecoveryWriter, PadStoreError, PendingTransition,
    SessionError, TransitionDecision, TransitionOutcome,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::platform::app_data::AppDataRoot;
use eframe::egui;

const PAD_VIEWPORT_SIZE: [f32; 2] = [1080.0, 760.0];
const MIN_PAD_VIEWPORT_SIZE: [f32; 2] = [500.0, 380.0];

fn pad_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("calculation_pad_native_viewport")
}

fn calculation_pad_tool_key() -> crate::settings::UtilityToolKey {
    crate::settings::UtilityToolKey::parse("calculation-pad")
        .expect("Calculation Pad is a registered utility window key")
}

fn pad_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Calculation Pad")
        .with_inner_size(PAD_VIEWPORT_SIZE)
        .with_min_inner_size(MIN_PAD_VIEWPORT_SIZE)
        .with_resizable(true)
        .with_visible(true)
}

pub(crate) struct CalculationPadDialog {
    pub(crate) open: bool,
    focus_source: bool,
    session: CalculationPadSession,
    inspector_selection: Option<InspectorSelection>,
    inspector_caret: Option<InspectorCaret>,
    inspector_selection_generation: u64,
    inspector_caret_generation: u64,
    inspector_context: std::sync::Arc<InspectorPadContext>,
    inspector_context_key: Option<(DocumentIncarnation, DocumentId, u64, Option<LineId>, u64)>,
    recovery_writer: PadRecoveryWriter,
    remember_source: bool,
    recovery_attempted: bool,
    last_recovery_identity: Option<(DocumentId, u64)>,
    host_state: UtilityHostState,
    deferred_source_focus: Option<(egui::ViewportId, egui::Id)>,
    open_chooser: bool,
    saved_documents: Vec<PadDocumentSummary>,
    examples_open: bool,
    save_as_open: bool,
    save_as_name: String,
    error: Option<String>,
    conflict_document: Option<DocumentId>,
    notice: Option<String>,
    pending_root_effects: Option<RootEffects>,
    host_close_requested: bool,
    #[cfg(test)]
    test_editor_id: Option<egui::Id>,
    #[cfg(test)]
    test_editor_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_copy_all_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_save_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_line_copy_rects: Vec<(usize, egui::Rect)>,
    #[cfg(test)]
    test_inspect_rects: Vec<(usize, egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_inspector_reopen: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_discard_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_keep_editing_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_window_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_examples_copy_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_host_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    test_callback_viewport_id: Option<egui::ViewportId>,
    #[cfg(test)]
    test_child_discard_requested: bool,
    #[cfg(test)]
    test_request_child_discard: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CalculationPadDialogEffect {
    CopyText(String),
    OpenInspector,
    ShowRoot,
    SaveRecoveryPreference(bool),
}

struct RootEffects {
    pass: ViewportPassProvenance,
    root_effect: Option<CalculationPadDialogEffect>,
    close_examples_requested: bool,
    host: Option<HostEffects>,
}

struct HostEffects {
    identity: PadHostIdentity,
    viewport_id: egui::ViewportId,
    viewport_class: egui::ViewportClass,
    viewport_info: Option<egui::ViewportInfo>,
    pass: Option<ViewportPassProvenance>,
    pass_accepted: bool,
    edited_source: Option<String>,
    document_basis: Option<(DocumentIncarnation, DocumentId, u64)>,
    inspector_selection: Option<LineId>,
    inspector_caret: Option<std::ops::Range<usize>>,
    open_inspector: bool,
    save_as_name: Option<String>,
    source_focus_requested: bool,
    source_editor_id: Option<egui::Id>,
    editor_history_action: Option<bool>,
    intent: Option<DialogIntent>,
    host_request: Option<PadHost>,
    result_copy: Option<ResultCopyIntent>,
    close_requested: bool,
    native_close_requested: bool,
    focus_viewport: bool,
    embedded_root_layer: Option<egui::LayerId>,
    embedded_root_focused_id: Option<egui::Id>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InspectorSelection {
    incarnation: DocumentIncarnation,
    document: DocumentId,
    line: LineId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InspectorCaret {
    pub(super) incarnation: DocumentIncarnation,
    pub(super) document: DocumentId,
    pub(super) revision: u64,
    pub(super) range: std::ops::Range<usize>,
    pub(super) generation: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InspectorPadContext {
    pub(super) candidate: Option<InspectorSourceSnapshot>,
    pub(super) caret: Option<InspectorCaret>,
    pub(super) selection_generation: u64,
    pub(super) editable: bool,
}

#[derive(Clone, Copy)]
enum ResultCopyTarget {
    All,
    Line(LineId),
}

#[derive(Clone, Copy)]
struct ResultCopyIntent {
    host: PadHostIdentity,
    viewport_id: egui::ViewportId,
    document_id: DocumentId,
    target: ResultCopyTarget,
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
    OpenExamples,
    Decision(TransitionDecision),
    SetRememberSource(bool),
}

impl CalculationPadDialog {
    fn examples_window_id() -> egui::Id {
        egui::Id::new("calculation-pad-examples-window")
    }

    fn root_host_layer_id() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("calculation-pad-window"))
    }

    fn host_identity(&self) -> PadHostIdentity {
        self.host_state.identity()
    }

    fn empty_host_effects(&self, viewport_id: egui::ViewportId) -> HostEffects {
        HostEffects {
            identity: self.host_identity(),
            viewport_id,
            viewport_class: egui::ViewportClass::Root,
            viewport_info: None,
            pass: None,
            pass_accepted: false,
            edited_source: None,
            document_basis: self
                .session
                .document_incarnation()
                .zip(self.session.current_id())
                .zip(self.session.revision())
                .map(|((incarnation, document), revision)| (incarnation, document, revision)),
            inspector_selection: None,
            inspector_caret: None,
            open_inspector: false,
            save_as_name: None,
            source_focus_requested: false,
            source_editor_id: None,
            editor_history_action: None,
            intent: None,
            host_request: None,
            result_copy: None,
            close_requested: false,
            native_close_requested: false,
            focus_viewport: false,
            embedded_root_layer: None,
            embedded_root_focused_id: None,
        }
    }

    pub(super) fn host_layer_if_root(&self) -> Option<egui::LayerId> {
        if !self.open {
            return None;
        }
        match self.host_state.presentation() {
            PadHost::Docked => Some(Self::root_host_layer_id()),
            PadHost::Popout => self.host_state.embedded_root_layer(),
        }
    }

    pub(super) fn embedded_host_owns_root_input(&self, ctx: &egui::Context) -> bool {
        if !self.open
            || self.host_state.presentation() != PadHost::Popout
            || ctx.viewport_id() != egui::ViewportId::ROOT
        {
            return false;
        }
        let Some(focused_id) = self.host_state.embedded_root_focused_id() else {
            return false;
        };
        let Some(response) = ctx.read_response(focused_id) else {
            return false;
        };
        let Some(layer) = self.host_state.embedded_root_layer() else {
            return false;
        };
        if response.layer_id != layer
            || !ctx.memory(|memory| {
                memory.focused() == Some(focused_id)
                    && memory.layer_ids().any(|memory_layer| memory_layer == layer)
            })
        {
            return false;
        }
        ctx.input(|input| input.focused)
            && !ctx.input(|input| {
                (input.pointer.any_pressed() || input.pointer.any_released())
                    && input.pointer.interact_pos().is_some_and(|position| {
                        ctx.layer_id_at(position)
                            .is_some_and(|hit_layer| hit_layer != layer)
                    })
            })
    }

    pub(super) fn examples_layer_if_root(&self) -> Option<egui::LayerId> {
        (self.open && self.examples_open)
            .then(|| egui::LayerId::new(egui::Order::Middle, Self::examples_window_id()))
    }

    pub(crate) fn new(root: &AppDataRoot) -> Self {
        Self {
            open: false,
            focus_source: false,
            session: CalculationPadSession::new(root),
            inspector_selection: None,
            inspector_caret: None,
            inspector_selection_generation: 0,
            inspector_caret_generation: 0,
            inspector_context: std::sync::Arc::new(InspectorPadContext::default()),
            inspector_context_key: None,
            recovery_writer: PadRecoveryWriter::new(PadRecoveryStore::new(root)),
            remember_source: false,
            recovery_attempted: false,
            last_recovery_identity: None,
            host_state: UtilityHostState::docked(),
            deferred_source_focus: None,
            open_chooser: false,
            saved_documents: Vec::new(),
            examples_open: false,
            save_as_open: false,
            save_as_name: String::new(),
            error: None,
            conflict_document: None,
            notice: None,
            pending_root_effects: None,
            host_close_requested: false,
            #[cfg(test)]
            test_editor_id: None,
            #[cfg(test)]
            test_editor_rect: None,
            #[cfg(test)]
            test_copy_all_rect: None,
            #[cfg(test)]
            test_save_rect: None,
            #[cfg(test)]
            test_line_copy_rects: Vec::new(),
            #[cfg(test)]
            test_inspect_rects: Vec::new(),
            #[cfg(test)]
            test_inspector_reopen: None,
            #[cfg(test)]
            test_window_rect: None,
            #[cfg(test)]
            test_discard_rect: None,
            #[cfg(test)]
            test_keep_editing_rect: None,
            #[cfg(test)]
            test_examples_button_rect: None,
            #[cfg(test)]
            test_examples_window_rect: None,
            #[cfg(test)]
            test_examples_copy_rect: None,
            #[cfg(test)]
            test_host_button_rect: None,
            #[cfg(test)]
            test_callback_viewport_id: None,
            #[cfg(test)]
            test_child_discard_requested: false,
            #[cfg(test)]
            test_request_child_discard: false,
        }
    }

    pub(crate) fn open(&mut self) {
        if !self.open {
            self.reset_host_to_docked();
        }
        self.ensure_session_open();
        self.restore_recovery_on_explicit_open();
        self.open = true;
        self.host_close_requested = false;
        self.request_source_focus();
    }

    pub(crate) fn ensure_open(&mut self) {
        let was_open = self.open;
        if !was_open {
            self.reset_host_to_docked();
        }
        self.ensure_session_open();
        self.open = true;
        if !was_open {
            self.request_source_focus();
        }
    }

    fn request_source_focus(&mut self) {
        self.focus_source = true;
        self.host_state.request_focus();
    }

    fn reset_host_to_docked(&mut self) {
        self.host_state.retire_to_docked();
        self.deferred_source_focus = None;
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.host_state.bootstrap_pending()
            || self.examples_open
            || (self.open && self.host_state.presentation() == PadHost::Popout)
    }

    fn ensure_session_open(&mut self) {
        if self.session.source().is_none() && self.session.pending_transition().is_none() {
            match self.session.request_new() {
                Ok(_) => {}
                Err(error) => self.set_error(error),
            }
        }
    }

    pub(crate) fn initialize_recovery_preference(&mut self, enabled: bool) {
        self.remember_source = enabled;
    }

    pub(crate) fn apply_committed_recovery_preference(
        &mut self,
        enabled: bool,
    ) -> anyhow::Result<()> {
        if self.remember_source == enabled {
            return Ok(());
        }
        self.remember_source = enabled;
        if enabled {
            if self.open {
                self.save_recovery_now(None)
            } else {
                Ok(())
            }
        } else {
            self.recovery_attempted = false;
            let result = self.recovery_writer.clear_now();
            if result.is_ok() {
                self.last_recovery_identity = None;
            }
            result
        }
    }

    fn restore_recovery_on_explicit_open(&mut self) {
        if !self.remember_source || self.recovery_attempted {
            return;
        }
        self.recovery_attempted = true;
        match self.recovery_writer.load() {
            Ok(Some(snapshot)) => {
                if let Err(error) = self.session.restore_recovered_source(snapshot.source) {
                    self.set_recovery_error(error.to_string());
                }
            }
            Ok(None) => {}
            Err(error) => self.set_recovery_error(error.to_string()),
        }
    }

    fn recovery_snapshot(&self) -> Option<PadRecoverySnapshot> {
        Some(PadRecoverySnapshot {
            document_id: self.session.current_id()?,
            revision: self.session.revision()?,
            source: self.session.source()?.to_owned(),
        })
    }

    fn schedule_recovery_snapshot(&mut self, ctx: &egui::Context) {
        if !self.remember_source {
            return;
        }
        let Some(snapshot) = self.recovery_snapshot() else {
            return;
        };
        let identity = (snapshot.document_id, snapshot.revision);
        if let Err(error) = self.recovery_writer.schedule(snapshot) {
            self.set_recovery_error(error.to_string());
        } else {
            self.last_recovery_identity = Some(identity);
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }
    }

    fn save_recovery_now(&mut self, snapshot: Option<PadRecoverySnapshot>) -> anyhow::Result<()> {
        if !self.remember_source {
            return Ok(());
        }
        let Some(snapshot) = snapshot.or_else(|| self.recovery_snapshot()) else {
            return Ok(());
        };
        let identity = (snapshot.document_id, snapshot.revision);
        self.recovery_writer.save_now(snapshot)?;
        self.last_recovery_identity = Some(identity);
        Ok(())
    }

    fn report_async_recovery_errors(&mut self) {
        let failures = self.recovery_writer.take_async_errors();
        self.publish_recovery_failures(failures);
    }

    fn recovery_error_identity(&self) -> Option<(DocumentId, u64)> {
        self.session
            .current_id()
            .zip(self.session.revision())
            .or(self.last_recovery_identity)
    }

    fn publish_recovery_failures(&mut self, failures: Vec<PadRecoveryWriteFailure>) {
        let current_identity = self
            .remember_source
            .then(|| self.recovery_error_identity())
            .flatten();
        let messages = failures
            .into_iter()
            .filter(|failure| current_identity == Some((failure.document_id, failure.revision)))
            .map(|failure| failure.message)
            .collect::<Vec<_>>();
        if !messages.is_empty() {
            self.set_recovery_error(messages.join("; "));
        }
    }

    fn set_recovery_error(&mut self, message: String) {
        self.error = Some(format!("Calculation Pad recovery failed: {message}"));
        self.notice = None;
    }

    pub(crate) fn shutdown_recovery(&mut self) -> anyhow::Result<()> {
        let mut failures = self.recovery_writer.take_async_errors();
        let mut direct_errors = Vec::new();
        if self.remember_source && self.open {
            match self.recovery_snapshot() {
                Some(snapshot) => {
                    if let Err(error) = self.save_recovery_now(Some(snapshot)) {
                        direct_errors.push(error.to_string());
                    }
                }
                None => direct_errors.push("open Calculation Pad has no recoverable source".into()),
            }
        }
        let flush_result = self.recovery_writer.flush();
        let shutdown_result = self.recovery_writer.shutdown();
        match flush_result {
            Ok(mut drained) => failures.append(&mut drained),
            Err(error) => direct_errors.push(error.to_string()),
        }
        match shutdown_result {
            Ok(mut drained) => failures.append(&mut drained),
            Err(error) => direct_errors.push(error.to_string()),
        }
        let current_identity = self
            .remember_source
            .then(|| self.recovery_error_identity())
            .flatten();
        direct_errors.extend(
            failures
                .into_iter()
                .filter(|failure| current_identity == Some((failure.document_id, failure.revision)))
                .map(|failure| failure.message),
        );
        if direct_errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(direct_errors.join("; ")))
        }
    }

    pub(crate) fn request_close(&mut self) {
        if !self.open {
            return;
        }
        if self.session.pending_transition().is_some() {
            self.open = true;
            return;
        }
        self.host_close_requested = true;
    }

    fn apply_close_request(&mut self) {
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
                    self.request_source_focus();
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
        application_input_allowed: bool,
        root_input_allowed: bool,
    ) {
        #[cfg(test)]
        {
            self.test_window_rect = None;
            self.test_discard_rect = None;
            self.test_keep_editing_rect = None;
            self.test_save_rect = None;
            self.test_editor_id = None;
            self.test_editor_rect = None;
            self.test_examples_button_rect = None;
            self.test_examples_window_rect = None;
            self.test_examples_copy_rect = None;
            self.test_host_button_rect = None;
            self.test_callback_viewport_id = None;
            self.test_child_discard_requested = false;
        }
        if !self.open {
            return;
        }

        let root_pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut host_effects = None;
        match self.host_state.presentation() {
            PadHost::Docked => {
                let identity = self.host_identity();
                let mut effects = self.empty_host_effects(egui::ViewportId::ROOT);
                let mut window_open = self.open;
                let mut body_authorized = false;
                let content_rect = ctx.content_rect();
                let min_size = egui::vec2(420.0, 300.0).min(content_rect.size());
                let output = egui::Window::new("Calculation Pad")
                    .id(Self::root_host_layer_id().id)
                    .open(&mut window_open)
                    .resizable(true)
                    .default_size(egui::vec2(900.0, 650.0))
                    .min_size(min_size)
                    .max_size(content_rect.size())
                    .show_utility(
                        ctx,
                        "calculation-pad",
                        Self::root_host_layer_id().id,
                        None,
                        crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::outer(
                            egui::vec2(900.0, 650.0),
                        )
                        .with_minimum(min_size)
                        .with_maximum(content_rect.size()),
                        |ui| {
                            let pass_authorized = first_ui_pass
                                && !ctx.will_discard()
                                && ui.is_visible()
                                && !ui.is_sizing_pass();
                            let modal_open =
                                ctx.memory(|memory| memory.top_modal_layer().is_some());
                            let surface_allowed = root_input_allowed && !modal_open;
                            body_authorized = pass_authorized && surface_allowed;
                            self.draw_host_contents(
                                ui,
                                ctx,
                                identity,
                                egui::ViewportId::ROOT,
                                body_authorized,
                                body_authorized,
                                &mut effects,
                            );
                        },
                    );

                #[cfg(test)]
                {
                    self.test_window_rect = output.as_ref().map(|output| output.response.rect);
                }
                let lifecycle_authorized = first_ui_pass
                    && !ctx.will_discard()
                    && output
                        .as_ref()
                        .is_some_and(|output| output.response.rect.intersects(ctx.content_rect()));
                effects.close_requested = lifecycle_authorized
                    && self.session.pending_transition().is_none()
                    && (self.host_close_requested || !window_open);
                effects.pass_accepted = lifecycle_authorized;
                effects.pass = lifecycle_authorized.then_some(root_pass);
                if body_authorized || effects.close_requested {
                    host_effects = Some(effects);
                }
            }
            PadHost::Popout => {
                let identity = self.host_identity();
                let mut callback_effects = None;
                ctx.show_viewport_immediate(
                    pad_viewport_id(),
                    pad_viewport_builder(),
                    |child, class| {
                        let child_ctx = child.ctx().clone();
                        let viewport_id = child_ctx.viewport_id();
                        let independent = class == egui::ViewportClass::Immediate
                            && viewport_id == pad_viewport_id();
                        let current_pass = child.current_pass_index() == 0;
                        let modal_open = child.memory(|memory| memory.top_modal_layer().is_some());
                        let (focused, native_close_requested) = child.input(|input| {
                            (
                                input.focused,
                                independent && input.viewport().close_requested(),
                            )
                        });
                        let pass_authorized = first_ui_pass
                            && current_pass
                            && !ctx.will_discard()
                            && !child.will_discard();
                        let surface_allowed = if independent {
                            application_input_allowed
                        } else {
                            root_input_allowed && viewport_id == egui::ViewportId::ROOT
                        };
                        let source_focus_allowed =
                            pass_authorized && surface_allowed && !modal_open;
                        let actions_allowed = source_focus_allowed
                            && (!independent || focused)
                            && !native_close_requested;
                        let popup_open = egui::Popup::is_any_open(&child_ctx);
                        let keyboard_close_requested = if actions_allowed && !popup_open {
                            child.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                            })
                        } else {
                            false
                        };

                        let mut effects = self.empty_host_effects(viewport_id);
                        effects.viewport_class = class;
                        if independent {
                            effects.viewport_info =
                                Some(child.input(|input| input.viewport().clone()));
                        }
                        effects.native_close_requested = native_close_requested;
                        effects.close_requested = pass_authorized
                            && self.session.pending_transition().is_none()
                            && (native_close_requested
                                || keyboard_close_requested
                                || self.host_close_requested);
                        effects.pass_accepted =
                            pass_authorized && !ctx.will_discard() && !child.will_discard();
                        effects.pass = effects
                            .pass_accepted
                            .then_some(ViewportPassProvenance::current(&child_ctx, viewport_id));
                        effects.focus_viewport = independent
                            && self.host_state.focus_pending()
                            && pass_authorized
                            && application_input_allowed
                            && !modal_open;

                        let mut body_rendered = false;
                        if independent {
                            let _output = egui::CentralPanel::default().show(child, |ui| {
                                body_rendered = ui.is_visible() && !ui.is_sizing_pass();
                                self.draw_host_contents(
                                    ui,
                                    &child_ctx,
                                    identity,
                                    viewport_id,
                                    actions_allowed && body_rendered,
                                    source_focus_allowed && body_rendered,
                                    &mut effects,
                                );
                            });
                            #[cfg(test)]
                            {
                                self.test_window_rect = Some(_output.response.rect);
                            }
                        } else {
                            body_rendered = child.is_visible() && !child.is_sizing_pass();
                            self.draw_host_contents(
                                child,
                                &child_ctx,
                                identity,
                                viewport_id,
                                actions_allowed && body_rendered,
                                source_focus_allowed && body_rendered,
                                &mut effects,
                            );
                            if viewport_id == egui::ViewportId::ROOT {
                                let layer = child.layer_id();
                                effects.embedded_root_layer = Some(layer);
                                let pointer_position =
                                    child.input(|input| input.pointer.interact_pos());
                                let pointer_active = child.input(|input| {
                                    input.pointer.any_pressed() || input.pointer.any_released()
                                });
                                let pointer_over_host = pointer_position.is_some_and(|position| {
                                    child_ctx.layer_id_at(position) == Some(layer)
                                });
                                let focused_id = child.memory(|memory| memory.focused());
                                effects.embedded_root_focused_id =
                                    if pointer_active && pointer_over_host {
                                        focused_id.or(effects.source_editor_id)
                                    } else if pointer_active {
                                        None
                                    } else {
                                        self.host_state
                                            .embedded_root_focused_id()
                                            .filter(|id| focused_id == Some(*id))
                                    };
                            }
                        }

                        if body_rendered
                            && pass_authorized
                            && surface_allowed
                            && !modal_open
                            && !effects.close_requested
                            && let Some((target_viewport, editor_id)) = self.deferred_source_focus
                            && target_viewport == viewport_id
                        {
                            self.deferred_source_focus = None;
                            child.memory_mut(|memory| memory.request_focus(editor_id));
                            if effects.embedded_root_layer.is_some() {
                                effects.embedded_root_focused_id = Some(editor_id);
                            }
                        }

                        #[cfg(test)]
                        if std::mem::take(&mut self.test_request_child_discard) {
                            child.request_discard("Calculation Pad test child pass discard");
                            self.test_child_discard_requested = true;
                        }
                        effects.pass_accepted =
                            pass_authorized && !ctx.will_discard() && !child.will_discard();
                        if !effects.pass_accepted {
                            effects.pass = None;
                            effects.embedded_root_layer = None;
                            effects.embedded_root_focused_id = None;
                        }
                        #[cfg(test)]
                        {
                            self.test_callback_viewport_id = Some(viewport_id);
                        }
                        callback_effects = Some(effects);
                    },
                );
                host_effects = callback_effects;
            }
        }

        let mut examples_window_open = self.examples_open;
        let mut examples_authorized = false;
        let mut root_effect = None;
        let examples_output = if self.examples_open {
            egui::Window::new("Calculation Pad Examples & Reference")
                .id(Self::examples_window_id())
                .order(egui::Order::Middle)
                .open(&mut examples_window_open)
                .resizable(true)
                .default_size(egui::vec2(620.0, 540.0))
                .min_size(egui::vec2(360.0, 280.0))
                .show(ctx, |ui| {
                    examples_authorized = first_ui_pass
                        && !ctx.will_discard()
                        && ui.is_visible()
                        && !ui.is_sizing_pass();
                    ui.add_enabled_ui(examples_authorized, |ui| {
                        self.examples_ui(ui, &mut root_effect);
                    });
                })
        } else {
            None
        };

        #[cfg(test)]
        {
            self.test_examples_window_rect =
                examples_output.as_ref().map(|output| output.response.rect);
        }

        let close_examples_requested = self.examples_open
            && !examples_window_open
            && first_ui_pass
            && !ctx.will_discard()
            && examples_output
                .as_ref()
                .is_some_and(|output| output.response.rect.intersects(ctx.content_rect()));
        let has_host_frame = host_effects.is_some();
        let has_root_frame_effect = root_effect.is_some() || close_examples_requested;
        if first_ui_pass && (has_host_frame || has_root_frame_effect || examples_authorized) {
            self.pending_root_effects = Some(RootEffects {
                pass: root_pass,
                root_effect,
                close_examples_requested,
                host: host_effects,
            });
        }
    }

    fn draw_host_contents(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
        actions_allowed: bool,
        source_focus_allowed: bool,
        effects: &mut HostEffects,
    ) {
        let content_rect = ui.clip_rect();
        let editor_id = egui::Id::new("calculation-pad-source");
        effects.source_editor_id = Some(editor_id);
        if source_focus_allowed && self.focus_source {
            effects.source_focus_requested = true;
        }
        self.clear_text_edit_undoer(ctx, editor_id);
        ui.add_enabled_ui(actions_allowed, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("calculation-pad-window-content")
                .max_height(content_rect.height())
                .show(ui, |ui| {
                    if actions_allowed && self.session.pending_transition().is_none() {
                        effects.editor_history_action =
                            self.take_editor_history_shortcut(ui, editor_id);
                    }
                    let source = self.session.source().unwrap_or_default().to_owned();
                    let evaluation = self.session.evaluation().cloned();
                    let mut staged_save_as_name = self.save_as_name.clone();
                    self.contents_ui(
                        ui,
                        editor_id,
                        source,
                        evaluation.as_ref(),
                        source_focus_allowed,
                        &mut staged_save_as_name,
                        &mut effects.source_focus_requested,
                        &mut effects.edited_source,
                        &mut effects.intent,
                        &mut effects.host_request,
                        &mut effects.result_copy,
                        &mut effects.inspector_selection,
                        &mut effects.inspector_caret,
                        &mut effects.open_inspector,
                        identity,
                        viewport_id,
                    );
                    if staged_save_as_name != self.save_as_name {
                        effects.save_as_name = Some(staged_save_as_name);
                    }
                });
        });
    }

    /// Apply host state only after ROOT and the originating host pass are retained.
    /// Source edits precede document intents and result-copy resolution.
    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<CalculationPadDialogEffect> {
        let Some(effects) = self.pending_root_effects.take() else {
            return Vec::new();
        };
        let root_retained = effects.pass.was_retained(ctx);
        if !root_retained {
            if effects
                .host
                .as_ref()
                .is_some_and(|host| host.native_close_requested)
            {
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return Vec::new();
        }

        let mut delivered = Vec::new();
        if let Some(effect) = effects.root_effect {
            delivered.push(effect);
        }
        if effects.close_examples_requested {
            self.examples_open = false;
        }

        let Some(mut host) = effects.host else {
            return delivered;
        };
        let host_pass_retained = host.pass_accepted
            && host.identity == self.host_identity()
            && self.open
            && host
                .pass
                .is_some_and(|pass| pass.viewport_id == host.viewport_id && pass.was_retained(ctx));
        if !host_pass_retained {
            if host.native_close_requested {
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
            }
            return delivered;
        }

        let current_basis = self
            .session
            .document_incarnation()
            .zip(self.session.current_id())
            .zip(self.session.revision())
            .map(|((incarnation, document), revision)| (incarnation, document, revision));
        if host.document_basis != current_basis {
            host.edited_source = None;
            host.editor_history_action = None;
            host.inspector_selection = None;
            host.inspector_caret = None;
            host.open_inspector = false;
        }

        let needs_repaint = host.edited_source.is_some()
            || host.save_as_name.is_some()
            || host.source_focus_requested
            || host.editor_history_action.is_some()
            || host.intent.is_some()
            || host.host_request.is_some()
            || host.result_copy.is_some()
            || host.close_requested
            || host.focus_viewport
            || self.host_state.bootstrap_pending();
        let host_request_requested = host.host_request.is_some();
        let accepted_source_or_history =
            host.edited_source.is_some() || host.editor_history_action.is_some();
        let before_document = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        let source_was_edited = host.edited_source.is_some();

        if let Some(source) = host.edited_source.take() {
            if let Err(error) = self.session.replace_source(source) {
                self.set_error(error);
                if host.close_requested {
                    self.host_close_requested = false;
                }
                if host.native_close_requested {
                    ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
                }
                ctx.request_repaint();
                return delivered;
            }
            self.notice = None;
        }
        if let Some(name) = host.save_as_name.take() {
            self.save_as_name = name;
        }
        match host.editor_history_action {
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
        if accepted_source_or_history {
            self.schedule_recovery_snapshot(ctx);
        }

        let recovery_transition_pending = host.intent.as_ref().is_some_and(|intent| {
            matches!(
                intent,
                DialogIntent::New
                    | DialogIntent::Open(_)
                    | DialogIntent::Decision(_)
                    | DialogIntent::SaveAs(_)
            )
        });
        let before_intent_recovery = (self.remember_source && recovery_transition_pending)
            .then(|| self.recovery_snapshot())
            .flatten();
        let pending_before_intent = self.session.pending_transition();
        let mut pending_decision = None;
        let mut transition_intent = false;
        if let Some(intent) = host.intent.take() {
            transition_intent = matches!(
                &intent,
                DialogIntent::New
                    | DialogIntent::Open(_)
                    | DialogIntent::Decision(_)
                    | DialogIntent::SaveAs(_)
            );
            if let DialogIntent::Decision(decision) = &intent {
                pending_decision = Some(*decision);
            } else if matches!(&intent, DialogIntent::SaveAs(_)) {
                pending_decision = Some(TransitionDecision::Save);
            }
            let show_root = matches!(&intent, DialogIntent::OpenExamples);
            let requested_recovery_preference = match &intent {
                DialogIntent::SetRememberSource(enabled) => Some(*enabled),
                _ => None,
            };
            self.apply_intent(intent);
            if show_root && self.examples_open {
                delivered.push(CalculationPadDialogEffect::ShowRoot);
            }
            if let Some(enabled) = requested_recovery_preference
                && enabled != self.remember_source
            {
                delivered.push(CalculationPadDialogEffect::SaveRecoveryPreference(enabled));
            }
        }
        if transition_intent && self.remember_source {
            let after_intent_recovery = self.recovery_snapshot();
            if let Some(snapshot) = after_intent_recovery.clone()
                && before_intent_recovery.as_ref() != Some(&snapshot)
            {
                if let Err(error) = self.save_recovery_now(Some(snapshot)) {
                    self.set_recovery_error(error.to_string());
                }
            } else if after_intent_recovery.is_none()
                && !self.open
                && pending_before_intent == Some(PendingTransition::Close)
                && pending_decision == Some(TransitionDecision::Save)
                && let Some(snapshot) = before_intent_recovery
                && let Err(error) = self.save_recovery_now(Some(snapshot))
            {
                self.set_recovery_error(error.to_string());
            }
        }
        let after_document = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        if before_document == after_document
            && let Some((incarnation, document)) = after_document
        {
            if let Some(line) = host.inspector_selection.take()
                && self
                    .session
                    .line_ids()
                    .is_some_and(|ids| ids.contains(&line))
            {
                let selected = InspectorSelection {
                    incarnation,
                    document,
                    line,
                };
                if self.inspector_selection.as_ref() != Some(&selected) {
                    self.inspector_selection = Some(selected);
                    self.inspector_selection_generation =
                        self.inspector_selection_generation.wrapping_add(1);
                }
            }
            if host.editor_history_action.is_none()
                && let Some(range) = host.inspector_caret.take()
                && let Some(revision) = self.session.revision()
                && self
                    .session
                    .source()
                    .and_then(|source| source.get(range.clone()))
                    .is_some()
            {
                let next = InspectorCaret {
                    incarnation,
                    document,
                    revision,
                    range,
                    generation: self.inspector_caret_generation,
                };
                if self.inspector_caret.as_ref().is_none_or(|old| {
                    old.incarnation != next.incarnation
                        || old.document != next.document
                        || old.revision != next.revision
                        || old.range != next.range
                }) {
                    self.inspector_caret_generation =
                        self.inspector_caret_generation.wrapping_add(1);
                    self.inspector_caret = Some(InspectorCaret {
                        generation: self.inspector_caret_generation,
                        ..next
                    });
                }
            } else if source_was_edited || host.editor_history_action.is_some() {
                self.inspector_caret = None;
            }
        }
        self.sync_inspector_context();
        if host.open_inspector && !host.close_requested && host.host_request.is_none() {
            delivered.push(CalculationPadDialogEffect::OpenInspector);
        }
        if host.close_requested {
            self.host_close_requested = false;
            if self.open {
                let recovery_before_close = self.recovery_snapshot();
                self.apply_close_request();
                if !self.open && self.session.source().is_none() {
                    if let Err(error) = self.save_recovery_now(recovery_before_close) {
                        self.set_recovery_error(error.to_string());
                    }
                    self.recovery_attempted = false;
                }
            }
        }

        if let Some(requested_host) = host.host_request.take()
            && self.open
            && self.apply_host_request(ctx, requested_host)
            && requested_host == PadHost::Docked
        {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                ctx,
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
            );
            delivered.push(CalculationPadDialogEffect::ShowRoot);
        }

        if self.host_identity() == host.identity
            && host.identity.host == PadHost::Popout
            && !host.close_requested
            && !host.native_close_requested
            && let Some(viewport_info) = host.viewport_info.as_ref()
        {
            crate::gui::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                host.viewport_class,
                host.pass_accepted,
                "Calculation Pad",
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
                viewport_info,
                egui::vec2(PAD_VIEWPORT_SIZE[0], PAD_VIEWPORT_SIZE[1]),
                Some(egui::vec2(
                    MIN_PAD_VIEWPORT_SIZE[0],
                    MIN_PAD_VIEWPORT_SIZE[1],
                )),
                None,
            );
        }

        if host.source_focus_requested {
            if self.host_state.identity() == host.identity {
                if host.viewport_id == egui::ViewportId::ROOT {
                    if let Some(editor_id) = host.source_editor_id {
                        ctx.memory_mut(|memory| memory.request_focus(editor_id));
                    }
                } else if let Some(editor_id) = host.source_editor_id {
                    self.deferred_source_focus = Some((host.viewport_id, editor_id));
                }
                self.focus_source = false;
            } else if self.open {
                self.request_source_focus();
            }
        }

        if host.focus_viewport && self.host_state.identity() == host.identity {
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Focus);
            self.host_state.complete_focus(host.identity);
        }

        if host.identity == self.host_identity() {
            self.host_state.complete_bootstrap(
                host.identity,
                host.embedded_root_layer,
                host.embedded_root_focused_id,
            );
        }

        if let Some(copy) = host.result_copy.take()
            && !host.close_requested
            && self.open
            && copy.host == self.host_identity()
            && copy.viewport_id == host.viewport_id
        {
            let copied = self.resolve_result_copy(copy);
            if let Some(effect) = copied {
                delivered.push(effect);
            }
        }

        let close_popout = host.identity.host == PadHost::Popout && !self.open;
        if close_popout {
            crate::gui::utility_dialog_size::close_native_utility_geometry_viewport(
                ctx,
                crate::gui::utility_window_geometry::NativeViewportIdentity {
                    key: calculation_pad_tool_key(),
                    viewport_id: host.viewport_id,
                    owner_generation: 0,
                    host_generation: host.identity.generation,
                },
            );
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Close);
            self.reset_host_to_docked();
        } else if host.native_close_requested {
            ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::CancelClose);
        }

        if needs_repaint {
            ctx.request_repaint_of(host.viewport_id);
        }
        self.report_async_recovery_errors();
        if !self.open {
            self.recovery_attempted = false;
        }
        if self.host_state.bootstrap_pending() || host_request_requested {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        self.sync_inspector_context();
        delivered
    }

    fn sync_inspector_context(&mut self) {
        let identity = self
            .session
            .document_incarnation()
            .zip(self.session.current_id());
        if self.inspector_selection.as_ref().is_some_and(|selected| {
            identity != Some((selected.incarnation, selected.document))
                || !self
                    .session
                    .line_ids()
                    .is_some_and(|ids| ids.contains(&selected.line))
        }) {
            self.inspector_selection = None;
            self.inspector_selection_generation =
                self.inspector_selection_generation.wrapping_add(1);
        }
        if self.inspector_caret.as_ref().is_some_and(|caret| {
            identity != Some((caret.incarnation, caret.document))
                || self.session.revision() != Some(caret.revision)
        }) {
            self.inspector_caret = None;
            self.inspector_caret_generation = self.inspector_caret_generation.wrapping_add(1);
        }
        let key =
            identity
                .zip(self.session.revision())
                .map(|((incarnation, document), revision)| {
                    (
                        incarnation,
                        document,
                        revision,
                        self.inspector_selection
                            .as_ref()
                            .map(|selection| selection.line),
                        self.inspector_selection_generation,
                    )
                });
        let mut context = None;
        if key != self.inspector_context_key {
            let selected = self
                .inspector_selection
                .as_ref()
                .map(|selection| selection.line);
            if selected.is_some() {
                self.session.evaluation();
            }
            let candidate = selected.and_then(|line| self.session.inspector_source(line));
            context = Some(InspectorPadContext {
                candidate,
                caret: self.inspector_caret.clone(),
                selection_generation: self.inspector_selection_generation,
                editable: self.session.pending_transition().is_none(),
            });
            self.inspector_context_key = key;
        }
        if context.is_none()
            && (self.inspector_context.caret != self.inspector_caret
                || self.inspector_context.selection_generation
                    != self.inspector_selection_generation
                || self.inspector_context.editable != self.session.pending_transition().is_none())
        {
            let mut updated = (*self.inspector_context).clone();
            updated.caret = self.inspector_caret.clone();
            updated.selection_generation = self.inspector_selection_generation;
            updated.editable = self.session.pending_transition().is_none();
            context = Some(updated);
        }
        if let Some(context) = context {
            self.inspector_context = std::sync::Arc::new(context);
        }
    }

    pub(super) fn inspector_context(&self) -> std::sync::Arc<InspectorPadContext> {
        std::sync::Arc::clone(&self.inspector_context)
    }

    pub(super) fn apply_inspector_insert(
        &mut self,
        ctx: &egui::Context,
        context: &InspectorPadContext,
        request: crate::calculation_pad::InspectorInsertRequest,
    ) -> bool {
        self.sync_inspector_context();
        if self.inspector_context.as_ref() != context
            || context.candidate.as_ref() != Some(request.source())
            || context
                .caret
                .as_ref()
                .is_none_or(|caret| &caret.range != request.target())
        {
            return false;
        }
        match self.session.apply_inspector_insert(request) {
            Ok(changed) => {
                if changed {
                    self.inspector_caret = None;
                    self.schedule_recovery_snapshot(ctx);
                    self.request_source_focus();
                }
                self.sync_inspector_context();
                changed
            }
            Err(error) => {
                self.error = Some(format!("Inspector insert refused: {error:?}"));
                false
            }
        }
    }

    fn apply_host_request(&mut self, ctx: &egui::Context, requested_host: PadHost) -> bool {
        if self.host_state.presentation() == requested_host {
            return false;
        }
        self.host_state.transition(requested_host);
        self.deferred_source_focus = None;
        match requested_host {
            PadHost::Docked => {
                self.focus_source = true;
                ctx.send_viewport_cmd_to(pad_viewport_id(), egui::ViewportCommand::Close);
            }
            PadHost::Popout => {
                self.focus_source = true;
            }
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
        true
    }

    fn resolve_result_copy(
        &mut self,
        intent: ResultCopyIntent,
    ) -> Option<CalculationPadDialogEffect> {
        if self.session.current_id() != Some(intent.document_id) {
            return None;
        }
        let evaluation = self.session.evaluation()?;
        let text = match intent.target {
            ResultCopyTarget::All => copy_all_results(evaluation),
            ResultCopyTarget::Line(line_id) => evaluation
                .lines
                .iter()
                .find(|line| line.id == line_id)
                .and_then(|line| match &line.outcome {
                    LineOutcome::Value(value) => value.formatted_result(),
                    _ => None,
                })?,
        };
        Some(CalculationPadDialogEffect::CopyText(text))
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
        host_request: &mut Option<PadHost>,
        result_copy: &mut Option<ResultCopyIntent>,
        inspector_selection: &mut Option<LineId>,
        inspector_caret: &mut Option<std::ops::Range<usize>>,
        open_inspector: &mut bool,
        host_identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
    ) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {
                *intent = Some(DialogIntent::New);
            }
            if ui.button("Open").clicked() {
                *intent = Some(DialogIntent::OpenChooser);
            }
            let save = ui.add_enabled(
                self.session.pending_transition().is_none(),
                egui::Button::new("Save"),
            );
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_save_rect = Some(save.rect);
            }
            if save.clicked() {
                *intent = Some(DialogIntent::Save);
            }
            if ui.button("Save As…").clicked() {
                *intent = Some(DialogIntent::OpenSaveAs);
            }
            let examples = ui.button("Examples & reference");
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_examples_button_rect = Some(examples.rect);
            }
            if examples.clicked() {
                *intent = Some(DialogIntent::OpenExamples);
            }
            let inspector = ui.button("Programmer Inspector…");
            #[cfg(test)]
            {
                self.test_inspector_reopen =
                    Some((inspector.rect, ui.clip_rect(), inspector.enabled()));
            }
            if inspector.clicked() {
                *open_inspector = true;
            }
            let (host_button, requested_host) =
                super::utility_host::presentation_button(ui, host_identity.host, true);
            #[cfg(test)]
            if ui.is_visible() && !ui.is_sizing_pass() {
                self.test_host_button_rect = Some(host_button.rect);
            }
            if host_button.clicked() {
                *host_request = Some(requested_host);
            }
            ui.separator();
            let mut remember_source = self.remember_source;
            if ui
                .checkbox(&mut remember_source, "Remember last source")
                .changed()
            {
                *intent = Some(DialogIntent::SetRememberSource(remember_source));
            }
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
                    inspector_caret,
                );
                self.results_ui(
                    &mut columns[1],
                    evaluation,
                    available_height,
                    result_copy,
                    host_identity,
                    viewport_id,
                    inspector_selection,
                    open_inspector,
                );
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
                inspector_caret,
            );
            self.results_ui(
                ui,
                evaluation,
                (available_height - half_height).max(90.0),
                result_copy,
                host_identity,
                viewport_id,
                inspector_selection,
                open_inspector,
            );
        }
    }

    fn examples_ui(&mut self, ui: &mut egui::Ui, effect: &mut Option<CalculationPadDialogEffect>) {
        ui.label(
            "One expression or name = expression per physical line; # starts a full-line comment.",
        );
        ui.label(
            "Results can be finite f64 scalars, exact fixed-width integers, dimensional quantities, or exact base conversions.",
        );

        ui.collapsing("Operator and function reference", |ui| {
            ui.label("Operators: +, -, *, /, ^, and % (remainder keeps the dividend sign). Unary + and - are supported; scientific notation accepts e or E, such as 1.25e-2.");
            ui.label("mod(a, b) gives a non-negative Euclidean remainder. Division and remainder by zero are errors.");
            ui.label("Functions: abs, signum, sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh, floor, round, ceil, trunc, fract, exp, sqrt, cbrt, ln, log, log2, log10, atan2, min, and max.");
            ui.label("Constants: PI, π, E, e, TAU, and τ. Trigonometric functions use radians.");
        });
        ui.collapsing("Exact integer and bit operations", |ui| {
            ui.label("Programmer operators, helpers, typed constructors, and exact-integer references select fixed-width integer arithmetic. Untyped programmer expressions default to u64; signed and unsigned widths from 8 to 128 bits are available.");
            ui.label("Checked arithmetic includes +, -, *, ^, idiv, rem, and %. Bitwise operators are &, |, ~, and word aliases and/or/xor/not; shifts use << and >>. / remains scalar division and is unavailable in typed integer expressions.");
            ui.label("Precedence (tight to loose): calls/grouping, ^, unary + - ~ not, * idiv rem %, + -, shifts, &/and, xor, |/or. Binary operators associate left except right-associative ^.");
            ui.label("Left shifts discard bits above the width, signed right shifts extend the sign, and shift counts are 0 through width-1. Nonnegative rotl/rotr counts reduce modulo width; popcount, mask, and extract operate on exact bit patterns.");
            ui.label("Integer overflow, division by zero, invalid shifts, and mixed modes are errors. Decimal and 0x/0o/0b integer tokens are parsed exactly; explicit constructors are required to convert scalar references safely.");
        });
        ui.collapsing("Dimensional unit arithmetic", |ui| {
            ui.label("Catalog units support compatible sums and differences, scalar scaling, linear products and quotients, and small integer powers. Examples: 10 m + 20 cm, (2 m) * (3 m), and (5 m) / (2 s).");
            ui.label("Targets use 'to', for example area = (2 m) * (3 m) followed by area to cm^2. Dimensions and month/year approximation metadata survive named and rN references and compatible conversions.");
            ui.label("Addition and subtraction require matching dimensions; scalars can combine with dimensionless quantities but not dimensioned quantities. Nonlinear temperature and fuel-economy units remain conversion-only; semantic catalog categories such as energy/torque and angle/data stay distinct.");
            ui.label("Quantity expressions are limited to 256 tokens, nesting depth 32, and unit powers from -8 through 8. The source expression limit is 2,048 bytes.");
        });
        ui.collapsing("Names, lines, units, and limits", |ui| {
            ui.label("Use earlier assignments by name. Braced names may contain spaces and Unicode.");
            ui.label("rN refers to the one-based physical source line N; blank and comment lines count.");
            ui.label("Physical quantities retain their dimensions: addition and subtraction require matching dimensions, with scalars treated as dimensionless.");
            ui.label("0x, 0b, and 0o are scalar integer prefixes, accepted only through 2^53. Exact base conversion remains a separate u128 path.");
            ui.label("Programmer integer literals are exact and are separate from scalar numeric literals.");
            ui.label("Finite f64 operands and intermediate results are required; source expressions are limited to 2,048 bytes, normalized expressions to 8 KiB, and nesting to 32 parentheses.");
            ui.label("Documents are limited to 64 KiB and 256 physical lines; names to 128 UTF-8 bytes; identifiers to 64 per line and 512 per document.");
        });

        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("calculation-pad-example-catalog")
            .max_height(ui.available_height())
            .show(ui, |ui| {
                for (group_index, group) in EXAMPLE_GROUPS.iter().enumerate() {
                    egui::CollapsingHeader::new(group.title)
                        .id_salt(("calculation-pad-example-group", group_index))
                        .default_open(true)
                        .show(ui, |ui| {
                            for example in group.examples {
                                ui.group(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong(example.title);
                                        let copy = ui.small_button("Copy example");
                                        #[cfg(test)]
                                        if self.test_examples_copy_rect.is_none() {
                                            self.test_examples_copy_rect = Some(copy.rect);
                                        }
                                        if copy.clicked() {
                                            *effect = Some(CalculationPadDialogEffect::CopyText(
                                                example.source.to_owned(),
                                            ));
                                        }
                                    });
                                    ui.code(example.source);
                                    ui.weak(example.explanation);
                                });
                            }
                        });
                }
            });
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
        inspector_caret: &mut Option<std::ops::Range<usize>>,
    ) {
        ui.heading("Source");
        let response = ui
            .add_enabled_ui(self.session.pending_transition().is_none(), |ui| {
                egui::ScrollArea::both()
                    .id_salt("calculation-pad-editor-scroll")
                    .max_height(max_height)
                    .show(ui, |ui| {
                        egui::TextEdit::multiline(source)
                            .id(editor_id)
                            .desired_rows(12)
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace)
                            .hint_text("total = 2 + 3\nr1 * 4")
                            .show(ui)
                    })
                    .inner
            })
            .inner;

        let response = &response.response;
        let cursor_interaction = response.clicked()
            || response.dragged()
            || response.changed()
            || ui.input(|input| {
                input.events.iter().any(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        matches!(
                            key,
                            egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                                | egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::Home
                                | egui::Key::End
                        ) || (*key == egui::Key::A && modifiers.command)
                    }
                    _ => false,
                })
            });
        if authorized_pass && response.has_focus() && cursor_interaction {
            if let Some(range) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), editor_id)
                .and_then(|state| state.cursor.char_range())
            {
                let to_byte = |index: usize| {
                    source
                        .char_indices()
                        .map(|(offset, _)| offset)
                        .chain(std::iter::once(source.len()))
                        .nth(index)
                };
                if let (Some(a), Some(b)) = (
                    to_byte(range.primary.index.into()),
                    to_byte(range.secondary.index.into()),
                ) {
                    *inspector_caret = Some(a.min(b)..a.max(b));
                }
            }
        }
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
        result_copy: &mut Option<ResultCopyIntent>,
        host_identity: PadHostIdentity,
        viewport_id: egui::ViewportId,
        inspector_selection: &mut Option<LineId>,
        open_inspector: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.heading("Results");
            let response = ui.button("Copy all");
            #[cfg(test)]
            {
                self.test_copy_all_rect = Some(response.rect);
            }
            if response.clicked()
                && let Some(document_id) = self.session.current_id()
            {
                *result_copy = Some(ResultCopyIntent {
                    host: host_identity,
                    viewport_id,
                    document_id,
                    target: ResultCopyTarget::All,
                });
            }
        });

        #[cfg(test)]
        self.test_line_copy_rects.clear();
        #[cfg(test)]
        self.test_inspect_rects.clear();

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
                                    if matches!(value, crate::calculation_pad::CalculationValue::Integer(_) | crate::calculation_pad::CalculationValue::ExactBase { .. }) {
                                        let inspect = ui.small_button("Inspect bits…");
                                        #[cfg(test)]
                                        self.test_inspect_rects.push((line.line_index, inspect.rect, ui.clip_rect(), inspect.enabled()));
                                        if inspect.clicked() {
                                            *inspector_selection = Some(line.id);
                                            *open_inspector = true;
                                        }
                                    }
                                    if value.is_approximate() {
                                        ui.weak("approx.");
                                    }
                                    let response = ui.small_button("Copy");
                                    #[cfg(test)]
                                    self.test_line_copy_rects
                                        .push((line.line_index, response.rect));
                                    if response.clicked()
                                        && let Some(document_id) = self.session.current_id()
                                    {
                                        *result_copy = Some(ResultCopyIntent {
                                            host: host_identity,
                                            viewport_id,
                                            document_id,
                                            target: ResultCopyTarget::Line(line.id),
                                        });
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
                let keep_editing = ui.button("Keep Editing");
                #[cfg(test)]
                if ui.is_visible() && !ui.is_sizing_pass() {
                    self.test_keep_editing_rect = Some(keep_editing.rect);
                }
                if keep_editing.clicked() {
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
            DialogIntent::OpenExamples => self.examples_open = true,
            DialogIntent::Decision(decision) => self.decide_pending(decision),
            DialogIntent::SetRememberSource(_) => {}
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
        let pending_before = self.session.pending_transition();
        if decision == TransitionDecision::Discard
            && pending_before.is_some()
            && self.remember_source
            && let Err(error) = self.recovery_writer.clear_now()
        {
            self.set_recovery_error(error.to_string());
            return;
        }
        if decision == TransitionDecision::Discard && pending_before.is_some() {
            self.last_recovery_identity = None;
        }
        match self.session.resolve_pending(decision) {
            Ok(TransitionOutcome::Applied) => self.after_transition_applied(),
            Ok(TransitionOutcome::DecisionRequired) => self.clear_feedback(),
            Ok(TransitionOutcome::Cancelled) => {
                self.open = true;
                self.request_source_focus();
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
        if self.open {
            self.request_source_focus();
        }
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
        self.examples_open = false;
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
    pub(crate) fn test_current_document_id(&self) -> Option<DocumentId> {
        self.session.current_id()
    }

    #[cfg(test)]
    pub(crate) fn test_is_dirty(&self) -> bool {
        self.session.is_dirty()
    }

    #[cfg(test)]
    pub(crate) fn test_save_button_rect(&self) -> Option<egui::Rect> {
        self.test_save_rect
    }

    #[cfg(test)]
    pub(crate) fn test_copy_all_button_rect(&self) -> Option<egui::Rect> {
        self.test_copy_all_rect
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
    pub(crate) fn test_editor_rect(&self) -> Option<egui::Rect> {
        self.test_editor_rect
    }

    #[cfg(test)]
    pub(crate) fn test_inspect_button(
        &self,
        index: usize,
    ) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_inspect_rects
            .iter()
            .find(|(line, _, _, _)| *line == index)
            .map(|(_, rect, clip, enabled)| (*rect, *clip, *enabled))
    }

    #[cfg(test)]
    pub(crate) fn test_inspector_reopen(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_inspector_reopen
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
    pub(crate) fn test_open_examples(&mut self) {
        self.examples_open = true;
    }

    #[cfg(test)]
    pub(crate) fn test_examples_button_rect(&self) -> Option<egui::Rect> {
        self.test_examples_button_rect
    }

    #[cfg(test)]
    pub(crate) fn test_examples_window_rect(&self) -> Option<egui::Rect> {
        self.test_examples_window_rect
    }

    #[cfg(test)]
    pub(crate) fn test_examples_copy_rect(&self) -> Option<egui::Rect> {
        self.test_examples_copy_rect
    }

    #[cfg(test)]
    pub(crate) fn test_apply_save_as(&mut self, name: &str) {
        self.save_as(name);
    }

    #[cfg(test)]
    pub(crate) fn test_open_document(&mut self, id: DocumentId) {
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
        CalculationPadSession, CalculationValue, LineOutcome, MAX_DOCUMENT_BYTES, PadDocumentStore,
        PadRecoverySnapshot, PadRecoveryStore, PadRecoveryWriteFailure, PadRecoveryWriter,
        PendingTransition, TransitionDecision,
    };
    use crate::platform::app_data::AppDataRoot;
    use eframe::egui;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn inspector_pad_receipt_rejects_stale_source_selection_and_caret() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();
        dialog.test_replace_source("u8(1)");
        dialog.session.evaluation();
        let line = dialog.session.line_id(0).unwrap();
        let mut old = dialog.empty_host_effects(egui::ViewportId::ROOT);
        old.pass_accepted = true;
        old.edited_source = Some("u8(2)".into());
        old.inspector_selection = Some(line);
        old.inspector_caret = Some(0..5);
        old.open_inspector = true;
        dialog.test_replace_source("u8(3)");
        assert!(complete_host_effects(&mut dialog, &ctx, old).is_empty());
        assert_eq!(dialog.test_source(), Some("u8(3)"));
        assert!(dialog.inspector_context().candidate.is_none());
        assert!(dialog.inspector_context().caret.is_none());
    }

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
        frame_effects(dialog, ctx, screen_size, events)
            .into_iter()
            .next()
    }

    fn frame_effects(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<CalculationPadDialogEffect> {
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
        });
        let effect = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effect
    }

    fn complete_host_effects(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        mut host: super::HostEffects,
    ) -> Vec<CalculationPadDialogEffect> {
        let root_pass = super::ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        host.pass = if host.pass_accepted {
            Some(super::ViewportPassProvenance::current(
                ctx,
                host.viewport_id,
            ))
        } else {
            None
        };

        let output = ctx.run_ui(input(egui::vec2(1200.0, 900.0), Vec::new()), |_| {});
        dialog.pending_root_effects = Some(super::RootEffects {
            pass: root_pass,
            root_effect: None,
            close_examples_requested: false,
            host: Some(host),
        });
        let effects = dialog.on_ui_frame_complete(ctx);
        output.drop_without_applying_deltas();
        effects
    }

    fn immediate_viewport_frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        root_events: Vec<egui::Event>,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_renderer(ctx, || {
            let output = ctx.run_ui(input(egui::vec2(1200.0, 900.0), root_events), |ui| {
                dialog.show(ui.ctx(), ui.ctx().current_pass_index() == 0, true, true);
            });
            let commands = output
                .viewport_output
                .get(&super::pad_viewport_id())
                .map(|output| output.commands.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            dialog.on_ui_frame_complete(ctx);
            commands
        })
    }

    fn immediate_child_frame(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<egui::ViewportCommand> {
        super::super::render::with_immediate_viewport_test_events_for(
            super::pad_viewport_id(),
            events,
            || immediate_viewport_frame(dialog, ctx, Vec::new()),
        )
    }

    fn click_immediate_child(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        point: egui::Pos2,
    ) -> Vec<egui::ViewportCommand> {
        let mut commands = Vec::new();
        for pressed in [true, false] {
            commands = immediate_child_frame(dialog, ctx, click_events(point, pressed));
        }
        commands
    }

    fn frame_requesting_discard(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (Option<CalculationPadDialogEffect>, bool) {
        let mut first_ui_pass_seen = false;
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            if first_ui_pass {
                first_ui_pass_seen = true;
                ui.ctx()
                    .request_discard("Calculation Pad discarded-pass regression");
            }
            dialog.show(ui.ctx(), first_ui_pass, true, true);
        });
        let effect = dialog.on_ui_frame_complete(ctx).into_iter().next();
        output.drop_without_applying_deltas();
        (effect, first_ui_pass_seen)
    }

    fn frame_requesting_late_discard(
        dialog: &mut CalculationPadDialog,
        ctx: &egui::Context,
        screen_size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> Option<CalculationPadDialogEffect> {
        let output = ctx.run_ui(input(screen_size, events), |ui| {
            let first_ui_pass = ui.ctx().current_pass_index() == 0;
            dialog.show(ui.ctx(), first_ui_pass, true, true);
            if first_ui_pass {
                ui.ctx()
                    .request_discard("Calculation Pad late ROOT discard regression");
            }
        });
        let effect = dialog.on_ui_frame_complete(ctx).into_iter().next();
        output.drop_without_applying_deltas();
        effect
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
    fn accepted_copy_resolves_after_same_frame_source_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        let document_id = dialog.test_current_document_id().unwrap();
        let retained_line = dialog.session.line_id(1).unwrap();
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 1);

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("amount = 5\namount * 3".into());
        host.result_copy = Some(super::ResultCopyIntent {
            host: host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(retained_line),
        });

        assert_eq!(
            complete_host_effects(&mut dialog, &egui::Context::default(), host),
            vec![CalculationPadDialogEffect::CopyText("15".into())]
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(dialog.session.evaluation_count(), 2);
        assert!(dialog.test_is_dirty());
        assert!(dialog.session.undo());
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 3);
        assert!(dialog.session.redo());
        assert_eq!(value(&mut dialog.session, 1), 15.0);
        assert_eq!(dialog.session.evaluation_count(), 4);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
    }

    #[test]
    fn retired_line_copy_is_refused_after_same_frame_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("1 + 2\n3 + 4");
        let document_id = dialog.test_current_document_id().unwrap();
        let surviving_line = dialog.session.line_id(0).unwrap();
        let retired_line = dialog.session.line_id(1).unwrap();

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("1 + 2".into());
        host.result_copy = Some(super::ResultCopyIntent {
            host: host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(retired_line),
        });

        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), host).is_empty());
        assert_eq!(dialog.test_source(), Some("1 + 2"));
        assert_eq!(dialog.session.line_id(0), Some(surviving_line));
        assert_ne!(dialog.session.line_id(0), Some(retired_line));
    }

    #[test]
    fn save_and_close_follow_an_accepted_same_frame_source_edit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Accepted edit");
        let document_id = dialog.test_current_document_id().unwrap();

        let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host.pass_accepted = true;
        host.edited_source = Some("amount = 9\namount * 2".into());
        host.intent = Some(DialogIntent::Save);
        host.close_requested = true;

        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), host).is_empty());
        assert!(!dialog.open);
        assert_eq!(dialog.session.source(), None);
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some("amount = 9\namount * 2".into()),
            "same-frame edit is captured before Save and close retire the session"
        );

        dialog.test_open_document(document_id);
        assert_eq!(dialog.test_source(), Some("amount = 9\namount * 2"));
        assert!(!dialog.test_is_dirty());
    }

    #[test]
    fn pad_recovery_preference_waits_for_an_accepted_pass_and_settings_commit() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();

        let mut rejected = dialog.empty_host_effects(egui::ViewportId::ROOT);
        rejected.intent = Some(DialogIntent::SetRememberSource(true));
        assert!(complete_host_effects(&mut dialog, &egui::Context::default(), rejected).is_empty());
        assert!(!dialog.remember_source);

        let mut accepted = dialog.empty_host_effects(egui::ViewportId::ROOT);
        accepted.pass_accepted = true;
        accepted.intent = Some(DialogIntent::SetRememberSource(true));
        assert_eq!(
            complete_host_effects(&mut dialog, &egui::Context::default(), accepted),
            vec![CalculationPadDialogEffect::SaveRecoveryPreference(true)]
        );
        assert!(
            !dialog.remember_source,
            "the runtime preference changes only after the settings-save acknowledgement"
        );
    }

    #[test]
    fn accepted_transition_intents_preserve_or_clear_recovery_at_lifecycle_boundaries() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("persisted draft");
        dialog.save_recovery_now(None).unwrap();

        let accept_intent = |dialog: &mut CalculationPadDialog, intent| {
            let mut host = dialog.empty_host_effects(egui::ViewportId::ROOT);
            host.pass_accepted = true;
            host.intent = Some(intent);
            complete_host_effects(dialog, &ctx, host)
        };

        accept_intent(&mut dialog, DialogIntent::New);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::New)
        );
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Cancel),
        );
        assert_eq!(dialog.test_source(), Some("persisted draft"));
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some("persisted draft".into()),
            "Cancel retains the prior recoverable draft"
        );

        accept_intent(&mut dialog, DialogIntent::New);
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Discard),
        );
        assert_eq!(dialog.test_source(), Some(""));
        assert_eq!(
            dialog
                .recovery_writer
                .load()
                .unwrap()
                .map(|snapshot| snapshot.source),
            Some(String::new()),
            "Discard before New replaces the prior source with a blank recovery record"
        );

        dialog.test_replace_source("queued close discard draft");
        dialog.schedule_recovery_snapshot(&ctx);
        let mut close = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close.pass_accepted = true;
        close.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Discard),
        );
        assert!(!dialog.open);
        assert_eq!(dialog.recovery_writer.load().unwrap(), None);

        dialog.open();
        assert_eq!(dialog.test_source(), Some(""));
        dialog.test_replace_source("queued disable draft");
        dialog.schedule_recovery_snapshot(&ctx);
        dialog.apply_committed_recovery_preference(false).unwrap();
        assert!(!dialog.remember_source);
        assert_eq!(dialog.recovery_writer.load().unwrap(), None);

        let mut close_disabled = dialog.empty_host_effects(egui::ViewportId::ROOT);
        close_disabled.pass_accepted = true;
        close_disabled.close_requested = true;
        complete_host_effects(&mut dialog, &ctx, close_disabled);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        accept_intent(
            &mut dialog,
            DialogIntent::Decision(TransitionDecision::Discard),
        );
        dialog.apply_committed_recovery_preference(true).unwrap();
        dialog.open();
        assert_eq!(dialog.test_source(), Some(""));
        assert_eq!(dialog.recovery_writer.load().unwrap(), None);
    }

    #[test]
    fn delayed_old_document_recovery_failure_is_not_published_on_the_new_document() {
        let (_directory, root, mut dialog) = fixture();
        let store = PadRecoveryStore::new(&root);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let release_receiver = std::sync::Arc::new(std::sync::Mutex::new(release_receiver));
        dialog
            .recovery_writer
            .set_before_write_hook(std::sync::Arc::new(move || {
                started_sender.send(()).unwrap();
                release_receiver.lock().unwrap().recv().unwrap();
            }));
        dialog.initialize_recovery_preference(true);
        dialog.open();
        dialog.test_replace_source("old document draft");
        let old_document = dialog.test_current_document_id().unwrap();
        let old_revision = dialog.session.revision().unwrap();
        std::fs::create_dir(store.path()).unwrap();
        dialog.schedule_recovery_snapshot(&egui::Context::default());

        started_receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("the background recovery write reached its delayed section");
        dialog.test_apply_save_as("new document identity");
        let new_document = dialog.test_current_document_id().unwrap();
        assert_ne!(new_document, old_document);
        release_sender.send(()).unwrap();

        let failures = dialog.recovery_writer.flush().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].document_id, old_document);
        assert_eq!(failures[0].revision, old_revision);
        assert!(!failures[0].message.is_empty());
        dialog.publish_recovery_failures(failures);
        assert!(
            dialog
                .error
                .as_deref()
                .is_none_or(|message| !message.contains("recovery failed")),
            "a delayed failure for the old identity must not be shown on the new document"
        );
    }

    #[test]
    fn recovery_load_is_lazy_until_explicit_open_and_preserves_a_live_draft_on_error() {
        let (_directory, root, mut dialog) = fixture();
        let mut saved_session = CalculationPadSession::new(&root);
        saved_session.replace_source("seed").unwrap();
        saved_session.save_as("identity").unwrap();
        let saved_id = saved_session.current_id().unwrap();
        let mut writer = PadRecoveryWriter::new(PadRecoveryStore::new(&root));
        writer
            .save_now(PadRecoverySnapshot {
                document_id: saved_id,
                revision: 7,
                source: "π = 2\r\nπ * 3\r\n".into(),
            })
            .unwrap();
        writer.shutdown().unwrap();

        dialog.initialize_recovery_preference(true);
        dialog.ensure_open();
        assert_eq!(dialog.test_source(), Some(""));
        dialog.open();
        assert_eq!(dialog.test_source(), Some("π = 2\r\nπ * 3\r\n"));
        assert!(dialog.test_is_dirty());
        assert!(!dialog.test_can_undo());

        let malformed_store = PadRecoveryStore::new(&root);
        std::fs::write(malformed_store.path(), b"not-json").unwrap();
        dialog.recovery_attempted = false;
        dialog.open();
        assert_eq!(dialog.test_source(), Some("π = 2\r\nπ * 3\r\n"));
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|message| message.contains("recovery failed"))
        );
        dialog.apply_committed_recovery_preference(false).unwrap();
        assert!(!malformed_store.path().exists());
        assert_eq!(saved_session.source(), Some("seed"));
    }

    #[test]
    fn explicit_examples_and_dock_requests_emit_show_root_only_after_host_acceptance() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        dialog.open();

        let mut examples = dialog.empty_host_effects(egui::ViewportId::ROOT);
        examples.pass_accepted = true;
        examples.intent = Some(DialogIntent::OpenExamples);
        assert_eq!(
            complete_host_effects(&mut dialog, &ctx, examples),
            vec![CalculationPadDialogEffect::ShowRoot]
        );
        assert!(dialog.examples_open);
        assert!(dialog.needs_root_ui_when_hidden());

        let retired_docked_identity = dialog.host_identity();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);
        let mut stale_dock = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_dock.pass_accepted = true;
        stale_dock.identity = retired_docked_identity;
        stale_dock.host_request = Some(super::PadHost::Docked);
        assert!(complete_host_effects(&mut dialog, &ctx, stale_dock).is_empty());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);

        let mut dock = dialog.empty_host_effects(egui::ViewportId::ROOT);
        dock.pass_accepted = true;
        dock.host_request = Some(super::PadHost::Docked);
        assert_eq!(
            complete_host_effects(&mut dialog, &ctx, dock),
            vec![CalculationPadDialogEffect::ShowRoot]
        );
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);

        dialog.examples_open = false;
        let mut stale_examples = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_examples.pass_accepted = true;
        stale_examples.identity.generation = stale_examples.identity.generation.wrapping_add(1);
        stale_examples.intent = Some(DialogIntent::OpenExamples);
        assert!(complete_host_effects(&mut dialog, &ctx, stale_examples).is_empty());
        assert!(!dialog.examples_open);
    }

    #[test]
    fn rejected_child_or_retired_host_effect_does_not_apply_source_or_copy() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount * 2");
        let document_id = dialog.test_current_document_id().unwrap();
        let line_id = dialog.session.line_id(1).unwrap();
        let original_source = dialog.test_source().unwrap().to_owned();
        let ctx = egui::Context::default();
        let retired_host_identity = dialog.host_identity();
        dialog.apply_host_request(&ctx, super::PadHost::Popout);

        let mut child_discard = dialog.empty_host_effects(super::pad_viewport_id());
        child_discard.pass_accepted = false;
        child_discard.edited_source = Some("amount = 5\namount * 2".into());
        child_discard.native_close_requested = true;
        child_discard.result_copy = Some(super::ResultCopyIntent {
            host: child_discard.identity,
            viewport_id: super::pad_viewport_id(),
            document_id,
            target: super::ResultCopyTarget::Line(line_id),
        });
        assert!(complete_host_effects(&mut dialog, &ctx, child_discard).is_empty());
        assert_eq!(dialog.test_source(), Some(original_source.as_str()));
        assert!(dialog.open);

        let mut stale_host = dialog.empty_host_effects(egui::ViewportId::ROOT);
        stale_host.pass_accepted = true;
        stale_host.identity = retired_host_identity;
        stale_host.edited_source = Some("amount = 8\namount * 2".into());
        stale_host.result_copy = Some(super::ResultCopyIntent {
            host: stale_host.identity,
            viewport_id: egui::ViewportId::ROOT,
            document_id,
            target: super::ResultCopyTarget::Line(line_id),
        });
        assert!(complete_host_effects(&mut dialog, &ctx, stale_host).is_empty());
        assert_eq!(dialog.test_source(), Some(original_source.as_str()));
        assert!(dialog.open);
    }

    #[test]
    fn studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(true);
        let initial_size = egui::vec2(1000.0, 760.0);
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        assert_eq!(value(&mut dialog.session, 1), 6.0);
        assert_eq!(dialog.session.evaluation_count(), 1);
        assert_eq!(
            dialog.session.request_new().unwrap(),
            crate::calculation_pad::TransitionOutcome::DecisionRequired
        );

        for _ in 0..3 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        let editor_id = dialog
            .test_editor_id
            .expect("the source editor is rendered");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        frame(&mut dialog, &ctx, initial_size, Vec::new());
        dialog.test_open_examples();
        assert!(dialog.needs_root_ui_when_hidden());
        frame(&mut dialog, &ctx, initial_size, Vec::new());
        let editor_center = dialog
            .test_editor_rect
            .expect("source editor rect")
            .center();
        frame(
            &mut dialog,
            &ctx,
            initial_size,
            vec![
                egui::Event::PointerMoved(editor_center),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -60.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(&mut dialog, &ctx, egui::vec2(1280.0, 900.0), Vec::new());
        dialog.examples_open = false;
        frame(&mut dialog, &ctx, initial_size, Vec::new());

        let state_before = (
            dialog.session.current_id(),
            dialog.session.current_name().map(str::to_owned),
            dialog.session.source().map(str::to_owned),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.line_ids().unwrap().to_vec(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
            dialog.session.pending_transition(),
            dialog.session.evaluation_count(),
            copy_all_results(dialog.session.evaluation().unwrap()),
        );
        let popout_button = dialog
            .test_host_button_rect
            .expect("the docked host offers Pop out");
        click(&mut dialog, &ctx, initial_size, popout_button.center());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);
        assert!(dialog.needs_root_ui_when_hidden());

        for _ in 0..3 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(egui::ViewportId::ROOT)
        );
        assert!(dialog.host_state.embedded_root_layer().is_some());
        assert!(!dialog.host_state.bootstrap_pending());
        assert!(dialog.needs_root_ui_when_hidden());
        let dock_button = dialog
            .test_host_button_rect
            .expect("the embedded popout offers Dock back");
        assert_eq!(
            click(&mut dialog, &ctx, initial_size, dock_button.center()),
            Some(CalculationPadDialogEffect::ShowRoot)
        );
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Docked);
        assert!(!dialog.needs_root_ui_when_hidden());

        for _ in 0..2 {
            frame(&mut dialog, &ctx, initial_size, Vec::new());
        }
        let state_after = (
            dialog.session.current_id(),
            dialog.session.current_name().map(str::to_owned),
            dialog.session.source().map(str::to_owned),
            dialog.session.revision(),
            dialog.session.is_dirty(),
            dialog.session.line_ids().unwrap().to_vec(),
            dialog.session.can_undo(),
            dialog.session.can_redo(),
            dialog.session.pending_transition(),
            dialog.session.evaluation_count(),
            copy_all_results(dialog.session.evaluation().unwrap()),
        );
        assert_eq!(state_after, state_before);
    }

    #[test]
    fn native_popout_child_discard_and_os_close_keep_the_edited_session() {
        let (_directory, _root, mut dialog) = fixture();
        let ctx = egui::Context::default();
        let size = egui::vec2(1000.0, 760.0);
        dialog.open();
        dialog.test_replace_source("amount = 2\namount * 3");
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let popout_button = dialog
            .test_host_button_rect
            .expect("the docked pad offers a popout action");
        click(&mut dialog, &ctx, size, popout_button.center());
        assert_eq!(dialog.host_state.presentation(), super::PadHost::Popout);
        for _ in 0..3 {
            immediate_viewport_frame(&mut dialog, &ctx, Vec::new());
        }
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(super::pad_viewport_id())
        );
        assert!(!dialog.host_state.bootstrap_pending());
        assert!(dialog.needs_root_ui_when_hidden());
        let editor_id = dialog
            .test_editor_id
            .expect("native child renders the editor");
        let editor_center = dialog
            .test_editor_rect
            .expect("native child publishes the editor rect")
            .center();
        click_immediate_child(&mut dialog, &ctx, editor_center);
        assert_eq!(
            super::super::render::immediate_viewport_test_focused_id(),
            Some((super::pad_viewport_id(), Some(editor_id)))
        );

        immediate_child_frame(
            &mut dialog,
            &ctx,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 5\namount * 3".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(value(&mut dialog.session, 1), 15.0);
        let accepted_revision = dialog.session.revision();
        let accepted_evaluations = dialog.session.evaluation_count();

        dialog.test_request_child_discard = true;
        immediate_child_frame(
            &mut dialog,
            &ctx,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 9\namount * 3".into()),
            ],
        );
        assert!(dialog.test_child_discard_requested);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert_eq!(dialog.session.revision(), accepted_revision);
        assert_eq!(dialog.session.evaluation_count(), accepted_evaluations);

        let close_pass_commands = super::super::render::with_immediate_viewport_test_close_request(
            super::pad_viewport_id(),
            || immediate_viewport_frame(&mut dialog, &ctx, Vec::new()),
        );
        assert_eq!(
            dialog.test_callback_viewport_id,
            Some(super::pad_viewport_id())
        );
        assert!(dialog.open);
        assert_eq!(
            dialog.test_pending_transition(),
            Some(PendingTransition::Close)
        );
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert!(
            !close_pass_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );

        let cancel_close_commands = immediate_viewport_frame(&mut dialog, &ctx, Vec::new());
        assert!(
            cancel_close_commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        let keep_editing = dialog
            .test_keep_editing_rect
            .expect("native close decision offers Keep Editing");
        click_immediate_child(&mut dialog, &ctx, keep_editing.center());
        assert!(dialog.open);
        assert_eq!(dialog.test_pending_transition(), None);
        assert_eq!(dialog.test_source(), Some("amount = 5\namount * 3"));
        assert!(dialog.test_is_dirty());
    }

    #[test]
    fn studio_usability_calculation_help_open_copy_preserves_draft_dirty_and_history() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        let draft = "draft = 7\ndraft * 3";
        dialog.test_replace_source(draft);
        let ctx = egui::Context::default();
        let size = egui::vec2(1200.0, 900.0);

        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let examples_button = dialog
            .test_examples_button_rect()
            .expect("the main pad has an Examples & reference action");
        assert_eq!(
            click(&mut dialog, &ctx, size, examples_button.center()),
            Some(CalculationPadDialogEffect::ShowRoot)
        );
        assert!(dialog.examples_open);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        assert!(
            dialog
                .test_examples_window_rect()
                .is_some_and(|rect| rect.is_positive())
        );

        let copy_button = dialog
            .test_examples_copy_rect()
            .expect("the shared catalog exposes a copy button");
        let source_before = dialog.test_source().unwrap().to_owned();
        let revision_before = dialog.session.revision();
        let can_undo_before = dialog.test_can_undo();
        let dirty_before = dialog.test_is_dirty();
        assert!(can_undo_before);
        assert!(dirty_before);

        assert_eq!(
            click(&mut dialog, &ctx, size, copy_button.center()),
            Some(CalculationPadDialogEffect::CopyText(
                "radius = 3\nPI * radius ^ 2".into()
            ))
        );
        assert_eq!(dialog.test_source(), Some(source_before.as_str()));
        assert_eq!(dialog.session.revision(), revision_before);
        assert_eq!(dialog.test_can_undo(), can_undo_before);
        assert_eq!(dialog.test_is_dirty(), dirty_before);
    }

    #[test]
    fn saved_document_reopens_and_dirty_close_write_conflict_keeps_the_draft() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 4\namount * 2");
        dialog.test_apply_save_as("Estimate");
        assert!(!dialog.session.is_dirty());
        let saved_id = dialog.session.current_id().unwrap();
        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
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
        frame(&mut dialog, &ctx, size, Vec::new());
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
    fn pad_late_root_discard_keeps_edited_source_and_saved_file_before_save() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Retained");
        let id = dialog.test_current_document_id().unwrap();
        let store = PadDocumentStore::new(&root);
        let path = store
            .directory()
            .join(format!("{:032x}.json", id.as_u128()));
        let saved_bytes = std::fs::read(&path).expect("Save As creates a native document");

        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }
        let editor_id = dialog
            .test_editor_id
            .expect("the source TextEdit is rendered");
        ctx.memory_mut(|memory| memory.request_focus(editor_id));
        assert!(dialog.test_editor_has_focus(&ctx));
        let original_source = "amount = 1\namount";
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());
        let discarded_edit = frame_requesting_late_discard(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 2\namount".into()),
            ],
        );
        assert_eq!(discarded_edit, None);
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());
        assert!(dialog.test_can_undo());
        assert!(dialog.session.undo());
        assert_eq!(dialog.test_source(), Some(""));
        assert!(dialog.session.redo());
        assert_eq!(dialog.test_source(), Some(original_source));
        assert!(!dialog.test_is_dirty());

        frame(&mut dialog, &ctx, size, Vec::new());
        let editor_rect = dialog
            .test_editor_rect
            .expect("a retained frame republishes the enabled source editor");
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(editor_rect.center(), true),
        );
        frame(
            &mut dialog,
            &ctx,
            size,
            click_events(editor_rect.center(), false),
        );
        assert!(dialog.test_editor_has_focus(&ctx));

        frame(
            &mut dialog,
            &ctx,
            size,
            vec![
                key(egui::Key::A, egui::Modifiers::COMMAND),
                egui::Event::Text("amount = 2\namount".into()),
            ],
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount"));
        assert!(dialog.test_is_dirty());

        let save = dialog.test_save_button_rect().expect("Save is visible");
        frame(&mut dialog, &ctx, size, click_events(save.center(), true));
        assert_eq!(
            frame_requesting_late_discard(
                &mut dialog,
                &ctx,
                size,
                click_events(save.center(), false),
            ),
            None,
            "the discarded Save action must not escape the ROOT completion boundary"
        );
        assert_eq!(dialog.test_source(), Some("amount = 2\namount"));
        assert!(dialog.test_is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    }

    #[test]
    fn pad_rejected_source_edit_aborts_same_frame_save_intent() {
        let (_directory, root, mut dialog) = fixture();
        dialog.open();
        dialog.test_replace_source("amount = 1\namount");
        dialog.test_apply_save_as("Retained");
        let id = dialog.test_current_document_id().unwrap();
        let path = PadDocumentStore::new(&root)
            .directory()
            .join(format!("{:032x}.json", id.as_u128()));
        let saved_bytes = std::fs::read(&path).unwrap();

        let ctx = egui::Context::default();
        let size = egui::vec2(960.0, 720.0);
        let pass = super::ViewportPassProvenance::current(&ctx, egui::ViewportId::ROOT);
        let mut host_effects = dialog.empty_host_effects(egui::ViewportId::ROOT);
        host_effects.pass = Some(pass);
        host_effects.pass_accepted = true;
        host_effects.edited_source = Some("x".repeat(MAX_DOCUMENT_BYTES + 1));
        host_effects.intent = Some(DialogIntent::Save);
        host_effects.close_requested = true;
        dialog.pending_root_effects = Some(super::RootEffects {
            pass,
            root_effect: None,
            close_examples_requested: false,
            host: Some(host_effects),
        });
        let output = ctx.run_ui(input(size, Vec::new()), |_| {});
        assert!(dialog.on_ui_frame_complete(&ctx).is_empty());
        output.drop_without_applying_deltas();

        assert_eq!(dialog.test_source(), Some("amount = 1\namount"));
        assert!(!dialog.test_is_dirty());
        assert!(dialog.open, "rejected source edits cannot apply Close");
        assert_eq!(dialog.test_pending_transition(), None);
        assert!(dialog.error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
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
        frame(&mut dialog, &ctx, size, Vec::new());
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
    fn collapsed_pad_host_close_is_applied_without_contents_ui() {
        let (_directory, _root, mut dialog) = fixture();
        dialog.open();
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let size = egui::vec2(960.0, 720.0);
        for _ in 0..3 {
            frame(&mut dialog, &ctx, size, Vec::new());
        }

        let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
            &ctx,
            egui::Id::new("calculation-pad-window").with("collapsing"),
            true,
        );
        collapsing.set_open(false);
        collapsing.store(&ctx);
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(
            dialog.test_editor_id.is_none(),
            "collapsed Pad skips its editor"
        );

        dialog.request_close();
        frame(&mut dialog, &ctx, size, Vec::new());
        assert!(!dialog.open);
        assert!(
            dialog.test_was_rendered(),
            "the collapsed title bar was rendered"
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
