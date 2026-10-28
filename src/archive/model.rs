//! Bounded ZIP catalog and extraction-plan domain types.
//!
//! A successful plan is only a lexical selection summary. It is not a destination
//! handle, extraction authorization, integrity check, or no-clobber publication.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::batch_rename::compare_paths_windows_ordinal;

/// Caller-owned identity for one immutable interpretation of an archive.
///
/// The caller must keep the underlying reader/revision stable while using entry
/// identities. A pathname or modification time alone is not sufficient proof.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArchiveRevision(u64);

impl ArchiveRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Physical central-directory identity. Duplicate names keep distinct ordinals.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArchiveEntryId {
    revision: ArchiveRevision,
    ordinal: u64,
}

impl ArchiveEntryId {
    pub const fn new(revision: ArchiveRevision, ordinal: u64) -> Self {
        Self { revision, ordinal }
    }

    pub const fn revision(self) -> ArchiveRevision {
        self.revision
    }

    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }
}

/// Finite bounds used by central-directory admission and pure plan construction.
///
/// These limits cover metadata and declared-size estimates only. Actual output
/// byte/count/ratio enforcement remains a future streaming extraction duty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveLimits {
    pub max_entries: usize,
    pub max_central_directory_bytes: u64,
    pub max_retained_metadata_bytes: usize,
    pub max_name_bytes: usize,
    pub max_extra_bytes: usize,
    pub max_comment_bytes: usize,
    pub max_eocd_scan_bytes: usize,
    pub max_eocd_candidates: usize,
    pub max_path_depth: usize,
    pub max_component_utf16_units: usize,
    pub max_relative_path_utf16_units: usize,
    pub max_selection_entries: usize,
    pub max_plan_nodes: usize,
    pub max_plan_path_bytes: usize,
    pub max_plan_work: usize,
    pub max_diagnostics: usize,
    pub max_occupancy_probes: usize,
    pub max_entry_declared_output_bytes: u64,
    pub max_total_declared_output_bytes: u64,
    pub max_expansion_ratio: u64,
    pub max_actual_entry_output_bytes: u64,
    pub max_actual_total_output_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 50_000,
            max_central_directory_bytes: 32 * 1024 * 1024,
            max_retained_metadata_bytes: 32 * 1024 * 1024,
            max_name_bytes: 4 * 1024,
            max_extra_bytes: 16 * 1024,
            max_comment_bytes: 16 * 1024,
            max_eocd_scan_bytes: 65_557,
            max_eocd_candidates: 8,
            max_path_depth: 128,
            max_component_utf16_units: 255,
            max_relative_path_utf16_units: 32_767,
            max_selection_entries: 50_000,
            max_plan_nodes: 100_000,
            max_plan_path_bytes: 8 * 1024 * 1024,
            max_plan_work: 8_000_000,
            max_diagnostics: 128,
            max_occupancy_probes: 100_000,
            max_entry_declared_output_bytes: 64 * 1024 * 1024 * 1024,
            max_total_declared_output_bytes: 1024 * 1024 * 1024 * 1024,
            max_expansion_ratio: 10_000,
            // Captured future execution policy; M1-A does not enforce output bytes.
            max_actual_entry_output_bytes: 2 * 1024 * 1024 * 1024,
            max_actual_total_output_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveFormat {
    Zip32,
    Zip64,
}

/// Exact end-record interpretation retained with a catalog revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveSource {
    pub revision: ArchiveRevision,
    pub archive_len: u64,
    pub central_directory_offset: u64,
    pub central_directory_size: u64,
    pub central_directory_end: u64,
    pub entry_count: u64,
    pub format: ArchiveFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryCompression {
    Stored,
    Deflated,
    Unsupported(u16),
}

impl EntryCompression {
    pub(crate) fn from_code(code: u16) -> Self {
        #[allow(deprecated)]
        match zip::CompressionMethod::from_u16(code) {
            zip::CompressionMethod::Stored => Self::Stored,
            zip::CompressionMethod::Deflated => Self::Deflated,
            _ => Self::Unsupported(code),
        }
    }

    pub const fn method_code(self) -> u16 {
        match self {
            Self::Stored => 0,
            Self::Deflated => 8,
            Self::Unsupported(code) => code,
        }
    }

    pub fn method_name(self) -> &'static str {
        zip::CompressionMethod::name_from_u16(self.method_code())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameEncodingProblem {
    InvalidUtf8,
    UnsupportedLegacyEncoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsafePathReason {
    Empty,
    Rooted,
    EmptyComponent,
    CurrentDirectoryComponent,
    ParentDirectoryComponent,
    ForbiddenCharacter,
    ControlCharacter,
    TrailingDotOrSpace,
    ReservedDeviceName,
    ComponentTooLong,
    TooDeep,
    RelativePathTooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsafeEntryReason {
    SymbolicLink,
    ReparsePoint,
    DeviceOrSpecialFile,
    UnsupportedFileKind,
}

/// Fixed-size issues preserve all relevant facts without per-entry Vec storage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EntryProblems {
    pub name_encoding: Option<NameEncodingProblem>,
    pub unsafe_path: Option<UnsafePathReason>,
    pub unsafe_kind: Option<UnsafeEntryReason>,
    pub encrypted: bool,
    pub unsupported_flags: u16,
    pub inconsistent_stored_size: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveEntryName {
    Utf8(String),
    InvalidUtf8(Vec<u8>),
    UnsupportedLegacyEncoding(Vec<u8>),
}

impl ArchiveEntryName {
    pub fn as_utf8(&self) -> Option<&str> {
        match self {
            Self::Utf8(name) => Some(name),
            Self::InvalidUtf8(_) | Self::UnsupportedLegacyEncoding(_) => None,
        }
    }

    pub fn raw_bytes(&self) -> &[u8] {
        match self {
            Self::Utf8(name) => name.as_bytes(),
            Self::InvalidUtf8(bytes) | Self::UnsupportedLegacyEncoding(bytes) => bytes,
        }
    }

    /// Bounded display-only rendering. Never use this value as an output path.
    pub fn display_lossy(&self) -> Cow<'_, str> {
        match self {
            Self::Utf8(name) => Cow::Borrowed(name),
            Self::InvalidUtf8(bytes) | Self::UnsupportedLegacyEncoding(bytes) => {
                String::from_utf8_lossy(bytes)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    pub(crate) id: ArchiveEntryId,
    pub(crate) name: ArchiveEntryName,
    pub(crate) kind: EntryKind,
    pub(crate) compression: EntryCompression,
    pub(crate) flags: u16,
    pub(crate) crc32: u32,
    pub(crate) compressed_size: u64,
    pub(crate) uncompressed_size: u64,
    pub(crate) local_header_offset: u64,
    pub(crate) central_header_offset: u64,
    pub(crate) version_made_by: u16,
    pub(crate) external_attributes: u32,
    pub(crate) problems: EntryProblems,
}

impl ArchiveEntry {
    pub fn id(&self) -> ArchiveEntryId {
        self.id
    }

    pub fn name(&self) -> &ArchiveEntryName {
        &self.name
    }

    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    pub const fn compression(&self) -> EntryCompression {
        self.compression
    }

    pub const fn flags(&self) -> u16 {
        self.flags
    }

    pub const fn crc32(&self) -> u32 {
        self.crc32
    }

    pub const fn compressed_size(&self) -> u64 {
        self.compressed_size
    }

    pub const fn uncompressed_size(&self) -> u64 {
        self.uncompressed_size
    }

    pub const fn local_header_offset(&self) -> u64 {
        self.local_header_offset
    }

    pub const fn central_header_offset(&self) -> u64 {
        self.central_header_offset
    }

    pub const fn version_made_by(&self) -> u16 {
        self.version_made_by
    }

    pub const fn external_attributes(&self) -> u32 {
        self.external_attributes
    }

    pub const fn problems(&self) -> EntryProblems {
        self.problems
    }

    pub const fn is_supported(&self) -> bool {
        matches!(
            self.compression,
            EntryCompression::Stored | EntryCompression::Deflated
        ) && !self.problems.encrypted
            && self.problems.unsupported_flags == 0
            && self.problems.name_encoding.is_none()
            && self.problems.unsafe_path.is_none()
            && self.problems.unsafe_kind.is_none()
            && !self.problems.inconsistent_stored_size
    }

    pub fn safe_relative_path<'a>(
        &'a self,
        limits: &ArchiveLimits,
    ) -> Result<SafeRelativePath<'a>, UnsafePathReason> {
        let name = self
            .name
            .as_utf8()
            .ok_or(UnsafePathReason::ForbiddenCharacter)?;
        SafeRelativePath::validate(name, self.kind, limits)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveCatalog {
    source: ArchiveSource,
    limits: ArchiveLimits,
    entries: Vec<ArchiveEntry>,
    retained_metadata_bytes: usize,
}

impl ArchiveCatalog {
    pub(crate) fn new(
        source: ArchiveSource,
        limits: ArchiveLimits,
        entries: Vec<ArchiveEntry>,
        retained_metadata_bytes: usize,
    ) -> Self {
        Self {
            source,
            limits,
            entries,
            retained_metadata_bytes,
        }
    }

    pub const fn source(&self) -> ArchiveSource {
        self.source
    }

    pub const fn limits(&self) -> &ArchiveLimits {
        &self.limits
    }

    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    pub const fn retained_metadata_bytes(&self) -> usize {
        self.retained_metadata_bytes
    }

    pub fn entry(&self, id: ArchiveEntryId) -> Option<&ArchiveEntry> {
        if id.revision != self.source.revision {
            return None;
        }
        self.entries
            .get(usize::try_from(id.ordinal).ok()?)
            .filter(|entry| entry.id == id)
    }
}

/// Borrowed view of an already-validated Windows-relative entry path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeRelativePath<'a> {
    original: &'a str,
    is_directory: bool,
    component_count: usize,
    utf16_units: usize,
}

impl<'a> SafeRelativePath<'a> {
    pub fn validate(
        path: &'a str,
        kind: EntryKind,
        limits: &ArchiveLimits,
    ) -> Result<Self, UnsafePathReason> {
        if path.is_empty() {
            return Err(UnsafePathReason::Empty);
        }
        if path.starts_with(['/', '\\']) {
            return Err(UnsafePathReason::Rooted);
        }

        let body = if kind == EntryKind::Directory && path.ends_with(['/', '\\']) {
            &path[..path.len() - 1]
        } else {
            path
        };
        if body.is_empty() {
            return Err(UnsafePathReason::Rooted);
        }

        let mut component_count = 0usize;
        let mut utf16_units = 0usize;
        for component in body.split(['/', '\\']) {
            if component.is_empty() {
                return Err(UnsafePathReason::EmptyComponent);
            }
            if component == "." {
                return Err(UnsafePathReason::CurrentDirectoryComponent);
            }
            if component == ".." {
                return Err(UnsafePathReason::ParentDirectoryComponent);
            }
            if component.ends_with(['.', ' ']) {
                return Err(UnsafePathReason::TrailingDotOrSpace);
            }
            if component.chars().any(char::is_control) {
                return Err(UnsafePathReason::ControlCharacter);
            }
            if component
                .chars()
                .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
            {
                return Err(UnsafePathReason::ForbiddenCharacter);
            }
            if is_reserved_device_component(component) {
                return Err(UnsafePathReason::ReservedDeviceName);
            }

            let component_units = component.encode_utf16().count();
            if component_units > limits.max_component_utf16_units {
                return Err(UnsafePathReason::ComponentTooLong);
            }
            component_count = component_count
                .checked_add(1)
                .ok_or(UnsafePathReason::TooDeep)?;
            if component_count > limits.max_path_depth {
                return Err(UnsafePathReason::TooDeep);
            }
            let separator_units = usize::from(component_count > 1);
            utf16_units = utf16_units
                .checked_add(separator_units)
                .and_then(|sum| sum.checked_add(component_units))
                .ok_or(UnsafePathReason::RelativePathTooLong)?;
            if utf16_units > limits.max_relative_path_utf16_units {
                return Err(UnsafePathReason::RelativePathTooLong);
            }
        }

        Ok(Self {
            original: body,
            is_directory: kind == EntryKind::Directory,
            component_count,
            utf16_units,
        })
    }

    pub fn as_str(self) -> &'a str {
        self.original
    }

    pub const fn is_directory(self) -> bool {
        self.is_directory
    }

    pub const fn component_count(self) -> usize {
        self.component_count
    }

    pub const fn utf16_units(self) -> usize {
        self.utf16_units
    }

    pub fn components(self) -> impl Iterator<Item = &'a str> {
        self.original.split(['/', '\\'])
    }
}

/// Owned safe path component string reserved for future confined extraction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedRelativePath {
    value: String,
    kind: EntryKind,
}

impl ValidatedRelativePath {
    pub(crate) fn from_safe(
        safe: SafeRelativePath<'_>,
        limits: &ArchiveLimits,
    ) -> Result<Self, PathCopyError> {
        let mut value = String::new();
        value
            .try_reserve_exact(safe.as_str().len())
            .map_err(|_| PathCopyError::Allocation)?;
        value.push_str(safe.as_str());
        if value.len() > limits.max_plan_path_bytes {
            return Err(PathCopyError::Limit);
        }
        Ok(Self {
            value,
            kind: if safe.is_directory() {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
        })
    }

    pub(crate) fn from_parent_prefix(
        value: &str,
        limits: &ArchiveLimits,
    ) -> Result<Self, PathCopyError> {
        let safe = SafeRelativePath::validate(value, EntryKind::Directory, limits)
            .map_err(PathCopyError::Unsafe)?;
        Self::from_safe(safe, limits)
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub(crate) fn retained_capacity(&self) -> usize {
        self.value.capacity()
    }

    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.value.split(['/', '\\'])
    }

    pub(crate) fn set_kind(&mut self, kind: EntryKind) {
        self.kind = kind;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathCopyError {
    Allocation,
    Limit,
    Unsafe(UnsafePathReason),
}

pub(crate) fn windows_component_cmp(left: &str, right: &str) -> Ordering {
    compare_paths_windows_ordinal(Path::new(left), Path::new(right))
}

pub(crate) fn windows_relative_cmp(left: &str, right: &str) -> Ordering {
    let mut left_parts = left.split(['/', '\\']);
    let mut right_parts = right.split(['/', '\\']);
    loop {
        match (left_parts.next(), right_parts.next()) {
            (Some(left), Some(right)) => {
                let order = windows_component_cmp(left, right);
                if order != Ordering::Equal {
                    return order;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

pub(crate) fn raw_name_is_descendant(raw_name: &[u8], parent: &str) -> bool {
    let raw_parts = raw_name.split(|byte| *byte == b'/' || *byte == b'\\');
    let parent_parts = parent.split(['/', '\\']);
    let mut raw_iter = raw_parts;
    let mut parent_iter = parent_parts;
    loop {
        match (raw_iter.next(), parent_iter.next()) {
            (Some(raw), Some(parent)) => {
                let Ok(raw) = std::str::from_utf8(raw) else {
                    return false;
                };
                if windows_component_cmp(raw, parent) != Ordering::Equal {
                    return false;
                }
            }
            (Some(_), None) => return true,
            _ => return false,
        }
    }
}

fn is_reserved_device_component(component: &str) -> bool {
    let base = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.']);
    let upper = base.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || is_port_device_alias(base)
}

fn is_port_device_alias(base: &str) -> bool {
    let mut chars = base.chars();
    let first = chars.next().map(|ch| ch.to_ascii_uppercase());
    let second = chars.next().map(|ch| ch.to_ascii_uppercase());
    let family = match (first, second) {
        (Some('C'), Some('O')) => chars.next().map(|ch| ch.to_ascii_uppercase()) == Some('M'),
        (Some('L'), Some('P')) => chars.next().map(|ch| ch.to_ascii_uppercase()) == Some('T'),
        _ => false,
    };
    let digit = chars.next();
    let no_suffix = chars.next().is_none();
    family && matches!(digit, Some('1'..='9' | '¹' | '²' | '³')) && no_suffix
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ArchiveFormatProblem {
    Truncated,
    MissingEndRecord,
    AmbiguousEndRecord,
    MultiDiskArchive,
    InvalidZip64Envelope,
    InvalidCentralDirectory,
    InvalidEntryRecord,
    InvalidExtraField,
    UnsupportedArchiveOffset,
    InconsistentEndRecords,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CatalogLimit {
    EndRecordSearchBytes,
    EndRecordCandidates,
    CentralDirectoryBytes,
    EntryCount,
    RetainedMetadataBytes,
    EntryNameBytes,
    EntryExtraBytes,
    EntryCommentBytes,
    ArchiveCommentBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogError {
    Cancelled,
    Allocation,
    LimitExceeded(CatalogLimit),
    Malformed(ArchiveFormatProblem),
    Io(std::io::ErrorKind),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("archive cataloging was cancelled"),
            Self::Allocation => f.write_str("archive catalog memory could not be reserved"),
            Self::LimitExceeded(limit) => write!(f, "archive catalog exceeded {limit:?}"),
            Self::Malformed(problem) => write!(f, "malformed ZIP archive: {problem:?}"),
            Self::Io(kind) => write!(f, "archive metadata read failed: {kind:?}"),
        }
    }
}

impl Error for CatalogError {}
#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ArchiveLimits {
        ArchiveLimits::default()
    }

    #[test]
    fn windows_relative_path_rejects_traversal_roots_streams_and_forbidden_names() {
        let attacks = [
            "../x",
            "..\\x",
            "a/../x",
            "/x",
            "\\x",
            "C:/x",
            "C:x",
            "\\\\server\\share",
            "\\\\?\\C:\\x",
            "\\\\.\\NUL",
            "a:b",
            "bad?.txt",
            "bad*.txt",
            "bad|name",
            "bad\"name",
            "bad<name",
            "bad>name",
            "trailing.",
            "trailing ",
            "CON",
            "CONIN$",
            "CONOUT$",
            "con.txt",
            "PRN.anything",
            "AUX.ext",
            "NUL .txt",
            "COM1.txt",
            "LPT9",
            "COM¹.log",
            "LPT².txt",
            "COM³.data",
            "a//b",
            "a\\\\b",
            "",
        ];
        for path in attacks {
            assert!(
                SafeRelativePath::validate(path, EntryKind::File, &limits()).is_err(),
                "accepted unsafe path {path:?}"
            );
        }
        assert_eq!(
            SafeRelativePath::validate("a\0b", EntryKind::File, &limits()),
            Err(UnsafePathReason::ControlCharacter)
        );
        assert_eq!(
            SafeRelativePath::validate("line\nbreak", EntryKind::File, &limits()),
            Err(UnsafePathReason::ControlCharacter)
        );
    }

    #[test]
    fn windows_relative_path_preserves_safe_unicode_and_directory_separator() {
        let safe = SafeRelativePath::validate(".hidden/資料/é.txt", EntryKind::File, &limits())
            .expect("safe non-ASCII relative names should be retained");
        assert_eq!(
            safe.components().collect::<Vec<_>>(),
            [".hidden", "資料", "é.txt"]
        );
        assert_eq!(safe.as_str(), ".hidden/資料/é.txt");

        let directory =
            SafeRelativePath::validate("safe\\folder\\", EntryKind::Directory, &limits())
                .expect("one final directory separator is allowed");
        assert_eq!(
            directory.components().collect::<Vec<_>>(),
            ["safe", "folder"]
        );
        assert!(directory.is_directory());
    }

    #[test]
    fn relative_path_enforces_component_depth_and_combined_utf16_bounds() {
        let mut limits = limits();
        limits.max_component_utf16_units = 3;
        assert!(SafeRelativePath::validate("abc", EntryKind::File, &limits).is_ok());
        assert_eq!(
            SafeRelativePath::validate("four", EntryKind::File, &limits),
            Err(UnsafePathReason::ComponentTooLong)
        );

        limits = ArchiveLimits::default();
        limits.max_path_depth = 2;
        assert_eq!(
            SafeRelativePath::validate("a/b/c", EntryKind::File, &limits),
            Err(UnsafePathReason::TooDeep)
        );

        limits = ArchiveLimits::default();
        limits.max_relative_path_utf16_units = 4;
        assert_eq!(
            SafeRelativePath::validate("ab/cd", EntryKind::File, &limits),
            Err(UnsafePathReason::RelativePathTooLong)
        );

        limits.max_relative_path_utf16_units = 3;
        assert_eq!(
            SafeRelativePath::validate("a/😀", EntryKind::File, &limits),
            Err(UnsafePathReason::RelativePathTooLong)
        );
    }
}
