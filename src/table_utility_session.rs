//! Dialog-owned asynchronous parsing, projection, selection, and export state.
//!
//! This module owns table identities, worker generations, and explicit file
//! reads. Clipboard access remains an explicit presentation action in the GUI
//! owner.

use crate::table_utility::{
    DelimiterChoice, TableCancellationToken, TableDocument, TableDocumentId, TableExportError,
    TableJsonExport, TableJsonShape, TableParseError, TableParseOptions, TableProjection,
    TableProjectionError, TableProjectionOptions, TableRowId, TableSelection, TableSort,
};
use eframe::egui;
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
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
    Json(TableJsonShape),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableExportPayload {
    Tsv(String),
    Json(TableJsonExport),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableExportReceipt {
    pub operation: TableExportOperation,
    pub lifetime: TableSessionLifetime,
    pub source_revision: TableSourceRevision,
    pub document: TableDocumentId,
    pub view_revision: TableViewRevision,
    pub selection_revision: TableSelectionRevision,
    pub payload: TableExportPayload,
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
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TableTestJobCounts {
    pub parse_submitted: usize,
    pub parse_executed: usize,
    pub project_submitted: usize,
    pub project_executed: usize,
}

#[cfg(test)]
#[derive(Default)]
struct TableTestJobCounters(Mutex<TableTestJobCounts>);

#[cfg(test)]
impl TableTestJobCounters {
    fn submitted(&self, kind: JobKind) {
        let mut counts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        match kind {
            JobKind::Parse => counts.parse_submitted += 1,
            JobKind::Project => counts.project_submitted += 1,
            JobKind::ReadFile | JobKind::Export => {}
        }
    }

    fn executed(&self, kind: JobKind) {
        let mut counts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        match kind {
            JobKind::Parse => counts.parse_executed += 1,
            JobKind::Project => counts.project_executed += 1,
            JobKind::ReadFile | JobKind::Export => {}
        }
    }

    fn snapshot(&self) -> TableTestJobCounts {
        *self.0.lock().unwrap_or_else(|error| error.into_inner())
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
        cancellation: TableCancellationToken,
    },
    Parse {
        bytes: Arc<[u8]>,
        options: TableParseOptions,
        cancellation: TableCancellationToken,
    },
    Project {
        document: Arc<TableDocument>,
        options: TableProjectionOptions,
        cancellation: TableCancellationToken,
    },
    Export {
        document: Arc<TableDocument>,
        projection: Arc<TableProjection>,
        rows: Vec<TableRowId>,
        columns: Vec<usize>,
        kind: TableExportKind,
        include_headers: bool,
        cancellation: TableCancellationToken,
    },
}

struct Job {
    key: JobKey,
    work: Work,
}

enum WorkResult {
    FileRead(Result<(PathBuf, Vec<u8>), FileReadError>),
    Parsed(Result<Arc<TableDocument>, TableParseError>),
    Projected(Result<Arc<TableProjection>, TableProjectionError>),
    Exported(Result<TableExportPayload, TableExportError>),
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
    fn run(self) -> Completion {
        let result = match self.work {
            Work::ReadFile { path, cancellation } => {
                let result = read_table_file(&path, &cancellation);
                WorkResult::FileRead(result)
            }
            Work::Parse {
                bytes,
                options,
                cancellation,
            } => WorkResult::Parsed(
                TableDocument::parse_bytes_with_cancellation(&bytes, options, &cancellation)
                    .map(Arc::new),
            ),
            Work::Project {
                document,
                options,
                cancellation,
            } => WorkResult::Projected(
                TableProjection::build_with_cancellation(&document, &options, &cancellation)
                    .map(Arc::new),
            ),
            Work::Export {
                document,
                projection,
                rows,
                columns,
                kind,
                include_headers,
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
                        TableExportKind::Json(shape) => projection
                            .export_json_with_cancellation(
                                &document,
                                &selection,
                                shape,
                                &cancellation,
                            )
                            .map(TableExportPayload::Json),
                    });
                WorkResult::Exported(result)
            }
        };
        Completion {
            key: self.key,
            result,
        }
    }
}

#[derive(Default)]
struct WorkerState {
    pending: Option<Job>,
    completed: Option<Completion>,
    active_cancellation: Option<TableCancellationToken>,
    shutdown: bool,
}

struct TableWorker {
    shared: Arc<(Mutex<WorkerState>, Condvar)>,
    join: Option<JoinHandle<()>>,
    #[cfg(test)]
    test_job_counts: Arc<TableTestJobCounters>,
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
                        job
                    };

                    #[cfg(test)]
                    worker_job_counts.executed(job.key.kind);
                    let completion = job.run();
                    let (lock, wake) = &*worker_state;
                    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                    state.active_cancellation = None;
                    if state.shutdown {
                        return;
                    }
                    let publish = state.pending.is_none() && !completion_cancelled(&completion);
                    if publish {
                        state.completed = Some(completion);
                        drop(state);
                        repaint.request_repaint();
                    } else {
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
        })
    }

    fn submit(&self, job: Job) {
        #[cfg(test)]
        self.test_job_counts.submitted(job.key.kind);
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state.active_cancellation.as_ref() {
            active.cancel();
        }
        state.completed = None;
        state.pending = Some(job);
        wake.notify_one();
    }

    fn cancel_pending(&self) {
        let (lock, wake) = &*self.shared;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(active) = state.active_cancellation.as_ref() {
            active.cancel();
        }
        state.pending = None;
        state.completed = None;
        wake.notify_one();
    }

    fn poll(&self) -> Option<Completion> {
        let (lock, _) = &*self.shared;
        lock.lock()
            .unwrap_or_else(|error| error.into_inner())
            .completed
            .take()
    }

    fn shutdown(&mut self) {
        let (lock, wake) = &*self.shared;
        {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.shutdown = true;
            state.pending = None;
            state.completed = None;
            if let Some(active) = state.active_cancellation.as_ref() {
                active.cancel();
            }
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
}

impl Drop for TableWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Job {
    fn cancellation(&self) -> TableCancellationToken {
        match &self.work {
            Work::ReadFile { cancellation, .. } => cancellation.clone(),
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
    pub source: Arc<str>,
    pub source_display_revision: u64,
    pub source_is_inline_editable: bool,
    pub source_dirty: bool,
    pub source_path: Option<PathBuf>,
    pub reading_source: bool,
    pub parse_options: TableParseOptions,
    pub parse_state: TableParseState,
    pub source_load_state: TableSourceLoadState,
    pub projection_state: TableProjectionState,
    pub document: Option<Arc<TableDocument>>,
    pub accepted_source_revision: Option<TableSourceRevision>,
    pub projection: Option<Arc<TableProjection>>,
    pub selected_rows: Option<Arc<HashSet<TableRowId>>>,
    pub selected_columns: HashSet<usize>,
    pub selected_visible_row_count: usize,
    pub export_pending: Option<TableExportOperation>,
    pub message: Option<String>,
}

impl TableSessionSnapshot {
    pub fn source_is_editable(&self) -> bool {
        self.source_is_inline_editable
    }

    pub fn accepted_options_are_current(&self) -> bool {
        !self.source_dirty
            && self.accepted_source_revision == Some(self.source_revision)
            && self
                .document
                .as_ref()
                .is_some_and(|document| document.options() == self.parse_options)
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
                self.document
                    .as_ref()
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
    lifetime: TableSessionLifetime,
    source_revision: TableSourceRevision,
    view_revision: TableViewRevision,
    selection_revision: TableSelectionRevision,
    source: Arc<str>,
    source_display_revision: u64,
    source_is_inline_editable: bool,
    source_dirty: bool,
    source_path: Option<PathBuf>,
    requested_source: Arc<[u8]>,
    parse_options: TableParseOptions,
    view_options: TableProjectionOptions,
    parse_state: TableParseState,
    source_load_state: TableSourceLoadState,
    projection_state: TableProjectionState,
    document: Option<Arc<TableDocument>>,
    accepted_source_revision: Option<TableSourceRevision>,
    projection: Option<Arc<TableProjection>>,
    projected_source_revision: Option<TableSourceRevision>,
    projected_view_revision: Option<TableViewRevision>,
    selected_rows: Option<Arc<HashSet<TableRowId>>>,
    selected_visible_row_count: usize,
    selected_columns: HashSet<usize>,
    active_key: Option<JobKey>,
    next_export: u64,
    export_pending: Option<TableExportOperation>,
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
        Self {
            worker: None,
            lifetime: TableSessionLifetime(0),
            source_revision: TableSourceRevision(0),
            view_revision: TableViewRevision(0),
            selection_revision: TableSelectionRevision(0),
            source: Arc::from(""),
            source_display_revision: 0,
            source_is_inline_editable: true,
            source_dirty: false,
            source_path: None,
            requested_source: Arc::from([]),
            parse_options: TableParseOptions::utility_default(),
            view_options: TableProjectionOptions::default(),
            parse_state: TableParseState::Empty,
            source_load_state: TableSourceLoadState::Idle,
            projection_state: TableProjectionState::Empty,
            document: None,
            accepted_source_revision: None,
            projection: None,
            projected_source_revision: None,
            projected_view_revision: None,
            selected_rows: None,
            selected_visible_row_count: 0,
            selected_columns: HashSet::new(),
            active_key: None,
            next_export: 0,
            export_pending: None,
            export_receipt: None,
            message: None,
            open: false,
        }
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
        if matches!(self.parse_state, TableParseState::Pending(_)) {
            self.request_parse();
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

    pub fn lifetime(&self) -> TableSessionLifetime {
        self.lifetime
    }

    pub fn replace_source(&mut self, source: String) -> Result<(), String> {
        if source.len() > crate::table_utility::TABLE_UTILITY_MAX_INPUT_BYTES {
            return Err(format!(
                "Table source exceeds the {} byte limit; the previous source was kept.",
                crate::table_utility::TABLE_UTILITY_MAX_INPUT_BYTES
            ));
        }
        self.advance_source_revision(true);
        self.source_is_inline_editable = source_is_inline_editable(&source);
        self.source = Arc::from(source);
        self.source_display_revision = self.source_display_revision.wrapping_add(1);
        self.source_dirty = true;
        self.source_path = None;
        self.message = None;
        Ok(())
    }

    pub fn inline_source_changed(&mut self, source: String) -> Result<(), String> {
        if source.len() > TABLE_INLINE_EDIT_MAX_BYTES || !source_line_count_is_editable(&source) {
            return Err(format!(
                "Inline table edits are limited to {} bytes and {} lines; the previous source was kept. Use Paste Text or Open File for a larger source.",
                TABLE_INLINE_EDIT_MAX_BYTES, TABLE_INLINE_EDIT_MAX_LINES,
            ));
        }
        self.advance_source_revision(true);
        self.source_is_inline_editable = true;
        self.source = Arc::from(source);
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
        self.advance_source_revision(false);
        self.parse_options = options;
        self.source_dirty = true;
        self.message = None;
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
        if self.document.is_some() {
            self.projection_state = TableProjectionState::Pending(self.view_revision);
        }
        self.schedule();
    }

    pub fn request_parse(&mut self) {
        self.advance_source_revision(true);
        self.requested_source = Arc::from(self.source.as_bytes());
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
        self.advance_source_revision(true);
        self.source_dirty = true;
        self.source_load_state = TableSourceLoadState::Reading(self.source_revision);
        self.message = Some("Reading the selected file…".into());
        let key = JobKey {
            lifetime: self.lifetime,
            source: self.source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::ReadFile,
            export: None,
        };
        let cancellation = TableCancellationToken::new();
        if let Some(worker) = self.worker.as_ref() {
            worker.submit(Job {
                key,
                work: Work::ReadFile { path, cancellation },
            });
        } else {
            return Err("The table worker is unavailable.".to_owned());
        }
        self.active_key = Some(key);
        Ok(())
    }

    pub fn set_selection(&mut self, rows: Option<HashSet<TableRowId>>, columns: HashSet<usize>) {
        let rows = rows.map(Arc::new);
        let changed =
            self.selected_rows.as_deref() != rows.as_deref() || self.selected_columns != columns;
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
        let Some(projection) = self.projection.as_ref() else {
            return;
        };
        if !projection.visible_rows().contains(&row) {
            return;
        }
        let changed = if self.selected_rows.is_some() {
            if let Some(rows) = self.selected_rows.as_mut() {
                let rows = Arc::make_mut(rows);
                if selected {
                    rows.insert(row)
                } else {
                    rows.remove(&row)
                }
            } else {
                false
            }
        } else if selected {
            false
        } else {
            let mut rows = projection
                .visible_rows()
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            let changed = rows.remove(&row);
            if changed {
                self.selected_rows = Some(Arc::new(rows));
            }
            changed
        };
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
        let Some(document) = self.document.as_ref() else {
            return;
        };
        if column >= document.column_count() {
            return;
        }
        let changed = if selected {
            self.selected_columns.insert(column)
        } else {
            self.selected_columns.remove(&column)
        };
        if changed {
            self.selection_revision =
                TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
            self.export_pending = None;
            self.export_receipt = None;
        }
    }

    pub fn request_export(
        &mut self,
        kind: TableExportKind,
        include_headers: bool,
    ) -> Result<TableExportOperation, String> {
        if !self.open {
            return Err("The Table Utility is closed.".into());
        }
        if self.accepted_source_revision != Some(self.source_revision)
            || self.source_dirty
            || self
                .document
                .as_ref()
                .is_none_or(|document| document.options() != self.parse_options)
            || self.projection_state != TableProjectionState::Ready(self.view_revision)
        {
            return Err(
                "Parse the current source and wait for its result view before exporting.".into(),
            );
        }
        let document = self
            .document
            .clone()
            .ok_or_else(|| "Parse a table before exporting.".to_owned())?;
        let projection = self
            .projection
            .clone()
            .ok_or_else(|| "Wait for the current table view before exporting.".to_owned())?;
        let rows = match &self.selected_rows {
            Some(rows) => projection
                .visible_rows()
                .iter()
                .copied()
                .filter(|row| rows.contains(row))
                .collect::<Vec<_>>(),
            None => projection.visible_rows().to_vec(),
        };
        let mut columns = self.selected_columns.iter().copied().collect::<Vec<_>>();
        columns.sort_unstable();
        if rows.is_empty() {
            return Err("Select at least one visible row before exporting.".into());
        }
        if columns.is_empty() {
            return Err("Select at least one column before exporting.".into());
        }
        let operation = TableExportOperation(self.next_export);
        self.next_export = self.next_export.wrapping_add(1);
        let key = JobKey {
            lifetime: self.lifetime,
            source: self.source_revision,
            view: self.view_revision,
            selection: self.selection_revision,
            kind: JobKind::Export,
            export: Some(operation),
        };
        let cancellation = TableCancellationToken::new();
        let job = Job {
            key,
            work: Work::Export {
                document,
                projection,
                rows,
                columns,
                kind,
                include_headers,
                cancellation,
            },
        };
        self.export_pending = Some(operation);
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
            && !self.source_dirty
            && receipt.lifetime == self.lifetime
            && receipt.source_revision == self.source_revision
            && receipt.view_revision == self.view_revision
            && receipt.selection_revision == self.selection_revision
            && self.accepted_source_revision == Some(receipt.source_revision)
            && self.projected_source_revision == Some(receipt.source_revision)
            && self.projected_view_revision == Some(receipt.view_revision)
            && self.document.as_ref().is_some_and(|document| {
                document.id() == receipt.document && document.options() == self.parse_options
            })
            && self
                .projection
                .as_ref()
                .is_some_and(|projection| projection.document_id() == receipt.document)
    }

    pub fn poll(&mut self) {
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        if let Some(completion) = worker.poll() {
            if self.active_key == Some(completion.key) {
                self.active_key = None;
            }
            if completion.key.lifetime == self.lifetime {
                self.accept(completion);
            }
        }
        self.schedule();
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
                            return;
                        }
                        self.source_dirty = false;
                        self.accepted_source_revision = Some(key.source);
                        self.parse_state = TableParseState::Ready(key.source);
                        self.selected_rows = None;
                        self.selected_visible_row_count = 0;
                        self.selected_columns = (0..document.column_count()).collect();
                        self.selection_revision =
                            TableSelectionRevision(self.selection_revision.0.wrapping_add(1));
                        self.projection = None;
                        self.projected_source_revision = None;
                        self.projected_view_revision = None;
                        self.document = Some(document);
                        self.projection_state = TableProjectionState::Pending(self.view_revision);
                        self.message = None;
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
                match result {
                    Ok((path, bytes)) => match String::from_utf8(bytes) {
                        Ok(source) => {
                            self.source_is_inline_editable = source_is_inline_editable(&source);
                            self.source = Arc::from(source);
                            self.source_display_revision =
                                self.source_display_revision.wrapping_add(1);
                            self.source_dirty = true;
                            self.source_path = Some(path);
                            self.source_load_state = TableSourceLoadState::Idle;
                            self.request_parse();
                            self.message = Some("File loaded; parsing the current source…".into());
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
                if !self.export_is_current(operation)
                    || key.lifetime != self.lifetime
                    || key.source != self.source_revision
                    || key.view != self.view_revision
                    || key.selection != self.selection_revision
                    || key.kind != JobKind::Export
                {
                    return;
                }
                self.export_pending = None;
                match result {
                    Ok(payload) => {
                        let document = self.document.as_ref().map(|doc| doc.id());
                        if let Some(document) = document {
                            self.export_receipt = Some(TableExportReceipt {
                                operation,
                                lifetime: key.lifetime,
                                source_revision: key.source,
                                document,
                                view_revision: key.view,
                                selection_revision: key.selection,
                                payload,
                            });
                        }
                    }
                    Err(TableExportError::Cancelled) => {}
                    Err(error) => self.message = Some(error.to_string()),
                }
            }
        }
    }

    fn export_is_current(&self, operation: TableExportOperation) -> bool {
        self.open
            && !self.source_dirty
            && self.export_pending == Some(operation)
            && self.accepted_source_revision == Some(self.source_revision)
            && self.projected_source_revision == Some(self.source_revision)
            && self.projected_view_revision == Some(self.view_revision)
            && self
                .document
                .as_ref()
                .is_some_and(|document| document.options() == self.parse_options)
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
        if cancel_read {
            self.source_load_state = TableSourceLoadState::Idle;
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
        if self.document.is_some()
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

    fn schedule_parse(&mut self) {
        self.schedule_parse_for_revision(self.source_revision);
    }

    fn schedule_parse_for_revision(&mut self, revision: TableSourceRevision) {
        let Some(worker) = self.worker.as_ref() else {
            return;
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
        worker.submit(Job {
            key,
            work: Work::Parse {
                bytes: Arc::clone(&self.requested_source),
                options: self.parse_options,
                cancellation,
            },
        });
        self.active_key = Some(key);
    }

    fn schedule_projection(&mut self) {
        let (Some(worker), Some(document), Some(source)) = (
            self.worker.as_ref(),
            self.document.clone(),
            self.accepted_source_revision,
        ) else {
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
        worker.submit(Job {
            key,
            work: Work::Project {
                document,
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
            source: Arc::clone(&self.source),
            source_display_revision: self.source_display_revision,
            source_is_inline_editable: self.source_is_inline_editable,
            source_dirty: self.source_dirty,
            source_path: self.source_path.clone(),
            reading_source: matches!(self.source_load_state, TableSourceLoadState::Reading(_)),
            parse_options: self.parse_options,
            parse_state: self.parse_state.clone(),
            source_load_state: self.source_load_state.clone(),
            projection_state: self.projection_state.clone(),
            document: self.document.clone(),
            accepted_source_revision: self.accepted_source_revision,
            projection: self.projection.clone(),
            selected_rows: self.selected_rows.clone(),
            selected_columns: self.selected_columns.clone(),
            selected_visible_row_count: self.selected_visible_row_count,
            export_pending: self.export_pending,
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
        WorkResult::Parsed(Err(error)) if error.kind == crate::table_utility::TableParseErrorKind::Cancelled
    ) || matches!(
        &completion.result,
        WorkResult::Projected(Err(TableProjectionError::Cancelled))
    ) || matches!(
        &completion.result,
        WorkResult::Exported(Err(TableExportError::Cancelled))
    )
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

fn read_table_file(
    path: &std::path::Path,
    cancellation: &TableCancellationToken,
) -> Result<(PathBuf, Vec<u8>), FileReadError> {
    let path = path.canonicalize().map_err(|error| {
        FileReadError::Failed(format!("Could not resolve the selected file: {error}"))
    })?;
    let mut file = File::open(&path).map_err(|error| {
        FileReadError::Failed(format!("Could not open the selected file: {error}"))
    })?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        if cancellation.is_cancelled() {
            return Err(FileReadError::Cancelled);
        }
        let read = file.read(&mut chunk).map_err(|error| {
            FileReadError::Failed(format!("Could not read the selected file: {error}"))
        })?;
        if read == 0 {
            return Ok((path, bytes));
        }
        if bytes.len().saturating_add(read) > crate::table_utility::TABLE_UTILITY_MAX_INPUT_BYTES {
            return Err(FileReadError::Failed(format!(
                "The selected file exceeds the {} byte table limit; the previous source was kept.",
                crate::table_utility::TABLE_UTILITY_MAX_INPUT_BYTES
            )));
        }
        bytes.try_reserve(read).map_err(|_| {
            FileReadError::Failed(
                "The selected file could not be read within available memory.".to_owned(),
            )
        })?;
        bytes.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_utility::TableSortMode;
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
        assert!(Arc::ptr_eq(&first.source, &second.source));

        let first_row = first
            .projection
            .as_ref()
            .and_then(|projection| projection.visible_rows().first())
            .copied()
            .expect("visible row identity");
        session.set_selection(Some(HashSet::from([first_row])), HashSet::from([0]));
        let selected = session.snapshot();
        let repeated = session.snapshot();
        assert!(Arc::ptr_eq(
            selected.selected_rows.as_ref().unwrap(),
            repeated.selected_rows.as_ref().unwrap()
        ));
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
