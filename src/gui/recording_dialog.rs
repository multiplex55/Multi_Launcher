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

#[derive(Default)]
struct RecorderRegionOverlay {
    service: Option<crate::gui::mkmacro_dialog::visual_overlay::NativeVisualOverlayService>,
    next_id: u64,
    preview: Option<(u64, u64)>,
    active: Option<(u64, u64)>,
    failed_generation: Option<u64>,
}

impl RecorderRegionOverlay {
    fn start(&mut self, ctx: &egui::Context, rect: ScreenRect, persistent: bool) -> Result<u64, String> {
        use crate::gui::mkmacro_dialog::visual_overlay::{NativeVisualOverlayService, VisualOverlayCommand};
        if self.service.is_none() {
            let context = ctx.clone();
            self.service = Some(NativeVisualOverlayService::start_notifying(Arc::new(move || {
                context.request_repaint_of(egui::ViewportId::ROOT);
            })).map_err(|error| format!("Could not start recording outline: {error}"))?);
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let id = self.next_id;
        self.service.as_ref().ok_or_else(|| "Recording outline service unavailable".to_owned())?.commands.send(VisualOverlayCommand::RecordingRectangle {
            operation_id: id, rect, persistent,
        }).map_err(|error| format!("Could not show recording outline: {error}"))?;
        Ok(id)
    }

    fn cancel(&mut self, id: u64) {
        if let Some(service) = &self.service {
            let _ = service.commands.send(crate::gui::mkmacro_dialog::visual_overlay::VisualOverlayCommand::Cancel {
                expected_operation_id: Some(id),
            });
        }
    }

    fn cancel_preview(&mut self) {
        if let Some((id, _)) = self.preview.take() { self.cancel(id); }
    }

    fn cancel_active(&mut self) {
        if let Some((_, id)) = self.active.take() { self.cancel(id); }
    }
}

impl Drop for RecorderRegionOverlay {
    fn drop(&mut self) {
        if let Some(service) = &mut self.service { service.shutdown_and_join(); }
    }
}

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

struct RecordingSetupFrame {
    owner: ViewportPassProvenance,
    receipt: RecordingSetupReceipt,
    info: Option<egui::ViewportInfo>,
    copies: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecordingUiCommand {
    ChoosePath { revision: u64 },
    PickRegion { revision: u64 },
    RefreshTargets { revision: u64 },
    ShowRegion { revision: u64 },
    Stop { generation: u64 },
    Discard { generation: u64 },
    CloseSetup,
    Host(super::utility_host::UtilityPresentation),
    RegionAcknowledged { revision: u64 },
}

#[derive(Clone, Copy)]
struct RecordingSetupReceipt {
    root: ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    native: bool,
}
#[derive(Clone, Copy)]
struct RecordingPopupAnchor {
    popup: egui::Id,
    layer: egui::LayerId,
    viewport: egui::ViewportId,
    epoch: u64,
}

#[derive(Clone, Copy)]
struct PendingAction {
    provenance: ViewportPassProvenance,
    setup: Option<RecordingSetupReceipt>,
    command: RecordingUiCommand,
}

struct PendingSelection {
    provenance: ViewportPassProvenance,
    setup: Option<RecordingSetupReceipt>,
    selection: RecordingSelection,
}

#[derive(Default)]
pub(crate) struct RecordingDialogState {
    pub(crate) open: bool,
    host: super::utility_host::UtilityHostState,
    presentation_prepared: bool,
    host_focus: bool,
    host_os_close_pending: bool,
    host_popups: Vec<RecordingPopupAnchor>,
    setup_pass: Option<RecordingSetupFrame>,
    accepted_chooser: Option<RecordingSetupReceipt>,
    selection: RecordingSelection,
    selection_revision: u64,
    pending_selection: Option<PendingSelection>,
    pending_actions: Vec<PendingAction>,
    displays: Vec<crate::recording::RecordingDisplayTarget>,
    windows: Vec<WindowDescriptor>,
    windows_generation: Option<u64>,
    windows_status: Option<crate::window_catalog::WindowDiscoveryStatus>,
    initial_target_request: bool,
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
    recorder_overlay: RecorderRegionOverlay,
    outline_error: Option<String>,
    selector_conflict: bool,
    #[cfg(test)]
    compact_stop_button_rect: Option<egui::Rect>,
    #[cfg(test)]
    compact_stop_button_clip: Option<egui::Rect>,
    #[cfg(test)]
    test_controls: std::collections::HashMap<&'static str, (egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_save_choice: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_chooser_calls: usize,
    #[cfg(test)]
    test_accept_target: bool,
    #[cfg(test)]
    test_copies: Vec<String>,
    #[cfg(test)]
    test_chooser_invalidation: u8,
    #[cfg(test)]
    test_last_request: Option<RecordingRequest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedRecordingAction {
    ChoosePath { selection_revision: u64 },
    PickRegion { selection_revision: u64 },
    RefreshTargets { selection_revision: u64 },
    RefreshWindows,
    ShowRegion { selection_revision: u64 },
    Stop { generation: u64 },
    Discard { generation: u64 },
    CloseSetup,
    ShowRoot,
    CopyText(String),
    RegionAcknowledged { selection_revision: u64 },
}

impl RecordingDialogState {
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.setup_epoch = self.setup_epoch.wrapping_add(1);
            self.initial_target_request = true;
        }
        self.open = true;
    }

    pub(crate) fn close(&mut self) {
        self.setup_epoch = self.setup_epoch.wrapping_add(1);
        self.open = false;
        self.initial_target_request = false;
        self.setup_pass = None;
        self.selection_revision = self.selection_revision.wrapping_add(1);
        self.pending_region = None;
        self.pending_region_revision = None;
        self.preview_cache = None;
        self.recorder_overlay.cancel_preview();
    }

    pub(crate) fn retire_recording_outline(&mut self) {
        self.recorder_overlay.cancel_preview();
        self.recorder_overlay.cancel_active();
        if let Some(service) = &self.recorder_overlay.service {
            service.request_shutdown();
        }
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
        window_catalog.request_refresh();
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
            if self.open && setup_epoch != self.setup_epoch {
                self.initial_target_request = true;
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
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

    fn refresh_window_snapshot(&mut self, window_catalog: &WindowCatalog) -> bool {
        let snapshot = window_catalog.discovery_snapshot();
        if self.windows_generation == Some(snapshot.generation)
            && self.windows_status == Some(snapshot.status)
        {
            return false;
        }
        self.windows = snapshot.windows.as_ref().clone();
        self.windows_generation = Some(snapshot.generation);
        self.windows_status = Some(snapshot.status);
        if self.selection.window.is_none()
            && let Some(first) = self.windows.first()
        {
            self.selection.window = Some((first.hwnd, first.pid));
            self.selection_revision = self.selection_revision.wrapping_add(1);
            self.preview_cache = None;
        }
        true
    }

    fn target_refresh_pending(&self) -> bool {
        self.target_refresh.is_some()
    }

    pub(crate) fn target_refresh_pending_for_shutdown(&self) -> bool {
        self.target_refresh.is_some()
    }

    fn render_setup_body(
        &mut self,
        ctx: &egui::Context,
        window_catalog: &WindowCatalog,
        controller: &crate::recording::RecordingController,
        owner_allowed: bool,
        first: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> (
        RecordingSelection,
        Vec<RecordingUiCommand>,
        bool,
        Vec<String>,
    ) {
        self.refresh_window_snapshot(window_catalog);
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
        let snapshot = controller.snapshot();
        let has_job = snapshot.active_generation.is_some();
        self.host_popups.clear();
        let output_start = ctx.output(|output| output.commands.len());
        let mut draw = |ui: &mut egui::Ui| {
            if let Some(presentation) = presentation {
                let can_move = owner_allowed
                    && !self.has_selector_operation()
                    && ui.is_visible()
                    && !ui.is_sizing_pass();
                let (button, next) =
                    super::utility_host::presentation_button(ui, presentation, can_move);
                #[cfg(test)]
                {
                    self.test_controls.insert(
                        "host-presentation",
                        (button.rect, ui.clip_rect(), button.enabled()),
                    );
                }
                if button.clicked() && first && !ctx.will_discard() {
                    commands.push(RecordingUiCommand::Host(next));
                }
            }
            ui.add_enabled_ui(owner_allowed, |ui| {
                    ui.add_enabled_ui(!has_job && !self.target_refresh_pending(), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Source");
                            let combo = egui::ComboBox::from_id_salt("recording-target-kind")
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
                            self.host_popups.push(RecordingPopupAnchor { popup:combo.response.id.with("popup"), layer:combo.response.layer_id, viewport:ui.ctx().viewport_id(), epoch:self.setup_epoch });
                            #[cfg(test)] { self.test_controls.insert("source", (combo.response.rect,ui.clip_rect(),combo.response.enabled())); }
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
                                let combo = egui::ComboBox::from_id_salt("recording-display")
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
                            self.host_popups.push(RecordingPopupAnchor { popup:combo.response.id.with("popup"), layer:combo.response.layer_id, viewport:ui.ctx().viewport_id(), epoch:self.setup_epoch });
                            #[cfg(test)] { self.test_controls.insert("display", (combo.response.rect,ui.clip_rect(),combo.response.enabled())); }
                                if draft.display != display_before {
                                    draft.region = None;
                                }
                                if ui.button("Select region…").clicked() {
                                    commands.push(RecordingUiCommand::PickRegion {
                                        revision: self.selection_revision,
                                    });
                                }
                                let show_region = ui.add_enabled(
                                    draft.region.is_some() && !has_job && !self.selector_conflict && !self.has_selector_operation() && preview.is_some(),
                                    egui::Button::new("Show region"),
                                );
                                #[cfg(test)] { self.test_controls.insert("show-region", (show_region.rect, ui.clip_rect(), show_region.enabled())); }
                                if show_region.clicked() {
                                    commands.push(RecordingUiCommand::ShowRegion { revision: self.selection_revision });
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
                                    .unwrap_or(if draft.window.is_some() { "Selected window unavailable" } else { "Select window" });
                                let combo = egui::ComboBox::from_id_salt("recording-window")
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
                            self.host_popups.push(RecordingPopupAnchor { popup:combo.response.id.with("popup"), layer:combo.response.layer_id, viewport:ui.ctx().viewport_id(), epoch:self.setup_epoch });
                            #[cfg(test)] { self.test_controls.insert("window", (combo.response.rect,ui.clip_rect(),combo.response.enabled())); }
                                match self.windows_status {
                                    Some(crate::window_catalog::WindowDiscoveryStatus::Never | crate::window_catalog::WindowDiscoveryStatus::Loading) | None => { ui.label("Loading windows…"); }
                                    Some(crate::window_catalog::WindowDiscoveryStatus::Failed) => { ui.colored_label(ui.visuals().error_fg_color, "Window discovery unavailable"); }
                                    Some(crate::window_catalog::WindowDiscoveryStatus::Ready) if self.windows.is_empty() => { ui.label("No windows available"); }
                                    _ => {}
                                }
                                if draft.region.take().is_some() {
                                    // Region selection belongs to display capture.
                                }
                            }
                        }

                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Format");
                            let combo = egui::ComboBox::from_id_salt("recording-format")
                                .selected_text(format_label(draft.format))
                                .show_ui(ui, |ui| {
                                    for format in [RecordingFormat::Gif, RecordingFormat::Mp4] {
                                        let response = ui.selectable_value(&mut draft.format, format, format_label(format));
                                        #[cfg(test)] { self.test_controls.insert(if format == RecordingFormat::Mp4 { "format-mp4" } else { "format-gif" }, (response.rect,ui.clip_rect(),response.enabled())); }
                                        if response.changed() {
                                            draft.frames_per_second = format.default_frame_rate();
                                        }
                                    }
                                });
                            self.host_popups.push(RecordingPopupAnchor { popup:combo.response.id.with("popup"), layer:combo.response.layer_id, viewport:ui.ctx().viewport_id(), epoch:self.setup_epoch });
                            #[cfg(test)] { self.test_controls.insert("format", (combo.response.rect,ui.clip_rect(),combo.response.enabled())); }
                            ui.label("FPS");
                            let fps = ui.add(egui::DragValue::new(&mut draft.frames_per_second).range(1..=60));
                            #[cfg(test)] { self.test_controls.insert("fps",(fps.rect,ui.clip_rect(),fps.enabled())); }
                            #[cfg(not(test))] let _ = fps;
                        });
                        ui.horizontal(|ui| {
                            ui.label("Output scale");
                            let combo = egui::ComboBox::from_id_salt("recording-output-scale")
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
                            self.host_popups.push(RecordingPopupAnchor { popup:combo.response.id.with("popup"), layer:combo.response.layer_id, viewport:ui.ctx().viewport_id(), epoch:self.setup_epoch });
                            #[cfg(test)] { self.test_controls.insert("scale", (combo.response.rect,ui.clip_rect(),combo.response.enabled())); }
                            ui.label("Duration (seconds)");
                            ui.add(
                                egui::DragValue::new(&mut draft.duration_seconds).range(1..=120),
                            );
                        });
                        let cursor = ui.checkbox(&mut draft.include_cursor, "Include cursor");
                        #[cfg(test)] { self.test_controls.insert("cursor",(cursor.rect,ui.clip_rect(),cursor.enabled())); }
                        #[cfg(not(test))] let _ = cursor;
                    });

                    let refresh = ui.add_enabled(!has_job, egui::Button::new("Refresh"));
                    #[cfg(test)]
                    self.test_controls.insert("refresh", (refresh.rect, ui.clip_rect(), refresh.enabled()));
                    if refresh.clicked() {
                        commands.push(RecordingUiCommand::RefreshTargets { revision: self.selection_revision });
                    }

                    if self.target_refresh_pending() {
                        ui.label("Refreshing display targets…");
                    }

                    if let Some(error) = self.target_error.as_deref() {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    if let Some(error) = self.outline_error.as_deref() {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.label("The yellow outline marks the selected display region. Windows may also show its own capture border; this app does not suppress it.");
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
                        let label = ui.label(format!(
                            "Frozen canvas: {} × {} → {} × {}",
                            request.canvas().width(),
                            request.canvas().height(),
                            width,
                            height
                        ));
                        #[cfg(test)] { self.test_controls.insert("preview-label",(label.rect,ui.clip_rect(),label.enabled())); }
                        #[cfg(not(test))] let _ = label;
                    } else {
                        ui.label("Choose a valid display or window to preview output size.");
                    }

                    ui.horizontal(|ui| {
                        let start = ui.add_enabled(!has_job, egui::Button::new("Choose output and record…"));
                        #[cfg(test)] { self.test_controls.insert("start",(start.rect,ui.clip_rect(),start.enabled())); }
                        if start.clicked() {
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
                    if let Some(result) = snapshot.result.as_ref() {
                        match result {
                            crate::recording::RecordingControllerOutcome::Saved(path) => {
                                let target_was_lost = matches!(
                                    snapshot.session.state(),
                                    crate::recording::RecorderState::Completed(summary)
                                        if summary.reason == crate::recording::RecordingEndReason::TargetLost
                                );
                                let label = if target_was_lost {
                                    format!("Saved early after the capture target was lost: {}", path.display())
                                } else {
                                    format!("Saved: {}", path.display())
                                };
                                ui.label(label);
                            }
                            crate::recording::RecordingControllerOutcome::Discarded => {
                                ui.label("Output discarded");
                            }
                            crate::recording::RecordingControllerOutcome::Failed(error) => {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    format!("Output failed: {error}"),
                                );
                            }
                        }
                    }
                    });
        };
        if let Some(ui) = child_ui {
            draw(ui);
        } else {
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
                    |ui| draw(ui),
                );
        }
        if !keep_open {
            commands.push(RecordingUiCommand::CloseSetup);
        }
        if self.pending_region.is_some() {
            commands.push(RecordingUiCommand::RegionAcknowledged {
                revision: self.selection_revision,
            });
        }
        let copies = ctx.output_mut(|output| {
            let mut copies = Vec::new();
            for command in output.commands.split_off(output_start) {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
            copies
        });
        (
            draft,
            commands,
            keep_open,
            if owner_allowed && first && !ctx.will_discard() {
                copies
            } else {
                Vec::new()
            },
        )
    }
    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            let key = crate::settings::UtilityToolKey::parse("screen-recording")
                .expect("stable utility key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host);
            self.presentation_prepared = true;
        }
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }

    pub(super) fn remember_final_presentation(&self, ctx: &egui::Context) {
        if self.open {
            let key = crate::settings::UtilityToolKey::parse("screen-recording")
                .expect("stable utility key");
            super::utility_host::remember_presentation(ctx, key, self.host.presentation());
        }
    }

    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("recording-setup-window"),
        ))
    }
    pub(crate) fn needs_setup_root_ui_when_hidden(&self) -> bool {
        self.open && self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }
    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.host.bootstrap_pending()
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host_focus = true;
        self.host.request_focus();
    }
    pub(crate) fn awaiting_root_focus(&self) -> bool {
        self.host_focus
            && self.host.presentation() == super::utility_host::UtilityPresentation::Docked
    }
    pub(crate) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("screen-recording-setup-viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse("screen-recording")
                .expect("stable recording key"),
            viewport_id: Self::viewport(),
            owner_generation: self.setup_epoch,
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
        self.host_focus = false;
        self.host_os_close_pending = false;
        self.setup_pass = None;
        self.host_popups.clear();
    }
    pub(crate) fn popup_layer(
        &self,
        ctx: &egui::Context,
        parent: egui::LayerId,
    ) -> Option<egui::LayerId> {
        let anchor = self.host_popups.iter().find(|a| {
            a.epoch == self.setup_epoch
                && a.viewport == ctx.viewport_id()
                && a.layer == parent
                && egui::Popup::is_id_open(ctx, a.popup)
        })?;
        let rect = ctx.memory(|m| m.area_rect(anchor.popup))?;
        let layer = egui::LayerId::new(egui::Order::Foreground, anchor.popup);
        let top = ctx.memory(|m| {
            m.layer_ids()
                .filter(|l| l.order == egui::Order::Foreground && m.areas().visible_last_frame(l))
                .last()
        });
        let sizing =
            egui::containers::AreaState::load(ctx, anchor.popup).is_some_and(|a| !a.interactable);
        (rect.is_positive()
            && top == Some(layer)
            && (ctx.layer_id_at(rect.center()) == Some(layer) || sizing))
            .then_some(layer)
    }
    #[cfg(test)]
    pub(super) fn test_presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    #[cfg(test)]
    pub(crate) fn test_selection(
        &self,
    ) -> (
        u64,
        u64,
        RecordingFormat,
        u8,
        u32,
        OutputScale,
        bool,
        Option<ScreenRect>,
    ) {
        (
            self.setup_epoch,
            self.selection_revision,
            self.selection.format,
            self.selection.frames_per_second,
            self.selection.duration_seconds,
            self.selection.scale,
            self.selection.include_cursor,
            self.selection.region,
        )
    }
    #[cfg(test)]
    pub(crate) fn test_install_display(&mut self) {
        let device = "fixture-display".to_owned();
        let bounds = ScreenRect::new(0, 0, 64, 48);
        self.displays = vec![crate::recording::RecordingDisplayTarget {
            target: CaptureTarget::display(77, device.clone(), bounds).unwrap(),
            device_name: device.clone(),
        }];
        self.selection.display = Some((77, device));
        self.targets_loaded = true;
        self.test_accept_target = true;
    }
    #[cfg(test)]
    pub(crate) fn test_compact_viewport() -> egui::ViewportId {
        compact_controller_id()
    }
    #[cfg(test)]
    pub(crate) fn test_compact_stop_rect(&self) -> Option<(egui::Rect, egui::Rect)> {
        self.compact_stop_button_rect.zip(self.compact_stop_button_clip)
    }
    #[cfg(test)]
    pub(crate) fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_controls.get(name).copied()
    }
    pub(crate) fn show_setup(
        &mut self,
        ctx: &egui::Context,
        catalog: &WindowCatalog,
        controller: &crate::recording::RecordingController,
        visible: bool,
    ) {
        self.show_hosted(
            ctx, catalog, controller, visible, visible, visible, true, false, true,
        );
    }
    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        catalog: &WindowCatalog,
        controller: &crate::recording::RecordingController,
        root_visible: bool,
        root_owner: bool,
        input_allowed: bool,
        first: bool,
        pinned: bool,
        shortcut_allowed: bool,
    ) {
        self.prepare_open_presentation(ctx);
        use super::utility_host::UtilityPresentation;
        if !self.open {
            return;
        }
        ctx.add_plugin(super::utility_host::HostedCopyCapture::default());
        if pinned {
            self.host_os_close_pending = false;
        }
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut staged = None;
        let mut owner = None;
        let mut info = None;
        let mut native = false;
        let mut owner_allowed = false;
        #[cfg(test)]
        self.test_controls.clear();
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let parent = egui::LayerId::new(
                    egui::Order::Middle,
                    egui::Id::new("recording-setup-window"),
                );
                let popup = self.popup_layer(ctx, parent);
                let allowed = root_owner
                    && (!egui::Popup::is_any_open(ctx) || popup.is_some())
                    && !self.has_selector_operation();
                owner_allowed = allowed;
                let shortcut = first
                    && allowed
                    && popup.is_none()
                    && !egui::Popup::is_any_open(ctx)
                    && shortcut_allowed
                    && !pinned
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let (draft, mut commands, _, copies) = self.render_setup_body(
                    ctx,
                    catalog,
                    controller,
                    allowed,
                    first,
                    None,
                    Some(host.host),
                );
                if shortcut {
                    commands.push(RecordingUiCommand::CloseSetup);
                }
                staged = Some((draft, commands, copies));
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title("Screen Recorder")
                .with_inner_size([480.0, 470.0])
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
                let parent = if is_native {
                    egui::LayerId::background()
                } else {
                    egui::LayerId::new(egui::Order::Middle, egui::Id::new("recording-setup-window"))
                };
                let popup = self.popup_layer(&c, parent);
                let allowed = if is_native {
                    input_allowed
                        && !self.has_selector_operation()
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && (!egui::Popup::is_any_open(&c) || popup.is_some())
                        && super::jump_mode::ui::focused_response_layer(&c)
                            .is_none_or(|l| l == parent || Some(l) == popup)
                } else {
                    root_owner && !self.has_selector_operation()
                };
                owner_allowed = allowed;
                if is_native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned && !self.has_selector_operation() {
                        self.host_os_close_pending = true;
                    }
                }
                let shortcut = child_first
                    && allowed
                    && !pinned
                    && popup.is_none()
                    && !egui::Popup::is_any_open(&c)
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
                        epoch: self.setup_epoch,
                        eligible: child_first && allowed && !c.will_discard(),
                    };
                    c.with_plugin(|p: &mut super::utility_host::HostedCopyCapture| {
                        p.armed.insert(c.viewport_id(), receipt);
                    });
                    egui::CentralPanel::default().show(child, |ui| {
                        let (draft, mut commands, _, copies) = self.render_setup_body(
                            &c,
                            catalog,
                            controller,
                            allowed,
                            child_first,
                            Some(ui),
                            Some(host.host),
                        );
                        if shortcut || (!pinned && self.host_os_close_pending) {
                            commands.push(RecordingUiCommand::CloseSetup);
                        }
                        staged = Some((draft, commands, copies));
                    });
                } else {
                    let (draft, mut commands, _, copies) = self.render_setup_body(
                        &c,
                        catalog,
                        controller,
                        allowed,
                        child_first,
                        None,
                        Some(host.host),
                    );
                    if shortcut {
                        commands.push(RecordingUiCommand::CloseSetup);
                    }
                    staged = Some((draft, commands, copies));
                }
            });
        }
        if let (Some(owner), Some((draft, mut commands, copies))) = (owner, staged) {
            if pinned {
                commands.retain(|command| !matches!(command, RecordingUiCommand::CloseSetup));
            }
            if first && !ctx.will_discard() && (owner_allowed || self.host_os_close_pending) {
                let receipt = RecordingSetupReceipt {
                    root,
                    host,
                    epoch: self.setup_epoch,
                    native,
                };
                self.drop_expired_staging(ctx, owner);
                if draft != self.selection {
                    self.pending_selection = Some(PendingSelection {
                        provenance: owner,
                        setup: Some(receipt),
                        selection: draft,
                    });
                }
                for command in commands {
                    self.stage_action(owner, command);
                    if let Some(last) = self.pending_actions.last_mut() {
                        last.setup = Some(receipt);
                    }
                }
                self.setup_pass = Some(RecordingSetupFrame {
                    owner,
                    receipt,
                    info,
                    copies,
                });
            }
        }
    }
    fn setup_receipt_retained(
        &self,
        ctx: &egui::Context,
        owner: ViewportPassProvenance,
        receipt: RecordingSetupReceipt,
    ) -> bool {
        owner.was_retained(ctx)
            && receipt.root.was_retained(ctx)
            && receipt.host == self.host.identity()
            && receipt.epoch == self.setup_epoch
            && self.open
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
        #[cfg(test)]
        let mut compact_stop_button_clip = None;
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
                        compact_stop_button_clip = Some(ui.clip_rect());
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
            self.compact_stop_button_clip = compact_stop_button_clip;
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
        let mut switched_to_window = false;
        if let Some(pending) = self.pending_selection.take() {
            if pending.provenance.was_retained(ctx)
                && pending.setup.is_none_or(|receipt| {
                    self.setup_receipt_retained(ctx, pending.provenance, receipt)
                })
            {
                if pending.selection != self.selection {
                    let previous_revision = self.selection_revision;
                    switched_to_window = self.selection.target_kind != RecordingTargetKind::Window
                        && pending.selection.target_kind == RecordingTargetKind::Window;
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
            if pending.setup.is_none()
                && !matches!(
                    pending.command,
                    RecordingUiCommand::Stop { .. } | RecordingUiCommand::Discard { .. }
                )
            {
                continue;
            }
            if pending.provenance.was_retained(ctx)
                && pending.setup.is_none_or(|receipt| {
                    self.setup_receipt_retained(ctx, pending.provenance, receipt)
                })
            {
                retained.push(pending);
            } else if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                <= pending.provenance.frame_nr
            {
                waiting.push(pending);
            }
        }
        self.pending_actions = waiting;
        let close_setup = retained.iter().any(|item| {
            item.setup.is_some() && matches!(item.command, RecordingUiCommand::CloseSetup)
        });
        let host_move = retained.iter().find_map(|item| {
            if item.setup.is_some() {
                if let RecordingUiCommand::Host(next) = item.command {
                    Some(next)
                } else {
                    None
                }
            } else {
                None
            }
        });
        if switched_to_window && !close_setup && host_move.is_none() {
            accepted.push(RetainedRecordingAction::RefreshWindows);
        }
        if let Some(frame) = self.setup_pass.take() {
            if self.setup_receipt_retained(ctx, frame.owner, frame.receipt) {
                let identity = self.native_identity(frame.receipt.host);
                if let Some(info) = frame.info.as_ref() {
                    super::utility_dialog_size::apply_native_utility_geometry_restore(
                        ctx,
                        egui::ViewportClass::Immediate,
                        true,
                        "Screen Recorder",
                        identity,
                        info,
                        egui::vec2(480.0, 470.0),
                        None,
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
                if frame.receipt.host.host == super::utility_host::UtilityPresentation::Popout {
                    self.host.complete_bootstrap(
                        frame.receipt.host,
                        (!frame.receipt.native).then_some(egui::LayerId::new(
                            egui::Order::Middle,
                            egui::Id::new("recording-setup-window"),
                        )),
                        None,
                    );
                }
                let copies = ctx
                    .with_plugin(|plugin: &mut super::utility_host::HostedCopyCapture| {
                        let mut found = Vec::new();
                        let mut other = Vec::new();
                        for (receipt, copies) in plugin.captured.drain(..) {
                            if receipt.owner.viewport_id != frame.owner.viewport_id {
                                other.push((receipt, copies));
                            } else if receipt.root == frame.receipt.root
                                && receipt.owner == frame.owner
                                && receipt.host == frame.receipt.host
                                && receipt.epoch == frame.receipt.epoch
                            {
                                found.extend(copies);
                            }
                        }
                        plugin.captured = other;
                        found
                    })
                    .unwrap_or_default();
                if !close_setup && host_move.is_none() {
                    accepted.extend(
                        frame
                            .copies
                            .into_iter()
                            .chain(copies)
                            .map(RetainedRecordingAction::CopyText),
                    );
                }
            }
        }
        for pending in retained {
            if pending.setup.is_some()
                && ((close_setup && !matches!(pending.command, RecordingUiCommand::CloseSetup))
                    || (!close_setup
                        && host_move.is_some()
                        && !matches!(pending.command, RecordingUiCommand::Host(_))))
            {
                continue;
            }
            if let RecordingUiCommand::Host(next) = pending.command {
                if !self.has_selector_operation() && self.host.transition(next) {
                    let key =
                        crate::settings::UtilityToolKey::parse("screen-recording").expect("stable utility key");
                    super::utility_host::remember_presentation(ctx, key, next);
                    self.host_popups.clear();
                    if next == super::utility_host::UtilityPresentation::Docked {
                        if let Some(receipt) = pending.setup {
                            let identity = self.native_identity(receipt.host);
                            super::utility_dialog_size::close_native_utility_geometry_viewport(
                                ctx, identity,
                            );
                            ctx.send_viewport_cmd_to(
                                identity.viewport_id,
                                egui::ViewportCommand::Close,
                            );
                        }
                        accepted.push(RetainedRecordingAction::ShowRoot);
                    }
                    self.host_focus = true;
                    ctx.request_repaint();
                }
                continue;
            }
            let command_revision = match pending.command {
                RecordingUiCommand::ChoosePath { revision }
                | RecordingUiCommand::PickRegion { revision }
                | RecordingUiCommand::RefreshTargets { revision }
                | RecordingUiCommand::ShowRegion { revision }
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
            if matches!(pending.command, RecordingUiCommand::ChoosePath { .. }) {
                self.accepted_chooser = pending.setup;
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
                RecordingUiCommand::ShowRegion { .. } => RetainedRecordingAction::ShowRegion {
                    selection_revision: effective_revision.unwrap_or(self.selection_revision),
                },
                RecordingUiCommand::Stop { generation } => {
                    RetainedRecordingAction::Stop { generation }
                }
                RecordingUiCommand::Discard { generation } => {
                    RetainedRecordingAction::Discard { generation }
                }
                RecordingUiCommand::CloseSetup => RetainedRecordingAction::CloseSetup,
                RecordingUiCommand::Host(_) => unreachable!("host handled before action mapping"),
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
                if !self.windows.iter().any(|window| window_key(window) == (hwnd, pid)) {
                    return Err("the selected window is no longer available".to_owned());
                }
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

    fn choose_save_path(
        &mut self,
        format: RecordingFormat,
        hwnd: Option<usize>,
    ) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(choice) = self.test_save_choice.take() {
            self.test_chooser_calls += 1;
            match std::mem::take(&mut self.test_chooser_invalidation) {
                1 => self.selection_revision = self.selection_revision.wrapping_add(1),
                2 => {
                    self.host
                        .transition(super::utility_host::UtilityPresentation::Popout);
                    self.host
                        .transition(super::utility_host::UtilityPresentation::Docked);
                }
                _ => {}
            }
            return choice;
        }
        let extension = match format {
            RecordingFormat::Gif => "gif",
            RecordingFormat::Mp4 => "mp4",
        };
        let picker = rfd::FileDialog::new()
            .add_filter(
                format!("{} recording", extension.to_ascii_uppercase()),
                &[extension],
            )
            .set_file_name(format!("Recording.{extension}"));
        #[cfg(windows)]
        let picker = {
            let hwnd = hwnd.and_then(|h| std::num::NonZeroIsize::new(h as isize))?;
            picker.set_parent(&super::add_action_dialog::NativeDialogParent(hwnd))
        };
        #[cfg(not(windows))]
        let _ = hwnd;
        picker.save_file()
    }
    #[cfg(test)]
    pub(crate) fn queue_save_choice_for_test(&mut self, path: Option<PathBuf>) {
        self.test_save_choice = Some(path);
    }
    #[cfg(test)]
    pub(crate) fn test_invalidate_next_chooser(&mut self, kind: u8) {
        self.test_chooser_invalidation = kind;
    }
    #[cfg(test)]
    pub(crate) fn test_last_request(&self) -> Option<&RecordingRequest> {
        self.test_last_request.as_ref()
    }
    #[cfg(test)]
    pub(crate) fn chooser_calls_for_test(&self) -> usize {
        self.test_chooser_calls
    }
    #[cfg(test)]
    pub(crate) fn take_test_copies(&mut self) -> Vec<String> {
        std::mem::take(&mut self.test_copies)
    }
    fn stage_action(&mut self, provenance: ViewportPassProvenance, command: RecordingUiCommand) {
        const PENDING_ACTION_LIMIT: usize = 8;
        if self.pending_actions.len() == PENDING_ACTION_LIMIT {
            self.pending_actions.remove(0);
        }
        self.pending_actions.push(PendingAction {
            provenance,
            setup: None,
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
    pub(crate) fn reconcile_recording_outline(&mut self, ctx: &egui::Context) {
        use crate::gui::mkmacro_dialog::visual_overlay::VisualOverlayEvent;
        use crate::recording::RecorderState;
        let selector_conflict = self.mkmacro_dialog.visual_overlay_controller().has_pending_selector();
        self.recording_dialog.selector_conflict = selector_conflict;
        let snapshot = self.recording_controller.snapshot();
        let desired = if !snapshot.session.stop_pending()
            && matches!(snapshot.session.state(), RecorderState::Countdown { .. } | RecorderState::Recording { .. })
        {
            snapshot.active_generation.and_then(|generation| snapshot.session.request().and_then(|request| {
                matches!(request.target().identity(), CaptureIdentity::Display { .. })
                    .then_some((generation, request.region()))
            }))
        } else { None };
        let overlay = &mut self.recording_dialog.recorder_overlay;
        let mut error_changed = false;
        let mut failure_to_report = None;
        if overlay.preview.is_some_and(|(_, revision)| {
            !self.recording_dialog.open || revision != self.recording_dialog.selection_revision || selector_conflict || desired.is_some()
        }) {
            overlay.cancel_preview();
        }
        let events: Vec<_> = overlay.service.as_ref().map(|service| service.events.try_iter().collect()).unwrap_or_default();
        for event in events {
            match event {
                VisualOverlayEvent::Error { operation_id, error } => {
                    if overlay.active.is_some_and(|(_, id)| id == operation_id) {
                        overlay.failed_generation = overlay.active.take().map(|(generation, _)| generation);
                        self.recording_dialog.outline_error = Some(format!("Recording outline unavailable: {error}"));
                        failure_to_report = self.recording_dialog.outline_error.clone();
                        error_changed = true;
                    } else if overlay.preview.is_some_and(|(id, _)| id == operation_id) {
                        overlay.preview = None;
                        self.recording_dialog.outline_error = Some(format!("Region preview unavailable: {error}"));
                        failure_to_report = self.recording_dialog.outline_error.clone();
                        error_changed = true;
                    }
                }
                VisualOverlayEvent::Expired { operation_id } | VisualOverlayEvent::Cancelled { operation_id }
                    if overlay.preview.is_some_and(|(id, _)| id == operation_id) => {
                    overlay.preview = None;
                }
                _ => {}
            }
        }
        if overlay.active.map(|(generation, _)| generation) != desired.map(|(generation, _)| generation) {
            overlay.cancel_active();
        }
        if let Some((generation, rect)) = desired
            && overlay.active.is_none()
            && overlay.failed_generation != Some(generation)
        {
            overlay.cancel_preview();
            match overlay.start(ctx, rect, true) {
                Ok(id) => { overlay.active = Some((generation, id)); self.recording_dialog.outline_error = None; }
                Err(error) => { overlay.failed_generation = Some(generation); self.recording_dialog.outline_error = Some(error.clone()); failure_to_report = Some(error); error_changed = true; }
            }
        }
        if error_changed && self.recording_dialog.open {
            let host = self.recording_dialog.host.identity();
            if host.host == super::utility_host::UtilityPresentation::Popout {
                ctx.request_repaint_of(self.recording_dialog.native_identity(host).viewport_id);
            }
        }
        if let Some(error) = failure_to_report {
            self.report_error_message("recording.outline", error);
        }
    }

    pub(crate) fn open_recording_setup(&mut self) {
        self.recording_dialog.open();
        self.focus_panel(super::Panel::RecordingDialog);
        if !self.recording_dialog.is_popped_out() {
            self.request_launcher_state(Some(true), Some(true));
        } else {
            self.recording_dialog.request_host_focus();
        }
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
        if self.recording_dialog.open && self.recording_dialog.initial_target_request {
            self.recording_dialog.initial_target_request = false;
            let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
            self.recording_dialog.refresh_targets(ctx, &catalog);
        }
        if self.recording_dialog.open {
            let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
            if self.recording_dialog.refresh_window_snapshot(&catalog) {
                let owner = self.recording_dialog.host.identity();
                if owner.host == super::utility_host::UtilityPresentation::Popout {
                    ctx.request_repaint_of(self.recording_dialog.native_identity(owner).viewport_id);
                }
            }
        }
        if !self.recording_dialog.target_refresh_pending_for_shutdown() {
            return;
        }
        let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
        self.recording_dialog.poll_target_refresh(ctx, &catalog);
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
                RetainedRecordingAction::RefreshWindows => {
                    let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
                    catalog.request_refresh();
                    self.recording_dialog.refresh_window_snapshot(&catalog);
                }
                RetainedRecordingAction::ShowRegion { selection_revision } => {
                    if selection_revision != self.recording_dialog.selection_revision()
                        || self.recording_dialog.selection.target_kind != RecordingTargetKind::Display
                        || self.recording_dialog.selection.region.is_none()
                        || self.recording_controller.has_active_job()
                        || self.recording_dialog.has_selector_operation()
                        || self.mkmacro_dialog.visual_overlay_controller().has_pending_selector()
                    { continue; }
                    let catalog = Arc::clone(&self.plugins.internal_services().window_catalog);
                    match self.recording_dialog.build_request(&catalog) {
                        Ok(request) => {
                            self.recording_dialog.recorder_overlay.cancel_preview();
                            match self.recording_dialog.recorder_overlay.start(ctx, request.region(), false) {
                                Ok(id) => { self.recording_dialog.recorder_overlay.preview = Some((id, selection_revision)); self.recording_dialog.outline_error = None; }
                                Err(error) => self.recording_dialog.outline_error = Some(error),
                            }
                        }
                        Err(error) => self.recording_dialog.outline_error = Some(error),
                    }
                    let host = self.recording_dialog.host.identity();
                    if host.host == super::utility_host::UtilityPresentation::Popout {
                        ctx.request_repaint_of(self.recording_dialog.native_identity(host).viewport_id);
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
                RetainedRecordingAction::ShowRoot => {
                    self.request_launcher_visibility(true);
                }
                RetainedRecordingAction::CopyText(text) => {
                    #[cfg(test)]
                    self.recording_dialog.test_copies.push(text);
                    #[cfg(not(test))]
                    if let Err(error) = crate::actions::clipboard::set_text(&text) {
                        self.report_error_message("recording.copy", error.to_string());
                    }
                }
                RetainedRecordingAction::RegionAcknowledged { .. } => {}
            }
        }
        self.reconcile_recording_outline(ctx);
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
        #[cfg(test)]
        {
            self.recording_dialog.test_last_request = Some(request.clone());
        }
        let Some(owner) = self.recording_dialog.accepted_chooser.take() else {
            return;
        };
        let host = self.recording_dialog.host.identity();
        let epoch = self.recording_dialog.setup_epoch;
        if owner.host != host || owner.epoch != epoch {
            return;
        }
        let identity = owner
            .native
            .then(|| self.recording_dialog.native_identity(owner.host));
        #[cfg(windows)]
        let hwnd = super::utility_dialog_size::utility_picker_hwnd(&self.egui_ctx, identity);
        #[cfg(not(windows))]
        let hwnd = None;
        let path = self
            .recording_dialog
            .choose_save_path(request.options().format(), hwnd);
        if !self.recording_dialog.open
            || self.recording_dialog.setup_epoch != epoch
            || self.recording_dialog.host.identity() != host
            || self.recording_dialog.selection_revision() != selection_revision
        {
            return;
        }
        #[cfg(all(windows, not(test)))]
        if hwnd.is_none()
            || hwnd != super::utility_dialog_size::utility_picker_hwnd(&self.egui_ctx, identity)
        {
            return;
        }
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
        #[cfg(test)]
        let validate_target = !self.recording_dialog.test_accept_target;
        #[cfg(not(test))]
        let validate_target = true;
        if validate_target
            && let Err(error) = crate::recording::validate_recording_target(request.target())
        {
            self.report_error_message("recording.start", error);
            return;
        }
        self.recording_dialog.recorder_overlay.cancel_preview();
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
        self.recording_dialog.remember_final_presentation(&self.egui_ctx);
        self.recording_dialog.retire_host(&self.egui_ctx);
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
    fn recording_outline_tracks_frozen_generation_and_retires_preview() {
        use crate::gui::mkmacro_dialog::visual_overlay::{NativeVisualOverlayService, ServiceTestObserver, ServiceTestRenderer, VisualOverlayCommand, VisualOverlayController};
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(&ctx, directory.path());
        let (mut draft, catalog) = selected_display_state();
        draft.open();
        draft.selection.format = RecordingFormat::Gif;
        draft.selection.frames_per_second = 10;
        let request = draft.build_request(&catalog).unwrap();
        let frozen_region = request.region();
        app.recording_dialog = draft;
        app.recording_controller = crate::recording::RecordingController::new(Arc::new(UiBackend), || {});
        let observer = Arc::new(ServiceTestObserver::default());
        let renderer = Arc::clone(&observer);
        app.recording_dialog.recorder_overlay.service = Some(NativeVisualOverlayService::start_with_observer(
            move || VisualOverlayController::new(Box::new(ServiceTestRenderer(renderer))), Arc::clone(&observer),
        ).unwrap());
        app.recording_dialog.set_region_operation(Some(999), app.recording_dialog.selection_revision);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let provenance = ViewportPassProvenance::current(ui.ctx(), egui::ViewportId::ROOT);
            let receipt = RecordingSetupReceipt { root: provenance, host: app.recording_dialog.host.identity(), epoch: app.recording_dialog.setup_epoch, native: false };
            app.recording_dialog.pending_actions.push(PendingAction { provenance, setup: Some(receipt), command: RecordingUiCommand::ShowRegion { revision: app.recording_dialog.selection_revision } });
        }).drop_without_applying_deltas();
        app.complete_recording_dialog(&ctx);
        assert!(observer.commands.lock().unwrap().is_empty());
        assert!(app.recording_dialog.recorder_overlay.preview.is_none());
        app.recording_dialog.set_region_operation(None, app.recording_dialog.selection_revision);
        let preview_id = app.recording_dialog.recorder_overlay.start(&ctx, frozen_region, false).unwrap();
        app.recording_dialog.recorder_overlay.preview = Some((preview_id, app.recording_dialog.selection_revision));
        observer.wait_for_commands(1);
        app.recording_dialog.selection_revision += 1;
        app.reconcile_recording_outline(&ctx);
        observer.wait_for_commands(2);
        assert!(app.recording_dialog.recorder_overlay.preview.is_none());
        let generation = app.recording_controller.prepare(request.clone(), directory.path().join("outline.gif"), &app.activity_center).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !matches!(app.recording_controller.snapshot().session.state(), crate::recording::RecorderState::Countdown { .. } | crate::recording::RecorderState::Recording { .. }) {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        app.reconcile_recording_outline(&ctx);
        observer.wait_for_commands(3);
        let active = app.recording_dialog.recorder_overlay.active.unwrap();
        assert_eq!(active.0, generation);
        assert!(matches!(observer.commands.lock().unwrap()[2], VisualOverlayCommand::RecordingRectangle { rect, persistent: true, .. } if rect == frozen_region));
        app.recording_dialog.close();
        for _ in 0..4 { app.reconcile_recording_outline(&ctx); }
        assert_eq!(app.recording_dialog.recorder_overlay.active, Some(active));
        assert_eq!(observer.commands.lock().unwrap().len(), 3);
        app.recording_controller.discard(generation).unwrap();
        app.reconcile_recording_outline(&ctx);
        observer.wait_for_commands(4);
        assert!(app.recording_dialog.recorder_overlay.active.is_none());
        assert!(matches!(observer.commands.lock().unwrap()[3], VisualOverlayCommand::Cancel { expected_operation_id: Some(id) } if id == active.1));
        while app.recording_controller.has_active_job() {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let window = CaptureTarget::window(11, 22, 33, ScreenRect::new(-40, -20, 32, 24)).unwrap();
        let window_request = RecordingRequest::new(window, ScreenRect::new(-40, -20, 32, 24), request.options().clone()).unwrap();
        let next = app.recording_controller.prepare(window_request, directory.path().join("window.gif"), &app.activity_center).unwrap();
        assert_ne!(generation, next);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !matches!(app.recording_controller.snapshot().session.state(), crate::recording::RecorderState::Countdown { .. } | crate::recording::RecorderState::Recording { .. }) {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        app.reconcile_recording_outline(&ctx);
        assert!(app.recording_dialog.recorder_overlay.active.is_none());
        assert_eq!(observer.commands.lock().unwrap().len(), 4);
        app.recording_controller.discard(next).unwrap();
        while app.recording_controller.has_active_job() {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let third = app.recording_controller.prepare(request, directory.path().join("again.gif"), &app.activity_center).unwrap();
        assert_ne!(third, generation);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !matches!(app.recording_controller.snapshot().session.state(), crate::recording::RecorderState::Countdown { .. } | crate::recording::RecorderState::Recording { .. }) {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        app.reconcile_recording_outline(&ctx);
        observer.wait_for_commands(5);
        let newer = app.recording_dialog.recorder_overlay.active.unwrap();
        assert_eq!(newer.0, third);
        assert_ne!(newer.1, active.1);
        app.recording_dialog.recorder_overlay.cancel(active.1);
        observer.wait_for_commands(6);
        app.reconcile_recording_outline(&ctx);
        assert_eq!(app.recording_dialog.recorder_overlay.active, Some(newer));
        app.recording_controller.discard(third).unwrap();
        app.reconcile_recording_outline(&ctx);
        observer.wait_for_commands(7);
        assert!(app.recording_dialog.recorder_overlay.active.is_none());
        while app.recording_controller.has_active_job() {
            app.recording_controller.poll();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn recorder_window_snapshot_preserves_identity_and_rejects_reused_handle() {
        let window = |hwnd, pid, title: &str| WindowDescriptor {
            hwnd, pid, title: title.into(), executable: None, process_path: None, class_name: None,
        };
        let original = window(42, 7, "Original");
        let mut state = RecordingDialogState::default();
        state.open();
        assert!(state.initial_target_request);
        let first = WindowCatalog::from_snapshot(vec![original.clone(), window(99, 8, "Other")]);
        state.refresh_window_snapshot(&first);
        assert_eq!(state.selection.window, Some((42, 7)));
        let updated = WindowCatalog::from_snapshot_with_descriptor(
            vec![window(99, 8, "Other"), window(42, 7, "Renamed")],
            move |_| Some(window(42, 88, "Reused")),
        );
        state.windows_generation = None;
        state.refresh_window_snapshot(&updated);
        assert_eq!(state.selection.window, Some((42, 7)));
        state.selection.target_kind = RecordingTargetKind::Window;
        assert!(state.build_request(&updated).unwrap_err().contains("no longer available"));
        let missing = WindowCatalog::from_snapshot(Vec::new());
        state.windows_generation = None;
        state.refresh_window_snapshot(&missing);
        assert_eq!(state.selection.window, Some((42, 7)));
        assert!(state.build_request(&missing).is_err());
    }

    #[test]
    fn recorder_open_and_retained_refresh_use_shared_cold_catalog() {
        struct Provider {
            started: mpsc::Sender<()>,
            release: mpsc::Receiver<Vec<WindowDescriptor>>,
        }
        impl crate::window_catalog::WindowProvider for Provider {
            fn enumerate(&mut self) -> Vec<WindowDescriptor> {
                self.started.send(()).unwrap();
                self.release.recv().unwrap()
            }
        }
        let _guard = crate::gui::TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let directory = tempfile::tempdir().unwrap();
        let mut app = test_app(&ctx, directory.path());
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (wake_tx, wake_rx) = mpsc::channel();
        let updates = Arc::new(crate::plugin::PluginSearchUpdates::default());
        let wake_ctx = ctx.clone();
        updates.set_repaint_callback(Arc::new(move || {
            wake_ctx.request_repaint_of(egui::ViewportId::ROOT);
            let _ = wake_tx.send(());
        }));
        let catalog = Arc::new(WindowCatalog::start(Provider { started: started_tx, release: release_rx }, updates));
        app.plugins.set_window_catalog_for_test(Arc::clone(&catalog));
        app.open_recording_setup();
        let (display_tx, display_rx) = mpsc::sync_channel(1);
        app.recording_dialog.target_refresh = Some(TargetRefreshWorker {
            result_rx: display_rx, result: None, setup_epoch: app.recording_dialog.setup_epoch,
            joined: Arc::new(AtomicBool::new(false)), settled: Arc::new(AtomicBool::new(false)),
            retirement_unavailable: Arc::new(AtomicBool::new(false)),
        });
        app.poll_recording_targets(&ctx);
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(catalog.discovery_snapshot().status, crate::window_catalog::WindowDiscoveryStatus::Loading);
        assert!(app.recording_dialog.host.transition(super::super::utility_host::UtilityPresentation::Popout));
        let native = app.recording_dialog.native_identity(app.recording_dialog.host.identity()).viewport_id;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let context = ui.ctx().clone();
            app.recording_dialog.render_setup_body(&context, &catalog, &app.recording_controller, true, true, Some(ui), None);
        }).drop_without_applying_deltas();
        assert!(app.recording_dialog.test_control("refresh").unwrap().2);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            let provenance = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
            let receipt = RecordingSetupReceipt { root: provenance, host: app.recording_dialog.host.identity(), epoch: app.recording_dialog.setup_epoch, native: false };
            let mut selection = app.recording_dialog.selection.clone();
            selection.target_kind = RecordingTargetKind::Window;
            app.recording_dialog.pending_selection = Some(PendingSelection { provenance, setup: Some(receipt), selection });
        }).drop_without_applying_deltas();
        app.complete_recording_dialog(&ctx);
        release_tx.send(vec![WindowDescriptor { title: "Editor".into(), hwnd: 42, pid: 7, executable: None, process_path: None, class_name: None }]).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        app.poll_recording_targets(&ctx);
        assert_eq!(app.recording_dialog.selection.window, Some((42, 7)));
        assert!(ctx.has_requested_repaint_for(&native));
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            let provenance = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
            let receipt = RecordingSetupReceipt { root: provenance, host: app.recording_dialog.host.identity(), epoch: app.recording_dialog.setup_epoch, native: false };
            app.recording_dialog.pending_actions.push(PendingAction { provenance, setup: Some(receipt), command: RecordingUiCommand::RefreshTargets { revision: app.recording_dialog.selection_revision } });
        }).drop_without_applying_deltas();
        app.complete_recording_dialog(&ctx);
        release_tx.send(Vec::new()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release_tx.send(Vec::new()).unwrap();
        wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        app.poll_recording_targets(&ctx);
        assert_eq!(app.recording_dialog.selection.window, Some((42, 7)));
        assert!(app.recording_dialog.windows.is_empty());
        app.recording_dialog.target_refresh = None;
        drop(display_tx);
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
