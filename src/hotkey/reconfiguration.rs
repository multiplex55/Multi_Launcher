//! Typed library-to-main requests for preparing launcher hotkey changes.
//!
//! Preparation validates a proposed route and proves the legacy listener can
//! be started. It does not publish the route or change persisted settings.

use crate::settings::Settings;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyReconfigurationId(u64);

impl HotkeyReconfigurationId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyRuntimeGeneration(pub u64);

/// Explicit result of the preparation phase. This does not mean the proposed
/// binding is active; activation belongs to the acknowledged apply phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedHotkeyReconfiguration {
    pub request_id: HotkeyReconfigurationId,
    pub generation: HotkeyRuntimeGeneration,
    pub shared_invocation: bool,
}

/// Terminal confirmation that the prepared route became active. A prepared
/// response by itself never represents a successful settings save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedHotkeyReconfiguration {
    pub request_id: HotkeyReconfigurationId,
    pub generation: HotkeyRuntimeGeneration,
}

/// Main-owner disposition of an accepted Apply or Cancel command.
/// Transport timeout/disconnection remains outside this enum and can never be
/// mistaken for proof that the old route was restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyTerminalDisposition {
    Applied(AppliedHotkeyReconfiguration),
    ConfirmedNotAppliedOrRestored(HotkeyReconfigurationError),
    RestorationUnavailable(HotkeyReconfigurationError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyReconfigurationError {
    RuntimeUnavailable,
    RuntimeAlreadyInstalled,
    Disconnected,
    IdentityExhausted,
    Validation(String),
    ListenerPreparation(String),
    PreparationAbandoned,
    TransactionCancelled,
    StaleGeneration,
    RuntimeTransition(String),
    Persistence(String),
    Rollback(String),
    TimedOut,
    InvalidPhase,
}

/// Why an Apply command could not be submitted. `NotEnqueued` proves this
/// ticket's candidate never entered the runtime queue; `AlreadyEnqueued`
/// means an earlier call may still be applied and therefore needs a terminal
/// owner receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyApplySubmissionError {
    NotEnqueued(HotkeyReconfigurationError),
    AlreadyEnqueued(HotkeyReconfigurationError),
}

impl std::fmt::Display for HotkeyApplySubmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnqueued(error) => write!(formatter, "Apply was not enqueued: {error}"),
            Self::AlreadyEnqueued(error) => {
                write!(formatter, "Apply may already be enqueued: {error}")
            }
        }
    }
}

impl std::fmt::Display for HotkeyReconfigurationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RuntimeUnavailable => formatter.write_str("hotkey runtime is unavailable"),
            Self::RuntimeAlreadyInstalled => {
                formatter.write_str("hotkey runtime receiver is already installed")
            }
            Self::Disconnected => {
                formatter.write_str("hotkey runtime request channel disconnected")
            }
            Self::IdentityExhausted => {
                formatter.write_str("hotkey request identity space exhausted")
            }
            Self::Validation(message) => write!(formatter, "hotkey route is invalid: {message}"),
            Self::ListenerPreparation(message) => {
                write!(
                    formatter,
                    "hotkey listener could not be prepared: {message}"
                )
            }
            Self::PreparationAbandoned => {
                formatter.write_str("hotkey preparation request was abandoned")
            }
            Self::TransactionCancelled => formatter
                .write_str("hotkey reconfiguration was cancelled and the prior route restored"),
            Self::StaleGeneration => {
                formatter.write_str("hotkey runtime changed while the save was pending")
            }
            Self::RuntimeTransition(message) => {
                write!(formatter, "hotkey runtime transition failed: {message}")
            }
            Self::Persistence(message) => {
                write!(formatter, "settings persistence failed: {message}")
            }
            Self::Rollback(message) => {
                write!(
                    formatter,
                    "settings rollback could not be completed: {message}"
                )
            }
            Self::TimedOut => formatter.write_str("hotkey runtime did not finish in time"),
            Self::InvalidPhase => formatter.write_str("hotkey request is in an invalid phase"),
        }
    }
}

impl std::error::Error for HotkeyReconfigurationError {}

enum RuntimeCommand {
    Prepare {
        request_id: HotkeyReconfigurationId,
        settings: Box<Settings>,
        sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
        completion: Sender<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>>,
    },
    Apply {
        request_id: HotkeyReconfigurationId,
        generation: HotkeyRuntimeGeneration,
        settings: Box<Settings>,
        sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
        completion: Sender<HotkeyTerminalDisposition>,
    },
    Cancel {
        request_id: HotkeyReconfigurationId,
        completion: Sender<HotkeyTerminalDisposition>,
    },
}

struct ClientInner {
    commands: Sender<RuntimeCommand>,
    wake: Sender<()>,
    owner_alive: Arc<AtomicBool>,
    next_request_id: AtomicU64,
    installation_id: u64,
}

#[derive(Clone)]
pub struct HotkeyReconfigurationClient {
    inner: Arc<ClientInner>,
}

pub struct HotkeyReconfigurationTicket {
    request_id: HotkeyReconfigurationId,
    completion: Receiver<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>>,
    terminal: Receiver<HotkeyTerminalDisposition>,
    terminal_sender: Sender<HotkeyTerminalDisposition>,
    client: HotkeyReconfigurationClient,
    sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
    prepared_generation: Option<HotkeyRuntimeGeneration>,
    apply_sent: bool,
    completed: bool,
    cancelled: bool,
    terminal_completed: bool,
}

impl HotkeyReconfigurationTicket {
    pub fn request_id(&self) -> HotkeyReconfigurationId {
        self.request_id
    }

    /// Polls once for the preparation result. A disconnected completion path
    /// is reported explicitly instead of being treated as success.
    pub fn try_complete(
        &mut self,
    ) -> Option<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>> {
        if self.completed || self.cancelled {
            return None;
        }
        let result = match self.completion.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(HotkeyReconfigurationError::Disconnected)),
        };
        if result.is_some() {
            self.completed = true;
            if let Some(Ok(prepared)) = &result {
                self.prepared_generation = Some(prepared.generation);
            } else {
                self.terminal_completed = true;
            }
        }
        result
    }

    /// Waits off the UI thread for the main owner's preparation response.
    pub fn wait_prepared(
        &mut self,
        timeout: Duration,
    ) -> Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError> {
        if self.completed || self.cancelled {
            return Err(HotkeyReconfigurationError::InvalidPhase);
        }
        let result = match self.completion.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(HotkeyReconfigurationError::TimedOut);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(HotkeyReconfigurationError::Disconnected);
            }
        };
        self.completed = true;
        match result {
            Ok(prepared) => {
                self.prepared_generation = Some(prepared.generation);
                Ok(prepared)
            }
            Err(error) => {
                self.terminal_completed = true;
                Err(error)
            }
        }
    }

    /// Sends the persisted candidate for activation after preparation has
    /// succeeded. The final owner disposition is polled separately.
    pub fn apply(&mut self, settings: Settings) -> Result<(), HotkeyApplySubmissionError> {
        if self.apply_sent {
            return Err(HotkeyApplySubmissionError::AlreadyEnqueued(
                HotkeyReconfigurationError::InvalidPhase,
            ));
        }
        let Some(generation) = self.prepared_generation else {
            return Err(HotkeyApplySubmissionError::NotEnqueued(
                HotkeyReconfigurationError::InvalidPhase,
            ));
        };
        if self.cancelled || self.terminal_completed {
            return Err(HotkeyApplySubmissionError::NotEnqueued(
                HotkeyReconfigurationError::InvalidPhase,
            ));
        }
        self.client
            .apply(
                self.request_id,
                generation,
                settings,
                self.sequence_hotkey_bindings.clone(),
                self.terminal_sender.clone(),
            )
            .map_err(HotkeyApplySubmissionError::NotEnqueued)?;
        self.apply_sent = true;
        Ok(())
    }

    /// Polls for an explicit main-owner disposition. The outer error denotes
    /// transport loss; only `ConfirmedNotAppliedOrRestored` authorizes a
    /// caller to roll back its persisted candidate.
    pub fn try_terminal_disposition(
        &mut self,
    ) -> Option<Result<HotkeyTerminalDisposition, HotkeyReconfigurationError>> {
        if self.terminal_completed {
            return None;
        }
        let result = match self.terminal.try_recv() {
            Ok(result) => Some(Ok(result)),
            Err(TryRecvError::Empty) if !self.client.inner.owner_alive.load(Ordering::Acquire) => {
                match self.terminal.try_recv() {
                    Ok(result) => Some(Ok(result)),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
                        Some(Err(HotkeyReconfigurationError::Disconnected))
                    }
                }
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(HotkeyReconfigurationError::Disconnected)),
        };
        if result.is_some() {
            self.terminal_completed = true;
        }
        result
    }

    /// Waits for the main owner's terminal disposition. A timeout or owner
    /// disconnection is returned as the outer error and is never a restore
    /// acknowledgement.
    pub fn wait_terminal_disposition(
        &mut self,
        timeout: Duration,
    ) -> Result<HotkeyTerminalDisposition, HotkeyReconfigurationError> {
        if self.terminal_completed {
            return Err(HotkeyReconfigurationError::InvalidPhase);
        }
        let started = Instant::now();
        loop {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                if let Some(result) = self.try_terminal_disposition() {
                    return result;
                }
                return Err(HotkeyReconfigurationError::TimedOut);
            }
            // std::sync::mpsc cannot wait on both the receipt and owner-liveness
            // flag. Check liveness at a bounded interval so shutdown does not
            // make a worker wait out the full transaction timeout.
            match self
                .terminal
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                Ok(result) => {
                    self.terminal_completed = true;
                    return Ok(result);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(result) = self.try_terminal_disposition() {
                        return result;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.terminal_completed = true;
                    return Err(HotkeyReconfigurationError::Disconnected);
                }
            }
        }
    }

    /// Cancels a staged preparation by its identity. The main runtime removes
    /// any dormant listener already prepared for this request.
    pub fn cancel(&mut self) -> Result<(), HotkeyReconfigurationError> {
        if !self.cancelled {
            self.client
                .cancel(self.request_id, self.terminal_sender.clone())?;
            self.cancelled = true;
        }
        Ok(())
    }
}

impl Drop for HotkeyReconfigurationTicket {
    fn drop(&mut self) {
        if !self.cancelled && !self.terminal_completed {
            let _ = self
                .client
                .cancel(self.request_id, self.terminal_sender.clone());
            self.cancelled = true;
        }
    }
}

impl HotkeyReconfigurationClient {
    fn new(
        commands: Sender<RuntimeCommand>,
        wake: Sender<()>,
        owner_alive: Arc<AtomicBool>,
        installation_id: u64,
    ) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                commands,
                wake,
                owner_alive,
                next_request_id: AtomicU64::new(1),
                installation_id,
            }),
        }
    }

    pub fn prepare(
        &self,
        settings: Settings,
    ) -> Result<HotkeyReconfigurationTicket, HotkeyReconfigurationError> {
        self.prepare_with_sequence_hotkey_bindings(settings, None)
    }

    pub fn prepare_with_sequence_hotkey_bindings(
        &self,
        settings: Settings,
        sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
    ) -> Result<HotkeyReconfigurationTicket, HotkeyReconfigurationError> {
        let request_id = self.next_request_id()?;
        let (completion_tx, completion_rx) = mpsc::channel();
        let (terminal_tx, terminal_rx) = mpsc::channel();
        self.inner
            .commands
            .send(RuntimeCommand::Prepare {
                request_id,
                settings: Box::new(settings),
                sequence_hotkey_bindings: sequence_hotkey_bindings.clone(),
                completion: completion_tx,
            })
            .map_err(|_| HotkeyReconfigurationError::Disconnected)?;

        let mut ticket = HotkeyReconfigurationTicket {
            request_id,
            completion: completion_rx,
            terminal: terminal_rx,
            terminal_sender: terminal_tx,
            client: self.clone(),
            sequence_hotkey_bindings,
            prepared_generation: None,
            apply_sent: false,
            completed: false,
            cancelled: false,
            terminal_completed: false,
        };
        if self.inner.wake.send(()).is_err() {
            let _ = ticket.cancel();
            return Err(HotkeyReconfigurationError::Disconnected);
        }
        Ok(ticket)
    }

    fn next_request_id(&self) -> Result<HotkeyReconfigurationId, HotkeyReconfigurationError> {
        self.inner
            .next_request_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(HotkeyReconfigurationId)
            .map_err(|_| HotkeyReconfigurationError::IdentityExhausted)
    }

    fn apply(
        &self,
        request_id: HotkeyReconfigurationId,
        generation: HotkeyRuntimeGeneration,
        settings: Settings,
        sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
        completion: Sender<HotkeyTerminalDisposition>,
    ) -> Result<(), HotkeyReconfigurationError> {
        if self
            .inner
            .commands
            .send(RuntimeCommand::Apply {
                request_id,
                generation,
                settings: Box::new(settings),
                sequence_hotkey_bindings,
                completion,
            })
            .is_err()
        {
            self.inner.owner_alive.store(false, Ordering::Release);
            return Err(HotkeyReconfigurationError::Disconnected);
        }
        // After enqueueing, the command may still be applied. A failed wake
        // must therefore resolve through the owner's terminal response.
        let _ = self.inner.wake.send(());
        Ok(())
    }

    fn cancel(
        &self,
        request_id: HotkeyReconfigurationId,
        completion: Sender<HotkeyTerminalDisposition>,
    ) -> Result<(), HotkeyReconfigurationError> {
        if self
            .inner
            .commands
            .send(RuntimeCommand::Cancel {
                request_id,
                completion,
            })
            .is_err()
        {
            self.inner.owner_alive.store(false, Ordering::Release);
            return Err(HotkeyReconfigurationError::Disconnected);
        }
        // Once queued, cancellation is resolved by the owner just like Apply.
        let _ = self.inner.wake.send(());
        Ok(())
    }
}

pub enum HotkeyRuntimeMessage {
    Prepare(HotkeyPreparationRequest),
    Apply(HotkeyApplicationRequest),
    Cancel(HotkeyCancellationRequest),
}

pub struct HotkeyPreparationRequest {
    request_id: HotkeyReconfigurationId,
    settings: Box<Settings>,
    sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
    completion: Option<Sender<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>>>,
}

pub struct HotkeyApplicationRequest {
    request_id: HotkeyReconfigurationId,
    generation: HotkeyRuntimeGeneration,
    settings: Box<Settings>,
    sequence_hotkey_bindings: Option<Vec<crate::paste_sequence::SequenceHotkeyBinding>>,
    completion: Option<Sender<HotkeyTerminalDisposition>>,
}

impl HotkeyApplicationRequest {
    pub fn request_id(&self) -> HotkeyReconfigurationId {
        self.request_id
    }

    pub fn generation(&self) -> HotkeyRuntimeGeneration {
        self.generation
    }

    pub fn settings(&self) -> &Settings {
        self.settings.as_ref()
    }

    pub fn sequence_hotkey_bindings(
        &self,
    ) -> Option<&[crate::paste_sequence::SequenceHotkeyBinding]> {
        self.sequence_hotkey_bindings.as_deref()
    }

    pub fn complete_applied(
        mut self,
        applied: AppliedHotkeyReconfiguration,
    ) -> Result<(), HotkeyReconfigurationError> {
        self.completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?
            .send(HotkeyTerminalDisposition::Applied(applied))
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }

    pub fn complete_restored(
        mut self,
        error: HotkeyReconfigurationError,
    ) -> Result<(), HotkeyReconfigurationError> {
        self.completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?
            .send(HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(
                error,
            ))
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }

    pub fn complete_restoration_unavailable(
        mut self,
        error: HotkeyReconfigurationError,
    ) -> Result<(), HotkeyReconfigurationError> {
        self.completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?
            .send(HotkeyTerminalDisposition::RestorationUnavailable(error))
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }
}

impl Drop for HotkeyApplicationRequest {
    fn drop(&mut self) {
        if let Some(completion) = self.completion.take() {
            let _ = completion.send(HotkeyTerminalDisposition::RestorationUnavailable(
                HotkeyReconfigurationError::RuntimeTransition(
                    "apply request was abandoned before a terminal result".into(),
                ),
            ));
        }
    }
}

pub struct HotkeyCancellationRequest {
    request_id: HotkeyReconfigurationId,
    completion: Option<Sender<HotkeyTerminalDisposition>>,
}

impl HotkeyCancellationRequest {
    pub fn request_id(&self) -> HotkeyReconfigurationId {
        self.request_id
    }

    pub fn complete_restored(
        mut self,
        error: HotkeyReconfigurationError,
    ) -> Result<(), HotkeyReconfigurationError> {
        self.completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?
            .send(HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(
                error,
            ))
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }

    pub fn complete_restoration_unavailable(
        mut self,
        error: HotkeyReconfigurationError,
    ) -> Result<(), HotkeyReconfigurationError> {
        self.completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?
            .send(HotkeyTerminalDisposition::RestorationUnavailable(error))
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }

    /// Releases this command's terminal sender when an accepted Apply is
    /// already authoritative. The application request will publish the single
    /// terminal result after it resolves the route transition.
    pub fn defer_terminal(&mut self) {
        self.completion.take();
    }
}

impl Drop for HotkeyCancellationRequest {
    fn drop(&mut self) {
        if let Some(completion) = self.completion.take() {
            let _ = completion.send(HotkeyTerminalDisposition::RestorationUnavailable(
                HotkeyReconfigurationError::RuntimeTransition(
                    "cancel request was abandoned before the prior route was confirmed".into(),
                ),
            ));
        }
    }
}

impl HotkeyPreparationRequest {
    pub fn request_id(&self) -> HotkeyReconfigurationId {
        self.request_id
    }

    pub fn settings(&self) -> &Settings {
        self.settings.as_ref()
    }

    pub fn sequence_hotkey_bindings(
        &self,
    ) -> Option<&[crate::paste_sequence::SequenceHotkeyBinding]> {
        self.sequence_hotkey_bindings.as_deref()
    }

    pub fn complete(
        mut self,
        result: Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>,
    ) -> Result<(), HotkeyReconfigurationError> {
        let completion = self
            .completion
            .take()
            .ok_or(HotkeyReconfigurationError::Disconnected)?;
        completion
            .send(result)
            .map_err(|_| HotkeyReconfigurationError::Disconnected)
    }
}

impl Drop for HotkeyPreparationRequest {
    fn drop(&mut self) {
        if let Some(completion) = self.completion.take() {
            let _ = completion.send(Err(HotkeyReconfigurationError::PreparationAbandoned));
        }
    }
}

pub struct HotkeyReconfigurationReceiver {
    commands: Receiver<RuntimeCommand>,
    owner_alive: Arc<AtomicBool>,
    installation_id: u64,
}

impl HotkeyReconfigurationReceiver {
    pub fn try_recv(&self) -> Result<Option<HotkeyRuntimeMessage>, HotkeyReconfigurationError> {
        match self.commands.try_recv() {
            Ok(RuntimeCommand::Prepare {
                request_id,
                settings,
                sequence_hotkey_bindings,
                completion,
            }) => Ok(Some(HotkeyRuntimeMessage::Prepare(
                HotkeyPreparationRequest {
                    request_id,
                    settings,
                    sequence_hotkey_bindings,
                    completion: Some(completion),
                },
            ))),
            Ok(RuntimeCommand::Apply {
                request_id,
                generation,
                settings,
                sequence_hotkey_bindings,
                completion,
            }) => Ok(Some(HotkeyRuntimeMessage::Apply(
                HotkeyApplicationRequest {
                    request_id,
                    generation,
                    settings,
                    sequence_hotkey_bindings,
                    completion: Some(completion),
                },
            ))),
            Ok(RuntimeCommand::Cancel {
                request_id,
                completion,
            }) => Ok(Some(HotkeyRuntimeMessage::Cancel(
                HotkeyCancellationRequest {
                    request_id,
                    completion: Some(completion),
                },
            ))),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(HotkeyReconfigurationError::Disconnected),
        }
    }
}

impl Drop for HotkeyReconfigurationReceiver {
    fn drop(&mut self) {
        self.owner_alive.store(false, Ordering::Release);
        if let Ok(mut slot) = client_slot().write()
            && slot
                .as_ref()
                .is_some_and(|client| client.inner.installation_id == self.installation_id)
        {
            *slot = None;
        }
    }
}

static CLIENT: OnceLock<RwLock<Option<HotkeyReconfigurationClient>>> = OnceLock::new();
static NEXT_INSTALLATION_ID: AtomicU64 = AtomicU64::new(1);

fn client_slot() -> &'static RwLock<Option<HotkeyReconfigurationClient>> {
    CLIENT.get_or_init(|| RwLock::new(None))
}

/// Installs the single main-owned receiver and publishes its library client.
/// The receiver must be polled by the main event loop for each wake.
pub fn install_main_runtime(
    wake: Sender<()>,
) -> Result<HotkeyReconfigurationReceiver, HotkeyReconfigurationError> {
    let installation_id = NEXT_INSTALLATION_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| HotkeyReconfigurationError::IdentityExhausted)?;
    let (command_tx, command_rx) = mpsc::channel();
    let owner_alive = Arc::new(AtomicBool::new(true));
    let client = HotkeyReconfigurationClient::new(
        command_tx,
        wake,
        Arc::clone(&owner_alive),
        installation_id,
    );
    let mut slot = client_slot()
        .write()
        .map_err(|_| HotkeyReconfigurationError::RuntimeUnavailable)?;
    if slot.is_some() {
        return Err(HotkeyReconfigurationError::RuntimeAlreadyInstalled);
    }
    *slot = Some(client);
    Ok(HotkeyReconfigurationReceiver {
        commands: command_rx,
        owner_alive,
        installation_id,
    })
}

/// Requests preparation from the currently installed main runtime. It fails
/// immediately when the runtime is absent or its command channel is closed.
pub fn prepare_hotkey_reconfiguration(
    settings: Settings,
) -> Result<HotkeyReconfigurationTicket, HotkeyReconfigurationError> {
    let client = client_slot()
        .read()
        .map_err(|_| HotkeyReconfigurationError::RuntimeUnavailable)?
        .clone()
        .ok_or(HotkeyReconfigurationError::RuntimeUnavailable)?;
    client.prepare(settings)
}

/// Requests preparation using a prospective, payload-free set of saved
/// sequence hotkeys. The caller persists that candidate only after the main
/// runtime acknowledges this preparation.
pub fn prepare_hotkey_reconfiguration_with_sequence_bindings(
    settings: Settings,
    sequence_hotkey_bindings: Vec<crate::paste_sequence::SequenceHotkeyBinding>,
) -> Result<HotkeyReconfigurationTicket, HotkeyReconfigurationError> {
    let client = client_slot()
        .read()
        .map_err(|_| HotkeyReconfigurationError::RuntimeUnavailable)?
        .clone()
        .ok_or(HotkeyReconfigurationError::RuntimeUnavailable)?;
    client.prepare_with_sequence_hotkey_bindings(settings, Some(sequence_hotkey_bindings))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn test_runtime() -> (
        HotkeyReconfigurationClient,
        HotkeyReconfigurationReceiver,
        Receiver<()>,
    ) {
        let (wake_tx, wake_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        let owner_alive = Arc::new(AtomicBool::new(true));
        (
            HotkeyReconfigurationClient::new(command_tx, wake_tx, Arc::clone(&owner_alive), 0),
            HotkeyReconfigurationReceiver {
                commands: command_rx,
                owner_alive,
                installation_id: 0,
            },
            wake_rx,
        )
    }

    #[test]
    fn save_request_requires_preparation_then_returns_a_typed_applied_ack() {
        let (client, receiver, wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let request_id = request.request_id();
        assert_eq!(request_id, ticket.request_id());
        let prepared = PreparedHotkeyReconfiguration {
            request_id,
            generation: HotkeyRuntimeGeneration(9),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        assert_eq!(ticket.try_complete(), Some(Ok(prepared)));

        ticket.apply(Settings::default()).unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };
        assert_eq!(request.request_id(), ticket.request_id());
        assert_eq!(request.generation(), prepared.generation);
        assert_eq!(request.settings().hotkey, Settings::default().hotkey);
        let applied = AppliedHotkeyReconfiguration {
            request_id: ticket.request_id(),
            generation: prepared.generation,
        };
        request.complete_applied(applied).unwrap();
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(HotkeyTerminalDisposition::Applied(applied)))
        );
    }

    #[test]
    fn paste_sequence_hotkey_bindings_survive_prepare_and_apply_requests() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::paste_sequence::SequenceStore::new(
            directory
                .path()
                .join(crate::paste_sequence::PASTE_SEQUENCES_FILE),
        );
        let mut snapshot = store.load().unwrap();
        let id = snapshot
            .upsert(crate::paste_sequence::SequenceDraft {
                id: None,
                name: "Prospective".into(),
                hotkey: Some(crate::paste_sequence::HotkeyChord::new("Ctrl+F8").unwrap()),
                mode: crate::paste_sequence::PlaybackMode::Automatic,
                delay_ms: 0,
                steps: vec![crate::paste_sequence::Step::LiteralText {
                    text: "private body".into(),
                }],
            })
            .unwrap();
        let bindings = snapshot.hotkey_bindings();
        let (client, receiver, _wake_rx) = test_runtime();
        let mut ticket = client
            .prepare_with_sequence_hotkey_bindings(Settings::default(), Some(bindings.clone()))
            .unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        assert_eq!(request.sequence_hotkey_bindings().unwrap()[0].id(), id);
        assert!(!format!("{:?}", bindings[0]).contains("private body"));
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(21),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        ticket.wait_prepared(Duration::from_secs(1)).unwrap();
        ticket.apply(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };
        assert_eq!(request.sequence_hotkey_bindings().unwrap()[0].id(), id);
        request
            .complete_applied(AppliedHotkeyReconfiguration {
                request_id: ticket.request_id(),
                generation: prepared.generation,
            })
            .unwrap();
        assert!(matches!(
            ticket.wait_terminal_disposition(Duration::from_secs(1)),
            Ok(HotkeyTerminalDisposition::Applied(_))
        ));
    }

    #[test]
    fn cancellation_has_a_terminal_receipt_and_cannot_be_mistaken_for_applied() {
        let (client, receiver, wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(3),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        assert_eq!(ticket.try_complete(), Some(Ok(prepared)));

        ticket.cancel().unwrap();
        assert_eq!(wake_rx.try_recv(), Ok(()));
        let HotkeyRuntimeMessage::Cancel(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected cancel request")
        };
        assert_eq!(request.request_id(), ticket.request_id());
        request
            .complete_restored(HotkeyReconfigurationError::TransactionCancelled)
            .unwrap();
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(
                HotkeyTerminalDisposition::ConfirmedNotAppliedOrRestored(
                    HotkeyReconfigurationError::TransactionCancelled
                )
            ))
        );
    }

    #[test]
    fn queued_apply_wake_failure_is_resolved_by_the_terminal_receipt() {
        let (client, receiver, wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(11),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        assert_eq!(ticket.try_complete(), Some(Ok(prepared)));

        drop(wake_rx);
        ticket.apply(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("accepted apply must remain queued after its wake receiver closes")
        };
        assert_eq!(ticket.try_terminal_disposition(), None);
        let applied = AppliedHotkeyReconfiguration {
            request_id: ticket.request_id(),
            generation: prepared.generation,
        };
        request.complete_applied(applied).unwrap();
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(HotkeyTerminalDisposition::Applied(applied)))
        );
    }

    #[test]
    fn owner_disconnect_after_queued_apply_or_cancel_is_terminal() {
        for queue_cancel in [false, true] {
            let (client, receiver, _wake_rx) = test_runtime();
            let mut ticket = client.prepare(Settings::default()).unwrap();
            let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap()
            else {
                panic!("expected prepare request")
            };
            let prepared = PreparedHotkeyReconfiguration {
                request_id: request.request_id(),
                generation: HotkeyRuntimeGeneration(12),
                shared_invocation: false,
            };
            request.complete(Ok(prepared)).unwrap();
            assert_eq!(ticket.try_complete(), Some(Ok(prepared)));

            if queue_cancel {
                ticket.cancel().unwrap();
            } else {
                ticket.apply(Settings::default()).unwrap();
            }
            drop(receiver);
            assert_eq!(
                ticket.try_terminal_disposition(),
                Some(Err(HotkeyReconfigurationError::Disconnected)),
                "queued command must not leave a terminal channel pending forever"
            );
        }
    }

    #[test]
    fn waiting_terminal_observes_owner_shutdown_without_waiting_full_timeout() {
        let (client, receiver, _wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(18),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        assert_eq!(ticket.try_complete(), Some(Ok(prepared)));
        ticket.apply(Settings::default()).unwrap();
        drop(receiver);

        let started = Instant::now();
        assert_eq!(
            ticket.wait_terminal_disposition(Duration::from_secs(30)),
            Err(HotkeyReconfigurationError::Disconnected)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn published_applied_result_wins_a_following_owner_drop() {
        let (client, receiver, _wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        let prepared = PreparedHotkeyReconfiguration {
            request_id: request.request_id(),
            generation: HotkeyRuntimeGeneration(13),
            shared_invocation: false,
        };
        request.complete(Ok(prepared)).unwrap();
        assert_eq!(ticket.try_complete(), Some(Ok(prepared)));
        ticket.apply(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Apply(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected apply request")
        };
        let applied = AppliedHotkeyReconfiguration {
            request_id: ticket.request_id(),
            generation: prepared.generation,
        };
        request.complete_applied(applied).unwrap();
        drop(receiver);
        assert_eq!(
            ticket.try_terminal_disposition(),
            Some(Ok(HotkeyTerminalDisposition::Applied(applied)))
        );
    }

    #[test]
    fn dropping_an_unhandled_request_is_an_explicit_error() {
        let (client, receiver, _wake_rx) = test_runtime();
        let mut ticket = client.prepare(Settings::default()).unwrap();
        let HotkeyRuntimeMessage::Prepare(request) = receiver.try_recv().unwrap().unwrap() else {
            panic!("expected prepare request")
        };
        drop(request);
        assert_eq!(
            ticket.try_complete(),
            Some(Err(HotkeyReconfigurationError::PreparationAbandoned))
        );
    }

    #[test]
    fn unavailable_library_owner_is_reported_without_accepting_a_request() {
        assert_eq!(
            prepare_hotkey_reconfiguration(Settings::default()).err(),
            Some(HotkeyReconfigurationError::RuntimeUnavailable)
        );
    }

    #[test]
    fn disconnected_receiver_is_not_reported_as_prepared() {
        let (client, receiver, _wake_rx) = test_runtime();
        drop(receiver);
        assert_eq!(
            client.prepare(Settings::default()).err(),
            Some(HotkeyReconfigurationError::Disconnected)
        );
    }
}

#[cfg(test)]
pub(crate) fn test_reconfiguration_runtime() -> (
    HotkeyReconfigurationClient,
    HotkeyReconfigurationReceiver,
    Receiver<()>,
) {
    tests::test_runtime()
}
