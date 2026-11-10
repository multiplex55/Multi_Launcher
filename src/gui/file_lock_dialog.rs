use crate::file_lock::{
    FileLockInspection, FileLockProcess, FileLockSession, FileLockSessionStatus,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::ViewportPassProvenance;
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const WINDOW_TITLE: &str = "File Lock Inspector";
const UTILITY_KEY: &str = "file-lock";
const ROW_HEIGHT: f32 = 24.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileLockRevealRequest {
    pub(crate) generation: u64,
    pub(crate) process_id: u32,
    pub(crate) creation_filetime: u64,
    pub(crate) executable_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileLockDialogEffect {
    CopySnapshot(String),
    CopyLabel(String),
    RevealExecutable(FileLockRevealRequest),
    FocusPanel,
    ShowRoot,
}

#[derive(Clone)]
enum FileLockAction {
    Host(super::utility_host::UtilityPresentation),
    Open(Option<PathBuf>),
    Prefill(PathBuf),
    ChooseFile,
    Inspect(PathBuf),
    Refresh {
        generation: u64,
        path: PathBuf,
    },
    Cancel {
        generation: u64,
    },
    SelectProcess {
        generation: u64,
        process_id: u32,
        creation_filetime: u64,
    },
    CopySnapshot {
        generation: u64,
    },
    Reveal(FileLockRevealRequest),
    Close,
}

struct PendingFileLockPass {
    provenance: ViewportPassProvenance,
    actions: Vec<FileLockAction>,
    owner: ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    native: bool,
    info: Option<egui::ViewportInfo>,
}
#[derive(Clone, Copy)]
struct FileLockOwnerPass {
    root: ViewportPassProvenance,
    owner: ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
}

#[derive(Clone)]
struct SubmittedInspection {
    generation: u64,
    path: PathBuf,
}

struct RevealWorker {
    request: FileLockRevealRequest,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<crate::file_lock::ValidatedFileLockExecutable, String>>,
    joined: Arc<AtomicBool>,
    retirement_unavailable: Arc<AtomicBool>,
}

struct ReadyReveal {
    request: FileLockRevealRequest,
    executable: Option<crate::file_lock::ValidatedFileLockExecutable>,
    dispatched: bool,
}

struct RevealRetirementReceipt {
    joined: Arc<AtomicBool>,
    unavailable: Arc<AtomicBool>,
    context: egui::Context,
    resolved: bool,
}

impl RevealRetirementReceipt {
    fn joined(mut self) {
        self.resolved = true;
        self.joined.store(true, Ordering::Release);
        self.context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

impl Drop for RevealRetirementReceipt {
    fn drop(&mut self) {
        if !self.resolved {
            self.unavailable.store(true, Ordering::Release);
            self.context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

pub(crate) struct FileLockDialogState {
    pub(crate) open: bool,
    host: super::utility_host::UtilityHostState,
    presentation_prepared: bool,
    host_epoch: u64,
    host_focus: bool,
    host_os_close_pending: bool,
    external_pending: Option<PendingFileLockPass>,
    session: FileLockSession,
    selected_path: Option<PathBuf>,
    submitted: Option<SubmittedInspection>,
    selected_process: Option<(u64, u32, u64)>,
    pending: Option<PendingFileLockPass>,
    latest_pass: Option<FileLockOwnerPass>,
    notice: Option<String>,
    reveal_worker: Option<RevealWorker>,
    reveal_ready: Option<ReadyReveal>,
    reveal_retirement_failed: bool,
    #[cfg(test)]
    test_file_choice: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_file_picker_calls: usize,
    #[cfg(test)]
    test_reveal_result: Option<Result<PathBuf, String>>,
    #[cfg(test)]
    test_hold_ready_dispatch: bool,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_refresh_enabled: bool,
}

impl FileLockDialogState {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        ctx.add_plugin(super::utility_host::HostedCopyCapture::default());
        let wake_context = ctx.clone();
        Self {
            open: false,
            host: super::utility_host::UtilityHostState::docked(),
            presentation_prepared: false,
            host_epoch: 0,
            host_focus: false,
            host_os_close_pending: false,
            external_pending: None,
            session: FileLockSession::new(move || {
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
            }),
            selected_path: None,
            submitted: None,
            selected_process: None,
            pending: None,
            latest_pass: None,
            notice: None,
            reveal_worker: None,
            reveal_ready: None,
            reveal_retirement_failed: false,
            #[cfg(test)]
            test_file_choice: None,
            #[cfg(test)]
            test_file_picker_calls: 0,
            #[cfg(test)]
            test_reveal_result: None,
            #[cfg(test)]
            test_hold_ready_dispatch: false,
            #[cfg(test)]
            test_action_rects: Vec::new(),
            #[cfg(test)]
            test_refresh_enabled: false,
        }
    }

    pub(crate) fn ensure_open(&mut self) {
        if !self.open {
            self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
            self.open = true;
        }
    }
    pub(crate) fn request_open(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage_external(
                ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                FileLockAction::Open(path),
            );
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(super) fn stage_file_search_prefill(
        &mut self,
        pass: ViewportPassProvenance,
        path: PathBuf,
    ) {
        self.stage_external(pass, FileLockAction::Prefill(path));
    }

    pub(crate) fn stage_prefill(&mut self, ctx: &egui::Context, path: PathBuf) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage_external(
                ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                FileLockAction::Prefill(path),
            );
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn request_close(&mut self, ctx: &egui::Context) {
        if self.open || self.pending.is_some() || self.external_pending.is_some() {
            self.stage(ctx, vec![FileLockAction::Close]);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.open = false;
        self.pending = None;
        self.invalidate_selection();
        self.cancel_reveal();
        self.session.close();
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.pending.is_some()
            || self.external_pending.is_some()
            || (self.open
                && self.host.presentation() == super::utility_host::UtilityPresentation::Popout)
            || self.session.view().is_busy()
            || self.reveal_worker.is_some()
            || self.reveal_ready.is_some()
    }

    fn render_host_body(
        &mut self,
        ctx: &egui::Context,
        input_allowed: bool,
        first_ui_pass: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> (Vec<FileLockAction>, bool) {
        self.poll_reveal_work();
        if !self.open {
            return (Vec::new(), false);
        }
        #[cfg(test)]
        self.test_action_rects.clear();

        let commit_allowed =
            input_allowed && first_ui_pass && !ctx.will_discard() && !egui::Popup::is_any_open(ctx);
        let view = self.session.view();
        let current_inspection = self.current_inspection(&view);
        let selected_path = self.selected_path.clone();
        let selected_process = self.selected_process;
        let notice = self.notice.clone();
        let reveal_busy = self.reveal_worker.is_some() || self.reveal_retirement_failed;
        let mut window_open = self.open;
        let mut actions = Vec::new();

        let mut draw = |ui: &mut egui::Ui| {
            let controls_allowed = commit_allowed && ui.is_visible() && !ui.is_sizing_pass();
            if let Some(presentation) = presentation {
                let (button, next) = super::utility_host::presentation_button(
                    ui,
                    presentation,
                    input_allowed && ui.is_visible() && !ui.is_sizing_pass(),
                );
                self.record_action_rect("host-presentation", button.rect, ui.clip_rect());
                if controls_allowed && button.clicked() {
                    actions.push(FileLockAction::Host(next));
                }
            }
            ui.add_enabled_ui(input_allowed && ui.is_visible() && !ui.is_sizing_pass(), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let choose = ui.button("Choose file…");
                            self.record_action_rect("choose_file", choose.rect, ui.clip_rect());
                            if choose.clicked() {
                                actions.push(FileLockAction::ChooseFile);
                            }

                            let inspect = ui.add_enabled(
                                selected_path.is_some() && !view.is_busy(),
                                egui::Button::new("Inspect"),
                            );
                            self.record_action_rect("inspect", inspect.rect, ui.clip_rect());
                            if inspect.clicked()
                                && let Some(path) = selected_path.clone()
                            {
                                actions.push(FileLockAction::Inspect(path));
                            }

                            let refresh = ui.add_enabled(
                                selected_path.is_some() && !view.is_busy(),
                                egui::Button::new("Refresh"),
                            );
                            self.record_action_rect("refresh", refresh.rect, ui.clip_rect());
                            #[cfg(test)]
                            {
                                self.test_refresh_enabled = refresh.enabled();
                            }
                            if refresh.clicked()
                                && let Some(path) = selected_path.clone()
                            {
                                actions.push(FileLockAction::Refresh {
                                    generation: view.generation(),
                                    path,
                                });
                            }

                            let cancel = ui.add_enabled(
                                view.is_busy(),
                                egui::Button::new("Cancel"),
                            );
                            self.record_action_rect("cancel", cancel.rect, ui.clip_rect());
                            if cancel.clicked() {
                                actions.push(FileLockAction::Cancel {
                                    generation: view.generation(),
                                });
                            }

                            let can_copy = current_inspection.is_some();
                            let copy = ui.add_enabled(
                                can_copy,
                                egui::Button::new("Copy snapshot"),
                            );
                            self.record_action_rect("copy_snapshot", copy.rect, ui.clip_rect());
                            if copy.clicked() {
                                actions.push(FileLockAction::CopySnapshot {
                                    generation: view.generation(),
                                });
                            }
                        });

                        if let Some(path) = selected_path.as_ref() {
                            let label = ui.label(format!("Selected file: {}", path.display()));
                            self.record_action_rect("selected-label",label.rect,ui.clip_rect());
                        } else {
                            ui.label("Choose a regular file, then select Inspect.");
                        }

                        let status_view = self
                            .submitted_matches_view(&view)
                            .then_some(&view);
                        show_inspection_status(ui, status_view, current_inspection);

                        if let Some(inspection) = current_inspection {
                            if inspection.reports_no_processes() {
                                ui.weak("No processes reported.");
                            } else {
                                if inspection.processes.iter().any(process_details_unavailable) {
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        "Some process details are unavailable; reported rows are retained.",
                                    );
                                }
                                let selected = selected_process.and_then(|(generation, pid, creation)| {
                                    (generation == view.generation())
                                        .then(|| {
                                            inspection.processes.iter().find(|process| {
                                                process.process_id == pid
                                                    && process.creation_filetime == creation
                                            })
                                        })
                                        .flatten()
                                });
                                let reveal_path = selected.and_then(|process| process.executable_path.as_ref());
                                let reveal = ui.add_enabled(
                                    reveal_path.is_some() && !reveal_busy,
                                    egui::Button::new("Reveal executable"),
                                );
                                self.record_action_rect("reveal", reveal.rect, ui.clip_rect());
                                if reveal.clicked()
                                    && let Some(process) = selected
                                    && let Some(executable_path) = process.executable_path.clone()
                                {
                                    actions.push(FileLockAction::Reveal(FileLockRevealRequest {
                                        generation: view.generation(),
                                        process_id: process.process_id,
                                        creation_filetime: process.creation_filetime,
                                        executable_path,
                                    }));
                                }
                                egui::ScrollArea::vertical()
                                    .id_salt("file_lock_process_rows")
                                    .max_height(
                                        (ui.available_height() - 52.0).max(ROW_HEIGHT * 3.0),
                                    )
                                    .show(ui, |ui| {
                                        for process in &inspection.processes {
                                            let key = (
                                                view.generation(),
                                                process.process_id,
                                                process.creation_filetime,
                                            );
                                            let selected = selected_process == Some(key);
                                            let label = format!(
                                                "PID {} • {}",
                                                process.process_id,
                                                process_label(process),
                                            );
                                            let response = ui.add_sized(
                                                [ui.available_width(), ROW_HEIGHT],
                                                egui::Button::selectable(selected, label),
                                            );
                                            self.record_action_rect(
                                                "process_row",
                                                response.rect,
                                                ui.clip_rect(),
                                            );
                                            if response.clicked() && controls_allowed {
                                                actions.push(FileLockAction::SelectProcess {
                                                    generation: view.generation(),
                                                    process_id: process.process_id,
                                                    creation_filetime: process.creation_filetime,
                                                });
                                            }
                                            ui.horizontal_wrapped(|ui| {
                                                ui.small(format!(
                                                    "Started {} • FILETIME {}",
                                                    format_process_start(process.creation_filetime),
                                                    process.creation_filetime,
                                                ));
                                                if !process.service_name.is_empty() {
                                                    ui.small(format!("Service: {}", process.service_name));
                                                }
                                            });
                                            if let Some(executable) = process.executable_path.as_ref() {
                                                ui.small(format!("Executable: {}", executable.display()));
                                            } else {
                                                ui.small("Executable path unavailable.");
                                            }
                                            ui.separator();
                                        }
                                    });

                            }
                            ui.weak("Windows reports these users; the list does not prove which process is denying access.");
                        }

                        if let Some(notice) = notice.as_ref() {
                            ui.colored_label(ui.visuals().warn_fg_color, notice);
                        }
                    });
        };
        if let Some(ui) = child_ui {
            draw(ui);
        } else {
            egui::Window::new(WINDOW_TITLE)
                .id(egui::Id::new(Some(WINDOW_TITLE)))
                .collapsible(false)
                .resizable(true)
                .default_size((760.0, 520.0))
                .min_size((520.0, 340.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    UTILITY_KEY,
                    egui::Id::new(WINDOW_TITLE),
                    Some(&mut window_open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::vec2(760.0, 520.0),
                        ),
                        egui::vec2(520.0, 340.0),
                        [None, None],
                    )
                    .with_maximum(ctx.content_rect().size()),
                    |ui| draw(ui),
                );
        }
        if !commit_allowed {
            actions.clear();
        }
        (actions, !window_open && commit_allowed)
    }
    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            let key =
                crate::settings::UtilityToolKey::parse("file-lock").expect("stable utility key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host);
            self.presentation_prepared = true;
        }
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }

    pub(super) fn remember_final_presentation(&self, ctx: &egui::Context) {
        if self.open {
            let key =
                crate::settings::UtilityToolKey::parse("file-lock").expect("stable utility key");
            super::utility_host::remember_presentation(ctx, key, self.host.presentation());
        }
    }

    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(Some(WINDOW_TITLE)),
        ))
    }
    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.host.bootstrap_pending()
    }
    pub(crate) fn awaiting_root_focus(&self) -> bool {
        self.host_focus
            && self.host.presentation() == super::utility_host::UtilityPresentation::Docked
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host_focus = true;
        self.host.request_focus();
    }
    pub(super) fn presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    pub(crate) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("file-lock-viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(UTILITY_KEY)
                .expect("stable folder-size key"),
            viewport_id: Self::viewport(),
            owner_generation: self.host_epoch,
            host_generation: host.generation,
        }
    }
    fn retire_host(&mut self, ctx: &egui::Context) {
        if self.host.presentation() == super::utility_host::UtilityPresentation::Popout {
            let identity = self.native_identity(self.host.identity());
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host.retire_to_docked();
        self.presentation_prepared = false;
        self.host_os_close_pending = false;
        self.host_focus = false;
        self.latest_pass = None;
    }
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        input_allowed: bool,
        first: bool,
    ) {
        self.show_hosted(
            ctx,
            root_visible,
            input_allowed,
            input_allowed,
            first,
            false,
            true,
        );
    }
    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        root_owner: bool,
        application_input_allowed: bool,
        first: bool,
        pinned: bool,
        shortcut_allowed: bool,
    ) {
        self.prepare_open_presentation(ctx);
        use super::utility_host::UtilityPresentation;
        self.poll_reveal_work();
        self.latest_pass = None;
        if !self.open {
            return;
        }
        if pinned {
            self.host_os_close_pending = false;
        }
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut owner = None;
        let mut native = false;
        let mut info = None;
        let mut staged = None;
        let mut owner_allowed = false;
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_owner && !egui::Popup::is_any_open(ctx);
                owner_allowed = allowed;
                let shortcut = first
                    && allowed
                    && shortcut_allowed
                    && !pinned
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let (actions, close) =
                    self.render_host_body(ctx, allowed, first, None, Some(host.host));
                staged = Some((actions, close || shortcut));
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title(WINDOW_TITLE)
                .with_inner_size([760.0, 520.0])
                .with_min_inner_size([520.0, 340.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let c = child.ctx().clone();
                let is_native =
                    class == egui::ViewportClass::Immediate && c.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner = Some(ViewportPassProvenance::current(&c, c.viewport_id()));
                    native = is_native;
                    if is_native {
                        info = Some(child.input(|i| i.viewport().clone()));
                    }
                } else {
                    owner = None;
                    info = None;
                    staged = None;
                }
                let allowed = if is_native {
                    application_input_allowed
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && !egui::Popup::is_any_open(&c)
                        && super::jump_mode::ui::focused_response_layer(&c)
                            .is_none_or(|l| l == egui::LayerId::background())
                } else {
                    root_owner && !egui::Popup::is_any_open(&c)
                };
                owner_allowed = allowed;
                if is_native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.host_os_close_pending = true;
                    }
                }
                let shortcut = child_first
                    && allowed
                    && !pinned
                    && (is_native || shortcut_allowed)
                    && c.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                if is_native {
                    let receipt = super::utility_host::HostedCopyReceipt {
                        root,
                        owner: ViewportPassProvenance::current(&c, c.viewport_id()),
                        host,
                        epoch: self.host_epoch,
                        eligible: child_first && allowed && !c.will_discard(),
                    };
                    c.with_plugin(|plugin: &mut super::utility_host::HostedCopyCapture| {
                        plugin.armed.insert(c.viewport_id(), receipt);
                    });
                    egui::CentralPanel::default().show(child, |ui| {
                        staged = Some(self.render_host_body(
                            &c,
                            allowed,
                            child_first,
                            Some(ui),
                            Some(host.host),
                        ));
                    });
                } else {
                    staged = Some(self.render_host_body(
                        &c,
                        allowed,
                        child_first,
                        None,
                        Some(host.host),
                    ));
                }
                if let Some((_, close)) = &mut staged {
                    *close |= shortcut || (is_native && !pinned && self.host_os_close_pending);
                }
            });
        }
        if let (Some(owner), Some((mut actions, close))) = (owner, staged) {
            if close && !pinned {
                actions = vec![FileLockAction::Close];
            } else if let Some(next) = actions.iter().find_map(|action| {
                if let FileLockAction::Host(next) = action {
                    Some(*next)
                } else {
                    None
                }
            }) {
                actions = vec![FileLockAction::Host(next)];
            }
            if first && !ctx.will_discard() {
                if let Some(previous) = self.pending.as_ref()
                    && previous
                        .actions
                        .iter()
                        .any(|action| matches!(action, FileLockAction::Close))
                {
                    return;
                }
                self.latest_pass = owner_allowed.then_some(FileLockOwnerPass {
                    root,
                    owner,
                    host,
                    epoch: self.host_epoch,
                });
                self.pending = Some(PendingFileLockPass {
                    provenance: root,
                    owner,
                    host,
                    epoch: self.host_epoch,
                    native,
                    info,
                    actions,
                });
            }
        }
    }

    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<FileLockDialogEffect> {
        self.poll_reveal_work();
        let mut effects = Vec::new();
        if self.pending.as_ref().is_some_and(|pending| {
            pending.provenance.was_retained(ctx)
                && pending.owner.was_retained(ctx)
                && pending
                    .actions
                    .iter()
                    .any(|action| matches!(action, FileLockAction::Close))
        }) {
            self.external_pending = None;
        }
        let external = self.external_pending.is_some();
        if let Some(pending) = self.external_pending.take().or_else(|| self.pending.take()) {
            if !pending.provenance.was_retained(ctx)
                || !pending.owner.was_retained(ctx)
                || (!external
                    && (pending.host != self.host.identity() || pending.epoch != self.host_epoch))
            {
                if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                    > pending.provenance.frame_nr
                {
                    self.pending = None;
                } else {
                    if external {
                        self.external_pending = Some(pending);
                    } else {
                        self.pending = Some(pending);
                    }
                }
            } else {
                if !external {
                    let identity = self.native_identity(pending.host);
                    if let Some(info) = pending.info.as_ref() {
                        super::utility_dialog_size::apply_native_utility_geometry_restore(
                            ctx,
                            egui::ViewportClass::Immediate,
                            true,
                            WINDOW_TITLE,
                            identity,
                            info,
                            egui::vec2(760.0, 520.0),
                            Some(egui::vec2(520.0, 340.0)),
                            None,
                        );
                        if self.host_focus {
                            ctx.send_viewport_cmd_to(
                                identity.viewport_id,
                                egui::ViewportCommand::Focus,
                            );
                        }
                    } else if self.host_focus {
                        if let Some(layer) = self.root_layer() {
                            ctx.move_to_top(layer);
                        }
                    }
                    self.host_focus = false;
                    if pending.host.host == super::utility_host::UtilityPresentation::Popout {
                        self.host.complete_bootstrap(
                            pending.host,
                            (!pending.native).then(|| {
                                egui::LayerId::new(
                                    egui::Order::Middle,
                                    egui::Id::new(Some(WINDOW_TITLE)),
                                )
                            }),
                            None,
                        );
                    }
                    let copies = ctx
                        .with_plugin(|plugin: &mut super::utility_host::HostedCopyCapture| {
                            let mut accepted = Vec::new();
                            let mut other = Vec::new();
                            for (receipt, copies) in plugin.captured.drain(..) {
                                if receipt.owner.viewport_id != pending.owner.viewport_id {
                                    other.push((receipt, copies));
                                } else if receipt.root == pending.provenance
                                    && receipt.owner == pending.owner
                                    && receipt.host == pending.host
                                    && receipt.epoch == pending.epoch
                                {
                                    accepted.extend(copies);
                                }
                            }
                            plugin.captured = other;
                            accepted
                        })
                        .unwrap_or_default();
                    if !pending.actions.iter().any(|action| {
                        matches!(action, FileLockAction::Close | FileLockAction::Host(_))
                    }) {
                        effects.extend(copies.into_iter().map(FileLockDialogEffect::CopyLabel));
                    }
                }
                for action in pending.actions {
                    match action {
                        FileLockAction::Host(next) => {
                            if self.host.transition(next) {
                                let key =
                                    crate::settings::UtilityToolKey::parse("file-lock").expect("stable utility key");
                                super::utility_host::remember_presentation(ctx, key, next);
                                self.latest_pass = None;
                                if next == super::utility_host::UtilityPresentation::Docked {
                                    let identity = self.native_identity(pending.host);
                                    super::utility_dialog_size::close_native_utility_geometry_viewport(ctx,identity);
                                    ctx.send_viewport_cmd_to(
                                        identity.viewport_id,
                                        egui::ViewportCommand::Close,
                                    );
                                    effects.push(FileLockDialogEffect::ShowRoot);
                                }
                                self.host_focus = true;
                            }
                        }
                        FileLockAction::Open(path) => {
                            if !self.open {
                                self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
                            }
                            self.open = true;
                            if let Some(path) = path {
                                self.apply_selected_path(path);
                            }
                            effects.push(FileLockDialogEffect::FocusPanel);
                        }
                        FileLockAction::Prefill(path) => {
                            if !self.open {
                                self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
                            }
                            self.open = true;
                            self.apply_selected_path(path);
                            effects.push(FileLockDialogEffect::FocusPanel);
                        }
                        FileLockAction::ChooseFile => {
                            let before_path = self.selected_path.clone();
                            let before_submitted = self
                                .submitted
                                .as_ref()
                                .map(|s| (s.generation, s.path.clone()));
                            let before_generation = self.session.view().generation();
                            let identity =
                                pending.native.then(|| self.native_identity(pending.host));
                            #[cfg(windows)]
                            let hwnd =
                                super::utility_dialog_size::utility_picker_hwnd(ctx, identity);
                            #[cfg(not(windows))]
                            let hwnd = None;
                            if let Some(path) = self.pick_file(hwnd) {
                                if !self.open
                                    || self.host_epoch != pending.epoch
                                    || self.host.identity() != pending.host
                                    || self.selected_path != before_path
                                    || self
                                        .submitted
                                        .as_ref()
                                        .map(|s| (s.generation, s.path.clone()))
                                        != before_submitted
                                    || self.session.view().generation() != before_generation
                                {
                                    continue;
                                }
                                #[cfg(all(windows, not(test)))]
                                if hwnd.is_none()
                                    || hwnd
                                        != super::utility_dialog_size::utility_picker_hwnd(
                                            ctx, identity,
                                        )
                                {
                                    continue;
                                }
                                if self.apply_selected_path(path) {
                                    self.notice = None;
                                }
                            }
                        }
                        FileLockAction::Inspect(path) => {
                            if self.selected_path.as_ref() != Some(&path) {
                                continue;
                            }
                            self.submit_inspection(path);
                        }
                        FileLockAction::Refresh { generation, path } => {
                            if generation != self.session.view().generation()
                                || self.selected_path.as_ref() != Some(&path)
                            {
                                continue;
                            }
                            self.submit_inspection(path);
                        }
                        FileLockAction::Cancel { generation } => {
                            if generation == self.session.view().generation() {
                                match self.session.cancel() {
                                    Ok(_) => {
                                        self.invalidate_selection();
                                        self.cancel_reveal();
                                        self.notice = Some("Inspection cancelled.".into());
                                    }
                                    Err(error) => self.notice = Some(error.to_string()),
                                }
                            }
                        }
                        FileLockAction::SelectProcess {
                            generation,
                            process_id,
                            creation_filetime,
                        } => {
                            let key = (generation, process_id, creation_filetime);
                            if self.process_is_current(key) {
                                if self.selected_process != Some(key) {
                                    self.cancel_reveal();
                                }
                                self.selected_process = Some(key);
                            }
                        }
                        FileLockAction::CopySnapshot { generation } => {
                            if let Some(inspection) = self.current_inspection(&self.session.view())
                                && self
                                    .submitted
                                    .as_ref()
                                    .is_some_and(|submitted| submitted.generation == generation)
                            {
                                effects.push(FileLockDialogEffect::CopySnapshot(copy_snapshot(
                                    inspection,
                                )));
                            }
                        }
                        FileLockAction::Reveal(request) => {
                            if self.reveal_request_is_current(&request) {
                                self.start_reveal(request, ctx);
                            }
                        }
                        FileLockAction::Close => {
                            self.remember_final_presentation(ctx);
                            self.retire_host(ctx);
                            self.external_pending = None;
                            self.close_current_selection();
                        }
                    }
                }
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }
        if external && self.pending.is_some() {
            effects.extend(self.on_ui_frame_complete(ctx));
            return effects;
        }

        if self
            .reveal_ready
            .as_ref()
            .is_some_and(|ready| !self.reveal_request_is_current(&ready.request))
        {
            self.reveal_ready = None;
        }
        if self.ready_dispatch_allowed()
            && self
                .reveal_ready
                .as_ref()
                .is_some_and(|ready| !ready.dispatched)
            && self.latest_pass.is_some_and(|pass| {
                pass.root.was_retained(ctx)
                    && pass.owner.was_retained(ctx)
                    && pass.host == self.host.identity()
                    && pass.epoch == self.host_epoch
            })
        {
            if let Some(ready) = self.reveal_ready.as_mut() {
                ready.dispatched = true;
                effects.push(FileLockDialogEffect::RevealExecutable(
                    ready.request.clone(),
                ));
            }
        } else if self.reveal_ready.is_some() {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        effects
    }

    fn ready_dispatch_allowed(&self) -> bool {
        #[cfg(test)]
        {
            !self.test_hold_ready_dispatch
        }
        #[cfg(not(test))]
        {
            true
        }
    }
    #[cfg(test)]
    pub(crate) fn test_hold_ready_dispatch(&mut self, hold: bool) {
        self.test_hold_ready_dispatch = hold;
    }

    pub(crate) fn reveal_request_is_current(&self, request: &FileLockRevealRequest) -> bool {
        self.submitted
            .as_ref()
            .is_some_and(|submitted| submitted.generation == request.generation)
            && self.selected_process
                == Some((
                    request.generation,
                    request.process_id,
                    request.creation_filetime,
                ))
            && self
                .current_inspection(&self.session.view())
                .is_some_and(|inspection| {
                    inspection.processes.iter().any(|process| {
                        process.process_id == request.process_id
                            && process.creation_filetime == request.creation_filetime
                            && process.executable_path.as_ref() == Some(&request.executable_path)
                    })
                })
    }

    pub(crate) fn reveal_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Requested executable reveal.".into(),
            Err(error) => format!("Could not reveal the executable: {error}"),
        });
    }

    pub(crate) fn label_copy_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Copied to clipboard.".into(),
            Err(error) => format!("Could not copy text: {error}"),
        });
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Inspection snapshot copied.".into(),
            Err(error) => format!("Could not copy the inspection snapshot: {error}"),
        });
    }

    pub(crate) fn take_reveal_path_for_dispatch(
        &mut self,
        request: &FileLockRevealRequest,
    ) -> Result<Option<PathBuf>, String> {
        if !self.reveal_request_is_current(request) {
            return Ok(None);
        }
        let Some(ready) = self.reveal_ready.as_ref() else {
            return Ok(None);
        };
        if !ready.dispatched || ready.request != *request {
            return Ok(None);
        }
        if let Some(executable) = ready.executable.as_ref() {
            if executable.path() != request.executable_path.as_path() {
                self.reveal_ready = None;
                return Err("the verified executable path changed before reveal".into());
            }
            if let Err(error) = executable.ensure_process_is_alive() {
                self.reveal_ready = None;
                return Err(error);
            }
        }
        let Some(ready) = self.reveal_ready.take() else {
            return Ok(None);
        };
        Ok(Some(
            ready
                .executable
                .map(|executable| executable.path().to_path_buf())
                .unwrap_or_else(|| ready.request.executable_path),
        ))
    }

    #[cfg(test)]
    pub(super) fn test_presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    #[cfg(test)]
    pub(crate) fn test_selected_process(&self) -> Option<(u64, u32, u64)> {
        self.selected_process
    }
    #[cfg(test)]
    pub(crate) fn test_reveal_ready(&self) -> bool {
        self.reveal_ready.is_some()
    }
    #[cfg(test)]
    pub(crate) fn set_file_choice_for_test(&mut self, choice: Option<PathBuf>) {
        self.test_file_choice = Some(choice);
    }

    #[cfg(test)]
    pub(crate) fn file_picker_calls_for_test(&self) -> usize {
        self.test_file_picker_calls
    }

    #[cfg(test)]
    pub(crate) fn set_reveal_result_for_test(&mut self, result: Result<PathBuf, String>) {
        self.test_reveal_result = Some(result);
    }

    #[cfg(test)]
    pub(crate) fn set_session_for_test(&mut self, session: FileLockSession) {
        self.session.close();
        self.session = session;
    }

    #[cfg(test)]
    pub(crate) fn selected_path_for_test(&self) -> Option<&std::path::Path> {
        self.selected_path.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn has_current_inspection_for_test(&self) -> bool {
        self.current_inspection(&self.session.view()).is_some()
    }

    #[cfg(test)]
    pub(crate) fn session_view_for_test(&self) -> crate::file_lock::FileLockSessionView {
        self.session.view()
    }

    #[cfg(test)]
    pub(crate) fn notice_for_test(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_action_rect(&self, name: &str) -> Option<(egui::Rect, egui::Rect)> {
        self.test_action_rects
            .iter()
            .find(|(candidate, _, _)| candidate == name)
            .map(|(_, rect, clip)| (*rect, *clip))
    }

    #[cfg(test)]
    pub(crate) fn test_refresh_enabled(&self) -> bool {
        self.test_refresh_enabled
    }

    #[cfg(test)]
    fn record_action_rect(&mut self, name: &str, rect: egui::Rect, clip: egui::Rect) {
        self.test_action_rects.push((name.to_owned(), rect, clip));
    }

    #[cfg(not(test))]
    fn record_action_rect(&mut self, _name: &str, _rect: egui::Rect, _clip: egui::Rect) {}

    fn current_inspection<'a>(
        &self,
        view: &'a crate::file_lock::FileLockSessionView,
    ) -> Option<&'a FileLockInspection> {
        let submitted = self.submitted.as_ref()?;
        if submitted.generation != view.generation()
            || !view.inspection_is_current()
            || self.selected_path.as_ref() != Some(&submitted.path)
        {
            return None;
        }
        let inspection = view.inspection()?.as_ref();
        (inspection.path == submitted.path).then_some(inspection)
    }

    fn submitted_matches_view(&self, view: &crate::file_lock::FileLockSessionView) -> bool {
        self.submitted.as_ref().is_some_and(|submitted| {
            submitted.generation == view.generation()
                && self.selected_path.as_ref() == Some(&submitted.path)
        })
    }

    fn submit_inspection(&mut self, path: PathBuf) {
        self.invalidate_selection();
        self.cancel_reveal();
        if let Err(error) = crate::file_lock::validate_file_path(&path) {
            self.submitted = None;
            self.notice = Some(error.to_string());
            return;
        }
        match self.session.refresh(path.clone()) {
            Ok(generation) => {
                self.submitted = Some(SubmittedInspection { generation, path });
                self.notice = None;
            }
            Err(error) => {
                self.submitted = None;
                self.notice = Some(error.to_string());
            }
        }
    }

    fn apply_selected_path(&mut self, path: PathBuf) -> bool {
        if let Err(error) = crate::file_lock::validate_file_path(&path) {
            if self.selected_path.as_ref() != Some(&path) {
                self.cancel_live_inspection();
                self.selected_path = None;
                self.invalidate_selection();
                self.cancel_reveal();
            }
            self.notice = Some(error.to_string());
            return false;
        }
        if self.selected_path.as_ref() == Some(&path) {
            return true;
        }
        self.cancel_live_inspection();
        self.selected_path = Some(path);
        self.invalidate_selection();
        self.cancel_reveal();
        self.notice = None;
        true
    }

    fn invalidate_selection(&mut self) {
        self.submitted = None;
        self.selected_process = None;
        self.reveal_ready = None;
    }

    fn close_current_selection(&mut self) {
        self.cancel_live_inspection();
        self.invalidate_selection();
        self.cancel_reveal();
        self.notice = None;
        self.open = false;
    }

    fn cancel_live_inspection(&mut self) {
        if matches!(
            self.session.view().status(),
            FileLockSessionStatus::Inspecting | FileLockSessionStatus::RefreshQueued
        ) {
            let _ = self.session.cancel();
        }
    }

    fn process_is_current(&self, key: (u64, u32, u64)) -> bool {
        let (generation, process_id, creation_filetime) = key;
        self.submitted
            .as_ref()
            .is_some_and(|submitted| submitted.generation == generation)
            && self
                .current_inspection(&self.session.view())
                .is_some_and(|inspection| {
                    inspection.processes.iter().any(|process| {
                        process.process_id == process_id
                            && process.creation_filetime == creation_filetime
                    })
                })
    }

    fn start_reveal(&mut self, request: FileLockRevealRequest, ctx: &egui::Context) {
        if self.reveal_retirement_failed {
            self.notice = Some(
                "Executable validation could not be safely retired; this inspector will not start another reveal check.".into(),
            );
            return;
        }
        if self.reveal_worker.is_some() {
            self.notice = Some("An executable check is already in progress.".into());
            return;
        }
        #[cfg(test)]
        if let Some(result) = self.test_reveal_result.take() {
            match result {
                Ok(path) if path == request.executable_path => {
                    self.reveal_ready = Some(ReadyReveal {
                        request,
                        executable: None,
                        dispatched: false,
                    });
                }
                Ok(_) => self.notice = Some("The reported executable path changed.".into()),
                Err(error) => self.notice = Some(error),
            }
            return;
        }

        let permit = match crate::thread_reaper::reserve() {
            Ok(permit) => permit,
            Err(error) => {
                self.notice = Some(format!("Could not reserve executable validation: {error}"));
                return;
            }
        };
        let completion = permit.completion_notifier();
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let pid = request.process_id;
        let creation_filetime = request.creation_filetime;
        let expected_path = request.executable_path.clone();
        let join = thread::Builder::new()
            .name("file-lock-executable-check".into())
            .spawn(move || {
                let result = if worker_cancelled.load(Ordering::Acquire) {
                    Err(
                        "the selected process changed before its executable could be checked"
                            .into(),
                    )
                } else {
                    crate::file_lock::validate_reported_executable(
                        pid,
                        creation_filetime,
                        &expected_path,
                    )
                };
                if !worker_cancelled.load(Ordering::Acquire) {
                    let _ = sender.send(result);
                }
                drop(sender);
                drop(completion);
            });
        match join {
            Ok(join) => {
                let joined = Arc::new(AtomicBool::new(false));
                let retirement_unavailable = Arc::new(AtomicBool::new(false));
                let receipt = RevealRetirementReceipt {
                    joined: Arc::clone(&joined),
                    unavailable: Arc::clone(&retirement_unavailable),
                    context: ctx.clone(),
                    resolved: false,
                };
                let reap_result = permit.reap_with_callback(join, move |_| receipt.joined());
                self.reveal_worker = Some(RevealWorker {
                    request,
                    cancelled,
                    result: receiver,
                    joined,
                    retirement_unavailable,
                });
                self.notice = Some(match reap_result {
                    Ok(()) => {
                        "Checking that the reported process and executable are still current…"
                            .into()
                    }
                    Err(error) => format!(
                        "Executable validation is running, but its worker could not be safely reaped: {error}"
                    ),
                });
            }
            Err(error) => {
                self.notice = Some(format!("Could not start executable validation: {error}"))
            }
        }
    }

    fn cancel_reveal(&mut self) {
        self.reveal_ready = None;
        if let Some(work) = self.reveal_worker.as_ref() {
            work.cancelled.store(true, Ordering::Release);
        }
    }

    fn poll_reveal_work(&mut self) {
        let Some(work) = self.reveal_worker.as_ref() else {
            return;
        };
        if work.retirement_unavailable.load(Ordering::Acquire) {
            if let Some(work) = self.reveal_worker.take() {
                work.cancelled.store(true, Ordering::Release);
            }
            self.reveal_retirement_failed = true;
            self.notice = Some(
                "Executable validation could not be safely retired; this inspector will not start another reveal check.".into(),
            );
            return;
        }
        if !work.joined.load(Ordering::Acquire) {
            return;
        }
        let Some(work) = self.reveal_worker.take() else {
            return;
        };
        if work.cancelled.load(Ordering::Acquire) {
            return;
        }
        match work.result.try_recv() {
            Ok(Ok(executable)) if executable.path() == work.request.executable_path => {
                self.reveal_ready = Some(ReadyReveal {
                    request: work.request,
                    executable: Some(executable),
                    dispatched: false,
                });
                self.notice = None;
            }
            Ok(Ok(_)) => {
                self.notice = Some("The reported executable path changed.".into());
            }
            Ok(Err(error)) => self.notice = Some(error),
            Err(_) => self.notice = Some("Executable validation did not return a result.".into()),
        }
    }

    fn stage_external(&mut self, provenance: ViewportPassProvenance, action: FileLockAction) {
        self.external_pending = Some(PendingFileLockPass {
            provenance,
            owner: provenance,
            host: self.host.identity(),
            epoch: self.host_epoch,
            native: false,
            info: None,
            actions: vec![action],
        });
    }
    fn stage(&mut self, ctx: &egui::Context, actions: Vec<FileLockAction>) {
        self.stage_with_provenance(
            ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
            actions,
        );
    }

    fn stage_with_provenance(
        &mut self,
        provenance: ViewportPassProvenance,
        actions: Vec<FileLockAction>,
    ) {
        if actions.is_empty() {
            return;
        }
        if actions
            .iter()
            .any(|action| matches!(action, FileLockAction::Close))
        {
            self.pending = Some(PendingFileLockPass {
                provenance,
                owner: provenance,
                host: self.host.identity(),
                epoch: self.host_epoch,
                native: false,
                info: None,
                actions: vec![FileLockAction::Close],
            });
            return;
        }
        match self.pending.as_mut() {
            Some(pending) if pending.provenance == provenance => {
                if !pending
                    .actions
                    .iter()
                    .any(|action| matches!(action, FileLockAction::Close))
                {
                    pending.actions.extend(actions);
                }
            }
            _ => {
                self.pending = Some(PendingFileLockPass {
                    provenance,
                    owner: provenance,
                    host: self.host.identity(),
                    epoch: self.host_epoch,
                    native: false,
                    info: None,
                    actions,
                });
            }
        }
    }

    fn pick_file(&mut self, hwnd: Option<usize>) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(choice) = self.test_file_choice.take() {
            self.test_file_picker_calls = self.test_file_picker_calls.saturating_add(1);
            return choice;
        }

        #[cfg(not(test))]
        {
            #[cfg(windows)]
            {
                let Some(hwnd) = hwnd.and_then(|h| std::num::NonZeroIsize::new(h as isize)) else {
                    self.notice = Some("Cannot verify the file picker window. Focus File Lock Inspector and try again.".into());
                    return None;
                };
                rfd::FileDialog::new()
                    .set_parent(&super::add_action_dialog::NativeDialogParent(hwnd))
                    .pick_file()
            }
            #[cfg(not(windows))]
            {
                let _ = hwnd;
                rfd::FileDialog::new().pick_file()
            }
        }
        #[cfg(test)]
        {
            let _ = hwnd;
            None
        }
    }
}

fn show_inspection_status(
    ui: &mut egui::Ui,
    view: Option<&crate::file_lock::FileLockSessionView>,
    inspection: Option<&FileLockInspection>,
) {
    if let Some(inspection) = inspection {
        let inspected_at = chrono::DateTime::<chrono::Local>::from(inspection.inspected_at);
        ui.label(format!(
            "Windows inspection completed {}",
            inspected_at.format("%Y-%m-%d %H:%M:%S %Z"),
        ));
        return;
    }
    let Some(view) = view else {
        return;
    };
    match view.status() {
        FileLockSessionStatus::Idle => {}
        FileLockSessionStatus::Inspecting => {
            ui.label("Inspecting reported file users…");
        }
        FileLockSessionStatus::RefreshQueued => {
            ui.label("A newer inspection is queued…");
        }
        FileLockSessionStatus::Complete => {
            ui.label("The completed inspection is no longer current; inspect again.");
        }
        FileLockSessionStatus::Failed => {
            if let Some(error) = view.inspection_error() {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            } else if let Some(error) = view.worker_error() {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
        }
        FileLockSessionStatus::Cancelled => {
            ui.label("Inspection cancelled.");
        }
        FileLockSessionStatus::Closed => {
            ui.label("The inspection session is closed.");
        }
    };
}

fn process_details_unavailable(process: &FileLockProcess) -> bool {
    process.executable_path.is_none()
        || (process.application_name.is_empty() && process.service_name.is_empty())
}

#[cfg(test)]
pub(crate) fn process_details_unavailable_for_test(process: &FileLockProcess) -> bool {
    process_details_unavailable(process)
}

fn process_label(process: &FileLockProcess) -> String {
    if !process.application_name.is_empty() {
        process.application_name.clone()
    } else if !process.service_name.is_empty() {
        process.service_name.clone()
    } else {
        "Process details unavailable".into()
    }
}

fn format_process_start(filetime: u64) -> String {
    const WINDOWS_TO_UNIX_EPOCH_SECONDS: i64 = 11_644_473_600;
    let seconds = (filetime / 10_000_000) as i64 - WINDOWS_TO_UNIX_EPOCH_SECONDS;
    let nanoseconds = ((filetime % 10_000_000) * 100) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanoseconds)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string()
        })
        .unwrap_or_else(|| "time unavailable".into())
}

fn copy_snapshot(inspection: &FileLockInspection) -> String {
    let inspected_at = chrono::DateTime::<chrono::Local>::from(inspection.inspected_at);
    let mut output = format!(
        "Windows reported users for {}\nInspected {}\n",
        inspection.path.display(),
        inspected_at.format("%Y-%m-%d %H:%M:%S %Z"),
    );
    for process in &inspection.processes {
        use std::fmt::Write as _;
        let _ = writeln!(
            output,
            "PID {}\tFILETIME {}\t{}\t{}\t{}",
            process.process_id,
            process.creation_filetime,
            process.application_name,
            process.service_name,
            process
                .executable_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "executable unavailable".into()),
        );
    }
    if inspection.processes.is_empty() {
        output.push_str("No processes reported.\n");
    }
    output.push_str(
        "These are processes Windows reported; this list does not prove which process is denying access.\n",
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lock_ui_stale_ready_reveal_is_retired_and_failed_check_is_not_actionable() {
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().expect("isolated lexical path root");
        let selected_path = directory.path().join("selected.txt");
        let changed_path = directory.path().join("changed.txt");
        let executable_path = directory.path().join("reported.exe");
        let job_executable = executable_path.clone();
        let session = FileLockSession::with_test_job(
            move |path| {
                Ok(FileLockInspection {
                    path,
                    identity: crate::file_operations::NativeFileIdentity::from_parts(1, [4; 16]),
                    inspected_at: std::time::SystemTime::now(),
                    processes: vec![FileLockProcess {
                        process_id: 57,
                        creation_filetime: 1_330_000_000_000_000,
                        application_name: "Fixture process".into(),
                        service_name: String::new(),
                        executable_path: Some(job_executable.clone()),
                    }],
                })
            },
            || {},
        );
        let mut state = FileLockDialogState::new(&ctx);
        state.set_session_for_test(session);
        state.selected_path = Some(selected_path.clone());
        state.submit_inspection(selected_path.clone());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !state.has_current_inspection_for_test() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(state.has_current_inspection_for_test());

        let key = (
            state.submitted.as_ref().unwrap().generation,
            57,
            1_330_000_000_000_000,
        );
        state.selected_process = Some(key);
        let request = FileLockRevealRequest {
            generation: key.0,
            process_id: key.1,
            creation_filetime: key.2,
            executable_path: executable_path.clone(),
        };

        state.test_reveal_result = Some(Err("the process identity changed".into()));
        state.start_reveal(request.clone(), &ctx);
        assert!(state.reveal_ready.is_none());
        assert_eq!(
            state.notice_for_test(),
            Some("the process identity changed")
        );

        state.test_reveal_result = Some(Ok(executable_path));
        state.start_reveal(request.clone(), &ctx);
        assert!(state.reveal_ready.is_some());
        assert!(state.reveal_request_is_current(&request));
        state.reveal_ready.as_mut().unwrap().dispatched = true;

        assert!(state.apply_selected_path(changed_path));
        assert!(state.reveal_ready.is_none());
        assert!(!state.reveal_request_is_current(&request));
        assert!(
            state
                .take_reveal_path_for_dispatch(&request)
                .expect("a stale request is ignored")
                .is_none()
        );
        assert!(state.on_ui_frame_complete(&ctx).is_empty());
        state.shutdown();
    }
}
