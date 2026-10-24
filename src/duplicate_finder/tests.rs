use super::discovery::{DiscoveryJob, DiscoveryLimits};
use super::model::{
    DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStatus, RootScan,
    RootScanStatus, ScanCounts,
};
use super::session::{DuplicateFinderSession, DuplicateFinderSessionError};
use crate::file_operations::ScanCancellation;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct StartedScan {
    generation: u64,
    roots: Vec<PathBuf>,
}

struct GatedJob {
    started: Mutex<Sender<StartedScan>>,
    release: Mutex<Receiver<()>>,
    calls: AtomicUsize,
}

impl GatedJob {
    fn new(started: Sender<StartedScan>, release: Receiver<()>) -> Self {
        Self {
            started: Mutex::new(started),
            release: Mutex::new(release),
            calls: AtomicUsize::new(0),
        }
    }
}

impl DiscoveryJob for GatedJob {
    fn scan(
        &self,
        roots: Vec<PathBuf>,
        generation: u64,
        _limits: DiscoveryLimits,
        _cancellation: &ScanCancellation,
        _progress: &mut dyn FnMut(DiscoveryProgress),
    ) -> DiscoverySnapshot {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.started
            .lock()
            .unwrap()
            .send(StartedScan {
                generation,
                roots: roots.clone(),
            })
            .expect("test receiver stays connected");
        self.release
            .lock()
            .unwrap()
            .recv()
            .expect("test releases each gated scan");
        completed_snapshot(generation, roots)
    }
}

struct CountingJob(AtomicUsize);

impl DiscoveryJob for CountingJob {
    fn scan(
        &self,
        roots: Vec<PathBuf>,
        generation: u64,
        _limits: DiscoveryLimits,
        _cancellation: &ScanCancellation,
        _progress: &mut dyn FnMut(DiscoveryProgress),
    ) -> DiscoverySnapshot {
        self.0.fetch_add(1, Ordering::AcqRel);
        completed_snapshot(generation, roots)
    }
}

fn completed_snapshot(generation: u64, roots: Vec<PathBuf>) -> DiscoverySnapshot {
    let counts = ScanCounts::default();
    DiscoverySnapshot {
        generation,
        status: DiscoveryStatus::Complete,
        progress: Some(DiscoveryProgress {
            generation,
            roots_total: roots.len(),
            roots_preflighted: roots.len(),
            roots_completed: roots.len(),
            current_root: None,
            counts,
        }),
        roots: roots
            .into_iter()
            .map(|selected_path| RootScan {
                selected_path,
                canonical_root: None,
                identity: None,
                status: RootScanStatus::Complete,
                counts,
            })
            .collect(),
        size_groups: Vec::new(),
        diagnostics: Vec::<DiscoveryDiagnostic>::new(),
        diagnostics_truncated: false,
        counts,
    }
}

fn receive_started(receiver: &Receiver<StartedScan>) -> StartedScan {
    receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("discovery worker starts within the test deadline")
}

fn send_release(sender: &Sender<()>) {
    sender.send(()).expect("gated worker is waiting");
}

fn finish_close(session: &DuplicateFinderSession) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match session.reopen() {
            Ok(()) => return,
            Err(DuplicateFinderSessionError::WorkerRetiring) => {}
            Err(error) => panic!("closed session should reopen after retirement: {error}"),
        }
        assert!(Instant::now() < deadline, "discovery worker did not retire");
        std::thread::yield_now();
    }
}

fn wait_for_current_complete(session: &DuplicateFinderSession, generation: u64) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let view = session.view();
        if view.snapshot.generation == generation
            && view.snapshot.status == DiscoveryStatus::Complete
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "current scan did not publish completion"
        );
        std::thread::yield_now();
    }
}

#[test]
fn dff_lifecycle_coalesces_pending_scans_and_reopens_only_after_retirement() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let job = Arc::new(GatedJob::new(started_tx, release_rx));
    let session = DuplicateFinderSession::with_test_job(DiscoveryLimits::default(), job);

    let first = session
        .start_scan(vec![PathBuf::from("first")])
        .expect("first scan accepted");
    assert_eq!(receive_started(&started_rx).generation, first);
    let superseded = session
        .start_scan(vec![PathBuf::from("superseded")])
        .expect("second request is coalesced");
    let latest = session
        .start_scan(vec![PathBuf::from("latest")])
        .expect("third request replaces the pending request");
    assert!(latest > superseded);

    send_release(&release_tx);
    let latest_started = receive_started(&started_rx);
    assert_eq!(latest_started.generation, latest);
    assert_eq!(latest_started.roots, vec![PathBuf::from("latest")]);
    assert!(matches!(
        started_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_eq!(session.view().snapshot.generation, latest);
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Scanning);

    session.close();
    assert!(session.view().closed);
    send_release(&release_tx);
    finish_close(&session);
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Idle);

    let reopened = session
        .start_scan(vec![PathBuf::from("after-reopen")])
        .expect("same session admits a scan after actual retirement");
    let reopened_started = receive_started(&started_rx);
    assert_eq!(reopened_started.generation, reopened);
    assert_eq!(reopened_started.roots, vec![PathBuf::from("after-reopen")]);
    send_release(&release_tx);
    wait_for_current_complete(&session, reopened);
    session.close();
    finish_close(&session);
}

#[test]
fn dff_lifecycle_cancel_invalidates_publication_while_io_drains() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let session = DuplicateFinderSession::with_test_job(
        DiscoveryLimits::default(),
        Arc::new(GatedJob::new(started_tx, release_rx)),
    );

    let generation = session
        .start_scan(vec![PathBuf::from("blocked-root")])
        .expect("scan accepted");
    assert_eq!(receive_started(&started_rx).generation, generation);
    assert!(session.cancel().expect("cancel remains nonblocking"));
    let cancelled = session.view();
    assert!(
        cancelled.busy,
        "native scan keeps its single worker slot while draining"
    );
    assert_eq!(cancelled.snapshot.status, DiscoveryStatus::Cancelled);
    assert!(cancelled.snapshot.generation > generation);

    let refused = session.start_scan(vec![PathBuf::from("must-not-start-yet")]);
    assert_eq!(
        refused,
        Err(DuplicateFinderSessionError::WorkerRetiring),
        "a cancelled native call retains the single worker slot until it drains"
    );
    let still_cancelled = session.view();
    assert_eq!(
        still_cancelled.snapshot.generation, cancelled.snapshot.generation,
        "a refused start does not advance the published generation"
    );
    assert_eq!(still_cancelled.snapshot.status, DiscoveryStatus::Cancelled);
    assert!(still_cancelled.busy);
    assert!(matches!(
        started_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    send_release(&release_tx);
    let deadline = Instant::now() + Duration::from_secs(3);
    while session.is_busy() {
        assert!(Instant::now() < deadline, "cancelled worker did not drain");
        std::thread::yield_now();
    }
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Cancelled);

    let resumed_generation = session
        .start_scan(vec![PathBuf::from("after-drain")])
        .expect("a fresh request is accepted after cancelled IO has drained");
    let resumed = receive_started(&started_rx);
    assert_eq!(resumed.generation, resumed_generation);
    assert_eq!(resumed.roots, vec![PathBuf::from("after-drain")]);
    send_release(&release_tx);
    wait_for_current_complete(&session, resumed_generation);

    session.close();
    finish_close(&session);
}

#[test]
fn dff_lifecycle_drop_transfers_blocked_worker_to_reaper_without_waiting() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let session = DuplicateFinderSession::with_test_job(
        DiscoveryLimits::default(),
        Arc::new(GatedJob::new(started_tx, release_rx)),
    );
    session
        .start_scan(vec![PathBuf::from("blocked-root")])
        .expect("scan accepted");
    let _started = receive_started(&started_rx);
    let reaped = session
        .reaper_probe_for_test()
        .expect("worker owns a reserved reaper slot before start");

    drop(session);
    assert!(
        !reaped.is_reaped(),
        "drop did not block waiting for filesystem work"
    );
    send_release(&release_tx);

    let deadline = Instant::now() + Duration::from_secs(3);
    while !reaped.is_reaped() {
        assert!(
            Instant::now() < deadline,
            "reaper did not join the transferred worker"
        );
        std::thread::yield_now();
    }
}

#[test]
fn dff_lifecycle_rejects_oversized_root_list_before_worker_creation() {
    let mut limits = DiscoveryLimits::default();
    limits.max_roots = 1;
    let job = Arc::new(CountingJob(AtomicUsize::new(0)));
    let session = DuplicateFinderSession::with_test_job(limits, job.clone());

    let generation = session
        .start_scan(vec![PathBuf::from("one"), PathBuf::from("two")])
        .expect("invalid selections publish a user-facing failed generation");
    let view = session.view();
    assert_eq!(view.snapshot.generation, generation);
    assert_eq!(view.snapshot.status, DiscoveryStatus::Failed);
    assert!(!view.busy);
    assert_eq!(job.0.load(Ordering::Acquire), 0);
}

#[test]
fn dff_lifecycle_reaper_capacity_refusal_does_not_start_or_retain_a_scan() {
    let job = Arc::new(CountingJob(AtomicUsize::new(0)));
    let session = DuplicateFinderSession::with_test_job_and_reap_permit_reserver(
        DiscoveryLimits::default(),
        job.clone(),
        Arc::new(|| Err("bounded worker join capacity exhausted")),
    );

    let error = session
        .start_scan(vec![PathBuf::from("root")])
        .expect_err("a worker cannot start without its reserved reaper slot");
    assert!(matches!(
        error,
        DuplicateFinderSessionError::WorkerCapacity(message)
            if message.contains("bounded worker join capacity exhausted")
    ));
    let view = session.view();
    assert_eq!(view.snapshot.status, DiscoveryStatus::Failed);
    assert!(!view.busy, "a refused request is not retained as pending");
    assert_eq!(job.0.load(Ordering::Acquire), 0);
}
