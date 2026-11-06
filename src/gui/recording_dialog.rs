use super::LauncherApp;
use crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController;
use crate::gui::mkmacro_dialog::visual_overlay::{
    OperationId, RectanglePurpose, VisualOverlayEvent,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::ViewportPassProvenance;
use crate::mkmacro::ScreenRect;
use crate::recording::{
    CaptureIdentity, CaptureTarget, FrameRate, OutputScale, RecordingFormat, RecordingOptions,
    RecordingRequest,
};
use crate::window_catalog::{WindowCatalog, WindowDescriptor};
use eframe::egui;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

struct TargetRefreshWorker {
    result_rx: Receiver<Result<Vec<crate::recording::RecordingDisplayTarget>, String>>,
    result: Option<Result<Vec<crate::recording::RecordingDisplayTarget>, String>>,
    setup_epoch: u64,
    joined: Arc<AtomicBool>,
    settled: Arc<AtomicBool>,
    retirement_unavailable: Arc<AtomicBool>,
}

struct TargetRefreshSettled {
    settled: Arc<AtomicBool>,
    context: egui::Context,
}

impl Drop for TargetRefreshSettled {
    fn drop(&mut self) {
        self.settled.store(true, Ordering::Release);
        self.context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

struct TargetRefreshRetirementTicket {
    joined: Arc<AtomicBool>,
    retirement_unavailable: Arc<AtomicBool>,
    context: egui::Context,
    resolved: bool,
}

impl TargetRefreshRetirementTicket {
    fn after_join(mut self, panicked: bool) {
        if panicked {
            self.retirement_unavailable.store(true, Ordering::Release);
        } else {
            self.joined.store(true, Ordering::Release);
        }
        self.resolved = true;
        self.context.request_repaint_of(egui::ViewportId::ROOT);
    }
}

impl Drop for TargetRefreshRetirementTicket {
    fn drop(&mut self) {
        if !self.resolved {
            self.retirement_unavailable.store(true, Ordering::Release);
            self.context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

fn compact_controller_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("multi-launcher-recording-controller")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RecordingTargetKind {
    #[default]
    Display,
    Window,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordingSelection {
    target_kind: RecordingTargetKind,
    display: Option<(usize, String)>,
    window: Option<(usize, u32)>,
    region: Option<ScreenRect>,
    format: RecordingFormat,
    frames_per_second: u8,
    duration_seconds: u32,
    scale: OutputScale,
    include_cursor: bool,
}

impl Default for RecordingSelection {
    fn default() -> Self {
        Self {
            target_kind: RecordingTargetKind::Display,
            display: None,
            window: None,
            region: None,
            format: RecordingFormat::Gif,
            frames_per_second: RecordingFormat::Gif.default_frame_rate(),
            duration_seconds: 30,
            scale: OutputScale::Fit,
            include_cursor: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordingUiCommand {
    ChoosePath { revision: u64 },
    PickRegion { revision: u64 },
    RefreshTargets { revision: u64 },
    Stop { generation: u64 },
    Discard { generation: u64 },
    CloseSetup,
    RegionAcknowledged { revision: u64 },
}

#[derive(Clone, Copy)]
struct PendingAction {
    provenance: ViewportPassProvenance,
    command: RecordingUiCommand,
}

struct PendingSelection {
    provenance: ViewportPassProvenance,
    selection: RecordingSelection,
}

#[derive(Default)]
pub(crate) struct RecordingDialogState {
    pub(crate) open: bool,
    selection: RecordingSelection,
    selection_revision: u64,
    pending_selection: Option<PendingSelection>,
    pending_actions: Vec<PendingAction>,
    displays: Vec<crate::recording::RecordingDisplayTarget>,
    windows: Vec<WindowDescriptor>,
    windows_generation: Option<u64>,
    targets_loaded: bool,
    target_error: Option<String>,
    target_refresh: Option<TargetRefreshWorker>,
    setup_epoch: u64,
    region_operation: Option<OperationId>,
    pending_region: Option<Result<ScreenRect, String>>,
    pending_region_revision: Option<u64>,
    region_operation_revision: Option<u64>,
    compact_was_open: bool,
    preview_cache: Option<(u64, u64, CaptureTarget)>,
    #[cfg(test)]
    compact_stop_button_rect: Option<egui::Rect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedRecordingAction {
    ChoosePath { selection_revision: u64 },
    PickRegion { selection_revision: u64 },
    RefreshTargets { selection_revision: u64 },
    Stop { generation: u64 },
    Discard { generation: u64 },
    CloseSetup,
    RegionAcknowledged { selection_revision: u64 },
}

impl RecordingDialogState {
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.setup_epoch = self.setup_epoch.wrapping_add(1);
        }
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.setup_epoch = self.setup_epoch.wrapping_add(1);
        self.open = false;
        self.selection_revision = self.selection_revision.wrapping_add(1);
        self.pending_region = None;
        self.pending_region_revision = None;
        self.preview_cache = None;
    }

    pub(crate) fn selection_revision(&self) -> u64 {
        self.selection_revision
    }

    pub(crate) fn region_operation(&self) -> Option<OperationId> {
        self.region_operation
    }

    pub(crate) fn set_region_operation(&mut self, operation: Option<OperationId>, revision: u64) {
        self.region_operation = operation;
        self.region_operation_revision = operation.map(|_| revision);
    }

    pub(crate) fn region_operation_revision(&self) -> Option<u64> {
        self.region_operation_revision
    }

    pub(crate) fn has_selector_operation(&self) -> bool {
        self.region_operation.is_some()
    }

    pub(crate) fn selector_terminal(&mut self, revision: u64, result: Result<ScreenRect, String>) {
        self.region_operation = None;
        self.region_operation_revision = None;
        if revision == self.selection_revision {
            self.pending_region = Some(result);
            self.pending_region_revision = Some(revision);
        }
    }

    pub(crate) fn has_active_job(
        &self,
        controller: &crate::recording::RecordingController,
    ) -> bool {
        controller.has_active_job()
    }

    pub(crate) fn needs_compact_viewport_bootstrap(
        &self,
        controller: &crate::recording::RecordingController,
    ) -> bool {
        controller.has_active_job() && !self.compact_was_open
    }

    pub(crate) fn needs_root_ui_when_hidden(
        &self,
        controller: &crate::recording::RecordingController,
    ) -> bool {
        controller.has_active_job() || self.region_operation.is_some()
    }

    pub(crate) fn refresh_targets(&mut self, ctx: &egui::Context, window_catalog: &WindowCatalog) {
        if !self.open {
            return;
        }
        self.refresh_window_snapshot(window_catalog);
        if self.target_refresh.is_some() {
            return;
        }
        self.target_error = None;
        let permit = match crate::thread_reaper::reserve() {
            Ok(permit) => permit,
            Err(error) => {
                self.target_error = Some(format!("could not reserve display enumeration: {error}"));
                self.targets_loaded = true;
                return;
            }
        };
        let (sender, result_rx) = mpsc::sync_channel(1);
        let joined = Arc::new(AtomicBool::new(false));
        let settled = Arc::new(AtomicBool::new(false));
        let retirement_unavailable = Arc::new(AtomicBool::new(false));
        let setup_epoch = self.setup_epoch;
        let completion = permit.completion_notifier();
        let worker_context = ctx.clone();
        let worker_settled = Arc::clone(&settled);
        let join = match thread::Builder::new()
            .name("launcher-recording-targets".into())
            .spawn(move || {
                let _completion = completion;
                let _settled = TargetRefreshSettled {
                    settled: worker_settled,
                    context: worker_context.clone(),
                };
                let result = catch_unwind(AssertUnwindSafe(
                    crate::recording::enumerate_recording_displays,
                ))
                .unwrap_or_else(|_| Err("display enumeration panicked".into()));
                let _ = sender.send(result);
                worker_context.request_repaint_of(egui::ViewportId::ROOT);
            }) {
            Ok(join) => join,
            Err(error) => {
                self.target_error = Some(format!("could not start display enumeration: {error}"));
                self.targets_loaded = true;
                return;
            }
        };
        let ticket = TargetRefreshRetirementTicket {
            joined: Arc::clone(&joined),
            retirement_unavailable: Arc::clone(&retirement_unavailable),
            context: ctx.clone(),
            resolved: false,
        };
        self.target_refresh = Some(TargetRefreshWorker {
            result_rx,
            result: None,
            setup_epoch,
            joined,
            settled,
            retirement_unavailable,
        });
        let _ = permit.reap_with_callback(join, move |panicked| ticket.after_join(panicked));
    }

    pub(crate) fn poll_target_refresh(
        &mut self,
        ctx: &egui::Context,
        window_catalog: &WindowCatalog,
    ) {
        if self.target_refresh.is_none() {
            return;
        }
        if self.open {
            self.refresh_window_snapshot(window_catalog);
        }
        let completed = self.target_refresh.as_mut().and_then(|worker| {
            if worker.result.is_none() {
                match worker.result_rx.try_recv() {
                    Ok(result) => worker.result = Some(result),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
                }
            }
            let unavailable = worker.retirement_unavailable.load(Ordering::Acquire);
            if worker.joined.load(Ordering::Acquire)
                || (unavailable && worker.settled.load(Ordering::Acquire))
            {
                Some((worker.result.take(), unavailable, worker.setup_epoch))
            } else {
                None
            }
        });
        let Some((result, unavailable, setup_epoch)) = completed else {
            return;
        };
        self.target_refresh = None;
        if setup_epoch != self.setup_epoch || !self.open {
            return;
        }
        if unavailable {
            self.target_error = Some("display enumeration could not be safely retired".into());
            self.targets_loaded = true;
            return;
        }
        match result {
            Some(Ok(displays)) => {
                self.displays = displays;
                self.target_error = None;
                if self.selection.display.is_none()
                    && let Some(first) = self.displays.first()
                    && let CaptureIdentity::Display {
                        hmonitor,
                        device_name,
                    } = first.target.identity()
                {
                    self.selection.display = Some((*hmonitor, device_name.clone()));
                    self.selection_revision = self.selection_revision.wrapping_add(1);
                }
                self.targets_loaded = true;
                self.preview_cache = None;
            }
            Some(Err(error)) => {
                self.displays.clear();
                self.target_error = Some(error);
                self.targets_loaded = true;
                self.preview_cache = None;
            }
            None => {
                self.displays.clear();
                self.target_error = Some("display enumeration returned no result".into());
                self.targets_loaded = true;
                self.preview_cache = None;
            }
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    fn refresh_window_snapshot(&mut self, window_catalog: &WindowCatalog) {
        let generation = window_catalog.generation();
        if self.windows_generation == Some(generation) {
            return;
        }
        self.windows = window_catalog.snapshot().as_ref().clone();
        self.windows_generation = Some(generation);
        if self.selection.window.is_none()
            && let Some(first) = self.windows.first()
        {
            self.selection.window = Some((first.hwnd, first.pid));
            self.selection_revision = self.selection_revision.wrapping_add(1);
            self.preview_cache = None;
        }
    }

    fn target_refresh_pending(&self) -> bool {
        self.target_refresh.is_some()
    }

    pub(crate) fn target_refresh_pending_for_shutdown(&self) -> bool {
        self.target_refresh.is_some()
    }

    pub(crate) fn show_setup(
        &mut self,
        ctx: &egui::Context,
        window_catalog: &WindowCatalog,
        controller: &crate::recording::RecordingController,
        visible: bool,
    ) {
        if !self.open || !visible {
            return;
        }
        self.refresh_window_snapshot(window_catalog);
        if !self.targets_loaded && self.target_refresh.is_none() {
            self.refresh_targets(ctx, window_catalog);
        }
        let pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        self.drop_expired_staging(ctx, pass);
        let mut draft = self.selection.clone();
        if let Some(Ok(region)) = self
            .pending_region
            .as_ref()
            .filter(|_| self.pending_region_revision == Some(self.selection_revision))
        {
            let matching = self
                .displays
                .iter()
                .filter(|display| display.target.target_bounds().contains(*region))
                .collect::<Vec<_>>();
            if matching.len() == 1 {
                draft.target_kind = RecordingTargetKind::Display;
                draft.display = Some(display_key(matching[0]));
                draft.region = Some(*region);
            }
        }
        let preview = self.preview_request(&draft, window_catalog);
        let mut commands = Vec::new();
        let mut keep_open = true;
        let has_job = controller.has_active_job();
        egui::Window::new("Screen Recorder")
            .id(egui::Id::new("recording-setup-window"))
            .default_size([480.0, 470.0])
            .show_utility(
                ctx,
                "screen-recording",
                egui::Id::new("Screen Recorder"),
                Some(&mut keep_open),
                crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                    crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                        egui::Vec2::new(480.0, 470.0),
                    ),
                    egui::Vec2::ZERO,
                    [None, None],
                ),
                |ui| {
                    ui.add_enabled_ui(!has_job && !self.target_refresh_pending(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Source");
                            egui::ComboBox::from_id_salt("recording-target-kind")
                                .selected_text(match draft.target_kind {
                                    RecordingTargetKind::Display => "Display / region",
                                    RecordingTargetKind::Window => "Window",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut draft.target_kind,
                                        RecordingTargetKind::Display,
                                        "Display / region",
                                    );
                                    ui.selectable_value(
                                        &mut draft.target_kind,
                                        RecordingTargetKind::Window,
                                        "Window",
                                    );
                                });
                            if ui.small_button("Refresh").clicked() {
                                commands.push(RecordingUiCommand::RefreshTargets {
                                    revision: self.selection_revision,
                                });
                            }
                        });
                        match draft.target_kind {
                            RecordingTargetKind::Display => {
                                let display_before = draft.display.clone();
                                let label = draft
                                    .display
                                    .as_ref()
                                    .and_then(|key| {
                                        self.displays
                                            .iter()
                                            .find(|display| display_key(display) == *key)
                                    })
                                    .map(|display| display.device_name.as_str())
                                    .unwrap_or("Select display");
                                egui::ComboBox::from_id_salt("recording-display")
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        for display in &self.displays {
                                            let key = display_key(display);
                                            ui.selectable_value(
                                                &mut draft.display,
                                                Some(key),
                                                display.device_name.as_str(),
                                            );
                                        }
                                    });
                                if draft.display != display_before {
                                    draft.region = None;
                                }
                                if ui.button("Select region…").clicked() {
                                    commands.push(RecordingUiCommand::PickRegion {
                                        revision: self.selection_revision,
                                    });
                                }
                                if let Some(region) = draft.region {
                                    ui.label(format!(
                                        "Region: {} × {} at {}, {}",
                                        region.width, region.height, region.x, region.y
                                    ));
                                    if ui.small_button("Use full display").clicked() {
                                        draft.region = None;
                                    }
                                }
                            }
                            RecordingTargetKind::Window => {
                                let label = draft
                                    .window
                                    .and_then(|key| {
                                        self.windows.iter().find(|window| window_key(window) == key)
                                    })
                                    .map(|window| window.title.as_str())
                                    .unwrap_or("Select window");
                                egui::ComboBox::from_id_salt("recording-window")
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        for window in &self.windows {
                                            ui.selectable_value(
                                                &mut draft.window,
                                                Some(window_key(window)),
                                                &window.title,
                                            );
                                        }
                                    });
                                if draft.region.take().is_some() {
                                    // Region selection belongs to display capture.
                                }
                            }
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Format");
                            egui::ComboBox::from_id_salt("recording-format")
                                .selected_text(format_label(draft.format))
                                .show_ui(ui, |ui| {
                                    for format in [RecordingFormat::Gif, RecordingFormat::Mp4] {
                                        if ui
                                            .selectable_value(
                                                &mut draft.format,
                                                format,
                                                format_label(format),
                                            )
                                            .changed()
                                        {
                                            draft.frames_per_second = format.default_frame_rate();
                                        }
                                    }
                                });
                            ui.label("FPS");
                            ui.add(
                                egui::DragValue::new(&mut draft.frames_per_second).range(1..=60),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Output scale");
                            egui::ComboBox::from_id_salt("recording-output-scale")
                                .selected_text(scale_label(draft.scale))
                                .show_ui(ui, |ui| {
                                    for scale in
                                        [OutputScale::Fit, OutputScale::Half, OutputScale::Quarter]
                                    {
                                        ui.selectable_value(
                                            &mut draft.scale,
                                            scale,
                                            scale_label(scale),
                                        );
                                    }
                                });
                            ui.label("Duration (seconds)");
                            ui.add(
                                egui::DragValue::new(&mut draft.duration_seconds).range(1..=120),
                            );
                        });
                        ui.checkbox(&mut draft.include_cursor, "Include cursor");
                    });

                    if self.target_refresh_pending() {
                        ui.label("Refreshing display targets…");
                    }

                    if let Some(error) = self.target_error.as_deref() {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(Ok(region)) = self.pending_region.as_ref() {
                        ui.label(format!(
                            "Selected region: {} × {} at {}, {}",
                            region.width, region.height, region.x, region.y
                        ));
                    }
                    if let Some(Err(error)) = self.pending_region.as_ref() {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(request) = preview.as_ref() {
                        let (width, height) = request.encoded_dimensions();
                        ui.label(format!(
                            "Frozen canvas: {} × {} → {} × {}",
                            request.canvas().width(),
                            request.canvas().height(),
                            width,
                            height
                        ));
                    } else {
                        ui.label("Choose a valid display or window to preview output size.");
                    }

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!has_job, egui::Button::new("Choose output and record…"))
                            .clicked()
                        {
                            commands.push(RecordingUiCommand::ChoosePath {
                                revision: self.selection_revision,
                            });
                        }
                        if has_job {
                            ui.label("The current recording owns its frozen settings.");
                        }
                        if let Some(error) = self.target_error.as_deref() {
                            ui.label(error);
                        }
                    });
                },
            );
        if !keep_open {
            commands.push(RecordingUiCommand::CloseSetup);
        }
        if self.pending_region.is_some() {
            commands.push(RecordingUiCommand::RegionAcknowledged {
                revision: self.selection_revision,
            });
        }
        if draft != self.selection {
            self.stage_selection(pass, draft);
        }
        for command in commands {
            self.stage_action(pass, command);
        }
    }

    pub(crate) fn show_compact_controller(
        &mut self,
        ctx: &egui::Context,
        controller: &crate::recording::RecordingController,
        actions_enabled: bool,
    ) {
        let snapshot = controller.snapshot();
        let Some(generation) = snapshot.active_generation else {
            self.pending_actions
                .retain(|pending| pending.provenance.viewport_id != compact_controller_id());
            if self.compact_was_open {
                ctx.send_viewport_cmd_to(compact_controller_id(), egui::ViewportCommand::Close);
                self.compact_was_open = false;
            }
            return;
        };
        self.compact_was_open = true;
        let mut staged_action = None;
        #[cfg(test)]
        let mut compact_stop_button_rect = None;
        let state = snapshot.session.state().clone();
        let builder = egui::ViewportBuilder::default()
            .with_title("Recording")
            .with_inner_size([300.0, 128.0])
            .with_resizable(false);
        ctx.show_viewport_immediate(compact_controller_id(), builder, |child, class| {
            let provenance =
                ViewportPassProvenance::current(child.ctx(), child.ctx().viewport_id());
            let independent = class == egui::ViewportClass::Immediate
                && child.ctx().viewport_id() == compact_controller_id();
            let mut command = None;
            let mut controls_focused = false;
            child.vertical(|ui| {
                let status = match &state {
                    crate::recording::RecorderState::Selecting => {
                        match snapshot.preparation.as_ref() {
                            Some(crate::recording::RecordingWriterPreparation::Pending) | None => {
                                "Preparing encoder…".to_owned()
                            }
                            Some(crate::recording::RecordingWriterPreparation::Ready) => {
                                "Preparing capture…".to_owned()
                            }
                            Some(crate::recording::RecordingWriterPreparation::Failed(error)) => {
                                format!("Could not prepare: {error}")
                            }
                        }
                    }
                    crate::recording::RecorderState::Countdown { remaining } => {
                        format!("Starting in {:.0}…", remaining.as_secs_f64().ceil())
                    }
                    crate::recording::RecorderState::Recording { elapsed, .. } => {
                        format!("Recording · {}s", elapsed.as_secs())
                    }
                    crate::recording::RecorderState::Finalizing { .. } => {
                        "Finalizing output…".into()
                    }
                    crate::recording::RecorderState::Completed(_) => "Saved".into(),
                    crate::recording::RecorderState::Failed(error) => {
                        format!("Failed: {error}")
                    }
                    crate::recording::RecorderState::Cancelled => "Discarding…".into(),
                    crate::recording::RecorderState::Idle => "Waiting".into(),
                };
                ui.label(status);
                if let Some(outcome) = snapshot.writer_outcome.as_ref() {
                    ui.label(match outcome {
                        crate::recording::RecordingControllerOutcome::Saved(path) => {
                            format!("Saved {}", path.display())
                        }
                        crate::recording::RecordingControllerOutcome::Discarded => {
                            "Output discarded".into()
                        }
                        crate::recording::RecordingControllerOutcome::Failed(error) => {
                            format!("Output failed: {error}")
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let is_recording =
                        matches!(&state, crate::recording::RecorderState::Recording { .. });
                    let stop =
                        ui.add_enabled(actions_enabled && is_recording, egui::Button::new("Stop"));
                    controls_focused |= stop.has_focus();
                    #[cfg(test)]
                    if is_recording && stop.enabled() {
                        compact_stop_button_rect = Some(stop.rect);
                    }
                    if stop.clicked() {
                        command = Some(RecordingUiCommand::Stop { generation });
                    }
                    let discard = ui.add_enabled(
                        actions_enabled
                            && !snapshot.session.finalization_claimed()
                            && !snapshot.writer_retired,
                        egui::Button::new("Discard"),
                    );
                    controls_focused |= discard.has_focus();
                    if discard.clicked() {
                        command = Some(RecordingUiCommand::Discard { generation });
                    }
                });
            });
            if independent && child.input(|input| input.viewport().close_requested()) {
                child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            let escape_belongs_to_controller = if independent {
                child.input(|input| input.focused)
            } else {
                controls_focused
            };
            if escape_belongs_to_controller
                && matches!(&state, crate::recording::RecorderState::Countdown { .. })
                && child
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                command = Some(RecordingUiCommand::Discard { generation });
            }
            if let Some(command) = command {
                staged_action = Some((provenance, command));
            }
        });
        #[cfg(test)]
        {
            self.compact_stop_button_rect = compact_stop_button_rect;
        }
        if let Some((provenance, command)) = staged_action {
            self.stage_action(provenance, command);
        }
    }

    pub(crate) fn complete_retained(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<RetainedRecordingAction> {
        let mut accepted = Vec::new();
        let mut committed_selection_pass = None;
        if let Some(pending) = self.pending_selection.take() {
            if pending.provenance.was_retained(ctx) {
                if pending.selection != self.selection {
                    let previous_revision = self.selection_revision;
                    self.selection = pending.selection;
                    self.selection_revision = self.selection_revision.wrapping_add(1);
                    if self.pending_region_revision == Some(previous_revision) {
                        self.pending_region_revision = Some(self.selection_revision);
                    }
                    self.preview_cache = None;
                    committed_selection_pass = Some(pending.provenance);
                }
            } else if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                <= pending.provenance.frame_nr
            {
                self.pending_selection = Some(pending);
            }
        }
        let mut retained = Vec::new();
        let mut waiting = Vec::new();
        for pending in std::mem::take(&mut self.pending_actions) {
            if pending.provenance.was_retained(ctx) {
                retained.push(pending);
            } else if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                <= pending.provenance.frame_nr
            {
                waiting.push(pending);
            }
        }
        self.pending_actions = waiting;
        for pending in retained {
            let command_revision = match pending.command {
                RecordingUiCommand::ChoosePath { revision }
                | RecordingUiCommand::PickRegion { revision }
                | RecordingUiCommand::RefreshTargets { revision }
                | RecordingUiCommand::RegionAcknowledged { revision } => Some(revision),
                _ => None,
            };
            let effective_revision = command_revision.and_then(|revision| {
                if revision == self.selection_revision {
                    Some(revision)
                } else if committed_selection_pass == Some(pending.provenance)
                    && revision.wrapping_add(1) == self.selection_revision
                {
                    Some(self.selection_revision)
                } else {
                    None
                }
            });
            if command_revision.is_some() && effective_revision.is_none() {
                continue;
            }
            accepted.push(match pending.command {
                RecordingUiCommand::ChoosePath { .. } => RetainedRecordingAction::ChoosePath {
                    selection_revision: effective_revision.unwrap_or(self.selection_revision),
                },
                RecordingUiCommand::PickRegion { .. } => RetainedRecordingAction::PickRegion {
                    selection_revision: effective_revision.unwrap_or(self.selection_revision),
                },
                RecordingUiCommand::RefreshTargets { .. } => {
                    RetainedRecordingAction::RefreshTargets {
                        selection_revision: effective_revision.unwrap_or(self.selection_revision),
                    }
                }
                RecordingUiCommand::Stop { generation } => {
                    RetainedRecordingAction::Stop { generation }
                }
                RecordingUiCommand::Discard { generation } => {
                    RetainedRecordingAction::Discard { generation }
                }
                RecordingUiCommand::CloseSetup => RetainedRecordingAction::CloseSetup,
                RecordingUiCommand::RegionAcknowledged { revision } => {
                    let revision = effective_revision.unwrap_or(revision);
                    if self.pending_region_revision == Some(revision) {
                        self.pending_region = None;
                        self.pending_region_revision = None;
                    }
                    RetainedRecordingAction::RegionAcknowledged {
                        selection_revision: revision,
                    }
                }
            });
        }
        accepted
    }

    pub(crate) fn build_request(
        &mut self,
        window_catalog: &WindowCatalog,
    ) -> Result<RecordingRequest, String> {
        self.build_request_for(&self.selection.clone(), window_catalog)
    }

    fn build_request_for(
        &self,
        selection: &RecordingSelection,
        window_catalog: &WindowCatalog,
    ) -> Result<RecordingRequest, String> {
        let (target, region) = match selection.target_kind {
            RecordingTargetKind::Display => {
                let (hmonitor, device) = selection
                    .display
                    .as_ref()
                    .ok_or_else(|| "select a display before recording".to_owned())?;
                let display = self
                    .displays
                    .iter()
                    .find(|display| {
                        matches!(
                            display.target.identity(),
                            CaptureIdentity::Display {
                                hmonitor: current_hmonitor,
                                device_name: current_device,
                            } if current_hmonitor == hmonitor && current_device == device
                        )
                    })
                    .ok_or_else(|| "the selected display changed; refresh targets".to_owned())?;
                if let Some(region) = selection.region {
                    let matching = self
                        .displays
                        .iter()
                        .filter(|candidate| candidate.target.target_bounds().contains(region))
                        .collect::<Vec<_>>();
                    if matching.len() != 1 {
                        return Err("the selected region must lie on exactly one display".into());
                    }
                    if display_key(matching[0]) != (*hmonitor, device.clone()) {
                        return Err("the selected region belongs to a different display".into());
                    }
                    (display.target.clone(), region)
                } else {
                    (display.target.clone(), display.target.target_bounds())
                }
            }
            RecordingTargetKind::Window => {
                let (hwnd, pid) = selection
                    .window
                    .ok_or_else(|| "select a window before recording".to_owned())?;
                let live = window_catalog
                    .describe_current(hwnd)
                    .filter(|window| window.pid == pid)
                    .ok_or_else(|| "the selected window is no longer available".to_owned())?;
                let target = crate::recording::describe_recording_window(live.hwnd, live.pid)?;
                let region = target.target_bounds();
                (target, region)
            }
        };
        make_request(selection, target, region)
    }

    fn preview_request(
        &mut self,
        selection: &RecordingSelection,
        window_catalog: &WindowCatalog,
    ) -> Option<RecordingRequest> {
        if selection != &self.selection {
            return None;
        }
        let cache_key = (self.selection_revision, window_catalog.generation());
        let target = self
            .preview_cache
            .as_ref()
            .filter(|(revision, generation, _)| (*revision, *generation) == cache_key)
            .map(|(_, _, target)| target.clone())
            .or_else(|| {
                let target = match selection.target_kind {
                    RecordingTargetKind::Display => {
                        let (hmonitor, device) = selection.display.as_ref()?;
                        self.displays
                            .iter()
                            .find(|display| display_key(display) == (*hmonitor, device.clone()))?
                            .target
                            .clone()
                    }
                    RecordingTargetKind::Window => {
                        let (hwnd, pid) = selection.window?;
                        let live = window_catalog
                            .describe_current(hwnd)
                            .filter(|window| window.pid == pid)?;
                        crate::recording::describe_recording_window(live.hwnd, live.pid).ok()?
                    }
                };
                self.preview_cache = Some((cache_key.0, cache_key.1, target.clone()));
                Some(target)
            })?;
        let (target, region) = if let Some(region) = selection.region {
            let matching = self
                .displays
                .iter()
                .filter(|display| display.target.target_bounds().contains(region))
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return None;
            }
            let (hmonitor, device) = selection.display.as_ref()?;
            if display_key(matching[0]) != (*hmonitor, device.clone()) {
                return None;
            }
            (target.clone(), region)
        } else {
            (target.clone(), target.target_bounds())
        };
        make_request(selection, target, region).ok()
    }

    pub(crate) fn begin_region_pick(
        &self,
        overlay: &SharedVisualOverlayController,
    ) -> Result<OperationId, String> {
        let bounds = virtual_desktop_bounds(&self.displays)
            .ok_or_else(|| "no display bounds are available for region selection".to_owned())?;
        overlay
            .begin_recording_region_rectangle_pick(bounds)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn region_is_unambiguously_on_display(&self, region: ScreenRect) -> bool {
        self.displays
            .iter()
            .filter(|display| display.target.target_bounds().contains(region))
            .count()
            == 1
    }

    pub(crate) fn choose_save_path(&self, format: RecordingFormat) -> Option<PathBuf> {
        let extension = match format {
            RecordingFormat::Gif => "gif",
            RecordingFormat::Mp4 => "mp4",
        };
        rfd::FileDialog::new()
            .add_filter(
                format!("{} recording", extension.to_ascii_uppercase()),
                &[extension],
            )
            .set_file_name(format!("Recording.{extension}"))
            .save_file()
    }

    fn stage_selection(
        &mut self,
        provenance: ViewportPassProvenance,
        selection: RecordingSelection,
    ) {
        self.pending_selection = Some(PendingSelection {
            provenance,
            selection,
        });
    }

    fn stage_action(&mut self, provenance: ViewportPassProvenance, command: RecordingUiCommand) {
        const PENDING_ACTION_LIMIT: usize = 8;
        if self.pending_actions.len() == PENDING_ACTION_LIMIT {
            self.pending_actions.remove(0);
        }
        self.pending_actions.push(PendingAction {
            provenance,
            command,
        });
    }

    fn drop_expired_staging(&mut self, ctx: &egui::Context, current: ViewportPassProvenance) {
        if self.pending_selection.as_ref().is_some_and(|pending| {
            pending.provenance.viewport_id == current.viewport_id
                && pending.provenance.frame_nr < current.frame_nr
        }) {
            self.pending_selection = None;
        }
        self.pending_actions.retain(|pending| {
            ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                <= pending.provenance.frame_nr
        });
    }
}

impl LauncherApp {
    pub(crate) fn open_recording_setup(&mut self) {
        self.recording_dialog.open();
        self.focus_panel(super::Panel::RecordingDialog);
        self.request_launcher_state(Some(true), Some(true));
    }

    pub(crate) fn poll_recording_region(&mut self, ctx: &egui::Context) {
        let Some(expected) = self.recording_dialog.region_operation() else {
            return;
        };
        let overlay = self.mkmacro_dialog.visual_overlay_controller();
        let Some(event) = overlay.poll_recording_region_rectangle_event(expected) else {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        };
        let result = match event {
            VisualOverlayEvent::RectangleConfirmed {
                operation_id,
                purpose: RectanglePurpose::RecordingRegion,
                rect,
            } if operation_id == expected => {
                if self
                    .recording_dialog
                    .region_is_unambiguously_on_display(rect)
                {
                    Ok(rect)
                } else {
                    Err("the selected region must lie on exactly one display".into())
                }
            }
            VisualOverlayEvent::Cancelled { operation_id } if operation_id == expected => {
                Err("region selection cancelled".into())
            }
            VisualOverlayEvent::Expired { operation_id } if operation_id == expected => {
                Err("region selection expired".into())
            }
            VisualOverlayEvent::Error {
                operation_id,
                error,
            } if operation_id == expected => Err(error.to_string()),
            _ => Err("recording region selector returned an unexpected event".into()),
        };
        let Some(revision) = self.recording_dialog.region_operation_revision() else {
            return;
        };
        self.recording_dialog.selector_terminal(revision, result);
        self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    pub(crate) fn poll_recording_targets(&mut self, ctx: &egui::Context) {
        if !self.recording_dialog.target_refresh_pending_for_shutdown() {
            return;
        }
        let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
        self.recording_dialog.poll_target_refresh(ctx, &catalog);
    }

    pub(crate) fn show_recording_setup(&mut self, ctx: &egui::Context, visible: bool) {
        let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
        self.recording_dialog
            .show_setup(ctx, &catalog, &self.recording_controller, visible);
    }

    pub(crate) fn show_recording_compact_controller(&mut self, ctx: &egui::Context) {
        self.recording_dialog.show_compact_controller(
            ctx,
            &self.recording_controller,
            !self
                .recording_controller
                .snapshot()
                .session
                .finalization_claimed(),
        );
    }

    pub(crate) fn complete_recording_dialog(&mut self, ctx: &egui::Context) {
        let actions = self.recording_dialog.complete_retained(ctx);
        for action in actions {
            match action {
                RetainedRecordingAction::ChoosePath { selection_revision } => {
                    self.choose_recording_output(selection_revision);
                }
                RetainedRecordingAction::PickRegion { selection_revision } => {
                    if selection_revision != self.recording_dialog.selection_revision() {
                        continue;
                    }
                    if self.recording_start_conflict().is_some()
                        || self
                            .mkmacro_dialog
                            .visual_overlay_controller()
                            .has_pending_selector()
                    {
                        self.report_error_message(
                            "recording.region",
                            "Finish the active screen selector before choosing a recording region",
                        );
                        continue;
                    }
                    match self
                        .recording_dialog
                        .begin_region_pick(&self.mkmacro_dialog.visual_overlay_controller())
                    {
                        Ok(operation) => {
                            self.recording_dialog
                                .set_region_operation(Some(operation), selection_revision);
                            ctx.request_repaint_after(Duration::from_millis(50));
                        }
                        Err(error) => self.report_error_message("recording.region", error),
                    }
                }
                RetainedRecordingAction::RefreshTargets { selection_revision } => {
                    if selection_revision == self.recording_dialog.selection_revision() {
                        let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
                        self.recording_dialog.refresh_targets(ctx, &catalog);
                    }
                }
                RetainedRecordingAction::Stop { generation } => {
                    match self.recording_controller.stop_generation(generation) {
                        Ok(true) => ctx.request_repaint(),
                        Ok(false) => {}
                        Err(error) => self.report_error_message("recording.stop", error),
                    }
                }
                RetainedRecordingAction::Discard { generation } => {
                    match self.recording_controller.discard(generation) {
                        Ok(true) => ctx.request_repaint(),
                        Ok(false) => {}
                        Err(error) => self.report_error_message("recording.discard", error),
                    }
                }
                RetainedRecordingAction::CloseSetup => self.close_recording_setup(),
                RetainedRecordingAction::RegionAcknowledged { .. } => {}
            }
        }
    }

    fn choose_recording_output(&mut self, selection_revision: u64) {
        if selection_revision != self.recording_dialog.selection_revision() {
            return;
        }
        if !self.recording_controller.can_start() {
            self.report_error_message(
                "recording.start",
                "The previous recording is still being retired",
            );
            return;
        }
        if let Some(reason) = self.recording_start_conflict() {
            self.report_error_message("recording.start", reason);
            return;
        }
        let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
        let request = match self.recording_dialog.build_request(&catalog) {
            Ok(request) => request,
            Err(error) => {
                self.report_error_message("recording.start", error);
                return;
            }
        };
        let path = self
            .recording_dialog
            .choose_save_path(request.options().format());
        self.complete_recording_output_choice(selection_revision, request, path);
    }

    fn complete_recording_output_choice(
        &mut self,
        selection_revision: u64,
        request: RecordingRequest,
        path: Option<PathBuf>,
    ) {
        let Some(path) = path else {
            return;
        };
        if selection_revision != self.recording_dialog.selection_revision() {
            return;
        }
        if !self.recording_controller.can_start() {
            self.report_error_message(
                "recording.start",
                "The previous recording is still being retired",
            );
            return;
        }
        if let Some(reason) = self.recording_start_conflict() {
            self.report_error_message("recording.start", reason);
            return;
        }
        if let Err(error) = crate::recording::validate_recording_target(request.target()) {
            self.report_error_message("recording.start", error);
            return;
        }
        match self
            .recording_controller
            .prepare(request, path, &self.activity_center)
        {
            Ok(_) => self.egui_ctx.request_repaint_of(egui::ViewportId::ROOT),
            Err(error) => self.report_error_message("recording.start", error),
        }
    }

    pub(crate) fn close_recording_setup(&mut self) {
        if let Some(operation) = self.recording_dialog.region_operation() {
            self.mkmacro_dialog
                .visual_overlay_controller()
                .cancel_recording_region_operation(operation);
        }
        self.recording_dialog.close();
        self.panel_states.recording_dialog = false;
        self.panel_stack
            .retain(|panel| *panel != super::Panel::RecordingDialog);
    }
}

fn make_request(
    selection: &RecordingSelection,
    target: CaptureTarget,
    region: ScreenRect,
) -> Result<RecordingRequest, String> {
    let frame_rate =
        FrameRate::new(selection.frames_per_second).map_err(|error| error.to_string())?;
    let options = RecordingOptions::new(
        selection.format,
        frame_rate,
        Duration::from_secs(u64::from(selection.duration_seconds)),
        selection.include_cursor,
    )
    .map_err(|error| error.to_string())?;
    RecordingRequest::new_with_scale(target, region, options, selection.scale)
        .map_err(|error| error.to_string())
}

fn display_key(display: &crate::recording::RecordingDisplayTarget) -> (usize, String) {
    match display.target.identity() {
        CaptureIdentity::Display {
            hmonitor,
            device_name,
        } => (*hmonitor, device_name.clone()),
        CaptureIdentity::Window { .. } => (0, String::new()),
    }
}

fn window_key(window: &WindowDescriptor) -> (usize, u32) {
    (window.hwnd, window.pid)
}

fn virtual_desktop_bounds(
    displays: &[crate::recording::RecordingDisplayTarget],
) -> Option<ScreenRect> {
    let first = displays.first()?.target.target_bounds();
    let (left, top, right, bottom) = displays.iter().skip(1).fold(
        (
            i64::from(first.x),
            i64::from(first.y),
            first.right(),
            first.bottom(),
        ),
        |(left, top, right, bottom), display| {
            let bounds = display.target.target_bounds();
            (
                left.min(i64::from(bounds.x)),
                top.min(i64::from(bounds.y)),
                right.max(bounds.right()),
                bottom.max(bounds.bottom()),
            )
        },
    );
    Some(ScreenRect::new(
        i32::try_from(left).ok()?,
        i32::try_from(top).ok()?,
        u32::try_from(right - left).ok()?,
        u32::try_from(bottom - top).ok()?,
    ))
}

fn format_label(format: RecordingFormat) -> &'static str {
    match format {
        RecordingFormat::Gif => "GIF",
        RecordingFormat::Mp4 => "MP4",
    }
}

fn scale_label(scale: OutputScale) -> &'static str {
    match scale {
        OutputScale::Fit => "Fit (maximum 1920 × 1080)",
        OutputScale::Half => "Half size",
        OutputScale::Quarter => "Quarter size",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recording::{CapturePoll, CaptureSource, CaptureStopToken, RecordingBackend};
    use std::sync::Arc;

    struct NeverBackend;

    struct NeverSource;

    struct UiBackend;

    struct UiSource {
        generation: u64,
        canvas: crate::recording::CanvasSize,
        sent_frame: bool,
    }

    impl RecordingBackend for NeverBackend {
        fn create_source(
            &self,
            _request: &RecordingRequest,
            _generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(NeverSource))
        }
    }

    impl CaptureSource for NeverSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            CapturePoll::Ended(crate::recording::RecordingEndReason::TargetLost)
        }
    }

    impl RecordingBackend for UiBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            Ok(Box::new(UiSource {
                generation,
                canvas: request.canvas(),
                sent_frame: false,
            }))
        }
    }

    impl CaptureSource for UiSource {
        fn poll(&mut self, max_wait: Duration, stop: &CaptureStopToken) -> CapturePoll {
            if !self.sent_frame {
                self.sent_frame = true;
                return CapturePoll::Frame {
                    generation: self.generation,
                    frame: crate::recording::RgbaFrame::new(
                        self.canvas,
                        vec![255; self.canvas.rgba_bytes()],
                    )
                    .unwrap(),
                };
            }
            if stop.stop_requested() {
                return CapturePoll::Ended(crate::recording::RecordingEndReason::UserStopped);
            }
            std::thread::sleep(max_wait.min(Duration::from_millis(5)));
            CapturePoll::Pending
        }
    }

    fn selected_display_state() -> (RecordingDialogState, Arc<WindowCatalog>) {
        let bounds = ScreenRect::new(-900, -240, 1400, 900);
        let region = ScreenRect::new(-701, -101, 321, 241);
        let device_name = "\\\\.\\DISPLAY-M16".to_owned();
        let display = crate::recording::RecordingDisplayTarget {
            target: CaptureTarget::display(77, device_name.clone(), bounds).unwrap(),
            device_name: device_name.clone(),
        };
        let mut state = RecordingDialogState::default();
        state.targets_loaded = true;
        state.displays.push(display);
        state.selection.display = Some((77, device_name));
        state.selection.region = Some(region);
        state.selection.format = RecordingFormat::Mp4;
        state.selection.frames_per_second = 24;
        state.selection.duration_seconds = 17;
        state.selection.scale = OutputScale::Quarter;
        state.selection.include_cursor = true;
        (state, WindowCatalog::from_snapshot(Vec::new()))
    }

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> crate::gui::LauncherApp {
        let mut settings = crate::settings::Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        crate::gui::LauncherApp::new_isolated_for_test_at(
            ctx,
            root,
            settings,
            Some(std::collections::HashSet::new()),
            Some(std::collections::HashMap::new()),
        )
    }

    #[test]
    fn recording_setup_cancelled_path_does_not_admit_work() {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let (mut state, catalog) = selected_display_state();
        let frozen = state.build_request(&catalog).unwrap();
        assert_eq!(
            frozen.target().identity(),
            &CaptureIdentity::Display {
                hmonitor: 77,
                device_name: "\\\\.\\DISPLAY-M16".into(),
            }
        );
        assert_eq!(
            frozen.target().target_bounds(),
            ScreenRect::new(-900, -240, 1400, 900)
        );
        assert_eq!(frozen.region(), ScreenRect::new(-701, -101, 321, 241));
        assert_eq!(frozen.scale(), OutputScale::Quarter);
        assert_eq!(frozen.options().format(), RecordingFormat::Mp4);
        assert_eq!(frozen.options().frame_rate().frames_per_second(), 24);
        assert_eq!(frozen.options().duration_limit(), Duration::from_secs(17));
        assert!(frozen.options().includes_cursor());
        assert_eq!(frozen.encoded_dimensions(), (82, 62));

        let second_device = "\\\\.\\DISPLAY-M16-2".to_owned();
        state
            .displays
            .push(crate::recording::RecordingDisplayTarget {
                target: CaptureTarget::display(
                    88,
                    second_device.clone(),
                    ScreenRect::new(800, 0, 900, 700),
                )
                .unwrap(),
                device_name: second_device.clone(),
            });
        state.selection.display = Some((88, second_device));
        assert!(state.build_request(&catalog).is_err());
        state.selection.display = Some((77, "\\\\.\\DISPLAY-M16".into()));
        state.displays.pop();

        state.selection.format = RecordingFormat::Gif;
        state.selection.frames_per_second = 10;
        state.selection.duration_seconds = 30;
        state.selection.scale = OutputScale::Fit;
        state.selection.include_cursor = false;
        assert_eq!(frozen.options().format(), RecordingFormat::Mp4);
        assert_eq!(frozen.scale(), OutputScale::Quarter);
        assert_eq!(frozen.options().frame_rate().frames_per_second(), 24);
        assert_eq!(frozen.options().duration_limit(), Duration::from_secs(17));
        assert!(frozen.options().includes_cursor());

        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(&ctx, directory.path());
        app.complete_recording_output_choice(0, frozen, None);
        assert!(app.recording_controller.can_start());
        assert!(!app.recording_controller.has_active_job());
        assert!(app.recording_controller.snapshot().result.is_none());
    }

    #[test]
    fn recording_ui_reciprocal_capture_guards_reject_before_mutation() {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let (mut state, catalog) = selected_display_state();
        state.selection.format = RecordingFormat::Gif;
        state.selection.frames_per_second = 10;
        let request = state.build_request(&catalog).unwrap();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(&ctx, directory.path());
        app.recording_controller =
            crate::recording::RecordingController::new(Arc::new(NeverBackend), || {});

        let destination = directory.path().join("guarded.gif");
        let generation = app
            .recording_controller
            .prepare(request.clone(), destination.clone(), &app.activity_center)
            .unwrap();
        assert!(app.recording_capture_conflict().is_some());
        app.begin_crop_screenshot();
        assert_eq!(app.crop_screenshot_operation, None);
        assert_eq!(
            app.recording_controller.snapshot().active_generation,
            Some(generation)
        );

        assert!(app.recording_controller.discard(generation).unwrap());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while app.recording_controller.has_active_job() {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }

        app.crop_screenshot_operation = Some(901);
        let blocked_destination = directory.path().join("blocked.gif");
        assert!(app.recording_start_conflict().is_some());
        app.complete_recording_output_choice(0, request, Some(blocked_destination.clone()));
        assert!(app.recording_controller.can_start());
        assert!(!blocked_destination.exists());
        app.crop_screenshot_operation = None;
    }

    #[test]
    fn recording_ui_hidden_controller_commits_only_retained_current_stop() {
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(&ctx, directory.path());
        app.recording_controller =
            crate::recording::RecordingController::new(Arc::new(UiBackend), || {});
        let stop_bridge = Arc::new(crate::recording::RecordingStopHotkeyBridge::default());
        app.recording_controller
            .attach_stop_hotkey_bridge(&stop_bridge);
        let target_bounds = ScreenRect::new(-64, -40, 32, 24);
        let target = CaptureTarget::window(11, 22, 33, target_bounds).unwrap();
        let options = crate::recording::RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            Duration::from_secs(120),
            false,
        )
        .unwrap();
        let request = RecordingRequest::new(target, target_bounds, options).unwrap();
        let generation = app
            .recording_controller
            .prepare(
                request,
                directory.path().join("hidden-controller.gif"),
                &app.activity_center,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = app.recording_controller.snapshot();
            if matches!(
                snapshot.preparation,
                Some(crate::recording::RecordingWriterPreparation::Ready)
            ) && matches!(
                snapshot.session.state(),
                crate::recording::RecorderState::Selecting
            ) {
                app.recording_controller.poll();
                break;
            }
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        loop {
            let snapshot = app.recording_controller.snapshot();
            if matches!(
                snapshot.session.state(),
                crate::recording::RecorderState::Recording { .. }
            ) && snapshot.session.frames_written() > 0
            {
                break;
            }
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(stop_bridge.capture_stoppable_generation(), Some(generation));

        crate::gui::send_event(crate::gui::WatchEvent::RecordingStopGeneration(
            generation.wrapping_add(1),
        ));
        app.process_watch_events();
        assert_eq!(stop_bridge.capture_stoppable_generation(), Some(generation));
        assert!(matches!(
            app.recording_controller.snapshot().session.state(),
            crate::recording::RecorderState::Recording { .. }
        ));

        let render_compact = |app: &mut crate::gui::LauncherApp,
                              events: Vec<egui::Event>,
                              complete_retained: bool| {
            crate::gui::render::with_immediate_viewport_renderer(&ctx, || {
                crate::gui::render::with_immediate_viewport_test_events_for(
                    compact_controller_id(),
                    events,
                    || {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(800.0, 600.0),
                                )),
                                ..Default::default()
                            },
                            |_| {
                                app.show_recording_compact_controller(&ctx);
                            },
                        );
                        if complete_retained {
                            app.complete_recording_dialog(&ctx);
                        }
                        output.drop_without_applying_deltas();
                    },
                )
            })
        };
        render_compact(&mut app, Vec::new(), true);
        let stale_pass = ViewportPassProvenance::current(&ctx, compact_controller_id());
        app.recording_dialog
            .stage_action(stale_pass, RecordingUiCommand::Discard { generation });
        render_compact(&mut app, Vec::new(), false);
        let stop_point = app
            .recording_dialog
            .compact_stop_button_rect
            .expect("the active compact controller paints its Stop control")
            .center();
        let click = |pressed| {
            vec![
                egui::Event::PointerMoved(stop_point),
                egui::Event::PointerButton {
                    pos: stop_point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        render_compact(&mut app, click(true), true);
        render_compact(&mut app, click(false), false);
        assert_eq!(app.recording_dialog.pending_actions.len(), 1);
        assert!(matches!(
            app.recording_dialog.pending_actions[0].command,
            RecordingUiCommand::Stop { generation: stopped } if stopped == generation
        ));
        app.complete_recording_dialog(&ctx);
        assert!(app.recording_dialog.pending_actions.is_empty());

        let finish_deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !matches!(
            app.recording_controller.snapshot().result,
            Some(crate::recording::RecordingControllerOutcome::Saved(_))
        ) {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < finish_deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            app.recording_controller.snapshot().result,
            Some(crate::recording::RecordingControllerOutcome::Saved(_))
        ));
        let retired_pass = ViewportPassProvenance::current(&ctx, compact_controller_id());
        app.recording_dialog
            .stage_action(retired_pass, RecordingUiCommand::Discard { generation });
        render_compact(&mut app, Vec::new(), true);
        assert!(app.recording_dialog.pending_actions.is_empty());
    }
}
