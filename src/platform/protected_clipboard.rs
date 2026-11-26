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
/// An opaque identity captured while the clipboard remains open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardStamp(u32);

impl ClipboardStamp {
    pub fn from_sequence(sequence: u32) -> Option<Self> {
        (sequence != 0).then_some(Self(sequence))
    }
}

pub enum StampedClipboardRead {
    Ordinary {
        text: String,
        stamp: Option<ClipboardStamp>,
    },
    Private,
    NonText,
}

pub fn read_stamped_clipboard_for_modify() -> Result<StampedClipboardRead, ClipboardError> {
    platform::read_stamped_for_modify(MODIFY_READ_TIMEOUT)
}

pub fn current_clipboard_stamp() -> Option<ClipboardStamp> {
    #[cfg(windows)]
    {
        ClipboardStamp::from_sequence(unsafe {
            windows::Win32::System::DataExchange::GetClipboardSequenceNumber()
        })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Called only while the caller's native clipboard writer holds OpenClipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenClipboardFormat {
    Ordinary,
    Private,
    NonText,
}

pub fn checked_open_clipboard_is_ordinary() -> Result<OpenClipboardFormat, ClipboardError> {
    platform::checked_open_is_ordinary()
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

/// Read one clipboard snapshot for Clipboard Modify on a bounded worker.
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
        },
        thread,
        time::Instant,
    };
    use windows::{
        ApplicationModel::DataTransfer::{
            Clipboard, ClipboardContentOptions, DataPackage, DataPackageView, StandardDataFormats,
        },
        Foundation::{AsyncStatus, IAsyncOperation},
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
    pub(super) fn checked_open_is_ordinary() -> Result<OpenClipboardFormat, ClipboardError> {
        use windows::Win32::{
            Foundation::{ERROR_CLIPBOARD_NOT_OPEN, ERROR_SUCCESS, GetLastError, SetLastError},
            System::DataExchange::{EnumClipboardFormats, RegisterClipboardFormatW},
        };
        let marker: Vec<u16> = PRIVATE_FORMAT.encode_utf16().chain(Some(0)).collect();
        let private_format =
            unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(marker.as_ptr())) };
        if private_format == 0 {
            return Err(ClipboardError::Unavailable);
        }
        let (private, has_text) = inspect_formats(private_format, |current| {
            unsafe { SetLastError(ERROR_SUCCESS) };
            let format = unsafe { EnumClipboardFormats(current) };
            if format != 0 {
                Ok(Some(format))
            } else {
                match unsafe { GetLastError() } {
                    ERROR_SUCCESS => Ok(None),
                    ERROR_CLIPBOARD_NOT_OPEN => Err(ClipboardError::Busy),
                    _ => Err(ClipboardError::Unavailable),
                }
            }
        })?;
        Ok(if private {
            OpenClipboardFormat::Private
        } else if has_text {
            OpenClipboardFormat::Ordinary
        } else {
            OpenClipboardFormat::NonText
        })
    }
    const MAX_MODIFY_READ_WORKERS: usize = 2;
    const MODIFY_READ_WORKER_NAME: &str = "clipboard-modify-read";
    // COM asks clients to retry when its server is temporarily unable to handle a call.
    const RPC_E_SERVERCALL_RETRYLATER_HRESULT: u32 = 0x8001010a;

    #[derive(Clone, Copy)]
    #[repr(u8)]
    enum ReadStage {
        WorkerAdmission,
        WorkerSpawn,
        RegisterMarker,
        OpenClipboard,
        EnumerateFormats,
        GetText,
        GlobalSize,
        GlobalLock,
        DecodeText,
        WorkerCleanup,
    }

    impl ReadStage {
        fn as_str(self) -> &'static str {
            match self {
                Self::WorkerAdmission => "worker_admission",
                Self::WorkerSpawn => "worker_spawn",
                Self::RegisterMarker => "RegisterClipboardFormatW",
                Self::OpenClipboard => "OpenClipboard",
                Self::EnumerateFormats => "EnumClipboardFormats",
                Self::GetText => "GetClipboardData(CF_UNICODETEXT)",
                Self::GlobalSize => "GlobalSize",
                Self::GlobalLock => "GlobalLock",
                Self::DecodeText => "decode_utf16",
                Self::WorkerCleanup => "clipboard cleanup",
            }
        }
        fn from_u8(value: u8) -> Self {
            match value {
                0 => Self::WorkerAdmission,
                1 => Self::WorkerSpawn,
                2 => Self::RegisterMarker,
                3 => Self::OpenClipboard,
                4 => Self::EnumerateFormats,
                5 => Self::GetText,
                6 => Self::GlobalSize,
                7 => Self::GlobalLock,
                8 => Self::DecodeText,
                _ => Self::WorkerCleanup,
            }
        }
    }

    struct ReadDiagnostics {
        started: Instant,
        stage: AtomicU8,
        failure_event_recorded: AtomicBool,
        timeout_event_recorded: AtomicBool,
    }
    impl ReadDiagnostics {
        fn new(started: Instant) -> Self {
            Self {
                started,
                stage: AtomicU8::new(ReadStage::WorkerAdmission as u8),
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
        fn elapsed_ms(&self) -> u64 {
            self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
        }
        fn timeout(&self) {
            if !self.timeout_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(target: "multi_launcher::clipboard", backend = "win32", outcome = "timeout", stage = self.current_stage().as_str(), elapsed_ms = self.elapsed_ms(), "Clipboard Modify read timed out");
            }
        }
        fn native_error(&self, stage: ReadStage, code: u32) {
            self.enter(stage);
            if !self.failure_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(target: "multi_launcher::clipboard", backend = "win32", outcome = "error", stage = stage.as_str(), elapsed_ms = self.elapsed_ms(), code, "Clipboard Modify read failed");
            }
        }
        fn issue(&self, outcome: &'static str, stage: ReadStage) {
            self.enter(stage);
            if !self.failure_event_recorded.swap(true, Ordering::AcqRel) {
                tracing::warn!(target: "multi_launcher::clipboard", backend = "win32", outcome, stage = stage.as_str(), elapsed_ms = self.elapsed_ms(), "Clipboard Modify read failed");
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
        let (sender, receiver) = crossbeam_channel::bounded(1);
        diagnostics.enter(ReadStage::WorkerSpawn);
        let spawned = thread::Builder::new()
            .name(MODIFY_READ_WORKER_NAME.into())
            .spawn(move || {
                let permit = permit;
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
                // The worker closure returns after native resources are released.
                // Free capacity before publishing
                // so a successful recovery admits the next caller immediately.
                drop(permit);
                let _ = sender.try_send(result);
            });
        match spawned {
            // Detach rather than joining: a stalled native call must not extend the caller wait.
            Ok(handle) => drop(handle),
            Err(_) => {
                diagnostics.issue("worker_spawn_failed", ReadStage::WorkerSpawn);
                return Err(ClipboardError::Busy);
            }
        }

        receive_bounded_result(&receiver, deadline, &cancelled, &diagnostics)
    }

    fn receive_bounded_result<T>(
        receiver: &crossbeam_channel::Receiver<Result<T, ClipboardError>>,
        deadline: Instant,
        cancelled: &AtomicBool,
        diagnostics: &ReadDiagnostics,
    ) -> Result<T, ClipboardError> {
        match receiver.recv_deadline(deadline) {
            Ok(result) => accept_worker_result(result, deadline, cancelled, diagnostics),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                expire_worker_wait(cancelled, diagnostics)
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected)
                if Instant::now() >= deadline =>
            {
                expire_worker_wait(cancelled, diagnostics)
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                diagnostics.issue("worker_exited_without_result", diagnostics.current_stage());
                Err(ClipboardError::Unavailable)
            }
        }
    }

    fn accept_worker_result<T>(
        result: Result<T, ClipboardError>,
        deadline: Instant,
        cancelled: &AtomicBool,
        diagnostics: &ReadDiagnostics,
    ) -> Result<T, ClipboardError> {
        if Instant::now() < deadline {
            result
        } else {
            expire_worker_wait(cancelled, diagnostics)
        }
    }

    fn expire_worker_wait<T>(
        cancelled: &AtomicBool,
        diagnostics: &ReadDiagnostics,
    ) -> Result<T, ClipboardError> {
        cancelled.store(true, Ordering::Release);
        diagnostics.timeout();
        Err(ClipboardError::TimedOut)
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
    struct Apartment(PhantomData<Rc<()>>);
    impl Apartment {
        fn enter() -> Result<Self, ClipboardError> {
            Self::enter_with(|_, error| map_error(error))
        }

        fn enter_for_read() -> Result<Self, ClipboardError> {
            Self::enter_with(map_read_error)
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
    }

    fn classify(view: &impl View, timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        classify_snapshot(|| view.private(), || view.has_text(), || view.text(timeout))
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

    // Enumerating the complete format list distinguishes an absent marker from
    // an inspection failure. The caller keeps the clipboard open throughout.
    fn inspect_formats(
        private_format: u32,
        mut next: impl FnMut(u32) -> Result<Option<u32>, ClipboardError>,
    ) -> Result<(bool, bool), ClipboardError> {
        let mut current = 0;
        let mut has_text = false;
        while let Some(format) = next(current)? {
            if format == private_format {
                return Ok((true, has_text));
            }
            has_text |= format == 13; // CF_UNICODETEXT
            current = format;
        }
        Ok((false, has_text))
    }

    fn decode_native_text(bytes: &[u8]) -> Result<String, ClipboardError> {
        if bytes.len() % 2 != 0 {
            return Err(ClipboardError::Unavailable);
        }
        let mut words = Vec::new();
        for pair in bytes.chunks_exact(2) {
            let word = u16::from_le_bytes([pair[0], pair[1]]);
            if word == 0 {
                return String::from_utf16(&words).map_err(|_| ClipboardError::Unavailable);
            }
            words.push(word);
        }
        Err(ClipboardError::Unavailable)
    }

    fn read_native(
        diagnostics: &ReadDiagnostics,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<StampedClipboardRead, ClipboardError> {
        use windows::Win32::{
            Foundation::{ERROR_SUCCESS, GetLastError, HGLOBAL, SetLastError},
            System::{
                DataExchange::{
                    CloseClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
                    RegisterClipboardFormatW,
                },
                Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            },
        };

        struct OpenGuard;
        impl Drop for OpenGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        struct LockGuard(HGLOBAL);
        impl Drop for LockGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = GlobalUnlock(self.0);
                }
            }
        }
        let check = |stage| {
            diagnostics.enter(stage);
            if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
                diagnostics.timeout();
                Err(ClipboardError::TimedOut)
            } else {
                Ok(())
            }
        };
        let failure = |stage, code| {
            diagnostics.native_error(stage, code);
            ClipboardError::Unavailable
        };
        check(ReadStage::RegisterMarker)?;
        let marker: Vec<u16> = PRIVATE_FORMAT.encode_utf16().chain(Some(0)).collect();
        let private_format =
            unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(marker.as_ptr())) };
        if private_format == 0 {
            return Err(failure(ReadStage::RegisterMarker, unsafe {
                GetLastError().0
            }));
        }
        check(ReadStage::OpenClipboard)?;
        // A failed open commonly means another process has the clipboard open.
        if let Err(error) = unsafe { OpenClipboard(None) } {
            let code = error.code().0 as u32;
            diagnostics.native_error(ReadStage::OpenClipboard, code);
            return Err(
                if code == 0x80070005 || code == 0x800700aa || code == 0x80004005 {
                    ClipboardError::Busy
                } else {
                    ClipboardError::Unavailable
                },
            );
        }
        let _open = OpenGuard;
        check(ReadStage::EnumerateFormats)?;
        let (private, has_text) = inspect_formats(private_format, |current| {
            check(ReadStage::EnumerateFormats)?;
            unsafe { SetLastError(ERROR_SUCCESS) };
            let format = unsafe { EnumClipboardFormats(current) };
            if format != 0 {
                return Ok(Some(format));
            }
            let error = unsafe { GetLastError() };
            if error == ERROR_SUCCESS {
                Ok(None)
            } else {
                Err(failure(ReadStage::EnumerateFormats, error.0))
            }
        })?;
        if private {
            return Ok(StampedClipboardRead::Private);
        }
        if !has_text {
            return Ok(StampedClipboardRead::NonText);
        }
        check(ReadStage::GetText)?;
        let handle = unsafe { GetClipboardData(13) }
            .map_err(|error| failure(ReadStage::GetText, error.code().0 as u32))?;
        let global = HGLOBAL(handle.0);
        check(ReadStage::GlobalSize)?;
        let size = unsafe { GlobalSize(global) };
        if size == 0 || size > isize::MAX as usize || size % 2 != 0 {
            diagnostics.issue("invalid_allocation_size", ReadStage::GlobalSize);
            return Err(ClipboardError::Unavailable);
        }
        check(ReadStage::GlobalLock)?;
        let pointer = unsafe { GlobalLock(global) }.cast::<u8>();
        if pointer.is_null() {
            return Err(failure(ReadStage::GlobalLock, unsafe { GetLastError().0 }));
        }
        let _lock = LockGuard(global);
        check(ReadStage::DecodeText)?;
        // The clipboard owns this allocation. The guards keep it locked and
        // open until after the bounded copy and UTF-16 validation complete.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, size) };
        let text = decode_native_text(bytes).map_err(|error| {
            diagnostics.issue("malformed_text", ReadStage::DecodeText);
            error
        })?;
        check(ReadStage::WorkerCleanup)?;
        let stamp = current_clipboard_stamp();
        Ok(StampedClipboardRead::Ordinary { text, stamp })
    }

    pub(super) fn read_for_modify(timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        read_stamped_for_modify(timeout).map(|read| match read {
            StampedClipboardRead::Ordinary { text, .. } => ClipboardRead::Ordinary(text),
            StampedClipboardRead::Private => ClipboardRead::Private,
            StampedClipboardRead::NonText => ClipboardRead::NonText,
        })
    }

    pub(super) fn read_stamped_for_modify(
        timeout: Duration,
    ) -> Result<StampedClipboardRead, ClipboardError> {
        run_bounded_worker(
            Arc::clone(&MODIFY_READ_WORKERS),
            timeout,
            |diagnostics, cancelled, deadline| read_native(&diagnostics, &cancelled, deadline),
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::sync::mpsc;

        const TEST_WAIT: Duration = Duration::from_millis(750);

        #[test]
        fn native_format_inspection_fails_closed_and_recognizes_absence() {
            let mut formats = [Some(13), Some(1234), None].into_iter();
            assert_eq!(
                inspect_formats(5678, |_| Ok(formats.next().unwrap())),
                Ok((false, true))
            );
            let mut formats = [Some(13), Some(5678)].into_iter();
            assert_eq!(
                inspect_formats(5678, |_| Ok(formats.next().unwrap())),
                Ok((true, true))
            );
            let mut count = 0;
            assert_eq!(
                inspect_formats(5678, |_| {
                    count += 1;
                    if count == 1 {
                        Ok(Some(13))
                    } else {
                        Err(ClipboardError::Unavailable)
                    }
                }),
                Err(ClipboardError::Unavailable)
            );
        }

        #[test]
        fn native_text_decoding_checks_termination_and_unicode() {
            assert_eq!(decode_native_text(&[0, 0]), Ok(String::new()));
            let text = "café\r\n😀";
            let bytes: Vec<u8> = text
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect();
            assert_eq!(decode_native_text(&bytes), Ok(text.into()));
            assert_eq!(
                decode_native_text(&[65, 0]),
                Err(ClipboardError::Unavailable)
            );
            assert_eq!(decode_native_text(&[65]), Err(ClipboardError::Unavailable));
            assert_eq!(
                decode_native_text(&[0, 0xd8, 0, 0]),
                Err(ClipboardError::Unavailable)
            );
        }

        // This test deliberately replaces the system clipboard and must be run
        // alone in an interactive session with a foreground application.
        #[test]
        #[ignore = "uses the real system clipboard"]
        fn native_reader_interoperates_with_protected_publisher() {
            use crate::password_generator::{PasswordSettings, generate_password};
            // The production publisher is normally called on the GUI STA.
            // Balance this initialization after the clipboard operations finish.
            unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.unwrap();
            let _apartment = Apartment(PhantomData);
            let mut clipboard = arboard::Clipboard::new().unwrap();
            for text in ["ml modify café\r\n😀", ""] {
                clipboard.set_text(text.to_owned()).unwrap();
                assert_eq!(
                    read_for_modify(TEST_WAIT),
                    Ok(ClipboardRead::Ordinary(text.into()))
                );
            }
            let secret = generate_password(&PasswordSettings::default(), 1).unwrap();
            copy_secret(&secret).unwrap();
            assert_eq!(read_for_modify(TEST_WAIT), Ok(ClipboardRead::Private));
            clipboard.set_text("ml modify recovery".to_owned()).unwrap();
            assert_eq!(
                read_for_modify(TEST_WAIT),
                Ok(ClipboardRead::Ordinary("ml modify recovery".into()))
            );
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
            assert_eq!(classify(&view, TEST_WAIT), Ok(ClipboardRead::Private));
            assert!(!view.fetched.get());

            let non_text = Fake {
                private: Ok(false),
                has_text: Ok(false),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(classify(&non_text, TEST_WAIT), Ok(ClipboardRead::NonText));
            assert!(!non_text.fetched.get());

            let unknown = Fake {
                private: Err(ClipboardError::Unavailable),
                has_text: Ok(true),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(
                classify(&unknown, TEST_WAIT),
                Err(ClipboardError::Unavailable)
            );
            assert!(!unknown.fetched.get());

            let text_check_failed = Fake {
                private: Ok(false),
                has_text: Err(ClipboardError::Unavailable),
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(
                classify(&text_check_failed, TEST_WAIT),
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
        fn queued_worker_result_after_absolute_deadline_is_rejected() {
            let deadline = Instant::now();
            let diagnostics = ReadDiagnostics::new(deadline);
            let cancelled = AtomicBool::new(false);
            let (sender, receiver) = crossbeam_channel::bounded(1);
            sender.try_send(Ok("late result".to_owned())).unwrap();

            assert_eq!(
                receive_bounded_result(&receiver, deadline, &cancelled, &diagnostics),
                Err(ClipboardError::TimedOut)
            );
            assert!(cancelled.load(Ordering::Acquire));
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
            assert_eq!(
                budget.active(),
                1,
                "the stuck worker should be the only occupied slot after recovery"
            );

            // Do not wait between the successful recovery and this admission:
            // its permit must be released before its result reaches the caller.
            let immediate = run_bounded_worker(Arc::clone(&budget), TEST_WAIT, |_, _, _| {
                Ok("immediate follow-up".to_owned())
            });
            assert_eq!(immediate, Ok("immediate follow-up".to_owned()));
            assert_eq!(budget.active(), 1);

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
    pub(super) fn checked_open_is_ordinary() -> Result<OpenClipboardFormat, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
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
    pub(super) fn read_stamped_for_modify(
        _: Duration,
    ) -> Result<StampedClipboardRead, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
}
