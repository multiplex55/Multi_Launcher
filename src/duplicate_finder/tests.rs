use super::discovery::{DiscoveryJob, DiscoveryLimits};
use super::model::{
    ContentProgress, DiscoveryDiagnostic, DiscoveryProgress, DiscoverySnapshot, DiscoveryStage,
    DiscoveryStatus, DuplicateTotals, RootScan, RootScanStatus, ScanCounts,
};
use super::session::{DuplicateFinderSession, DuplicateFinderSessionError};
use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityLifecycle, ActivityOutcome, ActivityRetirement,
};
use crate::file_operations::ScanCancellation;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Declared before the session so owner drop closes admission before release.
struct ReleaseScanOnDrop(Sender<()>);

impl Drop for ReleaseScanOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

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

struct PanickingJob(Sender<StartedScan>);

impl DiscoveryJob for PanickingJob {
    fn scan(
        &self,
        roots: Vec<PathBuf>,
        generation: u64,
        _limits: DiscoveryLimits,
        _cancellation: &ScanCancellation,
        _progress: &mut dyn FnMut(DiscoveryProgress),
    ) -> DiscoverySnapshot {
        self.0
            .send(StartedScan { generation, roots })
            .expect("test receiver stays connected");
        panic!("controlled duplicate discovery worker panic");
    }
}

fn completed_snapshot(generation: u64, roots: Vec<PathBuf>) -> DiscoverySnapshot {
    let counts = ScanCounts::default();
    DiscoverySnapshot {
        generation,
        status: DiscoveryStatus::Complete,
        progress: Some(DiscoveryProgress {
            generation,
            stage: DiscoveryStage::Complete,
            roots_total: roots.len(),
            roots_preflighted: roots.len(),
            roots_completed: roots.len(),
            current_root: None,
            counts,
            content: Some(ContentProgress::default()),
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
        verified_duplicate_groups: Vec::new(),
        identity_unverified_matches: Vec::new(),
        duplicate_totals: DuplicateTotals::default(),
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

#[test]
fn dff_activity_finishes_panicking_worker_without_live_cancel_capability() {
    let (started_tx, started_rx) = mpsc::channel();
    let mut session = DuplicateFinderSession::with_test_job(
        DiscoveryLimits::default(),
        Arc::new(PanickingJob(started_tx)),
    );
    let center = ActivityCenter::new();
    session.attach_activity_center(center.clone());

    let generation = session
        .start_scan(vec![PathBuf::from("panic-root")])
        .expect("scan is admitted before the controlled panic");
    let started = receive_started(&started_rx);
    assert_eq!(started.generation, generation);
    assert_eq!(started.roots, vec![PathBuf::from("panic-root")]);
    let key = center
        .snapshot()
        .rows
        .into_iter()
        .find(|row| row.key.incarnation().get() == generation)
        .expect("admitted scan has an exact Activity key")
        .key;

    let deadline = Instant::now() + Duration::from_secs(3);
    let failed = loop {
        if let Some(row) = center
            .snapshot()
            .rows
            .into_iter()
            .find(|row| row.key == key && row.lifecycle.is_terminal())
        {
            break row;
        }
        assert!(
            Instant::now() < deadline,
            "panic unwind did not publish the Activity terminal"
        );
        std::thread::yield_now();
    };
    assert_eq!(
        failed.lifecycle,
        ActivityLifecycle::Terminal(ActivityOutcome::Failed)
    );
    assert_eq!(failed.diagnostic, Some(ActivityDiagnostic::WorkerFailed));
    assert_eq!(failed.retirement, ActivityRetirement::Unavailable);
    assert!(
        !failed.can_cancel,
        "terminal panic rows retain no cancel capability"
    );
    assert_eq!(
        center.request_cancel(key),
        crate::activity::ActivityCancelResult::Terminal
    );

    let exit_deadline = Instant::now() + Duration::from_secs(3);
    while !session.worker_finished_for_test() {
        assert!(
            Instant::now() < exit_deadline,
            "panicking worker did not reach its actual thread exit"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        session.start_scan(vec![PathBuf::from("after-panic")]),
        Err(DuplicateFinderSessionError::WorkerPanicked),
        "the existing owner still observes and joins the panicked worker"
    );
    let retained = center
        .snapshot()
        .rows
        .into_iter()
        .find(|row| row.key == key)
        .expect("the exact failed operation remains in bounded history");
    assert_eq!(
        retained.lifecycle,
        ActivityLifecycle::Terminal(ActivityOutcome::Failed)
    );
    assert!(!retained.can_cancel);

    session.close();
    finish_close(&session);
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
fn activity_baseline_native_file_scan_cancel_and_warm_reopen() {
    use super::content::FirstReadCheckpointGate;
    use std::os::windows::fs::OpenOptionsExt;

    fn exclusive_read(path: &std::path::Path) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
    }

    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/productivity_expansion/activity_retirement");
    std::fs::create_dir_all(&fixture_root).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("g09-native-")
        .tempdir_in(&fixture_root)
        .unwrap();
    let payload = vec![42u8; 128 * 1024];
    let files = (0..8)
        .map(|index| directory.path().join(format!("fixture-{index}.bin")))
        .collect::<Vec<_>>();
    for path in &files {
        std::fs::write(path, &payload).unwrap();
        drop(exclusive_read(path).expect("baseline exclusive open must succeed"));
    }
    let gate = Arc::new(FirstReadCheckpointGate::new());
    struct Release(Arc<FirstReadCheckpointGate>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release();
        }
    }
    // Session drops first on unwind, then this releases its transferred worker.
    let _release = Release(Arc::clone(&gate));
    let session = DuplicateFinderSession::with_wake_and_first_read_checkpoint(
        Arc::new(|| {}),
        Arc::clone(&gate),
    );
    let operation = session
        .start_scan(vec![directory.path().to_owned()])
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !gate.reached() {
        assert!(
            Instant::now() < deadline,
            "native pre-read checkpoint not reached"
        );
        std::thread::yield_now();
    }
    let held = files
        .iter()
        .enumerate()
        .filter_map(|(index, path)| match exclusive_read(path) {
            Ok(probe) => {
                drop(probe);
                None
            }
            Err(error) => {
                assert_eq!(
                    error.raw_os_error(),
                    Some(32),
                    "held probe {index}: {error}"
                );
                Some(index)
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !held.is_empty(),
        "actual native reader must hold a source handle"
    );
    let reaped = session.reaper_probe_for_test().unwrap();
    assert!(session.cancel().unwrap());
    let cancelled_generation = session.view().snapshot.generation;
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Cancelled);
    assert!(session.is_busy());
    assert_eq!(
        session.start_scan(vec![directory.path().to_owned()]),
        Err(DuplicateFinderSessionError::WorkerRetiring)
    );
    assert_eq!(session.view().snapshot.generation, cancelled_generation);
    session.close();
    let closed_generation = session.view().snapshot.generation;
    for _ in 0..3 {
        assert_eq!(
            session.reopen(),
            Err(DuplicateFinderSessionError::WorkerRetiring)
        );
        assert_eq!(session.view().snapshot.status, DiscoveryStatus::Closed);
        assert_eq!(session.view().snapshot.generation, closed_generation);
        assert!(!reaped.is_reaped());
    }
    for &index in &held {
        assert_eq!(
            exclusive_read(&files[index]).unwrap_err().raw_os_error(),
            Some(32),
            "cancel/close must not release the held source before worker exit"
        );
    }
    gate.release();
    finish_close(&session);
    assert!(
        reaped.is_reaped(),
        "successful reopen follows the real reaper join"
    );
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Idle);
    assert!(session.view().snapshot.verified_duplicate_groups.is_empty());
    for path in &files {
        drop(exclusive_read(path).expect("post-join exclusive open must succeed"));
    }
    let warm = session
        .start_scan(vec![directory.path().to_owned()])
        .unwrap();
    assert!(warm > closed_generation && closed_generation > operation);
    wait_for_current_complete(&session, warm);
    assert_eq!(session.view().snapshot.verified_duplicate_groups.len(), 1);
    let warm_reaped = session.reaper_probe_for_test().unwrap();
    session.close();
    finish_close(&session);
    assert!(warm_reaped.is_reaped());
    directory
        .close()
        .expect("all task-owned fixtures are removed after joins");
    eprintln!(
        "G09_NATIVE_HANDLE {}",
        serde_json::json!({
            "owner":"DuplicateFinderSession", "fixture_files":files.len(),
            "fixture_bytes":files.len()*payload.len(), "baseline_exclusive_opens":files.len(),
            "held_source_indices":held, "held_raw_os_error":32,
            "cancel_close_still_held":true, "successful_reaper_join":true,
            "post_join_exclusive_opens":files.len(), "warm_duplicate_groups":1,
            "warm_worker_joined":true, "cleanup":"fixture directory removed",
            "gate":"native source acquired; before first file.read",
            "in_flight_ReadFile":"NOT_RUN", "capture_apartment":"NOT_RUN"
        })
    );
}

#[test]
fn dff_lifecycle_coalesces_pending_scans_and_reopens_only_after_retirement() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let _release = ReleaseScanOnDrop(release_tx.clone());
    let job = Arc::new(GatedJob::new(started_tx, release_rx));
    let session = DuplicateFinderSession::with_test_job(DiscoveryLimits::default(), job.clone());

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

    assert_eq!(job.calls.load(Ordering::Acquire), 2);
    assert!(session.cancel().unwrap());
    let cancelled_generation = session.view().snapshot.generation;
    assert_eq!(
        session.start_scan(vec![PathBuf::from("refused")]),
        Err(DuplicateFinderSessionError::WorkerRetiring)
    );
    assert_eq!(session.view().snapshot.generation, cancelled_generation);
    assert_eq!(job.calls.load(Ordering::Acquire), 2);
    let reaped = session.reaper_probe_for_test().unwrap();
    session.close();
    assert!(session.view().closed);
    let closed_generation = session.view().snapshot.generation;
    for _ in 0..3 {
        assert_eq!(
            session.reopen(),
            Err(DuplicateFinderSessionError::WorkerRetiring)
        );
        assert_eq!(session.view().snapshot.generation, closed_generation);
        assert_eq!(session.view().snapshot.status, DiscoveryStatus::Closed);
        assert!(!reaped.is_reaped());
        assert_eq!(job.calls.load(Ordering::Acquire), 2);
    }
    send_release(&release_tx);
    finish_close(&session);
    assert!(reaped.is_reaped());
    assert_eq!(session.view().snapshot.status, DiscoveryStatus::Idle);
    assert_eq!(session.view().snapshot.generation, closed_generation);
    assert!(session.view().snapshot.roots.is_empty());

    let reopened = session
        .start_scan(vec![PathBuf::from("after-reopen")])
        .expect("same session admits a scan after actual retirement");
    let reopened_started = receive_started(&started_rx);
    assert!(reopened > closed_generation);
    assert_eq!(reopened_started.generation, reopened);
    assert_eq!(reopened_started.roots, vec![PathBuf::from("after-reopen")]);
    send_release(&release_tx);
    wait_for_current_complete(&session, reopened);
    assert_eq!(job.calls.load(Ordering::Acquire), 3);
    assert_eq!(
        session.view().snapshot.roots[0].selected_path,
        PathBuf::from("after-reopen")
    );
    let fresh_reaped = session.reaper_probe_for_test().unwrap();
    session.close();
    finish_close(&session);
    assert!(fresh_reaped.is_reaped());
}

#[test]
fn dff_lifecycle_cancel_invalidates_publication_while_io_drains() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let _release = ReleaseScanOnDrop(release_tx.clone());
    let job = Arc::new(GatedJob::new(started_tx, release_rx));
    let session = DuplicateFinderSession::with_test_job(DiscoveryLimits::default(), job.clone());

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
    assert_eq!(job.calls.load(Ordering::Acquire), 1);
    assert!(!session.reaper_probe_for_test().unwrap().is_reaped());
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
