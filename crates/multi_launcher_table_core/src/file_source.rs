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
        mut file: File,
        identity: TableSourceIdentity,
        options: TableFileParseOptions,
        pool: TableResourcePool,
        workspace: &TableTempWorkspace,
        cancellation: &TableCancellationToken,
    ) -> Result<Self, TableFileSourceError> {
        check_parse_cancelled(Some(cancellation))?;
        if !workspace.uses_pool(&pool) {
            return Err(TableFileSourceError::Resource(
                TableResourceError::PoolMismatch,
            ));
        }
        let initial_metadata = file
            .metadata()
            .map_err(|error| TableFileSourceError::Io(error.to_string()))?;
        let initial_len = initial_metadata.len();
        let mut sparse_index = workspace.create_file("table-index")?;
        let working_lease = pool.reserve_ram(FILE_WORKING_RAM_BYTES)?;
        let mut metadata_lease = pool.reserve_ram(0)?;
        metadata_lease.grow(DIAGNOSTIC_RESERVATION)?;

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
                    drop(fields);
                }
                record_index = record_index.checked_add(1).ok_or_else(identity_error)?;
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

        let id = NEXT_FILE_SOURCE_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map(TableSourceId)
            .map_err(|_| TableParseError::new(TableParseErrorKind::IdentityExhausted, None))?;
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
        let buffer_lease = self.pool.reserve_ram(FILE_WORKING_RAM_BYTES)?;
        Ok(TableFileRowCursor {
            source: Arc::clone(self),
            next_ordinal: 0,
            cancellation,
            _buffer_lease: buffer_lease,
        })
    }

    fn read_row(
        &self,
        ordinal: u64,
        working_lease: &TableResourceLease,
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
                    let mut row_lease = self.pool.reserve_ram(MAX_DECODED_ROW_RESERVATION)?;
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
    _buffer_lease: TableResourceLease,
}

impl TableFileRowCursor {
    pub fn next_row(&mut self) -> Result<Option<TableFileRow>, TableFileSourceError> {
        if self.next_ordinal >= self.source.row_count {
            return Ok(None);
        }
        let row = self
            .source
            .read_row(self.next_ordinal, &self._buffer_lease, &self.cancellation)?
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

#[cfg(test)]
mod file_source_tests {
    use super::*;
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
        let decoder_peak_bound = FILE_WORKING_RAM_BYTES
            + MAX_HEADER_RESERVATION.max(MAX_DECODED_ROW_RESERVATION)
            + DIAGNOSTIC_RESERVATION
            + 8 * 1024;
        assert!(peak.ram_bytes > FILE_WORKING_RAM_BYTES);
        assert!(peak.ram_bytes <= decoder_peak_bound);
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
