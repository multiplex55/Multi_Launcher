//! Session-scoped one-shot mouse capture.
//!
//! The input reducer is deliberately independent of Win32. A native driver
//! installs low-level hooks on its capture worker, calls the reducer directly
//! from those callbacks, and only publishes a terminal outcome after teardown.

use std::collections::VecDeque;
use std::sync::Arc;
#[cfg(windows)]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
#[cfg(any(windows, test))]
use std::{cell::RefCell, rc::Rc};

use super::model::{CoordinateSample, PhysicalPoint};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureSessionId(u64);

impl CaptureSessionId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePhase {
    Arming,
    WaitingForFreshClick,
    Capturing,
    Draining,
    TearingDown,
    Completed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CaptureOutcome {
    Captured(CoordinateSample),
    Cancelled,
    Shutdown,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureStatus {
    pub session_id: CaptureSessionId,
    pub phase: CapturePhase,
    /// Set only after the native session and its suppression lease are gone.
    pub outcome: Option<CaptureOutcome>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEdge {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureModifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub windows: bool,
}

impl CaptureModifiers {
    pub fn any(self) -> bool {
        self.control || self.alt || self.shift || self.windows
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKey {
    Escape,
    Other(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureInput {
    Button {
        button: CaptureButton,
        edge: ButtonEdge,
        point: PhysicalPoint,
        injected: bool,
    },
    Wheel {
        delta: i16,
        injected: bool,
    },
    Key {
        key: CaptureKey,
        edge: ButtonEdge,
        modifiers: CaptureModifiers,
        injected: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDisposition {
    PassThrough,
    Consume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaptureRequest {
    Cancel,
    Shutdown,
}

const REQUEST_NONE: u8 = 0;
const REQUEST_CANCEL: u8 = 1;
const REQUEST_SHUTDOWN: u8 = 2;

struct CaptureControlInner {
    request: AtomicU8,
    #[cfg(windows)]
    thread_id: AtomicU32,
}

/// Non-blocking control for a capture worker. Windows requests post a private
/// thread message so the hook-owning message pump wakes without polling.
#[derive(Clone)]
pub struct CaptureControl {
    inner: Arc<CaptureControlInner>,
}

impl CaptureControl {
    fn new() -> Self {
        Self {
            inner: Arc::new(CaptureControlInner {
                request: AtomicU8::new(REQUEST_NONE),
                #[cfg(windows)]
                thread_id: AtomicU32::new(0),
            }),
        }
    }

    pub fn request_cancel(&self) {
        self.set_request(CaptureRequest::Cancel);
    }

    pub fn request_shutdown(&self) {
        self.set_request(CaptureRequest::Shutdown);
    }

    fn set_request(&self, request: CaptureRequest) {
        let requested = match request {
            CaptureRequest::Cancel => REQUEST_CANCEL,
            CaptureRequest::Shutdown => REQUEST_SHUTDOWN,
        };
        if request == CaptureRequest::Shutdown {
            self.inner.request.store(requested, Ordering::Release);
        } else {
            let _ = self.inner.request.compare_exchange(
                REQUEST_NONE,
                requested,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        self.wake_worker();
    }

    fn requested(&self) -> Option<CaptureRequest> {
        match self.inner.request.load(Ordering::Acquire) {
            REQUEST_CANCEL => Some(CaptureRequest::Cancel),
            REQUEST_SHUTDOWN => Some(CaptureRequest::Shutdown),
            _ => None,
        }
    }

    #[cfg(windows)]
    fn set_thread_id(&self, thread_id: u32) {
        self.inner.thread_id.store(thread_id, Ordering::Release);
        self.wake_worker();
    }

    #[cfg(windows)]
    fn wake_worker(&self) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;

        let thread_id = self.inner.thread_id.load(Ordering::Acquire);
        if thread_id == 0 {
            return;
        }
        unsafe {
            let _ = PostThreadMessageW(
                thread_id,
                windows_native::WM_CAPTURE_WAKE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    #[cfg(not(windows))]
    fn wake_worker(&self) {}
}

#[derive(Clone)]
struct CaptureUpdateSink {
    session_id: CaptureSessionId,
    sender: Sender<CaptureUpdate>,
}

impl CaptureUpdateSink {
    fn phase(&self, phase: CapturePhase) -> bool {
        self.sender
            .send(CaptureUpdate::Phase(self.session_id, phase))
            .is_ok()
    }

    fn completed(&self, outcome: CaptureOutcome) -> bool {
        self.sender
            .send(CaptureUpdate::Completed(self.session_id, outcome))
            .is_ok()
    }
}

enum CaptureUpdate {
    Phase(CaptureSessionId, CapturePhase),
    Completed(CaptureSessionId, CaptureOutcome),
}

/// A runtime owns installation, sampling, event delivery, native teardown and
/// suppression release. Its return value is published only after `run_session`
/// has completed those steps.
pub trait CaptureRuntime: Send + Sync + 'static {
    fn run_session(
        &self,
        session_id: CaptureSessionId,
        control: CaptureControl,
        updates: CaptureUpdatePublisher,
    ) -> CaptureOutcome;
}

/// Runtime-side phase publisher. Hook callbacks do not call this; their owner
/// reports state after returning to the capture worker's message pump.
#[derive(Clone)]
pub struct CaptureUpdatePublisher(CaptureUpdateSink);

impl CaptureUpdatePublisher {
    pub fn publish_phase(&self, phase: CapturePhase) -> bool {
        self.0.phase(phase)
    }
}

struct CaptureWorker {
    session_id: CaptureSessionId,
    control: CaptureControl,
    join: JoinHandle<()>,
}

/// Owns at most one capture worker. Repeated `begin` calls return the active
/// session id; cancellation and shutdown only signal the worker and never join
/// it while an owned input transition may still be held.
pub struct CoordinateCaptureController {
    runtime: Arc<dyn CaptureRuntime>,
    next_session_id: u64,
    worker: Option<CaptureWorker>,
    updates_tx: Sender<CaptureUpdate>,
    updates_rx: Receiver<CaptureUpdate>,
    status: Option<CaptureStatus>,
    pending_completion: Option<(CaptureSessionId, CaptureOutcome)>,
    completed: VecDeque<CaptureStatus>,
}

impl CoordinateCaptureController {
    pub fn new(runtime: Arc<dyn CaptureRuntime>) -> Self {
        let (updates_tx, updates_rx) = mpsc::channel();
        Self {
            runtime,
            next_session_id: 0,
            worker: None,
            updates_tx,
            updates_rx,
            status: None,
            pending_completion: None,
            completed: VecDeque::new(),
        }
    }

    pub fn begin(&mut self) -> Result<CaptureSessionId, String> {
        if let Some(worker) = self.worker.as_ref() {
            return Ok(worker.session_id);
        }
        if !self.completed.is_empty() {
            return Err(
                "A completed coordinate capture must be consumed before starting another".into(),
            );
        }

        self.next_session_id = self
            .next_session_id
            .checked_add(1)
            .ok_or_else(|| "Coordinate capture session id space was exhausted".to_string())?;
        let session_id = CaptureSessionId(self.next_session_id);
        let control = CaptureControl::new();
        self.status = Some(CaptureStatus {
            session_id,
            phase: CapturePhase::Arming,
            outcome: None,
        });
        self.pending_completion = None;

        let runtime = Arc::clone(&self.runtime);
        let updates = CaptureUpdateSink {
            session_id,
            sender: self.updates_tx.clone(),
        };
        let thread_control = control.clone();
        let join = thread::Builder::new()
            .name("coordinate-tool-capture".into())
            .spawn(move || {
                let outcome = runtime.run_session(
                    session_id,
                    thread_control,
                    CaptureUpdatePublisher(updates.clone()),
                );
                let _ = updates.completed(outcome);
            })
            .map_err(|error| {
                let status = CaptureStatus {
                    session_id,
                    phase: CapturePhase::Completed,
                    outcome: Some(CaptureOutcome::Failed(format!(
                        "Could not start coordinate capture worker: {error}"
                    ))),
                };
                self.status = Some(status.clone());
                self.completed.push_back(status);
                error.to_string()
            })?;
        self.worker = Some(CaptureWorker {
            session_id,
            control,
            join,
        });
        Ok(session_id)
    }

    /// Request cancellation without waiting for a consumed button/key release.
    pub fn cancel(&self) -> bool {
        let Some(worker) = self.active_worker() else {
            return false;
        };
        worker.control.request_cancel();
        true
    }

    /// Request teardown without blocking the calling GUI thread.
    pub fn request_shutdown(&self) -> bool {
        let Some(worker) = self.active_worker() else {
            return false;
        };
        worker.control.request_shutdown();
        true
    }

    pub fn status(&self) -> Option<&CaptureStatus> {
        self.status.as_ref()
    }

    /// A session remains active until its worker has been joined and its
    /// terminal outcome has been consumed by the owning GUI adapter.
    pub fn is_active(&self) -> bool {
        self.worker.is_some()
    }

    /// Take the oldest terminal result, including results whose status was
    /// replaced by a later `begin` call.
    pub fn take_completed(&mut self) -> Option<CaptureStatus> {
        self.completed.pop_front()
    }

    pub fn poll(&mut self) -> bool {
        // Check completion before draining. If the worker exits just after
        // this check, its terminal message remains queued for the next poll.
        let finished = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.join.is_finished());
        let mut changed = self.drain_updates();

        if finished {
            if let Some(worker) = self.worker.take() {
                let result = worker.join.join();
                // The terminal send is the worker's final action, so a message
                // can arrive after the first drain but before `is_finished`.
                // Join first, then drain once more before starting another id.
                changed |= self.drain_updates();
                let outcome = match (result, self.take_pending_completion(worker.session_id)) {
                    (Err(_), _) => CaptureOutcome::Failed(
                        "Coordinate capture worker stopped before cleanup was confirmed".into(),
                    ),
                    (Ok(()), Some(outcome)) => outcome,
                    (Ok(()), None) => CaptureOutcome::Failed(
                        "Coordinate capture worker exited without a terminal result".into(),
                    ),
                };
                changed |= self.finish_worker(worker.session_id, outcome);
                changed = true;
            }
        }
        changed
    }

    fn drain_updates(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.updates_rx.try_recv() {
                Ok(CaptureUpdate::Phase(session_id, phase)) => {
                    changed |= self.apply_phase_update(session_id, phase)
                }
                Ok(CaptureUpdate::Completed(session_id, outcome)) => {
                    changed |= self.stage_completion(session_id, outcome)
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    fn take_pending_completion(&mut self, session_id: CaptureSessionId) -> Option<CaptureOutcome> {
        match self.pending_completion.take() {
            Some((pending_id, outcome)) if pending_id == session_id => Some(outcome),
            _ => None,
        }
    }

    fn stage_completion(&mut self, session_id: CaptureSessionId, outcome: CaptureOutcome) -> bool {
        if !self
            .status
            .as_ref()
            .is_some_and(|status| status.session_id == session_id)
        {
            return false;
        }
        self.pending_completion = Some((session_id, outcome));
        true
    }

    fn finish_worker(&mut self, session_id: CaptureSessionId, outcome: CaptureOutcome) -> bool {
        let Some(status) = self
            .status
            .as_mut()
            .filter(|status| status.session_id == session_id)
        else {
            return false;
        };
        status.phase = CapturePhase::Completed;
        status.outcome = Some(outcome);
        self.completed.push_back(status.clone());
        true
    }

    fn active_worker(&self) -> Option<&CaptureWorker> {
        self.worker.as_ref().filter(|worker| {
            self.status.as_ref().is_some_and(|status| {
                status.session_id == worker.session_id && status.phase != CapturePhase::Completed
            })
        })
    }

    fn apply_phase_update(&mut self, session_id: CaptureSessionId, phase: CapturePhase) -> bool {
        let Some(status) = self.status.as_mut() else {
            return false;
        };
        if status.session_id != session_id {
            return false;
        }
        if self
            .pending_completion
            .as_ref()
            .is_some_and(|(pending_id, _)| *pending_id == session_id)
        {
            return false;
        }
        if status.phase == CapturePhase::Completed {
            return false;
        }
        // A late phase notification cannot move a requested teardown backwards.
        if matches!(
            status.phase,
            CapturePhase::Draining | CapturePhase::TearingDown
        ) && matches!(
            phase,
            CapturePhase::Arming | CapturePhase::WaitingForFreshClick | CapturePhase::Capturing
        ) {
            return false;
        }
        status.phase = phase;
        true
    }
}

impl Drop for CoordinateCaptureController {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.control.request_shutdown();
            // Dropping a JoinHandle detaches. The worker still owns its hooks,
            // state machine and suppression lease until the matching release.
            drop(worker.join);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InitialInputState {
    pub left_button_down: bool,
    pub escape_down: bool,
}

#[derive(Clone, Debug)]
enum MachineState {
    Arming {
        observed_left_down: bool,
        observed_escape_down: bool,
        requested: Option<CaptureOutcome>,
    },
    Waiting {
        ignore_left_until_up: bool,
        escape_unowned: bool,
    },
    Capturing {
        sample: Result<CoordinateSample, String>,
        escape_unowned: bool,
    },
    Draining {
        outcome: CaptureOutcome,
        left_owned: bool,
        escape_owned: bool,
        escape_unowned: bool,
    },
    TearingDown(CaptureOutcome),
    Completed(CaptureOutcome),
}

/// Pure transition machine used directly by the Windows hook callbacks and by
/// deterministic tests. Injected fresh left-button pairs follow the same
/// capture semantics as physical pairs; no provenance is silently exempted.
pub(crate) struct CaptureMachine {
    session_id: CaptureSessionId,
    state: MachineState,
}

impl CaptureMachine {
    fn new(session_id: CaptureSessionId) -> Self {
        Self {
            session_id,
            state: MachineState::Arming {
                observed_left_down: false,
                observed_escape_down: false,
                requested: None,
            },
        }
    }

    pub(crate) fn status(&self) -> CaptureStatus {
        let (phase, outcome) = match &self.state {
            MachineState::Arming { .. } => (CapturePhase::Arming, None),
            MachineState::Waiting { .. } => (CapturePhase::WaitingForFreshClick, None),
            MachineState::Capturing { .. } => (CapturePhase::Capturing, None),
            MachineState::Draining { .. } => (CapturePhase::Draining, None),
            MachineState::TearingDown(_) => (CapturePhase::TearingDown, None),
            MachineState::Completed(outcome) => (CapturePhase::Completed, Some(outcome.clone())),
        };
        CaptureStatus {
            session_id: self.session_id,
            phase,
            outcome,
        }
    }

    pub(crate) fn seed_initial_state(&mut self, initial: InitialInputState) {
        if let MachineState::Arming {
            observed_left_down,
            observed_escape_down,
            ..
        } = &mut self.state
        {
            *observed_left_down = initial.left_button_down;
            *observed_escape_down = initial.escape_down;
        }
    }

    pub(crate) fn install_complete(&mut self, final_state: InitialInputState) {
        let previous = std::mem::replace(
            &mut self.state,
            MachineState::TearingDown(CaptureOutcome::Failed(
                "capture machine left arming unexpectedly".into(),
            )),
        );
        let MachineState::Arming {
            observed_left_down,
            observed_escape_down,
            requested,
        } = previous
        else {
            return;
        };
        if let Some(outcome) = requested {
            self.state = MachineState::TearingDown(outcome);
        } else {
            self.state = MachineState::Waiting {
                ignore_left_until_up: observed_left_down || final_state.left_button_down,
                escape_unowned: observed_escape_down || final_state.escape_down,
            };
        }
    }

    fn install_failed(&mut self, error: String) {
        self.state = MachineState::TearingDown(CaptureOutcome::Failed(error));
    }

    pub(crate) fn handle_input(
        &mut self,
        input: CaptureInput,
        mut sample_at: impl FnMut(PhysicalPoint) -> Result<CoordinateSample, String>,
    ) -> CaptureDisposition {
        let state = std::mem::replace(
            &mut self.state,
            MachineState::TearingDown(CaptureOutcome::Failed(
                "capture transition was interrupted".into(),
            )),
        );
        match state {
            MachineState::Arming {
                mut observed_left_down,
                mut observed_escape_down,
                requested,
            } => {
                match input {
                    CaptureInput::Button {
                        button: CaptureButton::Left,
                        edge,
                        ..
                    } => observed_left_down = edge == ButtonEdge::Down,
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge,
                        ..
                    } => observed_escape_down = edge == ButtonEdge::Down,
                    _ => {}
                }
                self.state = MachineState::Arming {
                    observed_left_down,
                    observed_escape_down,
                    requested,
                };
                CaptureDisposition::PassThrough
            }
            MachineState::Waiting {
                mut ignore_left_until_up,
                mut escape_unowned,
            } => match input {
                CaptureInput::Button {
                    button: CaptureButton::Left,
                    edge: ButtonEdge::Down,
                    point,
                    injected: _,
                } if !ignore_left_until_up => {
                    let sample = sample_at(point);
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned,
                    };
                    CaptureDisposition::Consume
                }
                CaptureInput::Button {
                    button: CaptureButton::Left,
                    edge: ButtonEdge::Up,
                    ..
                } if ignore_left_until_up => {
                    ignore_left_until_up = false;
                    self.state = MachineState::Waiting {
                        ignore_left_until_up,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    injected: _,
                    ..
                } if escape_unowned => {
                    self.state = MachineState::Waiting {
                        ignore_left_until_up,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    modifiers,
                    injected: _,
                } if modifiers.any() => {
                    escape_unowned = true;
                    self.state = MachineState::Waiting {
                        ignore_left_until_up,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    ..
                } => {
                    self.state = MachineState::Draining {
                        outcome: CaptureOutcome::Cancelled,
                        left_owned: false,
                        escape_owned: true,
                        escape_unowned: false,
                    };
                    CaptureDisposition::Consume
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Up,
                    ..
                } if escape_unowned => {
                    escape_unowned = false;
                    self.state = MachineState::Waiting {
                        ignore_left_until_up,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                _ => {
                    self.state = MachineState::Waiting {
                        ignore_left_until_up,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
            },
            MachineState::Capturing {
                sample,
                mut escape_unowned,
            } => match input {
                CaptureInput::Button {
                    button: CaptureButton::Left,
                    edge: ButtonEdge::Down,
                    ..
                } => {
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned,
                    };
                    CaptureDisposition::Consume
                }
                CaptureInput::Button {
                    button: CaptureButton::Left,
                    edge: ButtonEdge::Up,
                    ..
                } => {
                    self.state = MachineState::TearingDown(match sample {
                        Ok(sample) => CaptureOutcome::Captured(sample),
                        Err(error) => CaptureOutcome::Failed(error),
                    });
                    CaptureDisposition::Consume
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    injected: _,
                    ..
                } if escape_unowned => {
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    modifiers,
                    injected: _,
                } if !modifiers.any() => {
                    self.state = MachineState::Draining {
                        outcome: CaptureOutcome::Cancelled,
                        left_owned: true,
                        escape_owned: true,
                        escape_unowned: false,
                    };
                    CaptureDisposition::Consume
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Down,
                    modifiers,
                    injected: _,
                } if modifiers.any() => {
                    escape_unowned = true;
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
                CaptureInput::Key {
                    key: CaptureKey::Escape,
                    edge: ButtonEdge::Up,
                    ..
                } if escape_unowned => {
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned: false,
                    };
                    CaptureDisposition::PassThrough
                }
                _ => {
                    self.state = MachineState::Capturing {
                        sample,
                        escape_unowned,
                    };
                    CaptureDisposition::PassThrough
                }
            },
            MachineState::Draining {
                outcome,
                mut left_owned,
                mut escape_owned,
                mut escape_unowned,
            } => {
                let disposition = match input {
                    CaptureInput::Button {
                        button: CaptureButton::Left,
                        edge: ButtonEdge::Down,
                        ..
                    } if left_owned => CaptureDisposition::Consume,
                    CaptureInput::Button {
                        button: CaptureButton::Left,
                        edge: ButtonEdge::Up,
                        ..
                    } if left_owned => {
                        left_owned = false;
                        CaptureDisposition::Consume
                    }
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Down,
                        ..
                    } if escape_owned => CaptureDisposition::Consume,
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Up,
                        ..
                    } if escape_owned => {
                        escape_owned = false;
                        CaptureDisposition::Consume
                    }
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Up,
                        ..
                    } if escape_unowned => {
                        escape_unowned = false;
                        CaptureDisposition::PassThrough
                    }
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Down,
                        ..
                    } if escape_unowned => CaptureDisposition::PassThrough,
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Down,
                        modifiers,
                        injected: _,
                    } if !escape_owned && !modifiers.any() => {
                        escape_owned = true;
                        CaptureDisposition::Consume
                    }
                    CaptureInput::Key {
                        key: CaptureKey::Escape,
                        edge: ButtonEdge::Down,
                        modifiers,
                        injected: _,
                    } if !escape_owned && modifiers.any() => {
                        escape_unowned = true;
                        CaptureDisposition::PassThrough
                    }
                    _ => CaptureDisposition::PassThrough,
                };
                if !left_owned && !escape_owned {
                    self.state = MachineState::TearingDown(outcome);
                } else {
                    self.state = MachineState::Draining {
                        outcome,
                        left_owned,
                        escape_owned,
                        escape_unowned,
                    };
                }
                disposition
            }
            MachineState::TearingDown(outcome) => {
                self.state = MachineState::TearingDown(outcome);
                CaptureDisposition::PassThrough
            }
            MachineState::Completed(outcome) => {
                self.state = MachineState::Completed(outcome);
                CaptureDisposition::PassThrough
            }
        }
    }

    pub(crate) fn request_cancel(&mut self) {
        self.request_outcome(CaptureOutcome::Cancelled);
    }

    pub(crate) fn request_shutdown(&mut self) {
        self.request_outcome(CaptureOutcome::Shutdown);
    }

    pub(crate) fn request_failure(&mut self, error: impl Into<String>) {
        self.request_outcome(CaptureOutcome::Failed(error.into()));
    }

    fn request_outcome(&mut self, requested: CaptureOutcome) {
        let state = std::mem::replace(
            &mut self.state,
            MachineState::TearingDown(CaptureOutcome::Failed(
                "capture request was interrupted".into(),
            )),
        );
        self.state = match state {
            MachineState::Arming {
                observed_left_down,
                observed_escape_down,
                requested: previous,
            } => MachineState::Arming {
                observed_left_down,
                observed_escape_down,
                requested: Some(prefer_terminal(previous, requested)),
            },
            MachineState::Waiting { .. } => MachineState::TearingDown(requested),
            MachineState::Capturing { escape_unowned, .. } => MachineState::Draining {
                outcome: requested,
                left_owned: true,
                escape_owned: false,
                escape_unowned,
            },
            MachineState::Draining {
                outcome,
                left_owned,
                escape_owned,
                escape_unowned,
            } => MachineState::Draining {
                outcome: prefer_terminal(Some(outcome), requested),
                left_owned,
                escape_owned,
                escape_unowned,
            },
            MachineState::TearingDown(outcome) => MachineState::TearingDown(outcome),
            MachineState::Completed(outcome) => MachineState::Completed(outcome),
        };
        self.enter_teardown_if_drained();
    }

    fn apply_control(&mut self, control: &CaptureControl) {
        match control.requested() {
            Some(CaptureRequest::Cancel) => self.request_cancel(),
            Some(CaptureRequest::Shutdown) => self.request_shutdown(),
            None => {}
        }
    }

    fn enter_teardown_if_drained(&mut self) {
        let state = std::mem::replace(
            &mut self.state,
            MachineState::TearingDown(CaptureOutcome::Failed(
                "capture drain was interrupted".into(),
            )),
        );
        self.state = match state {
            MachineState::Draining {
                outcome,
                left_owned: false,
                escape_owned: false,
                escape_unowned: _,
            } => MachineState::TearingDown(outcome),
            state => state,
        };
    }

    pub(crate) fn needs_teardown(&self) -> bool {
        matches!(self.state, MachineState::TearingDown(_))
    }

    pub(crate) fn complete_teardown(&mut self, cleanup: Result<(), String>) -> CaptureOutcome {
        let previous = std::mem::replace(
            &mut self.state,
            MachineState::Completed(CaptureOutcome::Failed(
                "capture teardown was interrupted".into(),
            )),
        );
        let outcome = match previous {
            MachineState::TearingDown(outcome) => match cleanup {
                Ok(()) => outcome,
                Err(error) => CaptureOutcome::Failed(error),
            },
            _ => CaptureOutcome::Failed("capture teardown started before input drain".into()),
        };
        self.state = MachineState::Completed(outcome.clone());
        outcome
    }
}

fn prefer_terminal(previous: Option<CaptureOutcome>, requested: CaptureOutcome) -> CaptureOutcome {
    match (previous, requested) {
        (Some(CaptureOutcome::Failed(error)), _) => CaptureOutcome::Failed(error),
        (Some(CaptureOutcome::Shutdown), _) => CaptureOutcome::Shutdown,
        (Some(CaptureOutcome::Cancelled), CaptureOutcome::Cancelled) => CaptureOutcome::Cancelled,
        (Some(CaptureOutcome::Cancelled), CaptureOutcome::Shutdown) => CaptureOutcome::Shutdown,
        (Some(CaptureOutcome::Cancelled), CaptureOutcome::Failed(error)) => {
            CaptureOutcome::Failed(error)
        }
        (_, outcome @ CaptureOutcome::Failed(_)) => outcome,
        (_, CaptureOutcome::Shutdown) => CaptureOutcome::Shutdown,
        (_, CaptureOutcome::Cancelled) => CaptureOutcome::Cancelled,
        (Some(outcome), CaptureOutcome::Captured(_)) => outcome,
        (None, CaptureOutcome::Captured(sample)) => CaptureOutcome::Captured(sample),
    }
}

pub(crate) trait CaptureSessionDriver {
    fn install(
        &mut self,
        machine: Rc<RefCell<CaptureMachine>>,
        control: CaptureControl,
    ) -> Result<InitialInputState, String>;

    fn pump(
        &mut self,
        machine: &Rc<RefCell<CaptureMachine>>,
        control: &CaptureControl,
        updates: &CaptureUpdatePublisher,
    ) -> Result<(), String>;

    fn teardown(&mut self) -> Result<(), String>;
}

pub(crate) fn drive_capture_session(
    session_id: CaptureSessionId,
    control: CaptureControl,
    updates: CaptureUpdatePublisher,
    driver: &mut dyn CaptureSessionDriver,
) -> CaptureOutcome {
    let machine = Rc::new(RefCell::new(CaptureMachine::new(session_id)));
    let _ = updates.publish_phase(CapturePhase::Arming);

    match driver.install(Rc::clone(&machine), control.clone()) {
        Ok(initial_state) => machine.borrow_mut().install_complete(initial_state),
        Err(error) => machine.borrow_mut().install_failed(error),
    }
    machine.borrow_mut().apply_control(&control);
    publish_machine_phase(&machine, &updates, &control);

    if !machine.borrow().needs_teardown() {
        if let Err(error) = driver.pump(&machine, &control, &updates) {
            machine.borrow_mut().request_failure(error);
        }
    }
    machine.borrow_mut().apply_control(&control);
    if !machine.borrow().needs_teardown() {
        machine
            .borrow_mut()
            .request_failure("capture input pump stopped before the owned input drained");
    }

    let _ = updates.publish_phase(CapturePhase::TearingDown);
    let cleanup = driver.teardown();
    machine.borrow_mut().complete_teardown(cleanup)
}

fn publish_machine_phase(
    machine: &Rc<RefCell<CaptureMachine>>,
    updates: &CaptureUpdatePublisher,
    control: &CaptureControl,
) {
    let phase = machine.borrow().status().phase;
    if !updates.publish_phase(phase) {
        control.request_shutdown();
    }
}

#[derive(Default)]
pub struct NativeCoordinateCaptureRuntime;

impl CaptureRuntime for NativeCoordinateCaptureRuntime {
    fn run_session(
        &self,
        session_id: CaptureSessionId,
        control: CaptureControl,
        updates: CaptureUpdatePublisher,
    ) -> CaptureOutcome {
        #[cfg(windows)]
        {
            let mut driver = windows_native::WindowsCaptureDriver::default();
            return drive_capture_session(session_id, control, updates, &mut driver);
        }
        #[cfg(not(windows))]
        {
            let mut driver = UnavailableCaptureDriver;
            drive_capture_session(session_id, control, updates, &mut driver)
        }
    }
}

#[cfg(not(windows))]
struct UnavailableCaptureDriver;

#[cfg(not(windows))]
impl CaptureSessionDriver for UnavailableCaptureDriver {
    fn install(
        &mut self,
        _machine: Rc<RefCell<CaptureMachine>>,
        _control: CaptureControl,
    ) -> Result<InitialInputState, String> {
        Err("Coordinate capture input hooks are available only on Windows".into())
    }

    fn pump(
        &mut self,
        _machine: &Rc<RefCell<CaptureMachine>>,
        _control: &CaptureControl,
        _updates: &CaptureUpdatePublisher,
    ) -> Result<(), String> {
        Ok(())
    }

    fn teardown(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
mod windows_native {
    use super::*;
    use std::mem;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, HHOOK, KBDLLHOOKSTRUCT, LLMHF_INJECTED,
        LLMHF_LOWER_IL_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PM_REMOVE, PeekMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
        WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_QUIT, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    pub(super) const WM_CAPTURE_WAKE: u32 = WM_APP + 0x31;

    thread_local! {
        static CALLBACK_CONTEXT: RefCell<Option<CallbackContext>> = const { RefCell::new(None) };
    }

    struct CallbackContext {
        machine: Rc<RefCell<CaptureMachine>>,
        control: CaptureControl,
        sampler: crate::coordinate_tool::NativeCoordinatePointSampler,
        thread_id: u32,
    }

    #[derive(Default)]
    pub(super) struct WindowsCaptureDriver {
        machine: Option<Rc<RefCell<CaptureMachine>>>,
        control: Option<CaptureControl>,
        thread_id: u32,
        mouse_hook: Option<HHOOK>,
        keyboard_hook: Option<HHOOK>,
        suppression: Option<crate::mouse_gestures::service::GestureSuppressionGuard>,
        callback_context_installed: bool,
    }

    impl CaptureSessionDriver for WindowsCaptureDriver {
        fn install(
            &mut self,
            machine: Rc<RefCell<CaptureMachine>>,
            control: CaptureControl,
        ) -> Result<InitialInputState, String> {
            self.machine = Some(Rc::clone(&machine));
            self.control = Some(control.clone());
            self.suppression = Some(crate::mouse_gestures::service::acquire_gesture_suppression());
            let sampler = crate::coordinate_tool::NativeCoordinatePointSampler::new()?;

            let mut queue_probe = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut queue_probe, None, 0, 0, PM_NOREMOVE);
            }
            self.thread_id = unsafe { GetCurrentThreadId() };
            control.set_thread_id(self.thread_id);

            let before_install = read_initial_input_state();
            machine.borrow_mut().seed_initial_state(before_install);
            CALLBACK_CONTEXT.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_some() {
                    return Err("another coordinate capture is active on this worker".to_string());
                }
                *slot = Some(CallbackContext {
                    machine: Rc::clone(&machine),
                    control: control.clone(),
                    sampler,
                    thread_id: self.thread_id,
                });
                Ok(())
            })?;
            self.callback_context_installed = true;

            let module = HINSTANCE(
                unsafe { GetModuleHandleW(None) }
                    .map_err(|error| format!("Could not load the coordinate hook module: {error}"))?
                    .0,
            );
            self.mouse_hook = Some(
                unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), module, 0) }
                    .map_err(|error| {
                        format!("Could not install the coordinate mouse hook: {error}")
                    })?,
            );
            self.keyboard_hook = Some(
                unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), module, 0) }
                    .map_err(|error| {
                    format!("Could not install the coordinate keyboard hook: {error}")
                })?,
            );

            // Recheck after both hooks are installed. The callback reducer also
            // tracks arming transitions, closing the query/install race.
            Ok(read_initial_input_state())
        }

        fn pump(
            &mut self,
            machine: &Rc<RefCell<CaptureMachine>>,
            control: &CaptureControl,
            updates: &CaptureUpdatePublisher,
        ) -> Result<(), String> {
            let mut message = MSG::default();
            let mut published_phase = machine.borrow().status().phase;
            loop {
                machine.borrow_mut().apply_control(control);
                if machine.borrow().needs_teardown() {
                    return Ok(());
                }

                let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
                if result.0 < 0 {
                    machine
                        .borrow_mut()
                        .request_failure("Coordinate hook message pump failed".into());
                    if machine.borrow().needs_teardown() {
                        return Ok(());
                    }
                    // Preserve an already-owned button/key pair while waiting
                    // for its matching release, even after a pump error.
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::WaitMessage();
                        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                            if message.message != WM_CAPTURE_WAKE && message.message != WM_QUIT {
                                let _ = TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                    }
                    publish_current_phase(machine, updates, control, &mut published_phase);
                    continue;
                }
                if result.0 == 0 {
                    machine.borrow_mut().request_shutdown();
                } else if message.message != WM_CAPTURE_WAKE {
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                publish_current_phase(machine, updates, control, &mut published_phase);
            }
        }

        fn teardown(&mut self) -> Result<(), String> {
            let mut errors = Vec::new();
            unhook(&mut self.keyboard_hook, "keyboard", &mut errors);
            unhook(&mut self.mouse_hook, "mouse", &mut errors);

            if self.callback_context_installed {
                CALLBACK_CONTEXT.with(|slot| {
                    slot.borrow_mut().take();
                });
                self.callback_context_installed = false;
            }
            self.machine.take();
            self.control.take();
            self.thread_id = 0;

            if let Some(mut suppression) = self.suppression.take() {
                suppression.release_synchronously();
            }

            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors.join("; "))
            }
        }
    }

    impl Drop for WindowsCaptureDriver {
        fn drop(&mut self) {
            let _ = self.teardown();
        }
    }

    fn unhook(slot: &mut Option<HHOOK>, label: &str, errors: &mut Vec<String>) {
        let Some(hook) = slot.as_ref().copied() else {
            return;
        };
        match unsafe { UnhookWindowsHookEx(hook) } {
            Ok(()) => *slot = None,
            Err(error) => errors.push(format!("Could not remove coordinate {label} hook: {error}")),
        }
    }

    fn read_initial_input_state() -> InitialInputState {
        let down = |key| unsafe { GetAsyncKeyState(key.0 as i32) < 0 };
        InitialInputState {
            left_button_down: down(VK_LBUTTON),
            escape_down: down(VK_ESCAPE),
        }
    }

    fn read_modifiers() -> CaptureModifiers {
        let down = |key| unsafe { GetAsyncKeyState(key.0 as i32) < 0 };
        CaptureModifiers {
            control: down(VK_LCONTROL) || down(VK_RCONTROL),
            alt: down(VK_LMENU) || down(VK_RMENU),
            shift: down(VK_LSHIFT) || down(VK_RSHIFT),
            windows: down(VK_LWIN) || down(VK_RWIN),
        }
    }

    fn publish_current_phase(
        machine: &Rc<RefCell<CaptureMachine>>,
        updates: &CaptureUpdatePublisher,
        control: &CaptureControl,
        previous: &mut CapturePhase,
    ) {
        machine.borrow_mut().apply_control(control);
        let phase = machine.borrow().status().phase;
        if phase != *previous {
            if !updates.publish_phase(phase) {
                control.request_shutdown();
                machine.borrow_mut().apply_control(control);
            }
            *previous = phase;
        }
    }

    fn wake_owner(thread_id: u32) {
        use windows::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_CAPTURE_WAKE, WPARAM(0), LPARAM(0));
        }
    }

    unsafe extern "system" fn mouse_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }
        let message = wparam.0 as u32;
        if message != WM_LBUTTONDOWN && message != WM_LBUTTONUP {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }

        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let input = CaptureInput::Button {
            button: CaptureButton::Left,
            edge: if message == WM_LBUTTONDOWN {
                ButtonEdge::Down
            } else {
                ButtonEdge::Up
            },
            point: PhysicalPoint::new(info.pt.x, info.pt.y),
            injected: (info.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED)) != 0,
        };
        let disposition = catch_unwind(AssertUnwindSafe(|| {
            CALLBACK_CONTEXT.with(|slot| {
                let mut slot = slot.try_borrow_mut().ok()?;
                let context = slot.as_mut()?;
                let mut machine = context.machine.try_borrow_mut().ok()?;
                machine.apply_control(&context.control);
                let result = machine.handle_input(input, |point| context.sampler.sample_at(point));
                if result == CaptureDisposition::Consume {
                    wake_owner(context.thread_id);
                }
                Some(result)
            })
        }))
        .ok()
        .flatten();
        if disposition == Some(CaptureDisposition::Consume) {
            LRESULT(1)
        } else {
            unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
        }
    }

    unsafe extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }
        let message = wparam.0 as u32;
        let edge = match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => ButtonEdge::Down,
            WM_KEYUP | WM_SYSKEYUP => ButtonEdge::Up,
            _ => return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) },
        };
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let key = if info.vkCode == VK_ESCAPE.0 as u32 {
            CaptureKey::Escape
        } else {
            CaptureKey::Other(info.vkCode)
        };
        if key != CaptureKey::Escape {
            return unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) };
        }
        let injected = info
            .flags
            .contains(windows::Win32::UI::WindowsAndMessaging::LLKHF_INJECTED);
        let input = CaptureInput::Key {
            key,
            edge,
            modifiers: read_modifiers(),
            injected,
        };
        let disposition = catch_unwind(AssertUnwindSafe(|| {
            CALLBACK_CONTEXT.with(|slot| {
                let mut slot = slot.try_borrow_mut().ok()?;
                let context = slot.as_mut()?;
                let mut machine = context.machine.try_borrow_mut().ok()?;
                machine.apply_control(&context.control);
                let result = machine.handle_input(input, |_| {
                    Err("Escape does not sample mouse coordinates".into())
                });
                if result == CaptureDisposition::Consume {
                    wake_owner(context.thread_id);
                }
                Some(result)
            })
        }))
        .ok()
        .flatten();
        if disposition == Some(CaptureDisposition::Consume) {
            LRESULT(1)
        } else {
            unsafe { CallNextHookEx(HHOOK::default(), code, wparam, lparam) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinate_tool::PhysicalPoint;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    fn sample(point: PhysicalPoint) -> CoordinateSample {
        CoordinateSample::new(point, None, None, None)
    }

    fn ready_machine(left_down: bool, escape_down: bool) -> CaptureMachine {
        let mut machine = CaptureMachine::new(CaptureSessionId(1));
        let initial = InitialInputState {
            left_button_down: left_down,
            escape_down,
        };
        machine.seed_initial_state(initial);
        machine.install_complete(initial);
        machine
    }

    fn left(edge: ButtonEdge, point: PhysicalPoint, injected: bool) -> CaptureInput {
        CaptureInput::Button {
            button: CaptureButton::Left,
            edge,
            point,
            injected,
        }
    }

    fn escape(edge: ButtonEdge, modifiers: CaptureModifiers, injected: bool) -> CaptureInput {
        CaptureInput::Key {
            key: CaptureKey::Escape,
            edge,
            modifiers,
            injected,
        }
    }

    fn no_sample(_: PhysicalPoint) -> Result<CoordinateSample, String> {
        panic!("only a fresh left-button down may sample")
    }

    #[test]
    fn capture_samples_at_fresh_down_and_consumes_the_matching_pair() {
        let click_point = PhysicalPoint::new(-1800, 230);
        let release_point = PhysicalPoint::new(40, 80);
        let mut machine = ready_machine(false, false);
        let sample_calls = AtomicUsize::new(0);

        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, click_point, true), |point| {
                assert_eq!(point, click_point);
                sample_calls.fetch_add(1, Ordering::Relaxed);
                Ok(sample(point))
            }),
            CaptureDisposition::Consume
        );
        assert_eq!(machine.status().phase, CapturePhase::Capturing);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, release_point, true), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(sample_calls.load(Ordering::Relaxed), 1);
        assert_eq!(machine.status().phase, CapturePhase::TearingDown);

        // A later click arrives during teardown and is forwarded.
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, release_point, false), no_sample),
            CaptureDisposition::PassThrough
        );
        assert_eq!(
            machine.complete_teardown(Ok(())),
            CaptureOutcome::Captured(sample(click_point))
        );
    }

    #[test]
    fn escape_mid_pair_drains_both_owned_releases_and_cancels_the_sample() {
        let click_point = PhysicalPoint::new(-300, 20);
        let mut machine = ready_machine(false, false);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, click_point, false), |point| {
                Ok(sample(point))
            }),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Down, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::Consume
        );
        assert_eq!(machine.status().phase, CapturePhase::Draining);
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Up, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::Consume
        );
        assert_eq!(machine.status().phase, CapturePhase::Draining);
        assert_eq!(
            machine.handle_input(
                left(ButtonEdge::Up, PhysicalPoint::new(9, 9), false),
                no_sample
            ),
            CaptureDisposition::Consume
        );
        assert_eq!(machine.status().phase, CapturePhase::TearingDown);
        assert_eq!(machine.complete_teardown(Ok(())), CaptureOutcome::Cancelled);
    }

    #[test]
    fn initial_left_and_escape_are_forwarded_until_fresh_transitions() {
        let mut machine = ready_machine(true, true);
        let mut sample_calls = 0;
        for input in [
            left(ButtonEdge::Down, PhysicalPoint::new(1, 2), false),
            left(ButtonEdge::Up, PhysicalPoint::new(1, 2), false),
            escape(ButtonEdge::Down, CaptureModifiers::default(), false),
            escape(ButtonEdge::Up, CaptureModifiers::default(), false),
        ] {
            assert_eq!(
                machine.handle_input(input, |_| {
                    sample_calls += 1;
                    Ok(sample(PhysicalPoint::new(1, 2)))
                }),
                CaptureDisposition::PassThrough
            );
        }
        assert_eq!(sample_calls, 0);
        assert_eq!(machine.status().phase, CapturePhase::WaitingForFreshClick);

        let fresh = PhysicalPoint::new(-40, 7);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, fresh, false), |point| {
                Ok(sample(point))
            }),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, fresh, false), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.complete_teardown(Ok(())),
            CaptureOutcome::Captured(sample(fresh))
        );
    }

    #[test]
    fn preheld_escape_repeat_and_release_stay_forwarded_during_click_capture() {
        let point = PhysicalPoint::new(-25, 12);
        let mut machine = ready_machine(false, true);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, point, false), |at| Ok(sample(at))),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Down, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Up, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(machine.status().phase, CapturePhase::Capturing);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, point, false), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.complete_teardown(Ok(())),
            CaptureOutcome::Captured(sample(point))
        );
    }

    #[test]
    fn modified_escape_down_remains_unowned_after_modifier_release_and_click() {
        let point = PhysicalPoint::new(40, -18);
        let mut machine = ready_machine(false, false);
        let control = CaptureModifiers {
            control: true,
            ..CaptureModifiers::default()
        };
        assert_eq!(
            machine.handle_input(escape(ButtonEdge::Down, control, false), no_sample),
            CaptureDisposition::PassThrough
        );
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, point, false), |at| Ok(sample(at))),
            CaptureDisposition::Consume
        );
        // The modifier is now up, but this Escape down is still a repeat of
        // the forwarded chord's key-down and cannot become an owned cancel.
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Down, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Up, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(machine.status().phase, CapturePhase::Capturing);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, point, false), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.complete_teardown(Ok(())),
            CaptureOutcome::Captured(sample(point))
        );
    }

    #[test]
    fn cancellation_drains_owned_click_and_forwards_unowned_escape_release() {
        let point = PhysicalPoint::new(15, 25);
        let mut machine = ready_machine(false, false);
        assert_eq!(
            machine.handle_input(
                escape(
                    ButtonEdge::Down,
                    CaptureModifiers {
                        shift: true,
                        ..CaptureModifiers::default()
                    },
                    false,
                ),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, point, false), |at| Ok(sample(at))),
            CaptureDisposition::Consume
        );

        machine.request_cancel();
        assert_eq!(machine.status().phase, CapturePhase::Draining);
        assert_eq!(
            machine.handle_input(
                escape(ButtonEdge::Up, CaptureModifiers::default(), false),
                no_sample
            ),
            CaptureDisposition::PassThrough
        );
        assert_eq!(machine.status().phase, CapturePhase::Draining);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, point, false), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(machine.status().phase, CapturePhase::TearingDown);
        assert_eq!(machine.complete_teardown(Ok(())), CaptureOutcome::Cancelled);
    }

    #[test]
    fn unrelated_input_and_escape_emergency_chords_pass_through() {
        let mut machine = ready_machine(false, false);
        let point = PhysicalPoint::new(0, 0);
        for input in [
            CaptureInput::Button {
                button: CaptureButton::Right,
                edge: ButtonEdge::Down,
                point,
                injected: false,
            },
            CaptureInput::Button {
                button: CaptureButton::Right,
                edge: ButtonEdge::Up,
                point,
                injected: false,
            },
            CaptureInput::Wheel {
                delta: 120,
                injected: false,
            },
            CaptureInput::Key {
                key: CaptureKey::Other(0x41),
                edge: ButtonEdge::Down,
                modifiers: CaptureModifiers::default(),
                injected: false,
            },
            escape(
                ButtonEdge::Down,
                CaptureModifiers {
                    control: true,
                    ..CaptureModifiers::default()
                },
                false,
            ),
            escape(
                ButtonEdge::Up,
                CaptureModifiers {
                    control: true,
                    ..CaptureModifiers::default()
                },
                false,
            ),
        ] {
            assert_eq!(
                machine.handle_input(input, no_sample),
                CaptureDisposition::PassThrough
            );
        }
        assert_eq!(machine.status().phase, CapturePhase::WaitingForFreshClick);
    }

    #[test]
    fn injected_fresh_left_pair_is_consumed_with_the_same_semantics() {
        let point = PhysicalPoint::new(-1920, 900);
        let mut machine = ready_machine(false, false);
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Down, point, true), |at| { Ok(sample(at)) }),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.handle_input(left(ButtonEdge::Up, point, true), no_sample),
            CaptureDisposition::Consume
        );
        assert_eq!(
            machine.complete_teardown(Ok(())),
            CaptureOutcome::Captured(sample(point))
        );
    }

    #[test]
    fn cancel_and_shutdown_during_a_pair_keep_owning_its_release() {
        for (stop, expected) in [
            (
                CaptureMachine::request_cancel as fn(&mut CaptureMachine),
                CaptureOutcome::Cancelled,
            ),
            (CaptureMachine::request_shutdown, CaptureOutcome::Shutdown),
        ] {
            let point = PhysicalPoint::new(-1, 2);
            let mut machine = ready_machine(false, false);
            assert_eq!(
                machine.handle_input(left(ButtonEdge::Down, point, false), |point| {
                    Ok(sample(point))
                }),
                CaptureDisposition::Consume
            );
            stop(&mut machine);
            assert_eq!(machine.status().phase, CapturePhase::Draining);
            assert_eq!(
                machine.handle_input(left(ButtonEdge::Up, point, false), no_sample),
                CaptureDisposition::Consume
            );
            assert_eq!(machine.complete_teardown(Ok(())), expected);
        }
    }

    struct FakeDriver {
        initial: InitialInputState,
        inputs: VecDeque<CaptureInput>,
        sample: CoordinateSample,
        fail_keyboard_install: bool,
        fail_teardown: bool,
        mouse_installed: bool,
        keyboard_installed: bool,
        log: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl FakeDriver {
        fn capture(log: Arc<std::sync::Mutex<Vec<&'static str>>>, point: PhysicalPoint) -> Self {
            Self {
                initial: InitialInputState {
                    left_button_down: false,
                    escape_down: false,
                },
                inputs: VecDeque::from([
                    left(ButtonEdge::Down, point, false),
                    left(ButtonEdge::Up, point, false),
                ]),
                sample: sample(point),
                fail_keyboard_install: false,
                fail_teardown: false,
                mouse_installed: false,
                keyboard_installed: false,
                log,
            }
        }
    }

    impl CaptureSessionDriver for FakeDriver {
        fn install(
            &mut self,
            machine: Rc<RefCell<CaptureMachine>>,
            _control: CaptureControl,
        ) -> Result<InitialInputState, String> {
            let mut log = self.log.lock().unwrap();
            log.push("suppression_acquired");
            log.push("sampler_created");
            log.push("mouse_hook_installed");
            self.mouse_installed = true;
            if self.fail_keyboard_install {
                log.push("keyboard_hook_install_failed");
                return Err("injected keyboard hook setup failure".into());
            }
            log.push("keyboard_hook_installed");
            self.keyboard_installed = true;
            machine.borrow_mut().seed_initial_state(self.initial);
            Ok(self.initial)
        }

        fn pump(
            &mut self,
            machine: &Rc<RefCell<CaptureMachine>>,
            _control: &CaptureControl,
            updates: &CaptureUpdatePublisher,
        ) -> Result<(), String> {
            while let Some(input) = self.inputs.pop_front() {
                machine.borrow_mut().handle_input(input, |point| {
                    assert_eq!(point, self.sample.desktop_point);
                    Ok(self.sample.clone())
                });
                let _ = updates.publish_phase(machine.borrow().status().phase);
                if machine.borrow().needs_teardown() {
                    break;
                }
            }
            Ok(())
        }

        fn teardown(&mut self) -> Result<(), String> {
            let mut log = self.log.lock().unwrap();
            if self.keyboard_installed {
                log.push("keyboard_hook_uninstalled");
                self.keyboard_installed = false;
            }
            if self.mouse_installed {
                log.push("mouse_hook_uninstalled");
                self.mouse_installed = false;
            }
            log.push("sampler_dropped");
            log.push("suppression_released");
            if self.fail_teardown {
                Err("injected unhook failure".into())
            } else {
                Ok(())
            }
        }
    }

    fn test_updates(
        session_id: CaptureSessionId,
    ) -> (CaptureUpdatePublisher, Receiver<CaptureUpdate>) {
        let (sender, receiver) = mpsc::channel();
        (
            CaptureUpdatePublisher(CaptureUpdateSink { session_id, sender }),
            receiver,
        )
    }

    #[test]
    fn partial_hook_setup_uninstalls_first_hook_then_releases_sampler_and_suppression() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut driver = FakeDriver::capture(Arc::clone(&log), PhysicalPoint::new(5, 6));
        driver.fail_keyboard_install = true;
        let session_id = CaptureSessionId(8);
        let control = CaptureControl::new();
        let (updates, receiver) = test_updates(session_id);

        let outcome = drive_capture_session(session_id, control, updates, &mut driver);
        assert_eq!(
            outcome,
            CaptureOutcome::Failed("injected keyboard hook setup failure".into())
        );
        assert_eq!(
            *log.lock().unwrap(),
            [
                "suppression_acquired",
                "sampler_created",
                "mouse_hook_installed",
                "keyboard_hook_install_failed",
                "mouse_hook_uninstalled",
                "sampler_dropped",
                "suppression_released",
            ]
        );
        assert!(matches!(
            receiver.try_recv(),
            Ok(CaptureUpdate::Phase(_, CapturePhase::Arming))
        ));
        assert!(matches!(
            receiver.try_recv(),
            Ok(CaptureUpdate::Phase(_, CapturePhase::TearingDown))
        ));
    }

    #[test]
    fn completion_is_not_successful_when_native_teardown_fails() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let point = PhysicalPoint::new(-12, 40);
        let mut driver = FakeDriver::capture(Arc::clone(&log), point);
        driver.fail_teardown = true;
        let session_id = CaptureSessionId(9);
        let (updates, _receiver) = test_updates(session_id);
        let outcome =
            drive_capture_session(session_id, CaptureControl::new(), updates, &mut driver);
        assert_eq!(
            outcome,
            CaptureOutcome::Failed("injected unhook failure".into())
        );
        assert_eq!(log.lock().unwrap().last(), Some(&"suppression_released"));
    }

    struct BlockingRuntime {
        started: Sender<CaptureSessionId>,
        release: std::sync::Mutex<Receiver<()>>,
        returned: Sender<()>,
        run_count: AtomicUsize,
    }

    impl CaptureRuntime for BlockingRuntime {
        fn run_session(
            &self,
            session_id: CaptureSessionId,
            control: CaptureControl,
            updates: CaptureUpdatePublisher,
        ) -> CaptureOutcome {
            self.run_count.fetch_add(1, Ordering::AcqRel);
            let _ = updates.publish_phase(CapturePhase::WaitingForFreshClick);
            let _ = self.started.send(session_id);
            let _ = self.release.lock().unwrap().recv();
            let _ = self.returned.send(());
            match control.requested() {
                Some(CaptureRequest::Shutdown) => CaptureOutcome::Shutdown,
                Some(CaptureRequest::Cancel) => CaptureOutcome::Cancelled,
                None => {
                    CaptureOutcome::Failed("fake runtime returned without a terminal input".into())
                }
            }
        }
    }

    #[test]
    fn controller_start_is_idempotent_and_cancel_never_joins_the_worker() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (returned_tx, returned_rx) = mpsc::channel();
        let runtime = Arc::new(BlockingRuntime {
            started: started_tx,
            release: std::sync::Mutex::new(release_rx),
            returned: returned_tx,
            run_count: AtomicUsize::new(0),
        });
        let mut controller = CoordinateCaptureController::new(runtime.clone());
        let session = controller.begin().unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            session
        );
        assert_eq!(controller.begin().unwrap(), session);
        assert_eq!(runtime.run_count.load(Ordering::Acquire), 1);

        // The runtime remains blocked as if it were draining a held click.
        // This call must only signal it, never join it.
        assert!(controller.cancel());
        assert!(returned_rx.try_recv().is_err());
        assert_eq!(controller.begin().unwrap(), session);

        release_tx.send(()).unwrap();
        returned_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        controller.poll();
        controller.poll();
    }

    struct TerminalBarrierRuntime {
        started: Sender<CaptureSessionId>,
        release: std::sync::Mutex<Receiver<()>>,
    }

    impl CaptureRuntime for TerminalBarrierRuntime {
        fn run_session(
            &self,
            session_id: CaptureSessionId,
            _control: CaptureControl,
            updates: CaptureUpdatePublisher,
        ) -> CaptureOutcome {
            let point = PhysicalPoint::new(session_id.get() as i32, 17);
            let outcome = CaptureOutcome::Captured(sample(point));
            // Inject a terminal notification while the fake worker is held at
            // a barrier. The controller must stage it until the worker exits.
            let _ = updates.0.completed(session_id, outcome.clone());
            let _ = self.started.send(session_id);
            let _ = self.release.lock().unwrap().recv();
            outcome
        }
    }

    struct ReleaseWorkersOnDrop(Sender<()>);

    impl Drop for ReleaseWorkersOnDrop {
        fn drop(&mut self) {
            for _ in 0..2 {
                let _ = self.0.send(());
            }
        }
    }

    #[test]
    fn controller_requires_terminal_result_consumption_before_restart() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let _release_workers_on_drop = ReleaseWorkersOnDrop(release_tx.clone());
        let runtime = Arc::new(TerminalBarrierRuntime {
            started: started_tx,
            release: std::sync::Mutex::new(release_rx),
        });
        let mut controller = CoordinateCaptureController::new(runtime);

        let first = controller.begin().unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            first
        );
        assert!(controller.poll());
        assert_eq!(controller.status().unwrap().session_id, first);
        assert_eq!(controller.status().unwrap().phase, CapturePhase::Arming);
        assert!(controller.take_completed().is_none());

        release_tx.send(()).unwrap();
        while controller
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.join.is_finished())
        {
            thread::yield_now();
        }
        assert!(controller.poll());
        assert_eq!(controller.status().unwrap().phase, CapturePhase::Completed);
        assert!(controller.begin().is_err());
        let first_result = controller.take_completed().unwrap();
        assert_eq!(first_result.session_id, first);
        assert_eq!(
            first_result.outcome,
            Some(CaptureOutcome::Captured(sample(PhysicalPoint::new(
                first.get() as i32,
                17,
            ))))
        );

        // The next session cannot replace status until the adapter has
        // published the prior outcome.
        let second = controller.begin().unwrap();
        assert_ne!(first, second);
        assert_eq!(controller.status().unwrap().session_id, second);
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            second
        );

        release_tx.send(()).unwrap();
        while controller
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.join.is_finished())
        {
            thread::yield_now();
        }
        assert!(controller.poll());
        assert_eq!(controller.take_completed().unwrap().session_id, second);
    }

    #[test]
    fn stale_session_updates_cannot_replace_a_newer_status() {
        let (sender, receiver) = mpsc::channel();
        let (dummy_tx, _dummy_rx) = mpsc::channel();
        let runtime = Arc::new(BlockingRuntime {
            started: dummy_tx,
            release: std::sync::Mutex::new(receiver),
            returned: sender,
            run_count: AtomicUsize::new(0),
        });
        let mut controller = CoordinateCaptureController::new(runtime);
        let old = CaptureSessionId(2);
        let current = CaptureSessionId(3);
        controller.status = Some(CaptureStatus {
            session_id: current,
            phase: CapturePhase::WaitingForFreshClick,
            outcome: None,
        });
        assert!(!controller.stage_completion(
            old,
            CaptureOutcome::Captured(sample(PhysicalPoint::new(1, 2))),
        ));
        assert_eq!(controller.status().unwrap().session_id, current);
        assert_eq!(
            controller.status().unwrap().phase,
            CapturePhase::WaitingForFreshClick
        );
    }
}
