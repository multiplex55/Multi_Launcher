//! Native child viewports and actions for the session-only screenshot pinboard.

use crate::mkmacro::ScreenRect;
use crate::pinboard::live::{
    PinLiveChildWake, PinLiveFrameLease, PinLiveFrameSource, PinLiveStatus,
};
use crate::pinboard::{
    ImageBudgetReservation, PinId, PinImageHandle, PinVisibilityAck,
    PinWindowVisibilityCoordinator, Pinboard,
};
use eframe::egui;
use image::ImageEncoder;
use std::borrow::Cow;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_PIN_ACTIONS: usize = 8;
const DEFAULT_PINNER_WIDTH: f32 = 720.0;
const DEFAULT_PINNER_HEIGHT: f32 = 520.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinScaleMode {
    Fit,
    Actual,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PinActionStatus {
    Idle,
    Pending(String),
    Finished(String),
    Failed(String),
}

#[derive(Clone)]
enum PinWindowImageSource {
    Static(PinImageHandle),
    Live(Arc<LiveDisplayPreparation>),
}

impl PinWindowImageSource {
    fn current_image_snapshot(&self) -> Option<PinImageHandle> {
        match self {
            Self::Static(image) => Some(image.clone()),
            Self::Live(preparation) => preparation.current_image_snapshot(),
        }
    }

    fn live_preparation(&self) -> Option<&Arc<LiveDisplayPreparation>> {
        match self {
            Self::Static(_) => None,
            Self::Live(preparation) => Some(preparation),
        }
    }
}

struct PreparedLiveImage {
    sequence: u64,
    color_image: Arc<egui::ColorImage>,
    _reservation: ImageBudgetReservation,
}

#[derive(Default)]
struct LiveDisplayPreparedState {
    frame: Option<Arc<PreparedLiveImage>>,
    retired_uploads: Vec<Arc<PreparedLiveImage>>,
    error: Option<String>,
    cancelled: bool,
}

#[derive(Default)]
struct LiveDisplayWakeState {
    pending: bool,
    cancelled: bool,
}

struct LiveDisplayPreparation {
    source: PinLiveFrameSource,
    pin_id: PinId,
    lifetime: u64,
    expected_dimensions: (u32, u32),
    prepared: Arc<Mutex<LiveDisplayPreparedState>>,
    prepared_change: Arc<(Mutex<u64>, Condvar)>,
    wake: Arc<(Mutex<LiveDisplayWakeState>, Condvar)>,
    child_wake: Arc<Mutex<Option<PinLiveChildWake>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl LiveDisplayPreparation {
    fn new(
        source: PinLiveFrameSource,
        pin_id: PinId,
        lifetime: u64,
        expected_dimensions: (u32, u32),
    ) -> Arc<Self> {
        let preparation = Arc::new(Self {
            source: source.clone(),
            pin_id,
            lifetime,
            expected_dimensions,
            prepared: Arc::new(Mutex::new(LiveDisplayPreparedState::default())),
            prepared_change: Arc::new((Mutex::new(0), Condvar::new())),
            wake: Arc::new((Mutex::new(LiveDisplayWakeState::default()), Condvar::new())),
            child_wake: Arc::new(Mutex::new(None)),
            worker: Mutex::new(None),
        });
        let worker = spawn_live_display_preparation(
            source,
            expected_dimensions,
            Arc::clone(&preparation.prepared),
            Arc::clone(&preparation.prepared_change),
            Arc::clone(&preparation.wake),
            Arc::clone(&preparation.child_wake),
            pin_id,
            lifetime,
        );
        match worker {
            Ok(worker) => {
                if let Ok(mut slot) = preparation.worker.lock() {
                    *slot = Some(worker);
                }
                preparation.wake();
            }
            Err(error) => {
                if let Ok(mut state) = preparation.prepared.lock() {
                    state.error = Some(format!("could not prepare live pin display: {error}"));
                }
            }
        }
        preparation
    }

    fn wake(&self) {
        let (state, changed) = &*self.wake;
        if let Ok(mut state) = state.lock()
            && !state.cancelled
        {
            state.pending = true;
            changed.notify_one();
        }
    }

    fn set_child_wake(&self, notifier: PinLiveChildWake) {
        if let Ok(mut child_wake) = self.child_wake.lock() {
            *child_wake = Some(notifier);
        }
    }

    fn notify_child(&self) {
        let notifier = self
            .child_wake
            .lock()
            .ok()
            .and_then(|notifier| notifier.clone());
        if let Some(notifier) = notifier {
            notifier();
        }
    }

    fn frame(&self) -> Option<Arc<PreparedLiveImage>> {
        let state = self.prepared.lock().ok()?;
        (!state.cancelled).then(|| state.frame.clone()).flatten()
    }

    fn error(&self) -> Option<String> {
        self.prepared.lock().ok()?.error.clone()
    }

    fn current_image_snapshot(&self) -> Option<PinImageHandle> {
        self.source.current_frame().map(|frame| frame.image)
    }

    fn status(&self) -> PinLiveStatus {
        self.source.status()
    }

    fn is_paused(&self) -> bool {
        matches!(self.status(), PinLiveStatus::Paused)
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.status(),
            PinLiveStatus::SourceClosed | PinLiveStatus::Error(_)
        )
    }

    fn pause(&self) -> bool {
        self.source.pause()
    }

    fn resume(&self) -> bool {
        self.source.resume()
    }

    fn cancel(&self) {
        {
            let (state, changed) = &*self.wake;
            if let Ok(mut state) = state.lock() {
                state.cancelled = true;
                state.pending = false;
                changed.notify_all();
            }
        }
        self.source.set_child_wake_notifier(None);
        if let Ok(mut state) = self.prepared.lock() {
            state.cancelled = true;
            state.error = None;
        }
        if let Ok(mut child_wake) = self.child_wake.lock() {
            *child_wake = None;
        }
    }

    fn cancel_and_join(&self) -> Result<(), String> {
        self.cancel();
        let worker = self
            .worker
            .lock()
            .map_err(|_| "live display worker lock was poisoned".to_owned())?
            .take();
        if let Some(worker) = worker {
            worker.join().map_err(|_| {
                "live display preparation worker terminated unexpectedly".to_owned()
            })?;
        }
        Ok(())
    }

    fn reap_retired_uploads(&self) -> usize {
        let Ok(mut state) = self.prepared.lock() else {
            return 0;
        };
        if state.cancelled
            && state
                .frame
                .as_ref()
                .is_some_and(|frame| Arc::strong_count(&frame.color_image) == 1)
        {
            state.frame = None;
        }
        state
            .retired_uploads
            .retain(|frame| Arc::strong_count(&frame.color_image) > 1);
        state.retired_uploads.len()
    }

    fn has_retained_uploads(&self) -> bool {
        let Ok(state) = self.prepared.lock() else {
            return true;
        };
        state
            .frame
            .as_ref()
            .is_some_and(|frame| !state.cancelled || Arc::strong_count(&frame.color_image) > 1)
            || !state.retired_uploads.is_empty()
    }

    fn worker_finished(&self) -> bool {
        self.worker
            .lock()
            .is_ok_and(|worker| worker.as_ref().is_none_or(JoinHandle::is_finished))
    }

    fn is_cancelled(&self) -> bool {
        self.prepared.lock().is_ok_and(|state| state.cancelled)
    }

    #[cfg(test)]
    fn retained_upload_count(&self) -> usize {
        self.prepared
            .lock()
            .map(|state| state.retired_uploads.len())
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn wait_for_sequence(&self, sequence: u64) -> bool {
        let (current, changed) = &*self.prepared_change;
        current
            .lock()
            .ok()
            .and_then(|current| {
                changed
                    .wait_timeout_while(current, std::time::Duration::from_secs(2), |current| {
                        *current < sequence
                    })
                    .ok()
            })
            .is_some_and(|(current, _)| *current >= sequence)
    }
}

impl Drop for LiveDisplayPreparation {
    fn drop(&mut self) {
        self.cancel();
        if let Ok(worker) = self.worker.get_mut()
            && let Some(worker) = worker.take()
            && worker.join().is_err()
        {
            tracing::error!("live pin display preparation worker terminated unexpectedly");
        }
    }
}

fn spawn_live_display_preparation(
    source: PinLiveFrameSource,
    expected_dimensions: (u32, u32),
    prepared: Arc<Mutex<LiveDisplayPreparedState>>,
    prepared_change: Arc<(Mutex<u64>, Condvar)>,
    wake: Arc<(Mutex<LiveDisplayWakeState>, Condvar)>,
    child_wake: Arc<Mutex<Option<PinLiveChildWake>>>,
    pin_id: PinId,
    lifetime: u64,
) -> Result<JoinHandle<()>, String> {
    thread::Builder::new()
        .name(format!("pin-display-{}-{lifetime}", pin_id.get()))
        .spawn(move || {
            'worker: loop {
                let (state, changed) = &*wake;
                let Ok(mut state) = state.lock() else {
                    break;
                };
                while !state.cancelled && !state.pending {
                    let has_retired = prepared
                        .lock()
                        .is_ok_and(|state| !state.retired_uploads.is_empty());
                    let result = if has_retired {
                        changed
                            .wait_timeout(state, std::time::Duration::from_millis(100))
                            .ok()
                            .map(|(next, _)| next)
                    } else {
                        changed.wait(state).ok()
                    };
                    let Some(next) = result else { return };
                    state = next;
                    if !state.cancelled {
                        state.pending = true;
                    }
                }
                if state.cancelled {
                    drop(state);
                    break 'worker;
                }
                state.pending = false;
                drop(state);

                let retained_uploads = if let Ok(mut state) = prepared.lock() {
                    state
                        .retired_uploads
                        .retain(|frame| Arc::strong_count(&frame.color_image) > 1);
                    state.retired_uploads.len()
                } else {
                    break;
                };

                let Some(frame) = source.current_frame() else {
                    continue;
                };
                if prepared
                    .lock()
                    .ok()
                    .and_then(|state| state.frame.as_ref().map(|current| current.sequence))
                    .is_some_and(|sequence| sequence >= frame.sequence)
                {
                    continue;
                }
                let dimensions = frame.image.pixels().dimensions();
                if dimensions != expected_dimensions {
                    set_live_display_error(
                        &prepared,
                        format!(
                            "live source changed size from {}×{} to {}×{}; display is suspended",
                            expected_dimensions.0,
                            expected_dimensions.1,
                            dimensions.0,
                            dimensions.1
                        ),
                    );
                    notify_live_display_child(&child_wake);
                    continue;
                }
                let cannot_replace = prepared.lock().is_ok_and(|state| {
                    state
                        .frame
                        .as_ref()
                        .is_some_and(|current| Arc::strong_count(&current.color_image) > 1)
                        && retained_uploads >= MAX_RETIRED_LIVE_UPLOADS
                });
                if cannot_replace {
                    set_live_display_error(
                        &prepared,
                        "live display is waiting for the renderer to release an older frame".into(),
                    );
                    notify_live_display_child(&child_wake);
                    continue;
                }
                let reservation = match frame
                    .image
                    .reserve_related_image_bytes(frame.image.image_bytes())
                {
                    Ok(reservation) => reservation,
                    Err(error) => {
                        set_live_display_error(
                            &prepared,
                            format!("live display is waiting for image memory: {error}"),
                        );
                        notify_live_display_child(&child_wake);
                        continue;
                    }
                };
                let pixels = frame.image.pixels();
                let color_image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
                    [dimensions.0 as usize, dimensions.1 as usize],
                    pixels.as_raw(),
                ));
                let prepared_frame = Arc::new(PreparedLiveImage {
                    sequence: frame.sequence,
                    color_image,
                    _reservation: reservation,
                });
                let published_sequence = prepared_frame.sequence;
                drop(frame);

                if source.is_cancelled() {
                    continue;
                }
                if let Ok(mut state) = prepared.lock() {
                    if state.cancelled {
                        continue;
                    }
                    if state
                        .frame
                        .as_ref()
                        .is_none_or(|current| current.sequence < prepared_frame.sequence)
                    {
                        if let Some(current) = state.frame.take()
                            && Arc::strong_count(&current.color_image) > 1
                        {
                            if state.retired_uploads.len() >= MAX_RETIRED_LIVE_UPLOADS {
                                state.frame = Some(current);
                                state.error = Some(
                                "live display is waiting for the renderer to release an older frame"
                                    .into(),
                            );
                                continue 'worker;
                            }
                            state.retired_uploads.push(current);
                        }
                        state.frame = Some(prepared_frame);
                        state.error = None;
                    }
                }
                if let Ok(mut sequence) = prepared_change.0.lock() {
                    *sequence = (*sequence).max(published_sequence);
                    prepared_change.1.notify_all();
                }
                notify_live_display_child(&child_wake);
            }
        })
        .map_err(|error| error.to_string())
}

fn is_terminal_live_status(status: &PinLiveStatus) -> bool {
    matches!(
        status,
        PinLiveStatus::SourceClosed | PinLiveStatus::Error(_)
    )
}

fn live_status_label(status: &PinLiveStatus) -> String {
    match status {
        PinLiveStatus::Live => "Live".into(),
        PinLiveStatus::Paused => "Paused".into(),
        PinLiveStatus::TemporarilyUnavailable(reason) => {
            format!("Temporarily unavailable · {reason}")
        }
        PinLiveStatus::SourceClosed => "Source closed · frozen frame".into(),
        PinLiveStatus::Error(error) => format!("Error · {error}"),
    }
}

fn set_live_display_error(prepared: &Mutex<LiveDisplayPreparedState>, error: String) {
    if let Ok(mut state) = prepared.lock()
        && !state.cancelled
    {
        state.error = Some(error);
    }
}

fn notify_live_display_child(child_wake: &Mutex<Option<PinLiveChildWake>>) {
    let notifier = child_wake.lock().ok().and_then(|notifier| notifier.clone());
    if let Some(notifier) = notifier {
        notifier();
    }
}

struct PinWindowRuntime {
    pin_id: PinId,
    lifetime: u64,
    process_id: u32,
    viewport_id: egui::ViewportId,
    title: String,
    image_source: PinWindowImageSource,
    dimensions: (u32, u32),
    state: Mutex<PinWindowState>,
}

const MAX_RETIRED_LIVE_UPLOADS: usize = 3;

struct PinWindowState {
    texture: Option<egui::TextureHandle>,
    texture_sequence: Option<u64>,
    scale_mode: PinScaleMode,
    zoom: f32,
    display_bounds: ScreenRect,
    always_on_top: bool,
    click_through: bool,
    desired_visible: bool,
    root_registered: bool,
    callback_entered: bool,
    close_requested: bool,
    close_command_sent: bool,
    next_request_id: u64,
    pending_request_id: Option<u64>,
    action_status: PinActionStatus,
    native: Option<NativeWindowBinding>,
    native_error: Option<String>,
    native_destroyed: bool,
    capture_visibility: Option<CaptureVisibilityState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PinControlChange {
    always_on_top: Option<bool>,
    click_through: Option<bool>,
}

#[derive(Clone)]
struct PinListEntry {
    pin_id: PinId,
    lifetime: u64,
    dimensions: (u32, u32),
    always_on_top: bool,
    click_through: bool,
    texture: Option<egui::TextureHandle>,
    native_status: String,
    controls_available: bool,
    live_status: Option<String>,
    live_paused: bool,
    live_terminal: bool,
}

enum PinListAction {
    Control {
        pin_id: PinId,
        lifetime: u64,
        change: PinControlChange,
    },
    RecoverAll,
    Close {
        pin_id: PinId,
        lifetime: u64,
    },
    LiveControl {
        pin_id: PinId,
        lifetime: u64,
        pause: bool,
    },
    CloseAll,
}

#[derive(Clone)]
struct NativeWindowBinding {
    hwnd: usize,
    process_id: u32,
    title: String,
    lifetime: u64,
    root_hwnd: usize,
}

trait PinNativeApi: Send + Sync {
    fn lookup(
        &self,
        title: &str,
        lifetime: u64,
        existing: Option<&NativeWindowBinding>,
        root_hwnd: usize,
    ) -> NativeLookup;

    fn is_visible(&self, binding: &NativeWindowBinding) -> Result<bool, String>;

    fn bounds(&self, binding: &NativeWindowBinding) -> Option<ScreenRect>;
}

struct SystemPinNativeApi;

impl PinNativeApi for SystemPinNativeApi {
    fn lookup(
        &self,
        title: &str,
        lifetime: u64,
        existing: Option<&NativeWindowBinding>,
        root_hwnd: usize,
    ) -> NativeLookup {
        native_window_for_pin(title, lifetime, existing, root_hwnd)
    }

    fn is_visible(&self, binding: &NativeWindowBinding) -> Result<bool, String> {
        native_window_visible(binding)
    }

    fn bounds(&self, binding: &NativeWindowBinding) -> Option<ScreenRect> {
        native_window_bounds(binding)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CaptureVisibilityState {
    identity: Option<crate::pinboard::PinCaptureIdentity>,
    prior_visible: Option<bool>,
    absence_proven: bool,
    hide_requested: bool,
    hide_acknowledged: bool,
    restore_intent: bool,
    restore_requested: bool,
    restore_acknowledged: bool,
}

struct PinWindowRegistry {
    pins: BTreeMap<PinId, Arc<PinWindowRuntime>>,
    retired: BTreeMap<PinId, Arc<PinWindowRuntime>>,
    live_preparations: BTreeMap<(PinId, u64), Arc<LiveDisplayPreparation>>,
    native_api: Arc<dyn PinNativeApi>,
    root_context: Option<egui::Context>,
    root_native_identity: (usize, u64),
    process_id: u32,
    session_id: u128,
    hide_identity: Option<crate::pinboard::PinCaptureIdentity>,
    restore_identity: Option<crate::pinboard::PinCaptureIdentity>,
    shutting_down: bool,
}

/// Owns child viewport registrations, textures, and the one bounded action worker.
pub(crate) struct PinWindowPresenter {
    registry: Arc<Mutex<PinWindowRegistry>>,
    actions: PinActionWorker,
    capture_feedback: Option<String>,
    capture_feedback_closed: Arc<AtomicBool>,
    capture_feedback_viewport_id: egui::ViewportId,
    pending_parent_cleanup: bool,
    pin_list_focus_requested: bool,
    #[cfg(test)]
    last_recover_all_widget_id: Option<egui::Id>,
}

impl PinWindowPresenter {
    pub(crate) fn new() -> Self {
        Self::with_native_api(Arc::new(SystemPinNativeApi))
    }

    fn with_native_api(native_api: Arc<dyn PinNativeApi>) -> Self {
        Self::with_native_api_and_save_picker(native_api, Arc::new(NativePinSavePicker))
    }

    fn with_native_api_and_save_picker(
        native_api: Arc<dyn PinNativeApi>,
        save_picker: Arc<dyn PinSavePicker>,
    ) -> Self {
        Self::with_native_api_and_targets(
            native_api,
            save_picker,
            Arc::new(SystemPinImageClipboard),
        )
    }

    fn with_native_api_and_targets(
        native_api: Arc<dyn PinNativeApi>,
        save_picker: Arc<dyn PinSavePicker>,
        clipboard: Arc<dyn PinImageClipboard>,
    ) -> Self {
        let process_id = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let session_id = now ^ u128::from(process_id);
        let registry = Arc::new(Mutex::new(PinWindowRegistry {
            pins: BTreeMap::new(),
            retired: BTreeMap::new(),
            live_preparations: BTreeMap::new(),
            native_api,
            root_context: None,
            root_native_identity: (0, 0),
            process_id,
            session_id,
            hide_identity: None,
            restore_identity: None,
            shutting_down: false,
        }));
        Self {
            registry,
            actions: PinActionWorker::with_action_targets(save_picker, clipboard),
            capture_feedback: None,
            capture_feedback_closed: Arc::new(AtomicBool::new(false)),
            capture_feedback_viewport_id: egui::ViewportId::from_hash_of((
                "pinboard-capture-feedback",
                process_id,
                session_id,
            )),
            pending_parent_cleanup: false,
            pin_list_focus_requested: false,
            #[cfg(test)]
            last_recover_all_widget_id: None,
        }
    }

    pub(crate) fn visibility_coordinator(&self) -> Box<dyn PinWindowVisibilityCoordinator> {
        Box::new(PinWindowVisibilityOwner {
            registry: Arc::clone(&self.registry),
        })
    }

    pub(crate) fn set_root_native_identity(&self, identity: (usize, u64)) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.root_native_identity = identity;
        }
    }

    pub(crate) fn queue_capture_feedback(&mut self, message: String) {
        if self.capture_feedback.as_ref() != Some(&message) {
            self.capture_feedback = Some(message);
            self.capture_feedback_closed.store(false, Ordering::Release);
        }
    }

    pub(crate) fn show_capture_feedback(&mut self, ctx: &egui::Context) {
        if ctx.embed_viewports() {
            return;
        }
        if self.capture_feedback_closed.load(Ordering::Acquire) {
            self.capture_feedback = None;
            return;
        }
        let Some(message) = self.capture_feedback.clone() else {
            return;
        };
        let viewport_id = self.capture_feedback_viewport_id;
        let closed = Arc::clone(&self.capture_feedback_closed);
        let builder = egui::ViewportBuilder::default()
            .with_title("Pinboard capture")
            .with_inner_size([390.0, 130.0])
            .with_resizable(false)
            .with_active(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        ctx.show_viewport_deferred(viewport_id, builder, move |ui, class| {
            if class != egui::ViewportClass::Deferred || ui.ctx().embed_viewports() {
                return;
            }
            let child_ctx = ui.ctx().clone();
            let native_close = child_ctx.input(|input| input.viewport().close_requested());
            let mut dismiss = false;
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Pinboard capture could not be completed.");
                ui.label(&message);
                dismiss = ui.button("Dismiss").clicked();
            });
            if native_close || dismiss {
                closed.store(true, Ordering::Release);
                child_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    /// Updates the owner from the domain model and registers its retained child callbacks.
    pub(crate) fn show(&mut self, ctx: &egui::Context, board: &Pinboard) {
        if ctx.embed_viewports() {
            return;
        }
        let mut callbacks = Vec::new();
        let mut close_viewports = Vec::new();
        let mut removed_retired = false;
        let (root_native_identity, shutting_down, native_api) = {
            let Ok(mut registry) = self.registry.lock() else {
                return;
            };
            registry.root_context = Some(ctx.clone());
            let process_id = registry.process_id;
            let session_id = registry.session_id;
            for runtime in registry.retired.values() {
                if let Ok(mut state) = runtime.state.lock() {
                    state.root_registered = false;
                }
            }
            let active_ids = board.pins().map(|pin| pin.id()).collect::<Vec<_>>();
            let active_set = active_ids
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            let stale = registry
                .pins
                .keys()
                .copied()
                .filter(|pin_id| !active_set.contains(pin_id))
                .collect::<Vec<_>>();
            for pin_id in stale {
                if let Some(runtime) = registry.pins.remove(&pin_id)
                    && let Some(viewport_id) = retire_pin_runtime(&mut registry, runtime)
                {
                    close_viewports.push(viewport_id);
                }
            }
            let destroyed_retired = registry
                .retired
                .iter()
                .filter_map(|(pin_id, runtime)| {
                    runtime
                        .state
                        .lock()
                        .is_ok_and(|state| state.native_destroyed)
                        .then_some(*pin_id)
                })
                .collect::<Vec<_>>();
            for pin_id in destroyed_retired {
                removed_retired |= registry.retired.remove(&pin_id).is_some();
            }
            let hide_identity = registry.hide_identity;
            let restore_identity = registry.restore_identity;
            let shutting_down = registry.shutting_down;
            for pin in board.pins() {
                let is_new = !registry.pins.contains_key(&pin.id());
                if is_new {
                    let lifetime = pin.id().get();
                    let viewport_id = pin_viewport_id(pin.id(), lifetime);
                    let title = pin_native_title(process_id, session_id, pin.id(), lifetime);
                    let (image_source, dimensions) = if let Some(source) = pin.live_frame_source() {
                        let Some(frame) = source.current_frame() else {
                            continue;
                        };
                        let dimensions = frame.image.pixels().dimensions();
                        drop(frame);
                        let preparation =
                            LiveDisplayPreparation::new(source, pin.id(), lifetime, dimensions);
                        registry
                            .live_preparations
                            .insert((pin.id(), lifetime), Arc::clone(&preparation));
                        (PinWindowImageSource::Live(preparation), dimensions)
                    } else {
                        let Some(image) = pin.current_image_handle() else {
                            continue;
                        };
                        let dimensions = image.pixels().dimensions();
                        (PinWindowImageSource::Static(image), dimensions)
                    };
                    let runtime = Arc::new(PinWindowRuntime {
                        pin_id: pin.id(),
                        lifetime,
                        process_id,
                        viewport_id,
                        title,
                        image_source,
                        dimensions,
                        state: Mutex::new(PinWindowState {
                            texture: None,
                            texture_sequence: None,
                            scale_mode: PinScaleMode::Fit,
                            zoom: pin.zoom().clamp(0.1, 8.0),
                            display_bounds: pin.display_bounds(),
                            always_on_top: pin.always_on_top(),
                            click_through: pin.click_through(),
                            desired_visible: true,
                            root_registered: false,
                            callback_entered: false,
                            close_requested: false,
                            close_command_sent: false,
                            next_request_id: 1,
                            pending_request_id: None,
                            action_status: PinActionStatus::Idle,
                            native: None,
                            native_error: None,
                            native_destroyed: false,
                            capture_visibility: None,
                        }),
                    });
                    if let Some(preparation) = runtime.image_source.live_preparation() {
                        let child_ctx = ctx.clone();
                        let weak_runtime = Arc::downgrade(&runtime);
                        let weak_preparation = Arc::downgrade(preparation);
                        let notifier: PinLiveChildWake = Arc::new(move || {
                            let Some(runtime) = weak_runtime.upgrade() else {
                                return;
                            };
                            let is_open = runtime.state.lock().is_ok_and(|state| {
                                !state.close_requested && !state.native_destroyed
                            });
                            if !is_open {
                                return;
                            }
                            if let Some(preparation) = weak_preparation.upgrade() {
                                preparation.wake();
                            }
                            child_ctx.request_repaint_of(runtime.viewport_id);
                        });
                        preparation.set_child_wake(Arc::clone(&notifier));
                        pin.set_live_child_wake_notifier(Some(notifier));
                    }
                    registry.pins.insert(pin.id(), runtime);
                }
                let Some(runtime) = registry.pins.get(&pin.id()).cloned() else {
                    continue;
                };
                if let Ok(mut state) = runtime.state.lock() {
                    state.display_bounds = pin.display_bounds();
                    if runtime.viewport_id == egui::ViewportId::ROOT
                        || runtime.viewport_id != pin_viewport_id(runtime.pin_id, runtime.lifetime)
                    {
                        state.root_registered = false;
                        state.native_error = Some("pin child viewport identity is invalid".into());
                        continue;
                    }
                    if state.close_requested || state.native_destroyed {
                        if !state.close_command_sent
                            && !state.native_destroyed
                            && runtime.viewport_id != egui::ViewportId::ROOT
                            && runtime.viewport_id
                                == pin_viewport_id(runtime.pin_id, runtime.lifetime)
                        {
                            state.close_command_sent = true;
                            close_viewports.push(runtime.viewport_id);
                        }
                        continue;
                    }
                    state.root_registered = true;
                }
                let suppress =
                    should_suppress_pin(&runtime, hide_identity, restore_identity, shutting_down);
                callbacks.push((runtime, suppress));
            }
            (
                registry.root_native_identity,
                registry.shutting_down,
                Arc::clone(&registry.native_api),
            )
        };
        for viewport_id in close_viewports {
            if !ctx.embed_viewports() {
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Visible(false));
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
            }
        }

        for (runtime, capture_suppressed) in callbacks {
            let mut builder = pin_viewport_builder(&runtime);
            if capture_suppressed || shutting_down {
                builder = builder.with_visible(false);
            }
            let action_client = self.actions.client();
            let callback_native_api = Arc::clone(&native_api);
            let callback_registry = Arc::downgrade(&self.registry);
            ctx.show_viewport_deferred(runtime.viewport_id, builder, move |ui, class| {
                render_pin_viewport(
                    ui,
                    class,
                    &runtime,
                    &action_client,
                    &callback_native_api,
                    root_native_identity,
                    &callback_registry,
                    capture_suppressed || shutting_down,
                );
            });
        }
        if self.pending_parent_cleanup || removed_retired {
            // This parent pass omitted any destroyed retired children. The
            // framework drops their deferred callback references when the
            // enclosing root UI pass completes.
            self.pending_parent_cleanup = false;
        }
    }

    pub(crate) fn show_pin_list(
        &mut self,
        ctx: &egui::Context,
        board: &mut Pinboard,
        open: &mut bool,
    ) {
        if !*open || ctx.embed_viewports() || ctx.viewport_id() != egui::ViewportId::ROOT {
            return;
        }
        let focus_requested = std::mem::take(&mut self.pin_list_focus_requested);
        if focus_requested {
            ctx.move_to_top(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("screenshot-pin-list"),
            ));
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
        }
        let (entries, native_api, root_hwnd, shutting_down) = {
            let Ok(registry) = self.registry.lock() else {
                return;
            };
            let entries = board
                .pins()
                .filter_map(|pin| {
                    let runtime = registry.pins.get(&pin.id())?;
                    let state = runtime.state.lock().ok()?;
                    let live_status = runtime
                        .image_source
                        .live_preparation()
                        .map(|preparation| preparation.status());
                    Some(PinListEntry {
                        pin_id: pin.id(),
                        lifetime: runtime.lifetime,
                        dimensions: runtime.dimensions,
                        always_on_top: state.always_on_top,
                        click_through: state.click_through,
                        texture: state.texture.clone(),
                        native_status: if state.close_requested || state.native_destroyed {
                            "Closing".into()
                        } else if let Some(error) = state.native_error.clone() {
                            error
                        } else if state.native.is_some() {
                            "Owned child window observed; control values below are desired state"
                                .into()
                        } else {
                            "Waiting for the owned child window".into()
                        },
                        controls_available: state.root_registered
                            && !state.close_requested
                            && !state.native_destroyed
                            && state.native.is_some(),
                        live_paused: matches!(live_status.as_ref(), Some(PinLiveStatus::Paused)),
                        live_terminal: live_status.as_ref().is_some_and(is_terminal_live_status),
                        live_status: live_status.map(|status| {
                            runtime
                                .image_source
                                .live_preparation()
                                .and_then(|preparation| preparation.error())
                                .unwrap_or_else(|| live_status_label(&status))
                        }),
                    })
                })
                .collect::<Vec<_>>();
            (
                entries,
                Arc::clone(&registry.native_api),
                registry.root_native_identity.0,
                registry.shutting_down,
            )
        };

        let mut actions = Vec::new();
        let mut focus_widget_id = None;
        #[cfg(test)]
        let mut recover_all_widget_id = None;
        egui::Window::new("Screenshot Pins")
            .id(egui::Id::new("screenshot-pin-list"))
            .open(open)
            .resizable(true)
            .default_size([620.0, 420.0])
            .show(ctx, |ui| {
                ui.label(
                    "Tab or Shift+Tab moves between controls; Space or Enter activates a focused control.",
                );
                let has_recoverable = entries
                    .iter()
                    .any(|entry| entry.click_through && entry.controls_available);
                ui.horizontal(|ui| {
                    let recover_all = ui.add_enabled(
                        has_recoverable && !shutting_down,
                        egui::Button::new("Recover All"),
                    );
                    if focus_requested && has_recoverable && !shutting_down {
                        focus_widget_id = Some(recover_all.id);
                    }
                    #[cfg(test)]
                    {
                        recover_all_widget_id = Some(recover_all.id);
                    }
                    if recover_all.clicked() {
                        actions.push(PinListAction::RecoverAll);
                    }
                    let close_all = ui.add_enabled(
                        !entries.is_empty() && !shutting_down,
                        egui::Button::new("Close All"),
                    );
                    if focus_requested
                        && !has_recoverable
                        && !entries.is_empty()
                        && !shutting_down
                    {
                        focus_widget_id = Some(close_all.id);
                    }
                    if close_all.clicked() {
                        actions.push(PinListAction::CloseAll);
                    }
                });
                ui.separator();
                if entries.is_empty() {
                    ui.label("No screenshot pins are open.");
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for entry in &entries {
                        ui.push_id(("pin-list-row", entry.pin_id.get(), entry.lifetime), |ui| {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    if let Some(texture) = &entry.texture {
                                        ui.add(egui::Image::new((
                                            texture.id(),
                                            egui::vec2(112.0, 72.0),
                                        )));
                                    } else {
                                        ui.allocate_ui(egui::vec2(112.0, 72.0), |ui| {
                                            ui.centered_and_justified(|ui| {
                                                ui.label("Preparing preview…");
                                            });
                                        });
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(format!(
                                            "Pin {} · {} × {}",
                                            entry.pin_id.get(),
                                            entry.dimensions.0,
                                            entry.dimensions.1
                                        ));
                                        ui.label(entry.native_status.to_string());
                                        if let Some(status) = &entry.live_status {
                                            ui.label(status);
                                            ui.horizontal(|ui| {
                                                let label = if entry.live_paused {
                                                    "Resume"
                                                } else {
                                                    "Pause"
                                                };
                                                if ui
                                                    .add_enabled(
                                                        entry.controls_available
                                                            && !entry.live_terminal
                                                            && !shutting_down,
                                                        egui::Button::new(label),
                                                    )
                                                    .clicked()
                                                {
                                                    actions.push(PinListAction::LiveControl {
                                                        pin_id: entry.pin_id,
                                                        lifetime: entry.lifetime,
                                                        pause: !entry.live_paused,
                                                    });
                                                }
                                            });
                                        }
                                        ui.horizontal(|ui| {
                                            let mut topmost = entry.always_on_top;
                                            if ui
                                                .add_enabled(
                                                    entry.controls_available && !shutting_down,
                                                    egui::Checkbox::new(&mut topmost, "Topmost"),
                                                )
                                                .changed()
                                            {
                                                actions.push(PinListAction::Control {
                                                    pin_id: entry.pin_id,
                                                    lifetime: entry.lifetime,
                                                    change: PinControlChange {
                                                        always_on_top: Some(topmost),
                                                        click_through: None,
                                                    },
                                                });
                                            }
                                            let mut click_through = entry.click_through;
                                            if ui
                                                .add_enabled(
                                                    entry.controls_available && !shutting_down,
                                                    egui::Checkbox::new(
                                                        &mut click_through,
                                                        "Click-Through",
                                                    ),
                                                )
                                                .changed()
                                            {
                                                actions.push(PinListAction::Control {
                                                    pin_id: entry.pin_id,
                                                    lifetime: entry.lifetime,
                                                    change: PinControlChange {
                                                        always_on_top: None,
                                                        click_through: Some(click_through),
                                                    },
                                                });
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add_enabled(
                                                    entry.click_through
                                                        && entry.controls_available
                                                        && !shutting_down,
                                                    egui::Button::new("Recover"),
                                                )
                                                .clicked()
                                            {
                                                actions.push(PinListAction::Control {
                                                    pin_id: entry.pin_id,
                                                    lifetime: entry.lifetime,
                                                    change: PinControlChange {
                                                        always_on_top: None,
                                                        click_through: Some(false),
                                                    },
                                                });
                                            }
                                            if ui
                                                .add_enabled(!shutting_down, egui::Button::new("Close"))
                                                .clicked()
                                            {
                                                actions.push(PinListAction::Close {
                                                    pin_id: entry.pin_id,
                                                    lifetime: entry.lifetime,
                                                });
                                            }
                                        });
                                    });
                                });
                            });
                        });
                    }
                });
            });
        if let Some(id) = focus_widget_id {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }

        #[cfg(test)]
        {
            self.last_recover_all_widget_id = recover_all_widget_id;
        }

        for action in actions {
            match action {
                PinListAction::Control {
                    pin_id,
                    lifetime,
                    change,
                } => {
                    if let Err(error) = self.apply_list_control(
                        ctx,
                        board,
                        pin_id,
                        lifetime,
                        change,
                        native_api.as_ref(),
                        root_hwnd,
                    ) {
                        tracing::debug!(pin_id = pin_id.get(), %error, "pin-list control request was rejected");
                    }
                }
                PinListAction::RecoverAll => {
                    for entry in &entries {
                        if entry.click_through
                            && let Err(error) = self.apply_list_control(
                                ctx,
                                board,
                                entry.pin_id,
                                entry.lifetime,
                                PinControlChange {
                                    always_on_top: None,
                                    click_through: Some(false),
                                },
                                native_api.as_ref(),
                                root_hwnd,
                            )
                            && let Ok(registry) = self.registry.lock()
                            && let Some(runtime) = registry
                                .pins
                                .get(&entry.pin_id)
                                .filter(|runtime| runtime.lifetime == entry.lifetime)
                            && let Ok(mut state) = runtime.state.lock()
                        {
                            state.native_error = Some(error);
                        }
                    }
                }
                PinListAction::Close { pin_id, lifetime } => {
                    self.close_lifetime(board, pin_id, lifetime);
                }
                PinListAction::LiveControl {
                    pin_id,
                    lifetime,
                    pause,
                } => {
                    let current = self.registry.lock().is_ok_and(|registry| {
                        registry
                            .pins
                            .get(&pin_id)
                            .is_some_and(|runtime| runtime.lifetime == lifetime)
                    });
                    if current && !shutting_down {
                        let changed = if pause {
                            board.pause_live(pin_id)
                        } else {
                            board.resume_live(pin_id)
                        };
                        if changed
                            && let Ok(registry) = self.registry.lock()
                            && let Some(runtime) = registry.pins.get(&pin_id)
                        {
                            ctx.request_repaint_of(runtime.viewport_id);
                        }
                    }
                }
                PinListAction::CloseAll => {
                    for entry in &entries {
                        self.close_lifetime(board, entry.pin_id, entry.lifetime);
                    }
                }
            }
        }
        // Style requests do not show or focus a pin. Wake the root owner so its
        // next parent pass can apply the latest desired builder state.
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    pub(crate) fn request_pin_list_focus(&mut self) {
        self.pin_list_focus_requested = true;
    }

    #[cfg(test)]
    pub(crate) fn set_pin_click_through_for_test(
        &mut self,
        ctx: &egui::Context,
        board: &mut Pinboard,
        pin_id: PinId,
        enabled: bool,
    ) -> Result<(), String> {
        let (lifetime, native_api, root_hwnd) = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            let runtime = registry
                .pins
                .get(&pin_id)
                .ok_or_else(|| "pin window is not registered".to_owned())?;
            (
                runtime.lifetime,
                Arc::clone(&registry.native_api),
                registry.root_native_identity.0,
            )
        };
        self.apply_list_control(
            ctx,
            board,
            pin_id,
            lifetime,
            PinControlChange {
                always_on_top: None,
                click_through: Some(enabled),
            },
            native_api.as_ref(),
            root_hwnd,
        )
    }

    #[cfg(test)]
    pub(super) fn recover_all_widget_id_for_test(&self) -> Option<egui::Id> {
        self.last_recover_all_widget_id
    }

    fn apply_list_control(
        &mut self,
        ctx: &egui::Context,
        board: &mut Pinboard,
        pin_id: PinId,
        lifetime: u64,
        change: PinControlChange,
        native_api: &dyn PinNativeApi,
        root_hwnd: usize,
    ) -> Result<(), String> {
        if ctx.embed_viewports() || ctx.viewport_id() != egui::ViewportId::ROOT {
            return Err("pin controls require the launcher root viewport".into());
        }
        if board.get(pin_id).is_none() {
            return Err("pin control belongs to a closed pin".into());
        }
        let runtime = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            if registry.shutting_down {
                return Err("pin window owner is shutting down".into());
            }
            registry
                .pins
                .get(&pin_id)
                .filter(|runtime| runtime.lifetime == lifetime)
                .cloned()
                .ok_or_else(|| "pin control lifetime is stale".to_owned())?
        };
        if let Err(error) = apply_pin_control_change(ctx, &runtime, native_api, root_hwnd, change) {
            if let Ok(mut state) = runtime.state.lock() {
                state.native_error = Some(error.clone());
            }
            return Err(error);
        }
        ctx.request_repaint_of(runtime.viewport_id);
        Ok(())
    }

    fn close_lifetime(&mut self, board: &mut Pinboard, pin_id: PinId, lifetime: u64) -> bool {
        let is_current = self.registry.lock().is_ok_and(|registry| {
            registry
                .pins
                .get(&pin_id)
                .is_some_and(|runtime| runtime.lifetime == lifetime)
        });
        is_current && self.close(pin_id, board)
    }

    pub(crate) fn needs_viewport_bootstrap(&self, board: &Pinboard) -> bool {
        if self.capture_feedback.is_some() && !self.capture_feedback_closed.load(Ordering::Acquire)
        {
            return true;
        }
        let Ok(registry) = self.registry.lock() else {
            return false;
        };
        if registry.shutting_down {
            return false;
        }
        self.pending_parent_cleanup
            || board.pins().any(|pin| {
                registry.pins.get(&pin.id()).is_none_or(|runtime| {
                    runtime
                        .state
                        .lock()
                        .is_ok_and(|state| !state.root_registered && !state.close_requested)
                })
            })
    }

    pub(crate) fn needs_root_ui_when_hidden(&self, board: &Pinboard) -> bool {
        (self.capture_feedback.is_some() && !self.capture_feedback_closed.load(Ordering::Acquire))
            || self.registry.lock().is_ok_and(|registry| {
                !registry.shutting_down
                    && (self.pending_parent_cleanup
                        || !board.is_empty()
                        || !registry.pins.is_empty()
                        || !registry.retired.is_empty())
            })
    }

    /// Applies child lifecycle and worker receipts on the launcher logic tick.
    pub(crate) fn poll(&mut self, ctx: &egui::Context, board: &mut Pinboard) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.root_context = Some(ctx.clone());
        }
        let mut close_ids = Vec::new();
        let mut destroyed_retired = Vec::new();
        let mut geometry = Vec::new();
        let (pins, retired, root_hwnd, native_api) = {
            let Ok(registry) = self.registry.lock() else {
                return;
            };
            (
                registry.pins.values().cloned().collect::<Vec<_>>(),
                registry.retired.values().cloned().collect::<Vec<_>>(),
                registry.root_native_identity.0,
                Arc::clone(&registry.native_api),
            )
        };

        for runtime in pins.iter().chain(retired.iter()) {
            let (root_registered, existing, closing) = {
                let Ok(state) = runtime.state.lock() else {
                    continue;
                };
                (
                    state.root_registered,
                    state.native.clone(),
                    state.close_requested || state.native_destroyed,
                )
            };
            let is_retired = registry_contains_retired(&self.registry, runtime.pin_id);
            if closing && !is_retired {
                close_ids.push(runtime.pin_id);
            }
            match lookup_owned_pin_window(
                native_api.as_ref(),
                runtime,
                existing.as_ref(),
                root_hwnd,
            ) {
                NativeLookup::Found(binding) => {
                    let bounds = native_api.bounds(&binding);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = Some(binding);
                        state.native_error = None;
                        state.native_destroyed = false;
                    }
                    if !closing && let Some(bounds) = bounds {
                        geometry.push((runtime.pin_id, bounds));
                    }
                }
                NativeLookup::Destroyed => {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = None;
                        state.native_error = None;
                        state.native_destroyed = true;
                        state.close_requested = true;
                    }
                    if is_retired {
                        destroyed_retired.push(runtime.pin_id);
                    } else {
                        close_ids.push(runtime.pin_id);
                    }
                }
                NativeLookup::Missing
                    if is_retired && closing && !root_registered && existing.is_none() =>
                {
                    // The owner sent Close and no longer registers this deferred
                    // viewport; an exact native lookup confirms no HWND remains.
                    destroyed_retired.push(runtime.pin_id);
                }
                NativeLookup::Missing => {
                    // A missing window is not treated as closed unless its exact native
                    // identity was previously bound and Windows confirmed that HWND died.
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = None;
                        state.native_error =
                            Some("waiting for the owned pin window to appear".into());
                    }
                }
                NativeLookup::Unsupported => {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = None;
                        state.native_error =
                            Some("native pin-window controls are unavailable".into());
                    }
                }
                NativeLookup::Failed(error) => {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = None;
                        state.native_error = Some(error.clone());
                    }
                    tracing::warn!(pin_id = runtime.pin_id.get(), %error, "pin native window lookup failed");
                }
            }
        }

        for (pin_id, bounds) in geometry {
            if let Some(pin) = board.get_mut(pin_id) {
                pin.set_display_bounds(bounds);
                if let Some(runtime) = pins.iter().find(|runtime| runtime.pin_id == pin_id)
                    && let Ok(mut state) = runtime.state.lock()
                {
                    state.display_bounds = bounds;
                }
            }
        }

        for completion in self.actions.drain_completions() {
            let Some(runtime) = pins.iter().find(|runtime| {
                runtime.pin_id == completion.pin_id && runtime.lifetime == completion.lifetime
            }) else {
                continue;
            };
            if let Ok(mut state) = runtime.state.lock()
                && state.pending_request_id == Some(completion.request_id)
            {
                state.pending_request_id = None;
                state.action_status = match completion.result {
                    Ok(Some(message)) => PinActionStatus::Finished(message),
                    Ok(None) => PinActionStatus::Idle,
                    Err(error) => PinActionStatus::Failed(error),
                };
                ctx.request_repaint_of(runtime.viewport_id);
            }
        }

        for runtime in &pins {
            let Ok(state) = runtime.state.lock() else {
                continue;
            };
            if let Some(pin) = board.get_mut(runtime.pin_id) {
                let _ = pin.set_zoom(state.zoom);
                pin.set_always_on_top(state.always_on_top);
                pin.set_click_through(state.click_through);
            }
        }

        for pin_id in close_ids {
            if board.close(pin_id) {
                self.actions.cancel_unstarted(pin_id);
                self.retire(pin_id);
            }
        }
        let mut removed_retired = false;
        if let Ok(mut registry) = self.registry.lock() {
            for pin_id in destroyed_retired {
                removed_retired |= registry.retired.remove(&pin_id).is_some();
            }
        }
        if removed_retired {
            self.pending_parent_cleanup = true;
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }

        let mut finished_preparations = Vec::new();
        if let Ok(registry) = self.registry.lock() {
            for (key, preparation) in &registry.live_preparations {
                if preparation.is_cancelled() {
                    preparation.reap_retired_uploads();
                    if preparation.worker_finished() && !preparation.has_retained_uploads() {
                        finished_preparations.push(*key);
                    }
                }
            }
        }
        if !finished_preparations.is_empty()
            && let Ok(mut registry) = self.registry.lock()
        {
            for key in finished_preparations {
                if let Some(preparation) = registry.live_preparations.remove(&key)
                    && let Err(error) = preparation.cancel_and_join()
                {
                    tracing::error!(%error, "could not join retired live display worker");
                }
            }
        }

        let has_pending_action = self.actions.has_pending_work();
        let has_visibility_work = self.registry.lock().is_ok_and(|registry| {
            registry.hide_identity.is_some()
                || registry.restore_identity.is_some()
                || !registry.retired.is_empty()
        });
        let has_retired_live_display = self.registry.lock().is_ok_and(|registry| {
            registry
                .live_preparations
                .values()
                .any(|preparation| preparation.is_cancelled())
        });
        if has_pending_action || has_visibility_work {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        } else if has_retired_live_display {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn retire(&mut self, pin_id: PinId) {
        let retired = self.registry.lock().ok().and_then(|mut registry| {
            let runtime = registry.pins.remove(&pin_id)?;
            let viewport_id = runtime.viewport_id;
            let should_close = retire_pin_runtime(&mut registry, runtime).is_some();
            Some((registry.root_context.clone(), viewport_id, should_close))
        });
        if let Some((Some(ctx), viewport_id, true)) = retired.filter(|(ctx, _, should_close)| {
            *should_close && ctx.as_ref().is_some_and(|ctx| !ctx.embed_viewports())
        }) {
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Visible(false));
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }

    pub(crate) fn close(&mut self, pin_id: PinId, board: &mut Pinboard) -> bool {
        let closed = board.close(pin_id);
        if closed {
            self.actions.cancel_unstarted(pin_id);
            self.retire(pin_id);
        }
        closed
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        let (root_context, viewport_ids) = {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            registry.shutting_down = true;
            registry.hide_identity = None;
            registry.restore_identity = None;
            let viewport_ids = registry
                .pins
                .values()
                .chain(registry.retired.values())
                .filter_map(|runtime| {
                    if runtime.viewport_id == egui::ViewportId::ROOT
                        || runtime.viewport_id != pin_viewport_id(runtime.pin_id, runtime.lifetime)
                    {
                        return None;
                    }
                    let Ok(mut state) = runtime.state.lock() else {
                        return None;
                    };
                    let should_close = !state.native_destroyed && !state.close_command_sent;
                    state.close_requested = true;
                    state.desired_visible = false;
                    state.root_registered = false;
                    state.close_command_sent = true;
                    state.capture_visibility = None;
                    state.native_error = None;
                    should_close.then_some(runtime.viewport_id)
                })
                .collect::<Vec<_>>();
            (registry.root_context.clone(), viewport_ids)
        };
        if let Some(ctx) = root_context.filter(|ctx| !ctx.embed_viewports()) {
            for viewport_id in viewport_ids {
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
            }
        }
        let action_result = self.actions.shutdown_and_join();
        let preparations = self
            .registry
            .lock()
            .map(|registry| {
                registry
                    .live_preparations
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut preparation_errors = Vec::new();
        for preparation in &preparations {
            if let Err(error) = preparation.cancel_and_join() {
                preparation_errors.push(error);
            }
            preparation.reap_retired_uploads();
        }
        if let Ok(mut registry) = self.registry.lock() {
            registry.pins.clear();
            registry.retired.clear();
            registry
                .live_preparations
                .retain(|_, preparation| preparation.has_retained_uploads());
            registry.hide_identity = None;
            registry.restore_identity = None;
            registry.root_context = None;
        }
        self.pending_parent_cleanup = false;
        match (action_result, preparation_errors.is_empty()) {
            (Err(error), _) => Err(error),
            (Ok(()), false) => Err(preparation_errors.join("; ")),
            (Ok(()), true) => Ok(()),
        }
    }
}

fn pin_native_title(process_id: u32, session_id: u128, pin_id: PinId, lifetime: u64) -> String {
    format!(
        "Screenshot Pin [{process_id}:{session_id:032x}:{}:{lifetime}]",
        pin_id.get()
    )
}

fn retire_pin_runtime(
    registry: &mut PinWindowRegistry,
    runtime: Arc<PinWindowRuntime>,
) -> Option<egui::ViewportId> {
    let pin_id = runtime.pin_id;
    if let Some(preparation) = runtime.image_source.live_preparation() {
        preparation.cancel();
    }
    let mut should_close = false;
    if let Ok(mut state) = runtime.state.lock() {
        state.close_requested = true;
        state.desired_visible = false;
        state.root_registered = false;
        state.capture_visibility = None;
        if !state.native_destroyed && !state.close_command_sent {
            state.close_command_sent = true;
            should_close = true;
        }
    }
    let viewport_id = runtime.viewport_id;
    registry.retired.insert(pin_id, runtime);
    let valid_child = registry.retired.get(&pin_id).is_some_and(|runtime| {
        viewport_id != egui::ViewportId::ROOT
            && runtime.viewport_id == pin_viewport_id(runtime.pin_id, runtime.lifetime)
    });
    (should_close && valid_child).then_some(viewport_id)
}

fn should_suppress_pin(
    runtime: &PinWindowRuntime,
    hide_identity: Option<crate::pinboard::PinCaptureIdentity>,
    restore_identity: Option<crate::pinboard::PinCaptureIdentity>,
    shutting_down: bool,
) -> bool {
    let Ok(state) = runtime.state.lock() else {
        return true;
    };
    if shutting_down || state.close_requested || !state.desired_visible {
        return true;
    }
    let Some(identity) = hide_identity else {
        return false;
    };
    if restore_identity != Some(identity) {
        return true;
    }
    state
        .capture_visibility
        .filter(|ticket| ticket.identity == Some(identity))
        .is_none_or(|ticket| ticket.prior_visible != Some(true))
}

fn registry_contains_retired(registry: &Arc<Mutex<PinWindowRegistry>>, pin_id: PinId) -> bool {
    registry
        .lock()
        .is_ok_and(|registry| registry.retired.contains_key(&pin_id))
}

/*
 * The block below used to live in this module before lifetime-aware retirement
 * was added. Keep state transitions in PinWindowPresenter so closed pins never
 * return to the active Pinboard or get shown by capture restoration.
 */

struct PinWindowVisibilityOwner {
    registry: Arc<Mutex<PinWindowRegistry>>,
}

impl PinWindowVisibilityCoordinator for PinWindowVisibilityOwner {
    fn request_hide_all(
        &mut self,
        identity: crate::pinboard::PinCaptureIdentity,
    ) -> Result<(), String> {
        let (ctx, entries, root_identity, native_api) = {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            let ctx = registry.root_context.clone();
            if ctx.as_ref().is_some_and(egui::Context::embed_viewports) {
                return Err("screenshot pin windows require native deferred viewports".into());
            }
            if registry
                .hide_identity
                .is_some_and(|current| current != identity)
            {
                return Err("another pin visibility transaction is active".into());
            }
            registry.hide_identity = Some(identity);
            registry.restore_identity = None;
            (
                ctx,
                registry
                    .pins
                    .values()
                    .chain(registry.retired.values())
                    .cloned()
                    .collect::<Vec<_>>(),
                registry.root_native_identity,
                Arc::clone(&registry.native_api),
            )
        };
        for runtime in entries {
            let (existing, never_registered, desired_visible, closed) = {
                let state = runtime
                    .state
                    .lock()
                    .map_err(|_| "pin window state lock was poisoned".to_owned())?;
                (
                    state.native.clone(),
                    !state.root_registered && !state.callback_entered && state.native.is_none(),
                    state.desired_visible,
                    state.close_requested || state.native_destroyed,
                )
            };
            let mut capture = CaptureVisibilityState {
                identity: Some(identity),
                ..Default::default()
            };
            match lookup_owned_pin_window(
                native_api.as_ref(),
                &runtime,
                existing.as_ref(),
                root_identity.0,
            ) {
                NativeLookup::Found(binding) => {
                    let prior_visible = native_api.is_visible(&binding)?;
                    if closed {
                        capture.prior_visible = Some(false);
                        capture.hide_requested = true;
                        if let Ok(mut state) = runtime.state.lock() {
                            state.desired_visible = false;
                            state.native = Some(binding);
                            state.capture_visibility = Some(capture);
                        }
                        let ctx = ctx.as_ref().ok_or_else(|| {
                            "pin viewport context is unavailable for capture exclusion".to_owned()
                        })?;
                        ctx.send_viewport_cmd_to(
                            runtime.viewport_id,
                            egui::ViewportCommand::Visible(false),
                        );
                        continue;
                    }
                    capture.prior_visible = Some(prior_visible);
                    capture.hide_requested = true;
                    if let Ok(mut state) = runtime.state.lock() {
                        state.desired_visible = prior_visible;
                        state.native = Some(binding);
                        state.capture_visibility = Some(capture);
                    }
                    let ctx = ctx.as_ref().ok_or_else(|| {
                        "pin viewport context is unavailable for capture exclusion".to_owned()
                    })?;
                    ctx.send_viewport_cmd_to(
                        runtime.viewport_id,
                        egui::ViewportCommand::Visible(false),
                    );
                }
                NativeLookup::Missing if never_registered => {
                    // The owner proves no deferred child registration was requested.
                    capture.prior_visible = Some(false);
                    capture.absence_proven = true;
                    if let Ok(mut state) = runtime.state.lock() {
                        state.capture_visibility = Some(capture);
                    }
                }
                NativeLookup::Destroyed => {
                    let retired = registry_contains_retired(&self.registry, runtime.pin_id);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native_destroyed = true;
                        state.close_requested = true;
                        capture.prior_visible = Some(false);
                        capture.absence_proven = retired;
                        state.capture_visibility = Some(capture);
                    }
                }
                NativeLookup::Missing | NativeLookup::Unsupported => {
                    // Keep the visibility intent that existed before capture. The
                    // actual native state remains unacknowledged until an exact
                    // owned HWND materializes and is observed.
                    capture.prior_visible = Some(desired_visible);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.capture_visibility = Some(capture);
                    }
                }
                NativeLookup::Failed(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn poll_hidden(&mut self, identity: crate::pinboard::PinCaptureIdentity) -> PinVisibilityAck {
        let (ctx, entries, root_identity, native_api, restore_requested) = match self
            .registry
            .lock()
        {
            Ok(registry) if registry.hide_identity == Some(identity) => (
                registry.root_context.clone(),
                registry
                    .pins
                    .values()
                    .chain(registry.retired.values())
                    .cloned()
                    .collect::<Vec<_>>(),
                registry.root_native_identity,
                Arc::clone(&registry.native_api),
                registry.restore_identity == Some(identity),
            ),
            Ok(_) => return PinVisibilityAck::Failed("pin hide request identity is stale".into()),
            Err(_) => {
                return PinVisibilityAck::Failed("pin window registry lock was poisoned".into());
            }
        };
        let mut pending = false;
        for runtime in entries {
            let (existing, callback_entered, desired_visible, closed, capture) = match runtime
                .state
                .lock()
            {
                Ok(state) => (
                    state.native.clone(),
                    state.callback_entered,
                    state.desired_visible,
                    state.close_requested || state.native_destroyed,
                    state.capture_visibility,
                ),
                Err(_) => {
                    return PinVisibilityAck::Failed("pin window state lock was poisoned".into());
                }
            };
            if capture.is_some_and(|state| state.identity != Some(identity)) {
                return PinVisibilityAck::Failed("pin hide ticket identity changed".into());
            }
            match lookup_owned_pin_window(
                native_api.as_ref(),
                &runtime,
                existing.as_ref(),
                root_identity.0,
            ) {
                NativeLookup::Found(binding) => {
                    let visible = match native_api.is_visible(&binding) {
                        Ok(visible) => visible,
                        Err(error) => return PinVisibilityAck::Failed(error),
                    };
                    if closed {
                        if let Ok(mut state) = runtime.state.lock() {
                            state.native = Some(binding);
                            state.desired_visible = false;
                            if let Some(ticket) = state.capture_visibility.as_mut() {
                                ticket.prior_visible = Some(false);
                                ticket.hide_requested = true;
                                ticket.restore_intent = true;
                                ticket.restore_acknowledged = true;
                                ticket.hide_acknowledged = !visible;
                            }
                        }
                        if visible {
                            let Some(ctx) = ctx.as_ref() else {
                                return PinVisibilityAck::Failed(
                                    "pin viewport context is unavailable for capture exclusion"
                                        .into(),
                                );
                            };
                            ctx.send_viewport_cmd_to(
                                runtime.viewport_id,
                                egui::ViewportCommand::Visible(false),
                            );
                            pending = true;
                        }
                        continue;
                    }
                    let Some(ticket) = capture.filter(|ticket| ticket.identity == Some(identity))
                    else {
                        pending = true;
                        continue;
                    };
                    if restore_requested || ticket.restore_intent {
                        let target = ticket.prior_visible.unwrap_or(desired_visible);
                        if let Ok(mut state) = runtime.state.lock() {
                            state.native = Some(binding.clone());
                            state.desired_visible = target;
                            if let Some(ticket) = state.capture_visibility.as_mut() {
                                ticket.restore_intent = true;
                                if visible == target {
                                    ticket.restore_acknowledged = true;
                                } else {
                                    ticket.restore_requested = true;
                                }
                            }
                        }
                        if visible != target {
                            let Some(ctx) = ctx.as_ref() else {
                                return PinVisibilityAck::Failed(
                                    "pin viewport context is unavailable for restoration".into(),
                                );
                            };
                            ctx.send_viewport_cmd_to(
                                runtime.viewport_id,
                                egui::ViewportCommand::Visible(target),
                            );
                            pending = true;
                        }
                    } else if !ticket.hide_requested {
                        if let Ok(mut state) = runtime.state.lock() {
                            state.native = Some(binding);
                            state.desired_visible = ticket.prior_visible.unwrap_or(visible);
                            if let Some(ticket) = state.capture_visibility.as_mut() {
                                ticket.prior_visible.get_or_insert(visible);
                                ticket.hide_requested = true;
                                ticket.hide_acknowledged = false;
                            }
                        }
                        let Some(ctx) = ctx.as_ref() else {
                            return PinVisibilityAck::Failed(
                                "pin viewport context is unavailable for capture exclusion".into(),
                            );
                        };
                        ctx.send_viewport_cmd_to(
                            runtime.viewport_id,
                            egui::ViewportCommand::Visible(false),
                        );
                        pending = true;
                    } else if !visible {
                        if let Ok(mut state) = runtime.state.lock() {
                            state.native = Some(binding);
                            if let Some(ticket) = state.capture_visibility.as_mut() {
                                ticket.hide_acknowledged = true;
                            }
                        }
                    } else {
                        pending = true;
                    }
                }
                NativeLookup::Destroyed => {
                    let retired = registry_contains_retired(&self.registry, runtime.pin_id);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native_destroyed = true;
                        state.close_requested = true;
                        if retired && let Some(ticket) = state.capture_visibility.as_mut() {
                            ticket.absence_proven = true;
                            ticket.hide_acknowledged = true;
                        }
                    }
                    pending |= !retired;
                }
                NativeLookup::Missing | NativeLookup::Unsupported => {
                    if capture.is_some_and(|ticket| ticket.absence_proven) {
                        continue;
                    }
                    // A root-registered or callback-entered child that has not been
                    // observed natively cannot be acknowledged by absence.
                    let _ = callback_entered;
                    pending = true;
                }
                NativeLookup::Failed(error) => return PinVisibilityAck::Failed(error),
            }
        }
        if pending {
            PinVisibilityAck::Pending
        } else {
            PinVisibilityAck::Acknowledged
        }
    }

    fn request_restore_all(
        &mut self,
        identity: crate::pinboard::PinCaptureIdentity,
    ) -> Result<(), String> {
        let (ctx, entries, root_identity, native_api) = {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            let ctx = registry.root_context.clone();
            if ctx.as_ref().is_some_and(egui::Context::embed_viewports) {
                return Err("screenshot pin windows require native deferred viewports".into());
            }
            if registry
                .hide_identity
                .is_some_and(|current| current != identity)
            {
                return Err("pin restore request identity is stale".into());
            }
            registry.restore_identity = Some(identity);
            (
                ctx,
                registry
                    .pins
                    .values()
                    .chain(registry.retired.values())
                    .cloned()
                    .collect::<Vec<_>>(),
                registry.root_native_identity,
                Arc::clone(&registry.native_api),
            )
        };
        for runtime in entries {
            let (existing, closed, desired_visible, capture) = {
                let state = runtime
                    .state
                    .lock()
                    .map_err(|_| "pin window state lock was poisoned".to_owned())?;
                (
                    state.native.clone(),
                    state.close_requested || state.native_destroyed,
                    state.desired_visible,
                    state.capture_visibility,
                )
            };
            let Some(capture) = capture.filter(|capture| capture.identity == Some(identity)) else {
                continue;
            };
            if closed {
                if let Ok(mut state) = runtime.state.lock()
                    && let Some(ticket) = state.capture_visibility.as_mut()
                {
                    ticket.restore_intent = true;
                    ticket.restore_acknowledged = true;
                }
                continue;
            }
            let target = capture.prior_visible.unwrap_or(desired_visible);
            match lookup_owned_pin_window(
                native_api.as_ref(),
                &runtime,
                existing.as_ref(),
                root_identity.0,
            ) {
                NativeLookup::Found(binding) => {
                    let visible = native_api.is_visible(&binding)?;
                    let restore_requested =
                        visible != target || (capture.hide_requested && !capture.hide_acknowledged);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = Some(binding);
                        state.desired_visible = target;
                        if let Some(ticket) = state.capture_visibility.as_mut() {
                            ticket.restore_intent = true;
                            ticket.restore_requested = restore_requested;
                            ticket.restore_acknowledged = !restore_requested;
                        }
                    }
                    if restore_requested {
                        let Some(ctx) = ctx.as_ref() else {
                            return Err(
                                "pin viewport context is unavailable for restoration".into()
                            );
                        };
                        ctx.send_viewport_cmd_to(
                            runtime.viewport_id,
                            egui::ViewportCommand::Visible(target),
                        );
                    }
                }
                NativeLookup::Destroyed => {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native_destroyed = true;
                        state.close_requested = true;
                        if let Some(ticket) = state.capture_visibility.as_mut() {
                            ticket.restore_intent = true;
                            ticket.restore_acknowledged = true;
                        }
                    }
                }
                NativeLookup::Missing | NativeLookup::Unsupported => {
                    if let Ok(mut state) = runtime.state.lock()
                        && let Some(ticket) = state.capture_visibility.as_mut()
                    {
                        ticket.restore_intent = true;
                    }
                }
                NativeLookup::Failed(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn poll_restored(&mut self, identity: crate::pinboard::PinCaptureIdentity) -> PinVisibilityAck {
        let (ctx, entries, root_identity, native_api) = match self.registry.lock() {
            Ok(registry) if registry.restore_identity == Some(identity) => (
                registry.root_context.clone(),
                registry
                    .pins
                    .values()
                    .chain(registry.retired.values())
                    .cloned()
                    .collect::<Vec<_>>(),
                registry.root_native_identity,
                Arc::clone(&registry.native_api),
            ),
            Ok(_) => {
                return PinVisibilityAck::Failed("pin restore request identity is stale".into());
            }
            Err(_) => {
                return PinVisibilityAck::Failed("pin window registry lock was poisoned".into());
            }
        };
        let mut pending = false;
        for runtime in entries {
            let (existing, closed, desired_visible, capture) = match runtime.state.lock() {
                Ok(state) => (
                    state.native.clone(),
                    state.close_requested || state.native_destroyed,
                    state.desired_visible,
                    state.capture_visibility,
                ),
                Err(_) => {
                    return PinVisibilityAck::Failed("pin window state lock was poisoned".into());
                }
            };
            let Some(capture) = capture.filter(|capture| capture.identity == Some(identity)) else {
                continue;
            };
            let hide_may_still_apply = capture.hide_requested && !capture.hide_acknowledged;
            if capture.restore_acknowledged && !hide_may_still_apply {
                continue;
            }
            if closed {
                if let Ok(mut state) = runtime.state.lock()
                    && let Some(ticket) = state.capture_visibility.as_mut()
                {
                    ticket.restore_acknowledged = true;
                }
                continue;
            }
            let target = capture.prior_visible.unwrap_or(desired_visible);
            match lookup_owned_pin_window(
                native_api.as_ref(),
                &runtime,
                existing.as_ref(),
                root_identity.0,
            ) {
                NativeLookup::Found(binding) => {
                    let visible = match native_api.is_visible(&binding) {
                        Ok(visible) => visible,
                        Err(error) => return PinVisibilityAck::Failed(error),
                    };
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = Some(binding);
                        state.desired_visible = target;
                        if let Some(ticket) = state.capture_visibility.as_mut() {
                            if visible == target {
                                ticket.restore_acknowledged = true;
                            } else if !ticket.restore_requested {
                                ticket.restore_requested = true;
                                let Some(ctx) = ctx.as_ref() else {
                                    return PinVisibilityAck::Failed(
                                        "pin viewport context is unavailable for restoration"
                                            .into(),
                                    );
                                };
                                ctx.send_viewport_cmd_to(
                                    runtime.viewport_id,
                                    egui::ViewportCommand::Visible(target),
                                );
                            }
                        }
                    }
                    if visible != target {
                        pending = true;
                    }
                }
                NativeLookup::Destroyed => {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native_destroyed = true;
                        state.close_requested = true;
                        if let Some(ticket) = state.capture_visibility.as_mut() {
                            ticket.restore_intent = true;
                            ticket.restore_acknowledged = true;
                        }
                    }
                }
                NativeLookup::Missing | NativeLookup::Unsupported => {
                    if capture.absence_proven {
                        if let Ok(mut state) = runtime.state.lock()
                            && let Some(ticket) = state.capture_visibility.as_mut()
                        {
                            ticket.restore_acknowledged = true;
                        }
                    } else {
                        pending = true;
                    }
                }
                NativeLookup::Failed(error) => return PinVisibilityAck::Failed(error),
            }
        }
        if pending {
            PinVisibilityAck::Pending
        } else {
            if let Ok(mut registry) = self.registry.lock()
                && registry.restore_identity == Some(identity)
            {
                registry.restore_identity = None;
                registry.hide_identity = None;
                for runtime in registry.pins.values().chain(registry.retired.values()) {
                    if let Ok(mut state) = runtime.state.lock() {
                        state.capture_visibility = None;
                    }
                }
            }
            PinVisibilityAck::Acknowledged
        }
    }

    fn cancel_and_restore_all(
        &mut self,
        identity: crate::pinboard::PinCaptureIdentity,
    ) -> Result<(), String> {
        let current = self
            .registry
            .lock()
            .map_err(|_| "pin window registry lock was poisoned".to_owned())?
            .hide_identity;
        if current.is_none() {
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| "pin window registry lock was poisoned".to_owned())?;
            registry.restore_identity = Some(identity);
            registry.hide_identity = Some(identity);
            return Ok(());
        }
        self.request_restore_all(identity)
    }

    fn shutdown_and_drain(
        &mut self,
        identity: crate::pinboard::PinCaptureIdentity,
    ) -> Result<(), String> {
        self.cancel_and_restore_all(identity)?;
        match self.poll_restored(identity) {
            PinVisibilityAck::Acknowledged => Ok(()),
            PinVisibilityAck::Pending => {
                Err("pin visibility restoration is still pending during shutdown".into())
            }
            PinVisibilityAck::Failed(error) => Err(error),
        }
    }
}

fn pin_viewport_id(pin_id: PinId, lifetime: u64) -> egui::ViewportId {
    let viewport_id = egui::ViewportId::from_hash_of(("screenshot-pin", pin_id.get(), lifetime));
    if viewport_id == egui::ViewportId::ROOT {
        egui::ViewportId::from_hash_of(("screenshot-pin-child", pin_id.get(), lifetime))
    } else {
        viewport_id
    }
}

fn pin_viewport_builder(runtime: &PinWindowRuntime) -> egui::ViewportBuilder {
    let width = runtime.dimensions.0.min(DEFAULT_PINNER_WIDTH as u32) as f32;
    let height = runtime.dimensions.1.min(DEFAULT_PINNER_HEIGHT as u32) as f32;
    let (always_on_top, click_through) = runtime
        .state
        .lock()
        .map(|state| (state.always_on_top, state.click_through))
        .unwrap_or((true, false));
    egui::ViewportBuilder::default()
        .with_title(runtime.title.clone())
        .with_inner_size([width.max(220.0), height.max(160.0)])
        .with_min_inner_size([220.0, 160.0])
        .with_resizable(true)
        .with_window_level(if always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        })
        .with_mouse_passthrough(click_through)
}

fn apply_pin_control_change(
    ctx: &egui::Context,
    runtime: &PinWindowRuntime,
    native_api: &dyn PinNativeApi,
    root_hwnd: usize,
    change: PinControlChange,
) -> Result<(), String> {
    if ctx.embed_viewports() || ctx.viewport_id() == egui::ViewportId::ROOT {
        if ctx.embed_viewports() {
            return Err("pin controls require independent native viewports".into());
        }
        if runtime.viewport_id == egui::ViewportId::ROOT {
            return Err("pin control cannot target the launcher root viewport".into());
        }
    } else if ctx.viewport_id() != runtime.viewport_id {
        return Err("pin control caller does not own this child viewport".into());
    }
    if runtime.viewport_id != pin_viewport_id(runtime.pin_id, runtime.lifetime) {
        return Err("pin control viewport identity is stale".into());
    }
    let (existing, current_topmost, current_click_through, root_registered) = {
        let state = runtime
            .state
            .lock()
            .map_err(|_| "pin window state lock was poisoned".to_owned())?;
        if state.close_requested || state.native_destroyed {
            return Err("pin control belongs to a closed child".into());
        }
        (
            state.native.clone(),
            state.always_on_top,
            state.click_through,
            state.root_registered,
        )
    };
    if !root_registered {
        return Err("pin child is not registered by the launcher root".into());
    }
    let binding = match lookup_owned_pin_window(native_api, runtime, existing.as_ref(), root_hwnd) {
        NativeLookup::Found(binding) => binding,
        NativeLookup::Destroyed => return Err("pin child window was destroyed".into()),
        NativeLookup::Missing => return Err("pin child window is not yet available".into()),
        NativeLookup::Unsupported => {
            return Err("native pin-window controls are unavailable".into());
        }
        NativeLookup::Failed(error) => return Err(error),
    };

    let next_topmost = change.always_on_top.unwrap_or(current_topmost);
    let next_click_through = change.click_through.unwrap_or(current_click_through);
    let topmost_changed = next_topmost != current_topmost;
    let click_through_changed = next_click_through != current_click_through;
    {
        let mut state = runtime
            .state
            .lock()
            .map_err(|_| "pin window state lock was poisoned".to_owned())?;
        if state.close_requested
            || state.native_destroyed
            || !state.root_registered
            || state.native.as_ref().is_some_and(|current| {
                current.hwnd != binding.hwnd
                    || current.lifetime != runtime.lifetime
                    || current.title != runtime.title
            })
        {
            return Err("pin child binding changed while applying its control".into());
        }
        state.native = Some(binding);
        state.native_error = None;
        state.always_on_top = next_topmost;
        state.click_through = next_click_through;
    }
    if topmost_changed {
        ctx.send_viewport_cmd_to(
            runtime.viewport_id,
            egui::ViewportCommand::WindowLevel(if next_topmost {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }),
        );
    }
    if click_through_changed {
        ctx.send_viewport_cmd_to(
            runtime.viewport_id,
            egui::ViewportCommand::MousePassthrough(next_click_through),
        );
    }
    ctx.request_repaint_of(egui::ViewportId::ROOT);
    Ok(())
}

fn render_pin_viewport(
    ui: &mut egui::Ui,
    class: egui::ViewportClass,
    runtime: &Arc<PinWindowRuntime>,
    actions: &PinActionClient,
    native_api: &Arc<dyn PinNativeApi>,
    root_native_identity: (usize, u64),
    callback_registry: &std::sync::Weak<Mutex<PinWindowRegistry>>,
    capture_suppressed: bool,
) {
    let ctx = ui.ctx().clone();
    let Some(registry) = callback_registry.upgrade() else {
        return;
    };
    let root_context = registry
        .lock()
        .ok()
        .and_then(|registry| registry.root_context.clone());
    drop(registry);
    let Some(root_context) = root_context else {
        return;
    };
    if ctx.embed_viewports() || class != egui::ViewportClass::Deferred {
        ui.label("Screenshot pins require an independent native window.");
        return;
    }
    if ctx.viewport_id() != runtime.viewport_id
        || runtime.viewport_id == egui::ViewportId::ROOT
        || runtime.viewport_id != pin_viewport_id(runtime.pin_id, runtime.lifetime)
        || root_context.embed_viewports()
    {
        ui.label("Screenshot pin ownership could not be verified.");
        return;
    }
    let existing_native = if let Ok(mut state) = runtime.state.lock() {
        state.callback_entered = true;
        state.native.clone()
    } else {
        None
    };
    let (close_requested, action_status, scale_mode, zoom, mut always_on_top, mut click_through) = {
        let Ok(state) = runtime.state.lock() else {
            return;
        };
        (
            state.close_requested,
            state.action_status.clone(),
            state.scale_mode,
            state.zoom,
            state.always_on_top,
            state.click_through,
        )
    };
    let mut native_destroyed = false;
    let native_ready = match lookup_owned_pin_window(
        native_api.as_ref(),
        runtime,
        existing_native.as_ref(),
        root_native_identity.0,
    ) {
        NativeLookup::Found(binding) => {
            if let Ok(mut state) = runtime.state.lock() {
                state.native = Some(binding);
                state.native_error = None;
                state.native_destroyed = false;
            }
            true
        }
        NativeLookup::Destroyed => {
            native_destroyed = true;
            if let Ok(mut state) = runtime.state.lock() {
                state.native = None;
                state.native_error = None;
                state.native_destroyed = true;
                state.close_requested = true;
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
            false
        }
        NativeLookup::Failed(error) => {
            if let Ok(mut state) = runtime.state.lock() {
                state.native = None;
                state.native_error = Some(error.clone());
            }
            tracing::warn!(pin_id = runtime.pin_id.get(), %error, "pin child native identity validation failed");
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            return;
        }
        NativeLookup::Missing => {
            if let Ok(mut state) = runtime.state.lock() {
                state.native = None;
                state.native_error = Some("waiting for the owned pin window to appear".into());
            }
            false
        }
        NativeLookup::Unsupported => {
            if let Ok(mut state) = runtime.state.lock() {
                state.native = None;
                state.native_error = Some("native pin-window controls are unavailable".into());
            }
            false
        }
    };
    if native_destroyed {
        return;
    }
    if capture_suppressed || close_requested {
        root_context
            .send_viewport_cmd_to(runtime.viewport_id, egui::ViewportCommand::Visible(false));
        if close_requested {
            if let Ok(mut state) = runtime.state.lock() {
                state.close_command_sent = true;
            }
            root_context.send_viewport_cmd_to(runtime.viewport_id, egui::ViewportCommand::Close);
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        return;
    }

    let first_pass = ctx.current_pass_index() == 0;
    let native_close = ctx.input(|input| input.viewport().close_requested());
    let mut escape = false;
    ctx.input_mut(|input| {
        escape = input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
    });
    if first_pass && native_close {
        if let Ok(mut state) = runtime.state.lock() {
            state.close_requested = true;
            state.close_command_sent = true;
        }
        root_context.send_viewport_cmd_to(runtime.viewport_id, egui::ViewportCommand::Close);
        root_context.request_repaint_of(egui::ViewportId::ROOT);
        return;
    }
    if first_pass
        && escape
        && click_through
        && native_ready
        && let Err(error) = apply_pin_control_change(
            &ctx,
            runtime,
            native_api.as_ref(),
            root_native_identity.0,
            PinControlChange {
                always_on_top: None,
                click_through: Some(false),
            },
        )
        && let Ok(mut state) = runtime.state.lock()
    {
        state.native_error = Some(error);
    }

    let texture = match ensure_pin_texture(&ctx, runtime) {
        Ok(texture) => texture,
        Err(error) => {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
            return;
        }
    };
    let child_pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
    let mut close_clicked = false;
    let mut copy_requested = false;
    let mut save_requested = false;
    let mut live_pause_change = None;
    let mut control_change = PinControlChange::default();
    let mut next_mode = scale_mode;
    let mut next_zoom = zoom;

    ui.push_id(("screenshot-pin", runtime.pin_id.get()), |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(next_mode == PinScaleMode::Fit, "Fit")
                .clicked()
            {
                next_mode = PinScaleMode::Fit;
            }
            if ui
                .selectable_label(next_mode == PinScaleMode::Actual, "1:1")
                .clicked()
            {
                next_mode = PinScaleMode::Actual;
            }
            if ui
                .selectable_label(next_mode == PinScaleMode::Custom, "Custom")
                .clicked()
            {
                next_mode = PinScaleMode::Custom;
            }
            if next_mode == PinScaleMode::Custom {
                ui.add(
                    egui::Slider::new(&mut next_zoom, 0.1..=8.0)
                        .logarithmic(true)
                        .text("Zoom"),
                );
            }
            if let Some(preparation) = runtime.image_source.live_preparation() {
                let status = preparation.status();
                if let Some(error) = preparation.error() {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                } else {
                    ui.label(live_status_label(&status));
                }
                if !is_terminal_live_status(&status) {
                    let paused = matches!(status, PinLiveStatus::Paused);
                    if ui.button(if paused { "Resume" } else { "Pause" }).clicked() && first_pass {
                        live_pause_change = Some(!paused);
                    }
                }
            }
            if ui.button("Copy Image").clicked() && first_pass {
                copy_requested = true;
            }
            if ui.button("Save As").clicked() && first_pass {
                save_requested = true;
            }
            let mut topmost = always_on_top;
            if ui
                .add_enabled(
                    native_ready,
                    egui::Checkbox::new(&mut topmost, "Always on Top"),
                )
                .changed()
                && first_pass
            {
                control_change.always_on_top = Some(topmost);
                always_on_top = topmost;
            }
            let mut passthrough = click_through;
            if ui
                .add_enabled(
                    native_ready,
                    egui::Checkbox::new(&mut passthrough, "Click-Through"),
                )
                .on_hover_text("Use Pin List or Escape to recover a click-through pin.")
                .changed()
                && first_pass
            {
                control_change.click_through = Some(passthrough);
                click_through = passthrough;
            }
            if ui.button("Close").clicked() && first_pass {
                close_clicked = true;
            }
        });
        if let PinActionStatus::Pending(message)
        | PinActionStatus::Finished(message)
        | PinActionStatus::Failed(message) = &action_status
        {
            let color = if matches!(action_status, PinActionStatus::Failed(_)) {
                egui::Color32::LIGHT_RED
            } else {
                ui.visuals().weak_text_color()
            };
            ui.colored_label(color, message);
        }

        let available = ui.available_size().max(egui::vec2(1.0, 1.0));
        let image_size = pin_image_display_size(
            runtime.dimensions,
            next_mode,
            next_zoom,
            available,
            child_pixels_per_point,
        );
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(texture) = &texture {
                    ui.add(egui::Image::new((texture.id(), image_size)));
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Waiting for a prepared source frame…");
                    });
                }
            });
    });

    if first_pass {
        let mut request_id = None;
        if (copy_requested || save_requested)
            && let Ok(mut state) = runtime.state.lock()
        {
            if state.pending_request_id.is_none() {
                if let Some(next_request_id) = state.next_request_id.checked_add(1) {
                    request_id = Some(state.next_request_id);
                    state.next_request_id = next_request_id;
                    state.pending_request_id = request_id;
                    state.action_status = PinActionStatus::Pending(if copy_requested {
                        "Copying image…".into()
                    } else {
                        "Choose a save location…".into()
                    });
                } else {
                    state.action_status = PinActionStatus::Failed(
                        "Pin action request IDs are exhausted; close and recapture this pin."
                            .into(),
                    );
                }
            } else {
                state.action_status = PinActionStatus::Failed(
                    "Finish the current pin image action before starting another.".into(),
                );
            }
        }
        if let Some(request_id) = request_id {
            if copy_requested {
                submit_pin_action(runtime, actions, request_id, PinActionKind::Copy);
            } else if save_requested {
                submit_pin_action(runtime, actions, request_id, PinActionKind::SaveAs);
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if let Some(pause) = live_pause_change
            && let Some(preparation) = runtime.image_source.live_preparation()
        {
            let changed = if pause {
                preparation.pause()
            } else {
                preparation.resume()
            };
            if changed {
                ctx.request_repaint_of(runtime.viewport_id);
            }
        }
        if next_mode != scale_mode || (next_mode == PinScaleMode::Custom && next_zoom != zoom) {
            if let Ok(mut state) = runtime.state.lock() {
                state.scale_mode = next_mode;
                state.zoom = next_zoom.clamp(0.1, 8.0);
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if control_change != PinControlChange::default()
            && let Err(error) = apply_pin_control_change(
                &ctx,
                runtime,
                native_api.as_ref(),
                root_native_identity.0,
                control_change,
            )
            && let Ok(mut state) = runtime.state.lock()
        {
            state.native_error = Some(error);
        }
        if close_clicked {
            if let Ok(mut state) = runtime.state.lock() {
                state.close_requested = true;
                state.close_command_sent = true;
            }
            root_context.send_viewport_cmd_to(runtime.viewport_id, egui::ViewportCommand::Close);
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

fn ensure_pin_texture(
    ctx: &egui::Context,
    runtime: &PinWindowRuntime,
) -> Result<Option<egui::TextureHandle>, String> {
    match &runtime.image_source {
        PinWindowImageSource::Static(image) => {
            if let Some(texture) = runtime
                .state
                .lock()
                .map_err(|_| "pin texture state lock was poisoned".to_owned())?
                .texture
                .as_ref()
                .cloned()
            {
                return Ok(Some(texture));
            }
            let pixels = image.pixels();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [runtime.dimensions.0 as usize, runtime.dimensions.1 as usize],
                pixels.as_raw(),
            );
            let texture = ctx.load_texture(
                format!(
                    "screenshot-pin-{}-{}",
                    runtime.pin_id.get(),
                    runtime.lifetime
                ),
                color_image,
                egui::TextureOptions::NEAREST,
            );
            let mut state = runtime
                .state
                .lock()
                .map_err(|_| "pin texture state lock was poisoned".to_owned())?;
            if let Some(existing) = state.texture.as_ref() {
                Ok(Some(existing.clone()))
            } else {
                state.texture = Some(texture.clone());
                Ok(Some(texture))
            }
        }
        PinWindowImageSource::Live(preparation) => {
            let Some(frame) = preparation.frame() else {
                return Ok(None);
            };
            let texture = {
                let mut state = runtime
                    .state
                    .lock()
                    .map_err(|_| "pin texture state lock was poisoned".to_owned())?;
                if state.texture_sequence == Some(frame.sequence) {
                    return Ok(state.texture.clone());
                }
                if let Some(texture) = state.texture.as_mut() {
                    texture.set(
                        egui::ImageData::Color(Arc::clone(&frame.color_image)),
                        egui::TextureOptions::NEAREST,
                    );
                    state.texture_sequence = Some(frame.sequence);
                    return Ok(state.texture.clone());
                }
                None
            };
            if let Some(texture) = texture {
                return Ok(Some(texture));
            }
            let texture = ctx.load_texture(
                format!(
                    "screenshot-pin-{}-{}",
                    runtime.pin_id.get(),
                    runtime.lifetime
                ),
                egui::ImageData::Color(Arc::clone(&frame.color_image)),
                egui::TextureOptions::NEAREST,
            );
            let mut state = runtime
                .state
                .lock()
                .map_err(|_| "pin texture state lock was poisoned".to_owned())?;
            if let Some(existing) = state.texture.as_ref() {
                Ok(Some(existing.clone()))
            } else {
                state.texture_sequence = Some(frame.sequence);
                state.texture = Some(texture.clone());
                Ok(Some(texture))
            }
        }
    }
}

fn pin_image_display_size(
    dimensions: (u32, u32),
    mode: PinScaleMode,
    zoom: f32,
    available_points: egui::Vec2,
    child_pixels_per_point: f32,
) -> egui::Vec2 {
    let source = egui::vec2(dimensions.0 as f32, dimensions.1 as f32);
    let ppp = child_pixels_per_point.max(f32::EPSILON);
    let scale = match mode {
        PinScaleMode::Fit => (available_points.x / source.x.max(1.0))
            .min(available_points.y / source.y.max(1.0))
            .max(f32::EPSILON),
        PinScaleMode::Actual => 1.0 / ppp,
        PinScaleMode::Custom => zoom.clamp(0.1, 8.0) / ppp,
    };
    source * scale
}

fn submit_pin_action(
    runtime: &PinWindowRuntime,
    client: &PinActionClient,
    request_id: u64,
    kind: PinActionKind,
) {
    let job = match build_pin_action_job(runtime, request_id, kind) {
        Ok(job) => job,
        Err(error) => {
            set_action_failure(runtime, request_id, error);
            return;
        }
    };
    match client.submit(job) {
        Ok(()) => {}
        Err(error) => set_action_failure(runtime, request_id, error),
    }
}

fn build_pin_action_job(
    runtime: &PinWindowRuntime,
    request_id: u64,
    kind: PinActionKind,
) -> Result<PinActionJob, String> {
    let image = runtime
        .image_source
        .current_image_snapshot()
        .ok_or_else(|| "No completed live frame is available yet".to_owned())?;
    Ok(PinActionJob {
        pin_id: runtime.pin_id,
        lifetime: runtime.lifetime,
        request_id,
        image,
        kind,
    })
}

fn set_action_failure(runtime: &PinWindowRuntime, request_id: u64, error: String) {
    if let Ok(mut state) = runtime.state.lock()
        && state.pending_request_id == Some(request_id)
    {
        state.pending_request_id = None;
        state.action_status = PinActionStatus::Failed(error);
    }
}

fn normalize_png_path(path: PathBuf) -> Result<PathBuf, String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        None => Ok(path.with_extension("png")),
        Some(extension) if extension.eq_ignore_ascii_case("png") => Ok(path),
        Some(_) => Err("Screenshot pins can be saved as PNG files.".into()),
    }
}

enum PinActionKind {
    Copy,
    SaveAs,
}

struct PinActionJob {
    pin_id: PinId,
    lifetime: u64,
    request_id: u64,
    image: PinImageHandle,
    kind: PinActionKind,
}

struct PinActionCompletion {
    pin_id: PinId,
    lifetime: u64,
    request_id: u64,
    result: Result<Option<String>, String>,
}

struct PinActionQueueState {
    queue: VecDeque<PinActionJob>,
    admitted_jobs: usize,
    active: bool,
    active_job: Option<(PinId, u64, u64)>,
    active_picker: Option<PinPickerCancellation>,
    shutdown: bool,
}

struct PinActionQueue {
    state: Mutex<PinActionQueueState>,
    wake: Condvar,
    completions: SyncSender<PinActionCompletion>,
    worker: Mutex<Option<JoinHandle<()>>>,
    save_picker: Arc<dyn PinSavePicker>,
    clipboard: Arc<dyn PinImageClipboard>,
}

struct PinActionWorker {
    shared: Arc<PinActionQueue>,
    completions: Receiver<PinActionCompletion>,
}

#[derive(Clone)]
struct PinActionClient {
    shared: Arc<PinActionQueue>,
}

trait PinSavePicker: Send + Sync {
    fn save_file(
        &self,
        pin_id: PinId,
        cancellation: &PinPickerCancellation,
    ) -> Result<Option<PathBuf>, String>;
}

trait PinImageClipboard: Send + Sync {
    fn copy_image(&self, image: &image::RgbaImage) -> Result<(), String>;
}

#[derive(Clone, Default)]
struct PinPickerCancellation {
    state: Arc<PinPickerCancellationState>,
}

#[derive(Default)]
struct PinPickerCancellationState {
    cancelled: AtomicBool,
    native_owner: Mutex<Option<usize>>,
    #[cfg(test)]
    cancel_calls: AtomicUsize,
    #[cfg(test)]
    wait_lock: Mutex<()>,
    #[cfg(test)]
    wait: Condvar,
}

impl PinPickerCancellation {
    fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    fn try_admit_dialog(&self) -> bool {
        !self.is_cancelled()
    }

    fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        #[cfg(test)]
        self.state.cancel_calls.fetch_add(1, Ordering::AcqRel);
        if let Ok(owner) = self.state.native_owner.lock()
            && let Some(owner) = *owner
        {
            post_pin_save_cancel(owner);
        }
        #[cfg(test)]
        self.state.wait.notify_all();
    }

    #[cfg(windows)]
    fn register_native_owner(&self, owner: usize) -> bool {
        let Ok(mut current) = self.state.native_owner.lock() else {
            return false;
        };
        if !self.try_admit_dialog() {
            return false;
        }
        *current = Some(owner);
        if self.is_cancelled() {
            post_pin_save_cancel(owner);
        }
        true
    }

    #[cfg(windows)]
    fn clear_native_owner(&self, owner: usize) {
        if let Ok(mut current) = self.state.native_owner.lock()
            && *current == Some(owner)
        {
            *current = None;
        }
    }

    #[cfg(test)]
    fn wait_for_cancel_calls(&self, count: usize, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let Ok(mut guard) = self.state.wait_lock.lock() else {
            return false;
        };
        while self.state.cancel_calls.load(Ordering::Acquire) < count {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let Ok((next, result)) = self.state.wait.wait_timeout(guard, remaining) else {
                return false;
            };
            guard = next;
            if result.timed_out() && self.state.cancel_calls.load(Ordering::Acquire) < count {
                return false;
            }
        }
        true
    }

    #[cfg(test)]
    fn cancel_count(&self) -> usize {
        self.state.cancel_calls.load(Ordering::Acquire)
    }
}

impl PinActionWorker {
    fn with_save_picker(save_picker: Arc<dyn PinSavePicker>) -> Self {
        Self::with_action_targets(save_picker, Arc::new(SystemPinImageClipboard))
    }

    fn with_action_targets(
        save_picker: Arc<dyn PinSavePicker>,
        clipboard: Arc<dyn PinImageClipboard>,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(MAX_PIN_ACTIONS);
        Self {
            shared: Arc::new(PinActionQueue {
                state: Mutex::new(PinActionQueueState {
                    queue: VecDeque::new(),
                    admitted_jobs: 0,
                    active: false,
                    active_job: None,
                    active_picker: None,
                    shutdown: false,
                }),
                wake: Condvar::new(),
                completions: sender,
                worker: Mutex::new(None),
                save_picker,
                clipboard,
            }),
            completions: receiver,
        }
    }

    fn client(&self) -> PinActionClient {
        PinActionClient {
            shared: Arc::clone(&self.shared),
        }
    }

    fn drain_completions(&self) -> Vec<PinActionCompletion> {
        let mut completions = Vec::new();
        while let Ok(completion) = self.completions.try_recv() {
            completions.push(completion);
        }
        if !completions.is_empty()
            && let Ok(mut state) = self.shared.state.lock()
        {
            state.admitted_jobs -= completions.len();
        }
        completions
    }

    fn cancel_unstarted(&self, pin_id: PinId) {
        if let Ok(mut state) = self.shared.state.lock() {
            let queued_before = state.queue.len();
            state.queue.retain(|job| job.pin_id != pin_id);
            state.admitted_jobs -= queued_before - state.queue.len();
        }
    }

    fn has_pending_work(&self) -> bool {
        // Admitted jobs include completions that the UI has not drained yet.
        // Keep the owner repainting until it consumes those receipts.
        self.shared
            .state
            .lock()
            .is_ok_and(|state| state.admitted_jobs > 0)
    }

    fn shutdown_and_join(&mut self) -> Result<(), String> {
        let active_picker = {
            let mut state = self
                .shared
                .state
                .lock()
                .map_err(|_| "pin action queue lock was poisoned".to_owned())?;
            state.shutdown = true;
            let queued_jobs = state.queue.len();
            state.admitted_jobs -= queued_jobs;
            state.queue.clear();
            let active_picker = state.active_picker.clone();
            self.shared.wake.notify_all();
            active_picker
        };
        if let Some(cancellation) = active_picker {
            cancellation.cancel();
        }
        let worker = self
            .shared
            .worker
            .lock()
            .map_err(|_| "pin action worker lock was poisoned".to_owned())?
            .take();
        if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| "pin image action worker terminated unexpectedly".to_owned())?;
        }
        Ok(())
    }
}

impl Drop for PinActionWorker {
    fn drop(&mut self) {
        let _ = self.shutdown_and_join();
    }
}

impl PinActionClient {
    fn submit(&self, job: PinActionJob) -> Result<(), String> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| "pin action queue lock was poisoned".to_owned())?;
        if state.shutdown {
            return Err("pin image actions are shutting down".into());
        }
        if state.admitted_jobs >= MAX_PIN_ACTIONS {
            return Err("pin image action queue is full".into());
        }
        let mut worker_slot = self
            .shared
            .worker
            .lock()
            .map_err(|_| "pin action worker lock was poisoned".to_owned())?;
        if worker_slot.is_none() {
            let shared = Arc::clone(&self.shared);
            let worker = thread::Builder::new()
                .name("pin-image-actions".into())
                .spawn(move || pin_action_worker(shared))
                .map_err(|error| format!("could not start pin image action worker: {error}"))?;
            *worker_slot = Some(worker);
        }
        drop(worker_slot);
        state.admitted_jobs += 1;
        state.queue.push_back(job);
        self.shared.wake.notify_one();
        Ok(())
    }
}

fn pin_action_worker(shared: Arc<PinActionQueue>) {
    loop {
        let (job, cancellation) = {
            let Ok(state) = shared.state.lock() else {
                return;
            };
            let mut state = match shared
                .wake
                .wait_while(state, |state| state.queue.is_empty() && !state.shutdown)
            {
                Ok(state) => state,
                Err(_) => return,
            };
            if state.shutdown && state.queue.is_empty() {
                return;
            }
            let Some(job) = state.queue.pop_front() else {
                continue;
            };
            state.active = true;
            let cancellation =
                matches!(&job.kind, PinActionKind::SaveAs).then(PinPickerCancellation::default);
            state.active_job = Some((job.pin_id, job.lifetime, job.request_id));
            state.active_picker = cancellation.clone();
            (job, cancellation)
        };
        let cancellation_for_job = cancellation.clone().unwrap_or_default();
        let save_picker = Arc::clone(&shared.save_picker);
        let clipboard = Arc::clone(&shared.clipboard);
        let result = run_pin_action_with_targets(&job, clipboard.as_ref(), || {
            save_picker.save_file(job.pin_id, &cancellation_for_job)
        });
        let completion = PinActionCompletion {
            pin_id: job.pin_id,
            lifetime: job.lifetime,
            request_id: job.request_id,
            result,
        };
        if shared.completions.send(completion).is_err() {
            return;
        }
        if let Ok(mut state) = shared.state.lock() {
            state.active = false;
            state.active_job = None;
            state.active_picker = None;
            shared.wake.notify_all();
        }
    }
}

struct NativePinSavePicker;

struct SystemPinImageClipboard;

impl PinImageClipboard for SystemPinImageClipboard {
    fn copy_image(&self, image: &image::RgbaImage) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new()
            .map_err(|error| format!("could not open the image clipboard: {error}"))?;
        clipboard
            .set_image(clipboard_image_data(image))
            .map_err(|error| format!("could not copy screenshot pin: {error}"))
    }
}

fn post_pin_save_cancel(owner: usize) {
    #[cfg(windows)]
    native::post_pin_save_cancel(owner);
    #[cfg(not(windows))]
    let _ = owner;
}

impl PinSavePicker for NativePinSavePicker {
    fn save_file(
        &self,
        pin_id: PinId,
        cancellation: &PinPickerCancellation,
    ) -> Result<Option<PathBuf>, String> {
        if cancellation.is_cancelled() {
            return Ok(None);
        }
        #[cfg(windows)]
        {
            native::save_pin_image(pin_id, cancellation)
        }
        #[cfg(not(windows))]
        {
            let _ = pin_id;
            Err("Save As picker is unsupported on this platform".into())
        }
    }
}

fn run_pin_action_with_targets(
    job: &PinActionJob,
    clipboard: &dyn PinImageClipboard,
    save_picker: impl FnOnce() -> Result<Option<PathBuf>, String>,
) -> Result<Option<String>, String> {
    match &job.kind {
        PinActionKind::Copy => {
            let image = job.image.pixels();
            clipboard.copy_image(image)?;
            Ok(Some("Copied pin image to the clipboard.".into()))
        }
        PinActionKind::SaveAs => {
            let Some(path) = save_picker()? else {
                return Ok(None);
            };
            let path = normalize_png_path(path)?;
            let image = job.image.pixels();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("could not create save folder: {error}"))?;
            }
            let file = std::fs::File::create(&path)
                .map_err(|error| format!("could not create {}: {error}", path.display()))?;
            image::codecs::png::PngEncoder::new(file)
                .write_image(
                    image.as_raw(),
                    image.width(),
                    image.height(),
                    image::ColorType::Rgba8,
                )
                .map_err(|error| format!("could not save {}: {error}", path.display()))?;
            Ok(Some(format!("Saved pin image to {}.", path.display())))
        }
    }
}

fn clipboard_image_data(image: &image::RgbaImage) -> arboard::ImageData<'static> {
    arboard::ImageData {
        width: image.width() as usize,
        height: image.height() as usize,
        bytes: Cow::Owned(image.as_raw().to_vec()),
    }
}

fn native_window_for_pin(
    title: &str,
    lifetime: u64,
    existing: Option<&NativeWindowBinding>,
    root_hwnd: usize,
) -> NativeLookup {
    #[cfg(windows)]
    {
        native::lookup(title, lifetime, existing, root_hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = (title, lifetime, existing, root_hwnd);
        NativeLookup::Unsupported
    }
}

fn native_window_visible(binding: &NativeWindowBinding) -> Result<bool, String> {
    #[cfg(windows)]
    {
        native::is_visible(binding)
    }
    #[cfg(not(windows))]
    {
        let _ = binding;
        Err("pin visibility acknowledgement is unavailable on this platform".into())
    }
}

fn native_window_bounds(binding: &NativeWindowBinding) -> Option<ScreenRect> {
    #[cfg(windows)]
    {
        native::bounds(binding).ok()
    }
    #[cfg(not(windows))]
    {
        let _ = binding;
        None
    }
}

enum NativeLookup {
    Found(NativeWindowBinding),
    Missing,
    Destroyed,
    Unsupported,
    Failed(String),
}

fn lookup_owned_pin_window(
    native_api: &dyn PinNativeApi,
    runtime: &PinWindowRuntime,
    existing: Option<&NativeWindowBinding>,
    root_hwnd: usize,
) -> NativeLookup {
    if root_hwnd == 0 || runtime.viewport_id == egui::ViewportId::ROOT {
        return NativeLookup::Failed("pin window has no valid non-root native owner".into());
    }
    if runtime.viewport_id != pin_viewport_id(runtime.pin_id, runtime.lifetime)
        || runtime.process_id != std::process::id()
    {
        return NativeLookup::Failed("pin runtime identity is stale or foreign".into());
    }
    if existing.is_some_and(|binding| {
        binding.hwnd == 0
            || binding.hwnd == root_hwnd
            || binding.process_id != runtime.process_id
            || binding.title != runtime.title
            || binding.lifetime != runtime.lifetime
            || binding.root_hwnd != root_hwnd
    }) {
        return NativeLookup::Failed("stored native pin binding is stale or foreign".into());
    }
    match native_api.lookup(&runtime.title, runtime.lifetime, existing, root_hwnd) {
        NativeLookup::Found(binding)
            if binding.hwnd != 0
                && binding.hwnd != root_hwnd
                && binding.process_id == runtime.process_id
                && binding.process_id == std::process::id()
                && binding.title == runtime.title
                && binding.lifetime == runtime.lifetime
                && binding.root_hwnd == root_hwnd
                && runtime.viewport_id == pin_viewport_id(runtime.pin_id, runtime.lifetime) =>
        {
            NativeLookup::Found(binding)
        }
        NativeLookup::Found(_) => NativeLookup::Failed(
            "native lookup returned a foreign, reused, or root window for this pin".into(),
        ),
        other => other,
    }
}

#[cfg(windows)]
mod native {
    use super::{NativeLookup, NativeWindowBinding, PinPickerCancellation, ScreenRect};
    use std::path::PathBuf;
    use windows::Win32::Foundation::{
        BOOL, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, RECT,
        WPARAM,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FileSaveDialog,
        IFileSaveDialog, SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, EnumWindows,
        GWLP_USERDATA, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, RegisterClassW,
        SetWindowLongPtrW, WM_APP, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };
    use windows::core::{HRESULT, PCWSTR, w};

    const PIN_SAVE_CANCEL_MESSAGE: u32 = WM_APP + 0x5A;
    const PIN_SAVE_OWNER_CLASS: windows::core::PCWSTR = w!("MultiLauncherPinSaveDialogOwner");
    const HRESULT_CANCELLED: HRESULT = HRESULT(0x8007_04C7_u32 as i32);

    struct PinSaveDialogOwnerState {
        dialog: IFileSaveDialog,
    }

    unsafe extern "system" fn pin_save_owner_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == PIN_SAVE_CANCEL_MESSAGE {
            let state =
                unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut PinSaveDialogOwnerState;
            if !state.is_null() {
                // The owner window and dialog are created, used, and destroyed on
                // the same STA thread. Cancellation is delivered to this WndProc
                // by its active COM dialog message loop.
                let _ = unsafe { (*state).dialog.Close(HRESULT_CANCELLED) };
                return LRESULT(0);
            }
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn register_pin_save_owner_class(instance: HINSTANCE) -> Result<(), String> {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pin_save_owner_proc),
            hInstance: instance,
            lpszClassName: PIN_SAVE_OWNER_CLASS,
            ..Default::default()
        };
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_CLASS_ALREADY_EXISTS {
                return Err(format!(
                    "could not register pin save dialog owner: {error:?}"
                ));
            }
        }
        Ok(())
    }

    pub(super) fn save_pin_image(
        pin_id: crate::pinboard::PinId,
        cancellation: &PinPickerCancellation,
    ) -> Result<Option<PathBuf>, String> {
        if cancellation.is_cancelled() {
            return Ok(None);
        }
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("could not initialize the pin save dialog STA: {error}"))?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _apartment = Apartment;
        let dialog: IFileSaveDialog =
            unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| format!("could not create the pin save dialog: {error}"))?;
        let options = unsafe { dialog.GetOptions() }
            .map_err(|error| format!("could not configure the pin save dialog: {error}"))?;
        unsafe {
            dialog
                .SetOptions(options | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT | FOS_PATHMUSTEXIST)
        }
        .map_err(|error| format!("could not configure the pin save dialog: {error}"))?;
        let filter_name: Vec<u16> = "PNG image".encode_utf16().chain([0]).collect();
        let filter_spec: Vec<u16> = "*.png".encode_utf16().chain([0]).collect();
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(filter_name.as_ptr()),
            pszSpec: PCWSTR(filter_spec.as_ptr()),
        }];
        unsafe { dialog.SetFileTypes(&filters) }
            .map_err(|error| format!("could not configure the pin save dialog: {error}"))?;
        let name = format!("screenshot-pin-{}.png", pin_id.get());
        let filename: Vec<u16> = name.encode_utf16().chain([0]).collect();
        unsafe { dialog.SetFileName(PCWSTR(filename.as_ptr())) }
            .map_err(|error| format!("could not configure the pin save dialog: {error}"))?;
        unsafe { dialog.SetDefaultExtension(w!("png")) }
            .map_err(|error| format!("could not configure the pin save dialog: {error}"))?;

        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("could not locate the launcher module: {error}"))?;
        let instance = HINSTANCE(module.0);
        register_pin_save_owner_class(instance)?;
        let mut owner_state = Box::new(PinSaveDialogOwnerState { dialog });
        let owner = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PIN_SAVE_OWNER_CLASS,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                instance,
                None,
            )
        }
        .map_err(|error| format!("could not create pin save cancellation owner: {error}"))?;
        unsafe {
            SetWindowLongPtrW(
                owner,
                GWLP_USERDATA,
                (&mut *owner_state as *mut PinSaveDialogOwnerState) as isize,
            );
        }
        let owner_value = owner.0 as usize;
        if !cancellation.register_native_owner(owner_value) || cancellation.is_cancelled() {
            cancellation.clear_native_owner(owner_value);
            unsafe { DestroyWindow(owner) }
                .map_err(|error| format!("could not close pin save cancellation owner: {error}"))?;
            return Ok(None);
        }

        let shown = unsafe { owner_state.dialog.Show(owner) };
        cancellation.clear_native_owner(owner_value);
        unsafe { DestroyWindow(owner) }
            .map_err(|error| format!("could not close pin save cancellation owner: {error}"))?;
        if cancellation.is_cancelled() {
            return Ok(None);
        }
        if let Err(error) = shown {
            if error.code() == HRESULT_CANCELLED {
                return Ok(None);
            }
            return Err(format!("pin save dialog failed: {error}"));
        }
        let item = unsafe { owner_state.dialog.GetResult() }
            .map_err(|error| format!("could not read the selected pin image path: {error}"))?;
        let display_name = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }
            .map_err(|error| format!("could not read the selected pin image path: {error}"))?;
        let mut length = 0;
        while unsafe { *display_name.0.add(length) } != 0 {
            length += 1;
        }
        let path =
            unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(display_name.0, length)) };
        unsafe { CoTaskMemFree(Some(display_name.0.cast())) };
        Ok(Some(PathBuf::from(path)))
    }

    pub(super) fn post_pin_save_cancel(owner: usize) {
        let _ = unsafe {
            PostMessageW(
                HWND(owner as *mut _),
                PIN_SAVE_CANCEL_MESSAGE,
                WPARAM(0),
                LPARAM(0),
            )
        };
    }

    struct Search<'a> {
        title: &'a [u16],
        process_id: u32,
        root_hwnd: usize,
        matches: Vec<usize>,
    }

    unsafe extern "system" fn find_exact_window(hwnd: HWND, parameter: LPARAM) -> BOOL {
        let search = unsafe { &mut *(parameter.0 as *mut Search<'_>) };
        let hwnd_value = hwnd.0 as usize;
        if hwnd_value == search.root_hwnd || !unsafe { IsWindow(hwnd) }.as_bool() {
            return BOOL(1);
        }
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != search.process_id {
            return BOOL(1);
        }
        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 {
            return BOOL(1);
        }
        let mut title = vec![0_u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut title) };
        if copied > 0 && title[..copied as usize] == search.title[..] {
            search.matches.push(hwnd_value);
        }
        BOOL(1)
    }

    pub(super) fn lookup(
        title: &str,
        lifetime: u64,
        existing: Option<&NativeWindowBinding>,
        root_hwnd: usize,
    ) -> NativeLookup {
        if let Some(existing) = existing {
            if existing.title != title || existing.lifetime != lifetime {
                return NativeLookup::Failed(
                    "a registered pin HWND was presented for another pin lifetime".into(),
                );
            }
            let hwnd = HWND(existing.hwnd as *mut _);
            if unsafe { !IsWindow(hwnd).as_bool() } {
                return NativeLookup::Destroyed;
            }
            match validate(
                hwnd,
                existing.process_id,
                &existing.title,
                lifetime,
                root_hwnd,
            ) {
                Ok(true) => {
                    let mut binding = existing.clone();
                    binding.root_hwnd = root_hwnd;
                    return NativeLookup::Found(binding);
                }
                Ok(false) => {
                    return NativeLookup::Failed(
                        "a registered pin HWND changed process, title, or lifetime".into(),
                    );
                }
                Err(error) => return NativeLookup::Failed(error),
            }
        }
        let mut title_wide = title.encode_utf16().collect::<Vec<_>>();
        title_wide.push(0);
        let mut search = Search {
            title: &title_wide[..title_wide.len() - 1],
            process_id: std::process::id(),
            root_hwnd,
            matches: Vec::new(),
        };
        let result = unsafe {
            EnumWindows(
                Some(find_exact_window),
                LPARAM((&mut search as *mut Search<'_>) as isize),
            )
        };
        if let Err(error) = result {
            return NativeLookup::Failed(format!("could not enumerate pin windows: {error}"));
        }
        match search.matches.as_slice() {
            [] => NativeLookup::Missing,
            [hwnd] => NativeLookup::Found(NativeWindowBinding {
                hwnd: *hwnd,
                process_id: search.process_id,
                title: title.to_owned(),
                lifetime,
                root_hwnd,
            }),
            _ => NativeLookup::Failed("more than one native window matched a pin identity".into()),
        }
    }

    fn validate(
        hwnd: HWND,
        expected_process_id: u32,
        expected_title: &str,
        lifetime: u64,
        root_hwnd: usize,
    ) -> Result<bool, String> {
        if hwnd.0 as usize == 0 || hwnd.0 as usize == root_hwnd {
            return Ok(false);
        }
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if process_id != expected_process_id || process_id != std::process::id() {
            return Ok(false);
        }
        if lifetime == 0 {
            return Ok(false);
        }
        let title_len = unsafe { GetWindowTextLengthW(hwnd) };
        if title_len <= 0 {
            return Ok(false);
        }
        let mut title = vec![0_u16; title_len as usize + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut title) };
        if copied <= 0 {
            return Ok(false);
        }
        Ok(String::from_utf16_lossy(&title[..copied as usize]) == expected_title)
    }

    pub(super) fn is_visible(binding: &NativeWindowBinding) -> Result<bool, String> {
        let hwnd = HWND(binding.hwnd as *mut _);
        if unsafe { !IsWindow(hwnd).as_bool() } {
            return Err("registered pin HWND was destroyed".into());
        }
        if !validate(
            hwnd,
            binding.process_id,
            &binding.title,
            binding.lifetime,
            binding.root_hwnd,
        )? {
            return Err("registered pin HWND no longer belongs to its lifetime".into());
        }
        Ok(unsafe { IsWindowVisible(hwnd) }.as_bool())
    }

    pub(super) fn bounds(binding: &NativeWindowBinding) -> Result<ScreenRect, String> {
        let hwnd = HWND(binding.hwnd as *mut _);
        if unsafe { !IsWindow(hwnd).as_bool() } {
            return Err("registered pin HWND was destroyed".into());
        }
        if !validate(
            hwnd,
            binding.process_id,
            &binding.title,
            binding.lifetime,
            binding.root_hwnd,
        )? {
            return Err("registered pin HWND no longer belongs to its lifetime".into());
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }
            .map_err(|error| format!("could not read pin window bounds: {error}"))?;
        let width = i64::from(rect.right) - i64::from(rect.left);
        let height = i64::from(rect.bottom) - i64::from(rect.top);
        let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
            return Err("native pin window bounds are invalid".into());
        };
        if width == 0 || height == 0 {
            return Err("native pin window bounds are empty".into());
        }
        Ok(ScreenRect::new(rect.left, rect.top, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinboard::live::{PinLiveCaptureBackend, test_support::FakePinLiveCaptureBackend};
    use crate::pinboard::source::{PinSourceBinding, PinSourceCrop, PinSourceWindowIdentity};
    use crate::pinboard::{PinCaptureGeneration, PinCaptureOperationId, PinboardLimits};
    use image::{Rgba, RgbaImage};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    const ROOT_HWND: usize = 0x100;

    #[derive(Clone)]
    struct FakeNativeWindow {
        binding: NativeWindowBinding,
        bounds: ScreenRect,
        visible: bool,
        alive: bool,
    }

    #[derive(Default)]
    struct FakePinNativeApi {
        windows: Mutex<BTreeMap<usize, FakeNativeWindow>>,
    }

    struct FixturePinSavePicker {
        path: PathBuf,
        started: mpsc::Sender<usize>,
        calls: AtomicUsize,
    }

    impl PinSavePicker for FixturePinSavePicker {
        fn save_file(
            &self,
            _pin_id: PinId,
            cancellation: &PinPickerCancellation,
        ) -> Result<Option<PathBuf>, String> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel);
            let _ = self.started.send(call);
            match call {
                0 => Ok(Some(self.path.clone())),
                1 => Ok(None),
                _ => {
                    if cancellation.wait_for_cancel_calls(2, std::time::Duration::from_secs(5)) {
                        Ok(None)
                    } else {
                        Err("fixture picker timed out awaiting cancellation".into())
                    }
                }
            }
        }
    }

    struct ShutdownBlockedPinSavePicker {
        started: mpsc::Sender<usize>,
        close_seen: mpsc::Sender<()>,
    }

    impl PinSavePicker for ShutdownBlockedPinSavePicker {
        fn save_file(
            &self,
            _pin_id: PinId,
            cancellation: &PinPickerCancellation,
        ) -> Result<Option<PathBuf>, String> {
            let _ = self.started.send(0);
            if cancellation.wait_for_cancel_calls(1, std::time::Duration::from_secs(5)) {
                let _ = self.close_seen.send(());
                Ok(None)
            } else {
                Err("fixture picker timed out awaiting shutdown cancellation".into())
            }
        }
    }

    struct ImmediateCancelPinSavePicker;

    impl PinSavePicker for ImmediateCancelPinSavePicker {
        fn save_file(
            &self,
            _pin_id: PinId,
            _cancellation: &PinPickerCancellation,
        ) -> Result<Option<PathBuf>, String> {
            Ok(None)
        }
    }

    impl FakePinNativeApi {
        fn contains(&self, hwnd: usize) -> bool {
            self.windows.lock().unwrap().contains_key(&hwnd)
        }

        fn add_window(
            &self,
            runtime: &PinWindowRuntime,
            hwnd: usize,
            visible: bool,
            bounds: ScreenRect,
        ) {
            self.windows.lock().unwrap().insert(
                hwnd,
                FakeNativeWindow {
                    binding: NativeWindowBinding {
                        hwnd,
                        process_id: std::process::id(),
                        title: runtime.title.clone(),
                        lifetime: runtime.lifetime,
                        root_hwnd: ROOT_HWND,
                    },
                    bounds,
                    visible,
                    alive: true,
                },
            );
        }

        fn set_visible(&self, hwnd: usize, visible: bool) {
            self.windows.lock().unwrap().get_mut(&hwnd).unwrap().visible = visible;
        }

        fn destroy(&self, hwnd: usize) {
            self.windows.lock().unwrap().get_mut(&hwnd).unwrap().alive = false;
        }

        fn is_visible_for_test(&self, hwnd: usize) -> bool {
            self.windows.lock().unwrap().get(&hwnd).unwrap().visible
        }
    }

    impl PinNativeApi for FakePinNativeApi {
        fn lookup(
            &self,
            title: &str,
            lifetime: u64,
            existing: Option<&NativeWindowBinding>,
            root_hwnd: usize,
        ) -> NativeLookup {
            if let Some(existing) = existing {
                if existing.hwnd == root_hwnd
                    || existing.title != title
                    || existing.lifetime != lifetime
                    || existing.process_id != std::process::id()
                {
                    return NativeLookup::Failed("fake native identity mismatch".into());
                }
                return match self.windows.lock().unwrap().get(&existing.hwnd) {
                    Some(window) if !window.alive => NativeLookup::Destroyed,
                    Some(window)
                        if window.binding.title == title
                            && window.binding.lifetime == lifetime
                            && window.binding.process_id == std::process::id() =>
                    {
                        let mut binding = window.binding.clone();
                        binding.root_hwnd = root_hwnd;
                        NativeLookup::Found(binding)
                    }
                    Some(_) => NativeLookup::Failed("fake native window was reused".into()),
                    None => NativeLookup::Destroyed,
                };
            }

            let windows = self.windows.lock().unwrap();
            let mut matches = windows.values().filter(|window| {
                window.alive
                    && window.binding.hwnd != root_hwnd
                    && window.binding.process_id == std::process::id()
                    && window.binding.title == title
                    && window.binding.lifetime == lifetime
            });
            match (matches.next(), matches.next()) {
                (Some(window), None) => {
                    let mut binding = window.binding.clone();
                    binding.root_hwnd = root_hwnd;
                    NativeLookup::Found(binding)
                }
                (None, _) => NativeLookup::Missing,
                _ => NativeLookup::Failed("fake pin identity was ambiguous".into()),
            }
        }

        fn is_visible(&self, binding: &NativeWindowBinding) -> Result<bool, String> {
            if binding.hwnd == binding.root_hwnd {
                return Err("fake native lookup selected ROOT".into());
            }
            let windows = self.windows.lock().map_err(|_| "fake lock poisoned")?;
            let window = windows
                .get(&binding.hwnd)
                .ok_or_else(|| "fake pin window is missing".to_owned())?;
            if !window.alive
                || window.binding.title != binding.title
                || window.binding.lifetime != binding.lifetime
                || window.binding.process_id != binding.process_id
            {
                return Err("fake pin window lifetime changed".into());
            }
            Ok(window.visible)
        }

        fn bounds(&self, binding: &NativeWindowBinding) -> Option<ScreenRect> {
            self.windows
                .lock()
                .ok()?
                .get(&binding.hwnd)
                .filter(|window| {
                    window.alive
                        && window.binding.title == binding.title
                        && window.binding.lifetime == binding.lifetime
                })
                .map(|window| window.bounds)
        }
    }

    fn fixture_directory() -> tempfile::TempDir {
        let parent = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("productivity_expansion");
        std::fs::create_dir_all(&parent).unwrap();
        tempfile::Builder::new()
            .prefix("pinboard-")
            .tempdir_in(parent)
            .unwrap()
    }

    fn add_pin(board: &mut Pinboard, index: u64, rect: ScreenRect) -> PinId {
        let identity = board
            .reserve_capture(PinCaptureOperationId::new(index).unwrap())
            .unwrap();
        board.reserve_selected_rect(identity, rect).unwrap();
        let color = Rgba([index as u8, (index + 30) as u8, (index + 70) as u8, 255]);
        board
            .stage_capture(
                identity,
                rect,
                RgbaImage::from_pixel(rect.width, rect.height, color),
            )
            .unwrap();
        board.commit_capture(identity).unwrap()
    }

    fn add_live_pin(
        board: &mut Pinboard,
        index: u64,
    ) -> (
        PinId,
        PinCaptureGeneration,
        mpsc::Sender<crate::pinboard::live::test_support::FakeFrame>,
    ) {
        let identity = board
            .reserve_live_capture(PinCaptureOperationId::new(index).unwrap())
            .unwrap();
        let rect = ScreenRect::new(0, 0, 2, 2);
        board.reserve_selected_rect(identity, rect).unwrap();
        let binding = PinSourceBinding {
            window: PinSourceWindowIdentity {
                hwnd: 51,
                process_id: 52,
                thread_id: 53,
            },
            captured_bounds: ScreenRect::new(0, 0, 2, 2),
            crop: PinSourceCrop {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
        };
        let (frame_tx, frame_rx) = mpsc::channel();
        let backend = FakePinLiveCaptureBackend {
            first_image: RgbaImage::from_pixel(2, 2, Rgba([11, 12, 13, 255])),
            first_content_size: (2, 2),
            stream: Mutex::new(Some(frame_rx)),
            first_frame_gate: None,
        };
        let mut session = backend
            .start(
                binding,
                identity.generation,
                board.take_live_reservation(identity).unwrap(),
                board.gpu_budget(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = session.try_first_frame_result() {
                result.unwrap();
                break;
            }
            assert!(Instant::now() < deadline, "fake live first frame timed out");
            thread::yield_now();
        }
        board
            .stage_live_capture(identity, binding, session)
            .unwrap();
        let pin_id = board.commit_capture(identity).unwrap();
        (pin_id, identity.generation, frame_tx)
    }

    fn child_input(viewport_id: egui::ViewportId, pixels_per_point: f32) -> egui::RawInput {
        let mut input = egui::RawInput {
            viewport_id,
            ..Default::default()
        };
        input.viewports.insert(
            viewport_id,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                native_pixels_per_point: Some(pixels_per_point),
                ..Default::default()
            },
        );
        input
    }

    fn root_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events,
            ..Default::default()
        }
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    struct PinboardFixtureCaptureBackend {
        desktop: ScreenRect,
        captures: AtomicUsize,
    }

    impl crate::mkmacro::ScreenCaptureBackend for PinboardFixtureCaptureBackend {
        fn virtual_desktop(&self) -> crate::mkmacro::ExecResult<ScreenRect> {
            Ok(self.desktop)
        }

        fn region_bounds(
            &self,
            _region: &crate::mkmacro::SearchRegion,
        ) -> crate::mkmacro::ExecResult<ScreenRect> {
            Ok(self.desktop)
        }

        fn capture_rect(
            &self,
            rect: ScreenRect,
            cancelled: &dyn Fn() -> bool,
        ) -> crate::mkmacro::ExecResult<image::RgbaImage> {
            if cancelled() {
                return Err(crate::mkmacro::cancelled_error());
            }
            let call = self.captures.fetch_add(1, Ordering::AcqRel) as u8;
            Ok(image::RgbaImage::from_pixel(
                rect.width,
                rect.height,
                image::Rgba([call + 17, call + 29, call + 43, 255]),
            ))
        }
    }

    #[derive(Default)]
    struct RecordingPinClipboard {
        images: Mutex<Vec<image::RgbaImage>>,
    }

    impl PinImageClipboard for RecordingPinClipboard {
        fn copy_image(&self, image: &image::RgbaImage) -> Result<(), String> {
            self.images.lock().unwrap().push(image.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct PinboardFixtureParkingLog {
        begins: AtomicUsize,
        restores: AtomicUsize,
        reparks: AtomicUsize,
        commits: AtomicUsize,
    }

    struct PinboardFixtureParkingFactory(Arc<PinboardFixtureParkingLog>);

    struct PinboardFixtureParking(Arc<PinboardFixtureParkingLog>);

    impl crate::gui::pinboard::LauncherCaptureParkingFactory for PinboardFixtureParkingFactory {
        fn begin(
            &self,
            _generation: crate::pinboard::PinCaptureGeneration,
            _hwnd: usize,
            _virtual_desktop: ScreenRect,
        ) -> Result<Box<dyn crate::gui::pinboard::LauncherCaptureParking>, String> {
            self.0.begins.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(PinboardFixtureParking(Arc::clone(&self.0))))
        }
    }

    impl crate::gui::pinboard::LauncherCaptureParking for PinboardFixtureParking {
        fn verify_capture_safe(&mut self) -> Result<bool, String> {
            Ok(true)
        }

        fn restore(&mut self) -> Result<(), String> {
            self.0.restores.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }

        fn repark(&mut self) -> Result<(), String> {
            self.0.reparks.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }

        fn commit_hidden(&mut self) {
            self.0.commits.fetch_add(1, Ordering::AcqRel);
        }
    }

    fn dispatch_pinboard_command(app: &mut crate::gui::LauncherApp, action: &str) {
        let invocation = crate::commands::parse_command(
            crate::actions::Action {
                label: action.into(),
                desc: String::new(),
                action: action.into(),
                args: None,
            },
            None,
            crate::commands::ActivationSource::Enter,
        )
        .unwrap();
        app.dispatch_command_invocation(invocation);
    }

    fn reflect_child_visibility_commands(
        app: &crate::gui::LauncherApp,
        native: &FakePinNativeApi,
        output: &egui::FullOutput,
    ) {
        let presenter = app.pinboard_capture.presenter_for_test();
        let registry = presenter.registry.lock().unwrap();
        for runtime in registry.pins.values().chain(registry.retired.values()) {
            assert_ne!(runtime.viewport_id, egui::ViewportId::ROOT);
            let hwnd = 0x200 + runtime.pin_id.get() as usize;
            if !native.contains(hwnd) {
                continue;
            }
            let Some(viewport) = output.viewport_output.get(&runtime.viewport_id) else {
                continue;
            };
            for command in &viewport.commands {
                if let egui::ViewportCommand::Visible(visible) = command {
                    native.set_visible(hwnd, *visible);
                }
            }
        }
    }

    fn launcher_pinboard_frame(
        app: &mut crate::gui::LauncherApp,
        native: &FakePinNativeApi,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let context = app.egui_ctx.clone();
        let output = context.run_ui(root_input(events), |root| {
            app.poll_pinboard_capture(root.ctx());
            app.show_screenshot_pins(root.ctx());
            let open = app.pin_list_open;
            app.pin_list_open = app.show_screenshot_pin_list(root.ctx(), open);
        });
        reflect_child_visibility_commands(app, native, &output);
        output
    }

    fn request_competing_query_focus(ctx: &egui::Context) -> egui::Id {
        let query_id = egui::Id::new("pinboard-competing-query-widget");
        let mut query = String::from("another query");
        let output = ctx.run_ui(root_input(Vec::new()), |root| {
            egui::CentralPanel::default().show(root, |ui| {
                ui.add(egui::TextEdit::singleline(&mut query).id(query_id))
                    .request_focus();
            });
        });
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(query_id));
        output.drop_without_applying_deltas();
        query_id
    }

    fn register_fixture_native_children(app: &crate::gui::LauncherApp, native: &FakePinNativeApi) {
        let presenter = app.pinboard_capture.presenter_for_test();
        let registry = presenter.registry.lock().unwrap();
        for runtime in registry.pins.values() {
            let hwnd = 0x200 + runtime.pin_id.get() as usize;
            if !native.contains(hwnd) {
                native.add_window(
                    runtime,
                    hwnd,
                    true,
                    app.pinboard_capture
                        .pinboard()
                        .get(runtime.pin_id)
                        .unwrap()
                        .display_bounds(),
                );
            }
        }
    }

    fn pinboard_selector_operations(
        overlay: &crate::gui::mkmacro_dialog::visual_capture_workflow::TestOverlayServiceFixture,
    ) -> Vec<u64> {
        overlay
            .observer
            .commands
            .lock()
            .unwrap()
            .iter()
            .filter_map(|command| match command {
                crate::gui::mkmacro_dialog::visual_overlay::VisualOverlayCommand::BeginRectanglePick {
                    operation_id,
                    purpose: crate::gui::mkmacro_dialog::visual_overlay::RectanglePurpose::PinboardCapture,
                    ..
                } => Some(*operation_id),
                _ => None,
            })
            .collect()
    }

    fn capture_pin_through_launcher_command(
        app: &mut crate::gui::LauncherApp,
        native: &FakePinNativeApi,
        overlay: &crate::gui::mkmacro_dialog::visual_capture_workflow::TestOverlayServiceFixture,
        expected_hidden_child: Option<PinId>,
        rect: ScreenRect,
    ) -> PinId {
        let prior_pin_count = app.pinboard_capture.pinboard().len();
        let prior_selector_count = pinboard_selector_operations(overlay).len();
        let expected_hwnd = expected_hidden_child.map(|pin_id| 0x200 + pin_id.get() as usize);
        dispatch_pinboard_command(app, "pinboard:capture");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut saw_child_hide_command = expected_hidden_child.is_none();
        while std::time::Instant::now() < deadline
            && pinboard_selector_operations(overlay).len() <= prior_selector_count
        {
            let output = launcher_pinboard_frame(app, native, Vec::new());
            if let Some(pin_id) = expected_hidden_child {
                let viewport_id = {
                    let presenter = app.pinboard_capture.presenter_for_test();
                    let registry = presenter.registry.lock().unwrap();
                    registry.pins[&pin_id].viewport_id
                };
                if output
                    .viewport_output
                    .get(&viewport_id)
                    .is_some_and(|viewport| {
                        viewport
                            .commands
                            .iter()
                            .any(|command| matches!(command, egui::ViewportCommand::Visible(false)))
                    })
                {
                    saw_child_hide_command = true;
                }
            }
            output.drop_without_applying_deltas();
            std::thread::yield_now();
        }
        let operation_id = pinboard_selector_operations(overlay)
            .get(prior_selector_count)
            .copied()
            .expect("selector starts after ROOT and existing-child hide acknowledgements");
        assert!(
            saw_child_hide_command,
            "existing child received its hide request"
        );
        if let Some(hwnd) = expected_hwnd {
            assert!(!native.is_visible_for_test(hwnd));
        }
        assert!(!app.visible_flag.load(Ordering::SeqCst));
        assert!(!app.last_visible);
        overlay.controller.inject_pinboard_event_for_test(
            crate::gui::mkmacro_dialog::visual_overlay::VisualOverlayEvent::RectangleConfirmed {
                operation_id,
                purpose:
                    crate::gui::mkmacro_dialog::visual_overlay::RectanglePurpose::PinboardCapture,
                rect,
            },
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline
            && app.pinboard_capture.pinboard().len() == prior_pin_count
        {
            let output = launcher_pinboard_frame(app, native, Vec::new());
            output.drop_without_applying_deltas();
            std::thread::yield_now();
        }
        assert_eq!(app.pinboard_capture.pinboard().len(), prior_pin_count + 1);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.last_visible);
        app.pinboard_capture.pinboard().pins().last().unwrap().id()
    }

    fn assert_no_pin_control_commands(output: &egui::FullOutput, context: &str) {
        for (viewport_id, viewport) in &output.viewport_output {
            for command in &viewport.commands {
                assert!(
                    !matches!(
                        command,
                        egui::ViewportCommand::WindowLevel(_)
                            | egui::ViewportCommand::MousePassthrough(_)
                            | egui::ViewportCommand::Focus
                            | egui::ViewportCommand::Visible(_)
                            | egui::ViewportCommand::OuterPosition(_)
                            | egui::ViewportCommand::InnerSize(_)
                            | egui::ViewportCommand::MinInnerSize(_)
                            | egui::ViewportCommand::MaxInnerSize(_)
                            | egui::ViewportCommand::ResizeIncrements(_)
                            | egui::ViewportCommand::BeginResize(_)
                            | egui::ViewportCommand::Resizable(_)
                            | egui::ViewportCommand::Minimized(_)
                            | egui::ViewportCommand::Maximized(_)
                            | egui::ViewportCommand::Fullscreen(_)
                            | egui::ViewportCommand::SetMonitor(_)
                            | egui::ViewportCommand::Decorations(_)
                            | egui::ViewportCommand::StartDrag
                            | egui::ViewportCommand::CursorPosition(_)
                            | egui::ViewportCommand::CursorGrab(_)
                            | egui::ViewportCommand::CursorVisible(_)
                    ),
                    "{context} emitted {command:?} for {viewport_id:?}"
                );
            }
        }
    }

    fn pin_callbacks(
        ctx: &egui::Context,
        presenter: &mut PinWindowPresenter,
        board: &Pinboard,
    ) -> BTreeMap<PinId, Arc<egui::DeferredViewportUiCallback>> {
        presenter.set_root_native_identity((ROOT_HWND, 1));
        let mut root_input = egui::RawInput::default();
        root_input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(2.0);
        let output = ctx.run_ui(root_input, |root| presenter.show(root.ctx(), board));
        let callbacks = board
            .pins()
            .map(|pin| {
                let callback = output
                    .viewport_output
                    .get(&pin_viewport_id(pin.id(), pin.id().get()))
                    .and_then(|viewport| viewport.viewport_ui_cb.clone())
                    .expect("pin deferred callback registered");
                (pin.id(), callback)
            })
            .collect();
        output.drop_without_applying_deltas();
        callbacks
    }

    fn publish_fake_live_frame(
        frames: &mpsc::Sender<crate::pinboard::live::test_support::FakeFrame>,
        generation: PinCaptureGeneration,
        sequence: u64,
        color: Rgba<u8>,
    ) {
        let (processed_tx, processed_rx) = mpsc::sync_channel(1);
        frames
            .send(crate::pinboard::live::test_support::FakeFrame {
                generation,
                sequence,
                content_size: (2, 2),
                image: RgbaImage::from_pixel(2, 2, color),
                processed: processed_tx,
            })
            .unwrap();
        processed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("fake source worker should acknowledge the frame");
    }

    fn delta_image(
        output: &egui::FullOutput,
        texture_id: egui::TextureId,
    ) -> Arc<egui::ColorImage> {
        let delta = output
            .textures_delta
            .set
            .iter()
            .find(|(id, _)| **id == texture_id)
            .and_then(|(_, deltas)| deltas.last())
            .map(|delta| &delta.image)
            .expect("the expected live texture has an update");
        match delta {
            egui::ImageData::Color(image) => image.clone(),
        }
    }

    #[test]
    fn pinboard_live_presenter_reuses_texture_and_charges_retained_uploads() {
        let mut board = Pinboard::with_limits(PinboardLimits::new(2, 256, 64)).unwrap();
        let (pin_id, generation, frames) = add_live_pin(&mut board, 1);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native.clone();
        let mut presenter = PinWindowPresenter::with_native_api(native_api);
        let repaint_ids = Arc::new(Mutex::new(Vec::new()));
        let repaint_ids_callback = Arc::clone(&repaint_ids);
        ctx.set_request_repaint_callback(move |info| {
            repaint_ids_callback.lock().unwrap().push(info.viewport_id);
        });
        let callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        let runtime = presenter.registry.lock().unwrap().pins[&pin_id].clone();
        fake_native.add_window(&runtime, 0x501, true, ScreenRect::new(0, 0, 2, 2));
        let preparation = runtime.image_source.live_preparation().unwrap().clone();
        assert!(preparation.wait_for_sequence(1));
        repaint_ids.lock().unwrap().clear();

        let callback = callbacks[&pin_id].clone();
        let first_output = ctx.run_ui(child_input(runtime.viewport_id, 1.0), |ui| callback(ui));
        let first_texture_id = runtime.state.lock().unwrap().texture.as_ref().unwrap().id();
        let retained_first_upload = delta_image(&first_output, first_texture_id);
        assert_eq!(
            retained_first_upload.pixels[0],
            egui::Color32::from_rgba_unmultiplied(11, 12, 13, 255)
        );
        first_output.drop_without_applying_deltas();

        assert!(board.pause_live(pin_id));
        assert_eq!(preparation.status(), PinLiveStatus::Paused);
        let paused_copy = build_pin_action_job(&runtime, 1, PinActionKind::Copy).unwrap();
        let paused_save = build_pin_action_job(&runtime, 2, PinActionKind::SaveAs).unwrap();
        publish_fake_live_frame(&frames, generation, 2, Rgba([201, 202, 203, 255]));
        assert_eq!(board.get(pin_id).unwrap().live_frame().unwrap().sequence, 1);
        assert!(board.resume_live(pin_id));
        assert_eq!(preparation.status(), PinLiveStatus::Live);
        for _ in 0..4 {
            let _ = ctx.run_logic(&root_input(Vec::new()), |_| {});
            let _ = ctx.run_logic(&child_input(runtime.viewport_id, 1.0), |_| {});
            if !ctx.has_requested_repaint_for(&runtime.viewport_id)
                && !ctx.has_requested_repaint_for(&egui::ViewportId::ROOT)
            {
                break;
            }
        }
        assert!(!ctx.has_requested_repaint_for(&runtime.viewport_id));
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));
        repaint_ids.lock().unwrap().clear();

        publish_fake_live_frame(&frames, generation, 3, Rgba([201, 202, 203, 255]));
        assert!(preparation.wait_for_sequence(3));
        assert!(ctx.has_requested_repaint_for(&runtime.viewport_id));
        assert!(!ctx.has_requested_repaint_for(&egui::ViewportId::ROOT));
        let child_repaints = repaint_ids.lock().unwrap().clone();
        assert!(
            child_repaints
                .iter()
                .all(|viewport| *viewport == runtime.viewport_id)
        );

        let second_output = ctx.run_ui(child_input(runtime.viewport_id, 1.0), |ui| callback(ui));
        assert_eq!(
            runtime.state.lock().unwrap().texture.as_ref().unwrap().id(),
            first_texture_id
        );
        let second_upload = delta_image(&second_output, first_texture_id);
        assert_eq!(
            second_upload.pixels[0],
            egui::Color32::from_rgba_unmultiplied(201, 202, 203, 255)
        );
        second_output.drop_without_applying_deltas();
        assert_eq!(preparation.retained_upload_count(), 1);
        assert_eq!(
            board.image_bytes(),
            64,
            "source and both retained display snapshots are budgeted"
        );

        let clipboard = RecordingPinClipboard::default();
        run_pin_action_with_targets(&paused_copy, &clipboard, || Ok(None)).unwrap();
        let save_dir = fixture_directory();
        let save_path = save_dir.path().join("paused-frame.png");
        run_pin_action_with_targets(&paused_save, &clipboard, || Ok(Some(save_path.clone())))
            .unwrap();
        assert_eq!(
            clipboard.images.lock().unwrap()[0].get_pixel(0, 0),
            &Rgba([11, 12, 13, 255])
        );
        assert_eq!(
            image::open(&save_path).unwrap().to_rgba8().get_pixel(0, 0),
            &Rgba([11, 12, 13, 255])
        );
        drop(paused_copy);
        drop(paused_save);
        drop(retained_first_upload);

        let release_deadline = Instant::now() + Duration::from_secs(2);
        while preparation.retained_upload_count() != 0 && Instant::now() < release_deadline {
            thread::yield_now();
        }
        assert_eq!(preparation.retained_upload_count(), 0);
        assert_eq!(board.image_bytes(), 32);

        let retained_upload_across_cancel = Arc::clone(&second_upload);
        presenter.close(pin_id, &mut board);
        assert!(preparation.cancel_and_join().is_ok());
        preparation.reap_retired_uploads();
        assert!(preparation.has_retained_uploads());
        assert_eq!(
            board.image_bytes(),
            32,
            "the renderer-held upload and current source frame remain charged after close"
        );
        drop(retained_upload_across_cancel);
        drop(second_upload);
        preparation.reap_retired_uploads();
        assert!(!preparation.has_retained_uploads());
        assert_eq!(
            board.image_bytes(),
            16,
            "only the shared source slot remains charged"
        );
        presenter.shutdown().unwrap();
    }

    #[test]
    fn pinboard_presenter_capture_feedback_re_registers_until_dismissed() {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let mut presenter = PinWindowPresenter::new();
        presenter.queue_capture_feedback("the source closed before a frame was available".into());
        let viewport_id = presenter.capture_feedback_viewport_id;

        for _ in 0..2 {
            let output = ctx.run_ui(root_input(Vec::new()), |root| {
                presenter.show_capture_feedback(root.ctx());
            });
            assert!(
                output.viewport_output.contains_key(&viewport_id),
                "feedback viewport must be registered on every root pass until dismissed"
            );
            output.drop_without_applying_deltas();
        }

        presenter
            .capture_feedback_closed
            .store(true, Ordering::Release);
        let output = ctx.run_ui(root_input(Vec::new()), |root| {
            presenter.show_capture_feedback(root.ctx());
        });
        assert!(presenter.capture_feedback.is_none());
        assert!(!output.viewport_output.contains_key(&viewport_id));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn pin_controls_use_exact_child_commands_and_reject_invalid_owners() {
        let mut board = Pinboard::default();
        let first_rect = ScreenRect::new(-1600, -100, 2, 2);
        let first = add_pin(&mut board, 1, first_rect);
        let second = add_pin(&mut board, 2, ScreenRect::new(200, 100, 2, 2));
        let original_zoom = board.get(first).unwrap().zoom();
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native.clone();
        let mut presenter = PinWindowPresenter::with_native_api(native_api);
        let callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        drop(callbacks);

        let (first_runtime, second_runtime) = {
            let registry = presenter.registry.lock().unwrap();
            (
                registry.pins[&first].clone(),
                registry.pins[&second].clone(),
            )
        };
        assert_ne!(first_runtime.viewport_id, egui::ViewportId::ROOT);
        assert_ne!(second_runtime.viewport_id, egui::ViewportId::ROOT);
        assert_ne!(first_runtime.viewport_id, second_runtime.viewport_id);
        assert!(
            first_runtime.state.lock().unwrap().always_on_top,
            "pins begin topmost"
        );
        assert!(!first_runtime.state.lock().unwrap().click_through);
        let builder = pin_viewport_builder(&first_runtime);
        assert_eq!(builder.window_level, Some(egui::WindowLevel::AlwaysOnTop));
        assert_eq!(builder.mouse_passthrough, Some(false));

        fake_native.add_window(&first_runtime, 0x201, true, first_rect);
        fake_native.add_window(
            &second_runtime,
            0x202,
            true,
            ScreenRect::new(200, 100, 400, 300),
        );
        let mut control_result = None;
        let output = ctx.run_ui(root_input(Vec::new()), |root| {
            presenter.show(root.ctx(), &board);
            control_result = Some(apply_pin_control_change(
                root.ctx(),
                &first_runtime,
                fake_native.as_ref(),
                ROOT_HWND,
                PinControlChange {
                    always_on_top: Some(false),
                    click_through: Some(true),
                },
            ));
        });
        assert_eq!(control_result.unwrap(), Ok(()));
        let child_output = output
            .viewport_output
            .get(&first_runtime.viewport_id)
            .expect("commands target the exact child viewport");
        assert!(child_output.commands.iter().any(|command| matches!(
            command,
            egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal)
        )));
        assert!(
            child_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::MousePassthrough(true)))
        );
        assert!(child_output.commands.iter().all(|command| !matches!(
            command,
            egui::ViewportCommand::Focus
                | egui::ViewportCommand::Visible(_)
                | egui::ViewportCommand::OuterPosition(_)
                | egui::ViewportCommand::InnerSize(_)
        )));
        assert!(
            output
                .viewport_output
                .get(&second_runtime.viewport_id)
                .is_none_or(|output| output.commands.is_empty())
        );
        let first_state = first_runtime.state.lock().unwrap();
        assert!(!first_state.always_on_top);
        assert!(first_state.click_through);
        assert_eq!(first_state.display_bounds, first_rect);
        assert_eq!(first_state.zoom, original_zoom);
        drop(first_state);
        let second_state = second_runtime.state.lock().unwrap();
        assert!(second_state.always_on_top);
        assert!(!second_state.click_through);
        drop(second_state);
        output.drop_without_applying_deltas();

        let embedded = egui::Context::default();
        embedded.set_embed_viewports(true);
        assert!(
            apply_pin_control_change(
                &embedded,
                &first_runtime,
                fake_native.as_ref(),
                ROOT_HWND,
                PinControlChange {
                    always_on_top: None,
                    click_through: Some(false),
                },
            )
            .is_err()
        );

        let foreign_viewport = egui::ViewportId::from_hash_of("unrelated-pin-child");
        let mut foreign_result = None;
        let foreign_output = ctx.run_ui(child_input(foreign_viewport, 1.0), |foreign| {
            foreign_result = Some(apply_pin_control_change(
                foreign.ctx(),
                &first_runtime,
                fake_native.as_ref(),
                ROOT_HWND,
                PinControlChange {
                    always_on_top: None,
                    click_through: Some(false),
                },
            ));
        });
        assert!(foreign_result.unwrap().is_err());
        assert_no_pin_control_commands(&foreign_output, "foreign child");
        assert!(
            foreign_output
                .viewport_output
                .get(&first_runtime.viewport_id)
                .is_none_or(|output| output.commands.is_empty()),
            "an unrelated child cannot receive commands for this pin"
        );
        foreign_output.drop_without_applying_deltas();

        let mut stale_binding = first_runtime
            .state
            .lock()
            .unwrap()
            .native
            .clone()
            .expect("the accepted style request records its observed child binding");
        stale_binding.hwnd = ROOT_HWND;
        first_runtime.state.lock().unwrap().native = Some(stale_binding);
        let mut stale_result = None;
        let stale_output = ctx.run_ui(root_input(Vec::new()), |root| {
            stale_result = Some(apply_pin_control_change(
                root.ctx(),
                &first_runtime,
                fake_native.as_ref(),
                ROOT_HWND,
                PinControlChange {
                    always_on_top: None,
                    click_through: Some(false),
                },
            ));
        });
        assert!(stale_result.unwrap().is_err());
        assert_no_pin_control_commands(&stale_output, "stale binding");
        assert!(
            stale_output
                .viewport_output
                .get(&first_runtime.viewport_id)
                .is_none_or(|output| output.commands.is_empty()),
            "a stale native binding cannot receive commands"
        );
        assert!(first_runtime.state.lock().unwrap().click_through);
        stale_output.drop_without_applying_deltas();
        presenter.shutdown().unwrap();
    }

    #[test]
    fn launcher_pinboard_commands_capture_two_pins_and_recover_all_by_keyboard() {
        let directory = fixture_directory();
        let mut app = crate::gui::command_host::tests::inert_test_app(directory.path());
        app.egui_ctx.set_embed_viewports(false);
        app.launcher_hwnd = Some(ROOT_HWND);
        app.root_window_bridge.set_identity_for_test(ROOT_HWND);
        app.visible_flag.store(true, Ordering::SeqCst);
        app.last_visible = true;
        let root_identity = app.root_window_bridge.identity();

        let overlay =
            crate::gui::mkmacro_dialog::visual_capture_workflow::SharedVisualOverlayController::test_fixture();
        app.mkmacro_dialog.visual_overlay = overlay.controller.clone();
        let desktop = ScreenRect::new(-1_920, -240, 3_840, 1_320);
        let capture_backend = Arc::new(PinboardFixtureCaptureBackend {
            desktop,
            captures: AtomicUsize::new(0),
        });
        let parking_log = Arc::new(PinboardFixtureParkingLog::default());
        let mut lifecycle = crate::gui::pinboard::PinboardCaptureLifecycle::new(
            overlay.controller.clone(),
            capture_backend.clone(),
        );
        lifecycle.set_parking_factory_for_test(Arc::new(PinboardFixtureParkingFactory(
            Arc::clone(&parking_log),
        )));

        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native.clone();
        let save_path = directory.path().join("explicit-pin-save.png");
        let (save_started_tx, save_started_rx) = mpsc::channel();
        let save_picker: Arc<dyn PinSavePicker> = Arc::new(FixturePinSavePicker {
            path: save_path.clone(),
            started: save_started_tx,
            calls: AtomicUsize::new(0),
        });
        let clipboard = Arc::new(RecordingPinClipboard::default());
        let presenter = PinWindowPresenter::with_native_api_and_targets(
            native_api,
            save_picker,
            clipboard.clone(),
        );
        lifecycle.install_window_presenter_for_test(presenter);
        app.pinboard_capture = lifecycle;

        let first = capture_pin_through_launcher_command(
            &mut app,
            fake_native.as_ref(),
            &overlay,
            None,
            ScreenRect::new(-300, 40, 8, 8),
        );
        assert_eq!(first.get(), 1);
        register_fixture_native_children(&app, fake_native.as_ref());
        let first_bind_frame = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        first_bind_frame.drop_without_applying_deltas();
        {
            let presenter = app.pinboard_capture.presenter_for_test();
            let registry = presenter.registry.lock().unwrap();
            let first_runtime = &registry.pins[&first];
            assert_ne!(first_runtime.viewport_id, egui::ViewportId::ROOT);
            assert!(first_runtime.state.lock().unwrap().native.is_some());
        }
        let context = app.egui_ctx.clone();
        app.pinboard_capture
            .set_pin_click_through_for_test(&context, first, true)
            .unwrap();
        let first_passthrough = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        first_passthrough.drop_without_applying_deltas();

        let second = capture_pin_through_launcher_command(
            &mut app,
            fake_native.as_ref(),
            &overlay,
            Some(first),
            ScreenRect::new(160, 120, 8, 8),
        );
        assert_eq!(second.get(), 2);
        register_fixture_native_children(&app, fake_native.as_ref());
        let second_bind_frame = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        second_bind_frame.drop_without_applying_deltas();
        let (first_viewport, second_viewport, second_bounds) = {
            let presenter = app.pinboard_capture.presenter_for_test();
            let registry = presenter.registry.lock().unwrap();
            let first_runtime = &registry.pins[&first];
            let second_runtime = &registry.pins[&second];
            assert_ne!(first_runtime.viewport_id, egui::ViewportId::ROOT);
            assert_ne!(second_runtime.viewport_id, egui::ViewportId::ROOT);
            assert_ne!(first_runtime.viewport_id, second_runtime.viewport_id);
            assert!(first_runtime.state.lock().unwrap().native.is_some());
            assert!(second_runtime.state.lock().unwrap().native.is_some());
            (
                first_runtime.viewport_id,
                second_runtime.viewport_id,
                app.pinboard_capture
                    .pinboard()
                    .get(second)
                    .unwrap()
                    .display_bounds(),
            )
        };
        let context = app.egui_ctx.clone();
        app.pinboard_capture
            .set_pin_click_through_for_test(&context, first, true)
            .unwrap();
        app.pinboard_capture
            .set_pin_click_through_for_test(&context, second, true)
            .unwrap();
        let passthrough = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        passthrough.drop_without_applying_deltas();

        let query_focus_id = request_competing_query_focus(&app.egui_ctx);
        app.focus_query = true;
        dispatch_pinboard_command(&mut app, "pinboard:list");
        assert!(app.pin_list_open);
        assert!(
            !app.focus_query,
            "the explicit list command cancels query refocus"
        );
        let list_frame = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        let recover_all_id = app
            .pinboard_capture
            .presenter_for_test()
            .recover_all_widget_id_for_test();
        let root_focus_requested = list_frame
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| {
                viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Focus))
            });
        let focused_widget = app.egui_ctx.memory(|memory| memory.focused());
        let top_layer = app.egui_ctx.top_layer_id();
        list_frame.drop_without_applying_deltas();
        let recover_all_id = recover_all_id.expect("Pin List rendered its Recover All control");
        assert!(root_focus_requested);
        assert_ne!(query_focus_id, recover_all_id);
        assert_eq!(
            focused_widget,
            Some(recover_all_id),
            "the explicit list command transfers focus from the query to Recover All"
        );
        assert_eq!(
            top_layer,
            Some(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("screenshot-pin-list"),
            )),
            "the explicitly opened list is raised above other root windows"
        );
        let passive_update = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        let passive_focus_requested = passive_update
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| {
                viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Focus))
            });
        passive_update.drop_without_applying_deltas();
        assert!(!passive_focus_requested);
        let recovery = launcher_pinboard_frame(
            &mut app,
            fake_native.as_ref(),
            vec![key_event(egui::Key::Enter)],
        );
        let recovered_children = [first_viewport, second_viewport].map(|viewport_id| {
            recovery
                .viewport_output
                .get(&viewport_id)
                .is_some_and(|viewport| {
                    viewport.commands.iter().any(|command| {
                        matches!(command, egui::ViewportCommand::MousePassthrough(false))
                    })
                })
        });
        let root_style_unchanged = recovery
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_none_or(|viewport| {
                !viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::MousePassthrough(_)))
            });
        recovery.drop_without_applying_deltas();
        assert!(recovered_children.into_iter().all(|recovered| recovered));
        assert!(root_style_unchanged);
        let recovery_sync = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        recovery_sync.drop_without_applying_deltas();
        assert!(
            !app.pinboard_capture
                .pinboard()
                .get(first)
                .unwrap()
                .click_through()
        );
        assert!(
            !app.pinboard_capture
                .pinboard()
                .get(second)
                .unwrap()
                .click_through()
        );

        assert!(clipboard.images.lock().unwrap().is_empty());
        assert!(!save_path.exists());
        let action_client = app.pinboard_capture.presenter_for_test().actions.client();
        let second_image = app
            .pinboard_capture
            .pinboard()
            .get(second)
            .unwrap()
            .snapshot()
            .unwrap()
            .image_handle();
        action_client
            .submit(PinActionJob {
                pin_id: second,
                lifetime: second.get(),
                request_id: 91,
                image: second_image.clone(),
                kind: PinActionKind::Copy,
            })
            .unwrap();
        action_client
            .submit(PinActionJob {
                pin_id: second,
                lifetime: second.get(),
                request_id: 92,
                image: second_image,
                kind: PinActionKind::SaveAs,
            })
            .unwrap();
        assert_eq!(
            save_started_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            0
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut completions = Vec::new();
        while std::time::Instant::now() < deadline && completions.len() < 2 {
            completions.extend(
                app.pinboard_capture
                    .presenter_for_test()
                    .actions
                    .drain_completions(),
            );
            std::thread::yield_now();
        }
        assert_eq!(completions.len(), 2);
        assert!(
            completions
                .iter()
                .all(|completion| completion.result.is_ok())
        );
        assert!(save_path.exists());
        assert_eq!(
            *image::open(&save_path)
                .unwrap()
                .into_rgba8()
                .get_pixel(0, 0),
            Rgba([18, 30, 44, 255])
        );
        assert_eq!(
            *clipboard.images.lock().unwrap()[0].get_pixel(0, 0),
            Rgba([18, 30, 44, 255])
        );
        assert_eq!(capture_backend.captures.load(Ordering::Acquire), 2);
        assert_eq!(parking_log.begins.load(Ordering::Acquire), 2);
        assert_eq!(parking_log.restores.load(Ordering::Acquire), 2);
        assert_eq!(app.root_window_bridge.identity(), root_identity);
        assert!(app.visible_flag.load(Ordering::SeqCst));
        assert!(app.last_visible);

        assert!(app.close_screenshot_pin(first));
        assert!(app.pinboard_capture.pinboard().get(first).is_none());
        assert!(fake_native.contains(0x200 + first.get() as usize));
        fake_native.destroy(0x200 + first.get() as usize);
        let close_frame = launcher_pinboard_frame(&mut app, fake_native.as_ref(), Vec::new());
        close_frame.drop_without_applying_deltas();
        assert_eq!(app.pinboard_capture.pinboard().len(), 1);
        assert_eq!(app.pinboard_capture.pinboard().image_bytes(), 8 * 8 * 4);
        assert_eq!(
            app.pinboard_capture
                .pinboard()
                .get(second)
                .unwrap()
                .display_bounds(),
            second_bounds
        );
        assert_eq!(app.root_window_bridge.identity(), root_identity);
        app.shutdown_pinboard_capture();
    }

    #[test]
    fn pin_list_is_keyboard_focusable_and_stale_rows_cannot_reopen_closed_pins() {
        let mut board = Pinboard::default();
        let first = add_pin(&mut board, 1, ScreenRect::new(-1600, -100, 2, 2));
        let second = add_pin(&mut board, 2, ScreenRect::new(200, 100, 2, 2));
        board.get_mut(first).unwrap().set_click_through(true);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native.clone();
        let mut presenter = PinWindowPresenter::with_native_api(native_api);
        let callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        drop(callbacks);

        let (first_runtime, second_runtime) = {
            let registry = presenter.registry.lock().unwrap();
            (
                registry.pins[&first].clone(),
                registry.pins[&second].clone(),
            )
        };
        fake_native.add_window(
            &first_runtime,
            0x301,
            true,
            ScreenRect::new(-1600, -100, 400, 300),
        );
        fake_native.add_window(
            &second_runtime,
            0x302,
            true,
            ScreenRect::new(200, 100, 400, 300),
        );
        presenter.poll(&ctx, &mut board);
        assert!(first_runtime.state.lock().unwrap().native.is_some());
        assert!(second_runtime.state.lock().unwrap().native.is_some());

        let first_lifetime = first_runtime.lifetime;
        let mut open = true;
        let first_frame = ctx.run_ui(root_input(Vec::new()), |root| {
            presenter.show_pin_list(root.ctx(), &mut board, &mut open);
        });
        assert!(open);
        first_frame.drop_without_applying_deltas();

        let tab = || root_input(vec![key_event(egui::Key::Tab)]);
        let first_tab = ctx.run_ui(tab(), |root| {
            presenter.show_pin_list(root.ctx(), &mut board, &mut open);
        });
        let first_focused_widget = ctx.memory(|memory| memory.focused());
        assert!(
            first_focused_widget.is_some(),
            "Tab reaches a Pin List control"
        );
        first_tab.drop_without_applying_deltas();
        let second_tab = ctx.run_ui(tab(), |root| {
            presenter.show_pin_list(root.ctx(), &mut board, &mut open);
        });
        let second_focused_widget = ctx.memory(|memory| memory.focused());
        assert!(second_focused_widget.is_some());
        assert_ne!(first_focused_widget, second_focused_widget);
        second_tab.drop_without_applying_deltas();

        assert!(presenter.close_lifetime(&mut board, first, first_lifetime));
        assert!(board.get(first).is_none());
        let mut stale_control_result = None;
        let stale_control = ctx.run_ui(root_input(Vec::new()), |root| {
            presenter.show_pin_list(root.ctx(), &mut board, &mut open);
            stale_control_result = Some(presenter.apply_list_control(
                root.ctx(),
                &mut board,
                first,
                first_lifetime,
                PinControlChange {
                    always_on_top: None,
                    click_through: Some(false),
                },
                fake_native.as_ref(),
                ROOT_HWND,
            ));
        });
        assert!(stale_control_result.unwrap().is_err());
        assert!(
            !stale_control
                .viewport_output
                .contains_key(&first_runtime.viewport_id)
        );
        stale_control.drop_without_applying_deltas();

        let remaining_callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        assert!(!remaining_callbacks.contains_key(&first));
        assert!(remaining_callbacks.contains_key(&second));
        drop(remaining_callbacks);

        let registry_weak = Arc::downgrade(&presenter.registry);
        presenter.shutdown().unwrap();
        assert!(presenter.registry.lock().unwrap().pins.is_empty());
        assert!(presenter.registry.lock().unwrap().retired.is_empty());
        assert!(second_runtime.state.lock().unwrap().close_requested);
        assert!(!second_runtime.state.lock().unwrap().root_registered);
        assert!(!presenter.actions.has_pending_work());
        drop(first_runtime);
        drop(second_runtime);
        drop(presenter);
        assert!(registry_weak.upgrade().is_none());
    }

    #[test]
    fn retained_child_callbacks_keep_textures_and_zoom_independent_of_root_scale() {
        let mut board = Pinboard::default();
        let first_rect = ScreenRect::new(-1920, -120, 2, 2);
        let second_rect = ScreenRect::new(1920, 80, 2, 2);
        let first = add_pin(&mut board, 1, first_rect);
        let second = add_pin(&mut board, 2, second_rect);
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native;
        let mut presenter = PinWindowPresenter::with_native_api(native_api);

        assert!(presenter.needs_viewport_bootstrap(&board));
        assert!(presenter.needs_root_ui_when_hidden(&board));
        let callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        assert_ne!(
            pin_viewport_id(first, first.get()),
            pin_viewport_id(second, second.get())
        );
        assert!(presenter.needs_root_ui_when_hidden(&board));
        assert!(!presenter.needs_viewport_bootstrap(&board));

        for pin_id in [first, second] {
            let callback = callbacks.get(&pin_id).unwrap();
            let output = ctx.run_ui(
                child_input(pin_viewport_id(pin_id, pin_id.get()), 1.5),
                |child| callback(child),
            );
            output.drop_without_applying_deltas();
        }
        drop(callbacks);

        let runtimes = presenter.registry.lock().unwrap();
        let first_runtime = runtimes.pins.get(&first).unwrap();
        let second_runtime = runtimes.pins.get(&second).unwrap();
        assert!(first_runtime.state.lock().unwrap().callback_entered);
        assert!(second_runtime.state.lock().unwrap().callback_entered);
        let first_texture = first_runtime
            .state
            .lock()
            .unwrap()
            .texture
            .as_ref()
            .unwrap()
            .id();
        let second_texture = second_runtime
            .state
            .lock()
            .unwrap()
            .texture
            .as_ref()
            .unwrap()
            .id();
        assert_ne!(first_texture, second_texture);
        drop(runtimes);

        let first_runtime = presenter.registry.lock().unwrap().pins[&first].clone();
        {
            let mut state = first_runtime.state.lock().unwrap();
            state.scale_mode = PinScaleMode::Custom;
            state.zoom = 3.25;
        }
        let callbacks_after_root_frame = pin_callbacks(&ctx, &mut presenter, &board);
        for pin_id in [first, second] {
            let callback = callbacks_after_root_frame.get(&pin_id).unwrap();
            let ppp = if pin_id == first { 2.0 } else { 1.0 };
            let mut observed_child_ppp = 0.0;
            let output = ctx.run_ui(
                child_input(pin_viewport_id(pin_id, pin_id.get()), ppp),
                |child| {
                    observed_child_ppp = child.ctx().pixels_per_point();
                    callback(child);
                },
            );
            output.drop_without_applying_deltas();
            assert!((observed_child_ppp - ppp).abs() < 0.001);
        }
        let state = first_runtime.state.lock().unwrap();
        assert_eq!(state.scale_mode, PinScaleMode::Custom);
        assert_eq!(state.zoom, 3.25);
        assert_eq!(state.texture.as_ref().unwrap().id(), first_texture);
        drop(state);
        let second_runtime = presenter.registry.lock().unwrap().pins[&second].clone();
        let second_state = second_runtime.state.lock().unwrap();
        assert_eq!(second_state.texture.as_ref().unwrap().id(), second_texture);
        drop(second_state);

        for ppp in [1.0, 1.5, 2.0] {
            let actual = pin_image_display_size(
                (2, 2),
                PinScaleMode::Actual,
                1.0,
                egui::vec2(100.0, 80.0),
                ppp,
            );
            assert!((actual.x * ppp - 2.0).abs() < 0.001);
            assert!((actual.y * ppp - 2.0).abs() < 0.001);
        }
        let custom = pin_image_display_size(
            (2, 1),
            PinScaleMode::Custom,
            2.0,
            egui::vec2(100.0, 80.0),
            1.5,
        );
        assert!((custom.x / custom.y - 2.0).abs() < 0.001);
        let fit =
            pin_image_display_size((2, 1), PinScaleMode::Fit, 1.0, egui::vec2(40.0, 30.0), 1.0);
        assert!((fit.x / fit.y - 2.0).abs() < 0.001);
        assert_eq!(board.get(first).unwrap().display_bounds(), first_rect);
        assert_eq!(board.get(second).unwrap().display_bounds(), second_rect);

        drop(callbacks_after_root_frame);
        drop(first_runtime);
        assert!(presenter.close(first, &mut board));
        assert_eq!(board.len(), 1);
        {
            let registry = presenter.registry.lock().unwrap();
            assert!(registry.retired.contains_key(&first));
            assert!(registry.pins.contains_key(&second));
            assert!(
                registry.retired[&first]
                    .state
                    .lock()
                    .unwrap()
                    .texture
                    .is_some()
            );
        }
        let remaining_callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        drop(remaining_callbacks);
        presenter.poll(&ctx, &mut board);
        assert!(
            !presenter
                .registry
                .lock()
                .unwrap()
                .retired
                .contains_key(&first)
        );
        assert_eq!(board.image_bytes(), 16);
        presenter.shutdown().unwrap();
    }

    #[test]
    fn native_hide_ack_and_bounded_worker_preserve_close_and_finish_lifetimes() {
        let mut board = Pinboard::default();
        let first = add_pin(&mut board, 1, ScreenRect::new(-1600, -100, 2, 2));
        let second = add_pin(&mut board, 2, ScreenRect::new(200, 100, 2, 2));
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let fake_native = Arc::new(FakePinNativeApi::default());
        let native_api: Arc<dyn PinNativeApi> = fake_native.clone();
        let (picker_started_tx, picker_started_rx) = mpsc::channel();
        let (close_seen_tx, close_seen_rx) = mpsc::channel();
        let save_picker: Arc<dyn PinSavePicker> = Arc::new(ShutdownBlockedPinSavePicker {
            started: picker_started_tx,
            close_seen: close_seen_tx,
        });
        let mut presenter =
            PinWindowPresenter::with_native_api_and_save_picker(native_api, save_picker);
        presenter.set_root_native_identity((ROOT_HWND, 1));
        let _callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        {
            let registry = presenter.registry.lock().unwrap();
            fake_native.add_window(
                registry.pins.get(&second).unwrap(),
                0x202,
                true,
                ScreenRect::new(200, 100, 400, 300),
            );
        }
        presenter.poll(&ctx, &mut board);
        drop(_callbacks);
        let identity = board
            .reserve_capture(PinCaptureOperationId::new(99).unwrap())
            .unwrap();
        assert!(board.cancel_capture(identity));
        let mut visibility = presenter.visibility_coordinator();
        visibility.request_hide_all(identity).unwrap();
        assert_eq!(visibility.poll_hidden(identity), PinVisibilityAck::Pending);

        // A registered child may materialize after the hide intent. It must bind
        // the exact native lifetime, record its prior visibility, and then wait
        // for the real hidden observation before ACK.
        let late_runtime = presenter.registry.lock().unwrap().pins[&first].clone();
        fake_native.add_window(
            &late_runtime,
            0x201,
            true,
            ScreenRect::new(-1600, -100, 400, 300),
        );
        assert_eq!(visibility.poll_hidden(identity), PinVisibilityAck::Pending);
        fake_native.set_visible(0x201, false);
        assert_eq!(visibility.poll_hidden(identity), PinVisibilityAck::Pending);
        let late_ticket = late_runtime
            .state
            .lock()
            .unwrap()
            .capture_visibility
            .unwrap();
        assert!(late_ticket.hide_requested);
        assert!(late_ticket.hide_acknowledged);
        assert_eq!(late_ticket.prior_visible, Some(true));
        drop(late_runtime);
        fake_native.set_visible(0x202, false);
        assert_eq!(
            visibility.poll_hidden(identity),
            PinVisibilityAck::Acknowledged
        );

        let fixture = fixture_directory();
        let save_path = fixture.path().join("explicit-save.png");
        let client = presenter.actions.client();
        let first_job = || PinActionJob {
            pin_id: first,
            lifetime: first.get(),
            request_id: 1,
            image: board.get(first).unwrap().snapshot().unwrap().image_handle(),
            kind: PinActionKind::SaveAs,
        };
        client.submit(first_job()).unwrap();
        let picker_started = picker_started_rx.recv_timeout(std::time::Duration::from_secs(5));
        let mut submit_ok = true;
        for request_id in 2..=MAX_PIN_ACTIONS as u64 {
            let mut job = first_job();
            job.request_id = request_id;
            submit_ok &= client.submit(job).is_ok();
        }
        let mut overflow = first_job();
        overflow.request_id = MAX_PIN_ACTIONS as u64 + 1;
        let queue_rejected_overflow = client.submit(overflow).is_err();
        let queue_at_limit = presenter
            .actions
            .shared
            .state
            .lock()
            .is_ok_and(|state| state.active && state.admitted_jobs == MAX_PIN_ACTIONS);

        assert!(presenter.close(first, &mut board));
        let close_token = presenter
            .actions
            .shared
            .state
            .lock()
            .unwrap()
            .active_picker
            .clone()
            .unwrap();
        let preserved_after_close = close_token.cancel_count() == 0
            && presenter.actions.shared.state.lock().is_ok_and(|state| {
                state.active && state.queue.is_empty() && state.admitted_jobs == 1
            });
        let close_did_not_cancel_picker = close_seen_rx.try_recv().is_err();
        assert!(queue_rejected_overflow);
        assert!(submit_ok);
        assert_eq!(picker_started.unwrap(), 0);
        assert!(queue_at_limit);
        assert!(preserved_after_close);
        assert!(close_did_not_cancel_picker);
        // The closed pin's queued and active action references still own their
        // image budget until the blocked picker has safely completed.
        assert_eq!(board.image_bytes(), 32);

        visibility.request_restore_all(identity).unwrap();
        assert!(!fake_native.is_visible_for_test(0x201));
        assert_eq!(
            visibility.poll_restored(identity),
            PinVisibilityAck::Pending
        );
        fake_native.set_visible(0x202, true);
        assert_eq!(
            visibility.poll_restored(identity),
            PinVisibilityAck::Acknowledged
        );

        let cancel_before_ack = board
            .reserve_capture(PinCaptureOperationId::new(100).unwrap())
            .unwrap();
        assert!(board.cancel_capture(cancel_before_ack));
        visibility.request_hide_all(cancel_before_ack).unwrap();
        // The native hide has applied, but no hidden ACK was polled yet.
        fake_native.set_visible(0x202, false);
        visibility.request_restore_all(cancel_before_ack).unwrap();
        assert_eq!(
            visibility.poll_restored(cancel_before_ack),
            PinVisibilityAck::Pending
        );
        fake_native.set_visible(0x202, true);
        assert_eq!(
            visibility.poll_restored(cancel_before_ack),
            PinVisibilityAck::Acknowledged
        );

        let hide_still_queued = board
            .reserve_capture(PinCaptureOperationId::new(101).unwrap())
            .unwrap();
        assert!(board.cancel_capture(hide_still_queued));
        visibility.request_hide_all(hide_still_queued).unwrap();
        assert!(fake_native.is_visible_for_test(0x202));
        visibility.request_restore_all(hide_still_queued).unwrap();
        let restore_ticket = presenter.registry.lock().unwrap().pins[&second]
            .state
            .lock()
            .unwrap()
            .capture_visibility
            .unwrap();
        assert!(restore_ticket.restore_intent);
        assert!(restore_ticket.restore_requested);
        assert!(!restore_ticket.restore_acknowledged);
        // The queued hide can apply after cancellation, so restoration must
        // remain pending until the native window is observed visible again.
        fake_native.set_visible(0x202, false);
        assert_eq!(
            visibility.poll_restored(hide_still_queued),
            PinVisibilityAck::Pending
        );
        fake_native.set_visible(0x202, true);
        assert_eq!(
            visibility.poll_restored(hide_still_queued),
            PinVisibilityAck::Acknowledged
        );

        fake_native.destroy(0x201);
        presenter.poll(&ctx, &mut board);
        assert!(
            !presenter
                .registry
                .lock()
                .unwrap()
                .retired
                .contains_key(&first)
        );
        assert_eq!(board.len(), 1);
        assert_eq!(board.image_bytes(), 32);
        assert!(presenter.needs_viewport_bootstrap(&board));
        assert!(presenter.needs_root_ui_when_hidden(&board));
        let cleanup_callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        assert_eq!(cleanup_callbacks.len(), 1);
        drop(cleanup_callbacks);
        assert!(!presenter.needs_viewport_bootstrap(&board));
        assert!(presenter.needs_root_ui_when_hidden(&board));
        assert_eq!(board.image_bytes(), 32);

        presenter.actions.shutdown_and_join().unwrap();
        assert_eq!(close_token.cancel_count(), 1);
        assert!(
            close_seen_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .is_ok()
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut completion = None;
        while std::time::Instant::now() < deadline {
            if let Some(receipt) = presenter.actions.drain_completions().into_iter().next() {
                completion = Some(receipt);
                break;
            }
            thread::yield_now();
        }
        let completion = completion.expect("worker returns a Save As receipt");
        assert_eq!(completion.pin_id, first);
        assert_eq!(completion.lifetime, first.get());
        assert_eq!(completion.request_id, 1);
        assert_eq!(completion.result, Ok(None));
        assert_eq!(board.image_bytes(), 16);

        // The active picker is canceled at shutdown, while closing a pin only
        // removes queued work and preserves an already-authorized snapshot.
        let not_admitted = PinPickerCancellation::default();
        not_admitted.cancel();
        assert!(!not_admitted.try_admit_dialog());

        let (save_started_tx, save_started_rx) = mpsc::channel();
        let save_picker: Arc<dyn PinSavePicker> = Arc::new(FixturePinSavePicker {
            path: save_path.clone(),
            started: save_started_tx,
            calls: AtomicUsize::new(0),
        });
        let mut save_worker = PinActionWorker::with_save_picker(save_picker);
        let save_client = save_worker.client();
        let second_job = PinActionJob {
            pin_id: second,
            lifetime: second.get(),
            request_id: 20,
            image: board
                .get(second)
                .unwrap()
                .snapshot()
                .unwrap()
                .image_handle(),
            kind: PinActionKind::SaveAs,
        };
        save_client.submit(second_job).unwrap();
        assert_eq!(
            save_started_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            0
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut saved = None;
        while std::time::Instant::now() < deadline {
            if let Some(receipt) = save_worker.drain_completions().into_iter().next() {
                saved = Some(receipt);
                break;
            }
            thread::yield_now();
        }
        let saved = saved.expect("Save As returns an explicit save receipt");
        assert_eq!(saved.request_id, 20);
        assert!(saved.result.as_ref().unwrap().is_some());
        let decoded = image::open(&save_path).unwrap().into_rgba8();
        assert_eq!(*decoded.get_pixel(0, 0), Rgba([2, 32, 72, 255]));
        assert_eq!(decoded.as_raw().len(), 16);

        let cancelled_job = PinActionJob {
            pin_id: second,
            lifetime: second.get(),
            request_id: 22,
            image: board
                .get(second)
                .unwrap()
                .snapshot()
                .unwrap()
                .image_handle(),
            kind: PinActionKind::SaveAs,
        };
        save_client.submit(cancelled_job).unwrap();
        assert_eq!(
            save_started_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            1
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut cancellation = None;
        while std::time::Instant::now() < deadline {
            if let Some(receipt) = save_worker.drain_completions().into_iter().next() {
                cancellation = Some(receipt);
                break;
            }
            thread::yield_now();
        }
        let cancellation = cancellation.expect("cancelled picker returns a receipt");
        assert_eq!(cancellation.result, Ok(None));
        assert_eq!(cancellation.request_id, 22);
        let second_pixels = board.get(second).unwrap().snapshot().unwrap().pixels();
        let clipboard = clipboard_image_data(second_pixels);
        assert_eq!(clipboard.width, second_pixels.width() as usize);
        assert_eq!(clipboard.height, second_pixels.height() as usize);
        assert_eq!(clipboard.bytes.as_ref(), second_pixels.as_raw().as_slice());

        save_worker.shutdown_and_join().unwrap();

        let mut backpressure_worker =
            PinActionWorker::with_save_picker(Arc::new(ImmediateCancelPinSavePicker));
        let backpressure_client = backpressure_worker.client();
        for request_id in 30..30 + MAX_PIN_ACTIONS as u64 {
            backpressure_client
                .submit(PinActionJob {
                    pin_id: second,
                    lifetime: second.get(),
                    request_id,
                    image: board
                        .get(second)
                        .unwrap()
                        .snapshot()
                        .unwrap()
                        .image_handle(),
                    kind: PinActionKind::SaveAs,
                })
                .unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while backpressure_worker
            .shared
            .state
            .lock()
            .is_ok_and(|state| state.active || !state.queue.is_empty())
            && std::time::Instant::now() < deadline
        {
            thread::yield_now();
        }
        assert!(backpressure_worker.shared.state.lock().is_ok_and(|state| {
            !state.active && state.queue.is_empty() && state.admitted_jobs == MAX_PIN_ACTIONS
        }));
        assert!(backpressure_worker.has_pending_work());
        assert_eq!(
            backpressure_worker
                .shared
                .state
                .lock()
                .unwrap()
                .admitted_jobs,
            MAX_PIN_ACTIONS
        );
        assert!(
            backpressure_client
                .submit(PinActionJob {
                    pin_id: second,
                    lifetime: second.get(),
                    request_id: 30 + MAX_PIN_ACTIONS as u64,
                    image: board
                        .get(second)
                        .unwrap()
                        .snapshot()
                        .unwrap()
                        .image_handle(),
                    kind: PinActionKind::SaveAs,
                })
                .is_err()
        );
        assert_eq!(
            backpressure_worker.drain_completions().len(),
            MAX_PIN_ACTIONS
        );
        assert_eq!(
            backpressure_worker
                .shared
                .state
                .lock()
                .unwrap()
                .admitted_jobs,
            0
        );
        assert!(!backpressure_worker.has_pending_work());
        backpressure_worker.shutdown_and_join().unwrap();

        assert!(presenter.close(second, &mut board));
        fake_native.destroy(0x202);
        presenter.poll(&ctx, &mut board);
        assert!(board.is_empty());
        assert!(presenter.needs_viewport_bootstrap(&board));
        assert!(presenter.needs_root_ui_when_hidden(&board));
        let final_cleanup_callbacks = pin_callbacks(&ctx, &mut presenter, &board);
        assert!(final_cleanup_callbacks.is_empty());
        drop(final_cleanup_callbacks);
        assert!(!presenter.needs_viewport_bootstrap(&board));
        assert!(!presenter.needs_root_ui_when_hidden(&board));
        presenter.shutdown().unwrap();
        drop(save_worker);
        drop(backpressure_worker);
        drop(presenter);
        drop(ctx);
        assert_eq!(board.image_bytes(), 0);
    }
}
