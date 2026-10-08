use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::model::{
    CoordinateEffectsStatus, CoordinateSample, CoordinateToolRuntimeState, FormattedCoordinate,
};
use super::settings::CoordinateToolPreferences;

const SAMPLE_INTERVAL: Duration = Duration::from_millis(16);

pub trait CoordinateSampler {
    fn sample(&mut self) -> Result<CoordinateSample, String>;
}

pub trait CoordinateSurfaceBackend {
    /// Pump the worker's window queue. `true` requests a full refresh after a
    /// display/DPI notification even when the sampled cursor is unchanged.
    fn poll_events(&mut self) -> Result<bool, String> {
        Ok(false)
    }

    fn effects_status(&self) -> CoordinateEffectsStatus {
        CoordinateEffectsStatus::default()
    }

    fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String>;
    fn shutdown(&mut self) -> Result<(), String>;
}

/// Native resources are created and used only from the coordinate worker.
pub trait CoordinateRuntimeFactory: Send + Sync + 'static {
    fn create_sampler(&self) -> Result<Box<dyn CoordinateSampler>, String>;
    fn create_backend(&self) -> Result<Box<dyn CoordinateSurfaceBackend>, String>;
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

#[derive(Debug)]
struct SharedState {
    preferences: CoordinateToolPreferences,
    runtime: CoordinateToolRuntimeState,
    latest_sample: Option<CoordinateSample>,
    pending_freeze: bool,
    sample_error: Option<String>,
    backend_error: Option<String>,
    effects_status: CoordinateEffectsStatus,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            preferences: CoordinateToolPreferences::default(),
            runtime: CoordinateToolRuntimeState::default(),
            latest_sample: None,
            pending_freeze: false,
            sample_error: None,
            backend_error: None,
            effects_status: CoordinateEffectsStatus::default(),
        }
    }
}

enum WorkerMessage {
    Shutdown,
}

struct WorkerHandle {
    sender: mpsc::Sender<WorkerMessage>,
    join: JoinHandle<Result<(), String>>,
}

/// Coordinates the HUD, crosshair, halo, and zoom and owns their single worker.
///
/// There is no thread while all four modes are disabled. Turning off the final
/// mode synchronously joins the worker, after its backend has released native
/// windows and drawing resources.
pub struct CoordinateToolController {
    factory: Arc<dyn CoordinateRuntimeFactory>,
    shared: Arc<Mutex<SharedState>>,
    worker: Option<WorkerHandle>,
}

impl CoordinateToolController {
    pub fn new(factory: Arc<dyn CoordinateRuntimeFactory>) -> Self {
        Self::new_with_preferences(factory, CoordinateToolPreferences::default())
    }

    pub fn new_with_preferences(
        factory: Arc<dyn CoordinateRuntimeFactory>,
        preferences: CoordinateToolPreferences,
    ) -> Self {
        let mut shared = SharedState::default();
        shared.preferences = preferences.normalized();
        Self {
            factory,
            shared: Arc::new(Mutex::new(shared)),
            worker: None,
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

    /// Disable the crosshair, halo, and zoom without changing HUD or inspector
    /// state. The shared worker remains active if the HUD is still enabled.
    pub fn disable_effects(&mut self) -> Result<(), String> {
        lock(&self.shared).runtime.disable_effects();
        self.reconcile_worker()
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
        }
        self.stop_worker()
    }

    fn reconcile_worker(&mut self) -> Result<(), String> {
        let active = lock(&self.shared).runtime.has_active_mode();
        match (active, self.worker.is_some()) {
            (true, false) => self.start_worker(),
            (false, true) => self.stop_worker(),
            _ => Ok(()),
        }
    }

    fn start_worker(&mut self) -> Result<(), String> {
        let factory = Arc::clone(&self.factory);
        let shared = Arc::clone(&self.shared);
        let (sender, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("coordinate-tool-passive".into())
            .spawn(move || run_worker(factory, receiver, ready_sender, shared))
            .map_err(|error| format!("Could not start coordinate worker: {error}"))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => {
                self.worker = Some(WorkerHandle { sender, join });
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
    receiver: mpsc::Receiver<WorkerMessage>,
    ready: mpsc::SyncSender<Result<(), String>>,
    shared: Arc<Mutex<SharedState>>,
) -> Result<(), String> {
    let mut sampler = match factory.create_sampler() {
        Ok(sampler) => sampler,
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let mut backend = match factory.create_backend() {
        Ok(backend) => backend,
        Err(error) => {
            drop(sampler);
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    if ready.send(Ok(())).is_err() {
        let shutdown = backend.shutdown();
        drop(backend);
        drop(sampler);
        return shutdown;
    }

    let mut last_good_sample: Option<CoordinateSample> = None;
    let mut last_frame: Option<CoordinateRenderFrame> = None;
    loop {
        match receiver.recv_timeout(SAMPLE_INTERVAL) {
            Ok(WorkerMessage::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let force_refresh = match backend.poll_events() {
            Ok(force_refresh) => force_refresh,
            Err(error) => {
                lock(&shared).backend_error = Some(error);
                false
            }
        };
        lock(&shared).effects_status = backend.effects_status();

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
            lock(&shared).effects_status = backend.effects_status();
        }
    }

    let shutdown = backend.shutdown();
    lock(&shared).effects_status = backend.effects_status();
    drop(backend);
    drop(sampler);
    shutdown
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
        CoordinateRenderFrame, CoordinateRuntimeFactory, CoordinateSampler,
        CoordinateSurfaceBackend, CoordinateToolController,
    };
    use crate::coordinate_tool::model::{
        CoordinateEffectsStatus, CoordinateSample, CursorEffectStatus, ForegroundClientGeometry,
        MonitorGeometry, MonitorId, PhysicalPoint, PhysicalRect,
    };

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
            if call_number == 1 {
                if let Some(gate) = self.sample_gate.as_ref() {
                    let (release_sender, release_receiver) = mpsc::sync_channel(0);
                    if gate.send(release_sender).is_ok() {
                        let _ = release_receiver.recv_timeout(Duration::from_secs(3));
                    }
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
    }

    impl CoordinateSurfaceBackend for FakeSurfaceBackend {
        fn render(&mut self, frame: &CoordinateRenderFrame) -> Result<(), String> {
            self.rendered
                .send(frame.clone())
                .map_err(|error| error.to_string())
        }

        fn effects_status(&self) -> CoordinateEffectsStatus {
            self.effects_status.lock().unwrap().clone()
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
        let mut controller = CoordinateToolController::new(Arc::new(factory));
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
        let mut controller = CoordinateToolController::new(Arc::new(factory));
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
        let mut controller = CoordinateToolController::new(Arc::new(factory));
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
}
