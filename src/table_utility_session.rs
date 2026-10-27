//! Dialog-owned asynchronous parsing, projection, selection, and export state.
//!
//! This module owns table identities, worker generations, and explicit file
//! reads. Clipboard access remains an explicit presentation action in the GUI
//! owner.

use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityRetirement, ActivitySlot, ActivityTask,
};
use crate::table_utility::{
    DelimiterChoice, RowWidthPolicy, TableAcceptedSource, TableCancellationToken, TableDelimiter,
    TableDocument, TableDocumentId, TableExportError, TableFileBackedSource, TableFilePageStop,
    TableFileParseOptions, TableFileProjectedPage, TableFileRowId, TableFileRowSelection,
    TableFileRowSelectionError, TableFileScanProgress, TableFileSourceError, TableJsonExport,
    TableJsonShape, TableParseError, TableParseErrorKind, TableParseOptions, TableProjection,
    TableProjectionError, TableProjectionOptions, TableResourceError, TableResourceLease,
    TableResourceOwned, TableResourcePolicy, TableResourcePool, TableResourceUsage, TableRowId,
    TableSelection, TableSort, TableSourceIdentity, TableTempWorkspace, TableUtilityProfile,
};
use eframe::egui;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// Inline text editing stays deliberately smaller than the full accepted
/// source limit. Large explicit paste/file sources remain intact but are
/// replaced as a whole rather than being laid out as a giant TextEdit.
pub const TABLE_INLINE_EDIT_MAX_BYTES: usize = 256 * 1024;
/// Keep multiline editor layout bounded even for tiny, newline-heavy sources.
pub const TABLE_INLINE_EDIT_MAX_LINES: usize = 1_000;
const TABLE_SOURCE_PREVIEW_CHARS: usize = 2_048;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableSessionLifetime(u64);
impl TableSessionLifetime {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableSourceRevision(u64);
impl TableSourceRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableViewRevision(u64);
impl TableViewRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableSelectionRevision(u64);
impl TableSelectionRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableExportOperation(u64);
impl TableExportOperation {
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableExportKind {
    Tsv,
    Delimited(TableDelimiter),
    Json(TableJsonShape),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableExportBasis {
    CurrentSource,
    PreviousValid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableExportPayload {
    Tsv(String),
    Delimited {
        delimiter: TableDelimiter,
        text: String,
    },
    Json(TableJsonExport),
}

#[derive(Clone)]
pub struct TableExportReceipt {
    pub operation: TableExportOperation,
    pub basis: TableExportBasis,
    pub lifetime: TableSessionLifetime,
    /// The draft revision at the time this export was requested.
    pub source_revision: TableSourceRevision,
    /// The accepted immutable source revision backing the document.
    pub accepted_source_revision: TableSourceRevision,
    pub document: TableDocumentId,
    pub parse_options: TableParseOptions,
    pub view_revision: TableViewRevision,
    pub selection_revision: TableSelectionRevision,
    pub payload: TableResourceOwned<TableExportPayload>,
}

impl std::fmt::Debug for TableExportReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TableExportReceipt")
            .field("operation", &self.operation)
            .field("basis", &self.basis)
            .field("lifetime", &self.lifetime)
            .field("source_revision", &self.source_revision)
            .field("accepted_source_revision", &self.accepted_source_revision)
            .field("document", &self.document)
            .field("parse_options", &self.parse_options)
            .field("view_revision", &self.view_revision)
            .field("selection_revision", &self.selection_revision)
            .field("payload", &self.payload)
            .finish()
    }
}

impl PartialEq for TableExportReceipt {
    fn eq(&self, other: &Self) -> bool {
        self.operation == other.operation
            && self.basis == other.basis
            && self.lifetime == other.lifetime
            && self.source_revision == other.source_revision
            && self.accepted_source_revision == other.accepted_source_revision
            && self.document == other.document
            && self.parse_options == other.parse_options
            && self.view_revision == other.view_revision
            && self.selection_revision == other.selection_revision
            && self.payload == other.payload
    }
}

impl Eq for TableExportReceipt {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingTableExport {
    operation: TableExportOperation,
    basis: TableExportBasis,
    source_revision: TableSourceRevision,
    accepted_source_revision: TableSourceRevision,
    document: TableDocumentId,
    parse_options: TableParseOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableParseState {
    Empty,
    Pending(TableSourceRevision),
    Ready(TableSourceRevision),
    Failed {
        revision: TableSourceRevision,
        error: TableParseError,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableProjectionState {
    Empty,
    Pending(TableViewRevision),
    Ready(TableViewRevision),
    Failed {
        revision: TableViewRevision,
        error: TableProjectionError,
    },
    ResourceFailed {
        revision: TableViewRevision,
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableSourceLoadState {
    Idle,
    Reading(TableSourceRevision),
    Failed {
        revision: TableSourceRevision,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobKind {
    ReadFile,
    Parse,
    Project,
    Export,
    Page,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TableTestJobCounts {
    pub parse_submitted: usize,
    pub parse_executed: usize,
    pub project_submitted: usize,
    pub project_executed: usize,
    pub scan_submitted: usize,
    pub scan_executed: usize,
    pub page_submitted: usize,
    pub page_executed: usize,
}

#[cfg(test)]
#[derive(Default)]
struct TableTestJobCounters(Mutex<TableTestJobCounts>);

#[cfg(test)]
impl TableTestJobCounters {
    fn submitted(&self, work: &Work) {
        let mut counts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        match work {
            Work::ScanFile { .. } => counts.scan_submitted += 1,
            Work::FilePage { .. } => counts.page_submitted += 1,
            Work::Parse { .. } => counts.parse_submitted += 1,
            Work::Project { .. } => counts.project_submitted += 1,
            Work::ReadFile { .. } | Work::Export { .. } => {}
        }
    }

    fn executed(&self, work: &Work) {
        let mut counts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        match work {
            Work::ScanFile { .. } => counts.scan_executed += 1,
            Work::FilePage { .. } => counts.page_executed += 1,
            Work::Parse { .. } => counts.parse_executed += 1,
            Work::Project { .. } => counts.project_executed += 1,
            Work::ReadFile { .. } | Work::Export { .. } => {}
        }
    }

    fn snapshot(&self) -> TableTestJobCounts {
        *self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[cfg(test)]
#[derive(Default)]
struct TableTestJobGateState {
    entered: bool,
    released: bool,
}

#[cfg(test)]
struct TableTestJobGate {
    kind: JobKind,
    state: Mutex<TableTestJobGateState>,
    wake: Condvar,
}

#[cfg(test)]
impl TableTestJobGate {
    fn block_until_released(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.entered = true;
        self.wake.notify_all();
        while !state.released {
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }
}

#[cfg(test)]
struct TableTestJobGateHandle(Arc<TableTestJobGate>);

#[cfg(test)]
impl TableTestJobGateHandle {
    fn wait_until_entered(&self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while !state.entered {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, result) = self
                .0
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if result.timed_out() && !state.entered {
                return false;
            }
        }
        true
    }

    fn release(&self) {
        let mut state = self
            .0
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.released = true;
        self.0.wake.notify_all();
    }
}

#[cfg(test)]
impl Drop for TableTestJobGateHandle {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JobKey {
    lifetime: TableSessionLifetime,
    source: TableSourceRevision,
    view: TableViewRevision,
    selection: TableSelectionRevision,
    kind: JobKind,
    export: Option<TableExportOperation>,
}

enum Work {
    ReadFile {
        path: PathBuf,
        options: TableParseOptions,
        pool: TableResourcePool,
        cancellation: TableCancellationToken,
    },
    ScanFile {
        path: PathBuf,
        options: TableFileParseOptions,
        pool: TableResourcePool,
        cancellation: TableCancellationToken,
    },
    FilePage {
        source: Arc<TableFileBackedSource>,
        start_row: u64,
        source_columns: Vec<usize>,
        source_columns_lease: TableResourceLease,
        cancellation: TableCancellationToken,
    },
    Parse {
        bytes: Arc<[u8]>,
        _input_lease: Arc<TableResourceLease>,
        document_lease: TableResourceLease,
        options: TableParseOptions,
        cancellation: TableCancellationToken,
    },
    Project {
        document: TableResourceOwned<TableDocument>,
        projection_lease: Arc<TableResourceLease>,
        options: TableProjectionOptions,
        cancellation: TableCancellationToken,
    },
    Export {
        document: TableResourceOwned<TableDocument>,
        projection: TableResourceOwned<TableProjection>,
        rows: Vec<TableRowId>,
        columns: Vec<usize>,
        kind: TableExportKind,
        include_headers: bool,
        output_lease: Arc<TableResourceLease>,
        cancellation: TableCancellationToken,
    },
}

struct Job {
    key: JobKey,
    work: Work,
    activity: Option<ActivityTask>,
    #[cfg(test)]
    test_drop_probe: Option<TableTestJobDropProbe>,
}

#[cfg(test)]
struct TableTestJobDropProbe {
    center: ActivityCenter,
    key: crate::activity::ActivityKey,
    pool: TableResourcePool,
    expected_usage: TableResourceUsage,
    observed_before_terminal: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(test)]
impl Drop for TableTestJobDropProbe {
    fn drop(&mut self) {
        let activity_is_nonterminal = self
            .center
            .snapshot()
            .rows
            .into_iter()
            .find(|row| row.key == self.key)
            .is_some_and(|row| !row.lifecycle.is_terminal());
        let payload_usage_was_released = self.pool.usage() == self.expected_usage;
        self.observed_before_terminal.store(
            activity_is_nonterminal && payload_usage_was_released,
            std::sync::atomic::Ordering::SeqCst,
        );
    }
}

enum WorkResult {
    FileRead(Result<(PathBuf, Vec<u8>, TableResourceLease), FileReadError>),
    FileScanned(Result<(PathBuf, Arc<TableFileBackedSource>), FileReadError>),
    FilePage(Result<TableResourceOwned<TableFileProjectedPage>, TableFileSourceError>),
    Parsed(Result<TableResourceOwned<TableDocument>, TableParseError>),
    Projected(Result<TableResourceOwned<TableProjection>, TableProjectionError>),
    Exported(Result<(TableExportPayload, Arc<TableResourceLease>), TableExportError>),
}

enum FileReadError {
    Cancelled,
    Failed(String),
}

struct Completion {
    key: JobKey,
    result: WorkResult,
}

impl Job {
    fn install_cancel_handler(&self) {
        if let Some(activity) = self.activity.as_ref() {
            let cancellation = self.cancellation();
            activity.set_cancel_handler(move || cancellation.cancel());
        }
    }

    fn cancel_before_start(self) {
        let work = self.work;
        let activity = self.activity;
        #[cfg(test)]
        let test_drop_probe = self.test_drop_probe;
        drop(work);
        #[cfg(test)]
        drop(test_drop_probe);
        if let Some(activity) = activity {
            activity.finish(
                ActivityOutcome::Cancelled,
                None,
                ActivityRetirement::Confirmed,
            );
        }
    }

    fn run<F>(self, mut scan_progress: F) -> Completion
    where
        F: FnMut(TableFileScanProgress),
    {
        let cancellation = self.cancellation();
        if let Some(activity) = self.activity.as_ref() {
            activity.mark_running();
        }
        let result = match self.work {
            Work::ReadFile {
                path,
                options,
                pool,
                cancellation,
            } => match read_compact_table_file(&path, options, &pool, &cancellation) {
                Ok((path, bytes, lease)) => WorkResult::FileRead(Ok((path, bytes, lease))),
                Err(error) => WorkResult::FileRead(Err(error)),
            },
            Work::ScanFile {
                path,
                options,
                pool,
                cancellation,
            } => WorkResult::FileScanned(scan_table_file(
                &path,
                options,
                pool,
                &cancellation,
                &mut scan_progress,
            )),
            Work::FilePage {
                source,
                start_row,
                source_columns,
                source_columns_lease,
                cancellation,
            } => {
                let result = source.projected_page(
                    start_row,
                    &source_columns,
                    crate::table_utility::TABLE_FILE_PAGE_MAX_ROWS,
                    crate::table_utility::TABLE_FILE_PAGE_MAX_BYTES,
                    crate::table_utility::TABLE_FILE_PAGE_PREVIEW_CHARS,
                    cancellation,
                );
                drop(source_columns);
                drop(source_columns_lease);
                WorkResult::FilePage(result)
            }
            Work::Parse {
                bytes,
                _input_lease,
                document_lease,
                options,
                cancellation,
            } => WorkResult::Parsed(
                TableDocument::parse_bytes_with_cancellation(&bytes, options, &cancellation).map(
                    |document| {
                        TableResourceOwned::new(Arc::new(document), Arc::new(document_lease))
                    },
                ),
            ),
            Work::Project {
                document,
                projection_lease,
                options,
                cancellation,
            } => WorkResult::Projected(
                TableProjection::build_with_cancellation(&document, &options, &cancellation).map(
                    |projection| TableResourceOwned::new(Arc::new(projection), projection_lease),
                ),
            ),
            Work::Export {
                document,
                projection,
                rows,
                columns,
                kind,
                include_headers,
                output_lease,
                cancellation,
            } => {
                let result = projection
                    .select_with_cancellation(&document, &rows, &columns, &cancellation)
                    .map_err(TableExportError::from)
                    .and_then(|selection: TableSelection| match kind {
                        TableExportKind::Tsv => projection
                            .export_tsv_with_cancellation(
                                &document,
                                &selection,
                                include_headers,
                                &cancellation,
                            )
                            .map(TableExportPayload::Tsv),
                        TableExportKind::Delimited(delimiter) => projection
                            .export_delimited_with_cancellation(
                                &document,
                                &selection,
                                delimiter,
                                include_headers,
                                &cancellation,
                            )
                            .map(|text| TableExportPayload::Delimited { delimiter, text }),
                        TableExportKind::Json(shape) => projection
                            .export_json_with_cancellation(
                                &document,
                                &selection,
                                shape,
                                &cancellation,
                            )
                            .map(TableExportPayload::Json),
                    });
                WorkResult::Exported(result.map(|payload| (payload, output_lease)))
            }
        };
        let completion = Completion {
            key: self.key,
            result,
        };
        if let Some(activity) = self.activity.as_ref() {
            let outcome = if cancellation.is_cancelled() || completion_cancelled(&completion) {
                ActivityOutcome::Cancelled
            } else if work_result_succeeded(&completion.result) {
                ActivityOutcome::Completed
            } else {
                ActivityOutcome::Failed
            };
            let diagnostic =
                (outcome == ActivityOutcome::Failed).then_some(ActivityDiagnostic::WorkerFailed);
            activity.finish(outcome, diagnostic, ActivityRetirement::Unavailable);
        }
        completion
    }
}

#[derive(Default)]
struct WorkerState {
    pending: Option<Job>,
    completed: Option<Completion>,
    active_cancellation: Option<TableCancellationToken>,
    active_key: Option<JobKey>,
    scan_progress: Option<(JobKey, TableFileScanProgress)>,
    shutdown: bool,
}

struct TableWorker {
    shared: Arc<(Mutex<WorkerState>, Condvar)>,
    join: Option<JoinHandle<()>>,
    #[cfg(test)]
    test_job_counts: Arc<TableTestJobCounters>,
    #[cfg(test)]
    test_job_gate: Arc<Mutex<Option<Arc<TableTestJobGate>>>>,
}

impl TableWorker {
    fn new(ctx: &egui::Context) -> Result<Self, String> {
        let shared = Arc::new((Mutex::new(WorkerState::default()), Condvar::new()));
        let worker_state = Arc::clone(&shared);
        let repaint = ctx.clone();
        #[cfg(test)]
        let test_job_counts = Arc::new(TableTestJobCounters::default());
        #[cfg(test)]
        let worker_job_counts = Arc::clone(&test_job_counts);
        #[cfg(test)]
        let test_job_gate = Arc::new(Mutex::new(None));
        #[cfg(test)]
        let worker_test_job_gate = Arc::clone(&test_job_gate);
        let join = thread::Builder::new()
            .name("table-utility-worker".to_owned())
            .spawn(move || {
                loop {
                    let job = {
                        let (lock, wake) = &*worker_state;
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        while state.pending.is_none() && !state.shutdown {
                            state = wake.wait(state).unwrap_or_else(|error| error.into_inner());
                        }
                        if state.shutdown {
                            return;
                        }
                        let Some(job) = state.pending.take() else {
                            continue;
                        };
                        state.active_cancellation = Some(job.cancellation());
                        state.active_key = Some(job.key);
                        job
                    };

                    #[cfg(test)]
                    worker_job_counts.executed(&job.work);
                    #[cfg(test)]
                    {
                        let gate = {
                            let mut pending = worker_test_job_gate
                                .lock()
                                .unwrap_or_else(|error| error.into_inner());
                            if pending
                                .as_ref()
                                .is_some_and(|gate| gate.kind == job.key.kind)
                            {
                                pending.take()
                            } else {
                                None
                            }
                        };
                        if let Some(gate) = gate {
                            gate.block_until_released();
                        }
                    }
                    let key = job.key;
                    let progress_state = Arc::clone(&worker_state);
                    let progress_repaint = repaint.clone();
                    let completion = job.run(move |progress| {
                        let (lock, wake) = &*progress_state;
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        if state.shutdown
                            || state.active_key != Some(key)
                            || state.pending.is_some()
                        {
                            return;
                        }
                        state.scan_progress = Some((key, progress));
                        drop(state);
                        progress_repaint.request_repaint();
                        wake.notify_one();
                    });
                    let (lock, wake) = &*worker_state;
                    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                    state.active_cancellation = None;
                    state.active_key = None;
                    if state.shutdown {
                        return;
                    }
                    let publish = state.pending.is_none() && !completion_cancelled(&completion);
                    if publish {
                        state.completed = Some(completion);
                        drop(state);
                        repaint.request_repaint();
                    } else {
                        state.scan_progress = None;
                        drop(state);
                    }
                    wake.notify_one();
                }
            })
            .map_err(|error| format!("Could not start the table worker: {error}"))?;
        Ok(Self {
            shared,
            join: Some(join),
            #[cfg(test)]
            test_job_counts,
            #[cfg(test)]
            test_job_gate,
        })
    }

    fn submit(&self, job: Job) {
        #[cfg(test)]
        self.test_job_counts.submitted(&job.work);
        job.install_cancel_handler();
        let (lock, wake) = &*self.shared;
        let replaced = {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(active) = state.active_cancellation.as_ref() {
                active.cancel();
            }
            state.completed = None;
            state.scan_progress = None;
            let replaced = state.pending.replace(job);
            wake.notify_one();
            replaced
        };
        if let Some(replaced) = replaced {
            replaced.cancel_before_start();
        }
    }

    fn cancel_pending(&self) {
        let (lock, wake) = &*self.shared;
        let pending = {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(active) = state.active_cancellation.as_ref() {
                active.cancel();
            }
            let pending = state.pending.take();
            state.completed = None;
            state.scan_progress = None;
            pending
        };
        if let Some(pending) = pending {
            pending.cancel_before_start();
        }
        wake.notify_one();
    }

    fn poll(&self) -> Option<Completion> {
        let (lock, _) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|error| error.into_inner())
            .completed
            .take()
    }

    fn poll_scan_progress(&self) -> Option<(JobKey, TableFileScanProgress)> {
        let (lock, _) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|error| error.into_inner())
            .scan_progress
            .take()
    }

    fn shutdown(&mut self) {
        let (lock, wake) = &*self.shared;
        let pending = {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.shutdown = true;
            let pending = state.pending.take();
            state.completed = None;
            state.scan_progress = None;
            if let Some(active) = state.active_cancellation.as_ref() {
                active.cancel();
            }
            pending
        };
        if let Some(pending) = pending {
            pending.cancel_before_start();
        }
        wake.notify_one();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    #[cfg(test)]
    fn test_job_counts(&self) -> TableTestJobCounts {
        self.test_job_counts.snapshot()
    }

    #[cfg(test)]
    fn hold_next_job(&self, kind: JobKind) -> TableTestJobGateHandle {
        let gate = Arc::new(TableTestJobGate {
            kind,
            state: Mutex::new(TableTestJobGateState::default()),
            wake: Condvar::new(),
        });
        let mut pending = self
            .test_job_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(
            pending.is_none(),
            "only one controlled table job gate is supported"
        );
        *pending = Some(Arc::clone(&gate));
        TableTestJobGateHandle(gate)
    }
}

impl Drop for TableWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Job {
    fn cancellation(&self) -> TableCancellationToken {
        match &self.work {
            Work::ReadFile { cancellation, .. }
            | Work::ScanFile { cancellation, .. }
            | Work::FilePage { cancellation, .. } => cancellation.clone(),
            Work::Parse { cancellation, .. }
            | Work::Project { cancellation, .. }
            | Work::Export { cancellation, .. } => cancellation.clone(),
        }
    }
}

/// Snapshot used by the UI. Arc-backed data is immutable and identified by
/// engine IDs; callers must pass those IDs back rather than reconstructing
/// them from row positions.
#[derive(Clone)]
pub struct TableSessionSnapshot {
    pub lifetime: TableSessionLifetime,
    pub source_revision: TableSourceRevision,
    pub view_revision: TableViewRevision,
    pub selection_revision: TableSelectionRevision,
    pub source: TableResourceOwned<str>,
    pub source_display_revision: u64,
    pub source_is_inline_editable: bool,
    pub source_dirty: bool,
    pub source_path: Option<PathBuf>,
    pub reading_source: bool,
    pub parse_options: TableParseOptions,
    pub parse_state: TableParseState,
    pub source_load_state: TableSourceLoadState,
    pub projection_state: TableProjectionState,
    /// The one authoritative accepted source. `document` remains a compact
    /// convenience view derived from this owner.
    pub accepted_source: Option<TableAcceptedSource>,
    pub document: Option<TableResourceOwned<TableDocument>>,
    pub accepted_source_revision: Option<TableSourceRevision>,
    pub projection: Option<TableResourceOwned<TableProjection>>,
    pub selected_rows: Option<TableResourceOwned<HashSet<TableRowId>>>,
    pub selected_columns: TableResourceOwned<HashSet<usize>>,
    pub selected_visible_row_count: usize,
    /// Bounded preview and validated count while a file-backed scan is still
    /// provisional. This is deliberately separate from `accepted_source`.
    pub file_scan_progress: Option<TableFileScanProgress>,
    /// The one bounded page retained for an accepted file-backed source.
    pub file_page: Option<TableResourceOwned<TableFileProjectedPage>>,
    pub file_page_pending: bool,
    pub file_page_start: u64,
    pub file_row_selection: Option<TableFileRowSelection>,
    pub export_pending: Option<TableExportOperation>,
    pub message: Option<String>,
}

impl TableSessionSnapshot {
    pub fn document(&self) -> Option<&TableResourceOwned<TableDocument>> {
        self.document.as_ref()
    }

    pub fn source_is_editable(&self) -> bool {
        self.source_is_inline_editable
    }

    pub fn accepted_options_are_current(&self) -> bool {
        !self.source_dirty
            && self.accepted_source_revision == Some(self.source_revision)
            && self
                .accepted_source
                .as_ref()
                .is_some_and(|source| accepted_source_matches_options(source, self.parse_options))
    }

    /// A dirty or failed draft may coexist with an explicitly exportable
    /// immutable last-good document and its original parse options.
    pub fn has_previous_valid_data(&self) -> bool {
        self.accepted_source.is_some()
            && self.accepted_source_revision.is_some()
            && (self.source_dirty || self.accepted_source_revision != Some(self.source_revision))
    }

    pub fn selected_visible_rows(&self) -> Vec<TableRowId> {
        let Some(projection) = &self.projection else {
            return Vec::new();
        };
        match &self.selected_rows {
            Some(selected) => projection
                .visible_rows()
                .iter()
                .copied()
                .filter(|row| selected.contains(row))
                .collect(),
            None => projection.visible_rows().to_vec(),
        }
    }

    pub fn selected_column_indices(&self) -> Vec<usize> {
        self.selected_columns.iter().copied().collect()
    }

    pub fn can_export(&self) -> bool {
        self.accepted_options_are_current()
            && self.projection_state == TableProjectionState::Ready(self.view_revision)
            && self.projection.as_ref().is_some_and(|projection| {
                self.document()
                    .is_some_and(|document| projection.document_id() == document.id())
                    && !projection.visible_rows().is_empty()
            })
            && self.selected_visible_row_count > 0
            && !self.selected_columns.is_empty()
    }
}

/// One retained table dialog session. Full draft text and last-good immutable
/// results are separate so failed edits never relabel a prior parse.
pub struct TableUtilitySession {
    worker: Option<TableWorker>,
    activity_center: Option<ActivityCenter>,
    lifetime: TableSessionLifetime,
    source_revision: TableSourceRevision,
    view_revision: TableViewRevision,
    selection_revision: TableSelectionRevision,
    source: Arc<str>,
    source_resource_lease: Option<Arc<TableResourceLease>>,
    source_display_revision: u64,
    source_is_inline_editable: bool,
    source_dirty: bool,
    source_path: Option<PathBuf>,
    pending_file_path: Option<PathBuf>,
    pending_file_backed_options: Option<TableFileParseOptions>,
    pending_file_admission: Option<TableResourcePool>,
    requested_source: Arc<[u8]>,
    requested_source_resource_lease: Option<Arc<TableResourceLease>>,
    requested_source_admission: Option<TableResourcePool>,
    resource_pool: TableResourcePool,
    resource_policy_error: Option<String>,
    parse_options: TableParseOptions,
    view_options: TableProjectionOptions,
    parse_state: TableParseState,
    source_load_state: TableSourceLoadState,
    projection_state: TableProjectionState,
    accepted_source: Option<TableAcceptedSource>,
    accepted_source_revision: Option<TableSourceRevision>,
    projection: Option<TableResourceOwned<TableProjection>>,
    projected_source_revision: Option<TableSourceRevision>,
    projected_view_revision: Option<TableViewRevision>,
    selected_rows: Option<TableResourceOwned<HashSet<TableRowId>>>,
    selected_visible_row_count: usize,
    selected_columns: TableResourceOwned<HashSet<usize>>,
    file_scan_progress: Option<TableFileScanProgress>,
    file_page: Option<TableResourceOwned<TableFileProjectedPage>>,
    file_page_pending: bool,
    file_page_start: u64,
    file_page_key: Option<JobKey>,
    file_row_selection: Option<TableFileRowSelection>,
    active_key: Option<JobKey>,
    next_export: u64,
    export_pending: Option<PendingTableExport>,
    export_receipt: Option<TableExportReceipt>,
    message: Option<String>,
    open: bool,
}

impl Default for TableUtilitySession {
    fn default() -> Self {
        Self::new()
    }
}

impl TableUtilitySession {
    pub fn new() -> Self {
        Self::new_with_resource_policy(Ok(TableResourcePolicy::default()))
    }

    pub fn new_with_resource_policy(
        policy: Result<TableResourcePolicy, TableResourceError>,
    ) -> Self {
        let (resource_pool, resource_policy_error) = match policy {
            Ok(policy) => (TableResourcePool::new(policy), None),
            Err(error) => (
                TableResourcePool::new(TableResourcePolicy::default()),
                Some(error.to_string()),
            ),
        };
        Self {
            worker: None,
            activity_center: None,
            lifetime: TableSessionLifetime(0),
            source_revision: TableSourceRevision(0),
            view_revision: TableViewRevision(0),
            selection_revision: TableSelectionRevision(0),
            source: Arc::from(""),
            source_resource_lease: None,
            source_display_revision: 0,
            source_is_inline_editable: true,
            source_dirty: false,
            source_path: None,
            pending_file_path: None,
            pending_file_backed_options: None,
            pending_file_admission: None,
            requested_source: Arc::from([]),
            requested_source_resource_lease: None,
            requested_source_admission: None,
            resource_pool,
            resource_policy_error,
            parse_options: TableParseOptions::utility_default(),
            view_options: TableProjectionOptions::default(),
            parse_state: TableParseState::Empty,
            source_load_state: TableSourceLoadState::Idle,
            projection_state: TableProjectionState::Empty,
            accepted_source: None,
            accepted_source_revision: None,
            projection: None,
            projected_source_revision: None,
            projected_view_revision: None,
            selected_rows: None,
            selected_visible_row_count: 0,
            selected_columns: TableResourceOwned::without_payload_charge(Arc::new(HashSet::new())),
            file_scan_progress: None,
            file_page: None,
            file_page_pending: false,
            file_page_start: 0,
            file_page_key: None,
            file_row_selection: None,
            active_key: None,
            next_export: 0,
            export_pending: None,
            export_receipt: None,
            message: None,
            open: false,
        }
    }

    pub fn attach_activity_center(&mut self, center: ActivityCenter) {
        self.activity_center = Some(center);
    }

    fn activity_task(&self, key: JobKey) -> Option<ActivityTask> {
        let center = self.activity_center.as_ref()?;
        let (slot, operation) = match key.kind {
            JobKind::ReadFile => (ActivitySlot::Primary, ActivityOperation::ReadTableFile),
            JobKind::Parse => (ActivitySlot::Primary, ActivityOperation::ParseTable),
            JobKind::Project | JobKind::Page => {
                (ActivitySlot::Secondary, ActivityOperation::ProjectTable)
            }
            JobKind::Export => (ActivitySlot::Export, ActivityOperation::ExportTable),
        };
        center.start(
            ActivityOwner::Table,
            slot,
            ActivityIncarnation::new(key.lifetime.0),
            operation,
        )
    }

    /// Apply validated persisted preferences to the shared pool. Existing
    /// leases retain their captured admission limits and usage.
    pub fn set_resource_policy(&mut self, policy: Result<TableResourcePolicy, TableResourceError>) {
        match policy {
            Ok(policy) => {
                self.resource_pool.update_policy(policy);
                self.resource_policy_error = None;
                if self.message.as_deref().is_some_and(|message| {
                    message.starts_with("Table resource preferences are invalid:")
                }) {
                    self.message = None;
                }
                self.schedule();
            }
            Err(error) => {
                let message = error.to_string();
                self.resource_policy_error = Some(message.clone());
                self.message = Some(format!("Table resource preferences are invalid: {message}"));
            }
        }
    }

    fn invalid_resource_policy_message(&self) -> Option<String> {
        self.resource_policy_error
            .as_ref()
            .map(|error| format!("Table resource preferences are invalid: {error}"))
    }

    pub fn open(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.open {
            return Ok(());
        }
        if self.worker.is_none() {
            self.worker = Some(TableWorker::new(ctx)?);
        }
        self.lifetime = TableSessionLifetime(self.lifetime.0.wrapping_add(1));
        self.open = true;
        self.active_key = None;
        self.export_pending = None;
        self.export_receipt = None;
        if self
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .is_some()
            && self.file_page.is_none()
        {
            self.file_page_pending = true;
        }
        if matches!(self.parse_state, TableParseState::Pending(_)) {
            if let Some(admission) = self.requested_source_admission.clone() {
                self.request_parse_with_admission(admission);
            } else {
                self.request_parse();
            }
        } else {
            self.schedule();
        }
        Ok(())
    }

    pub fn close(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.lifetime = TableSessionLifetime(self.lifetime.0.wrapping_add(1));
        self.active_key = None;
        self.export_pending = None;
        self.export_receipt = None;
        self.pending_file_path = None;
        self.pending_file_backed_options = None;
        self.pending_file_admission = None;
        self.file_scan_progress = None;
        self.message = None;
        if let Some(worker) = &self.worker {
            worker.cancel_pending();
        }
        if matches!(self.source_load_state, TableSourceLoadState::Reading(_)) {
            self.source_load_state = TableSourceLoadState::Idle;
            self.restore_last_parse_state();
        }
        if let TableProjectionState::Pending(_) = self.projection_state {
            self.projection_state = self
                .projected_view_revision
                .map_or(TableProjectionState::Empty, TableProjectionState::Ready);
        }
    }

    pub fn shutdown(&mut self) {
        self.close();
        if let Some(mut worker) = self.worker.take() {
            worker.shutdown();
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    fn document(&self) -> Option<&TableResourceOwned<TableDocument>> {
        self.accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::resident_document)
    }

    pub fn lifetime(&self) -> TableSessionLifetime {
        self.lifetime
    }

    pub fn replace_source(&mut self, source: String) -> Result<(), String> {
        if let Some(message) = self.invalid_resource_policy_message() {
            return Err(message);
        }
        let max_input_bytes = self
            .parse_options
            .limits
            .max_input_bytes
            .min(crate::table_utility::STRICT_CSV_MAX_INPUT_BYTES);
        if source.len() > max_input_bytes {
            return Err(format!(
                "Table source exceeds the {} byte limit; the previous source was kept.",
                max_input_bytes
            ));
        }
        let inline_editable = source_is_inline_editable(&source);
        let source_lease = reserve_source_text(&self.resource_pool, &source, inline_editable)
            .map_err(|error| error.to_string())?;
        self.advance_source_revision(true);
        self.source_is_inline_editable = inline_editable;
        self.source = Arc::from(source);
        self.source_resource_lease = Some(Arc::new(source_lease));
        self.source_display_revision = self.source_display_revision.wrapping_add(1);
        self.source_dirty = true;
        self.source_path = None;
        self.message = None;
        Ok(())
    }

    pub fn inline_source_changed(&mut self, source: String) -> Result<(), String> {
        if let Some(message) = self.invalid_resource_policy_message() {
            return Err(message);
        }
        if source.len() > TABLE_INLINE_EDIT_MAX_BYTES || !source_line_count_is_editable(&source) {
            return Err(format!(
                "Inline table edits are limited to {} bytes and {} lines; the previous source was kept. Use Paste Text or Open File for a larger source.",
                TABLE_INLINE_EDIT_MAX_BYTES, TABLE_INLINE_EDIT_MAX_LINES,
            ));
        }
        let source_lease = reserve_source_text(&self.resource_pool, &source, true)
            .map_err(|error| error.to_string())?;
        self.advance_source_revision(true);
        self.source_is_inline_editable = true;
        self.source = Arc::from(source);
        self.source_resource_lease = Some(Arc::new(source_lease));
        self.source_display_revision = self.source_display_revision.wrapping_add(1);
        self.source_dirty = true;
        self.message = None;
        Ok(())
    }

    pub fn parse_options(&self) -> TableParseOptions {
        self.parse_options
    }

    pub fn set_parse_options(&mut self, options: TableParseOptions) {
        if self.parse_options == options {
            return;
        }
        let reread = matches!(self.source_load_state, TableSourceLoadState::Reading(_))
            .then(|| {
                self.pending_file_path.clone().map(|path| {
                    (
                        path,
                        self.pending_file_backed_options,
                        self.pending_file_admission.clone(),
                    )
                })
            })
            .flatten();
        self.advance_source_revision(true);
        self.parse_options = options;
        self.source_dirty = true;
        self.message = None;
        if let Some((path, file_options, admission)) = reread {
            let result = match (file_options, admission) {
                (Some(mut file_options), Some(admission)) => {
                    file_options.delimiter = options.delimiter;
                    file_options.has_header = options.has_header;
                    file_options.strict_width = options.row_width == RowWidthPolicy::Strict;
                    self.queue_file_backed_source_with_admission(path, file_options, admission)
                }
                (None, Some(admission)) => self.queue_file_read_with_admission(path, admission),
                (None, None) => {
                    Err("The pending file read lost its resource admission.".to_owned())
                }
                (Some(_), None) => {
                    Err("The pending file scan lost its resource admission.".to_owned())
                }
            };
            if let Err(message) = result {
                let revision = self.source_revision;
                self.source_load_state = TableSourceLoadState::Failed {
                    revision,
                    message: message.clone(),
                };
                self.restore_last_parse_state();
                self.message = Some(message);
            }
        }
    }

    pub fn set_delimiter(&mut self, delimiter: DelimiterChoice) {
        self.set_parse_options(TableParseOptions {
            delimiter,
            ..self.parse_options
        });
    }

    pub fn set_has_header(&mut self, has_header: bool) {
        self.set_parse_options(TableParseOptions {
            has_header,
            ..self.parse_options
        });
    }

    pub fn view_options(&self) -> &TableProjectionOptions {
        &self.view_options
    }

    pub fn set_filter(&mut self, filter: String) {
        if self.view_options.filter != filter {
            self.view_options.filter = filter;
            self.advance_view_revision();
        }
    }

    pub fn set_sort(&mut self, sort: Option<TableSort>) {
        if self.view_options.sort != sort {
            self.view_options.sort = sort;
            self.advance_view_revision();
        }
    }

    pub fn set_filter_columns(&mut self, columns: Vec<usize>) {
        if self.view_options.filter_columns != columns {
            self.view_options.filter_columns = columns;
            self.advance_view_revision();
        }
    }

    fn advance_view_revision(&mut self) {
        self.view_revision = TableViewRevision(self.view_revision.0.wrapping_add(1));
        self.export_pending = None;
        self.export_receipt = None;
        if self.document().is_some() {
            self.projection_state = TableProjectionState::Pending(self.view_revision);
        }
        self.schedule();
    }

    pub fn request_parse(&mut self) {
        let Ok(admission) = self.capture_resource_admission() else {
            return;
        };
        self.request_parse_with_admission(admission);
    }

    fn capture_resource_admission(&mut self) -> Result<TableResourcePool, String> {
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message.clone());
            Err(message)
        } else {
            Ok(self.resource_pool.capture_policy())
        }
    }

    fn request_parse_with_admission(&mut self, admission: TableResourcePool) {
        self.advance_source_revision(true);
        let source_bytes = self.source.as_bytes();
        let input_lease = match reserve_arc_bytes(&admission, source_bytes.len()) {
            Ok(lease) => lease,
            Err(error) => {
                self.restore_last_parse_state();
                self.message = Some(format!(
                    "Could not reserve memory for the requested table parse: {error}"
                ));
                return;
            }
        };
        self.requested_source = Arc::from(self.source.as_bytes());
        self.requested_source_resource_lease = Some(Arc::new(input_lease));
        self.requested_source_admission = Some(admission);
        self.parse_state = TableParseState::Pending(self.source_revision);
        self.message = None;
        self.schedule();
    }

    pub fn paste_source(&mut self, source: String) -> Result<(), String> {
        self.replace_source(source)?;
        self.request_parse();
        Ok(())
    }

    /// Queue an explicit file selection on the session's sole worker. The
    /// retained source/document are not replaced unless bounded UTF-8 reading
    /// succeeds.
    pub fn request_file_read(&mut self, path: PathBuf) -> Result<(), String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        if self.worker.is_none() {
            return Err("The table worker is unavailable.".to_owned());
        }
        let admission = self.capture_resource_admission()?;
        self.advance_source_revision(true);
        self.source_dirty = true;
        self.queue_file_read_with_admission(path, admission)
    }

    /// Queue a fully validated file-backed source on the existing worker.
    /// The compact file-read API remains capped and unchanged for existing UI
    /// callers; this typed API is the M1-A route for larger files.
    pub fn request_file_backed_source(
        &mut self,
        path: PathBuf,
        options: TableFileParseOptions,
    ) -> Result<(), String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        if self.worker.is_none() {
            return Err("The table worker is unavailable.".to_owned());
        }
        let admission = self.capture_resource_admission()?;
        self.advance_source_revision(true);
        self.source_dirty = true;
        self.queue_file_backed_source_with_admission(path, options, admission)
    }

    /// Request one bounded page from the accepted file-backed source. A page
    /// generation is independent of the source revision, so changing pages
    /// cancels stale page work without making the accepted source dirty.
    pub fn request_file_page(&mut self, start_row: u64) -> Result<(), String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        let _admission = self.capture_resource_admission()?;
        let source = self
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .cloned()
            .ok_or_else(|| "There is no accepted file-backed table source.".to_owned())?;
        if start_row > source.row_count() {
            return Err("The requested page starts beyond the end of the table.".into());
        }
        if !self.file_page_pending
            && self.file_page_start == start_row
            && self.file_page.as_ref().is_some_and(|page| {
                page.source_id() == source.id() && page.start_row() == start_row
            })
            && self.file_page_key.is_some_and(|key| {
                Some(key.source) == self.accepted_source_revision && key.view == self.view_revision
            })
        {
            return Ok(());
        }
        if self.file_page_pending && self.file_page_start == start_row {
            return Ok(());
        }
        self.file_page_start = start_row;
        self.view_revision = TableViewRevision(self.view_revision.0.wrapping_add(1));
        self.file_page_key = None;
        self.file_page_pending = true;
        self.export_pending = None;
        self.export_receipt = None;
        self.schedule();
        Ok(())
    }

    pub fn set_file_row_selected(&mut self, row: TableFileRowId, selected: bool) {
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message);
            return;
        }
        let Some(current) = self.file_row_selection.as_ref() else {
            return;
        };
        if current.is_selected(row) == selected {
            return;
        }
        match current.set_selected(row, selected, &self.resource_pool) {
            Ok(selection) => {
                self.file_row_selection = Some(selection);
                self.selection_revision =
                    TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
                self.export_pending = None;
                self.export_receipt = None;
            }
            Err(TableFileRowSelectionError::WrongSource) => {
                self.message = Some("The selected row belongs to an older file source.".into());
            }
            Err(TableFileRowSelectionError::RowOutOfRange) => {
                self.message = Some("The selected row is outside the accepted file source.".into());
            }
            Err(error) => {
                self.message = Some(format!("Could not update the file row selection: {error}"));
            }
        }
    }

    fn queue_file_backed_source_with_admission(
        &mut self,
        path: PathBuf,
        options: TableFileParseOptions,
        admission: TableResourcePool,
    ) -> Result<(), String> {
        let Some(worker) = self.worker.as_ref() else {
            return Err("The table worker is unavailable.".to_owned());
        };
        self.source_load_state = TableSourceLoadState::Reading(self.source_revision);
        self.message = Some("Validating the selected file…".into());
        self.pending_file_path = Some(path.clone());
        self.pending_file_backed_options = Some(options);
        self.pending_file_admission = Some(admission.clone());
        let key = JobKey {
            lifetime: self.lifetime,
            source: self.source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::ReadFile,
            export: None,
        };
        let activity = self.activity_task(key);
        worker.submit(Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::ScanFile {
                path,
                options,
                pool: admission,
                cancellation: TableCancellationToken::new(),
            },
        });
        self.active_key = Some(key);
        Ok(())
    }

    fn queue_file_read_with_admission(
        &mut self,
        path: PathBuf,
        admission: TableResourcePool,
    ) -> Result<(), String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        let Some(worker) = self.worker.as_ref() else {
            return Err("The table worker is unavailable.".to_owned());
        };
        self.source_load_state = TableSourceLoadState::Reading(self.source_revision);
        self.message = Some("Reading the selected file…".into());
        self.pending_file_path = Some(path.clone());
        self.pending_file_backed_options = None;
        self.pending_file_admission = Some(admission.clone());
        let key = JobKey {
            lifetime: self.lifetime,
            source: self.source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::ReadFile,
            export: None,
        };
        let cancellation = TableCancellationToken::new();
        let activity = self.activity_task(key);
        worker.submit(Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::ReadFile {
                path,
                options: self.parse_options,
                pool: admission,
                cancellation,
            },
        });
        self.active_key = Some(key);
        Ok(())
    }

    pub fn set_selection(&mut self, rows: Option<HashSet<TableRowId>>, columns: HashSet<usize>) {
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message);
            return;
        }
        let rows = match rows {
            Some(rows) => {
                let lease = match reserve_selection_memory(
                    &self.resource_pool.capture_policy(),
                    rows.capacity(),
                ) {
                    Ok(lease) => lease,
                    Err(error) => {
                        self.message = Some(format!(
                            "Could not reserve memory for the row selection: {error}"
                        ));
                        return;
                    }
                };
                Some(TableResourceOwned::new(Arc::new(rows), Arc::new(lease)))
            }
            None => None,
        };
        let columns = match reserve_selection_memory_for::<usize>(
            &self.resource_pool.capture_policy(),
            columns.capacity(),
        ) {
            Ok(lease) => TableResourceOwned::new(Arc::new(columns), Arc::new(lease)),
            Err(error) => {
                self.message = Some(format!(
                    "Could not reserve memory for the column selection: {error}"
                ));
                return;
            }
        };
        let changed = self.selected_rows.as_deref() != rows.as_deref()
            || self.selected_columns.as_ref() != columns.as_ref();
        if changed {
            self.selected_rows = rows;
            self.selected_columns = columns;
            self.recount_selected_visible_rows();
            self.selection_revision =
                TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
            self.export_pending = None;
            self.export_receipt = None;
        }
    }

    pub fn set_row_selected(&mut self, row: TableRowId, selected: bool) {
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message);
            return;
        }
        let Some(projection) = self.projection.as_ref() else {
            return;
        };
        if !projection.visible_rows().contains(&row) {
            return;
        }
        let old_rows = self.selected_rows.as_ref();
        let (capacity, has_selected_rows) = match old_rows {
            Some(rows) => (rows.capacity().max(rows.len().saturating_add(1)), true),
            None if !selected => (projection.visible_rows().len(), false),
            None => return,
        };
        let lease = match reserve_selection_memory(&self.resource_pool.capture_policy(), capacity) {
            Ok(lease) => Arc::new(lease),
            Err(error) => {
                self.message = Some(format!(
                    "Could not reserve memory for the row selection: {error}"
                ));
                return;
            }
        };
        let mut rows = HashSet::new();
        let additional = capacity.saturating_sub(old_rows.map_or(0, |rows| rows.len()));
        if rows.try_reserve(additional).is_err() {
            self.message = Some("Could not allocate memory for the row selection.".into());
            return;
        }
        if let Some(old_rows) = old_rows {
            rows.extend(old_rows.iter().copied());
        } else {
            rows.extend(projection.visible_rows().iter().copied());
        }
        let changed = if selected {
            rows.insert(row)
        } else {
            rows.remove(&row)
        };
        if changed || !has_selected_rows {
            self.selected_rows = Some(TableResourceOwned::new(Arc::new(rows), lease));
        }
        if changed {
            self.selected_visible_row_count = if selected {
                self.selected_visible_row_count.saturating_add(1)
            } else {
                self.selected_visible_row_count.saturating_sub(1)
            };
            self.selection_revision =
                TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
            self.export_pending = None;
            self.export_receipt = None;
        }
    }

    pub fn set_column_selected(&mut self, column: usize, selected: bool) {
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message);
            return;
        }
        let column_count = self
            .document()
            .map(|document| document.column_count())
            .or_else(|| {
                self.accepted_source
                    .as_ref()
                    .and_then(TableAcceptedSource::file_source)
                    .map(|source| source.column_count())
            });
        let Some(column_count) = column_count else {
            return;
        };
        if column >= column_count {
            return;
        }
        let capacity = self
            .selected_columns
            .capacity()
            .max(self.selected_columns.len().saturating_add(1));
        let lease = match reserve_selection_memory_for::<usize>(
            &self.resource_pool.capture_policy(),
            capacity,
        ) {
            Ok(lease) => Arc::new(lease),
            Err(error) => {
                self.message = Some(format!(
                    "Could not reserve memory for the column selection: {error}"
                ));
                return;
            }
        };
        let mut columns = HashSet::new();
        let additional = capacity.saturating_sub(self.selected_columns.len());
        if columns.try_reserve(additional).is_err() {
            self.message = Some("Could not allocate memory for the column selection.".into());
            return;
        }
        columns.extend(self.selected_columns.iter().copied());
        let changed = if selected {
            columns.insert(column)
        } else {
            columns.remove(&column)
        };
        if changed {
            self.selected_columns = TableResourceOwned::new(Arc::new(columns), lease);
            self.selection_revision =
                TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
            self.export_pending = None;
            self.export_receipt = None;
            if self
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_source)
                .is_some()
            {
                self.view_revision = TableViewRevision(self.view_revision.0.wrapping_add(1));
                self.file_page_key = None;
                self.file_page_pending = true;
                self.schedule();
            }
        }
    }

    pub fn request_export(
        &mut self,
        kind: TableExportKind,
        include_headers: bool,
    ) -> Result<TableExportOperation, String> {
        self.request_export_with_basis(kind, include_headers, TableExportBasis::CurrentSource)
    }

    /// Queue an explicit export of the last accepted immutable result even
    /// while a newer source draft is dirty or failed. The generated receipt
    /// remains tied to both the accepted document and this draft revision.
    pub fn request_export_previous_valid(
        &mut self,
        kind: TableExportKind,
        include_headers: bool,
    ) -> Result<TableExportOperation, String> {
        self.request_export_with_basis(kind, include_headers, TableExportBasis::PreviousValid)
    }

    fn request_export_with_basis(
        &mut self,
        kind: TableExportKind,
        include_headers: bool,
        basis: TableExportBasis,
    ) -> Result<TableExportOperation, String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        if let Some(message) = self.invalid_resource_policy_message() {
            self.message = Some(message.clone());
            return Err(message);
        }
        if basis == TableExportBasis::PreviousValid
            && self.has_previous_valid_data()
            && self.source_work_pending()
        {
            return Err(
                "Wait for the current file read or parse to finish before exporting the previous valid table."
                    .into(),
            );
        }
        let document = self
            .document()
            .cloned()
            .ok_or_else(|| "Parse a table before exporting.".to_owned())?;
        let accepted_source_revision = self
            .accepted_source_revision
            .filter(|revision| self.projected_source_revision == Some(*revision))
            .ok_or_else(|| "Wait for an accepted table result before exporting.".to_owned())?;
        match basis {
            TableExportBasis::CurrentSource
                if accepted_source_revision != self.source_revision
                    || self.source_dirty
                    || document.options() != self.parse_options =>
            {
                return Err(
                    "Parse the current source and wait for its result view before exporting."
                        .into(),
                );
            }
            TableExportBasis::PreviousValid if !self.has_previous_valid_data() => {
                return Err("There is no previous valid table to export.".into());
            }
            _ => {}
        }
        if self.projection_state != TableProjectionState::Ready(self.view_revision)
            || self.projected_view_revision != Some(self.view_revision)
        {
            return Err("Wait for the current table view before exporting.".into());
        }
        let projection = self
            .projection
            .clone()
            .ok_or_else(|| "Wait for the current table view before exporting.".to_owned())?;
        if self.selected_columns.is_empty() {
            return Err("Select at least one column before exporting.".into());
        }
        let selected_rows = self.selected_rows.as_deref();
        let row_count = projection
            .visible_rows()
            .iter()
            .filter(|row| selected_rows.is_none_or(|selected| selected.contains(row)))
            .count();
        if row_count == 0 {
            return Err("Select at least one visible row before exporting.".into());
        }
        let output_lease = reserve_export_memory(
            &self.resource_pool.capture_policy(),
            &document,
            &projection,
            selected_rows,
            &self.selected_columns,
            include_headers,
        )
        .map_err(|error| format!("Could not reserve memory for the export: {error}"))?;
        let mut columns = self.selected_columns.iter().copied().collect::<Vec<_>>();
        columns.sort_unstable();
        let rows = match &self.selected_rows {
            Some(rows) => projection
                .visible_rows()
                .iter()
                .copied()
                .filter(|row| rows.contains(row))
                .collect::<Vec<_>>(),
            None => projection.visible_rows().to_vec(),
        };
        let operation = TableExportOperation(self.next_export);
        self.next_export = self.next_export.wrapping_add(1);
        let document_id = document.id();
        let parse_options = document.options();
        let key = JobKey {
            lifetime: self.lifetime,
            source: self.source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::Export,
            export: Some(operation),
        };
        let cancellation = TableCancellationToken::new();
        let activity = self.activity_task(key);
        let job = Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::Export {
                document,
                projection,
                rows,
                columns,
                kind,
                include_headers,
                output_lease: Arc::new(output_lease),
                cancellation,
            },
        };
        self.export_pending = Some(PendingTableExport {
            operation,
            basis,
            source_revision: self.source_revision,
            accepted_source_revision,
            document: document_id,
            parse_options,
        });
        self.export_receipt = None;
        self.active_key = Some(key);
        self.worker
            .as_ref()
            .ok_or_else(|| "The table worker is unavailable.".to_owned())?
            .submit(job);
        Ok(operation)
    }

    pub fn take_export_receipt(
        &mut self,
        operation: TableExportOperation,
    ) -> Option<TableExportReceipt> {
        if !self
            .export_receipt
            .as_ref()
            .is_some_and(|receipt| self.export_receipt_is_current(receipt))
        {
            self.export_receipt = None;
            return None;
        }
        self.export_receipt
            .take()
            .filter(|receipt| receipt.operation == operation)
    }

    pub fn export_receipt_is_current(&self, receipt: &TableExportReceipt) -> bool {
        self.open
            && receipt.lifetime == self.lifetime
            && receipt.source_revision == self.source_revision
            && Some(receipt.accepted_source_revision) == self.accepted_source_revision
            && receipt.view_revision == self.view_revision
            && receipt.selection_revision == self.selection_revision
            && self.projected_source_revision == Some(receipt.accepted_source_revision)
            && self.projected_view_revision == Some(receipt.view_revision)
            && self.document().is_some_and(|document| {
                document.id() == receipt.document && document.options() == receipt.parse_options
            })
            && match receipt.basis {
                TableExportBasis::CurrentSource => {
                    !self.source_dirty
                        && receipt.accepted_source_revision == receipt.source_revision
                        && receipt.parse_options == self.parse_options
                }
                TableExportBasis::PreviousValid => true,
            }
            && self
                .projection
                .as_ref()
                .is_some_and(|projection| projection.document_id() == receipt.document)
    }

    pub fn poll(&mut self) {
        let Some((scan_progress, completion)) = self
            .worker
            .as_ref()
            .map(|worker| (worker.poll_scan_progress(), worker.poll()))
        else {
            return;
        };
        if let Some((key, progress)) = scan_progress {
            self.accept_file_scan_progress(key, progress);
        }
        if let Some(completion) = completion {
            if self.active_key == Some(completion.key) {
                self.active_key = None;
            }
            if completion.key.lifetime == self.lifetime {
                self.accept(completion);
            }
        }
        self.schedule();
    }

    fn accept_file_scan_progress(&mut self, key: JobKey, progress: TableFileScanProgress) {
        if self.open
            && key.lifetime == self.lifetime
            && key.source == self.source_revision
            && key.kind == JobKind::ReadFile
            && self.source_load_state == TableSourceLoadState::Reading(key.source)
        {
            self.message = Some(format!(
                "Validating the selected file… {} records checked; schema currently has {} columns.",
                progress.validated_rows(),
                progress.schema_columns()
            ));
            self.file_scan_progress = Some(progress);
        }
    }

    fn accept(&mut self, completion: Completion) {
        let key = completion.key;
        match completion.result {
            WorkResult::Parsed(result) => {
                if !self.open || key.source != self.source_revision || key.kind != JobKind::Parse {
                    return;
                }
                match result {
                    Ok(document) => {
                        if document.options() != self.parse_options {
                            self.restore_last_parse_state();
                            self.requested_source = Arc::from([]);
                            self.requested_source_resource_lease = None;
                            self.requested_source_admission = None;
                            return;
                        }
                        let Some(admission) = self.requested_source_admission.clone() else {
                            self.restore_last_parse_state();
                            self.requested_source = Arc::from([]);
                            self.requested_source_resource_lease = None;
                            self.message = Some(
                                "The completed parse lost its resource admission; the previous result was kept."
                                    .into(),
                            );
                            return;
                        };
                        let selected_columns =
                            match all_selected_columns(&admission, document.column_count()) {
                                Ok(columns) => columns,
                                Err(error) => {
                                    self.restore_last_parse_state();
                                    self.requested_source = Arc::from([]);
                                    self.requested_source_resource_lease = None;
                                    self.requested_source_admission = None;
                                    self.message = Some(format!(
                                        "Could not reserve memory for the column selection: {error}"
                                    ));
                                    return;
                                }
                            };
                        self.source_dirty = false;
                        self.accepted_source_revision = Some(key.source);
                        self.parse_state = TableParseState::Ready(key.source);
                        self.selected_rows = None;
                        self.selected_visible_row_count = 0;
                        self.selected_columns = selected_columns;
                        self.selection_revision =
                            TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
                        self.file_scan_progress = None;
                        self.file_page = None;
                        self.file_page_key = None;
                        self.file_page_start = 0;
                        self.file_page_pending = false;
                        self.file_row_selection = None;
                        self.projection = None;
                        self.projected_source_revision = None;
                        self.projected_view_revision = None;
                        self.accepted_source = Some(TableAcceptedSource::Resident(document));
                        self.requested_source = Arc::from([]);
                        self.requested_source_resource_lease = None;
                        self.requested_source_admission = None;
                        self.projection_state = TableProjectionState::Pending(self.view_revision);
                        self.message = self.invalid_resource_policy_message();
                    }
                    Err(error) => {
                        if error.kind != crate::table_utility::TableParseErrorKind::Cancelled {
                            self.parse_state = TableParseState::Failed {
                                revision: key.source,
                                error,
                            };
                            self.message = Some(
                                "The edited source did not parse. The last successful result remains loaded with its original parse options.".into(),
                            );
                            self.requested_source = Arc::from([]);
                            self.requested_source_resource_lease = None;
                            self.requested_source_admission = None;
                        }
                    }
                }
            }
            WorkResult::FileRead(result) => {
                if !self.open
                    || key.kind != JobKind::ReadFile
                    || self.source_load_state != TableSourceLoadState::Reading(key.source)
                {
                    return;
                }
                self.pending_file_path = None;
                self.pending_file_backed_options = None;
                let Some(admission) = self.pending_file_admission.take() else {
                    self.source_load_state = TableSourceLoadState::Failed {
                        revision: key.source,
                        message: "The file read lost its resource admission.".to_owned(),
                    };
                    self.restore_last_parse_state();
                    self.message = Some(
                        "The file read lost its resource admission; the previous source was kept."
                            .to_owned(),
                    );
                    return;
                };
                match result {
                    Ok((path, bytes, input_lease)) => match String::from_utf8(bytes) {
                        Ok(source) => {
                            let inline_editable = source_is_inline_editable(&source);
                            match reserve_source_text(&admission, &source, inline_editable) {
                                Ok(source_lease) => {
                                    self.source_is_inline_editable = inline_editable;
                                    self.source = Arc::from(source);
                                    self.source_resource_lease = Some(Arc::new(source_lease));
                                    self.source_display_revision =
                                        self.source_display_revision.wrapping_add(1);
                                    self.source_dirty = true;
                                    self.source_path = Some(path);
                                    self.source_load_state = TableSourceLoadState::Idle;
                                    self.request_parse_with_admission(admission);
                                    self.message =
                                        Some("File loaded; parsing the current source…".into());
                                }
                                Err(error) => {
                                    let message = format!(
                                        "Could not reserve memory for the selected file; the previous source was kept: {error}"
                                    );
                                    self.source_load_state = TableSourceLoadState::Failed {
                                        revision: key.source,
                                        message: message.clone(),
                                    };
                                    self.restore_last_parse_state();
                                    self.message = Some(message);
                                }
                            }
                            drop(input_lease);
                        }
                        Err(_) => {
                            let message = "The selected file is not valid UTF-8; the previous source was kept.".to_owned();
                            self.source_load_state = TableSourceLoadState::Failed {
                                revision: key.source,
                                message: message.clone(),
                            };
                            self.restore_last_parse_state();
                            self.message = Some(message);
                        }
                    },
                    Err(FileReadError::Cancelled) => {
                        self.source_load_state = TableSourceLoadState::Idle;
                        self.restore_last_parse_state();
                    }
                    Err(FileReadError::Failed(message)) => {
                        self.source_load_state = TableSourceLoadState::Failed {
                            revision: key.source,
                            message: message.clone(),
                        };
                        self.restore_last_parse_state();
                        self.message = Some(message);
                    }
                }
            }
            WorkResult::FileScanned(result) => {
                if !self.open
                    || key.kind != JobKind::ReadFile
                    || self.source_load_state != TableSourceLoadState::Reading(key.source)
                {
                    return;
                }
                self.pending_file_path = None;
                self.pending_file_backed_options = None;
                self.file_scan_progress = None;
                let Some(admission) = self.pending_file_admission.take() else {
                    let message = "The completed file scan lost its resource admission; the previous source was kept.".to_owned();
                    self.source_load_state = TableSourceLoadState::Failed {
                        revision: key.source,
                        message: message.clone(),
                    };
                    self.restore_last_parse_state();
                    self.message = Some(message);
                    return;
                };
                match result {
                    Ok((path, source)) => {
                        let selected_columns = match all_selected_columns(
                            &admission,
                            source.column_count(),
                        ) {
                            Ok(columns) => columns,
                            Err(error) => {
                                let message = format!(
                                    "Could not reserve memory for the file-backed column selection; the previous source was kept: {error}"
                                );
                                self.source_load_state = TableSourceLoadState::Failed {
                                    revision: key.source,
                                    message: message.clone(),
                                };
                                self.restore_last_parse_state();
                                self.message = Some(message);
                                return;
                            }
                        };
                        let file_row_selection = match TableFileRowSelection::all_selected(
                            source.id(),
                            source.row_count(),
                            &admission,
                        ) {
                            Ok(selection) => selection,
                            Err(error) => {
                                let message = format!(
                                    "Could not reserve memory for the file-backed row selection; the previous source was kept: {error}"
                                );
                                self.source_load_state = TableSourceLoadState::Failed {
                                    revision: key.source,
                                    message: message.clone(),
                                };
                                self.restore_last_parse_state();
                                self.message = Some(message);
                                return;
                            }
                        };
                        self.parse_options = parse_options_for_file_source(source.options());
                        self.accepted_source = Some(TableAcceptedSource::FileBacked(source));
                        self.accepted_source_revision = Some(key.source);
                        self.parse_state = TableParseState::Ready(key.source);
                        self.source = Arc::from("");
                        self.source_resource_lease = None;
                        self.requested_source = Arc::from([]);
                        self.requested_source_resource_lease = None;
                        self.source_display_revision = self.source_display_revision.wrapping_add(1);
                        self.source_is_inline_editable = false;
                        self.source_dirty = false;
                        self.source_path = Some(path);
                        self.source_load_state = TableSourceLoadState::Idle;
                        self.projection = None;
                        self.projected_source_revision = None;
                        self.projected_view_revision = None;
                        self.projection_state = TableProjectionState::Empty;
                        self.selected_rows = None;
                        self.selected_visible_row_count = 0;
                        self.selected_columns = selected_columns;
                        self.file_row_selection = Some(file_row_selection);
                        self.file_page = None;
                        self.file_page_key = None;
                        self.file_page_start = 0;
                        self.file_page_pending = true;
                        self.selection_revision =
                            TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
                        self.message =
                            Some(self.invalid_resource_policy_message().unwrap_or_else(|| {
                                "File validation completed; loading the first bounded page…".into()
                            }));
                    }
                    Err(FileReadError::Cancelled) => {
                        self.source_load_state = TableSourceLoadState::Idle;
                        self.restore_last_parse_state();
                    }
                    Err(FileReadError::Failed(message)) => {
                        self.source_load_state = TableSourceLoadState::Failed {
                            revision: key.source,
                            message: message.clone(),
                        };
                        self.restore_last_parse_state();
                        self.message = Some(message);
                    }
                }
            }
            WorkResult::FilePage(result) => {
                let accepted_revision = self.accepted_source_revision;
                let accepted_source_id = self
                    .accepted_source
                    .as_ref()
                    .and_then(TableAcceptedSource::file_id);
                if !self.open
                    || key.kind != JobKind::Page
                    || Some(key.source) != accepted_revision
                    || key.view != self.view_revision
                    || !self.file_page_pending
                {
                    return;
                }
                match result {
                    Ok(page)
                        if Some(page.source_id()) == accepted_source_id
                            && page.start_row() == self.file_page_start =>
                    {
                        self.file_page = Some(page);
                        self.file_page_key = Some(key);
                        self.file_page_pending = false;
                        self.message = None;
                    }
                    Ok(_) => {
                        self.file_page_pending = false;
                        self.message = Some(
                            "The file-backed page did not match the accepted source; the previous page was kept."
                                .into(),
                        );
                    }
                    Err(TableFileSourceError::Parse(TableParseError {
                        kind: TableParseErrorKind::Cancelled,
                        ..
                    })) => {
                        self.file_page_pending = false;
                    }
                    Err(error) => {
                        self.file_page_pending = false;
                        self.message = Some(format!(
                            "Could not load the requested file-backed page; the previous page was kept: {error}"
                        ));
                    }
                }
            }
            WorkResult::Projected(result) => {
                if !self.open
                    || key.source
                        != self
                            .accepted_source_revision
                            .unwrap_or(TableSourceRevision(u64::MAX))
                    || key.view != self.view_revision
                    || key.kind != JobKind::Project
                {
                    return;
                }
                match result {
                    Ok(projection) => {
                        self.selected_visible_row_count = match self.selected_rows.as_ref() {
                            Some(selected) => projection
                                .visible_rows()
                                .iter()
                                .filter(|row| selected.contains(row))
                                .count(),
                            None => projection.visible_rows().len(),
                        };
                        self.projection = Some(projection);
                        self.projected_source_revision = Some(key.source);
                        self.projected_view_revision = Some(key.view);
                        self.projection_state = TableProjectionState::Ready(key.view);
                        self.message = None;
                    }
                    Err(error) => {
                        if error != TableProjectionError::Cancelled {
                            self.projection_state = TableProjectionState::Failed {
                                revision: key.view,
                                error,
                            };
                            self.message = Some(error.to_string());
                        }
                    }
                }
            }
            WorkResult::Exported(result) => {
                let Some(operation) = key.export else {
                    return;
                };
                let Some(pending) = self
                    .export_pending
                    .filter(|pending| pending.operation == operation)
                else {
                    return;
                };
                if !self.export_is_current(operation, &key)
                    || key.lifetime != self.lifetime
                    || key.view != self.view_revision
                    || key.selection != self.selection_revision
                    || key.kind != JobKind::Export
                {
                    return;
                }
                self.export_pending = None;
                match result {
                    Ok((payload, resource_lease)) => {
                        let document = self.document().map(|doc| doc.id());
                        if let Some(document) = document {
                            self.export_receipt = Some(TableExportReceipt {
                                operation,
                                basis: pending.basis,
                                lifetime: key.lifetime,
                                source_revision: pending.source_revision,
                                accepted_source_revision: pending.accepted_source_revision,
                                document,
                                parse_options: pending.parse_options,
                                view_revision: key.view,
                                selection_revision: key.selection,
                                payload: TableResourceOwned::new(Arc::new(payload), resource_lease),
                            });
                        }
                    }
                    Err(TableExportError::Cancelled) => {}
                    Err(error) => self.message = Some(error.to_string()),
                }
            }
        }
    }

    fn export_is_current(&self, operation: TableExportOperation, key: &JobKey) -> bool {
        let Some(pending) = self.export_pending else {
            return false;
        };
        self.open
            && pending.operation == operation
            && pending.source_revision == self.source_revision
            && pending.source_revision == key.source
            && self.accepted_source_revision == Some(pending.accepted_source_revision)
            && self.document().is_some_and(|document| {
                document.id() == pending.document && document.options() == pending.parse_options
            })
            && self.projected_source_revision == Some(pending.accepted_source_revision)
            && self.projected_view_revision == Some(self.view_revision)
            && match pending.basis {
                TableExportBasis::CurrentSource => {
                    !self.source_dirty
                        && pending.accepted_source_revision == self.source_revision
                        && pending.parse_options == self.parse_options
                }
                TableExportBasis::PreviousValid => self.has_previous_valid_data(),
            }
    }

    fn has_previous_valid_data(&self) -> bool {
        self.accepted_source.is_some()
            && self.accepted_source_revision.is_some()
            && (self.source_dirty || self.accepted_source_revision != Some(self.source_revision))
    }

    fn source_work_pending(&self) -> bool {
        matches!(self.source_load_state, TableSourceLoadState::Reading(_))
            || matches!(self.parse_state, TableParseState::Pending(_))
    }

    fn restore_last_parse_state(&mut self) {
        self.parse_state = self
            .accepted_source_revision
            .map_or(TableParseState::Empty, TableParseState::Ready);
    }

    /// Advance the identity of the source/options draft and revoke outputs
    /// derived from the previous draft. A pending explicit read is retained
    /// across option edits, but source edits and Parse replace it.
    fn advance_source_revision(&mut self, cancel_read: bool) {
        self.source_revision = TableSourceRevision(self.source_revision.0.wrapping_add(1));
        self.export_pending = None;
        self.export_receipt = None;
        self.requested_source = Arc::from([]);
        self.requested_source_resource_lease = None;
        self.requested_source_admission = None;
        if cancel_read {
            self.source_load_state = TableSourceLoadState::Idle;
            self.file_scan_progress = None;
            self.pending_file_path = None;
            self.pending_file_backed_options = None;
            self.pending_file_admission = None;
        }
        if matches!(self.parse_state, TableParseState::Pending(_)) {
            self.restore_last_parse_state();
        }
    }

    fn recount_selected_visible_rows(&mut self) {
        self.selected_visible_row_count =
            match (self.selected_rows.as_ref(), self.projection.as_ref()) {
                (Some(selected), Some(projection)) => projection
                    .visible_rows()
                    .iter()
                    .filter(|row| selected.contains(row))
                    .count(),
                (None, Some(projection)) => projection.visible_rows().len(),
                (_, None) => 0,
            };
    }

    fn schedule(&mut self) {
        if !self.open {
            return;
        }
        if matches!(self.source_load_state, TableSourceLoadState::Reading(_)) {
            return;
        }
        if matches!(self.parse_state, TableParseState::Pending(revision) if revision == self.source_revision)
        {
            if self.active_key.is_some_and(|key| {
                key.lifetime == self.lifetime
                    && key.source == self.source_revision
                    && matches!(key.kind, JobKind::Parse | JobKind::ReadFile)
            }) {
                return;
            }
            self.schedule_parse();
            return;
        }
        if self
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .is_some()
        {
            self.schedule_file_page();
            return;
        }
        if self.document().is_some()
            && (self.projected_source_revision != self.accepted_source_revision
                || self.projected_view_revision != Some(self.view_revision))
        {
            if self.active_key.is_some_and(|key| {
                key.lifetime == self.lifetime
                    && key.source
                        == self
                            .accepted_source_revision
                            .unwrap_or(self.source_revision)
                    && key.view == self.view_revision
                    && key.kind == JobKind::Project
            }) {
                return;
            }
            self.schedule_projection();
        }
    }

    fn schedule_file_page(&mut self) {
        if !self.file_page_pending {
            return;
        }
        let Some((source, source_revision)) = self
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .cloned()
            .zip(self.accepted_source_revision)
        else {
            self.file_page_pending = false;
            return;
        };
        if self.file_page_start > source.row_count() {
            self.file_page_pending = false;
            self.message = Some("The requested page starts beyond the end of the file.".into());
            return;
        }
        if let Some(message) = self.invalid_resource_policy_message() {
            self.file_page_pending = false;
            self.message = Some(message);
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let key = JobKey {
            lifetime: self.lifetime,
            source: source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::Page,
            export: None,
        };
        if self.active_key.is_some_and(|active| {
            active.lifetime == key.lifetime
                && active.source == key.source
                && active.view == key.view
                && active.kind == key.kind
                && active.export == key.export
        }) {
            return;
        }
        let column_admission = self.resource_pool.capture_policy();
        let source_columns_lease = match reserve_selection_memory_for::<usize>(
            &column_admission,
            self.selected_columns.len(),
        ) {
            Ok(lease) => lease,
            Err(error) => {
                self.file_page_pending = false;
                self.message = Some(format!(
                    "Could not reserve memory for the requested file page's column list: {error}"
                ));
                return;
            }
        };
        let mut source_columns = Vec::new();
        if source_columns
            .try_reserve_exact(self.selected_columns.len())
            .is_err()
        {
            self.file_page_pending = false;
            self.message =
                Some("Could not allocate the requested file page's projected columns.".into());
            return;
        }
        source_columns.extend(self.selected_columns.iter().copied());
        source_columns.sort_unstable();
        let activity = self.activity_task(key);
        worker.submit(Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::FilePage {
                source,
                start_row: self.file_page_start,
                source_columns,
                source_columns_lease,
                cancellation: TableCancellationToken::new(),
            },
        });
        self.active_key = Some(key);
    }

    fn schedule_parse(&mut self) {
        self.schedule_parse_for_revision(self.source_revision);
    }

    fn schedule_parse_for_revision(&mut self, revision: TableSourceRevision) {
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let Some(admission) = self.requested_source_admission.clone() else {
            self.restore_last_parse_state();
            self.requested_source = Arc::from([]);
            self.requested_source_resource_lease = None;
            self.message = Some("The requested parse lost its resource admission.".into());
            return;
        };
        let Some(input_lease) = self.requested_source_resource_lease.clone() else {
            self.restore_last_parse_state();
            self.requested_source = Arc::from([]);
            self.requested_source_admission = None;
            self.message = Some("Could not reserve the requested source bytes for parsing.".into());
            return;
        };
        let document_lease = match reserve_document_for_parse(
            &admission,
            self.requested_source.len(),
            self.parse_options,
        ) {
            Ok(lease) => lease,
            Err(error) => {
                self.restore_last_parse_state();
                self.requested_source = Arc::from([]);
                self.requested_source_resource_lease = None;
                self.requested_source_admission = None;
                self.message = Some(format!(
                    "Could not reserve memory for the requested table parse: {error}"
                ));
                return;
            }
        };
        let key = JobKey {
            lifetime: self.lifetime,
            source: revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::Parse,
            export: None,
        };
        let cancellation = TableCancellationToken::new();
        let activity = self.activity_task(key);
        worker.submit(Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::Parse {
                bytes: Arc::clone(&self.requested_source),
                _input_lease: input_lease,
                document_lease,
                options: self.parse_options,
                cancellation,
            },
        });
        self.active_key = Some(key);
    }

    fn schedule_projection(&mut self) {
        let (Some(document), Some(source)) =
            (self.document().cloned(), self.accepted_source_revision)
        else {
            return;
        };
        if let Some(message) = self.invalid_resource_policy_message() {
            self.projection_state = TableProjectionState::ResourceFailed {
                revision: self.view_revision,
                message: message.clone(),
            };
            self.message = Some(message);
            return;
        }
        let projection_lease = match reserve_projection_memory(
            &self.resource_pool.capture_policy(),
            &document,
            &self.view_options,
        ) {
            Ok(lease) => Arc::new(lease),
            Err(error) => {
                self.projection_state = TableProjectionState::ResourceFailed {
                    revision: self.view_revision,
                    message: error.to_string(),
                };
                self.message = Some(format!(
                    "Could not reserve memory for the table view: {error}"
                ));
                return;
            }
        };
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        let key = JobKey {
            lifetime: self.lifetime,
            source,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::Project,
            export: None,
        };
        let cancellation = TableCancellationToken::new();
        let activity = self.activity_task(key);
        worker.submit(Job {
            key,
            activity,
            #[cfg(test)]
            test_drop_probe: None,
            work: Work::Project {
                document,
                projection_lease,
                options: self.view_options.clone(),
                cancellation,
            },
        });
        self.projection_state = TableProjectionState::Pending(self.view_revision);
        self.active_key = Some(key);
    }

    pub fn snapshot(&self) -> TableSessionSnapshot {
        TableSessionSnapshot {
            lifetime: self.lifetime,
            source_revision: self.source_revision,
            view_revision: self.view_revision,
            selection_revision: self.selection_revision,
            source: self.source_resource_lease.as_ref().map_or_else(
                || TableResourceOwned::without_payload_charge(Arc::clone(&self.source)),
                |lease| TableResourceOwned::new(Arc::clone(&self.source), Arc::clone(lease)),
            ),
            source_display_revision: self.source_display_revision,
            source_is_inline_editable: self.source_is_inline_editable,
            source_dirty: self.source_dirty,
            source_path: self.source_path.clone(),
            reading_source: matches!(self.source_load_state, TableSourceLoadState::Reading(_)),
            parse_options: self.parse_options,
            parse_state: self.parse_state.clone(),
            source_load_state: self.source_load_state.clone(),
            projection_state: self.projection_state.clone(),
            accepted_source: self.accepted_source.clone(),
            document: self.document().cloned(),
            accepted_source_revision: self.accepted_source_revision,
            projection: self.projection.clone(),
            selected_rows: self.selected_rows.clone(),
            selected_columns: self.selected_columns.clone(),
            selected_visible_row_count: self.selected_visible_row_count,
            file_scan_progress: self.file_scan_progress.clone(),
            file_page: self.file_page.clone(),
            file_page_pending: self.file_page_pending,
            file_page_start: self.file_page_start,
            file_row_selection: self.file_row_selection.clone(),
            export_pending: self.export_pending.map(|pending| pending.operation),
            message: self.message.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_job_counts(&self) -> TableTestJobCounts {
        self.worker
            .as_ref()
            .map(TableWorker::test_job_counts)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn test_resource_usage(&self) -> TableResourceUsage {
        self.resource_pool.usage()
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }
}

// The worker completion publication filters cancelled jobs before they reach
// the UI. This helper intentionally only recognizes cancellation outcomes;
// ordinary parse/projection/export errors remain publishable diagnostics.
fn completion_cancelled(completion: &Completion) -> bool {
    matches!(
        &completion.result,
        WorkResult::FileRead(Err(FileReadError::Cancelled))
    ) || matches!(
        &completion.result,
        WorkResult::FileScanned(Err(FileReadError::Cancelled))
    ) || matches!(
        &completion.result,
        WorkResult::FilePage(Err(TableFileSourceError::Parse(TableParseError {
            kind: TableParseErrorKind::Cancelled,
            ..
        })))
    ) || matches!(
        &completion.result,
        WorkResult::Parsed(Err(error)) if error.kind == crate::table_utility::TableParseErrorKind::Cancelled
    ) || matches!(
        &completion.result,
        WorkResult::Projected(Err(TableProjectionError::Cancelled))
    ) || matches!(
        &completion.result,
        WorkResult::Exported(Err(TableExportError::Cancelled))
    )
}

fn work_result_succeeded(result: &WorkResult) -> bool {
    match result {
        WorkResult::FileRead(result) => result.is_ok(),
        WorkResult::FileScanned(result) => result.is_ok(),
        WorkResult::FilePage(result) => result.is_ok(),
        WorkResult::Parsed(result) => result.is_ok(),
        WorkResult::Projected(result) => result.is_ok(),
        WorkResult::Exported(result) => result.is_ok(),
    }
}

fn source_line_count_is_editable(source: &str) -> bool {
    let mut lines = 1_usize;
    let mut previous_was_carriage_return = false;
    for byte in source.bytes() {
        match byte {
            b'\r' => lines = lines.saturating_add(1),
            b'\n' if !previous_was_carriage_return => lines = lines.saturating_add(1),
            _ => {}
        }
        previous_was_carriage_return = byte == b'\r';
    }
    lines <= TABLE_INLINE_EDIT_MAX_LINES
}

fn source_is_inline_editable(source: &str) -> bool {
    source.len() <= TABLE_INLINE_EDIT_MAX_BYTES && source_line_count_is_editable(source)
}

fn reserve_source_text(
    pool: &TableResourcePool,
    source: &String,
    inline_editable: bool,
) -> Result<TableResourceLease, TableResourceError> {
    let bytes = u64::try_from(source.capacity())
        .map_err(|_| TableResourceError::ArithmeticOverflow)?
        .checked_add((2 * std::mem::size_of::<usize>()) as u64)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let mut lease = pool.reserve_ram(bytes)?;
    // The source view keeps a bounded preview and, for inline sources, a
    // separately editable text cache. Reserve those retained GUI copies with
    // the immutable source owner rather than leaving them invisible to the
    // session's aggregate budget.
    let display_cache = TABLE_SOURCE_PREVIEW_CHARS
        .checked_mul(4)
        .and_then(|bytes| {
            bytes.checked_add(if inline_editable {
                // `show_source` retains its editor buffer and makes one
                // bounded frame-local copy for egui. Include both allocations
                // before accepting the source that can populate them.
                2 * TABLE_INLINE_EDIT_MAX_BYTES
            } else {
                0
            })
        })
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    lease
        .grow(u64::try_from(display_cache).map_err(|_| TableResourceError::ArithmeticOverflow)?)?;
    Ok(lease)
}

fn reserve_arc_bytes(
    pool: &TableResourcePool,
    length: usize,
) -> Result<TableResourceLease, TableResourceError> {
    let bytes = u64::try_from(length)
        .map_err(|_| TableResourceError::ArithmeticOverflow)?
        .checked_add((2 * std::mem::size_of::<usize>()) as u64)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    pool.reserve_ram(bytes)
}

fn reserve_selection_memory(
    pool: &TableResourcePool,
    capacity: usize,
) -> Result<TableResourceLease, TableResourceError> {
    reserve_selection_memory_for::<TableRowId>(pool, capacity)
}

fn reserve_selection_memory_for<T>(
    pool: &TableResourcePool,
    capacity: usize,
) -> Result<TableResourceLease, TableResourceError> {
    let per_bucket = std::mem::size_of::<T>()
        .checked_add(3 * std::mem::size_of::<usize>())
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let bytes = capacity
        .checked_mul(per_bucket)
        .and_then(|bytes| bytes.checked_mul(2))
        .and_then(|bytes| bytes.checked_add(64))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    pool.reserve_ram(u64::try_from(bytes).map_err(|_| TableResourceError::ArithmeticOverflow)?)
}

fn all_selected_columns(
    pool: &TableResourcePool,
    column_count: usize,
) -> Result<TableResourceOwned<HashSet<usize>>, TableResourceError> {
    let lease = Arc::new(reserve_selection_memory_for::<usize>(pool, column_count)?);
    let mut columns = HashSet::new();
    columns
        .try_reserve(column_count)
        .map_err(|_| TableResourceError::ArithmeticOverflow)?;
    columns.extend(0..column_count);
    Ok(TableResourceOwned::new(Arc::new(columns), lease))
}

fn reserve_document_for_parse(
    pool: &TableResourcePool,
    input_len: usize,
    options: TableParseOptions,
) -> Result<TableResourceLease, TableResourceError> {
    let limits = options.limits;
    let rows = limits.max_data_rows.min(input_len.saturating_add(1));
    let columns = limits.max_columns.min(input_len.saturating_add(1));
    let row_column_bound = rows
        .checked_mul(columns)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let cell_bound = limits
        .max_cells
        .min(row_column_bound)
        .min(input_len.saturating_add(rows));
    let records = rows.saturating_add(usize::from(options.has_header));
    // Admission accounts for both the lexical shape records and the decoded
    // outer row vector which coexist while the document is materialized.
    let bytes = input_len
        .checked_add(
            cell_bound
                .checked_mul(std::mem::size_of::<String>())
                .ok_or(TableResourceError::ArithmeticOverflow)?,
        )
        .and_then(|bytes| {
            rows.checked_mul(std::mem::size_of::<crate::table_utility::TableRow>())
                .and_then(|row_bytes| bytes.checked_add(row_bytes))
        })
        .and_then(|bytes| {
            rows.checked_mul(std::mem::size_of::<Vec<String>>())
                .and_then(|row_bytes| bytes.checked_add(row_bytes))
        })
        .and_then(|bytes| {
            records
                .checked_mul(2 * 64)
                .and_then(|shape_bytes| bytes.checked_add(shape_bytes))
        })
        .and_then(|bytes| {
            if options.has_header {
                columns
                    .checked_mul(std::mem::size_of::<Option<String>>())
                    .and_then(|header_bytes| bytes.checked_add(header_bytes))
            } else {
                Some(bytes)
            }
        })
        .and_then(|bytes| bytes.checked_add(64 * 1024))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    pool.reserve_ram(u64::try_from(bytes).map_err(|_| TableResourceError::ArithmeticOverflow)?)
}

fn reserve_projection_memory(
    pool: &TableResourcePool,
    document: &TableDocument,
    options: &TableProjectionOptions,
) -> Result<TableResourceLease, TableResourceError> {
    let row_bytes = document
        .rows()
        .len()
        .checked_mul(std::mem::size_of::<TableRowId>())
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let max_cell_bytes = document
        .rows()
        .iter()
        .flat_map(|row| row.cells())
        .map(String::len)
        .max()
        .unwrap_or_default();
    let scratch_bytes = max_cell_bytes
        .checked_add(options.filter.len())
        .and_then(|bytes| bytes.checked_mul(4))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let bytes = row_bytes
        .checked_add(scratch_bytes)
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<TableProjection>()))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    pool.reserve_ram(u64::try_from(bytes).map_err(|_| TableResourceError::ArithmeticOverflow)?)
}

fn reserve_export_memory(
    pool: &TableResourcePool,
    document: &TableDocument,
    projection: &TableProjection,
    selected_rows: Option<&HashSet<TableRowId>>,
    columns: &HashSet<usize>,
    include_headers: bool,
) -> Result<TableResourceLease, TableResourceError> {
    let mut row_count = 0_usize;
    let mut copied_cell_bytes = 0_usize;
    let mut copied_cell_count = 0_usize;
    if include_headers {
        for (index, header) in document.headers().into_iter().flatten().enumerate() {
            if !columns.contains(&index) {
                continue;
            }
            copied_cell_bytes = copied_cell_bytes
                .checked_add(header.as_deref().map_or(0, str::len))
                .ok_or(TableResourceError::ArithmeticOverflow)?;
            copied_cell_count = copied_cell_count
                .checked_add(usize::from(header.is_some()))
                .ok_or(TableResourceError::ArithmeticOverflow)?;
        }
    }
    for row_id in projection
        .visible_rows()
        .iter()
        .copied()
        .filter(|row| selected_rows.is_none_or(|selected| selected.contains(row)))
    {
        row_count = row_count
            .checked_add(1)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        let row = document
            .row(row_id)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        for column in columns {
            if let Some(cell) = row.cells().get(*column) {
                copied_cell_bytes = copied_cell_bytes
                    .checked_add(cell.len())
                    .ok_or(TableResourceError::ArithmeticOverflow)?;
                copied_cell_count = copied_cell_count
                    .checked_add(1)
                    .ok_or(TableResourceError::ArithmeticOverflow)?;
            }
        }
    }
    let row_ids = row_count
        .checked_mul(std::mem::size_of::<TableRowId>() * 3)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let selected_columns = columns
        .len()
        .checked_mul(std::mem::size_of::<usize>() * 2)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let cell_values = copied_cell_bytes
        .checked_add(
            copied_cell_count
                .checked_mul(std::mem::size_of::<String>() + 2 * std::mem::size_of::<usize>())
                .ok_or(TableResourceError::ArithmeticOverflow)?,
        )
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    // Selection copies cell strings before formatting the bounded output.
    let bytes = (2 * crate::table_utility::TABLE_UTILITY_MAX_OUTPUT_BYTES)
        .checked_add(row_ids)
        .and_then(|bytes| bytes.checked_add(selected_columns))
        .and_then(|bytes| bytes.checked_add(cell_values))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    pool.reserve_ram(u64::try_from(bytes).map_err(|_| TableResourceError::ArithmeticOverflow)?)
}

fn accepted_source_matches_options(
    source: &TableAcceptedSource,
    options: TableParseOptions,
) -> bool {
    match source {
        TableAcceptedSource::Resident(document) => document.options() == options,
        TableAcceptedSource::FileBacked(source) => {
            let file_options = source.options();
            file_options.delimiter == options.delimiter
                && file_options.has_header == options.has_header
                && file_options.strict_width == (options.row_width == RowWidthPolicy::Strict)
                && file_options.profile.initial_parse_options().limits == options.limits
        }
    }
}

fn parse_options_for_file_source(options: TableFileParseOptions) -> TableParseOptions {
    let defaults = options.profile.initial_parse_options();
    TableParseOptions {
        delimiter: options.delimiter,
        has_header: options.has_header,
        row_width: if options.strict_width {
            RowWidthPolicy::Strict
        } else {
            RowWidthPolicy::Lenient
        },
        limits: defaults.limits,
    }
}

fn read_compact_table_file(
    path: &Path,
    options: TableParseOptions,
    pool: &TableResourcePool,
    cancellation: &TableCancellationToken,
) -> Result<(PathBuf, Vec<u8>, TableResourceLease), FileReadError> {
    let path = path.canonicalize().map_err(|error| {
        FileReadError::Failed(format!("Could not resolve the selected file: {error}"))
    })?;
    let mut file = open_readonly_source(&path)?;
    let metadata = file.metadata().map_err(|error| {
        FileReadError::Failed(format!("Could not inspect the selected file: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(FileReadError::Failed(
            "The selected path is not a regular file.".into(),
        ));
    }
    if metadata.len() > options.limits.max_input_bytes as u64 {
        return Err(FileReadError::Failed(format!(
            "The selected file exceeds the {} byte limit for the current table profile; the previous source was kept.",
            options.limits.max_input_bytes
        )));
    }
    let lease = pool
        .reserve_ram(options.limits.max_input_bytes as u64)
        .map_err(|error| FileReadError::Failed(error.to_string()))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(options.limits.max_input_bytes)
        .map_err(|_| {
            FileReadError::Failed(
                "The selected file could not be read within available memory.".to_owned(),
            )
        })?;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        if cancellation.is_cancelled() {
            return Err(FileReadError::Cancelled);
        }
        let read = file.read(&mut chunk).map_err(|error| {
            FileReadError::Failed(format!("Could not read the selected file: {error}"))
        })?;
        if read == 0 {
            return Ok((path, bytes, lease));
        }
        if bytes.len().saturating_add(read) > options.limits.max_input_bytes {
            return Err(FileReadError::Failed(format!(
                "The selected file exceeds the {} byte limit for the current table profile; the previous source was kept.",
                options.limits.max_input_bytes
            )));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
}

fn scan_table_file<F>(
    path: &Path,
    options: TableFileParseOptions,
    pool: TableResourcePool,
    cancellation: &TableCancellationToken,
    progress: &mut F,
) -> Result<(PathBuf, Arc<TableFileBackedSource>), FileReadError>
where
    F: FnMut(TableFileScanProgress),
{
    let path = path.canonicalize().map_err(|error| {
        FileReadError::Failed(format!("Could not resolve the selected file: {error}"))
    })?;
    let file = open_readonly_source(&path)?;
    let metadata = file.metadata().map_err(|error| {
        FileReadError::Failed(format!("Could not inspect the selected file: {error}"))
    })?;
    if !metadata.is_file() {
        return Err(FileReadError::Failed(
            "The selected path is not a regular file.".into(),
        ));
    }
    let identity = source_identity(&file)?;
    let source = scan_open_table_file(file, identity, options, pool, cancellation, progress)?;
    Ok((path, Arc::new(source)))
}

fn scan_open_table_file<F>(
    file: File,
    identity: TableSourceIdentity,
    options: TableFileParseOptions,
    pool: TableResourcePool,
    cancellation: &TableCancellationToken,
    progress: &mut F,
) -> Result<TableFileBackedSource, FileReadError>
where
    F: FnMut(TableFileScanProgress),
{
    let temp_root = std::env::temp_dir();
    let free_space_root = temp_root.clone();
    let workspace =
        TableTempWorkspace::create_with_free_space_probe(&temp_root, pool.clone(), move || {
            temp_volume_available_bytes(&free_space_root)
        })
        .map_err(|error| FileReadError::Failed(error.to_string()))?;
    TableFileBackedSource::scan_with_progress(
        file,
        identity,
        options,
        pool,
        &workspace,
        cancellation,
        progress,
    )
    .map_err(file_source_error)
}

fn file_source_error(error: TableFileSourceError) -> FileReadError {
    if matches!(
        &error,
        TableFileSourceError::Parse(TableParseError {
            kind: TableParseErrorKind::Cancelled,
            ..
        })
    ) {
        FileReadError::Cancelled
    } else {
        FileReadError::Failed(error.to_string())
    }
}

#[cfg(windows)]
fn open_readonly_source(path: &Path) -> Result<File, FileReadError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
        .map_err(|error| {
            FileReadError::Failed(format!(
                "Could not open the selected file for consistent read-only access: {error}"
            ))
        })
}

#[cfg(not(windows))]
fn open_readonly_source(path: &Path) -> Result<File, FileReadError> {
    OpenOptions::new().read(true).open(path).map_err(|error| {
        FileReadError::Failed(format!("Could not open the selected file: {error}"))
    })
}

#[cfg(windows)]
fn source_identity(file: &File) -> Result<TableSourceIdentity, FileReadError> {
    let identity = crate::file_operations::native_file_identity(file).map_err(|error| {
        FileReadError::Failed(format!(
            "Could not identify the selected file handle: {error}"
        ))
    })?;
    Ok(TableSourceIdentity {
        volume_serial_number: identity.volume_serial_number(),
        file_id: identity.file_id(),
    })
}

#[cfg(not(windows))]
fn source_identity(_file: &File) -> Result<TableSourceIdentity, FileReadError> {
    Err(FileReadError::Failed(
        "Large-file admission requires a native file identity adapter on this platform.".into(),
    ))
}

#[cfg(windows)]
fn temp_volume_available_bytes(path: &Path) -> Result<u64, TableResourceError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    // SAFETY: `path` is a live NUL-terminated UTF-16 path and `available` is
    // writable storage for the documented byte count output.
    unsafe { GetDiskFreeSpaceExW(PCWSTR(path.as_ptr()), Some(&mut available), None, None) }
        .map_err(|error| TableResourceError::Io(error.to_string()))?;
    Ok(available)
}

#[cfg(not(windows))]
fn temp_volume_available_bytes(_path: &Path) -> Result<u64, TableResourceError> {
    Err(TableResourceError::Io(
        "The temporary volume free-space adapter is unavailable on this platform.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_utility::TableSortMode;
    #[cfg(windows)]
    use std::io::Write;
    use std::time::{Duration, Instant};

    fn ready_snapshot(session: &mut TableUtilitySession) -> TableSessionSnapshot {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.poll();
            let snapshot = session.snapshot();
            if matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(&snapshot.projection_state, TableProjectionState::Ready(_))
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "table worker did not become ready: {:?} / {:?}",
                snapshot.parse_state,
                snapshot.projection_state
            );
            thread::yield_now();
        }
    }

    fn latest_table_activity_key(
        center: &ActivityCenter,
        operation: ActivityOperation,
    ) -> crate::activity::ActivityKey {
        center
            .snapshot()
            .rows
            .into_iter()
            .filter(|row| row.key.owner() == ActivityOwner::Table && row.operation == operation)
            .max_by_key(|row| row.key.operation_id().get())
            .map(|row| row.key)
            .expect("the table operation should have an Activity identity")
    }

    fn wait_for_activity_terminal(
        center: &ActivityCenter,
        key: crate::activity::ActivityKey,
        timeout: Duration,
    ) -> crate::activity::ActivityRow {
        let deadline = Instant::now() + timeout;
        loop {
            let row = center
                .snapshot()
                .rows
                .into_iter()
                .find(|row| row.key == key)
                .expect("the admitted table Activity row should remain observable");
            if row.lifecycle.is_terminal() {
                return row;
            }
            assert!(
                Instant::now() < deadline,
                "table Activity did not terminate"
            );
            thread::yield_now();
        }
    }

    fn wait_for_worker_completion(session: &TableUtilitySession, key: JobKey, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let published = session.worker.as_ref().is_some_and(|worker| {
                let state = worker
                    .shared
                    .0
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                state
                    .completed
                    .as_ref()
                    .is_some_and(|completion| completion.key == key)
            });
            if published {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "table worker completion was not published"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn pending_job_releases_its_resource_leases_before_confirmed_activity_retirement() {
        use crate::activity::{ActivityLifecycle, ActivitySlot};
        use std::sync::atomic::{AtomicBool, Ordering};

        let pool = TableResourcePool::new(TableResourcePolicy::default());
        let baseline = pool.usage();
        let input_lease = pool.reserve_ram(128).expect("reserve job input");
        let document_lease = pool.reserve_ram(256).expect("reserve job output");
        assert!(pool.usage().ram_bytes > baseline.ram_bytes);

        let center = ActivityCenter::new();
        let activity = center
            .start(
                ActivityOwner::Table,
                ActivitySlot::Primary,
                ActivityIncarnation::new(1),
                ActivityOperation::ParseTable,
            )
            .expect("retain the pending job's Activity row");
        let activity_key = activity.key();
        let drop_observed_in_order = Arc::new(AtomicBool::new(false));

        Job {
            key: JobKey {
                lifetime: TableSessionLifetime(1),
                source: TableSourceRevision(1),
                view: TableViewRevision(1),
                selection: TableSelectionRevision(1),
                kind: JobKind::Parse,
                export: None,
            },
            work: Work::Parse {
                bytes: Arc::from([]),
                _input_lease: Arc::new(input_lease),
                document_lease,
                options: TableParseOptions::utility_default(),
                cancellation: TableCancellationToken::new(),
            },
            activity: Some(activity),
            test_drop_probe: Some(TableTestJobDropProbe {
                center: center.clone(),
                key: activity_key,
                pool: pool.clone(),
                expected_usage: baseline,
                observed_before_terminal: Arc::clone(&drop_observed_in_order),
            }),
        }
        .cancel_before_start();

        assert!(
            drop_observed_in_order.load(Ordering::SeqCst),
            "the Work payload and its leases must be gone before Confirmed is published"
        );
        assert_eq!(pool.usage(), baseline);
        let row = center
            .snapshot()
            .rows
            .into_iter()
            .find(|row| row.key == activity_key)
            .expect("the cancelled Activity row remains retained");
        assert_eq!(
            row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Cancelled)
        );
        assert_eq!(row.retirement, ActivityRetirement::Confirmed);
    }

    #[test]
    fn table_activity_observes_file_scan_and_page_lifecycle() {
        use crate::activity::{ActivityCancelResult, ActivityLifecycle};

        let ctx = egui::Context::default();
        let center = ActivityCenter::new();
        let mut session = TableUtilitySession::new();
        session.attach_activity_center(center.clone());
        session.open(&ctx).expect("start the table worker");
        let page_gate = session
            .worker
            .as_ref()
            .expect("the table worker is open")
            .hold_next_job(JobKind::Page);

        let directory = tempfile::tempdir().expect("file-backed Activity fixture directory");
        let first_path = directory.path().join("first.tsv");
        let replaced_path = directory.path().join("replaced.tsv");
        let current_path = directory.path().join("current.tsv");
        for path in [&first_path, &replaced_path, &current_path] {
            std::fs::write(path, b"name\tvalue\nalpha\t1\nbeta\t2\n")
                .expect("write the small file-backed fixture");
        }
        let options = TableFileParseOptions::for_profile(TableUtilityProfile::Table, true);
        let timeout = Duration::from_secs(5);

        session
            .request_file_backed_source(first_path.clone(), options)
            .expect("admit the first file scan");
        let first_scan_job = session.active_key.expect("the scan has a session key");
        assert_eq!(first_scan_job.kind, JobKind::ReadFile);
        let first_scan_activity =
            latest_table_activity_key(&center, ActivityOperation::ReadTableFile);
        let first_scan_row = wait_for_activity_terminal(&center, first_scan_activity, timeout);
        wait_for_worker_completion(&session, first_scan_job, timeout);
        assert_eq!(
            first_scan_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        );
        assert_eq!(
            first_scan_row.retirement,
            ActivityRetirement::Unavailable,
            "the persistent worker does not provide a per-job join receipt"
        );
        let before_scan_poll = session.snapshot();
        assert_eq!(
            before_scan_poll.source_load_state,
            TableSourceLoadState::Reading(first_scan_job.source)
        );
        assert!(before_scan_poll.accepted_source.is_none());

        // Acceptance of the scan queues its first page on the same worker.
        // The test-only gate holds that actual Page job before its work begins.
        session.poll();
        let first_page_job = session
            .active_key
            .expect("the first page has a session key");
        assert_eq!(first_page_job.kind, JobKind::Page);
        assert_eq!(first_page_job.source, first_scan_job.source);
        assert!(
            page_gate.wait_until_entered(timeout),
            "page job did not reach the gate"
        );
        let first_page_activity =
            latest_table_activity_key(&center, ActivityOperation::ProjectTable);
        assert_ne!(first_page_activity, first_scan_activity);
        assert_eq!(
            center
                .snapshot()
                .rows
                .iter()
                .find(|row| row.key == first_page_activity)
                .expect("page Activity row")
                .lifecycle,
            ActivityLifecycle::Queued
        );
        assert_eq!(
            center.request_cancel(first_page_activity),
            ActivityCancelResult::Requested,
            "the page's cancel control targets its own token"
        );

        // Two source requests arrive while the page is held: the first scan
        // remains queued and must finish as cancelled when the second replaces it.
        session
            .request_file_backed_source(replaced_path, options)
            .expect("admit the replaceable file scan");
        let replaced_scan_job = session.active_key.expect("the replaceable scan has a key");
        let replaced_scan_activity =
            latest_table_activity_key(&center, ActivityOperation::ReadTableFile);
        session
            .request_file_backed_source(current_path.clone(), options)
            .expect("admit the current file scan");
        let current_scan_job = session.active_key.expect("the current scan has a key");
        let current_scan_activity =
            latest_table_activity_key(&center, ActivityOperation::ReadTableFile);
        assert_ne!(replaced_scan_activity, current_scan_activity);
        assert_eq!(current_scan_job.kind, JobKind::ReadFile);
        assert_eq!(
            center.request_cancel(first_page_activity),
            ActivityCancelResult::Requested,
            "repeating the old key can only signal the old page token"
        );
        let replaced_scan_row =
            wait_for_activity_terminal(&center, replaced_scan_activity, timeout);
        assert_eq!(
            replaced_scan_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Cancelled)
        );
        assert_eq!(
            replaced_scan_row.retirement,
            ActivityRetirement::Confirmed,
            "the replaced scan never started and its owned payload was dropped"
        );
        assert!(page_gate.wait_until_entered(timeout));
        let current_scan_row = center
            .snapshot()
            .rows
            .into_iter()
            .find(|row| row.key == current_scan_activity)
            .expect("current scan Activity row");
        assert!(!current_scan_row.lifecycle.is_terminal());

        page_gate.release();
        let first_page_row = wait_for_activity_terminal(&center, first_page_activity, timeout);
        assert_eq!(
            first_page_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Cancelled)
        );
        assert_eq!(first_page_row.retirement, ActivityRetirement::Unavailable);
        let current_scan_row = wait_for_activity_terminal(&center, current_scan_activity, timeout);
        wait_for_worker_completion(&session, current_scan_job, timeout);
        assert_eq!(
            current_scan_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        );
        assert_eq!(current_scan_row.retirement, ActivityRetirement::Unavailable);
        let before_current_scan_poll = session.snapshot();
        assert_eq!(
            before_current_scan_poll.source_load_state,
            TableSourceLoadState::Reading(current_scan_job.source)
        );
        assert_eq!(
            before_current_scan_poll.accepted_source_revision,
            Some(first_scan_job.source),
            "terminal worker facts are visible before session acceptance"
        );

        session.poll();
        let final_page_job = session
            .active_key
            .expect("the accepted source schedules a page");
        assert_eq!(final_page_job.kind, JobKind::Page);
        assert_eq!(final_page_job.source, current_scan_job.source);
        let final_page_activity =
            latest_table_activity_key(&center, ActivityOperation::ProjectTable);
        let final_page_row = wait_for_activity_terminal(&center, final_page_activity, timeout);
        wait_for_worker_completion(&session, final_page_job, timeout);
        assert_eq!(
            final_page_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Completed)
        );
        assert_eq!(final_page_row.retirement, ActivityRetirement::Unavailable);
        let before_page_poll = session.snapshot();
        assert!(before_page_poll.file_page_pending);
        assert!(before_page_poll.file_page.is_none());
        session.poll();
        let accepted = session.snapshot();
        assert_eq!(
            accepted.source_path.as_deref(),
            Some(current_path.as_path())
        );
        assert_eq!(
            accepted.accepted_source_revision,
            Some(current_scan_job.source)
        );
        assert!(accepted.file_page.is_some());
        assert!(!accepted.file_page_pending);

        // A real scan failure is terminally Failed while preserving the last
        // accepted source and page. This exercises FileScanned's error mapping.
        let missing_path = directory.path().join("missing.tsv");
        session
            .request_file_backed_source(missing_path, options)
            .expect("admit the failing file scan");
        let failed_scan_job = session.active_key.expect("the failed scan has a key");
        let failed_scan_activity =
            latest_table_activity_key(&center, ActivityOperation::ReadTableFile);
        let failed_scan_row = wait_for_activity_terminal(&center, failed_scan_activity, timeout);
        wait_for_worker_completion(&session, failed_scan_job, timeout);
        assert_eq!(
            failed_scan_row.lifecycle,
            ActivityLifecycle::Terminal(ActivityOutcome::Failed)
        );
        assert_eq!(
            failed_scan_row.diagnostic,
            Some(ActivityDiagnostic::WorkerFailed)
        );
        assert_eq!(failed_scan_row.retirement, ActivityRetirement::Unavailable);
        assert_eq!(
            session.snapshot().source_load_state,
            TableSourceLoadState::Reading(failed_scan_job.source)
        );
        session.poll();
        let after_failure = session.snapshot();
        assert!(matches!(
            after_failure.source_load_state,
            TableSourceLoadState::Failed { revision, .. } if revision == failed_scan_job.source
        ));
        assert_eq!(
            after_failure.accepted_source_revision,
            Some(current_scan_job.source)
        );
        assert!(after_failure.file_page.is_some());

        let rows = center.snapshot().rows;
        assert!(rows.iter().all(|row| row.lifecycle.is_terminal()));
        let counts = session.test_job_counts();
        assert_eq!(counts.scan_submitted, 4);
        assert_eq!(
            counts.scan_executed, 3,
            "the replaced pending scan never executes"
        );
        assert_eq!(counts.page_submitted, 2);
        assert_eq!(counts.page_executed, 2);
        session.shutdown();
        drop(before_scan_poll);
        drop(before_current_scan_poll);
        drop(before_page_poll);
        drop(accepted);
        drop(after_failure);
        drop(session);
        directory
            .close()
            .expect("all session snapshots and file handles are released before cleanup");
    }

    // Baseline probe: method acknowledgment and active-job clearance are not UI
    // frame latency or worker-thread retirement. No production policy changes.
    #[test]
    fn activity_baseline_table_cancel_and_warm_reopen() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.set_has_header(true);
        session.open(&ctx).unwrap();
        session.paste_source("value\nwarm".to_owned()).unwrap();
        let warm = ready_snapshot(&mut session);
        assert_eq!(warm.document.as_ref().unwrap().rows().len(), 1);
        session.close();
        session.open(&ctx).unwrap();
        assert!(session.snapshot().document.is_some());
        let source = format!("value\n{}", "abcdefghij0123456789\n".repeat(40_000));
        let bytes = source.len();
        session.paste_source(source).unwrap();
        let operation = session.snapshot().source_revision.value();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let active = session
                .worker
                .as_ref()
                .unwrap()
                .shared
                .0
                .lock()
                .unwrap()
                .active_cancellation
                .is_some();
            if active {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "actual table job was not observed active"
            );
            thread::yield_now();
        }
        let cancel = Instant::now();
        session.close();
        let ack_us = cancel.elapsed().as_micros();
        loop {
            let state = session.worker.as_ref().unwrap().shared.0.lock().unwrap();
            let idle = state.active_cancellation.is_none() && state.pending.is_none();
            drop(state);
            if idle {
                break;
            }
            assert!(Instant::now() < deadline, "table job did not clear");
            thread::yield_now();
        }
        let job_clear_us = cancel.elapsed().as_micros();
        let retained_document = session.snapshot().document.is_some();
        assert!(retained_document, "close preserves last-good document");
        assert_eq!(session.source().len(), bytes);
        session.shutdown();
        let joined_us = cancel.elapsed().as_micros();
        assert!(session.worker.is_none());
        eprintln!(
            "ACTIVITY_BASELINE {}",
            serde_json::json!({
                "owner":"table", "operation_source_revision":operation,
                "fixture_rows":40000,"fixture_bytes":bytes,"warm_reopen":true,
                "cancel_ack_us":ack_us,"job_clear_observed_us":job_clear_us,
                "worker_join_us":joined_us,"retained_source_bytes":bytes,
                "last_good_document_retained":retained_document,"ui_frame_latency":"not measured",
                "deadline_seconds":5,"cleanup":"joined"
            })
        );
    }

    fn loaded_source_snapshot(session: &mut TableUtilitySession) -> TableSessionSnapshot {
        loaded_source_snapshot_with_timeout(session, Duration::from_secs(30))
    }

    fn loaded_source_snapshot_with_timeout(
        session: &mut TableUtilitySession,
        timeout: Duration,
    ) -> TableSessionSnapshot {
        let deadline = Instant::now() + timeout;
        loop {
            session.poll();
            let snapshot = session.snapshot();
            if !snapshot.reading_source
                && snapshot.source_path.is_some()
                && matches!(
                    snapshot.source_load_state,
                    TableSourceLoadState::Idle | TableSourceLoadState::Failed { .. }
                )
            {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "table source worker did not finish: {:?}",
                snapshot.source_load_state
            );
            thread::yield_now();
        }
    }

    fn loaded_file_page_snapshot_with_timeout(
        session: &mut TableUtilitySession,
        timeout: Duration,
    ) -> TableSessionSnapshot {
        let deadline = Instant::now() + timeout;
        loop {
            session.poll();
            let snapshot = session.snapshot();
            if !snapshot.file_page_pending && snapshot.file_page.is_some() {
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "file-backed page did not finish: {:?} / {:?}",
                snapshot.source_load_state,
                snapshot.file_page.as_ref().map(|page| page.start_row())
            );
            thread::yield_now();
        }
    }

    fn controlled_file_fixture(
        pool: &TableResourcePool,
        name: &str,
        contents: &[u8],
    ) -> (
        tempfile::TempDir,
        TableTempWorkspace,
        Arc<TableFileBackedSource>,
    ) {
        let directory = tempfile::tempdir().expect("controlled file fixture directory");
        let path = directory.path().join(name);
        std::fs::write(&path, contents).expect("write controlled file fixture");
        let workspace = TableTempWorkspace::create(std::env::temp_dir(), pool.clone(), u64::MAX)
            .expect("create controlled file workspace");
        let file = File::open(path).expect("open controlled file fixture");
        let source = TableFileBackedSource::scan(
            file,
            TableSourceIdentity {
                volume_serial_number: 1,
                file_id: [0x5a; 16],
            },
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
            pool.clone(),
            &workspace,
            &TableCancellationToken::new(),
        )
        .expect("scan controlled file fixture");
        (directory, workspace, Arc::new(source))
    }

    fn controlled_file_page(
        source: &Arc<TableFileBackedSource>,
        start_row: u64,
        columns: &[usize],
    ) -> TableResourceOwned<TableFileProjectedPage> {
        source
            .projected_page(
                start_row,
                columns,
                crate::table_utility::TABLE_FILE_PAGE_MAX_ROWS,
                crate::table_utility::TABLE_FILE_PAGE_MAX_BYTES,
                crate::table_utility::TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            )
            .expect("build controlled page completion")
    }

    fn controlled_page_key(session: &TableUtilitySession) -> JobKey {
        JobKey {
            lifetime: session.lifetime,
            source: session
                .accepted_source_revision
                .expect("controlled accepted source revision"),
            view: session.view_revision,
            selection: session.selection_revision,
            kind: JobKind::Page,
            export: None,
        }
    }

    fn install_controlled_file_source(
        session: &mut TableUtilitySession,
        source: Arc<TableFileBackedSource>,
        page: TableResourceOwned<TableFileProjectedPage>,
    ) -> JobKey {
        session.open = true;
        session.lifetime = TableSessionLifetime(1);
        let revision = session.source_revision;
        session.accepted_source = Some(TableAcceptedSource::FileBacked(Arc::clone(&source)));
        session.accepted_source_revision = Some(revision);
        session.parse_options = parse_options_for_file_source(source.options());
        session.parse_state = TableParseState::Ready(revision);
        session.source_load_state = TableSourceLoadState::Idle;
        session.source_dirty = false;
        session.source_is_inline_editable = false;
        let admission = session.resource_pool.capture_policy();
        session.selected_columns = all_selected_columns(&admission, source.column_count())
            .expect("reserve controlled file columns");
        session.file_row_selection = Some(
            TableFileRowSelection::all_selected(source.id(), source.row_count(), &admission)
                .expect("reserve controlled file row selection"),
        );
        session.file_page_start = page.start_row();
        session.file_page_pending = false;
        session.file_page_key = None;
        session.file_page = Some(page);
        let key = controlled_page_key(session);
        session.file_page_key = Some(key);
        key
    }

    fn await_worker_scan_progress(
        session: &TableUtilitySession,
        timeout: Duration,
    ) -> (JobKey, TableFileScanProgress) {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(progress) = session
                .worker
                .as_ref()
                .expect("the controlled scan worker is open")
                .poll_scan_progress()
            {
                return progress;
            }
            assert!(
                Instant::now() < deadline,
                "the scan worker did not publish provisional progress"
            );
            thread::yield_now();
        }
    }

    #[test]
    fn cloned_snapshot_handles_keep_each_allocation_charged_after_session_drop() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("name,value\nalpha,1\n".to_owned())
            .expect("queue a compact source");
        let ready = ready_snapshot(&mut session);
        let row = ready
            .projection
            .as_ref()
            .and_then(|projection| projection.visible_rows().first())
            .copied()
            .expect("one projected row");
        session.set_selection(Some(HashSet::from([row])), HashSet::from([0]));
        // The pre-selection snapshot still owns its original default-column
        // reservation. Drop it before measuring the handles retained below.
        drop(ready);

        let snapshot = session.snapshot();
        let pool = session.resource_pool.clone();
        let document = snapshot.document.clone().expect("resident document");
        let source = snapshot.source.clone();
        let projection = snapshot.projection.clone().expect("projection");
        let selected_rows = snapshot.selected_rows.clone().expect("row selection");
        let selected_columns = snapshot.selected_columns.clone();
        let retained_usage = pool.usage().ram_bytes;
        assert!(retained_usage > 0);

        drop(snapshot);
        session.shutdown();
        drop(session);
        assert_eq!(pool.usage().ram_bytes, retained_usage);

        drop(document);
        let after_document = pool.usage().ram_bytes;
        assert!(after_document < retained_usage);
        drop(projection);
        assert!(pool.usage().ram_bytes < after_document);
        drop(selected_rows);
        let after_selected_rows = pool.usage().ram_bytes;
        assert!(after_selected_rows < after_document);
        drop(selected_columns);
        assert!(pool.usage().ram_bytes < after_selected_rows);
        assert_eq!(source.as_ref(), "name,value\nalpha,1\n");
        drop(source);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn cloned_export_payload_owner_keeps_output_charged_after_receipt_and_session_drop() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("name,value\nalpha,1\n".to_owned())
            .expect("queue source");
        let ready = ready_snapshot(&mut session);
        drop(ready);

        let pool = session.resource_pool.clone();
        let operation = session
            .request_export(TableExportKind::Tsv, false)
            .expect("queue export");
        let deadline = Instant::now() + Duration::from_secs(5);
        let receipt = loop {
            session.poll();
            if let Some(receipt) = session.take_export_receipt(operation) {
                break receipt;
            }
            assert!(Instant::now() < deadline, "TSV export did not complete");
            thread::yield_now();
        };
        let payload = receipt.payload.clone();
        assert_eq!(
            payload.as_ref(),
            &TableExportPayload::Tsv("alpha\t1\n".to_owned())
        );
        assert!(pool.usage().ram_bytes > 0);

        drop(receipt);
        session.shutdown();
        drop(session);
        assert!(pool.usage().ram_bytes > 0);

        drop(payload);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn invalid_preferences_refuse_new_work_but_admitted_compact_read_finishes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("admitted.tsv");
        let rejected_path = directory.path().join("rejected.tsv");
        std::fs::write(&path, b"name\tvalue\nnew\t2\n").expect("write admitted fixture");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("name,value\nold,1\n".to_owned())
            .expect("queue previous valid source");
        let previous = ready_snapshot(&mut session);
        let previous_document_id = previous.document.as_ref().unwrap().id();
        let previous_source = previous.source.to_string();
        let previous_columns = previous.selected_columns.clone();
        drop(previous);

        session
            .request_file_read(path.clone())
            .expect("admit compact file read");
        let resource_error = TableResourceError::InvalidPolicy("test preference error");
        session.set_resource_policy(Err(resource_error));
        let admitted_source_revision = session.source_revision;

        assert!(
            session
                .request_file_read(rejected_path.clone())
                .expect_err("invalid settings refuse a new compact read")
                .contains("resource preferences are invalid")
        );
        let file_options = TableFileParseOptions {
            profile: TableUtilityProfile::Table,
            delimiter: session.parse_options.delimiter,
            has_header: session.parse_options.has_header,
            strict_width: session.parse_options.row_width == RowWidthPolicy::Strict,
        };
        assert!(
            session
                .request_file_backed_source(rejected_path, file_options)
                .expect_err("invalid settings refuse a new file-backed scan")
                .contains("resource preferences are invalid")
        );
        assert!(
            session
                .paste_source("name,value\nrejected,3\n".to_owned())
                .expect_err("invalid settings refuse a new paste")
                .contains("resource preferences are invalid")
        );
        session.request_parse();
        assert_eq!(session.source_revision, admitted_source_revision);
        assert_eq!(session.source(), previous_source);
        session.set_selection(Some(HashSet::new()), HashSet::new());
        assert_eq!(session.selected_columns.as_ref(), previous_columns.as_ref());
        assert!(
            session
                .request_export(TableExportKind::Tsv, true)
                .expect_err("invalid settings refuse a new export")
                .contains("resource preferences are invalid")
        );
        session.set_filter("new".to_owned());

        let expected_path = std::fs::canonicalize(path).expect("canonical path");
        let deadline = Instant::now() + Duration::from_secs(30);
        let admitted = loop {
            session.poll();
            let snapshot = session.snapshot();
            if snapshot.source_path.as_ref() == Some(&expected_path)
                && !snapshot.reading_source
                && snapshot.accepted_source_revision == Some(session.source_revision)
                && matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(
                    &snapshot.projection_state,
                    TableProjectionState::ResourceFailed { .. }
                )
            {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "the admitted compact read/parse did not finish: {:?} / {:?}",
                snapshot.source_load_state,
                snapshot.parse_state
            );
            thread::yield_now();
        };
        assert_eq!(admitted.source.as_ref(), "name\tvalue\nnew\t2\n");
        assert_ne!(
            admitted.document.as_ref().unwrap().id(),
            previous_document_id
        );
        let accepted_cells = admitted.document.as_ref().unwrap().rows()[0].cells();
        assert_eq!(accepted_cells[0], "new");
        assert_eq!(accepted_cells[1], "2");

        session.set_resource_policy(Ok(TableResourcePolicy::default()));
        let restored = ready_snapshot(&mut session);
        assert!(matches!(
            &restored.projection_state,
            TableProjectionState::Ready(_)
        ));
        session.shutdown();
    }

    #[test]
    fn compact_read_parse_keeps_the_policy_captured_at_admission() {
        const INPUT_BYTES: usize = 9 * 1024 * 1024;
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("large-compact-field.csv");
        std::fs::write(&path, vec![b'x'; INPUT_BYTES]).expect("write compact fixture");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .request_file_read(path.clone())
            .expect("admit compact file read under default limits");
        let lower_policy =
            TableResourcePolicy::new(64 * 1024 * 1024, TableResourcePolicy::DEFAULT_TEMP_BYTES)
                .expect("valid lower policy");
        session.set_resource_policy(Ok(lower_policy));

        let expected_path = std::fs::canonicalize(path).expect("canonical path");
        let deadline = Instant::now() + Duration::from_secs(60);
        let accepted = loop {
            session.poll();
            let snapshot = session.snapshot();
            if snapshot.source_path.as_ref() == Some(&expected_path)
                && !snapshot.reading_source
                && snapshot.accepted_source_revision == Some(session.source_revision)
                && matches!(&snapshot.parse_state, TableParseState::Ready(_))
            {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "parse did not finish using its captured admission: {:?} / {:?}",
                snapshot.source_load_state,
                snapshot.parse_state
            );
            thread::yield_now();
        };
        assert_eq!(accepted.source.len(), INPUT_BYTES);
        assert_eq!(
            accepted.document.as_ref().unwrap().rows()[0].cells()[0].len(),
            INPUT_BYTES
        );
        assert_eq!(
            session.resource_pool.policy().ram_limit_bytes(),
            lower_policy.ram_limit_bytes()
        );
        session.shutdown();
    }

    #[test]
    fn failed_default_column_reservation_keeps_the_previous_accepted_result() {
        let policy =
            TableResourcePolicy::new(64 * 1024 * 1024, TableResourcePolicy::DEFAULT_TEMP_BYTES)
                .expect("valid constrained policy");
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new_with_resource_policy(Ok(policy));
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("left,right\na,b\n".to_owned())
            .expect("queue original source");
        let previous = ready_snapshot(&mut session);
        let previous_revision = previous.accepted_source_revision.unwrap();
        let previous_document_id = previous.document.as_ref().unwrap().id();
        let previous_options = previous.document.as_ref().unwrap().options();
        let previous_rows = previous.selected_rows.clone();
        let previous_columns = previous.selected_columns.clone();
        drop(previous);

        let admission = session.resource_pool.capture_policy();
        let source_len = session.source.len();
        let input_lease = reserve_arc_bytes(&admission, source_len).expect("input reservation");
        let input_bytes = input_lease.bytes();
        let document_lease =
            reserve_document_for_parse(&admission, source_len, session.parse_options)
                .expect("document reservation");
        let document_bytes = document_lease.bytes();
        let selection_lease = reserve_selection_memory_for::<usize>(&admission, 2)
            .expect("column selection reservation");
        let selection_bytes = selection_lease.bytes();
        drop((input_lease, document_lease, selection_lease));

        let used = session.resource_pool.usage().ram_bytes;
        let leave_before_selection = selection_bytes - 1;
        let filler_bytes = policy
            .ram_limit_bytes()
            .checked_sub(used)
            .and_then(|remaining| remaining.checked_sub(input_bytes))
            .and_then(|remaining| remaining.checked_sub(document_bytes))
            .and_then(|remaining| remaining.checked_sub(leave_before_selection))
            .expect("fixture leaves room for the admitted parse");
        let filler = session
            .resource_pool
            .reserve_ram(filler_bytes)
            .expect("fill all but one byte needed for default selection");

        session.request_parse();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            session.poll();
            if session
                .message
                .as_deref()
                .is_some_and(|message| message.contains("column selection"))
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "parse did not reach default-selection preflight"
            );
            thread::yield_now();
        }

        assert_eq!(session.accepted_source_revision, Some(previous_revision));
        assert_eq!(session.document().unwrap().id(), previous_document_id);
        assert_eq!(session.document().unwrap().options(), previous_options);
        assert_eq!(session.selected_rows.as_deref(), previous_rows.as_deref());
        assert_eq!(session.selected_columns.as_ref(), previous_columns.as_ref());
        assert!(
            matches!(&session.parse_state, TableParseState::Ready(revision) if *revision == previous_revision)
        );

        drop(filler);
        session.request_parse();
        let accepted = ready_snapshot(&mut session);
        assert_eq!(accepted.document.unwrap().rows().len(), 1);
        session.shutdown();
    }

    #[test]
    fn table_session_latest_source_wins_across_close_and_reopen() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.set_has_header(true);
        session.open(&ctx).expect("start session worker");
        let initial_lifetime = session.snapshot().lifetime;
        session
            .paste_source("value\nstale".to_owned())
            .expect("queue first source");
        session.close();
        session
            .replace_source("value\nlatest".to_owned())
            .expect("retain replacement source while closed");
        // A replacement made while closed is an explicit draft. Submit it
        // before reopening; opening the dialog must not silently parse edits.
        session.request_parse();
        session.open(&ctx).expect("reopen session");

        let snapshot = ready_snapshot(&mut session);
        assert!(snapshot.lifetime.value() > initial_lifetime.value());
        assert_eq!(snapshot.source.as_ref(), "value\nlatest");
        let document = snapshot.document.expect("latest source document");
        let row = document.rows().first().expect("one latest row");
        assert_eq!(row.cells()[0], "latest");
        session.shutdown();
    }

    #[test]
    fn table_session_reads_an_explicit_file_on_its_worker() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("table.tsv");
        std::fs::write(&path, b"name\tvalue\nalpha\t1\nbeta\t2\nbravo\t3\n")
            .expect("write fixture");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .request_file_read(path.clone())
            .expect("queue file read");
        session.set_filter("b".to_owned());
        session.set_sort(Some(TableSort {
            column: 0,
            direction: crate::table_utility::TableSortDirection::Descending,
            mode: TableSortMode::Lexical,
        }));
        let snapshot = ready_snapshot(&mut session);

        assert_eq!(
            snapshot.source.as_ref(),
            "name\tvalue\nalpha\t1\nbeta\t2\nbravo\t3\n"
        );
        assert_eq!(
            snapshot.source_path,
            Some(std::fs::canonicalize(path).unwrap())
        );
        assert!(snapshot.accepted_options_are_current());
        let document = snapshot.document.as_ref().expect("read table document");
        let projection = snapshot
            .projection
            .as_ref()
            .expect("latest filter projection");
        assert!(matches!(
            snapshot.source_load_state,
            TableSourceLoadState::Idle
        ));
        assert_eq!(document.rows().len(), 3);
        let names = projection
            .visible_rows()
            .iter()
            .map(|row| document.row(*row).expect("projected row").cells()[0].as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["bravo", "beta"]);
        session.shutdown();
    }

    #[test]
    fn file_backed_profile_and_row_width_are_independent_across_reread() {
        for profile in [TableUtilityProfile::Table, TableUtilityProfile::Csv] {
            for strict_width in [false, true] {
                let directory = tempfile::tempdir().expect("temporary directory");
                let path = directory.path().join("profile.csv");
                std::fs::write(&path, b"name,value\nalpha,1\nbeta,2\n").expect("write fixture");
                let options = TableFileParseOptions {
                    profile,
                    delimiter: DelimiterChoice::Explicit(TableDelimiter::Comma),
                    has_header: true,
                    strict_width,
                };

                let ctx = egui::Context::default();
                let mut session = TableUtilitySession::new();
                session.open(&ctx).expect("start session worker");
                session
                    .request_file_backed_source(path.clone(), options)
                    .expect("queue typed file source");
                let initially_accepted = loaded_source_snapshot(&mut session);
                let accepted_source = initially_accepted
                    .accepted_source
                    .as_ref()
                    .and_then(TableAcceptedSource::file_source)
                    .expect("accepted file-backed source");
                assert_eq!(accepted_source.options().profile, profile);
                assert_eq!(accepted_source.options().strict_width, strict_width);
                assert!(initially_accepted.accepted_options_are_current());
                drop(initially_accepted);

                session
                    .request_file_backed_source(path.clone(), options)
                    .expect("queue a second typed scan");
                let mut changed_options = parse_options_for_file_source(options);
                changed_options.row_width = if strict_width {
                    RowWidthPolicy::Lenient
                } else {
                    RowWidthPolicy::Strict
                };
                session.set_parse_options(changed_options);
                assert_eq!(
                    session.pending_file_backed_options.unwrap().profile,
                    profile,
                    "pending width changes retain the explicit source profile"
                );

                let reread = loaded_source_snapshot(&mut session);
                let reread_source = reread
                    .accepted_source
                    .as_ref()
                    .and_then(TableAcceptedSource::file_source)
                    .expect("reread file-backed source");
                assert_eq!(reread_source.options().profile, profile);
                assert_eq!(reread_source.options().strict_width, !strict_width);
                assert!(reread.accepted_options_are_current());
                session.shutdown();
            }
        }
    }

    #[test]
    fn file_backed_source_can_switch_to_resident_parse_and_keep_last_good_on_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("accepted.csv");
        std::fs::write(&path, b"name,value\nfile,1\n").expect("write file-backed fixture");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .request_file_backed_source(
                path,
                TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
            )
            .expect("queue file-backed source");
        let file_ready = loaded_source_snapshot(&mut session);
        let file_id = file_ready
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_id)
            .expect("file source is accepted");
        assert!(file_ready.document.is_none());
        drop(file_ready);

        session
            .paste_source("name,value\nresident,2\n".to_owned())
            .expect("queue a compact resident parse after the file-backed source");
        let resident = ready_snapshot(&mut session);
        let resident_document = resident.document.as_ref().expect("resident document");
        let resident_id = resident_document.id();
        assert!(matches!(
            resident.accepted_source,
            Some(TableAcceptedSource::Resident(_))
        ));
        assert_eq!(
            resident_document.rows().first().and_then(|row| row.cell(0)),
            Some("resident")
        );
        assert!(resident.file_page.is_none());
        assert!(!resident.file_page_pending);
        assert!(resident.file_row_selection.is_none());
        drop(resident);

        session
            .paste_source("name,value\n\"unterminated".to_owned())
            .expect("queue malformed replacement");
        let deadline = Instant::now() + Duration::from_secs(5);
        let failed = loop {
            session.poll();
            let snapshot = session.snapshot();
            if matches!(&snapshot.parse_state, TableParseState::Failed { .. }) {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "malformed resident replacement did not reach a terminal parse state"
            );
            thread::yield_now();
        };
        let retained_document = failed
            .document
            .as_ref()
            .expect("last-good document retained");
        assert_eq!(retained_document.id(), resident_id);
        assert_eq!(
            retained_document.rows().first().and_then(|row| row.cell(0)),
            Some("resident")
        );
        assert!(
            matches!(
                failed.accepted_source.as_ref(),
                Some(TableAcceptedSource::Resident(_))
            ),
            "the failed resident draft cannot replace its previous valid source"
        );
        assert!(failed.file_page.is_none());
        assert!(!failed.file_page_pending);
        assert!(failed.file_row_selection.is_none());
        assert_ne!(
            failed
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_id),
            Some(file_id),
            "the successfully scheduled resident parse replaced the older file source"
        );
        session.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn provisional_file_scan_progress_is_distinct_and_cleared_on_terminal_results() {
        let directory = tempfile::tempdir().expect("provisional scan workspace");
        let valid_path = directory.path().join("valid.csv");
        std::fs::write(&valid_path, b"name,value\nnew,1\nsecond,2\n")
            .expect("write valid scan fixture");
        let invalid_path = directory.path().join("late-width.csv");
        let mut invalid_source = String::from("name,value\n");
        for row in 0..16 {
            invalid_source.push_str(&format!("row-{row},valid\n"));
        }
        invalid_source.push_str("late,width,error\n");
        std::fs::write(&invalid_path, invalid_source).expect("write late-error fixture");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("name,value\nold,1\n".to_owned())
            .expect("queue previous accepted source");
        let old = ready_snapshot(&mut session);
        let old_revision = old.accepted_source_revision.expect("resident revision");
        let old_document_id = old.document.as_ref().unwrap().id();
        assert!(old.can_export());
        drop(old);

        let csv_options = TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true);
        session
            .request_file_backed_source(valid_path, csv_options)
            .expect("queue valid file-backed scan");
        let (valid_key, valid_progress) =
            await_worker_scan_progress(&session, Duration::from_secs(5));
        assert_eq!(valid_key.source, session.source_revision);
        session.accept_file_scan_progress(valid_key, valid_progress.clone());

        let pending = session.snapshot();
        assert!(pending.reading_source);
        assert!(pending.source_dirty);
        assert_ne!(
            pending.accepted_source_revision,
            Some(pending.source_revision)
        );
        assert_eq!(pending.accepted_source_revision, Some(old_revision));
        assert_eq!(pending.document.as_ref().unwrap().id(), old_document_id);
        assert!(matches!(
            &pending.accepted_source,
            Some(TableAcceptedSource::Resident(_))
        ));
        assert!(!pending.can_export());
        assert_eq!(
            pending
                .file_scan_progress
                .as_ref()
                .unwrap()
                .validated_rows(),
            valid_progress.validated_rows()
        );
        let current_preview_id = valid_progress.preview().source_id();

        let mut stale_key = valid_key;
        stale_key.source = old_revision;
        session.accept_file_scan_progress(stale_key, valid_progress);
        assert_eq!(
            session
                .file_scan_progress
                .as_ref()
                .unwrap()
                .preview()
                .source_id(),
            current_preview_id,
            "a late preview from the previous source cannot replace the active scan preview"
        );

        let valid = loaded_source_snapshot_with_timeout(&mut session, Duration::from_secs(10));
        let valid_source_id = valid
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_id)
            .expect("valid scan becomes the accepted file source");
        assert_eq!(
            valid
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_source)
                .unwrap()
                .row_count(),
            2
        );
        assert!(valid.file_scan_progress.is_none());
        assert!(valid.accepted_options_are_current());
        drop(valid);
        let valid_page =
            loaded_file_page_snapshot_with_timeout(&mut session, Duration::from_secs(5));
        assert_eq!(
            valid_page.file_page.as_ref().unwrap().source_id(),
            valid_source_id
        );
        drop(valid_page);

        session
            .request_file_backed_source(invalid_path, csv_options)
            .expect("queue late-failing file-backed scan");
        let (failed_key, failed_progress) =
            await_worker_scan_progress(&session, Duration::from_secs(5));
        assert_eq!(failed_key.source, session.source_revision);
        session.accept_file_scan_progress(failed_key, failed_progress.clone());
        let pending_failure = session.snapshot();
        assert!(pending_failure.reading_source);
        assert!(pending_failure.source_dirty);
        assert!(!pending_failure.can_export());
        assert_eq!(
            pending_failure
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_id),
            Some(valid_source_id)
        );
        assert!(pending_failure.file_scan_progress.as_ref().is_some());
        drop(pending_failure);

        let failed_revision = session.source_revision;
        let deadline = Instant::now() + Duration::from_secs(10);
        let failed = loop {
            session.poll();
            let snapshot = session.snapshot();
            if matches!(
                &snapshot.source_load_state,
                TableSourceLoadState::Failed { revision, .. } if *revision == failed_revision
            ) {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "late width failure did not terminate: {:?}",
                snapshot.source_load_state
            );
            thread::yield_now();
        };
        assert!(failed.file_scan_progress.is_none());
        assert_eq!(
            failed
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_id),
            Some(valid_source_id),
            "a late scan error leaves the last accepted source intact"
        );
        assert!(failed.source_dirty);
        assert!(!failed.can_export());
        drop(failed);

        session.shutdown();
        drop(session);
        drop(failed_progress);
        drop(directory);
    }

    #[cfg(windows)]
    #[test]
    fn table_session_admits_large_file_backed_source_and_preserves_last_good() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("large-table.csv");
        let payload = "x".repeat(80);
        {
            let mut file = File::create(&path).expect("create large fixture");
            writeln!(file, "id,value").unwrap();
            for row in 0..220_000_u64 {
                writeln!(file, "{row},{payload}").unwrap();
            }
        }
        let byte_count = std::fs::metadata(&path).unwrap().len();
        assert!(byte_count > crate::table_utility::STRICT_CSV_MAX_INPUT_BYTES as u64);

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .request_file_backed_source(
                path.clone(),
                TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
            )
            .expect("queue large-file admission");
        let accepted = loaded_source_snapshot_with_timeout(&mut session, Duration::from_secs(120));
        let accepted_source = accepted
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .expect("large file is the accepted file-backed source");
        assert!(accepted.document.is_none());
        assert_eq!(accepted_source.byte_count(), byte_count);
        assert_eq!(accepted_source.row_count(), 220_000);
        assert!(accepted.accepted_options_are_current());
        let first_page_snapshot =
            loaded_file_page_snapshot_with_timeout(&mut session, Duration::from_secs(120));
        let first_page = first_page_snapshot.file_page.as_ref().expect("first page");
        assert_eq!(first_page.source_id(), accepted_source.id());
        assert_eq!(first_page.start_row(), 0);
        assert_eq!(first_page.total_rows(), Some(220_000));
        assert_eq!(
            first_page.rows().len(),
            crate::table_utility::TABLE_FILE_PAGE_MAX_ROWS
        );
        assert_eq!(first_page.rows()[0].id().ordinal(), 0);
        assert_eq!(
            first_page.rows()[0].cell_for_source_column(first_page, 0),
            Some("0")
        );
        assert_eq!(
            first_page_snapshot
                .file_row_selection
                .as_ref()
                .expect("sparse all-selected default")
                .selected_count(),
            220_000
        );
        let stale_page = first_page_snapshot
            .file_page
            .as_ref()
            .expect("first page")
            .clone();
        let stale_key = JobKey {
            lifetime: session.lifetime,
            source: session.accepted_source_revision.unwrap(),
            view: session.view_revision,
            selection: session.selection_revision,
            kind: JobKind::Page,
            export: None,
        };
        drop(first_page_snapshot);
        session
            .request_file_page(219_999)
            .expect("request the indexed last page");
        session.accept(Completion {
            key: stale_key,
            result: WorkResult::FilePage(Ok(stale_page)),
        });
        assert_eq!(
            session.file_page.as_ref().unwrap().start_row(),
            0,
            "a page from an older view generation cannot replace the current page"
        );
        assert!(session.file_page_pending);
        let last_page_snapshot =
            loaded_file_page_snapshot_with_timeout(&mut session, Duration::from_secs(120));
        let last_page = last_page_snapshot.file_page.as_ref().expect("last page");
        assert_eq!(last_page.start_row(), 219_999);
        assert_eq!(last_page.rows().len(), 1);
        assert_eq!(last_page.rows()[0].id().ordinal(), 219_999);
        assert_eq!(
            last_page.rows()[0].cell_for_source_column(last_page, 0),
            Some("219999")
        );
        drop(last_page_snapshot);
        let original_identity = accepted_source.identity();
        let mut last_row_cursor = Arc::clone(accepted_source)
            .cursor()
            .expect("open bounded row cursor");
        last_row_cursor.seek(219_999);
        let last_row = last_row_cursor
            .next_row()
            .expect("read indexed late row")
            .expect("late row exists");
        assert_eq!(last_row.cells(), ["219999", payload.as_str()]);
        drop(last_row_cursor);
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());

        let first_lifetime = accepted.lifetime;
        session.close();
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        session
            .open(&ctx)
            .expect("reopen accepted file-backed session");
        let reopened = session.snapshot();
        assert!(reopened.lifetime.value() > first_lifetime.value());
        assert_eq!(
            reopened
                .accepted_source
                .as_ref()
                .and_then(TableAcceptedSource::file_source)
                .expect("accepted source survives close/reopen")
                .identity(),
            original_identity
        );

        let invalid_path = directory.path().join("invalid-width.csv");
        std::fs::write(&invalid_path, format!("{}\n", vec!["x"; 1025].join(",")))
            .expect("write invalid replacement");
        session
            .request_file_backed_source(
                invalid_path,
                TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
            )
            .expect("queue invalid replacement");
        let failed = loaded_source_snapshot_with_timeout(&mut session, Duration::from_secs(120));
        assert!(matches!(
            failed.source_load_state,
            TableSourceLoadState::Failed { .. }
        ));
        let retained = failed
            .accepted_source
            .as_ref()
            .and_then(TableAcceptedSource::file_source)
            .expect("last-good file-backed source remains accepted");
        assert_eq!(retained.identity(), original_identity);
        assert_eq!(retained.byte_count(), byte_count);

        drop(failed);
        drop(reopened);
        drop(accepted);
        session.shutdown();
        drop(session);
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_ok());
    }

    #[test]
    fn file_page_completions_gate_source_and_view_but_not_row_selection() {
        let mut csv = String::from("left,right\n");
        for row in 0..130 {
            csv.push_str(&format!("row-{row},{row}\n"));
        }

        let mut session = TableUtilitySession::new();
        let (directory, workspace, source) = controlled_file_fixture(
            &session.resource_pool,
            "completion-source.csv",
            csv.as_bytes(),
        );
        let first_page = controlled_file_page(&source, 0, &[0, 1]);
        let first_row = first_page.rows()[0].id();
        let first_key =
            install_controlled_file_source(&mut session, Arc::clone(&source), first_page);

        let initial_view = session.view_revision;
        session
            .request_file_page(0)
            .expect("requesting the already-current page succeeds");
        assert_eq!(session.view_revision, initial_view);
        assert!(!session.file_page_pending);

        session
            .request_file_page(64)
            .expect("queue the second bounded page");
        let page_key = controlled_page_key(&session);
        assert!(session.file_page_pending);
        assert_ne!(page_key.view, first_key.view);
        session.set_file_row_selected(first_row, false);
        assert_ne!(session.selection_revision, page_key.selection);
        assert_eq!(session.view_revision, page_key.view);
        session.accept(Completion {
            key: page_key,
            result: WorkResult::FilePage(Ok(controlled_file_page(&source, 64, &[0, 1]))),
        });
        assert!(!session.file_page_pending);
        assert_eq!(session.file_page.as_ref().unwrap().start_row(), 64);
        assert!(
            !session
                .file_row_selection
                .as_ref()
                .unwrap()
                .is_selected(first_row)
        );

        session
            .request_file_page(0)
            .expect("queue a page in the current column projection");
        let stale_view_key = controlled_page_key(&session);
        session.set_column_selected(1, false);
        assert_ne!(session.view_revision, stale_view_key.view);
        session.accept(Completion {
            key: stale_view_key,
            result: WorkResult::FilePage(Ok(controlled_file_page(&source, 0, &[0, 1]))),
        });
        assert!(session.file_page_pending);
        assert_eq!(session.file_page.as_ref().unwrap().start_row(), 64);

        let replacement_csv = b"left,right\nreplacement,1\n";
        let (_replacement_directory, _replacement_workspace, replacement) = controlled_file_fixture(
            &session.resource_pool,
            "replacement-source.csv",
            replacement_csv,
        );
        let stale_source_key = controlled_page_key(&session);
        let replacement_revision =
            TableSourceRevision(session.source_revision.value().wrapping_add(1));
        session.source_revision = replacement_revision;
        session.accepted_source_revision = Some(replacement_revision);
        session.accepted_source = Some(TableAcceptedSource::FileBacked(Arc::clone(&replacement)));
        session.parse_options = parse_options_for_file_source(replacement.options());
        session.parse_state = TableParseState::Ready(replacement_revision);
        session.source_dirty = false;
        session.selected_columns = all_selected_columns(
            &session.resource_pool.capture_policy(),
            replacement.column_count(),
        )
        .unwrap();
        session.file_row_selection = Some(
            TableFileRowSelection::all_selected(
                replacement.id(),
                replacement.row_count(),
                &session.resource_pool,
            )
            .unwrap(),
        );
        let replacement_page = controlled_file_page(&replacement, 0, &[0, 1]);
        session.file_page = Some(replacement_page);
        session.file_page_start = 0;
        session.file_page_pending = true;
        session.file_page_key = None;
        session.accept(Completion {
            key: stale_source_key,
            result: WorkResult::FilePage(Ok(controlled_file_page(&source, 0, &[0, 1]))),
        });
        let retained = session
            .file_page
            .as_ref()
            .expect("replacement page retained");
        assert_eq!(retained.source_id(), replacement.id());
        assert!(session.file_page_pending);

        session.close();
        drop(session);
        drop(source);
        drop(replacement);
        drop(workspace);
        drop(directory);
    }

    #[test]
    fn pending_file_page_is_not_resubmitted_after_row_selection_changes() {
        let ctx = egui::Context::default();
        let mut csv = String::from("left,right\n");
        for row in 0..130 {
            csv.push_str(&format!("row-{row},{row}\n"));
        }

        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        let (directory, workspace, source) = controlled_file_fixture(
            &session.resource_pool,
            "row-selection-page-dedup.csv",
            csv.as_bytes(),
        );
        let first_page = controlled_file_page(&source, 0, &[0, 1]);
        let first_row = first_page.rows()[0].id();
        install_controlled_file_source(&mut session, Arc::clone(&source), first_page);

        session
            .request_file_page(64)
            .expect("submit the second bounded page");
        let submitted_key = session.active_key.expect("page request is active");
        assert_eq!(session.test_job_counts().page_submitted, 1);
        assert_eq!(submitted_key.kind, JobKind::Page);

        session.set_file_row_selected(first_row, false);
        assert_ne!(session.selection_revision, submitted_key.selection);
        assert_eq!(session.view_revision, submitted_key.view);
        assert!(session.file_page_pending);

        // The worker may already have finished, but until poll consumes its
        // completion the page identity remains active. Scheduling again must
        // not replace the same page merely because row selection changed.
        session.schedule_file_page();
        assert_eq!(session.test_job_counts().page_submitted, 1);
        assert_eq!(session.active_key, Some(submitted_key));

        session.shutdown();
        drop(session);
        drop(source);
        drop(workspace);
        drop(directory);
    }

    #[test]
    fn file_page_column_metadata_quota_refusal_preserves_old_page() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        let (directory, workspace, source) = controlled_file_fixture(
            &session.resource_pool,
            "page-column-quota.csv",
            b"left,right\na,1\nb,2\n",
        );
        let original_page = controlled_file_page(&source, 0, &[0, 1]);
        install_controlled_file_source(&mut session, Arc::clone(&source), original_page.clone());

        let request_lease = reserve_selection_memory_for::<usize>(
            &session.resource_pool.capture_policy(),
            session.selected_columns.len(),
        )
        .expect("measure the bounded page-column descriptor reservation");
        let request_bytes = request_lease.bytes();
        drop(request_lease);
        const RAM_LIMIT: u64 = 64 * 1024 * 1024;
        let policy = TableResourcePolicy::new(RAM_LIMIT, TableResourcePolicy::DEFAULT_TEMP_BYTES)
            .expect("minimum valid constrained policy");
        session.set_resource_policy(Ok(policy));
        let usage_before_fill = session.test_resource_usage();
        let filler_bytes = RAM_LIMIT
            .checked_sub(usage_before_fill.ram_bytes)
            .and_then(|remaining| remaining.checked_sub(request_bytes))
            .and_then(|remaining| remaining.checked_add(1))
            .expect("fixture can leave exactly one byte less than the request");
        let filler_lease = session
            .resource_pool
            .reserve_ram(filler_bytes)
            .expect("fill the policy while leaving the page request one byte short");
        let usage_before = session.test_resource_usage();

        session
            .request_file_page(1)
            .expect("request is accepted before page-column admission");
        assert!(!session.file_page_pending);
        assert_eq!(session.test_job_counts().page_submitted, 0);
        assert_eq!(session.file_page.as_deref(), Some(original_page.as_ref()));
        assert_eq!(session.file_page.as_ref().unwrap().start_row(), 0);
        assert_eq!(session.test_resource_usage(), usage_before);
        assert!(
            session
                .message
                .as_deref()
                .is_some_and(|message| message.contains("requested file page's column list"))
        );
        for _ in 0..3 {
            session.poll();
        }
        assert!(!session.file_page_pending);
        assert_eq!(session.test_job_counts().page_submitted, 0);
        assert_eq!(session.file_page.as_deref(), Some(original_page.as_ref()));
        assert_eq!(session.test_resource_usage(), usage_before);

        session.shutdown();
        drop(filler_lease);
        drop(session);
        drop(source);
        drop(workspace);
        drop(directory);
    }

    #[test]
    fn file_page_failure_and_cancellation_preserve_the_last_good_page() {
        let mut csv = String::from("left,right\n");
        for row in 0..130 {
            csv.push_str(&format!("row-{row},{row}\n"));
        }

        let mut session = TableUtilitySession::new();
        let (directory, workspace, source) =
            controlled_file_fixture(&session.resource_pool, "page-errors.csv", csv.as_bytes());
        let original_page = controlled_file_page(&source, 0, &[0, 1]);
        install_controlled_file_source(&mut session, Arc::clone(&source), original_page.clone());

        session
            .request_file_page(64)
            .expect("queue a page whose completion will fail");
        let failed_key = controlled_page_key(&session);
        session.accept(Completion {
            key: failed_key,
            result: WorkResult::FilePage(Err(TableFileSourceError::Io(
                "injected page read failure".into(),
            ))),
        });
        assert!(!session.file_page_pending);
        assert_eq!(session.file_page.as_deref(), Some(original_page.as_ref()));
        let failure_message = session.message.clone().expect("page failure is visible");
        assert!(failure_message.contains("previous page was kept"));

        session
            .request_file_page(64)
            .expect("queue a page whose completion will be cancelled");
        let cancelled_key = controlled_page_key(&session);
        session.accept(Completion {
            key: cancelled_key,
            result: WorkResult::FilePage(Err(TableFileSourceError::Parse(TableParseError {
                kind: TableParseErrorKind::Cancelled,
                location: None,
                expected_columns: None,
                actual_columns: None,
            }))),
        });
        assert!(!session.file_page_pending);
        assert_eq!(session.file_page.as_deref(), Some(original_page.as_ref()));
        assert_eq!(session.message.as_deref(), Some(failure_message.as_str()));

        session.close();
        drop(session);
        drop(source);
        drop(workspace);
        drop(directory);
    }

    #[test]
    fn file_page_completion_from_closed_lifetime_is_ignored_after_reopen() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        let (directory, workspace, source) = controlled_file_fixture(
            &session.resource_pool,
            "page-lifetime.csv",
            b"left,right\nfirst,1\nsecond,2\n",
        );
        let page = controlled_file_page(&source, 0, &[0, 1]);
        let original_key =
            install_controlled_file_source(&mut session, Arc::clone(&source), page.clone());

        session.close();
        session.accept(Completion {
            key: original_key,
            result: WorkResult::FilePage(Ok(controlled_file_page(&source, 0, &[0, 1]))),
        });
        assert_eq!(session.file_page.as_deref(), Some(page.as_ref()));

        session
            .open(&ctx)
            .expect("reopen session with a fresh worker");
        assert_ne!(session.lifetime, original_key.lifetime);
        let stale_completion = Completion {
            key: original_key,
            result: WorkResult::FilePage(Ok(controlled_file_page(&source, 0, &[0, 1]))),
        };
        let worker = session.worker.as_ref().expect("reopened worker");
        let (lock, _) = &*worker.shared;
        lock.lock()
            .unwrap_or_else(|error| error.into_inner())
            .completed = Some(stale_completion);
        session.poll();
        assert_eq!(session.file_page.as_deref(), Some(page.as_ref()));
        assert!(!session.file_page_pending);

        session.shutdown();
        drop(session);
        drop(source);
        drop(workspace);
        drop(directory);
    }

    #[test]
    fn csv_policy_session_file_limit_comes_from_current_profile_and_requeues_read() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let exact_path = directory.path().join("exact.csv");
        let exact_bytes = b"h,v\nx,1\n";
        assert_eq!(exact_bytes.len(), 8);
        std::fs::write(&exact_path, exact_bytes).expect("write exact profile limit");

        let too_large_path = directory.path().join("over.csv");
        let too_large_bytes = b"h,v\nx,10\n";
        assert_eq!(too_large_bytes.len(), 9);
        std::fs::write(&too_large_path, too_large_bytes).expect("write over-limit file");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .request_file_read(exact_path.clone())
            .expect("start read under table profile");

        // The read has not been consumed by the UI owner yet. Changing the
        // typed policy invalidates it and queues the same path with CSV's
        // selected byte limit and parse options.
        let mut csv_options = TableParseOptions::strict_csv_route(TableDelimiter::Comma, true);
        csv_options.limits.max_input_bytes = exact_bytes.len();
        let initial_revision = session.snapshot().source_revision;
        session.set_parse_options(csv_options);
        assert_ne!(session.snapshot().source_revision, initial_revision);

        let exact_path = std::fs::canonicalize(exact_path).unwrap();
        let accepted = {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                session.poll();
                let snapshot = session.snapshot();
                if snapshot.source_path.as_ref() == Some(&exact_path)
                    && matches!(snapshot.source_load_state, TableSourceLoadState::Idle)
                    && snapshot.accepted_options_are_current()
                    && snapshot.document.is_some()
                    && snapshot.projection_state
                        == TableProjectionState::Ready(snapshot.view_revision)
                {
                    break snapshot;
                }
                assert!(
                    Instant::now() < deadline,
                    "profile read did not finish: {:?} / {:?} / {:?}",
                    snapshot.source_load_state,
                    snapshot.parse_state,
                    snapshot.projection_state
                );
                thread::yield_now();
            }
        };
        assert_eq!(accepted.source.as_ref(), "h,v\nx,1\n");
        let document = accepted.document.as_ref().unwrap();
        assert_eq!(document.options(), csv_options);
        assert_eq!(document.rows()[0].cells(), &["x", "1"]);
        let accepted_document_id = document.id();

        let operation = session
            .request_export(TableExportKind::Delimited(TableDelimiter::Comma), true)
            .expect("queue CSV export for accepted profile");
        let receipt = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                session.poll();
                if let Some(receipt) = session.take_export_receipt(operation) {
                    break receipt;
                }
                assert!(Instant::now() < deadline, "CSV export did not complete");
                thread::yield_now();
            }
        };
        assert_eq!(
            receipt.payload.as_ref(),
            &TableExportPayload::Delimited {
                delimiter: TableDelimiter::Comma,
                text: "h,v\nx,1\n".to_owned(),
            }
        );
        assert_eq!(receipt.parse_options, csv_options);

        session
            .request_file_read(too_large_path.clone())
            .expect("queue file one byte over the selected profile");
        let too_large_path = std::fs::canonicalize(too_large_path).unwrap();
        let refused = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                session.poll();
                let snapshot = session.snapshot();
                if matches!(
                    snapshot.source_load_state,
                    TableSourceLoadState::Failed { .. }
                ) {
                    break snapshot;
                }
                assert!(
                    Instant::now() < deadline,
                    "over-limit CSV read did not fail"
                );
                thread::yield_now();
            }
        };
        assert_eq!(refused.source.as_ref(), "h,v\nx,1\n");
        assert_eq!(refused.source_path.as_ref(), Some(&exact_path));
        assert_eq!(
            refused.document.as_ref().unwrap().id(),
            accepted_document_id
        );
        assert_eq!(refused.document.as_ref().unwrap().options(), csv_options);
        assert!(!refused.accepted_options_are_current());
        assert_eq!(std::fs::metadata(too_large_path).unwrap().len(), 9);
        session.shutdown();
    }

    #[test]
    fn csv_policy_session_failed_draft_keeps_good_options_for_explicit_export() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        let csv_options = TableParseOptions::strict_csv_route(TableDelimiter::Comma, true);
        session.set_parse_options(csv_options);
        session.open(&ctx).expect("start session worker");
        session
            .paste_source("name,value\nalpha,001\n".to_owned())
            .expect("queue valid CSV");
        let accepted = ready_snapshot(&mut session);
        let accepted_revision = accepted.accepted_source_revision.unwrap();
        let accepted_document = accepted.document.as_ref().unwrap();
        let accepted_document_id = accepted_document.id();
        assert_eq!(accepted_document.options(), csv_options);

        let directory = tempfile::tempdir().expect("temporary directory");
        let missing_path = directory.path().join("missing.csv");
        session
            .request_file_read(missing_path)
            .expect("queue a replacement source read");
        let read_busy = session
            .request_export_previous_valid(TableExportKind::Delimited(TableDelimiter::Comma), true)
            .expect_err("previous-valid export must not replace a pending file read");
        assert!(read_busy.contains("file read or parse to finish"));
        assert!(matches!(
            session.snapshot().source_load_state,
            TableSourceLoadState::Reading(_)
        ));
        assert_eq!(session.export_pending, None);
        let read_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.poll();
            if matches!(
                session.snapshot().source_load_state,
                TableSourceLoadState::Failed { .. }
            ) {
                break;
            }
            assert!(
                Instant::now() < read_deadline,
                "missing file read did not fail"
            );
            thread::yield_now();
        }
        assert_eq!(
            session.snapshot().document.as_ref().unwrap().id(),
            accepted_document_id
        );

        session
            .replace_source("name,value\nshort\n".to_owned())
            .expect("retain malformed draft");
        session.request_parse();
        let parse_busy = session
            .request_export_previous_valid(TableExportKind::Delimited(TableDelimiter::Comma), true)
            .expect_err("previous-valid export must not replace a pending parse");
        assert!(parse_busy.contains("file read or parse to finish"));
        assert!(matches!(
            session.snapshot().parse_state,
            TableParseState::Pending(_)
        ));
        assert_eq!(session.export_pending, None);
        let failed = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                session.poll();
                let snapshot = session.snapshot();
                if matches!(snapshot.parse_state, TableParseState::Failed { .. }) {
                    break snapshot;
                }
                assert!(
                    Instant::now() < deadline,
                    "strict malformed CSV did not fail"
                );
                thread::yield_now();
            }
        };
        assert!(failed.has_previous_valid_data());
        assert!(failed.source_dirty);
        assert_eq!(failed.document.as_ref().unwrap().id(), accepted_document_id);
        assert_eq!(failed.document.as_ref().unwrap().options(), csv_options);
        assert_ne!(
            failed.accepted_source_revision,
            Some(failed.source_revision)
        );
        assert!(!failed.accepted_options_are_current());
        assert!(
            session
                .request_export(TableExportKind::Delimited(TableDelimiter::Comma), true)
                .is_err()
        );

        let operation = session
            .request_export_previous_valid(TableExportKind::Delimited(TableDelimiter::Comma), true)
            .expect("explicitly queue export of last-good CSV");
        let receipt = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                session.poll();
                if let Some(receipt) = session.take_export_receipt(operation) {
                    break receipt;
                }
                assert!(
                    Instant::now() < deadline,
                    "previous-valid CSV export did not complete"
                );
                thread::yield_now();
            }
        };
        assert_eq!(receipt.basis, TableExportBasis::PreviousValid);
        assert_eq!(receipt.source_revision, failed.source_revision);
        assert_eq!(receipt.accepted_source_revision, accepted_revision);
        assert_eq!(receipt.parse_options, csv_options);
        assert_eq!(
            receipt.payload.as_ref(),
            &TableExportPayload::Delimited {
                delimiter: TableDelimiter::Comma,
                text: "name,value\nalpha,001\n".to_owned(),
            }
        );

        session
            .inline_source_changed("name,value\nbeta,002\n".to_owned())
            .expect("new draft invalidates the old receipt");
        assert!(!session.export_receipt_is_current(&receipt));
        assert!(session.take_export_receipt(operation).is_none());
        session.shutdown();
    }

    #[test]
    fn table_session_stale_parse_cannot_accept_an_inline_edit() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .replace_source("value\nsubmitted".to_owned())
            .expect("retain initial source");
        session.request_parse();
        let submitted_revision = session.snapshot().source_revision;

        session
            .inline_source_changed("value\nedited".to_owned())
            .expect("edit source before polling the submitted parse");
        let edited_revision = session.snapshot().source_revision;
        assert_ne!(submitted_revision, edited_revision);
        session.poll();
        let after_stale_reply = session.snapshot();
        assert_eq!(after_stale_reply.source.as_ref(), "value\nedited");
        assert_eq!(after_stale_reply.accepted_source_revision, None);
        assert!(after_stale_reply.source_dirty);

        session.request_parse();
        let parsed_edit = ready_snapshot(&mut session);
        let document = parsed_edit
            .document
            .expect("explicitly parsed edited source");
        let row = document.rows().first().expect("one data row");
        assert_eq!(row.cells()[0], "edited");
        assert_eq!(
            parsed_edit.accepted_source_revision,
            Some(parsed_edit.source_revision)
        );
        session.shutdown();
    }

    #[test]
    fn table_session_draft_edits_revoke_queued_export_receipts() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session.set_has_header(true);
        session
            .paste_source("name,value\nalpha,1\n".to_owned())
            .expect("queue source");
        let parsed = ready_snapshot(&mut session);
        let row = parsed
            .projection
            .as_ref()
            .and_then(|projection| projection.visible_rows().first())
            .copied()
            .expect("one visible row");
        session.set_selection(Some(HashSet::from([row])), HashSet::from([0]));
        let operation = session
            .request_export(TableExportKind::Tsv, false)
            .expect("queue explicit output");
        let old_revision = session.snapshot().source_revision;

        session
            .inline_source_changed("name,value\nbeta,2\n".to_owned())
            .expect("edit after queuing export");
        assert_ne!(session.snapshot().source_revision, old_revision);
        assert_eq!(session.snapshot().export_pending, None);
        let deadline = Instant::now() + Duration::from_secs(5);
        while session.active_key.is_some() && Instant::now() < deadline {
            session.poll();
            thread::yield_now();
        }
        session.poll();
        assert!(
            session.active_key.is_none(),
            "the obsolete export completes or cancels"
        );
        assert!(session.take_export_receipt(operation).is_none());
        assert!(!session.snapshot().can_export());

        session.request_parse();
        let current = ready_snapshot(&mut session);
        let document = current.document.expect("edited source parsed explicitly");
        assert_eq!(document.rows()[0].cells()[0], "beta");
        session.shutdown();
    }

    #[test]
    fn table_session_snapshot_shares_source_and_row_selection_handles() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .paste_source("name,value\nalpha,1\nbeta,2\n".to_owned())
            .expect("queue source");
        let first = ready_snapshot(&mut session);
        let second = session.snapshot();
        assert!(first.source.ptr_eq(&second.source));

        let first_row = first
            .projection
            .as_ref()
            .and_then(|projection| projection.visible_rows().first())
            .copied()
            .expect("visible row identity");
        session.set_selection(Some(HashSet::from([first_row])), HashSet::from([0]));
        let selected = session.snapshot();
        let repeated = session.snapshot();
        assert!(
            selected
                .selected_rows
                .as_ref()
                .unwrap()
                .ptr_eq(repeated.selected_rows.as_ref().unwrap())
        );
        assert_eq!(repeated.selected_visible_row_count, 1);
        session.shutdown();
    }

    #[test]
    fn table_session_line_heavy_source_uses_bounded_preview_without_truncation() {
        let source = "x\n".repeat(TABLE_INLINE_EDIT_MAX_LINES + 1);
        let mut session = TableUtilitySession::new();
        session
            .replace_source(source.clone())
            .expect("within domain byte limit");
        let snapshot = session.snapshot();
        assert!(!snapshot.source_is_editable());
        assert_eq!(snapshot.source.as_ref(), source.as_str());
        assert!(
            session
                .inline_source_changed("y\n".repeat(TABLE_INLINE_EDIT_MAX_LINES + 1))
                .is_err()
        );
        assert_eq!(session.source(), source.as_str());

        let carriage_return_source = "x\r".repeat(TABLE_INLINE_EDIT_MAX_LINES + 1);
        session
            .replace_source(carriage_return_source.clone())
            .expect("line-heavy source remains within the parser byte limit");
        assert!(!session.snapshot().source_is_editable());
        assert_eq!(session.source(), carriage_return_source.as_str());
    }

    #[test]
    fn table_session_empty_visible_projection_disables_export() {
        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session.open(&ctx).expect("start session worker");
        session
            .paste_source("name,value\nalpha,1\n".to_owned())
            .expect("queue source");
        ready_snapshot(&mut session);
        session.set_filter("no matching row".to_owned());
        let snapshot = ready_snapshot(&mut session);
        assert_eq!(snapshot.selected_visible_row_count, 0);
        assert!(!snapshot.can_export());
        assert!(session.request_export(TableExportKind::Tsv, false).is_err());
        session.shutdown();
    }

    #[test]
    fn table_workflow_source_admission_accepts_exact_limit_and_refuses_one_more_byte() {
        let max_bytes = crate::table_utility::TABLE_UTILITY_MAX_INPUT_BYTES;
        let exact_source = format!("{}\n", "x".repeat(max_bytes - 1));
        assert_eq!(exact_source.len(), max_bytes);
        let too_large_source = format!("{exact_source}x");

        let ctx = egui::Context::default();
        let mut session = TableUtilitySession::new();
        session
            .replace_source(exact_source.clone())
            .expect("the exact byte limit is admitted");
        session.open(&ctx).expect("start session worker");
        session.request_parse();
        let deadline = Instant::now() + Duration::from_secs(20);
        let inline_snapshot = loop {
            session.poll();
            let snapshot = session.snapshot();
            if matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(&snapshot.projection_state, TableProjectionState::Ready(_))
            {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "exact-limit inline source did not parse: {:?} / {:?}",
                snapshot.parse_state,
                snapshot.projection_state
            );
            thread::yield_now();
        };
        assert_eq!(inline_snapshot.source.len(), max_bytes);
        assert_eq!(inline_snapshot.document.as_ref().unwrap().rows().len(), 1);
        let inline_document_id = inline_snapshot.document.as_ref().unwrap().id();
        assert!(session.replace_source(too_large_source.clone()).is_err());
        let after_oversized_inline = session.snapshot();
        assert_eq!(session.source(), exact_source);
        assert_eq!(
            after_oversized_inline.document.as_ref().unwrap().id(),
            inline_document_id
        );
        assert!(after_oversized_inline.accepted_options_are_current());

        let directory = tempfile::tempdir().expect("temporary directory");
        let exact_path = directory.path().join("exact.csv");
        std::fs::write(&exact_path, exact_source.as_bytes()).expect("write exact-limit file");
        let canonical_exact = std::fs::canonicalize(&exact_path).expect("canonical exact file");
        session
            .request_file_read(exact_path)
            .expect("queue exact-limit file read");
        let deadline = Instant::now() + Duration::from_secs(20);
        let exact_file_snapshot = loop {
            session.poll();
            let snapshot = session.snapshot();
            if snapshot.source_path.as_ref() == Some(&canonical_exact)
                && matches!(&snapshot.source_load_state, TableSourceLoadState::Idle)
                && matches!(&snapshot.parse_state, TableParseState::Ready(_))
                && matches!(&snapshot.projection_state, TableProjectionState::Ready(_))
            {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "exact-limit file did not finish: {:?} / {:?} / {:?}",
                snapshot.source_load_state,
                snapshot.parse_state,
                snapshot.projection_state
            );
            thread::yield_now();
        };
        assert_eq!(exact_file_snapshot.source.as_ref(), exact_source);
        assert_eq!(
            exact_file_snapshot.document.as_ref().unwrap().rows().len(),
            1
        );
        let file_document_id = exact_file_snapshot.document.as_ref().unwrap().id();

        let too_large_path = directory.path().join("too-large.csv");
        std::fs::write(&too_large_path, too_large_source.as_bytes())
            .expect("write one-byte-over file");
        session
            .request_file_read(too_large_path.clone())
            .expect("queue oversized file read");
        let deadline = Instant::now() + Duration::from_secs(10);
        let refused = loop {
            session.poll();
            let snapshot = session.snapshot();
            if matches!(
                &snapshot.source_load_state,
                TableSourceLoadState::Failed { .. }
            ) {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "oversized file did not reach a refusal state: {:?}",
                snapshot.source_load_state
            );
            thread::yield_now();
        };
        assert_eq!(refused.source.as_ref(), exact_source);
        assert_eq!(refused.source_path.as_ref(), Some(&canonical_exact));
        assert_eq!(refused.document.as_ref().unwrap().id(), file_document_id);
        assert!(!refused.accepted_options_are_current());
        assert_eq!(
            std::fs::metadata(&too_large_path)
                .expect("oversized fixture remains on disk")
                .len(),
            (max_bytes + 1) as u64
        );
        session.shutdown();
    }
}
