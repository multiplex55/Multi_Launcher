//! Recording lifecycle. This is deliberately separate from playback execution.
use super::{
    HookEvent, HookService, MkMacroStore, NormalizationConfig, RecordedStep, RecordingBoundary,
    normalize, should_record,
};
use anyhow::{Result, anyhow, bail};
use std::{
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
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
    pub macro_id: u64,
    pub generated_steps: Vec<RecordedStep>,
    pub dropped_event_count: u64,
}

pub trait RecorderClock: Send + Sync + 'static {
    fn now_us(&self) -> u64;
}
pub struct SystemRecorderClock {
    epoch: Instant,
}
impl Default for SystemRecorderClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl RecorderClock for SystemRecorderClock {
    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
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
    macro_id: Option<u64>,
    config: NormalizationConfig,
    raw: Vec<RecordingBoundary>,
    started_us: u64,
    pause_started_us: Option<u64>,
    paused_us: u64,
}
pub struct RecorderRuntime {
    hooks: HookService,
    clock: Arc<dyn RecorderClock>,
    store: Arc<MkMacroStore>,
    guard: Arc<SharedOperationGuard>,
    state: Mutex<State>,
    snapshot: RwLock<Arc<RecorderSnapshot>>,
}
impl RecorderRuntime {
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
        Self {
            hooks,
            clock,
            store,
            guard,
            state: Mutex::new(State {
                mode: RecorderRuntimeState::Idle,
                macro_id: None,
                config: Default::default(),
                raw: vec![],
                started_us: 0,
                pause_started_us: None,
                paused_us: 0,
            }),
            snapshot: RwLock::new(Arc::new(RecorderSnapshot::default())),
        }
    }
    fn drain(&self, s: &mut State) {
        while let Some(e) = self.hooks.try_event() {
            if s.mode == RecorderRuntimeState::Recording
                && should_record(&e, s.config.record_injected_input)
            {
                s.raw.push(RecordingBoundary::Event(e));
            }
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
        let revision = self.snapshot.read().unwrap().revision;
        *self.snapshot.write().unwrap() = Arc::new(RecorderSnapshot {
            state: s.mode,
            macro_id: s.macro_id,
            elapsed: Duration::from_micros(elapsed),
            raw_event_count: s
                .raw
                .iter()
                .filter(|x| matches!(x, RecordingBoundary::Event(_)))
                .count() as u64,
            estimated_action_count: s
                .raw
                .iter()
                .filter(|x| matches!(x, RecordingBoundary::Event(_)))
                .count(),
            dropped_event_count: self.hooks.dropped_events(),
            revision: revision + 1,
        });
    }
    pub fn snapshot(&self) -> Arc<RecorderSnapshot> {
        let mut s = self.state.lock().unwrap();
        self.drain(&mut s);
        self.publish(&s);
        self.snapshot.read().unwrap().clone()
    }
    pub fn start(&self, macro_id: u64, config: NormalizationConfig) -> Result<()> {
        if !self
            .store
            .snapshot()
            .macros
            .iter()
            .any(|m| m.id == macro_id)
        {
            bail!("macro {macro_id} was not found")
        }
        let mut s = self.state.lock().unwrap();
        if s.mode != RecorderRuntimeState::Idle {
            bail!("a recorder is already active")
        }
        if !self.guard.claim(Operation::Recording) {
            bail!("playback is active")
        }
        s.raw.clear();
        s.macro_id = Some(macro_id);
        s.config = config;
        s.started_us = self.clock.now_us();
        s.paused_us = 0;
        s.pause_started_us = None;
        self.hooks
            .set_record_injected_input(s.config.record_injected_input);
        if !self.hooks.start() {
            self.guard.release(Operation::Recording);
            s.macro_id = None;
            return Err(anyhow!("failed to start hook service"));
        }
        s.mode = RecorderRuntimeState::Recording;
        self.publish(&s);
        Ok(())
    }
    pub fn pause(&self) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        self.drain(&mut s);
        if s.mode != RecorderRuntimeState::Recording {
            bail!("recorder is not recording")
        }
        let now = self.clock.now_us();
        if !self.hooks.pause() {
            bail!("failed to pause hook service")
        };
        s.raw.push(RecordingBoundary::Pause { timestamp_us: now });
        s.pause_started_us = Some(now);
        s.mode = RecorderRuntimeState::Paused;
        self.publish(&s);
        Ok(())
    }
    pub fn resume(&self) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        self.drain(&mut s);
        if s.mode != RecorderRuntimeState::Paused {
            bail!("recorder is not paused")
        }
        let now = self.clock.now_us();
        if !self.hooks.resume() {
            bail!("failed to resume hook service")
        };
        if let Some(p) = s.pause_started_us.take() {
            s.paused_us += now.saturating_sub(p)
        }
        s.raw.push(RecordingBoundary::Resume { timestamp_us: now });
        s.mode = RecorderRuntimeState::Recording;
        self.publish(&s);
        Ok(())
    }
    pub fn stop(&self) -> Result<RecordingResult> {
        let mut s = self.state.lock().unwrap();
        if s.mode == RecorderRuntimeState::Idle {
            bail!("recorder is not active")
        }
        // Tell the adapter to disable its callbacks first. Events already accepted by the
        // bounded channel are then frozen into this session before normalization begins.
        if !self.hooks.stop() {
            s.mode = RecorderRuntimeState::Idle;
            s.macro_id = None;
            s.raw.clear();
            self.guard.release(Operation::Recording);
            self.publish(&s);
            return Err(anyhow!("failed to stop hook service"));
        }
        std::thread::yield_now();
        self.drain(&mut s);
        s.mode = RecorderRuntimeState::Stopping;
        self.publish(&s);
        let id = s
            .macro_id
            .ok_or_else(|| anyhow!("recording target was lost"))?;
        let raw = std::mem::take(&mut s.raw);
        let cfg = s.config.clone();
        let dropped = self.hooks.dropped_events();
        s.mode = RecorderRuntimeState::Idle;
        s.macro_id = None;
        s.pause_started_us = None;
        self.guard.release(Operation::Recording);
        self.publish(&s);
        drop(s);
        Ok(RecordingResult {
            macro_id: id,
            generated_steps: normalize(&raw, &cfg, None),
            dropped_event_count: dropped,
        })
    }
    pub fn shutdown(&self) {
        if self.state.lock().unwrap().mode != RecorderRuntimeState::Idle {
            let _ = self.stop();
        }
        self.hooks.shutdown();
        self.guard.release(Operation::Recording);
    }
}
impl Drop for RecorderRuntime {
    fn drop(&mut self) {
        self.shutdown()
    }
}
