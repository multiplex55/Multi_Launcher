//! Private owner for cancellable Windows archive I/O requests.
//!
//! A request is a synchronous-on-the-caller-thread core around one overlapped
//! native operation. It creates no worker of its own, so a later archive
//! worker can reuse one thread for many bounded requests. The request, buffer,
//! event, and file handle remain owned until `GetOverlappedResult` reports the
//! operation's terminal result. Test-only worker ownership proves cancellation
//! and joining without making per-request threads part of the production API.
//! Current archive cataloging and `OwnedStage` flushing still use synchronous
//! `File` operations; callers must not treat this module as having migrated
//! those paths.

#![allow(dead_code)] // Activated by the later archive writer milestone.

use std::fs::File;
use std::marker::PhantomPinned;
use std::os::windows::io::AsRawHandle;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
#[cfg(test)]
use std::time::{Duration, Instant};

use windows::Wdk::Storage::FileSystem::{
    FILE_MODE_INFORMATION, FILE_SYNCHRONOUS_IO_ALERT, FILE_SYNCHRONOUS_IO_NONALERT,
    FileModeInformation, NtQueryInformationFile,
};
use windows::Win32::Foundation::{BOOL, CloseHandle, ERROR_IO_PENDING, ERROR_NOT_FOUND, HANDLE};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::IO::IO_STATUS_BLOCK;
use windows::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0,
};
use windows::Win32::System::Threading::CreateEventW;
use windows::core::PCWSTR;

const MAX_IO_BUFFER_BYTES: usize = 1024 * 1024;
static NEXT_IO_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    BeforeSubmission,
    Submitting,
    Pending,
    Completing,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CancelDisposition {
    BeforeSubmission,
    Requested,
    AlreadyComplete,
    Failed {
        hresult: i32,
        win32_code: Option<u32>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CancelReceipt {
    disposition: CancelDisposition,
    submission_lock_was_contended: bool,
    api_attempts: u32,
}

impl CancelReceipt {
    pub(super) fn disposition(self) -> CancelDisposition {
        self.disposition
    }

    pub(super) fn api_attempts(self) -> u32 {
        self.api_attempts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeIoTerminal {
    Completed {
        transferred: u32,
    },
    CancelledBeforeSubmission,
    Failed {
        operation: &'static str,
        hresult: i32,
        win32_code: Option<u32>,
    },
}

#[derive(Debug)]
pub(super) enum NativeIoStartError {
    InvalidBufferLength,
    SynchronousHandle { mode: u32 },
    HandleModeQueryFailed { ntstatus: i32 },
}

#[derive(Default)]
struct ResourceLifecycle {
    request_released: AtomicBool,
    buffer_released: AtomicBool,
    event_closed: AtomicBool,
}

struct RequestState {
    phase: Phase,
    cancel_requested: bool,
    cancel_disposition: Option<CancelDisposition>,
    cancel_api_attempts: u32,
    overlapped_address: usize,
}

struct SharedRequest {
    id: u64,
    file: Arc<File>,
    state: Mutex<RequestState>,
    changed: Condvar,
    lifecycle: Arc<ResourceLifecycle>,
}

/// A file handle whose mode was queried and verified as overlapped before it
/// can be used to construct an I/O request.
pub(super) struct VerifiedOverlappedFile {
    file: Arc<File>,
}

impl VerifiedOverlappedFile {
    pub(super) fn verify(file: File) -> Result<Self, NativeIoStartError> {
        let file = Arc::new(file);
        let mut information = FILE_MODE_INFORMATION::default();
        let mut io_status = IO_STATUS_BLOCK::default();
        let status = unsafe {
            NtQueryInformationFile(
                HANDLE(file.as_raw_handle() as _),
                &mut io_status,
                std::ptr::from_mut(&mut information).cast(),
                std::mem::size_of::<FILE_MODE_INFORMATION>() as u32,
                FileModeInformation,
            )
        };
        if status.0 < 0 {
            return Err(NativeIoStartError::HandleModeQueryFailed { ntstatus: status.0 });
        }
        let synchronous_modes = FILE_SYNCHRONOUS_IO_ALERT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0;
        if information.Mode & synchronous_modes != 0 {
            return Err(NativeIoStartError::SynchronousHandle {
                mode: information.Mode,
            });
        }
        Ok(Self { file })
    }
}

struct RequestStorage {
    overlapped: OVERLAPPED,
    buffer: Option<Box<[u8]>>,
    event: OwnedEvent,
    lifecycle: Arc<ResourceLifecycle>,
    _pin: PhantomPinned,
}

impl Drop for RequestStorage {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            drop(buffer);
            self.lifecycle
                .buffer_released
                .store(true, Ordering::Release);
        }
        self.lifecycle
            .request_released
            .store(true, Ordering::Release);
    }
}

struct OwnedEvent {
    handle: HANDLE,
    lifecycle: Arc<ResourceLifecycle>,
}

impl OwnedEvent {
    fn create(lifecycle: Arc<ResourceLifecycle>) -> Result<Self, windows::core::Error> {
        let handle = unsafe { CreateEventW(None, BOOL(1), BOOL(0), PCWSTR::null())? };
        lifecycle.event_closed.store(false, Ordering::Release);
        Ok(Self { handle, lifecycle })
    }
}

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        let closed = unsafe { CloseHandle(self.handle) }.is_ok();
        self.lifecycle.event_closed.store(closed, Ordering::Release);
    }
}

enum IoRequest {
    Read { offset: u64, length: usize },
    Write { offset: u64, bytes: Box<[u8]> },
}

/// One bounded overlapped operation. `execute` runs on its caller's worker
/// thread and never creates a thread of its own.
pub(super) struct NativeIoRequest {
    shared: Arc<SharedRequest>,
    request: IoRequest,
}

/// Cancellation capability published before the request is submitted.
#[derive(Clone)]
pub(super) struct NativeIoCancelHandle {
    shared: Arc<SharedRequest>,
}

/// Native completion. Read bytes are intentionally absent from Debug output
/// and routine status.
pub(super) struct NativeIoCompletion {
    request_id: u64,
    terminal: NativeIoTerminal,
    cancel: Option<CancelDisposition>,
    cancel_api_attempts: u32,
    read_buffer: Option<Box<[u8]>>,
    lifecycle: Arc<ResourceLifecycle>,
}

impl NativeIoRequest {
    /// Prepare an overlapped read. `file` must be an archive-owned handle
    /// opened for overlapped I/O and must remain owned for this request.
    pub(super) fn read(
        file: &VerifiedOverlappedFile,
        offset: u64,
        length: usize,
    ) -> Result<(Self, NativeIoCancelHandle), NativeIoStartError> {
        if length == 0 || length > MAX_IO_BUFFER_BYTES {
            return Err(NativeIoStartError::InvalidBufferLength);
        }
        Ok(Self::new(
            Arc::clone(&file.file),
            IoRequest::Read { offset, length },
        ))
    }

    /// Prepare an overlapped write under the same handle-lifetime contract as
    /// [`Self::read`]. The payload is owned by the request until completion.
    pub(super) fn write(
        file: &VerifiedOverlappedFile,
        offset: u64,
        bytes: Box<[u8]>,
    ) -> Result<(Self, NativeIoCancelHandle), NativeIoStartError> {
        if bytes.is_empty() || bytes.len() > MAX_IO_BUFFER_BYTES {
            return Err(NativeIoStartError::InvalidBufferLength);
        }
        Ok(Self::new(
            Arc::clone(&file.file),
            IoRequest::Write { offset, bytes },
        ))
    }

    fn new(file: Arc<File>, request: IoRequest) -> (Self, NativeIoCancelHandle) {
        let shared = Arc::new(SharedRequest {
            id: NEXT_IO_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
            file,
            state: Mutex::new(RequestState {
                phase: Phase::BeforeSubmission,
                cancel_requested: false,
                cancel_disposition: None,
                cancel_api_attempts: 0,
                overlapped_address: 0,
            }),
            changed: Condvar::new(),
            lifecycle: Arc::new(ResourceLifecycle {
                request_released: AtomicBool::new(true),
                buffer_released: AtomicBool::new(true),
                event_closed: AtomicBool::new(true),
            }),
        });
        let cancel = NativeIoCancelHandle {
            shared: Arc::clone(&shared),
        };
        (Self { shared, request }, cancel)
    }

    /// Issue this request on the already-owned I/O worker thread.
    pub(super) fn execute(self) -> NativeIoCompletion {
        run_request(self.shared, self.request, || {}, || {})
    }

    #[cfg(test)]
    fn execute_with_gates(
        self,
        before_admission: impl FnOnce(),
        before_issue: impl FnOnce(),
    ) -> NativeIoCompletion {
        run_request(self.shared, self.request, before_admission, before_issue)
    }
}

impl NativeIoCancelHandle {
    pub(super) fn cancel(&self) -> CancelReceipt {
        self.cancel_with_contention_callback(|| {})
    }

    fn cancel_with_contention_callback(
        &self,
        on_submission_contention: impl FnOnce(),
    ) -> CancelReceipt {
        let shared = &self.shared;
        let mut callback = Some(on_submission_contention);
        let (mut state, contended) = match shared.state.try_lock() {
            Ok(state) => (state, false),
            Err(TryLockError::WouldBlock) => {
                if let Some(callback) = callback.take() {
                    callback();
                }
                (lock_state(&shared.state), true)
            }
            Err(TryLockError::Poisoned(poisoned)) => (poisoned.into_inner(), false),
        };
        if let Some(disposition) = state.cancel_disposition {
            return CancelReceipt {
                disposition,
                submission_lock_was_contended: contended,
                api_attempts: state.cancel_api_attempts,
            };
        }

        state.cancel_requested = true;
        let disposition = match state.phase {
            Phase::BeforeSubmission | Phase::Submitting => CancelDisposition::BeforeSubmission,
            Phase::Pending => {
                if state.overlapped_address == 0 {
                    CancelDisposition::AlreadyComplete
                } else {
                    state.cancel_api_attempts += 1;
                    let overlapped = state.overlapped_address as *const OVERLAPPED;
                    match unsafe {
                        CancelIoEx(HANDLE(shared.file.as_raw_handle() as _), Some(overlapped))
                    } {
                        Ok(()) => CancelDisposition::Requested,
                        Err(error) if is_win32_error(&error, ERROR_NOT_FOUND.0) => {
                            CancelDisposition::AlreadyComplete
                        }
                        Err(error) => CancelDisposition::Failed {
                            hresult: error.code().0,
                            win32_code: win32_code(&error),
                        },
                    }
                }
            }
            Phase::Completing | Phase::Terminal => CancelDisposition::AlreadyComplete,
        };
        state.cancel_disposition = Some(disposition);
        shared.changed.notify_all();
        CancelReceipt {
            disposition,
            submission_lock_was_contended: contended,
            api_attempts: state.cancel_api_attempts,
        }
    }

    #[cfg(test)]
    fn wait_until_pending(&self, timeout: Duration) -> bool {
        let shared = &self.shared;
        let deadline = Instant::now() + timeout;
        let mut state = lock_state(&shared.state);
        loop {
            match state.phase {
                Phase::Pending => return true,
                Phase::Terminal => return false,
                Phase::BeforeSubmission | Phase::Submitting | Phase::Completing => {}
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let duration = deadline.saturating_duration_since(now);
            let waited = shared.changed.wait_timeout(state, duration);
            match waited {
                Ok((next, timeout)) => {
                    state = next;
                    if timeout.timed_out() && state.phase != Phase::Pending {
                        return false;
                    }
                }
                Err(poisoned) => {
                    let (next, _) = poisoned.into_inner();
                    state = next;
                }
            }
        }
    }

    #[cfg(test)]
    fn lifecycle_for_test(&self) -> Arc<ResourceLifecycle> {
        Arc::clone(&self.shared.lifecycle)
    }

    #[cfg(test)]
    fn weak_owner_for_test(&self) -> std::sync::Weak<SharedRequest> {
        Arc::downgrade(&self.shared)
    }

    #[cfg(test)]
    fn request_id_for_test(&self) -> u64 {
        self.shared.id
    }
}

impl NativeIoCompletion {
    pub(super) fn terminal(&self) -> NativeIoTerminal {
        self.terminal
    }

    pub(super) fn resources_released(&self) -> bool {
        self.lifecycle.request_released.load(Ordering::Acquire)
            && self.lifecycle.buffer_released.load(Ordering::Acquire)
            && self.lifecycle.event_closed.load(Ordering::Acquire)
    }

    pub(super) fn take_read_buffer(&mut self) -> Option<Box<[u8]>> {
        let buffer = self.read_buffer.take();
        if buffer.is_some() {
            self.lifecycle
                .buffer_released
                .store(true, Ordering::Release);
        }
        buffer
    }

    pub(super) fn cancel_disposition(&self) -> Option<CancelDisposition> {
        self.cancel
    }

    pub(super) fn cancel_api_attempts(&self) -> u32 {
        self.cancel_api_attempts
    }
}

impl Drop for NativeIoCompletion {
    fn drop(&mut self) {
        if let Some(buffer) = self.read_buffer.take() {
            drop(buffer);
            self.lifecycle
                .buffer_released
                .store(true, Ordering::Release);
        }
    }
}

fn run_request(
    shared: Arc<SharedRequest>,
    request: IoRequest,
    before_admission: impl FnOnce(),
    before_issue: impl FnOnce(),
) -> NativeIoCompletion {
    before_admission();
    let (offset, is_read, buffer) = match request {
        IoRequest::Read { offset, length } => (offset, true, vec![0; length].into_boxed_slice()),
        IoRequest::Write { offset, bytes } => (offset, false, bytes),
    };
    let event = match OwnedEvent::create(Arc::clone(&shared.lifecycle)) {
        Ok(event) => event,
        Err(error) => {
            return complete_request(
                shared,
                failure_terminal("CreateEventW(archive I/O)", &error),
                None,
            );
        }
    };
    shared
        .lifecycle
        .request_released
        .store(false, Ordering::Release);
    shared
        .lifecycle
        .buffer_released
        .store(false, Ordering::Release);
    let overlapped = OVERLAPPED {
        hEvent: event.handle,
        Anonymous: OVERLAPPED_0 {
            Anonymous: OVERLAPPED_0_0 {
                Offset: offset as u32,
                OffsetHigh: (offset >> 32) as u32,
            },
        },
        ..Default::default()
    };
    let mut storage = Box::pin(RequestStorage {
        overlapped,
        buffer: Some(buffer),
        event,
        lifecycle: Arc::clone(&shared.lifecycle),
        _pin: PhantomPinned,
    });
    let storage_ptr =
        unsafe { Pin::as_mut(&mut storage).get_unchecked_mut() as *mut RequestStorage };
    let overlapped_ptr = unsafe { std::ptr::addr_of_mut!((*storage_ptr).overlapped) };
    let buffer_ptr = unsafe {
        (*storage_ptr)
            .buffer
            .as_mut()
            .map_or(std::ptr::null_mut(), |buffer| buffer.as_mut_ptr())
    };
    let buffer_length = unsafe {
        (*storage_ptr)
            .buffer
            .as_ref()
            .map_or(0, |buffer| buffer.len())
    };
    if buffer_ptr.is_null() || buffer_length == 0 {
        drop(storage);
        return complete_request(
            shared,
            NativeIoTerminal::Failed {
                operation: "validate owned native I/O buffer",
                hresult: 0x8007_0057_u32 as i32,
                win32_code: Some(87),
            },
            None,
        );
    }

    let file_handle = HANDLE(shared.file.as_raw_handle() as _);
    let mut pending_guard = PendingIoDrain {
        file_handle,
        overlapped: overlapped_ptr,
        active: false,
    };
    let issue = {
        let mut state = lock_state(&shared.state);
        if state.cancel_requested {
            state.phase = Phase::Terminal;
            state.overlapped_address = 0;
            shared.changed.notify_all();
            drop(state);
            drop(pending_guard);
            drop(storage);
            return complete_request(shared, NativeIoTerminal::CancelledBeforeSubmission, None);
        }
        state.phase = Phase::Submitting;
        state.overlapped_address = overlapped_ptr as usize;
        before_issue();
        let buffer = unsafe { std::slice::from_raw_parts_mut(buffer_ptr, buffer_length) };
        let result = unsafe {
            if is_read {
                ReadFile(file_handle, Some(buffer), None, Some(overlapped_ptr))
            } else {
                WriteFile(file_handle, Some(buffer), None, Some(overlapped_ptr))
            }
        };
        match result {
            Ok(()) => {
                state.phase = Phase::Completing;
                state.overlapped_address = 0;
                shared.changed.notify_all();
                Ok(false)
            }
            Err(error) if is_win32_error(&error, ERROR_IO_PENDING.0) => {
                pending_guard.active = true;
                state.phase = Phase::Pending;
                shared.changed.notify_all();
                Ok(true)
            }
            Err(error) => {
                state.phase = Phase::Terminal;
                state.overlapped_address = 0;
                shared.changed.notify_all();
                Err(failure_terminal(
                    if is_read {
                        "ReadFile(overlapped)"
                    } else {
                        "WriteFile(overlapped)"
                    },
                    &error,
                ))
            }
        }
    };

    let pending = match issue {
        Ok(pending) => pending,
        Err(terminal) => {
            drop(pending_guard);
            drop(storage);
            return complete_request(shared, terminal, None);
        }
    };
    let mut transferred = 0_u32;
    let completion =
        unsafe { GetOverlappedResult(file_handle, overlapped_ptr, &mut transferred, BOOL(1)) };
    pending_guard.active = false;
    let terminal = match completion {
        Ok(()) => NativeIoTerminal::Completed { transferred },
        Err(error) => failure_terminal(
            if pending {
                if is_read {
                    "GetOverlappedResult(ReadFile)"
                } else {
                    "GetOverlappedResult(WriteFile)"
                }
            } else if is_read {
                "GetOverlappedResult(immediate ReadFile)"
            } else {
                "GetOverlappedResult(immediate WriteFile)"
            },
            &error,
        ),
    };
    {
        let mut state = lock_state(&shared.state);
        state.phase = Phase::Terminal;
        state.overlapped_address = 0;
        shared.changed.notify_all();
    }
    let read_buffer = if is_read && matches!(terminal, NativeIoTerminal::Completed { .. }) {
        unsafe { (*storage_ptr).buffer.take() }
    } else {
        None
    };
    drop(storage);
    complete_request(shared, terminal, read_buffer)
}

fn complete_request(
    shared: Arc<SharedRequest>,
    terminal: NativeIoTerminal,
    read_buffer: Option<Box<[u8]>>,
) -> NativeIoCompletion {
    let (cancel, cancel_api_attempts) = {
        let mut state = lock_state(&shared.state);
        state.phase = Phase::Terminal;
        state.overlapped_address = 0;
        shared.changed.notify_all();
        (state.cancel_disposition, state.cancel_api_attempts)
    };
    let lifecycle = Arc::clone(&shared.lifecycle);
    let request_id = shared.id;
    drop(shared);
    NativeIoCompletion {
        request_id,
        terminal,
        cancel,
        cancel_api_attempts,
        read_buffer,
        lifecycle,
    }
}

struct PendingIoDrain {
    file_handle: HANDLE,
    overlapped: *const OVERLAPPED,
    active: bool,
}

impl Drop for PendingIoDrain {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        // SAFETY: this guard is armed only after ReadFile/WriteFile reports
        // ERROR_IO_PENDING. It keeps the pinned request and buffer alive while
        // it cancels and drains that exact OVERLAPPED during unwind.
        let _ = unsafe { CancelIoEx(self.file_handle, Some(self.overlapped)) };
        let mut transferred = 0_u32;
        let _ = unsafe {
            GetOverlappedResult(self.file_handle, self.overlapped, &mut transferred, BOOL(1))
        };
    }
}

fn failure_terminal(operation: &'static str, error: &windows::core::Error) -> NativeIoTerminal {
    NativeIoTerminal::Failed {
        operation,
        hresult: error.code().0,
        win32_code: win32_code(error),
    }
}

fn win32_code(error: &windows::core::Error) -> Option<u32> {
    let hresult = error.code().0 as u32;
    (hresult & 0xffff_0000 == 0x8007_0000).then_some(hresult & 0xffff)
}

fn is_win32_error(error: &windows::core::Error, code: u32) -> bool {
    error.code().0 == hresult_from_win32(code)
}

const fn hresult_from_win32(code: u32) -> i32 {
    if code == 0 {
        0
    } else {
        ((code & 0xffff) | 0x8007_0000) as i32
    }
}

fn lock_state(mutex: &Mutex<RequestState>) -> MutexGuard<'_, RequestState> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::sync::mpsc::{self, Receiver, SyncSender};
    use std::thread::{self, JoinHandle};

    use windows::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, GENERIC_WRITE,
        INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, OPEN_EXISTING,
        PIPE_ACCESS_INBOUND,
    };
    use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(1);
    const PENDING_TIMEOUT: Duration = Duration::from_secs(5);

    /// Test-only owner mirroring the later extraction worker's obligation:
    /// publish cancellation before submission, and join before retiring the
    /// request owner. The production request core itself creates no thread.
    struct JoinedTestWorker {
        cancel: Option<NativeIoCancelHandle>,
        owner: std::sync::Weak<SharedRequest>,
        release_gate: Option<SyncSender<()>>,
        worker: Option<JoinHandle<NativeIoCompletion>>,
    }

    struct JoinedTestRetirement {
        completion: NativeIoCompletion,
        owner_released: bool,
        worker_joined: bool,
    }

    struct ReleaseGateOnDrop(Option<SyncSender<()>>);

    impl ReleaseGateOnDrop {
        fn new(sender: SyncSender<()>) -> Self {
            Self(Some(sender))
        }

        fn disarm(&mut self) {
            drop(self.0.take());
        }
    }

    impl Drop for ReleaseGateOnDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.try_send(());
            }
        }
    }

    impl JoinedTestWorker {
        fn start(
            request: NativeIoRequest,
            cancel: NativeIoCancelHandle,
            release_gate: Option<SyncSender<()>>,
            before_admission: impl FnOnce() + Send + 'static,
            before_issue: impl FnOnce() + Send + 'static,
        ) -> Self {
            let owner = cancel.weak_owner_for_test();
            let worker = thread::Builder::new()
                .name("archive-native-io-test".to_owned())
                .spawn(move || request.execute_with_gates(before_admission, before_issue))
                .expect("spawn owned test worker");
            Self {
                cancel: Some(cancel),
                owner,
                release_gate,
                worker: Some(worker),
            }
        }

        fn cancel(&self) -> CancelReceipt {
            self.cancel
                .as_ref()
                .expect("live test cancellation owner")
                .cancel()
        }

        fn wait_until_pending(&self, timeout: Duration) -> bool {
            self.cancel
                .as_ref()
                .expect("live test cancellation owner")
                .wait_until_pending(timeout)
        }

        fn join(mut self) -> JoinedTestRetirement {
            let worker = self.worker.take().expect("test worker joined once");
            let completion = worker.join().expect("native I/O test worker completes");
            drop(self.cancel.take());
            let owner_released = self.owner.upgrade().is_none();
            JoinedTestRetirement {
                completion,
                owner_released,
                worker_joined: true,
            }
        }
    }

    impl Drop for JoinedTestWorker {
        fn drop(&mut self) {
            if let Some(release_gate) = self.release_gate.take() {
                let _ = release_gate.try_send(());
            }
            if let Some(worker) = self.worker.take() {
                if let Some(cancel) = self.cancel.as_ref() {
                    cancel.cancel();
                }
                // Never detach, including on an assertion-unwind path.
                let _ = worker.join();
            }
            drop(self.cancel.take());
        }
    }

    impl JoinedTestRetirement {
        fn terminal(&self) -> NativeIoTerminal {
            self.completion.terminal()
        }

        fn worker_joined(&self) -> bool {
            self.worker_joined
        }

        fn resources_released(&self) -> bool {
            self.completion.resources_released() && self.owner_released
        }

        fn cancel_disposition(&self) -> Option<CancelDisposition> {
            self.completion.cancel_disposition()
        }

        fn cancel_api_attempts(&self) -> u32 {
            self.completion.cancel_api_attempts()
        }
    }

    struct ConnectedPipe {
        name: Vec<u16>,
        server: Option<File>,
        client: Option<File>,
    }

    fn connected_pipe() -> io::Result<ConnectedPipe> {
        let pipe_name = format!(
            r"\\.\pipe\multi-launcher-archive-io-{}-{}",
            std::process::id(),
            NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed)
        );
        let name: Vec<u16> = std::ffi::OsStr::new(&pipe_name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let server_handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                4096,
                4096,
                0,
                None,
            )
        };
        if server_handle == INVALID_HANDLE_VALUE {
            return Err(last_os_error());
        }
        // SAFETY: CreateNamedPipeW returned a new owned pipe handle.
        let server = unsafe { File::from_raw_handle(server_handle.0 as RawHandle) };
        let connect_lifecycle = Arc::new(ResourceLifecycle::default());
        let event = OwnedEvent::create(Arc::clone(&connect_lifecycle)).map_err(io_from_win32)?;
        let mut overlapped = Box::pin(OVERLAPPED {
            hEvent: event.handle,
            ..Default::default()
        });
        let overlapped_ptr = Pin::as_mut(&mut overlapped).get_mut() as *mut OVERLAPPED;
        let connect_result = unsafe { ConnectNamedPipe(server_handle, Some(overlapped_ptr)) };
        let mut connect_drain = PendingIoDrain {
            file_handle: server_handle,
            overlapped: overlapped_ptr,
            active: false,
        };
        let connect_pending = match connect_result {
            Ok(()) => false,
            Err(error) if is_win32_error(&error, ERROR_IO_PENDING.0) => {
                connect_drain.active = true;
                true
            }
            Err(error) if is_win32_error(&error, ERROR_PIPE_CONNECTED.0) => false,
            Err(error) => return Err(io_from_win32(error)),
        };
        let client_handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        }
        .map_err(io_from_win32)?;
        // SAFETY: CreateFileW returned a new owned client pipe handle.
        let client = unsafe { File::from_raw_handle(client_handle.0 as RawHandle) };
        if connect_pending {
            let mut transferred = 0_u32;
            unsafe {
                GetOverlappedResult(server_handle, overlapped_ptr, &mut transferred, BOOL(1))
            }
            .map_err(io_from_win32)?;
            connect_drain.active = false;
        }
        drop(overlapped);
        drop(event);
        Ok(ConnectedPipe {
            name,
            server: Some(server),
            client: Some(client),
        })
    }

    fn last_os_error() -> io::Error {
        io::Error::last_os_error()
    }

    fn io_from_win32(error: windows::core::Error) -> io::Error {
        io::Error::from_raw_os_error(win32_code(&error).unwrap_or(error.code().0 as u32) as i32)
    }

    fn assert_pipe_name_retired(name: &[u16]) {
        let reopened = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_WRITE.0,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                HANDLE::default(),
            )
        };
        match reopened {
            Err(error) => assert!(is_win32_error(&error, ERROR_FILE_NOT_FOUND.0)),
            Ok(handle) => {
                // SAFETY: the unexpected handle is closed immediately so the
                // failed assertion cannot leak the test pipe instance.
                let unexpected = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
                drop(unexpected);
                panic!("owned native pipe remained open after request retirement");
            }
        }
    }

    fn wait_channel(receiver: &Receiver<()>, what: &str) {
        receiver
            .recv_timeout(PENDING_TIMEOUT)
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    fn release_channel(sender: &SyncSender<()>) {
        sender.send(()).expect("release test gate");
    }

    fn assert_lifecycle_released(lifecycle: &ResourceLifecycle) {
        assert!(lifecycle.request_released.load(Ordering::Acquire));
        assert!(lifecycle.buffer_released.load(Ordering::Acquire));
        assert!(lifecycle.event_closed.load(Ordering::Acquire));
    }

    #[test]
    fn pending_read_cancel_drains_native_result_and_releases_owned_resources() {
        let mut pipe = connected_pipe().expect("create connected overlapped pipe fixture");
        let server = pipe.server.take().expect("take server handle");
        let verified_file = VerifiedOverlappedFile::verify(server)
            .expect("verify the fixture's overlapped server handle");
        let (request, cancel) = NativeIoRequest::read(&verified_file, 0, 64)
            .expect("prepare one bounded overlapped read");
        drop(verified_file);
        let lifecycle = cancel.lifecycle_for_test();
        let weak_owner = cancel.weak_owner_for_test();
        let request_id = cancel.request_id_for_test();
        let operation = JoinedTestWorker::start(request, cancel, None, || {}, || {});
        assert!(operation.wait_until_pending(PENDING_TIMEOUT));

        let first_cancel = operation.cancel();
        let repeated_cancel = operation.cancel();
        assert_eq!(first_cancel.disposition(), CancelDisposition::Requested);
        assert_eq!(first_cancel.api_attempts(), 1);
        assert_eq!(repeated_cancel.disposition(), first_cancel.disposition());
        assert_eq!(repeated_cancel.api_attempts(), 1);

        let retirement = operation.join();
        assert_eq!(retirement.completion.request_id, request_id);
        assert!(retirement.worker_joined());
        assert_eq!(
            retirement.terminal(),
            NativeIoTerminal::Failed {
                operation: "GetOverlappedResult(ReadFile)",
                hresult: hresult_from_win32(ERROR_OPERATION_ABORTED.0),
                win32_code: Some(ERROR_OPERATION_ABORTED.0),
            }
        );
        assert_eq!(
            retirement.cancel_disposition(),
            Some(CancelDisposition::Requested)
        );
        assert_eq!(retirement.cancel_api_attempts(), 1);
        assert!(retirement.resources_released());
        assert_lifecycle_released(&lifecycle);
        assert!(weak_owner.upgrade().is_none());

        drop(retirement);
        drop(pipe.client.take());
        assert_pipe_name_retired(&pipe.name);
    }

    #[test]
    fn cancel_before_submission_and_restart_retire_the_old_request() {
        let mut first_pipe = connected_pipe().expect("create first pipe fixture");
        let server = first_pipe.server.take().expect("take first server handle");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let verified_file = VerifiedOverlappedFile::verify(server)
            .expect("verify the fixture's overlapped server handle");
        let (request, cancel) = NativeIoRequest::read(&verified_file, 0, 32)
            .expect("prepare gated pre-admission request");
        drop(verified_file);
        let first = JoinedTestWorker::start(
            request,
            cancel,
            Some(release_tx.clone()),
            move || {
                entered_tx.send(()).expect("signal pre-admission gate");
                release_rx.recv().expect("wait at pre-admission gate");
            },
            || {},
        );
        let first_lifecycle = first
            .cancel
            .as_ref()
            .expect("first cancellation owner")
            .lifecycle_for_test();
        let first_owner = first.owner.clone();
        let first_id = first
            .cancel
            .as_ref()
            .expect("first cancellation owner")
            .request_id_for_test();
        wait_channel(&entered_rx, "pre-admission gate");
        let cancellation = first.cancel();
        assert_eq!(
            cancellation.disposition(),
            CancelDisposition::BeforeSubmission
        );
        assert_eq!(cancellation.api_attempts(), 0);
        release_channel(&release_tx);
        let first_retirement = first.join();
        assert_eq!(
            first_retirement.terminal(),
            NativeIoTerminal::CancelledBeforeSubmission
        );
        assert!(first_retirement.worker_joined());
        assert!(first_retirement.resources_released());
        assert_lifecycle_released(&first_lifecycle);
        assert!(first_owner.upgrade().is_none());
        drop(first_retirement);
        drop(first_pipe.client.take());
        assert_pipe_name_retired(&first_pipe.name);

        let mut second_pipe = connected_pipe().expect("create restart pipe fixture");
        let second_server = second_pipe
            .server
            .take()
            .expect("take restart server handle");
        let second_verified_file = VerifiedOverlappedFile::verify(second_server)
            .expect("verify the restart fixture's overlapped handle");
        let (second_request, second_cancel) = NativeIoRequest::read(&second_verified_file, 0, 32)
            .expect("prepare a fresh request after retirement");
        drop(second_verified_file);
        assert_ne!(second_cancel.request_id_for_test(), first_id);
        let second = JoinedTestWorker::start(second_request, second_cancel, None, || {}, || {});
        assert!(second.wait_until_pending(PENDING_TIMEOUT));
        second.cancel();
        let second_retirement = second.join();
        assert!(second_retirement.worker_joined());
        assert!(second_retirement.resources_released());
        drop(second_retirement);
        drop(second_pipe.client.take());
        assert_pipe_name_retired(&second_pipe.name);
    }

    #[test]
    fn cancel_during_submission_waits_for_admission_then_cancels_pending_io() {
        let mut pipe = connected_pipe().expect("create admission-race pipe fixture");
        let server = pipe.server.take().expect("take server handle");
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let verified_file = VerifiedOverlappedFile::verify(server)
            .expect("verify the fixture's overlapped server handle");
        let (request, cancel) =
            NativeIoRequest::read(&verified_file, 0, 32).expect("prepare gated issue request");
        drop(verified_file);
        let operation = JoinedTestWorker::start(
            request,
            cancel,
            Some(release_tx.clone()),
            || {},
            move || {
                entered_tx.send(()).expect("signal final issue boundary");
                release_rx.recv().expect("wait at final issue boundary");
            },
        );
        let lifecycle = operation
            .cancel
            .as_ref()
            .expect("issue cancellation owner")
            .lifecycle_for_test();
        let weak_owner = operation.owner.clone();
        wait_channel(&entered_rx, "final issue boundary");

        let (cancel_waiting_tx, cancel_waiting_rx) = mpsc::sync_channel(1);
        let cancel_handle = operation
            .cancel
            .as_ref()
            .expect("issue cancellation owner")
            .clone();
        let cancel_result = thread::scope(|scope| {
            let mut release_on_unwind = ReleaseGateOnDrop::new(release_tx.clone());
            let cancel_thread = scope.spawn(move || {
                cancel_handle.cancel_with_contention_callback(|| {
                    cancel_waiting_tx
                        .send(())
                        .expect("signal cancellation contending with issue lock");
                })
            });
            wait_channel(&cancel_waiting_rx, "cancel/issue lock contention");
            release_channel(&release_tx);
            release_on_unwind.disarm();
            cancel_thread.join().expect("join cancellation caller")
        });
        assert!(cancel_result.submission_lock_was_contended);
        assert_eq!(cancel_result.disposition(), CancelDisposition::Requested);
        assert_eq!(cancel_result.api_attempts(), 1);

        let retirement = operation.join();
        assert_eq!(
            retirement.terminal(),
            NativeIoTerminal::Failed {
                operation: "GetOverlappedResult(ReadFile)",
                hresult: hresult_from_win32(ERROR_OPERATION_ABORTED.0),
                win32_code: Some(ERROR_OPERATION_ABORTED.0),
            }
        );
        assert!(retirement.worker_joined());
        assert!(retirement.resources_released());
        assert_lifecycle_released(&lifecycle);
        assert!(weak_owner.upgrade().is_none());
        drop(retirement);
        drop(pipe.client.take());
        assert_pipe_name_retired(&pipe.name);
    }

    #[test]
    fn dropping_pending_operation_cancels_and_joins_instead_of_detaching() {
        let mut pipe = connected_pipe().expect("create drop-path pipe fixture");
        let server = pipe.server.take().expect("take server handle");
        let verified_file = VerifiedOverlappedFile::verify(server)
            .expect("verify the fixture's overlapped server handle");
        let (request, cancel) =
            NativeIoRequest::read(&verified_file, 0, 32).expect("prepare drop-path pending read");
        drop(verified_file);
        let lifecycle = cancel.lifecycle_for_test();
        let weak_owner = cancel.weak_owner_for_test();
        let operation = JoinedTestWorker::start(request, cancel, None, || {}, || {});
        assert!(operation.wait_until_pending(PENDING_TIMEOUT));
        drop(operation);
        assert_lifecycle_released(&lifecycle);
        assert!(weak_owner.upgrade().is_none());
        drop(pipe.client.take());
        assert_pipe_name_retired(&pipe.name);
    }

    #[test]
    fn synchronous_file_handle_is_rejected_before_request_submission() {
        let directory = tempfile::tempdir().expect("create owned synchronous-file directory");
        let fixture = tempfile::NamedTempFile::new_in(directory.path())
            .expect("create owned synchronous-file fixture");
        let file = fixture.reopen().expect("open synchronous fixture handle");
        let result = VerifiedOverlappedFile::verify(file);
        match result {
            Err(NativeIoStartError::SynchronousHandle { mode }) => {
                let synchronous_modes =
                    FILE_SYNCHRONOUS_IO_ALERT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0;
                assert_ne!(mode & synchronous_modes, 0);
            }
            Err(error) => panic!("synchronous file mode query failed: {error:?}"),
            Ok(_) => panic!("synchronous file handle passed overlapped verification"),
        }
    }
}
