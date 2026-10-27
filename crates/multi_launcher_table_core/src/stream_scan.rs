use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read};

use super::{
    DelimiterEvidence, LexState, TableCancellationToken, TableDelimiter, TableLocation,
    TableParseError, TableParseErrorKind, check_parse_cancelled, location_error,
};

#[derive(Clone, Copy)]
pub(crate) struct StreamLimits {
    pub max_columns: usize,
    /// Widths above this limit share one bounded delimiter-evidence bucket.
    pub evidence_width_limit: usize,
    pub max_records: Option<u64>,
    pub max_record_bytes: Option<u64>,
    pub max_field_bytes: Option<u64>,
    pub has_header: bool,
    pub strict_width: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct StreamRecord {
    pub width: usize,
    pub start: TableLocation,
    pub end_offset: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StreamSummary {
    pub record_count: u64,
    pub end_offset: u64,
    pub data_rows: u64,
    pub schema_width: usize,
    pub score: DelimiterEvidence,
    pub stopped_early: bool,
}

pub(crate) fn scan_reader<R, F, E>(
    input: R,
    base_offset: u64,
    delimiter: TableDelimiter,
    limits: StreamLimits,
    cancellation: Option<&TableCancellationToken>,
    capture_limit: Option<usize>,
    mut on_record: F,
) -> Result<StreamSummary, E>
where
    R: Read,
    F: FnMut(StreamRecord, &[u8]) -> Result<bool, E>,
    E: From<TableParseError>,
{
    let mut reader = BufReader::with_capacity(64 * 1024, input);
    let mut offset = base_offset;
    let mut line = 1usize;
    let mut column = 1usize;
    let mut record_start = base_offset;
    let mut record_start_line = line;
    let mut record_number = 1u64;
    let mut field_number = 1usize;
    let mut state = LexState::FieldStart;
    let mut touched = false;
    let mut encoded_record_bytes = 0u64;
    let mut decoded_record_bytes = 0u64;
    let mut decoded_field_bytes = 0u64;
    let mut next_cancel = 0u64;
    let mut record_buffer = Vec::new();
    if let Some(capacity) = capture_limit {
        record_buffer
            .try_reserve_exact(capacity)
            .map_err(|_| allocation_error())?;
    }
    if base_offset == 0 {
        let prefix = reader
            .fill_buf()
            .map_err(|_| TableParseError::new(TableParseErrorKind::IoError, None))?;
        if prefix.starts_with(&[0xFF, 0xFE]) || prefix.starts_with(&[0xFE, 0xFF]) {
            return Err(
                TableParseError::new(TableParseErrorKind::UnsupportedEncoding, None).into(),
            );
        }
        if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
            for _ in 0..3 {
                consume_byte(
                    &mut reader,
                    &mut offset,
                    &mut encoded_record_bytes,
                    &mut record_buffer,
                    capture_limit,
                )?;
            }
        }
    }
    let mut width_counts = BTreeMap::<usize, u64>::new();
    let mut summary = StreamSummary::default();
    let mut expected_width = None;

    loop {
        if offset >= next_cancel {
            check_parse_cancelled(cancellation)?;
            next_cancel = offset.saturating_add(4096);
        }
        let Some(byte) = peek_byte(&mut reader)? else {
            break;
        };
        let location = TableLocation {
            byte_offset: usize::try_from(offset).map_err(|_| identity_error())?,
            line,
            column,
            record: usize::try_from(record_number).map_err(|_| identity_error())?,
            field: field_number,
        };

        if byte == 0 {
            return Err(location_error(
                TableParseErrorKind::BinaryInput,
                usize::try_from(base_offset).map_err(|_| identity_error())?,
                usize::try_from(offset.saturating_sub(base_offset))
                    .map_err(|_| identity_error())?,
                line,
                column,
                usize::try_from(record_number).map_err(|_| identity_error())?,
                field_number,
            )
            .into());
        }

        let mut record_end = false;
        let mut blank_line = false;
        match state {
            LexState::FieldStart => {
                if byte == delimiter.byte() {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    touched = true;
                    field_number = next_field(field_number, limits.max_columns, location)?;
                    column = column.saturating_add(1);
                } else if byte == b'"' {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    touched = true;
                    state = LexState::Quoted;
                    column = column.saturating_add(1);
                } else if byte == b'\n' || byte == b'\r' {
                    consume_line_ending(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                        byte,
                        base_offset,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )?;
                    line = line.saturating_add(1);
                    column = 1;
                    record_end = true;
                    blank_line = !touched && field_number == 1;
                } else {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    touched = true;
                    state = LexState::Unquoted;
                    add_decoded_byte(
                        &mut decoded_field_bytes,
                        &mut decoded_record_bytes,
                        limits,
                        location,
                    )?;
                    column = advance_column(byte, column);
                }
            }
            LexState::Unquoted => {
                if byte == delimiter.byte() {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    field_number = next_field(field_number, limits.max_columns, location)?;
                    decoded_field_bytes = 0;
                    state = LexState::FieldStart;
                    column = column.saturating_add(1);
                } else if byte == b'"' {
                    return Err(location_error(
                        TableParseErrorKind::MalformedDelimitedText,
                        usize::try_from(base_offset).map_err(|_| identity_error())?,
                        usize::try_from(offset.saturating_sub(base_offset))
                            .map_err(|_| identity_error())?,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )
                    .into());
                } else if byte == b'\n' || byte == b'\r' {
                    consume_line_ending(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                        byte,
                        base_offset,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )?;
                    line = line.saturating_add(1);
                    column = 1;
                    record_end = true;
                } else {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    add_decoded_byte(
                        &mut decoded_field_bytes,
                        &mut decoded_record_bytes,
                        limits,
                        location,
                    )?;
                    column = advance_column(byte, column);
                }
            }
            LexState::Quoted => {
                if byte == b'"' {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    column = column.saturating_add(1);
                    if peek_byte(&mut reader)? == Some(b'"') {
                        consume_byte(
                            &mut reader,
                            &mut offset,
                            &mut encoded_record_bytes,
                            &mut record_buffer,
                            capture_limit,
                        )?;
                        add_decoded_byte(
                            &mut decoded_field_bytes,
                            &mut decoded_record_bytes,
                            limits,
                            location,
                        )?;
                        column = column.saturating_add(1);
                    } else {
                        state = LexState::AfterQuote;
                    }
                } else if byte == b'\n' || byte == b'\r' {
                    consume_line_ending(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                        byte,
                        base_offset,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )?;
                    decoded_field_bytes =
                        decoded_field_bytes.saturating_add(if byte == b'\r' { 2 } else { 1 });
                    decoded_record_bytes =
                        decoded_record_bytes.saturating_add(if byte == b'\r' { 2 } else { 1 });
                    check_field_and_record_limits(
                        decoded_field_bytes,
                        decoded_record_bytes,
                        limits,
                        location,
                    )?;
                    line = line.saturating_add(1);
                    column = 1;
                } else {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    add_decoded_byte(
                        &mut decoded_field_bytes,
                        &mut decoded_record_bytes,
                        limits,
                        location,
                    )?;
                    column = advance_column(byte, column);
                }
            }
            LexState::AfterQuote => {
                if byte == delimiter.byte() {
                    consume_byte(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                    )?;
                    field_number = next_field(field_number, limits.max_columns, location)?;
                    decoded_field_bytes = 0;
                    state = LexState::FieldStart;
                    column = column.saturating_add(1);
                } else if byte == b'\n' || byte == b'\r' {
                    consume_line_ending(
                        &mut reader,
                        &mut offset,
                        &mut encoded_record_bytes,
                        &mut record_buffer,
                        capture_limit,
                        byte,
                        base_offset,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )?;
                    line = line.saturating_add(1);
                    column = 1;
                    record_end = true;
                } else {
                    return Err(location_error(
                        TableParseErrorKind::MalformedDelimitedText,
                        usize::try_from(base_offset).map_err(|_| identity_error())?,
                        usize::try_from(offset.saturating_sub(base_offset))
                            .map_err(|_| identity_error())?,
                        line,
                        column,
                        usize::try_from(record_number).map_err(|_| identity_error())?,
                        field_number,
                    )
                    .into());
                }
            }
        }

        if encoded_record_bytes > limits.max_record_bytes.unwrap_or(u64::MAX) {
            return Err(location_error(
                TableParseErrorKind::InputTooLarge,
                usize::try_from(base_offset).map_err(|_| identity_error())?,
                usize::try_from(record_start.saturating_sub(base_offset))
                    .map_err(|_| identity_error())?,
                record_start_line,
                1,
                usize::try_from(record_number).map_err(|_| identity_error())?,
                1,
            )
            .into());
        }

        if record_end {
            if !blank_line {
                let record = StreamRecord {
                    width: field_number,
                    start: TableLocation {
                        byte_offset: usize::try_from(record_start).map_err(|_| identity_error())?,
                        line: record_start_line,
                        column: 1,
                        record: usize::try_from(record_number).map_err(|_| identity_error())?,
                        field: 1,
                    },
                    end_offset: offset,
                };
                summary = account_record(
                    summary,
                    record,
                    limits,
                    &mut expected_width,
                    &mut width_counts,
                )?;
                let raw = if capture_limit.is_some() {
                    &record_buffer[..]
                } else {
                    &[]
                };
                if !on_record(record, raw)? {
                    summary.stopped_early = true;
                    summary.end_offset = offset;
                    return finish_summary(summary, &width_counts).map_err(Into::into);
                }
                record_number = record_number.checked_add(1).ok_or_else(identity_error)?;
            }
            record_start = offset;
            record_start_line = line;
            field_number = 1;
            state = LexState::FieldStart;
            touched = false;
            encoded_record_bytes = 0;
            decoded_record_bytes = 0;
            decoded_field_bytes = 0;
            record_buffer.clear();
        }
    }

    if state == LexState::Quoted {
        return Err(location_error(
            TableParseErrorKind::MalformedDelimitedText,
            usize::try_from(base_offset).map_err(|_| identity_error())?,
            usize::try_from(offset.saturating_sub(base_offset)).map_err(|_| identity_error())?,
            line,
            column,
            usize::try_from(record_number).map_err(|_| identity_error())?,
            field_number,
        )
        .into());
    }
    if touched || field_number > 1 || state != LexState::FieldStart {
        let record = StreamRecord {
            width: field_number,
            start: TableLocation {
                byte_offset: usize::try_from(record_start).map_err(|_| identity_error())?,
                line: record_start_line,
                column: 1,
                record: usize::try_from(record_number).map_err(|_| identity_error())?,
                field: 1,
            },
            end_offset: offset,
        };
        summary = account_record(
            summary,
            record,
            limits,
            &mut expected_width,
            &mut width_counts,
        )?;
        let raw = if capture_limit.is_some() {
            &record_buffer[..]
        } else {
            &[]
        };
        if !on_record(record, raw)? {
            summary.stopped_early = true;
            summary.end_offset = offset;
            return finish_summary(summary, &width_counts).map_err(Into::into);
        }
    }
    check_parse_cancelled(cancellation)?;
    summary.end_offset = offset;
    finish_summary(summary, &width_counts).map_err(Into::into)
}

fn finish_summary(
    mut summary: StreamSummary,
    width_counts: &BTreeMap<usize, u64>,
) -> Result<StreamSummary, TableParseError> {
    summary.score.modal_multicolumn_count =
        usize::try_from(width_counts.values().copied().max().unwrap_or(0))
            .map_err(|_| identity_error())?;
    Ok(summary)
}

fn account_record(
    mut summary: StreamSummary,
    record: StreamRecord,
    limits: StreamLimits,
    expected_width: &mut Option<usize>,
    width_counts: &mut BTreeMap<usize, u64>,
) -> Result<StreamSummary, TableParseError> {
    if let Some(maximum) = limits.max_records
        && summary.record_count
            >= maximum
                .checked_add(u64::from(limits.has_header))
                .ok_or_else(identity_error)?
    {
        return Err(TableParseError::new(
            TableParseErrorKind::TooManyRows,
            Some(record.start),
        ));
    }
    if record.width > limits.max_columns {
        return Err(TableParseError::new(
            TableParseErrorKind::TooManyColumns,
            Some(record.start),
        ));
    }
    if limits.strict_width {
        if limits.has_header && summary.record_count == 0 {
            *expected_width = Some(record.width);
        } else if !limits.has_header && summary.record_count == 0 {
            *expected_width = Some(record.width);
        } else if let Some(expected) = *expected_width
            && record.width != expected
        {
            return Err(TableParseError::width(record.start, expected, record.width));
        }
    }
    summary.record_count = summary
        .record_count
        .checked_add(1)
        .ok_or_else(identity_error)?;
    summary.schema_width = summary.schema_width.max(record.width);
    if record.width > 1 {
        summary.score.multicolumn_records = summary
            .score
            .multicolumn_records
            .checked_add(1)
            .ok_or_else(identity_error)?;
        let evidence_width = record
            .width
            .min(limits.evidence_width_limit.saturating_add(1));
        let count = width_counts.entry(evidence_width).or_default();
        *count = count.checked_add(1).ok_or_else(identity_error)?;
    }
    summary.score.widest_record = summary.score.widest_record.max(record.width);
    summary.data_rows = summary
        .record_count
        .saturating_sub(u64::from(limits.has_header));
    Ok(summary)
}

fn next_field(
    field: usize,
    max_columns: usize,
    location: TableLocation,
) -> Result<usize, TableParseError> {
    let next = field.checked_add(1).ok_or_else(identity_error)?;
    if next > max_columns {
        return Err(TableParseError::new(
            TableParseErrorKind::TooManyColumns,
            Some(location),
        ));
    }
    Ok(next)
}

fn add_decoded_byte(
    field_bytes: &mut u64,
    record_bytes: &mut u64,
    limits: StreamLimits,
    location: TableLocation,
) -> Result<(), TableParseError> {
    *field_bytes = field_bytes.checked_add(1).ok_or_else(identity_error)?;
    *record_bytes = record_bytes.checked_add(1).ok_or_else(identity_error)?;
    check_field_and_record_limits(*field_bytes, *record_bytes, limits, location)
}

fn check_field_and_record_limits(
    field_bytes: u64,
    record_bytes: u64,
    limits: StreamLimits,
    location: TableLocation,
) -> Result<(), TableParseError> {
    if field_bytes > limits.max_field_bytes.unwrap_or(u64::MAX)
        || record_bytes > limits.max_record_bytes.unwrap_or(u64::MAX)
    {
        return Err(TableParseError::new(
            TableParseErrorKind::InputTooLarge,
            Some(location),
        ));
    }
    Ok(())
}

fn consume_byte<R: BufRead>(
    reader: &mut R,
    offset: &mut u64,
    encoded: &mut u64,
    capture: &mut Vec<u8>,
    capture_limit: Option<usize>,
) -> Result<u8, TableParseError> {
    let byte = peek_byte(reader)?.ok_or_else(identity_error)?;
    if let Some(limit) = capture_limit
        && capture.len() >= limit
    {
        return Err(TableParseError::new(
            TableParseErrorKind::InputTooLarge,
            None,
        ));
    }
    reader.consume(1);
    *offset = offset.checked_add(1).ok_or_else(identity_error)?;
    *encoded = encoded.checked_add(1).ok_or_else(identity_error)?;
    if capture_limit.is_some() {
        capture.push(byte);
    }
    Ok(byte)
}

fn consume_line_ending<R: BufRead>(
    reader: &mut R,
    offset: &mut u64,
    encoded: &mut u64,
    capture: &mut Vec<u8>,
    capture_limit: Option<usize>,
    first: u8,
    base_offset: u64,
    line: usize,
    column: usize,
    record: usize,
    field: usize,
) -> Result<(), TableParseError> {
    let first_offset = *offset;
    consume_byte(reader, offset, encoded, capture, capture_limit)?;
    if first == b'\r' {
        if peek_byte(reader)? != Some(b'\n') {
            return Err(location_error(
                TableParseErrorKind::MalformedDelimitedText,
                usize::try_from(base_offset).map_err(|_| identity_error())?,
                usize::try_from(first_offset.saturating_sub(base_offset))
                    .map_err(|_| identity_error())?,
                line,
                column,
                record,
                field,
            ));
        }
        consume_byte(reader, offset, encoded, capture, capture_limit)?;
    }
    Ok(())
}
fn peek_byte<R: BufRead>(reader: &mut R) -> Result<Option<u8>, TableParseError> {
    reader
        .fill_buf()
        .map(|bytes| bytes.first().copied())
        .map_err(|_error| TableParseError::new(TableParseErrorKind::IoError, None))
}

fn advance_column(byte: u8, column: usize) -> usize {
    if byte & 0b1100_0000 != 0b1000_0000 {
        column.saturating_add(1)
    } else {
        column
    }
}

fn allocation_error() -> TableParseError {
    TableParseError::new(TableParseErrorKind::AllocationFailed, None)
}

fn identity_error() -> TableParseError {
    TableParseError::new(TableParseErrorKind::IdentityExhausted, None)
}
