use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::stream_scan::{self, StreamLimits, StreamRecord};
use super::{
    DelimiterChoice, TableCancellationToken, TableDelimiter, TableDocument, TableDocumentId,
    TableParseError, TableParseErrorKind, TableResourceError, TableResourceLease,
    TableResourceOwned, TableResourcePool, TableTempFile, TableTempWorkspace, TableUtilityProfile,
    check_parse_cancelled, copy_record_fields_with_capacity, csv_decode_error, decode_utf8,
};

pub const TABLE_FILE_MAX_COLUMNS: usize = 1024;
pub const TABLE_FILE_MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;
pub const TABLE_FILE_MAX_FIELD_BYTES: u64 = 4 * 1024 * 1024;
pub const TABLE_FILE_INDEX_STRIDE: u64 = 1024;
pub const TABLE_FILE_PAGE_MAX_ROWS: usize = 64;
pub const TABLE_FILE_PAGE_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const TABLE_FILE_PAGE_PREVIEW_CHARS: usize = 160;
pub const TABLE_FILE_PROVISIONAL_MAX_ROWS: usize = 16;
pub const TABLE_FILE_PROVISIONAL_MAX_BYTES: u64 = 256 * 1024;
pub const TABLE_FILE_PROVISIONAL_MAX_COLUMNS: usize = 32;
pub const TABLE_FILE_PROVISIONAL_CELL_CHARS: usize = 80;
const FILE_WORKING_RAM_BYTES: u64 = 3 * TABLE_FILE_MAX_RECORD_BYTES
    + 64 * 1024
    + TABLE_FILE_MAX_COLUMNS as u64
        * (std::mem::size_of::<String>() as u64 + 2 * std::mem::size_of::<usize>() as u64);
const MAX_DECODED_ROW_RESERVATION: u64 = 2 * TABLE_FILE_MAX_RECORD_BYTES
    + TABLE_FILE_MAX_COLUMNS as u64 * std::mem::size_of::<String>() as u64;
const DIAGNOSTIC_RESERVATION: u64 =
    super::TABLE_DIAGNOSTIC_EXAMPLES as u64 * std::mem::size_of::<TableSourceDiagnostic>() as u64;
const MAX_HEADER_RESERVATION: u64 = 2 * TABLE_FILE_MAX_RECORD_BYTES
    + TABLE_FILE_MAX_COLUMNS as u64
        * (std::mem::size_of::<String>() as u64 + std::mem::size_of::<Option<String>>() as u64);
static NEXT_FILE_SOURCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableSourceId(u64);

impl TableSourceId {
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Stable identity copied from the application’s retained native file handle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableSourceIdentity {
    pub volume_serial_number: u64,
    pub file_id: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableFileParseOptions {
    pub profile: TableUtilityProfile,
    pub delimiter: DelimiterChoice,
    pub has_header: bool,
    pub strict_width: bool,
}

impl TableFileParseOptions {
    pub const fn for_profile(profile: TableUtilityProfile, has_header: bool) -> Self {
        match profile {
            TableUtilityProfile::Table => Self {
                profile,
                delimiter: DelimiterChoice::Auto,
                has_header,
                strict_width: false,
            },
            TableUtilityProfile::Csv => Self {
                profile,
                delimiter: DelimiterChoice::Explicit(TableDelimiter::Comma),
                has_header,
                strict_width: true,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableSourceDiagnostic {
    ShortRowPadded {
        record: u64,
        actual_columns: usize,
        padded_columns: usize,
    },
    SchemaWidened {
        record: u64,
        previous_columns: usize,
        new_columns: usize,
    },
    EmptyHeader {
        column: usize,
    },
    DuplicateHeader {
        column: usize,
        first_column: usize,
    },
    AdditionalWarningsOmitted {
        count: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableFileSourceError {
    Parse(TableParseError),
    Resource(TableResourceError),
    Io(String),
    SourceChanged,
    RowOutOfRange,
    ProjectionColumnOutOfRange,
    ProjectionLimit,
}

impl std::fmt::Display for TableFileSourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => std::fmt::Display::fmt(error, formatter),
            Self::Resource(error) => std::fmt::Display::fmt(error, formatter),
            Self::Io(error) => write!(formatter, "Could not read the selected table file: {error}"),
            Self::SourceChanged => formatter.write_str(
                "The selected table file changed while it was being validated; the previous source was kept.",
            ),
            Self::RowOutOfRange => formatter.write_str("The requested table row is out of range."),
            Self::ProjectionColumnOutOfRange => {
                formatter.write_str("The projected table column is outside the source schema.")
            }
            Self::ProjectionLimit => {
                formatter.write_str("The projected table page exceeds its bounded page budget.")
            }
        }
    }
}

impl std::error::Error for TableFileSourceError {}

impl From<TableParseError> for TableFileSourceError {
    fn from(value: TableParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<TableResourceError> for TableFileSourceError {
    fn from(value: TableResourceError) -> Self {
        Self::Resource(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableFileRowId {
    source: TableSourceId,
    ordinal: u64,
}

impl TableFileRowId {
    pub const fn source(self) -> TableSourceId {
        self.source
    }

    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableFilePageStop {
    Provisional,
    RowLimit,
    ByteLimit,
    EndOfSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableFileProjectedRow {
    id: TableFileRowId,
    cells: Vec<String>,
}

impl TableFileProjectedRow {
    pub const fn id(&self) -> TableFileRowId {
        self.id
    }

    pub fn cells(&self) -> &[String] {
        &self.cells
    }

    pub fn cell_for_source_column(
        &self,
        page: &TableFileProjectedPage,
        column: usize,
    ) -> Option<&str> {
        let index = page
            .source_columns
            .iter()
            .position(|source| *source == column)?;
        self.cells.get(index).map(String::as_str)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableFileProjectedPage {
    source_id: TableSourceId,
    start_row: u64,
    next_row: u64,
    total_rows: Option<u64>,
    source_columns: Vec<usize>,
    rows: Vec<TableFileProjectedRow>,
    stop: TableFilePageStop,
}

impl TableFileProjectedPage {
    pub const fn source_id(&self) -> TableSourceId {
        self.source_id
    }

    pub const fn start_row(&self) -> u64 {
        self.start_row
    }

    pub const fn next_row(&self) -> u64 {
        self.next_row
    }

    pub const fn total_rows(&self) -> Option<u64> {
        self.total_rows
    }

    pub fn source_columns(&self) -> &[usize] {
        &self.source_columns
    }

    pub fn rows(&self) -> &[TableFileProjectedRow] {
        &self.rows
    }

    pub const fn stop(&self) -> TableFilePageStop {
        self.stop
    }

    pub const fn is_complete(&self) -> bool {
        self.total_rows.is_some()
    }
}

#[derive(Clone)]
pub struct TableFileScanProgress {
    preview: TableResourceOwned<TableFileProjectedPage>,
    validated_rows: u64,
    schema_columns: usize,
}

impl TableFileScanProgress {
    pub fn preview(&self) -> &TableResourceOwned<TableFileProjectedPage> {
        &self.preview
    }

    pub const fn validated_rows(&self) -> u64 {
        self.validated_rows
    }

    pub const fn schema_columns(&self) -> usize {
        self.schema_columns
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableFileRowSelectionMode {
    AllExcept,
    Only,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableFileRowSelectionError {
    WrongSource,
    RowOutOfRange,
    Resource(TableResourceError),
    Allocation,
}

impl std::fmt::Display for TableFileRowSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSource => {
                formatter.write_str("The selected row belongs to another file source.")
            }
            Self::RowOutOfRange => {
                formatter.write_str("The selected row is outside the file source.")
            }
            Self::Resource(error) => std::fmt::Display::fmt(error, formatter),
            Self::Allocation => {
                formatter.write_str("Could not allocate the compact row selection.")
            }
        }
    }
}

impl std::error::Error for TableFileRowSelectionError {}

/// A source-bound exact selection predicate whose retained metadata is sparse.
/// Clones retain the same resource charge; edits allocate a newly charged set.
#[derive(Clone)]
pub struct TableFileRowSelection {
    source_id: TableSourceId,
    row_count: u64,
    mode: TableFileRowSelectionMode,
    exceptions: TableResourceOwned<HashSet<TableFileRowId>>,
}

impl TableFileRowSelection {
    pub fn all_selected(
        source_id: TableSourceId,
        row_count: u64,
        pool: &TableResourcePool,
    ) -> Result<Self, TableResourceError> {
        let retained = row_selection_retained_bytes(0)?;
        let lease = pool.capture_policy().reserve_ram(retained)?;
        let rows = HashSet::new();
        Ok(Self {
            source_id,
            row_count,
            mode: TableFileRowSelectionMode::AllExcept,
            exceptions: TableResourceOwned::new(Arc::new(rows), Arc::new(lease)),
        })
    }

    pub fn none_selected(
        source_id: TableSourceId,
        row_count: u64,
        pool: &TableResourcePool,
    ) -> Result<Self, TableResourceError> {
        let mut selection = Self::all_selected(source_id, row_count, pool)?;
        selection.mode = TableFileRowSelectionMode::Only;
        Ok(selection)
    }

    pub const fn source_id(&self) -> TableSourceId {
        self.source_id
    }

    pub const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub const fn mode(&self) -> TableFileRowSelectionMode {
        self.mode
    }

    pub fn selected_count(&self) -> u64 {
        let count = u64::try_from(self.exceptions.len()).unwrap_or(u64::MAX);
        match self.mode {
            TableFileRowSelectionMode::AllExcept => self.row_count.saturating_sub(count),
            TableFileRowSelectionMode::Only => count.min(self.row_count),
        }
    }

    pub fn is_selected(&self, row: TableFileRowId) -> bool {
        if row.source != self.source_id || row.ordinal >= self.row_count {
            return false;
        }
        let contains = self.exceptions.contains(&row);
        match self.mode {
            TableFileRowSelectionMode::AllExcept => !contains,
            TableFileRowSelectionMode::Only => contains,
        }
    }

    pub fn exception_count(&self) -> usize {
        self.exceptions.len()
    }

    pub fn set_selected(
        &self,
        row: TableFileRowId,
        selected: bool,
        pool: &TableResourcePool,
    ) -> Result<Self, TableFileRowSelectionError> {
        if row.source != self.source_id {
            return Err(TableFileRowSelectionError::WrongSource);
        }
        if row.ordinal >= self.row_count {
            return Err(TableFileRowSelectionError::RowOutOfRange);
        }
        if self.is_selected(row) == selected {
            return Ok(self.clone());
        }

        let mut rows = HashSet::new();
        let old_len = self.exceptions.len();
        let new_len = match (self.mode, selected) {
            (TableFileRowSelectionMode::AllExcept, true)
            | (TableFileRowSelectionMode::Only, false) => old_len.saturating_sub(1),
            (TableFileRowSelectionMode::AllExcept, false)
            | (TableFileRowSelectionMode::Only, true) => old_len.saturating_add(1),
        };
        let reserved = row_selection_reservation_bytes(new_len)
            .map_err(TableFileRowSelectionError::Resource)?;
        let mut lease = pool
            .capture_policy()
            .reserve_ram(reserved)
            .map_err(TableFileRowSelectionError::Resource)?;
        rows.try_reserve(new_len)
            .map_err(|_| TableFileRowSelectionError::Allocation)?;
        let removes_exception = matches!(
            (self.mode, selected),
            (TableFileRowSelectionMode::AllExcept, true) | (TableFileRowSelectionMode::Only, false)
        );
        if removes_exception {
            rows.extend(
                self.exceptions
                    .iter()
                    .copied()
                    .filter(|existing| *existing != row),
            );
        } else {
            rows.extend(self.exceptions.iter().copied());
            rows.insert(row);
        }
        let retained = row_selection_retained_bytes(rows.capacity())
            .map_err(TableFileRowSelectionError::Resource)?;
        if retained > lease.bytes() {
            lease
                .grow(retained - lease.bytes())
                .map_err(TableFileRowSelectionError::Resource)?;
        } else {
            lease.shrink(lease.bytes() - retained);
        }
        Ok(Self {
            source_id: self.source_id,
            row_count: self.row_count,
            mode: self.mode,
            exceptions: TableResourceOwned::new(Arc::new(rows), Arc::new(lease)),
        })
    }
}

fn row_selection_retained_bytes(capacity: usize) -> Result<u64, TableResourceError> {
    const HASH_TABLE_OWNER_OVERHEAD: u64 = 64;
    let per_bucket = u64::try_from(std::mem::size_of::<TableFileRowId>())
        .map_err(|_| TableResourceError::ArithmeticOverflow)?
        .checked_add(1)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let buckets = u64::try_from(capacity)
        .map_err(|_| TableResourceError::ArithmeticOverflow)?
        .checked_mul(2)
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    u64::try_from(std::mem::size_of::<HashSet<TableFileRowId>>())
        .map_err(|_| TableResourceError::ArithmeticOverflow)?
        .checked_add(HASH_TABLE_OWNER_OVERHEAD)
        .and_then(|fixed| {
            buckets
                .checked_mul(per_bucket)
                .and_then(|bucket_bytes| fixed.checked_add(bucket_bytes))
        })
        .ok_or(TableResourceError::ArithmeticOverflow)
}

fn row_selection_reservation_bytes(new_len: usize) -> Result<u64, TableResourceError> {
    let capacity_guard = if new_len == 0 {
        0
    } else {
        new_len
            .checked_mul(2)
            .ok_or(TableResourceError::ArithmeticOverflow)?
            .max(3)
    };
    row_selection_retained_bytes(capacity_guard)
}

pub struct TableFileRow {
    id: TableFileRowId,
    cells: Vec<String>,
    _lease: TableResourceLease,
}

impl TableFileRow {
    pub const fn id(&self) -> TableFileRowId {
        self.id
    }

    pub fn cells(&self) -> &[String] {
        &self.cells
    }

    pub fn cell(&self, column: usize) -> Option<&str> {
        self.cells.get(column).map(String::as_str)
    }
}

/// The single accepted-source owner used by sessions. Large data never receives
/// a synthetic compact document or empty resident row vector.
#[derive(Clone)]
pub enum TableAcceptedSource {
    Resident(TableResourceOwned<TableDocument>),
    FileBacked(Arc<TableFileBackedSource>),
}

impl TableAcceptedSource {
    pub fn resident_document(&self) -> Option<&TableResourceOwned<TableDocument>> {
        match self {
            Self::Resident(document) => Some(document),
            Self::FileBacked(_) => None,
        }
    }

    pub fn file_source(&self) -> Option<&Arc<TableFileBackedSource>> {
        match self {
            Self::Resident(_) => None,
            Self::FileBacked(source) => Some(source),
        }
    }

    pub fn resident_id(&self) -> Option<TableDocumentId> {
        self.resident_document().map(|document| document.id())
    }

    pub fn file_id(&self) -> Option<TableSourceId> {
        self.file_source().map(|source| source.id())
    }
}

pub struct TableFileBackedSource {
    id: TableSourceId,
    identity: TableSourceIdentity,
    options: TableFileParseOptions,
    profile: TableUtilityProfile,
    delimiter: TableDelimiter,
    headers: Option<Vec<Option<String>>>,
    column_count: usize,
    row_count: u64,
    byte_count: u64,
    diagnostics: Vec<TableSourceDiagnostic>,
    omitted_diagnostics: u64,
    source: Mutex<File>,
    sparse_index: Mutex<TableTempFile>,
    pool: TableResourcePool,
    _metadata_lease: TableResourceLease,
}

impl TableFileBackedSource {
    pub fn scan(
        file: File,
        identity: TableSourceIdentity,
        options: TableFileParseOptions,
        pool: TableResourcePool,
        workspace: &TableTempWorkspace,
        cancellation: &TableCancellationToken,
    ) -> Result<Self, TableFileSourceError> {
        Self::scan_with_progress(
            file,
            identity,
            options,
            pool,
            workspace,
            cancellation,
            |_| {},
        )
    }

    /// Validate a file on the caller's worker while publishing only a small,
    /// charged preview. The callback runs at the record boundary; it must be
    /// quick and must not retain file buffers or decoded source rows.
    pub fn scan_with_progress<F>(
        mut file: File,
        identity: TableSourceIdentity,
        options: TableFileParseOptions,
        pool: TableResourcePool,
        workspace: &TableTempWorkspace,
        cancellation: &TableCancellationToken,
        mut progress: F,
    ) -> Result<Self, TableFileSourceError>
    where
        F: FnMut(TableFileScanProgress),
    {
        check_parse_cancelled(Some(cancellation))?;
        if !workspace.uses_pool(&pool) {
            return Err(TableFileSourceError::Resource(
                TableResourceError::PoolMismatch,
            ));
        }
        let id = next_source_id()?;
        let initial_metadata = file
            .metadata()
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;
        let initial_len = initial_metadata.len();
        let mut sparse_index = workspace.create_file("table-index")?;
        let working_lease = pool.reserve_ram(FILE_WORKING_RAM_BYTES)?;
        let mut metadata_lease = pool.reserve_ram(0)?;
        metadata_lease.grow(DIAGNOSTIC_RESERVATION)?;
        let mut provisional_lease = pool.reserve_ram(TABLE_FILE_PROVISIONAL_MAX_BYTES).ok();
        let mut provisional_rows = Vec::new();
        if provisional_lease.is_some()
            && provisional_rows
                .try_reserve_exact(TABLE_FILE_PROVISIONAL_MAX_ROWS)
                .is_err()
        {
            provisional_lease = None;
        }

        let delimiter = select_delimiter(&mut file, options, cancellation, TABLE_FILE_MAX_COLUMNS)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;

        let mut headers: Option<Vec<Option<String>>> = None;
        let mut diagnostics = Vec::new();
        diagnostics
            .try_reserve_exact(super::TABLE_DIAGNOSTIC_EXAMPLES)
            .map_err(|_| TableFileSourceError::Parse(allocation_error()))?;
        let mut omitted_diagnostics = 0_u64;
        let mut observed_width = 0_usize;
        let mut record_index = 0_u64;

        let limits = StreamLimits {
            max_columns: TABLE_FILE_MAX_COLUMNS,
            evidence_width_limit: TABLE_FILE_MAX_COLUMNS,
            max_records: None,
            max_record_bytes: Some(TABLE_FILE_MAX_RECORD_BYTES),
            max_field_bytes: Some(TABLE_FILE_MAX_FIELD_BYTES),
            has_header: options.has_header,
            strict_width: options.strict_width,
        };
        let summary = stream_scan::scan_reader(
            &mut file,
            0,
            delimiter,
            limits,
            Some(cancellation),
            Some(usize::try_from(TABLE_FILE_MAX_RECORD_BYTES).expect("8 MiB fits usize")),
            |record, raw| -> Result<bool, TableFileSourceError> {
                let is_header = options.has_header && record_index == 0;
                if is_header {
                    // Reserve retained header plus temporary field/vector
                    // ownership before decoding allocates any of it.
                    metadata_lease.grow(MAX_HEADER_RESERVATION)?;
                }
                let first_source_record = record.start.byte_offset == 0;
                let fields = decode_record(raw, delimiter, first_source_record, record.width)?;
                validate_decoded_fields(&fields, record)?;
                if fields.len() != record.width {
                    return Err(malformed_record(record).into());
                }
                if is_header {
                    let mut labels = Vec::new();
                    observed_width = record.width;
                    labels
                        .try_reserve_exact(fields.len())
                        .map_err(|_| allocation_error())?;
                    labels.extend(fields.into_iter().map(Some));
                    let retained = header_retained_bytes(&labels, labels.capacity())?;
                    metadata_lease.shrink(MAX_HEADER_RESERVATION - retained);
                    headers = Some(labels);
                } else {
                    let data_ordinal = record_index.saturating_sub(u64::from(options.has_header));
                    if data_ordinal % TABLE_FILE_INDEX_STRIDE == 0 {
                        let offset = u64::try_from(record.start.byte_offset)
                            .map_err(|_| identity_error())?;
                        sparse_index.append(&offset.to_le_bytes())?;
                    }
                    if record.width > observed_width && observed_width > 0 {
                        push_diagnostic(
                            &mut diagnostics,
                            &mut omitted_diagnostics,
                            TableSourceDiagnostic::SchemaWidened {
                                record: u64::try_from(record.start.record)
                                    .map_err(|_| identity_error())?,
                                previous_columns: observed_width,
                                new_columns: record.width,
                            },
                        )?;
                    }
                    if record.width < observed_width {
                        push_diagnostic(
                            &mut diagnostics,
                            &mut omitted_diagnostics,
                            TableSourceDiagnostic::ShortRowPadded {
                                record: u64::try_from(record.start.record)
                                    .map_err(|_| identity_error())?,
                                actual_columns: record.width,
                                padded_columns: observed_width - record.width,
                            },
                        )?;
                    }
                    observed_width = observed_width.max(record.width);
                    // Keep decoded data ephemeral. It is validated here and
                    // materialized only by a bounded cursor request.
                    if provisional_lease.is_some()
                        && provisional_rows.len() < TABLE_FILE_PROVISIONAL_MAX_ROWS
                    {
                        if let Some(row) = provisional_row(id, data_ordinal, &fields) {
                            provisional_rows.push(row);
                        }
                    }
                    drop(fields);
                }
                record_index = record_index.checked_add(1).ok_or_else(identity_error)?;
                let validated_rows = record_index.saturating_sub(u64::from(options.has_header));
                if validated_rows == 1
                    || validated_rows == TABLE_FILE_PROVISIONAL_MAX_ROWS as u64
                    || validated_rows % 1024 == 0
                {
                    publish_scan_progress(
                        &pool,
                        id,
                        &provisional_rows,
                        observed_width,
                        validated_rows,
                        &mut progress,
                    );
                }
                Ok(true)
            },
        )?;

        let final_metadata = file
            .metadata()
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;
        if final_metadata.len() != initial_len || summary.end_offset != initial_len {
            return Err(TableFileSourceError::SourceChanged);
        }
        if options.has_header && headers.is_none() {
            headers = Some(Vec::new());
        }
        let column_count = summary.schema_width;
        if let Some(labels) = headers.as_mut()
            && labels.len() < column_count
        {
            let missing = column_count - labels.len();
            let previous_header = header_retained_bytes(labels, labels.capacity())?;
            let growth_reservation =
                TABLE_FILE_MAX_COLUMNS as u64 * std::mem::size_of::<Option<String>>() as u64;
            metadata_lease.grow(growth_reservation)?;
            labels
                .try_reserve_exact(missing)
                .map_err(|_| allocation_error())?;
            labels.resize(column_count, None);
            let actual_growth = header_retained_bytes(labels, labels.capacity())?;
            let retained_growth = actual_growth.saturating_sub(previous_header);
            metadata_lease.shrink(growth_reservation.saturating_sub(retained_growth));
            push_diagnostic(
                &mut diagnostics,
                &mut omitted_diagnostics,
                TableSourceDiagnostic::SchemaWidened {
                    record: 1,
                    previous_columns: column_count - missing,
                    new_columns: column_count,
                },
            )?;
        }
        if let Some(labels) = headers.as_ref() {
            append_header_diagnostics(labels, &mut diagnostics, &mut omitted_diagnostics)?;
        }
        if omitted_diagnostics > 0 && diagnostics.len() < super::TABLE_DIAGNOSTIC_EXAMPLES {
            diagnostics.push(TableSourceDiagnostic::AdditionalWarningsOmitted {
                count: omitted_diagnostics,
            });
        }
        sparse_index.flush_and_rewind()?;
        check_parse_cancelled(Some(cancellation))?;
        // Holding the lease until the accepted source is dropped accounts the
        // retained header and bounded metadata. The working record/decoder
        // reservation ends here.
        drop(working_lease);

        Ok(Self {
            id,
            identity,
            options,
            profile: options.profile,
            delimiter,
            headers,
            column_count,
            row_count: summary.data_rows,
            byte_count: summary.end_offset,
            diagnostics,
            omitted_diagnostics,
            source: Mutex::new(file),
            sparse_index: Mutex::new(sparse_index),
            // A future cursor is a new operation and observes the current
            // policy, while the source/index leases above retain this scan's
            // admission-time limits and shared accounting pool.
            pool: pool.current_policy_view(),
            _metadata_lease: metadata_lease,
        })
    }

    pub const fn id(&self) -> TableSourceId {
        self.id
    }

    pub const fn identity(&self) -> TableSourceIdentity {
        self.identity
    }

    pub const fn options(&self) -> TableFileParseOptions {
        self.options
    }

    pub const fn profile(&self) -> TableUtilityProfile {
        self.profile
    }

    pub const fn delimiter(&self) -> TableDelimiter {
        self.delimiter
    }

    pub fn headers(&self) -> Option<&[Option<String>]> {
        self.headers.as_deref()
    }

    pub const fn column_count(&self) -> usize {
        self.column_count
    }

    pub const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub fn diagnostics(&self) -> &[TableSourceDiagnostic] {
        &self.diagnostics
    }

    pub const fn omitted_diagnostic_count(&self) -> u64 {
        self.omitted_diagnostics
    }

    pub const fn validation_complete(&self) -> bool {
        true
    }

    pub fn cursor(self: &Arc<Self>) -> Result<TableFileRowCursor, TableFileSourceError> {
        self.cursor_with_cancellation(TableCancellationToken::new())
    }

    pub fn cursor_with_cancellation(
        self: &Arc<Self>,
        cancellation: TableCancellationToken,
    ) -> Result<TableFileRowCursor, TableFileSourceError> {
        self.cursor_with_admission(self.pool.capture_policy(), cancellation)
    }

    fn cursor_with_admission(
        self: &Arc<Self>,
        admission: TableResourcePool,
        cancellation: TableCancellationToken,
    ) -> Result<TableFileRowCursor, TableFileSourceError> {
        let buffer_lease = admission.reserve_ram(FILE_WORKING_RAM_BYTES)?;
        Ok(TableFileRowCursor {
            source: Arc::clone(self),
            next_ordinal: 0,
            cancellation,
            admission,
            _buffer_lease: buffer_lease,
        })
    }

    /// Build a bounded page of truncated display cells. Full cells continue to
    /// be available through the cursor for later complete-file operations.
    pub fn projected_page(
        self: &Arc<Self>,
        start_row: u64,
        source_columns: &[usize],
        max_rows: usize,
        max_bytes: u64,
        preview_chars: usize,
        cancellation: TableCancellationToken,
    ) -> Result<TableResourceOwned<TableFileProjectedPage>, TableFileSourceError> {
        if start_row > self.row_count {
            return Err(TableFileSourceError::RowOutOfRange);
        }
        if source_columns.len() > TABLE_FILE_MAX_COLUMNS {
            return Err(TableFileSourceError::ProjectionLimit);
        }
        if source_columns
            .iter()
            .any(|column| *column >= self.column_count)
        {
            return Err(TableFileSourceError::ProjectionColumnOutOfRange);
        }
        let max_rows = max_rows.clamp(1, TABLE_FILE_PAGE_MAX_ROWS);
        let max_bytes = max_bytes.clamp(1, TABLE_FILE_PAGE_MAX_BYTES);
        let admission = self.pool.capture_policy();
        let mut lease = admission.reserve_ram(max_bytes)?;

        let mut columns = Vec::new();
        columns
            .try_reserve_exact(source_columns.len())
            .map_err(|_| TableFileSourceError::Parse(allocation_error()))?;
        columns.extend_from_slice(source_columns);
        let mut rows = Vec::new();
        rows.try_reserve_exact(max_rows)
            .map_err(|_| TableFileSourceError::Parse(allocation_error()))?;
        let mut page = TableFileProjectedPage {
            source_id: self.id,
            start_row,
            next_row: start_row,
            total_rows: Some(self.row_count),
            source_columns: columns,
            rows,
            stop: TableFilePageStop::EndOfSource,
        };
        if projected_page_retained_bytes(&page)? > max_bytes {
            return Err(TableFileSourceError::ProjectionLimit);
        }
        if start_row < self.row_count {
            let mut cursor = self.cursor_with_admission(admission, cancellation)?;
            cursor.seek(start_row);
            page.stop = TableFilePageStop::RowLimit;
            for _ in 0..max_rows {
                check_parse_cancelled(Some(&cursor.cancellation))?;
                let Some(row) = cursor.next_row()? else {
                    page.stop = TableFilePageStop::EndOfSource;
                    break;
                };
                let mut string_capacity = 0_usize;
                for column in source_columns {
                    let value = row.cell(*column).unwrap_or_default();
                    let (preview, truncated) = bounded_preview(value, preview_chars);
                    string_capacity = string_capacity
                        .checked_add(preview.len())
                        .and_then(|bytes| bytes.checked_add(if truncated { 3 } else { 0 }))
                        .ok_or(TableFileSourceError::ProjectionLimit)?;
                }
                let cell_bytes = source_columns
                    .len()
                    .checked_mul(std::mem::size_of::<String>())
                    .ok_or(TableFileSourceError::ProjectionLimit)?;
                let row_estimate = std::mem::size_of::<TableFileProjectedRow>()
                    .checked_add(cell_bytes.saturating_mul(2))
                    .and_then(|bytes| bytes.checked_add(string_capacity.saturating_mul(2)))
                    .ok_or(TableFileSourceError::ProjectionLimit)?;
                let row_estimate = u64::try_from(row_estimate)
                    .map_err(|_| TableFileSourceError::ProjectionLimit)?;
                let estimated_page = projected_page_retained_bytes(&page)?
                    .checked_add(row_estimate)
                    .ok_or(TableFileSourceError::ProjectionLimit)?;
                if estimated_page > max_bytes {
                    page.stop = TableFilePageStop::ByteLimit;
                    page.next_row = row.id.ordinal;
                    break;
                }

                let mut cells = Vec::new();
                cells
                    .try_reserve_exact(source_columns.len())
                    .map_err(|_| TableFileSourceError::Parse(allocation_error()))?;
                for column in source_columns {
                    let value = row.cell(*column).unwrap_or_default();
                    let (preview, truncated) = bounded_preview(value, preview_chars);
                    let extra = if truncated { 3 } else { 0 };
                    let mut text = String::new();
                    text.try_reserve_exact(preview.len().saturating_add(extra))
                        .map_err(|_| TableFileSourceError::Parse(allocation_error()))?;
                    text.push_str(preview);
                    if truncated {
                        text.push('…');
                    }
                    cells.push(text);
                }
                let row_id = row.id;
                drop(row);
                page.rows.push(TableFileProjectedRow { id: row_id, cells });
                let actual_page_bytes = projected_page_retained_bytes(&page)?;
                if actual_page_bytes as u64 > max_bytes {
                    let rejected = page
                        .rows
                        .pop()
                        .expect("the projected row was just appended");
                    page.stop = TableFilePageStop::ByteLimit;
                    page.next_row = rejected.id.ordinal;
                    break;
                }
                page.next_row = row_id
                    .ordinal
                    .checked_add(1)
                    .ok_or(TableFileSourceError::ProjectionLimit)?;
                if page.next_row >= self.row_count {
                    page.stop = TableFilePageStop::EndOfSource;
                    break;
                }
            }
        }

        let retained = projected_page_retained_bytes(&page)? as u64;
        if retained > lease.bytes() {
            lease.grow(retained - lease.bytes())?;
        } else {
            lease.shrink(lease.bytes() - retained);
        }
        Ok(TableResourceOwned::new(Arc::new(page), Arc::new(lease)))
    }

    fn read_row(
        &self,
        ordinal: u64,
        working_lease: &TableResourceLease,
        admission: &TableResourcePool,
        cancellation: &TableCancellationToken,
    ) -> Result<Option<TableFileRow>, TableFileSourceError> {
        if ordinal >= self.row_count {
            return Ok(None);
        }
        let checkpoint_ordinal = (ordinal / TABLE_FILE_INDEX_STRIDE) * TABLE_FILE_INDEX_STRIDE;
        let index_offset = (ordinal / TABLE_FILE_INDEX_STRIDE)
            .checked_mul(8)
            .ok_or_else(identity_error)?;
        let start_offset = self
            .sparse_index
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .read_u64_at(index_offset)?;

        let mut file = self
            .source
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        file.seek(SeekFrom::Start(start_offset))
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;
        let target = ordinal - checkpoint_ordinal;
        let mut seen = 0_u64;
        let mut result = None;
        let limits = StreamLimits {
            max_columns: TABLE_FILE_MAX_COLUMNS,
            evidence_width_limit: TABLE_FILE_MAX_COLUMNS,
            max_records: None,
            max_record_bytes: Some(TABLE_FILE_MAX_RECORD_BYTES),
            max_field_bytes: Some(TABLE_FILE_MAX_FIELD_BYTES),
            has_header: false,
            strict_width: false,
        };
        let summary = stream_scan::scan_reader(
            &mut *file,
            start_offset,
            self.delimiter,
            limits,
            Some(cancellation),
            Some(usize::try_from(TABLE_FILE_MAX_RECORD_BYTES).expect("8 MiB fits usize")),
            |record, raw| -> Result<bool, TableFileSourceError> {
                if seen == target {
                    check_parse_cancelled(Some(cancellation))?;
                    let mut row_lease = admission.reserve_ram(MAX_DECODED_ROW_RESERVATION)?;
                    let mut fields =
                        decode_record(raw, self.delimiter, start_offset == 0, self.column_count)?;
                    validate_decoded_fields(&fields, record)?;
                    if fields.len() > self.column_count {
                        return Err(malformed_record(record).into());
                    }
                    fields.resize(self.column_count, String::new());
                    let needed = decoded_row_reservation(&fields, fields.capacity())?;
                    row_lease.shrink(row_lease.bytes().saturating_sub(needed));
                    result = Some(TableFileRow {
                        id: TableFileRowId {
                            source: self.id,
                            ordinal,
                        },
                        cells: fields,
                        _lease: row_lease,
                    });
                    Ok(false)
                } else {
                    seen = seen.checked_add(1).ok_or_else(identity_error)?;
                    Ok(true)
                }
            },
        )?;
        let _ = summary;
        let _ = working_lease;
        result
            .map(Some)
            .ok_or_else(|| TableFileSourceError::RowOutOfRange)
    }
}

pub struct TableFileRowCursor {
    source: Arc<TableFileBackedSource>,
    next_ordinal: u64,
    cancellation: TableCancellationToken,
    admission: TableResourcePool,
    _buffer_lease: TableResourceLease,
}

impl TableFileRowCursor {
    pub fn next_row(&mut self) -> Result<Option<TableFileRow>, TableFileSourceError> {
        if self.next_ordinal >= self.source.row_count {
            return Ok(None);
        }
        let row = self
            .source
            .read_row(
                self.next_ordinal,
                &self._buffer_lease,
                &self.admission,
                &self.cancellation,
            )?
            .ok_or(TableFileSourceError::RowOutOfRange)?;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(identity_error)?;
        Ok(Some(row))
    }

    pub fn seek(&mut self, ordinal: u64) {
        self.next_ordinal = ordinal;
    }
}

fn bounded_preview(value: &str, max_chars: usize) -> (&str, bool) {
    value
        .char_indices()
        .nth(max_chars)
        .map_or((value, false), |(end, _)| (&value[..end], true))
}

fn provisional_row(
    source_id: TableSourceId,
    ordinal: u64,
    fields: &[String],
) -> Option<TableFileProjectedRow> {
    let count = fields.len().min(TABLE_FILE_PROVISIONAL_MAX_COLUMNS);
    let mut cells = Vec::new();
    cells.try_reserve_exact(count).ok()?;
    for field in fields.iter().take(count) {
        let (preview, truncated) = bounded_preview(field, TABLE_FILE_PROVISIONAL_CELL_CHARS);
        let extra = usize::from(truncated) * '…'.len_utf8();
        let mut cell = String::new();
        cell.try_reserve_exact(preview.len().checked_add(extra)?)
            .ok()?;
        cell.push_str(preview);
        if truncated {
            cell.push('…');
        }
        cells.push(cell);
    }
    Some(TableFileProjectedRow {
        id: TableFileRowId {
            source: source_id,
            ordinal,
        },
        cells,
    })
}

fn publish_scan_progress<F>(
    pool: &TableResourcePool,
    source_id: TableSourceId,
    preview_rows: &[TableFileProjectedRow],
    schema_columns: usize,
    validated_rows: u64,
    progress: &mut F,
) where
    F: FnMut(TableFileScanProgress),
{
    if preview_rows.is_empty() {
        return;
    }
    let mut lease = match pool.reserve_ram(TABLE_FILE_PROVISIONAL_MAX_BYTES) {
        Ok(lease) => lease,
        Err(_) => return,
    };
    let column_count = schema_columns.min(TABLE_FILE_PROVISIONAL_MAX_COLUMNS);
    let mut source_columns = Vec::new();
    if source_columns.try_reserve_exact(column_count).is_err() {
        return;
    }
    source_columns.extend(0..column_count);
    let mut rows = Vec::new();
    if rows.try_reserve_exact(preview_rows.len()).is_err() {
        return;
    }
    for row in preview_rows {
        let mut cells = Vec::new();
        if cells.try_reserve_exact(column_count).is_err() {
            return;
        }
        for index in 0..column_count {
            let value = row.cells.get(index).map_or("", String::as_str);
            let mut cell = String::new();
            if cell.try_reserve_exact(value.len()).is_err() {
                return;
            }
            cell.push_str(value);
            cells.push(cell);
        }
        rows.push(TableFileProjectedRow { id: row.id, cells });
    }
    let page = TableFileProjectedPage {
        source_id,
        start_row: 0,
        next_row: u64::try_from(rows.len()).unwrap_or(u64::MAX),
        total_rows: None,
        source_columns,
        rows,
        stop: TableFilePageStop::Provisional,
    };
    let retained = match projected_page_retained_bytes(&page) {
        Ok(retained) if retained <= lease.bytes() => retained,
        _ => return,
    };
    lease.shrink(lease.bytes() - retained);
    progress(TableFileScanProgress {
        preview: TableResourceOwned::new(Arc::new(page), Arc::new(lease)),
        validated_rows,
        schema_columns,
    });
}

fn projected_page_retained_bytes(
    page: &TableFileProjectedPage,
) -> Result<u64, TableFileSourceError> {
    let mut bytes = u64::try_from(std::mem::size_of::<TableFileProjectedPage>())
        .map_err(|_| TableFileSourceError::ProjectionLimit)?;
    bytes = bytes
        .checked_add(
            u64::try_from(page.source_columns.capacity())
                .map_err(|_| TableFileSourceError::ProjectionLimit)?
                .checked_mul(std::mem::size_of::<usize>() as u64)
                .ok_or(TableFileSourceError::ProjectionLimit)?,
        )
        .and_then(|current| {
            u64::try_from(page.rows.capacity())
                .ok()?
                .checked_mul(std::mem::size_of::<TableFileProjectedRow>() as u64)
                .and_then(|row_bytes| current.checked_add(row_bytes))
        })
        .ok_or(TableFileSourceError::ProjectionLimit)?;
    for row in &page.rows {
        bytes = bytes
            .checked_add(
                u64::try_from(row.cells.capacity())
                    .map_err(|_| TableFileSourceError::ProjectionLimit)?
                    .checked_mul(std::mem::size_of::<String>() as u64)
                    .ok_or(TableFileSourceError::ProjectionLimit)?,
            )
            .ok_or(TableFileSourceError::ProjectionLimit)?;
        for cell in &row.cells {
            bytes = bytes
                .checked_add(
                    u64::try_from(cell.capacity())
                        .map_err(|_| TableFileSourceError::ProjectionLimit)?,
                )
                .ok_or(TableFileSourceError::ProjectionLimit)?;
        }
    }
    Ok(bytes)
}

fn select_delimiter(
    file: &mut File,
    options: TableFileParseOptions,
    cancellation: &TableCancellationToken,
    evidence_width_limit: usize,
) -> Result<TableDelimiter, TableFileSourceError> {
    let limits = StreamLimits {
        max_columns: evidence_width_limit,
        evidence_width_limit,
        max_records: None,
        max_record_bytes: None,
        max_field_bytes: None,
        has_header: options.has_header,
        strict_width: false,
    };
    let mut scan = |delimiter| {
        file.seek(SeekFrom::Start(0))
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;
        let summary = stream_scan::scan_reader(
            &mut *file,
            0,
            delimiter,
            limits,
            Some(cancellation),
            None,
            |_, _| -> Result<bool, TableFileSourceError> { Ok(true) },
        )?;
        Ok::<_, TableFileSourceError>(summary)
    };
    check_parse_cancelled(Some(cancellation))?;
    match options.delimiter {
        DelimiterChoice::Explicit(delimiter) => {
            scan(delimiter)?;
            Ok(delimiter)
        }
        DelimiterChoice::Auto => {
            let comma = scan(TableDelimiter::Comma);
            if let Err(TableFileSourceError::Parse(error)) = &comma
                && error.kind == TableParseErrorKind::Cancelled
            {
                return Err(TableFileSourceError::Parse(error.clone()));
            }
            check_parse_cancelled(Some(cancellation))?;
            let tab = scan(TableDelimiter::Tab);
            if let Err(TableFileSourceError::Parse(error)) = &tab
                && error.kind == TableParseErrorKind::Cancelled
            {
                return Err(TableFileSourceError::Parse(error.clone()));
            }
            match (comma, tab) {
                (Ok(comma), Ok(tab)) => {
                    if tab.score > comma.score {
                        Ok(TableDelimiter::Tab)
                    } else {
                        Ok(TableDelimiter::Comma)
                    }
                }
                (Err(comma), Ok(tab)) if tab.score.multicolumn_records > 0 => {
                    let _ = comma;
                    Ok(TableDelimiter::Tab)
                }
                (Ok(comma), Err(tab)) if comma.score.multicolumn_records > 0 => {
                    let _ = tab;
                    Ok(TableDelimiter::Comma)
                }
                (Err(comma), _) => Err(comma),
                (_, Err(tab)) => Err(tab),
            }
        }
    }
}

fn decode_record(
    raw: &[u8],
    delimiter: TableDelimiter,
    first_source_record: bool,
    minimum_field_capacity: usize,
) -> Result<Vec<String>, TableParseError> {
    let (text, _) = decode_utf8(raw)?;
    if let Some(offset) = text.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(TableParseError::new(
            TableParseErrorKind::BinaryInput,
            Some(super::location_for_prefix(&text[..offset], 0, offset, 1, 1)),
        ));
    }
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter.byte())
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::None)
        .from_reader(Cursor::new(raw));
    let mut record = csv::StringRecord::new();
    if !reader.read_record(&mut record).map_err(csv_decode_error)? {
        return Err(TableParseError::new(
            TableParseErrorKind::MalformedDelimitedText,
            None,
        ));
    }
    let mut fields = copy_record_fields_with_capacity(&record, minimum_field_capacity)?;
    if !first_source_record
        && raw.starts_with(&[0xEF, 0xBB, 0xBF])
        && let Some(first) = fields.first_mut()
    {
        first.insert_str(0, "\u{FEFF}");
    }
    Ok(fields)
}

fn validate_decoded_fields(fields: &[String], record: StreamRecord) -> Result<(), TableParseError> {
    if fields.len() > TABLE_FILE_MAX_COLUMNS
        || fields
            .iter()
            .any(|field| field.len() as u64 > TABLE_FILE_MAX_FIELD_BYTES)
        || fields
            .iter()
            .try_fold(0_u64, |sum, field| sum.checked_add(field.len() as u64))
            .is_none_or(|bytes| bytes > TABLE_FILE_MAX_RECORD_BYTES)
    {
        return Err(TableParseError::new(
            TableParseErrorKind::InputTooLarge,
            Some(record.start),
        ));
    }
    Ok(())
}

fn decoded_row_reservation(
    fields: &[String],
    vector_capacity: usize,
) -> Result<u64, TableFileSourceError> {
    let bytes = fields
        .iter()
        .try_fold(0_u64, |total, field| {
            total.checked_add(u64::try_from(field.capacity()).ok()?)
        })
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    let slots = u64::try_from(vector_capacity)
        .ok()
        .and_then(|count| count.checked_mul(std::mem::size_of::<String>() as u64))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    bytes
        .checked_add(slots)
        .ok_or(TableResourceError::ArithmeticOverflow.into())
}

fn header_retained_bytes(
    headers: &[Option<String>],
    vector_capacity: usize,
) -> Result<u64, TableFileSourceError> {
    let slots = u64::try_from(vector_capacity)
        .ok()
        .and_then(|count| count.checked_mul(std::mem::size_of::<Option<String>>() as u64))
        .ok_or(TableResourceError::ArithmeticOverflow)?;
    headers
        .iter()
        .flatten()
        .try_fold(slots, |total, header| {
            total.checked_add(u64::try_from(header.capacity()).ok()?)
        })
        .ok_or_else(|| TableResourceError::ArithmeticOverflow.into())
}

fn push_diagnostic(
    diagnostics: &mut Vec<TableSourceDiagnostic>,
    omitted: &mut u64,
    diagnostic: TableSourceDiagnostic,
) -> Result<(), TableResourceError> {
    if diagnostics.len() < super::TABLE_DIAGNOSTIC_EXAMPLES {
        diagnostics.push(diagnostic);
    } else {
        *omitted = omitted
            .checked_add(1)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
    }
    Ok(())
}

fn append_header_diagnostics(
    headers: &[Option<String>],
    diagnostics: &mut Vec<TableSourceDiagnostic>,
    omitted: &mut u64,
) -> Result<(), TableResourceError> {
    for (index, header) in headers.iter().enumerate() {
        let Some(header) = header else {
            continue;
        };
        if header.is_empty() {
            push_diagnostic(
                diagnostics,
                omitted,
                TableSourceDiagnostic::EmptyHeader { column: index + 1 },
            )?;
        }
        for (first, previous) in headers.iter().take(index).enumerate() {
            if previous.as_deref() == Some(header.as_str()) {
                push_diagnostic(
                    diagnostics,
                    omitted,
                    TableSourceDiagnostic::DuplicateHeader {
                        column: index + 1,
                        first_column: first + 1,
                    },
                )?;
                break;
            }
        }
    }
    Ok(())
}

fn malformed_record(record: StreamRecord) -> TableParseError {
    TableParseError::new(
        TableParseErrorKind::MalformedDelimitedText,
        Some(record.start),
    )
}

fn allocation_error() -> TableParseError {
    TableParseError::new(TableParseErrorKind::AllocationFailed, None)
}

fn identity_error() -> TableParseError {
    TableParseError::new(TableParseErrorKind::IdentityExhausted, None)
}

fn next_source_id() -> Result<TableSourceId, TableFileSourceError> {
    NEXT_FILE_SOURCE_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map(TableSourceId)
        .map_err(|_| identity_error().into())
}

#[cfg(test)]
mod file_source_tests {
    use super::*;
    use crate::TableResourcePolicy;
    use std::io::Write;

    fn workspace() -> (TableResourcePool, TableTempWorkspace) {
        let pool = TableResourcePool::new(super::super::TableResourcePolicy::default());
        let workspace =
            TableTempWorkspace::create(std::env::temp_dir(), pool.clone(), u64::MAX).unwrap();
        (pool, workspace)
    }

    fn identity() -> TableSourceIdentity {
        TableSourceIdentity {
            volume_serial_number: 1,
            file_id: [7; 16],
        }
    }

    fn source_from_bytes(
        bytes: &[u8],
        options: TableFileParseOptions,
    ) -> Arc<TableFileBackedSource> {
        let path = std::env::temp_dir().join(format!(
            "table-source-test-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let source = TableFileBackedSource::scan(
            file,
            identity(),
            options,
            pool,
            &workspace,
            &TableCancellationToken::new(),
        )
        .unwrap();
        let _ = std::fs::remove_file(path);
        Arc::new(source)
    }

    fn candidate_file(bytes: &[u8]) -> (std::path::PathBuf, File) {
        let path = std::env::temp_dir().join(format!(
            "table-delimiter-candidate-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        let file = File::open(&path).unwrap();
        (path, file)
    }

    fn scan_with_column_limit(
        file: &mut File,
        delimiter: TableDelimiter,
        max_columns: usize,
    ) -> Result<(), TableParseError> {
        file.seek(SeekFrom::Start(0)).unwrap();
        stream_scan::scan_reader(
            file,
            0,
            delimiter,
            StreamLimits {
                max_columns,
                evidence_width_limit: max_columns,
                max_records: None,
                max_record_bytes: None,
                max_field_bytes: None,
                has_header: false,
                strict_width: false,
            },
            None,
            None,
            |_, _| Ok(true),
        )
        .map(|_| ())
    }

    fn scan_stream_with_limits(
        input: &[u8],
        limits: StreamLimits,
    ) -> Result<stream_scan::StreamSummary, TableParseError> {
        stream_scan::scan_reader(
            Cursor::new(input),
            0,
            TableDelimiter::Comma,
            limits,
            None,
            None,
            |_, _| Ok(true),
        )
    }

    fn delimited_fields(count: usize) -> Vec<u8> {
        let mut input = Vec::with_capacity(count.saturating_mul(2));
        for index in 0..count {
            if index != 0 {
                input.push(b',');
            }
            input.push(b'x');
        }
        input
    }

    #[test]
    fn streaming_limits_accept_exact_record_field_and_header_column_caps() {
        let record_limit = TABLE_FILE_MAX_RECORD_BYTES as usize;
        let exact_record = vec![b'x'; record_limit];
        let record_limits = StreamLimits {
            max_columns: TABLE_FILE_MAX_COLUMNS,
            evidence_width_limit: TABLE_FILE_MAX_COLUMNS,
            max_records: None,
            max_record_bytes: Some(TABLE_FILE_MAX_RECORD_BYTES),
            max_field_bytes: None,
            has_header: false,
            strict_width: false,
        };
        assert_eq!(
            scan_stream_with_limits(&exact_record, record_limits)
                .unwrap()
                .record_count,
            1
        );
        let mut over_record = exact_record;
        over_record.push(b'x');
        assert_eq!(
            scan_stream_with_limits(&over_record, record_limits)
                .unwrap_err()
                .kind,
            TableParseErrorKind::InputTooLarge
        );

        let field_limit = TABLE_FILE_MAX_FIELD_BYTES as usize;
        let exact_field = vec![b'x'; field_limit];
        let field_limits = StreamLimits {
            max_record_bytes: None,
            max_field_bytes: Some(TABLE_FILE_MAX_FIELD_BYTES),
            ..record_limits
        };
        assert_eq!(
            scan_stream_with_limits(&exact_field, field_limits)
                .unwrap()
                .record_count,
            1
        );
        let mut over_field = exact_field;
        over_field.push(b'x');
        assert_eq!(
            scan_stream_with_limits(&over_field, field_limits)
                .unwrap_err()
                .kind,
            TableParseErrorKind::InputTooLarge
        );

        let mut header_and_row = delimited_fields(TABLE_FILE_MAX_COLUMNS);
        header_and_row.push(b'\n');
        header_and_row.extend(delimited_fields(TABLE_FILE_MAX_COLUMNS));
        let column_limits = StreamLimits {
            max_records: Some(1),
            max_record_bytes: None,
            max_field_bytes: None,
            has_header: true,
            strict_width: true,
            ..record_limits
        };
        let summary = scan_stream_with_limits(&header_and_row, column_limits).unwrap();
        assert_eq!(summary.schema_width, TABLE_FILE_MAX_COLUMNS);
        assert_eq!(summary.record_count, 2);
        assert_eq!(summary.data_rows, 1);

        let overwide_header = delimited_fields(TABLE_FILE_MAX_COLUMNS + 1);
        assert_eq!(
            scan_stream_with_limits(&overwide_header, column_limits)
                .unwrap_err()
                .kind,
            TableParseErrorKind::TooManyColumns
        );
    }

    #[test]
    fn auto_delimiter_keeps_tab_choice_when_discarded_comma_width_exceeds_bound() {
        let bytes = b"alpha\tmany,commas,a,b,c\none\ttwo\tthree\none\ttwo\tthree";
        let (path, mut file) = candidate_file(bytes);
        let delimiter = select_delimiter(
            &mut file,
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, false),
            &TableCancellationToken::new(),
            3,
        )
        .unwrap();
        assert_eq!(delimiter, TableDelimiter::Tab);
        assert!(scan_with_column_limit(&mut file, delimiter, 3).is_ok());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn auto_delimiter_keeps_comma_choice_when_discarded_tab_width_exceeds_bound() {
        let bytes = b"a,b,c\tx\ty\tz\tw\np,q,r\nu,v,w";
        let (path, mut file) = candidate_file(bytes);
        let delimiter = select_delimiter(
            &mut file,
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, false),
            &TableCancellationToken::new(),
            3,
        )
        .unwrap();
        assert_eq!(delimiter, TableDelimiter::Comma);
        assert!(scan_with_column_limit(&mut file, delimiter, 3).is_ok());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn auto_delimiter_falls_back_when_the_overwide_candidate_is_rejected() {
        let bytes = b"a,b,c\tz\nx,y,z\tw\nq,r,s\tv";
        let (path, mut file) = candidate_file(bytes);
        let delimiter = select_delimiter(
            &mut file,
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, false),
            &TableCancellationToken::new(),
            2,
        )
        .unwrap();
        assert_eq!(delimiter, TableDelimiter::Tab);
        assert!(scan_with_column_limit(&mut file, delimiter, 2).is_ok());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn auto_delimiter_uses_grammar_fallback_for_quoted_tab_fields() {
        let bytes = b"head\t\"many, commas\"\nrow\t\"two, commas\"";
        let (path, mut file) = candidate_file(bytes);
        let delimiter = select_delimiter(
            &mut file,
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, false),
            &TableCancellationToken::new(),
            2,
        )
        .unwrap();
        assert_eq!(delimiter, TableDelimiter::Tab);
        assert!(scan_with_column_limit(&mut file, delimiter, 2).is_ok());
        drop(file);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn file_backed_cursor_matches_compact_rows_and_pads_lenient_widths() {
        let bytes = b"\xEF\xBB\xBFname,note\r\nalpha,\"a,b\"\r\nbeta,\"two\r\nlines\"\ngamma,tail";
        let options = TableFileParseOptions {
            profile: TableUtilityProfile::Csv,
            delimiter: DelimiterChoice::Explicit(TableDelimiter::Comma),
            has_header: true,
            strict_width: false,
        };
        let source = source_from_bytes(bytes, options);
        let compact =
            TableDocument::parse_bytes(bytes, TableUtilityProfile::Csv.initial_parse_options())
                .unwrap();
        assert_eq!(source.row_count(), compact.rows().len() as u64);
        assert_eq!(source.column_count(), compact.column_count());
        assert_eq!(source.headers(), compact.headers());
        let mut cursor = source.cursor().unwrap();
        for expected in compact.rows() {
            let actual = cursor.next_row().unwrap().unwrap();
            assert_eq!(actual.cells(), expected.cells());
        }
        assert!(cursor.next_row().unwrap().is_none());
    }

    #[test]
    fn file_backed_cursor_matches_compact_parser_across_bom_blank_and_utf8_chunks() {
        let options = TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true);
        let mut bytes = b"\xEF\xBB\xBFname,note\n\n".to_vec();
        let multibyte_offset = 64 * 1024 - 1;
        bytes.resize(multibyte_offset, b'x');
        bytes.extend_from_slice("é,\"first\r\nsecond\"\r\n\n".as_bytes());

        let compact =
            TableDocument::parse_bytes(&bytes, TableUtilityProfile::Csv.initial_parse_options())
                .unwrap();
        let source = source_from_bytes(&bytes, options);
        let mut cursor = source.cursor().unwrap();
        assert_eq!(source.row_count(), compact.rows().len() as u64);
        for expected in compact.rows() {
            let actual = cursor.next_row().unwrap().unwrap();
            assert_eq!(actual.cells(), expected.cells());
        }
        assert!(cursor.next_row().unwrap().is_none());
    }

    #[test]
    fn file_backed_malformed_record_matches_compact_error_location() {
        let bytes = b"name,value\nalpha,\"unterminated";
        let options = TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true);
        let compact =
            TableDocument::parse_bytes(bytes, TableUtilityProfile::Csv.initial_parse_options())
                .unwrap_err();
        let (path, file) = candidate_file(bytes);
        let (pool, workspace) = workspace();
        let streamed = TableFileBackedSource::scan(
            file,
            identity(),
            options,
            pool,
            &workspace,
            &TableCancellationToken::new(),
        );
        let streamed = match streamed {
            Err(TableFileSourceError::Parse(error)) => error,
            Err(_) => panic!("malformed text should preserve its parser error"),
            Ok(_) => panic!("malformed text unexpectedly produced a source"),
        };
        assert_eq!(streamed, compact);
        drop(workspace);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn file_backed_cursor_observes_cancellation_before_rescanning() {
        let source = source_from_bytes(
            b"first\nsecond",
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, false),
        );
        let cancellation = TableCancellationToken::new();
        let mut cursor = source
            .cursor_with_cancellation(cancellation.clone())
            .unwrap();
        cancellation.cancel();

        let error = match cursor.next_row() {
            Err(error) => error,
            Ok(_) => panic!("cancelled cursor unexpectedly returned a row"),
        };
        assert!(matches!(
            error,
            TableFileSourceError::Parse(TableParseError {
                kind: TableParseErrorKind::Cancelled,
                ..
            })
        ));
    }

    #[test]
    fn file_backed_projected_pages_are_source_bound_bounded_and_sparse_selection_scales_to_u64() {
        let bytes = b"name,value,note\nr0,v0,first\nr1,v1,second\nr2,v2,third\nr3,v3,last";
        let source = source_from_bytes(
            bytes,
            TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true),
        );
        let page = source
            .projected_page(
                1,
                &[2, 0],
                TABLE_FILE_PAGE_MAX_ROWS,
                TABLE_FILE_PAGE_MAX_BYTES,
                16,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(page.source_id(), source.id());
        assert_eq!(page.start_row(), 1);
        assert_eq!(page.next_row(), 4);
        assert_eq!(page.total_rows(), Some(4));
        assert_eq!(page.stop(), TableFilePageStop::EndOfSource);
        assert_eq!(page.source_columns(), &[2, 0]);
        assert_eq!(page.rows().len(), 3);
        assert_eq!(page.rows()[0].id().ordinal(), 1);
        assert_eq!(
            page.rows()[0].cell_for_source_column(&page, 2),
            Some("second")
        );
        assert_eq!(page.rows()[0].cell_for_source_column(&page, 0), Some("r1"));

        let first_page = source
            .projected_page(
                0,
                &[0],
                2,
                TABLE_FILE_PAGE_MAX_BYTES,
                TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(first_page.rows().len(), 2);
        assert_eq!(first_page.next_row(), 2);
        assert_eq!(first_page.stop(), TableFilePageStop::RowLimit);

        let last_page = source
            .projected_page(
                3,
                &[1],
                2,
                TABLE_FILE_PAGE_MAX_BYTES,
                TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(last_page.rows().len(), 1);
        assert_eq!(
            last_page.rows()[0].cell_for_source_column(&last_page, 1),
            Some("v3")
        );
        assert_eq!(last_page.next_row(), 4);
        assert_eq!(last_page.stop(), TableFilePageStop::EndOfSource);
        let eof = source
            .projected_page(
                4,
                &[0],
                1,
                TABLE_FILE_PAGE_MAX_BYTES,
                TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert!(eof.rows().is_empty());
        assert!(matches!(
            source.projected_page(
                5,
                &[0],
                1,
                TABLE_FILE_PAGE_MAX_BYTES,
                TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            ),
            Err(TableFileSourceError::RowOutOfRange)
        ));
        assert!(matches!(
            source.projected_page(
                0,
                &[TABLE_FILE_MAX_COLUMNS],
                1,
                TABLE_FILE_PAGE_MAX_BYTES,
                TABLE_FILE_PAGE_PREVIEW_CHARS,
                TableCancellationToken::new(),
            ),
            Err(TableFileSourceError::ProjectionColumnOutOfRange)
        ));

        let pool = source.pool.clone();
        let all = TableFileRowSelection::all_selected(source.id(), u64::MAX, &pool).unwrap();
        assert_eq!(all.mode(), TableFileRowSelectionMode::AllExcept);
        assert_eq!(all.selected_count(), u64::MAX);
        let exception = TableFileRowId {
            source: source.id(),
            ordinal: u64::MAX - 1,
        };
        let deselected = all.set_selected(exception, false, &pool).unwrap();
        assert_eq!(deselected.selected_count(), u64::MAX - 1);
        assert!(!deselected.is_selected(exception));
        let restored = deselected.set_selected(exception, true, &pool).unwrap();
        assert_eq!(restored.selected_count(), u64::MAX);

        let none = TableFileRowSelection::none_selected(source.id(), u64::MAX, &pool).unwrap();
        let only = none.set_selected(exception, true, &pool).unwrap();
        assert_eq!(only.mode(), TableFileRowSelectionMode::Only);
        assert_eq!(only.selected_count(), 1);
        let cleared = only.set_selected(exception, false, &pool).unwrap();
        assert_eq!(cleared.selected_count(), 0);
        assert_eq!(cleared.exception_count(), 0);
        assert!(matches!(
            all.set_selected(
                TableFileRowId {
                    source: TableSourceId(source.id().value().saturating_add(1)),
                    ordinal: 0,
                },
                false,
                &pool,
            ),
            Err(TableFileRowSelectionError::WrongSource)
        ));
    }

    #[test]
    fn file_backed_page_distinguishes_byte_limit_and_eof_and_bounds_previews() {
        let payload = "v".repeat(4096);
        let input = format!("value\n{payload}\n{payload}\n{payload}\n");
        let source = source_from_bytes(
            input.as_bytes(),
            TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
        );

        let full = source
            .projected_page(
                0,
                &[0],
                2,
                TABLE_FILE_PAGE_MAX_BYTES,
                160,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(full.rows().len(), 2);
        assert_eq!(full.stop(), TableFilePageStop::RowLimit);
        let empty_page_bytes = u64::try_from(std::mem::size_of::<TableFileProjectedPage>())
            .unwrap()
            + std::mem::size_of::<usize>() as u64
            + 2 * std::mem::size_of::<TableFileProjectedRow>() as u64;
        let one_row_estimate = std::mem::size_of::<TableFileProjectedRow>() as u64
            + 2 * std::mem::size_of::<String>() as u64
            + 2 * (160_u64 + 3);
        let one_row_budget = empty_page_bytes + one_row_estimate;
        assert!(one_row_budget < TABLE_FILE_PAGE_MAX_BYTES);
        let byte_limited = source
            .projected_page(
                0,
                &[0],
                2,
                one_row_budget,
                160,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(byte_limited.rows().len(), 1);
        assert_eq!(byte_limited.stop(), TableFilePageStop::ByteLimit);
        assert_eq!(byte_limited.total_rows(), Some(3));
        assert!(
            byte_limited.next_row() < byte_limited.total_rows().unwrap(),
            "a byte-limited page is not an end-of-source page"
        );

        let bounded_cell = source
            .projected_page(
                0,
                &[0],
                1,
                TABLE_FILE_PAGE_MAX_BYTES,
                32,
                TableCancellationToken::new(),
            )
            .unwrap();
        let cell = bounded_cell.rows()[0]
            .cell_for_source_column(&bounded_cell, 0)
            .unwrap();
        assert_eq!(cell.chars().count(), 33);
        assert!(cell.ends_with('…'));

        let no_columns = source
            .projected_page(
                0,
                &[],
                2,
                TABLE_FILE_PAGE_MAX_BYTES,
                32,
                TableCancellationToken::new(),
            )
            .unwrap();
        assert_eq!(no_columns.rows().len(), 2);
        assert!(
            no_columns.rows().iter().all(|row| row.cells().is_empty()),
            "zero-column pages retain row identity without cell payloads"
        );
        assert_eq!(no_columns.stop(), TableFilePageStop::RowLimit);
    }

    #[test]
    fn file_row_selection_quota_refusal_is_transactional_and_clones_retain_charge() {
        let pool = TableResourcePool::new(super::super::TableResourcePolicy::default());
        let source = TableSourceId(0x9a);
        let selection = TableFileRowSelection::all_selected(source, 8, &pool).unwrap();
        let retained_snapshot = selection.clone();
        let baseline = pool.usage().ram_bytes;
        assert!(
            baseline > 0,
            "the empty selection owner has a retained charge"
        );

        let reserve_for_blocker = TableResourcePolicy::DEFAULT_RAM_BYTES
            .checked_sub(baseline)
            .and_then(|available| available.checked_sub(1))
            .expect("selection leaves room under the default quota");
        let blocker = pool
            .capture_policy()
            .reserve_ram(reserve_for_blocker)
            .unwrap();
        let row = TableFileRowId { source, ordinal: 3 };
        let usage_before_edit = pool.usage().ram_bytes;
        assert!(matches!(
            selection.set_selected(row, false, &pool),
            Err(TableFileRowSelectionError::Resource(
                TableResourceError::RamQuota { .. }
            ))
        ));
        assert_eq!(pool.usage().ram_bytes, usage_before_edit);
        assert!(selection.is_selected(row));
        assert_eq!(selection.exception_count(), 0);
        drop(blocker);

        drop(selection);
        assert_eq!(
            pool.usage().ram_bytes,
            baseline,
            "a snapshot clone retains the original selection's full charge"
        );
        drop(retained_snapshot);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn late_width_error_leaves_only_provisional_preview_publications() {
        let path = std::env::temp_dir().join(format!(
            "table-late-malformed-progress-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut bytes = b"id,value\n".to_vec();
        for row in 0..16 {
            bytes.extend_from_slice(format!("{row},valid\n").as_bytes());
        }
        bytes.extend_from_slice(b"17,late,extra\n");
        std::fs::write(&path, bytes).unwrap();
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let mut previews = Vec::new();
        let options = TableFileParseOptions {
            profile: TableUtilityProfile::Csv,
            delimiter: DelimiterChoice::Explicit(TableDelimiter::Comma),
            has_header: true,
            strict_width: true,
        };
        let result = TableFileBackedSource::scan_with_progress(
            file,
            identity(),
            options,
            pool,
            &workspace,
            &TableCancellationToken::new(),
            |progress| previews.push(progress),
        );
        let error = match result {
            Err(TableFileSourceError::Parse(error)) => error,
            Err(other) => panic!("late row-width error should preserve parser context: {other}"),
            Ok(_) => panic!("late row-width error unexpectedly produced an accepted source"),
        };
        assert_eq!(error.kind, TableParseErrorKind::RowWidthMismatch);
        assert_eq!(error.expected_columns, Some(2));
        assert_eq!(error.actual_columns, Some(3));
        assert_eq!(error.location.map(|location| location.record), Some(18));
        assert!(
            previews
                .iter()
                .any(|progress| progress.validated_rows() == 16),
            "valid preceding rows were observable only through a provisional preview"
        );
        assert!(previews.iter().all(|progress| progress.preview().stop()
            == TableFilePageStop::Provisional
            && progress.preview().total_rows().is_none()));
        drop(previews);
        drop(workspace);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn file_backed_scan_publishes_only_bounded_provisional_rows() {
        let path = std::env::temp_dir().join(format!(
            "table-progress-test-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut bytes = b"id,value,extra\n".to_vec();
        for row in 0..40 {
            bytes.extend_from_slice(format!("{row},{},{}\n", "v".repeat(200), row).as_bytes());
        }
        std::fs::write(&path, &bytes).unwrap();
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let mut progress_updates = Vec::new();
        let source = TableFileBackedSource::scan_with_progress(
            file,
            identity(),
            TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true),
            pool,
            &workspace,
            &TableCancellationToken::new(),
            |progress| progress_updates.push(progress),
        )
        .unwrap();
        assert_eq!(source.row_count(), 40);
        assert_eq!(progress_updates.len(), 2);
        for progress in &progress_updates {
            let page = progress.preview();
            assert_eq!(page.source_id(), source.id());
            assert_eq!(page.stop(), TableFilePageStop::Provisional);
            assert_eq!(page.total_rows(), None);
            assert!(page.rows().len() <= TABLE_FILE_PROVISIONAL_MAX_ROWS);
            assert!(page.source_columns().len() <= TABLE_FILE_PROVISIONAL_MAX_COLUMNS);
            assert!(page.rows().iter().all(|row| {
                row.cells()
                    .iter()
                    .all(|cell| cell.chars().count() <= TABLE_FILE_PROVISIONAL_CELL_CHARS + 1)
            }));
        }
        assert_eq!(progress_updates[0].validated_rows(), 1);
        assert_eq!(progress_updates[0].preview().rows().len(), 1);
        assert_eq!(progress_updates[1].validated_rows(), 16);
        assert_eq!(progress_updates[1].preview().rows().len(), 16);
        assert!(progress_updates[1].schema_columns() <= TABLE_FILE_MAX_COLUMNS);
        drop(progress_updates);
        drop(source);
        drop(workspace);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn decoded_row_capacity_stays_charged_only_while_the_row_is_retained() {
        let path = std::env::temp_dir().join(format!(
            "table-row-capacity-test-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"left,right\nalpha,beta\n").unwrap();
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let source = Arc::new(
            TableFileBackedSource::scan(
                file,
                identity(),
                TableFileParseOptions::for_profile(TableUtilityProfile::Table, true),
                pool.clone(),
                &workspace,
                &TableCancellationToken::new(),
            )
            .unwrap(),
        );
        let mut cursor = source.cursor().unwrap();
        let cursor_usage = pool.usage().ram_bytes;
        let row = cursor.next_row().unwrap().unwrap();
        let row_usage = pool.usage().ram_bytes;
        assert!(row_usage > cursor_usage);
        drop(row);
        assert_eq!(pool.usage().ram_bytes, cursor_usage);

        drop(cursor);
        drop(source);
        drop(workspace);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn file_backed_source_scans_past_the_legacy_sixteen_mib_route() {
        let path = std::env::temp_dir().join(format!(
            "table-large-source-test-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(b"id,value\n").unwrap();
        let payload = "x".repeat(80);
        for row in 0..240_000_u64 {
            writeln!(file, "{row},payload-{row:06}-{payload}").unwrap();
        }
        drop(file);
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let source = Arc::new(
            TableFileBackedSource::scan(
                file,
                identity(),
                TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true),
                pool.clone(),
                &workspace,
                &TableCancellationToken::new(),
            )
            .unwrap(),
        );
        assert!(source.byte_count() > 16 * 1024 * 1024);
        assert_eq!(source.row_count(), 240_000);
        let mut cursor = source.cursor().unwrap();
        cursor.seek(239_999);
        let late = cursor.next_row().unwrap().unwrap();
        assert_eq!(late.cell(0), Some("239999"));
        assert_eq!(
            late.cell(1),
            Some(format!("payload-239999-{payload}").as_str())
        );
        assert!(pool.usage().ram_bytes < 32 * 1024 * 1024);
        let peak = pool.peak_usage();
        // The bounded provisional builder remains charged alongside the
        // working buffers and diagnostic lease. During header decoding, the
        // maximum header reservation also overlaps those owners; later the
        // decoded-row reservation is smaller than that header bound.
        let decoder_peak_bound = FILE_WORKING_RAM_BYTES
            + MAX_HEADER_RESERVATION.max(MAX_DECODED_ROW_RESERVATION)
            + DIAGNOSTIC_RESERVATION
            + TABLE_FILE_PROVISIONAL_MAX_BYTES
            + 8 * 1024;
        assert!(peak.ram_bytes > FILE_WORKING_RAM_BYTES);
        assert!(
            peak.ram_bytes <= decoder_peak_bound,
            "observed peak {} exceeds derived bound {}",
            peak.ram_bytes,
            decoder_peak_bound
        );
        drop(source);
        drop(workspace);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn file_backed_scan_rejects_oversized_first_header_record() {
        let path = std::env::temp_dir().join(format!(
            "table-oversized-header-test-{}.csv",
            NEXT_FILE_SOURCE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = File::create(&path).unwrap();
        let block = vec![b'x'; 1024 * 1024];
        for _ in 0..8 {
            file.write_all(&block).unwrap();
        }
        file.write_all(b"x\n").unwrap();
        drop(file);
        let file = File::open(&path).unwrap();
        let (pool, workspace) = workspace();
        let result = TableFileBackedSource::scan(
            file,
            identity(),
            TableFileParseOptions::for_profile(TableUtilityProfile::Csv, true),
            pool,
            &workspace,
            &TableCancellationToken::new(),
        );
        assert!(matches!(
            result,
            Err(TableFileSourceError::Parse(TableParseError {
                kind: TableParseErrorKind::InputTooLarge,
                ..
            }))
        ));
        let _ = std::fs::remove_file(path);
    }
}
