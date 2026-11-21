//! Protected clipboard boundary. No value is logged or included in errors.
use crate::password_generator::{SecretResult, SecretResultId};
use std::{fmt, time::Duration};

const MODIFY_READ_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyStatus {
    AcceptedAndPersisted,
    AcceptedButNotPersisted,
}
#[derive(PartialEq, Eq)]
pub enum ClipboardRead {
    Ordinary(String),
    Private,
    NonText,
}
impl fmt::Debug for ClipboardRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ordinary(_) => f.write_str("Ordinary([redacted])"),
            Self::Private => f.write_str("Private"),
            Self::NonText => f.write_str("NonText"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Busy,
    NotAccepted,
    Unsupported,
    Unavailable,
    TimedOut,
}
impl ClipboardError {
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }
}
impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "The clipboard is busy.",
            Self::NotAccepted => "Windows could not set the clipboard.",
            Self::Unsupported => "Protected clipboard copying is unavailable on this platform.",
            Self::Unavailable => "The clipboard could not be accessed.",
            Self::TimedOut => "The clipboard did not respond in time.",
        })
    }
}
impl std::error::Error for ClipboardError {}

/// A rejected headless copy retains its generated result for an explicit retry.
pub struct RejectedSecretCopy {
    error: ClipboardError,
    result: SecretResult,
}

impl RejectedSecretCopy {
    pub(crate) fn new(error: ClipboardError, result: SecretResult) -> Self {
        Self { error, result }
    }

    pub fn result_id(&self) -> SecretResultId {
        self.result.id()
    }

    pub fn retry(self) -> Result<CopyStatus, Self> {
        copy_secret(&self.result).map_err(|error| Self {
            error,
            result: self.result,
        })
    }
}

impl fmt::Debug for RejectedSecretCopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RejectedSecretCopy")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for RejectedSecretCopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for RejectedSecretCopy {}

/// Copy only a generator-owned result. A successful return means the OS
/// accepted the package; persistence failure is a separate accepted status.
pub fn copy_secret(secret: &SecretResult) -> Result<CopyStatus, ClipboardError> {
    platform::copy(secret)
}
/// Read one clipboard snapshot. Call from a worker when possible: cross-process
/// clipboard providers may take time even though the async wait is bounded.
pub fn read_clipboard() -> Result<ClipboardRead, ClipboardError> {
    platform::read(Duration::from_millis(750))
}

/// Read one clipboard snapshot for Clipboard Modify; Windows uses a bounded WinRT worker.
pub fn read_clipboard_for_modify() -> Result<ClipboardRead, ClipboardError> {
    platform::read_for_modify(MODIFY_READ_TIMEOUT)
}

/// Give interactive search and command capture a short wait for delayed text.
pub(crate) fn read_clipboard_briefly() -> Result<ClipboardRead, ClipboardError> {
    platform::read(Duration::from_millis(32))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        marker::PhantomData,
        rc::Rc,
        sync::{
            Arc, LazyLock,
            atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
            mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
        },
        thread,
        time::Instant,
    };
    use windows::{
        ApplicationModel::DataTransfer::{
            Clipboard, ClipboardContentOptions, DataPackage, DataPackageView, StandardDataFormats,
        },
        Foundation::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation},
        Storage::Streams::InMemoryRandomAccessStream,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::WinRT::{
                RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
            },
        },
        core::{HSTRING, IInspectable, Interface},
    };
    const PRIVATE_FORMAT: &str = "application/x-multi-launcher-private-text-v1";
    const MAX_MODIFY_READ_WORKERS: usize = 2;
    const MODIFY_READ_WORKER_NAME: &str = "clipboard-modify-read";
    // COM asks clients to retry when its server is temporarily unable to handle a call.
    const RPC_E_SERVERCALL_RETRYLATER_HRESULT: u32 = 0x8001010a;

    #[derive(Clone, Copy)]
    #[repr(u8)]
    enum ReadStage {
        WorkerAdmission = 0,
        WorkerSpawn,
        MtaInitialize,
        GetContent,
        PrivateMarkerCheck,
        TextFormat,
        TextFormatCheck,
        GetTextAsync,
        AsyncStatus,
        CompletionRegistration,
        CompletionWait,
        Completion,
        ErrorCode,
        GetResults,
        Cancel,
    }

    impl ReadStage {
        fn as_str(self) -> &'static str {
            match self {
                Self::WorkerAdmission => "worker_admission",
                Self::WorkerSpawn => "worker_spawn",
                Self::MtaInitialize => "RoInitialize(MTA)",
                Self::GetContent => "Clipboard.GetContent",
                Self::PrivateMarkerCheck => "DataPackageView.Contains(private-format)",
                Self::TextFormat => "StandardDataFormats.Text",
                Self::TextFormatCheck => "DataPackageView.Contains(text-format)",
                Self::GetTextAsync => "DataPackageView.GetTextAsync",
                Self::AsyncStatus => "IAsyncOperation.Status",
                Self::CompletionRegistration => "IAsyncOperation.SetCompleted",
                Self::CompletionWait => "IAsyncOperation.completion",
                Self::Completion => "IAsyncOperation.completed",
                Self::ErrorCode => "IAsyncOperation.ErrorCode",
                Self::GetResults => "IAsyncOperation.GetResults",
                Self::Cancel => "IAsyncOperation.Cancel",
            }
        }

        fn from_u8(value: u8) -> Self {
            match value {
                0 => Self::WorkerAdmission,
                1 => Self::WorkerSpawn,
                2 => Self::MtaInitialize,
                3 => Self::GetContent,
                4 => Self::PrivateMarkerCheck,
                5 => Self::TextFormat,
                6 => Self::TextFormatCheck,
                7 => Self::GetTextAsync,
                8 => Self::AsyncStatus,
                9 => Self::CompletionRegistration,
                10 => Self::CompletionWait,
                11 => Self::Completion,
                12 => Self::ErrorCode,
                13 => Self::GetResults,
                _ => Self::Cancel,
            }
        }
    }

    const APARTMENT_UNKNOWN: u8 = 0;
    const APARTMENT_MTA: u8 = 1;

    struct ReadDiagnostics {
        started: Instant,
        stage: AtomicU8,
        apartment: AtomicU8,
        failure_event_recorded: AtomicBool,
        timeout_event_recorded: AtomicBool,
    }

    impl ReadDiagnostics {
        fn new(started: Instant) -> Self {
            Self {
                started,
                stage: AtomicU8::new(ReadStage::WorkerAdmission as u8),
                apartment: AtomicU8::new(APARTMENT_UNKNOWN),
                failure_event_recorded: AtomicBool::new(false),
                timeout_event_recorded: AtomicBool::new(false),
            }
        }

        fn enter(&self, stage: ReadStage) {
            self.stage.store(stage as u8, Ordering::Release);
        }

        fn current_stage(&self) -> ReadStage {
            ReadStage::from_u8(self.stage.load(Ordering::Acquire))
        }

        fn mark_mta(&self) {
            self.apartment.store(APARTMENT_MTA, Ordering::Release);
        }

        fn apartment(&self) -> &'static str {
            match self.apartment.load(Ordering::Acquire) {
                APARTMENT_MTA => "MTA",
                _ => "unknown",
            }
        }

        fn elapsed_ms(&self) -> u64 {
            self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
        }

        fn timeout(&self) {
            if !self.timeout_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(
                    target: "multi_launcher::clipboard",
                    outcome = "timeout",
                    stage = self.current_stage().as_str(),
                    elapsed_ms = self.elapsed_ms(),
                    apartment = self.apartment(),
                    "WinRT clipboard Modify read timed out"
                );
            }
        }

        fn hresult(&self, stage: ReadStage, hresult: u32) {
            self.enter(stage);
            if !self.failure_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(
                    target: "multi_launcher::clipboard",
                    outcome = "hresult",
                    stage = stage.as_str(),
                    elapsed_ms = self.elapsed_ms(),
                    apartment = self.apartment(),
                    hresult,
                    "WinRT clipboard Modify read failed"
                );
            }
        }

        fn issue(&self, outcome: &'static str, stage: ReadStage) {
            self.enter(stage);
            if !self.failure_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(
                    target: "multi_launcher::clipboard",
                    outcome,
                    stage = stage.as_str(),
                    elapsed_ms = self.elapsed_ms(),
                    apartment = self.apartment(),
                    "WinRT clipboard Modify read failed"
                );
            }
        }
    }

    struct ReadWorkerBudget {
        active: AtomicUsize,
        limit: usize,
    }

    impl ReadWorkerBudget {
        fn new(limit: usize) -> Self {
            Self {
                active: AtomicUsize::new(0),
                limit,
            }
        }

        fn try_reserve(self: &Arc<Self>) -> Option<ReadWorkerPermit> {
            let mut active = self.active.load(Ordering::Acquire);
            loop {
                if active >= self.limit {
                    return None;
                }
                match self.active.compare_exchange_weak(
                    active,
                    active + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        return Some(ReadWorkerPermit {
                            budget: Arc::clone(self),
                        });
                    }
                    Err(current) => active = current,
                }
            }
        }

        #[cfg(test)]
        fn active(&self) -> usize {
            self.active.load(Ordering::Acquire)
        }
    }

    struct ReadWorkerPermit {
        budget: Arc<ReadWorkerBudget>,
    }

    impl Drop for ReadWorkerPermit {
        fn drop(&mut self) {
            self.budget.active.fetch_sub(1, Ordering::AcqRel);
        }
    }

    static MODIFY_READ_WORKERS: LazyLock<Arc<ReadWorkerBudget>> =
        LazyLock::new(|| Arc::new(ReadWorkerBudget::new(MAX_MODIFY_READ_WORKERS)));

    fn run_bounded_worker<T: Send + 'static>(
        budget: Arc<ReadWorkerBudget>,
        timeout: Duration,
        worker: impl FnOnce(Arc<ReadDiagnostics>, Arc<AtomicBool>, Instant) -> Result<T, ClipboardError>
        + Send
        + 'static,
    ) -> Result<T, ClipboardError> {
        let started = Instant::now();
        let deadline = started + timeout;
        let diagnostics = Arc::new(ReadDiagnostics::new(started));
        diagnostics.enter(ReadStage::WorkerAdmission);
        let Some(permit) = budget.try_reserve() else {
            diagnostics.issue("worker_saturated", ReadStage::WorkerAdmission);
            return Err(ClipboardError::Busy);
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_diagnostics = Arc::clone(&diagnostics);
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = thread::Builder::new()
            .name(MODIFY_READ_WORKER_NAME.into())
            .spawn(move || {
                let _permit = permit;
                let result = worker(
                    Arc::clone(&worker_diagnostics),
                    Arc::clone(&worker_cancelled),
                    deadline,
                );
                let result =
                    if worker_cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                        worker_diagnostics.timeout();
                        Err(ClipboardError::TimedOut)
                    } else {
                        match result {
                            Err(ClipboardError::TimedOut) => {
                                worker_diagnostics.timeout();
                                Err(ClipboardError::TimedOut)
                            }
                            result => result,
                        }
                    };
                let _ = sender.try_send(result);
            });
        match spawned {
            // Detach rather than joining: a stalled WinRT call must not extend the caller wait.
            Ok(handle) => drop(handle),
            Err(_) => {
                diagnostics.issue("worker_spawn_failed", ReadStage::WorkerSpawn);
                return Err(ClipboardError::Busy);
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                cancelled.store(true, Ordering::Release);
                diagnostics.timeout();
                Err(ClipboardError::TimedOut)
            }
            Err(RecvTimeoutError::Disconnected) => {
                diagnostics.issue("worker_exited_without_result", diagnostics.current_stage());
                Err(ClipboardError::Unavailable)
            }
        }
    }

    fn map_hresult(hresult: u32) -> ClipboardError {
        match hresult {
            0x800401d0 | 0x80010001 | RPC_E_SERVERCALL_RETRYLATER_HRESULT | 0x800700aa => {
                ClipboardError::Busy
            }
            _ => ClipboardError::Unavailable,
        }
    }
    fn map_error(error: windows::core::Error) -> ClipboardError {
        map_hresult(error.code().0 as u32)
    }
    fn map_read_hresult(stage: &'static str, hresult: u32) -> ClipboardError {
        tracing::warn!(
            target: "multi_launcher::clipboard",
            operation = stage,
            hresult = %format!("{hresult:#010x}"),
            "Windows Runtime clipboard read failed"
        );
        map_hresult(hresult)
    }
    fn map_read_error(stage: &'static str, error: windows::core::Error) -> ClipboardError {
        map_read_hresult(stage, error.code().0 as u32)
    }
    struct ModifyReadContext<'a> {
        diagnostics: &'a ReadDiagnostics,
        cancelled: &'a AtomicBool,
        deadline: Instant,
    }

    impl ModifyReadContext<'_> {
        fn enter(&self, stage: ReadStage) -> Result<(), ClipboardError> {
            self.diagnostics.enter(stage);
            self.check()
        }

        fn is_stopped(&self) -> bool {
            self.cancelled.load(Ordering::Acquire) || Instant::now() >= self.deadline
        }

        fn check(&self) -> Result<(), ClipboardError> {
            if self.is_stopped() {
                self.diagnostics.timeout();
                Err(ClipboardError::TimedOut)
            } else {
                Ok(())
            }
        }

        fn remaining(&self) -> Duration {
            self.deadline.saturating_duration_since(Instant::now())
        }

        fn call<T>(
            &self,
            stage: ReadStage,
            call: impl FnOnce() -> windows::core::Result<T>,
        ) -> Result<T, ClipboardError> {
            self.enter(stage)?;
            match call() {
                Ok(value) => {
                    self.check()?;
                    Ok(value)
                }
                Err(error) => Err(self.map_hresult(stage, error.code().0 as u32)),
            }
        }

        fn map_hresult(&self, stage: ReadStage, hresult: u32) -> ClipboardError {
            if self.is_stopped() {
                self.diagnostics.timeout();
                ClipboardError::TimedOut
            } else {
                self.diagnostics.hresult(stage, hresult);
                map_hresult(hresult)
            }
        }
    }

    struct Apartment(PhantomData<Rc<()>>);
    impl Apartment {
        fn enter() -> Result<Self, ClipboardError> {
            Self::enter_with(|_, error| map_error(error))
        }

        fn enter_for_read() -> Result<Self, ClipboardError> {
            Self::enter_with(map_read_error)
        }

        fn enter_mta_for_modify(context: &ModifyReadContext<'_>) -> Result<Self, ClipboardError> {
            context.enter(ReadStage::MtaInitialize)?;
            // This runs only on a fresh worker. Never fall back to an STA that
            // would need a message pump while the caller waits for completion.
            match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                Ok(()) => {
                    let apartment = Self(PhantomData);
                    context.diagnostics.mark_mta();
                    context.check()?;
                    Ok(apartment)
                }
                Err(error) => {
                    Err(context.map_hresult(ReadStage::MtaInitialize, error.code().0 as u32))
                }
            }
        }

        fn enter_with(
            map_error: impl Fn(&'static str, windows::core::Error) -> ClipboardError,
        ) -> Result<Self, ClipboardError> {
            match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                Ok(()) => Ok(Self(PhantomData)),
                Err(error) if error.code() == RPC_E_CHANGED_MODE => {
                    unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.map_err(|error| {
                        map_error("RoInitialize(RO_INIT_SINGLETHREADED)", error)
                    })?;
                    Ok(Self(PhantomData))
                }
                Err(error) => Err(map_error("RoInitialize(RO_INIT_MULTITHREADED)", error)),
            }
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    fn package(text: &str) -> Result<(DataPackage, ClipboardContentOptions), ClipboardError> {
        let package = DataPackage::new().map_err(map_error)?;
        package.SetText(&HSTRING::from(text)).map_err(map_error)?;
        // A one-byte stream is a WinRT-marshallable custom-format payload.
        let marker = InMemoryRandomAccessStream::new().map_err(map_error)?;
        marker.SetSize(1).map_err(map_error)?;
        marker.Seek(0).map_err(map_error)?;
        let marker: IInspectable = marker.cast().map_err(map_error)?;
        package
            .SetData(&HSTRING::from(PRIVATE_FORMAT), &marker)
            .map_err(map_error)?;
        let options = ClipboardContentOptions::new().map_err(map_error)?;
        options.SetIsAllowedInHistory(false).map_err(map_error)?;
        options.SetIsRoamable(false).map_err(map_error)?;
        Ok((package, options))
    }
    fn publish(
        set: impl FnOnce() -> Result<bool, ClipboardError>,
        flush: impl FnOnce() -> Result<(), ClipboardError>,
    ) -> Result<CopyStatus, ClipboardError> {
        if !set()? {
            return Err(ClipboardError::NotAccepted);
        }
        Ok(if flush().is_ok() {
            CopyStatus::AcceptedAndPersisted
        } else {
            CopyStatus::AcceptedButNotPersisted
        })
    }
    pub(super) fn copy(secret: &SecretResult) -> Result<CopyStatus, ClipboardError> {
        let _apartment = Apartment::enter()?;
        let (content, options) = package(secret.expose())?;
        publish(
            || Clipboard::SetContentWithOptions(&content, &options).map_err(map_error),
            || Clipboard::Flush().map_err(map_error),
        )
    }
    trait View {
        fn private(&self) -> Result<bool, ClipboardError>;
        fn has_text(&self) -> Result<bool, ClipboardError>;
        fn text(&self, timeout: Duration) -> Result<String, ClipboardError>;

        fn private_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<bool, ClipboardError> {
            context.enter(ReadStage::PrivateMarkerCheck)?;
            self.private()
        }

        fn has_text_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<bool, ClipboardError> {
            context.enter(ReadStage::TextFormatCheck)?;
            self.has_text()
        }

        fn text_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<String, ClipboardError> {
            context.enter(ReadStage::GetTextAsync)?;
            let text = self.text(context.remaining())?;
            context.check()?;
            Ok(text)
        }
    }
    impl View for DataPackageView {
        fn private(&self) -> Result<bool, ClipboardError> {
            self.Contains(&HSTRING::from(PRIVATE_FORMAT))
                .map_err(|error| map_read_error("DataPackageView.Contains(private-format)", error))
        }
        fn has_text(&self) -> Result<bool, ClipboardError> {
            self.Contains(
                &StandardDataFormats::Text()
                    .map_err(|error| map_read_error("StandardDataFormats::Text", error))?,
            )
            .map_err(|error| map_read_error("DataPackageView.Contains(text-format)", error))
        }
        fn text(&self, timeout: Duration) -> Result<String, ClipboardError> {
            wait_text(
                self.GetTextAsync()
                    .map_err(|error| map_read_error("DataPackageView.GetTextAsync", error))?,
                timeout,
            )
        }

        fn private_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<bool, ClipboardError> {
            context.call(ReadStage::PrivateMarkerCheck, || {
                self.Contains(&HSTRING::from(PRIVATE_FORMAT))
            })
        }

        fn has_text_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<bool, ClipboardError> {
            let text_format = context.call(ReadStage::TextFormat, StandardDataFormats::Text)?;
            context.call(ReadStage::TextFormatCheck, || self.Contains(&text_format))
        }

        fn text_for_modify(
            &self,
            context: &ModifyReadContext<'_>,
        ) -> Result<String, ClipboardError> {
            context.enter(ReadStage::GetTextAsync)?;
            let operation = match self.GetTextAsync() {
                Ok(operation) => operation,
                Err(error) => {
                    return Err(context.map_hresult(ReadStage::GetTextAsync, error.code().0 as u32));
                }
            };
            if let Err(error) = context.check() {
                cancel_operation(&operation, context.diagnostics);
                return Err(error);
            }
            wait_text_for_modify(operation, context)
        }
    }

    fn classify(view: &impl View, timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        classify_snapshot(|| view.private(), || view.has_text(), || view.text(timeout))
    }

    fn classify_for_modify(
        view: &impl View,
        context: &ModifyReadContext<'_>,
    ) -> Result<ClipboardRead, ClipboardError> {
        classify_snapshot(
            || view.private_for_modify(context),
            || view.has_text_for_modify(context),
            || view.text_for_modify(context),
        )
    }

    fn classify_snapshot(
        private_check: impl FnOnce() -> Result<bool, ClipboardError>,
        text_check: impl FnOnce() -> Result<bool, ClipboardError>,
        read_text: impl FnOnce() -> Result<String, ClipboardError>,
    ) -> Result<ClipboardRead, ClipboardError> {
        if private_check()? {
            return Ok(ClipboardRead::Private);
        }
        if !text_check()? {
            return Ok(ClipboardRead::NonText);
        }
        Ok(ClipboardRead::Ordinary(read_text()?))
    }

    enum CompletionSignal {
        Status(AsyncStatus),
        Disconnected,
    }

    fn signal_completion(sender: &SyncSender<AsyncStatus>, status: AsyncStatus) {
        let _ = sender.try_send(status);
    }

    fn wait_for_completion(
        receiver: &Receiver<AsyncStatus>,
        context: &ModifyReadContext<'_>,
    ) -> Result<CompletionSignal, ClipboardError> {
        context.enter(ReadStage::CompletionWait)?;
        match receiver.recv_timeout(context.remaining()) {
            Ok(status) => Ok(CompletionSignal::Status(status)),
            Err(RecvTimeoutError::Timeout) => {
                context.diagnostics.timeout();
                Err(ClipboardError::TimedOut)
            }
            Err(RecvTimeoutError::Disconnected) => Ok(CompletionSignal::Disconnected),
        }
    }

    fn cancel_operation(operation: &IAsyncOperation<HSTRING>, diagnostics: &ReadDiagnostics) {
        diagnostics.enter(ReadStage::Cancel);
        if let Err(error) = operation.Cancel() {
            tracing::warn!(
                target: "multi_launcher::clipboard",
                outcome = "cancel_hresult",
                stage = ReadStage::Cancel.as_str(),
                elapsed_ms = diagnostics.elapsed_ms(),
                apartment = diagnostics.apartment(),
                hresult = error.code().0 as u32,
                "WinRT clipboard Modify read cancellation failed"
            );
        }
    }

    fn complete_text_operation(
        operation: &IAsyncOperation<HSTRING>,
        status: AsyncStatus,
        context: &ModifyReadContext<'_>,
    ) -> Result<String, ClipboardError> {
        match status {
            AsyncStatus::Completed => context
                .call(ReadStage::GetResults, || operation.GetResults())
                .map(|text| text.to_string_lossy()),
            AsyncStatus::Started => {
                context
                    .diagnostics
                    .issue("completion_not_terminal", ReadStage::Completion);
                cancel_operation(operation, context.diagnostics);
                Err(ClipboardError::Unavailable)
            }
            _ => {
                let error_code = context.call(ReadStage::ErrorCode, || operation.ErrorCode())?;
                Err(context.map_hresult(ReadStage::Completion, error_code.0 as u32))
            }
        }
    }

    fn operation_status(
        operation: &IAsyncOperation<HSTRING>,
        context: &ModifyReadContext<'_>,
    ) -> Result<AsyncStatus, ClipboardError> {
        context.call(ReadStage::AsyncStatus, || operation.Status())
    }

    fn wait_text_for_modify(
        operation: IAsyncOperation<HSTRING>,
        context: &ModifyReadContext<'_>,
    ) -> Result<String, ClipboardError> {
        let status = match operation_status(&operation, context) {
            Ok(status) => status,
            Err(error) => {
                cancel_operation(&operation, context.diagnostics);
                return Err(error);
            }
        };
        if status != AsyncStatus::Started {
            return complete_text_operation(&operation, status, context);
        }

        let (sender, receiver) = mpsc::sync_channel(1);
        let handler = AsyncOperationCompletedHandler::new(move |_, status| {
            signal_completion(&sender, status);
            Ok(())
        });
        if let Err(error) = context.call(ReadStage::CompletionRegistration, || {
            operation.SetCompleted(&handler)
        }) {
            cancel_operation(&operation, context.diagnostics);
            return Err(error);
        }

        // SetCompleted may race an operation that finished after the initial
        // status check. Querying once after registration closes that race; the
        // bounded channel also retains a callback delivered before this query.
        let status = match operation_status(&operation, context) {
            Ok(status) => status,
            Err(error) => {
                cancel_operation(&operation, context.diagnostics);
                return Err(error);
            }
        };
        if status != AsyncStatus::Started {
            return complete_text_operation(&operation, status, context);
        }

        match wait_for_completion(&receiver, context) {
            Ok(CompletionSignal::Status(status)) => {
                if let Err(error) = context.check() {
                    cancel_operation(&operation, context.diagnostics);
                    return Err(error);
                }
                let status = if status == AsyncStatus::Started {
                    let status = match operation_status(&operation, context) {
                        Ok(status) => status,
                        Err(error) => {
                            cancel_operation(&operation, context.diagnostics);
                            return Err(error);
                        }
                    };
                    if status == AsyncStatus::Started {
                        context
                            .diagnostics
                            .issue("completion_not_terminal", ReadStage::Completion);
                        cancel_operation(&operation, context.diagnostics);
                        return Err(ClipboardError::Unavailable);
                    }
                    status
                } else {
                    status
                };
                complete_text_operation(&operation, status, context)
            }
            Ok(CompletionSignal::Disconnected) => {
                let status = match operation_status(&operation, context) {
                    Ok(status) => status,
                    Err(error) => {
                        cancel_operation(&operation, context.diagnostics);
                        return Err(error);
                    }
                };
                if status == AsyncStatus::Started {
                    context
                        .diagnostics
                        .issue("completion_signal_lost", ReadStage::Completion);
                    cancel_operation(&operation, context.diagnostics);
                    Err(ClipboardError::Unavailable)
                } else {
                    complete_text_operation(&operation, status, context)
                }
            }
            Err(error) => {
                if error == ClipboardError::TimedOut {
                    cancel_operation(&operation, context.diagnostics);
                }
                Err(error)
            }
        }
    }

    fn wait_text(
        operation: IAsyncOperation<HSTRING>,
        timeout: Duration,
    ) -> Result<String, ClipboardError> {
        let deadline = Instant::now() + timeout;
        loop {
            match operation
                .Status()
                .map_err(|error| map_read_error("IAsyncOperation.Status", error))?
            {
                AsyncStatus::Completed => {
                    return operation
                        .GetResults()
                        .map(|s| s.to_string_lossy())
                        .map_err(|error| map_read_error("IAsyncOperation.GetResults", error));
                }
                AsyncStatus::Started if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                AsyncStatus::Started => {
                    let _ = operation.Cancel();
                    return Err(ClipboardError::TimedOut);
                }
                _ => {
                    let hresult = operation
                        .ErrorCode()
                        .map_err(|error| map_read_error("IAsyncOperation.ErrorCode", error))?;
                    return Err(map_read_hresult(
                        "IAsyncOperation completed with an error",
                        hresult.0 as u32,
                    ));
                }
            }
        }
    }
    pub(super) fn read(timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        let _apartment = Apartment::enter_for_read()?;
        let view = Clipboard::GetContent()
            .map_err(|error| map_read_error("Clipboard.GetContent", error))?;
        classify(&view, timeout)
    }

    pub(super) fn read_for_modify(timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        run_bounded_worker(
            Arc::clone(&MODIFY_READ_WORKERS),
            timeout,
            |diagnostics, cancelled, deadline| {
                let context = ModifyReadContext {
                    diagnostics: &diagnostics,
                    cancelled: &cancelled,
                    deadline,
                };
                let _apartment = Apartment::enter_mta_for_modify(&context)?;
                let view = context.call(ReadStage::GetContent, Clipboard::GetContent)?;
                classify_for_modify(&view, &context)
            },
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const TEST_WAIT: Duration = Duration::from_millis(750);
        const TEST_MTA_WAIT: Duration = Duration::from_secs(5);

        fn context_for_test<'a>(
            diagnostics: &'a ReadDiagnostics,
            cancelled: &'a AtomicBool,
        ) -> ModifyReadContext<'a> {
            ModifyReadContext {
                diagnostics,
                cancelled,
                deadline: Instant::now() + TEST_MTA_WAIT,
            }
        }

        #[test]
        fn clipboard_hresult_classifies_retryable_contention_separately() {
            for hresult in [
                0x800401d0,
                0x80010001,
                RPC_E_SERVERCALL_RETRYLATER_HRESULT,
                0x800700aa,
            ] {
                assert_eq!(map_hresult(hresult), ClipboardError::Busy);
            }
            assert_eq!(map_hresult(0x80070005), ClipboardError::Unavailable);
        }

        #[test]
        fn modify_read_deadline_is_distinct_from_hresult_failure() {
            let start = Instant::now();
            let diagnostics = ReadDiagnostics::new(start);
            let not_cancelled = AtomicBool::new(false);
            let hresult_context = ModifyReadContext {
                diagnostics: &diagnostics,
                cancelled: &not_cancelled,
                deadline: start + TEST_WAIT,
            };
            assert_eq!(
                hresult_context.map_hresult(ReadStage::GetContent, 0x80070005),
                ClipboardError::Unavailable
            );

            let timeout_start = Instant::now();
            let timeout_diagnostics = ReadDiagnostics::new(timeout_start);
            let cancelled = AtomicBool::new(true);
            let timeout_context = ModifyReadContext {
                diagnostics: &timeout_diagnostics,
                cancelled: &cancelled,
                deadline: timeout_start + TEST_WAIT,
            };
            assert_eq!(
                timeout_context.map_hresult(ReadStage::GetContent, 0x80070005),
                ClipboardError::TimedOut
            );
        }

        #[test]
        fn completion_signal_is_one_shot_and_late_callback_does_not_block() {
            let diagnostics = ReadDiagnostics::new(Instant::now());
            let cancelled = AtomicBool::new(false);
            let context = context_for_test(&diagnostics, &cancelled);
            let (sender, receiver) = mpsc::sync_channel(1);
            signal_completion(&sender, AsyncStatus::Completed);
            signal_completion(&sender, AsyncStatus::Started);
            assert!(matches!(
                wait_for_completion(&receiver, &context),
                Ok(CompletionSignal::Status(AsyncStatus::Completed))
            ));

            drop(receiver);
            let (finished_tx, finished_rx) = mpsc::channel();
            let late_callback = thread::spawn(move || {
                signal_completion(&sender, AsyncStatus::Started);
                let _ = finished_tx.send(());
            });
            finished_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("late completion callback should not block on a dropped receiver");
            late_callback.join().unwrap();
        }

        #[test]
        fn completion_signal_waits_for_asynchronous_delivery() {
            let diagnostics = ReadDiagnostics::new(Instant::now());
            let cancelled = AtomicBool::new(false);
            let context = context_for_test(&diagnostics, &cancelled);
            let (sender, receiver) = mpsc::sync_channel(1);
            let callback = thread::spawn(move || {
                signal_completion(&sender, AsyncStatus::Completed);
            });

            assert!(matches!(
                wait_for_completion(&receiver, &context),
                Ok(CompletionSignal::Status(AsyncStatus::Completed))
            ));
            callback.join().unwrap();
        }

        #[test]
        fn completion_wait_reports_deadline_without_hresult() {
            let start = Instant::now();
            let diagnostics = ReadDiagnostics::new(start);
            let cancelled = AtomicBool::new(false);
            let context = ModifyReadContext {
                diagnostics: &diagnostics,
                cancelled: &cancelled,
                deadline: start,
            };
            let (_sender, receiver) = mpsc::sync_channel(1);

            assert!(matches!(
                wait_for_completion(&receiver, &context),
                Err(ClipboardError::TimedOut)
            ));
        }

        #[test]
        fn protected_clipboard_package_contains_marker_and_disables_history() {
            let _apartment = Apartment::enter().unwrap();
            let (package, options) = package("fixture text").unwrap();
            assert!(!options.IsAllowedInHistory().unwrap());
            assert!(!options.IsRoamable().unwrap());
            let view = package.GetView().unwrap();
            assert!(view.Contains(&HSTRING::from(PRIVATE_FORMAT)).unwrap());
            assert!(
                view.Contains(&StandardDataFormats::Text().unwrap())
                    .unwrap()
            );
            assert_eq!(classify(&view, TEST_WAIT).unwrap(), ClipboardRead::Private);
            assert_eq!(
                wait_text(view.GetTextAsync().unwrap(), TEST_WAIT).unwrap(),
                "fixture text"
            );
            let ordinary = DataPackage::new().unwrap();
            ordinary.SetText(&HSTRING::from("fixture text")).unwrap();
            assert_eq!(
                classify(&ordinary.GetView().unwrap(), TEST_WAIT).unwrap(),
                ClipboardRead::Ordinary("fixture text".into())
            );
        }

        #[test]
        fn modify_mta_reader_classifies_an_in_memory_package_without_system_clipboard() {
            let result = run_bounded_worker(
                Arc::new(ReadWorkerBudget::new(1)),
                TEST_MTA_WAIT,
                |diagnostics, cancelled, deadline| {
                    let context = ModifyReadContext {
                        diagnostics: &diagnostics,
                        cancelled: &cancelled,
                        deadline,
                    };
                    let _apartment = Apartment::enter_mta_for_modify(&context)?;
                    let ordinary = DataPackage::new().map_err(map_error)?;
                    ordinary
                        .SetText(&HSTRING::from("fixture text"))
                        .map_err(map_error)?;
                    let view = ordinary.GetView().map_err(map_error)?;
                    let fixture = classify_for_modify(&view, &context)?;

                    let empty = DataPackage::new().map_err(map_error)?;
                    empty.SetText(&HSTRING::from("")).map_err(map_error)?;
                    let empty_view = empty.GetView().map_err(map_error)?;
                    let empty = classify_for_modify(&empty_view, &context)?;
                    Ok((fixture, empty))
                },
            );
            assert_eq!(
                result,
                Ok((
                    ClipboardRead::Ordinary("fixture text".into()),
                    ClipboardRead::Ordinary(String::new()),
                ))
            );
        }

        struct Fake {
            private: Result<bool, ClipboardError>,
            has_text: Result<bool, ClipboardError>,
            fetched: std::cell::Cell<bool>,
        }
        impl View for Fake {
            fn private(&self) -> Result<bool, ClipboardError> {
                self.private
            }
            fn has_text(&self) -> Result<bool, ClipboardError> {
                self.has_text
            }
            fn text(&self, _: Duration) -> Result<String, ClipboardError> {
                self.fetched.set(true);
                Ok("fixture".into())
            }
        }
        #[test]
        fn protected_clipboard_private_never_fetches_text_and_refusal_never_flushes() {
            let view = Fake {
                private: Ok(true),
                has_text: Ok(true),
                fetched: std::cell::Cell::new(false),
            };
            let diagnostics = ReadDiagnostics::new(Instant::now());
            let cancelled = AtomicBool::new(false);
            let context = context_for_test(&diagnostics, &cancelled);
            assert_eq!(
                classify_for_modify(&view, &context),
                Ok(ClipboardRead::Private)
            );
            assert!(!view.fetched.get());

            let non_text = Fake {
                private: Ok(false),
                has_text: Ok(false),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(
                classify_for_modify(&non_text, &context),
                Ok(ClipboardRead::NonText)
            );
            assert!(!non_text.fetched.get());

            let unknown = Fake {
                private: Err(ClipboardError::Unavailable),
                has_text: Ok(true),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(
                classify_for_modify(&unknown, &context),
                Err(ClipboardError::Unavailable)
            );
            assert!(!unknown.fetched.get());

            let text_check_failed = Fake {
                private: Ok(false),
                has_text: Err(ClipboardError::Unavailable),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(
                classify_for_modify(&text_check_failed, &context),
                Err(ClipboardError::Unavailable)
            );
            assert!(!text_check_failed.fetched.get());

            let flushed = std::cell::Cell::new(false);
            assert_eq!(
                publish(
                    || Ok(false),
                    || {
                        flushed.set(true);
                        Ok(())
                    }
                ),
                Err(ClipboardError::NotAccepted)
            );
            assert!(!flushed.get());
            assert_eq!(
                publish(|| Ok(true), || Err(ClipboardError::Unavailable)),
                Ok(CopyStatus::AcceptedButNotPersisted)
            );
        }

        #[test]
        fn bounded_worker_returns_success_and_late_output_is_discarded() {
            let budget = Arc::new(ReadWorkerBudget::new(MAX_MODIFY_READ_WORKERS));
            let success = run_bounded_worker(Arc::clone(&budget), TEST_WAIT, |_, _, _| {
                Ok("fixture".to_owned())
            });
            assert_eq!(success, Ok("fixture".to_owned()));
            let success_release_deadline = Instant::now() + Duration::from_secs(1);
            while budget.active() != 0 && Instant::now() < success_release_deadline {
                thread::yield_now();
            }
            assert_eq!(budget.active(), 0);

            let (started_tx, started_rx) = mpsc::sync_channel(1);
            let (release_tx, release_rx) = mpsc::channel();
            let (exited_tx, exited_rx) = mpsc::channel();
            let timed_out = run_bounded_worker(
                Arc::clone(&budget),
                Duration::from_millis(30),
                move |_, _, _| {
                    let _ = started_tx.try_send(());
                    let _ = release_rx.recv();
                    let _ = exited_tx.send(());
                    Ok("late clipboard text".to_owned())
                },
            );
            assert_eq!(timed_out, Err(ClipboardError::TimedOut));
            started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("timed-out worker should have started");

            // The timed-out worker still owns a slot, but the second slot
            // remains available for a fresh read.
            let recovered = run_bounded_worker(Arc::clone(&budget), TEST_WAIT, |_, _, _| {
                Ok("fresh clipboard text".to_owned())
            });
            assert_eq!(recovered, Ok("fresh clipboard text".to_owned()));

            release_tx.send(()).unwrap();
            exited_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("late worker should finish after release");
            let release_deadline = Instant::now() + Duration::from_secs(1);
            while budget.active() != 0 && Instant::now() < release_deadline {
                thread::yield_now();
            }
            assert_eq!(budget.active(), 0);
        }

        #[test]
        fn timed_out_workers_remain_bounded_and_saturation_is_retryable() {
            let budget = Arc::new(ReadWorkerBudget::new(MAX_MODIFY_READ_WORKERS));
            let mut releases = Vec::new();
            let mut exited_receivers = Vec::new();
            for _ in 0..MAX_MODIFY_READ_WORKERS {
                let (started_tx, started_rx) = mpsc::sync_channel(1);
                let (release_tx, release_rx) = mpsc::channel();
                let (exited_tx, exited_rx) = mpsc::channel();
                let result = run_bounded_worker(
                    Arc::clone(&budget),
                    Duration::from_millis(30),
                    move |_, _, _| {
                        let _ = started_tx.try_send(());
                        let _ = release_rx.recv();
                        let _ = exited_tx.send(());
                        Ok(())
                    },
                );
                assert_eq!(result, Err(ClipboardError::TimedOut));
                started_rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("timed-out worker should continue to own its slot");
                releases.push(release_tx);
                exited_receivers.push(exited_rx);
            }

            assert_eq!(budget.active(), MAX_MODIFY_READ_WORKERS);
            let unexpected_spawn = Arc::new(AtomicBool::new(false));
            let worker_was_spawned = Arc::clone(&unexpected_spawn);
            assert_eq!(
                run_bounded_worker(Arc::clone(&budget), TEST_WAIT, move |_, _, _| {
                    worker_was_spawned.store(true, Ordering::Release);
                    Ok(())
                }),
                Err(ClipboardError::Busy)
            );
            assert!(!unexpected_spawn.load(Ordering::Acquire));

            for release in releases {
                release.send(()).unwrap();
            }
            for exited in exited_receivers {
                exited
                    .recv_timeout(Duration::from_secs(1))
                    .expect("timed-out worker should exit after release");
            }
            let release_deadline = Instant::now() + Duration::from_secs(1);
            while budget.active() != 0 && Instant::now() < release_deadline {
                thread::yield_now();
            }
            assert_eq!(budget.active(), 0);
        }
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn copy(_: &SecretResult) -> Result<CopyStatus, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
    pub(super) fn read(_: Duration) -> Result<ClipboardRead, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable)?;
        match clipboard.get_text() {
            Ok(text) => Ok(ClipboardRead::Ordinary(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(ClipboardRead::NonText),
            Err(arboard::Error::ClipboardOccupied) => Err(ClipboardError::Busy),
            Err(_) => Err(ClipboardError::Unavailable),
        }
    }

    pub(super) fn read_for_modify(timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        read(timeout)
    }
}
