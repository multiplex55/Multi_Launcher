mod files;

use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::gui::jump_mode::ui::{
    FocusDirective, JumpModeScope, JumpModeUi, NavigationDialogKind, NavigationOwner,
    NavigationViewportBinding, register_and_apply_target,
};
use crate::gui::jump_mode::{SurfaceId, TargetGeneration, TargetId, TargetKind};
use crate::xml_toolkit_engine::{MAX_XML_INPUT_BYTES, XmlFormatMode};
use crate::xml_toolkit_session::{
    XmlDetailIntent, XmlDetailStatus, XmlEngineSession, XmlFormatOutput, XmlOutputStatus,
    XmlParseStatus, XmlSchemaStatus, XmlSessionError, XmlSessionSnapshot as SessionSnapshot,
    XmlTreeStatus, XmlXPathStatus,
};
use eframe::egui;
use files::{
    SaveOutcome, read_xml_file, save_xml_file, validate_local_path, validate_utf8_declaration,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const VIEWPORT_ID: &str = "xml_toolkit_viewport";
const MAX_OUTPUT_PREVIEW_CHARS: usize = 8_000;

/// Small response adapter shared by the XML form and its virtualized child
/// views. Identities are static control paths or typed, non-payload entity
/// identities; content and paths never enter Jump metadata.
pub(crate) struct XmlJumpTargets<'a> {
    scope: Option<&'a JumpModeScope>,
    visual_order: u64,
}

impl<'a> XmlJumpTargets<'a> {
    pub(crate) fn new(scope: Option<&'a JumpModeScope>) -> Self {
        Self {
            scope,
            visual_order: 0,
        }
    }

    fn register(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: impl Into<String>,
        kind: TargetKind,
    ) -> Option<(TargetId, Option<FocusDirective>)> {
        let identity = identity.as_ref().to_owned();
        let id = self.scope.map(|scope| scope.target_id(&identity));
        let directive = register_and_apply_target(
            self.scope,
            ui,
            response,
            identity,
            label,
            kind,
            &mut self.visual_order,
        );
        id.map(|id| (id, directive))
    }

    pub(crate) fn button(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::Button);
    }

    pub(crate) fn text_entry(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
    ) {
        let _ = self.register(ui, response, identity, label, TargetKind::TextEntry);
    }

    pub(crate) fn selectable(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        identity: impl AsRef<str>,
        label: &'static str,
        kind: TargetKind,
    ) -> bool {
        let Some((target_id, directive)) = self.register(ui, response, identity, label, kind)
        else {
            return false;
        };
        if directive == Some(FocusDirective::Select) {
            response.request_focus();
            if let Some(scope) = self.scope {
                scope.record_select_focus_owner(&target_id, response.id);
            }
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum XmlToolkitTab {
    #[default]
    Source,
    Tree,
    XPath,
    Validation,
    Output,
}

#[derive(Clone)]
struct PendingSave {
    id: u64,
    dialog_generation: u64,
    path: PathBuf,
    payload: Arc<str>,
    needs_confirmation: bool,
}

enum SessionLifecycle {
    Idle,
    Starting {
        join: JoinHandle<Result<XmlEngineSession, XmlSessionError>>,
        cancel_on_ready: bool,
    },
    Active(XmlEngineSession),
    Closing(XmlEngineSession),
    Failed(String),
}

enum FileJob {
    Load {
        id: u64,
        source_generation: u64,
        dialog_generation: u64,
        path: PathBuf,
    },
    Save {
        id: u64,
        dialog_generation: u64,
        path: PathBuf,
        payload: Arc<str>,
        overwrite_confirmed: bool,
    },
}

enum FileMessage {
    Job(FileJob),
    Shutdown,
}

enum FileResult {
    Loaded(Result<String, String>),
    Saved(Result<SaveOutcome, String>),
}

struct FileCompletion {
    id: u64,
    source_generation: Option<u64>,
    dialog_generation: u64,
    path: PathBuf,
    result: FileResult,
}

struct XmlFileWorker {
    sender: mpsc::SyncSender<FileMessage>,
    receiver: mpsc::Receiver<FileCompletion>,
    join: Option<JoinHandle<()>>,
    in_flight: Option<u64>,
}

impl XmlFileWorker {
    fn start(ctx: &egui::Context) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let (completion_sender, completion_receiver) = mpsc::channel();
        let repaint_context = ctx.clone();
        let join = thread::Builder::new()
            .name("xml-toolkit-files".to_owned())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    let FileMessage::Job(job) = message else {
                        return;
                    };
                    let completion = match job {
                        FileJob::Load {
                            id,
                            source_generation,
                            dialog_generation,
                            path,
                        } => FileCompletion {
                            id,
                            source_generation: Some(source_generation),
                            dialog_generation,
                            path: path.clone(),
                            result: FileResult::Loaded(read_xml_file(&path)),
                        },
                        FileJob::Save {
                            id,
                            dialog_generation,
                            path,
                            payload,
                            overwrite_confirmed,
                        } => FileCompletion {
                            id,
                            source_generation: None,
                            dialog_generation,
                            path: path.clone(),
                            result: FileResult::Saved(save_xml_file(
                                &path,
                                payload.as_bytes(),
                                overwrite_confirmed,
                            )),
                        },
                    };
                    if completion_sender.send(completion).is_err() {
                        return;
                    }
                    repaint_context.request_repaint();
                }
            })
            .map_err(|error| format!("Could not start XML file worker: {error}"))?;
        Ok(Self {
            sender,
            receiver: completion_receiver,
            join: Some(join),
            in_flight: None,
        })
    }

    fn submit(&mut self, id: u64, job: FileJob) -> Result<(), String> {
        if self.in_flight.is_some() {
            return Err("An XML file operation is already running.".into());
        }
        self.sender
            .try_send(FileMessage::Job(job))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    "An XML file operation is already queued.".to_owned()
                }
                mpsc::TrySendError::Disconnected(_) => {
                    "The XML file worker has stopped.".to_owned()
                }
            })?;
        self.in_flight = Some(id);
        Ok(())
    }

    fn poll(&mut self) -> Option<FileCompletion> {
        match self.receiver.try_recv() {
            Ok(completion) => {
                if self.in_flight == Some(completion.id) {
                    self.in_flight = None;
                }
                Some(completion)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.in_flight = None;
                None
            }
        }
    }

    fn is_pending(&self) -> bool {
        self.in_flight.is_some()
    }

    fn shutdown(mut self) {
        let _ = self.sender.send(FileMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Default)]
struct UiIntents {
    close: bool,
    edit_source: bool,
    clear_source: bool,
    paste_clipboard: bool,
    open_xml: bool,
    save_source: bool,
    save_output: bool,
    format: Option<XmlFormatMode>,
    apply_output: bool,
    copy_source: bool,
    copy_output: bool,
    tree: crate::gui::xml_toolkit_tree::XmlToolkitTreeIntents,
    xpath: crate::gui::xml_toolkit_xpath::XmlToolkitXPathIntents,
}

/// One resizable XML utility window and its single live asynchronous session.
/// The source text and view drafts survive a close/reopen cycle; the COM
/// session does not. Worker owners remain here until shutdown completes.
pub struct XmlToolkitDialogState {
    pub open: bool,
    source: String,
    session: SessionLifecycle,
    file_worker: Option<XmlFileWorker>,
    clipboard: Arc<dyn ClipboardBackend>,
    tree: crate::gui::xml_toolkit_tree::XmlToolkitTreeViewState,
    xpath: crate::gui::xml_toolkit_xpath::XmlToolkitXPathViewState,
    schema: crate::gui::xml_toolkit_schema::XmlToolkitSchemaViewState,
    tab: XmlToolkitTab,
    focus_viewport: bool,
    bootstrap_pending: bool,
    source_generation: u64,
    accepted_source_generation: Option<u64>,
    dialog_generation: u64,
    native_jump_mode: JumpModeUi,
    native_jump_frame: Option<u64>,
    observed_presentation: Option<(u64, bool)>,
    next_file_request: u64,
    pending_save: Option<PendingSave>,
    feedback: Option<Result<String, String>>,
    schema_path_draft: Option<PathBuf>,
    #[cfg(test)]
    overwrite_button_rects: [egui::Rect; 2],
}

impl Default for XmlToolkitDialogState {
    fn default() -> Self {
        Self::with_clipboard(Arc::new(ArboardClipboardBackend))
    }
}

impl XmlToolkitDialogState {
    pub fn with_clipboard(clipboard: Arc<dyn ClipboardBackend>) -> Self {
        Self {
            open: false,
            source: String::new(),
            session: SessionLifecycle::Idle,
            file_worker: None,
            clipboard,
            tree: crate::gui::xml_toolkit_tree::XmlToolkitTreeViewState::default(),
            xpath: crate::gui::xml_toolkit_xpath::XmlToolkitXPathViewState::default(),
            schema: crate::gui::xml_toolkit_schema::XmlToolkitSchemaViewState::default(),
            tab: XmlToolkitTab::Source,
            focus_viewport: false,
            bootstrap_pending: false,
            source_generation: 0,
            accepted_source_generation: None,
            dialog_generation: 0,
            native_jump_mode: JumpModeUi::default(),
            native_jump_frame: None,
            observed_presentation: None,
            next_file_request: 0,
            pending_save: None,
            feedback: None,
            schema_path_draft: None,
            #[cfg(test)]
            overwrite_button_rects: [egui::Rect::NOTHING; 2],
        }
    }

    /// Open and focus the existing toolkit. Repeated commands preserve the
    /// current editor/session and never read the clipboard implicitly.
    pub fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.dialog_generation = self.dialog_generation.wrapping_add(1);
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
            self.observed_presentation = None;
            self.bootstrap_pending = true;
            if matches!(&self.session, SessionLifecycle::Failed(_)) {
                self.session = SessionLifecycle::Idle;
            }
        }
        self.focus_viewport = true;
    }

    pub(crate) fn reopen_existing(&mut self) {
        self.open();
    }

    pub(crate) fn end_session(&mut self) {
        if !self.open
            && matches!(
                &self.session,
                SessionLifecycle::Idle | SessionLifecycle::Failed(_)
            )
        {
            return;
        }
        self.open = false;
        self.focus_viewport = false;
        self.bootstrap_pending = false;
        self.dialog_generation = self.dialog_generation.wrapping_add(1);
        self.source_generation = self.source_generation.wrapping_add(1);
        self.accepted_source_generation = None;
        self.pending_save = None;
        self.native_jump_mode.cancel_all();
        self.native_jump_frame = None;
        self.observed_presentation = None;
        let _ = self.schema.take_intent();
        if let SessionLifecycle::Active(session) = &self.session {
            self.schema_path_draft = session
                .snapshot()
                .schema
                .selected_path
                .as_ref()
                .map(|path| path.as_path().to_path_buf());
        }
        let lifecycle = std::mem::replace(&mut self.session, SessionLifecycle::Idle);
        self.session = match lifecycle {
            SessionLifecycle::Starting {
                join,
                cancel_on_ready: _,
            } => SessionLifecycle::Starting {
                join,
                cancel_on_ready: true,
            },
            SessionLifecycle::Active(session) => {
                session.request_shutdown();
                SessionLifecycle::Closing(session)
            }
            other => other,
        };
        self.sync_views(&SessionSnapshot::default());
    }

    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.bootstrap_pending
    }

    /// Reaps async owners and renders the immediate native utility viewport.
    /// Clipboard and file work is only started from an explicit committed UI
    /// intent in the first pass of the owning viewport.
    pub fn show(&mut self, ctx: &egui::Context) {
        let root_native_allowed = ctx.viewport_id() == egui::ViewportId::ROOT
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && !egui::Popup::is_any_open(ctx);
        self.show_with_jump_mode(
            ctx,
            None,
            None,
            NavigationOwner::Blocked,
            true,
            root_native_allowed,
            false,
        );
    }

    pub(crate) fn jump_mode_pending_overwrite_identity(&self) -> Option<u64> {
        self.pending_save.as_ref().and_then(|pending| {
            (pending.needs_confirmation && pending.dialog_generation == self.dialog_generation)
                .then_some(pending.id)
        })
    }

    pub(crate) fn root_navigation_instance(&self, current: Option<u64>) -> Option<u64> {
        current.filter(|instance| self.observed_presentation != Some((*instance, true)))
    }

    pub(crate) fn show_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        root_scope: Option<&JumpModeScope>,
        parent_instance: Option<u64>,
        root_owner: NavigationOwner,
        application_input_allowed: bool,
        root_native_application_allowed: bool,
        jump_mode_enabled: bool,
    ) {
        self.poll_file_worker();
        self.poll_session(ctx);
        if !self.open {
            if self.observed_presentation.take().is_some() {
                self.native_jump_mode.cancel_all();
            }
            self.native_jump_frame = None;
            self.request_repaint_if_pending(ctx);
            return;
        }

        let focus = self.focus_viewport;
        let mut intents = UiIntents::default();
        let mut viewport_seen = false;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of(VIEWPORT_ID),
            viewport_builder(),
            |child, class| {
                viewport_seen = true;
                let independent = class == egui::ViewportClass::Immediate;
                let first_pass = child.current_pass_index() == 0;
                let presentation = (self.dialog_generation, independent);
                if self.observed_presentation != Some(presentation) {
                    self.observed_presentation = Some(presentation);
                    self.native_jump_mode.cancel_all();
                    self.native_jump_frame = None;
                }
                if independent && focus {
                    child.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                if first_pass {
                    self.bootstrap_pending = false;
                }
                let child_ctx = child.ctx().clone();
                self.ensure_session_start(&child_ctx);
                if independent {
                    let close = self.show_immediate_viewport(
                        child,
                        parent_instance,
                        application_input_allowed,
                        jump_mode_enabled,
                        &mut intents,
                    );
                    intents.close |= close;
                } else {
                    let size = child.input(|input| input.content_rect().size());
                    let available = (size - egui::vec2(24.0, 48.0)).max(egui::vec2(1.0, 1.0));
                    let mut open = self.open;
                    let current_top_layer =
                        crate::gui::jump_mode::ui::visible_top_middle_layer(&child_ctx);
                    let root_parent_current = root_native_application_allowed
                        && current_top_layer == Some(xml_toolkit_embedded_layer_id());
                    let confirmation_identity = self.jump_mode_pending_overwrite_identity();
                    let root_confirmation_current = root_native_application_allowed
                        && confirmation_identity.is_some_and(|confirmation| {
                            current_top_layer
                                == Some(
                                    crate::gui::jump_mode::ui::xml_overwrite_confirmation_layer_id(
                                        parent_instance.unwrap_or(self.dialog_generation),
                                        confirmation,
                                    ),
                                )
                        });
                    let keyboard_close = first_pass
                        && root_parent_current
                        && !egui::Popup::is_any_open(&child_ctx)
                        && child.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    if keyboard_close {
                        intents.close = true;
                    }
                    egui::Window::new("XML Toolkit")
                        .id(egui::Id::new("xml_toolkit_embedded_window"))
                        .open(&mut open)
                        .collapsible(false)
                        .resizable(true)
                        .default_size(available.min(egui::vec2(1120.0, 760.0)))
                        .min_size(available.min(egui::vec2(480.0, 340.0)))
                        .max_size(available)
                        .show(child, |ui| {
                            ui.add_enabled_ui(root_parent_current, |ui| {
                                self.show_body(
                                    ui,
                                    &child_ctx,
                                    first_pass && root_parent_current && !intents.close,
                                    root_parent_current,
                                    root_scope.filter(|_| {
                                        matches!(
                                            root_owner,
                                            NavigationOwner::Dialog {
                                                kind: NavigationDialogKind::XmlToolkit,
                                                ..
                                            }
                                        )
                                    }),
                                    &mut intents,
                                    parent_instance,
                                    root_owner,
                                    root_confirmation_current,
                                    root_scope.filter(|_| root_confirmation_current),
                                )
                            })
                        });
                    if root_parent_current {
                        self.open = open;
                    }
                    if root_parent_current && !open && first_pass {
                        intents.close = true;
                    }
                }
            },
        );
        self.focus_viewport = false;
        if viewport_seen {
            self.process_intents(ctx, intents);
        }
        self.poll_file_worker();
        self.poll_session(ctx);
        self.request_repaint_if_pending(ctx);
    }

    fn show_immediate_viewport(
        &mut self,
        child: &mut egui::Ui,
        parent_instance: Option<u64>,
        application_input_allowed: bool,
        jump_mode_enabled: bool,
        intents: &mut UiIntents,
    ) -> bool {
        let ctx = child.ctx().clone();
        let viewport = viewport_id();
        let confirmation_identity = self.jump_mode_pending_overwrite_identity();
        let layer_parent_instance = parent_instance.unwrap_or(self.dialog_generation);
        let owner = parent_instance.map(|parent_instance| {
            confirmation_identity.map_or(
                NavigationOwner::Dialog {
                    kind: NavigationDialogKind::XmlToolkit,
                    instance: parent_instance,
                },
                |confirmation_instance| NavigationOwner::XmlOverwriteConfirmation {
                    parent_instance,
                    confirmation_instance,
                },
            )
        });
        let binding = owner.map(|owner| NavigationViewportBinding {
            viewport,
            owner,
            base_layer: match owner {
                NavigationOwner::XmlOverwriteConfirmation {
                    parent_instance,
                    confirmation_instance,
                } => crate::gui::jump_mode::ui::xml_overwrite_confirmation_layer_id(
                    parent_instance,
                    confirmation_instance,
                ),
                _ => egui::LayerId::background(),
            },
        });
        self.native_jump_mode.set_enabled(jump_mode_enabled);
        let frame_number = ctx.cumulative_frame_nr();
        if self.native_jump_frame != Some(frame_number) {
            self.native_jump_mode.begin_frame();
            self.native_jump_frame = Some(frame_number);
        }
        let navigation_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                application_input_allowed && jump_mode_enabled,
            )
        });
        let jump_owner_current = binding.is_some_and(|binding| {
            navigation_owner == binding.owner
                && self.native_jump_mode.frame_navigation_owner() == binding.owner
        });
        self.native_jump_mode.handle_viewport_input(
            &ctx,
            jump_owner_current,
            ctx.current_pass_index() == 0,
            viewport,
        );

        let visible_layer = crate::gui::jump_mode::ui::visible_top_middle_layer(&ctx);
        let confirmation_layer = confirmation_identity.map(|confirmation| {
            crate::gui::jump_mode::ui::xml_overwrite_confirmation_layer_id(
                layer_parent_instance,
                confirmation,
            )
        });
        let expected_layer = confirmation_layer
            .or_else(|| binding.map(|binding| binding.base_layer))
            .unwrap_or_else(egui::LayerId::background);
        let popup_open = egui::Popup::is_any_open(&ctx);
        let native_owner_current = application_input_allowed
            && ctx.viewport_id() == viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && if confirmation_identity.is_some() {
                visible_layer == Some(expected_layer)
            } else {
                visible_layer.is_none()
            }
            && !popup_open
            && crate::gui::jump_mode::ui::focused_response_layer(&ctx)
                .is_none_or(|layer| expected_layer == layer);
        let first_pass = ctx.current_pass_index() == 0;
        let parent_owner_current = native_owner_current && confirmation_identity.is_none();
        let confirmation_owner_current = native_owner_current && confirmation_identity.is_some();
        let native_close_requested =
            first_pass && child.input(|input| input.viewport().close_requested());
        let keyboard_close = first_pass
            && parent_owner_current
            && !popup_open
            && child.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
            });
        if first_pass && confirmation_owner_current && !popup_open {
            self.consume_overwrite_cancel_shortcut(&ctx);
        }
        let close = native_close_requested || keyboard_close;
        if close {
            self.native_jump_mode.cancel_all();
            self.native_jump_frame = None;
        }

        self.native_jump_mode.begin_ui_pass();
        let scope = if jump_owner_current {
            binding.and_then(|binding| match binding.owner {
                NavigationOwner::Dialog {
                    kind: NavigationDialogKind::XmlToolkit,
                    instance,
                } => self.native_jump_mode.scope_for_viewport_binding(
                    SurfaceId::dialog("xml-toolkit", instance),
                    TargetGeneration(self.dialog_generation),
                    "xml-toolkit/form",
                    binding,
                ),
                NavigationOwner::XmlOverwriteConfirmation {
                    parent_instance,
                    confirmation_instance,
                } => self.native_jump_mode.scope_for_viewport_binding(
                    SurfaceId::popup(
                        "xml-overwrite-confirmation",
                        confirmation_instance,
                        SurfaceId::dialog("xml-toolkit", parent_instance),
                    ),
                    TargetGeneration(confirmation_instance),
                    "xml-toolkit/overwrite",
                    binding,
                ),
                _ => None,
            })
        } else {
            None
        };
        let (parent_scope, confirmation_scope) = match binding.map(|binding| binding.owner) {
            Some(NavigationOwner::Dialog {
                kind: NavigationDialogKind::XmlToolkit,
                ..
            }) => (scope.as_ref(), None),
            Some(NavigationOwner::XmlOverwriteConfirmation { .. }) => (None, scope.as_ref()),
            _ => (None, None),
        };
        if !parent_owner_current && !confirmation_owner_current {
            self.surrender_xml_focus(&ctx, Some(layer_parent_instance));
        }
        egui::CentralPanel::default().show(child, |ui| {
            ui.add_enabled_ui(parent_owner_current, |ui| {
                self.show_body(
                    ui,
                    &ctx,
                    first_pass && parent_owner_current && !close,
                    parent_owner_current,
                    parent_scope,
                    intents,
                    parent_instance,
                    binding.map_or(NavigationOwner::Blocked, |binding| binding.owner),
                    confirmation_owner_current,
                    confirmation_scope,
                )
            });
        });
        self.native_jump_mode.finish_ui_pass();
        let after_owner = binding.map_or(NavigationOwner::Blocked, |binding| {
            self.native_jump_mode.update_navigation_owner_for_viewport(
                &ctx,
                binding,
                application_input_allowed && jump_mode_enabled,
            )
        });
        if let Some(binding) = binding {
            if self.native_jump_mode.prepare_pass_for_viewport(
                &ctx,
                binding,
                after_owner,
                application_input_allowed,
            ) {
                crate::gui::jump_mode::paint::paint(&ctx, &self.native_jump_mode);
            }
            if !ctx.will_discard() {
                self.native_jump_mode.complete_frame_for_viewport(
                    &ctx,
                    binding,
                    after_owner,
                    application_input_allowed,
                );
            }
        } else if !ctx.will_discard() {
            self.native_jump_mode.cancel_all();
        }
        if close {
            child.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        close
    }

    fn surrender_xml_focus(&self, ctx: &egui::Context, parent_instance: Option<u64>) {
        if ctx.viewport_id() != egui::ViewportId::ROOT && ctx.viewport_id() != viewport_id() {
            return;
        }
        let xml_layer = if ctx.viewport_id() == viewport_id() {
            egui::LayerId::background()
        } else {
            xml_toolkit_embedded_layer_id()
        };
        let confirmation_layer =
            self.jump_mode_pending_overwrite_identity()
                .and_then(|confirmation| {
                    parent_instance.map(|parent_instance| {
                        crate::gui::jump_mode::ui::xml_overwrite_confirmation_layer_id(
                            parent_instance,
                            confirmation,
                        )
                    })
                });
        if let Some(focused) = ctx.memory(|memory| memory.focused())
            && ctx.read_response(focused).is_some_and(|response| {
                response.layer_id == xml_layer || Some(response.layer_id) == confirmation_layer
            })
        {
            ctx.memory_mut(|memory| memory.surrender_focus(focused));
        }
    }

    fn consume_overwrite_cancel_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
        }) {
            self.cancel_pending_overwrite();
        }
    }

    fn cancel_pending_overwrite(&mut self) {
        self.pending_save = None;
        self.feedback = Some(Ok("Save cancelled.".into()));
    }

    fn show_body(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        process_actions: bool,
        _native_ui_owner_current: bool,
        scope: Option<&JumpModeScope>,
        intents: &mut UiIntents,
        parent_instance: Option<u64>,
        _owner: NavigationOwner,
        confirmation_owner_current: bool,
        confirmation_scope: Option<&JumpModeScope>,
    ) {
        let mut targets = XmlJumpTargets::new(scope);
        let snapshot = self.current_snapshot();
        self.tree.sync(&snapshot);
        self.xpath.sync(&snapshot);

        ui.horizontal_wrapped(|ui| {
            let open = ui.button("Open XML…");
            targets.button(ui, &open, "chrome/open-xml", "Open XML file");
            if open.clicked() && process_actions {
                intents.open_xml = true;
            }
            let paste = ui.button("Paste Clipboard");
            targets.button(ui, &paste, "chrome/paste-clipboard", "Paste XML text");
            if paste.clicked() && process_actions {
                intents.paste_clipboard = true;
            }
            let copy_source = ui.button("Copy Source");
            targets.button(
                ui,
                &copy_source,
                format!("document/{}/copy-source", self.source_generation),
                "Copy XML source",
            );
            if copy_source.clicked() && process_actions {
                intents.copy_source = true;
            }
            let clear_source = ui.button("Clear Source");
            targets.button(
                ui,
                &clear_source,
                format!("document/{}/clear-source", self.source_generation),
                "Clear XML source",
            );
            if clear_source.clicked() && process_actions {
                intents.clear_source = true;
            }
            let output = current_output(&snapshot);
            let copy_output = ui.add_enabled(output.is_some(), egui::Button::new("Copy Output"));
            targets.button(
                ui,
                &copy_output,
                format!("document/{}/copy-output", self.source_generation),
                "Copy formatted XML output",
            );
            if copy_output.clicked() && process_actions {
                intents.copy_output = true;
            }
            for (label, mode) in [
                ("Format", XmlFormatMode::Pretty),
                ("Minify", XmlFormatMode::Minify),
            ] {
                let response = ui.add_enabled(
                    snapshot.parse == XmlParseStatus::Ready,
                    egui::Button::new(label),
                );
                targets.button(
                    ui,
                    &response,
                    format!("document/{}/format/{mode:?}", self.source_generation),
                    label,
                );
                if response.clicked() && process_actions {
                    intents.format = Some(mode);
                }
            }
            let output_text = output.map(|output| output.text.as_str());
            let can_apply = output_text.is_some_and(|text| text.len() <= MAX_XML_INPUT_BYTES);
            let apply_output = ui
                .add_enabled(can_apply, egui::Button::new("Apply Output to Source"))
                .on_hover_text(
                    if output_text.is_some_and(|text| text.len() > MAX_XML_INPUT_BYTES) {
                        "Output exceeds the 4 MiB source limit. Copy or save it instead."
                    } else {
                        "Replace the editor only when you choose to apply this output."
                    },
                );
            targets.button(
                ui,
                &apply_output,
                format!("document/{}/apply-output", self.source_generation),
                "Apply XML output to source",
            );
            if apply_output.clicked() && process_actions {
                intents.apply_output = true;
            }
            let save_source = ui.button("Save Source As…");
            targets.button(
                ui,
                &save_source,
                format!("document/{}/save-source", self.source_generation),
                "Save XML source as",
            );
            if save_source.clicked() && process_actions {
                intents.save_source = true;
            }
            let save_output =
                ui.add_enabled(output.is_some(), egui::Button::new("Save Output As…"));
            targets.button(
                ui,
                &save_output,
                format!("document/{}/save-output", self.source_generation),
                "Save XML output as",
            );
            if save_output.clicked() && process_actions {
                intents.save_output = true;
            }
            let close = ui.button("Close");
            targets.button(ui, &close, "chrome/close", "Close XML Toolkit");
            if close.clicked() && process_actions {
                intents.close = true;
            }
        });

        self.show_feedback(ui);
        self.show_parse_status(ui, &snapshot);
        if let Some(output) = current_output(&snapshot)
            && output.text.len() > MAX_XML_INPUT_BYTES
        {
            ui.weak(format!(
                "This output is {} bytes and exceeds the 4 MiB source limit. Copy and Save Output remain available; Apply is disabled.",
                output.text.len()
            ));
        }
        if matches!(&self.session, SessionLifecycle::Starting { .. }) {
            ui.weak("Starting the secure XML worker…");
        } else if let SessionLifecycle::Failed(error) = &self.session {
            ui.colored_label(ui.visuals().error_fg_color, error);
        } else if matches!(&self.session, SessionLifecycle::Closing(_)) {
            ui.weak("The previous XML worker is closing before a new session can start…");
        }

        ui.horizontal_wrapped(|ui| {
            let mut selected_tab = self.tab;
            for (tab, title) in [
                (XmlToolkitTab::Source, "Source"),
                (XmlToolkitTab::Tree, "Tree"),
                (XmlToolkitTab::XPath, "XPath"),
                (XmlToolkitTab::Validation, "Validation"),
                (XmlToolkitTab::Output, "Output"),
            ] {
                let response = ui.selectable_value(&mut selected_tab, tab, title);
                targets.selectable(
                    ui,
                    &response,
                    format!("tab/{tab:?}"),
                    "XML Toolkit view",
                    TargetKind::Tab,
                );
            }
            if process_actions {
                self.tab = selected_tab;
            }
        });
        ui.separator();
        match self.tab {
            XmlToolkitTab::Source => {
                let size = ui.available_size();
                egui::ScrollArea::both()
                    .id_salt("xml_toolkit_source_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut source_draft = self.source.clone();
                        let response = ui.add_sized(
                            size,
                            egui::TextEdit::multiline(&mut source_draft)
                                .id(egui::Id::new("xml_toolkit_source_editor"))
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(14)
                                .hint_text("Paste or type XML here"),
                        );
                        targets.text_entry(ui, &response, "source/editor", "XML source text");
                        if process_actions && response.changed() {
                            self.source = source_draft;
                            intents.edit_source = true;
                        }
                    });
                ui.weak(format!(
                    "Source input is limited to {MAX_XML_INPUT_BYTES} UTF-8 bytes. XML is parsed offline with DTD and external entity resolution disabled."
                ));
            }
            XmlToolkitTab::Tree => {
                if process_actions {
                    intents.tree = self.tree.show_scoped(ui, &snapshot, &mut targets, true);
                } else {
                    let mut tree = self.tree.clone();
                    let _ = tree.show_scoped(ui, &snapshot, &mut targets, false);
                }
            }
            XmlToolkitTab::XPath => {
                if process_actions {
                    intents.xpath =
                        self.xpath
                            .show_scoped(ui, &snapshot, &mut self.tree, &mut targets, true);
                } else {
                    let mut xpath = self.xpath.clone();
                    let mut tree = self.tree.clone();
                    let _ = xpath.show_scoped(ui, &snapshot, &mut tree, &mut targets, false);
                }
            }
            XmlToolkitTab::Validation => {
                self.schema
                    .show_scoped(ui, &snapshot, &mut targets, process_actions);
            }
            XmlToolkitTab::Output => self.show_output(ui, &snapshot),
        }

        self.show_overwrite_confirmation_scoped(
            ctx,
            confirmation_scope,
            confirmation_owner_current,
            parent_instance.unwrap_or(self.dialog_generation),
        );

        if self
            .file_worker
            .as_ref()
            .is_some_and(XmlFileWorker::is_pending)
        {
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
    }

    fn show_overwrite_confirmation(&mut self, ctx: &egui::Context, _process_actions: bool) {
        self.show_overwrite_confirmation_scoped(ctx, None, false, self.dialog_generation);
    }

    fn show_overwrite_confirmation_scoped(
        &mut self,
        ctx: &egui::Context,
        scope: Option<&JumpModeScope>,
        confirmation_owner_current: bool,
        parent_instance: u64,
    ) {
        let Some((confirmation_instance, path)) = self
            .pending_save
            .as_ref()
            .filter(|pending| pending.needs_confirmation)
            .map(|pending| (pending.id, pending.path.clone()))
        else {
            return;
        };

        let mut open = true;
        let mut replace_clicked = false;
        let mut cancel_clicked = false;
        let mut targets = XmlJumpTargets::new(scope);
        let first_pass = ctx.current_pass_index() == 0;
        if first_pass && confirmation_owner_current {
            self.consume_overwrite_cancel_shortcut(ctx);
        }
        #[cfg(test)]
        let mut button_rects = [egui::Rect::NOTHING; 2];
        egui::Window::new("Confirm XML file overwrite")
            .id(
                crate::gui::jump_mode::ui::xml_overwrite_confirmation_layer_id(
                    parent_instance,
                    confirmation_instance,
                )
                .id,
            )
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(confirmation_owner_current, |ui| {
                    ui.label(format!("{} already exists.", path.display()));
                    ui.label("Replace it with the selected UTF-8 XML payload?");
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let replace = ui.button("Replace existing file");
                        targets.button(
                            ui,
                            &replace,
                            "confirm-replace",
                            "Replace existing XML file",
                        );
                        #[cfg(test)]
                        {
                            button_rects[0] = replace.rect;
                        }
                        replace_clicked =
                            replace.clicked() && confirmation_owner_current && first_pass;
                        let cancel = ui.button("Cancel Save");
                        targets.button(ui, &cancel, "cancel-save", "Cancel XML save");
                        #[cfg(test)]
                        {
                            button_rects[1] = cancel.rect;
                        }
                        cancel_clicked =
                            cancel.clicked() && confirmation_owner_current && first_pass;
                    });
                });
            });

        #[cfg(test)]
        {
            self.overwrite_button_rects = button_rects;
        }
        if replace_clicked {
            self.confirm_overwrite(ctx);
        } else if (cancel_clicked || !open) && confirmation_owner_current && first_pass {
            self.cancel_pending_overwrite();
        }
    }

    fn show_feedback(&self, ui: &mut egui::Ui) {
        if let Some(feedback) = &self.feedback {
            match feedback {
                Ok(message) => {
                    ui.label(message);
                }
                Err(message) => {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
            }
        }
    }

    fn show_parse_status(&self, ui: &mut egui::Ui, snapshot: &SessionSnapshot) {
        match &snapshot.parse {
            XmlParseStatus::Empty => ui.weak("No XML source is loaded."),
            XmlParseStatus::Pending => ui.weak("Parsing the current XML source…"),
            XmlParseStatus::Ready => ui.label("XML is well formed."),
            XmlParseStatus::Failed(error) => ui.colored_label(
                ui.visuals().error_fg_color,
                format!("XML parse error: {}", xml_error_message(error)),
            ),
        };
    }

    fn show_output(&self, ui: &mut egui::Ui, snapshot: &SessionSnapshot) {
        match &snapshot.output {
            XmlOutputStatus::Empty => {
                ui.weak("Format or minify the current parsed source to create output.");
            }
            XmlOutputStatus::Pending(mode) => {
                ui.weak(match mode {
                    XmlFormatMode::Pretty => "Formatting the current XML…",
                    XmlFormatMode::Minify => "Minifying the current XML…",
                });
            }
            XmlOutputStatus::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("XML output failed: {}", xml_error_message(error)),
                );
            }
            XmlOutputStatus::Ready(output) => {
                ui.label(format!("{} UTF-8 bytes", output.text.len()));
                let preview = bounded_prefix(&output.text, MAX_OUTPUT_PREVIEW_CHARS);
                let mut preview = preview.to_owned();
                egui::ScrollArea::both()
                    .id_salt("xml_toolkit_output_preview")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut preview)
                                .font(egui::TextStyle::Monospace)
                                .interactive(false),
                        );
                    });
                if preview.len() < output.text.len() {
                    ui.weak("Output preview is clipped. Copy or save the full output explicitly.");
                }
                ui.weak(
                    "Format and Minify keep this output separate until you explicitly apply it.",
                );
            }
        };
    }

    fn process_intents(&mut self, ctx: &egui::Context, intents: UiIntents) {
        if intents.close {
            self.end_session();
            return;
        }

        if intents.open_xml
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("XML document", &["xml", "xsl", "xslt"])
                .pick_file()
        {
            self.request_load(ctx, path);
        }

        let schema_intent = self.schema.take_intent();
        if matches!(
            schema_intent,
            Some(crate::gui::xml_toolkit_schema::XmlToolkitSchemaIntent::ChooseSchema)
        ) && let Some(path) = rfd::FileDialog::new()
            .add_filter("XML Schema", &["xsd"])
            .pick_file()
        {
            self.select_schema(path);
        }

        let mut replacement = None;
        if intents.paste_clipboard {
            replacement = Some(self.read_clipboard_source());
        } else if intents.clear_source {
            replacement = Some(Ok(String::new()));
        } else if intents.apply_output {
            replacement = Some(output_text_for_apply(&self.current_snapshot()));
        } else if intents.edit_source {
            replacement = Some(Ok(self.source.clone()));
        }

        if let Some(replacement) = replacement {
            match replacement {
                Ok(source) => self.accept_source(source),
                Err(error) => self.feedback = Some(Err(error)),
            }
        }

        if intents.copy_source {
            let source = self.source.clone();
            self.copy_text(&source, "Copied authored XML source.");
        }
        if let (Some(mode), Some(revision)) = (intents.format, self.current_snapshot().revision) {
            let result = match (&self.session, self.accepted_source_generation) {
                (SessionLifecycle::Active(session), Some(generation))
                    if generation == self.source_generation =>
                {
                    Some(session.format(revision, mode))
                }
                _ => None,
            };
            if let Some(result) = result {
                match result {
                    Ok(()) => {
                        self.feedback = Some(Ok(match mode {
                            XmlFormatMode::Pretty => "Formatting current XML…".into(),
                            XmlFormatMode::Minify => "Minifying current XML…".into(),
                        }))
                    }
                    Err(error) => {
                        self.feedback = Some(Err(format!("Could not format XML: {error:?}")))
                    }
                }
            }
        }
        if intents.copy_output {
            match self.current_output_text() {
                Ok(output) => self.copy_text(&output, "Copied current XML output."),
                Err(error) => self.feedback = Some(Err(error)),
            }
        }
        if intents.save_source {
            self.choose_save_path(ctx, Arc::<str>::from(self.source.as_str()));
        }
        if intents.save_output {
            match self.current_output_text() {
                Ok(output) => self.choose_save_path(ctx, Arc::<str>::from(output)),
                Err(error) => self.feedback = Some(Err(error)),
            }
        }

        let _scalar_copy_intent_committed = intents.xpath.scalar_copy.is_some();
        let mut action_errors = Vec::new();
        let mut completed_copies = Vec::new();
        let active_session = match (&self.session, self.accepted_source_generation) {
            (SessionLifecycle::Active(session), Some(generation))
                if generation == self.source_generation =>
            {
                Some(session)
            }
            _ => None,
        };
        if let Some(session) = active_session {
            if let Some(revision) = intents.xpath.invalidate
                && let Err(error) = session.invalidate_xpath(revision)
            {
                action_errors.push(format!(
                    "Could not invalidate stale XPath results: {error:?}"
                ));
            }
            for tree_intents in [intents.tree, intents.xpath.tree] {
                if let Some(detail) = tree_intents.detail
                    && let Err(error) = session.request_detail(detail)
                {
                    action_errors.push(format!(
                        "Could not inspect the selected XML node: {error:?}"
                    ));
                }
            }
            if let Some(request) = intents.xpath.query
                && let Err(error) = session.request_xpath(request)
            {
                action_errors.push(format!("Could not run XPath: {error:?}"));
            }
            if let Some(text) = self
                .xpath
                .take_valid_scalar_copy_completion(&session.snapshot())
            {
                completed_copies.push((text.to_string(), "Copied current XPath scalar text."));
            }
            if let Some(crate::gui::xml_toolkit_schema::XmlToolkitSchemaIntent::ClearSchema) =
                schema_intent
                && let Err(error) = session.clear_schema_path()
            {
                action_errors.push(format!("Could not clear the selected XSD: {error:?}"));
            }
            if let Some(crate::gui::xml_toolkit_schema::XmlToolkitSchemaIntent::Validate) =
                schema_intent
                && let Err(error) = session.validate_selected_schema()
            {
                action_errors.push(format!("Could not validate XML: {error:?}"));
            }
            let snapshot = session.snapshot();
            if let Some(output) = self.tree.take_valid_copy_completion(&snapshot) {
                completed_copies.push((output.value.clone(), "Copied the selected XML value."));
            }
            self.schema_path_draft = snapshot
                .schema
                .selected_path
                .as_ref()
                .map(|path| path.as_path().to_path_buf());
        }
        if let Some(error) = action_errors.into_iter().next() {
            self.feedback = Some(Err(error));
        }
        for (text, label) in completed_copies {
            self.copy_text(&text, label);
        }
    }

    fn read_clipboard_source(&self) -> Result<String, String> {
        let source = self.clipboard.read_text().map_err(|error| match error {
            crate::clipboard_modify::clipboard::ClipboardError::NonText => {
                "Clipboard does not contain Unicode text.".to_owned()
            }
            error => format!("Could not read clipboard: {error}"),
        })?;
        if source.len() > MAX_XML_INPUT_BYTES {
            return Err(format!(
                "Clipboard XML exceeds the {MAX_XML_INPUT_BYTES}-byte source limit."
            ));
        }
        validate_utf8_declaration(&source)?;
        Ok(source)
    }

    fn accept_source(&mut self, source: String) {
        self.source = source;
        self.source_generation = self.source_generation.wrapping_add(1);
        self.accepted_source_generation = None;
        self.pending_save = None;
        self.feedback = None;
        let update = match &self.session {
            SessionLifecycle::Active(session) => Some((
                session.replace_source(self.source.clone()),
                session.snapshot(),
            )),
            _ => None,
        };
        if let Some((result, snapshot)) = update {
            if let Err(error) = result {
                self.feedback = Some(Err(format!("Could not update XML source: {error:?}")));
                self.sync_views(&SessionSnapshot::default());
            } else {
                self.accepted_source_generation = Some(self.source_generation);
                self.sync_views(&snapshot);
            }
        } else {
            self.sync_views(&SessionSnapshot::default());
        }
    }

    fn select_schema(&mut self, path: PathBuf) {
        if let Err(error) = validate_local_path(&path) {
            self.feedback = Some(Err(error));
            return;
        }
        let result = match (&self.session, self.accepted_source_generation) {
            (SessionLifecycle::Active(session), Some(generation))
                if generation == self.source_generation =>
            {
                Some(session.select_schema_path(path.clone()))
            }
            _ => None,
        };
        let Some(result) = result else {
            self.feedback = Some(Err("The XML worker is not ready to select an XSD.".into()));
            return;
        };
        match result {
            Ok(_) => {
                self.schema_path_draft = Some(path);
                self.feedback = None;
            }
            Err(error) => self.feedback = Some(Err(format!("Could not select XSD: {error:?}"))),
        }
    }

    fn request_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Err(error) = validate_local_path(&path) {
            self.feedback = Some(Err(error));
            return;
        }
        let id = self.next_file_request;
        let Some(next_id) = id.checked_add(1) else {
            self.feedback = Some(Err("XML file request identifiers are exhausted.".into()));
            return;
        };
        self.next_file_request = next_id;
        let source_generation = self.source_generation;
        let dialog_generation = self.dialog_generation;
        let worker = match self.ensure_file_worker(ctx) {
            Ok(worker) => worker,
            Err(error) => {
                self.feedback = Some(Err(error));
                return;
            }
        };
        match worker.submit(
            id,
            FileJob::Load {
                id,
                source_generation,
                dialog_generation,
                path,
            },
        ) {
            Ok(()) => self.feedback = Some(Ok("Loading selected XML file…".into())),
            Err(error) => self.feedback = Some(Err(error)),
        }
    }

    fn choose_save_path(&mut self, ctx: &egui::Context, payload: Arc<str>) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("XML document", &["xml"])
            .save_file()
        else {
            return;
        };
        self.submit_save(ctx, path, payload, false);
    }

    fn submit_save(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        payload: Arc<str>,
        overwrite_confirmed: bool,
    ) {
        if let Err(error) = validate_local_path(&path) {
            self.feedback = Some(Err(error));
            return;
        }
        let id = self.next_file_request;
        let Some(next_id) = id.checked_add(1) else {
            self.feedback = Some(Err("XML file request identifiers are exhausted.".into()));
            return;
        };
        self.next_file_request = next_id;
        let dialog_generation = self.dialog_generation;
        let pending = PendingSave {
            id,
            dialog_generation,
            path: path.clone(),
            payload: Arc::clone(&payload),
            needs_confirmation: false,
        };
        let worker = match self.ensure_file_worker(ctx) {
            Ok(worker) => worker,
            Err(error) => {
                self.feedback = Some(Err(error));
                return;
            }
        };
        match worker.submit(
            id,
            FileJob::Save {
                id,
                dialog_generation,
                path,
                payload,
                overwrite_confirmed,
            },
        ) {
            Ok(()) => {
                self.pending_save = Some(pending);
                self.feedback = Some(Ok("Saving the selected XML payload…".into()));
            }
            Err(error) => self.feedback = Some(Err(error)),
        }
    }

    fn confirm_overwrite(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_save.take() else {
            return;
        };
        if !pending.needs_confirmation
            || pending.dialog_generation != self.dialog_generation
            || !self.open
        {
            return;
        }
        self.submit_save(ctx, pending.path, pending.payload, true);
    }

    fn ensure_file_worker(&mut self, ctx: &egui::Context) -> Result<&mut XmlFileWorker, String> {
        if self.file_worker.is_none() {
            self.file_worker = Some(XmlFileWorker::start(ctx)?);
        }
        self.file_worker
            .as_mut()
            .ok_or_else(|| "The XML file worker is unavailable.".into())
    }

    fn poll_file_worker(&mut self) {
        let completion = self.file_worker.as_mut().and_then(XmlFileWorker::poll);
        let Some(completion) = completion else {
            return;
        };
        self.apply_file_completion(completion);
    }

    fn apply_file_completion(&mut self, completion: FileCompletion) {
        if !self.open || completion.dialog_generation != self.dialog_generation {
            return;
        }
        match completion.result {
            FileResult::Loaded(result) => {
                let current_request = completion.source_generation == Some(self.source_generation);
                if !current_request {
                    return;
                }
                match result {
                    Ok(source) => {
                        self.accept_source(source);
                        self.feedback = Some(Ok(format!("Loaded {}.", completion.path.display())));
                        self.tab = XmlToolkitTab::Source;
                    }
                    Err(error) => self.feedback = Some(Err(error)),
                }
            }
            FileResult::Saved(result) => {
                let is_current = self.pending_save.as_ref().is_some_and(|pending| {
                    pending.id == completion.id
                        && pending.dialog_generation == self.dialog_generation
                });
                if !is_current {
                    return;
                }
                match result {
                    Ok(SaveOutcome::OverwriteRequired) => {
                        if let Some(pending) = self.pending_save.as_mut() {
                            pending.needs_confirmation = true;
                        }
                        self.feedback =
                            Some(Ok("Confirm replacement before this file is changed.".into()));
                    }
                    Ok(SaveOutcome::Saved) => {
                        self.pending_save = None;
                        self.feedback = Some(Ok(format!("Saved {}.", completion.path.display())));
                    }
                    Err(error) => {
                        self.pending_save = None;
                        self.feedback = Some(Err(error));
                    }
                }
            }
        }
    }

    fn copy_text(&mut self, text: &str, success: &'static str) {
        self.feedback = Some(
            self.clipboard
                .write_text(text)
                .map(|()| success.to_owned())
                .map_err(|error| format!("Could not copy XML: {error}")),
        );
    }

    fn current_output_text(&self) -> Result<String, String> {
        output_text_for_explicit_use(&self.current_snapshot())
    }

    fn current_snapshot(&self) -> Arc<SessionSnapshot> {
        match (&self.session, self.accepted_source_generation) {
            (SessionLifecycle::Active(session), Some(generation))
                if generation == self.source_generation =>
            {
                session.snapshot()
            }
            _ => Arc::new(SessionSnapshot::default()),
        }
    }

    fn active_session(&self) -> Option<&XmlEngineSession> {
        match (&self.session, self.accepted_source_generation) {
            (SessionLifecycle::Active(session), Some(generation))
                if generation == self.source_generation =>
            {
                Some(session)
            }
            _ => None,
        }
    }

    fn sync_active_views(&mut self, session: &XmlEngineSession) {
        let snapshot = session.snapshot();
        self.sync_views(&snapshot);
    }

    fn sync_views(&mut self, snapshot: &SessionSnapshot) {
        self.tree.sync(snapshot);
        self.xpath.sync(snapshot);
    }

    fn ensure_session_start(&mut self, ctx: &egui::Context) {
        if !self.open || !matches!(&self.session, SessionLifecycle::Idle) {
            return;
        }
        let repaint_context = ctx.clone();
        match thread::Builder::new()
            .name("xml-toolkit-session-start".to_owned())
            .spawn(move || {
                let result = XmlEngineSession::start();
                repaint_context.request_repaint();
                result
            }) {
            Ok(join) => {
                self.session = SessionLifecycle::Starting {
                    join,
                    cancel_on_ready: false,
                };
            }
            Err(error) => {
                self.session =
                    SessionLifecycle::Failed(format!("Could not start the XML session: {error}"));
            }
        }
    }

    fn poll_session(&mut self, ctx: &egui::Context) {
        let lifecycle = std::mem::replace(&mut self.session, SessionLifecycle::Idle);
        self.session = match lifecycle {
            SessionLifecycle::Starting {
                join,
                cancel_on_ready,
            } if join.is_finished() => match join.join() {
                Ok(Ok(session)) if self.open && !cancel_on_ready => {
                    match session.replace_source(self.source.clone()) {
                        Ok(_) => self.accepted_source_generation = Some(self.source_generation),
                        Err(error) => {
                            self.accepted_source_generation = None;
                            self.feedback =
                                Some(Err(format!("Could not load XML source: {error:?}")));
                        }
                    }
                    if let Some(path) = self.schema_path_draft.clone()
                        && let Err(error) = session.select_schema_path(path)
                    {
                        self.feedback =
                            Some(Err(format!("Could not restore selected XSD: {error:?}")));
                    }
                    if self.accepted_source_generation == Some(self.source_generation) {
                        self.sync_active_views(&session);
                    } else {
                        self.sync_views(&SessionSnapshot::default());
                    }
                    SessionLifecycle::Active(session)
                }
                Ok(Ok(session)) => {
                    self.accepted_source_generation = None;
                    session.request_shutdown();
                    SessionLifecycle::Closing(session)
                }
                Ok(Err(error)) => {
                    self.accepted_source_generation = None;
                    if self.open {
                        self.feedback = Some(Err(format!(
                            "Could not start secure XML session: {error:?}"
                        )));
                        SessionLifecycle::Failed(format!("XML session unavailable: {error:?}"))
                    } else {
                        SessionLifecycle::Idle
                    }
                }
                Err(_) => {
                    self.accepted_source_generation = None;
                    if self.open {
                        SessionLifecycle::Failed(
                            "The XML session startup worker stopped unexpectedly.".into(),
                        )
                    } else {
                        SessionLifecycle::Idle
                    }
                }
            },
            SessionLifecycle::Starting {
                join,
                cancel_on_ready,
            } => {
                ctx.request_repaint_after(Duration::from_millis(40));
                SessionLifecycle::Starting {
                    join,
                    cancel_on_ready,
                }
            }
            SessionLifecycle::Active(session) => {
                if self.accepted_source_generation == Some(self.source_generation) {
                    let snapshot = session.snapshot();
                    self.sync_views(&snapshot);
                    if session_pending(&snapshot) {
                        ctx.request_repaint_after(Duration::from_millis(40));
                    }
                } else {
                    self.sync_views(&SessionSnapshot::default());
                }
                SessionLifecycle::Active(session)
            }
            SessionLifecycle::Closing(session) => match session.try_reap() {
                Ok(true) => {
                    self.accepted_source_generation = None;
                    self.sync_views(&SessionSnapshot::default());
                    ctx.request_repaint();
                    SessionLifecycle::Idle
                }
                Ok(false) => {
                    ctx.request_repaint_after(Duration::from_millis(40));
                    SessionLifecycle::Closing(session)
                }
                Err(error) => {
                    self.accepted_source_generation = None;
                    self.sync_views(&SessionSnapshot::default());
                    self.feedback = Some(Err(format!("XML worker shutdown failed: {error:?}")));
                    SessionLifecycle::Idle
                }
            },
            other => other,
        };
    }

    /// Reap owners while the dialog is closed from the existing root runtime
    /// tick. A bounded repaint keeps the cleanup tick alive only while an
    /// owner or file operation is still pending; completion cannot publish
    /// state into a future dialog generation.
    pub(crate) fn poll_closed_background(&mut self, ctx: &egui::Context) {
        if self.open {
            return;
        }
        self.poll_file_worker();
        let lifecycle = std::mem::replace(&mut self.session, SessionLifecycle::Idle);
        self.session = match lifecycle {
            SessionLifecycle::Starting { join, .. } if join.is_finished() => match join.join() {
                Ok(Ok(session)) => {
                    self.accepted_source_generation = None;
                    session.request_shutdown();
                    SessionLifecycle::Closing(session)
                }
                Ok(Err(_)) | Err(_) => {
                    self.accepted_source_generation = None;
                    self.sync_views(&SessionSnapshot::default());
                    SessionLifecycle::Idle
                }
            },
            SessionLifecycle::Starting { join, .. } => SessionLifecycle::Starting {
                join,
                cancel_on_ready: true,
            },
            SessionLifecycle::Active(session) => {
                self.accepted_source_generation = None;
                session.request_shutdown();
                SessionLifecycle::Closing(session)
            }
            SessionLifecycle::Closing(session) => match session.try_reap() {
                Ok(true) => {
                    self.accepted_source_generation = None;
                    self.sync_views(&SessionSnapshot::default());
                    SessionLifecycle::Idle
                }
                Ok(false) => SessionLifecycle::Closing(session),
                Err(error) => {
                    self.accepted_source_generation = None;
                    self.sync_views(&SessionSnapshot::default());
                    self.feedback = Some(Err(format!("XML worker shutdown failed: {error:?}")));
                    SessionLifecycle::Idle
                }
            },
            other => other,
        };
        let file_pending = self
            .file_worker
            .as_ref()
            .is_some_and(XmlFileWorker::is_pending);
        let session_pending = matches!(
            &self.session,
            SessionLifecycle::Starting { .. } | SessionLifecycle::Closing(_)
        );
        if file_pending || session_pending {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn request_repaint_if_pending(&self, ctx: &egui::Context) {
        if self
            .file_worker
            .as_ref()
            .is_some_and(XmlFileWorker::is_pending)
            || matches!(
                &self.session,
                SessionLifecycle::Starting { .. } | SessionLifecycle::Closing(_)
            )
            || self
                .active_session()
                .is_some_and(|session| session_pending(&session.snapshot()))
        {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    /// Called from eframe's application shutdown path, outside egui rendering.
    /// Active and starting owners are joined only here; normal close/reopen
    /// uses request_shutdown plus nonblocking try_reap instead.
    pub(crate) fn shutdown(&mut self) {
        self.end_session();
        let lifecycle = std::mem::replace(&mut self.session, SessionLifecycle::Idle);
        match lifecycle {
            SessionLifecycle::Starting { join, .. } => match join.join() {
                Ok(Ok(session)) => {
                    let _ = session.shutdown();
                }
                Ok(Err(error)) => tracing::warn!(?error, "XML session startup failed at shutdown"),
                Err(_) => tracing::warn!("XML session startup worker panicked at shutdown"),
            },
            SessionLifecycle::Active(session) | SessionLifecycle::Closing(session) => {
                if let Err(error) = session.shutdown() {
                    tracing::warn!(?error, "XML session shutdown failed");
                }
            }
            SessionLifecycle::Idle | SessionLifecycle::Failed(_) => {}
        }
        if let Some(worker) = self.file_worker.take() {
            worker.shutdown();
        }
    }

    #[cfg(test)]
    fn show_feedback_for_test(&self) -> Option<&Result<String, String>> {
        self.feedback.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_current_snapshot(&self) -> Arc<SessionSnapshot> {
        self.current_snapshot()
    }

    #[cfg(test)]
    pub(crate) fn test_source(&self) -> &str {
        &self.source
    }

    #[cfg(test)]
    pub(crate) fn test_set_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
    }

    #[cfg(test)]
    pub(crate) fn test_select_tree_tab(&mut self) {
        self.tab = XmlToolkitTab::Tree;
    }

    #[cfg(test)]
    pub(crate) fn test_selected_tree_node(
        &self,
    ) -> Option<crate::xml_toolkit_session::XmlNodeSelection> {
        self.tree.selected()
    }

    #[cfg(test)]
    pub(crate) fn test_request_overwrite_confirmation(
        &mut self,
        id: u64,
        path: PathBuf,
        payload: impl Into<Arc<str>>,
    ) {
        self.pending_save = Some(PendingSave {
            id,
            dialog_generation: self.dialog_generation,
            path,
            payload: payload.into(),
            needs_confirmation: true,
        });
    }

    #[cfg(test)]
    pub(crate) fn test_has_pending_overwrite(&self) -> bool {
        self.pending_save
            .as_ref()
            .is_some_and(|pending| pending.needs_confirmation)
    }

    #[cfg(test)]
    pub(crate) fn set_active_session_for_test(&mut self, session: XmlEngineSession) {
        self.session = SessionLifecycle::Active(session);
        self.open = true;
        self.accepted_source_generation = Some(self.source_generation);
        let snapshot = if let SessionLifecycle::Active(session) = &self.session {
            let _ = session.replace_source(self.source.clone());
            Some(session.snapshot())
        } else {
            None
        };
        if let Some(snapshot) = snapshot {
            self.sync_views(&snapshot);
        }
    }
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("XML Toolkit")
        .with_inner_size([1120.0, 760.0])
        .with_min_inner_size([480.0, 340.0])
        .with_resizable(true)
}

fn current_output(snapshot: &SessionSnapshot) -> Option<&XmlFormatOutput> {
    let XmlOutputStatus::Ready(output) = &snapshot.output else {
        return None;
    };
    (snapshot.revision == Some(output.revision)).then_some(output.as_ref())
}

fn output_text_for_apply(snapshot: &SessionSnapshot) -> Result<String, String> {
    let output = current_output(snapshot)
        .ok_or_else(|| "Formatted XML output is no longer current.".to_owned())?;
    if output.text.len() > MAX_XML_INPUT_BYTES {
        return Err(format!(
            "Formatted XML output exceeds the {MAX_XML_INPUT_BYTES}-byte source limit; copy or save it instead."
        ));
    }
    Ok(output.text.clone())
}

fn output_text_for_explicit_use(snapshot: &SessionSnapshot) -> Result<String, String> {
    current_output(snapshot)
        .map(|output| output.text.clone())
        .ok_or_else(|| "No current XML output is ready.".into())
}

fn session_pending(snapshot: &SessionSnapshot) -> bool {
    matches!(snapshot.parse, XmlParseStatus::Pending)
        || matches!(snapshot.output, XmlOutputStatus::Pending(_))
        || matches!(snapshot.detail, XmlDetailStatus::Pending(_))
        || matches!(snapshot.xpath, XmlXPathStatus::Pending(_))
        || matches!(snapshot.schema.status, XmlSchemaStatus::Pending(_))
}

fn bounded_prefix(text: &str, maximum_chars: usize) -> &str {
    text.char_indices()
        .nth(maximum_chars)
        .map_or(text, |(index, _)| &text[..index])
}

fn xml_error_message(error: &crate::xml_toolkit_engine::XmlEngineError) -> String {
    use crate::xml_toolkit_engine::XmlEngineError;
    match error {
        XmlEngineError::Parse(diagnostic) => {
            let location = match (diagnostic.line, diagnostic.column) {
                (Some(line), Some(column)) => format!("line {line}, column {column}: "),
                (Some(line), None) => format!("line {line}: "),
                (None, Some(column)) => format!("column {column}: "),
                (None, None) => String::new(),
            };
            format!("{location}{}", diagnostic.message)
        }
        XmlEngineError::InputTooLarge { limit } => {
            format!("source exceeds the {limit}-byte input limit")
        }
        XmlEngineError::UnsupportedPlatform => {
            "the XML backend is supported on Windows only".into()
        }
        other => format!("{other:?}"),
    }
}

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(VIEWPORT_ID)
}

fn xml_toolkit_embedded_layer_id() -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("xml_toolkit_embedded_window"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::Mutex;

    struct FakeClipboard {
        reads: Mutex<usize>,
        writes: Mutex<Vec<String>>,
        source: Mutex<Result<String, ClipboardError>>,
    }

    impl Default for FakeClipboard {
        fn default() -> Self {
            Self {
                reads: Mutex::new(0),
                writes: Mutex::new(Vec::new()),
                source: Mutex::new(Ok(String::new())),
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            *self.reads.lock().unwrap() += 1;
            self.source.lock().unwrap().clone()
        }

        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    thread_local! {
        static XML_IMMEDIATE_RENDERER_INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static XML_VIEWPORT_EVENTS: std::cell::RefCell<Vec<egui::Event>> = const { std::cell::RefCell::new(Vec::new()) };
        static XML_VIEWPORT_FOCUSED_ID: std::cell::RefCell<Option<egui::Id>> = const { std::cell::RefCell::new(None) };
        static XML_VIEWPORT_SOURCE_RECT: std::cell::RefCell<Option<egui::Rect>> = const { std::cell::RefCell::new(None) };
    }

    fn install_xml_immediate_viewport_renderer() {
        XML_IMMEDIATE_RENDERER_INSTALLED.with(|installed| {
            assert!(
                !installed.replace(true),
                "the XML test installs its immediate renderer once"
            );
        });
        egui::Context::set_immediate_viewport_renderer(|parent_ctx, mut viewport| {
            let viewport_id = viewport.ids.this;
            let parent_id = viewport.ids.parent;
            let size = viewport
                .builder
                .inner_size
                .unwrap_or_else(|| parent_ctx.content_rect().size());
            let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut viewports = parent_ctx.input(|input| input.raw.viewports.clone());
            viewports.insert(
                viewport_id,
                egui::ViewportInfo {
                    parent: Some(parent_id),
                    focused: Some(true),
                    inner_rect: Some(screen_rect),
                    title: viewport.builder.title.clone(),
                    ..Default::default()
                },
            );
            let events =
                XML_VIEWPORT_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()));
            let output = parent_ctx.run_ui(
                egui::RawInput {
                    viewport_id,
                    viewports,
                    screen_rect: Some(screen_rect),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(ui.ctx().viewport_id(), viewport_id);
                    assert_eq!(ui.ctx().parent_viewport_id(), parent_id);
                    (viewport.viewport_ui_cb)(ui);
                    XML_VIEWPORT_FOCUSED_ID.with(|focused| {
                        *focused.borrow_mut() = ui.ctx().memory(|memory| memory.focused());
                    });
                    XML_VIEWPORT_SOURCE_RECT.with(|source| {
                        *source.borrow_mut() = ui
                            .ctx()
                            .read_response(egui::Id::new("xml_toolkit_source_editor"))
                            .map(|response| response.rect);
                    });
                },
            );
            output.drop_without_applying_deltas();
            assert_eq!(parent_ctx.viewport_id(), parent_id);
        });
    }

    fn render_xml_immediate_frame(
        ctx: &egui::Context,
        dialog: &mut XmlToolkitDialogState,
        events: Vec<egui::Event>,
    ) {
        XML_VIEWPORT_EVENTS.with(|pending| *pending.borrow_mut() = events);
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(960.0, 680.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                dialog.show(ctx);
            },
        )
        .drop_without_applying_deltas();
    }

    #[test]
    fn m7f_xml_open_and_passive_frames_do_not_touch_clipboard_and_paste_reads_once() {
        let clipboard = Arc::new(FakeClipboard::default());
        *clipboard.source.lock().unwrap() = Ok("<root>é🦀</root>".into());
        let mut dialog = XmlToolkitDialogState::with_clipboard(clipboard.clone());
        dialog.open();
        dialog.open();
        let context = egui::Context::default();
        for _ in 0..2 {
            context
                .run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
                .drop_without_applying_deltas();
        }
        assert_eq!(*clipboard.reads.lock().unwrap(), 0);
        assert!(clipboard.writes.lock().unwrap().is_empty());

        dialog.process_intents(
            &context,
            UiIntents {
                paste_clipboard: true,
                ..UiIntents::default()
            },
        );
        assert_eq!(*clipboard.reads.lock().unwrap(), 1);
        assert_eq!(dialog.source, "<root>é🦀</root>");
        dialog.shutdown();
    }

    #[test]
    fn m7f_xml_stale_load_completion_cannot_replace_edited_source_or_closed_draft() {
        let clipboard = Arc::new(FakeClipboard::default());
        let mut dialog = XmlToolkitDialogState::with_clipboard(clipboard);
        dialog.open = true;
        dialog.source = "<edited/>".into();
        dialog.source_generation = 9;
        let stale = FileCompletion {
            id: 1,
            source_generation: Some(8),
            dialog_generation: dialog.dialog_generation,
            path: PathBuf::from("C:\\disposable\\old.xml"),
            result: FileResult::Loaded(Ok("<stale/>".into())),
        };
        dialog.apply_file_completion(stale);
        assert_eq!(dialog.source, "<edited/>");

        let failed_current_load = FileCompletion {
            id: 2,
            source_generation: Some(dialog.source_generation),
            dialog_generation: dialog.dialog_generation,
            path: PathBuf::from("C:\\disposable\\missing.xml"),
            result: FileResult::Loaded(Err("missing fixture".into())),
        };
        dialog.apply_file_completion(failed_current_load);
        assert_eq!(dialog.source, "<edited/>");
        assert!(
            matches!(dialog.show_feedback_for_test(), Some(Err(error)) if error == "missing fixture")
        );

        dialog.end_session();
        let closed_generation = dialog.dialog_generation;
        let late_completion = FileCompletion {
            id: 3,
            source_generation: Some(dialog.source_generation),
            dialog_generation: closed_generation - 1,
            path: PathBuf::from("C:\\disposable\\late.xml"),
            result: FileResult::Loaded(Ok("<late/>".into())),
        };
        dialog.apply_file_completion(late_completion);
        assert_eq!(dialog.source, "<edited/>");
        dialog.shutdown();
    }

    #[test]
    fn m7f_xml_output_apply_respects_source_limit_while_copy_and_save_keep_full_payload() {
        let output = XmlFormatOutput {
            revision: crate::xml_toolkit_session::XmlSourceRevision::for_test(1),
            mode: XmlFormatMode::Pretty,
            text: "x".repeat(MAX_XML_INPUT_BYTES + 1),
        };
        let snapshot = SessionSnapshot {
            revision: Some(output.revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Ready(Arc::new(output)),
            ..SessionSnapshot::default()
        };
        let authored_source = "<root/>".to_owned();
        assert!(
            output_text_for_apply(&snapshot)
                .unwrap_err()
                .contains("exceeds the 4194304-byte source limit")
        );
        let explicit_output = output_text_for_explicit_use(&snapshot)
            .expect("Copy and Save keep the bounded current output");
        assert_eq!(explicit_output.len(), MAX_XML_INPUT_BYTES + 1);
        assert_eq!(authored_source, "<root/>");
    }

    #[test]
    fn m7f_xml_viewport_is_resizable_and_keeps_a_bounded_minimum() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(1120.0, 760.0)));
        assert_eq!(builder.min_inner_size, Some(egui::vec2(480.0, 340.0)));
        assert_eq!(builder.resizable, Some(true));
        assert!(viewport_id() != egui::ViewportId::ROOT);
    }

    #[test]
    fn m7f_xml_overwrite_confirmation_controls_remain_reachable_in_small_viewport() {
        let context = egui::Context::default();
        let mut dialog = XmlToolkitDialogState::with_clipboard(Arc::new(FakeClipboard::default()));
        dialog.open = true;
        dialog.pending_save = Some(PendingSave {
            id: 1,
            dialog_generation: dialog.dialog_generation,
            path: PathBuf::from("C:\\disposable\\result.xml"),
            payload: Arc::from("<root/>"),
            needs_confirmation: true,
        });
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 340.0));
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| dialog.show_overwrite_confirmation(ctx, false),
            )
            .drop_without_applying_deltas();

        for button in dialog.overwrite_button_rects {
            assert!(button.is_positive(), "overwrite controls must be laid out");
            assert!(
                screen.contains(button.center()),
                "control must stay on screen"
            );
        }
    }

    #[test]
    fn jump_mode_xml_native_viewport_keeps_default_off_source_input() {
        install_xml_immediate_viewport_renderer();
        let context = egui::Context::default();
        context.set_embed_viewports(false);
        let clipboard = Arc::new(FakeClipboard::default());
        let mut dialog = XmlToolkitDialogState::with_clipboard(clipboard.clone());
        dialog.open();
        dialog.session = SessionLifecycle::Failed("isolated UI fixture".into());

        render_xml_immediate_frame(&context, &mut dialog, Vec::new());
        render_xml_immediate_frame(&context, &mut dialog, Vec::new());
        assert!(dialog.native_jump_mode.retained_registry().is_empty());
        let source_rect = XML_VIEWPORT_SOURCE_RECT
            .with(|rect| *rect.borrow())
            .expect("the actual native source editor was laid out");
        assert!(source_rect.is_positive());
        let pointer = source_rect.center();
        render_xml_immediate_frame(
            &context,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        render_xml_immediate_frame(
            &context,
            &mut dialog,
            vec![egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let source_id = egui::Id::new("xml_toolkit_source_editor");
        assert_eq!(
            XML_VIEWPORT_FOCUSED_ID.with(|focused| *focused.borrow()),
            Some(source_id),
            "ordinary pointer focus remains available with Jump Mode disabled"
        );

        let source = "<root>λ🦀</root>";
        render_xml_immediate_frame(
            &context,
            &mut dialog,
            vec![egui::Event::Text(source.into())],
        );
        assert_eq!(dialog.source, source);
        assert!(clipboard.writes.lock().unwrap().is_empty());
        dialog.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn m7f_xml_close_during_startup_reaps_in_closed_background_poll_and_discards_late_load() {
        use std::time::Instant;

        let mut dialog = XmlToolkitDialogState::with_clipboard(Arc::new(FakeClipboard::default()));
        dialog.open = true;
        dialog.source = "<draft/>".into();
        dialog.source_generation = 4;
        let prior_dialog_generation = dialog.dialog_generation;
        dialog.session = SessionLifecycle::Starting {
            join: thread::Builder::new()
                .name("xml-toolkit-test-startup".into())
                .spawn(XmlEngineSession::start)
                .expect("startup test thread"),
            cancel_on_ready: false,
        };
        dialog.end_session();

        let started = Instant::now();
        let mut saw_closing = false;
        let context = egui::Context::default();
        while !matches!(&dialog.session, SessionLifecycle::Idle) {
            dialog.poll_closed_background(&context);
            saw_closing |= matches!(&dialog.session, SessionLifecycle::Closing(_));
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "closed XML session did not reap"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(saw_closing, "a started owner must be shut down and reaped");

        dialog.apply_file_completion(FileCompletion {
            id: 17,
            source_generation: Some(4),
            dialog_generation: prior_dialog_generation,
            path: PathBuf::from("C:\\disposable\\late.xml"),
            result: FileResult::Loaded(Ok("<stale/>".into())),
        });
        assert_eq!(dialog.source, "<draft/>");
        dialog.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn m7f_xml_session_workflow_uses_one_current_session_for_real_parse_xpath_format_xsd_and_reopen()
     {
        use crate::xml_toolkit_engine::{XmlNodeDetailKind, XmlTreeNodeKind};
        use crate::xml_toolkit_session::{
            XmlNamespaceContextRevision, XmlXPathQueryRequest, XmlXPathRequestId,
        };
        use std::time::Instant;

        let clipboard = Arc::new(FakeClipboard::default());
        let mut dialog = XmlToolkitDialogState::with_clipboard(clipboard);
        dialog.open = true;
        dialog.source = "<root xmlns='urn:test'><item id='one'>one</item></root>".into();
        dialog.source_generation = 1;
        let session = XmlEngineSession::start().expect("Windows MSXML session");
        dialog.set_active_session_for_test(session);
        let directory = tempfile::tempdir().expect("disposable XSD directory");
        let xsd = directory.path().join("schema.xsd");
        std::fs::write(
            &xsd,
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema' targetNamespace='urn:test' xmlns='urn:test' elementFormDefault='qualified'><xs:element name='root'><xs:complexType><xs:sequence><xs:element name='item'><xs:complexType><xs:simpleContent><xs:extension base='xs:string'><xs:attribute name='id' type='xs:string' use='required'/></xs:extension></xs:simpleContent></xs:complexType></xs:element></xs:sequence></xs:complexType></xs:element></xs:schema>",
        )
        .expect("write local XSD fixture");

        let wait_for = |session: &XmlEngineSession, ready: fn(&SessionSnapshot) -> bool| {
            let started = Instant::now();
            loop {
                let snapshot = session.snapshot();
                if ready(&snapshot) {
                    return snapshot;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "XML session timed out"
                );
                thread::sleep(Duration::from_millis(5));
            }
        };
        let session = dialog.active_session().unwrap();
        let snapshot = wait_for(session, |snapshot| snapshot.parse == XmlParseStatus::Ready);
        let projection = match &snapshot.tree {
            XmlTreeStatus::Ready(projection) => projection,
            _ => panic!("ready parse must publish one tree projection"),
        };
        let item = projection
            .nodes
            .iter()
            .find(|node| node.kind == XmlTreeNodeKind::Element && node.local_name == "item")
            .expect("projected namespaced item");
        session
            .format(snapshot.revision.unwrap(), XmlFormatMode::Pretty)
            .expect("queue format");
        let formatted = wait_for(session, |snapshot| {
            matches!(snapshot.output, XmlOutputStatus::Ready(_))
        });
        assert!(current_output(&formatted).unwrap().text.contains("<item"));

        let invalid_xpath = XmlXPathQueryRequest {
            revision: snapshot.revision.unwrap(),
            document: projection.document,
            request: XmlXPathRequestId::from_counter(1),
            expression: "//p:item".into(),
            namespace_context_revision: XmlNamespaceContextRevision::from_counter(1),
            namespaces: vec![("p".into(), "urn:test".into())],
        };
        session
            .request_xpath(invalid_xpath)
            .expect("queue namespaced XPath");
        let queried = wait_for(session, |snapshot| {
            matches!(snapshot.xpath, XmlXPathStatus::Nodes { .. })
        });
        assert!(
            matches!(queried.xpath, XmlXPathStatus::Nodes { ref nodes, .. } if nodes.len() == 1)
        );
        assert!(
            session
                .request_detail(crate::xml_toolkit_session::XmlNodeDetailRequest {
                    revision: snapshot.revision.unwrap(),
                    document: projection.document,
                    node: item.id,
                    selection_request:
                        crate::xml_toolkit_session::XmlSelectionRequestId::from_counter(1),
                    detail_request: crate::xml_toolkit_session::XmlDetailRequestId::from_counter(1),
                    kind: XmlNodeDetailKind::ElementText,
                    intent: XmlDetailIntent::Copy,
                })
                .is_ok()
        );

        session
            .select_schema_path(xsd)
            .expect("select local schema");
        session
            .validate_selected_schema()
            .expect("queue XSD validation");
        let valid = wait_for(session, |snapshot| {
            matches!(snapshot.schema.status, XmlSchemaStatus::Valid(_))
        });
        assert!(matches!(valid.schema.status, XmlSchemaStatus::Valid(_)));

        dialog.accept_source("<root xmlns='urn:test'><item>missing-id</item></root>".into());
        let session = dialog.active_session().unwrap();
        let invalid = wait_for(session, |snapshot| snapshot.parse == XmlParseStatus::Ready);
        assert_eq!(invalid.xpath, XmlXPathStatus::NotRun);
        assert!(matches!(invalid.schema.status, XmlSchemaStatus::NotRun));
        session
            .validate_selected_schema()
            .expect("queue invalid XSD validation");
        let violation = wait_for(session, |snapshot| {
            matches!(
                snapshot.schema.status,
                XmlSchemaStatus::SchemaViolation { .. }
            )
        });
        assert!(matches!(
            violation.schema.status,
            XmlSchemaStatus::SchemaViolation { .. }
        ));

        dialog.accept_source(String::new());
        let empty = dialog.active_session().unwrap().snapshot();
        assert_eq!(empty.parse, XmlParseStatus::Empty);
        assert_eq!(empty.xpath, XmlXPathStatus::Empty);
        assert!(matches!(empty.tree, XmlTreeStatus::Empty));
        dialog.end_session();
        let context = egui::Context::default();
        let started = Instant::now();
        while matches!(&dialog.session, SessionLifecycle::Closing(_)) {
            dialog.poll_session(&context);
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "XML session shutdown timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(&dialog.session, SessionLifecycle::Idle));
        dialog.open();
        dialog.ensure_session_start(&context);
        let started = Instant::now();
        loop {
            dialog.poll_session(&context);
            if matches!(&dialog.session, SessionLifecycle::Active(_)) {
                break;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "XML session reopen timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(dialog.source, "");
        dialog.shutdown();
    }

    #[test]
    fn m7f_xml_failure_feedback_is_distinct_and_does_not_publish_invalid_source() {
        let clipboard = Arc::new(FakeClipboard::default());
        *clipboard.source.lock().unwrap() = Err(ClipboardError::NonText);
        let mut dialog = XmlToolkitDialogState::with_clipboard(clipboard.clone());
        dialog.source = "<preserved/>".into();
        dialog.process_intents(
            &egui::Context::default(),
            UiIntents {
                paste_clipboard: true,
                ..UiIntents::default()
            },
        );
        assert_eq!(dialog.source, "<preserved/>");
        assert_eq!(*clipboard.reads.lock().unwrap(), 1);
        assert!(
            matches!(dialog.show_feedback_for_test(), Some(Err(error)) if error.contains("text"))
        );
    }
}
