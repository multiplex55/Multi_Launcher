//! Native child viewports and actions for the session-only screenshot pinboard.

use crate::mkmacro::ScreenRect;
use crate::pinboard::{
    PinId, PinImageHandle, PinVisibilityAck, PinWindowVisibilityCoordinator, Pinboard,
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

struct PinWindowRuntime {
    pin_id: PinId,
    lifetime: u64,
    viewport_id: egui::ViewportId,
    title: String,
    image: PinImageHandle,
    dimensions: (u32, u32),
    state: Mutex<PinWindowState>,
}

struct PinWindowState {
    texture: Option<egui::TextureHandle>,
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
    native_destroyed: bool,
    capture_visibility: Option<CaptureVisibilityState>,
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
    pending_parent_cleanup: bool,
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
        let process_id = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let registry = Arc::new(Mutex::new(PinWindowRegistry {
            pins: BTreeMap::new(),
            retired: BTreeMap::new(),
            native_api,
            root_context: None,
            root_native_identity: (0, 0),
            process_id,
            session_id: now ^ u128::from(process_id),
            hide_identity: None,
            restore_identity: None,
            shutting_down: false,
        }));
        Self {
            registry,
            actions: PinActionWorker::with_save_picker(save_picker),
            pending_parent_cleanup: false,
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

    /// Updates the owner from the domain model and registers its retained child callbacks.
    pub(crate) fn show(&mut self, ctx: &egui::Context, board: &Pinboard) {
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
                if let Some(runtime) = registry.pins.remove(&pin_id) {
                    if let Some(viewport_id) = retire_pin_runtime(&mut registry, runtime) {
                        close_viewports.push(viewport_id);
                    }
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
                let runtime = registry.pins.entry(pin.id()).or_insert_with(|| {
                    let lifetime = pin.id().get();
                    let viewport_id = pin_viewport_id(pin.id(), lifetime);
                    let title = pin_native_title(process_id, session_id, pin.id(), lifetime);
                    Arc::new(PinWindowRuntime {
                        pin_id: pin.id(),
                        lifetime,
                        viewport_id,
                        title,
                        image: pin.snapshot().image_handle(),
                        dimensions: pin.snapshot().physical_dimensions(),
                        state: Mutex::new(PinWindowState {
                            texture: None,
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
                            native_destroyed: false,
                            capture_visibility: None,
                        }),
                    })
                });
                if let Ok(mut state) = runtime.state.lock() {
                    state.display_bounds = pin.display_bounds();
                    if state.close_requested || state.native_destroyed {
                        if !state.close_command_sent && !state.native_destroyed {
                            state.close_command_sent = true;
                            close_viewports.push(runtime.viewport_id);
                        }
                        continue;
                    }
                    state.root_registered = true;
                }
                let suppress =
                    should_suppress_pin(runtime, hide_identity, restore_identity, shutting_down);
                callbacks.push((Arc::clone(runtime), suppress));
            }
            (
                registry.root_native_identity,
                registry.shutting_down,
                Arc::clone(&registry.native_api),
            )
        };
        self.actions.set_root_context(ctx.clone());
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
            let root_context = ctx.clone();
            ctx.show_viewport_deferred(runtime.viewport_id, builder, move |ui, class| {
                render_pin_viewport(
                    ui,
                    class,
                    &runtime,
                    &action_client,
                    &callback_native_api,
                    root_native_identity,
                    &root_context,
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

    pub(crate) fn needs_viewport_bootstrap(&self, board: &Pinboard) -> bool {
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
        self.registry.lock().is_ok_and(|registry| {
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
            self.actions.set_root_context(ctx.clone());
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
            let (title, lifetime, root_registered, existing, closing) = {
                let Ok(state) = runtime.state.lock() else {
                    continue;
                };
                (
                    runtime.title.clone(),
                    runtime.lifetime,
                    state.root_registered,
                    state.native.clone(),
                    state.close_requested || state.native_destroyed,
                )
            };
            let is_retired = registry_contains_retired(&self.registry, runtime.pin_id);
            if closing && !is_retired {
                close_ids.push(runtime.pin_id);
            }
            match native_api.lookup(&title, lifetime, existing.as_ref(), root_hwnd) {
                NativeLookup::Found(binding) => {
                    let bounds = native_api.bounds(&binding);
                    if let Ok(mut state) = runtime.state.lock() {
                        state.native = Some(binding);
                        state.native_destroyed = false;
                    }
                    if !closing && let Some(bounds) = bounds {
                        geometry.push((runtime.pin_id, bounds));
                    }
                }
                NativeLookup::Destroyed => {
                    if let Ok(mut state) = runtime.state.lock() {
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
                NativeLookup::Missing | NativeLookup::Unsupported => {
                    // A missing window is not treated as closed unless its exact native
                    // identity was previously bound and Windows confirmed that HWND died.
                }
                NativeLookup::Failed(error) => {
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

        let has_pending_action = self.actions.has_pending_work();
        let has_visibility_work = self.registry.lock().is_ok_and(|registry| {
            registry.hide_identity.is_some()
                || registry.restore_identity.is_some()
                || !registry.retired.is_empty()
        });
        if has_pending_action || has_visibility_work {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
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
            (
                registry.root_context.clone(),
                registry
                    .pins
                    .values()
                    .chain(registry.retired.values())
                    .filter_map(|runtime| {
                        runtime
                            .state
                            .lock()
                            .is_ok_and(|state| !state.native_destroyed && !state.close_command_sent)
                            .then_some(runtime.viewport_id)
                    })
                    .collect::<Vec<_>>(),
            )
        };
        if let Some(ctx) = root_context.filter(|ctx| !ctx.embed_viewports()) {
            for viewport_id in viewport_ids {
                ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Close);
            }
        }
        self.actions.shutdown_and_join()?;
        if let Ok(mut registry) = self.registry.lock() {
            registry.pins.clear();
            registry.retired.clear();
            registry.hide_identity = None;
            registry.restore_identity = None;
            registry.root_context = None;
        }
        Ok(())
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
    let mut should_close = false;
    if let Ok(mut state) = runtime.state.lock() {
        state.close_requested = true;
        state.desired_visible = false;
        if !state.native_destroyed && !state.close_command_sent {
            state.close_command_sent = true;
            should_close = true;
        }
    }
    let viewport_id = runtime.viewport_id;
    registry.retired.insert(runtime.pin_id, runtime);
    should_close.then_some(viewport_id)
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
        .map_or(true, |ticket| ticket.prior_visible != Some(true))
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
            let (title, lifetime, existing, never_registered, desired_visible) = {
                let state = runtime
                    .state
                    .lock()
                    .map_err(|_| "pin window state lock was poisoned".to_owned())?;
                (
                    runtime.title.clone(),
                    runtime.lifetime,
                    state.native.clone(),
                    !state.root_registered && !state.callback_entered && state.native.is_none(),
                    state.desired_visible,
                )
            };
            let mut capture = CaptureVisibilityState {
                identity: Some(identity),
                ..Default::default()
            };
            match native_api.lookup(&title, lifetime, existing.as_ref(), root_identity.0) {
                NativeLookup::Found(binding) => {
                    let prior_visible = native_api.is_visible(&binding)?;
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
            let (title, lifetime, existing, callback_entered, desired_visible, capture) =
                match runtime.state.lock() {
                    Ok(state) => (
                        runtime.title.clone(),
                        runtime.lifetime,
                        state.native.clone(),
                        state.callback_entered,
                        state.desired_visible,
                        state.capture_visibility,
                    ),
                    Err(_) => {
                        return PinVisibilityAck::Failed(
                            "pin window state lock was poisoned".into(),
                        );
                    }
                };
            if capture.is_some_and(|state| state.identity != Some(identity)) {
                return PinVisibilityAck::Failed("pin hide ticket identity changed".into());
            }
            match native_api.lookup(&title, lifetime, existing.as_ref(), root_identity.0) {
                NativeLookup::Found(binding) => {
                    let visible = match native_api.is_visible(&binding) {
                        Ok(visible) => visible,
                        Err(error) => return PinVisibilityAck::Failed(error),
                    };
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
            let (title, lifetime, existing, closed, desired_visible, capture) = {
                let state = runtime
                    .state
                    .lock()
                    .map_err(|_| "pin window state lock was poisoned".to_owned())?;
                (
                    runtime.title.clone(),
                    runtime.lifetime,
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
            match native_api.lookup(&title, lifetime, existing.as_ref(), root_identity.0) {
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
            let (title, lifetime, existing, closed, desired_visible, capture) = match runtime
                .state
                .lock()
            {
                Ok(state) => (
                    runtime.title.clone(),
                    runtime.lifetime,
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
            match native_api.lookup(&title, lifetime, existing.as_ref(), root_identity.0) {
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
            if let Ok(mut registry) = self.registry.lock() {
                if registry.restore_identity == Some(identity) {
                    registry.restore_identity = None;
                    registry.hide_identity = None;
                    for runtime in registry.pins.values().chain(registry.retired.values()) {
                        if let Ok(mut state) = runtime.state.lock() {
                            state.capture_visibility = None;
                        }
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
    egui::ViewportId::from_hash_of(("screenshot-pin", pin_id.get(), lifetime))
}

fn pin_viewport_builder(runtime: &PinWindowRuntime) -> egui::ViewportBuilder {
    let width = runtime.dimensions.0.min(DEFAULT_PINNER_WIDTH as u32) as f32;
    let height = runtime.dimensions.1.min(DEFAULT_PINNER_HEIGHT as u32) as f32;
    let always_on_top = runtime.state.lock().is_ok_and(|state| state.always_on_top);
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
}

fn render_pin_viewport(
    ui: &mut egui::Ui,
    class: egui::ViewportClass,
    runtime: &Arc<PinWindowRuntime>,
    actions: &PinActionClient,
    native_api: &Arc<dyn PinNativeApi>,
    root_native_identity: (usize, u64),
    root_context: &egui::Context,
    capture_suppressed: bool,
) {
    let ctx = ui.ctx().clone();
    if class == egui::ViewportClass::EmbeddedWindow {
        ui.label("Screenshot pins require an independent native window.");
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
    match native_api.lookup(
        &runtime.title,
        runtime.lifetime,
        existing_native.as_ref(),
        root_native_identity.0,
    ) {
        NativeLookup::Found(binding) => {
            if let Ok(mut state) = runtime.state.lock() {
                state.native = Some(binding);
                state.native_destroyed = false;
            }
        }
        NativeLookup::Destroyed => {
            native_destroyed = true;
            if let Ok(mut state) = runtime.state.lock() {
                state.native_destroyed = true;
                state.close_requested = true;
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        NativeLookup::Failed(error) => {
            tracing::warn!(pin_id = runtime.pin_id.get(), %error, "pin child native identity validation failed");
        }
        NativeLookup::Missing | NativeLookup::Unsupported => {}
    }
    if native_destroyed {
        return;
    }
    if capture_suppressed || close_requested {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if close_requested {
            if let Ok(mut state) = runtime.state.lock() {
                state.close_command_sent = true;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        root_context.request_repaint_of(egui::ViewportId::ROOT);
        return;
    }
    if first_pass && escape && click_through {
        click_through = false;
        if let Ok(mut state) = runtime.state.lock() {
            state.click_through = false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
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
            if ui.button("Copy Image").clicked() && first_pass {
                copy_requested = true;
            }
            if ui.button("Save As").clicked() && first_pass {
                save_requested = true;
            }
            if ui
                .selectable_label(always_on_top, "Always on Top")
                .clicked()
                && first_pass
            {
                always_on_top = !always_on_top;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if always_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }));
            }
            if ui
                .selectable_label(click_through, "Click-Through")
                .on_hover_text("Press Escape while this pin is focused to turn click-through off.")
                .clicked()
                && first_pass
            {
                click_through = !click_through;
                ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(click_through));
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
                ui.add(egui::Image::new((texture.id(), image_size)));
            });
    });

    if first_pass {
        let mut request_id = None;
        if copy_requested || save_requested {
            if let Ok(mut state) = runtime.state.lock() {
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
        }
        if let Some(request_id) = request_id {
            if copy_requested {
                submit_pin_action(runtime, actions, request_id, PinActionKind::Copy);
            } else if save_requested {
                submit_pin_action(runtime, actions, request_id, PinActionKind::SaveAs);
            }
        }
        if next_mode != scale_mode || (next_mode == PinScaleMode::Custom && next_zoom != zoom) {
            if let Ok(mut state) = runtime.state.lock() {
                state.scale_mode = next_mode;
                state.zoom = next_zoom.clamp(0.1, 8.0);
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if always_on_top != runtime_state_bool(runtime, true) {
            if let Ok(mut state) = runtime.state.lock() {
                state.always_on_top = always_on_top;
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if click_through != runtime_state_bool(runtime, false) {
            if let Ok(mut state) = runtime.state.lock() {
                state.click_through = click_through;
            }
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
        if close_clicked {
            if let Ok(mut state) = runtime.state.lock() {
                state.close_requested = true;
                state.close_command_sent = true;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            root_context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

fn runtime_state_bool(runtime: &PinWindowRuntime, topmost: bool) -> bool {
    runtime.state.lock().map_or(false, |state| {
        if topmost {
            state.always_on_top
        } else {
            state.click_through
        }
    })
}

fn ensure_pin_texture(
    ctx: &egui::Context,
    runtime: &PinWindowRuntime,
) -> Result<egui::TextureHandle, String> {
    if let Some(texture) = runtime
        .state
        .lock()
        .map_err(|_| "pin texture state lock was poisoned".to_owned())?
        .texture
        .as_ref()
        .cloned()
    {
        return Ok(texture);
    }
    let image = runtime.image.pixels();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [runtime.dimensions.0 as usize, runtime.dimensions.1 as usize],
        image.as_raw(),
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
        Ok(existing.clone())
    } else {
        state.texture = Some(texture.clone());
        Ok(texture)
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
    let job = PinActionJob {
        pin_id: runtime.pin_id,
        lifetime: runtime.lifetime,
        request_id,
        image: runtime.image.clone(),
        kind,
    };
    match client.submit(job) {
        Ok(()) => {}
        Err(error) => set_action_failure(runtime, request_id, error),
    }
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
    root_context: Mutex<Option<egui::Context>>,
    save_picker: Arc<dyn PinSavePicker>,
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
                root_context: Mutex::new(None),
                save_picker,
            }),
            completions: receiver,
        }
    }

    fn client(&self) -> PinActionClient {
        PinActionClient {
            shared: Arc::clone(&self.shared),
        }
    }

    fn set_root_context(&self, ctx: egui::Context) {
        if let Ok(mut root_context) = self.shared.root_context.lock() {
            *root_context = Some(ctx);
        }
    }

    fn drain_completions(&self) -> Vec<PinActionCompletion> {
        let mut completions = Vec::new();
        loop {
            match self.completions.try_recv() {
                Ok(completion) => completions.push(completion),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
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
            let active_picker = state
                .active_job
                .is_some_and(|(active_pin, _, _)| active_pin == pin_id)
                .then(|| state.active_picker.clone())
                .flatten();
            drop(state);
            if let Some(cancellation) = active_picker {
                cancellation.cancel();
            }
        }
    }

    fn has_pending_work(&self) -> bool {
        self.shared
            .state
            .lock()
            .is_ok_and(|state| state.active || !state.queue.is_empty())
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
        if let Ok(mut root_context) = self.shared.root_context.lock() {
            *root_context = None;
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
            let Ok(mut state) = shared.state.lock() else {
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
        let result = run_pin_action_with_save_picker(&job, || {
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
        if let Ok(root_context) = shared.root_context.lock()
            && let Some(ctx) = root_context.as_ref()
        {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

struct NativePinSavePicker;

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
            return native::save_pin_image(pin_id, cancellation);
        }
        #[cfg(not(windows))]
        {
            let _ = pin_id;
            Err("Save As picker is unsupported on this platform".into())
        }
    }
}

fn run_pin_action_with_save_picker(
    job: &PinActionJob,
    save_picker: impl FnOnce() -> Result<Option<PathBuf>, String>,
) -> Result<Option<String>, String> {
    match &job.kind {
        PinActionKind::Copy => {
            let image = job.image.pixels();
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|error| format!("could not open the image clipboard: {error}"))?;
            clipboard
                .set_image(clipboard_image_data(image))
                .map_err(|error| format!("could not copy screenshot pin: {error}"))?;
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
    use crate::pinboard::PinCaptureOperationId;
    use image::{Rgba, RgbaImage};
    use std::sync::mpsc;

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
            if !cancellation.wait_for_cancel_calls(1, std::time::Duration::from_secs(5)) {
                return Err("fixture picker timed out awaiting close cancellation".into());
            }
            let _ = self.close_seen.send(());
            if !cancellation.wait_for_cancel_calls(2, std::time::Duration::from_secs(5)) {
                return Err("fixture picker timed out awaiting shutdown cancellation".into());
            }
            Ok(None)
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

    fn pin_callbacks(
        ctx: &egui::Context,
        presenter: &mut PinWindowPresenter,
        board: &Pinboard,
    ) -> BTreeMap<PinId, Arc<egui::DeferredViewportUiCallback>> {
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
    fn native_hide_ack_and_bounded_worker_preserve_close_and_cancel_lifetimes() {
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
            image: board.get(first).unwrap().snapshot().image_handle(),
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
        let cancelled_after_close = close_token.cancel_count() == 1
            && presenter.actions.shared.state.lock().is_ok_and(|state| {
                state.active && state.queue.is_empty() && state.admitted_jobs == 1
            });
        let close_cancellation_reached_picker = close_seen_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok();
        assert!(queue_rejected_overflow);
        assert!(submit_ok);
        assert_eq!(picker_started.unwrap(), 0);
        assert!(queue_at_limit);
        assert!(cancelled_after_close);
        assert!(close_cancellation_reached_picker);
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
        assert_eq!(close_token.cancel_count(), 2);
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

        // The active picker observes close cancellation and remains blocked
        // until the presenter's worker shutdown sends its separate signal.
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
            image: board.get(second).unwrap().snapshot().image_handle(),
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
            image: board.get(second).unwrap().snapshot().image_handle(),
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
        let second_pixels = board.get(second).unwrap().snapshot().pixels();
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
                    image: board.get(second).unwrap().snapshot().image_handle(),
                    kind: PinActionKind::SaveAs,
                })
                .unwrap();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while backpressure_worker.has_pending_work() && std::time::Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(!backpressure_worker.has_pending_work());
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
                    image: board.get(second).unwrap().snapshot().image_handle(),
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
