use super::*;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, LPARAM, RECT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Pipes::{GetNamedPipeInfo, PeekNamedPipe};
use windows::Win32::System::Threading::{CREATE_NO_WINDOW, TerminateProcess, WaitForSingleObject};
use windows::core::PCWSTR;

const PROTOCOL_VERSION: u16 = 1;
const MAX_REQUEST_FRAME_BYTES: usize = 2 * 1024;
const MIN_REQUEST_PIPE_CAPACITY_BYTES: usize = MAX_REQUEST_FRAME_BYTES + 4;
const MAX_REPORTED_PIPE_CAPACITY_BYTES: usize = 1024 * 1024;
const FORCE_EXIT_CODE: u32 = 0xB17E;

pub struct WindowsHelperFactory;

impl HelperFactory for WindowsHelperFactory {
    fn start(
        &mut self,
        session: BrightnessSessionId,
        token: String,
        deadline: Instant,
    ) -> Result<Box<dyn HelperSession>, BrightnessFailure> {
        let executable = std::env::current_exe()
            .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::HelperStartFailure))?;
        let job = create_kill_on_close_job()?;
        let mut child = Command::new(executable)
            .arg("--internal-brightness-helper")
            .arg(&token)
            .arg(format!("{:032x}", session.0))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::HelperStartFailure))?;
        let process_handle = raw_process_handle(&child);
        if let Err(error) = unsafe { AssignProcessToJobObject(job.0, process_handle) } {
            let cleanup = terminate_started_process(
                &mut child,
                Some(job),
                false,
                Instant::now() + HELPER_CLEANUP_DEADLINE,
            );
            return Err(start_failure(error.code().0 as u32, cleanup));
        }
        let Some(stdin) = child.stdin.take() else {
            let cleanup = terminate_started_process(
                &mut child,
                Some(job),
                true,
                Instant::now() + HELPER_CLEANUP_DEADLINE,
            );
            return Err(start_failure(0, cleanup));
        };
        let Some(stdout) = child.stdout.take() else {
            let cleanup = terminate_started_process(
                &mut child,
                Some(job),
                true,
                Instant::now() + HELPER_CLEANUP_DEADLINE,
            );
            return Err(start_failure(0, cleanup));
        };
        let mut session = WindowsHelperSession {
            child,
            stdin,
            stdout,
            job: Some(job),
            token,
            session,
            last_generation: 0,
            poisoned: false,
            request_pipe_empty: true,
            request_pipe_capacity: 0,
            cleanup_disposition: None,
        };
        let hello = match session.read_envelope(deadline, None) {
            Ok(hello) => hello,
            Err(failure) => {
                session.poisoned = true;
                let failure =
                    match session.force_terminate(Instant::now() + HELPER_CLEANUP_DEADLINE) {
                        Ok(disposition) => BrightnessFailure {
                            process_cleanup: Some(disposition),
                            ..failure
                        },
                        Err(cleanup_failure) => cleanup_failure,
                    };
                return Err(failure);
            }
        };
        let request_pipe_capacity = match &hello.body {
            Message::Hello {
                request_pipe_capacity,
            } => usize::try_from(*request_pipe_capacity).unwrap_or_default(),
            _ => 0,
        };
        if !envelope_matches(
            &hello,
            &session.token,
            session.session,
            BrightnessRequestId(0),
            0,
        ) || !(MIN_REQUEST_PIPE_CAPACITY_BYTES..=MAX_REPORTED_PIPE_CAPACITY_BYTES)
            .contains(&request_pipe_capacity)
        {
            session.poisoned = true;
            let mut failure = BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure);
            if let Err(cleanup_failure) =
                session.force_terminate(Instant::now() + HELPER_CLEANUP_DEADLINE)
            {
                failure = cleanup_failure;
            } else {
                failure.process_cleanup = session.cleanup_disposition;
            }
            return Err(failure);
        }
        session.request_pipe_capacity = request_pipe_capacity;
        Ok(Box::new(session))
    }
}

struct OwnedJob(HANDLE);

// SAFETY: this handle is uniquely owned by the service worker and is only
// accessed through that worker after ownership moves into its session.
unsafe impl Send for OwnedJob {}

impl Drop for OwnedJob {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
            self.0 = HANDLE::default();
        }
    }
}

fn create_kill_on_close_job() -> Result<OwnedJob, BrightnessFailure> {
    let handle =
        unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(|error| BrightnessFailure {
            kind: BrightnessFailureKind::HelperStartFailure,
            operation: None,
            native_code: Some(error.code().0 as u32),
            process_cleanup: None,
            set_disposition: None,
            set_cause: None,
        })?;
    let job = OwnedJob(handle);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|error| BrightnessFailure {
        kind: BrightnessFailureKind::HelperStartFailure,
        operation: None,
        native_code: Some(error.code().0 as u32),
        process_cleanup: None,
        set_disposition: None,
        set_cause: None,
    })?;
    Ok(job)
}

fn raw_process_handle(child: &Child) -> HANDLE {
    HANDLE(child.as_raw_handle())
}

struct WindowsHelperSession {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    job: Option<OwnedJob>,
    token: String,
    session: BrightnessSessionId,
    last_generation: u64,
    poisoned: bool,
    request_pipe_empty: bool,
    request_pipe_capacity: usize,
    cleanup_disposition: Option<HelperCleanupDisposition>,
}

impl HelperSession for WindowsHelperSession {
    fn session_id(&self) -> BrightnessSessionId {
        self.session
    }

    fn refresh(
        &mut self,
        request_id: BrightnessRequestId,
        generation: u64,
        cancellation: &BrightnessCancellation,
        deadline: Instant,
    ) -> Result<MonitorInventory, BrightnessFailure> {
        if self.poisoned || generation <= self.last_generation {
            return Err(BrightnessFailure::new(
                BrightnessFailureKind::ProtocolFailure,
            ));
        }
        let envelope = self
            .exchange(
                request_id,
                generation,
                Message::Refresh,
                cancellation,
                deadline,
            )
            .map_err(|failure| failure.failure)?;
        match envelope.body {
            Message::Refreshed(Ok(inventory))
                if inventory.session == self.session && inventory.generation == generation =>
            {
                self.last_generation = generation;
                Ok(inventory)
            }
            Message::Refreshed(Err(failure)) => Err(failure),
            _ => {
                self.poisoned = true;
                Err(BrightnessFailure::new(
                    BrightnessFailureKind::ProtocolFailure,
                ))
            }
        }
    }

    fn set(
        &mut self,
        request_id: BrightnessRequestId,
        target: &MonitorTarget,
        percent: u8,
        cancellation: &BrightnessCancellation,
        deadline: Instant,
    ) -> Result<BrightnessSetAcknowledgement, BrightnessFailure> {
        if self.poisoned
            || target.session != self.session
            || target.generation != self.last_generation
        {
            return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget)
                .with_set_disposition(BrightnessSetDisposition::NotAttempted));
        }
        let envelope = self
            .exchange(
                request_id,
                target.generation,
                Message::Set {
                    target: target.clone(),
                    percent,
                },
                cancellation,
                deadline,
            )
            .map_err(|failure| {
                failure
                    .failure
                    .with_set_disposition(if failure.request_may_have_been_dispatched {
                        BrightnessSetDisposition::Unknown
                    } else {
                        BrightnessSetDisposition::NotAttempted
                    })
            })?;
        match envelope.body {
            Message::SetResult(Ok(ack))
                if ack.target == *target && ack.requested_percent == percent =>
            {
                Ok(ack)
            }
            Message::SetResult(Err(failure)) => {
                let disposition = failure
                    .set_disposition
                    .unwrap_or(BrightnessSetDisposition::Unknown);
                Err(failure.with_set_disposition(disposition))
            }
            _ => {
                self.poisoned = true;
                Err(
                    BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure)
                        .with_set_disposition(BrightnessSetDisposition::Unknown),
                )
            }
        }
    }

    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> Result<HelperCleanupDisposition, BrightnessFailure> {
        if self.child.try_wait().ok().flatten().is_some() {
            self.job.take();
            return Ok(self
                .cleanup_disposition
                .unwrap_or(HelperCleanupDisposition::ProcessExitVerified));
        }
        if !self.poisoned {
            let cancellation = BrightnessCancellation::new();
            let now = Instant::now();
            let graceful_deadline = now + deadline.saturating_duration_since(now) / 2;
            let envelope = self.exchange(
                BrightnessRequestId(u64::MAX),
                self.last_generation,
                Message::Shutdown,
                &cancellation,
                graceful_deadline,
            );
            if envelope.is_ok_and(|reply| matches!(reply.body, Message::ShutdownAck))
                && wait_child_exit(
                    &mut self.child,
                    deadline.saturating_duration_since(Instant::now()),
                )
            {
                self.job.take();
                return Ok(HelperCleanupDisposition::NormalExitVerified);
            }
        }
        self.force_terminate(deadline)
    }
}

impl WindowsHelperSession {
    fn exchange(
        &mut self,
        request_id: BrightnessRequestId,
        generation: u64,
        body: Message,
        cancellation: &BrightnessCancellation,
        deadline: Instant,
    ) -> Result<Envelope, HelperExchangeFailure> {
        if cancellation.is_cancelled() {
            self.poisoned = true;
            return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled).into());
        }
        if !self.request_pipe_empty {
            self.poisoned = true;
            return Err(BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure).into());
        }
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            token: self.token.clone(),
            session: self.session,
            request_id,
            generation,
            body,
        };
        let frame = encode_protocol_frame(&envelope, MAX_REQUEST_FRAME_BYTES)?;
        if frame.len() > self.request_pipe_capacity {
            return Err(BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure).into());
        }
        // The parent has one writer and sends only after receiving the prior
        // response. The child cannot respond until read_exact consumed this
        // complete frame, so the measured empty pipe can accept it atomically
        // within its reported outbound buffer capacity.
        self.request_pipe_empty = false;
        if self
            .stdin
            .write_all(&frame)
            .and_then(|_| self.stdin.flush())
            .is_err()
        {
            self.poisoned = true;
            return Err(HelperExchangeFailure::dispatched(BrightnessFailure::new(
                BrightnessFailureKind::ProtocolFailure,
            )));
        }
        let response = self
            .read_envelope(deadline, Some(cancellation))
            .map_err(HelperExchangeFailure::dispatched)?;
        if envelope_matches(&response, &self.token, self.session, request_id, generation) {
            // Producing a response requires the helper to consume the whole
            // request frame before dispatching it.
            self.request_pipe_empty = true;
            Ok(response)
        } else {
            self.poisoned = true;
            let _ = self.force_terminate(Instant::now() + HELPER_CLEANUP_DEADLINE);
            Err(HelperExchangeFailure::dispatched(BrightnessFailure::new(
                BrightnessFailureKind::ProtocolFailure,
            )))
        }
    }

    fn read_envelope(
        &mut self,
        deadline: Instant,
        cancellation: Option<&BrightnessCancellation>,
    ) -> Result<Envelope, BrightnessFailure> {
        let mut accumulator = ProtocolFrameAccumulator::default();
        loop {
            if cancellation.is_some_and(BrightnessCancellation::is_cancelled) {
                return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
            }
            if Instant::now() >= deadline {
                return Err(BrightnessFailure::new(BrightnessFailureKind::HelperTimeout));
            }
            let mut available = 0u32;
            match unsafe {
                PeekNamedPipe(
                    HANDLE(self.stdout.as_raw_handle()),
                    None,
                    0,
                    None,
                    Some(&mut available as *mut _),
                    None,
                )
            } {
                Ok(()) if available == 0 => {}
                Ok(()) => {
                    let mut buffer = [0u8; 8192];
                    let can_read = (available as usize).min(buffer.len());
                    let read = self.stdout.read(&mut buffer[..can_read]).map_err(|_| {
                        BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure)
                    })?;
                    if read == 0 {
                        return Err(BrightnessFailure::new(
                            BrightnessFailureKind::ProtocolFailure,
                        ));
                    }
                    match accumulator.push(&buffer[..read]) {
                        Ok(Some(envelope)) => return Ok(envelope),
                        Ok(None) => {}
                        Err(failure) => {
                            self.poisoned = true;
                            return Err(failure);
                        }
                    }
                }
                Err(error) => {
                    let code = error.code().0 as u32;
                    if code == 0x8007_03e3 || self.child.try_wait().ok().flatten().is_some() {
                        return Err(BrightnessFailure::new(
                            BrightnessFailureKind::ProtocolFailure,
                        ));
                    }
                    return Err(BrightnessFailure {
                        kind: BrightnessFailureKind::ProtocolFailure,
                        operation: None,
                        native_code: Some(code),
                        process_cleanup: None,
                        set_disposition: None,
                        set_cause: None,
                    });
                }
            }
            if self.child.try_wait().ok().flatten().is_some() {
                return Err(BrightnessFailure::new(
                    BrightnessFailureKind::ProtocolFailure,
                ));
            }
            thread::sleep(
                CHILD_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn force_terminate(
        &mut self,
        deadline: Instant,
    ) -> Result<HelperCleanupDisposition, BrightnessFailure> {
        if self.child.try_wait().ok().flatten().is_some() {
            self.job.take();
            self.poisoned = true;
            let disposition = self
                .cleanup_disposition
                .unwrap_or(HelperCleanupDisposition::ProcessExitVerified);
            self.cleanup_disposition = Some(disposition);
            return Ok(disposition);
        }
        let process = raw_process_handle(&self.child);
        let mut termination_code = None;
        let total = deadline.saturating_duration_since(Instant::now());
        let mut wait_slice = total / 2;
        let terminate_job = self
            .job
            .as_ref()
            .map(|job| unsafe { TerminateJobObject(job.0, FORCE_EXIT_CODE) });
        if let Some(Err(error)) = terminate_job {
            termination_code = Some(error.code().0 as u32);
            self.job.take();
            wait_slice = total / 2;
        }
        let mut exited = wait_child_exit(&mut self.child, wait_slice);
        if !exited {
            // Closing the kill-on-close job is an independent owned fallback.
            self.job.take();
            exited = wait_child_exit(
                &mut self.child,
                deadline.saturating_duration_since(Instant::now()) / 2,
            );
        }
        if !exited {
            if let Err(error) = unsafe { TerminateProcess(process, FORCE_EXIT_CODE) } {
                termination_code.get_or_insert(error.code().0 as u32);
            }
            exited = wait_child_exit(
                &mut self.child,
                deadline.saturating_duration_since(Instant::now()),
            );
        }
        if !exited {
            self.job.take();
            self.poisoned = true;
            self.cleanup_disposition = Some(HelperCleanupDisposition::ExitUnverified);
            return Err(BrightnessFailure {
                kind: BrightnessFailureKind::HelperCleanupFailure,
                operation: Some(BrightnessOperation::VerifyHelperExit),
                native_code: termination_code,
                process_cleanup: Some(HelperCleanupDisposition::ExitUnverified),
                set_disposition: None,
                set_cause: None,
            });
        }
        self.poisoned = true;
        self.job.take();
        self.cleanup_disposition = Some(HelperCleanupDisposition::ForcedTerminationExitVerified);
        Ok(HelperCleanupDisposition::ForcedTerminationExitVerified)
    }
}

impl Drop for WindowsHelperSession {
    fn drop(&mut self) {
        if self.job.is_some() {
            let _ = self.force_terminate(Instant::now() + HELPER_CLEANUP_DEADLINE);
        }
    }
}

fn write_response_frame<W: Write>(
    writer: &mut W,
    envelope: &Envelope,
) -> Result<(), BrightnessFailure> {
    write_frame_limit(writer, envelope, MAX_FRAME_BYTES)
}

fn write_frame_limit<W: Write>(
    writer: &mut W,
    envelope: &Envelope,
    maximum: usize,
) -> Result<(), BrightnessFailure> {
    let frame = encode_protocol_frame(envelope, maximum)?;
    writer
        .write_all(&frame)
        .and_then(|_| writer.flush())
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))
}

fn request_pipe_capacity(handle: HANDLE) -> Result<u32, BrightnessFailure> {
    // Query the child-side stdin handle. Its readable handle has the rights
    // required by GetNamedPipeInfo; the parent's ChildStdin write handle does
    // not grant FILE_READ_ATTRIBUTES on every standard-library implementation.
    let mut outbound_capacity = 0u32;
    let mut inbound_capacity = 0u32;
    unsafe {
        GetNamedPipeInfo(
            handle,
            None,
            Some(&mut outbound_capacity as *mut _),
            Some(&mut inbound_capacity as *mut _),
            None,
        )
    }
    .map_err(|error| BrightnessFailure {
        kind: BrightnessFailureKind::HelperStartFailure,
        operation: None,
        native_code: Some(error.code().0 as u32),
        process_cleanup: None,
        set_disposition: None,
        set_cause: None,
    })?;
    Ok(outbound_capacity.min(inbound_capacity))
}

fn start_failure(
    native_code: u32,
    cleanup: Result<HelperCleanupDisposition, BrightnessFailure>,
) -> BrightnessFailure {
    match cleanup {
        Ok(disposition) => BrightnessFailure {
            kind: BrightnessFailureKind::HelperStartFailure,
            operation: None,
            native_code: Some(native_code),
            process_cleanup: Some(disposition),
            set_disposition: None,
            set_cause: None,
        },
        Err(failure) => failure,
    }
}

fn terminate_started_process(
    child: &mut Child,
    job: Option<OwnedJob>,
    assigned_to_job: bool,
    deadline: Instant,
) -> Result<HelperCleanupDisposition, BrightnessFailure> {
    if child.try_wait().ok().flatten().is_some() {
        drop(job);
        return Ok(HelperCleanupDisposition::ProcessExitVerified);
    }
    let process = raw_process_handle(child);
    let mut native_code = None;
    if assigned_to_job {
        if let Some(job) = job.as_ref()
            && let Err(error) = unsafe { TerminateJobObject(job.0, FORCE_EXIT_CODE) }
        {
            native_code = Some(error.code().0 as u32);
        }
    } else if let Err(error) = unsafe { TerminateProcess(process, FORCE_EXIT_CODE) } {
        native_code = Some(error.code().0 as u32);
    }
    let first_wait = deadline.saturating_duration_since(Instant::now()) / 2;
    if wait_child_exit(child, first_wait) {
        drop(job);
        return Ok(HelperCleanupDisposition::ForcedTerminationExitVerified);
    }
    drop(job); // Closes the kill-on-close job for the assigned case.
    if let Err(error) = unsafe { TerminateProcess(process, FORCE_EXIT_CODE) } {
        native_code.get_or_insert(error.code().0 as u32);
    }
    if wait_child_exit(child, deadline.saturating_duration_since(Instant::now())) {
        return Ok(HelperCleanupDisposition::ForcedTerminationExitVerified);
    }
    Err(BrightnessFailure {
        kind: BrightnessFailureKind::HelperCleanupFailure,
        operation: Some(BrightnessOperation::VerifyHelperExit),
        native_code,
        process_cleanup: Some(HelperCleanupDisposition::ExitUnverified),
        set_disposition: None,
        set_cause: None,
    })
}

fn read_frame_blocking<R: Read>(reader: &mut R) -> Result<Envelope, BrightnessFailure> {
    let mut length = [0u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?;
    let payload_len = u32::from_le_bytes(length) as usize;
    if payload_len == 0 || payload_len > MAX_REQUEST_FRAME_BYTES {
        return Err(BrightnessFailure::new(
            BrightnessFailureKind::ProtocolFailure,
        ));
    }
    let mut frame = Vec::with_capacity(payload_len + 4);
    frame.extend_from_slice(&length);
    frame.resize(payload_len + 4, 0);
    reader
        .read_exact(&mut frame[4..])
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?;
    decode_protocol_frame(&frame)
}

fn wait_child_exit(child: &mut Child, timeout: Duration) -> bool {
    let millis = timeout.as_millis().min(u128::from(u32::MAX)) as u32;
    match unsafe { WaitForSingleObject(raw_process_handle(child), millis) } {
        WAIT_OBJECT_0 => child.try_wait().ok().flatten().is_some(),
        WAIT_TIMEOUT => false,
        _ => false,
    }
}

pub(super) fn run_helper(token: String, session: BrightnessSessionId) -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let request_pipe_capacity = match request_pipe_capacity(HANDLE(input.as_raw_handle())) {
        Ok(capacity)
            if (capacity as usize) >= MIN_REQUEST_PIPE_CAPACITY_BYTES
                && (capacity as usize) <= MAX_REPORTED_PIPE_CAPACITY_BYTES =>
        {
            capacity
        }
        _ => return 3,
    };
    if write_response_frame(
        &mut output,
        &Envelope {
            version: PROTOCOL_VERSION,
            token: token.clone(),
            session,
            request_id: BrightnessRequestId(0),
            generation: 0,
            body: Message::Hello {
                request_pipe_capacity,
            },
        },
    )
    .is_err()
    {
        return 3;
    }
    let mut last_generation = 0u64;
    let mut last_inventory: Option<MonitorInventory> = None;
    loop {
        let request = match read_frame_blocking(&mut input) {
            Ok(request) => request,
            Err(_) => return 4,
        };
        if request.version != PROTOCOL_VERSION
            || request.token != token
            || request.session != session
        {
            return 5;
        }
        let response = match request.body {
            Message::Refresh if request.generation > last_generation => {
                let cancellation = BrightnessCancellation::new();
                let mut api = NativeMonitorApiImpl::default();
                let result =
                    refresh_with_backend(&mut api, session, request.generation, &cancellation);
                if let Ok(inventory) = &result {
                    last_generation = inventory.generation;
                    last_inventory = Some(inventory.clone());
                }
                Message::Refreshed(result)
            }
            Message::Set { target, percent }
                if request.generation == last_generation
                    && target.session == session
                    && target.generation == request.generation =>
            {
                let result = if let Some(inventory) = last_inventory.as_ref() {
                    let cancellation = BrightnessCancellation::new();
                    let mut api = NativeMonitorApiImpl::default();
                    let result =
                        set_with_backend(&mut api, inventory, &target, percent, &cancellation);
                    if result.as_ref().err().is_some_and(|failure| {
                        failure.set_disposition == Some(BrightnessSetDisposition::Unknown)
                    }) {
                        last_inventory = None;
                    }
                    result
                } else {
                    Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget)
                        .with_set_disposition(BrightnessSetDisposition::NotAttempted))
                };
                Message::SetResult(result)
            }
            Message::Shutdown => {
                let response = Envelope {
                    version: PROTOCOL_VERSION,
                    token,
                    session,
                    request_id: request.request_id,
                    generation: request.generation,
                    body: Message::ShutdownAck,
                };
                let result = write_response_frame(&mut output, &response);
                return if result.is_ok() { 0 } else { 6 };
            }
            _ => return 7,
        };
        let response = Envelope {
            version: PROTOCOL_VERSION,
            token: token.clone(),
            session,
            request_id: request.request_id,
            generation: request.generation,
            body: response,
        };
        if write_response_frame(&mut output, &response).is_err() {
            return 8;
        }
    }
}

#[derive(Default)]
struct NativeMonitorApiImpl {
    raw_monitors: Vec<HMONITOR>,
}

#[derive(Clone, Copy)]
struct NativeLogicalHandle {
    monitor: HMONITOR,
}

impl NativeMonitorApi for NativeMonitorApiImpl {
    type Lease = WindowsPhysicalLease;

    fn enumerate_logical(&mut self) -> Result<Vec<LogicalMonitor>, NativeFailure> {
        self.raw_monitors.clear();
        let mut context = EnumContext::default();
        let result = unsafe {
            EnumDisplayMonitors(
                HDC(std::ptr::null_mut()),
                None,
                Some(enumerate_display_callback),
                LPARAM(&mut context as *mut EnumContext as isize),
            )
        };
        if !result.as_bool() || context.overflow {
            let code = if context.overflow {
                None
            } else {
                Some(unsafe { GetLastError().0 })
            };
            return Err(NativeFailure {
                operation: BrightnessOperation::EnumerateLogical,
                code,
            });
        }
        self.raw_monitors = context.raw_monitors;
        Ok(context.logicals)
    }

    fn acquire_physical(&mut self, logical: &LogicalMonitor) -> Result<Self::Lease, NativeFailure> {
        let handle = self
            .raw_monitors
            .get(logical.index as usize)
            .copied()
            .ok_or(NativeFailure {
                operation: BrightnessOperation::GetPhysicalMonitors,
                code: None,
            })?;
        let mut count = 0u32;
        unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut count) }.map_err(
            |error| NativeFailure {
                operation: BrightnessOperation::GetPhysicalCount,
                code: Some(error.code().0 as u32),
            },
        )?;
        if count as usize > MAX_PHYSICAL_MONITORS {
            return Err(NativeFailure {
                operation: BrightnessOperation::GetPhysicalCount,
                code: None,
            });
        }
        let mut handles = vec![PHYSICAL_MONITOR::default(); count as usize];
        if count > 0 {
            unsafe { GetPhysicalMonitorsFromHMONITOR(handle, &mut handles) }.map_err(|error| {
                NativeFailure {
                    operation: BrightnessOperation::GetPhysicalMonitors,
                    code: Some(error.code().0 as u32),
                }
            })?;
        }
        let monitors = handles
            .iter()
            .enumerate()
            .map(|(index, physical)| {
                // PHYSICAL_MONITOR is packed, so copy its UTF-16 array before
                // taking a slice reference to avoid an unaligned borrow.
                let description = physical.szPhysicalMonitorDescription;
                let end = description
                    .iter()
                    .position(|character| *character == 0)
                    .unwrap_or(description.len());
                PhysicalMonitor {
                    physical_index: index as u32,
                    description: String::from_utf16_lossy(&description[..end]),
                }
            })
            .collect();
        Ok(WindowsPhysicalLease {
            handles: Some(handles),
            monitors,
        })
    }
}

#[derive(Default)]
struct EnumContext {
    raw_monitors: Vec<HMONITOR>,
    logicals: Vec<LogicalMonitor>,
    overflow: bool,
}

unsafe extern "system" fn enumerate_display_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    parameter: LPARAM,
) -> windows::Win32::Foundation::BOOL {
    let context = unsafe { &mut *(parameter.0 as *mut EnumContext) };
    if context.logicals.len() >= MAX_LOGICAL_MONITORS {
        context.overflow = true;
        return false.into();
    }
    let index = context.logicals.len() as u32;
    context.raw_monitors.push(monitor);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let device_name =
        if unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) }.as_bool() {
            let end = info
                .szDevice
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(info.szDevice.len());
            Some(String::from_utf16_lossy(&info.szDevice[..end]))
        } else {
            None
        };
    context.logicals.push(LogicalMonitor { index, device_name });
    true.into()
}

struct WindowsPhysicalLease {
    handles: Option<Vec<PHYSICAL_MONITOR>>,
    monitors: Vec<PhysicalMonitor>,
}

impl PhysicalMonitorLease for WindowsPhysicalLease {
    fn monitors(&self) -> &[PhysicalMonitor] {
        &self.monitors
    }

    fn read_brightness(&mut self, index: usize) -> Result<NativeBrightnessRange, NativeFailure> {
        let Some(physical) = self.handles.as_ref().and_then(|handles| handles.get(index)) else {
            return Err(NativeFailure {
                operation: BrightnessOperation::ReadBrightness,
                code: None,
            });
        };
        let (mut minimum, mut current, mut maximum) = (0, 0, 0);
        if unsafe {
            GetMonitorBrightness(
                physical.hPhysicalMonitor,
                &mut minimum,
                &mut current,
                &mut maximum,
            )
        } == 0
        {
            return Err(NativeFailure {
                operation: BrightnessOperation::ReadBrightness,
                code: Some(unsafe { GetLastError().0 }),
            });
        }
        Ok(NativeBrightnessRange {
            minimum,
            current,
            maximum,
        })
    }

    fn set_brightness(&mut self, index: usize, value: u32) -> Result<(), NativeFailure> {
        let Some(physical) = self.handles.as_ref().and_then(|handles| handles.get(index)) else {
            return Err(NativeFailure {
                operation: BrightnessOperation::SetBrightness,
                code: None,
            });
        };
        if unsafe { SetMonitorBrightness(physical.hPhysicalMonitor, value) } == 0 {
            return Err(NativeFailure {
                operation: BrightnessOperation::SetBrightness,
                code: Some(unsafe { GetLastError().0 }),
            });
        }
        Ok(())
    }

    fn destroy(mut self: Box<Self>) -> Result<(), NativeFailure> {
        let Some(handles) = self.handles.take() else {
            return Ok(());
        };
        if handles.is_empty() {
            return Ok(());
        }
        unsafe { DestroyPhysicalMonitors(&handles) }.map_err(|error| NativeFailure {
            operation: BrightnessOperation::DestroyPhysicalMonitors,
            code: Some(error.code().0 as u32),
        })
    }
}

impl Drop for WindowsPhysicalLease {
    fn drop(&mut self) {
        if let Some(handles) = self.handles.take() {
            let _ = unsafe { DestroyPhysicalMonitors(&handles) };
        }
    }
}
