//! Typed library-to-main requests for preparing launcher hotkey changes.
//!
//! Preparation validates a proposed route and proves the legacy listener can
//! be started. It does not publish the route or change persisted settings.

use crate::settings::Settings;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, OnceLock, RwLock};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyReconfigurationError {
    RuntimeUnavailable,
    RuntimeAlreadyInstalled,
    Disconnected,
    IdentityExhausted,
    Validation(String),
    ListenerPreparation(String),
    PreparationAbandoned,
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
        }
    }
}

impl std::error::Error for HotkeyReconfigurationError {}

enum RuntimeCommand {
    Prepare {
        request_id: HotkeyReconfigurationId,
        settings: Box<Settings>,
        completion: Sender<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>>,
    },
    Cancel {
        request_id: HotkeyReconfigurationId,
    },
}

struct ClientInner {
    commands: Sender<RuntimeCommand>,
    wake: Sender<()>,
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
    client: HotkeyReconfigurationClient,
    completed: bool,
    cancelled: bool,
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
        }
        result
    }

    /// Cancels a staged preparation by its identity. The main runtime removes
    /// any dormant listener already prepared for this request.
    pub fn cancel(&mut self) {
        if !self.cancelled {
            self.client.cancel(self.request_id);
            self.cancelled = true;
        }
    }
}

impl Drop for HotkeyReconfigurationTicket {
    fn drop(&mut self) {
        if !self.cancelled {
            self.client.cancel(self.request_id);
            self.cancelled = true;
        }
    }
}

impl HotkeyReconfigurationClient {
    fn new(commands: Sender<RuntimeCommand>, wake: Sender<()>, installation_id: u64) -> Self {
        Self {
            inner: Arc::new(ClientInner {
                commands,
                wake,
                next_request_id: AtomicU64::new(1),
                installation_id,
            }),
        }
    }

    pub fn prepare(
        &self,
        settings: Settings,
    ) -> Result<HotkeyReconfigurationTicket, HotkeyReconfigurationError> {
        let request_id = self.next_request_id()?;
        let (completion_tx, completion_rx) = mpsc::channel();
        self.inner
            .commands
            .send(RuntimeCommand::Prepare {
                request_id,
                settings: Box::new(settings),
                completion: completion_tx,
            })
            .map_err(|_| HotkeyReconfigurationError::Disconnected)?;

        let ticket = HotkeyReconfigurationTicket {
            request_id,
            completion: completion_rx,
            client: self.clone(),
            completed: false,
            cancelled: false,
        };
        if self.inner.wake.send(()).is_err() {
            self.cancel(request_id);
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

    fn cancel(&self, request_id: HotkeyReconfigurationId) {
        if self
            .inner
            .commands
            .send(RuntimeCommand::Cancel { request_id })
            .is_ok()
        {
            let _ = self.inner.wake.send(());
        }
    }
}

pub enum HotkeyRuntimeMessage {
    Prepare(HotkeyPreparationRequest),
    Cancel(HotkeyReconfigurationId),
}

pub struct HotkeyPreparationRequest {
    request_id: HotkeyReconfigurationId,
    settings: Box<Settings>,
    completion: Option<Sender<Result<PreparedHotkeyReconfiguration, HotkeyReconfigurationError>>>,
}

impl HotkeyPreparationRequest {
    pub fn request_id(&self) -> HotkeyReconfigurationId {
        self.request_id
    }

    pub fn settings(&self) -> &Settings {
        self.settings.as_ref()
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
    installation_id: u64,
}

impl HotkeyReconfigurationReceiver {
    pub fn try_recv(&self) -> Result<Option<HotkeyRuntimeMessage>, HotkeyReconfigurationError> {
        match self.commands.try_recv() {
            Ok(RuntimeCommand::Prepare {
                request_id,
                settings,
                completion,
            }) => Ok(Some(HotkeyRuntimeMessage::Prepare(
                HotkeyPreparationRequest {
                    request_id,
                    settings,
                    completion: Some(completion),
                },
            ))),
            Ok(RuntimeCommand::Cancel { request_id }) => {
                Ok(Some(HotkeyRuntimeMessage::Cancel(request_id)))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(HotkeyReconfigurationError::Disconnected),
        }
    }
}

impl Drop for HotkeyReconfigurationReceiver {
    fn drop(&mut self) {
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
    let client = HotkeyReconfigurationClient::new(command_tx, wake, installation_id);
    let mut slot = client_slot()
        .write()
        .map_err(|_| HotkeyReconfigurationError::RuntimeUnavailable)?;
    if slot.is_some() {
        return Err(HotkeyReconfigurationError::RuntimeAlreadyInstalled);
    }
    *slot = Some(client);
    Ok(HotkeyReconfigurationReceiver {
        commands: command_rx,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_runtime() -> (
        HotkeyReconfigurationClient,
        HotkeyReconfigurationReceiver,
        Receiver<()>,
    ) {
        let (wake_tx, wake_rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        (
            HotkeyReconfigurationClient::new(command_tx, wake_tx, 0),
            HotkeyReconfigurationReceiver {
                commands: command_rx,
                installation_id: 0,
            },
            wake_rx,
        )
    }

    #[test]
    fn preparation_acknowledges_the_typed_request_identity_and_generation() {
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
