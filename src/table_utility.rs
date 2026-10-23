//! Bounded, immutable CSV/TSV parsing and read-only result projections.
//!
//! This module deliberately owns no files, workers, dialogs, or clipboard
//! access. A later UI can keep one [`TableDocument`] and rebuild cheap row
//! index projections without reparsing or changing the source strings.

use std::cmp::Ordering;
use std::fmt;
use std::io::{self, Write};
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};

use std::collections::BTreeMap;

pub const TABLE_UTILITY_MAX_INPUT_BYTES: usize = 10 * 1024 * 1024;
pub const TABLE_UTILITY_MAX_DATA_ROWS: usize = 50_000;
pub const TABLE_UTILITY_MAX_COLUMNS: usize = 128;
pub const TABLE_UTILITY_MAX_CELLS: usize = 1_000_000;
pub const TABLE_UTILITY_MAX_MATERIALIZED_BYTES: usize = 48 * 1024 * 1024;
pub const TABLE_UTILITY_MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const TABLE_CELL_PREVIEW_CHARS: usize = 160;
pub const TABLE_DIAGNOSTIC_EXAMPLES: usize = 64;
/// Keep transient search projections bounded independently of table size.
pub const TABLE_FILTER_MAX_BYTES: usize = 64 * 1024;

/// Maximum limits supported by the shared engine so later routes can select
/// their stricter CSV profile without adding another parser.
pub const STRICT_CSV_MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const STRICT_CSV_MAX_DATA_ROWS: usize = 100_000;
pub const STRICT_CSV_MAX_COLUMNS: usize = 256;
pub const STRICT_CSV_MAX_CELLS: usize = 1_000_000;
pub const STRICT_CSV_MAX_MATERIALIZED_BYTES: usize = 64 * 1024 * 1024;
pub const STRICT_CSV_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CONFIGURABLE_PREVIEW_CHARS: usize = 512;
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const SCANNER_CANCELLATION_INTERVAL: usize = 4096;
static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_VIEW_ID: AtomicU64 = AtomicU64::new(1);

/// A lightweight, one-shot cancellation signal for later asynchronous owners.
/// The parser, projection, and exporters never create or retain worker state.
#[derive(Clone, Debug, Default)]
pub struct TableCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl TableCancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, AtomicOrdering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(AtomicOrdering::Acquire)
    }

    fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

fn check_parse_cancelled(
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableParseError> {
    if cancellation.is_some_and(TableCancellationToken::is_cancelled) {
        Err(TableParseError::new(TableParseErrorKind::Cancelled, None))
    } else {
        Ok(())
    }
}

fn check_projection_cancelled(
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableProjectionError> {
    if cancellation.is_some_and(TableCancellationToken::is_cancelled) {
        Err(TableProjectionError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_export_cancelled(
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableExportError> {
    if cancellation.is_some_and(TableCancellationToken::is_cancelled) {
        Err(TableExportError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TableDelimiter {
    Comma,
    Tab,
}

impl TableDelimiter {
    pub const fn byte(self) -> u8 {
        match self {
            Self::Comma => b',',
            Self::Tab => b'\t',
        }
    }

    pub const fn as_char(self) -> char {
        self.byte() as char
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DelimiterChoice {
    Auto,
    Explicit(TableDelimiter),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowWidthPolicy {
    /// Short rows are padded and reported; extra columns widen the schema.
    Lenient,
    /// Every data row must match the header or first data row width.
    Strict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableResourceLimits {
    pub max_input_bytes: usize,
    pub max_data_rows: usize,
    pub max_columns: usize,
    pub max_cells: usize,
    /// Approximate retained cell payload plus one `String` slot per cell.
    pub max_materialized_bytes: usize,
    pub max_output_bytes: usize,
    pub cell_preview_chars: usize,
}

impl TableResourceLimits {
    /// Defaults for the compact in-app table utility (M9-A/G2).
    pub const fn utility() -> Self {
        Self {
            max_input_bytes: TABLE_UTILITY_MAX_INPUT_BYTES,
            max_data_rows: TABLE_UTILITY_MAX_DATA_ROWS,
            max_columns: TABLE_UTILITY_MAX_COLUMNS,
            max_cells: TABLE_UTILITY_MAX_CELLS,
            max_materialized_bytes: TABLE_UTILITY_MAX_MATERIALIZED_BYTES,
            max_output_bytes: TABLE_UTILITY_MAX_OUTPUT_BYTES,
            cell_preview_chars: TABLE_CELL_PREVIEW_CHARS,
        }
    }

    /// The later explicit CSV route can opt into these larger, strict limits.
    pub const fn strict_csv_route() -> Self {
        Self {
            max_input_bytes: STRICT_CSV_MAX_INPUT_BYTES,
            max_data_rows: STRICT_CSV_MAX_DATA_ROWS,
            max_columns: STRICT_CSV_MAX_COLUMNS,
            max_cells: STRICT_CSV_MAX_CELLS,
            max_materialized_bytes: STRICT_CSV_MAX_MATERIALIZED_BYTES,
            max_output_bytes: STRICT_CSV_MAX_OUTPUT_BYTES,
            cell_preview_chars: TABLE_CELL_PREVIEW_CHARS,
        }
    }

    fn validate(self) -> Result<(), TableParseError> {
        if self.max_input_bytes == 0
            || self.max_input_bytes > STRICT_CSV_MAX_INPUT_BYTES
            || self.max_data_rows == 0
            || self.max_data_rows > STRICT_CSV_MAX_DATA_ROWS
            || self.max_columns == 0
            || self.max_columns > STRICT_CSV_MAX_COLUMNS
            || self.max_cells == 0
            || self.max_cells > STRICT_CSV_MAX_CELLS
            || self.max_materialized_bytes == 0
            || self.max_materialized_bytes > STRICT_CSV_MAX_MATERIALIZED_BYTES
            || self.max_output_bytes == 0
            || self.max_output_bytes > STRICT_CSV_MAX_OUTPUT_BYTES
            || self.cell_preview_chars == 0
            || self.cell_preview_chars > MAX_CONFIGURABLE_PREVIEW_CHARS
        {
            return Err(TableParseError::new(
                TableParseErrorKind::InvalidLimits,
                None,
            ));
        }
        Ok(())
    }
}

impl Default for TableResourceLimits {
    fn default() -> Self {
        Self::utility()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableParseOptions {
    pub delimiter: DelimiterChoice,
    pub has_header: bool,
    pub row_width: RowWidthPolicy,
    pub limits: TableResourceLimits,
}

impl TableParseOptions {
    pub const fn utility_default() -> Self {
        Self {
            delimiter: DelimiterChoice::Auto,
            has_header: false,
            row_width: RowWidthPolicy::Lenient,
            limits: TableResourceLimits::utility(),
        }
    }

    /// A strict, explicit-delimiter profile for a future CSV command. It uses
    /// the same parser and validator as the utility's default profile.
    pub const fn strict_csv_route(delimiter: TableDelimiter, has_header: bool) -> Self {
        Self {
            delimiter: DelimiterChoice::Explicit(delimiter),
            has_header,
            row_width: RowWidthPolicy::Strict,
            limits: TableResourceLimits::strict_csv_route(),
        }
    }
}

impl Default for TableParseOptions {
    fn default() -> Self {
        Self::utility_default()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableDocumentId(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableRowId {
    document: TableDocumentId,
    source_index: usize,
}

impl TableRowId {
    pub const fn document(self) -> TableDocumentId {
        self.document
    }

    pub const fn source_index(self) -> usize {
        self.source_index
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TableViewId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableLocation {
    /// Zero-based byte offset into the original source, including a BOM.
    pub byte_offset: usize,
    /// One-based physical line and Unicode scalar column.
    pub line: usize,
    pub column: usize,
    /// One-based logical record and field.
    pub record: usize,
    pub field: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableParseErrorKind {
    InputTooLarge,
    TooManyRows,
    TooManyColumns,
    TooManyCells,
    MaterializationLimit,
    OutputTooLarge,
    InvalidUtf8,
    UnsupportedEncoding,
    BinaryInput,
    MalformedDelimitedText,
    RowWidthMismatch,
    AllocationFailed,
    InvalidLimits,
    IdentityExhausted,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableParseError {
    pub kind: TableParseErrorKind,
    pub location: Option<TableLocation>,
    pub expected_columns: Option<usize>,
    pub actual_columns: Option<usize>,
}

impl TableParseError {
    const fn new(kind: TableParseErrorKind, location: Option<TableLocation>) -> Self {
        Self {
            kind,
            location,
            expected_columns: None,
            actual_columns: None,
        }
    }

    fn width(location: TableLocation, expected_columns: usize, actual_columns: usize) -> Self {
        Self {
            kind: TableParseErrorKind::RowWidthMismatch,
            location: Some(location),
            expected_columns: Some(expected_columns),
            actual_columns: Some(actual_columns),
        }
    }

    pub const fn description(&self) -> &'static str {
        match self.kind {
            TableParseErrorKind::InputTooLarge => "The input exceeds the configured byte limit.",
            TableParseErrorKind::TooManyRows => "The input exceeds the configured row limit.",
            TableParseErrorKind::TooManyColumns => "The input exceeds the configured column limit.",
            TableParseErrorKind::TooManyCells => "The table exceeds the configured cell limit.",
            TableParseErrorKind::MaterializationLimit => {
                "The table exceeds the configured memory estimate."
            }
            TableParseErrorKind::OutputTooLarge => {
                "The selected output exceeds the configured byte limit."
            }
            TableParseErrorKind::InvalidUtf8 => "The input is not valid UTF-8 text.",
            TableParseErrorKind::UnsupportedEncoding => {
                "UTF-16 input is unsupported; provide UTF-8 text."
            }
            TableParseErrorKind::BinaryInput => "The input contains binary data.",
            TableParseErrorKind::MalformedDelimitedText => {
                "The delimited text has malformed quoting or record separators."
            }
            TableParseErrorKind::RowWidthMismatch => {
                "A row does not match the expected number of columns."
            }
            TableParseErrorKind::AllocationFailed => {
                "The table could not be allocated within available memory."
            }
            TableParseErrorKind::InvalidLimits => "The configured table limits are invalid.",
            TableParseErrorKind::IdentityExhausted => {
                "No additional table snapshot identity is available."
            }
            TableParseErrorKind::Cancelled => "The table operation was cancelled.",
        }
    }
}

impl fmt::Display for TableParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.description())?;
        if let Some(location) = self.location {
            write!(
                formatter,
                " (record {}, field {}, line {}, column {}).",
                location.record, location.field, location.line, location.column
            )?;
        }
        if let (Some(expected), Some(actual)) = (self.expected_columns, self.actual_columns) {
            write!(formatter, " Expected {expected} columns, found {actual}.")?;
        }
        Ok(())
    }
}

impl std::error::Error for TableParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableWarning {
    ShortRowPadded {
        record: usize,
        actual_columns: usize,
        padded_columns: usize,
    },
    SchemaWidened {
        record: usize,
        previous_columns: usize,
        new_columns: usize,
    },
    HeaderLabelsMissing {
        first_column: usize,
        count: usize,
    },
    EmptyHeader {
        column: usize,
    },
    DuplicateHeader {
        column: usize,
        first_column: usize,
    },
    AdditionalWarningsOmitted {
        count: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableRow {
    id: TableRowId,
    cells: Vec<String>,
}

impl TableRow {
    pub const fn id(&self) -> TableRowId {
        self.id
    }

    pub fn cells(&self) -> &[String] {
        &self.cells
    }

    pub fn cell(&self, column: usize) -> Option<&str> {
        self.cells.get(column).map(String::as_str)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableDocument {
    id: TableDocumentId,
    delimiter: TableDelimiter,
    has_header: bool,
    /// `None` marks a schema column added beyond the source header row.
    headers: Option<Vec<Option<String>>>,
    rows: Vec<TableRow>,
    column_count: usize,
    diagnostics: Vec<TableWarning>,
    omitted_diagnostics: usize,
    options: TableParseOptions,
}

impl TableDocument {
    pub fn parse(input: &str, options: TableParseOptions) -> Result<Self, TableParseError> {
        Self::parse_bytes(input.as_bytes(), options)
    }

    /// Parse with a caller-owned cancellation signal. Existing synchronous
    /// callers can continue using [`Self::parse`] without allocating a token.
    pub fn parse_with_cancellation(
        input: &str,
        options: TableParseOptions,
        cancellation: &TableCancellationToken,
    ) -> Result<Self, TableParseError> {
        Self::parse_bytes_with_cancellation(input.as_bytes(), options, cancellation)
    }

    pub fn parse_bytes(input: &[u8], options: TableParseOptions) -> Result<Self, TableParseError> {
        Self::parse_bytes_inner(input, options, None)
    }

    pub fn parse_bytes_with_cancellation(
        input: &[u8],
        options: TableParseOptions,
        cancellation: &TableCancellationToken,
    ) -> Result<Self, TableParseError> {
        Self::parse_bytes_inner(input, options, Some(cancellation))
    }

    fn parse_bytes_inner(
        input: &[u8],
        options: TableParseOptions,
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<Self, TableParseError> {
        check_parse_cancelled(cancellation)?;
        options.limits.validate()?;
        if input.len() > options.limits.max_input_bytes {
            return Err(TableParseError::new(
                TableParseErrorKind::InputTooLarge,
                None,
            ));
        }
        let (payload, base_offset) = decode_utf8(input)?;
        check_parse_cancelled(cancellation)?;
        if let Some(offset) = payload.as_bytes().iter().position(|byte| *byte == 0) {
            let prefix = &payload[..offset];
            return Err(TableParseError::new(
                TableParseErrorKind::BinaryInput,
                Some(location_for_prefix(prefix, base_offset, offset, 1, 1)),
            ));
        }

        let (delimiter, lexical) =
            scan_for_delimiter(payload.as_bytes(), base_offset, options, cancellation)?;
        validate_widths(&lexical.records, options, cancellation)?;
        let mut schema_width = 0usize;
        for (index, record) in lexical.records.iter().enumerate() {
            if index % 256 == 0 {
                check_parse_cancelled(cancellation)?;
            }
            schema_width = schema_width.max(record.width);
        }
        let data_rows = lexical
            .records
            .len()
            .saturating_sub(usize::from(options.has_header));
        validate_storage(
            data_rows,
            schema_width,
            options.has_header,
            input.len(),
            options.limits,
        )?;

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter.byte())
            .has_headers(false)
            .flexible(true)
            .trim(csv::Trim::None)
            // csv-core owns removal of exactly one leading UTF-8 BOM. The
            // lexical pass scans `payload`, which has removed that same BOM
            // once for location accounting, so a second literal BOM remains
            // part of the first cell.
            .from_reader(input);
        let mut decoded = Vec::<Vec<String>>::new();
        decoded
            .try_reserve(data_rows)
            .map_err(|_| TableParseError::new(TableParseErrorKind::AllocationFailed, None))?;
        let mut decoded_header = None;
        let mut record = csv::StringRecord::new();
        let mut record_index = 0usize;
        while reader.read_record(&mut record).map_err(csv_decode_error)? {
            check_parse_cancelled(cancellation)?;
            let width_meta = lexical.records.get(record_index).ok_or_else(|| {
                TableParseError::new(TableParseErrorKind::MalformedDelimitedText, None)
            })?;
            if record.len() != width_meta.width {
                return Err(TableParseError::new(
                    TableParseErrorKind::MalformedDelimitedText,
                    Some(width_meta.start),
                ));
            }
            if options.has_header && record_index == 0 {
                let fields = copy_record_fields(&record)?;
                let mut headers = Vec::new();
                headers.try_reserve(fields.len()).map_err(|_| {
                    TableParseError::new(TableParseErrorKind::AllocationFailed, None)
                })?;
                headers.extend(fields.into_iter().map(Some));
                decoded_header = Some(headers);
            } else {
                decoded.push(copy_record_fields(&record)?);
            }
            record_index += 1;
        }
        check_parse_cancelled(cancellation)?;
        if record_index != lexical.records.len() {
            return Err(TableParseError::new(
                TableParseErrorKind::MalformedDelimitedText,
                lexical.records.get(record_index).map(|record| record.start),
            ));
        }

        let mut diagnostics = Vec::new();
        let mut omitted_diagnostics = 0usize;
        let mut observed_width = if options.has_header {
            lexical.records.first().map_or(0, |record| record.width)
        } else {
            0
        };
        for (index, shape) in lexical
            .records
            .iter()
            .enumerate()
            .skip(usize::from(options.has_header))
        {
            if index % 256 == 0 {
                check_parse_cancelled(cancellation)?;
            }
            if shape.width > observed_width && observed_width > 0 {
                add_warning(
                    &mut diagnostics,
                    &mut omitted_diagnostics,
                    TableWarning::SchemaWidened {
                        record: index + 1,
                        previous_columns: observed_width,
                        new_columns: shape.width,
                    },
                );
            }
            observed_width = observed_width.max(shape.width);
        }
        if let Some(headers) = decoded_header.as_mut() {
            if headers.len() < schema_width {
                add_warning(
                    &mut diagnostics,
                    &mut omitted_diagnostics,
                    TableWarning::HeaderLabelsMissing {
                        first_column: headers.len() + 1,
                        count: schema_width - headers.len(),
                    },
                );
                headers
                    .try_reserve(schema_width - headers.len())
                    .map_err(|_| {
                        TableParseError::new(TableParseErrorKind::AllocationFailed, None)
                    })?;
                headers.resize(schema_width, None);
            }
        }
        for (index, cells) in decoded.iter_mut().enumerate() {
            if index % 256 == 0 {
                check_parse_cancelled(cancellation)?;
            }
            let record_meta_index = index + usize::from(options.has_header);
            let original_width = lexical.records[record_meta_index].width;
            if original_width < schema_width {
                add_warning(
                    &mut diagnostics,
                    &mut omitted_diagnostics,
                    TableWarning::ShortRowPadded {
                        record: record_meta_index + 1,
                        actual_columns: original_width,
                        padded_columns: schema_width - original_width,
                    },
                );
                cells.try_reserve(schema_width - cells.len()).map_err(|_| {
                    TableParseError::new(TableParseErrorKind::AllocationFailed, None)
                })?;
                cells.resize(schema_width, String::new());
            }
        }
        if let Some(headers) = decoded_header.as_ref() {
            collect_header_diagnostics(headers, &mut diagnostics, &mut omitted_diagnostics);
        }
        if omitted_diagnostics > 0 {
            diagnostics.push(TableWarning::AdditionalWarningsOmitted {
                count: omitted_diagnostics,
            });
        }

        let id = next_document_id()?;
        let mut rows = Vec::new();
        rows.try_reserve(decoded.len())
            .map_err(|_| TableParseError::new(TableParseErrorKind::AllocationFailed, None))?;
        for (source_index, cells) in decoded.into_iter().enumerate() {
            if source_index % 256 == 0 {
                check_parse_cancelled(cancellation)?;
            }
            rows.push(TableRow {
                id: TableRowId {
                    document: id,
                    source_index,
                },
                cells,
            });
        }
        check_parse_cancelled(cancellation)?;

        Ok(Self {
            id,
            delimiter,
            has_header: options.has_header,
            headers: decoded_header,
            rows,
            column_count: schema_width,
            diagnostics,
            omitted_diagnostics,
            options,
        })
    }

    pub const fn id(&self) -> TableDocumentId {
        self.id
    }

    pub const fn delimiter(&self) -> TableDelimiter {
        self.delimiter
    }

    pub const fn has_header(&self) -> bool {
        self.has_header
    }

    pub const fn column_count(&self) -> usize {
        self.column_count
    }

    pub fn headers(&self) -> Option<&[Option<String>]> {
        self.headers.as_deref()
    }

    pub fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    pub fn row(&self, id: TableRowId) -> Option<&TableRow> {
        if id.document != self.id {
            return None;
        }
        self.rows.get(id.source_index).filter(|row| row.id == id)
    }

    pub fn diagnostics(&self) -> &[TableWarning] {
        &self.diagnostics
    }

    pub const fn omitted_diagnostic_count(&self) -> usize {
        self.omitted_diagnostics
    }

    pub const fn options(&self) -> TableParseOptions {
        self.options
    }

    pub fn bounded_preview(&self, row: TableRowId, column: usize) -> Option<String> {
        self.row(row)
            .and_then(|row| row.cell(column))
            .map(|cell| bounded_cell_preview(cell, self.options.limits.cell_preview_chars))
    }
}

#[derive(Clone, Copy, Debug)]
struct RecordShape {
    width: usize,
    start: TableLocation,
}

#[derive(Clone, Debug)]
struct LexicalScan {
    records: Vec<RecordShape>,
    score: DelimiterEvidence,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DelimiterEvidence {
    multicolumn_records: usize,
    modal_multicolumn_count: usize,
    widest_record: usize,
}

impl Ord for DelimiterEvidence {
    fn cmp(&self, other: &Self) -> Ordering {
        (
            self.modal_multicolumn_count,
            self.multicolumn_records,
            self.widest_record,
        )
            .cmp(&(
                other.modal_multicolumn_count,
                other.multicolumn_records,
                other.widest_record,
            ))
    }
}

impl PartialOrd for DelimiterEvidence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LexState {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn decode_utf8(input: &[u8]) -> Result<(&str, usize), TableParseError> {
    if input.starts_with(&[0xFF, 0xFE]) || input.starts_with(&[0xFE, 0xFF]) {
        return Err(TableParseError::new(
            TableParseErrorKind::UnsupportedEncoding,
            None,
        ));
    }
    let base_offset = usize::from(input.starts_with(UTF8_BOM)) * UTF8_BOM.len();
    let payload = &input[base_offset..];
    match std::str::from_utf8(payload) {
        Ok(text) => Ok((text, base_offset)),
        Err(error) => {
            let valid = &payload[..error.valid_up_to()];
            let prefix = std::str::from_utf8(valid).unwrap_or_default();
            Err(TableParseError::new(
                TableParseErrorKind::InvalidUtf8,
                Some(location_for_prefix(
                    prefix,
                    base_offset,
                    error.valid_up_to(),
                    1,
                    1,
                )),
            ))
        }
    }
}

fn copy_record_fields(record: &csv::StringRecord) -> Result<Vec<String>, TableParseError> {
    let mut fields = Vec::new();
    fields
        .try_reserve(record.len())
        .map_err(|_| TableParseError::new(TableParseErrorKind::AllocationFailed, None))?;
    for value in record.iter() {
        let mut field = String::new();
        field
            .try_reserve(value.len())
            .map_err(|_| TableParseError::new(TableParseErrorKind::AllocationFailed, None))?;
        field.push_str(value);
        fields.push(field);
    }
    Ok(fields)
}

fn location_for_prefix(
    prefix: &str,
    base_offset: usize,
    relative_offset: usize,
    record: usize,
    field: usize,
) -> TableLocation {
    let mut line = 1;
    let mut column = 1;
    let mut previous_cr = false;
    for character in prefix.chars() {
        match character {
            '\r' => {
                line += 1;
                column = 1;
                previous_cr = true;
            }
            '\n' => {
                if !previous_cr {
                    line += 1;
                }
                column = 1;
                previous_cr = false;
            }
            _ => {
                column += 1;
                previous_cr = false;
            }
        }
    }
    TableLocation {
        byte_offset: base_offset.saturating_add(relative_offset),
        line,
        column,
        record,
        field,
    }
}

fn scan_for_delimiter(
    input: &[u8],
    base_offset: usize,
    options: TableParseOptions,
    cancellation: Option<&TableCancellationToken>,
) -> Result<(TableDelimiter, LexicalScan), TableParseError> {
    check_parse_cancelled(cancellation)?;
    match options.delimiter {
        DelimiterChoice::Explicit(delimiter) => Ok((
            delimiter,
            scan_records(input, base_offset, delimiter, options, cancellation)?,
        )),
        DelimiterChoice::Auto => {
            let comma = scan_records(
                input,
                base_offset,
                TableDelimiter::Comma,
                options,
                cancellation,
            );
            if let Err(error) = &comma
                && error.kind == TableParseErrorKind::Cancelled
            {
                return Err(error.clone());
            }
            check_parse_cancelled(cancellation)?;
            let tab = scan_records(
                input,
                base_offset,
                TableDelimiter::Tab,
                options,
                cancellation,
            );
            if let Err(error) = &tab
                && error.kind == TableParseErrorKind::Cancelled
            {
                return Err(error.clone());
            }
            match (comma, tab) {
                (Ok(comma_scan), Ok(tab_scan)) => {
                    if tab_scan.score > comma_scan.score {
                        Ok((TableDelimiter::Tab, tab_scan))
                    } else {
                        // Comma is the deterministic tie fallback, including
                        // inputs with no meaningful multi-column evidence.
                        Ok((TableDelimiter::Comma, comma_scan))
                    }
                }
                (Err(comma_error), Ok(tab_scan)) => {
                    if tab_scan.score.multicolumn_records > 0 {
                        Ok((TableDelimiter::Tab, tab_scan))
                    } else {
                        // Do not let malformed comma data pass as one TSV cell.
                        Err(comma_error)
                    }
                }
                (Ok(comma_scan), Err(tab_error)) => {
                    if comma_scan.score.multicolumn_records > 0 {
                        Ok((TableDelimiter::Comma, comma_scan))
                    } else {
                        Err(tab_error)
                    }
                }
                (Err(comma_error), Err(_)) => Err(comma_error),
            }
        }
    }
}

fn scan_records(
    input: &[u8],
    base_offset: usize,
    delimiter: TableDelimiter,
    options: TableParseOptions,
    cancellation: Option<&TableCancellationToken>,
) -> Result<LexicalScan, TableParseError> {
    let bytes = input;
    let mut records = Vec::new();
    let mut index = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;
    let mut record_start = 0usize;
    let mut record_start_line = 1usize;
    let mut record_number = 1usize;
    let mut field_number = 1usize;
    let mut state = LexState::FieldStart;
    let mut record_touched = false;
    let mut next_cancellation_checkpoint = 0usize;

    let mut schema_width = 0usize;
    let mut push_record = |records: &mut Vec<RecordShape>,
                           width: usize,
                           start_offset: usize,
                           start_line: usize,
                           record: usize,
                           end_offset: usize|
     -> Result<(), TableParseError> {
        let allowed_records = options
            .limits
            .max_data_rows
            .saturating_add(usize::from(options.has_header));
        if records.len() >= allowed_records {
            return Err(TableParseError::new(
                TableParseErrorKind::TooManyRows,
                Some(TableLocation {
                    byte_offset: base_offset.saturating_add(start_offset),
                    line: start_line,
                    column: 1,
                    record,
                    field: 1,
                }),
            ));
        }
        if width > options.limits.max_columns {
            return Err(TableParseError::new(
                TableParseErrorKind::TooManyColumns,
                Some(TableLocation {
                    byte_offset: base_offset.saturating_add(start_offset),
                    line: start_line,
                    column: 1,
                    record,
                    field: options.limits.max_columns.saturating_add(1),
                }),
            ));
        }
        let record_meta = RecordShape {
            width,
            start: TableLocation {
                byte_offset: base_offset.saturating_add(start_offset),
                line: start_line,
                column: 1,
                record,
                field: 1,
            },
        };
        records
            .try_reserve(1)
            .map_err(|_| TableParseError::new(TableParseErrorKind::AllocationFailed, None))?;
        records.push(record_meta);

        let data_rows = records
            .len()
            .saturating_sub(usize::from(options.has_header));
        schema_width = schema_width.max(width);
        validate_storage(
            data_rows,
            schema_width,
            options.has_header,
            end_offset,
            options.limits,
        )
    };

    while index < bytes.len() {
        scanner_cancellation_checkpoint(index, &mut next_cancellation_checkpoint, cancellation)?;
        let byte = bytes[index];
        match state {
            LexState::FieldStart => {
                if byte == delimiter.byte() {
                    record_touched = true;
                    field_number += 1;
                    if field_number > options.limits.max_columns {
                        return Err(location_error(
                            TableParseErrorKind::TooManyColumns,
                            base_offset,
                            index,
                            line,
                            column,
                            record_number,
                            field_number,
                        ));
                    }
                    index += 1;
                    column += 1;
                } else if byte == b'"' {
                    record_touched = true;
                    state = LexState::Quoted;
                    index += 1;
                    column += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    let (next_index, next_line) = consume_record_terminator(
                        bytes,
                        index,
                        base_offset,
                        line,
                        column,
                        record_number,
                        field_number,
                    )?;
                    let blank_line = !record_touched && field_number == 1;
                    if !blank_line {
                        push_record(
                            &mut records,
                            field_number,
                            record_start,
                            record_start_line,
                            record_number,
                            base_offset.saturating_add(next_index),
                        )?;
                        record_number += 1;
                    }
                    index = next_index;
                    line = next_line;
                    column = 1;
                    record_start = index;
                    record_start_line = line;
                    field_number = 1;
                    state = LexState::FieldStart;
                    record_touched = false;
                } else {
                    record_touched = true;
                    state = LexState::Unquoted;
                    advance_utf8_byte(bytes, &mut index, &mut column);
                }
            }
            LexState::Unquoted => {
                if byte == delimiter.byte() {
                    field_number += 1;
                    if field_number > options.limits.max_columns {
                        return Err(location_error(
                            TableParseErrorKind::TooManyColumns,
                            base_offset,
                            index,
                            line,
                            column,
                            record_number,
                            field_number,
                        ));
                    }
                    index += 1;
                    column += 1;
                    state = LexState::FieldStart;
                } else if byte == b'"' {
                    return Err(location_error(
                        TableParseErrorKind::MalformedDelimitedText,
                        base_offset,
                        index,
                        line,
                        column,
                        record_number,
                        field_number,
                    ));
                } else if byte == b'\n' || byte == b'\r' {
                    let (next_index, next_line) = consume_record_terminator(
                        bytes,
                        index,
                        base_offset,
                        line,
                        column,
                        record_number,
                        field_number,
                    )?;
                    push_record(
                        &mut records,
                        field_number,
                        record_start,
                        record_start_line,
                        record_number,
                        base_offset.saturating_add(next_index),
                    )?;
                    index = next_index;
                    line = next_line;
                    column = 1;
                    record_start = index;
                    record_start_line = line;
                    record_number += 1;
                    field_number = 1;
                    state = LexState::FieldStart;
                    record_touched = false;
                } else {
                    advance_utf8_byte(bytes, &mut index, &mut column);
                }
            }
            LexState::Quoted => {
                if byte == b'"' {
                    if bytes.get(index + 1) == Some(&b'"') {
                        index += 2;
                        column += 2;
                    } else {
                        index += 1;
                        column += 1;
                        state = LexState::AfterQuote;
                    }
                } else if byte == b'\n' || byte == b'\r' {
                    let (next_index, next_line) = consume_embedded_newline(
                        bytes,
                        index,
                        base_offset,
                        line,
                        column,
                        record_number,
                        field_number,
                    )?;
                    index = next_index;
                    line = next_line;
                    column = 1;
                } else {
                    advance_utf8_byte(bytes, &mut index, &mut column);
                }
            }
            LexState::AfterQuote => {
                if byte == delimiter.byte() {
                    field_number += 1;
                    if field_number > options.limits.max_columns {
                        return Err(location_error(
                            TableParseErrorKind::TooManyColumns,
                            base_offset,
                            index,
                            line,
                            column,
                            record_number,
                            field_number,
                        ));
                    }
                    index += 1;
                    column += 1;
                    state = LexState::FieldStart;
                } else if byte == b'\n' || byte == b'\r' {
                    let (next_index, next_line) = consume_record_terminator(
                        bytes,
                        index,
                        base_offset,
                        line,
                        column,
                        record_number,
                        field_number,
                    )?;
                    push_record(
                        &mut records,
                        field_number,
                        record_start,
                        record_start_line,
                        record_number,
                        base_offset.saturating_add(next_index),
                    )?;
                    index = next_index;
                    line = next_line;
                    column = 1;
                    record_start = index;
                    record_start_line = line;
                    record_number += 1;
                    field_number = 1;
                    state = LexState::FieldStart;
                    record_touched = false;
                } else {
                    return Err(location_error(
                        TableParseErrorKind::MalformedDelimitedText,
                        base_offset,
                        index,
                        line,
                        column,
                        record_number,
                        field_number,
                    ));
                }
            }
        }
    }

    if state == LexState::Quoted {
        return Err(location_error(
            TableParseErrorKind::MalformedDelimitedText,
            base_offset,
            bytes.len(),
            line,
            column,
            record_number,
            field_number,
        ));
    }
    if record_touched || field_number > 1 || state != LexState::FieldStart {
        push_record(
            &mut records,
            field_number,
            record_start,
            record_start_line,
            record_number,
            base_offset.saturating_add(bytes.len()),
        )?;
    }

    let mut width_counts = BTreeMap::<usize, usize>::new();
    let mut multicolumn_records = 0usize;
    let mut widest_record = 0usize;
    for (index, shape) in records.iter().enumerate() {
        if index % 256 == 0 {
            check_parse_cancelled(cancellation)?;
        }
        widest_record = widest_record.max(shape.width);
        if shape.width > 1 {
            multicolumn_records += 1;
            *width_counts.entry(shape.width).or_default() += 1;
        }
    }
    check_parse_cancelled(cancellation)?;
    let modal_multicolumn_count = width_counts.values().copied().max().unwrap_or(0);
    Ok(LexicalScan {
        records,
        score: DelimiterEvidence {
            multicolumn_records,
            modal_multicolumn_count,
            widest_record,
        },
    })
}

fn consume_record_terminator(
    bytes: &[u8],
    index: usize,
    base_offset: usize,
    line: usize,
    column: usize,
    record: usize,
    field: usize,
) -> Result<(usize, usize), TableParseError> {
    match bytes[index] {
        b'\n' => Ok((index + 1, line + 1)),
        b'\r' if bytes.get(index + 1) == Some(&b'\n') => Ok((index + 2, line + 1)),
        _ => Err(location_error(
            TableParseErrorKind::MalformedDelimitedText,
            base_offset,
            index,
            line,
            column,
            record,
            field,
        )),
    }
}

fn consume_embedded_newline(
    bytes: &[u8],
    index: usize,
    base_offset: usize,
    line: usize,
    column: usize,
    record: usize,
    field: usize,
) -> Result<(usize, usize), TableParseError> {
    consume_record_terminator(bytes, index, base_offset, line, column, record, field)
}

fn advance_utf8_byte(bytes: &[u8], index: &mut usize, column: &mut usize) {
    if bytes[*index] & 0b1100_0000 != 0b1000_0000 {
        *column += 1;
    }
    *index += 1;
}

fn location_error(
    kind: TableParseErrorKind,
    base_offset: usize,
    offset: usize,
    line: usize,
    column: usize,
    record: usize,
    field: usize,
) -> TableParseError {
    TableParseError::new(
        kind,
        Some(TableLocation {
            byte_offset: base_offset.saturating_add(offset),
            line,
            column,
            record,
            field,
        }),
    )
}

fn scanner_cancellation_checkpoint(
    byte_offset: usize,
    next_checkpoint: &mut usize,
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableParseError> {
    if byte_offset >= *next_checkpoint {
        check_parse_cancelled(cancellation)?;
        *next_checkpoint = byte_offset.saturating_add(SCANNER_CANCELLATION_INTERVAL);
    }
    Ok(())
}

fn validate_widths(
    records: &[RecordShape],
    options: TableParseOptions,
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableParseError> {
    check_parse_cancelled(cancellation)?;
    if options.row_width != RowWidthPolicy::Strict || records.is_empty() {
        return Ok(());
    }
    let data_start = usize::from(options.has_header);
    let Some(first_data) = records.get(data_start) else {
        return Ok(());
    };
    let expected = if options.has_header {
        records[0].width
    } else {
        first_data.width
    };
    for (index, record) in records.iter().skip(data_start).enumerate() {
        if index % 256 == 0 {
            check_parse_cancelled(cancellation)?;
        }
        if record.width != expected {
            return Err(TableParseError::width(record.start, expected, record.width));
        }
    }
    Ok(())
}

fn validate_storage(
    data_rows: usize,
    schema_width: usize,
    has_header: bool,
    input_bytes: usize,
    limits: TableResourceLimits,
) -> Result<(), TableParseError> {
    let row_cells = data_rows
        .checked_mul(schema_width)
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::TooManyCells, None))?;
    let header_cells = if has_header { schema_width } else { 0 };
    let cells = row_cells
        .checked_add(header_cells)
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::TooManyCells, None))?;
    if cells > limits.max_cells {
        return Err(TableParseError::new(
            TableParseErrorKind::TooManyCells,
            None,
        ));
    }
    let slots_bytes = cells
        .checked_mul(size_of::<String>())
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::MaterializationLimit, None))?;
    let row_slots = data_rows
        .checked_mul(size_of::<TableRow>())
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::MaterializationLimit, None))?;
    // Auto mode may hold both delimiter shape scans while comparing evidence.
    let shape_count = data_rows.saturating_add(usize::from(has_header));
    let scan_metadata = shape_count
        .checked_mul(size_of::<RecordShape>())
        .and_then(|bytes| bytes.checked_mul(2))
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::MaterializationLimit, None))?;
    let estimate = input_bytes
        .checked_add(slots_bytes)
        .and_then(|bytes| bytes.checked_add(row_slots))
        .and_then(|bytes| bytes.checked_add(scan_metadata))
        .ok_or_else(|| TableParseError::new(TableParseErrorKind::MaterializationLimit, None))?;
    if estimate > limits.max_materialized_bytes {
        return Err(TableParseError::new(
            TableParseErrorKind::MaterializationLimit,
            None,
        ));
    }
    Ok(())
}

fn csv_decode_error(error: csv::Error) -> TableParseError {
    let location = error.position().map(|position| {
        let offset = usize::try_from(position.byte()).unwrap_or(usize::MAX);
        TableLocation {
            // The CSV reader receives the original byte slice, so its byte
            // offset is already relative to the original input (including a
            // stripped BOM, when present).
            byte_offset: offset,
            line: usize::try_from(position.line()).unwrap_or(1),
            column: 1,
            record: usize::try_from(position.record())
                .unwrap_or(1)
                .saturating_add(1),
            field: 1,
        }
    });
    TableParseError::new(TableParseErrorKind::MalformedDelimitedText, location)
}

fn add_warning(warnings: &mut Vec<TableWarning>, omitted: &mut usize, warning: TableWarning) {
    if warnings.len() < TABLE_DIAGNOSTIC_EXAMPLES {
        warnings.push(warning);
    } else {
        *omitted = omitted.saturating_add(1);
    }
}

fn collect_header_diagnostics(
    headers: &[Option<String>],
    warnings: &mut Vec<TableWarning>,
    omitted: &mut usize,
) {
    let mut first_seen = std::collections::HashMap::<&str, usize>::new();
    for (index, header) in headers.iter().enumerate() {
        let column = index + 1;
        let Some(header) = header else { continue };
        if header.is_empty() {
            add_warning(warnings, omitted, TableWarning::EmptyHeader { column });
        }
        if let Some(first_column) = first_seen.get(header.as_str()).copied() {
            add_warning(
                warnings,
                omitted,
                TableWarning::DuplicateHeader {
                    column,
                    first_column,
                },
            );
        } else {
            first_seen.insert(header.as_str(), column);
        }
    }
}

fn next_document_id() -> Result<TableDocumentId, TableParseError> {
    NEXT_DOCUMENT_ID
        .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |id| {
            id.checked_add(1)
        })
        .map(TableDocumentId)
        .map_err(|_| TableParseError::new(TableParseErrorKind::IdentityExhausted, None))
}

fn next_view_id() -> Result<TableViewId, TableProjectionError> {
    NEXT_VIEW_ID
        .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |id| {
            id.checked_add(1)
        })
        .map(TableViewId)
        .map_err(|_| TableProjectionError::IdentityExhausted)
}

/// Returns a Unicode-safe, payload-preserving preview. It is for a deliberate
/// future UI preview only; filtering and export always use the full cell.
pub fn bounded_cell_preview(value: &str, max_chars: usize) -> String {
    let mut preview = String::new();
    let mut chars = value.chars();
    for _ in 0..max_chars {
        let Some(character) = chars.next() else {
            return preview;
        };
        preview.push(character);
    }
    if chars.next().is_some() {
        preview.push('…');
    }
    preview
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableSortDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableSortMode {
    Lexical,
    AutoNumeric,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableSort {
    pub column: usize,
    pub direction: TableSortDirection,
    pub mode: TableSortMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableProjectionOptions {
    pub filter: String,
    /// Empty means search every column. A non-empty list is an explicit set.
    pub filter_columns: Vec<usize>,
    pub sort: Option<TableSort>,
}

impl Default for TableProjectionOptions {
    fn default() -> Self {
        Self {
            filter: String::new(),
            filter_columns: Vec::new(),
            sort: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableProjectionError {
    Cancelled,
    IdentityExhausted,
    AllocationFailed,
    FilterTooLarge,
    TooManyFilterColumns,
    TooManySelectedColumns,
    TooManySelectedRows,
    WrongDocument,
    WrongView,
    InvalidColumn,
    NoColumns,
    NoRows,
    HiddenRow,
    DuplicateRow,
    InvalidObjectHeaders,
}

impl fmt::Display for TableProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cancelled => "The table operation was cancelled.",
            Self::IdentityExhausted => "No additional table view identity is available.",
            Self::AllocationFailed => "The table view could not be allocated.",
            Self::FilterTooLarge => "The table filter exceeds the configured byte limit.",
            Self::TooManyFilterColumns => "The filter contains too many column references.",
            Self::TooManySelectedColumns => "The selection contains too many columns.",
            Self::TooManySelectedRows => "The selection contains too many rows.",
            Self::WrongDocument => "The selection belongs to a different table snapshot.",
            Self::WrongView => "The selection belongs to a different table view.",
            Self::InvalidColumn => "A selected column is no longer available.",
            Self::NoColumns => "Select at least one column.",
            Self::NoRows => "Select at least one visible row.",
            Self::HiddenRow => "A selected row is filtered out of the current view.",
            Self::DuplicateRow => "A selected row appears more than once.",
            Self::InvalidObjectHeaders => {
                "Object JSON requires a non-empty unique header for every selected column."
            }
        })
    }
}

impl std::error::Error for TableProjectionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectiveSortMode {
    Lexical,
    Numeric,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableProjection {
    id: TableViewId,
    document: TableDocumentId,
    visible_rows: Vec<TableRowId>,
    effective_sort: Option<EffectiveSortMode>,
}

impl TableProjection {
    pub fn build(
        document: &TableDocument,
        options: &TableProjectionOptions,
    ) -> Result<Self, TableProjectionError> {
        Self::build_inner(document, options, None)
    }

    pub fn build_with_cancellation(
        document: &TableDocument,
        options: &TableProjectionOptions,
        cancellation: &TableCancellationToken,
    ) -> Result<Self, TableProjectionError> {
        Self::build_inner(document, options, Some(cancellation))
    }

    fn build_inner(
        document: &TableDocument,
        options: &TableProjectionOptions,
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<Self, TableProjectionError> {
        check_projection_cancelled(cancellation)?;
        if options.filter_columns.len() > document.column_count {
            return Err(TableProjectionError::TooManyFilterColumns);
        }
        if options
            .filter_columns
            .iter()
            .any(|column| *column >= document.column_count)
        {
            return Err(TableProjectionError::InvalidColumn);
        }
        if options.filter.len() > TABLE_FILTER_MAX_BYTES {
            return Err(TableProjectionError::FilterTooLarge);
        }
        if let Some(sort) = options.sort
            && sort.column >= document.column_count
        {
            return Err(TableProjectionError::InvalidColumn);
        }

        let query = options.filter.to_lowercase();
        let mut visible_rows = Vec::new();
        visible_rows
            .try_reserve(document.rows.len())
            .map_err(|_| TableProjectionError::AllocationFailed)?;
        for (index, row) in document.rows.iter().enumerate() {
            if index % 128 == 0 {
                check_projection_cancelled(cancellation)?;
            }
            let matches = query.is_empty()
                || if options.filter_columns.is_empty() {
                    row.cells
                        .iter()
                        .any(|cell| cell.to_lowercase().contains(&query))
                } else {
                    options.filter_columns.iter().any(|column| {
                        row.cells
                            .get(*column)
                            .is_some_and(|cell| cell.to_lowercase().contains(&query))
                    })
                };
            if matches {
                visible_rows.push(row.id);
            }
        }

        let effective_sort = if let Some(sort) = options.sort {
            let numeric = if sort.mode == TableSortMode::AutoNumeric {
                let mut has_populated_values = false;
                let mut all_populated_values_are_finite = true;
                for (index, row) in document.rows.iter().enumerate() {
                    if index % 128 == 0 {
                        check_projection_cancelled(cancellation)?;
                    }
                    if let Some(value) = row.cells.get(sort.column)
                        && !value.is_empty()
                    {
                        has_populated_values = true;
                        all_populated_values_are_finite &= finite_number(value).is_some();
                    }
                }
                has_populated_values && all_populated_values_are_finite
            } else {
                false
            };
            let effective = if numeric {
                EffectiveSortMode::Numeric
            } else {
                EffectiveSortMode::Lexical
            };
            sort_visible_rows(&mut visible_rows, document, sort, effective, cancellation)?;
            Some(effective)
        } else {
            None
        };

        check_projection_cancelled(cancellation)?;
        Ok(Self {
            id: next_view_id()?,
            document: document.id,
            visible_rows,
            effective_sort,
        })
    }

    pub const fn id(&self) -> TableViewId {
        self.id
    }

    pub const fn document_id(&self) -> TableDocumentId {
        self.document
    }

    pub fn visible_rows(&self) -> &[TableRowId] {
        &self.visible_rows
    }

    pub const fn effective_sort_mode(&self) -> Option<EffectiveSortMode> {
        self.effective_sort
    }

    /// Bind a selection to the exact source snapshot and visible projection.
    /// Rows are stored in displayed order and columns in original source order.
    pub fn select(
        &self,
        document: &TableDocument,
        selected_rows: &[TableRowId],
        selected_columns: &[usize],
    ) -> Result<TableSelection, TableProjectionError> {
        self.select_inner(document, selected_rows, selected_columns, None)
    }

    pub fn select_with_cancellation(
        &self,
        document: &TableDocument,
        selected_rows: &[TableRowId],
        selected_columns: &[usize],
        cancellation: &TableCancellationToken,
    ) -> Result<TableSelection, TableProjectionError> {
        self.select_inner(
            document,
            selected_rows,
            selected_columns,
            Some(cancellation),
        )
    }

    fn select_inner(
        &self,
        document: &TableDocument,
        selected_rows: &[TableRowId],
        selected_columns: &[usize],
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<TableSelection, TableProjectionError> {
        check_projection_cancelled(cancellation)?;
        if self.document != document.id {
            return Err(TableProjectionError::WrongDocument);
        }
        if selected_columns.is_empty() {
            return Err(TableProjectionError::NoColumns);
        }
        if selected_rows.is_empty() {
            return Err(TableProjectionError::NoRows);
        }
        if selected_columns.len() > document.column_count {
            return Err(TableProjectionError::TooManySelectedColumns);
        }
        if selected_rows.len() > self.visible_rows.len() {
            return Err(TableProjectionError::TooManySelectedRows);
        }
        if selected_columns
            .iter()
            .any(|column| *column >= document.column_count)
        {
            return Err(TableProjectionError::InvalidColumn);
        }

        let mut requested = std::collections::HashSet::new();
        requested
            .try_reserve(selected_rows.len())
            .map_err(|_| TableProjectionError::AllocationFailed)?;
        for (index, row) in selected_rows.iter().enumerate() {
            if index % 128 == 0 {
                check_projection_cancelled(cancellation)?;
            }
            if row.document != document.id || document.row(*row).is_none() {
                return Err(TableProjectionError::WrongDocument);
            }
            if !requested.insert(*row) {
                return Err(TableProjectionError::DuplicateRow);
            }
        }
        let mut rows = Vec::new();
        rows.try_reserve(selected_rows.len())
            .map_err(|_| TableProjectionError::AllocationFailed)?;
        for (index, visible) in self.visible_rows.iter().enumerate() {
            if index % 128 == 0 {
                check_projection_cancelled(cancellation)?;
            }
            if requested.remove(visible) {
                rows.push(*visible);
            }
        }
        if !requested.is_empty() {
            return Err(TableProjectionError::HiddenRow);
        }
        let mut columns = Vec::new();
        columns
            .try_reserve(selected_columns.len())
            .map_err(|_| TableProjectionError::AllocationFailed)?;
        columns.extend_from_slice(selected_columns);
        check_projection_cancelled(cancellation)?;
        columns.sort_unstable();
        columns.dedup();
        check_projection_cancelled(cancellation)?;
        Ok(TableSelection {
            document: self.document,
            view: self.id,
            rows,
            columns,
        })
    }

    pub fn export_tsv(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        include_headers: bool,
    ) -> Result<String, TableExportError> {
        self.export_tsv_inner(document, selection, include_headers, None)
    }

    pub fn export_tsv_with_cancellation(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        include_headers: bool,
        cancellation: &TableCancellationToken,
    ) -> Result<String, TableExportError> {
        self.export_tsv_inner(document, selection, include_headers, Some(cancellation))
    }

    fn export_tsv_inner(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        include_headers: bool,
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<String, TableExportError> {
        check_export_cancelled(cancellation)?;
        self.validate_selection_inner(document, selection, cancellation)?;
        let sink = BoundedOutput::new(
            document.options.limits.max_output_bytes,
            cancellation.map(TableCancellationToken::flag),
        );
        let mut writer = csv::WriterBuilder::new()
            .delimiter(b'\t')
            .terminator(csv::Terminator::Any(b'\n'))
            .has_headers(false)
            .buffer_capacity(8 * 1024)
            .from_writer(sink);
        if include_headers {
            if let Some(headers) = document.headers.as_ref() {
                let fields: Vec<&str> = selection
                    .columns
                    .iter()
                    .map(|column| {
                        headers
                            .get(*column)
                            .and_then(Option::as_deref)
                            .unwrap_or("")
                    })
                    .collect();
                writer
                    .write_record(fields)
                    .map_err(|_| writer_error(&writer))?;
            }
        }
        for (index, row_id) in selection.rows.iter().enumerate() {
            if index % 128 == 0 {
                check_export_cancelled(cancellation)?;
            }
            let row = document
                .row(*row_id)
                .ok_or(TableProjectionError::WrongDocument)?;
            let fields: Vec<&str> = selection
                .columns
                .iter()
                .map(|column| row.cell(*column).unwrap_or(""))
                .collect();
            writer
                .write_record(fields)
                .map_err(|_| writer_error(&writer))?;
        }
        let sink = writer.into_inner().map_err(|error| {
            let writer = error.into_inner();
            writer_error(&writer)
        })?;
        check_export_cancelled(cancellation)?;
        sink.into_string()
    }

    pub fn export_json(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        requested_shape: TableJsonShape,
    ) -> Result<TableJsonExport, TableExportError> {
        self.export_json_inner(document, selection, requested_shape, None)
    }

    pub fn export_json_with_cancellation(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        requested_shape: TableJsonShape,
        cancellation: &TableCancellationToken,
    ) -> Result<TableJsonExport, TableExportError> {
        self.export_json_inner(document, selection, requested_shape, Some(cancellation))
    }

    fn export_json_inner(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        requested_shape: TableJsonShape,
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<TableJsonExport, TableExportError> {
        check_export_cancelled(cancellation)?;
        self.validate_selection_inner(document, selection, cancellation)?;
        let (shape, fallback_reason) = resolve_json_shape(document, selection, requested_shape)?;
        let mut output = BoundedOutput::new(
            document.options.limits.max_output_bytes,
            cancellation.map(TableCancellationToken::flag),
        );
        write_json_table(&mut output, document, selection, shape, cancellation)?;
        check_export_cancelled(cancellation)?;
        let text = output.into_string()?;
        Ok(TableJsonExport {
            text,
            shape,
            fallback_reason,
        })
    }

    fn validate_selection_inner(
        &self,
        document: &TableDocument,
        selection: &TableSelection,
        cancellation: Option<&TableCancellationToken>,
    ) -> Result<(), TableProjectionError> {
        check_projection_cancelled(cancellation)?;
        if self.document != document.id || selection.document != document.id {
            return Err(TableProjectionError::WrongDocument);
        }
        if selection.view != self.id {
            return Err(TableProjectionError::WrongView);
        }
        if selection.columns.is_empty() {
            return Err(TableProjectionError::NoColumns);
        }
        if selection.rows.is_empty() {
            return Err(TableProjectionError::NoRows);
        }
        if selection
            .columns
            .iter()
            .any(|column| *column >= document.column_count)
        {
            return Err(TableProjectionError::InvalidColumn);
        }
        for (index, row) in selection.rows.iter().enumerate() {
            if index % 128 == 0 {
                check_projection_cancelled(cancellation)?;
            }
            if document.row(*row).is_none() {
                return Err(TableProjectionError::WrongDocument);
            }
            // A selection can only be constructed by `TableProjection::select`
            // and its fields are private, so this exact view identity proves
            // visibility without a quadratic scan of all visible rows here.
        }
        check_projection_cancelled(cancellation)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableSelection {
    document: TableDocumentId,
    view: TableViewId,
    rows: Vec<TableRowId>,
    columns: Vec<usize>,
}

impl TableSelection {
    pub fn rows(&self) -> &[TableRowId] {
        &self.rows
    }

    pub fn columns(&self) -> &[usize] {
        &self.columns
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableJsonShape {
    Auto,
    Objects,
    Arrays,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableResolvedJsonShape {
    Objects,
    Arrays,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JsonShapeFallbackReason {
    HeadersDisabled,
    HeaderMissing,
    EmptyHeader,
    DuplicateHeader,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableJsonExport {
    pub text: String,
    pub shape: TableResolvedJsonShape,
    pub fallback_reason: Option<JsonShapeFallbackReason>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableExportError {
    Projection(TableProjectionError),
    Cancelled,
    OutputTooLarge,
    AllocationFailed,
    WriterFailure,
    InvalidUtf8Output,
}

impl From<TableProjectionError> for TableExportError {
    fn from(value: TableProjectionError) -> Self {
        if value == TableProjectionError::Cancelled {
            Self::Cancelled
        } else {
            Self::Projection(value)
        }
    }
}

impl fmt::Display for TableExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => error.fmt(formatter),
            Self::Cancelled => formatter.write_str("The table operation was cancelled."),
            Self::OutputTooLarge => {
                formatter.write_str("The selected output exceeds the configured byte limit.")
            }
            Self::AllocationFailed => {
                formatter.write_str("The selected output could not be allocated.")
            }
            Self::WriterFailure => formatter.write_str("The selected output could not be encoded."),
            Self::InvalidUtf8Output => {
                formatter.write_str("The selected output was not valid UTF-8.")
            }
        }
    }
}

impl std::error::Error for TableExportError {}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
    output_too_large: bool,
    allocation_failed: bool,
    cancelled: bool,
    cancellation: Option<Arc<AtomicBool>>,
}

impl BoundedOutput {
    fn new(limit: usize, cancellation: Option<Arc<AtomicBool>>) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            output_too_large: false,
            allocation_failed: false,
            cancelled: false,
            cancellation,
        }
    }

    fn into_string(self) -> Result<String, TableExportError> {
        if self.cancelled {
            return Err(TableExportError::Cancelled);
        }
        if self.output_too_large {
            return Err(TableExportError::OutputTooLarge);
        }
        if self.allocation_failed {
            return Err(TableExportError::AllocationFailed);
        }
        String::from_utf8(self.bytes).map_err(|_| TableExportError::InvalidUtf8Output)
    }
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(AtomicOrdering::Acquire))
        {
            self.cancelled = true;
            return Err(io::Error::other("table operation cancelled"));
        }
        let Some(new_len) = self.bytes.len().checked_add(bytes.len()) else {
            self.output_too_large = true;
            return Err(io::Error::other("table output limit exceeded"));
        };
        if new_len > self.limit {
            self.output_too_large = true;
            return Err(io::Error::other("table output limit exceeded"));
        }
        if self.bytes.try_reserve(bytes.len()).is_err() {
            self.allocation_failed = true;
            return Err(io::Error::other("table output allocation failed"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn writer_error(writer: &csv::Writer<BoundedOutput>) -> TableExportError {
    output_error(writer.get_ref())
}

fn output_error(output: &BoundedOutput) -> TableExportError {
    if output.cancelled {
        TableExportError::Cancelled
    } else if output.output_too_large {
        TableExportError::OutputTooLarge
    } else if output.allocation_failed {
        TableExportError::AllocationFailed
    } else {
        TableExportError::WriterFailure
    }
}

fn resolve_json_shape(
    document: &TableDocument,
    selection: &TableSelection,
    requested: TableJsonShape,
) -> Result<(TableResolvedJsonShape, Option<JsonShapeFallbackReason>), TableExportError> {
    if requested == TableJsonShape::Arrays {
        return Ok((TableResolvedJsonShape::Arrays, None));
    }
    let headers = document.headers.as_ref();
    let problem = if !document.has_header {
        Some(JsonShapeFallbackReason::HeadersDisabled)
    } else if headers.is_none() {
        Some(JsonShapeFallbackReason::HeaderMissing)
    } else if let Some(headers) = headers {
        if selection
            .columns
            .iter()
            .any(|column| *column >= headers.len())
            || selection
                .columns
                .iter()
                .any(|column| headers[*column].is_none())
        {
            Some(JsonShapeFallbackReason::HeaderMissing)
        } else if selection
            .columns
            .iter()
            .any(|column| headers[*column].as_deref() == Some(""))
        {
            Some(JsonShapeFallbackReason::EmptyHeader)
        } else {
            let mut names = std::collections::HashSet::new();
            if selection.columns.iter().any(|column| {
                headers[*column]
                    .as_deref()
                    .is_some_and(|name| !names.insert(name))
            }) {
                Some(JsonShapeFallbackReason::DuplicateHeader)
            } else {
                None
            }
        }
    } else {
        Some(JsonShapeFallbackReason::HeaderMissing)
    };
    if let Some(reason) = problem {
        return match requested {
            TableJsonShape::Auto => Ok((TableResolvedJsonShape::Arrays, Some(reason))),
            TableJsonShape::Objects => Err(TableProjectionError::InvalidObjectHeaders.into()),
            TableJsonShape::Arrays => Ok((TableResolvedJsonShape::Arrays, None)),
        };
    }
    Ok((TableResolvedJsonShape::Objects, None))
}

fn write_json_table(
    output: &mut BoundedOutput,
    document: &TableDocument,
    selection: &TableSelection,
    shape: TableResolvedJsonShape,
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableExportError> {
    write_limited(output, b"[")?;
    for (row_index, row_id) in selection.rows.iter().enumerate() {
        if row_index % 128 == 0 {
            check_export_cancelled(cancellation)?;
        }
        if row_index > 0 {
            write_limited(output, b",")?;
        }
        let row = document
            .row(*row_id)
            .ok_or(TableProjectionError::WrongDocument)?;
        match shape {
            TableResolvedJsonShape::Arrays => {
                write_limited(output, b"[")?;
                for (column_index, column) in selection.columns.iter().enumerate() {
                    if column_index % 128 == 0 {
                        check_export_cancelled(cancellation)?;
                    }
                    if column_index > 0 {
                        write_limited(output, b",")?;
                    }
                    let result =
                        serde_json::to_writer(&mut *output, row.cell(*column).unwrap_or(""));
                    if result.is_err() {
                        return Err(output_error(output));
                    }
                }
                write_limited(output, b"]")?;
            }
            TableResolvedJsonShape::Objects => {
                let headers = document
                    .headers
                    .as_ref()
                    .ok_or(TableProjectionError::InvalidObjectHeaders)?;
                write_limited(output, b"{")?;
                for (column_index, column) in selection.columns.iter().enumerate() {
                    if column_index % 128 == 0 {
                        check_export_cancelled(cancellation)?;
                    }
                    if column_index > 0 {
                        write_limited(output, b",")?;
                    }
                    let header = headers
                        .get(*column)
                        .and_then(Option::as_deref)
                        .ok_or(TableProjectionError::InvalidObjectHeaders)?;
                    if serde_json::to_writer(&mut *output, header).is_err() {
                        return Err(output_error(output));
                    }
                    write_limited(output, b":")?;
                    if serde_json::to_writer(&mut *output, row.cell(*column).unwrap_or("")).is_err()
                    {
                        return Err(output_error(output));
                    }
                }
                write_limited(output, b"}")?;
            }
        }
    }
    write_limited(output, b"]")
}

fn write_limited(output: &mut BoundedOutput, bytes: &[u8]) -> Result<(), TableExportError> {
    if output.write_all(bytes).is_err() {
        return Err(output_error(output));
    }
    Ok(())
}

fn finite_number(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn compare_row_ids(
    left: TableRowId,
    right: TableRowId,
    document: &TableDocument,
    sort: TableSort,
    effective: EffectiveSortMode,
) -> Ordering {
    let left_value = document.rows[left.source_index]
        .cells
        .get(sort.column)
        .map(String::as_str)
        .unwrap_or("");
    let right_value = document.rows[right.source_index]
        .cells
        .get(sort.column)
        .map(String::as_str)
        .unwrap_or("");
    let ordering = match effective {
        EffectiveSortMode::Numeric => {
            match (finite_number(left_value), finite_number(right_value)) {
                (Some(left), Some(right)) => left.total_cmp(&right),
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        EffectiveSortMode::Lexical => left_value
            .chars()
            .flat_map(char::to_lowercase)
            .cmp(right_value.chars().flat_map(char::to_lowercase)),
    };
    let ordering = match sort.direction {
        TableSortDirection::Ascending => ordering,
        TableSortDirection::Descending => ordering.reverse(),
    };
    // Tie order is source order in both sort directions.
    ordering.then_with(|| left.source_index.cmp(&right.source_index))
}

fn sort_visible_rows(
    rows: &mut Vec<TableRowId>,
    document: &TableDocument,
    sort: TableSort,
    effective: EffectiveSortMode,
    cancellation: Option<&TableCancellationToken>,
) -> Result<(), TableProjectionError> {
    check_projection_cancelled(cancellation)?;
    if rows.len() < 2 {
        return Ok(());
    }

    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(rows.len())
        .map_err(|_| TableProjectionError::AllocationFailed)?;
    let mut width = 1usize;
    let mut comparisons = 0usize;
    while width < rows.len() {
        scratch.clear();
        let mut run_start = 0usize;
        while run_start < rows.len() {
            let middle = run_start.saturating_add(width).min(rows.len());
            let end = middle.saturating_add(width).min(rows.len());
            let mut left = run_start;
            let mut right = middle;
            while left < middle && right < end {
                comparisons += 1;
                if comparisons % 256 == 0 {
                    check_projection_cancelled(cancellation)?;
                }
                if compare_row_ids(rows[left], rows[right], document, sort, effective)
                    != Ordering::Greater
                {
                    scratch.push(rows[left]);
                    left += 1;
                } else {
                    scratch.push(rows[right]);
                    right += 1;
                }
            }
            scratch.extend_from_slice(&rows[left..middle]);
            scratch.extend_from_slice(&rows[right..end]);
            run_start = end;
        }
        std::mem::swap(rows, &mut scratch);
        check_projection_cancelled(cancellation)?;
        width = width.saturating_mul(2);
    }
    check_projection_cancelled(cancellation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explicit_options(delimiter: TableDelimiter, has_header: bool) -> TableParseOptions {
        TableParseOptions {
            delimiter: DelimiterChoice::Explicit(delimiter),
            has_header,
            ..TableParseOptions::default()
        }
    }

    fn parse_comma(input: &str, has_header: bool) -> TableDocument {
        TableDocument::parse(input, explicit_options(TableDelimiter::Comma, has_header)).unwrap()
    }

    fn select_all(
        document: &TableDocument,
        projection: &TableProjection,
        columns: &[usize],
    ) -> TableSelection {
        let rows = projection.visible_rows().to_vec();
        projection.select(document, &rows, columns).unwrap()
    }

    #[test]
    fn table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings() {
        let document = TableDocument::parse(
            "name,note\r\nA,\"say \"\"hi, there\"\"\nand continue\"\r\nB,tab\tinside\r\n",
            explicit_options(TableDelimiter::Comma, true),
        )
        .unwrap();
        assert_eq!(document.rows().len(), 2);
        assert_eq!(document.rows()[0].cell(0), Some("A"));
        assert_eq!(
            document.rows()[0].cell(1),
            Some("say \"hi, there\"\nand continue")
        );
        assert_eq!(document.rows()[1].cell(1), Some("tab\tinside"));
    }

    #[test]
    fn table_parse_tsv_quoted_tabs_and_lf_are_decoded_by_the_same_engine() {
        let mut options = explicit_options(TableDelimiter::Tab, true);
        options.row_width = RowWidthPolicy::Strict;
        let document = TableDocument::parse("left\tright\n1\t\"two\tparts\"", options).unwrap();
        assert_eq!(document.delimiter(), TableDelimiter::Tab);
        assert_eq!(document.rows()[0].cells(), &["1", "two\tparts"]);
        assert!(document.diagnostics().is_empty());
    }

    #[test]
    fn table_parse_auto_is_quote_aware_and_uses_comma_for_ties() {
        let tsv = TableDocument::parse("a\tb\nc\t\"d\te\"", TableParseOptions::default()).unwrap();
        assert_eq!(tsv.delimiter(), TableDelimiter::Tab);
        assert_eq!(tsv.rows()[1].cells(), &["c", "d\te"]);

        let tied = TableDocument::parse("a,b\tc\nx,y\tz", TableParseOptions::default()).unwrap();
        assert_eq!(tied.delimiter(), TableDelimiter::Comma);

        let malformed =
            TableDocument::parse("name,value\nx,\"unterminated", TableParseOptions::default())
                .unwrap_err();
        assert_eq!(malformed.kind, TableParseErrorKind::MalformedDelimitedText);
    }

    #[test]
    fn table_parse_bom_unicode_and_whitespace_are_exact() {
        let bytes = [UTF8_BOM, "名前, 値\n雪, café ".as_bytes()].concat();
        let document =
            TableDocument::parse_bytes(&bytes, explicit_options(TableDelimiter::Comma, true))
                .unwrap();
        assert_eq!(document.headers().unwrap()[0].as_deref(), Some("名前"));
        assert_eq!(document.rows()[0].cell(0), Some("雪"));
        assert_eq!(document.rows()[0].cell(1), Some(" café "));
    }

    #[test]
    fn table_parse_decoder_removes_only_the_file_bom() {
        let double_bom = [UTF8_BOM, UTF8_BOM, "value".as_bytes()].concat();
        let document =
            TableDocument::parse_bytes(&double_bom, explicit_options(TableDelimiter::Comma, false))
                .unwrap();
        assert_eq!(document.rows()[0].cell(0), Some("\u{FEFF}value"));

        let quoted_leading_bom = [UTF8_BOM, "\"\u{FEFF}quoted\"".as_bytes()].concat();
        let document = TableDocument::parse_bytes(
            &quoted_leading_bom,
            explicit_options(TableDelimiter::Comma, false),
        )
        .unwrap();
        assert_eq!(document.rows()[0].cell(0), Some("\u{FEFF}quoted"));
    }

    #[test]
    fn table_parse_malformed_quotes_report_unicode_location_without_source_text() {
        let error = TableDocument::parse(
            "a,b\né,\"ok\"x",
            explicit_options(TableDelimiter::Comma, false),
        )
        .unwrap_err();
        assert_eq!(error.kind, TableParseErrorKind::MalformedDelimitedText);
        let location = error.location.unwrap();
        assert_eq!(location.line, 2);
        assert_eq!(location.column, 7);
        assert_eq!(location.record, 2);
        assert_eq!(location.field, 2);
        assert_eq!(location.byte_offset, 11);
        assert!(!error.to_string().contains("ok"));

        let unclosed = TableDocument::parse(
            "a,\"unterminated",
            explicit_options(TableDelimiter::Comma, false),
        )
        .unwrap_err();
        assert_eq!(unclosed.kind, TableParseErrorKind::MalformedDelimitedText);
        assert_eq!(unclosed.location.unwrap().record, 1);

        let quote_in_unquoted =
            TableDocument::parse("a\nb\"c", explicit_options(TableDelimiter::Comma, false))
                .unwrap_err();
        assert_eq!(
            quote_in_unquoted.kind,
            TableParseErrorKind::MalformedDelimitedText
        );

        let bare_cr = TableDocument::parse("a\rb", explicit_options(TableDelimiter::Comma, false))
            .unwrap_err();
        assert_eq!(bare_cr.kind, TableParseErrorKind::MalformedDelimitedText);
    }

    #[test]
    fn table_parse_lenient_width_widens_and_pads_without_losing_headers() {
        let input = "h,,h\nx\n1,2,3,extra\n";
        let document =
            TableDocument::parse(input, explicit_options(TableDelimiter::Comma, true)).unwrap();
        assert_eq!(document.column_count(), 4);
        assert_eq!(
            document.headers().unwrap(),
            &[
                Some("h".to_owned()),
                Some(String::new()),
                Some("h".to_owned()),
                None
            ]
        );
        assert_eq!(document.rows()[0].cells(), &["x", "", "", ""]);
        assert_eq!(document.rows()[1].cells(), &["1", "2", "3", "extra"]);
        assert!(
            document
                .diagnostics()
                .contains(&TableWarning::ShortRowPadded {
                    record: 2,
                    actual_columns: 1,
                    padded_columns: 3,
                })
        );
        assert!(
            document
                .diagnostics()
                .contains(&TableWarning::SchemaWidened {
                    record: 3,
                    previous_columns: 3,
                    new_columns: 4,
                })
        );
        assert!(
            document
                .diagnostics()
                .contains(&TableWarning::HeaderLabelsMissing {
                    first_column: 4,
                    count: 1,
                })
        );
        assert!(
            document
                .diagnostics()
                .contains(&TableWarning::EmptyHeader { column: 2 })
        );
        assert!(
            document
                .diagnostics()
                .contains(&TableWarning::DuplicateHeader {
                    column: 3,
                    first_column: 1,
                })
        );

        let route_profile = TableParseOptions::strict_csv_route(TableDelimiter::Comma, true);
        assert_eq!(route_profile.row_width, RowWidthPolicy::Strict);
        assert_eq!(
            route_profile.delimiter,
            DelimiterChoice::Explicit(TableDelimiter::Comma)
        );
        assert_eq!(
            route_profile.limits.max_input_bytes,
            STRICT_CSV_MAX_INPUT_BYTES
        );
        assert_eq!(route_profile.limits.max_data_rows, STRICT_CSV_MAX_DATA_ROWS);
        let error = TableDocument::parse(input, route_profile).unwrap_err();
        assert_eq!(error.kind, TableParseErrorKind::RowWidthMismatch);
        assert_eq!(error.expected_columns, Some(3));
        assert_eq!(error.actual_columns, Some(1));
        assert_eq!(error.location.unwrap().record, 2);
    }

    #[test]
    fn table_parse_empty_and_unsupported_encodings_are_explicit() {
        let mut options = TableParseOptions::default();
        options.has_header = true;
        let empty = TableDocument::parse("", options).unwrap();
        assert_eq!(empty.column_count(), 0);
        assert!(empty.rows().is_empty());
        assert!(empty.headers().is_none());

        let utf16 = TableDocument::parse_bytes(&[0xFF, 0xFE, b'a', 0], options).unwrap_err();
        assert_eq!(utf16.kind, TableParseErrorKind::UnsupportedEncoding);
        let invalid_utf8 = TableDocument::parse_bytes(&[b'a', 0xFF], options).unwrap_err();
        assert_eq!(invalid_utf8.kind, TableParseErrorKind::InvalidUtf8);
        let binary = TableDocument::parse_bytes(b"a\0b", options).unwrap_err();
        assert_eq!(binary.kind, TableParseErrorKind::BinaryInput);
        assert_eq!(binary.location.unwrap().byte_offset, 1);
    }

    #[test]
    fn table_parse_ignores_empty_physical_lines_like_the_shared_decoder() {
        let document = TableDocument::parse(
            "\n\r\nvalue\n\n",
            explicit_options(TableDelimiter::Comma, false),
        )
        .unwrap();
        assert_eq!(document.rows().len(), 1);
        assert_eq!(document.rows()[0].cells(), &["value"]);
    }

    #[test]
    fn table_parse_cancellation_is_reported_without_source_details() {
        let cancellation = TableCancellationToken::new();
        cancellation.cancel();
        let error = TableDocument::parse_with_cancellation(
            "private,cells\nsecret,value",
            explicit_options(TableDelimiter::Comma, true),
            &cancellation,
        )
        .unwrap_err();
        assert_eq!(error.kind, TableParseErrorKind::Cancelled);
        assert!(!error.to_string().contains("secret"));
    }

    #[test]
    fn table_parse_scanner_checkpoint_survives_escaped_quote_stride() {
        let cancellation = TableCancellationToken::new();
        let mut next_checkpoint = 0usize;
        scanner_cancellation_checkpoint(0, &mut next_checkpoint, Some(&cancellation)).unwrap();
        cancellation.cancel();

        let mut observed_offset = None;
        for byte_offset in (1..=SCANNER_CANCELLATION_INTERVAL + 3).step_by(2) {
            if let Err(error) = scanner_cancellation_checkpoint(
                byte_offset,
                &mut next_checkpoint,
                Some(&cancellation),
            ) {
                assert_eq!(error.kind, TableParseErrorKind::Cancelled);
                observed_offset = Some(byte_offset);
                break;
            }
        }
        assert_eq!(observed_offset, Some(SCANNER_CANCELLATION_INTERVAL + 1));
    }

    #[test]
    fn table_parse_limits_reject_input_rows_columns_cells_and_materialization() {
        let mut cases = Vec::new();
        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_input_bytes = 4;
        cases.push(("a,b,c", options, TableParseErrorKind::InputTooLarge));

        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_data_rows = 1;
        cases.push(("a\nb", options, TableParseErrorKind::TooManyRows));

        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_columns = 1;
        cases.push(("a,b", options, TableParseErrorKind::TooManyColumns));

        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_cells = 3;
        cases.push(("a,b\n1,2", options, TableParseErrorKind::TooManyCells));

        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_materialized_bytes = 1;
        cases.push(("a", options, TableParseErrorKind::MaterializationLimit));

        for (input, options, expected) in cases {
            let error = TableDocument::parse(input, options).unwrap_err();
            assert_eq!(error.kind, expected, "unexpected bound for {input:?}");
        }
    }

    #[test]
    fn table_projection_numeric_and_lexical_sort_keep_source_ties_in_both_directions() {
        let document = parse_comma("name,n\nfirst,2\nsecond,10\nthird,2\nempty,\n", true);
        let ascending = TableProjection::build(
            &document,
            &TableProjectionOptions {
                sort: Some(TableSort {
                    column: 1,
                    direction: TableSortDirection::Ascending,
                    mode: TableSortMode::AutoNumeric,
                }),
                ..TableProjectionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            ascending.effective_sort_mode(),
            Some(EffectiveSortMode::Numeric)
        );
        let source_indices: Vec<_> = ascending
            .visible_rows()
            .iter()
            .map(|row| row.source_index())
            .collect();
        assert_eq!(source_indices, [3, 0, 2, 1]);

        let descending = TableProjection::build(
            &document,
            &TableProjectionOptions {
                sort: Some(TableSort {
                    column: 1,
                    direction: TableSortDirection::Descending,
                    mode: TableSortMode::AutoNumeric,
                }),
                ..TableProjectionOptions::default()
            },
        )
        .unwrap();
        let source_indices: Vec<_> = descending
            .visible_rows()
            .iter()
            .map(|row| row.source_index())
            .collect();
        assert_eq!(source_indices, [1, 0, 2, 3]);

        let lexical = parse_comma("n\n10\n2\nword", true);
        let lexical_view = TableProjection::build(
            &lexical,
            &TableProjectionOptions {
                sort: Some(TableSort {
                    column: 0,
                    direction: TableSortDirection::Ascending,
                    mode: TableSortMode::AutoNumeric,
                }),
                ..TableProjectionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(
            lexical_view.effective_sort_mode(),
            Some(EffectiveSortMode::Lexical)
        );
        let values: Vec<_> = lexical_view
            .visible_rows()
            .iter()
            .map(|row| lexical.row(*row).unwrap().cell(0).unwrap())
            .collect();
        assert_eq!(values, ["10", "2", "word"]);
    }

    #[test]
    fn table_projection_filter_selection_and_stale_identities_are_source_bound() {
        let document = parse_comma("name,group\nAlpha,One\nBeta,Two\nGamma,one\n", true);
        let view = TableProjection::build(
            &document,
            &TableProjectionOptions {
                filter: "ONE".to_owned(),
                filter_columns: vec![1],
                sort: Some(TableSort {
                    column: 0,
                    direction: TableSortDirection::Descending,
                    mode: TableSortMode::Lexical,
                }),
            },
        )
        .unwrap();
        assert_eq!(view.visible_rows().len(), 2);
        let alpha = document.rows()[0].id();
        let gamma = document.rows()[2].id();
        assert_eq!(view.visible_rows(), &[gamma, alpha]);
        let selection = view.select(&document, &[alpha, gamma], &[1, 0]).unwrap();
        assert_eq!(selection.rows(), &[gamma, alpha]);
        assert_eq!(selection.columns(), &[0, 1]);

        let hidden = document.rows()[1].id();
        assert_eq!(
            view.select(&document, &[hidden], &[0]),
            Err(TableProjectionError::HiddenRow)
        );
        assert_eq!(
            view.select(&document, &[alpha], &[]),
            Err(TableProjectionError::NoColumns)
        );
        assert!(view.select(&document, &[alpha], &[9]).is_err());

        let replacement = parse_comma("name,group\nAlpha,One\nBeta,Two\nGamma,one\n", true);
        assert_ne!(document.id(), replacement.id());
        assert_eq!(
            view.select(&replacement, &[replacement.rows()[0].id()], &[0]),
            Err(TableProjectionError::WrongDocument)
        );
        let other_view =
            TableProjection::build(&document, &TableProjectionOptions::default()).unwrap();
        assert_eq!(
            other_view.export_tsv(&document, &selection, false),
            Err(TableExportError::Projection(
                TableProjectionError::WrongView
            ))
        );
    }

    #[test]
    fn table_projection_bounds_filter_and_selection_metadata() {
        let document = parse_comma("a,b\n1,2\n3,4", true);
        let too_many_filter_columns = TableProjectionOptions {
            filter_columns: vec![0, 1, 0],
            ..TableProjectionOptions::default()
        };
        assert_eq!(
            TableProjection::build(&document, &too_many_filter_columns),
            Err(TableProjectionError::TooManyFilterColumns)
        );

        let projection =
            TableProjection::build(&document, &TableProjectionOptions::default()).unwrap();
        assert_eq!(
            projection.select(&document, &projection.visible_rows()[..1], &[0, 0, 0]),
            Err(TableProjectionError::TooManySelectedColumns)
        );
        assert_eq!(
            projection.select(
                &document,
                &[
                    projection.visible_rows()[0],
                    projection.visible_rows()[1],
                    projection.visible_rows()[0],
                ],
                &[0],
            ),
            Err(TableProjectionError::TooManySelectedRows)
        );

        let mut parse_options = explicit_options(TableDelimiter::Comma, false);
        parse_options.limits.max_input_bytes = 4;
        let small_document = TableDocument::parse("a,b", parse_options).unwrap();
        let oversized_filter = TableProjectionOptions {
            filter: "q".repeat(TABLE_FILTER_MAX_BYTES + 1),
            ..TableProjectionOptions::default()
        };
        assert_eq!(
            TableProjection::build(&small_document, &oversized_filter),
            Err(TableProjectionError::FilterTooLarge)
        );
    }

    #[test]
    fn table_projection_cancellation_covers_build_selection_and_exports() {
        let document = parse_comma("name,value\na,1\nb,2", true);
        let cancelled = TableCancellationToken::new();
        cancelled.cancel();
        assert_eq!(
            TableProjection::build_with_cancellation(
                &document,
                &TableProjectionOptions::default(),
                &cancelled,
            ),
            Err(TableProjectionError::Cancelled)
        );

        let projection =
            TableProjection::build(&document, &TableProjectionOptions::default()).unwrap();
        assert_eq!(
            projection.select_with_cancellation(
                &document,
                &projection.visible_rows()[..1],
                &[0],
                &cancelled,
            ),
            Err(TableProjectionError::Cancelled)
        );

        let selection = select_all(&document, &projection, &[0, 1]);
        assert_eq!(
            projection.export_tsv_with_cancellation(&document, &selection, true, &cancelled),
            Err(TableExportError::Cancelled)
        );
        assert_eq!(
            projection.export_json_with_cancellation(
                &document,
                &selection,
                TableJsonShape::Auto,
                &cancelled,
            ),
            Err(TableExportError::Cancelled)
        );

        let output_token = TableCancellationToken::new();
        let mut output = BoundedOutput::new(64, Some(output_token.flag()));
        output_token.cancel();
        assert!(std::io::Write::write_all(&mut output, b"payload").is_err());
        assert_eq!(output_error(&output), TableExportError::Cancelled);
    }

    #[test]
    fn table_projection_tsv_quotes_round_trip_and_keeps_selected_column_order() {
        let document = TableDocument::parse(
            "name,note,rank\nA,\"x\ty\",2\nB,normal,1\nC,\"line\nnext\",3\n",
            explicit_options(TableDelimiter::Comma, true),
        )
        .unwrap();
        let view = TableProjection::build(
            &document,
            &TableProjectionOptions {
                sort: Some(TableSort {
                    column: 2,
                    direction: TableSortDirection::Descending,
                    mode: TableSortMode::AutoNumeric,
                }),
                ..TableProjectionOptions::default()
            },
        )
        .unwrap();
        let row_a = document.rows()[0].id();
        let row_c = document.rows()[2].id();
        let selection = view.select(&document, &[row_a, row_c], &[2, 1]).unwrap();
        assert_eq!(selection.rows(), &[row_c, row_a]);
        assert_eq!(selection.columns(), &[1, 2]);
        let tsv = view.export_tsv(&document, &selection, true).unwrap();
        assert_eq!(tsv, "note\trank\n\"line\nnext\"\t3\n\"x\ty\"\t2\n");

        let round_trip =
            TableDocument::parse(&tsv, explicit_options(TableDelimiter::Tab, true)).unwrap();
        assert_eq!(round_trip.headers().unwrap()[0].as_deref(), Some("note"));
        assert_eq!(round_trip.rows()[0].cells(), &["line\nnext", "3"]);
        assert_eq!(round_trip.rows()[1].cells(), &["x\ty", "2"]);
    }

    #[test]
    fn table_projection_json_shapes_preserve_strings_and_report_header_fallbacks() {
        let document = parse_comma("id,name\n001,\" café \"", true);
        let view = TableProjection::build(&document, &TableProjectionOptions::default()).unwrap();
        let selection = select_all(&document, &view, &[0, 1]);
        let object = view
            .export_json(&document, &selection, TableJsonShape::Auto)
            .unwrap();
        assert_eq!(object.shape, TableResolvedJsonShape::Objects);
        assert_eq!(object.fallback_reason, None);
        let value: serde_json::Value = serde_json::from_str(&object.text).unwrap();
        assert_eq!(value[0]["id"], "001");
        assert_eq!(value[0]["name"], " café ");

        let duplicate = parse_comma("x,x\nleft,right", true);
        let view = TableProjection::build(&duplicate, &TableProjectionOptions::default()).unwrap();
        let selection = select_all(&duplicate, &view, &[0, 1]);
        let auto = view
            .export_json(&duplicate, &selection, TableJsonShape::Auto)
            .unwrap();
        assert_eq!(auto.shape, TableResolvedJsonShape::Arrays);
        assert_eq!(
            auto.fallback_reason,
            Some(JsonShapeFallbackReason::DuplicateHeader)
        );
        assert_eq!(auto.text, "[[\"left\",\"right\"]]");
        assert_eq!(
            view.export_json(&duplicate, &selection, TableJsonShape::Objects),
            Err(TableExportError::Projection(
                TableProjectionError::InvalidObjectHeaders
            ))
        );

        let blank = parse_comma(",value\nleft,right", true);
        let view = TableProjection::build(&blank, &TableProjectionOptions::default()).unwrap();
        let selection = select_all(&blank, &view, &[0, 1]);
        assert_eq!(
            view.export_json(&blank, &selection, TableJsonShape::Auto)
                .unwrap()
                .fallback_reason,
            Some(JsonShapeFallbackReason::EmptyHeader)
        );

        let no_header = parse_comma("001,value", false);
        let view = TableProjection::build(&no_header, &TableProjectionOptions::default()).unwrap();
        let selection = select_all(&no_header, &view, &[0, 1]);
        assert_eq!(
            view.export_json(&no_header, &selection, TableJsonShape::Auto)
                .unwrap()
                .fallback_reason,
            Some(JsonShapeFallbackReason::HeadersDisabled)
        );
    }

    #[test]
    fn table_projection_output_limit_is_enforced_while_writing() {
        let mut options = explicit_options(TableDelimiter::Comma, false);
        options.limits.max_output_bytes = 8;
        let document = TableDocument::parse("123456789", options).unwrap();
        let view = TableProjection::build(&document, &TableProjectionOptions::default()).unwrap();
        let selection = select_all(&document, &view, &[0]);
        assert_eq!(
            view.export_tsv(&document, &selection, false),
            Err(TableExportError::OutputTooLarge)
        );
        assert_eq!(
            view.export_json(&document, &selection, TableJsonShape::Arrays),
            Err(TableExportError::OutputTooLarge)
        );
    }

    #[test]
    fn table_projection_preview_is_bounded_without_changing_exported_value() {
        let document = parse_comma("value\n雪と café", true);
        let row = document.rows()[0].id();
        assert_eq!(
            document.bounded_preview(row, 0).as_deref(),
            Some("雪と café")
        );
        assert_eq!(bounded_cell_preview("abcdef", 3), "abc…");
        assert_eq!(document.rows()[0].cell(0), Some("雪と café"));
    }
}
