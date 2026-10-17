use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::model::{
    ClickEffectStatus, ClickPressEvent, ClickPressSink, CoordinateEffectsStatus, CoordinateSample,
    CoordinateToolRuntimeState, FormattedCoordinate,
};
use super::render::{CLICK_ANIMATION_INTERVAL, ClickEffectFrame, ClickEffectManager};
use super::settings::{ClickEffectPreferences, CoordinateToolPreferences};
use crate::mouse_gestures::service::{ClickObservationGuard, acquire_click_observation};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(16);
const CLICK_QUEUE_CAPACITY: usize = 64;
const CLICK_PENDING_CAPACITY: usize = 64;

pub trait CoordinateSampler {
    fn sample(&mut self) -> Result<CoordinateSample, String>;
}

pub trait CoordinateSurfaceBackend {
    /// Pump the worker's window queue. `true` requests a full refresh after a
    /// display/DPI notification even when the sampled cursor is unchanged.
    fn poll_events(&mut self) -> Result<bool, String> {
        Ok(false)
    }

    /// Refresh current native effect sources for an unchanged successful
    /// frame, without redrawing the HUD or other cheap overlays.
    ///
    /// The worker calls this only after a successful sample when the frame is
    /// equal to the last successfully rendered frame and no forced refresh or
    /// backend retry is pending. Backends without independently animated
    /// sources may keep the default no-op implementation.
    fn refresh_stationary_sources(&mut self, _frame: &CoordinateRenderFrame) -> Result<(), String> {
        Ok(())
    }

    fn effects_status(&self) -> CoordinateEffectsStatus {
        CoordinateEffectsStatus::default()
    }

    /// Present the current bounded click animation on the same worker-owned
    /// passive surface backend. Empty input clears any visible click surfaces.
    fn render_click_effects(&mut self, _frames: &[ClickEffectFrame]) -> Result<(), String> {
        Ok(())
    }

    fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String>;
    fn shutdown(&mut self) -> Result<(), String>;
}

/// Native resources are created and used only from the coordinate worker.
pub trait CoordinateRuntimeFactory: Send + Sync + 'static {
    fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String>;
    fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String>;
}

pub trait ClickObservationLease: Send {}

pub trait ClickObservationProvider: Send + Sync + 'static {
    fn acquire(
        &self,
        generation: u64,
        sink: Arc<dyn ClickPressSink>,
    ) -> Result<Box<dyn ClickObservationLease>, String>;
}

#[derive(Debug, Default)]
pub struct MouseGestureClickObservationProvider;

impl ClickObservationLease for ClickObservationGuard {}

impl ClickObservationProvider for MouseGestureClickObservationProvider {
    fn acquire(
        &self,
        generation: u64,
        sink: Arc<dyn ClickPressSink>,
    ) -> Result<Box<dyn ClickObservationLease>, String> {
        acquire_click_observation(generation, sink)
            .map(|guard| Box::new(guard) as Box<dyn ClickObservationLease>)
            .map_err(|error| format!("Could not enable click visualization: {error}"))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoordinateRenderFrame {
    pub preferences: CoordinateToolPreferences,
    pub runtime_state: CoordinateToolRuntimeState,
    /// A sample returned successfully in this update. `None` means the sampler
    /// failed; crosshair, halo, and zoom effects must consume only this live
    /// sample, never the frozen HUD display or last-known placement sample.
    pub current_sample: Option<CoordinateSample>,
    /// The frozen sample, or the current sample when not frozen.
    pub displayed_sample: Option<CoordinateSample>,
    /// Last successful sample, used only to keep the HUD near its previous
    /// location while a failure message is displayed.
    pub placement_sample: Option<CoordinateSample>,
    pub sample_error: Option<String>,
}

#[derive(Debug, Default)]
struct SharedState {
    preferences: CoordinateToolPreferences,
    runtime: CoordinateToolRuntimeState,
    latest_sample: Option<CoordinateSample>,
    pending_freeze: bool,
    sample_error: Option<String>,
    backend_error: Option<String>,
    effects_status: CoordinateEffectsStatus,
    click_generation: u64,
    click_presses: VecDeque<ClickPressEvent>,
}

enum WorkerMessage {
    Shutdown,
    ClickPress {
        event: ClickPressEvent,
        appearance: ClickEffectPreferences,
    },
    ClearClickEffects,
    ResetClickEffects {
        generation: u64,
    },
    SetCursorModes {
        enabled: bool,
        response: mpsc::SyncSender<Result<(), String>>,
    },
}

struct WorkerHandle {
    sender: SyncSender<WorkerMessage>,
    join: JoinHandle<Result<(), String>>,
    cursor_modes: bool,
}

struct WorkerClickPressSink {
    sender: SyncSender<WorkerMessage>,
    shared: Arc<Mutex<SharedState>>,
}

impl ClickPressSink for WorkerClickPressSink {
    fn try_send(&self, event: ClickPressEvent) -> bool {
        // Hook callbacks never wait on application state. If preferences are
        // being committed concurrently, this visual-only event is dropped.
        let appearance = match self.shared.try_lock() {
            Ok(shared) => shared.preferences.click,
            Err(TryLockError::Poisoned(_)) | Err(TryLockError::WouldBlock) => return false,
        };
        self.sender
            .try_send(WorkerMessage::ClickPress { event, appearance })
            .is_ok()
    }
}

/// Coordinates the HUD, cursor effects, click ingress, and owns their single worker.
///
/// Click-only mode owns the existing passive surface backend, but no cursor
/// sampler; while idle it blocks on its bounded event queue. Turning off the
/// final mode synchronously joins the worker.
pub struct CoordinateToolController {
    factory: Arc<dyn CoordinateRuntimeFactory>,
    click_observation: Arc<dyn ClickObservationProvider>,
    shared: Arc<Mutex<SharedState>>,
    worker: Option<WorkerHandle>,
    click_lease: Option<Box<dyn ClickObservationLease>>,
}

impl CoordinateToolController {
    pub fn new(factory: Arc<dyn CoordinateRuntimeFactory>) -> Self {
        Self::new_with_preferences(factory, CoordinateToolPreferences::default())
    }

    pub fn new_with_preferences(
        factory: Arc<dyn CoordinateRuntimeFactory>,
        preferences: CoordinateToolPreferences,
    ) -> Self {
        Self::new_with_preferences_and_click_observation(
            factory,
            preferences,
            Arc::new(MouseGestureClickObservationProvider),
        )
    }

    pub fn new_with_preferences_and_click_observation(
        factory: Arc<dyn CoordinateRuntimeFactory>,
        preferences: CoordinateToolPreferences,
        click_observation: Arc<dyn ClickObservationProvider>,
    ) -> Self {
        let mut shared = SharedState::default();
        shared.preferences = preferences.normalized();
        Self {
            factory,
            click_observation,
            shared: Arc::new(Mutex::new(shared)),
            worker: None,
            click_lease: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub fn set_hud_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.set_mode(
            enabled,
            CoordinateToolRuntimeState::hud_enabled,
            CoordinateToolRuntimeState::set_hud_enabled,
        )
    }

    pub fn set_crosshair_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.set_mode(
            enabled,
            CoordinateToolRuntimeState::crosshair_enabled,
            CoordinateToolRuntimeState::set_crosshair_enabled,
        )
    }

    pub fn set_halo_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.set_mode(
            enabled,
            CoordinateToolRuntimeState::halo_enabled,
            CoordinateToolRuntimeState::set_halo_enabled,
        )
    }

    pub fn set_zoom_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.set_mode(
            enabled,
            CoordinateToolRuntimeState::zoom_enabled,
            CoordinateToolRuntimeState::set_zoom_enabled,
        )
    }

    /// Disable cursor and click effects without changing HUD or inspector
    /// state. The shared worker remains active if the HUD is still enabled.
    pub fn disable_effects(&mut self) -> Result<(), String> {
        let had_click = {
            let mut shared = lock(&self.shared);
            let had_click = shared.runtime.click_enabled();
            shared.runtime.disable_effects();
            shared.click_generation = shared.click_generation.wrapping_add(1);
            shared.click_presses.clear();
            shared.effects_status.set_click(ClickEffectStatus::Disabled);
            had_click
        };
        self.click_lease.take();
        if had_click {
            self.send_click_clear();
        }
        self.reconcile_worker()
    }

    /// Toggle session-only click visualization and acquire or release its
    /// passive lease on the shared mouse hook.
    pub fn set_click_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let (previous, generation) = {
            let mut shared = lock(&self.shared);
            let previous = shared.runtime.click_enabled();
            if previous == enabled {
                return Ok(());
            }
            shared.click_generation = shared.click_generation.wrapping_add(1);
            shared.click_presses.clear();
            shared.runtime.set_click_enabled(enabled);
            shared.effects_status.set_click(if enabled {
                ClickEffectStatus::Idle
            } else {
                ClickEffectStatus::Disabled
            });
            (previous, shared.click_generation)
        };

        if !enabled {
            // Invalidate before unregistering so any callback already holding
            // the old sink can only enqueue a stale generation.
            self.click_lease.take();
            self.send_click_clear();
            return self.reconcile_worker();
        }

        if let Err(error) = self.reconcile_worker() {
            self.rollback_click_enable(previous);
            return Err(error);
        }

        let Some(sender) = self.worker.as_ref().map(|worker| worker.sender.clone()) else {
            self.rollback_click_enable(previous);
            return Err("Coordinate worker did not start for click visualization".into());
        };
        let _ = sender.try_send(WorkerMessage::ResetClickEffects { generation });
        let sink: Arc<dyn ClickPressSink> = Arc::new(WorkerClickPressSink {
            sender,
            shared: Arc::clone(&self.shared),
        });
        match self.click_observation.acquire(generation, sink) {
            Ok(lease) => {
                self.click_lease = Some(lease);
                Ok(())
            }
            Err(error) => {
                self.rollback_click_enable(previous);
                Err(error)
            }
        }
    }

    fn rollback_click_enable(&mut self, previous: bool) {
        {
            let mut shared = lock(&self.shared);
            shared.runtime.set_click_enabled(previous);
            shared.click_generation = shared.click_generation.wrapping_add(1);
            shared.click_presses.clear();
            shared.effects_status.set_click(ClickEffectStatus::Disabled);
        }
        self.click_lease.take();
        if let Err(error) = self.reconcile_worker() {
            tracing::error!(
                ?error,
                "failed to stop click worker after observer setup failed"
            );
        }
    }

    /// Drain physical press events that have passed generation and queue checks.
    /// The renderer can consume them without consulting sampled cursor state.
    pub fn take_click_presses(&self) -> Vec<ClickPressEvent> {
        lock(&self.shared).click_presses.drain(..).collect()
    }

    fn set_mode(
        &mut self,
        enabled: bool,
        is_enabled: fn(&CoordinateToolRuntimeState) -> bool,
        set_enabled: fn(&mut CoordinateToolRuntimeState, bool),
    ) -> Result<(), String> {
        let previous = {
            let mut shared = lock(&self.shared);
            let previous = is_enabled(&shared.runtime);
            set_enabled(&mut shared.runtime, enabled);
            previous
        };
        if let Err(error) = self.reconcile_worker() {
            let mut shared = lock(&self.shared);
            let still_requested_active = shared.runtime.has_active_mode();
            if still_requested_active {
                set_enabled(&mut shared.runtime, previous);
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn set_preferences(
        &mut self,
        preferences: CoordinateToolPreferences,
    ) -> Result<(), String> {
        lock(&self.shared).preferences = preferences.normalized();
        self.reconcile_worker()
    }

    /// Freeze the latest sample. If the worker has not sampled yet, freezing
    /// takes effect on its first successful sample.
    pub fn freeze(&self) {
        let mut shared = lock(&self.shared);
        if shared.runtime.is_frozen() {
            return;
        }
        if let Some(sample) = shared.latest_sample.clone() {
            shared.runtime.freeze(&sample);
        } else {
            shared.pending_freeze = true;
        }
    }

    pub fn unfreeze(&self) {
        let mut shared = lock(&self.shared);
        shared.pending_freeze = false;
        shared.runtime.unfreeze();
    }

    /// Record a value after an external clipboard write succeeds.
    pub fn record_successful_copy(&self, copied: FormattedCoordinate) {
        lock(&self.shared).runtime.record_successful_copy(copied);
    }

    /// Return the sample represented by the HUD for copying. Frozen data is
    /// intentionally stable; live data is available only while sampling is
    /// active and after a successful current observation.
    pub fn sample_for_copy(&self) -> Result<CoordinateSample, String> {
        let shared = lock(&self.shared);
        if let Some(sample) = shared.runtime.frozen_sample() {
            return Ok(sample.clone());
        }
        if self.worker.is_none() {
            return Err(
                "No live coordinate sample. Enable the HUD or a cursor effect first.".into(),
            );
        }
        shared.latest_sample.clone().ok_or_else(|| {
            shared
                .sample_error
                .clone()
                .unwrap_or_else(|| "No current coordinate sample is available yet.".into())
        })
    }

    pub fn runtime_state(&self) -> CoordinateToolRuntimeState {
        lock(&self.shared).runtime.clone()
    }

    pub fn preferences(&self) -> CoordinateToolPreferences {
        lock(&self.shared).preferences.clone()
    }

    pub fn last_error(&self) -> Option<String> {
        let shared = lock(&self.shared);
        shared
            .backend_error
            .clone()
            .or_else(|| shared.sample_error.clone())
    }

    /// Current native halo and zoom lifecycle status, refreshed independently
    /// of whether a cheap HUD/crosshair frame needed rendering.
    pub fn effects_status(&self) -> CoordinateEffectsStatus {
        lock(&self.shared).effects_status.clone()
    }

    /// Stop and join the passive worker. The join completes only after backend
    /// shutdown has attempted to close all native resources.
    pub fn shutdown(&mut self) -> Result<(), String> {
        {
            let mut shared = lock(&self.shared);
            shared.runtime.set_hud_enabled(false);
            shared.runtime.disable_effects();
            shared.pending_freeze = false;
            shared.click_generation = shared.click_generation.wrapping_add(1);
            shared.click_presses.clear();
            shared.effects_status.set_click(ClickEffectStatus::Disabled);
        }
        self.click_lease.take();
        self.stop_worker()
    }

    fn reconcile_worker(&mut self) -> Result<(), String> {
        let (active, cursor_modes) = {
            let shared = lock(&self.shared);
            (
                shared.runtime.has_active_mode(),
                shared.runtime.has_cursor_mode(),
            )
        };
        match (active, self.worker.as_ref()) {
            (true, None) => self.start_worker(cursor_modes),
            (false, Some(_)) => self.stop_worker(),
            (true, Some(worker)) if worker.cursor_modes != cursor_modes => {
                self.configure_cursor_modes(cursor_modes)
            }
            _ => Ok(()),
        }
    }

    fn send_click_clear(&self) {
        if let Some(worker) = &self.worker {
            // If the bounded ingress queue is full, the worker also observes
            // the invalidated generation and clears the visual state before
            // handling any stale event.
            let _ = worker.sender.try_send(WorkerMessage::ClearClickEffects);
        }
    }

    fn start_worker(&mut self, cursor_modes: bool) -> Result<(), String> {
        let factory = Arc::clone(&self.factory);
        let shared = Arc::clone(&self.shared);
        let (sender, receiver) = mpsc::sync_channel(CLICK_QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("coordinate-tool-passive".into())
            .spawn(move || run_worker(factory, receiver, ready_sender, shared, cursor_modes))
            .map_err(|error| format!("Could not start coordinate worker: {error}"))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => {
                self.worker = Some(WorkerHandle {
                    sender,
                    join,
                    cursor_modes,
                });
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(error) => {
                let _ = join.join();
                Err(format!("Coordinate worker stopped during setup: {error}"))
            }
        }
    }

    fn configure_cursor_modes(&mut self, enabled: bool) -> Result<(), String> {
        let worker = self
            .worker
            .as_mut()
            .ok_or_else(|| "Coordinate worker is not running".to_string())?;
        let (response, receiver) = mpsc::sync_channel(1);
        worker
            .sender
            .send(WorkerMessage::SetCursorModes { enabled, response })
            .map_err(|error| format!("Could not update coordinate worker: {error}"))?;
        let result = receiver
            .recv()
            .map_err(|error| format!("Coordinate worker stopped during update: {error}"))?;
        if result.is_ok() || !enabled {
            worker.cursor_modes = enabled;
        }
        result
    }

    fn stop_worker(&mut self) -> Result<(), String> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let _ = worker.sender.send(WorkerMessage::Shutdown);
        let result = worker
            .join
            .join()
            .map_err(|_| "Coordinate worker panicked during shutdown".to_string())?;
        let mut shared = lock(&self.shared);
        shared.latest_sample = None;
        shared.pending_freeze = false;
        shared.sample_error = None;
        shared.click_presses.clear();
        result
    }
}

impl Drop for CoordinateToolController {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn run_worker(
    factory: Arc<dyn CoordinateRuntimeFactory>,
    receiver: Receiver<WorkerMessage>,
    ready: mpsc::SyncSender<Result<(), String>>,
    shared: Arc<Mutex<SharedState>>,
    mut cursor_modes: bool,
) -> Result<(), String> {
    let (mut sampler, mut backend) = if cursor_modes {
        match create_cursor_runtime(factory.as_ref()) {
            Ok((sampler, backend)) => (Some(sampler), Some(backend)),
            Err(error) => {
                let _ = ready.send(Err(error.clone()));
                return Err(error);
            }
        }
    } else {
        match factory.create_backend() {
            Ok(backend) => (None, Some(backend)),
            Err(error) => {
                let _ = ready.send(Err(error.clone()));
                return Err(error);
            }
        }
    };
    if ready.send(Ok(())).is_err() {
        return shutdown_cursor_runtime(&mut sampler, &mut backend, &shared);
    }

    let mut last_good_sample: Option<CoordinateSample> = None;
    let mut last_frame: Option<CoordinateRenderFrame> = None;
    let mut click_effects = ClickEffectManager::default();
    let mut click_render_error: Option<(u64, String)> = None;
    loop {
        let click_animation_active = click_effects.is_active();
        let received = if cursor_modes || click_animation_active {
            receiver
                .recv_timeout(SAMPLE_INTERVAL.min(CLICK_ANIMATION_INTERVAL))
                .map_err(|error| match error {
                    mpsc::RecvTimeoutError::Timeout => "timeout",
                    mpsc::RecvTimeoutError::Disconnected => "disconnected",
                })
        } else {
            receiver.recv().map_err(|_| "disconnected")
        };

        match received {
            Err("timeout") => {}
            Err("disconnected") | Ok(WorkerMessage::Shutdown) => break,
            Ok(WorkerMessage::ClickPress { event, appearance }) => {
                if record_click_press(&shared, event) {
                    clear_click_error_for_generation(&mut click_render_error, event.generation);
                    if click_render_error.is_none() {
                        let retired_old_generation =
                            retire_click_generation(&mut click_effects, event.generation);
                        let admitted = click_effects.admit(event, appearance, Instant::now());
                        // A delayed or invalid new-generation press may be
                        // discarded. In that case, explicitly hide the retired
                        // generation instead of leaving its last frame onscreen.
                        if retired_old_generation
                            && !admitted
                            && let Some(backend) = backend.as_mut()
                            && let Err(error) = backend.render_click_effects(&[])
                        {
                            click_render_error = Some((event.generation, error.clone()));
                            tracing::warn!(?error, "click visualization became unavailable");
                        }
                    }
                }
            }
            Ok(WorkerMessage::ClearClickEffects) => {
                click_effects.clear();
                if let Some(backend) = backend.as_mut() {
                    let _ = backend.render_click_effects(&[]);
                }
            }
            Ok(WorkerMessage::ResetClickEffects { generation }) => {
                if lock(&shared).click_generation == generation {
                    if click_effects.clear()
                        && let Some(backend) = backend.as_mut()
                    {
                        let _ = backend.render_click_effects(&[]);
                    }
                    clear_click_error_for_generation(&mut click_render_error, generation);
                }
            }
            Ok(WorkerMessage::SetCursorModes { enabled, response }) => {
                let result = if enabled == cursor_modes {
                    Ok(())
                } else if enabled {
                    match factory.create_sampler() {
                        Ok(new_sampler) => {
                            sampler = Some(new_sampler);
                            cursor_modes = true;
                            last_good_sample = None;
                            last_frame = None;
                            Ok(())
                        }
                        Err(error) => {
                            lock(&shared).backend_error = Some(error.clone());
                            Err(error)
                        }
                    }
                } else {
                    sampler.take();
                    cursor_modes = false;
                    last_good_sample = None;
                    last_frame = None;
                    {
                        let mut shared = lock(&shared);
                        shared.latest_sample = None;
                        shared.pending_freeze = false;
                        shared.sample_error = None;
                    }
                    if let Some(backend) = backend.as_mut()
                        && let Err(error) = backend.render(&empty_coordinate_frame(&shared))
                    {
                        lock(&shared).backend_error = Some(error);
                    }
                    update_cursor_effect_status(&shared, backend.as_ref());
                    Ok(())
                };
                let _ = response.send(result);
            }
            Err(_) => break,
        }

        let (click_enabled, click_generation) = {
            let shared = lock(&shared);
            (shared.runtime.click_enabled(), shared.click_generation)
        };
        clear_click_error_for_generation(&mut click_render_error, click_generation);
        if (!click_enabled
            || click_effects
                .active_generation()
                .is_some_and(|generation| generation != click_generation))
            && click_effects.clear()
        {
            if let Some(backend) = backend.as_mut() {
                let _ = backend.render_click_effects(&[]);
            }
        }

        if click_enabled && click_render_error.is_none() {
            let had_effects = click_effects.is_active();
            let click_frames = click_effects.frames_at(Instant::now());
            if had_effects || !click_frames.is_empty() {
                let click_only = !cursor_modes;
                if click_only
                    && let Some(backend) = backend.as_mut()
                    && let Err(error) = backend.poll_events()
                {
                    tracing::warn!(?error, "click visualization window message pump failed");
                }
                if let Some(backend) = backend.as_mut()
                    && let Err(error) = backend.render_click_effects(&click_frames)
                {
                    click_render_error = Some((click_generation, error.clone()));
                    click_effects.clear();
                    let _ = backend.render_click_effects(&[]);
                    tracing::warn!(?error, "click visualization became unavailable");
                }
            }
        }
        update_click_status(
            &shared,
            click_enabled,
            click_effects.is_active(),
            click_render_error.as_ref().map(|(_, error)| error.as_str()),
        );

        if !cursor_modes {
            continue;
        }

        let (Some(sampler), Some(backend)) = (sampler.as_mut(), backend.as_mut()) else {
            continue;
        };

        let force_refresh = match backend.poll_events() {
            Ok(force_refresh) => force_refresh,
            Err(error) => {
                lock(&shared).backend_error = Some(error);
                false
            }
        };
        let sample_result = sampler.sample();
        let current_sample = sample_result.as_ref().ok().cloned();
        let sample_error = sample_result.err();
        let frame = {
            let mut shared = lock(&shared);
            if let Some(sample) = current_sample.as_ref() {
                last_good_sample = Some(sample.clone());
                shared.latest_sample = Some(sample.clone());
                shared.sample_error = None;
                if shared.pending_freeze {
                    shared.runtime.freeze(sample);
                    shared.pending_freeze = false;
                }
            } else {
                // A prior successful point remains useful only for HUD
                // placement. Expose no stale live sample to freeze or any
                // later click-time copy path after this observation failed.
                shared.latest_sample = None;
                shared.sample_error = sample_error.clone();
            }
            let runtime_state = shared.runtime.clone();
            let displayed_sample = runtime_state
                .frozen_sample()
                .cloned()
                .or_else(|| current_sample.clone());
            CoordinateRenderFrame {
                preferences: shared.preferences.clone(),
                runtime_state,
                current_sample: current_sample.clone(),
                displayed_sample,
                placement_sample: current_sample.clone().or_else(|| last_good_sample.clone()),
                sample_error,
            }
        };

        let needs_retry = lock(&shared).backend_error.is_some();
        if force_refresh || last_frame.as_ref() != Some(&frame) || needs_retry {
            match backend.render(&frame) {
                Ok(()) => {
                    last_frame = Some(frame);
                    lock(&shared).backend_error = None;
                }
                Err(error) => lock(&shared).backend_error = Some(error),
            }
        } else if frame.current_sample.is_some()
            && let Err(error) = backend.refresh_stationary_sources(&frame)
        {
            lock(&shared).backend_error = Some(error);
        }
        update_cursor_effect_status(&shared, Some(&backend));
    }

    shutdown_cursor_runtime(&mut sampler, &mut backend, &shared)
}

fn empty_coordinate_frame(shared: &Arc<Mutex<SharedState>>) -> CoordinateRenderFrame {
    let shared = lock(shared);
    CoordinateRenderFrame {
        preferences: shared.preferences.clone(),
        runtime_state: shared.runtime.clone(),
        current_sample: None,
        displayed_sample: None,
        placement_sample: None,
        sample_error: None,
    }
}

fn update_click_status(
    shared: &Arc<Mutex<SharedState>>,
    enabled: bool,
    animating: bool,
    error: Option<&str>,
) {
    let status = if !enabled {
        ClickEffectStatus::Disabled
    } else if let Some(error) = error {
        ClickEffectStatus::Unavailable(error.to_string())
    } else if animating {
        ClickEffectStatus::Animating
    } else {
        ClickEffectStatus::Idle
    };
    lock(shared).effects_status.set_click(status);
}

/// Retire old animation markers before admitting a valid press from a newer
/// session generation. This closes the off/on race where a dropped clear
/// message could otherwise let the worker's later generation sweep erase the
/// new press along with the old animation.
fn retire_click_generation(click_effects: &mut ClickEffectManager, generation: u64) -> bool {
    click_effects
        .active_generation()
        .is_some_and(|active_generation| active_generation != generation)
        && click_effects.clear()
}

fn clear_click_error_for_generation(error: &mut Option<(u64, String)>, generation: u64) {
    if error
        .as_ref()
        .is_some_and(|(failed_generation, _)| *failed_generation != generation)
    {
        *error = None;
    }
}

fn update_cursor_effect_status(
    shared: &Arc<Mutex<SharedState>>,
    backend: Option<&Box<dyn CoordinateSurfaceBackend>>,
) {
    let Some(backend) = backend else {
        return;
    };
    let click = lock(shared).effects_status.click().clone();
    let mut status = backend.effects_status();
    status.set_click(click);
    lock(shared).effects_status = status;
}

fn create_cursor_runtime(
    factory: &dyn CoordinateRuntimeFactory,
) -> Result<
    (
        Box<dyn CoordinateSampler>,
        Box<dyn CoordinateSurfaceBackend>,
    ),
    String,
> {
    // The sampler may install a thread DPI context nested inside the backend's
    // context. Create the backend first so dropping the sampler restores the
    // backend context while its native surfaces remain alive.
    let mut backend = factory.create_backend()?;
    match factory.create_sampler() {
        Ok(sampler) => Ok((sampler, backend)),
        Err(error) => {
            if let Err(shutdown_error) = backend.shutdown() {
                tracing::warn!(
                    ?shutdown_error,
                    "backend cleanup failed after sampler setup"
                );
            }
            drop(backend);
            Err(error)
        }
    }
}

fn shutdown_cursor_runtime(
    sampler: &mut Option<Box<dyn CoordinateSampler>>,
    backend: &mut Option<Box<dyn CoordinateSurfaceBackend>>,
    shared: &Arc<Mutex<SharedState>>,
) -> Result<(), String> {
    // Sampler DPI state is nested inside the backend's window DPI context.
    // Restore the backend context before shutting down its native resources.
    drop(sampler.take());
    let shutdown = backend
        .take()
        .map(|mut backend| {
            let result = backend.shutdown();
            lock(shared).effects_status = backend.effects_status();
            result
        })
        .unwrap_or(Ok(()));
    let mut shared = lock(shared);
    shared.latest_sample = None;
    shared.pending_freeze = false;
    shared.sample_error = None;
    shutdown
}

fn record_click_press(shared: &Arc<Mutex<SharedState>>, event: ClickPressEvent) -> bool {
    let mut shared = lock(shared);
    if !shared.runtime.click_enabled() || shared.click_generation != event.generation {
        return false;
    }
    // Keep only the newest bounded ingress history if the UI/test observer is
    // slower than a visual-only click burst.
    if shared.click_presses.len() == CLICK_PENDING_CAPACITY {
        shared.click_presses.pop_front();
    }
    shared.click_presses.push_back(event);
    true
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use super::{
        CLICK_QUEUE_CAPACITY, ClickEffectManager, ClickEffectPreferences, ClickObservationLease,
        ClickObservationProvider, CoordinateRenderFrame, CoordinateRuntimeFactory,
        CoordinateSampler, CoordinateSurfaceBackend, CoordinateToolController,
        create_cursor_runtime, retire_click_generation, shutdown_cursor_runtime,
    };
    use crate::coordinate_tool::model::{
        ClickButton, ClickPressEvent, ClickPressSink, CoordinateEffectsStatus, CoordinateSample,
        CursorEffectStatus, ForegroundClientGeometry, MonitorGeometry, MonitorId, PhysicalPoint,
        PhysicalRect,
    };
    use crate::coordinate_tool::render::ClickEffectFrame;
    use crate::mouse_gestures::overlay::OverlayBackend;
    use crate::mouse_gestures::service::{
        ClickObservationGuard, CursorPositionProvider, MockHookBackend, MouseGestureConfig,
        MouseGestureService, NoMatchBehavior, OverlayFactory, RightClickBackend,
    };

    struct InertGestureOverlay;

    impl OverlayBackend for InertGestureOverlay {
        fn draw_trail_segment(
            &mut self,
            _from: (f32, f32),
            _to: (f32, f32),
            _color: [u8; 4],
            _width: f32,
        ) {
        }

        fn clear_trail(&mut self) {}

        fn show_hint(&mut self, _text: &str, _position: (f32, f32)) {}

        fn hide_hint(&mut self) {}
    }

    struct InertGestureOverlayFactory;

    impl OverlayFactory for InertGestureOverlayFactory {
        fn trail_backend(&self) -> Box<dyn OverlayBackend> {
            Box::new(InertGestureOverlay)
        }

        fn hint_backend(&self) -> Box<dyn OverlayBackend> {
            Box::new(InertGestureOverlay)
        }
    }

    struct InertRightClickBackend;

    impl RightClickBackend for InertRightClickBackend {
        fn send_right_click(&self) {}
    }

    struct FixedGestureCursor;

    impl CursorPositionProvider for FixedGestureCursor {
        fn cursor_position(&self) -> Option<(f32, f32)> {
            Some((0.0, 0.0))
        }
    }

    struct LocalGestureClickObservationProvider {
        service: Arc<Mutex<MouseGestureService>>,
    }

    impl ClickObservationProvider for LocalGestureClickObservationProvider {
        fn acquire(
            &self,
            generation: u64,
            sink: Arc<dyn ClickPressSink>,
        ) -> Result<Box<dyn ClickObservationLease>, String> {
            ClickObservationGuard::acquire(Arc::clone(&self.service), generation, sink)
                .map(|guard| Box::new(guard) as Box<dyn ClickObservationLease>)
                .map_err(|error| error.to_string())
        }
    }

    #[derive(Clone)]
    struct FakeClickObservation {
        active: Arc<Mutex<Option<(u64, Arc<dyn ClickPressSink>)>>>,
        acquisitions: Arc<AtomicUsize>,
        releases: Arc<AtomicUsize>,
        failure: Option<String>,
    }

    impl FakeClickObservation {
        fn new(failure: Option<&str>) -> Self {
            Self {
                active: Arc::new(Mutex::new(None)),
                acquisitions: Arc::new(AtomicUsize::new(0)),
                releases: Arc::new(AtomicUsize::new(0)),
                failure: failure.map(str::to_string),
            }
        }

        fn current(&self) -> Option<(u64, Arc<dyn ClickPressSink>)> {
            self.active.lock().unwrap().clone()
        }
    }

    struct FakeClickLease {
        active: Arc<Mutex<Option<(u64, Arc<dyn ClickPressSink>)>>>,
        releases: Arc<AtomicUsize>,
        generation: u64,
    }

    impl ClickObservationLease for FakeClickLease {}

    impl Drop for FakeClickLease {
        fn drop(&mut self) {
            let mut active = self.active.lock().unwrap();
            if active
                .as_ref()
                .is_some_and(|(generation, _)| *generation == self.generation)
            {
                *active = None;
            }
            self.releases.fetch_add(1, Ordering::AcqRel);
        }
    }

    impl ClickObservationProvider for FakeClickObservation {
        fn acquire(
            &self,
            generation: u64,
            sink: Arc<dyn ClickPressSink>,
        ) -> Result<Box<dyn ClickObservationLease>, String> {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            *self.active.lock().unwrap() = Some((generation, sink));
            self.acquisitions.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(FakeClickLease {
                active: Arc::clone(&self.active),
                releases: Arc::clone(&self.releases),
                generation,
            }))
        }
    }

    #[derive(Clone)]
    struct FakeFactory {
        sampler_creations: Arc<AtomicUsize>,
        backend_creations: Arc<AtomicUsize>,
        sample_calls: Arc<AtomicUsize>,
        shutdowns: Arc<AtomicUsize>,
        samples: Arc<Mutex<VecDeque<Result<CoordinateSample, String>>>>,
        fallback: Result<CoordinateSample, String>,
        backend_failure: Option<String>,
        effects_status: Arc<Mutex<CoordinateEffectsStatus>>,
        stationary_frames: Arc<Mutex<Vec<CoordinateRenderFrame>>>,
        click_render_calls: Arc<AtomicUsize>,
        click_renders: Arc<Mutex<Vec<Vec<ClickEffectFrame>>>>,
        click_render_failures: Arc<AtomicUsize>,
        poll_calls: Arc<AtomicUsize>,
        render_calls: Arc<AtomicUsize>,
        sample_gate: Option<mpsc::Sender<mpsc::SyncSender<()>>>,
        rendered: mpsc::Sender<CoordinateRenderFrame>,
    }

    impl FakeFactory {
        fn new(
            samples: impl IntoIterator<Item = Result<CoordinateSample, String>>,
            fallback: Result<CoordinateSample, String>,
        ) -> (Self, mpsc::Receiver<CoordinateRenderFrame>) {
            let (rendered, receiver) = mpsc::channel();
            (
                Self {
                    sampler_creations: Arc::new(AtomicUsize::new(0)),
                    backend_creations: Arc::new(AtomicUsize::new(0)),
                    sample_calls: Arc::new(AtomicUsize::new(0)),
                    shutdowns: Arc::new(AtomicUsize::new(0)),
                    samples: Arc::new(Mutex::new(samples.into_iter().collect())),
                    fallback,
                    backend_failure: None,
                    effects_status: Arc::new(Mutex::new(CoordinateEffectsStatus::default())),
                    stationary_frames: Arc::new(Mutex::new(Vec::new())),
                    click_render_calls: Arc::new(AtomicUsize::new(0)),
                    click_renders: Arc::new(Mutex::new(Vec::new())),
                    click_render_failures: Arc::new(AtomicUsize::new(0)),
                    poll_calls: Arc::new(AtomicUsize::new(0)),
                    render_calls: Arc::new(AtomicUsize::new(0)),
                    sample_gate: None,
                    rendered,
                },
                receiver,
            )
        }

        fn with_sample_gate(
            samples: impl IntoIterator<Item = Result<CoordinateSample, String>>,
            fallback: Result<CoordinateSample, String>,
        ) -> (
            Self,
            mpsc::Receiver<CoordinateRenderFrame>,
            mpsc::Receiver<mpsc::SyncSender<()>>,
        ) {
            let (mut factory, rendered) = Self::new(samples, fallback);
            let (gate, requests) = mpsc::channel();
            factory.sample_gate = Some(gate);
            (factory, rendered, requests)
        }

        fn count(&self, counter: &AtomicUsize) -> usize {
            counter.load(Ordering::Acquire)
        }
    }

    struct FakeSampler {
        calls: Arc<AtomicUsize>,
        samples: Arc<Mutex<VecDeque<Result<CoordinateSample, String>>>>,
        fallback: Result<CoordinateSample, String>,
        sample_gate: Option<mpsc::Sender<mpsc::SyncSender<()>>>,
    }

    impl CoordinateSampler for FakeSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            let call_number = self.calls.fetch_add(1, Ordering::AcqRel);
            if call_number == 1
                && let Some(gate) = self.sample_gate.as_ref()
            {
                let (release_sender, release_receiver) = mpsc::sync_channel(0);
                if gate.send(release_sender).is_ok() {
                    let _ = release_receiver.recv_timeout(Duration::from_secs(3));
                }
            }
            self.samples
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| self.fallback.clone())
        }
    }

    struct FakeSurfaceBackend {
        rendered: mpsc::Sender<CoordinateRenderFrame>,
        shutdowns: Arc<AtomicUsize>,
        effects_status: Arc<Mutex<CoordinateEffectsStatus>>,
        stationary_frames: Arc<Mutex<Vec<CoordinateRenderFrame>>>,
        click_render_calls: Arc<AtomicUsize>,
        click_renders: Arc<Mutex<Vec<Vec<ClickEffectFrame>>>>,
        click_render_failures: Arc<AtomicUsize>,
        poll_calls: Arc<AtomicUsize>,
        render_calls: Arc<AtomicUsize>,
    }

    impl CoordinateSurfaceBackend for FakeSurfaceBackend {
        fn poll_events(&mut self) -> Result<bool, String> {
            self.poll_calls.fetch_add(1, Ordering::AcqRel);
            Ok(false)
        }

        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            self.render_calls.fetch_add(1, Ordering::AcqRel);
            self.rendered
                .send(frame.clone())
                .map_err(|error| error.to_string())
        }

        fn render_click_effects(&mut self, frames: &[ClickEffectFrame]) -> Result<(), String> {
            self.click_render_calls.fetch_add(1, Ordering::AcqRel);
            self.click_renders.lock().unwrap().push(frames.to_vec());
            if !frames.is_empty()
                && self
                    .click_render_failures
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                        remaining.checked_sub(1)
                    })
                    .is_ok()
            {
                Err("injected click surface failure".into())
            } else {
                Ok(())
            }
        }

        fn effects_status(&self) -> CoordinateEffectsStatus {
            self.effects_status.lock().unwrap().clone()
        }

        fn refresh_stationary_sources(
            &mut self,
            frame: &CoordinateRenderFrame,
        ) -> Result<(), String> {
            self.stationary_frames.lock().unwrap().push(frame.clone());
            Ok(())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.shutdowns.fetch_add(1, Ordering::AcqRel);
            *self.effects_status.lock().unwrap() = CoordinateEffectsStatus::default();
            Ok(())
        }
    }

    impl CoordinateRuntimeFactory for FakeFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            self.sampler_creations.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(FakeSampler {
                calls: Arc::clone(&self.sample_calls),
                samples: Arc::clone(&self.samples),
                fallback: self.fallback.clone(),
                sample_gate: self.sample_gate.clone(),
            }))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            self.backend_creations.fetch_add(1, Ordering::AcqRel);
            if let Some(error) = &self.backend_failure {
                return Err(error.clone());
            }
            Ok(Box::new(FakeSurfaceBackend {
                rendered: self.rendered.clone(),
                shutdowns: Arc::clone(&self.shutdowns),
                effects_status: Arc::clone(&self.effects_status),
                stationary_frames: Arc::clone(&self.stationary_frames),
                click_render_calls: Arc::clone(&self.click_render_calls),
                click_renders: Arc::clone(&self.click_renders),
                click_render_failures: Arc::clone(&self.click_render_failures),
                poll_calls: Arc::clone(&self.poll_calls),
                render_calls: Arc::clone(&self.render_calls),
            }))
        }
    }

    struct LifecycleFactory {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    struct LifecycleSampler {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    struct LifecycleBackend {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl CoordinateSampler for LifecycleSampler {
        fn sample(&mut self) -> Result<CoordinateSample, String> {
            Err("lifecycle sampler should not be sampled".into())
        }
    }

    impl Drop for LifecycleSampler {
        fn drop(&mut self) {
            self.events.lock().unwrap().push("drop_sampler");
        }
    }

    impl CoordinateSurfaceBackend for LifecycleBackend {
        fn render(&mut self, _frame: &CoordinateRenderFrame) -> Result<(), String> {
            Ok(())
        }

        fn shutdown(&mut self) -> Result<(), String> {
            self.events.lock().unwrap().push("shutdown_backend");
            Ok(())
        }
    }

    impl Drop for LifecycleBackend {
        fn drop(&mut self) {
            self.events.lock().unwrap().push("drop_backend");
        }
    }

    impl CoordinateRuntimeFactory for LifecycleFactory {
        fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String> {
            self.events.lock().unwrap().push("create_sampler");
            Ok(Box::new(LifecycleSampler {
                events: Arc::clone(&self.events),
            }))
        }

        fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String> {
            self.events.lock().unwrap().push("create_backend");
            Ok(Box::new(LifecycleBackend {
                events: Arc::clone(&self.events),
            }))
        }
    }

    fn sample(x: i32, y: i32, monitor_id: &str) -> CoordinateSample {
        let monitor = PhysicalRect::new(-1920, 0, 0, 1080).unwrap();
        CoordinateSample::new(
            PhysicalPoint::new(x, y),
            Some(PhysicalRect::new(-1920, 0, 1920, 1080).unwrap()),
            Some(MonitorGeometry {
                id: MonitorId::new(monitor_id),
                bounds: monitor,
                work_area: PhysicalRect::new(-1920, 0, 0, 1040).unwrap(),
                effective_dpi: Some((96, 96)),
            }),
            Some(ForegroundClientGeometry::new(
                PhysicalPoint::new(-1800, 40),
                Some(PhysicalRect::new(-1800, 40, -100, 800).unwrap()),
            )),
        )
    }

    fn receive(receiver: &mpsc::Receiver<CoordinateRenderFrame>) -> CoordinateRenderFrame {
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should publish a frame")
    }

    fn receive_matching(
        receiver: &mpsc::Receiver<CoordinateRenderFrame>,
        mut predicate: impl FnMut(&CoordinateRenderFrame) -> bool,
    ) -> CoordinateRenderFrame {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let frame = receiver
                .recv_timeout(remaining)
                .expect("worker should publish the requested runtime frame");
            if predicate(&frame) {
                return frame;
            }
        }
    }

    fn wait_for_stationary_frames(
        factory: &FakeFactory,
        minimum: usize,
    ) -> Vec<CoordinateRenderFrame> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let frames = factory.stationary_frames.lock().unwrap().clone();
            if frames.len() >= minimum {
                return frames;
            }
            assert!(
                Instant::now() < deadline,
                "worker should refresh unchanged successful frames"
            );
            std::thread::yield_now();
        }
    }

    fn wait_for_click_render(
        factory: &FakeFactory,
        mut predicate: impl FnMut(&[ClickEffectFrame]) -> bool,
    ) -> Vec<ClickEffectFrame> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(frames) = factory
                .click_renders
                .lock()
                .unwrap()
                .iter()
                .find(|frames| predicate(frames))
            {
                return frames.clone();
            }
            assert!(
                Instant::now() < deadline,
                "worker should render click state"
            );
            std::thread::yield_now();
        }
    }

    fn wait_for_click_status(
        controller: &CoordinateToolController,
        expected: super::ClickEffectStatus,
    ) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while controller.effects_status().click() != &expected {
            assert!(Instant::now() < deadline, "click status should settle");
            std::thread::yield_now();
        }
    }

    #[test]
    fn off_on_generation_retires_old_marker_before_admitting_the_new_press() {
        let now = Instant::now();
        let mut click_effects = ClickEffectManager::default();
        let appearance = ClickEffectPreferences::default();
        let old_press = ClickPressEvent {
            button: ClickButton::Left,
            position: PhysicalPoint::new(-500, 80),
            observed_at: now,
            generation: 7,
        };
        assert!(click_effects.admit(old_press, appearance, now));
        assert_eq!(click_effects.active_generation(), Some(7));

        // Model an off/on transition whose queued clear was dropped while the
        // bounded worker ingress was full.
        assert!(retire_click_generation(&mut click_effects, 9));
        assert!(!click_effects.is_active());
        let new_press = ClickPressEvent {
            button: ClickButton::Right,
            position: PhysicalPoint::new(-1200, 300),
            observed_at: now + Duration::from_millis(1),
            generation: 9,
        };
        assert!(click_effects.admit(new_press, appearance, now + Duration::from_millis(1),));
        assert_eq!(click_effects.active_generation(), Some(9));
        assert!(!retire_click_generation(&mut click_effects, 9));
        let frames = click_effects.frames_at(now + Duration::from_millis(2));
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].button, ClickButton::Right);
        assert_eq!(frames[0].origin, PhysicalPoint::new(-1216, 284));
    }

    fn receive_click(controller: &CoordinateToolController) -> ClickPressEvent {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut events = controller.take_click_presses();
            if let Some(event) = events.pop() {
                return event;
            }
            assert!(
                Instant::now() < deadline,
                "worker should record click ingress"
            );
            std::thread::yield_now();
        }
    }

    fn click_event(generation: u64, button: ClickButton, point: PhysicalPoint) -> ClickPressEvent {
        ClickPressEvent {
            button,
            position: point,
            observed_at: Instant::now(),
            generation,
        }
    }

    #[test]
    fn one_worker_serves_independent_repeated_modes_and_stops_before_returning() {
        let sample = sample(-1800, 200, "DISPLAY1");
        let (factory, rendered) = FakeFactory::new([], Ok(sample));
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        assert!(!controller.is_running());
        assert_eq!(factory.count(&factory.sampler_creations), 0);

        controller.set_hud_enabled(true).unwrap();
        let hud = receive(&rendered);
        assert!(hud.runtime_state.hud_enabled());
        assert!(!hud.runtime_state.crosshair_enabled());
        controller.set_hud_enabled(true).unwrap();
        controller.set_crosshair_enabled(true).unwrap();
        controller.set_halo_enabled(true).unwrap();
        controller.set_zoom_enabled(true).unwrap();
        let all_modes = receive_matching(&rendered, |frame| {
            frame.runtime_state.hud_enabled()
                && frame.runtime_state.crosshair_enabled()
                && frame.runtime_state.halo_enabled()
                && frame.runtime_state.zoom_enabled()
        });
        assert!(all_modes.runtime_state.hud_enabled());
        assert!(all_modes.runtime_state.crosshair_enabled());
        assert!(all_modes.runtime_state.halo_enabled());
        assert!(all_modes.runtime_state.zoom_enabled());
        assert_eq!(
            all_modes.current_sample.as_ref().unwrap().desktop_point,
            PhysicalPoint::new(-1800, 200)
        );
        controller.set_zoom_enabled(false).unwrap();
        let without_zoom = receive_matching(&rendered, |frame| {
            frame.runtime_state.hud_enabled()
                && frame.runtime_state.crosshair_enabled()
                && frame.runtime_state.halo_enabled()
                && !frame.runtime_state.zoom_enabled()
        });
        assert!(without_zoom.runtime_state.halo_enabled());
        assert!(!without_zoom.runtime_state.zoom_enabled());

        controller.set_halo_enabled(false).unwrap();
        let without_halo = receive_matching(&rendered, |frame| {
            frame.runtime_state.hud_enabled()
                && frame.runtime_state.crosshair_enabled()
                && !frame.runtime_state.halo_enabled()
                && !frame.runtime_state.zoom_enabled()
        });
        assert!(without_halo.runtime_state.crosshair_enabled());

        controller.set_hud_enabled(false).unwrap();
        let crosshair_only = receive_matching(&rendered, |frame| {
            !frame.runtime_state.hud_enabled()
                && frame.runtime_state.crosshair_enabled()
                && !frame.runtime_state.halo_enabled()
                && !frame.runtime_state.zoom_enabled()
        });
        assert!(!crosshair_only.runtime_state.hud_enabled());
        assert_eq!(
            crosshair_only
                .current_sample
                .as_ref()
                .unwrap()
                .desktop_point,
            PhysicalPoint::new(-1800, 200)
        );

        controller.set_hud_enabled(true).unwrap();
        controller.set_halo_enabled(true).unwrap();
        controller.set_zoom_enabled(true).unwrap();
        let _ = receive_matching(&rendered, |frame| {
            frame.runtime_state.hud_enabled()
                && frame.runtime_state.crosshair_enabled()
                && frame.runtime_state.halo_enabled()
                && frame.runtime_state.zoom_enabled()
        });

        let all_modes = controller.runtime_state();
        assert!(all_modes.hud_enabled());
        assert!(all_modes.crosshair_enabled());
        assert!(all_modes.halo_enabled());
        assert!(all_modes.zoom_enabled());
        assert!(all_modes.has_active_mode());
        assert_eq!(factory.count(&factory.sampler_creations), 1);
        assert_eq!(factory.count(&factory.backend_creations), 1);

        controller.disable_effects().unwrap();
        let hud_only = receive_matching(&rendered, |frame| {
            frame.runtime_state.hud_enabled()
                && !frame.runtime_state.crosshair_enabled()
                && !frame.runtime_state.halo_enabled()
                && !frame.runtime_state.zoom_enabled()
        });
        assert!(hud_only.runtime_state.hud_enabled());
        assert!(!hud_only.runtime_state.crosshair_enabled());
        assert!(!hud_only.runtime_state.halo_enabled());
        assert!(!hud_only.runtime_state.zoom_enabled());
        assert!(controller.is_running());
        assert_eq!(factory.count(&factory.shutdowns), 0);

        controller.set_hud_enabled(false).unwrap();
        assert!(!controller.is_running());
        assert_eq!(factory.count(&factory.shutdowns), 1);
        let calls_at_stop = factory.count(&factory.sample_calls);
        controller.set_crosshair_enabled(false).unwrap();
        assert_eq!(factory.count(&factory.sample_calls), calls_at_stop);
    }

    #[test]
    fn freeze_keeps_display_sample_while_hud_placement_tracks_live_cursor() {
        let (factory, rendered, sample_gate) = FakeFactory::with_sample_gate(
            [
                Ok(sample(-1800, 200, "DISPLAY1")),
                Ok(sample(-1600, 300, "DISPLAY2")),
            ],
            Ok(sample(-1600, 300, "DISPLAY2")),
        );
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_hud_enabled(true).unwrap();
        controller.set_crosshair_enabled(true).unwrap();
        controller.set_halo_enabled(true).unwrap();
        controller.set_zoom_enabled(true).unwrap();
        let first = receive(&rendered);
        assert_eq!(
            first.current_sample.as_ref().unwrap().desktop_point,
            PhysicalPoint::new(-1800, 200)
        );

        let release_next_sample = sample_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should wait at the sample gate after its first frame");
        controller.freeze();
        release_next_sample.send(()).unwrap();
        let next = receive(&rendered);
        assert_eq!(
            next.displayed_sample.as_ref().unwrap().desktop_point,
            PhysicalPoint::new(-1800, 200)
        );
        assert_eq!(
            next.current_sample.as_ref().unwrap().desktop_point,
            PhysicalPoint::new(-1600, 300)
        );
        assert_eq!(
            next.placement_sample.as_ref().unwrap().desktop_point,
            PhysicalPoint::new(-1600, 300)
        );
        assert!(next.runtime_state.crosshair_enabled());
        assert!(next.runtime_state.halo_enabled());
        assert!(next.runtime_state.zoom_enabled());
        controller.shutdown().unwrap();
        assert!(!controller.runtime_state().has_active_mode());
    }

    #[test]
    fn sampling_failure_is_explicit_and_does_not_reuse_current_coordinates() {
        let (factory, rendered) = FakeFactory::new([], Err("cursor query failed".into()));
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_crosshair_enabled(true).unwrap();
        let frame = receive(&rendered);
        assert!(frame.current_sample.is_none());
        assert!(frame.displayed_sample.is_none());
        assert_eq!(frame.sample_error.as_deref(), Some("cursor query failed"));
        assert_eq!(
            controller.last_error().as_deref(),
            Some("cursor query failed")
        );
        controller.shutdown().unwrap();
    }

    #[test]
    fn freeze_after_sampling_failure_does_not_capture_the_last_good_placement_sample() {
        let first_sample = sample(-1800, 200, "DISPLAY1");
        let (factory, rendered, sample_gate) = FakeFactory::with_sample_gate(
            [Ok(first_sample.clone()), Err("cursor query failed".into())],
            Err("cursor query failed".into()),
        );
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_hud_enabled(true).unwrap();
        let first = receive(&rendered);
        assert_eq!(first.current_sample, Some(first_sample.clone()));
        let release_failure = sample_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("second sample should reach the explicit gate");
        controller.set_halo_enabled(true).unwrap();
        controller.set_zoom_enabled(true).unwrap();
        release_failure.send(()).unwrap();

        let unavailable = receive(&rendered);
        assert!(unavailable.current_sample.is_none());
        assert!(unavailable.displayed_sample.is_none());
        assert_eq!(unavailable.placement_sample, Some(first_sample));
        assert!(unavailable.runtime_state.halo_enabled());
        assert!(unavailable.runtime_state.zoom_enabled());
        controller.freeze();
        assert!(!controller.runtime_state().is_frozen());
        assert!(factory.stationary_frames.lock().unwrap().is_empty());
        controller.shutdown().unwrap();
    }

    #[test]
    fn copy_sample_is_live_or_frozen_and_never_uses_failed_or_inactive_live_data() {
        let first_sample = sample(-1800, 200, "DISPLAY1");
        let (factory, rendered, sample_gate) = FakeFactory::with_sample_gate(
            [Ok(first_sample.clone()), Err("cursor query failed".into())],
            Err("cursor query failed".into()),
        );
        let mut controller = CoordinateToolController::new(Arc::new(factory));
        controller.set_hud_enabled(true).unwrap();
        assert_eq!(
            receive(&rendered).current_sample,
            Some(first_sample.clone())
        );
        assert_eq!(controller.sample_for_copy().unwrap(), first_sample);

        let release_failure = sample_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("second sample should reach the explicit gate");
        controller.freeze();
        release_failure.send(()).unwrap();
        let failed = receive(&rendered);
        assert!(failed.current_sample.is_none());
        assert_eq!(
            controller.sample_for_copy().unwrap(),
            failed.runtime_state.frozen_sample().unwrap().clone()
        );

        controller.unfreeze();
        assert_eq!(
            controller.sample_for_copy().unwrap_err(),
            "cursor query failed"
        );
        controller.shutdown().unwrap();
        assert!(controller.sample_for_copy().is_err());
    }

    #[test]
    fn changed_monitor_geometry_reaches_backend_and_unchanged_frames_are_skipped() {
        let first = sample(-1800, 200, "DISPLAY1");
        let changed = sample(-1800, 200, "DISPLAY2");
        let (factory, rendered) = FakeFactory::new(
            [Ok(first.clone()), Ok(changed.clone())],
            Ok(changed.clone()),
        );
        let mut controller = CoordinateToolController::new(Arc::new(factory));
        controller.set_hud_enabled(true).unwrap();
        assert_eq!(
            receive(&rendered)
                .current_sample
                .unwrap()
                .monitor
                .unwrap()
                .id,
            MonitorId::new("DISPLAY1")
        );
        assert_eq!(
            receive(&rendered)
                .current_sample
                .unwrap()
                .monitor
                .unwrap()
                .id,
            MonitorId::new("DISPLAY2")
        );
        assert!(rendered.recv_timeout(Duration::from_millis(100)).is_err());
        controller.shutdown().unwrap();
        assert_eq!(
            controller.effects_status().halo(),
            &CursorEffectStatus::Disabled
        );
    }

    #[test]
    fn stationary_refresh_uses_last_successful_frame_without_redrawing() {
        let stable = sample(-1800, 200, "DISPLAY1");
        let (factory, rendered) = FakeFactory::new([], Ok(stable));
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_hud_enabled(true).unwrap();

        let first_render = receive(&rendered);
        let stationary_frames = wait_for_stationary_frames(&factory, 2);
        assert!(stationary_frames.iter().all(|frame| frame == &first_render));
        assert!(rendered.recv_timeout(Duration::from_millis(100)).is_err());

        controller.shutdown().unwrap();
    }

    #[test]
    fn no_sampler_or_backend_is_created_until_a_mode_is_enabled() {
        let (factory, _rendered) = FakeFactory::new([], Ok(sample(0, 0, "DISPLAY1")));
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_preferences(Default::default()).unwrap();
        assert!(!controller.is_running());
        assert_eq!(factory.count(&factory.sampler_creations), 0);
        assert_eq!(factory.count(&factory.backend_creations), 0);
        controller.shutdown().unwrap();
    }

    #[test]
    fn halo_and_zoom_setters_roll_back_when_worker_startup_fails() {
        let (mut factory, _rendered) = FakeFactory::new([], Ok(sample(0, 0, "DISPLAY1")));
        factory.backend_failure = Some("native backend initialization failed".into());
        let factory = Arc::new(factory);
        let mut controller = CoordinateToolController::new(factory.clone());

        assert_eq!(
            controller.set_halo_enabled(true).unwrap_err(),
            "native backend initialization failed"
        );
        assert!(!controller.runtime_state().halo_enabled());
        assert!(!controller.runtime_state().has_active_mode());
        assert!(!controller.is_running());

        assert_eq!(
            controller.set_zoom_enabled(true).unwrap_err(),
            "native backend initialization failed"
        );
        assert!(!controller.runtime_state().zoom_enabled());
        assert!(!controller.runtime_state().has_active_mode());
        assert_eq!(factory.count(&factory.sampler_creations), 2);
        assert_eq!(factory.count(&factory.backend_creations), 2);
        controller.shutdown().unwrap();
    }

    #[test]
    fn effect_status_refreshes_without_a_cheap_overlay_render() {
        let (factory, rendered) = FakeFactory::new([], Ok(sample(-1800, 200, "DISPLAY1")));
        let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));
        controller.set_hud_enabled(true).unwrap();
        let _initial_frame = receive(&rendered);
        assert_eq!(
            controller.effects_status().halo(),
            &CursorEffectStatus::Disabled
        );

        factory
            .effects_status
            .lock()
            .unwrap()
            .set_halo(CursorEffectStatus::Unavailable(
                "magnifier filter setup failed".into(),
            ));

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if matches!(
                controller.effects_status().halo(),
                CursorEffectStatus::Unavailable(reason)
                    if reason == "magnifier filter setup failed"
            ) {
                break;
            }
            assert!(Instant::now() < deadline, "effect status should refresh");
            std::thread::yield_now();
        }
        assert!(rendered.recv_timeout(Duration::from_millis(100)).is_err());
        controller.shutdown().unwrap();
    }

    #[test]
    fn halo_only_and_zoom_only_modes_each_own_the_worker_lifecycle() {
        for (enable_effect, halo_enabled) in [
            (
                CoordinateToolController::set_halo_enabled
                    as fn(&mut CoordinateToolController, bool) -> Result<(), String>,
                true,
            ),
            (
                CoordinateToolController::set_zoom_enabled
                    as fn(&mut CoordinateToolController, bool) -> Result<(), String>,
                false,
            ),
        ] {
            let sample = sample(-1800, 200, "DISPLAY1");
            let (factory, rendered) = FakeFactory::new([], Ok(sample.clone()));
            let mut controller = CoordinateToolController::new(Arc::new(factory.clone()));

            enable_effect(&mut controller, true).unwrap();
            let effect_frame = receive_matching(&rendered, |frame| {
                frame.runtime_state.halo_enabled() == halo_enabled
                    && frame.runtime_state.zoom_enabled() != halo_enabled
            });
            assert!(!effect_frame.runtime_state.hud_enabled());
            assert!(!effect_frame.runtime_state.crosshair_enabled());
            assert_eq!(effect_frame.current_sample, Some(sample));
            assert!(controller.is_running());
            assert_eq!(factory.count(&factory.sampler_creations), 1);
            assert_eq!(factory.count(&factory.backend_creations), 1);

            enable_effect(&mut controller, false).unwrap();
            assert!(!controller.is_running());
            assert!(!controller.runtime_state().has_active_mode());
            assert_eq!(factory.count(&factory.shutdowns), 1);
            controller.shutdown().unwrap();
        }
    }

    #[test]
    fn click_only_worker_is_event_driven_and_keeps_exact_physical_press_coordinates() {
        let (factory, _rendered) = FakeFactory::new([], Ok(sample(0, 0, "DISPLAY1")));
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut preferences = crate::coordinate_tool::CoordinateToolPreferences::default();
        preferences.click.duration_ms = 50;
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            preferences,
            provider,
        );

        controller.set_click_enabled(true).unwrap();
        assert!(controller.is_running());
        assert!(controller.runtime_state().click_enabled());
        assert!(controller.runtime_state().has_active_mode());
        assert!(!controller.runtime_state().has_cursor_mode());
        assert_eq!(factory.count(&factory.sampler_creations), 0);
        assert_eq!(factory.count(&factory.backend_creations), 1);
        assert_eq!(factory.count(&factory.sample_calls), 0);
        std::thread::sleep(Duration::from_millis(32));
        assert_eq!(factory.count(&factory.poll_calls), 0);
        assert_eq!(factory.count(&factory.render_calls), 0);
        assert_eq!(factory.count(&factory.click_render_calls), 0);

        let (generation, sink) = observer.current().unwrap();
        let point = PhysicalPoint::new(-2014, 113);
        let event = click_event(generation, ClickButton::Middle, point);
        assert!(sink.try_send(event));
        let observed = receive_click(&controller);
        assert_eq!(observed.button, ClickButton::Middle);
        assert_eq!(observed.position, point);
        assert_eq!(observed.generation, generation);
        assert_eq!(observed.observed_at, event.observed_at);
        let first_frame = wait_for_click_render(&factory, |frames| !frames.is_empty());
        assert_eq!(first_frame.len(), 1);
        assert_eq!(first_frame[0].button, ClickButton::Middle);
        assert_eq!(first_frame[0].origin, PhysicalPoint::new(-2030, 97));
        wait_for_click_status(&controller, super::ClickEffectStatus::Animating);
        assert_eq!(factory.count(&factory.sample_calls), 0);
        assert_eq!(factory.count(&factory.backend_creations), 1);

        wait_for_click_render(&factory, |frames| frames.is_empty());
        let rendered_after_expiry = factory.count(&factory.click_render_calls);
        assert_eq!(
            factory
                .click_renders
                .lock()
                .unwrap()
                .iter()
                .filter(|frames| frames.is_empty())
                .count(),
            1,
            "the final expiration should clear the surface once"
        );
        wait_for_click_status(&controller, super::ClickEffectStatus::Idle);
        let polls_after_expiry = factory.count(&factory.poll_calls);
        std::thread::sleep(Duration::from_millis(32));
        assert_eq!(
            factory.count(&factory.click_render_calls),
            rendered_after_expiry
        );
        assert_eq!(factory.count(&factory.poll_calls), polls_after_expiry);
        assert_eq!(factory.count(&factory.sample_calls), 0);
        assert_eq!(factory.count(&factory.render_calls), 0);

        controller.set_click_enabled(false).unwrap();
        assert!(!controller.is_running());
        assert!(!controller.runtime_state().click_enabled());
        assert!(observer.current().is_none());
        assert_eq!(observer.releases.load(Ordering::Acquire), 1);
    }

    #[test]
    fn click_surface_failure_rejects_session_presses_and_other_modes_keep_running() {
        let (factory, rendered) = FakeFactory::new([], Ok(sample(10, 20, "DISPLAY1")));
        factory.click_render_failures.store(1, Ordering::Release);
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut preferences = crate::coordinate_tool::CoordinateToolPreferences::default();
        preferences.click.duration_ms = 50;
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            preferences,
            provider,
        );

        controller.set_click_enabled(true).unwrap();
        let (generation, sink) = observer.current().unwrap();
        assert!(sink.try_send(click_event(
            generation,
            ClickButton::Left,
            PhysicalPoint::new(-300, 100),
        )));
        receive_click(&controller);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !matches!(
            controller.effects_status().click(),
            super::ClickEffectStatus::Unavailable(reason)
                if reason == "injected click surface failure"
        ) {
            assert!(
                Instant::now() < deadline,
                "click failure should be reported"
            );
            std::thread::yield_now();
        }

        let calls_after_failure = factory.count(&factory.click_render_calls);
        let polls_after_failure = factory.count(&factory.poll_calls);
        assert!(sink.try_send(click_event(
            generation,
            ClickButton::Right,
            PhysicalPoint::new(-500, 300),
        )));
        receive_click(&controller);
        std::thread::sleep(Duration::from_millis(32));
        assert_eq!(
            factory.count(&factory.click_render_calls),
            calls_after_failure
        );
        assert_eq!(factory.count(&factory.poll_calls), polls_after_failure);
        assert_eq!(factory.count(&factory.sample_calls), 0);

        controller.set_hud_enabled(true).unwrap();
        let hud_frame = receive(&rendered);
        assert!(hud_frame.runtime_state.hud_enabled());
        assert!(matches!(
            controller.effects_status().click(),
            super::ClickEffectStatus::Unavailable(_)
        ));
        controller.shutdown().unwrap();
    }

    #[test]
    fn full_click_queue_off_on_recovers_failed_surface_and_admits_new_generation() {
        let (factory, rendered, sample_gate) =
            FakeFactory::with_sample_gate([], Ok(sample(10, 20, "DISPLAY1")));
        factory.click_render_failures.store(1, Ordering::Release);
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut preferences = crate::coordinate_tool::CoordinateToolPreferences::default();
        preferences.click.duration_ms = 2000;
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            preferences,
            provider,
        );

        controller.set_click_enabled(true).unwrap();
        let (old_generation, old_sink) = observer.current().unwrap();
        assert!(old_sink.try_send(click_event(
            old_generation,
            ClickButton::Left,
            PhysicalPoint::new(-300, 100),
        )));
        receive_click(&controller);
        wait_for_click_status(
            &controller,
            super::ClickEffectStatus::Unavailable("injected click surface failure".into()),
        );

        controller.set_hud_enabled(true).unwrap();
        let _ = receive(&rendered);
        let release_sample = sample_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("worker should block at its second cursor sample");
        for index in 0..CLICK_QUEUE_CAPACITY {
            assert!(old_sink.try_send(click_event(
                old_generation,
                ClickButton::Left,
                PhysicalPoint::new(index as i32, 100),
            )));
        }
        assert!(!old_sink.try_send(click_event(
            old_generation,
            ClickButton::Left,
            PhysicalPoint::new(999, 100),
        )));

        // Both bounded clear/reset notifications are dropped while the worker
        // is blocked. Its shared generation must still retire the old error.
        controller.set_click_enabled(false).unwrap();
        controller.set_click_enabled(true).unwrap();
        let (new_generation, new_sink) = observer.current().unwrap();
        assert_ne!(old_generation, new_generation);
        release_sample.send(()).unwrap();

        let new_press_point = PhysicalPoint::new(-1200, 350);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if new_sink.try_send(click_event(
                new_generation,
                ClickButton::Right,
                new_press_point,
            )) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "worker should drain the full queue"
            );
            std::thread::sleep(Duration::from_millis(1));
        }

        let admitted = receive_click(&controller);
        assert_eq!(admitted.generation, new_generation);
        assert_eq!(admitted.position, new_press_point);
        let rendered_click = wait_for_click_render(&factory, |frames| {
            frames.iter().any(|frame| {
                frame.button == ClickButton::Right && frame.origin == PhysicalPoint::new(-1216, 334)
            })
        });
        assert_eq!(rendered_click[0].button, ClickButton::Right);
        assert_eq!(
            controller.effects_status().click(),
            &super::ClickEffectStatus::Animating
        );
        controller.shutdown().unwrap();
    }

    #[test]
    fn sampler_dpi_context_is_nested_inside_backend_and_dropped_first() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let factory = LifecycleFactory {
            events: Arc::clone(&events),
        };
        let (sampler, backend) = create_cursor_runtime(&factory).unwrap();
        let mut sampler = Some(sampler);
        let mut backend = Some(backend);
        let shared = Arc::new(Mutex::new(super::SharedState::default()));

        shutdown_cursor_runtime(&mut sampler, &mut backend, &shared).unwrap();

        assert_eq!(
            *events.lock().unwrap(),
            [
                "create_backend",
                "create_sampler",
                "drop_sampler",
                "shutdown_backend",
                "drop_backend",
            ]
        );
    }

    #[test]
    fn raw_hook_click_reaches_worker_without_changing_gesture_policy_or_sampled_coordinates() {
        let moved_sample = sample(700, 800, "DISPLAY1");
        let (factory, rendered, sample_gate) = FakeFactory::with_sample_gate(
            [
                Ok(moved_sample.clone()),
                Err("cursor query failed after press".into()),
            ],
            Err("cursor query failed after press".into()),
        );
        let (hook_backend, hook) = MockHookBackend::new();
        let service = Arc::new(Mutex::new(
            MouseGestureService::new_with_backend_and_overlays(
                Box::new(hook_backend),
                Arc::new(InertGestureOverlayFactory),
                Arc::new(InertRightClickBackend),
                Arc::new(FixedGestureCursor),
            ),
        ));
        let mut gesture_config = MouseGestureConfig::default();
        gesture_config.enabled = true;
        gesture_config.no_match_behavior = NoMatchBehavior::DoNothing;
        service.lock().unwrap().update_db(None);
        service.lock().unwrap().update_config(gesture_config);
        assert!(service.lock().unwrap().is_running());

        let provider: Arc<dyn ClickObservationProvider> =
            Arc::new(LocalGestureClickObservationProvider {
                service: Arc::clone(&service),
            });
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            Default::default(),
            provider,
        );

        controller.set_hud_enabled(true).unwrap();
        let first = receive(&rendered);
        assert_eq!(first.current_sample, Some(moved_sample.clone()));
        let release_failure = sample_gate
            .recv_timeout(Duration::from_secs(2))
            .expect("the second sample should wait at the deterministic gate");
        controller.freeze();
        controller.record_successful_copy(crate::coordinate_tool::FormattedCoordinate {
            space: crate::coordinate_tool::CoordinateSpace::Desktop,
            point: moved_sample.desktop_point,
            text: "700,800".into(),
        });
        release_failure.send(()).unwrap();
        let failed_frame = receive_matching(&rendered, |frame| {
            frame.current_sample.is_none()
                && frame
                    .sample_error
                    .as_deref()
                    .is_some_and(|error| error.contains("cursor query failed after press"))
        });
        assert_eq!(failed_frame.placement_sample, Some(moved_sample.clone()));
        assert!(failed_frame.runtime_state.is_frozen());
        assert_eq!(failed_frame.displayed_sample, Some(moved_sample.clone()));

        controller.set_click_enabled(true).unwrap();

        assert_eq!(hook.install_count(), 1, "clicks share the gesture hook");
        let point = PhysicalPoint::new(-1510, 620);
        let press_started = Instant::now();
        assert!(hook.emit_physical_button_down(ClickButton::Right, point, false, false));
        let press = receive_click(&controller);
        let press_finished = Instant::now();
        assert_eq!(press.button, ClickButton::Right);
        assert_eq!(press.position, point);
        assert!(press.observed_at >= press_started && press.observed_at <= press_finished);

        let rendered_click = wait_for_click_render(&factory, |frames| {
            frames
                .first()
                .is_some_and(|frame| frame.button == ClickButton::Right)
        });
        assert_eq!(rendered_click.len(), 1);
        assert_eq!(rendered_click[0].origin, PhysicalPoint::new(-1526, 604));
        assert!(factory.count(&factory.sample_calls) >= 2);
        assert_eq!(
            controller.effects_status().click(),
            &super::ClickEffectStatus::Animating
        );

        assert!(!hook.emit_physical_button_down(ClickButton::Right, point, true, false));
        assert!(!hook.emit_physical_button_down(ClickButton::Right, point, false, true));
        assert!(
            controller.take_click_presses().is_empty(),
            "injected or tagged replay must not create a second visual"
        );

        let copy = controller.runtime_state().last_successful_copy().cloned();
        controller.disable_effects().unwrap();
        let state = controller.runtime_state();
        assert!(state.hud_enabled());
        assert!(state.is_frozen());
        assert!(!state.crosshair_enabled());
        assert!(!state.halo_enabled());
        assert!(!state.zoom_enabled());
        assert!(!state.click_enabled());
        assert_eq!(state.last_successful_copy(), copy.as_ref());
        wait_for_click_render(&factory, |frames| frames.is_empty());
        assert_eq!(
            controller.effects_status().click(),
            &super::ClickEffectStatus::Disabled
        );
        assert!(service.lock().unwrap().is_running());
        assert_eq!(hook.uninstall_count(), 0, "gesture consumer keeps the hook");

        controller.set_click_enabled(true).unwrap();
        assert_eq!(hook.install_count(), 1, "shutdown lease reuses the hook");
        controller.shutdown().unwrap();
        assert!(!controller.is_running());
        assert!(hook.emit_physical_button_down(ClickButton::Right, point, false, false));
        assert!(controller.take_click_presses().is_empty());
        assert_eq!(factory.count(&factory.shutdowns), 1);
        service.lock().unwrap().stop();
        assert_eq!(hook.uninstall_count(), 1);
    }

    #[test]
    fn saved_click_appearance_never_enables_the_session_runtime() {
        let (factory, _rendered) = FakeFactory::new([], Ok(sample(0, 0, "DISPLAY1")));
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut preferences = crate::coordinate_tool::CoordinateToolPreferences::default();
        preferences.click.size_px = 48;
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            preferences,
            provider,
        );

        assert!(!controller.runtime_state().click_enabled());
        assert!(!controller.is_running());
        controller
            .set_preferences(crate::coordinate_tool::CoordinateToolPreferences {
                click: crate::coordinate_tool::ClickEffectPreferences {
                    duration_ms: 700,
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        assert_eq!(controller.preferences().click.duration_ms, 700);
        assert!(!controller.runtime_state().click_enabled());
        assert!(!controller.is_running());
        assert_eq!(observer.acquisitions.load(Ordering::Acquire), 0);
        assert_eq!(factory.count(&factory.sampler_creations), 0);
        assert_eq!(factory.count(&factory.backend_creations), 0);
    }

    #[test]
    fn off_on_invalidates_old_click_generation_while_cursor_worker_continues() {
        let (factory, rendered) = FakeFactory::new([], Ok(sample(-1800, 200, "DISPLAY1")));
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            Default::default(),
            provider,
        );

        controller.set_hud_enabled(true).unwrap();
        let _ = receive(&rendered);
        controller.set_click_enabled(true).unwrap();
        let (old_generation, old_sink) = observer.current().unwrap();
        assert!(old_sink.try_send(click_event(
            old_generation,
            ClickButton::Left,
            PhysicalPoint::new(-900, 500),
        )));
        let old_visual = wait_for_click_render(&factory, |frames| !frames.is_empty());
        assert_eq!(old_visual[0].button, ClickButton::Left);
        controller.set_click_enabled(false).unwrap();
        assert!(
            controller.is_running(),
            "the HUD keeps the shared worker alive"
        );
        controller.set_click_enabled(true).unwrap();
        let (new_generation, new_sink) = observer.current().unwrap();
        assert!(new_generation > old_generation);

        assert!(old_sink.try_send(click_event(
            old_generation,
            ClickButton::Left,
            PhysicalPoint::new(1, 2),
        )));
        let expected_point = PhysicalPoint::new(-31, 77);
        assert!(new_sink.try_send(click_event(
            new_generation,
            ClickButton::Right,
            expected_point,
        )));

        let observed = receive_click(&controller);
        assert_eq!(observed.button, ClickButton::Right);
        assert_eq!(observed.position, expected_point);
        assert_eq!(observed.generation, new_generation);
        assert!(controller.take_click_presses().is_empty());
        let new_visual = wait_for_click_render(&factory, |frames| {
            frames
                .first()
                .is_some_and(|frame| frame.button == ClickButton::Right)
        });
        assert_eq!(new_visual.len(), 1);
        assert_eq!(new_visual[0].origin, PhysicalPoint::new(-47, 61));
        assert_eq!(
            controller.effects_status().click(),
            &super::ClickEffectStatus::Animating
        );
        assert_eq!(factory.count(&factory.sampler_creations), 1);

        controller.set_click_enabled(false).unwrap();
        controller.set_hud_enabled(false).unwrap();
        assert!(!controller.is_running());
    }

    #[test]
    fn click_observer_setup_failure_rolls_back_runtime_enablement() {
        let (factory, _rendered) = FakeFactory::new([], Ok(sample(0, 0, "DISPLAY1")));
        let observer = Arc::new(FakeClickObservation::new(Some("observer unavailable")));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory.clone()),
            Default::default(),
            provider,
        );

        assert_eq!(
            controller.set_click_enabled(true).unwrap_err(),
            "observer unavailable"
        );
        assert!(!controller.runtime_state().click_enabled());
        assert!(!controller.runtime_state().has_active_mode());
        assert!(!controller.is_running());
        assert_eq!(factory.count(&factory.sampler_creations), 0);
        assert_eq!(factory.count(&factory.backend_creations), 1);
        assert_eq!(factory.count(&factory.shutdowns), 1);
        assert_eq!(observer.acquisitions.load(Ordering::Acquire), 0);
    }

    #[test]
    fn effects_off_releases_click_observation_while_preserving_hud_and_freeze() {
        let (factory, rendered) = FakeFactory::new([], Ok(sample(-1800, 200, "DISPLAY1")));
        let observer = Arc::new(FakeClickObservation::new(None));
        let provider: Arc<dyn ClickObservationProvider> = observer.clone();
        let mut controller = CoordinateToolController::new_with_preferences_and_click_observation(
            Arc::new(factory),
            Default::default(),
            provider,
        );

        controller.set_hud_enabled(true).unwrap();
        let _ = receive(&rendered);
        controller.freeze();
        controller.set_click_enabled(true).unwrap();
        assert!(observer.current().is_some());

        controller.disable_effects().unwrap();
        let runtime = controller.runtime_state();
        assert!(runtime.hud_enabled());
        assert!(runtime.is_frozen());
        assert!(!runtime.click_enabled());
        assert!(observer.current().is_none());
        assert!(controller.is_running());

        controller.set_hud_enabled(false).unwrap();
        assert!(!controller.is_running());
    }
}
