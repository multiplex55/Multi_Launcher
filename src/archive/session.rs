//! Private, sequential owner for archive preparation and extraction.
//!
//! UI callers submit typed requests and poll value snapshots. Native opens,
//! cataloging, occupancy probes, layout validation, and extraction all run on
//! owned workers; a worker result is joined before its handles are retained or
//! released by the session.

use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use crate::file_operations::ScanCancellation;

use super::native::{NativeArchiveDestination, NativeArchiveSource};
use super::native_io::ArchiveIoControl;
use super::plan::{
    ArchiveSelection, CapturedOutputPolicy, ExtractionPlan, PlanBuildError, PlanReport,
    SelectionTarget, build_extraction_plan,
};
#[cfg(test)]
use super::writer::WriterTestGate;
use super::writer::{
    ArchiveWriteFailureKind, ArchiveWriteJob, ArchiveWriteProgress, ArchiveWriteReport,
    validate_plan_for_preview,
};
use super::{ArchiveCatalog, ArchiveLimits, ArchiveRevision};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveSessionPhase {
    Closed,
    Empty,
    Opening,
    Ready,
    PreparingPreview,
    PreviewReady,
    PreviewRejected,
    Extracting,
    Finished,
    Failed,
    Closing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveSessionFailure {
    Cancelled,
    SourceAdmission,
    CatalogRead,
    SourceChanged,
    DestinationAdmission,
    PlanAllocation,
    PlanLimit,
    Layout(ArchiveWriteFailureKind),
    PreparationStart,
    WorkerPanicked,
    WriterStart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveSessionRequestError {
    Busy,
    NoCatalog,
    NoDestination,
    NoPreview,
    StalePreview,
    GenerationExhausted,
    SpawnFailed,
    SelectionLimit,
    SelectionAllocation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct PreviewToken {
    session_id: u64,
    session_generation: u64,
    source_revision: ArchiveRevision,
    destination_revision: u64,
    selection_revision: u64,
    preview_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArchivePreviewSummary {
    pub token: PreviewToken,
    pub output_count: usize,
    pub file_count: usize,
    pub directory_count: usize,
    pub declared_output_bytes: u64,
    pub captured_policy: CapturedOutputPolicy,
    pub requires_destination_confirmation: bool,
}

#[derive(Clone)]
pub(crate) struct ArchiveSessionSnapshot {
    pub phase: ArchiveSessionPhase,
    pub source_revision: Option<ArchiveRevision>,
    pub destination_revision: u64,
    pub selection_revision: u64,
    pub has_destination: bool,
    pub catalog: Option<Arc<ArchiveCatalog>>,
    pub selection: Arc<[SelectionTarget]>,
    pub preview: Option<ArchivePreviewSummary>,
    pub plan_issues: Option<Arc<PlanReport>>,
    pub failure: Option<ArchiveSessionFailure>,
    pub write_progress: Option<ArchiveWriteProgress>,
    pub write_report: Option<ArchiveWriteReport>,
}

pub(crate) struct ArchiveSession {
    session_id: u64,
    session_generation: u64,
    limits: ArchiveLimits,
    next_source_revision: u64,
    source_revision: Option<ArchiveRevision>,
    source: Option<NativeArchiveSource>,
    catalog: Option<Arc<ArchiveCatalog>>,
    destination_revision: u64,
    destination_path: Option<PathBuf>,
    destination: Option<NativeArchiveDestination>,
    selection_revision: u64,
    selection: ArchiveSelection,
    preview_generation: u64,
    preview: Option<StoredPreview>,
    plan_issues: Option<Arc<PlanReport>>,
    failure: Option<ArchiveSessionFailure>,
    write_report: Option<ArchiveWriteReport>,
    active: Option<ActiveWorker>,
    closed: bool,
    #[cfg(test)]
    last_worker_thread: Option<thread::ThreadId>,
    #[cfg(test)]
    before_flush_gate: Option<Arc<WriterTestGate>>,
    #[cfg(test)]
    next_open_gate: Option<Arc<PreparationTestGate>>,
    #[cfg(test)]
    next_preview_gate: Option<Arc<PreparationTestGate>>,
    #[cfg(test)]
    last_preparation_terminal_failure: Option<ArchiveSessionFailure>,
}

struct StoredPreview {
    summary: ArchivePreviewSummary,
    plan: ExtractionPlan,
}

#[cfg(test)]
struct PreparationTestGate {
    reached: SyncSender<()>,
    resume: Mutex<Receiver<()>>,
    panic_after_release: bool,
}

#[cfg(test)]
struct PreparationGateRelease(Option<SyncSender<()>>);

#[cfg(test)]
impl PreparationTestGate {
    fn new(panic_after_release: bool) -> (Arc<Self>, Receiver<()>, PreparationGateRelease) {
        let (reached_tx, reached_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::sync_channel(1);
        (
            Arc::new(Self {
                reached: reached_tx,
                resume: Mutex::new(resume_rx),
                panic_after_release,
            }),
            reached_rx,
            PreparationGateRelease(Some(resume_tx)),
        )
    }

    fn reach_and_wait(&self) {
        let _ = self.reached.try_send(());
        let _ = self
            .resume
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .recv_timeout(std::time::Duration::from_secs(15));
        if self.panic_after_release {
            panic!("controlled archive preview preparation panic");
        }
    }
}

#[cfg(test)]
impl PreparationGateRelease {
    fn release(&mut self) {
        if let Some(resume) = self.0.take() {
            let _ = resume.send(());
        }
    }
}

#[cfg(test)]
impl Drop for PreparationGateRelease {
    fn drop(&mut self) {
        self.release();
    }
}

enum ActiveWorker {
    Preparation(PreparationJob),
    Extraction {
        job: ArchiveWriteJob,
        session_generation: u64,
    },
}

struct PreparationJob {
    stamp: RevisionStamp,
    kind: PreparationKind,
    cancellation: ScanCancellation,
    io: ArchiveIoControl,
    worker: JoinHandle<PreparationCompletion>,
}

#[derive(Clone, Copy)]
enum PreparationKind {
    Open,
    Preview,
}

#[derive(Clone, Copy)]
struct RevisionStamp {
    session_generation: u64,
    source_revision: ArchiveRevision,
    destination_revision: u64,
    selection_revision: u64,
    preview_generation: u64,
}

enum PreparationCompletion {
    Opened {
        stamp: RevisionStamp,
        result: Result<(NativeArchiveSource, ArchiveCatalog), ArchiveSessionFailure>,
        #[cfg(test)]
        worker_thread: thread::ThreadId,
    },
    Previewed {
        stamp: RevisionStamp,
        source: NativeArchiveSource,
        destination: Option<NativeArchiveDestination>,
        result: Result<PreviewBuild, ArchiveSessionFailure>,
        #[cfg(test)]
        worker_thread: thread::ThreadId,
    },
}

enum PreviewBuild {
    Ready {
        plan: ExtractionPlan,
        output_count: usize,
        file_count: usize,
        directory_count: usize,
        declared_output_bytes: u64,
        captured_policy: CapturedOutputPolicy,
    },
    Rejected(Arc<PlanReport>),
}

impl ArchiveSession {
    pub(crate) fn new(limits: ArchiveLimits) -> Self {
        Self {
            session_id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            session_generation: 0,
            limits,
            next_source_revision: 0,
            source_revision: None,
            source: None,
            catalog: None,
            destination_revision: 0,
            destination_path: None,
            destination: None,
            selection_revision: 0,
            selection: ArchiveSelection::new(),
            preview_generation: 0,
            preview: None,
            plan_issues: None,
            failure: None,
            write_report: None,
            active: None,
            closed: false,
            #[cfg(test)]
            last_worker_thread: None,
            #[cfg(test)]
            before_flush_gate: None,
            #[cfg(test)]
            next_open_gate: None,
            #[cfg(test)]
            next_preview_gate: None,
            #[cfg(test)]
            last_preparation_terminal_failure: None,
        }
    }

    pub(crate) fn request_open(&mut self, path: PathBuf) -> Result<(), ArchiveSessionRequestError> {
        if self.active.is_some() {
            return Err(ArchiveSessionRequestError::Busy);
        }

        let next_source_revision = self
            .next_source_revision
            .checked_add(1)
            .ok_or(ArchiveSessionRequestError::GenerationExhausted)?;
        let next_selection_revision = next_revision(self.selection_revision)?;
        let next_preview_generation = next_revision(self.preview_generation)?;
        let next_session_generation = next_revision(self.session_generation)?;
        self.closed = false;
        self.session_generation = next_session_generation;
        self.next_source_revision = next_source_revision;
        self.preview_generation = next_preview_generation;
        self.selection_revision = next_selection_revision;
        self.selection = ArchiveSelection::new();
        self.source = None;
        self.catalog = None;
        self.preview = None;
        self.plan_issues = None;
        self.failure = None;
        self.write_report = None;
        #[cfg(test)]
        {
            self.last_preparation_terminal_failure = None;
        }
        let source_revision = ArchiveRevision::new(next_source_revision);
        self.source_revision = Some(source_revision);
        let stamp = self.stamp(source_revision);
        let cancellation = ScanCancellation::new();
        let io = ArchiveIoControl::new(cancellation.clone());
        let worker_io = io.clone();
        let worker_cancellation = cancellation.clone();
        let limits = self.limits;
        #[cfg(test)]
        let open_gate = self.next_open_gate.take();
        let worker = thread::Builder::new()
            .name("archive-open-catalog".to_owned())
            .spawn(move || {
                #[cfg(test)]
                let worker_thread = thread::current().id();
                let result = if worker_cancellation.is_cancelled() {
                    Err(ArchiveSessionFailure::Cancelled)
                } else {
                    match NativeArchiveSource::open(&path, source_revision, limits) {
                        Err(_) if worker_cancellation.is_cancelled() => {
                            Err(ArchiveSessionFailure::Cancelled)
                        }
                        Err(_) => Err(ArchiveSessionFailure::SourceAdmission),
                        Ok(_source) if worker_cancellation.is_cancelled() => {
                            Err(ArchiveSessionFailure::Cancelled)
                        }
                        Ok(mut source) => {
                            #[cfg(test)]
                            if let Some(gate) = open_gate {
                                gate.reach_and_wait();
                            }
                            if worker_cancellation.is_cancelled() {
                                Err(ArchiveSessionFailure::Cancelled)
                            } else {
                                source
                                    .read_catalog_with_io(&worker_cancellation, &worker_io)
                                    .map(|catalog| (source, catalog))
                                    .map_err(|_| {
                                        if worker_cancellation.is_cancelled() {
                                            ArchiveSessionFailure::Cancelled
                                        } else {
                                            ArchiveSessionFailure::CatalogRead
                                        }
                                    })
                            }
                        }
                    }
                };
                PreparationCompletion::Opened {
                    stamp,
                    result,
                    #[cfg(test)]
                    worker_thread,
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(_) => {
                self.source_revision = None;
                self.failure = Some(ArchiveSessionFailure::PreparationStart);
                return Err(ArchiveSessionRequestError::SpawnFailed);
            }
        };
        self.active = Some(ActiveWorker::Preparation(PreparationJob {
            stamp,
            kind: PreparationKind::Open,
            cancellation,
            io,
            worker,
        }));
        Ok(())
    }

    pub(crate) fn select_destination(
        &mut self,
        path: PathBuf,
    ) -> Result<(), ArchiveSessionRequestError> {
        if matches!(self.active.as_ref(), Some(ActiveWorker::Extraction { .. })) {
            return Err(ArchiveSessionRequestError::Busy);
        }
        let destination_revision = next_revision(self.destination_revision)?;
        let preview_generation = next_revision(self.preview_generation)?;
        self.destination_revision = destination_revision;
        self.preview_generation = preview_generation;
        self.destination_path = Some(path);
        self.destination = None;
        self.preview = None;
        self.plan_issues = None;
        self.failure = None;
        Ok(())
    }

    pub(crate) fn set_selection(
        &mut self,
        selection: ArchiveSelection,
    ) -> Result<(), ArchiveSessionRequestError> {
        if matches!(self.active.as_ref(), Some(ActiveWorker::Extraction { .. })) {
            return Err(ArchiveSessionRequestError::Busy);
        }
        if self.catalog.is_none() {
            return Err(ArchiveSessionRequestError::NoCatalog);
        }
        let selection_revision = next_revision(self.selection_revision)?;
        let preview_generation = next_revision(self.preview_generation)?;
        self.selection_revision = selection_revision;
        self.preview_generation = preview_generation;
        self.selection = selection;
        self.preview = None;
        self.plan_issues = None;
        self.failure = None;
        Ok(())
    }

    pub(crate) fn select_all(&mut self) -> Result<(), ArchiveSessionRequestError> {
        if matches!(self.active.as_ref(), Some(ActiveWorker::Extraction { .. })) {
            return Err(ArchiveSessionRequestError::Busy);
        }
        let catalog = self
            .catalog
            .as_ref()
            .ok_or(ArchiveSessionRequestError::NoCatalog)?;
        let selection = ArchiveSelection::all_physical(catalog).map_err(|error| match error {
            super::plan::SelectionAddError::LimitExceeded => {
                ArchiveSessionRequestError::SelectionLimit
            }
            super::plan::SelectionAddError::Allocation => {
                ArchiveSessionRequestError::SelectionAllocation
            }
        })?;
        self.set_selection(selection)
    }

    pub(crate) fn clear_selection(&mut self) -> Result<(), ArchiveSessionRequestError> {
        self.set_selection(ArchiveSelection::new())
    }

    pub(crate) fn request_preview(&mut self) -> Result<(), ArchiveSessionRequestError> {
        if self.active.is_some() {
            return Err(ArchiveSessionRequestError::Busy);
        }
        let Some(source_revision) = self.source_revision else {
            return Err(ArchiveSessionRequestError::NoCatalog);
        };
        let preview_generation = next_revision(self.preview_generation)?;
        if self.catalog.is_none() {
            return Err(ArchiveSessionRequestError::NoCatalog);
        }
        if self.destination_path.is_none() {
            return Err(ArchiveSessionRequestError::NoDestination);
        }
        let Some(source) = self.source.take() else {
            return Err(ArchiveSessionRequestError::NoCatalog);
        };
        let Some(catalog) = self.catalog.as_ref().cloned() else {
            self.source = Some(source);
            return Err(ArchiveSessionRequestError::NoCatalog);
        };
        let Some(destination_path) = self.destination_path.clone() else {
            self.source = Some(source);
            return Err(ArchiveSessionRequestError::NoDestination);
        };
        self.preview_generation = preview_generation;
        self.preview = None;
        self.plan_issues = None;
        self.failure = None;
        #[cfg(test)]
        {
            self.last_preparation_terminal_failure = None;
        }
        let stamp = self.stamp(source_revision);
        let selection = self.selection.clone();
        let existing_destination = self.destination.take();
        let cancellation = ScanCancellation::new();
        let io = ArchiveIoControl::new(cancellation.clone());
        let worker_io = io.clone();
        let worker_cancellation = cancellation.clone();
        #[cfg(test)]
        let preview_gate = self.next_preview_gate.take();
        let worker = thread::Builder::new()
            .name("archive-preview".to_owned())
            .spawn(move || {
                #[cfg(test)]
                let worker_thread = thread::current().id();
                let mut source = source;
                let mut destination = existing_destination;
                let result = if worker_cancellation.is_cancelled() {
                    Err(ArchiveSessionFailure::Cancelled)
                } else {
                    if destination.is_none() {
                        destination = NativeArchiveDestination::open(&destination_path).ok();
                    }
                    match destination.as_ref() {
                        None => Err(ArchiveSessionFailure::DestinationAdmission),
                        Some(destination) => {
                            #[cfg(test)]
                            if let Some(gate) = preview_gate {
                                gate.reach_and_wait();
                            }
                            build_preview(
                                &mut source,
                                &catalog,
                                destination,
                                &selection,
                                &worker_cancellation,
                                &worker_io,
                            )
                        }
                    }
                };
                PreparationCompletion::Previewed {
                    stamp,
                    source,
                    destination,
                    result,
                    #[cfg(test)]
                    worker_thread,
                }
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(_) => {
                self.source_revision = None;
                self.catalog = None;
                self.failure = Some(ArchiveSessionFailure::PreparationStart);
                return Err(ArchiveSessionRequestError::SpawnFailed);
            }
        };
        self.active = Some(ActiveWorker::Preparation(PreparationJob {
            stamp,
            kind: PreparationKind::Preview,
            cancellation,
            io,
            worker,
        }));
        Ok(())
    }

    pub(crate) fn confirm(
        &mut self,
        token: PreviewToken,
    ) -> Result<(), ArchiveSessionRequestError> {
        if self.active.is_some() {
            return Err(ArchiveSessionRequestError::Busy);
        }
        let Some(preview) = self.preview.as_ref() else {
            return Err(if self.catalog.is_some() {
                ArchiveSessionRequestError::StalePreview
            } else {
                ArchiveSessionRequestError::NoPreview
            });
        };
        if token != preview.summary.token || !self.token_is_current(token) {
            return Err(ArchiveSessionRequestError::StalePreview);
        }
        if self.source.is_none() {
            return Err(ArchiveSessionRequestError::NoCatalog);
        }
        if self.catalog.is_none() {
            return Err(ArchiveSessionRequestError::NoCatalog);
        }
        if self.destination.is_none() {
            return Err(ArchiveSessionRequestError::NoDestination);
        }
        let selection_revision = next_revision(self.selection_revision)?;
        let session_generation = next_revision(self.session_generation)?;
        let Some(preview) = self.preview.take() else {
            return Err(ArchiveSessionRequestError::StalePreview);
        };
        let Some(source) = self.source.take() else {
            return Err(ArchiveSessionRequestError::NoCatalog);
        };
        let Some(catalog) = self.catalog.take() else {
            self.source = Some(source);
            return Err(ArchiveSessionRequestError::NoCatalog);
        };
        let Some(destination) = self.destination.take() else {
            self.source = Some(source);
            self.catalog = Some(catalog);
            return Err(ArchiveSessionRequestError::NoDestination);
        };
        self.plan_issues = None;
        self.failure = None;
        self.write_report = None;
        self.source_revision = None;
        self.session_generation = session_generation;
        self.selection_revision = selection_revision;
        self.selection = ArchiveSelection::new();
        let cancellation = ScanCancellation::new();
        #[cfg(test)]
        let job = if let Some(gate) = self.before_flush_gate.take() {
            ArchiveWriteJob::start_shared_with_before_flush_gate(
                source,
                catalog,
                preview.plan,
                destination,
                cancellation,
                gate,
            )
        } else {
            ArchiveWriteJob::start_shared(source, catalog, preview.plan, destination, cancellation)
        };
        #[cfg(not(test))]
        let job =
            ArchiveWriteJob::start_shared(source, catalog, preview.plan, destination, cancellation);
        match job {
            Ok(job) => {
                self.active = Some(ActiveWorker::Extraction {
                    job,
                    session_generation: self.session_generation,
                });
                Ok(())
            }
            Err(_) => {
                self.failure = Some(ArchiveSessionFailure::WriterStart);
                Err(ArchiveSessionRequestError::SpawnFailed)
            }
        }
    }

    pub(crate) fn cancel(&mut self) {
        self.preview_generation = self.preview_generation.saturating_add(1);
        self.preview = None;
        self.plan_issues = None;
        self.failure = Some(ArchiveSessionFailure::Cancelled);
        match self.active.as_mut() {
            Some(ActiveWorker::Preparation(job)) => {
                if matches!(job.kind, PreparationKind::Open) {
                    self.source_revision = None;
                    self.catalog = None;
                    self.selection = ArchiveSelection::new();
                }
                job.cancellation.cancel();
                job.io.cancel();
            }
            Some(ActiveWorker::Extraction { job, .. }) => job.cancel(),
            None => {}
        }
    }

    pub(crate) fn close(&mut self) {
        self.closed = true;
        self.session_generation = self.session_generation.saturating_add(1);
        self.source_revision = None;
        self.destination_revision = self.destination_revision.saturating_add(1);
        self.selection_revision = self.selection_revision.saturating_add(1);
        self.preview_generation = self.preview_generation.saturating_add(1);
        self.source = None;
        self.catalog = None;
        self.destination_path = None;
        self.destination = None;
        self.selection = ArchiveSelection::new();
        self.preview = None;
        self.plan_issues = None;
        self.failure = None;
        self.write_report = None;
        self.cancel_active();
    }

    pub(crate) fn poll(&mut self) -> bool {
        let preparation_finished = matches!(
            self.active.as_ref(),
            Some(ActiveWorker::Preparation(job)) if job.worker.is_finished()
        );
        if preparation_finished {
            let Some(ActiveWorker::Preparation(job)) = self.active.take() else {
                return false;
            };
            let PreparationJob {
                stamp,
                cancellation,
                io,
                worker,
                ..
            } = job;
            let completion = worker.join();
            drop((cancellation, io));
            match completion {
                Ok(completion) => {
                    #[cfg(test)]
                    {
                        self.last_preparation_terminal_failure = match &completion {
                            PreparationCompletion::Opened { result, .. } => {
                                result.as_ref().err().copied()
                            }
                            PreparationCompletion::Previewed { result, .. } => {
                                result.as_ref().err().copied()
                            }
                        };
                    }
                    self.apply_preparation_completion(completion)
                }
                Err(_) => {
                    #[cfg(test)]
                    {
                        self.last_preparation_terminal_failure =
                            Some(ArchiveSessionFailure::WorkerPanicked);
                    }
                    let source_owner_current = self.source_owner_is_current(stamp);
                    if source_owner_current {
                        self.source_revision = None;
                        self.source = None;
                        self.catalog = None;
                        self.preview = None;
                        self.plan_issues = None;
                        self.failure = Some(ArchiveSessionFailure::WorkerPanicked);
                    }
                    #[cfg(test)]
                    {
                        self.last_worker_thread = None;
                    }
                }
            }
            return true;
        }

        let report = match self.active.as_mut() {
            Some(ActiveWorker::Extraction { job, .. }) => job.try_join_finished(),
            _ => None,
        };
        if let Some(report) = report {
            let result_generation = match self.active.as_ref() {
                Some(ActiveWorker::Extraction {
                    session_generation, ..
                }) => *session_generation,
                _ => self.session_generation,
            };
            self.active = None;
            if !self.closed && result_generation == self.session_generation {
                self.failure = None;
                self.write_report = Some(report);
            }
            return true;
        }
        false
    }

    pub(crate) fn snapshot(&self) -> ArchiveSessionSnapshot {
        if self.closed && self.active.is_some() {
            return self.snapshot_with_phase(ArchiveSessionPhase::Closing, None);
        }
        let (phase, write_progress) = match self.active.as_ref() {
            Some(ActiveWorker::Preparation(job)) => (
                match job.kind {
                    PreparationKind::Open => ArchiveSessionPhase::Opening,
                    PreparationKind::Preview => ArchiveSessionPhase::PreparingPreview,
                },
                None,
            ),
            Some(ActiveWorker::Extraction { job, .. }) => {
                (ArchiveSessionPhase::Extracting, Some(job.progress()))
            }
            None if self.closed => (ArchiveSessionPhase::Closed, None),
            None if self.preview.is_some() => (ArchiveSessionPhase::PreviewReady, None),
            None if self.plan_issues.is_some() => (ArchiveSessionPhase::PreviewRejected, None),
            None if self.failure.is_some() => (ArchiveSessionPhase::Failed, None),
            None if self.write_report.is_some() => (ArchiveSessionPhase::Finished, None),
            None if self.catalog.is_some() => (ArchiveSessionPhase::Ready, None),
            None => (ArchiveSessionPhase::Empty, None),
        };
        self.snapshot_with_phase(phase, write_progress)
    }

    pub(crate) fn has_active_worker(&self) -> bool {
        self.active.is_some()
    }

    fn snapshot_with_phase(
        &self,
        phase: ArchiveSessionPhase,
        write_progress: Option<ArchiveWriteProgress>,
    ) -> ArchiveSessionSnapshot {
        ArchiveSessionSnapshot {
            phase,
            source_revision: self.source_revision,
            destination_revision: self.destination_revision,
            selection_revision: self.selection_revision,
            has_destination: self.destination_path.is_some(),
            catalog: self.catalog.clone(),
            selection: Arc::from(self.selection.targets().to_vec()),
            preview: self.preview.as_ref().map(|preview| preview.summary.clone()),
            plan_issues: self.plan_issues.clone(),
            failure: self.failure,
            write_progress,
            write_report: self.write_report.clone(),
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.close();
        let Some(active) = self.active.take() else {
            return;
        };
        match active {
            ActiveWorker::Preparation(job) => {
                let PreparationJob {
                    cancellation,
                    io,
                    worker,
                    ..
                } = job;
                cancellation.cancel();
                io.cancel();
                let _ = worker.join();
                drop((cancellation, io));
            }
            ActiveWorker::Extraction { job, .. } => {
                job.cancel();
                let _ = job.join();
            }
        }
    }

    fn apply_preparation_completion(&mut self, completion: PreparationCompletion) {
        match completion {
            PreparationCompletion::Opened {
                stamp,
                result,
                #[cfg(test)]
                worker_thread,
            } => {
                #[cfg(test)]
                {
                    self.last_worker_thread = Some(worker_thread);
                }
                if self.closed
                    || self.session_generation != stamp.session_generation
                    || self.source_revision != Some(stamp.source_revision)
                {
                    return;
                }
                match result {
                    Ok((source, catalog)) => {
                        let selection = ArchiveSelection::all_physical(&catalog);
                        self.source = Some(source);
                        self.catalog = Some(Arc::new(catalog));
                        match selection {
                            Ok(selection) => {
                                match (
                                    next_revision(self.selection_revision),
                                    next_revision(self.preview_generation),
                                ) {
                                    (Ok(selection_revision), Ok(preview_generation)) => {
                                        self.selection_revision = selection_revision;
                                        self.preview_generation = preview_generation;
                                        self.selection = selection;
                                        self.failure = None;
                                    }
                                    _ => self.failure = Some(ArchiveSessionFailure::PlanLimit),
                                }
                            }
                            Err(super::plan::SelectionAddError::LimitExceeded) => {
                                self.failure = Some(ArchiveSessionFailure::PlanLimit)
                            }
                            Err(super::plan::SelectionAddError::Allocation) => {
                                self.failure = Some(ArchiveSessionFailure::PlanAllocation)
                            }
                        }
                    }
                    Err(failure) => {
                        self.source_revision = None;
                        self.source = None;
                        self.catalog = None;
                        self.failure = Some(failure);
                    }
                }
            }
            PreparationCompletion::Previewed {
                stamp,
                source,
                destination,
                result,
                #[cfg(test)]
                worker_thread,
            } => {
                #[cfg(test)]
                {
                    self.last_worker_thread = Some(worker_thread);
                }
                let source_current = !self.closed
                    && self.session_generation == stamp.session_generation
                    && self.source_revision == Some(stamp.source_revision);
                let destination_current =
                    !self.closed && self.destination_revision == stamp.destination_revision;
                let selection_current = self.selection_revision == stamp.selection_revision;
                let preview_current = self.preview_generation == stamp.preview_generation
                    && self.session_generation == stamp.session_generation;
                if source_current {
                    self.source = Some(source);
                }
                if destination_current {
                    self.destination = destination;
                }
                if !source_current {
                    return;
                }
                if matches!(&result, Err(ArchiveSessionFailure::SourceChanged)) {
                    self.source_revision = None;
                    self.source = None;
                    self.catalog = None;
                }
                if !destination_current || !selection_current || !preview_current {
                    return;
                }
                match result {
                    Ok(PreviewBuild::Ready {
                        plan,
                        output_count,
                        file_count,
                        directory_count,
                        declared_output_bytes,
                        captured_policy,
                    }) => {
                        let token = PreviewToken {
                            session_id: self.session_id,
                            session_generation: stamp.session_generation,
                            source_revision: stamp.source_revision,
                            destination_revision: stamp.destination_revision,
                            selection_revision: stamp.selection_revision,
                            preview_generation: stamp.preview_generation,
                        };
                        self.preview = Some(StoredPreview {
                            summary: ArchivePreviewSummary {
                                token,
                                output_count,
                                file_count,
                                directory_count,
                                declared_output_bytes,
                                captured_policy,
                                requires_destination_confirmation: true,
                            },
                            plan,
                        });
                        self.plan_issues = None;
                        self.failure = None;
                    }
                    Ok(PreviewBuild::Rejected(report)) => {
                        self.preview = None;
                        self.plan_issues = Some(report);
                        self.failure = None;
                    }
                    Err(failure) => {
                        self.preview = None;
                        self.plan_issues = None;
                        self.failure = Some(failure);
                    }
                }
            }
        }
    }

    fn stamp(&self, source_revision: ArchiveRevision) -> RevisionStamp {
        RevisionStamp {
            session_generation: self.session_generation,
            source_revision,
            destination_revision: self.destination_revision,
            selection_revision: self.selection_revision,
            preview_generation: self.preview_generation,
        }
    }

    fn source_owner_is_current(&self, stamp: RevisionStamp) -> bool {
        !self.closed
            && self.session_generation == stamp.session_generation
            && self.source_revision == Some(stamp.source_revision)
    }

    fn token_is_current(&self, token: PreviewToken) -> bool {
        !self.closed
            && token.session_id == self.session_id
            && token.session_generation == self.session_generation
            && self.source_revision == Some(token.source_revision)
            && self.destination_revision == token.destination_revision
            && self.selection_revision == token.selection_revision
            && self.preview_generation == token.preview_generation
    }

    fn cancel_active(&mut self) {
        match self.active.as_mut() {
            Some(ActiveWorker::Preparation(job)) => {
                job.cancellation.cancel();
                job.io.cancel();
            }
            Some(ActiveWorker::Extraction { job, .. }) => job.cancel(),
            None => {}
        }
    }
}

impl Drop for ArchiveSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn next_revision(value: u64) -> Result<u64, ArchiveSessionRequestError> {
    value
        .checked_add(1)
        .ok_or(ArchiveSessionRequestError::GenerationExhausted)
}

fn build_preview(
    source: &mut NativeArchiveSource,
    catalog: &ArchiveCatalog,
    destination: &NativeArchiveDestination,
    selection: &ArchiveSelection,
    cancellation: &ScanCancellation,
    io: &ArchiveIoControl,
) -> Result<PreviewBuild, ArchiveSessionFailure> {
    if cancellation.is_cancelled() {
        return Err(ArchiveSessionFailure::Cancelled);
    }
    source
        .verify_catalog_with_io(catalog, cancellation, io)
        .map_err(|_| {
            if cancellation.is_cancelled() {
                ArchiveSessionFailure::Cancelled
            } else {
                ArchiveSessionFailure::SourceChanged
            }
        })?;
    destination
        .verify_identity()
        .map_err(|_| ArchiveSessionFailure::DestinationAdmission)?;
    let mut occupancy_probe =
        |path: &super::ValidatedRelativePath| destination.probe_occupancy(path);
    let plan =
        match build_extraction_plan(catalog, selection, Some(&mut occupancy_probe), cancellation) {
            Ok(plan) => plan,
            Err(PlanBuildError::Rejected(report)) => {
                return Ok(PreviewBuild::Rejected(Arc::new(report)));
            }
            Err(PlanBuildError::Cancelled) => return Err(ArchiveSessionFailure::Cancelled),
            Err(PlanBuildError::Allocation) => return Err(ArchiveSessionFailure::PlanAllocation),
            Err(PlanBuildError::LimitExceeded(_)) => return Err(ArchiveSessionFailure::PlanLimit),
        };
    if cancellation.is_cancelled() {
        return Err(ArchiveSessionFailure::Cancelled);
    }
    validate_plan_for_preview(source, catalog, &plan, io).map_err(|(kind, _entry)| match kind {
        ArchiveWriteFailureKind::Cancelled => ArchiveSessionFailure::Cancelled,
        ArchiveWriteFailureKind::SourceChanged => ArchiveSessionFailure::SourceChanged,
        other => ArchiveSessionFailure::Layout(other),
    })?;
    Ok(PreviewBuild::Ready {
        output_count: plan.outputs().len(),
        file_count: plan.files().count(),
        directory_count: plan.directories().count(),
        declared_output_bytes: plan.declared_total_output_bytes(),
        captured_policy: plan.policy(),
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::thread;
    use std::time::{Duration, Instant};

    use tempfile::TempDir;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    use crate::archive::{ArchiveEntryId, EntryKind};

    const SESSION_TIMEOUT: Duration = Duration::from_secs(15);

    struct Fixture {
        _temp: TempDir,
        archive: PathBuf,
        destination: PathBuf,
    }

    impl Fixture {
        fn new(entries: &[(&str, &[u8])]) -> Self {
            let temp = tempfile::tempdir().expect("create archive session fixture");
            let archive = temp.path().join("source.zip");
            let destination = temp.path().join("destination");
            fs::create_dir(&destination).expect("create fixture destination");
            let file = fs::File::create(&archive).expect("create fixture archive");
            let mut zip = ZipWriter::new(file);
            for (name, bytes) in entries {
                zip.start_file(
                    *name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .expect("start fixture ZIP entry");
                zip.write_all(bytes).expect("write fixture ZIP entry");
            }
            zip.finish().expect("finish fixture archive");
            Self {
                _temp: temp,
                archive,
                destination,
            }
        }

        fn with_physical_duplicate_names() -> Self {
            let temp = tempfile::tempdir().expect("create duplicate-name fixture");
            let archive = temp.path().join("source.zip");
            let destination = temp.path().join("destination");
            fs::create_dir(&destination).expect("create fixture destination");
            let file = fs::File::create(&archive).expect("create fixture archive");
            let mut zip = ZipWriter::new(file);
            for (name, bytes) in [("same.bin", &b"first"[..]), ("name.bin", &b"second"[..])] {
                zip.start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .expect("write unique fixture entry name");
                zip.write_all(bytes).expect("write fixture ZIP entry");
            }
            zip.finish().expect("finish fixture archive");

            // zip 9 rejects duplicate names in ZipWriter. Give it two unique
            // equal-length names, then patch both local and central records so
            // the archive contains two real physical entries with one name.
            let mut bytes = fs::read(&archive).expect("read fixture archive bytes");
            let occurrences = bytes
                .windows(b"name.bin".len())
                .enumerate()
                .filter_map(|(index, window)| (window == &b"name.bin"[..]).then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(occurrences.len(), 2, "local and central names are present");
            for index in occurrences {
                bytes[index..index + b"same.bin".len()].copy_from_slice(b"same.bin");
            }
            fs::write(&archive, bytes).expect("patch duplicate physical names");

            Self {
                _temp: temp,
                archive,
                destination,
            }
        }
    }

    fn settle(session: &mut ArchiveSession) -> ArchiveSessionSnapshot {
        let deadline = Instant::now() + SESSION_TIMEOUT;
        loop {
            session.poll();
            let snapshot = session.snapshot();
            if !matches!(
                snapshot.phase,
                ArchiveSessionPhase::Opening
                    | ArchiveSessionPhase::PreparingPreview
                    | ArchiveSessionPhase::Extracting
                    | ArchiveSessionPhase::Closing
            ) {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "archive session did not retire within the bounded fixture wait: {:?}",
                snapshot.phase
            );
            thread::yield_now();
        }
    }

    fn selected(catalog: &ArchiveCatalog, ordinals: &[usize]) -> ArchiveSelection {
        let mut selection = ArchiveSelection::new();
        let limits = catalog.limits();
        for ordinal in ordinals {
            let id = catalog
                .entries()
                .get(*ordinal)
                .expect("fixture entry ordinal exists")
                .id();
            selection
                .add(SelectionTarget::Entry(id), &limits)
                .expect("select bounded physical entry");
        }
        selection
    }

    fn start_open(session: &mut ArchiveSession, fixture: &Fixture) -> ArchiveSessionSnapshot {
        session
            .request_open(fixture.archive.clone())
            .expect("start asynchronous held-source admission and catalog");
        settle(session)
    }

    #[test]
    fn session_preview_keeps_held_handles_and_has_no_effect_before_confirmation() {
        let fixture = Fixture::new(&[("missing/nested/payload.bin", b"archive payload")]);
        let caller_thread = thread::current().id();
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        assert_eq!(opened.phase, ArchiveSessionPhase::Ready);
        assert_ne!(session.last_worker_thread, Some(caller_thread));
        let catalog = opened
            .catalog
            .expect("catalog snapshot published after join");
        let source_identity = session.source.as_ref().expect("held source").identity();
        let entry = catalog.entries()[0].id();

        session
            .select_destination(fixture.destination.clone())
            .expect("select destination path without opening it on caller");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select by physical archive identity");
        session.request_preview().expect("start background preview");
        assert!(!fixture.destination.join("missing").exists());
        let previewed = settle(&mut session);
        assert_eq!(previewed.phase, ArchiveSessionPhase::PreviewReady);
        assert_ne!(session.last_worker_thread, Some(caller_thread));
        assert_eq!(session.source.as_ref().unwrap().identity(), source_identity);
        assert!(session.destination.is_some());
        assert!(!fixture.destination.join("missing").exists());
        assert_eq!(fs::read_dir(&fixture.destination).unwrap().count(), 0);
        let summary = previewed
            .preview
            .expect("validated preview requires confirmation");
        assert!(summary.requires_destination_confirmation);
        assert_eq!(summary.output_count, 3);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.directory_count, 2);
        assert_eq!(
            summary.declared_output_bytes,
            b"archive payload".len() as u64
        );
        assert_eq!(
            summary.captured_policy.max_actual_total_output_bytes,
            8 * 1024 * 1024 * 1024
        );

        session
            .confirm(summary.token)
            .expect("confirm current preview once");
        assert_eq!(
            session.confirm(summary.token),
            Err(ArchiveSessionRequestError::Busy),
            "a second confirmation cannot start another writer"
        );
        let completed = settle(&mut session);
        assert_eq!(completed.phase, ArchiveSessionPhase::Finished);
        let report = completed.write_report.expect("joined extraction report");
        assert_eq!(
            report.status,
            super::super::writer::ArchiveWriteStatus::Completed
        );
        assert_eq!(report.published_entries, [entry]);
        assert_eq!(
            fs::read(fixture.destination.join("missing/nested/payload.bin")).unwrap(),
            b"archive payload"
        );
        assert!(session.destination.is_none());
    }

    #[test]
    fn default_and_bulk_selection_are_transactional_and_revisioned() {
        let fixture = Fixture::with_physical_duplicate_names();
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.as_ref().unwrap();
        let all: Vec<_> = catalog
            .entries()
            .iter()
            .map(|entry| SelectionTarget::Entry(entry.id()))
            .collect();
        assert_eq!(opened.selection.as_ref(), all);
        assert_ne!(catalog.entries()[0].id(), catalog.entries()[1].id());
        session
            .select_destination(fixture.destination.clone())
            .unwrap();
        session.set_selection(selected(catalog, &[0])).unwrap();
        session.request_preview().unwrap();
        let token = settle(&mut session).preview.unwrap().token;
        let revision = session.selection_revision;
        let preview_generation = session.preview_generation;
        session.clear_selection().unwrap();
        assert!(session.snapshot().preview.is_none());
        assert_eq!(
            session.confirm(token),
            Err(ArchiveSessionRequestError::StalePreview)
        );
        assert_eq!(session.selection_revision, revision + 1);
        assert_eq!(session.preview_generation, preview_generation + 1);
        for _ in 0..3 {
            session.poll();
            assert!(session.snapshot().selection.is_empty());
        }
        session.select_all().unwrap();
        assert_eq!(session.selection_revision, revision + 2);
        assert_eq!(session.preview_generation, preview_generation + 2);
        assert_eq!(session.snapshot().selection.as_ref(), all);
        session.clear_selection().unwrap();
        let reloaded = start_open(&mut session, &fixture);
        let reloaded_catalog = reloaded.catalog.unwrap();
        assert_eq!(
            reloaded.selection.as_ref(),
            reloaded_catalog
                .entries()
                .iter()
                .map(|entry| SelectionTarget::Entry(entry.id()))
                .collect::<Vec<_>>()
        );
        session
            .request_open(fixture._temp.path().join("missing.zip"))
            .unwrap();
        assert!(session.snapshot().selection.is_empty());
        let failed = settle(&mut session);
        assert!(failed.catalog.is_none());
        assert!(failed.selection.is_empty());

        let mut limits = ArchiveLimits::default();
        limits.max_selection_entries = 1;
        let mut limited = ArchiveSession::new(limits);
        let opened = start_open(&mut limited, &fixture);
        assert_eq!(opened.failure, Some(ArchiveSessionFailure::PlanLimit));
        assert!(opened.selection.is_empty());
        let catalog = opened.catalog.unwrap();
        limited.set_selection(selected(&catalog, &[0])).unwrap();
        let before = limited.snapshot();
        let generation = limited.preview_generation;
        assert_eq!(
            limited.select_all(),
            Err(ArchiveSessionRequestError::SelectionLimit)
        );
        assert_eq!(limited.snapshot().selection, before.selection);
        assert_eq!(limited.selection_revision, before.selection_revision);
        assert_eq!(limited.preview_generation, generation);
    }

    #[test]
    fn session_tokens_invalidate_on_selection_and_duplicate_names_use_physical_ids() {
        let fixture = Fixture::with_physical_duplicate_names();
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.expect("catalog ready");
        let first_id = catalog.entries()[0].id();
        let second_id = catalog.entries()[1].id();
        assert_ne!(first_id, second_id);
        assert_eq!(catalog.entry(first_id).unwrap().kind(), EntryKind::File);
        assert_eq!(catalog.entry(second_id).unwrap().kind(), EntryKind::File);
        session
            .select_destination(fixture.destination.clone())
            .expect("select destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select first physical duplicate");
        session.request_preview().expect("begin first preview");
        session
            .set_selection(selected(&catalog, &[1]))
            .expect("changing selection invalidates in-flight result");
        let stale_completion = settle(&mut session);
        assert_eq!(stale_completion.phase, ArchiveSessionPhase::Ready);
        assert!(stale_completion.preview.is_none());
        assert_eq!(
            stale_completion.selection.as_ref(),
            &[SelectionTarget::Entry(second_id)]
        );

        session
            .request_preview()
            .expect("preview second physical duplicate");
        let ready = settle(&mut session);
        let old_token = ready
            .preview
            .expect("current physical selection preview")
            .token;
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("selection change invalidates completed token");
        assert_eq!(
            session.confirm(old_token),
            Err(ArchiveSessionRequestError::StalePreview)
        );
        session
            .set_selection(selected(&catalog, &[1]))
            .expect("select second physical duplicate again");
        session
            .request_preview()
            .expect("refresh token for current selection");
        let ready = settle(&mut session);
        let token = ready.preview.expect("fresh preview token").token;
        session
            .confirm(token)
            .expect("confirm current duplicate entry");
        let completed = settle(&mut session);
        assert_eq!(
            completed.write_report.unwrap().published_entries,
            [second_id]
        );
        assert_eq!(
            fs::read(fixture.destination.join("same.bin")).unwrap(),
            b"second"
        );
    }

    #[test]
    fn session_destination_change_discards_late_preview_and_uses_current_root() {
        let fixture = Fixture::new(&[("payload.bin", b"selected payload")]);
        let replacement_destination = fixture._temp.path().join("replacement-destination");
        fs::create_dir(&replacement_destination).expect("create replacement destination");

        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.expect("catalog ready");
        session
            .select_destination(fixture.destination.clone())
            .expect("select initial destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select physical entry");
        session.request_preview().expect("begin initial preview");

        session
            .select_destination(replacement_destination.clone())
            .expect("destination revision invalidates in-flight preview");
        let stale = settle(&mut session);
        assert_eq!(stale.phase, ArchiveSessionPhase::Ready);
        assert!(stale.preview.is_none());
        assert_eq!(stale.destination_revision, 2);
        assert!(session.destination.is_none());

        session
            .request_preview()
            .expect("preview current destination");
        let preview = settle(&mut session)
            .preview
            .expect("current destination receives its own preview token");
        session
            .confirm(preview.token)
            .expect("confirm current destination token");
        assert_eq!(settle(&mut session).phase, ArchiveSessionPhase::Finished);
        assert!(!fixture.destination.join("payload.bin").exists());
        assert_eq!(
            fs::read(replacement_destination.join("payload.bin")).unwrap(),
            b"selected payload"
        );
    }

    #[test]
    fn session_source_replacement_invalidates_completed_preview_token() {
        let first = Fixture::new(&[("first.bin", b"first")]);
        let second = Fixture::new(&[("second.bin", b"second")]);
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &first);
        let catalog = opened.catalog.expect("first catalog ready");
        session
            .select_destination(first.destination.clone())
            .expect("select destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select first physical entry");
        session.request_preview().expect("prepare first preview");
        let token = settle(&mut session)
            .preview
            .expect("first source receives a preview token")
            .token;

        session
            .request_open(second.archive.clone())
            .expect("replace source after prior preparation joined");
        assert_eq!(
            session.confirm(token),
            Err(ArchiveSessionRequestError::Busy),
            "an old token cannot start work during source replacement"
        );
        let replacement = settle(&mut session);
        let replacement_catalog = replacement.catalog.expect("replacement catalog ready");
        assert_eq!(
            replacement_catalog.entries()[0].name().as_utf8(),
            Some("second.bin")
        );
        assert_eq!(
            session.confirm(token),
            Err(ArchiveSessionRequestError::StalePreview)
        );
    }

    #[test]
    fn session_stale_preview_panic_discards_lost_capability_and_requires_reload() {
        let fixture = Fixture::new(&[("payload.bin", b"payload")]);
        let replacement_destination = fixture._temp.path().join("replacement");
        fs::create_dir(&replacement_destination).expect("create replacement destination");
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.expect("catalog ready");
        session
            .select_destination(fixture.destination.clone())
            .expect("select first destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select physical entry");

        let (gate, reached, mut release) = PreparationTestGate::new(true);
        session.next_preview_gate = Some(gate);
        session
            .request_preview()
            .expect("start owned preview preparation");
        reached
            .recv_timeout(SESSION_TIMEOUT)
            .expect("preview worker holds source and destination before the gate");
        session
            .select_destination(replacement_destination)
            .expect("invalidate the in-flight preview destination revision");
        release.release();

        let failed = settle(&mut session);
        assert_eq!(failed.phase, ArchiveSessionPhase::Failed);
        assert_eq!(
            session.last_preparation_terminal_failure,
            Some(ArchiveSessionFailure::WorkerPanicked)
        );
        assert_eq!(failed.failure, Some(ArchiveSessionFailure::WorkerPanicked));
        assert_eq!(failed.source_revision, None);
        assert!(failed.catalog.is_none());
        assert!(session.source.is_none());
        assert!(session.destination.is_none());
        assert_eq!(
            session.request_preview(),
            Err(ArchiveSessionRequestError::NoCatalog),
            "a lost held source is never silently reopened for the old catalog"
        );

        let reloaded = start_open(&mut session, &fixture);
        assert_eq!(reloaded.phase, ArchiveSessionPhase::Ready);
        session.shutdown();
    }

    #[test]
    fn session_close_cancels_and_joins_before_a_new_open() {
        let fixture = Fixture::new(&[("one.bin", b"one")]);
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        session
            .request_open(fixture.archive.clone())
            .expect("start source preparation");
        let source_revision = session.source_revision;
        assert_eq!(
            session.request_open(fixture.archive.clone()),
            Err(ArchiveSessionRequestError::Busy),
            "replacement is rejected without cancelling the current preparation"
        );
        assert_eq!(session.source_revision, source_revision);
        session.close();
        assert!(matches!(
            session.snapshot().phase,
            ArchiveSessionPhase::Closing | ArchiveSessionPhase::Closed
        ));
        assert_eq!(session.snapshot().catalog, None);
        assert_eq!(
            session.request_open(fixture.archive.clone()),
            Err(ArchiveSessionRequestError::Busy),
            "a retiring preparation owner cannot be replaced"
        );
        assert_eq!(settle(&mut session).phase, ArchiveSessionPhase::Closed);
        assert!(session.snapshot().catalog.is_none());
        assert!(session.snapshot().selection.is_empty());

        let reopened = start_open(&mut session, &fixture);
        assert_eq!(reopened.phase, ArchiveSessionPhase::Ready);
        session
            .request_open(fixture.archive.clone())
            .expect("start one more preparation for shutdown");
        session.shutdown();
        assert_eq!(session.snapshot().phase, ArchiveSessionPhase::Closed);
        assert!(
            session.active.is_none(),
            "shutdown joins and releases the owner"
        );
    }

    #[test]
    fn session_cancel_retains_preparation_until_join_then_allows_reopen() {
        let fixture = Fixture::new(&[("one.bin", b"one")]);
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let (gate, reached, mut release) = PreparationTestGate::new(false);
        session.next_open_gate = Some(gate);
        session
            .request_open(fixture.archive.clone())
            .expect("start cancellable source admission and catalog");
        reached
            .recv_timeout(SESSION_TIMEOUT)
            .expect("open worker has admitted and retained the source handle");
        assert!(!session.poll(), "poll does not join an unfinished worker");
        assert!(session.active.is_some());

        session.cancel();
        let Some(ActiveWorker::Preparation(job)) = session.active.as_ref() else {
            panic!("cancel retains the preparation owner until terminal completion");
        };
        assert!(job.cancellation.is_cancelled());
        assert!(job.io.is_cancelled());
        assert!(
            !session.poll(),
            "cancellation does not retire before worker exit"
        );
        assert_eq!(
            session.request_open(fixture.archive.clone()),
            Err(ArchiveSessionRequestError::Busy),
            "replacement remains blocked while the cancelled owner is retained"
        );
        release.release();
        let cancelled = settle(&mut session);
        assert_eq!(
            session.last_preparation_terminal_failure,
            Some(ArchiveSessionFailure::Cancelled),
            "the worker's actual cancelled result is recorded before stale filtering"
        );
        assert_eq!(cancelled.failure, Some(ArchiveSessionFailure::Cancelled));
        assert!(
            session.active.is_none(),
            "poll joins before releasing the owner"
        );

        let reopened = start_open(&mut session, &fixture);
        assert_eq!(reopened.phase, ArchiveSessionPhase::Ready);
        session.shutdown();
    }

    #[test]
    fn session_cancel_retires_active_extraction_and_cleans_its_stage() {
        let fixture = Fixture::new(&[("pending.bin", b"staged but not published")]);
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.expect("catalog ready");
        let entry_id = catalog.entries()[0].id();
        session
            .select_destination(fixture.destination.clone())
            .expect("select destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select physical entry");
        session.request_preview().expect("prepare preview");
        let preview = settle(&mut session)
            .preview
            .expect("preview requires explicit confirmation");

        let (gate, reached, resume) = WriterTestGate::new();
        session.before_flush_gate = Some(gate);
        session
            .confirm(preview.token)
            .expect("start one owned extraction worker");
        reached
            .recv_timeout(SESSION_TIMEOUT)
            .expect("writer reaches its owned pre-flush gate");
        assert!(
            fs::read_dir(&fixture.destination)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".stage")),
            "the cancellation boundary follows actual stage writes"
        );

        session.cancel();
        resume.send(()).expect("release the owned writer gate");
        let cancelled = settle(&mut session);
        assert_eq!(cancelled.phase, ArchiveSessionPhase::Finished);
        let report = cancelled.write_report.expect("joined cancellation report");
        assert_eq!(
            report.status,
            super::super::writer::ArchiveWriteStatus::Cancelled
        );
        assert_eq!(report.cancelled_entry, Some(entry_id));
        assert!(report.published_entries.is_empty());
        assert!(!fixture.destination.join("pending.bin").exists());
        assert!(
            !fs::read_dir(&fixture.destination)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".stage")),
            "cancellation joins after removing only its owned stage"
        );
        session.shutdown();
    }

    #[test]
    fn session_preserves_competitor_created_after_preview() {
        let fixture = Fixture::new(&[("winner.bin", b"archive bytes")]);
        let mut session = ArchiveSession::new(ArchiveLimits::default());
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.unwrap();
        session
            .select_destination(fixture.destination.clone())
            .expect("select destination");
        session
            .set_selection(selected(&catalog, &[0]))
            .expect("select one physical entry");
        session.request_preview().expect("probe empty destination");
        let preview = settle(&mut session)
            .preview
            .expect("vacant target yields a confirmable preview");
        fs::write(fixture.destination.join("winner.bin"), b"foreign winner")
            .expect("create competitor after preview");

        session
            .confirm(preview.token)
            .expect("start no-replace writer");
        let completed = settle(&mut session);
        let report = completed.write_report.expect("writer report");
        assert_eq!(
            report.status,
            super::super::writer::ArchiveWriteStatus::Failed
        );
        assert_eq!(
            fs::read(fixture.destination.join("winner.bin")).unwrap(),
            b"foreign winner"
        );
        assert!(
            !fs::read_dir(&fixture.destination)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".stage"))
        );
    }

    #[test]
    fn session_reports_partial_run_and_keeps_completed_output() {
        let fixture = Fixture::new(&[("first.bin", b"abc"), ("second.bin", b"de")]);
        let mut limits = ArchiveLimits::default();
        limits.max_actual_total_output_bytes = 3;
        let mut session = ArchiveSession::new(limits);
        let opened = start_open(&mut session, &fixture);
        let catalog = opened.catalog.unwrap();
        session
            .select_destination(fixture.destination.clone())
            .expect("select destination");
        session
            .set_selection(selected(&catalog, &[0, 1]))
            .expect("select both physical entries");
        session.request_preview().expect("preview both entries");
        let preview = settle(&mut session).preview.expect("valid declared plan");
        assert_eq!(preview.captured_policy.max_actual_total_output_bytes, 3);

        session.confirm(preview.token).expect("confirm exact plan");
        let completed = settle(&mut session);
        let report = completed.write_report.expect("partial writer report");
        assert_eq!(
            report.status,
            super::super::writer::ArchiveWriteStatus::Partial
        );
        assert_eq!(report.published_entries, [catalog.entries()[0].id()]);
        assert_eq!(report.actual_bytes_written, 3);
        assert_eq!(
            fs::read(fixture.destination.join("first.bin")).unwrap(),
            b"abc"
        );
        assert!(!fixture.destination.join("second.bin").exists());
        assert!(
            !fs::read_dir(&fixture.destination)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".stage"))
        );
    }
}
