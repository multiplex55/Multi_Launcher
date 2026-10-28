//! Bounded ZIP end-record and physical central-directory reader.
//!
//! This module intentionally does not construct `zip::read::ZipArchive` or a
//! streaming `ZipFile`: the former collapses duplicate names and allocates its
//! name map before application limits can be applied, while the latter reads
//! entry payload during Drop. Listing reads end records and central metadata
//! only. ZIP64 extensible data, entry extras and comments are length-validated
//! and skipped without retaining their contents.

use std::io::{Read, Seek, SeekFrom};
use std::mem::size_of;

use crate::file_operations::ScanCancellation;

use super::model::{
    ArchiveCatalog, ArchiveEntry, ArchiveEntryId, ArchiveEntryName, ArchiveFormat, ArchiveLimits,
    ArchiveRevision, ArchiveSource, CatalogError, CatalogLimit, EntryCompression, EntryKind,
    EntryProblems, NameEncodingProblem, UnsafeEntryReason, UnsafePathReason,
};

const EOCD_SIGNATURE: u32 = 0x0605_4b50;
const ZIP64_EOCD_SIGNATURE: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const EOCD_FIXED_BYTES: usize = 22;
const ZIP64_LOCATOR_BYTES: usize = 20;
const ZIP64_EOCD_FIXED_BYTES: usize = 56;
const CENTRAL_HEADER_BYTES: usize = 46;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const UTF8_NAME_FLAG: u16 = 1 << 11;
const ENCRYPTED_FLAGS: u16 = (1 << 0) | (1 << 6) | (1 << 13);
const DOS_DIRECTORY_ATTRIBUTE: u16 = 0x0010;
const DOS_REPARSE_ATTRIBUTE: u16 = 0x0400;
const MAX_ZIP64_EXTENSIBLE_DATA_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateInspection {
    NotAnEndRecord,
    Invalid(crate::archive::model::ArchiveFormatProblem),
    OverLimit(CatalogLimit),
    Valid(ArchiveSource),
}

#[derive(Clone, Copy, Debug)]
struct EndFields {
    disk_number: u16,
    central_disk: u16,
    entries_on_disk: u16,
    entry_count: u16,
    central_size: u32,
    central_offset: u32,
    comment_size: u16,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedEnd {
    central_offset: u64,
    central_size: u64,
    entry_count: u64,
    central_end: u64,
    format: ArchiveFormat,
}

#[derive(Clone, Copy, Debug, Default)]
struct Zip64EntryValues {
    uncompressed_size: Option<u64>,
    compressed_size: Option<u64>,
    local_header_offset: Option<u64>,
    disk_start: Option<u32>,
}

/// Read bounded ZIP metadata from a caller-owned, seekable, stable source.
///
/// `revision` is supplied by the owner and must identify the same immutable
/// bytes when a later operation uses the returned entry ids. This routine only
/// seeks/reads the footer and central directory; it never reads an entry local
/// header or compressed payload.
pub fn read_catalog<R: Read + Seek>(
    reader: &mut R,
    revision: ArchiveRevision,
    limits: ArchiveLimits,
    cancellation: &ScanCancellation,
) -> Result<ArchiveCatalog, CatalogError> {
    check_cancel(cancellation)?;
    let archive_len = reader
        .seek(SeekFrom::End(0))
        .map_err(|error| CatalogError::Io(error.kind()))?;
    if archive_len < EOCD_FIXED_BYTES as u64 {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::Truncated,
        ));
    }

    let source = find_end_record(reader, archive_len, revision, &limits, cancellation)?;
    let minimum_central_bytes = source
        .entry_count
        .checked_mul(CENTRAL_HEADER_BYTES as u64)
        .ok_or(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
        ))?;
    if minimum_central_bytes > source.central_directory_size {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
        ));
    }

    let entry_count = usize::try_from(source.entry_count)
        .map_err(|_| CatalogError::LimitExceeded(CatalogLimit::EntryCount))?;
    let minimum_entry_bytes =
        entry_count
            .checked_mul(size_of::<ArchiveEntry>())
            .ok_or(CatalogError::LimitExceeded(
                CatalogLimit::RetainedMetadataBytes,
            ))?;
    if minimum_entry_bytes > limits.max_retained_metadata_bytes {
        return Err(CatalogError::LimitExceeded(
            CatalogLimit::RetainedMetadataBytes,
        ));
    }

    let mut entries = Vec::new();
    entries
        .try_reserve_exact(entry_count)
        .map_err(|_| CatalogError::Allocation)?;
    let mut retained_metadata_bytes = entries
        .capacity()
        .checked_mul(size_of::<ArchiveEntry>())
        .ok_or(CatalogError::LimitExceeded(
            CatalogLimit::RetainedMetadataBytes,
        ))?;
    if retained_metadata_bytes > limits.max_retained_metadata_bytes {
        return Err(CatalogError::LimitExceeded(
            CatalogLimit::RetainedMetadataBytes,
        ));
    }

    reader
        .seek(SeekFrom::Start(source.central_directory_offset))
        .map_err(|error| CatalogError::Io(error.kind()))?;
    let mut cursor = source.central_directory_offset;
    let mut minimum_local_offset = None;

    for ordinal in 0..source.entry_count {
        check_cancel(cancellation)?;
        let record_offset = cursor;
        let fixed_end =
            cursor
                .checked_add(CENTRAL_HEADER_BYTES as u64)
                .ok_or(CatalogError::Malformed(
                    crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
                ))?;
        if fixed_end > source.central_directory_end
            || fixed_end
                .checked_sub(source.central_directory_offset)
                .is_none_or(|consumed| consumed > source.central_directory_size)
        {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::Truncated,
            ));
        }
        let mut fixed = [0_u8; CENTRAL_HEADER_BYTES];
        read_exact_checked(reader, &mut fixed)?;
        check_cancel(cancellation)?;
        cursor = fixed_end;
        if le_u32(&fixed, 0) != CENTRAL_HEADER_SIGNATURE {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ));
        }

        let version_made_by = le_u16(&fixed, 4);
        let flags = le_u16(&fixed, 8);
        let method_code = le_u16(&fixed, 10);
        let crc32 = le_u32(&fixed, 16);
        let compressed32 = le_u32(&fixed, 20);
        let uncompressed32 = le_u32(&fixed, 24);
        let name_len = usize::from(le_u16(&fixed, 28));
        let extra_len = usize::from(le_u16(&fixed, 30));
        let comment_len = usize::from(le_u16(&fixed, 32));
        let disk_start16 = le_u16(&fixed, 34);
        let external_attributes = le_u32(&fixed, 38);
        let local_offset32 = le_u32(&fixed, 42);

        if name_len > limits.max_name_bytes {
            return Err(CatalogError::LimitExceeded(CatalogLimit::EntryNameBytes));
        }
        if extra_len > limits.max_extra_bytes {
            return Err(CatalogError::LimitExceeded(CatalogLimit::EntryExtraBytes));
        }
        if comment_len > limits.max_comment_bytes {
            return Err(CatalogError::LimitExceeded(CatalogLimit::EntryCommentBytes));
        }
        let record_bytes = CENTRAL_HEADER_BYTES
            .checked_add(name_len)
            .and_then(|bytes| bytes.checked_add(extra_len))
            .and_then(|bytes| bytes.checked_add(comment_len))
            .ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord,
            ))?;
        let next_cursor = cursor
            .checked_add((name_len + extra_len + comment_len) as u64)
            .ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ))?;
        if next_cursor > source.central_directory_end
            || next_cursor
                .checked_sub(source.central_directory_offset)
                .is_none_or(|consumed| consumed > source.central_directory_size)
        {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::Truncated,
            ));
        }

        let requested_metadata =
            retained_metadata_bytes
                .checked_add(name_len)
                .ok_or(CatalogError::LimitExceeded(
                    CatalogLimit::RetainedMetadataBytes,
                ))?;
        if requested_metadata > limits.max_retained_metadata_bytes {
            return Err(CatalogError::LimitExceeded(
                CatalogLimit::RetainedMetadataBytes,
            ));
        }
        let mut raw_name = Vec::new();
        raw_name
            .try_reserve_exact(name_len)
            .map_err(|_| CatalogError::Allocation)?;
        raw_name.resize(name_len, 0);
        read_exact_checked(reader, &mut raw_name)?;
        check_cancel(cancellation)?;
        cursor = cursor
            .checked_add(name_len as u64)
            .ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ))?;
        let name_capacity = raw_name.capacity();
        if retained_metadata_bytes
            .checked_add(name_capacity)
            .is_none_or(|used| used > limits.max_retained_metadata_bytes)
        {
            return Err(CatalogError::LimitExceeded(
                CatalogLimit::RetainedMetadataBytes,
            ));
        }

        let mut extra_values = Zip64EntryValues::default();
        let needs_uncompressed = uncompressed32 == u32::MAX;
        let needs_compressed = compressed32 == u32::MAX;
        let needs_local_offset = local_offset32 == u32::MAX;
        let needs_disk_start = disk_start16 == u16::MAX;
        parse_central_extra(
            reader,
            extra_len,
            needs_uncompressed,
            needs_compressed,
            needs_local_offset,
            needs_disk_start,
            &mut extra_values,
            cancellation,
        )?;
        check_cancel(cancellation)?;
        cursor = cursor
            .checked_add(extra_len as u64)
            .ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ))?;

        let comment_skip = i64::try_from(comment_len).map_err(|_| {
            CatalogError::Malformed(crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord)
        })?;
        reader
            .seek(SeekFrom::Current(comment_skip))
            .map_err(|error| CatalogError::Io(error.kind()))?;
        cursor = cursor
            .checked_add(comment_len as u64)
            .ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ))?;
        debug_assert_eq!(cursor, next_cursor);
        debug_assert_eq!(record_bytes as u64, cursor - record_offset);

        let compressed_size = if needs_compressed {
            extra_values.compressed_size.ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
            ))?
        } else {
            u64::from(compressed32)
        };
        let uncompressed_size = if needs_uncompressed {
            extra_values
                .uncompressed_size
                .ok_or(CatalogError::Malformed(
                    crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
                ))?
        } else {
            u64::from(uncompressed32)
        };
        let local_header_offset = if needs_local_offset {
            extra_values
                .local_header_offset
                .ok_or(CatalogError::Malformed(
                    crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
                ))?
        } else {
            u64::from(local_offset32)
        };
        let disk_start = if needs_disk_start {
            extra_values.disk_start.ok_or(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
            ))?
        } else {
            u32::from(disk_start16)
        };
        if disk_start != 0 {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::MultiDiskArchive,
            ));
        }
        let local_header_end =
            local_header_offset
                .checked_add(30)
                .ok_or(CatalogError::Malformed(
                    crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord,
                ))?;
        if local_header_offset >= source.central_directory_offset
            || local_header_end > source.central_directory_offset
        {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord,
            ));
        }
        minimum_local_offset = Some(
            minimum_local_offset.map_or(local_header_offset, |current: u64| {
                current.min(local_header_offset)
            }),
        );

        let name_capacity = name_capacity.max(name_len);
        retained_metadata_bytes = retained_metadata_bytes.checked_add(name_capacity).ok_or(
            CatalogError::LimitExceeded(CatalogLimit::RetainedMetadataBytes),
        )?;
        let name = decode_name(raw_name, flags);
        let (kind, unsafe_kind) = classify_kind(&name, version_made_by, external_attributes);
        let compression = EntryCompression::from_code(method_code);
        let safe_path_problem = name
            .as_utf8()
            .and_then(|value| super::model::SafeRelativePath::validate(value, kind, &limits).err());
        let problems = EntryProblems {
            name_encoding: match name {
                ArchiveEntryName::InvalidUtf8(_) => Some(NameEncodingProblem::InvalidUtf8),
                ArchiveEntryName::UnsupportedLegacyEncoding(_) => {
                    Some(NameEncodingProblem::UnsupportedLegacyEncoding)
                }
                ArchiveEntryName::Utf8(_) => None,
            },
            unsafe_path: safe_path_problem,
            unsafe_kind,
            encrypted: flags & ENCRYPTED_FLAGS != 0 || method_code == 99,
            inconsistent_stored_size: method_code == 0 && compressed_size != uncompressed_size,
            unsupported_flags: unsupported_flags(flags, method_code),
        };
        entries.push(ArchiveEntry {
            id: ArchiveEntryId::new(revision, ordinal),
            name,
            kind,
            compression,
            flags,
            crc32,
            compressed_size,
            uncompressed_size,
            local_header_offset,
            central_header_offset: record_offset,
            version_made_by,
            external_attributes,
            problems,
        });
    }

    if cursor != source.central_directory_end
        || cursor
            .checked_sub(source.central_directory_offset)
            .is_none_or(|consumed| consumed != source.central_directory_size)
    {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
        ));
    }
    if source.entry_count == 0 {
        if source.central_directory_offset != 0 {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::UnsupportedArchiveOffset,
            ));
        }
    } else if minimum_local_offset != Some(0) {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::UnsupportedArchiveOffset,
        ));
    }

    check_cancel(cancellation)?;
    Ok(ArchiveCatalog::new(
        source,
        limits,
        entries,
        retained_metadata_bytes,
    ))
}

fn find_end_record<R: Read + Seek>(
    reader: &mut R,
    archive_len: u64,
    revision: ArchiveRevision,
    limits: &ArchiveLimits,
    cancellation: &ScanCancellation,
) -> Result<ArchiveSource, CatalogError> {
    let scan_len = archive_len.min(65_557);
    if scan_len > limits.max_eocd_scan_bytes as u64 {
        return Err(CatalogError::LimitExceeded(
            CatalogLimit::EndRecordSearchBytes,
        ));
    }
    if scan_len < EOCD_FIXED_BYTES as u64 {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::Truncated,
        ));
    }

    let first_candidate = archive_len - EOCD_FIXED_BYTES as u64;
    let earliest_candidate = archive_len - scan_len;
    let mut candidate_attempts = 0usize;
    let mut saw_signature = false;
    let mut over_limit = None;
    let mut last_problem = crate::archive::model::ArchiveFormatProblem::MissingEndRecord;
    let mut position = first_candidate;

    loop {
        check_cancel(cancellation)?;
        let mut signature = [0_u8; 4];
        read_exact_at(reader, position, &mut signature, cancellation)?;
        if le_u32(&signature, 0) == EOCD_SIGNATURE {
            saw_signature = true;
            candidate_attempts =
                candidate_attempts
                    .checked_add(1)
                    .ok_or(CatalogError::LimitExceeded(
                        CatalogLimit::EndRecordCandidates,
                    ))?;
            if candidate_attempts > limits.max_eocd_candidates {
                return Err(CatalogError::LimitExceeded(
                    CatalogLimit::EndRecordCandidates,
                ));
            }
            let mut fixed = [0_u8; EOCD_FIXED_BYTES];
            read_exact_at(reader, position, &mut fixed, cancellation)?;
            let comment_size = usize::from(le_u16(&fixed, 20));
            let comment_end = position
                .checked_add(EOCD_FIXED_BYTES as u64)
                .and_then(|offset| offset.checked_add(comment_size as u64));
            if comment_end == Some(archive_len) {
                match inspect_end_candidate(
                    reader,
                    position,
                    archive_len,
                    revision,
                    &fixed,
                    comment_size,
                    limits,
                    cancellation,
                )? {
                    CandidateInspection::Valid(source) => return Ok(source),
                    CandidateInspection::OverLimit(limit) => {
                        over_limit.get_or_insert(limit);
                    }
                    CandidateInspection::Invalid(problem) => last_problem = problem,
                    CandidateInspection::NotAnEndRecord => {}
                }
            }
        }

        if position == earliest_candidate {
            break;
        }
        position -= 1;
    }

    if let Some(limit) = over_limit {
        return Err(CatalogError::LimitExceeded(limit));
    }
    Err(CatalogError::Malformed(if saw_signature {
        last_problem
    } else {
        crate::archive::model::ArchiveFormatProblem::MissingEndRecord
    }))
}

#[allow(clippy::too_many_arguments)]
fn inspect_end_candidate<R: Read + Seek>(
    reader: &mut R,
    eocd_offset: u64,
    archive_len: u64,
    revision: ArchiveRevision,
    fixed: &[u8; EOCD_FIXED_BYTES],
    comment_size: usize,
    limits: &ArchiveLimits,
    cancellation: &ScanCancellation,
) -> Result<CandidateInspection, CatalogError> {
    let end = EndFields {
        disk_number: le_u16(fixed, 4),
        central_disk: le_u16(fixed, 6),
        entries_on_disk: le_u16(fixed, 8),
        entry_count: le_u16(fixed, 10),
        central_size: le_u32(fixed, 12),
        central_offset: le_u32(fixed, 16),
        comment_size: le_u16(fixed, 20),
    };
    if usize::from(end.comment_size) != comment_size {
        return Ok(CandidateInspection::NotAnEndRecord);
    }

    let zip64_sentinel = end.entries_on_disk == u16::MAX
        || end.entry_count == u16::MAX
        || end.central_size == u32::MAX
        || end.central_offset == u32::MAX;
    let mut resolved = None;
    if eocd_offset >= ZIP64_LOCATOR_BYTES as u64 {
        let locator_offset = eocd_offset - ZIP64_LOCATOR_BYTES as u64;
        let mut locator = [0_u8; ZIP64_LOCATOR_BYTES];
        read_exact_at(reader, locator_offset, &mut locator, cancellation)?;
        if le_u32(&locator, 0) == ZIP64_LOCATOR_SIGNATURE {
            resolved = match read_zip64_end(reader, locator_offset, &locator, cancellation)? {
                Ok(value) => Some(value),
                Err(problem) => return Ok(CandidateInspection::Invalid(problem)),
            };
        }
    }

    if end.disk_number != 0 || end.central_disk != 0 {
        return Ok(CandidateInspection::Invalid(
            crate::archive::model::ArchiveFormatProblem::MultiDiskArchive,
        ));
    }

    let (entry_count, central_size, central_offset, central_end, format) = if let Some(zip64) =
        resolved
    {
        if (end.entries_on_disk != u16::MAX
            && u64::from(end.entries_on_disk) != zip64.entries_on_disk)
            || (end.entry_count != u16::MAX && u64::from(end.entry_count) != zip64.entry_count)
            || (end.central_size != u32::MAX && u64::from(end.central_size) != zip64.central_size)
            || (end.central_offset != u32::MAX
                && u64::from(end.central_offset) != zip64.central_offset)
        {
            return Ok(CandidateInspection::Invalid(
                crate::archive::model::ArchiveFormatProblem::InconsistentEndRecords,
            ));
        }
        (
            zip64.entry_count,
            zip64.central_size,
            zip64.central_offset,
            zip64.central_end,
            ArchiveFormat::Zip64,
        )
    } else if zip64_sentinel {
        return Ok(CandidateInspection::Invalid(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    } else {
        if end.entries_on_disk != end.entry_count {
            return Ok(CandidateInspection::Invalid(
                crate::archive::model::ArchiveFormatProblem::MultiDiskArchive,
            ));
        }
        (
            u64::from(end.entry_count),
            u64::from(end.central_size),
            u64::from(end.central_offset),
            eocd_offset,
            ArchiveFormat::Zip32,
        )
    };

    let calculated_end = match central_offset.checked_add(central_size) {
        Some(value) if value == central_end => value,
        _ => {
            return Ok(CandidateInspection::Invalid(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
            ));
        }
    };
    if calculated_end > eocd_offset || central_end > archive_len {
        return Ok(CandidateInspection::Invalid(
            crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
        ));
    }
    if comment_size > limits.max_comment_bytes {
        return Ok(CandidateInspection::OverLimit(
            CatalogLimit::ArchiveCommentBytes,
        ));
    }
    if central_size > limits.max_central_directory_bytes {
        return Ok(CandidateInspection::OverLimit(
            CatalogLimit::CentralDirectoryBytes,
        ));
    }
    let count = usize::try_from(entry_count)
        .map_err(|_| CatalogError::LimitExceeded(CatalogLimit::EntryCount))?;
    if count > limits.max_entries {
        return Ok(CandidateInspection::OverLimit(CatalogLimit::EntryCount));
    }
    if entry_count == 0 && central_size != 0 {
        return Ok(CandidateInspection::Invalid(
            crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory,
        ));
    }
    if entry_count == 0 && central_offset != 0 {
        return Ok(CandidateInspection::Invalid(
            crate::archive::model::ArchiveFormatProblem::UnsupportedArchiveOffset,
        ));
    }

    Ok(CandidateInspection::Valid(ArchiveSource {
        revision,
        archive_len,
        central_directory_offset: central_offset,
        central_directory_size: central_size,
        central_directory_end: calculated_end,
        entry_count,
        format,
    }))
}

#[derive(Clone, Copy, Debug)]
struct Zip64End {
    entries_on_disk: u64,
    entry_count: u64,
    central_size: u64,
    central_offset: u64,
    central_end: u64,
}

fn read_zip64_end<R: Read + Seek>(
    reader: &mut R,
    locator_offset: u64,
    locator: &[u8; ZIP64_LOCATOR_BYTES],
    cancellation: &ScanCancellation,
) -> Result<Result<Zip64End, crate::archive::model::ArchiveFormatProblem>, CatalogError> {
    if le_u32(locator, 4) != 0 || le_u32(locator, 16) != 1 {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::MultiDiskArchive,
        ));
    }
    let zip64_offset = le_u64(locator, 8);
    if zip64_offset
        .checked_add(ZIP64_EOCD_FIXED_BYTES as u64)
        .is_none_or(|end| end > locator_offset)
    {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    }
    let mut fixed = [0_u8; ZIP64_EOCD_FIXED_BYTES];
    read_exact_at(reader, zip64_offset, &mut fixed, cancellation)?;
    if le_u32(&fixed, 0) != ZIP64_EOCD_SIGNATURE {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    }
    let record_size = le_u64(&fixed, 4);
    if record_size < 44 {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    }
    let extensible_size = record_size - 44;
    if extensible_size > MAX_ZIP64_EXTENSIBLE_DATA_BYTES {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    }
    let record_end = zip64_offset
        .checked_add(12)
        .and_then(|start| start.checked_add(record_size));
    if record_end != Some(locator_offset) {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope,
        ));
    }
    let disk_number = le_u32(&fixed, 16);
    let central_disk = le_u32(&fixed, 20);
    let entries_on_disk = le_u64(&fixed, 24);
    let entry_count = le_u64(&fixed, 32);
    if disk_number != 0 || central_disk != 0 || entries_on_disk != entry_count {
        return Ok(Err(
            crate::archive::model::ArchiveFormatProblem::MultiDiskArchive,
        ));
    }
    Ok(Ok(Zip64End {
        entries_on_disk,
        entry_count,
        central_size: le_u64(&fixed, 40),
        central_offset: le_u64(&fixed, 48),
        central_end: zip64_offset,
    }))
}

#[allow(clippy::too_many_arguments)]
fn parse_central_extra<R: Read + Seek>(
    reader: &mut R,
    extra_len: usize,
    need_uncompressed: bool,
    need_compressed: bool,
    need_local_offset: bool,
    need_disk_start: bool,
    values: &mut Zip64EntryValues,
    cancellation: &ScanCancellation,
) -> Result<(), CatalogError> {
    let mut remaining = extra_len;
    let mut saw_zip64 = false;
    while remaining > 0 {
        check_cancel(cancellation)?;
        if remaining < 4 {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
            ));
        }
        let mut header = [0_u8; 4];
        read_exact_checked(reader, &mut header)?;
        remaining -= 4;
        let id = le_u16(&header, 0);
        let field_len = usize::from(le_u16(&header, 2));
        if field_len > remaining {
            return Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
            ));
        }
        if id == ZIP64_EXTRA_ID {
            if saw_zip64 {
                return Err(CatalogError::Malformed(
                    crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
                ));
            }
            saw_zip64 = true;
            let mut field_remaining = field_len;
            if need_uncompressed {
                values.uncompressed_size = Some(read_extra_u64(reader, &mut field_remaining)?);
            }
            if need_compressed {
                values.compressed_size = Some(read_extra_u64(reader, &mut field_remaining)?);
            }
            if need_local_offset {
                values.local_header_offset = Some(read_extra_u64(reader, &mut field_remaining)?);
            }
            if need_disk_start {
                values.disk_start = Some(read_extra_u32(reader, &mut field_remaining)?);
            }
            skip_current(reader, field_remaining as u64)?;
        } else {
            skip_current(reader, field_len as u64)?;
        }
        remaining -= field_len;
    }
    if (need_uncompressed && values.uncompressed_size.is_none())
        || (need_compressed && values.compressed_size.is_none())
        || (need_local_offset && values.local_header_offset.is_none())
        || (need_disk_start && values.disk_start.is_none())
    {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
        ));
    }
    Ok(())
}

fn read_extra_u64<R: Read + Seek>(
    reader: &mut R,
    remaining: &mut usize,
) -> Result<u64, CatalogError> {
    if *remaining < 8 {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
        ));
    }
    let mut value = [0_u8; 8];
    read_exact_checked(reader, &mut value)?;
    *remaining -= 8;
    Ok(u64::from_le_bytes(value))
}

fn read_extra_u32<R: Read + Seek>(
    reader: &mut R,
    remaining: &mut usize,
) -> Result<u32, CatalogError> {
    if *remaining < 4 {
        return Err(CatalogError::Malformed(
            crate::archive::model::ArchiveFormatProblem::InvalidExtraField,
        ));
    }
    let mut value = [0_u8; 4];
    read_exact_checked(reader, &mut value)?;
    *remaining -= 4;
    Ok(u32::from_le_bytes(value))
}

fn decode_name(bytes: Vec<u8>, flags: u16) -> ArchiveEntryName {
    let utf8_flag = flags & UTF8_NAME_FLAG != 0;
    if utf8_flag || bytes.is_ascii() {
        return match String::from_utf8(bytes) {
            Ok(name) => ArchiveEntryName::Utf8(name),
            Err(error) => ArchiveEntryName::InvalidUtf8(error.into_bytes()),
        };
    }
    ArchiveEntryName::UnsupportedLegacyEncoding(bytes)
}

fn unsupported_flags(flags: u16, method_code: u16) -> u16 {
    // Descriptor and UTF-8 flags are supported generally; Deflate option bits
    // are meaningful only for method 8. Encryption bits have a separate status.
    let mut supported = (1_u16 << 3) | UTF8_NAME_FLAG | ENCRYPTED_FLAGS;
    if method_code == 8 {
        supported |= (1_u16 << 1) | (1_u16 << 2);
    }
    flags & !supported
}

fn classify_kind(
    name: &ArchiveEntryName,
    version_made_by: u16,
    external_attributes: u32,
) -> (EntryKind, Option<UnsafeEntryReason>) {
    let host = (version_made_by >> 8) as u8;
    let raw = name.raw_bytes();
    let name_directory = raw.ends_with(b"/") || raw.ends_with(b"\\");
    let high_attributes = (external_attributes >> 16) as u16;
    let unix_type = high_attributes & 0xF000;
    let unix_host = host == 3 || host == 19;
    let dos_host = matches!(host, 0 | 10 | 11 | 14);
    let dos_directory = dos_host && (external_attributes as u16) & DOS_DIRECTORY_ATTRIBUTE != 0;
    let reparse = dos_host && (external_attributes as u16) & DOS_REPARSE_ATTRIBUTE != 0;

    let unsafe_kind = if unix_host && unix_type == 0xA000 {
        Some(UnsafeEntryReason::SymbolicLink)
    } else if reparse {
        Some(UnsafeEntryReason::ReparsePoint)
    } else if unix_host && unix_type != 0 && unix_type != 0x8000 && unix_type != 0x4000 {
        Some(UnsafeEntryReason::DeviceOrSpecialFile)
    } else {
        None
    };
    let kind = if name_directory || dos_directory || (unix_host && unix_type == 0x4000) {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    (kind, unsafe_kind)
}

fn read_exact_at<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    buffer: &mut [u8],
    cancellation: &ScanCancellation,
) -> Result<(), CatalogError> {
    check_cancel(cancellation)?;
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| CatalogError::Io(error.kind()))?;
    read_exact_checked(reader, buffer)?;
    check_cancel(cancellation)
}

fn read_exact_checked<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<(), CatalogError> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            CatalogError::Malformed(crate::archive::model::ArchiveFormatProblem::Truncated)
        } else {
            CatalogError::Io(error.kind())
        }
    })
}

fn skip_current<R: Seek>(reader: &mut R, bytes: u64) -> Result<(), CatalogError> {
    let bytes = i64::try_from(bytes).map_err(|_| {
        CatalogError::Malformed(crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord)
    })?;
    reader
        .seek(SeekFrom::Current(bytes))
        .map(|_| ())
        .map_err(|error| CatalogError::Io(error.kind()))
}

fn check_cancel(cancellation: &ScanCancellation) -> Result<(), CatalogError> {
    if cancellation.is_cancelled() {
        Err(CatalogError::Cancelled)
    } else {
        Ok(())
    }
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}
#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read, Seek, SeekFrom};
    use std::ops::Range;

    use super::*;

    struct Spec {
        name: Vec<u8>,
        payload: Vec<u8>,
        expanded: u32,
        flags: u16,
        method: u16,
        made_by: u16,
        external: u32,
        extra: Vec<u8>,
        comment: Vec<u8>,
    }

    impl Spec {
        fn file(name: &[u8], payload: &[u8]) -> Self {
            Self {
                name: name.to_vec(),
                payload: payload.to_vec(),
                expanded: payload.len() as u32,
                flags: 0,
                method: 0,
                made_by: 20,
                external: 0,
                extra: Vec::new(),
                comment: Vec::new(),
            }
        }
    }

    fn put16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn put32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
    fn put64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn make_zip(specs: &[Spec], archive_comment: &[u8]) -> (Vec<u8>, Vec<Range<usize>>) {
        let mut bytes = Vec::new();
        let mut payloads = Vec::new();
        let mut offsets = Vec::new();
        for spec in specs {
            offsets.push(bytes.len() as u32);
            put32(&mut bytes, 0x0403_4b50);
            put16(&mut bytes, 20);
            put16(&mut bytes, spec.flags);
            put16(&mut bytes, spec.method);
            put16(&mut bytes, 0);
            put16(&mut bytes, 0);
            put32(&mut bytes, 0);
            put32(&mut bytes, spec.payload.len() as u32);
            put32(&mut bytes, spec.expanded);
            put16(&mut bytes, spec.name.len() as u16);
            put16(&mut bytes, 0);
            bytes.extend_from_slice(&spec.name);
            let start = bytes.len();
            bytes.extend_from_slice(&spec.payload);
            payloads.push(start..bytes.len());
        }

        let central_offset = bytes.len() as u32;
        for (spec, local_offset) in specs.iter().zip(offsets.iter().copied()) {
            put32(&mut bytes, 0x0201_4b50);
            put16(&mut bytes, spec.made_by);
            put16(&mut bytes, 20);
            put16(&mut bytes, spec.flags);
            put16(&mut bytes, spec.method);
            put16(&mut bytes, 0);
            put16(&mut bytes, 0);
            put32(&mut bytes, 0);
            put32(&mut bytes, spec.payload.len() as u32);
            put32(&mut bytes, spec.expanded);
            put16(&mut bytes, spec.name.len() as u16);
            put16(&mut bytes, spec.extra.len() as u16);
            put16(&mut bytes, spec.comment.len() as u16);
            put16(&mut bytes, 0);
            put16(&mut bytes, 0);
            put32(&mut bytes, spec.external);
            put32(&mut bytes, local_offset);
            bytes.extend_from_slice(&spec.name);
            bytes.extend_from_slice(&spec.extra);
            bytes.extend_from_slice(&spec.comment);
        }
        let central_size = bytes.len() as u32 - central_offset;
        put32(&mut bytes, EOCD_SIGNATURE);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, specs.len() as u16);
        put16(&mut bytes, specs.len() as u16);
        put32(&mut bytes, central_size);
        put32(&mut bytes, central_offset);
        put16(&mut bytes, archive_comment.len() as u16);
        bytes.extend_from_slice(archive_comment);
        (bytes, payloads)
    }

    fn make_zip64_one() -> (Vec<u8>, Vec<Range<usize>>) {
        let mut bytes = Vec::new();
        let name = b"zip64.txt";
        let payload = b"x";
        put32(&mut bytes, 0x0403_4b50);
        put16(&mut bytes, 45);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put32(&mut bytes, 0);
        put32(&mut bytes, 1);
        put32(&mut bytes, 1);
        put16(&mut bytes, name.len() as u16);
        put16(&mut bytes, 0);
        bytes.extend_from_slice(name);
        let payload_start = bytes.len();
        bytes.extend_from_slice(payload);
        let payloads = vec![payload_start..bytes.len()];
        let central_offset = bytes.len() as u64;
        let mut zip64_extra = Vec::new();
        put16(&mut zip64_extra, ZIP64_EXTRA_ID);
        put16(&mut zip64_extra, 24);
        put64(&mut zip64_extra, 1);
        put64(&mut zip64_extra, 1);
        put64(&mut zip64_extra, 0);
        put32(&mut bytes, CENTRAL_HEADER_SIGNATURE);
        put16(&mut bytes, 45);
        put16(&mut bytes, 45);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put32(&mut bytes, 0);
        put32(&mut bytes, u32::MAX);
        put32(&mut bytes, u32::MAX);
        put16(&mut bytes, name.len() as u16);
        put16(&mut bytes, zip64_extra.len() as u16);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put32(&mut bytes, 0);
        put32(&mut bytes, u32::MAX);
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&zip64_extra);
        let central_size = bytes.len() as u64 - central_offset;
        let zip64_offset = bytes.len() as u64;
        put32(&mut bytes, ZIP64_EOCD_SIGNATURE);
        put64(&mut bytes, 44);
        put16(&mut bytes, 45);
        put16(&mut bytes, 45);
        put32(&mut bytes, 0);
        put32(&mut bytes, 0);
        put64(&mut bytes, 1);
        put64(&mut bytes, 1);
        put64(&mut bytes, central_size);
        put64(&mut bytes, central_offset);
        put32(&mut bytes, ZIP64_LOCATOR_SIGNATURE);
        put32(&mut bytes, 0);
        put64(&mut bytes, zip64_offset);
        put32(&mut bytes, 1);
        put32(&mut bytes, EOCD_SIGNATURE);
        put16(&mut bytes, 0);
        put16(&mut bytes, 0);
        put16(&mut bytes, u16::MAX);
        put16(&mut bytes, u16::MAX);
        put32(&mut bytes, u32::MAX);
        put32(&mut bytes, u32::MAX);
        put16(&mut bytes, 0);
        (bytes, payloads)
    }

    struct RejectPayloadReader {
        cursor: Cursor<Vec<u8>>,
        payloads: Vec<Range<usize>>,
        reads: usize,
    }

    impl RejectPayloadReader {
        fn new(bytes: Vec<u8>, payloads: Vec<Range<usize>>) -> Self {
            Self {
                cursor: Cursor::new(bytes),
                payloads,
                reads: 0,
            }
        }
    }

    impl Read for RejectPayloadReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let start = self.cursor.position() as usize;
            let count = self.cursor.read(buffer)?;
            self.reads += 1;
            let end = start.saturating_add(count);
            if count != 0
                && self
                    .payloads
                    .iter()
                    .any(|range| start < range.end && end > range.start)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "entry payload read is forbidden during cataloging",
                ));
            }
            Ok(count)
        }
    }

    impl Seek for RejectPayloadReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.cursor.seek(position)
        }
    }

    /// Allows only bounded ZIP footer discovery reads. The backward EOCD
    /// search may probe four-byte signatures anywhere in its defined suffix,
    /// including bytes that happen to overlap an entry payload in a malformed
    /// archive. Fixed footer reads are admitted only at footer signatures or
    /// the exact optional-locator position adjacent to an EOCD candidate, and
    /// never when they overlap a known payload. This reader is intentionally
    /// used only by malformed/over-limit fixtures.
    struct BoundedFooterProbeReader {
        cursor: Cursor<Vec<u8>>,
        payloads: Vec<Range<usize>>,
        suffix_start: u64,
        archive_len: u64,
        last_eocd_candidate: Option<u64>,
        max_signature_probes: usize,
        max_candidates: usize,
        max_total_bytes: usize,
        signature_probes: usize,
        eocd_candidates: usize,
        zip64_locator_reads: usize,
        zip64_record_reads: usize,
        bytes_read: usize,
        forbidden_reads: usize,
    }

    impl BoundedFooterProbeReader {
        fn new(bytes: Vec<u8>, payloads: Vec<Range<usize>>, limits: ArchiveLimits) -> Self {
            let archive_len = bytes.len() as u64;
            let scan_bytes = limits.max_eocd_scan_bytes.min(65_557).min(bytes.len());
            let suffix_start = archive_len.saturating_sub(scan_bytes as u64);
            let max_signature_probes = scan_bytes.saturating_sub(EOCD_FIXED_BYTES - 1);
            let max_total_bytes = max_signature_probes.saturating_mul(4).saturating_add(
                limits.max_eocd_candidates.saturating_mul(
                    EOCD_FIXED_BYTES + ZIP64_LOCATOR_BYTES + ZIP64_EOCD_FIXED_BYTES,
                ),
            );
            Self {
                cursor: Cursor::new(bytes),
                payloads,
                suffix_start,
                archive_len,
                last_eocd_candidate: None,
                max_signature_probes,
                max_candidates: limits.max_eocd_candidates,
                max_total_bytes,
                signature_probes: 0,
                eocd_candidates: 0,
                zip64_locator_reads: 0,
                zip64_record_reads: 0,
                bytes_read: 0,
                forbidden_reads: 0,
            }
        }

        fn stats(&self) -> String {
            format!(
                "footer reader: probes={}/{}, candidates={}/{}, locator_reads={}/{}, zip64_reads={}/{}, bytes={}/{}, forbidden={}",
                self.signature_probes,
                self.max_signature_probes,
                self.eocd_candidates,
                self.max_candidates,
                self.zip64_locator_reads,
                self.max_candidates,
                self.zip64_record_reads,
                self.max_candidates,
                self.bytes_read,
                self.max_total_bytes,
                self.forbidden_reads,
            )
        }

        fn assert_bounded(&self) {
            assert!(
                self.signature_probes <= self.max_signature_probes
                    && self.eocd_candidates <= self.max_candidates
                    && self.zip64_locator_reads <= self.max_candidates
                    && self.zip64_record_reads <= self.max_candidates
                    && self
                        .eocd_candidates
                        .saturating_add(self.zip64_locator_reads)
                        .saturating_add(self.zip64_record_reads)
                        <= self.max_candidates.saturating_mul(3)
                    && self.bytes_read <= self.max_total_bytes
                    && self.forbidden_reads == 0,
                "{}",
                self.stats()
            );
        }

        fn deny_read(&mut self, message: &'static str) -> io::Result<usize> {
            self.forbidden_reads += 1;
            Err(io::Error::new(io::ErrorKind::PermissionDenied, message))
        }

        fn overlaps_payload(&self, start: u64, end: u64) -> bool {
            self.payloads.iter().any(|range| {
                let payload_start = range.start as u64;
                let payload_end = range.end as u64;
                start < payload_end && end > payload_start
            })
        }
    }

    impl Read for BoundedFooterProbeReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }

            let start = self.cursor.position();
            let end = match start.checked_add(buffer.len() as u64) {
                Some(end) if start >= self.suffix_start && end <= self.archive_len => end,
                _ => return self.deny_read("footer read escaped the bounded archive suffix"),
            };

            let (signature, zip64_record_size) = {
                let bytes = self.cursor.get_ref();
                let offset = start as usize;
                let signature = (buffer.len() >= 4).then(|| le_u32(bytes, offset));
                let zip64_record_size =
                    (buffer.len() == ZIP64_EOCD_FIXED_BYTES).then(|| le_u64(bytes, offset + 4));
                (signature, zip64_record_size)
            };
            match buffer.len() {
                4 => {
                    if self.signature_probes >= self.max_signature_probes {
                        return self.deny_read("footer signature-probe bound exceeded");
                    }
                    self.signature_probes += 1;
                }
                EOCD_FIXED_BYTES if signature == Some(EOCD_SIGNATURE) => {
                    if self.eocd_candidates >= self.max_candidates {
                        return self.deny_read("footer fixed-record bound exceeded");
                    }
                    if self.overlaps_payload(start, end) {
                        return self.deny_read("EOCD fixed-record read overlaps entry payload");
                    }
                    self.eocd_candidates += 1;
                    self.last_eocd_candidate = Some(start);
                }
                ZIP64_LOCATOR_BYTES
                    if start.checked_add(ZIP64_LOCATOR_BYTES as u64)
                        == self.last_eocd_candidate =>
                {
                    if self.zip64_locator_reads >= self.max_candidates {
                        return self.deny_read("footer fixed-record bound exceeded");
                    }
                    if self.overlaps_payload(start, end) {
                        return self.deny_read("locator probe overlaps entry payload");
                    }
                    self.zip64_locator_reads += 1;
                }
                ZIP64_EOCD_FIXED_BYTES
                    if signature == Some(ZIP64_EOCD_SIGNATURE)
                        && zip64_record_size.is_some_and(|size| size >= 44) =>
                {
                    if self.zip64_record_reads >= self.max_candidates {
                        return self.deny_read("footer fixed-record bound exceeded");
                    }
                    if self.overlaps_payload(start, end) {
                        return self.deny_read("ZIP64 fixed-record read overlaps entry payload");
                    }
                    self.zip64_record_reads += 1;
                }
                _ => return self.deny_read("non-footer metadata or payload read is forbidden"),
            }

            if self.bytes_read.saturating_add(buffer.len()) > self.max_total_bytes {
                return self.deny_read("footer byte bound exceeded");
            }
            let count = self.cursor.read(buffer)?;
            self.bytes_read = self.bytes_read.saturating_add(count);
            Ok(count)
        }
    }

    impl Seek for BoundedFooterProbeReader {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.cursor.seek(position)
        }
    }

    struct CentralReadGuard {
        cursor: Cursor<Vec<u8>>,
        central_start: u64,
        central_end: u64,
        scanning_central: bool,
        central_bytes_read: usize,
        forbidden_reads: usize,
    }

    impl CentralReadGuard {
        fn new(bytes: Vec<u8>, central_start: usize, central_end: usize) -> Self {
            Self {
                cursor: Cursor::new(bytes),
                central_start: central_start as u64,
                central_end: central_end as u64,
                scanning_central: false,
                central_bytes_read: 0,
                forbidden_reads: 0,
            }
        }
    }

    impl Read for CentralReadGuard {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }
            let start = self.cursor.position();
            if self.scanning_central && start >= self.central_end {
                self.forbidden_reads += 1;
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "read beyond declared central directory is forbidden",
                ));
            }
            let allowed = if self.scanning_central && start < self.central_end {
                usize::try_from(self.central_end - start)
                    .unwrap_or(usize::MAX)
                    .min(buffer.len())
            } else {
                buffer.len()
            };
            let count = self.cursor.read(&mut buffer[..allowed])?;
            if !self.scanning_central
                && start == self.central_start
                && count >= CENTRAL_HEADER_BYTES
                && le_u32(buffer, 0) == CENTRAL_HEADER_SIGNATURE
            {
                self.scanning_central = true;
                self.central_bytes_read += count;
            } else if self.scanning_central && start >= self.central_start {
                self.central_bytes_read += count;
            }
            Ok(count)
        }
    }

    impl Seek for CentralReadGuard {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.cursor.seek(position)
        }
    }

    struct CancelOnCentralHeaderRead {
        cursor: Cursor<Vec<u8>>,
        central_offset: u64,
        cancellation: ScanCancellation,
        reads_after_cancel: usize,
    }

    impl Read for CancelOnCentralHeaderRead {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.cancellation.is_cancelled() {
                self.reads_after_cancel += 1;
            }
            let start = self.cursor.position();
            let count = self.cursor.read(buffer)?;
            if count != 0 && start == self.central_offset {
                self.cancellation.cancel();
            }
            Ok(count)
        }
    }

    impl Seek for CancelOnCentralHeaderRead {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            self.cursor.seek(position)
        }
    }

    fn read(
        reader: &mut impl ReadSeek,
        limits: ArchiveLimits,
    ) -> Result<ArchiveCatalog, CatalogError> {
        read_catalog(
            reader,
            ArchiveRevision::new(31),
            limits,
            &ScanCancellation::new(),
        )
    }

    trait ReadSeek: Read + Seek {}
    impl<T: Read + Seek> ReadSeek for T {}

    #[test]
    fn metadata_catalog_preserves_physical_duplicates_and_never_reads_payloads() {
        let mut a = Spec::file(b"same.txt", b"data-one");
        a.method = 0;
        let mut b = Spec::file(b"same.txt", &[0x03, 0x00]);
        b.method = 8;
        b.expanded = 0;
        let directory = Spec::file(b"folder/", b"");
        let (bytes, payloads) = make_zip(&[a, b, directory], b"small archive comment");
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, ArchiveLimits::default()).unwrap();
        assert_eq!(catalog.source().format, ArchiveFormat::Zip32);
        assert_eq!(catalog.entries().len(), 3);
        assert_eq!(
            catalog.entries()[0].id(),
            ArchiveEntryId::new(ArchiveRevision::new(31), 0)
        );
        assert_eq!(
            catalog.entries()[1].id(),
            ArchiveEntryId::new(ArchiveRevision::new(31), 1)
        );
        assert_eq!(catalog.entries()[0].name().as_utf8(), Some("same.txt"));
        assert_eq!(catalog.entries()[1].name().as_utf8(), Some("same.txt"));
        assert_eq!(catalog.entries()[2].kind(), EntryKind::Directory);
        assert_eq!(catalog.entries()[0].compression(), EntryCompression::Stored);
        assert_eq!(
            catalog.entries()[1].compression(),
            EntryCompression::Deflated
        );
        assert!(
            reader.reads < 256,
            "footer and central metadata work is bounded"
        );
    }

    #[test]
    fn zip64_catalog_reads_physical_record_and_honors_entry_zip64_values() {
        let (bytes, payloads) = make_zip64_one();
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, ArchiveLimits::default()).unwrap();
        assert_eq!(catalog.source().format, ArchiveFormat::Zip64);
        assert_eq!(catalog.source().entry_count, 1);
        assert_eq!(catalog.entries()[0].uncompressed_size(), 1);
        assert_eq!(catalog.entries()[0].local_header_offset(), 0);

        let (mut oversized, payloads) = make_zip64_one();
        let zip64_offset = oversized
            .windows(4)
            .position(|window| window == ZIP64_EOCD_SIGNATURE.to_le_bytes())
            .unwrap();
        let locator_offset = zip64_offset + ZIP64_EOCD_FIXED_BYTES;
        let extensible_size = MAX_ZIP64_EXTENSIBLE_DATA_BYTES as usize + 1;
        oversized.splice(
            locator_offset..locator_offset,
            std::iter::repeat_n(0, extensible_size),
        );
        let record_size = 44_u64 + extensible_size as u64;
        oversized[zip64_offset + 4..zip64_offset + 12].copy_from_slice(&record_size.to_le_bytes());
        let limits = ArchiveLimits::default();
        let mut reader = BoundedFooterProbeReader::new(oversized, payloads, limits);
        let result = read(&mut reader, limits);
        assert_eq!(
            result.as_ref().err(),
            Some(&CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidZip64Envelope
            )),
            "oversized extensible data should fail during bounded footer inspection; {}",
            reader.stats()
        );
        reader.assert_bounded();
    }

    #[test]
    fn unsupported_encrypted_unsafe_and_bad_encoding_entries_remain_visible() {
        let mut invalid_utf8 = Spec::file(b"bad\xff.txt", b"x");
        invalid_utf8.flags = UTF8_NAME_FLAG;
        let legacy = Spec::file(b"legacy\x82.txt", b"x");
        let traversal = Spec::file(b"../outside", b"x");
        let mut symlink = Spec::file(b"link", b"target");
        symlink.made_by = (3 << 8) | 20;
        symlink.external = (0xa000_u32) << 16;
        let mut encrypted = Spec::file(b"secret", b"x");
        encrypted.flags = 1;
        let mut unsupported = Spec::file(b"unsupported", b"x");
        unsupported.method = 12;
        let (bytes, payloads) = make_zip(
            &[
                invalid_utf8,
                legacy,
                traversal,
                symlink,
                encrypted,
                unsupported,
            ],
            b"",
        );
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, ArchiveLimits::default()).unwrap();
        assert_eq!(catalog.entries().len(), 6);
        assert_eq!(
            catalog.entries()[0].problems().name_encoding,
            Some(NameEncodingProblem::InvalidUtf8)
        );
        assert_eq!(
            catalog.entries()[1].problems().name_encoding,
            Some(NameEncodingProblem::UnsupportedLegacyEncoding)
        );
        assert_eq!(
            catalog.entries()[2].problems().unsafe_path,
            Some(UnsafePathReason::ParentDirectoryComponent)
        );
        assert_eq!(
            catalog.entries()[3].problems().unsafe_kind,
            Some(UnsafeEntryReason::SymbolicLink)
        );
        assert!(catalog.entries()[4].problems().encrypted);
        assert_eq!(
            catalog.entries()[5].compression(),
            EntryCompression::Unsupported(12)
        );

        let mut dos_directory = Spec::file(b"dos-dir", b"");
        dos_directory.external = 0x0010;
        let mut reparse = Spec::file(b"reparse", b"");
        reparse.external = 0x0400;
        let mut unknown_flags = Spec::file(b"flags", b"x");
        unknown_flags.flags = 1 << 5;
        let mut allowed_deflate_flags = Spec::file(b"deflate-flags", b"x");
        allowed_deflate_flags.method = 8;
        allowed_deflate_flags.flags = (1 << 1) | (1 << 2) | (1 << 3) | UTF8_NAME_FLAG;
        let (bytes, payloads) = make_zip(
            &[dos_directory, reparse, unknown_flags, allowed_deflate_flags],
            b"",
        );
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, ArchiveLimits::default()).unwrap();
        assert_eq!(catalog.entries()[0].kind(), EntryKind::Directory);
        assert_eq!(
            catalog.entries()[1].problems().unsafe_kind,
            Some(UnsafeEntryReason::ReparsePoint)
        );
        assert_ne!(catalog.entries()[2].problems().unsupported_flags, 0);
        assert_eq!(catalog.entries()[3].problems().unsupported_flags, 0);
    }

    #[test]
    fn tiny_catalog_limits_stop_before_an_incomplete_listing_can_be_returned() {
        let mut spec = Spec::file(b"bounded-name.txt", b"payload");
        spec.extra = vec![0x34, 0x12, 2, 0, 9, 8];
        let (bytes, payloads) = make_zip(&[spec], b"comment");
        let mut limits = ArchiveLimits::default();
        limits.max_entries = 0;
        let mut reader = BoundedFooterProbeReader::new(bytes.clone(), payloads.clone(), limits);
        let result = read(&mut reader, limits);
        assert_eq!(
            result.as_ref().err(),
            Some(&CatalogError::LimitExceeded(CatalogLimit::EntryCount)),
            "entry-count limit should stop at the footer; {}",
            reader.stats()
        );
        reader.assert_bounded();

        limits = ArchiveLimits::default();
        limits.max_name_bytes = 2;
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads.clone());
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(CatalogLimit::EntryNameBytes))
        ));

        limits = ArchiveLimits::default();
        limits.max_extra_bytes = 2;
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads.clone());
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(CatalogLimit::EntryExtraBytes))
        ));

        limits = ArchiveLimits::default();
        limits.max_central_directory_bytes = 0;
        let mut reader = BoundedFooterProbeReader::new(bytes.clone(), payloads.clone(), limits);
        let result = read(&mut reader, limits);
        assert_eq!(
            result.as_ref().err(),
            Some(&CatalogError::LimitExceeded(
                CatalogLimit::CentralDirectoryBytes
            )),
            "central-directory byte limit should stop at the footer; {}",
            reader.stats()
        );
        reader.assert_bounded();

        limits = ArchiveLimits::default();
        limits.max_retained_metadata_bytes = 0;
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads.clone());
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(
                CatalogLimit::RetainedMetadataBytes
            ))
        ));

        limits = ArchiveLimits::default();
        limits.max_comment_bytes = 2;
        let mut reader = BoundedFooterProbeReader::new(bytes.clone(), payloads.clone(), limits);
        let result = read(&mut reader, limits);
        assert_eq!(
            result.as_ref().err(),
            Some(&CatalogError::LimitExceeded(
                CatalogLimit::ArchiveCommentBytes
            )),
            "archive-comment limit should stop at the footer; {}",
            reader.stats()
        );
        reader.assert_bounded();

        let mut entry_comment = Spec::file(b"commented.txt", b"x");
        entry_comment.comment = b"long comment".to_vec();
        let (entry_comment_bytes, entry_comment_payloads) = make_zip(&[entry_comment], b"");
        limits = ArchiveLimits::default();
        limits.max_comment_bytes = 2;
        let mut reader = RejectPayloadReader::new(entry_comment_bytes, entry_comment_payloads);
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(CatalogLimit::EntryCommentBytes))
        ));

        limits = ArchiveLimits::default();
        limits.max_eocd_scan_bytes = 8;
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads.clone());
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(
                CatalogLimit::EndRecordSearchBytes
            ))
        ));

        limits = ArchiveLimits::default();
        limits.max_eocd_candidates = 0;
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads.clone());
        assert!(matches!(
            read(&mut reader, limits),
            Err(CatalogError::LimitExceeded(
                CatalogLimit::EndRecordCandidates
            ))
        ));
    }

    #[test]
    fn impossible_central_counts_are_rejected_before_central_reads_or_entry_reservation() {
        let (valid, _) = make_zip(&[Spec::file(b"one.txt", b"x")], b"");
        let eocd = valid.len() - EOCD_FIXED_BYTES;
        let central_start = valid
            .windows(4)
            .position(|window| window == CENTRAL_HEADER_SIGNATURE.to_le_bytes())
            .unwrap();

        let mut count_exceeds_extent = valid.clone();
        count_exceeds_extent[eocd + 8..eocd + 10].copy_from_slice(&2_u16.to_le_bytes());
        count_exceeds_extent[eocd + 10..eocd + 12].copy_from_slice(&2_u16.to_le_bytes());
        let mut limits = ArchiveLimits::default();
        limits.max_retained_metadata_bytes = 0;
        let mut reader = CentralReadGuard::new(count_exceeds_extent, central_start, eocd);
        assert!(matches!(
            read_catalog(
                &mut reader,
                ArchiveRevision::new(31),
                limits,
                &ScanCancellation::new(),
            ),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory
            ))
        ));
        assert_eq!(reader.central_bytes_read, 0);
        assert_eq!(reader.forbidden_reads, 0);

        let mut short_fixed_extent = valid;
        short_fixed_extent[eocd + 12..eocd + 16].copy_from_slice(&45_u32.to_le_bytes());
        short_fixed_extent[eocd + 16..eocd + 20]
            .copy_from_slice(&((eocd - 45) as u32).to_le_bytes());
        let mut reader = CentralReadGuard::new(short_fixed_extent, eocd - 45, eocd);
        assert!(matches!(
            read_catalog(
                &mut reader,
                ArchiveRevision::new(31),
                ArchiveLimits::default(),
                &ScanCancellation::new(),
            ),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidCentralDirectory
            ))
        ));
        assert_eq!(reader.central_bytes_read, 0);
        assert_eq!(reader.forbidden_reads, 0);
    }

    #[test]
    fn each_central_fixed_header_is_range_checked_before_read() {
        let long_name = format!("d/{}", "x".repeat(44));
        let (mut bytes, _) = make_zip(&[Spec::file(long_name.as_bytes(), b"x")], b"");
        let eocd = bytes.len() - EOCD_FIXED_BYTES;
        let central_start = bytes
            .windows(4)
            .position(|window| window == CENTRAL_HEADER_SIGNATURE.to_le_bytes())
            .unwrap();
        let central_size = u32::from_le_bytes(bytes[eocd + 12..eocd + 16].try_into().unwrap());
        assert_eq!(
            central_size as usize,
            CENTRAL_HEADER_BYTES + long_name.len()
        );
        bytes[eocd + 8..eocd + 10].copy_from_slice(&2_u16.to_le_bytes());
        bytes[eocd + 10..eocd + 12].copy_from_slice(&2_u16.to_le_bytes());

        let mut reader = CentralReadGuard::new(bytes, central_start, eocd);
        assert!(matches!(
            read_catalog(
                &mut reader,
                ArchiveRevision::new(31),
                ArchiveLimits::default(),
                &ScanCancellation::new(),
            ),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::Truncated
            ))
        ));
        assert_eq!(reader.central_bytes_read, central_size as usize);
        assert_eq!(
            reader.forbidden_reads, 0,
            "the absent second fixed header is rejected without attempting its read"
        );
    }

    #[test]
    fn false_eocd_signature_inside_comment_does_not_replace_real_directory() {
        let (mut bytes, payloads) = make_zip(&[Spec::file(b"real.txt", b"body")], &[0; 30]);
        let comment_start = bytes.len() - 30;
        bytes[comment_start..comment_start + 4].copy_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        bytes[comment_start + 4..comment_start + 20].fill(0);
        bytes[comment_start + 20..comment_start + 22].copy_from_slice(&8_u16.to_le_bytes());
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, ArchiveLimits::default()).unwrap();
        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].name().as_utf8(), Some("real.txt"));
    }

    #[test]
    fn empty_eocd_lookalike_with_nonzero_offset_in_comment_falls_back_to_real_catalog() {
        let (mut bytes, payloads) = make_zip(&[Spec::file(b"real.txt", b"body")], &[0; 22]);
        let fake_eocd = bytes.len() - EOCD_FIXED_BYTES;
        let real_eocd = fake_eocd - EOCD_FIXED_BYTES;
        let central_header = bytes
            .windows(4)
            .position(|window| window == CENTRAL_HEADER_SIGNATURE.to_le_bytes())
            .expect("the real entry's central record is present");
        let real_central_size = u32::from_le_bytes(
            bytes[real_eocd + 12..real_eocd + 16]
                .try_into()
                .expect("the real EOCD has a central-directory size"),
        );

        // A complete zero-entry EOCD whose empty central directory starts at
        // its own offset is envelope-consistent, but this nonzero offset is
        // unsupported. It must not hide the actual EOCD immediately before it.
        bytes[fake_eocd..fake_eocd + 4].copy_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        bytes[fake_eocd + 4..fake_eocd + 16].fill(0);
        bytes[fake_eocd + 16..fake_eocd + 20].copy_from_slice(&(fake_eocd as u32).to_le_bytes());
        bytes[fake_eocd + 20..fake_eocd + 22].fill(0);

        let revision = ArchiveRevision::new(32);
        let mut reader = RejectPayloadReader::new(bytes.clone(), payloads);
        let catalog = read_catalog(
            &mut reader,
            revision,
            ArchiveLimits::default(),
            &ScanCancellation::new(),
        )
        .expect("the real EOCD must be retried after the unsupported comment candidate");

        assert_eq!(catalog.source().revision, revision);
        assert_eq!(catalog.source().archive_len, bytes.len() as u64);
        assert_eq!(
            catalog.source().central_directory_offset,
            central_header as u64
        );
        assert_eq!(
            catalog.source().central_directory_size,
            u64::from(real_central_size)
        );
        assert_eq!(catalog.source().central_directory_end, real_eocd as u64);
        assert_eq!(catalog.source().entry_count, 1);
        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].id(), ArchiveEntryId::new(revision, 0));
        assert_eq!(catalog.entries()[0].name().as_utf8(), Some("real.txt"));
        assert_eq!(
            catalog.entries()[0].central_header_offset(),
            central_header as u64
        );
        assert_eq!(catalog.entries()[0].uncompressed_size(), 4);
        assert!(
            reader.reads < 256,
            "cataloging remains bounded and does not read the entry payload"
        );

        // A real empty archive with a nonzero empty-directory offset and no
        // earlier valid EOCD retains the established typed rejection.
        let (mut unsupported, no_payloads) = make_zip(&[], b"");
        unsupported.splice(0..0, [0_u8; 4]);
        let unsupported_eocd = unsupported.len() - EOCD_FIXED_BYTES;
        unsupported[unsupported_eocd + 16..unsupported_eocd + 20]
            .copy_from_slice(&(unsupported_eocd as u32).to_le_bytes());
        let mut reader = RejectPayloadReader::new(unsupported, no_payloads);
        assert!(matches!(
            read(&mut reader, ArchiveLimits::default()),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::UnsupportedArchiveOffset
            ))
        ));
    }

    #[test]
    fn over_limit_signature_in_comment_does_not_hide_later_valid_empty_eocd() {
        let comment = [0_u8; 30];
        let (mut bytes, payloads) = make_zip(&[], &comment);
        let real_eocd = bytes
            .windows(4)
            .rposition(|window| window == EOCD_SIGNATURE.to_le_bytes())
            .unwrap();
        let false_eocd = real_eocd + EOCD_FIXED_BYTES;
        bytes[false_eocd..false_eocd + 4].copy_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        bytes[false_eocd + 4..false_eocd + 8].fill(0);
        bytes[false_eocd + 8..false_eocd + 10].copy_from_slice(&1_u16.to_le_bytes());
        bytes[false_eocd + 10..false_eocd + 12].copy_from_slice(&1_u16.to_le_bytes());
        bytes[false_eocd + 12..false_eocd + 16].fill(0);
        bytes[false_eocd + 16..false_eocd + 20].copy_from_slice(&(false_eocd as u32).to_le_bytes());
        bytes[false_eocd + 20..false_eocd + 22].copy_from_slice(&8_u16.to_le_bytes());

        let mut limits = ArchiveLimits::default();
        limits.max_entries = 0;
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let catalog = read(&mut reader, limits)
            .expect("the valid empty archive EOCD must win over an over-limit comment lookalike");
        assert!(catalog.entries().is_empty());
        assert_eq!(catalog.source().entry_count, 0);
    }

    #[test]
    fn malformed_extra_and_out_of_range_local_record_are_rejected() {
        let mut malformed = Spec::file(b"bad-extra", b"x");
        malformed.extra = vec![0x34, 0x12, 8, 0];
        let (bytes, payloads) = make_zip(&[malformed], b"");
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        assert!(matches!(
            read(&mut reader, ArchiveLimits::default()),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidExtraField
            ))
        ));

        let (mut bytes, payloads) = make_zip(&[Spec::file(b"bad-local", b"x")], b"");
        let central = bytes
            .windows(4)
            .position(|window| window == CENTRAL_HEADER_SIGNATURE.to_le_bytes())
            .unwrap();
        let eocd = bytes.len() - EOCD_FIXED_BYTES;
        let central_offset = u32::from_le_bytes(bytes[eocd + 16..eocd + 20].try_into().unwrap());
        bytes[central + 42..central + 46].copy_from_slice(&central_offset.to_le_bytes());
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        assert!(matches!(
            read(&mut reader, ArchiveLimits::default()),
            Err(CatalogError::Malformed(
                crate::archive::model::ArchiveFormatProblem::InvalidEntryRecord
            ))
        ));
    }

    #[test]
    fn malformed_count_and_cancellation_never_return_partial_catalogs() {
        let (mut bytes, _) = make_zip(&[Spec::file(b"one", b"x")], b"");
        let eocd = bytes
            .windows(4)
            .rposition(|w| w == EOCD_SIGNATURE.to_le_bytes())
            .unwrap();
        bytes[eocd + 10] = 0;
        bytes[eocd + 11] = 0;
        // A malformed footer may force bounded signature probes beyond the valid EOCD suffix.
        let mut reader = Cursor::new(bytes);
        assert!(matches!(
            read(&mut reader, ArchiveLimits::default()),
            Err(CatalogError::Malformed(_))
        ));

        let (bytes, payloads) = make_zip(&[Spec::file(b"one", b"x")], b"");
        let mut reader = RejectPayloadReader::new(bytes, payloads);
        let token = ScanCancellation::new();
        token.cancel();
        assert!(matches!(
            read_catalog(
                &mut reader,
                ArchiveRevision::new(31),
                ArchiveLimits::default(),
                &token
            ),
            Err(CatalogError::Cancelled)
        ));

        let (bytes, _) = make_zip(&[Spec::file(b"one", b"x")], b"");
        let central_offset = bytes
            .windows(4)
            .position(|window| window == CENTRAL_HEADER_SIGNATURE.to_le_bytes())
            .unwrap() as u64;
        let token = ScanCancellation::new();
        let mut reader = CancelOnCentralHeaderRead {
            cursor: Cursor::new(bytes),
            central_offset,
            cancellation: token.clone(),
            reads_after_cancel: 0,
        };
        assert!(matches!(
            read_catalog(
                &mut reader,
                ArchiveRevision::new(31),
                ArchiveLimits::default(),
                &token
            ),
            Err(CatalogError::Cancelled)
        ));
        assert_eq!(
            reader.reads_after_cancel, 0,
            "cancellation is observed before another metadata read"
        );
    }
}
