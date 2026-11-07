//! Bounded driver for one Explorer tab operation. Native evidence remains on
//! the worker that obtained it; this layer only drives the operation owner.
use super::identity::SnapshotFailure;
use super::operation::{
    FolderRequestKind, FolderRequestStep, FolderTabOperation, FolderTabRequest,
    InitialTargetEvidence, OperationAction, OperationControl, OperationFailure, OperationId,
    OperationReport, RequestResult, VerifiedFolderTab, VerifiedTabEvidence,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapState {
    NotStarted,
    Attempted,
    Attributed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendDiagnostic {
    pub reason: String,
    pub bootstrap: BootstrapState,
    pub owned_resource_remaining: bool,
    pub owned_resource_path: Option<std::path::PathBuf>,
    /// Whether the current requested-folder mutation may have run. A private
    /// bootstrap can have side effects while this remains false.
    pub requested_step_attempted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerTabsReport {
    pub operation: OperationReport,
    pub diagnostic: Option<BackendDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerTabsStartError {
    Busy,
    UnsupportedPlatform,
    DeadlineOutOfRange,
    TokenExhausted,
    WorkerStartFailed,
}

pub enum ExplorerTabsJobState<'a> {
    Idle,
    Running {
        id: u64,
        progress: Option<&'a OperationReport>,
    },
    Finished {
        id: u64,
        report: &'a ExplorerTabsReport,
    },
    WorkerFailed {
        id: u64,
    },
}

struct Worker {
    id: u64,
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<WorkerUpdate>,
    thread: JoinHandle<()>,
    received: Option<ExplorerTabsReport>,
    progress: Option<OperationReport>,
}

enum WorkerUpdate {
    Progress(OperationReport),
    Finished(ExplorerTabsReport),
}

/// One worker slot, including a cancelled worker until its native calls retire.
/// Keep this owner outside an individual window's visible lifecycle.
#[derive(Default)]
pub struct ExplorerTabsJob {
    worker: Option<Worker>,
    finished: Option<(u64, ExplorerTabsReport)>,
    worker_failed: Option<u64>,
}

static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);
static WORKER_ACTIVE: AtomicBool = AtomicBool::new(false);

struct WorkerLease;

impl WorkerLease {
    fn acquire() -> Result<Self, ExplorerTabsStartError> {
        WORKER_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| ExplorerTabsStartError::Busy)
    }
}

impl Drop for WorkerLease {
    fn drop(&mut self) {
        WORKER_ACTIVE.store(false, Ordering::Release);
    }
}

impl ExplorerTabsJob {
    #[cfg(windows)]
    pub fn start(
        &mut self,
        request: FolderTabRequest,
        timeout: Duration,
    ) -> Result<u64, ExplorerTabsStartError> {
        self.start_with_factory(request, timeout, |id, cancelled, deadline| {
            super::windows::WindowsExplorerTabsPort::new(id, cancelled, deadline)
        })
    }

    #[cfg(not(windows))]
    pub fn start(
        &mut self,
        _request: FolderTabRequest,
        _timeout: Duration,
    ) -> Result<u64, ExplorerTabsStartError> {
        Err(ExplorerTabsStartError::UnsupportedPlatform)
    }

    pub(crate) fn start_with_factory<P, F>(
        &mut self,
        request: FolderTabRequest,
        timeout: Duration,
        factory: F,
    ) -> Result<u64, ExplorerTabsStartError>
    where
        P: ExplorerTabsPort + 'static,
        F: FnOnce(OperationId, Arc<AtomicBool>, Instant) -> P + Send + 'static,
    {
        if self.worker.is_some() {
            return Err(ExplorerTabsStartError::Busy);
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ExplorerTabsStartError::DeadlineOutOfRange)?;
        let id = NEXT_OPERATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| ExplorerTabsStartError::TokenExhausted)?;
        let operation_id =
            OperationId::from_token(id).ok_or(ExplorerTabsStartError::TokenExhausted)?;
        let lease = WorkerLease::acquire()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("explorer-folder-tabs".into())
            .spawn(move || {
                let _lease = lease;
                let mut port = factory(operation_id, Arc::clone(&worker_cancelled), deadline);
                let report = drive(
                    request,
                    operation_id,
                    &mut port,
                    &worker_cancelled,
                    deadline,
                    &mut |progress| {
                        let _ = sender.send(WorkerUpdate::Progress(progress));
                    },
                );
                drop(port);
                let _ = sender.send(WorkerUpdate::Finished(report));
            })
            .map_err(|_| ExplorerTabsStartError::WorkerStartFailed)?;
        self.finished = None;
        self.worker_failed = None;
        self.worker = Some(Worker {
            id,
            cancelled,
            receiver,
            thread,
            received: None,
            progress: None,
        });
        Ok(id)
    }

    pub fn cancel(&self) {
        if let Some(worker) = &self.worker {
            worker.cancelled.store(true, Ordering::Release);
        }
    }

    pub fn poll(&mut self) -> ExplorerTabsJobState<'_> {
        if let Some(worker) = &mut self.worker {
            while let Ok(update) = worker.receiver.try_recv() {
                match update {
                    WorkerUpdate::Progress(report) => worker.progress = Some(report),
                    WorkerUpdate::Finished(report) => worker.received = Some(report),
                }
            }
            if worker.thread.is_finished() {
                let mut worker = self.worker.take().expect("worker was present");
                let id = worker.id;
                if worker.thread.join().is_ok() {
                    while let Ok(update) = worker.receiver.try_recv() {
                        if let WorkerUpdate::Finished(report) = update {
                            worker.received = Some(report);
                        }
                    }
                    if let Some(report) = worker.received {
                        self.finished = Some((id, report));
                    } else {
                        self.worker_failed = Some(id);
                    }
                } else {
                    self.worker_failed = Some(id);
                }
            }
        }
        if let Some(worker) = &self.worker {
            return ExplorerTabsJobState::Running {
                id: worker.id,
                progress: worker.progress.as_ref(),
            };
        }
        if let Some((id, report)) = &self.finished {
            return ExplorerTabsJobState::Finished { id: *id, report };
        }
        if let Some(id) = self.worker_failed {
            return ExplorerTabsJobState::WorkerFailed { id };
        }
        ExplorerTabsJobState::Idle
    }
}

impl Drop for ExplorerTabsJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The native implementation and deterministic tests use exactly this driver.
/// Only the worker's native port may construct the typed observation evidence.
pub(crate) trait ExplorerTabsPort {
    fn preflight(&mut self) -> Result<(), BackendDiagnostic>;
    fn request(&mut self, step: &FolderRequestStep) -> Result<RequestResult, BackendDiagnostic>;
    fn observe_initial(
        &mut self,
        step: &FolderRequestStep,
    ) -> Result<InitialTargetEvidence, SnapshotFailure>;
    fn observe_append(
        &mut self,
        step: &FolderRequestStep,
        prefix: &[VerifiedFolderTab],
    ) -> Result<VerifiedTabEvidence, SnapshotFailure>;
    fn verify_final(&mut self, prefix: &[VerifiedFolderTab]) -> Result<(), BackendDiagnostic>;
    fn diagnostic(&self) -> Option<BackendDiagnostic> {
        None
    }
}

pub(crate) fn drive<P: ExplorerTabsPort>(
    request: FolderTabRequest,
    operation_id: OperationId,
    port: &mut P,
    cancelled: &AtomicBool,
    deadline: Instant,
    publish: &mut impl FnMut(OperationReport),
) -> ExplorerTabsReport {
    let mut operation = FolderTabOperation::new(request, operation_id);
    let control = || {
        if cancelled.load(Ordering::Acquire) {
            OperationControl::Cancelled
        } else if Instant::now() >= deadline {
            OperationControl::DeadlineExpired
        } else {
            OperationControl::Continue
        }
    };
    let mut diagnostic = None;
    publish(operation.report());
    if control() == OperationControl::Continue {
        let result = port.preflight();
        if let Err(error) = result {
            diagnostic = Some(error);
            let after = control();
            if after == OperationControl::Continue {
                operation.backend_failure(OperationFailure::CapabilityUnavailable);
            } else {
                operation.next_action(after);
            }
        }
    }
    loop {
        match operation.next_action(control()) {
            OperationAction::Request(step) => {
                let before = control();
                if before != OperationControl::Continue {
                    operation.stop_before_dispatch(before);
                    continue;
                }
                let returned = match port.request(&step) {
                    Ok(result) => result,
                    Err(error) => {
                        let attempted = error.requested_step_attempted;
                        diagnostic = Some(error);
                        let after = control();
                        if after == OperationControl::Continue {
                            if attempted {
                                operation.backend_failure(OperationFailure::NativeOperationFailed);
                            } else {
                                operation.backend_failure_before_dispatch(
                                    OperationFailure::CapabilityUnavailable,
                                );
                            }
                        } else if !attempted {
                            operation.stop_before_dispatch(after);
                        } else {
                            operation.next_action(after);
                        }
                        continue;
                    }
                };
                if !matches!(
                    operation.request_returned(&step, returned, control()),
                    OperationAction::WaitingForObservation
                ) {
                    continue;
                }
                let before = control();
                if before != OperationControl::Continue {
                    operation.next_action(before);
                    continue;
                }
                let after = match step.kind() {
                    FolderRequestKind::CreateInitialWindow => {
                        let evidence = port.observe_initial(&step);
                        operation.observe_initial_target(evidence, control())
                    }
                    FolderRequestKind::OpenAdditionalTab => {
                        let report = operation.report();
                        let evidence = port.observe_append(&step, report.verified_prefix());
                        operation.observe_additional_tab(evidence, control())
                    }
                };
                if matches!(
                    after,
                    OperationAction::Verified(_) | OperationAction::VerifyFinal
                ) {
                    publish(operation.report());
                }
                if matches!(after, OperationAction::Finished(_)) {
                    continue;
                }
            }
            OperationAction::VerifyFinal => {
                if control() != OperationControl::Continue {
                    operation.next_action(control());
                    continue;
                }
                let report = operation.report();
                let result = port.verify_final(report.verified_prefix());
                if let Err(error) = &result {
                    diagnostic = Some(error.clone());
                }
                operation.final_verification_returned(result.is_ok(), control());
            }
            OperationAction::Finished(report) => {
                return ExplorerTabsReport {
                    operation: report,
                    diagnostic: diagnostic.or_else(|| port.diagnostic()),
                };
            }
            OperationAction::Verified(_) => {}
            OperationAction::WaitingForNativeResult | OperationAction::WaitingForObservation => {
                operation.backend_failure(OperationFailure::NativeOperationFailed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer_tabs::identity::test_support as identity;
    use crate::explorer_tabs::operation::{
        FolderIdentityResult, FolderPath, OperationOutcome, test_support as support,
    };
    use std::sync::Arc;
    use std::time::Duration;

    struct FakePort {
        id: OperationId,
        log: Vec<String>,
        preflight_ok: bool,
        final_ok: bool,
        cancel_on_request: Option<Arc<AtomicBool>>,
        cancel_on_preflight: Option<Arc<AtomicBool>>,
        request_error: bool,
        bootstrap_only_failure: bool,
        fail_initial: bool,
    }
    impl ExplorerTabsPort for FakePort {
        fn preflight(&mut self) -> Result<(), BackendDiagnostic> {
            self.log.push("preflight".into());
            if let Some(cancelled) = &self.cancel_on_preflight {
                cancelled.store(true, Ordering::Release);
            }
            if self.preflight_ok {
                Ok(())
            } else {
                Err(problem(BootstrapState::NotStarted))
            }
        }
        fn request(
            &mut self,
            step: &FolderRequestStep,
        ) -> Result<RequestResult, BackendDiagnostic> {
            self.log.push(format!(
                "request:{}:{}",
                step.request_index(),
                step.folder().as_str()
            ));
            if let Some(cancelled) = &self.cancel_on_request {
                cancelled.store(true, Ordering::Release);
            }
            if self.bootstrap_only_failure {
                let mut diagnostic = problem(BootstrapState::Attributed);
                diagnostic.requested_step_attempted = false;
                Err(diagnostic)
            } else if self.request_error {
                Err(problem(BootstrapState::Attempted))
            } else {
                Ok(RequestResult::Accepted)
            }
        }
        fn observe_initial(
            &mut self,
            step: &FolderRequestStep,
        ) -> Result<InitialTargetEvidence, SnapshotFailure> {
            self.log.push("observe:initial".into());
            if self.fail_initial {
                return Err(SnapshotFailure::ShellUnavailable);
            }
            Ok(support::initial_evidence(
                self.id,
                step,
                42,
                1,
                100,
                FolderIdentityResult::ExactPidl,
            ))
        }
        fn observe_append(
            &mut self,
            step: &FolderRequestStep,
            _prefix: &[VerifiedFolderTab],
        ) -> Result<VerifiedTabEvidence, SnapshotFailure> {
            self.log.push(format!("observe:{}", step.request_index()));
            Ok(support::tab_evidence(
                self.id,
                identity::window(42),
                1,
                step,
                100 + step.request_index() as u64,
                step.expected_ordinal(),
                FolderIdentityResult::ExactPidl,
            ))
        }
        fn verify_final(&mut self, prefix: &[VerifiedFolderTab]) -> Result<(), BackendDiagnostic> {
            self.log.push(format!("final:{}", prefix.len()));
            if self.final_ok {
                Ok(())
            } else {
                Err(problem(BootstrapState::Attributed))
            }
        }
        fn diagnostic(&self) -> Option<BackendDiagnostic> {
            self.fail_initial
                .then(|| problem(BootstrapState::Attributed))
        }
    }
    fn problem(bootstrap: BootstrapState) -> BackendDiagnostic {
        BackendDiagnostic {
            reason: "fixture capability".into(),
            bootstrap,
            owned_resource_remaining: bootstrap != BootstrapState::NotStarted,
            owned_resource_path: None,
            requested_step_attempted: bootstrap != BootstrapState::NotStarted,
        }
    }
    fn request() -> FolderTabRequest {
        FolderTabRequest::new(
            [r"C:\One", r"C:\One", r"\\server\share\日本語"]
                .iter()
                .map(|path| FolderPath::parse(path).unwrap())
                .collect(),
        )
        .unwrap()
    }
    fn port() -> FakePort {
        FakePort {
            id: support::operation_id(7),
            log: Vec::new(),
            preflight_ok: true,
            final_ok: true,
            cancel_on_request: None,
            cancel_on_preflight: None,
            request_error: false,
            bootstrap_only_failure: false,
            fail_initial: false,
        }
    }
    fn run(port: &mut FakePort, cancelled: &AtomicBool) -> ExplorerTabsReport {
        drive(
            request(),
            port.id,
            port,
            cancelled,
            Instant::now() + Duration::from_secs(5),
            &mut |_| {},
        )
    }
    #[test]
    fn explorer_tabs_backend_drives_ordered_duplicate_paths_and_final_validation() {
        let mut port = port();
        let report = run(&mut port, &AtomicBool::new(false));
        assert_eq!(report.operation.outcome(), OperationOutcome::Completed);
        assert_eq!(report.operation.verified_prefix().len(), 3);
        assert_eq!(
            port.log
                .iter()
                .filter(|event| event.starts_with("request:"))
                .count(),
            3
        );
        assert_eq!(port.log.last().unwrap(), "final:3");
        assert_eq!(
            report.operation.verified_prefix()[0].folder(),
            report.operation.verified_prefix()[1].folder()
        );
        assert_ne!(
            report.operation.verified_prefix()[0].identity(),
            report.operation.verified_prefix()[1].identity()
        );
    }
    #[test]
    fn explorer_tabs_backend_preflight_refusal_has_zero_requests() {
        let mut port = port();
        port.preflight_ok = false;
        let report = run(&mut port, &AtomicBool::new(false));
        assert_eq!(
            report.operation.outcome(),
            OperationOutcome::Failed(OperationFailure::CapabilityUnavailable)
        );
        assert!(report.operation.attempted_unverified().is_none());
        assert_eq!(port.log, ["preflight"]);
        assert_eq!(
            report.diagnostic.unwrap().bootstrap,
            BootstrapState::NotStarted
        );
    }
    #[test]
    fn explorer_tabs_backend_cancel_after_request_never_retries() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut port = port();
        port.cancel_on_request = Some(cancelled.clone());
        let report = run(&mut port, &cancelled);
        assert_eq!(report.operation.outcome(), OperationOutcome::Cancelled);
        assert!(report.operation.attempted_unverified().is_some());
        assert_eq!(
            port.log
                .iter()
                .filter(|event| event.starts_with("request:"))
                .count(),
            1
        );
        assert!(!port.log.iter().any(|event| event.starts_with("observe:")));
    }

    #[test]
    fn explorer_tabs_backend_cancel_during_error_takes_precedence() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut preflight = port();
        preflight.preflight_ok = false;
        preflight.cancel_on_preflight = Some(cancelled.clone());
        let report = run(&mut preflight, &cancelled);
        assert_eq!(report.operation.outcome(), OperationOutcome::Cancelled);
        assert!(report.operation.attempted_unverified().is_none());
        assert_eq!(preflight.log, ["preflight"]);

        cancelled.store(false, Ordering::Release);
        let mut dispatched = port();
        dispatched.request_error = true;
        dispatched.cancel_on_request = Some(cancelled.clone());
        let report = run(&mut dispatched, &cancelled);
        assert_eq!(report.operation.outcome(), OperationOutcome::Cancelled);
        assert!(report.operation.attempted_unverified().is_some());
        assert_eq!(dispatched.log.len(), 2);
    }
    #[test]
    fn explorer_tabs_backend_final_failure_never_reports_completed() {
        let mut port = port();
        port.final_ok = false;
        let report = run(&mut port, &AtomicBool::new(false));
        assert_eq!(
            report.operation.outcome(),
            OperationOutcome::Failed(OperationFailure::FinalVerificationFailed)
        );
        assert_eq!(report.operation.verified_prefix().len(), 3);
    }
    #[test]
    fn explorer_tabs_backend_post_bootstrap_failure_reports_partial_attempt() {
        let mut port = port();
        port.fail_initial = true;
        let report = run(&mut port, &AtomicBool::new(false));
        assert_eq!(
            report.operation.outcome(),
            OperationOutcome::Failed(OperationFailure::SnapshotIncomplete(
                SnapshotFailure::ShellUnavailable
            ))
        );
        assert!(report.operation.verified_prefix().is_empty());
        assert!(report.operation.attempted_unverified().is_some());
        assert_eq!(
            report.diagnostic.unwrap().bootstrap,
            BootstrapState::Attributed
        );
        assert_eq!(
            port.log
                .iter()
                .filter(|event| event.starts_with("request:"))
                .count(),
            1
        );
    }

    #[test]
    fn explorer_tabs_backend_bootstrap_only_failure_preserves_all_requested_folders() {
        let mut port = port();
        port.bootstrap_only_failure = true;
        let report = run(&mut port, &AtomicBool::new(false));
        assert_eq!(
            report.operation.outcome(),
            OperationOutcome::Failed(OperationFailure::CapabilityUnavailable)
        );
        assert!(report.operation.attempted_unverified().is_none());
        assert_eq!(report.operation.remaining(), request().folders());
        let diagnostic = report.diagnostic.unwrap();
        assert_eq!(diagnostic.bootstrap, BootstrapState::Attributed);
        assert!(diagnostic.owned_resource_remaining);
        assert!(!diagnostic.requested_step_attempted);
        assert_eq!(port.log.len(), 2);

        let expected = request();
        let mut operation = FolderTabOperation::new(expected.clone(), support::operation_id(9));
        assert!(matches!(
            operation.next_action(OperationControl::Continue),
            OperationAction::Request(_)
        ));
        operation.stop_before_dispatch(OperationControl::Cancelled);
        assert!(operation.report().attempted_unverified().is_none());
        assert_eq!(operation.report().remaining(), expected.folders());
    }

    #[test]
    fn explorer_tabs_backend_job_reports_progress_and_keeps_retiring_slot() {
        struct PausingPort {
            inner: FakePort,
            reached: std::sync::mpsc::Sender<()>,
            release: std::sync::mpsc::Receiver<()>,
        }
        impl ExplorerTabsPort for PausingPort {
            fn preflight(&mut self) -> Result<(), BackendDiagnostic> {
                self.inner.preflight()
            }
            fn request(
                &mut self,
                step: &FolderRequestStep,
            ) -> Result<RequestResult, BackendDiagnostic> {
                if step.request_index() == 1 {
                    self.reached.send(()).unwrap();
                    self.release.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                self.inner.request(step)
            }
            fn observe_initial(
                &mut self,
                step: &FolderRequestStep,
            ) -> Result<InitialTargetEvidence, SnapshotFailure> {
                self.inner.observe_initial(step)
            }
            fn observe_append(
                &mut self,
                step: &FolderRequestStep,
                prefix: &[VerifiedFolderTab],
            ) -> Result<VerifiedTabEvidence, SnapshotFailure> {
                self.inner.observe_append(step, prefix)
            }
            fn verify_final(
                &mut self,
                prefix: &[VerifiedFolderTab],
            ) -> Result<(), BackendDiagnostic> {
                self.inner.verify_final(prefix)
            }
        }
        let (reached_tx, reached_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut job = ExplorerTabsJob::default();
        let id = job
            .start_with_factory(request(), Duration::from_secs(5), move |id, _, _| {
                let mut inner = port();
                inner.id = id;
                PausingPort {
                    inner,
                    reached: reached_tx,
                    release: release_rx,
                }
            })
            .unwrap();
        reached_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(job.poll(), ExplorerTabsJobState::Running { id: current, progress: Some(report) } if current == id && report.verified_prefix().len() == 1)
        );
        job.cancel();
        assert!(matches!(
            job.start_with_factory(request(), Duration::from_secs(5), |id, _, _| {
                let mut p = port();
                p.id = id;
                p
            }),
            Err(ExplorerTabsStartError::Busy)
        ));
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match job.poll() {
                ExplorerTabsJobState::Finished {
                    id: current,
                    report,
                } => {
                    assert_eq!(current, id);
                    assert_eq!(report.operation.outcome(), OperationOutcome::Cancelled);
                    assert_eq!(report.operation.verified_prefix().len(), 1);
                    assert!(report.operation.attempted_unverified().is_some());
                    break;
                }
                ExplorerTabsJobState::Running { .. } if Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                _ => panic!("worker did not retire with its cancellation report"),
            }
        }

        let (reached_tx, reached_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut abandoned = ExplorerTabsJob::default();
        abandoned
            .start_with_factory(request(), Duration::from_secs(5), move |id, _, _| {
                let mut inner = port();
                inner.id = id;
                PausingPort {
                    inner,
                    reached: reached_tx,
                    release: release_rx,
                }
            })
            .unwrap();
        reached_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(abandoned);
        let mut replacement = ExplorerTabsJob::default();
        assert!(matches!(
            replacement.start_with_factory(request(), Duration::from_secs(5), |id, _, _| {
                let mut p = port();
                p.id = id;
                p
            }),
            Err(ExplorerTabsStartError::Busy)
        ));
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while WORKER_ACTIVE.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(
            !WORKER_ACTIVE.load(Ordering::Acquire),
            "abandoned worker did not retire"
        );
        assert!(
            replacement
                .start_with_factory(request(), Duration::from_secs(5), |id, _, _| {
                    let mut p = port();
                    p.id = id;
                    p
                })
                .is_ok()
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match replacement.poll() {
                ExplorerTabsJobState::Finished { report, .. } => {
                    assert_eq!(report.operation.outcome(), OperationOutcome::Completed);
                    break;
                }
                ExplorerTabsJobState::Running { .. } if Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                _ => panic!("replacement worker did not complete"),
            }
        }
    }
}
