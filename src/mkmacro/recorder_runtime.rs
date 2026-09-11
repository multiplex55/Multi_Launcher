//! Recording lifecycle. Capture processing is delegated to one ordered worker;
//! this type remains the authority for admission and lifecycle state.
use super::{
    EventEnricher, HookService, KeyboardTranslator, MkMacroStore, NormalizationConfig,
    RecorderObserverSession, RecorderProcessor, RecordingTarget, SystemKeyboardTranslator,
    WindowsEventEnricher, recorder_now_us,
};
use anyhow::{Result, anyhow, bail};
use std::{
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderRuntimeState {
    Idle,
    Recording,
    Paused,
    Stopping,
}
#[derive(Debug, Clone)]
pub struct RecorderSnapshot {
    pub state: RecorderRuntimeState,
    pub macro_id: Option<u64>,
    pub elapsed: Duration,
    pub raw_event_count: u64,
    pub estimated_action_count: usize,
    pub dropped_event_count: u64,
    pub revision: u64,
}
impl Default for RecorderSnapshot {
    fn default() -> Self {
        Self {
            state: RecorderRuntimeState::Idle,
            macro_id: None,
            elapsed: Duration::ZERO,
            raw_event_count: 0,
            estimated_action_count: 0,
            dropped_event_count: 0,
            revision: 0,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RecordingResult {
    pub target: RecordingTarget,
    pub literal_steps: Vec<super::RecordedStep>,
    pub plan: super::RecordingPlan,
    pub suggestions: Vec<super::RecordingSuggestion>,
    pub clipboard_observations: Vec<super::ClipboardObservation>,
    pub click_inspections: Vec<super::ClickInspection>,
    pub window_observations: Vec<super::WindowObservation>,
    pub notes: Vec<super::RecordingNote>,
    pub capture_duration: Duration,
    pub raw_event_count: u64,
    pub dropped_event_count: u64,
}

pub trait RecorderClock: Send + Sync + 'static {
    fn now_us(&self) -> u64;
}
pub struct SystemRecorderClock;
impl Default for SystemRecorderClock {
    fn default() -> Self {
        Self
    }
}
impl RecorderClock for SystemRecorderClock {
    fn now_us(&self) -> u64 {
        recorder_now_us()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Playback,
    Recording,
}
#[derive(Default)]
pub struct SharedOperationGuard(Mutex<Option<Operation>>);
impl SharedOperationGuard {
    pub(crate) fn claim(&self, op: Operation) -> bool {
        let mut v = self.0.lock().unwrap();
        if v.is_some() {
            false
        } else {
            *v = Some(op);
            true
        }
    }
    pub(crate) fn release(&self, op: Operation) {
        let mut v = self.0.lock().unwrap();
        if *v == Some(op) {
            *v = None
        }
    }
    pub(crate) fn active(&self, op: Operation) -> bool {
        *self.0.lock().unwrap() == Some(op)
    }
}

struct State {
    mode: RecorderRuntimeState,
    review_transfer_pending: bool,
    shutting_down: bool,
    services_shutdown: bool,
    target: Option<RecordingTarget>,
    started_us: u64,
    pause_started_us: Option<u64>,
    paused_us: u64,
    dropped_baseline: u64,
}
struct RecorderStopGuard<'a> {
    runtime: &'a RecorderRuntime,
    hold_operation_on_success: bool,
    completed: bool,
}
impl Drop for RecorderStopGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.runtime.state.lock().unwrap();
        state.mode = RecorderRuntimeState::Idle;
        state.target = None;
        state.pause_started_us = None;
        if !self.hold_operation_on_success || !self.completed {
            state.review_transfer_pending = false;
            self.runtime.guard.release(Operation::Recording);
        }
        self.runtime.publish(&state);
        drop(state);
        self.runtime.shutdown_services_if_ready();
    }
}
pub struct RecorderRuntime {
    hooks: HookService,
    processor: RecorderProcessor,
    clock: Arc<dyn RecorderClock>,
    store: Arc<MkMacroStore>,
    guard: Arc<SharedOperationGuard>,
    state: Mutex<State>,
    snapshot: RwLock<Arc<RecorderSnapshot>>,
}
impl RecorderRuntime {
    pub(crate) fn document_snapshot(&self) -> Option<Arc<super::MkMacroDocument>> {
        Some(self.store.snapshot())
    }
    pub fn new(
        store: Arc<MkMacroStore>,
        hooks: HookService,
        clock: Arc<dyn RecorderClock>,
    ) -> Self {
        Self::with_guard(
            store,
            hooks,
            clock,
            Arc::new(SharedOperationGuard::default()),
        )
    }
    pub(crate) fn with_guard(
        store: Arc<MkMacroStore>,
        hooks: HookService,
        clock: Arc<dyn RecorderClock>,
        guard: Arc<SharedOperationGuard>,
    ) -> Self {
        Self::with_guard_and_observer_factory(
            store,
            hooks,
            clock,
            guard,
            Arc::new(RecorderObserverSession::production),
        )
    }
    pub(crate) fn with_guard_and_observer_factory(
        store: Arc<MkMacroStore>,
        hooks: HookService,
        clock: Arc<dyn RecorderClock>,
        guard: Arc<SharedOperationGuard>,
        observer_factory: Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
    ) -> Self {
        Self::with_guard_and_components(
            store,
            hooks,
            clock,
            guard,
            Box::new(SystemKeyboardTranslator),
            observer_factory,
            Box::new(WindowsEventEnricher::default()),
        )
    }
    pub(crate) fn with_guard_and_components(
        store: Arc<MkMacroStore>,
        hooks: HookService,
        clock: Arc<dyn RecorderClock>,
        guard: Arc<SharedOperationGuard>,
        translator: Box<dyn KeyboardTranslator>,
        observer_factory: Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
        enricher: Box<dyn EventEnricher>,
    ) -> Self {
        let events = hooks
            .take_events()
            .expect("hook event receiver already owned");
        Self {
            processor: RecorderProcessor::with_all_components(
                events,
                translator,
                observer_factory,
                enricher,
            ),
            hooks,
            clock,
            store,
            guard,
            state: Mutex::new(State {
                mode: RecorderRuntimeState::Idle,
                review_transfer_pending: false,
                shutting_down: false,
                services_shutdown: false,
                target: None,
                started_us: 0,
                pause_started_us: None,
                paused_us: 0,
                dropped_baseline: 0,
            }),
            snapshot: RwLock::new(Arc::new(RecorderSnapshot::default())),
        }
    }
    fn publish(&self, s: &State) {
        let now = self.clock.now_us();
        let elapsed = if s.mode == RecorderRuntimeState::Idle {
            0
        } else {
            now.saturating_sub(s.started_us)
                .saturating_sub(s.paused_us)
                .saturating_sub(s.pause_started_us.map_or(0, |p| now.saturating_sub(p)))
        };
        let p = self.processor.snapshot();
        let revision = self.snapshot.read().unwrap().revision + 1;
        *self.snapshot.write().unwrap() = Arc::new(RecorderSnapshot {
            state: s.mode,
            macro_id: s.target.map(|t| t.macro_id),
            elapsed: Duration::from_micros(elapsed),
            raw_event_count: p.raw_event_count,
            estimated_action_count: p.estimated_action_count,
            dropped_event_count: self
                .hooks
                .dropped_events()
                .saturating_sub(s.dropped_baseline),
            revision,
        });
    }
    pub fn snapshot(&self) -> Arc<RecorderSnapshot> {
        let s = self.state.lock().unwrap();
        self.publish(&s);
        self.snapshot.read().unwrap().clone()
    }
    pub fn start(&self, macro_id: u64, config: NormalizationConfig) -> Result<()> {
        self.start_target(
            RecordingTarget {
                macro_id,
                insertion_anchor_step_id: None,
                insertion_anchor_generation: None,
            },
            config,
            Vec::new(),
        )
    }
    pub fn start_target(
        &self,
        target: RecordingTarget,
        config: NormalizationConfig,
        held_keys: Vec<u32>,
    ) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.shutting_down {
            bail!("recorder runtime is shutting down")
        }
        if s.mode != RecorderRuntimeState::Idle {
            bail!("a recorder is already active")
        }
        if !self.guard.claim(Operation::Recording) {
            bail!("playback is active")
        }
        let dropped_baseline = self.hooks.dropped_events();
        if let Err(e) = self
            .processor
            .begin(target, config.clone(), self.hooks.fence(), held_keys)
        {
            self.guard.release(Operation::Recording);
            return Err(e);
        }
        self.hooks
            .set_record_injected_input(config.record_injected_input);
        if !self.hooks.start() {
            let _ = self.processor.finish(self.hooks.fence(), Vec::new());
            self.guard.release(Operation::Recording);
            return Err(anyhow!("failed to start hook service"));
        }
        s.mode = RecorderRuntimeState::Recording;
        s.target = Some(target);
        s.started_us = self.clock.now_us();
        s.pause_started_us = None;
        s.paused_us = 0;
        s.dropped_baseline = dropped_baseline;
        self.publish(&s);
        Ok(())
    }
    pub fn pause(&self) -> Result<()> {
        self.pause_with_control(Vec::new())
    }
    pub fn pause_with_control(&self, occurrence: Vec<u32>) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.mode != RecorderRuntimeState::Recording {
            bail!("recorder is not recording")
        }
        if !self.hooks.pause() {
            let _ = self.processor.finish(self.hooks.fence(), occurrence);
            s.mode = RecorderRuntimeState::Idle;
            s.target = None;
            s.pause_started_us = None;
            self.guard.release(Operation::Recording);
            self.publish(&s);
            bail!("failed to pause hook service; recording was terminated")
        }
        let now = self.clock.now_us();
        if let Err(error) = self.processor.pause(now, self.hooks.fence(), occurrence) {
            let _ = self.hooks.stop();
            let _ = self.processor.finish(self.hooks.fence(), Vec::new());
            s.mode = RecorderRuntimeState::Idle;
            s.target = None;
            s.pause_started_us = None;
            self.guard.release(Operation::Recording);
            self.publish(&s);
            return Err(anyhow!("failed to pause recorder processor: {error}"));
        }
        s.pause_started_us = Some(now);
        s.mode = RecorderRuntimeState::Paused;
        self.publish(&s);
        Ok(())
    }
    pub fn resume(&self) -> Result<()> {
        self.resume_with_held(Vec::new())
    }
    pub fn resume_with_held(&self, held_keys: Vec<u32>) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if s.mode != RecorderRuntimeState::Paused {
            bail!("recorder is not paused")
        }
        let now = self.clock.now_us();
        if let Err(error) = self.processor.resume(now, held_keys) {
            let _ = self.hooks.stop();
            let _ = self.processor.finish(self.hooks.fence(), Vec::new());
            s.mode = RecorderRuntimeState::Idle;
            s.target = None;
            s.pause_started_us = None;
            self.guard.release(Operation::Recording);
            self.publish(&s);
            return Err(anyhow!("failed to resume recorder processor: {error}"));
        }
        if !self.hooks.resume() {
            let _ = self.hooks.stop();
            let _ = self.processor.finish(self.hooks.fence(), Vec::new());
            s.mode = RecorderRuntimeState::Idle;
            s.target = None;
            s.pause_started_us = None;
            self.guard.release(Operation::Recording);
            self.publish(&s);
            return Err(anyhow!(
                "failed to resume hook service; recording was terminated"
            ));
        }
        if let Some(p) = s.pause_started_us.take() {
            s.paused_us += now.saturating_sub(p)
        }
        s.mode = RecorderRuntimeState::Recording;
        self.publish(&s);
        Ok(())
    }
    pub fn suppress_control(&self, occurrence: Vec<u32>) -> Result<()> {
        let s = self.state.lock().unwrap();
        if s.mode != RecorderRuntimeState::Recording {
            bail!("recorder is not recording")
        }
        let fence = self
            .hooks
            .synchronize()
            .ok_or_else(|| anyhow!("failed to synchronize hook service"))?;
        self.processor.control_occurrence(fence, occurrence)
    }
    /// Adds an instantaneous review marker without synthesizing a macro action.
    /// Markers remain valid while manually paused.
    pub fn marker(&self) -> Result<()> {
        let s = self.state.lock().unwrap();
        if !matches!(
            s.mode,
            RecorderRuntimeState::Recording | RecorderRuntimeState::Paused
        ) {
            bail!("recorder is not active")
        }
        self.processor.marker(self.clock.now_us())
    }
    /// Adds transient review text. UI-owned pause/resume policy is intentionally
    /// kept outside this API so an annotation cannot resume a manual pause.
    pub fn annotation(&self, text: String) -> Result<()> {
        let s = self.state.lock().unwrap();
        if !matches!(
            s.mode,
            RecorderRuntimeState::Recording | RecorderRuntimeState::Paused
        ) {
            bail!("recorder is not active")
        }
        if text.trim().is_empty() {
            bail!("annotation cannot be empty")
        }
        self.processor.annotation(self.clock.now_us(), text)
    }
    pub fn stop(&self) -> Result<RecordingResult> {
        self.stop_with_control(Vec::new())
    }
    pub fn stop_with_control(&self, occurrence: Vec<u32>) -> Result<RecordingResult> {
        self.stop_with_control_inner(occurrence, true)
    }
    pub(crate) fn stop_for_review_with_control(
        &self,
        occurrence: Vec<u32>,
    ) -> Result<RecordingResult> {
        self.stop_with_control_inner(occurrence, false)
    }
    pub(crate) fn complete_review_transfer(&self) {
        self.state.lock().unwrap().review_transfer_pending = false;
        self.guard.release(Operation::Recording);
        self.shutdown_services_if_ready();
    }
    fn stop_with_control_inner(
        &self,
        occurrence: Vec<u32>,
        release_operation: bool,
    ) -> Result<RecordingResult> {
        let mut s = self.state.lock().unwrap();
        if !matches!(
            s.mode,
            RecorderRuntimeState::Recording | RecorderRuntimeState::Paused
        ) {
            bail!("recorder is not active or is already stopping")
        }
        // Publish Stopping before the potentially slow hook/observer barriers.
        // The guard below owns every terminal cleanup path from this point.
        s.mode = RecorderRuntimeState::Stopping;
        self.publish(&s);
        let started_us = s.started_us;
        let paused_us = s.paused_us;
        let pause_started_us = s.pause_started_us;
        let dropped_baseline = s.dropped_baseline;
        drop(s);
        let mut stop_guard = RecorderStopGuard {
            runtime: self,
            hold_operation_on_success: !release_operation,
            completed: false,
        };
        if !self.hooks.stop() {
            let _ = self.processor.finish(self.hooks.fence(), occurrence);
            return Err(anyhow!("failed to stop hook service"));
        }
        // Capture duration ends with input capture, not after potentially slow
        // observation/UIA worker finalization.
        let stopped_us = self.clock.now_us();
        let processed = self.processor.finish(self.hooks.fence(), occurrence);
        let capture_duration = Duration::from_micros(
            stopped_us
                .saturating_sub(started_us)
                .saturating_sub(paused_us)
                .saturating_sub(
                    pause_started_us.map_or(0, |paused| stopped_us.saturating_sub(paused)),
                ),
        );
        let dropped = self.hooks.dropped_events().saturating_sub(dropped_baseline);
        let p = processed?;
        if !release_operation {
            self.state.lock().unwrap().review_transfer_pending = true;
        }
        stop_guard.completed = true;
        Ok(RecordingResult {
            target: p.target,
            literal_steps: p.literal_steps,
            plan: p.plan,
            suggestions: p.suggestions,
            clipboard_observations: p.clipboard_observations,
            click_inspections: p.click_inspections,
            window_observations: p.window_observations,
            notes: p.notes,
            capture_duration,
            raw_event_count: p.raw_event_count,
            dropped_event_count: dropped,
        })
    }
    pub fn shutdown(&self) {
        let mode = self.state.lock().unwrap().mode;
        self.shutdown_after_observing(mode);
    }
    fn shutdown_after_observing(&self, _observed_mode: RecorderRuntimeState) {
        let mode = {
            let mut state = self.state.lock().unwrap();
            state.shutting_down = true;
            state.mode
        };
        match mode {
            RecorderRuntimeState::Stopping => return,
            RecorderRuntimeState::Recording | RecorderRuntimeState::Paused => {
                let _ = self.stop();
            }
            RecorderRuntimeState::Idle => {}
        }
        self.shutdown_services_if_ready();
    }
    fn shutdown_services_if_ready(&self) {
        let mut state = self.state.lock().unwrap();
        if state.mode != RecorderRuntimeState::Idle
            || state.review_transfer_pending
            || state.services_shutdown
            || !state.shutting_down
        {
            return;
        }
        state.services_shutdown = true;
        drop(state);
        self.hooks.shutdown();
        self.processor.shutdown();
        self.guard.release(Operation::Recording);
    }
}
impl Drop for RecorderRuntime {
    fn drop(&mut self) {
        self.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{
        AuxiliaryObservationWorker, CallbackSender, HookCommand, HookCommandRequest, HookEvent,
        HookLoopAdapter, KeyTransition, KeyTranslation, KeyboardTranslationRequest,
        ObservationBaseline, RawWindowObservation, RecordedAction, WindowEventSource,
    };
    use std::{
        collections::VecDeque,
        sync::{
            Condvar,
            atomic::{AtomicU64, AtomicUsize, Ordering},
        },
    };

    #[derive(Default)]
    struct TestClock(AtomicU64);
    impl RecorderClock for TestClock {
        fn now_us(&self) -> u64 {
            self.0.fetch_add(1_000, Ordering::SeqCst)
        }
    }

    struct SyntheticTranslator;
    impl KeyboardTranslator for SyntheticTranslator {
        fn initial_key_state(&mut self) -> [u8; 256] {
            [0; 256]
        }
        fn translate(&mut self, _: &KeyboardTranslationRequest) -> KeyTranslation {
            KeyTranslation::None
        }
    }
    struct SyntheticEnricher;
    impl EventEnricher for SyntheticEnricher {
        fn enrich(&mut self, _: &HookEvent) -> Option<crate::mkmacro::EventContext> {
            None
        }
    }

    struct ScriptedHookLoop {
        failures: Arc<Mutex<VecDeque<HookCommand>>>,
        seen: Arc<Mutex<Vec<HookCommand>>>,
        burst_on_start: usize,
    }
    impl HookLoopAdapter for ScriptedHookLoop {
        fn run(
            self,
            commands: std::sync::mpsc::Receiver<HookCommandRequest>,
            callback: CallbackSender,
        ) {
            while let Ok(request) = commands.recv() {
                self.seen.lock().unwrap().push(request.command);
                let fails = {
                    let mut failures = self.failures.lock().unwrap();
                    if failures.front() == Some(&request.command) {
                        failures.pop_front();
                        true
                    } else {
                        false
                    }
                };
                let shutdown = request.command == HookCommand::Shutdown;
                if !fails && request.command == HookCommand::Start {
                    for index in 0..self.burst_on_start {
                        callback.submit(HookEvent::Key {
                            timestamp_us: index as u64,
                            transition: if index % 2 == 0 {
                                KeyTransition::Down
                            } else {
                                KeyTransition::Up
                            },
                            vk: 65,
                            scan_code: 30,
                            flags: 0,
                            extra_info: 0,
                        });
                    }
                }
                request.acknowledge(!fails);
                if shutdown {
                    break;
                }
            }
        }
    }

    struct ImmediateResumeInput;
    impl HookLoopAdapter for ImmediateResumeInput {
        fn run(
            self,
            commands: std::sync::mpsc::Receiver<HookCommandRequest>,
            callback: CallbackSender,
        ) {
            while let Ok(request) = commands.recv() {
                let shutdown = request.command == HookCommand::Shutdown;
                if request.command == HookCommand::Resume {
                    callback.submit(HookEvent::Key {
                        timestamp_us: 4_000,
                        transition: KeyTransition::Down,
                        vk: 0x42,
                        scan_code: 0x30,
                        flags: 0,
                        extra_info: 0,
                    });
                }
                request.acknowledge(true);
                if shutdown {
                    break;
                }
            }
        }
    }

    struct BlockingShutdownLoop {
        gate: Arc<(Mutex<(bool, bool)>, Condvar)>,
    }
    impl HookLoopAdapter for BlockingShutdownLoop {
        fn run(
            self,
            commands: std::sync::mpsc::Receiver<HookCommandRequest>,
            _callback: CallbackSender,
        ) {
            while let Ok(request) = commands.recv() {
                if request.command == HookCommand::Shutdown {
                    let (lock, wake) = &*self.gate;
                    let mut state = lock.lock().unwrap();
                    state.0 = true;
                    wake.notify_all();
                    while !state.1 {
                        state = wake.wait(state).unwrap();
                    }
                    request.acknowledge(true);
                    break;
                }
                request.acknowledge(true);
            }
        }
    }

    fn empty_observer() -> RecorderObserverSession {
        RecorderObserverSession::with_parts(
            ObservationBaseline::default(),
            AuxiliaryObservationWorker::spawn(None, None),
        )
    }

    fn test_runtime(
        path: &std::path::Path,
        guard: Arc<SharedOperationGuard>,
        failures: Arc<Mutex<VecDeque<HookCommand>>>,
        seen: Arc<Mutex<Vec<HookCommand>>>,
        burst_on_start: usize,
        observer_factory: Arc<dyn Fn() -> RecorderObserverSession + Send + Sync>,
    ) -> RecorderRuntime {
        let (store, _) = MkMacroStore::open(path).unwrap();
        RecorderRuntime::with_guard_and_components(
            Arc::new(store),
            HookService::with_adapter(
                ScriptedHookLoop {
                    failures,
                    seen,
                    burst_on_start,
                },
                1,
            ),
            Arc::new(TestClock::default()),
            guard,
            Box::new(SyntheticTranslator),
            observer_factory,
            Box::new(SyntheticEnricher),
        )
    }

    #[test]
    fn false_stop_acknowledgement_releases_admission_and_allows_retry_session() {
        let dir = tempfile::tempdir().unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let failures = Arc::new(Mutex::new(VecDeque::from([HookCommand::Stop])));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runtime = test_runtime(
            dir.path(),
            guard.clone(),
            failures,
            seen,
            0,
            Arc::new(empty_observer),
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        assert!(runtime.stop().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert!(!guard.active(Operation::Recording));

        runtime.start(2, NormalizationConfig::default()).unwrap();
        assert_eq!(runtime.stop().unwrap().target.macro_id, 2);
        assert!(!guard.active(Operation::Recording));
    }

    #[test]
    fn hook_pause_and_resume_disconnects_terminate_and_release_admission() {
        let dir = tempfile::tempdir().unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let failures = Arc::new(Mutex::new(VecDeque::from([HookCommand::Pause])));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runtime = test_runtime(
            dir.path(),
            guard.clone(),
            failures.clone(),
            seen.clone(),
            0,
            Arc::new(empty_observer),
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        assert!(runtime.pause().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert!(!guard.active(Operation::Recording));
        assert_eq!(
            *seen.lock().unwrap(),
            vec![HookCommand::Start, HookCommand::Pause]
        );

        runtime.start(2, NormalizationConfig::default()).unwrap();
        runtime.pause().unwrap();
        failures.lock().unwrap().push_back(HookCommand::Resume);
        assert!(runtime.resume().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert!(!guard.active(Operation::Recording));

        runtime.start(3, NormalizationConfig::default()).unwrap();
        runtime.stop().unwrap();
    }

    #[test]
    fn shutdown_marks_runtime_terminal_before_service_teardown() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
        let runtime = Arc::new(RecorderRuntime::with_guard_and_components(
            Arc::new(store),
            HookService::with_adapter(BlockingShutdownLoop { gate: gate.clone() }, 8),
            Arc::new(TestClock::default()),
            guard.clone(),
            Box::new(SyntheticTranslator),
            Arc::new(empty_observer),
            Box::new(SyntheticEnricher),
        ));

        let shutting_down = runtime.clone();
        let worker = std::thread::spawn(move || shutting_down.shutdown());
        {
            let (lock, wake) = &*gate;
            let state = lock.lock().unwrap();
            let (mut state, timeout) = wake
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
                .unwrap();
            assert!(!timeout.timed_out(), "Shutdown did not reach hook teardown");

            let error = runtime
                .start(1, NormalizationConfig::default())
                .unwrap_err();
            assert!(error.to_string().contains("shutting down"));
            assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
            assert!(!guard.active(Operation::Recording));

            state.1 = true;
            wake.notify_all();
        }
        worker.join().unwrap();
    }

    #[test]
    fn input_emitted_during_hook_resume_is_after_the_processor_resume_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let runtime = RecorderRuntime::with_guard_and_components(
            Arc::new(store),
            HookService::with_adapter(ImmediateResumeInput, 8),
            Arc::new(TestClock::default()),
            Arc::new(SharedOperationGuard::default()),
            Box::new(SyntheticTranslator),
            Arc::new(empty_observer),
            Box::new(SyntheticEnricher),
        );
        let mut config = NormalizationConfig::default();
        config.record_window_context = false;
        runtime.start(1, config).unwrap();
        runtime.pause().unwrap();
        runtime.resume().unwrap();
        let result = runtime.stop().unwrap();
        assert_eq!(result.raw_event_count, 1);
        assert!(matches!(
            result.literal_steps.as_slice(),
            [step]
                if matches!(
                    &step.action,
                    RecordedAction::Key {
                        down: true,
                        vk: 0x42,
                        ..
                    }
                )
        ));
    }

    #[test]
    fn disconnected_processor_during_pause_terminates_and_releases_shared_admission() {
        let dir = tempfile::tempdir().unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runtime = test_runtime(
            dir.path(),
            guard.clone(),
            Arc::new(Mutex::new(VecDeque::new())),
            seen.clone(),
            0,
            Arc::new(empty_observer),
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        runtime.processor.panic_worker_for_test();
        assert!(runtime.pause().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [HookCommand::Start, HookCommand::Pause, HookCommand::Stop]
        );
        assert!(!guard.active(Operation::Recording));

        runtime.start(2, NormalizationConfig::default()).unwrap();
        runtime.stop().unwrap();
    }

    #[test]
    fn disconnected_processor_during_resume_terminates_and_releases_admission() {
        let dir = tempfile::tempdir().unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runtime = test_runtime(
            dir.path(),
            guard.clone(),
            Arc::new(Mutex::new(VecDeque::new())),
            seen.clone(),
            0,
            Arc::new(empty_observer),
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        runtime.pause().unwrap();
        runtime.processor.panic_worker_for_test();
        assert!(runtime.resume().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            [HookCommand::Start, HookCommand::Pause, HookCommand::Stop,]
        );
        assert!(!guard.active(Operation::Recording));

        runtime.start(2, NormalizationConfig::default()).unwrap();
        runtime.stop().unwrap();
    }

    struct PanicOnFinish;
    impl WindowEventSource for PanicOnFinish {
        fn drain(&mut self) -> Vec<RawWindowObservation> {
            Vec::new()
        }
        fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
            panic!("fake observer finish panic")
        }
    }

    struct BlockingFinish {
        gate: Arc<(Mutex<(bool, bool, usize)>, Condvar)>,
    }
    impl WindowEventSource for BlockingFinish {
        fn drain(&mut self) -> Vec<RawWindowObservation> {
            Vec::new()
        }
        fn shutdown_and_drain(&mut self) -> Vec<RawWindowObservation> {
            let (lock, wake) = &*self.gate;
            let mut state = lock.lock().unwrap();
            state.0 = true;
            state.2 += 1;
            wake.notify_all();
            while !state.1 {
                state = wake.wait(state).unwrap();
            }
            Vec::new()
        }
    }

    #[test]
    fn concurrent_stop_and_shutdown_cannot_consume_or_release_a_review_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new((Mutex::new((false, false, 0)), Condvar::new()));
        let observer_factory = {
            let gate = gate.clone();
            Arc::new(move || {
                RecorderObserverSession::with_sources(
                    ObservationBaseline::default(),
                    AuxiliaryObservationWorker::spawn(None, None),
                    Some(Box::new(BlockingFinish { gate: gate.clone() })),
                )
            })
        };
        let runtime = Arc::new(RecorderRuntime::with_guard_and_components(
            Arc::new(store),
            HookService::with_adapter(
                ScriptedHookLoop {
                    failures: Arc::new(Mutex::new(VecDeque::new())),
                    seen: seen.clone(),
                    burst_on_start: 0,
                },
                8,
            ),
            Arc::new(TestClock::default()),
            guard.clone(),
            Box::new(SyntheticTranslator),
            observer_factory,
            Box::new(SyntheticEnricher),
        ));
        runtime.start(1, NormalizationConfig::default()).unwrap();
        let shutdown_observation = runtime.state.lock().unwrap().mode;
        assert_eq!(shutdown_observation, RecorderRuntimeState::Recording);
        let stopping = runtime.clone();
        let worker = std::thread::spawn(move || stopping.stop_for_review_with_control(Vec::new()));
        {
            let (lock, wake) = &*gate;
            let state = lock.lock().unwrap();
            let (mut state, timeout) = wake
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
                .unwrap();
            assert!(
                !timeout.timed_out(),
                "first Stop did not reach observer finish"
            );
            assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Stopping);
            assert!(runtime.stop().is_err());
            runtime.shutdown_after_observing(shutdown_observation);
            assert!(guard.active(Operation::Recording));
            assert_eq!(state.2, 1);
            state.1 = true;
            wake.notify_all();
        }
        assert_eq!(worker.join().unwrap().unwrap().target.macro_id, 1);
        assert!(guard.active(Operation::Recording));
        runtime.complete_review_transfer();
        assert!(!guard.active(Operation::Recording));
        assert!(runtime.state.lock().unwrap().services_shutdown);
        assert_eq!(
            seen.lock()
                .unwrap()
                .iter()
                .filter(|command| **command == HookCommand::Shutdown)
                .count(),
            1
        );
    }

    #[test]
    fn shutdown_racing_direct_stop_defers_exactly_one_service_teardown() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(dir.path()).unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new((Mutex::new((false, false, 0)), Condvar::new()));
        let observer_factory = {
            let gate = gate.clone();
            Arc::new(move || {
                RecorderObserverSession::with_sources(
                    ObservationBaseline::default(),
                    AuxiliaryObservationWorker::spawn(None, None),
                    Some(Box::new(BlockingFinish { gate: gate.clone() })),
                )
            })
        };
        let runtime = Arc::new(RecorderRuntime::with_guard_and_components(
            Arc::new(store),
            HookService::with_adapter(
                ScriptedHookLoop {
                    failures: Arc::new(Mutex::new(VecDeque::new())),
                    seen: seen.clone(),
                    burst_on_start: 0,
                },
                8,
            ),
            Arc::new(TestClock::default()),
            guard.clone(),
            Box::new(SyntheticTranslator),
            observer_factory,
            Box::new(SyntheticEnricher),
        ));
        runtime.start(1, NormalizationConfig::default()).unwrap();
        let stopping = runtime.clone();
        let worker = std::thread::spawn(move || stopping.stop());
        {
            let (lock, wake) = &*gate;
            let state = lock.lock().unwrap();
            let (mut state, timeout) = wake
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.0)
                .unwrap();
            assert!(!timeout.timed_out(), "Stop did not reach observer finish");
            runtime.shutdown();
            assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Stopping);
            state.1 = true;
            wake.notify_all();
        }
        assert_eq!(worker.join().unwrap().unwrap().target.macro_id, 1);
        assert!(runtime.state.lock().unwrap().services_shutdown);
        assert!(!guard.active(Operation::Recording));
        assert_eq!(
            seen.lock()
                .unwrap()
                .iter()
                .filter(|command| **command == HookCommand::Shutdown)
                .count(),
            1
        );
    }

    #[test]
    fn processor_panic_during_stop_recovers_the_same_runtime_for_a_new_session() {
        let dir = tempfile::tempdir().unwrap();
        let guard = Arc::new(SharedOperationGuard::default());
        let finishes = Arc::new(AtomicUsize::new(0));
        let observer_factory = {
            let finishes = finishes.clone();
            Arc::new(move || {
                if finishes.fetch_add(1, Ordering::SeqCst) == 0 {
                    RecorderObserverSession::with_sources(
                        ObservationBaseline::default(),
                        AuxiliaryObservationWorker::spawn(None, None),
                        Some(Box::new(PanicOnFinish)),
                    )
                } else {
                    empty_observer()
                }
            })
        };
        let runtime = test_runtime(
            dir.path(),
            guard.clone(),
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(Vec::new())),
            0,
            observer_factory,
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        assert!(runtime.stop().is_err());
        assert_eq!(runtime.snapshot().state, RecorderRuntimeState::Idle);
        assert!(!guard.active(Operation::Recording));

        runtime.start(2, NormalizationConfig::default()).unwrap();
        runtime.stop().unwrap();
    }

    #[test]
    fn snapshots_use_cached_observation_state_and_drop_counts_are_session_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let factories = Arc::new(AtomicUsize::new(0));
        let observer_factory = {
            let factories = factories.clone();
            Arc::new(move || {
                factories.fetch_add(1, Ordering::SeqCst);
                empty_observer()
            })
        };
        let runtime = test_runtime(
            dir.path(),
            Arc::new(SharedOperationGuard::default()),
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(Vec::new())),
            10_000,
            observer_factory,
        );
        runtime.start(1, NormalizationConfig::default()).unwrap();
        for _ in 0..20 {
            let _ = runtime.snapshot();
        }
        assert_eq!(factories.load(Ordering::SeqCst), 1);
        let first = runtime.stop().unwrap();
        let first_total = runtime.hooks.dropped_events();
        assert!(first.dropped_event_count > 0);
        assert_eq!(first.dropped_event_count, first_total);

        runtime.start(2, NormalizationConfig::default()).unwrap();
        let second = runtime.stop().unwrap();
        let second_total = runtime.hooks.dropped_events();
        assert_eq!(factories.load(Ordering::SeqCst), 2);
        assert_eq!(second.dropped_event_count, second_total - first_total);
        assert!(second.dropped_event_count > 0);
    }

    #[test]
    fn idle_stop_does_not_issue_a_hook_command() {
        let dir = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let runtime = test_runtime(
            dir.path(),
            Arc::new(SharedOperationGuard::default()),
            Arc::new(Mutex::new(VecDeque::new())),
            seen.clone(),
            0,
            Arc::new(empty_observer),
        );
        assert!(runtime.stop().is_err());
        assert!(seen.lock().unwrap().is_empty());
    }
}
