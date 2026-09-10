//! Recording lifecycle. Capture processing is delegated to one ordered worker;
//! this type remains the authority for admission and lifecycle state.
use super::{
    HookService, MkMacroStore, NormalizationConfig, RecorderProcessor, RecordingTarget,
    recorder_now_us,
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
    pub macro_id: u64,
    pub generated_steps: Vec<super::RecordedStep>,
    pub literal_steps: Vec<super::RecordedStep>,
    pub plan: super::RecordingPlan,
    pub suggestions: Vec<super::RecordingSuggestion>,
    pub clipboard_observations: Vec<super::ClipboardObservation>,
    pub click_inspections: Vec<super::ClickInspection>,
    pub window_observations: Vec<super::WindowObservation>,
    pub notes: Vec<super::RecordingNote>,
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
    target: Option<RecordingTarget>,
    started_us: u64,
    pause_started_us: Option<u64>,
    paused_us: u64,
    dropped_baseline: u64,
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
    pub(crate) fn store_contains(&self, id: u64) -> bool {
        self.store.snapshot().macros.iter().any(|m| m.id == id)
    }
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
        let events = hooks
            .take_events()
            .expect("hook event receiver already owned");
        Self {
            processor: RecorderProcessor::new(events),
            hooks,
            clock,
            store,
            guard,
            state: Mutex::new(State {
                mode: RecorderRuntimeState::Idle,
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
        if !self.store_contains(target.macro_id) {
            bail!("macro {} was not found", target.macro_id)
        }
        let mut s = self.state.lock().unwrap();
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
            bail!("failed to pause hook service")
        }
        let now = self.clock.now_us();
        self.processor.pause(now, self.hooks.fence(), occurrence)?;
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
        self.processor.resume(now, held_keys)?;
        if !self.hooks.resume() {
            return Err(anyhow!("failed to resume hook service"));
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
        let mut s = self.state.lock().unwrap();
        if s.mode == RecorderRuntimeState::Idle {
            bail!("recorder is not active")
        }
        if !self.hooks.stop() {
            return Err(anyhow!("failed to stop hook service"));
        }
        s.mode = RecorderRuntimeState::Stopping;
        self.publish(&s);
        let processed = self.processor.finish(self.hooks.fence(), occurrence);
        let dropped = self
            .hooks
            .dropped_events()
            .saturating_sub(s.dropped_baseline);
        s.mode = RecorderRuntimeState::Idle;
        s.target = None;
        s.pause_started_us = None;
        self.guard.release(Operation::Recording);
        self.publish(&s);
        let p = processed?;
        Ok(RecordingResult {
            target: p.target,
            macro_id: p.target.macro_id,
            generated_steps: p.generated_steps,
            literal_steps: p.literal_steps,
            plan: p.plan,
            suggestions: p.suggestions,
            clipboard_observations: p.clipboard_observations,
            click_inspections: p.click_inspections,
            window_observations: p.window_observations,
            notes: p.notes,
            dropped_event_count: dropped,
        })
    }
    pub fn shutdown(&self) {
        if self.state.lock().unwrap().mode != RecorderRuntimeState::Idle {
            let _ = self.stop();
        }
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
