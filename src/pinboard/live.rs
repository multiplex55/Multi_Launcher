//! Bounded window-content capture sessions and their latest completed frame.
//!
//! The pinboard owns the immutable RGBA leases. Each live session owns a
//! source-bound worker, while its shared frame slot is the only publication
//! path used by future pin-window callbacks.

use super::{ImageBudgetReservation, PinImageHandle};
use crate::pinboard::PinCaptureGeneration;
use crate::pinboard::source::{PinSourceBinding, PinSourceCrop};
use image::RgbaImage;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MIB: usize = 1024 * 1024;
const FRAME_POOL_BUFFERS: usize = 2;
const FRAME_INTERVAL: Duration = Duration::from_millis(100);
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_GPU_BYTES: usize = 256 * MIB;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinLiveStatus {
    Live,
    Paused,
    TemporarilyUnavailable(String),
    SourceClosed,
    Error(String),
}

/// Child viewport repaint hook. The presenter supplies a notifier scoped to
/// one child pin; live publication never wakes the launcher root.
pub(crate) type PinLiveChildWake = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct PinLiveFrameLease {
    pub sequence: u64,
    pub image: PinImageHandle,
    pub source_content_size: (u32, u32),
}

impl fmt::Debug for PinLiveFrameLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinLiveFrameLease")
            .field("sequence", &self.sequence)
            .field("dimensions", &self.image.pixels().dimensions())
            .field("source_content_size", &self.source_content_size)
            .finish()
    }
}

struct FrameSlotState {
    frame: Option<PinLiveFrameLease>,
    status: PinLiveStatus,
    cancelled: bool,
    publication_epoch: u64,
}

/// Stable, bounded latest-frame handoff. Publishing replaces the old lease;
/// consumers that still hold that lease remain charged until they drop it.
struct PinLiveFrameSlot {
    generation: PinCaptureGeneration,
    state: Mutex<FrameSlotState>,
    child_wake: Mutex<Option<PinLiveChildWake>>,
}

impl PinLiveFrameSlot {
    fn new(generation: PinCaptureGeneration) -> Self {
        Self {
            generation,
            child_wake: Mutex::new(None),
            state: Mutex::new(FrameSlotState {
                frame: None,
                status: PinLiveStatus::TemporarilyUnavailable(
                    "Waiting for the first source frame".into(),
                ),
                cancelled: false,
                publication_epoch: 0,
            }),
        }
    }

    fn publish(
        &self,
        generation: PinCaptureGeneration,
        publication_epoch: u64,
        frame: PinLiveFrameLease,
    ) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if generation != self.generation
            || state.cancelled
            || state.publication_epoch != publication_epoch
            || matches!(state.status, PinLiveStatus::Paused)
            || matches!(
                state.status,
                PinLiveStatus::SourceClosed | PinLiveStatus::Error(_)
            )
            || state
                .frame
                .as_ref()
                .is_some_and(|current| frame.sequence <= current.sequence)
        {
            return false;
        }
        state.frame = Some(frame);
        if !matches!(state.status, PinLiveStatus::Paused) {
            state.status = PinLiveStatus::Live;
        }
        drop(state);
        self.notify_child();
        true
    }

    fn current_frame(&self) -> Option<PinLiveFrameLease> {
        let state = self.state.lock().ok()?;
        (!state.cancelled).then(|| state.frame.clone()).flatten()
    }

    fn status(&self) -> PinLiveStatus {
        self.state
            .lock()
            .map(|state| state.status.clone())
            .unwrap_or_else(|_| PinLiveStatus::Error("live pin state is unavailable".into()))
    }

    fn set_status(&self, generation: PinCaptureGeneration, status: PinLiveStatus) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if generation != self.generation || state.cancelled {
            return false;
        }
        if is_terminal_status(&state.status) {
            return state.status == status;
        }
        if matches!(status, PinLiveStatus::Paused) != matches!(state.status, PinLiveStatus::Paused)
            || is_terminal_status(&status)
        {
            let Some(next_epoch) = state.publication_epoch.checked_add(1) else {
                state.status =
                    PinLiveStatus::Error("live frame publication epoch exhausted".into());
                return false;
            };
            state.publication_epoch = next_epoch;
        }
        let changed = state.status != status;
        state.status = status;
        drop(state);
        if changed {
            self.notify_child();
        }
        true
    }

    fn set_child_wake_notifier(&self, notifier: Option<PinLiveChildWake>) {
        if let Ok(mut child_wake) = self.child_wake.lock() {
            *child_wake = notifier;
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

    fn publication_epoch(&self) -> u64 {
        self.state
            .lock()
            .map(|state| state.publication_epoch)
            .unwrap_or(u64::MAX)
    }

    fn cancel(&self, generation: PinCaptureGeneration) {
        if generation == self.generation
            && let Ok(mut state) = self.state.lock()
        {
            state.cancelled = true;
        }
    }

    fn has_frame(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.frame.is_some() && !state.cancelled)
    }
}

fn is_terminal_status(status: &PinLiveStatus) -> bool {
    matches!(
        status,
        PinLiveStatus::SourceClosed | PinLiveStatus::Error(_)
    )
}

#[derive(Default)]
struct WorkerFlags {
    cancelled: bool,
    paused: bool,
    activated: bool,
    frame_arrived: bool,
    source_closed: bool,
}

struct WorkerSignal {
    flags: Mutex<WorkerFlags>,
    frame_changed: Condvar,
    control_changed: Condvar,
}

impl WorkerSignal {
    fn new() -> Self {
        Self {
            flags: Mutex::new(WorkerFlags::default()),
            frame_changed: Condvar::new(),
            control_changed: Condvar::new(),
        }
    }

    fn notify_frame(&self) {
        if let Ok(mut flags) = self.flags.lock() {
            flags.frame_arrived = true;
            self.frame_changed.notify_one();
        }
    }

    fn notify_source_closed(&self) {
        if let Ok(mut flags) = self.flags.lock() {
            flags.source_closed = true;
            self.frame_changed.notify_all();
            self.control_changed.notify_all();
        }
    }

    fn cancel(&self) {
        if let Ok(mut flags) = self.flags.lock() {
            flags.cancelled = true;
            self.frame_changed.notify_all();
            self.control_changed.notify_all();
        }
    }

    fn set_paused(&self, paused: bool) -> bool {
        let Ok(mut flags) = self.flags.lock() else {
            return false;
        };
        if flags.cancelled || !flags.activated || flags.paused == paused {
            return false;
        }
        flags.paused = paused;
        self.frame_changed.notify_all();
        self.control_changed.notify_all();
        true
    }

    fn activate(&self) -> bool {
        let Ok(mut flags) = self.flags.lock() else {
            return false;
        };
        if flags.cancelled || flags.activated {
            return false;
        }
        flags.activated = true;
        // The native frame pool may already contain frames that arrived while
        // the pin was waiting for its creation transaction to restore the
        // launcher. Schedule one drain after activation so those buffered
        // frames do not have to produce another FrameArrived event.
        flags.frame_arrived = true;
        self.frame_changed.notify_all();
        self.control_changed.notify_all();
        true
    }

    fn snapshot(&self) -> WorkerFlagsSnapshot {
        self.flags
            .lock()
            .map(|flags| WorkerFlagsSnapshot {
                cancelled: flags.cancelled,
                paused: flags.paused,
                activated: flags.activated,
                frame_arrived: flags.frame_arrived,
                source_closed: flags.source_closed,
            })
            .unwrap_or(WorkerFlagsSnapshot {
                cancelled: true,
                paused: false,
                activated: false,
                frame_arrived: false,
                source_closed: false,
            })
    }

    fn wait_until_activated(&self) -> WorkerFlagsSnapshot {
        let Ok(flags) = self.flags.lock() else {
            return self.snapshot();
        };
        let result = self.control_changed.wait_while(flags, |flags| {
            !flags.cancelled && !flags.activated && !flags.source_closed
        });
        match result {
            Ok(flags) => WorkerFlagsSnapshot::from(&*flags),
            Err(_) => self.snapshot(),
        }
    }

    fn wait_while_paused(&self) -> WorkerFlagsSnapshot {
        let Ok(flags) = self.flags.lock() else {
            return self.snapshot();
        };
        let result = self.control_changed.wait_while(flags, |flags| {
            !flags.cancelled && flags.paused && !flags.source_closed
        });
        match result {
            Ok(flags) => WorkerFlagsSnapshot::from(&*flags),
            Err(_) => self.snapshot(),
        }
    }

    fn wait_for_frame_or_control(&self, timeout: Duration) -> WorkerFlagsSnapshot {
        let Ok(flags) = self.flags.lock() else {
            return self.snapshot();
        };
        let result = self
            .frame_changed
            .wait_timeout_while(flags, timeout, |flags| {
                !flags.cancelled && !flags.paused && !flags.frame_arrived && !flags.source_closed
            });
        match result {
            Ok((mut flags, _)) => {
                let snapshot = WorkerFlagsSnapshot::from(&*flags);
                flags.frame_arrived = false;
                snapshot
            }
            Err(_) => self.snapshot(),
        }
    }
}

#[derive(Clone, Copy)]
struct WorkerFlagsSnapshot {
    cancelled: bool,
    paused: bool,
    activated: bool,
    frame_arrived: bool,
    source_closed: bool,
}

impl From<&WorkerFlags> for WorkerFlagsSnapshot {
    fn from(flags: &WorkerFlags) -> Self {
        Self {
            cancelled: flags.cancelled,
            paused: flags.paused,
            activated: flags.activated,
            frame_arrived: flags.frame_arrived,
            source_closed: flags.source_closed,
        }
    }
}

/// Cloneable, generation-scoped access to a live pin's single frame slot and
/// controls. The capture session remains the sole worker owner; UI consumers
/// never retain an initial seed image or a second frame source.
#[derive(Clone)]
pub(crate) struct PinLiveFrameSource {
    generation: PinCaptureGeneration,
    slot: Arc<PinLiveFrameSlot>,
    signal: Arc<WorkerSignal>,
    worker_running: Arc<AtomicBool>,
}

impl PinLiveFrameSource {
    pub(crate) fn current_frame(&self) -> Option<PinLiveFrameLease> {
        self.slot.current_frame()
    }

    pub(crate) fn status(&self) -> PinLiveStatus {
        self.slot.status()
    }

    pub(crate) fn set_child_wake_notifier(&self, notifier: Option<PinLiveChildWake>) {
        self.slot.set_child_wake_notifier(notifier);
    }

    pub(crate) fn pause(&self) -> bool {
        if !self.worker_running.load(Ordering::Acquire)
            || is_terminal_status(&self.status())
            || !self.slot.has_frame()
        {
            return false;
        }
        let changed = self.signal.set_paused(true);
        changed && self.slot.set_status(self.generation, PinLiveStatus::Paused)
    }

    pub(crate) fn resume(&self) -> bool {
        if !self.worker_running.load(Ordering::Acquire)
            || is_terminal_status(&self.status())
            || !self.slot.has_frame()
        {
            return false;
        }
        let changed = self.signal.set_paused(false);
        changed && self.slot.set_status(self.generation, PinLiveStatus::Live)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.signal.snapshot().cancelled
    }
}

type LiveWorker = Box<
    dyn FnOnce(
            PinSourceBinding,
            PinCaptureGeneration,
            ImageBudgetReservation,
            Arc<SharedGpuBudget>,
            Arc<WorkerSignal>,
            Arc<PinLiveFrameSlot>,
            SyncSender<Result<(), String>>,
        ) -> Result<(), String>
        + Send,
>;

/// One source-bound live capture lifetime. The worker owns native objects and
/// stops publishing as soon as this session is cancelled or its pin closes.
pub(crate) struct PinLiveCaptureSession {
    binding: PinSourceBinding,
    source: PinLiveFrameSource,
    first_frame: Receiver<Result<(), String>>,
    first_frame_reported: bool,
    worker: Option<JoinHandle<()>>,
}

impl fmt::Debug for PinLiveCaptureSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinLiveCaptureSession")
            .field("generation", &self.source.generation.get())
            .field("binding", &self.binding)
            .field("status", &self.source.status())
            .finish_non_exhaustive()
    }
}

impl PinLiveCaptureSession {
    fn spawn(
        binding: PinSourceBinding,
        generation: PinCaptureGeneration,
        reservation: ImageBudgetReservation,
        gpu_budget: Arc<SharedGpuBudget>,
        worker: LiveWorker,
    ) -> Result<Self, String> {
        let slot = Arc::new(PinLiveFrameSlot::new(generation));
        let signal = Arc::new(WorkerSignal::new());
        let worker_running = Arc::new(AtomicBool::new(true));
        let (first_frame_tx, first_frame) = mpsc::sync_channel(1);
        let thread_binding = binding;
        let thread_slot = Arc::clone(&slot);
        let thread_signal = Arc::clone(&signal);
        let thread_worker_running = Arc::clone(&worker_running);
        let join = thread::Builder::new()
            .name(format!("pin-live-{}", generation.get()))
            .spawn(move || {
                let result = worker(
                    thread_binding,
                    generation,
                    reservation,
                    gpu_budget,
                    Arc::clone(&thread_signal),
                    Arc::clone(&thread_slot),
                    first_frame_tx.clone(),
                );
                if let Err(error) = result {
                    thread_slot.set_status(generation, PinLiveStatus::Error(error.clone()));
                    let _ = first_frame_tx.try_send(Err(error));
                } else if !thread_slot.has_frame() {
                    let error = "window capture stopped before its first frame".to_owned();
                    thread_slot.set_status(generation, PinLiveStatus::Error(error.clone()));
                    let _ = first_frame_tx.try_send(Err(error));
                } else if !thread_signal.snapshot().cancelled
                    && !is_terminal_status(&thread_slot.status())
                {
                    thread_slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable("live capture worker stopped".into()),
                    );
                }
                thread_worker_running.store(false, Ordering::Release);
            })
            .map_err(|error| format!("could not start the live capture worker: {error}"))?;
        Ok(Self {
            binding,
            source: PinLiveFrameSource {
                generation,
                slot,
                signal,
                worker_running,
            },
            first_frame,
            first_frame_reported: false,
            worker: Some(join),
        })
    }

    pub(crate) fn binding(&self) -> PinSourceBinding {
        self.binding
    }

    pub(crate) fn current_frame(&self) -> Option<PinLiveFrameLease> {
        self.source.current_frame()
    }

    pub(crate) fn status(&self) -> PinLiveStatus {
        self.source.status()
    }

    pub(crate) fn set_child_wake_notifier(&self, notifier: Option<PinLiveChildWake>) {
        self.source.set_child_wake_notifier(notifier);
    }

    pub(crate) fn frame_source(&self) -> PinLiveFrameSource {
        self.source.clone()
    }

    pub(crate) fn has_first_frame(&self) -> bool {
        self.source.slot.has_frame()
    }

    pub(crate) fn try_first_frame_result(&mut self) -> Option<Result<(), String>> {
        if self.first_frame_reported {
            return None;
        }
        match self.first_frame.try_recv() {
            Ok(result) => {
                self.first_frame_reported = true;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.first_frame_reported = true;
                Some(Err(
                    "window capture worker stopped before its first frame".into()
                ))
            }
        }
    }

    #[cfg(test)]
    fn wait_first_frame_result(&mut self, timeout: Duration) -> Option<Result<(), String>> {
        if self.first_frame_reported {
            return None;
        }
        let result = match self.first_frame.recv_timeout(timeout) {
            Ok(result) => Some(result),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => Some(Err(
                "window capture worker stopped before its first frame".into(),
            )),
        };
        if result.is_some() {
            self.first_frame_reported = true;
        }
        result
    }

    pub(crate) fn activate(&self) -> bool {
        if !self.has_first_frame()
            || !self.source.worker_running.load(Ordering::Acquire)
            || is_terminal_status(&self.status())
        {
            return false;
        }
        self.source.signal.activate()
    }

    pub(crate) fn pause(&self) -> bool {
        self.source.pause()
    }

    pub(crate) fn resume(&self) -> bool {
        self.source.resume()
    }

    pub(crate) fn cancel(&self) {
        self.source.signal.cancel();
        self.source.slot.cancel(self.source.generation);
    }

    pub(crate) fn cancel_and_join(&mut self) -> Result<(), String> {
        if let Some(worker) = self.cancel_and_take_worker() {
            worker
                .join()
                .map_err(|_| "window capture worker terminated unexpectedly".to_owned())?;
        }
        Ok(())
    }

    pub(crate) fn cancel_and_take_worker(&mut self) -> Option<JoinHandle<()>> {
        self.cancel();
        self.worker.take()
    }
}

impl Drop for PinLiveCaptureSession {
    fn drop(&mut self) {
        self.cancel();
        // Closing one child must not block the root UI on native teardown. The
        // worker owns its resources and exits after the cancellation signal.
        self.worker.take();
    }
}

pub(crate) trait PinLiveCaptureBackend: Send + Sync {
    fn start(
        &self,
        binding: PinSourceBinding,
        generation: PinCaptureGeneration,
        reservation: ImageBudgetReservation,
        gpu_budget: Arc<SharedGpuBudget>,
    ) -> Result<PinLiveCaptureSession, String>;
}

#[derive(Default)]
pub(crate) struct NativePinLiveCaptureBackend;

impl PinLiveCaptureBackend for NativePinLiveCaptureBackend {
    fn start(
        &self,
        binding: PinSourceBinding,
        generation: PinCaptureGeneration,
        reservation: ImageBudgetReservation,
        gpu_budget: Arc<SharedGpuBudget>,
    ) -> Result<PinLiveCaptureSession, String> {
        PinLiveCaptureSession::spawn(
            binding,
            generation,
            reservation,
            gpu_budget,
            Box::new(run_native_capture),
        )
    }
}

/// GPU frame-pool and crop staging textures have their own fixed budget. CPU
/// image leases are accounted separately by `SharedImageBudget`.
pub(crate) struct SharedGpuBudget {
    limit: usize,
    used: std::sync::atomic::AtomicUsize,
}

impl Default for SharedGpuBudget {
    fn default() -> Self {
        Self {
            limit: MAX_GPU_BYTES,
            used: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl SharedGpuBudget {
    fn reserve(self: &Arc<Self>, bytes: usize) -> Result<GpuBudgetReservation, String> {
        self.used
            .try_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |used| used.checked_add(bytes).filter(|total| *total <= self.limit),
            )
            .map(|_| GpuBudgetReservation {
                budget: Arc::clone(self),
                bytes,
            })
            .map_err(|used| {
                format!(
                    "live window capture needs {bytes} GPU bytes; {} remain",
                    self.limit.saturating_sub(used)
                )
            })
    }
}

struct GpuBudgetReservation {
    budget: Arc<SharedGpuBudget>,
    bytes: usize,
}

impl GpuBudgetReservation {
    fn resize(&mut self, bytes: usize) -> Result<(), String> {
        if bytes > self.bytes {
            let additional = bytes - self.bytes;
            self.budget
                .used
                .try_update(
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                    |used| {
                        used.checked_add(additional)
                            .filter(|total| *total <= self.budget.limit)
                    },
                )
                .map_err(|used| {
                    format!(
                        "resized window needs {bytes} GPU bytes; {} remain",
                        self.budget.limit.saturating_sub(used)
                    )
                })?;
        } else if bytes < self.bytes {
            self.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    fn release(&self, bytes: usize) {
        let result = self.budget.used.try_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |used| used.checked_sub(bytes),
        );
        debug_assert!(result.is_ok(), "GPU budget released exactly once");
    }
}

impl Drop for GpuBudgetReservation {
    fn drop(&mut self) {
        self.release(self.bytes);
    }
}

#[derive(Default)]
struct FrameCadence {
    last_completed: Option<Duration>,
}

impl FrameCadence {
    fn wait_remaining(&self, now: Duration) -> Option<Duration> {
        let last = self.last_completed?;
        let elapsed = now.saturating_sub(last);
        (elapsed < FRAME_INTERVAL).then(|| FRAME_INTERVAL - elapsed)
    }

    fn record_completed(&mut self, now: Duration) {
        self.last_completed = Some(now);
    }
}

fn validate_source_frame_geometry(
    binding: PinSourceBinding,
    content_size: (u32, u32),
    expected_initial: bool,
) -> Result<(), String> {
    if content_size.0 == 0 || content_size.1 == 0 {
        return Err("the source window has no capturable content".into());
    }
    let crop = binding.crop;
    let right = u64::from(crop.x) + u64::from(crop.width);
    let bottom = u64::from(crop.y) + u64::from(crop.height);
    if right > u64::from(content_size.0) || bottom > u64::from(content_size.1) {
        return Err("the selected crop is outside the resized source window".into());
    }
    if expected_initial
        && content_size
            != (
                binding.captured_bounds.width,
                binding.captured_bounds.height,
            )
    {
        return Err(format!(
            "source frame is {}×{} but the selected window frame was {}×{}; live crop origin cannot be verified",
            content_size.0,
            content_size.1,
            binding.captured_bounds.width,
            binding.captured_bounds.height
        ));
    }
    Ok(())
}

fn estimate_gpu_bytes(source_size: (u32, u32), crop: PinSourceCrop) -> Result<usize, String> {
    let full = usize::try_from(source_size.0)
        .ok()
        .and_then(|width| {
            usize::try_from(source_size.1)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "source window dimensions overflow GPU accounting".to_owned())?;
    let crop = usize::try_from(crop.width)
        .ok()
        .and_then(|width| {
            usize::try_from(crop.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "selected crop dimensions overflow GPU accounting".to_owned())?;
    full.checked_mul(FRAME_POOL_BUFFERS)
        .and_then(|pool| pool.checked_add(crop))
        .ok_or_else(|| "live capture GPU allocation size overflowed".to_owned())
}

fn publish_first_frame(
    binding: PinSourceBinding,
    generation: PinCaptureGeneration,
    sequence: u64,
    content_size: (u32, u32),
    image: RgbaImage,
    reservation: ImageBudgetReservation,
    publication_epoch: u64,
    slot: &PinLiveFrameSlot,
    sender: &SyncSender<Result<(), String>>,
) -> Result<(), String> {
    validate_source_frame_geometry(binding, content_size, true)?;
    if image.dimensions() != (binding.crop.width, binding.crop.height) {
        return Err("window capture returned an invalid first crop size".into());
    }
    let image = reservation
        .into_image(image)
        .map_err(|error| error.to_string())?;
    if !slot.publish(
        generation,
        publication_epoch,
        PinLiveFrameLease {
            sequence,
            image,
            source_content_size: content_size,
        },
    ) {
        return Err("first live frame was rejected as stale or cancelled".into());
    }
    sender
        .try_send(Ok(()))
        .map_err(|_| "pinboard stopped waiting for the first source frame".to_owned())
}

fn publish_next_frame(
    binding: PinSourceBinding,
    generation: PinCaptureGeneration,
    sequence: u64,
    content_size: (u32, u32),
    image: RgbaImage,
    reservation: ImageBudgetReservation,
    publication_epoch: u64,
    slot: &PinLiveFrameSlot,
) -> Result<bool, String> {
    validate_source_frame_geometry(binding, content_size, false)?;
    if image.dimensions() != (binding.crop.width, binding.crop.height) {
        return Err("window capture returned an invalid crop size".into());
    }
    if reservation.bytes() != image.as_raw().len() {
        return Err("live source frame does not match its reserved crop size".into());
    }
    let image = reservation
        .into_image(image)
        .map_err(|error| error.to_string())?;
    Ok(slot.publish(
        generation,
        publication_epoch,
        PinLiveFrameLease {
            sequence,
            image,
            source_content_size: content_size,
        },
    ))
}

fn run_native_capture(
    binding: PinSourceBinding,
    generation: PinCaptureGeneration,
    reservation: ImageBudgetReservation,
    gpu_budget: Arc<SharedGpuBudget>,
    signal: Arc<WorkerSignal>,
    slot: Arc<PinLiveFrameSlot>,
    first_frame_sender: SyncSender<Result<(), String>>,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::run(
            binding,
            generation,
            reservation,
            gpu_budget,
            signal,
            slot,
            first_frame_sender,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (binding, generation, reservation, gpu_budget, signal, slot);
        let _ = first_frame_sender.try_send(Err(
            "live window capture is supported only on Windows 10 1903 and later".into(),
        ));
        Err("live window capture is supported only on Windows 10 1903 and later".into())
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::pinboard::source::PinSourceWindowIdentity;
    use std::time::Duration;
    use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
    use windows::Graphics::Capture::{
        Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
        GraphicsCaptureSession,
    };
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Graphics::SizeInt32;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::D3D11_BOX;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
        D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
    };
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::System::WinRT::Direct3D11::{
        CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
    };
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    };
    use windows::core::{Interface, factory};

    struct RoApartment;

    impl RoApartment {
        fn initialize() -> Result<Self, String> {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|error| {
                format!("could not initialize capture worker COM apartment: {error}")
            })?;
            Ok(Self)
        }
    }

    impl Drop for RoApartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    struct NativeCaptureResources {
        item: GraphicsCaptureItem,
        winrt_device: IDirect3DDevice,
        source_size: SizeInt32,
        frame_signal: Arc<WorkerSignal>,
        frame_pool: Option<Direct3D11CaptureFramePool>,
        frame_token: Option<EventRegistrationToken>,
        closed_token: Option<EventRegistrationToken>,
        capture_session: Option<GraphicsCaptureSession>,
    }

    impl NativeCaptureResources {
        fn new(
            item: GraphicsCaptureItem,
            winrt_device: IDirect3DDevice,
            source_size: SizeInt32,
            frame_signal: Arc<WorkerSignal>,
            generation: PinCaptureGeneration,
            slot: Arc<PinLiveFrameSlot>,
        ) -> Result<Self, String> {
            let closed_signal = Arc::clone(&frame_signal);
            let closed_slot = Arc::clone(&slot);
            let closed_token = item
                .Closed(&TypedEventHandler::<
                    GraphicsCaptureItem,
                    windows::core::IInspectable,
                >::new(move |_, _| {
                    closed_slot.set_status(generation, PinLiveStatus::SourceClosed);
                    closed_signal.notify_source_closed();
                    Ok(())
                }))
                .map_err(|error| {
                    format!("could not subscribe to source-window closure: {error}")
                })?;
            Ok(Self {
                item,
                winrt_device,
                source_size,
                frame_signal,
                frame_pool: None,
                frame_token: None,
                closed_token: Some(closed_token),
                capture_session: None,
            })
        }

        fn start_capture(
            &mut self,
            binding: PinSourceBinding,
            gpu_reservation: &mut GpuBudgetReservation,
        ) -> Result<(), String> {
            if self.capture_session.is_some() || self.frame_pool.is_some() {
                return Err("source-window capture resources were already started".into());
            }

            // Capture sessions are tied to the frame pool that created them.
            // Pause closes both; resume creates a new pool for the same exact
            // GraphicsCaptureItem rather than reusing a closed session's pool.
            self.source_size = self
                .item
                .Size()
                .map_err(|error| format!("could not query source window capture size: {error}"))?;
            let source_dimensions = valid_dimensions(self.source_size)?;
            validate_source_frame_geometry(binding, source_dimensions, false)?;
            gpu_reservation.resize(estimate_gpu_bytes(source_dimensions, binding.crop)?)?;

            let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                &self.winrt_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                FRAME_POOL_BUFFERS as i32,
                self.source_size,
            )
            .map_err(|error| format!("could not create a window frame pool: {error}"))?;
            let frame_signal = Arc::clone(&self.frame_signal);
            let frame_token = match frame_pool.FrameArrived(&TypedEventHandler::<
                Direct3D11CaptureFramePool,
                windows::core::IInspectable,
            >::new(move |_, _| {
                frame_signal.notify_frame();
                Ok(())
            })) {
                Ok(token) => token,
                Err(error) => {
                    let _ = frame_pool.Close();
                    return Err(format!(
                        "could not subscribe to source-window frames: {error}"
                    ));
                }
            };
            let capture_session = match frame_pool.CreateCaptureSession(&self.item) {
                Ok(session) => session,
                Err(error) => {
                    let _ = frame_pool.RemoveFrameArrived(frame_token);
                    let _ = frame_pool.Close();
                    return Err(format!(
                        "could not create a source-window capture session: {error}"
                    ));
                }
            };
            self.frame_pool = Some(frame_pool);
            self.frame_token = Some(frame_token);
            self.capture_session = Some(capture_session);
            self.capture_session
                .as_ref()
                .ok_or_else(|| "source-window capture session was not created".to_owned())?
                .StartCapture()
                .map_err(|error| format!("could not start source-window capture: {error}"))
        }

        fn stop_capture(&mut self) {
            if let Some(session) = self.capture_session.take() {
                let _ = session.Close();
            }
            if let Some(frame_pool) = self.frame_pool.take() {
                if let Some(token) = self.frame_token.take() {
                    let _ = frame_pool.RemoveFrameArrived(token);
                }
                Self::drain_pool(&frame_pool);
                let _ = frame_pool.Close();
            }
        }

        fn drain_frames(&self) {
            if let Some(frame_pool) = self.frame_pool.as_ref() {
                Self::drain_pool(frame_pool);
            }
        }

        fn drain_pool(frame_pool: &Direct3D11CaptureFramePool) {
            for _ in 0..FRAME_POOL_BUFFERS {
                let Ok(frame) = frame_pool.TryGetNextFrame() else {
                    break;
                };
                drop(frame);
            }
        }

        fn try_get_next_frame(&self) -> Option<Direct3D11CaptureFrame> {
            self.frame_pool.as_ref()?.TryGetNextFrame().ok()
        }

        fn recreate_frame_pool(&mut self, source_size: SizeInt32) -> Result<(), String> {
            let frame_pool = self
                .frame_pool
                .as_ref()
                .ok_or_else(|| "source-window frame pool was not created".to_owned())?;
            frame_pool
                .Recreate(
                    &self.winrt_device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    FRAME_POOL_BUFFERS as i32,
                    source_size,
                )
                .map_err(|error| format!("could not resize source-window frame pool: {error}"))?;
            self.source_size = source_size;
            Ok(())
        }
    }

    impl Drop for NativeCaptureResources {
        fn drop(&mut self) {
            self.stop_capture();
            if let Some(token) = self.closed_token.take() {
                let _ = self.item.RemoveClosed(token);
            }
            // Keep the exact capture item alive until its subscriptions and
            // replaceable frame pool are closed; never reopen a recycled HWND.
        }
    }

    pub(super) fn run(
        binding: PinSourceBinding,
        generation: PinCaptureGeneration,
        reservation: ImageBudgetReservation,
        gpu_budget: Arc<SharedGpuBudget>,
        signal: Arc<WorkerSignal>,
        slot: Arc<PinLiveFrameSlot>,
        first_frame_sender: SyncSender<Result<(), String>>,
    ) -> Result<(), String> {
        let _apartment = RoApartment::initialize()?;
        if !GraphicsCaptureSession::IsSupported()
            .map_err(|error| format!("Windows Graphics Capture support check failed: {error}"))?
        {
            return Err("Windows Graphics Capture is not supported on this Windows version".into());
        }
        let identity = binding.window;
        let (d3d_device, context) = create_device()?;
        let winrt_device = create_winrt_device(&d3d_device)?;
        let item = create_window_item(identity)?;
        let initial_size = item
            .Size()
            .map_err(|error| format!("could not query source window capture size: {error}"))?;
        let initial_dimensions = valid_dimensions(initial_size)?;
        validate_source_frame_geometry(binding, initial_dimensions, true)?;
        let mut gpu_reservation =
            gpu_budget.reserve(estimate_gpu_bytes(initial_dimensions, binding.crop)?)?;
        let mut resources = NativeCaptureResources::new(
            item,
            winrt_device.clone(),
            initial_size,
            Arc::clone(&signal),
            generation,
            Arc::clone(&slot),
        )?;
        resources.start_capture(binding, &mut gpu_reservation)?;

        let cpu_budget = reservation.shared_budget();
        let mut first_reservation = Some(reservation);
        let mut sequence = 1_u64;
        let mut staging = None;
        let startup_deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        let mut next_health_check = Instant::now();
        let clock_origin = Instant::now();
        let mut cadence = FrameCadence::default();
        let mut capture_running = true;
        'capture: loop {
            let flags = signal.snapshot();
            if flags.cancelled {
                break;
            }
            if flags.source_closed {
                if first_reservation.is_some() {
                    let message =
                        "the selected source window closed before a frame arrived".to_owned();
                    let _ = first_frame_sender.try_send(Err(message.clone()));
                    return Err(message);
                }
                slot.set_status(generation, PinLiveStatus::SourceClosed);
                break;
            }
            if flags.paused {
                if capture_running {
                    resources.stop_capture();
                    capture_running = false;
                }
                staging = None;
                gpu_reservation.resize(0)?;
                slot.set_status(generation, PinLiveStatus::Paused);
                let resumed = signal.wait_while_paused();
                if resumed.cancelled {
                    break;
                }
                if resumed.source_closed {
                    continue;
                }
                match validate_native_window(identity) {
                    NativeWindowState::Closed => {
                        signal.notify_source_closed();
                        continue;
                    }
                    NativeWindowState::Unavailable(reason) => {
                        slot.set_status(
                            generation,
                            PinLiveStatus::TemporarilyUnavailable(reason.into()),
                        );
                        continue;
                    }
                    NativeWindowState::Available => {
                        resources.start_capture(binding, &mut gpu_reservation)?;
                        capture_running = true;
                    }
                }
                continue;
            }
            if Instant::now() >= next_health_check {
                next_health_check = Instant::now() + Duration::from_millis(750);
                match validate_native_window(identity) {
                    NativeWindowState::Closed => {
                        signal.notify_source_closed();
                        continue;
                    }
                    NativeWindowState::Unavailable(reason) => {
                        if first_reservation.is_some() {
                            if Instant::now() >= startup_deadline {
                                let message = format!("{reason}; restore the window and try again");
                                let _ = first_frame_sender.try_send(Err(message.clone()));
                                return Err(message);
                            }
                        } else if !flags.paused {
                            slot.set_status(
                                generation,
                                PinLiveStatus::TemporarilyUnavailable(reason.into()),
                            );
                        }
                        if capture_running {
                            resources.stop_capture();
                            capture_running = false;
                            staging = None;
                            gpu_reservation.resize(0)?;
                        }
                    }
                    NativeWindowState::Available => {
                        if !capture_running {
                            resources.start_capture(binding, &mut gpu_reservation)?;
                            capture_running = true;
                        }
                    }
                }
            }
            if first_reservation.is_some() && Instant::now() >= startup_deadline {
                let message = "no capturable source frame arrived; the window may be protected or unavailable".to_owned();
                let _ = first_frame_sender.try_send(Err(message.clone()));
                return Err(message);
            }
            let wait = if first_reservation.is_some() {
                Duration::from_millis(200)
            } else {
                Duration::from_millis(750)
            };
            let flags = signal.wait_for_frame_or_control(wait);
            if flags.cancelled {
                break;
            }
            if flags.source_closed {
                continue;
            }
            if !flags.frame_arrived {
                continue;
            }
            if !capture_running || flags.paused || !flags.activated && first_reservation.is_none() {
                continue;
            }
            if first_reservation.is_none() {
                while let Some(remaining) = cadence.wait_remaining(clock_origin.elapsed()) {
                    let throttled = signal.wait_for_frame_or_control(remaining);
                    if throttled.cancelled {
                        break 'capture;
                    }
                    if throttled.paused || throttled.source_closed {
                        continue 'capture;
                    }
                }
            }
            // Coalesce any queued frames to the latest one. The frame pool is
            // fixed-size, so this never creates an unbounded image backlog.
            let mut latest_frame = None;
            for _ in 0..FRAME_POOL_BUFFERS {
                let Some(next) = resources.try_get_next_frame() else {
                    break;
                };
                latest_frame = Some(next);
            }
            let Some(frame) = latest_frame else {
                continue;
            };
            // A pause/resume during any subsequent processing invalidates this
            // lease, so a stale in-flight conversion cannot replace the frozen
            // frame after the control transition.
            let publication_epoch = slot.publication_epoch();
            let content_size = match frame
                .ContentSize()
                .and_then(|size| Ok(valid_dimensions(size)))
            {
                Ok(Ok(size)) => size,
                Ok(Err(message)) => {
                    slot.set_status(generation, PinLiveStatus::TemporarilyUnavailable(message));
                    continue;
                }
                Err(error) => {
                    slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable(format!(
                            "source frame has no content size: {error}"
                        )),
                    );
                    continue;
                }
            };
            if let Err(error) =
                validate_source_frame_geometry(binding, content_size, first_reservation.is_some())
            {
                slot.set_status(generation, PinLiveStatus::TemporarilyUnavailable(error));
                if first_reservation.is_some() {
                    continue;
                }
                continue;
            }
            let surface = match frame.Surface() {
                Ok(surface) => surface,
                Err(error) => {
                    slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable(format!(
                            "source content is protected or unavailable: {error}"
                        )),
                    );
                    continue;
                }
            };
            let access: IDirect3DDxgiInterfaceAccess = match surface.cast() {
                Ok(access) => access,
                Err(error) => {
                    slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable(format!(
                            "source frame surface is unsupported: {error}"
                        )),
                    );
                    continue;
                }
            };
            let source_texture: ID3D11Texture2D = match unsafe { access.GetInterface() } {
                Ok(texture) => texture,
                Err(error) => {
                    slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable(format!(
                            "source frame texture is inaccessible: {error}"
                        )),
                    );
                    continue;
                }
            };
            let mut source_desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { source_texture.GetDesc(&mut source_desc) };
            if source_desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
                || source_desc.Width < content_size.0
                || source_desc.Height < content_size.1
                || source_desc.SampleDesc.Count != 1
            {
                if first_reservation.is_some() {
                    let message =
                        "source frame pixels do not match the selected window geometry".to_owned();
                    let _ = first_frame_sender.try_send(Err(message.clone()));
                    return Err(message);
                }
                let estimate = estimate_gpu_bytes(content_size, binding.crop)?;
                let previous_dimensions = valid_dimensions(resources.source_size)?;
                let previous_estimate = estimate_gpu_bytes(previous_dimensions, binding.crop)?;
                if estimate > previous_estimate {
                    gpu_reservation.resize(estimate)?;
                }
                if let Err(error) = resources.recreate_frame_pool(SizeInt32 {
                    Width: content_size.0 as i32,
                    Height: content_size.1 as i32,
                }) {
                    gpu_reservation.resize(previous_estimate)?;
                    slot.set_status(
                        generation,
                        PinLiveStatus::TemporarilyUnavailable(format!(
                            "source resize is not capturable: {error}"
                        )),
                    );
                } else {
                    gpu_reservation.resize(estimate)?;
                }
                continue;
            }
            if staging
                .as_ref()
                .is_none_or(|(dimensions, _): &((u32, u32), ID3D11Texture2D)| {
                    *dimensions != (binding.crop.width, binding.crop.height)
                })
            {
                staging = Some((
                    (binding.crop.width, binding.crop.height),
                    create_staging_texture(&d3d_device, binding.crop, source_desc.Format)?,
                ));
            }
            let Some((_, staging_texture)) = staging.as_ref() else {
                continue;
            };
            let crop_box = D3D11_BOX {
                left: binding.crop.x,
                top: binding.crop.y,
                front: 0,
                right: binding.crop.x + binding.crop.width,
                bottom: binding.crop.y + binding.crop.height,
                back: 1,
            };
            if !source_can_publish_frame(identity, generation, &signal, &slot) {
                continue;
            }
            unsafe {
                context.CopySubresourceRegion(
                    staging_texture,
                    0,
                    0,
                    0,
                    0,
                    &source_texture,
                    0,
                    Some(&crop_box),
                );
            }
            let image_bytes = usize::try_from(binding.crop.width)
                .ok()
                .and_then(|width| {
                    usize::try_from(binding.crop.height)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| "selected crop allocation size overflowed".to_owned())?;
            // Reserve CPU bytes before allocating the converted crop. The first
            // lease was reserved by the selection transaction; later frames
            // compete atomically with all other pin and clipboard leases.
            let initial_frame_candidate = first_reservation.is_some();
            let frame_reservation = if let Some(reservation) = first_reservation.take() {
                if reservation.bytes() != image_bytes {
                    return Err("first source frame does not match its reserved crop size".into());
                }
                reservation
            } else {
                match cpu_budget.reserve(image_bytes) {
                    Ok(reservation) => reservation,
                    Err(error) => {
                        slot.set_status(
                            generation,
                            PinLiveStatus::TemporarilyUnavailable(error.to_string()),
                        );
                        continue;
                    }
                }
            };
            let image = read_staging_rgba(&context, staging_texture, binding.crop)?;
            if !source_can_publish_frame(identity, generation, &signal, &slot) {
                if initial_frame_candidate {
                    first_reservation = Some(frame_reservation);
                }
                continue;
            }
            cadence.record_completed(clock_origin.elapsed());
            if slot.has_frame() {
                sequence = sequence
                    .checked_add(1)
                    .ok_or_else(|| "live frame sequence exhausted".to_owned())?;
                if publish_next_frame(
                    binding,
                    generation,
                    sequence,
                    content_size,
                    image,
                    frame_reservation,
                    publication_epoch,
                    &slot,
                )? {
                    slot.set_status(generation, PinLiveStatus::Live);
                }
            } else {
                publish_first_frame(
                    binding,
                    generation,
                    sequence,
                    content_size,
                    image,
                    frame_reservation,
                    publication_epoch,
                    &slot,
                    &first_frame_sender,
                )?;
            }
            drop(frame);
            // Keep a stable association to this exact native item for the
            // complete session; an HWND replacement is never reopened.
        }
        if signal.snapshot().source_closed && !signal.snapshot().cancelled {
            slot.set_status(generation, PinLiveStatus::SourceClosed);
        }
        Ok(())
    }

    fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|error| format!("could not create a BGRA Direct3D device: {error}"))?;
        Ok((
            device.ok_or_else(|| "Direct3D did not return a device".to_owned())?,
            context.ok_or_else(|| "Direct3D did not return an immediate context".to_owned())?,
        ))
    }

    fn create_winrt_device(device: &ID3D11Device) -> Result<IDirect3DDevice, String> {
        let dxgi: IDXGIDevice = device
            .cast()
            .map_err(|error| format!("could not query the Direct3D DXGI device: {error}"))?;
        let inspectable =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(|error| {
                format!("could not create the Windows Graphics Capture device: {error}")
            })?;
        inspectable
            .cast()
            .map_err(|error| format!("could not query the Windows capture device: {error}"))
    }

    fn create_window_item(
        identity: PinSourceWindowIdentity,
    ) -> Result<GraphicsCaptureItem, String> {
        let hwnd = HWND(identity.hwnd as *mut core::ffi::c_void);
        if !unsafe { IsWindow(hwnd).as_bool() } {
            return Err("the selected source window no longer exists".into());
        }
        if native_identity(hwnd) != Some(identity) {
            return Err("the selected source window identity changed before capture".into());
        }
        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, _>().map_err(|error| {
                format!("Windows Graphics Capture window interop is unavailable: {error}")
            })?;
        unsafe { interop.CreateForWindow::<_, GraphicsCaptureItem>(hwnd) }.map_err(|error| {
            format!("could not bind capture to the selected source window: {error}")
        })
    }

    fn native_identity(hwnd: HWND) -> Option<PinSourceWindowIdentity> {
        let mut process_id = 0;
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        (thread_id != 0 && process_id != 0).then_some(PinSourceWindowIdentity {
            hwnd: hwnd.0 as usize,
            process_id,
            thread_id,
        })
    }

    enum NativeWindowState {
        Available,
        Unavailable(&'static str),
        Closed,
    }

    fn validate_native_window(identity: PinSourceWindowIdentity) -> NativeWindowState {
        let hwnd = HWND(identity.hwnd as *mut core::ffi::c_void);
        if !unsafe { IsWindow(hwnd).as_bool() } || native_identity(hwnd) != Some(identity) {
            return NativeWindowState::Closed;
        }
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return NativeWindowState::Unavailable("source window is hidden");
        }
        if unsafe { IsIconic(hwnd).as_bool() } {
            return NativeWindowState::Unavailable("source window is minimized");
        }
        let mut cloaked = 0_u32;
        if unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                std::mem::size_of_val(&cloaked) as u32,
            )
        }
        .is_err()
        {
            return NativeWindowState::Unavailable(
                "source window visibility could not be verified",
            );
        }
        if cloaked != 0 {
            NativeWindowState::Unavailable("source window is cloaked")
        } else {
            NativeWindowState::Available
        }
    }

    fn source_can_publish_frame(
        identity: PinSourceWindowIdentity,
        generation: PinCaptureGeneration,
        signal: &WorkerSignal,
        slot: &PinLiveFrameSlot,
    ) -> bool {
        let flags = signal.snapshot();
        if flags.cancelled || flags.paused {
            return false;
        }
        if flags.source_closed {
            slot.set_status(generation, PinLiveStatus::SourceClosed);
            return false;
        }
        match validate_native_window(identity) {
            NativeWindowState::Available => true,
            NativeWindowState::Unavailable(reason) => {
                slot.set_status(
                    generation,
                    PinLiveStatus::TemporarilyUnavailable(reason.into()),
                );
                false
            }
            NativeWindowState::Closed => {
                slot.set_status(generation, PinLiveStatus::SourceClosed);
                signal.notify_source_closed();
                false
            }
        }
    }

    fn valid_dimensions(size: SizeInt32) -> Result<(u32, u32), String> {
        if size.Width <= 0 || size.Height <= 0 {
            return Err("source window has zero or negative capture dimensions".into());
        }
        Ok((size.Width as u32, size.Height as u32))
    }

    fn create_staging_texture(
        device: &ID3D11Device,
        crop: PinSourceCrop,
        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    ) -> Result<ID3D11Texture2D, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: crop.width,
            Height: crop.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut texture = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(|error| format!("could not create crop staging texture: {error}"))?;
        texture.ok_or_else(|| "Direct3D did not return a crop staging texture".into())
    }

    fn read_staging_rgba(
        context: &ID3D11DeviceContext,
        texture: &ID3D11Texture2D,
        crop: PinSourceCrop,
    ) -> Result<RgbaImage, String> {
        let resource: ID3D11Resource = texture
            .cast()
            .map_err(|error| format!("could not query crop staging resource: {error}"))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(&resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
            .map_err(|error| format!("could not map captured crop: {error}"))?;
        let result = copy_mapped_bgra_to_rgba(
            mapped.pData.cast(),
            mapped.RowPitch as usize,
            crop.width,
            crop.height,
        );
        unsafe { context.Unmap(&resource, 0) };
        result
    }

    fn copy_mapped_bgra_to_rgba(
        source: *const u8,
        row_pitch: usize,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, String> {
        let row_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| "captured crop row size overflowed".to_owned())?;
        if source.is_null() || row_pitch < row_bytes {
            return Err("captured crop has an invalid Direct3D row pitch".into());
        }
        let len = row_bytes
            .checked_mul(usize::try_from(height).map_err(|_| "invalid crop height")?)
            .ok_or_else(|| "captured crop byte size overflowed".to_owned())?;
        let mut rgba = vec![0; len];
        for row in 0..height as usize {
            // SAFETY: D3D11 guarantees the mapped staging bytes are readable
            // for each row; the row pitch and crop dimensions were checked.
            let bgra_row =
                unsafe { std::slice::from_raw_parts(source.add(row * row_pitch), row_bytes) };
            let rgba_row = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
            for (source, target) in bgra_row.chunks_exact(4).zip(rgba_row.chunks_exact_mut(4)) {
                target[0] = source[2];
                target[1] = source[1];
                target[2] = source[0];
                target[3] = source[3];
            }
        }
        RgbaImage::from_raw(width, height, rgba)
            .ok_or_else(|| "could not construct RGBA image from captured crop".into())
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) struct FakePinLiveCaptureBackend {
        pub(crate) first_image: RgbaImage,
        pub(crate) first_content_size: (u32, u32),
        pub(crate) stream: Mutex<Option<Receiver<FakeFrame>>>,
        pub(crate) first_frame_gate: Option<Arc<FakeFirstFrameGate>>,
    }

    pub(crate) struct FakeFirstFrameGate {
        pub(crate) started: SyncSender<()>,
        pub(crate) release: Mutex<Receiver<()>>,
    }

    pub(crate) struct FakeFrame {
        pub(crate) generation: PinCaptureGeneration,
        pub(crate) sequence: u64,
        pub(crate) content_size: (u32, u32),
        pub(crate) image: RgbaImage,
        pub(crate) processed: SyncSender<()>,
    }

    impl PinLiveCaptureBackend for FakePinLiveCaptureBackend {
        fn start(
            &self,
            binding: PinSourceBinding,
            generation: PinCaptureGeneration,
            reservation: ImageBudgetReservation,
            gpu_budget: Arc<SharedGpuBudget>,
        ) -> Result<PinLiveCaptureSession, String> {
            let first_image = self.first_image.clone();
            let content_size = self.first_content_size;
            let stream = self.stream.lock().ok().and_then(|mut stream| stream.take());
            let first_frame_gate = self.first_frame_gate.clone();
            PinLiveCaptureSession::spawn(
                binding,
                generation,
                reservation,
                gpu_budget,
                Box::new(
                    move |binding, generation, reservation, _, signal, slot, first_tx| {
                        let cpu_budget = reservation.shared_budget();
                        if let Some(gate) = first_frame_gate {
                            gate.started.try_send(()).map_err(|error| {
                                format!("fake live worker start signal failed: {error}")
                            })?;
                            let release = gate
                                .release
                                .lock()
                                .map_err(|_| "fake live first-frame gate was poisoned")?;
                            loop {
                                if signal.snapshot().cancelled {
                                    return Ok(());
                                }
                                match release.recv_timeout(Duration::from_millis(10)) {
                                    Ok(()) => break,
                                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                                        return Err(
                                            "fake live first-frame gate was disconnected".into()
                                        );
                                    }
                                }
                            }
                        }
                        let publication_epoch = slot.publication_epoch();
                        publish_first_frame(
                            binding,
                            generation,
                            1,
                            content_size,
                            first_image,
                            reservation,
                            publication_epoch,
                            &slot,
                            &first_tx,
                        )?;
                        let flags = signal.wait_until_activated();
                        if flags.cancelled {
                            return Ok(());
                        }
                        let Some(stream) = stream else {
                            while !signal.snapshot().cancelled {
                                signal.wait_for_frame_or_control(Duration::from_millis(25));
                            }
                            return Ok(());
                        };
                        while !signal.snapshot().cancelled {
                            match stream.recv_timeout(Duration::from_millis(10)) {
                                Ok(frame) => {
                                    let publication_epoch = slot.publication_epoch();
                                    let flags = signal.snapshot();
                                    let processed = frame.processed.clone();
                                    if frame.generation != generation || flags.paused {
                                        let _ = processed.try_send(());
                                        continue;
                                    }
                                    let frame_bytes = frame.image.as_raw().len();
                                    let frame_reservation = match cpu_budget.reserve(frame_bytes) {
                                        Ok(reservation) => reservation,
                                        Err(error) => {
                                            slot.set_status(
                                                generation,
                                                PinLiveStatus::TemporarilyUnavailable(
                                                    error.to_string(),
                                                ),
                                            );
                                            let _ = processed.try_send(());
                                            continue;
                                        }
                                    };
                                    if let Err(error) = publish_next_frame(
                                        binding,
                                        frame.generation,
                                        frame.sequence,
                                        frame.content_size,
                                        frame.image,
                                        frame_reservation,
                                        publication_epoch,
                                        &slot,
                                    ) {
                                        slot.set_status(
                                            generation,
                                            PinLiveStatus::TemporarilyUnavailable(error),
                                        );
                                    }
                                    let _ = processed.try_send(());
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                        Ok(())
                    },
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::ScreenRect;
    use crate::pinboard::source::PinSourceWindowIdentity;
    use crate::pinboard::{PinCaptureGeneration, SharedImageBudget};
    use std::sync::atomic::Ordering;

    fn binding(crop: PinSourceCrop) -> PinSourceBinding {
        PinSourceBinding {
            window: PinSourceWindowIdentity {
                hwnd: 11,
                process_id: 22,
                thread_id: 33,
            },
            captured_bounds: ScreenRect::new(-20, 15, 12, 10),
            crop,
        }
    }

    #[test]
    fn live_frame_geometry_requires_exact_initial_origin_size_and_contains_resizes() {
        let binding = binding(PinSourceCrop {
            x: 3,
            y: 2,
            width: 4,
            height: 3,
        });
        assert!(validate_source_frame_geometry(binding, (12, 10), true).is_ok());
        assert!(validate_source_frame_geometry(binding, (13, 10), true).is_err());
        assert!(validate_source_frame_geometry(binding, (15, 10), false).is_ok());
        assert!(validate_source_frame_geometry(binding, (6, 10), false).is_err());
    }

    #[test]
    fn live_gpu_estimate_bounds_two_frame_buffers_and_crop_staging() {
        let bytes = estimate_gpu_bytes(
            (100, 50),
            PinSourceCrop {
                x: 2,
                y: 3,
                width: 10,
                height: 5,
            },
        )
        .unwrap();
        assert_eq!(bytes, 100 * 50 * 4 * 2 + 10 * 5 * 4);
        let budget = Arc::new(SharedGpuBudget {
            limit: bytes,
            used: std::sync::atomic::AtomicUsize::new(0),
        });
        let first = budget.reserve(bytes).unwrap();
        assert!(budget.reserve(1).is_err());
        drop(first);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }

    #[test]
    fn frame_cadence_caps_completed_frames_to_ten_per_second() {
        let mut cadence = FrameCadence::default();
        assert_eq!(cadence.wait_remaining(Duration::ZERO), None);
        cadence.record_completed(Duration::ZERO);
        assert_eq!(
            cadence.wait_remaining(Duration::from_millis(99)),
            Some(Duration::from_millis(1))
        );
        assert_eq!(cadence.wait_remaining(Duration::from_millis(100)), None);
        cadence.record_completed(Duration::from_millis(100));
        assert_eq!(
            cadence.wait_remaining(Duration::from_millis(199)),
            Some(Duration::from_millis(1))
        );
        assert_eq!(cadence.wait_remaining(Duration::from_millis(250)), None);
    }

    #[test]
    fn worker_signal_returns_coalesced_frame_arrival_once() {
        let signal = WorkerSignal::new();
        signal.notify_frame();
        signal.notify_frame();

        assert!(
            signal
                .wait_for_frame_or_control(Duration::ZERO)
                .frame_arrived
        );
        assert!(
            !signal
                .wait_for_frame_or_control(Duration::ZERO)
                .frame_arrived
        );
    }

    #[test]
    fn worker_signal_activation_schedules_a_drain_for_buffered_frames() {
        let signal = WorkerSignal::new();
        signal.notify_frame();
        let inactive = signal.wait_for_frame_or_control(Duration::ZERO);
        assert!(inactive.frame_arrived);
        assert!(!inactive.activated);

        assert!(signal.activate());
        let activated = signal.wait_for_frame_or_control(Duration::ZERO);
        assert!(activated.activated);
        assert!(activated.frame_arrived);
        assert!(
            !signal
                .wait_for_frame_or_control(Duration::ZERO)
                .frame_arrived
        );
    }

    #[test]
    fn latest_frame_slot_rejects_stale_publications_and_charges_retained_leases() {
        let budget = Arc::new(SharedImageBudget {
            limit: 32,
            reserved_and_allocated_bytes: std::sync::atomic::AtomicUsize::new(0),
        });
        let generation = PinCaptureGeneration(std::num::NonZeroU64::new(7).unwrap());
        let other_generation = PinCaptureGeneration(std::num::NonZeroU64::new(8).unwrap());
        let slot = PinLiveFrameSlot::new(generation);
        let child_wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let wake_counter = Arc::clone(&child_wakes);
        slot.set_child_wake_notifier(Some(Arc::new(move || {
            wake_counter.fetch_add(1, Ordering::AcqRel);
        })));
        let make_frame = |sequence| {
            let image = budget
                .reserve(16)
                .unwrap()
                .into_image(RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])))
                .unwrap();
            PinLiveFrameLease {
                sequence,
                image,
                source_content_size: (12, 10),
            }
        };

        assert!(slot.publish(generation, 0, make_frame(1)));
        assert_eq!(child_wakes.load(Ordering::Acquire), 1);
        let retained_old_frame = slot.current_frame().unwrap();
        assert!(!slot.publish(other_generation, 0, make_frame(2)));
        assert!(!slot.publish(generation, 0, make_frame(1)));
        assert_eq!(slot.current_frame().unwrap().sequence, 1);
        assert_eq!(budget.used_bytes(), 16);

        assert!(slot.set_status(generation, PinLiveStatus::Paused));
        assert_eq!(child_wakes.load(Ordering::Acquire), 2);
        assert_eq!(slot.publication_epoch(), 1);
        assert!(!slot.publish(generation, 0, make_frame(2)));
        assert_eq!(child_wakes.load(Ordering::Acquire), 2);
        assert_eq!(slot.current_frame().unwrap().sequence, 1);
        assert!(slot.set_status(generation, PinLiveStatus::Live));
        assert_eq!(child_wakes.load(Ordering::Acquire), 3);
        assert_eq!(slot.publication_epoch(), 2);
        assert!(!slot.publish(generation, 1, make_frame(2)));
        assert!(slot.publish(generation, 2, make_frame(2)));
        assert_eq!(child_wakes.load(Ordering::Acquire), 4);
        assert_eq!(slot.current_frame().unwrap().sequence, 2);
        assert_eq!(budget.used_bytes(), 32);
        drop(retained_old_frame);
        assert_eq!(budget.used_bytes(), 16);

        assert!(slot.set_status(generation, PinLiveStatus::SourceClosed));
        assert_eq!(child_wakes.load(Ordering::Acquire), 5);
        let terminal_epoch = slot.publication_epoch();
        assert!(!slot.set_status(generation, PinLiveStatus::Live));
        assert!(!slot.set_status(generation, PinLiveStatus::Error("late failure".into())));
        assert!(!slot.publish(generation, terminal_epoch, make_frame(3)));
        assert_eq!(budget.used_bytes(), 16);
        let error_slot = PinLiveFrameSlot::new(generation);
        assert!(error_slot.set_status(generation, PinLiveStatus::Error("worker failed".into())));
        assert!(!error_slot.set_status(generation, PinLiveStatus::Live));
        assert!(!error_slot.set_status(generation, PinLiveStatus::SourceClosed));

        slot.cancel(generation);
        assert!(!slot.publish(generation, terminal_epoch, make_frame(3)));
        drop(slot);
        drop(error_slot);
        assert_eq!(budget.used_bytes(), 0);
    }

    #[test]
    fn fake_live_session_keeps_first_frame_and_pause_resume_sequence_fenced() {
        use crate::pinboard::live::test_support::{FakeFrame, FakePinLiveCaptureBackend};

        let budget = Arc::new(SharedImageBudget {
            limit: 32,
            reserved_and_allocated_bytes: std::sync::atomic::AtomicUsize::new(0),
        });
        let generation = PinCaptureGeneration(std::num::NonZeroU64::new(9).unwrap());
        let binding = binding(PinSourceCrop {
            x: 3,
            y: 2,
            width: 2,
            height: 2,
        });
        let (frame_tx, frame_rx) = mpsc::channel();
        let backend = FakePinLiveCaptureBackend {
            first_image: RgbaImage::from_pixel(2, 2, image::Rgba([4, 5, 6, 255])),
            first_content_size: (12, 10),
            stream: Mutex::new(Some(frame_rx)),
            first_frame_gate: None,
        };
        let mut session = backend
            .start(
                binding,
                generation,
                budget.reserve(16).unwrap(),
                Arc::new(SharedGpuBudget::default()),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = session.try_first_frame_result() {
                result.unwrap();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "first live frame was not delivered"
            );
            thread::yield_now();
        }
        assert_eq!(session.current_frame().unwrap().sequence, 1);
        assert!(session.activate());
        assert!(session.pause());
        assert_eq!(session.status(), PinLiveStatus::Paused);
        let (paused_processed_tx, paused_processed_rx) = mpsc::sync_channel(1);
        frame_tx
            .send(FakeFrame {
                generation,
                sequence: 2,
                content_size: (12, 10),
                image: RgbaImage::from_pixel(2, 2, image::Rgba([7, 8, 9, 255])),
                processed: paused_processed_tx,
            })
            .unwrap();
        paused_processed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("paused frame should be acknowledged without publication");
        assert_eq!(session.current_frame().unwrap().sequence, 1);

        assert!(session.resume());
        let (resumed_processed_tx, resumed_processed_rx) = mpsc::sync_channel(1);
        frame_tx
            .send(FakeFrame {
                generation,
                sequence: 3,
                content_size: (14, 10),
                image: RgbaImage::from_pixel(2, 2, image::Rgba([10, 11, 12, 255])),
                processed: resumed_processed_tx,
            })
            .unwrap();
        resumed_processed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("resumed frame should be published");
        assert_eq!(session.current_frame().unwrap().sequence, 3);
        session.cancel_and_join().unwrap();
        assert!(session.current_frame().is_some());
        assert_eq!(budget.used_bytes(), 16);
        assert!(!session.pause());
        assert!(!session.resume());
    }

    #[cfg(windows)]
    mod native_smoke {
        use super::*;
        use crate::pinboard::source::{NativePinSourceResolver, PinSourceResolver};
        use crate::pinboard::{Pin, PinCaptureOperationId, Pinboard, PinboardLimits};
        use image::Rgba;
        use std::io::{BufRead, BufReader, Write};
        use std::os::windows::process::CommandExt;
        use std::path::PathBuf;
        use std::process::{Child, ChildStdin, Command, Stdio};
        use std::sync::mpsc::{self, Receiver, SyncSender};
        use std::time::SystemTime;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CROP_X: u32 = 43;
        const CROP_Y: u32 = 57;
        const CROP_SIZE: u32 = 64;

        struct SourceReady {
            nonce: String,
            hwnd: usize,
            process_id: u32,
            thread_id: u32,
            bounds: ScreenRect,
        }

        impl SourceReady {
            fn parse(line: &str) -> Result<Self, String> {
                let fields: Vec<_> = line.split('|').collect();
                if fields.len() != 9 || fields[0] != "READY" {
                    return Err(format!("invalid source fixture handshake: {line}"));
                }
                let parse = |value: &str, name: &str| {
                    value
                        .parse::<u64>()
                        .map_err(|error| format!("invalid {name} in fixture handshake: {error}"))
                };
                let hwnd = usize::try_from(parse(fields[2], "HWND")?)
                    .map_err(|_| "fixture HWND does not fit usize".to_owned())?;
                let process_id = u32::try_from(parse(fields[3], "PID")?)
                    .map_err(|_| "fixture PID does not fit u32".to_owned())?;
                let thread_id = u32::try_from(parse(fields[4], "thread ID")?)
                    .map_err(|_| "fixture thread ID does not fit u32".to_owned())?;
                let x = i32::try_from(
                    fields[5]
                        .parse::<i64>()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|_| "fixture X coordinate does not fit i32".to_owned())?;
                let y = i32::try_from(
                    fields[6]
                        .parse::<i64>()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|_| "fixture Y coordinate does not fit i32".to_owned())?;
                let width = u32::try_from(parse(fields[7], "width")?)
                    .map_err(|_| "fixture width does not fit u32".to_owned())?;
                let height = u32::try_from(parse(fields[8], "height")?)
                    .map_err(|_| "fixture height does not fit u32".to_owned())?;
                Ok(Self {
                    nonce: fields[1].to_owned(),
                    hwnd,
                    process_id,
                    thread_id,
                    bounds: ScreenRect::new(x, y, width, height),
                })
            }
        }

        struct OwnedSourceFixture {
            child: Child,
            stdin: Option<ChildStdin>,
            lines: Receiver<String>,
            reader: Option<thread::JoinHandle<()>>,
            next_command_id: u64,
            finished: bool,
        }

        impl OwnedSourceFixture {
            fn spawn(nonce: &str) -> Result<Self, String> {
                let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join("fixtures")
                    .join("pinboard_live_source.ps1");
                let mut child = Command::new("powershell.exe")
                    .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-STA", "-File"])
                    .arg(script)
                    .arg(nonce)
                    .creation_flags(CREATE_NO_WINDOW)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|error| {
                        format!("could not start the owned PowerShell fixture: {error}")
                    })?;
                let stdout = child.stdout.take().ok_or_else(|| {
                    let _ = child.kill();
                    let _ = child.wait();
                    "PowerShell fixture has no stdout pipe".to_owned()
                })?;
                let (line_tx, lines) = mpsc::sync_channel(32);
                let reader = match thread::Builder::new()
                    .name("pinboard-live-fixture-output".into())
                    .spawn(move || {
                        for line in BufReader::new(stdout).lines() {
                            match line {
                                Ok(line) => {
                                    if line_tx.try_send(line).is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }) {
                    Ok(reader) => reader,
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("could not read fixture output: {error}"));
                    }
                };
                Ok(Self {
                    stdin: child.stdin.take(),
                    child,
                    lines,
                    reader: Some(reader),
                    next_command_id: 1,
                    finished: false,
                })
            }

            fn next_line(&self, timeout: Duration) -> Result<String, String> {
                self.lines.recv_timeout(timeout).map_err(|error| {
                    format!("timed out waiting for the owned source fixture: {error}")
                })
            }

            fn wait_ready(&self, nonce: &str) -> Result<SourceReady, String> {
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(
                            "owned source fixture did not become ready within 10 seconds".into(),
                        );
                    }
                    let line = self.next_line(remaining)?;
                    if line.starts_with("ERROR|") {
                        return Err(format!("owned source fixture reported {line}"));
                    }
                    let ready = SourceReady::parse(&line)?;
                    if ready.nonce != nonce {
                        return Err("owned source fixture nonce did not match the test".into());
                    }
                    return Ok(ready);
                }
            }

            fn command(&mut self, command: &str) -> Result<(), String> {
                let id = self.next_command_id;
                self.next_command_id = self
                    .next_command_id
                    .checked_add(1)
                    .ok_or_else(|| "fixture command identity exhausted".to_owned())?;
                let stdin = self
                    .stdin
                    .as_mut()
                    .ok_or_else(|| "owned source fixture stdin is closed".to_owned())?;
                writeln!(stdin, "{command} {id}")
                    .and_then(|()| stdin.flush())
                    .map_err(|error| format!("could not send command to owned fixture: {error}"))?;
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(format!(
                            "owned source fixture did not acknowledge {command}"
                        ));
                    }
                    let line = self.next_line(remaining)?;
                    if line == format!("ACK|{id}") {
                        return Ok(());
                    }
                    if line.starts_with(&format!("ERROR|{id}|")) || line.starts_with("ERROR|") {
                        return Err(format!("owned source fixture rejected {command}: {line}"));
                    }
                    return Err(format!(
                        "unexpected output from owned source fixture: {line}"
                    ));
                }
            }

            fn finish(&mut self) -> Result<(), String> {
                if self
                    .child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_none()
                {
                    self.command("QUIT")?;
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                let status = loop {
                    if let Some(status) =
                        self.child.try_wait().map_err(|error| error.to_string())?
                    {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        self.child.kill().map_err(|error| {
                            format!("could not reap the owned fixture: {error}")
                        })?;
                        break self.child.wait().map_err(|error| error.to_string())?;
                    }
                    thread::yield_now();
                };
                self.stdin.take();
                if let Some(reader) = self.reader.take() {
                    reader.join().map_err(|_| {
                        "owned fixture stdout reader terminated unexpectedly".to_owned()
                    })?;
                }
                self.finished = true;
                if !status.success() {
                    return Err(format!("owned PowerShell fixture exited with {status}"));
                }
                Ok(())
            }
        }

        impl Drop for OwnedSourceFixture {
            fn drop(&mut self) {
                if !self.finished && self.child.try_wait().ok().flatten().is_none() {
                    if let Some(stdin) = self.stdin.as_mut() {
                        let id = self.next_command_id;
                        let _ = writeln!(stdin, "QUIT {id}").and_then(|()| stdin.flush());
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while self.child.try_wait().ok().flatten().is_none()
                        && Instant::now() < deadline
                    {
                        thread::yield_now();
                    }
                    if self.child.try_wait().ok().flatten().is_none() {
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                    }
                }
                self.stdin.take();
                if let Some(reader) = self.reader.take() {
                    let _ = reader.join();
                }
            }
        }

        struct LocalLiveSession(Option<PinLiveCaptureSession>);

        impl Drop for LocalLiveSession {
            fn drop(&mut self) {
                if let Some(session) = self.0.as_mut() {
                    let _ = session.cancel_and_join();
                }
            }
        }

        fn fixture_nonce() -> String {
            format!(
                "{:x}",
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos())
            )
        }

        fn wait_for_first_frame(session: &mut PinLiveCaptureSession) -> Result<(), String> {
            session
                .wait_first_frame_result(Duration::from_secs(10))
                .unwrap_or_else(|| {
                    Err("the native live source produced no first frame in 10 seconds".into())
                })
        }

        fn first_anchor(frame: &PinLiveFrameLease) -> Rgba<u8> {
            *frame.image.pixels().get_pixel(8, 8)
        }

        fn second_anchor(frame: &PinLiveFrameLease) -> Rgba<u8> {
            *frame.image.pixels().get_pixel(43, 32)
        }

        fn phase_colors(phase: char) -> (Rgba<u8>, Rgba<u8>) {
            match phase {
                'A' => (Rgba([250, 210, 25, 255]), Rgba([20, 180, 245, 255])),
                'B' => (Rgba([35, 240, 70, 255]), Rgba([250, 90, 15, 255])),
                'C' => (Rgba([245, 40, 170, 255]), Rgba([25, 210, 230, 255])),
                _ => unreachable!(),
            }
        }

        fn assert_phase(frame: &PinLiveFrameLease, phase: char) {
            let (first, second) = phase_colors(phase);
            assert_eq!(
                first_anchor(frame),
                first,
                "wrong first anchor for phase {phase} at sequence {}",
                frame.sequence
            );
            assert_eq!(
                second_anchor(frame),
                second,
                "wrong second anchor for phase {phase} at sequence {}",
                frame.sequence
            );
        }

        fn wait_for_phase(
            board: &Pinboard,
            pin_id: crate::pinboard::PinId,
            after_sequence: u64,
            phase: char,
            wakes: &Receiver<()>,
            timeout: Duration,
        ) -> Result<PinLiveFrameLease, String> {
            let deadline = Instant::now() + timeout;
            loop {
                if let Some(frame) = board.get(pin_id).and_then(Pin::live_frame)
                    && frame.sequence > after_sequence
                    && first_anchor(&frame) == phase_colors(phase).0
                {
                    assert_phase(&frame, phase);
                    return Ok(frame);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    let status = board.get(pin_id).and_then(Pin::live_status);
                    return Err(format!(
                        "no newer phase {phase} frame arrived after sequence {after_sequence}; status: {status:?}"
                    ));
                }
                match wakes.recv_timeout(remaining) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("native live frame wake channel disconnected".into());
                    }
                }
            }
        }

        fn drain_wakes(wakes: &Receiver<()>) {
            while wakes.try_recv().is_ok() {}
        }

        #[test]
        #[ignore = "requires an interactive Windows desktop with Windows Graphics Capture support"]
        fn native_live_pin_owned_window_smoke() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

            const CROP_BYTES: usize = (CROP_SIZE * CROP_SIZE * 4) as usize;
            let nonce = fixture_nonce();
            let mut fixture = OwnedSourceFixture::spawn(&nonce).unwrap_or_else(|error| {
                panic!("could not start the native source fixture: {error}")
            });
            let ready = fixture
                .wait_ready(&nonce)
                .unwrap_or_else(|error| panic!("native source fixture was not ready: {error}"));
            assert_eq!(ready.process_id, fixture.child.id());
            assert_ne!(ready.process_id, std::process::id());
            assert_eq!((ready.bounds.width, ready.bounds.height), (360, 240));
            let hwnd = HWND(ready.hwnd as *mut core::ffi::c_void);
            assert!(
                unsafe { IsWindow(hwnd) }.as_bool(),
                "fixture HWND is not live"
            );
            let mut actual_process_id = 0;
            let actual_thread_id =
                unsafe { GetWindowThreadProcessId(hwnd, Some(&mut actual_process_id)) };
            assert_eq!(actual_process_id, ready.process_id);
            assert_eq!(actual_thread_id, ready.thread_id);

            fixture.command("PAINT A").unwrap();
            let selection = ScreenRect::new(
                ready.bounds.x + CROP_X as i32,
                ready.bounds.y + CROP_Y as i32,
                CROP_SIZE,
                CROP_SIZE,
            );
            let resolver = NativePinSourceResolver;
            let binding = resolver.resolve(selection).unwrap_or_else(|error| {
                panic!("native source resolver rejected owned source: {error}")
            });
            assert_eq!(binding.window.hwnd, ready.hwnd);
            assert_eq!(binding.window.process_id, ready.process_id);
            assert_eq!(binding.window.thread_id, ready.thread_id);
            assert_eq!(binding.captured_bounds, ready.bounds);
            assert_eq!(
                binding.crop,
                PinSourceCrop {
                    x: CROP_X,
                    y: CROP_Y,
                    width: CROP_SIZE,
                    height: CROP_SIZE,
                }
            );

            let mut board = Pinboard::with_limits(PinboardLimits::default()).unwrap();
            let operation_id = PinCaptureOperationId::new(0x5049_4e42_4f41_5244).unwrap();
            let capture = board.reserve_live_capture(operation_id).unwrap();
            board.reserve_selected_rect(capture, selection).unwrap();
            let reservation = board.take_live_reservation(capture).unwrap();
            assert_eq!(reservation.bytes(), CROP_BYTES);
            let gpu_budget = board.gpu_budget();
            let mut local_session = LocalLiveSession(Some(
                NativePinLiveCaptureBackend
                    .start(
                        binding,
                        capture.generation,
                        reservation,
                        Arc::clone(&gpu_budget),
                    )
                    .unwrap_or_else(|error| panic!("native live capture could not start: {error}")),
            ));
            let wake_source = local_session.0.as_ref().unwrap().frame_source();
            let (wake_tx, wakes) = mpsc::sync_channel::<()>(1);
            let wake_sender: SyncSender<()> = wake_tx;
            wake_source.set_child_wake_notifier(Some(Arc::new(move || {
                let _ = wake_sender.try_send(());
            })));
            wait_for_first_frame(local_session.0.as_mut().unwrap()).unwrap_or_else(|error| {
                panic!("native source did not publish its first frame: {error}")
            });

            board
                .stage_live_capture(capture, binding, local_session.0.take().unwrap())
                .unwrap();
            let pin_id = board.commit_capture(capture).unwrap();
            let initial = board.get(pin_id).unwrap().live_frame().unwrap();
            assert_eq!(initial.sequence, 1);
            assert_eq!(initial.source_content_size, (360, 240));
            assert_phase(&initial, 'A');

            let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("pinboard-live-smoke")
                .join(&nonce);
            std::fs::create_dir_all(&output_dir).unwrap();
            let initial_path = output_dir.join("initial.png");
            initial.image.pixels().save(&initial_path).unwrap();
            eprintln!(
                "native live smoke initial frame: {}",
                initial_path.display()
            );

            fixture.command("MOVE").unwrap();
            fixture.command("COVER").unwrap();
            fixture.command("PAINT B").unwrap();
            let covered = wait_for_phase(
                &board,
                pin_id,
                initial.sequence,
                'B',
                &wakes,
                Duration::from_secs(5),
            )
            .unwrap_or_else(|error| {
                panic!("moved/covered source did not publish phase B: {error}")
            });
            assert_eq!(covered.source_content_size, (360, 240));
            assert_ne!(first_anchor(&covered), Rgba([255, 0, 255, 255]));
            let covered_path = output_dir.join("covered-updated.png");
            covered.image.pixels().save(&covered_path).unwrap();
            eprintln!(
                "native live smoke moved, covered frame: {}",
                covered_path.display()
            );

            assert!(board.pause_live(pin_id), "live source did not pause");
            assert_eq!(
                board.get(pin_id).unwrap().live_status(),
                Some(PinLiveStatus::Paused)
            );
            let frozen = board.get(pin_id).unwrap().live_frame().unwrap();
            assert!(frozen.sequence >= covered.sequence);
            assert_phase(&frozen, 'B');
            drain_wakes(&wakes);
            fixture.command("PAINT C").unwrap();
            let frozen_until = Instant::now() + Duration::from_millis(350);
            while Instant::now() < frozen_until {
                let current = board.get(pin_id).unwrap().live_frame().unwrap();
                assert_eq!(
                    current.sequence, frozen.sequence,
                    "a paused source published a frame"
                );
                assert_eq!(first_anchor(&current), first_anchor(&frozen));
                assert_eq!(second_anchor(&current), second_anchor(&frozen));
                let remaining = frozen_until.saturating_duration_since(Instant::now());
                let _ = wakes.recv_timeout(remaining.min(Duration::from_millis(25)));
            }
            assert!(board.resume_live(pin_id), "live source did not resume");
            let resumed = wait_for_phase(
                &board,
                pin_id,
                frozen.sequence,
                'C',
                &wakes,
                Duration::from_secs(5),
            )
            .unwrap_or_else(|error| panic!("resumed source did not publish phase C: {error}"));

            fixture.command("CLOSE_SOURCE").unwrap();
            let closed_deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if board.get(pin_id).unwrap().live_status() == Some(PinLiveStatus::SourceClosed) {
                    break;
                }
                let remaining = closed_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    panic!(
                        "source HWND closed but capture did not become terminal; status: {:?}",
                        board.get(pin_id).unwrap().live_status()
                    );
                }
                let _ = wakes.recv_timeout(remaining);
            }
            assert!(
                !board.resume_live(pin_id),
                "closed source unexpectedly resumed"
            );
            let closed_frame = board.get(pin_id).unwrap().live_frame().unwrap();
            assert!(closed_frame.sequence >= resumed.sequence);
            assert_phase(&closed_frame, 'C');
            let terminal_sequence = closed_frame.sequence;
            let terminal_until = Instant::now() + Duration::from_millis(200);
            while Instant::now() < terminal_until {
                let current = board.get(pin_id).unwrap().live_frame().unwrap();
                assert_eq!(current.sequence, terminal_sequence);
                assert_eq!(first_anchor(&current), first_anchor(&closed_frame));
                assert_eq!(second_anchor(&current), second_anchor(&closed_frame));
                let remaining = terminal_until.saturating_duration_since(Instant::now());
                let _ = wakes.recv_timeout(remaining.min(Duration::from_millis(25)));
            }

            drop(initial);
            drop(covered);
            drop(frozen);
            drop(resumed);
            drop(closed_frame);
            drop(wake_source);
            assert!(board.close(pin_id));
            board.shutdown_live_sessions_and_join().unwrap();
            assert_eq!(
                board.image_bytes(),
                0,
                "CPU image budget was not released after close"
            );
            assert_eq!(
                gpu_budget.used.load(Ordering::Acquire),
                0,
                "GPU capture reservation was not released after close"
            );
            drop(gpu_budget);
            fixture.finish().unwrap();
        }
    }
}
