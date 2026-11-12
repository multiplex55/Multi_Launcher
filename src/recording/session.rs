use super::model::{
    FrameError, FrameStamp, FrameTimeline, RecorderState, RecordingEndReason, RecordingFailure,
    RecordingRequest, RecordingSummary, RgbaFrame, TimedFrame,
};
use crate::thread_reaper::{self, ReapPermit};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_COUNTDOWN: Duration = Duration::from_secs(10);
const SOURCE_POLL_WAIT: Duration = Duration::from_millis(50);
const RETIREMENT_PENDING: u8 = 0;
const RETIREMENT_CONFIRMED: u8 = 1;
const RETIREMENT_UNAVAILABLE: u8 = 2;
const STOP_RUNNING: u8 = 0;
const STOP_REQUESTED: u8 = 1;
const STOP_CANCELLED: u8 = 2;

pub trait RecorderClock: Send + Sync + 'static {
    /// A monotonic duration from an arbitrary origin, not wall-clock time.
    fn now(&self) -> Duration;
}

pub struct SystemRecorderClock {
    origin: Instant,
}

impl Default for SystemRecorderClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl RecorderClock for SystemRecorderClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[derive(Clone)]
pub struct CaptureStopToken {
    state: Arc<AtomicU8>,
}

impl CaptureStopToken {
    fn new() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(STOP_RUNNING)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == STOP_CANCELLED
    }

    pub fn stop_requested(&self) -> bool {
        self.state.load(Ordering::Acquire) == STOP_REQUESTED
    }

    fn request_stop(&self) {
        let _ = self.state.compare_exchange(
            STOP_RUNNING,
            STOP_REQUESTED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn cancel(&self) {
        self.state.store(STOP_CANCELLED, Ordering::Release);
    }
}

pub enum CapturePoll {
    Frame { generation: u64, frame: RgbaFrame },
    Pending,
    Ended(RecordingEndReason),
    Failed(String),
}

/// Source implementations must return within `max_wait` and observe the stop
/// token. Native ownership remains on this one worker until the source drops.
/// The source itself is intentionally not `Send`: a backend can construct
/// thread-affine WinRT/D3D objects in `create_source` on their owning worker.
pub trait CaptureSource: 'static {
    fn start(
        &mut self,
        _request: &RecordingRequest,
        _stop: &CaptureStopToken,
    ) -> Result<(), String> {
        Ok(())
    }

    fn poll(&mut self, max_wait: Duration, stop: &CaptureStopToken) -> CapturePoll;
}

pub trait RecordingBackend: Send + Sync + 'static {
    fn create_source(
        &self,
        request: &RecordingRequest,
        generation: u64,
    ) -> Result<Box<dyn CaptureSource>, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureTerminal {
    generation: u64,
    reason: RecordingEndReason,
    elapsed: Duration,
    accepted_frames: u64,
}

impl CaptureTerminal {
    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub const fn reason(self) -> RecordingEndReason {
        self.reason
    }

    pub const fn elapsed(self) -> Duration {
        self.elapsed
    }

    pub const fn accepted_frames(self) -> u64 {
        self.accepted_frames
    }
}

/// A sink presents this only after it has successfully finalized the exact
/// generation. `frames_written` must equal the number of acknowledged leases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SinkReceipt {
    generation: u64,
    frames_written: u64,
}

impl SinkReceipt {
    pub const fn new(generation: u64, frames_written: u64) -> Self {
        Self {
            generation,
            frames_written,
        }
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub const fn frames_written(self) -> u64 {
        self.frames_written
    }
}

/// A one-use publication boundary. A pending capture can be cancelled until
/// its consumer obtains this permit; after claiming it, only completion or
/// explicit sink failure can settle the document.
pub struct FinalizationPermit {
    shared: Arc<SessionShared>,
    generation: u64,
    claim_id: u64,
    frames_written: u64,
    settled: bool,
}

impl FinalizationPermit {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// The acknowledged consumer count frozen atomically with this claim.
    pub const fn frames_written(&self) -> u64 {
        self.frames_written
    }

    /// Reports a sink/publication failure for this exact claimed generation.
    /// The unclaimed `RecordingSession::fail_sink` path cannot revoke
    /// a consumer after its publication claim.
    pub fn fail(&mut self, message: impl Into<String>) -> Result<(), RecordingSessionError> {
        let failure = RecordingFailure::Sink(message.into());
        if self.settled {
            return Err(RecordingSessionError::NotFinalizing);
        }
        let mut inner = self.shared.inner.lock().unwrap();
        if inner.generation != self.generation
            || inner.finalization_claim_id != Some(self.claim_id)
            || !matches!(&inner.state, RecorderState::Finalizing { .. })
        {
            return Err(RecordingSessionError::StaleGeneration);
        }
        inner.mailbox = None;
        inner.finalization_claim_id = None;
        inner.terminal = None;
        inner.pending_terminal = None;
        inner.state = RecorderState::Failed(failure);
        self.shared.clear_stoppable_generation(self.generation);
        self.settled = true;
        drop(inner);
        (self.shared.wake)();
        Ok(())
    }
}

impl Drop for FinalizationPermit {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let failed = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.generation == self.generation
                && inner.finalization_claim_id == Some(self.claim_id)
                && matches!(&inner.state, RecorderState::Finalizing { .. })
            {
                inner.finalization_claim_id = None;
                inner.terminal = None;
                inner.pending_terminal = None;
                inner.mailbox = None;
                inner.state = RecorderState::Failed(RecordingFailure::Sink(
                    "finalization permit was abandoned before publication settled".into(),
                ));
                self.shared.clear_stoppable_generation(self.generation);
                true
            } else {
                false
            }
        };
        self.settled = true;
        if failed {
            (self.shared.wake)();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingSessionError {
    Closed,
    Busy,
    MissingSelection,
    InvalidTransition,
    CountdownTooLong,
    GenerationExhausted,
    LeaseIdExhausted,
    StaleGeneration,
    NotFinalizing,
    PendingFrame,
    ConsumerBusy,
    NoFramesWritten,
    InvalidSinkReceipt,
    WrongSession,
    ClockMovedBackwards,
    WorkerCapacity(String),
    WorkerStart(String),
    WorkerRetirement(String),
}

impl std::fmt::Display for RecordingSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("recording session is closed"),
            Self::Busy => formatter.write_str("the previous recording worker is still retiring"),
            Self::MissingSelection => {
                formatter.write_str("select a capture target before starting")
            }
            Self::InvalidTransition => {
                formatter.write_str("recording action is not valid in this state")
            }
            Self::CountdownTooLong => formatter.write_str("countdown must not exceed ten seconds"),
            Self::GenerationExhausted => {
                formatter.write_str("recording generation space is exhausted")
            }
            Self::LeaseIdExhausted => {
                formatter.write_str("frame lease identifier space is exhausted")
            }
            Self::StaleGeneration => {
                formatter.write_str("recording generation is no longer current")
            }
            Self::NotFinalizing => {
                formatter.write_str("capture has not reached its finalization boundary")
            }
            Self::PendingFrame => {
                formatter.write_str("consume the latest captured frame before finalizing")
            }
            Self::ConsumerBusy => {
                formatter.write_str("the previous captured frame is still consumer-owned")
            }
            Self::NoFramesWritten => {
                formatter.write_str("no captured frames were written to the sink")
            }
            Self::InvalidSinkReceipt => {
                formatter.write_str("sink receipt does not match this recording")
            }
            Self::WrongSession => {
                formatter.write_str("finalization permit belongs to another recording session")
            }
            Self::ClockMovedBackwards => {
                formatter.write_str("monotonic recording clock moved backwards")
            }
            Self::WorkerCapacity(message)
            | Self::WorkerStart(message)
            | Self::WorkerRetirement(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for RecordingSessionError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingSessionView {
    generation: u64,
    request: Option<RecordingRequest>,
    state: RecorderState,
    terminal: Option<CaptureTerminal>,
    worker_busy: bool,
    stop_pending: bool,
    accepted_frames: u64,
    frames_written: u64,
    mailbox_replacements: u64,
    finalization_claimed: bool,
}

impl RecordingSessionView {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn request(&self) -> Option<&RecordingRequest> {
        self.request.as_ref()
    }

    pub fn state(&self) -> &RecorderState {
        &self.state
    }

    pub const fn terminal(&self) -> Option<CaptureTerminal> {
        self.terminal
    }

    pub const fn worker_busy(&self) -> bool {
        self.worker_busy
    }

    pub const fn stop_pending(&self) -> bool {
        self.stop_pending
    }

    pub const fn accepted_frames(&self) -> u64 {
        self.accepted_frames
    }

    pub const fn frames_written(&self) -> u64 {
        self.frames_written
    }

    pub const fn mailbox_replacements(&self) -> u64 {
        self.mailbox_replacements
    }

    pub const fn finalization_claimed(&self) -> bool {
        self.finalization_claimed
    }
}

struct SessionInner {
    closed: bool,
    generation: u64,
    request: Option<RecordingRequest>,
    state: RecorderState,
    countdown_deadline: Option<Duration>,
    started_at: Option<Duration>,
    last_clock: Option<Duration>,
    stop_token: Option<CaptureStopToken>,
    pending_terminal: Option<CaptureTerminal>,
    terminal: Option<CaptureTerminal>,
    mailbox: Option<(u64, TimedFrame)>,
    consumer_lease: Option<u64>,
    next_lease_id: u64,
    next_finalization_claim_id: u64,
    finalization_claim_id: Option<u64>,
    accepted_frames: u64,
    frames_written: u64,
    mailbox_replacements: u64,
    worker: Option<WorkerSlot>,
}

enum WorkerSlot {
    Starting {
        generation: u64,
    },
    Running(WorkerOwner),
    Retiring {
        generation: u64,
        receipt: Arc<RetirementReceipt>,
        settled: Arc<AtomicBool>,
    },
}

struct WorkerOwner {
    generation: u64,
    join: JoinHandle<()>,
    reap_permit: ReapPermit,
    settled: Arc<AtomicBool>,
}

struct RetirementReceipt {
    state: AtomicU8,
}

impl RetirementReceipt {
    fn pending() -> Self {
        Self {
            state: AtomicU8::new(RETIREMENT_PENDING),
        }
    }
}

struct SessionShared {
    inner: Mutex<SessionInner>,
    stoppable_generation: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
    clock: Arc<dyn RecorderClock>,
    backend: Arc<dyn RecordingBackend>,
}

impl SessionShared {
    fn clear_stoppable_generation(&self, generation: u64) {
        let _ = self.stoppable_generation.compare_exchange(
            generation,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

/// Clears the native-hook projection before the live source is dropped on any
/// worker exit. The generation compare prevents an old source retirement from
/// clearing a newer session's admission.
struct StoppableGenerationGuard<'a> {
    shared: &'a SessionShared,
    generation: u64,
}

impl Drop for StoppableGenerationGuard<'_> {
    fn drop(&mut self) {
        self.shared.clear_stoppable_generation(self.generation);
    }
}

pub struct RecordingSession {
    shared: Arc<SessionShared>,
}

pub struct ConsumerFrame {
    shared: Weak<SessionShared>,
    generation: u64,
    lease_id: u64,
    frame: TimedFrame,
    released: bool,
}

impl ConsumerFrame {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn frame(&self) -> &TimedFrame {
        &self.frame
    }

    /// Records that the sink accepted this frame. Dropping without this call
    /// releases the single consumer lease but does not count it as written.
    pub fn acknowledge_written(mut self) -> Result<(), RecordingSessionError> {
        let Some(shared) = self.shared.upgrade() else {
            return Err(RecordingSessionError::Closed);
        };
        let mut inner = shared.inner.lock().unwrap();
        if inner.closed || inner.generation != self.generation {
            return Err(RecordingSessionError::StaleGeneration);
        }
        if inner.consumer_lease != Some(self.lease_id) {
            return Err(RecordingSessionError::StaleGeneration);
        }
        if !matches!(
            &inner.state,
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
        ) {
            return Err(RecordingSessionError::InvalidTransition);
        }
        inner.frames_written = inner.frames_written.saturating_add(1);
        inner.consumer_lease = None;
        self.released = true;
        drop(inner);
        (shared.wake)();
        Ok(())
    }
}

impl Drop for ConsumerFrame {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let released = {
            let mut inner = shared.inner.lock().unwrap();
            if inner.consumer_lease == Some(self.lease_id) {
                inner.consumer_lease = None;
                true
            } else {
                false
            }
        };
        if released {
            (shared.wake)();
        }
    }
}

impl RecordingSession {
    pub fn new(
        clock: Arc<dyn RecorderClock>,
        backend: Arc<dyn RecordingBackend>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(SessionShared {
                inner: Mutex::new(SessionInner {
                    closed: false,
                    generation: 0,
                    request: None,
                    state: RecorderState::Idle,
                    countdown_deadline: None,
                    started_at: None,
                    last_clock: None,
                    stop_token: None,
                    pending_terminal: None,
                    terminal: None,
                    mailbox: None,
                    consumer_lease: None,
                    next_lease_id: 0,
                    next_finalization_claim_id: 0,
                    finalization_claim_id: None,
                    accepted_frames: 0,
                    frames_written: 0,
                    mailbox_replacements: 0,
                    worker: None,
                }),
                stoppable_generation: Arc::new(AtomicU64::new(0)),
                wake: Arc::new(wake),
                clock,
                backend,
            }),
        }
    }

    /// Selects and freezes one source/region/options request. No worker is
    /// created until `start` or an expired countdown is polled.
    pub fn select(&self, request: RecordingRequest) -> Result<u64, RecordingSessionError> {
        self.poll()?;
        let generation = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            if inner.worker.is_some() {
                return Err(RecordingSessionError::Busy);
            }
            if matches!(&inner.state, RecorderState::Finalizing { .. }) {
                return Err(RecordingSessionError::InvalidTransition);
            }
            let generation = inner
                .generation
                .checked_add(1)
                .ok_or(RecordingSessionError::GenerationExhausted)?;
            self.shared.stoppable_generation.store(0, Ordering::Release);
            inner.generation = generation;
            inner.request = Some(request);
            inner.state = RecorderState::Selecting;
            inner.countdown_deadline = None;
            inner.started_at = None;
            inner.last_clock = None;
            inner.stop_token = None;
            inner.pending_terminal = None;
            inner.terminal = None;
            inner.finalization_claim_id = None;
            inner.mailbox = None;
            inner.accepted_frames = 0;
            inner.frames_written = 0;
            inner.mailbox_replacements = 0;
            generation
        };
        (self.shared.wake)();
        Ok(generation)
    }

    pub fn begin_countdown(&self, duration: Duration) -> Result<(), RecordingSessionError> {
        if duration > MAX_COUNTDOWN {
            return Err(RecordingSessionError::CountdownTooLong);
        }
        let now = self.shared.clock.now();
        {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            if inner.request.is_none() {
                return Err(RecordingSessionError::MissingSelection);
            }
            if !matches!(&inner.state, RecorderState::Selecting) || inner.worker.is_some() {
                return Err(RecordingSessionError::InvalidTransition);
            }
            let deadline = now
                .checked_add(duration)
                .ok_or(RecordingSessionError::CountdownTooLong)?;
            inner.countdown_deadline = Some(deadline);
            inner.state = RecorderState::Countdown {
                remaining: duration,
            };
        }
        (self.shared.wake)();
        Ok(())
    }

    /// Starts immediately from the selected state. Countdown start is driven
    /// by `poll`, so no worker exists before it expires.
    pub fn start(&self) -> Result<(), RecordingSessionError> {
        let generation = {
            let inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            if inner.request.is_none() {
                return Err(RecordingSessionError::MissingSelection);
            }
            if !matches!(&inner.state, RecorderState::Selecting) {
                return Err(RecordingSessionError::InvalidTransition);
            }
            inner.generation
        };
        self.start_worker(generation)
    }

    pub fn poll(&self) -> Result<(), RecordingSessionError> {
        self.poll_worker_retirement(false)?;
        let now = self.shared.clock.now();
        let countdown = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Ok(());
            }
            let Some(deadline) = inner.countdown_deadline else {
                return Ok(());
            };
            if !matches!(&inner.state, RecorderState::Countdown { .. }) {
                inner.countdown_deadline = None;
                return Ok(());
            }
            if now >= deadline {
                inner.countdown_deadline = None;
                inner.state = RecorderState::Selecting;
                Some(inner.generation)
            } else {
                inner.state = RecorderState::Countdown {
                    remaining: deadline - now,
                };
                None
            }
        };
        if let Some(generation) = countdown {
            self.start_worker(generation)?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<bool, RecordingSessionError> {
        let changed = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            let RecorderState::Recording {
                elapsed: accepted_elapsed,
                ..
            } = &inner.state
            else {
                return Ok(false);
            };
            if inner.pending_terminal.is_some()
                || inner
                    .stop_token
                    .as_ref()
                    .is_some_and(CaptureStopToken::stop_requested)
            {
                return Ok(false);
            }
            let Some(token) = inner.stop_token.clone() else {
                return Ok(false);
            };
            let Some(limit) = inner
                .request
                .as_ref()
                .map(|request| request.options().duration_limit())
            else {
                return Ok(false);
            };
            let now = self.shared.clock.now();
            let Some(started_at) = inner.started_at else {
                return Err(RecordingSessionError::ClockMovedBackwards);
            };
            let Some(raw_elapsed) = now.checked_sub(started_at) else {
                return Err(RecordingSessionError::ClockMovedBackwards);
            };
            if inner.last_clock.is_some_and(|observed| now < observed)
                || raw_elapsed < *accepted_elapsed
            {
                return Err(RecordingSessionError::ClockMovedBackwards);
            }
            let elapsed = limit.clamp(raw_elapsed);
            let reason = if limit.reached(elapsed) {
                RecordingEndReason::DurationLimit
            } else {
                RecordingEndReason::UserStopped
            };
            inner.last_clock = Some(now);
            token.request_stop();
            self.shared.clear_stoppable_generation(inner.generation);
            inner.pending_terminal = Some(CaptureTerminal {
                generation: inner.generation,
                reason,
                elapsed,
                accepted_frames: inner.accepted_frames,
            });
            true
        };
        if changed {
            (self.shared.wake)();
        }
        Ok(changed)
    }

    pub fn cancel(&self) -> Result<bool, RecordingSessionError> {
        let changed = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            if inner.finalization_claim_id.is_some() {
                // The consumer has crossed the explicit publication boundary.
                // Its output transaction now owns completion or failure.
                return Ok(false);
            }
            if !matches!(
                &inner.state,
                RecorderState::Selecting
                    | RecorderState::Countdown { .. }
                    | RecorderState::Recording { .. }
                    | RecorderState::Finalizing { .. }
            ) {
                return Ok(false);
            }
            let Some(next_generation) = inner.generation.checked_add(1) else {
                inner.closed = true;
                if let Some(token) = inner.stop_token.as_ref() {
                    token.cancel();
                }
                self.shared.stoppable_generation.store(0, Ordering::Release);
                inner.mailbox = None;
                inner.request = None;
                inner.pending_terminal = None;
                inner.terminal = None;
                inner.finalization_claim_id = None;
                inner.state = RecorderState::Cancelled;
                return Err(RecordingSessionError::GenerationExhausted);
            };
            inner.generation = next_generation;
            inner.countdown_deadline = None;
            inner.started_at = None;
            inner.last_clock = None;
            inner.pending_terminal = None;
            inner.terminal = None;
            inner.finalization_claim_id = None;
            if let Some(token) = inner.stop_token.as_ref() {
                token.cancel();
            }
            self.shared.stoppable_generation.store(0, Ordering::Release);
            inner.stop_token = None;
            inner.mailbox = None;
            inner.request = None;
            inner.accepted_frames = 0;
            inner.frames_written = 0;
            inner.mailbox_replacements = 0;
            // Keep consumer_lease until its owner releases it. This prevents
            // holding arbitrarily many stale raw frames across generations.
            inner.state = RecorderState::Cancelled;
            true
        };
        if changed {
            (self.shared.wake)();
        }
        Ok(changed)
    }

    /// Invalidates publication and transfers a live capture handle to the
    /// bounded reaper without waiting for a native poll to return.
    pub fn close(&self) {
        {
            let mut inner = self.shared.inner.lock().unwrap();
            if !inner.closed {
                inner.closed = true;
                self.shared.stoppable_generation.store(0, Ordering::Release);
                // A claimed finalization is already authorized to publish.
                // Closing the owner retires capture, but it must not revoke
                // that exact consumer's ability to report its output receipt.
                if inner.finalization_claim_id.is_none() {
                    inner.generation = inner.generation.saturating_add(1);
                    inner.countdown_deadline = None;
                    inner.started_at = None;
                    inner.last_clock = None;
                    inner.pending_terminal = None;
                    inner.terminal = None;
                    if let Some(token) = inner.stop_token.as_ref() {
                        token.cancel();
                    }
                    inner.stop_token = None;
                    inner.mailbox = None;
                    inner.request = None;
                    if !matches!(
                        &inner.state,
                        RecorderState::Completed(_) | RecorderState::Failed(_)
                    ) {
                        inner.state = RecorderState::Cancelled;
                    }
                }
            }
        }
        let _ = self.poll_worker_retirement(true);
        (self.shared.wake)();
    }

    pub fn take_latest_frame(
        &self,
        generation: u64,
    ) -> Result<Option<ConsumerFrame>, RecordingSessionError> {
        let mut inner = self.shared.inner.lock().unwrap();
        if inner.closed || inner.generation != generation {
            return Err(RecordingSessionError::StaleGeneration);
        }
        if !matches!(
            &inner.state,
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
        ) {
            return Err(RecordingSessionError::InvalidTransition);
        }
        if inner.consumer_lease.is_some() {
            return Err(RecordingSessionError::ConsumerBusy);
        }
        let Some((frame_generation, frame)) = inner.mailbox.take() else {
            return Ok(None);
        };
        if frame_generation != generation {
            return Err(RecordingSessionError::StaleGeneration);
        }
        let lease_id = inner
            .next_lease_id
            .checked_add(1)
            .ok_or(RecordingSessionError::LeaseIdExhausted)?;
        inner.next_lease_id = lease_id;
        inner.consumer_lease = Some(lease_id);
        drop(inner);
        Ok(Some(ConsumerFrame {
            shared: Arc::downgrade(&self.shared),
            generation,
            lease_id,
            frame,
            released: false,
        }))
    }

    pub fn claim_finalization(
        &self,
        generation: u64,
    ) -> Result<FinalizationPermit, RecordingSessionError> {
        let (claim_id, frames_written, terminal_error) = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed || inner.generation != generation {
                return Err(RecordingSessionError::StaleGeneration);
            }
            if inner.worker.is_some() {
                return Err(RecordingSessionError::Busy);
            }
            let terminal = inner
                .terminal
                .filter(|terminal| terminal.generation() == generation)
                .ok_or(RecordingSessionError::NotFinalizing)?;
            if !matches!(&inner.state, RecorderState::Finalizing { .. }) {
                return Err(RecordingSessionError::NotFinalizing);
            }
            if inner.finalization_claim_id.is_some() {
                return Err(RecordingSessionError::InvalidTransition);
            }
            if inner.mailbox.is_some() {
                return Err(RecordingSessionError::PendingFrame);
            }
            if inner.consumer_lease.is_some() {
                return Err(RecordingSessionError::ConsumerBusy);
            }
            if terminal.accepted_frames() == 0 || inner.frames_written == 0 {
                inner.state = RecorderState::Failed(RecordingFailure::NoFrames);
                (None, None, Some(RecordingSessionError::NoFramesWritten))
            } else if let Some(claim_id) = inner.next_finalization_claim_id.checked_add(1) {
                inner.next_finalization_claim_id = claim_id;
                inner.finalization_claim_id = Some(claim_id);
                (Some(claim_id), Some(inner.frames_written), None)
            } else {
                inner.state = RecorderState::Failed(RecordingFailure::InvalidFinalizationReceipt);
                (None, None, Some(RecordingSessionError::GenerationExhausted))
            }
        };
        (self.shared.wake)();
        let (claim_id, frames_written) = match (claim_id, frames_written, terminal_error) {
            (Some(claim_id), Some(frames_written), None) => (claim_id, frames_written),
            (None, None, Some(error)) => return Err(error),
            _ => return Err(RecordingSessionError::InvalidTransition),
        };
        Ok(FinalizationPermit {
            shared: Arc::clone(&self.shared),
            generation,
            claim_id,
            frames_written,
            settled: false,
        })
    }

    pub fn complete_finalization(
        &self,
        permit: &mut FinalizationPermit,
        receipt: SinkReceipt,
    ) -> Result<RecordingSummary, RecordingSessionError> {
        if !Arc::ptr_eq(&self.shared, &permit.shared) {
            return Err(RecordingSessionError::WrongSession);
        }
        let (summary, terminal_error) = {
            let mut inner = self.shared.inner.lock().unwrap();
            if permit.settled || inner.generation != permit.generation {
                return Err(RecordingSessionError::StaleGeneration);
            }
            let terminal = inner
                .terminal
                .filter(|terminal| terminal.generation() == permit.generation)
                .ok_or(RecordingSessionError::NotFinalizing)?;
            if !matches!(&inner.state, RecorderState::Finalizing { .. }) {
                return Err(RecordingSessionError::NotFinalizing);
            }
            if inner.finalization_claim_id != Some(permit.claim_id) {
                return Err(RecordingSessionError::InvalidSinkReceipt);
            }
            if inner.mailbox.is_some() {
                return Err(RecordingSessionError::PendingFrame);
            }
            if inner.consumer_lease.is_some() {
                return Err(RecordingSessionError::ConsumerBusy);
            }
            if terminal.accepted_frames() == 0 || inner.frames_written == 0 {
                inner.state = RecorderState::Failed(RecordingFailure::NoFrames);
                inner.finalization_claim_id = None;
                permit.settled = true;
                (None, Some(RecordingSessionError::NoFramesWritten))
            } else if receipt.generation() != permit.generation
                || receipt.frames_written() != inner.frames_written
            {
                inner.finalization_claim_id = None;
                inner.state = RecorderState::Failed(RecordingFailure::InvalidFinalizationReceipt);
                permit.settled = true;
                (None, Some(RecordingSessionError::InvalidSinkReceipt))
            } else {
                let summary = RecordingSummary {
                    reason: terminal.reason(),
                    elapsed: terminal.elapsed(),
                    frames_written: inner.frames_written,
                };
                inner.state = RecorderState::Completed(summary.clone());
                inner.stop_token = None;
                inner.pending_terminal = None;
                inner.terminal = None;
                inner.finalization_claim_id = None;
                permit.settled = true;
                (Some(summary), None)
            }
        };
        (self.shared.wake)();
        match (summary, terminal_error) {
            (Some(summary), None) => Ok(summary),
            (None, Some(error)) => Err(error),
            _ => unreachable!("finalization result is internally consistent"),
        }
    }

    /// Fences an unclaimed sink failure while capture may still be active.
    /// Cancellation is signalled atomically, but the worker slot remains
    /// owned until the source has dropped and the handle has been reaped.
    pub fn fail_sink(
        &self,
        generation: u64,
        message: impl Into<String>,
    ) -> Result<(), RecordingSessionError> {
        let mut inner = self.shared.inner.lock().unwrap();
        if inner.closed || inner.generation != generation {
            return Err(RecordingSessionError::StaleGeneration);
        }
        if inner.finalization_claim_id.is_some() {
            return Err(RecordingSessionError::InvalidTransition);
        }
        if !matches!(
            &inner.state,
            RecorderState::Recording { .. } | RecorderState::Finalizing { .. }
        ) {
            return Err(RecordingSessionError::InvalidTransition);
        }
        if let Some(token) = inner.stop_token.as_ref() {
            token.cancel();
        }
        self.shared.clear_stoppable_generation(generation);
        inner.mailbox = None;
        inner.pending_terminal = None;
        inner.terminal = None;
        inner.state = RecorderState::Failed(RecordingFailure::Sink(message.into()));
        drop(inner);
        (self.shared.wake)();
        Ok(())
    }

    pub fn view(&self) -> RecordingSessionView {
        let inner = self.shared.inner.lock().unwrap();
        RecordingSessionView {
            generation: inner.generation,
            request: inner.request.clone(),
            state: inner.state.clone(),
            terminal: inner.terminal,
            worker_busy: inner.worker.is_some(),
            stop_pending: inner
                .stop_token
                .as_ref()
                .is_some_and(CaptureStopToken::stop_requested),
            accepted_frames: inner.accepted_frames,
            frames_written: inner.frames_written,
            mailbox_replacements: inner.mailbox_replacements,
            finalization_claimed: inner.finalization_claim_id.is_some(),
        }
    }

    pub(crate) fn stoppable_generation_projection(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.shared.stoppable_generation)
    }

    fn start_worker(&self, generation: u64) -> Result<(), RecordingSessionError> {
        let (request, stop) = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.closed {
                return Err(RecordingSessionError::Closed);
            }
            if inner.generation != generation || !matches!(&inner.state, RecorderState::Selecting) {
                return Err(RecordingSessionError::InvalidTransition);
            }
            if inner.worker.is_some() {
                return Err(RecordingSessionError::Busy);
            }
            let request = inner
                .request
                .clone()
                .ok_or(RecordingSessionError::MissingSelection)?;
            let stop = CaptureStopToken::new();
            inner.stop_token = Some(stop.clone());
            inner.worker = Some(WorkerSlot::Starting { generation });
            (request, stop)
        };

        let reap_permit = match thread_reaper::reserve() {
            Ok(permit) => permit,
            Err(error) => {
                self.finish_worker_start_failure(
                    generation,
                    RecordingFailure::WorkerCapacity(error.to_owned()),
                );
                return Err(RecordingSessionError::WorkerCapacity(error.to_owned()));
            }
        };
        let completion = reap_permit.completion_notifier();
        let settled = Arc::new(AtomicBool::new(false));
        let worker_settled = Arc::clone(&settled);
        let shared = Arc::clone(&self.shared);
        let worker_stop = stop.clone();
        let worker_request = request.clone();
        let worker_generation = generation;
        let join = match thread::Builder::new()
            .name("screen-recorder-capture".into())
            .spawn(move || {
                let completion = completion;
                let _settled = WorkerSettled::new(worker_settled, Arc::clone(&shared.wake));
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_capture_worker(&shared, worker_generation, worker_request, worker_stop)
                }));
                let outcome = match result {
                    Ok(outcome) => outcome,
                    Err(_) => WorkerOutcome::Failed(RecordingFailure::WorkerPanicked),
                };
                publish_worker_outcome(&shared, worker_generation, outcome);
                (shared.wake)();
                drop(_settled);
                drop(completion);
            }) {
            Ok(join) => join,
            Err(error) => {
                self.finish_worker_start_failure(
                    generation,
                    RecordingFailure::WorkerStart(error.to_string()),
                );
                return Err(RecordingSessionError::WorkerStart(error.to_string()));
            }
        };
        {
            let mut inner = self.shared.inner.lock().unwrap();
            if matches!(
                &inner.worker,
                Some(WorkerSlot::Starting {
                    generation: current
                }) if *current == generation
            ) {
                inner.worker = Some(WorkerSlot::Running(WorkerOwner {
                    generation,
                    join,
                    reap_permit,
                    settled,
                }));
            } else {
                // A concurrent close/cancel can retire this generation while
                // thread creation is in flight. Transfer the exact handle.
                drop(inner);
                transfer_worker(
                    generation,
                    join,
                    reap_permit,
                    Arc::clone(&self.shared),
                    Arc::new(RetirementReceipt::pending()),
                );
                return Ok(());
            }
        }
        self.poll_worker_retirement(stop.is_cancelled())?;
        (self.shared.wake)();
        Ok(())
    }

    fn finish_worker_start_failure(&self, generation: u64, failure: RecordingFailure) {
        let mut inner = self.shared.inner.lock().unwrap();
        if inner.generation == generation && !inner.closed {
            inner.worker = None;
            inner.stop_token = None;
            inner.state = RecorderState::Failed(failure);
        } else if matches!(
            &inner.worker,
            Some(WorkerSlot::Starting {
                generation: current
            }) if *current == generation
        ) {
            inner.worker = None;
        }
        drop(inner);
        (self.shared.wake)();
    }

    fn poll_worker_retirement(&self, force_transfer: bool) -> Result<(), RecordingSessionError> {
        let transfer = {
            let mut inner = self.shared.inner.lock().unwrap();
            match inner.worker.take() {
                Some(WorkerSlot::Running(worker))
                    if force_transfer || worker.settled.load(Ordering::Acquire) =>
                {
                    let receipt = Arc::new(RetirementReceipt::pending());
                    let generation = worker.generation;
                    inner.worker = Some(WorkerSlot::Retiring {
                        generation,
                        receipt: Arc::clone(&receipt),
                        settled: Arc::clone(&worker.settled),
                    });
                    Some((worker, receipt))
                }
                Some(worker) => {
                    inner.worker = Some(worker);
                    None
                }
                None => None,
            }
        };
        if let Some((worker, receipt)) = transfer {
            transfer_worker(
                worker.generation,
                worker.join,
                worker.reap_permit,
                Arc::clone(&self.shared),
                receipt,
            );
        }

        let retirement = {
            let mut inner = self.shared.inner.lock().unwrap();
            let (generation, receipt) = match inner.worker.as_ref() {
                Some(WorkerSlot::Retiring {
                    generation,
                    receipt,
                    settled,
                }) if receipt.state.load(Ordering::Acquire) != RETIREMENT_PENDING
                    && settled.load(Ordering::Acquire) =>
                {
                    (*generation, Arc::clone(receipt))
                }
                _ => return Ok(()),
            };
            let receipt_state = receipt.state.load(Ordering::Acquire);
            inner.worker = None;
            if receipt_state == RETIREMENT_UNAVAILABLE
                && !inner.closed
                && inner.generation == generation
                && inner.finalization_claim_id.is_none()
                && !matches!(&inner.state, RecorderState::Cancelled)
                && !matches!(
                    &inner.state,
                    RecorderState::Completed(_) | RecorderState::Failed(_)
                )
            {
                let failure = RecordingFailure::WorkerRetirement(
                    "capture source returned, but its thread handle could not be joined".into(),
                );
                inner.state = RecorderState::Failed(failure.clone());
                Some(RecordingSessionError::WorkerRetirement(failure.to_string()))
            } else {
                None
            }
        };
        if let Some(error) = retirement {
            (self.shared.wake)();
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for RecordingSession {
    fn drop(&mut self) {
        self.close();
    }
}

struct WorkerSettled {
    settled: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl WorkerSettled {
    fn new(settled: Arc<AtomicBool>, wake: Arc<dyn Fn() + Send + Sync + 'static>) -> Self {
        Self { settled, wake }
    }
}

impl Drop for WorkerSettled {
    fn drop(&mut self) {
        self.settled.store(true, Ordering::Release);
        (self.wake)();
    }
}

struct RetirementTicket {
    shared: Weak<SessionShared>,
    generation: u64,
    receipt: Arc<RetirementReceipt>,
    invoked: bool,
}

impl RetirementTicket {
    fn after_join(mut self, panicked: bool) {
        if let Some(shared) = self.shared.upgrade() {
            if panicked {
                let mut inner = shared.inner.lock().unwrap();
                if !inner.closed
                    && inner.generation == self.generation
                    && inner.finalization_claim_id.is_none()
                    && !matches!(&inner.state, RecorderState::Cancelled)
                    && !matches!(
                        &inner.state,
                        RecorderState::Completed(_) | RecorderState::Failed(_)
                    )
                {
                    inner.state = RecorderState::Failed(RecordingFailure::WorkerPanicked);
                    shared.clear_stoppable_generation(self.generation);
                }
            }
            self.receipt
                .state
                .store(RETIREMENT_CONFIRMED, Ordering::Release);
            (shared.wake)();
        } else {
            self.receipt
                .state
                .store(RETIREMENT_CONFIRMED, Ordering::Release);
        }
        self.invoked = true;
    }
}

impl Drop for RetirementTicket {
    fn drop(&mut self) {
        if !self.invoked {
            self.receipt
                .state
                .store(RETIREMENT_UNAVAILABLE, Ordering::Release);
            if let Some(shared) = self.shared.upgrade() {
                (shared.wake)();
            }
        }
    }
}

fn transfer_worker(
    generation: u64,
    join: JoinHandle<()>,
    permit: ReapPermit,
    shared: Arc<SessionShared>,
    receipt: Arc<RetirementReceipt>,
) {
    let ticket = RetirementTicket {
        shared: Arc::downgrade(&shared),
        generation,
        receipt,
        invoked: false,
    };
    let _ = permit.reap_with_callback(join, move |panicked| ticket.after_join(panicked));
}

enum WorkerOutcome {
    Terminal(CaptureTerminal),
    Failed(RecordingFailure),
    Cancelled,
}

fn run_capture_worker(
    shared: &SessionShared,
    generation: u64,
    request: RecordingRequest,
    stop: CaptureStopToken,
) -> WorkerOutcome {
    if stop.is_cancelled() {
        return WorkerOutcome::Cancelled;
    }
    let mut source = match shared.backend.create_source(&request, generation) {
        Ok(source) => source,
        Err(error) => return WorkerOutcome::Failed(RecordingFailure::Source(error)),
    };
    // Declare this after `source` so it drops first: global shortcut admission
    // closes before potentially slow native source teardown begins.
    let _stoppable_generation = StoppableGenerationGuard { shared, generation };
    if stop.is_cancelled() {
        return WorkerOutcome::Cancelled;
    }
    if let Err(error) = source.start(&request, &stop) {
        return WorkerOutcome::Failed(RecordingFailure::Source(error));
    }
    if stop.is_cancelled() {
        return WorkerOutcome::Cancelled;
    }

    // Anchor after the source has actually started, never when the UI queued
    // the worker. The source drops before this function returns its terminal.
    let started_at = shared.clock.now();
    let options = request.options();
    let mut timeline = FrameTimeline::new(started_at, options);
    {
        let mut inner = shared.inner.lock().unwrap();
        if inner.closed || inner.generation != generation || stop.is_cancelled() {
            return WorkerOutcome::Cancelled;
        }
        inner.started_at = Some(started_at);
        inner.last_clock = Some(started_at);
        inner.state = RecorderState::Recording {
            elapsed: Duration::ZERO,
            accepted_frames: 0,
        };
        shared
            .stoppable_generation
            .store(generation, Ordering::Release);
    }
    (shared.wake)();

    let terminal = loop {
        if stop.is_cancelled() {
            return WorkerOutcome::Cancelled;
        }
        if let Some(terminal) = current_terminal(shared, generation) {
            break terminal;
        }
        let now = shared.clock.now();
        let elapsed = match observe_elapsed(shared, generation, now, options.duration_limit()) {
            Ok(Some(elapsed)) => elapsed,
            Ok(None) => {
                if let Some(terminal) = current_terminal(shared, generation) {
                    break terminal;
                }
                return WorkerOutcome::Cancelled;
            }
            Err(error) => return WorkerOutcome::Failed(RecordingFailure::InvalidFrame(error)),
        };
        if options.duration_limit().reached(elapsed) {
            match claim_terminal(
                shared,
                generation,
                RecordingEndReason::DurationLimit,
                now,
                options.duration_limit(),
            ) {
                Ok(terminal) => break terminal,
                Err(outcome) => return outcome,
            }
        }

        let event = source.poll(SOURCE_POLL_WAIT, &stop);
        if stop.is_cancelled() {
            return WorkerOutcome::Cancelled;
        }
        // Stop time is frozen by `stop` before the source returns. A frame or
        // target-loss event produced after that boundary cannot change it.
        if let Some(terminal) = current_terminal(shared, generation) {
            break terminal;
        }

        match event {
            CapturePoll::Frame {
                generation: frame_generation,
                frame,
            } => {
                if frame_generation != generation {
                    continue;
                }
                let frame_time = shared.clock.now();
                let Some(_) = (match observe_elapsed(
                    shared,
                    generation,
                    frame_time,
                    options.duration_limit(),
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        return WorkerOutcome::Failed(RecordingFailure::InvalidFrame(error));
                    }
                }) else {
                    if let Some(terminal) = current_terminal(shared, generation) {
                        break terminal;
                    }
                    return WorkerOutcome::Cancelled;
                };
                let stamp = match timeline.stamp(frame_time) {
                    Ok(stamp) => stamp,
                    Err(error) => {
                        return WorkerOutcome::Failed(RecordingFailure::InvalidFrame(error));
                    }
                };
                match stamp {
                    FrameStamp::Accepted(elapsed) => {
                        let mut inner = shared.inner.lock().unwrap();
                        if inner.closed
                            || inner.generation != generation
                            || stop.is_cancelled()
                            || inner.pending_terminal.is_some()
                        {
                            let candidate = inner
                                .pending_terminal
                                .filter(|terminal| terminal.generation() == generation);
                            drop(inner);
                            if let Some(terminal) = candidate {
                                break terminal;
                            }
                            return WorkerOutcome::Cancelled;
                        }
                        if frame.size() != request.canvas() {
                            let failure =
                                RecordingFailure::InvalidFrame(FrameError::WrongDimensions);
                            inner.mailbox = None;
                            shared.clear_stoppable_generation(generation);
                            inner.state = RecorderState::Failed(failure.clone());
                            return WorkerOutcome::Failed(failure);
                        }
                        if inner.mailbox.take().is_some() {
                            inner.mailbox_replacements =
                                inner.mailbox_replacements.saturating_add(1);
                        }
                        inner.accepted_frames = inner.accepted_frames.saturating_add(1);
                        inner.mailbox = Some((generation, TimedFrame::new(elapsed, frame)));
                        inner.state = RecorderState::Recording {
                            elapsed,
                            accepted_frames: inner.accepted_frames,
                        };
                        drop(inner);
                        (shared.wake)();
                    }
                    FrameStamp::DroppedForCadence => {}
                    FrameStamp::DurationLimit(_) => {
                        match claim_terminal(
                            shared,
                            generation,
                            RecordingEndReason::DurationLimit,
                            frame_time,
                            options.duration_limit(),
                        ) {
                            Ok(terminal) => break terminal,
                            Err(outcome) => return outcome,
                        }
                    }
                }
            }
            CapturePoll::Pending => {
                let now = shared.clock.now();
                let elapsed =
                    match observe_elapsed(shared, generation, now, options.duration_limit()) {
                        Ok(Some(elapsed)) => elapsed,
                        Ok(None) => {
                            if let Some(terminal) = current_terminal(shared, generation) {
                                break terminal;
                            }
                            return WorkerOutcome::Cancelled;
                        }
                        Err(error) => {
                            return WorkerOutcome::Failed(RecordingFailure::InvalidFrame(error));
                        }
                    };
                if options.duration_limit().reached(elapsed) {
                    match claim_terminal(
                        shared,
                        generation,
                        RecordingEndReason::DurationLimit,
                        now,
                        options.duration_limit(),
                    ) {
                        Ok(terminal) => break terminal,
                        Err(outcome) => return outcome,
                    }
                }
            }
            CapturePoll::Ended(reason) => {
                let now = shared.clock.now();
                match claim_terminal(shared, generation, reason, now, options.duration_limit()) {
                    Ok(terminal) => break terminal,
                    Err(outcome) => return outcome,
                }
            }
            CapturePoll::Failed(error) => {
                let now = shared.clock.now();
                let elapsed =
                    match observe_elapsed(shared, generation, now, options.duration_limit()) {
                        Ok(Some(elapsed)) => elapsed,
                        Ok(None) => {
                            if let Some(terminal) = current_terminal(shared, generation) {
                                break terminal;
                            }
                            return WorkerOutcome::Cancelled;
                        }
                        Err(error) => {
                            return WorkerOutcome::Failed(RecordingFailure::InvalidFrame(error));
                        }
                    };
                if options.duration_limit().reached(elapsed) {
                    match claim_terminal(
                        shared,
                        generation,
                        RecordingEndReason::DurationLimit,
                        now,
                        options.duration_limit(),
                    ) {
                        Ok(terminal) => break terminal,
                        Err(outcome) => return outcome,
                    }
                }
                return WorkerOutcome::Failed(RecordingFailure::Source(error));
            }
        }
    };

    // `source` is dropped here, before the terminal becomes visible to a
    // consumer that may begin sink finalization.
    drop(source);
    WorkerOutcome::Terminal(terminal)
}

fn current_terminal(shared: &SessionShared, generation: u64) -> Option<CaptureTerminal> {
    let inner = shared.inner.lock().unwrap();
    inner
        .pending_terminal
        .filter(|terminal| terminal.generation() == generation)
}

fn observe_elapsed(
    shared: &SessionShared,
    generation: u64,
    now: Duration,
    duration_limit: super::RecordingDurationLimit,
) -> Result<Option<Duration>, FrameError> {
    let mut inner = shared.inner.lock().unwrap();
    if inner.closed
        || inner.generation != generation
        || inner
            .stop_token
            .as_ref()
            .is_some_and(CaptureStopToken::is_cancelled)
        || inner.pending_terminal.is_some()
    {
        return Ok(None);
    }
    let started_at = inner.started_at.ok_or(FrameError::TimeBeforeStart)?;
    if now < started_at || inner.last_clock.is_some_and(|last| now < last) {
        return Err(FrameError::ClockMovedBackwards);
    }
    inner.last_clock = Some(now);
    let elapsed = duration_limit.clamp(now - started_at);
    if let RecorderState::Recording {
        accepted_frames, ..
    } = &inner.state
    {
        let accepted_frames = *accepted_frames;
        inner.state = RecorderState::Recording {
            elapsed,
            accepted_frames,
        };
    }
    Ok(Some(elapsed))
}

/// Claims the first terminal observation while holding the same lock that
/// accepts a user Stop. That freezes reason/time before a potentially slow
/// source destructor runs.
fn claim_terminal(
    shared: &SessionShared,
    generation: u64,
    reason: RecordingEndReason,
    now: Duration,
    duration_limit: super::RecordingDurationLimit,
) -> Result<CaptureTerminal, WorkerOutcome> {
    let mut inner = shared.inner.lock().unwrap();
    if inner.closed || inner.generation != generation {
        return Err(WorkerOutcome::Cancelled);
    }
    if inner
        .stop_token
        .as_ref()
        .is_some_and(CaptureStopToken::is_cancelled)
    {
        return Err(WorkerOutcome::Cancelled);
    }
    if let Some(terminal) = inner.pending_terminal {
        return Ok(terminal);
    }
    let Some(started_at) = inner.started_at else {
        return Err(WorkerOutcome::Failed(RecordingFailure::InvalidFrame(
            FrameError::TimeBeforeStart,
        )));
    };
    if now < started_at || inner.last_clock.is_some_and(|last| now < last) {
        return Err(WorkerOutcome::Failed(RecordingFailure::InvalidFrame(
            FrameError::ClockMovedBackwards,
        )));
    }
    let elapsed = duration_limit.clamp(now - started_at);
    let reason = if duration_limit.reached(elapsed) {
        RecordingEndReason::DurationLimit
    } else {
        reason
    };
    let terminal = CaptureTerminal {
        generation,
        reason,
        elapsed,
        accepted_frames: inner.accepted_frames,
    };
    shared.clear_stoppable_generation(generation);
    inner.last_clock = Some(now);
    inner.state = RecorderState::Recording {
        elapsed,
        accepted_frames: inner.accepted_frames,
    };
    inner.pending_terminal = Some(terminal);
    Ok(terminal)
}

fn publish_worker_outcome(shared: &SessionShared, generation: u64, outcome: WorkerOutcome) {
    let mut inner = shared.inner.lock().unwrap();
    // An obsolete worker must never clear or otherwise mutate a newer
    // generation's mailbox.
    if inner.closed || inner.generation != generation {
        return;
    }
    shared.clear_stoppable_generation(generation);
    if matches!(
        &inner.state,
        RecorderState::Failed(RecordingFailure::Sink(_))
    ) {
        return;
    }
    if inner
        .stop_token
        .as_ref()
        .is_some_and(CaptureStopToken::is_cancelled)
    {
        return;
    }
    match outcome {
        WorkerOutcome::Terminal(reported_terminal) => {
            let Some(terminal) = inner
                .pending_terminal
                .filter(|terminal| terminal.generation() == generation)
            else {
                inner.mailbox = None;
                inner.stop_token = None;
                inner.state = RecorderState::Failed(RecordingFailure::InvalidFinalizationReceipt);
                return;
            };
            if terminal != reported_terminal {
                inner.mailbox = None;
                inner.pending_terminal = None;
                inner.stop_token = None;
                inner.state = RecorderState::Failed(RecordingFailure::InvalidFinalizationReceipt);
                return;
            }
            inner.terminal = Some(terminal);
            inner.pending_terminal = None;
            inner.stop_token = None;
            if terminal.accepted_frames() == 0 {
                inner.mailbox = None;
                inner.state = RecorderState::Failed(RecordingFailure::NoFrames);
            } else {
                inner.state = RecorderState::Finalizing {
                    reason: terminal.reason(),
                    elapsed: terminal.elapsed(),
                };
            }
        }
        WorkerOutcome::Failed(failure) => {
            inner.mailbox = None;
            inner.pending_terminal = None;
            inner.terminal = None;
            inner.stop_token = None;
            inner.state = RecorderState::Failed(failure);
        }
        WorkerOutcome::Cancelled => {
            inner.mailbox = None;
            inner.pending_terminal = None;
            inner.terminal = None;
            inner.stop_token = None;
            inner.state = RecorderState::Cancelled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ScreenRect;
    use crate::recording::model::{
        CanvasSize, CaptureTarget, FrameRate, RecordingFormat, RecordingOptions, RecordingRequest,
    };
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, AtomicUsize};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};

    #[derive(Default)]
    struct FakeClock {
        nanos: AtomicU64,
    }

    impl FakeClock {
        fn set(&self, time: Duration) {
            self.nanos.store(duration_nanos(time), Ordering::SeqCst);
        }

        fn advance(&self, delta: Duration) {
            self.nanos
                .fetch_add(duration_nanos(delta), Ordering::SeqCst);
        }
    }

    impl RecorderClock for FakeClock {
        fn now(&self) -> Duration {
            Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
        }
    }

    fn duration_nanos(duration: Duration) -> u64 {
        u64::try_from(duration.as_nanos()).expect("test clock duration fits u64 nanoseconds")
    }

    type SourceFactory = dyn Fn(CanvasSize, u64) -> Box<dyn CaptureSource> + Send + Sync;

    struct FakeBackend {
        factory: Box<SourceFactory>,
        created: AtomicUsize,
    }

    impl FakeBackend {
        fn new(
            factory: impl Fn(CanvasSize, u64) -> Box<dyn CaptureSource> + Send + Sync + 'static,
        ) -> Self {
            Self {
                factory: Box::new(factory),
                created: AtomicUsize::new(0),
            }
        }

        fn created(&self) -> usize {
            self.created.load(Ordering::SeqCst)
        }
    }

    impl RecordingBackend for FakeBackend {
        fn create_source(
            &self,
            request: &RecordingRequest,
            generation: u64,
        ) -> Result<Box<dyn CaptureSource>, String> {
            self.created.fetch_add(1, Ordering::SeqCst);
            Ok((self.factory)(request.canvas(), generation))
        }
    }

    #[derive(Clone)]
    enum ScriptStep {
        Frame(Duration),
        Pending(Duration),
        Ended(Duration, RecordingEndReason),
        Failed(Duration, String),
    }

    struct ScriptSource {
        clock: Arc<FakeClock>,
        generation: u64,
        size: CanvasSize,
        steps: VecDeque<ScriptStep>,
        drop_gate: Option<(Sender<()>, Receiver<()>)>,
    }

    impl ScriptSource {
        fn new(
            clock: Arc<FakeClock>,
            generation: u64,
            size: CanvasSize,
            steps: impl IntoIterator<Item = ScriptStep>,
        ) -> Self {
            Self {
                clock,
                generation,
                size,
                steps: steps.into_iter().collect(),
                drop_gate: None,
            }
        }

        fn with_drop_gate(mut self, started: Sender<()>, release: Receiver<()>) -> Self {
            self.drop_gate = Some((started, release));
            self
        }
    }

    impl CaptureSource for ScriptSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            match self.steps.pop_front().unwrap_or(ScriptStep::Ended(
                Duration::ZERO,
                RecordingEndReason::TargetLost,
            )) {
                ScriptStep::Frame(advance) => {
                    self.clock.advance(advance);
                    CapturePoll::Frame {
                        generation: self.generation,
                        frame: frame(self.size, 1),
                    }
                }
                ScriptStep::Pending(advance) => {
                    self.clock.advance(advance);
                    CapturePoll::Pending
                }
                ScriptStep::Ended(advance, reason) => {
                    self.clock.advance(advance);
                    CapturePoll::Ended(reason)
                }
                ScriptStep::Failed(advance, message) => {
                    self.clock.advance(advance);
                    CapturePoll::Failed(message)
                }
            }
        }
    }

    impl Drop for ScriptSource {
        fn drop(&mut self) {
            if let Some((started, release)) = self.drop_gate.take() {
                let _ = started.send(());
                let _ = release.recv_timeout(Duration::from_secs(3));
            }
        }
    }

    struct GatedFrameSource {
        generation: u64,
        size: CanvasSize,
        poll_count: usize,
        entered: Sender<()>,
        release: Receiver<()>,
    }

    impl CaptureSource for GatedFrameSource {
        fn poll(&mut self, max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            match self.poll_count {
                0 => {
                    self.poll_count = 1;
                    CapturePoll::Frame {
                        generation: self.generation,
                        frame: frame(self.size, 1),
                    }
                }
                1 => {
                    let _ = self.entered.send(());
                    match self.release.recv_timeout(max_wait) {
                        Ok(()) => {
                            self.poll_count = 2;
                            CapturePoll::Frame {
                                generation: self.generation,
                                frame: frame(self.size, 2),
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => CapturePoll::Pending,
                        Err(RecvTimeoutError::Disconnected) => {
                            CapturePoll::Ended(RecordingEndReason::TargetLost)
                        }
                    }
                }
                _ => CapturePoll::Ended(RecordingEndReason::TargetLost),
            }
        }
    }

    struct BlockingPollSource {
        generation: u64,
        size: CanvasSize,
        entered: Sender<()>,
        release: Receiver<()>,
    }

    impl CaptureSource for BlockingPollSource {
        fn poll(&mut self, _max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            let _ = self.entered.send(());
            match self.release.recv_timeout(Duration::from_secs(3)) {
                Ok(()) => CapturePoll::Frame {
                    generation: self.generation,
                    frame: frame(self.size, 7),
                },
                Err(_) => CapturePoll::Pending,
            }
        }
    }

    struct HandshakeSource {
        clock: Arc<FakeClock>,
        generation: u64,
        size: CanvasSize,
        count: usize,
        index: usize,
        awaiting_ack: bool,
        ack: Receiver<()>,
    }

    impl CaptureSource for HandshakeSource {
        fn poll(&mut self, max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
            if self.awaiting_ack {
                match self.ack.recv_timeout(max_wait) {
                    Ok(()) => {
                        self.awaiting_ack = false;
                        self.clock.advance(Duration::from_millis(100));
                    }
                    Err(RecvTimeoutError::Timeout) => return CapturePoll::Pending,
                    Err(RecvTimeoutError::Disconnected) => {
                        return CapturePoll::Ended(RecordingEndReason::TargetLost);
                    }
                }
            }
            if self.index >= self.count {
                return CapturePoll::Ended(RecordingEndReason::TargetLost);
            }
            self.index += 1;
            self.awaiting_ack = true;
            CapturePoll::Frame {
                generation: self.generation,
                frame: frame(self.size, self.index as u8),
            }
        }
    }

    fn frame(size: CanvasSize, marker: u8) -> RgbaFrame {
        let mut pixels = vec![0; size.rgba_bytes()];
        if let Some(first) = pixels.first_mut() {
            *first = marker;
        }
        RgbaFrame::new(size, pixels).unwrap()
    }

    fn options(duration: Duration) -> RecordingOptions {
        RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            crate::recording::RecordingDurationLimit::Timed(duration),
            false,
        )
        .unwrap()
    }

    fn request(duration: Duration) -> RecordingRequest {
        let region = ScreenRect::new(-32, 10, 32, 24);
        let target = CaptureTarget::window(100, 200, 300, region).unwrap();
        RecordingRequest::new(target, region, options(duration)).unwrap()
    }

    fn backend(
        factory: impl Fn(CanvasSize, u64) -> Box<dyn CaptureSource> + Send + Sync + 'static,
    ) -> Arc<FakeBackend> {
        Arc::new(FakeBackend::new(factory))
    }

    fn session(clock: Arc<FakeClock>, backend: Arc<FakeBackend>) -> Arc<RecordingSession> {
        Arc::new(RecordingSession::new(clock, backend, || {}))
    }

    fn wait_for(
        session: &RecordingSession,
        predicate: impl Fn(&RecordingSessionView) -> bool,
    ) -> RecordingSessionView {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Err(error) = session.poll() {
                panic!("session poll failed while waiting: {error}");
            }
            let view = session.view();
            if predicate(&view) {
                return view;
            }
            assert!(
                Instant::now() < deadline,
                "session did not settle: {view:?}"
            );
            thread::yield_now();
        }
    }

    fn scripted_session(
        steps: Vec<ScriptStep>,
        duration: Duration,
    ) -> (Arc<RecordingSession>, Arc<FakeClock>) {
        let clock = Arc::new(FakeClock::default());
        let source_clock = Arc::clone(&clock);
        let steps = Arc::new(steps);
        let source_steps = Arc::clone(&steps);
        let backend = backend(move |size, generation| {
            Box::new(ScriptSource::new(
                Arc::clone(&source_clock),
                generation,
                size,
                source_steps.iter().cloned(),
            ))
        });
        let session = session(Arc::clone(&clock), backend);
        session.select(request(duration)).unwrap();
        (session, clock)
    }

    fn wait_finalizing(session: &RecordingSession) -> RecordingSessionView {
        wait_for(session, |view| {
            matches!(view.state(), RecorderState::Finalizing { .. })
                || matches!(view.state(), RecorderState::Failed(_))
        })
    }

    fn wait_finalizing_and_retired(session: &RecordingSession) -> RecordingSessionView {
        wait_for(session, |view| {
            (matches!(view.state(), RecorderState::Finalizing { .. }) && !view.worker_busy())
                || matches!(view.state(), RecorderState::Failed(_))
        })
    }

    fn acknowledge_latest(session: &RecordingSession, generation: u64) {
        let frame = session
            .take_latest_frame(generation)
            .unwrap()
            .expect("script publishes a frame");
        frame.acknowledge_written().unwrap();
    }

    #[test]
    fn recording_session_countdown_cancel_does_not_create_capture_source() {
        let clock = Arc::new(FakeClock::default());
        let backend = backend(|_, _| {
            Box::new(ScriptSource::new(
                Arc::new(FakeClock::default()),
                0,
                CanvasSize::new(32, 24).unwrap(),
                [],
            ))
        });
        let session = session(clock, Arc::clone(&backend));
        session.select(request(Duration::from_secs(2))).unwrap();
        session.begin_countdown(Duration::from_secs(5)).unwrap();
        assert!(session.cancel().unwrap());
        assert_eq!(backend.created(), 0);
        assert_eq!(session.view().state(), &RecorderState::Cancelled);
    }

    #[test]
    fn recording_session_latest_mailbox_keeps_elapsed_gap_and_exact_receipt() {
        let (session, _) = scripted_session(
            vec![
                ScriptStep::Frame(Duration::ZERO),
                ScriptStep::Frame(Duration::from_millis(100)),
                ScriptStep::Frame(Duration::from_millis(100)),
                ScriptStep::Ended(Duration::from_millis(40), RecordingEndReason::TargetLost),
            ],
            Duration::from_secs(2),
        );
        let generation = session.view().generation();
        session.start().unwrap();
        let view = wait_finalizing_and_retired(&session);
        assert!(matches!(view.state(), RecorderState::Finalizing { .. }));
        assert_eq!(view.accepted_frames(), 3);
        assert_eq!(view.mailbox_replacements(), 2);

        let latest = session.take_latest_frame(generation).unwrap().unwrap();
        assert_eq!(latest.frame().elapsed(), Duration::from_millis(200));
        latest.acknowledge_written().unwrap();
        let mut permit = session.claim_finalization(generation).unwrap();
        let summary = session
            .complete_finalization(&mut permit, SinkReceipt::new(generation, 1))
            .unwrap();
        assert_eq!(summary.reason, RecordingEndReason::TargetLost);
        assert_eq!(summary.elapsed, Duration::from_millis(240));
        assert_eq!(summary.frames_written, 1);
    }

    #[test]
    fn recording_session_consumer_drains_frame_while_next_source_poll_waits() {
        let clock = Arc::new(FakeClock::default());
        let (ack_tx, ack_rx) = mpsc::channel();
        let source_clock = Arc::clone(&clock);
        let ack_slot = Arc::new(Mutex::new(Some(ack_rx)));
        let source_ack = Arc::clone(&ack_slot);
        let backend = backend(move |size, generation| {
            Box::new(HandshakeSource {
                clock: Arc::clone(&source_clock),
                generation,
                size,
                count: 4,
                index: 0,
                awaiting_ack: false,
                ack: source_ack
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one recording uses the source once"),
            })
        });
        let session = session(Arc::clone(&clock), backend);
        let generation = session.select(request(Duration::from_secs(2))).unwrap();
        let consumer_session = Arc::clone(&session);
        session.start().unwrap();
        let consumer = thread::spawn(move || {
            let mut elapsed = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(4);
            loop {
                let _ = consumer_session.poll();
                match consumer_session.take_latest_frame(generation) {
                    Ok(Some(lease)) => {
                        elapsed.push(lease.frame().elapsed());
                        lease.acknowledge_written().unwrap();
                        ack_tx.send(()).unwrap();
                    }
                    Ok(None) | Err(RecordingSessionError::InvalidTransition) => {}
                    Err(error) => panic!("consumer lease failed: {error}"),
                }
                let view = consumer_session.view();
                if matches!(view.state(), RecorderState::Finalizing { .. })
                    && !view.worker_busy()
                    && consumer_session
                        .take_latest_frame(generation)
                        .unwrap()
                        .is_none()
                {
                    break;
                }
                assert!(Instant::now() < deadline, "consumer timed out: {view:?}");
                thread::yield_now();
            }
            elapsed
        });

        let elapsed = consumer.join().unwrap();
        assert_eq!(
            elapsed,
            vec![
                Duration::ZERO,
                Duration::from_millis(100),
                Duration::from_millis(200),
                Duration::from_millis(300),
            ]
        );
        assert_eq!(session.view().frames_written(), 4);
        let mut permit = session.claim_finalization(generation).unwrap();
        session
            .complete_finalization(&mut permit, SinkReceipt::new(generation, 4))
            .unwrap();
    }

    #[test]
    fn recording_session_stop_freezes_time_and_ignores_inflight_frame() {
        let clock = Arc::new(FakeClock::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_slot = Arc::new(Mutex::new(Some(release_rx)));
        let source_release = Arc::clone(&release_slot);
        let entered = entered_tx.clone();
        let backend = backend(move |size, generation| {
            Box::new(GatedFrameSource {
                generation,
                size,
                poll_count: 0,
                entered: entered.clone(),
                release: source_release
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one source per test"),
            })
        });
        let session = session(Arc::clone(&clock), backend);
        let stop_bridge = crate::recording::RecordingStopHotkeyBridge::default();
        stop_bridge.attach(&session);
        let generation = session.select(request(Duration::from_secs(2))).unwrap();
        session.start().unwrap();
        let recording = wait_for(&session, |view| view.accepted_frames() == 1);
        assert!(matches!(recording.state(), RecorderState::Recording { .. }));
        assert_eq!(stop_bridge.capture_stoppable_generation(), Some(generation));
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("source reached the in-flight second poll");

        clock.set(Duration::from_millis(100));
        assert!(session.stop().unwrap());
        assert!(matches!(
            session.view().state(),
            RecorderState::Recording { .. }
        ));
        assert_eq!(
            stop_bridge.capture_stoppable_generation(),
            None,
            "a pending Stop token closes hook admission before the visible state advances"
        );
        clock.set(Duration::from_millis(500));
        assert!(!session.stop().unwrap(), "the first Stop is immutable");
        release_tx.send(()).unwrap();

        let view = wait_finalizing_and_retired(&session);
        let terminal = view.terminal().expect("terminal is retained for consumer");
        assert_eq!(terminal.reason(), RecordingEndReason::UserStopped);
        assert_eq!(terminal.elapsed(), Duration::from_millis(100));
        assert_eq!(terminal.accepted_frames(), 1);
        assert_eq!(
            view.accepted_frames(),
            1,
            "in-flight frame was not published"
        );
        acknowledge_latest(&session, generation);
        let mut permit = session.claim_finalization(generation).unwrap();
        session
            .complete_finalization(&mut permit, SinkReceipt::new(generation, 1))
            .unwrap();
    }

    #[test]
    fn recording_session_close_retires_blocked_source_without_waiting_or_late_publish() {
        let clock = Arc::new(FakeClock::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_slot = Arc::new(Mutex::new(Some(release_rx)));
        let source_release = Arc::clone(&release_slot);
        let entered = entered_tx.clone();
        let backend = backend(move |size, generation| {
            Box::new(BlockingPollSource {
                generation,
                size,
                entered: entered.clone(),
                release: source_release
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one source per test"),
            })
        });
        let session = session(clock, backend);
        let generation = session.select(request(Duration::from_secs(2))).unwrap();
        session.start().unwrap();
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("fake source entered its blocked native-poll stand-in");

        let (closed_tx, closed_rx) = mpsc::channel();
        let closing_session = Arc::clone(&session);
        let close_thread = thread::spawn(move || {
            closing_session.close();
            let _ = closed_tx.send(());
        });
        let returned_without_release = closed_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        // Always release the fake even if the assertion fails, so a broken
        // synchronous close cannot strand the test process.
        let _ = release_tx.send(());
        close_thread.join().unwrap();
        assert!(
            returned_without_release,
            "close waited for the capture poll"
        );

        let view = wait_for(&session, |view| !view.worker_busy());
        assert_eq!(view.state(), &RecorderState::Cancelled);
        assert!(view.generation() > generation);
        assert_eq!(view.accepted_frames(), 0);
        assert!(matches!(
            session.take_latest_frame(generation),
            Err(RecordingSessionError::StaleGeneration)
        ));
    }

    #[test]
    fn recording_session_target_lost_terminal_precedes_slow_source_drop() {
        let clock = Arc::new(FakeClock::default());
        let (drop_started_tx, drop_started_rx) = mpsc::channel();
        let (drop_release_tx, drop_release_rx) = mpsc::channel();
        let source_clock = Arc::clone(&clock);
        let drop_release_slot = Arc::new(Mutex::new(Some(drop_release_rx)));
        let source_drop_release = Arc::clone(&drop_release_slot);
        let drop_started = drop_started_tx.clone();
        let backend = backend(move |size, generation| {
            let source = ScriptSource::new(
                Arc::clone(&source_clock),
                generation,
                size,
                [
                    ScriptStep::Frame(Duration::ZERO),
                    ScriptStep::Ended(Duration::from_millis(200), RecordingEndReason::TargetLost),
                ],
            )
            .with_drop_gate(
                drop_started.clone(),
                source_drop_release
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one source per test"),
            );
            Box::new(source)
        });
        let session = session(Arc::clone(&clock), backend);
        let stop_bridge = crate::recording::RecordingStopHotkeyBridge::default();
        stop_bridge.attach(&session);
        let generation = session.select(request(Duration::from_secs(2))).unwrap();
        session.start().unwrap();
        drop_started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("terminal was claimed before source destruction");
        assert_eq!(stop_bridge.capture_stoppable_generation(), None);

        clock.set(Duration::from_millis(500));
        assert!(
            !session.stop().unwrap(),
            "natural terminal already owns the end"
        );
        drop_release_tx.send(()).unwrap();
        let view = wait_finalizing(&session);
        let terminal = view.terminal().expect("terminal survives source teardown");
        assert_eq!(terminal.reason(), RecordingEndReason::TargetLost);
        assert_eq!(terminal.elapsed(), Duration::from_millis(200));
        acknowledge_latest(&session, generation);
    }

    #[test]
    fn recording_session_invalid_frame_clears_stop_admission_before_slow_source_drop() {
        let clock = Arc::new(FakeClock::default());
        let (drop_started_tx, drop_started_rx) = mpsc::channel();
        let (drop_release_tx, drop_release_rx) = mpsc::channel();
        let source_clock = Arc::clone(&clock);
        let drop_release_slot = Arc::new(Mutex::new(Some(drop_release_rx)));
        let source_drop_release = Arc::clone(&drop_release_slot);
        let drop_started = drop_started_tx.clone();
        let backend = backend(move |_, generation| {
            Box::new(
                ScriptSource::new(
                    Arc::clone(&source_clock),
                    generation,
                    CanvasSize::new(16, 16).unwrap(),
                    [ScriptStep::Frame(Duration::ZERO)],
                )
                .with_drop_gate(
                    drop_started.clone(),
                    source_drop_release
                        .lock()
                        .unwrap()
                        .take()
                        .expect("one source per test"),
                ),
            )
        });
        let session = session(Arc::clone(&clock), backend);
        let stop_bridge = crate::recording::RecordingStopHotkeyBridge::default();
        stop_bridge.attach(&session);
        session.select(request(Duration::from_secs(2))).unwrap();
        session.start().unwrap();

        drop_started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("invalid frame failed before source destruction");
        assert!(matches!(
            session.view().state(),
            RecorderState::Failed(RecordingFailure::InvalidFrame(FrameError::WrongDimensions))
        ));
        assert_eq!(stop_bridge.capture_stoppable_generation(), None);

        drop_release_tx.send(()).unwrap();
        wait_for(&session, |view| !view.worker_busy());
    }

    #[test]
    fn recording_session_unlimited_pending_stays_live_past_old_limits_until_stop() {
        struct PendingSource;
        impl CaptureSource for PendingSource {
            fn poll(&mut self, max_wait: Duration, _stop: &CaptureStopToken) -> CapturePoll {
                std::thread::park_timeout(max_wait);
                CapturePoll::Pending
            }
        }
        let clock = Arc::new(FakeClock::default());
        let backend = backend(|_, _| Box::new(PendingSource));
        let session = session(Arc::clone(&clock), backend);
        let bounds = ScreenRect::new(0, 0, 2, 2);
        let target = CaptureTarget::window(1, 1, 1, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            crate::recording::RecordingDurationLimit::Unlimited,
            false,
        )
        .unwrap();
        session
            .select(RecordingRequest::new(target, bounds, options).unwrap())
            .unwrap();
        session.start().unwrap();
        wait_for(&session, |view| {
            matches!(view.state(), RecorderState::Recording { .. })
        });
        for elapsed in [
            Duration::from_secs(121),
            Duration::from_millis(655_350),
            Duration::from_secs(1_001),
        ] {
            clock.set(elapsed);
            let view = wait_for(
                &session,
                |view| matches!(view.state(), RecorderState::Recording { elapsed: actual, .. } if *actual == elapsed),
            );
            assert!(view.terminal().is_none());
        }
        assert!(session.stop().unwrap());
        let view = wait_for(&session, |view| {
            matches!(
                view.state(),
                RecorderState::Failed(RecordingFailure::NoFrames)
            )
        });
        let terminal = view.terminal().unwrap();
        assert_eq!(terminal.reason(), RecordingEndReason::UserStopped);
        assert_eq!(terminal.elapsed(), Duration::from_secs(1_001));
    }

    #[test]
    fn recording_session_unlimited_stop_freezes_frame_time_before_source_returns() {
        let clock = Arc::new(FakeClock::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let slot = Arc::new(Mutex::new(Some(release_rx)));
        let backend = backend(move |size, generation| {
            Box::new(GatedFrameSource {
                generation,
                size,
                poll_count: 0,
                entered: entered_tx.clone(),
                release: slot.lock().unwrap().take().unwrap(),
            })
        });
        let session = session(Arc::clone(&clock), backend);
        let bounds = ScreenRect::new(0, 0, 2, 2);
        let target = CaptureTarget::window(1, 1, 1, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            crate::recording::RecordingDurationLimit::Unlimited,
            false,
        )
        .unwrap();
        session
            .select(RecordingRequest::new(target, bounds, options).unwrap())
            .unwrap();
        session.start().unwrap();
        wait_for(&session, |view| view.accepted_frames() == 1);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        clock.set(Duration::from_secs(1001));
        assert!(session.stop().unwrap());
        clock.set(Duration::from_secs(1100));
        release_tx.send(()).unwrap();
        let view = wait_finalizing_and_retired(&session);
        let terminal = view.terminal().unwrap();
        assert_eq!(terminal.reason(), RecordingEndReason::UserStopped);
        assert_eq!(terminal.elapsed(), Duration::from_secs(1001));
        assert_eq!(terminal.accepted_frames(), 1);
    }

    #[test]
    fn recording_session_unlimited_frame_target_loss_keeps_real_terminal_time() {
        let clock = Arc::new(FakeClock::default());
        let source_clock = Arc::clone(&clock);
        let backend = backend(move |size, generation| {
            Box::new(ScriptSource::new(
                Arc::clone(&source_clock),
                generation,
                size,
                [
                    ScriptStep::Frame(Duration::ZERO),
                    ScriptStep::Ended(Duration::from_secs(1001), RecordingEndReason::TargetLost),
                ],
            ))
        });
        let session = session(clock, backend);
        let bounds = ScreenRect::new(0, 0, 2, 2);
        let target = CaptureTarget::window(1, 1, 1, bounds).unwrap();
        let options = RecordingOptions::new(
            RecordingFormat::Gif,
            FrameRate::new(10).unwrap(),
            crate::recording::RecordingDurationLimit::Unlimited,
            false,
        )
        .unwrap();
        session
            .select(RecordingRequest::new(target, bounds, options).unwrap())
            .unwrap();
        session.start().unwrap();
        let view = wait_finalizing(&session);
        let terminal = view.terminal().unwrap();
        assert_eq!(terminal.reason(), RecordingEndReason::TargetLost);
        assert_eq!(terminal.elapsed(), Duration::from_secs(1001));
        assert_eq!(terminal.accepted_frames(), 1);
    }

    #[test]
    fn recording_session_finite_pending_stops_at_exact_17_and_1000_seconds() {
        for seconds in [17, 1_000] {
            let (session, _) = scripted_session(
                vec![ScriptStep::Pending(Duration::from_secs(seconds + 1))],
                Duration::from_secs(seconds),
            );
            session.start().unwrap();
            let view = wait_for(&session, |view| {
                matches!(
                    view.state(),
                    RecorderState::Failed(RecordingFailure::NoFrames)
                )
            });
            let terminal = view.terminal().unwrap();
            assert_eq!(terminal.reason(), RecordingEndReason::DurationLimit);
            assert_eq!(terminal.elapsed(), Duration::from_secs(seconds));
        }
    }

    #[test]
    fn recording_session_duration_limit_on_pending_source_fails_without_frames() {
        let (session, _) = scripted_session(
            vec![ScriptStep::Pending(Duration::from_secs(1))],
            Duration::from_secs(1),
        );
        session.start().unwrap();
        let view = wait_for(&session, |view| {
            matches!(view.state(), RecorderState::Failed(_))
        });
        assert_eq!(
            view.state(),
            &RecorderState::Failed(RecordingFailure::NoFrames)
        );
        let terminal = view.terminal().expect("duration terminal is retained");
        assert_eq!(terminal.reason(), RecordingEndReason::DurationLimit);
        assert_eq!(terminal.elapsed(), Duration::from_secs(1));
    }

    fn one_frame_finalizing_session() -> (Arc<RecordingSession>, u64) {
        let (session, _) = scripted_session(
            vec![
                ScriptStep::Frame(Duration::ZERO),
                ScriptStep::Ended(Duration::from_millis(10), RecordingEndReason::TargetLost),
            ],
            Duration::from_secs(2),
        );
        let generation = session.view().generation();
        session.start().unwrap();
        let view = wait_finalizing_and_retired(&session);
        assert!(matches!(view.state(), RecorderState::Finalizing { .. }));
        acknowledge_latest(&session, generation);
        (session, generation)
    }

    #[test]
    fn recording_session_finalization_permit_is_session_bound_and_close_safe() {
        let (first, first_generation) = one_frame_finalizing_session();
        let (second, second_generation) = one_frame_finalizing_session();
        let settled = wait_for(&first, |view| {
            matches!(view.state(), RecorderState::Finalizing { .. }) && !view.worker_busy()
        });
        assert!(matches!(settled.state(), RecorderState::Finalizing { .. }));
        assert_eq!(
            first.select(request(Duration::from_secs(2))),
            Err(RecordingSessionError::InvalidTransition),
            "joining the capture worker does not reopen an active finalization"
        );
        let mut permit = first.claim_finalization(first_generation).unwrap();

        assert!(
            !first.cancel().unwrap(),
            "claimed publication cannot cancel"
        );
        assert_eq!(
            second.complete_finalization(&mut permit, SinkReceipt::new(second_generation, 1),),
            Err(RecordingSessionError::WrongSession)
        );
        assert!(matches!(
            second.view().state(),
            RecorderState::Finalizing { .. }
        ));

        first.close();
        let summary = first
            .complete_finalization(&mut permit, SinkReceipt::new(first_generation, 1))
            .unwrap();
        assert_eq!(summary.frames_written, 1);
        assert!(matches!(first.view().state(), RecorderState::Completed(_)));

        assert!(
            second.cancel().unwrap(),
            "unclaimed finalization remains cancellable"
        );
        assert_eq!(second.view().state(), &RecorderState::Cancelled);
    }

    #[test]
    fn recording_session_abandoned_permit_fails_and_unclaimed_failure_is_separate() {
        let (session, generation) = one_frame_finalizing_session();
        let permit = session.claim_finalization(generation).unwrap();
        assert_eq!(
            session.fail_sink(generation, "unclaimed failure"),
            Err(RecordingSessionError::InvalidTransition)
        );
        drop(permit);
        match session.view().state() {
            RecorderState::Failed(RecordingFailure::Sink(message)) => {
                assert!(message.contains("abandoned"));
            }
            state => panic!("abandoned permit left unexpected state {state:?}"),
        }
    }

    #[test]
    fn recording_session_output_failure_during_capture_stops_source_and_keeps_sink_error() {
        let clock = Arc::new(FakeClock::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_slot = Arc::new(Mutex::new(Some(release_rx)));
        let source_release = Arc::clone(&release_slot);
        let backend = backend(move |size, generation| {
            Box::new(GatedFrameSource {
                generation,
                size,
                poll_count: 0,
                entered: entered_tx.clone(),
                release: source_release
                    .lock()
                    .unwrap()
                    .take()
                    .expect("one recording uses the source once"),
            })
        });
        let session = session(clock, backend);
        let generation = session.select(request(Duration::from_secs(2))).unwrap();
        session.start().unwrap();
        entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("capture source should block in its second poll");

        let frame = session
            .take_latest_frame(generation)
            .unwrap()
            .expect("capture should publish its initial frame");
        frame.acknowledge_written().unwrap();
        session
            .fail_sink(generation, "encoder rejected a frame")
            .unwrap();
        let failed = session.view();
        assert!(matches!(
            failed.state(),
            RecorderState::Failed(RecordingFailure::Sink(message))
                if message == "encoder rejected a frame"
        ));
        assert!(failed.worker_busy(), "the source stays owned until reaped");
        assert!(!failed.finalization_claimed());

        release_tx.send(()).unwrap();
        let retired = wait_for(&session, |view| !view.worker_busy());
        assert!(matches!(
            retired.state(),
            RecorderState::Failed(RecordingFailure::Sink(message))
                if message == "encoder rejected a frame"
        ));
        assert_eq!(retired.frames_written(), 1);
    }

    #[test]
    fn recording_session_unsettled_failed_retirement_keeps_worker_slot() {
        let clock = Arc::new(FakeClock::default());
        let backend = backend(|_, _| {
            Box::new(ScriptSource::new(
                Arc::new(FakeClock::default()),
                0,
                CanvasSize::new(32, 24).unwrap(),
                [],
            ))
        });
        let session = session(clock, backend);
        let settled = Arc::new(AtomicBool::new(false));
        let receipt = Arc::new(RetirementReceipt::pending());
        receipt
            .state
            .store(RETIREMENT_UNAVAILABLE, Ordering::Release);
        {
            let mut inner = session.shared.inner.lock().unwrap();
            inner.generation = 7;
            inner.state = RecorderState::Selecting;
            inner.worker = Some(WorkerSlot::Retiring {
                generation: 7,
                receipt: Arc::clone(&receipt),
                settled: Arc::clone(&settled),
            });
        }

        session.poll_worker_retirement(false).unwrap();
        assert!(session.view().worker_busy());
        assert_eq!(session.view().state(), &RecorderState::Selecting);

        settled.store(true, Ordering::Release);
        assert!(matches!(
            session.poll_worker_retirement(false),
            Err(RecordingSessionError::WorkerRetirement(_))
        ));
        assert!(!session.view().worker_busy());
        assert!(matches!(
            session.view().state(),
            RecorderState::Failed(RecordingFailure::WorkerRetirement(_))
        ));
    }

    #[test]
    fn recording_session_stale_worker_cannot_clear_new_generation_mailbox() {
        let clock = Arc::new(FakeClock::default());
        let backend = backend(|_, generation| {
            Box::new(ScriptSource::new(
                Arc::new(FakeClock::default()),
                generation,
                CanvasSize::new(32, 24).unwrap(),
                [],
            ))
        });
        let session = session(clock, backend);
        let expected = TimedFrame::new(
            Duration::from_millis(30),
            frame(CanvasSize::new(32, 24).unwrap(), 9),
        );
        {
            let mut inner = session.shared.inner.lock().unwrap();
            inner.generation = 2;
            inner.state = RecorderState::Recording {
                elapsed: Duration::from_millis(30),
                accepted_frames: 1,
            };
            inner.mailbox = Some((2, expected.clone()));
        }
        publish_worker_outcome(&session.shared, 1, WorkerOutcome::Cancelled);

        let inner = session.shared.inner.lock().unwrap();
        assert_eq!(inner.generation, 2);
        assert!(
            inner
                .mailbox
                .as_ref()
                .is_some_and(|(generation, frame)| *generation == 2 && frame == &expected)
        );
        assert!(matches!(&inner.state, RecorderState::Recording { .. }));
    }
}
