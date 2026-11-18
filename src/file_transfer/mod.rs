//! Exact, read-only Windows admission for transfer review.
//!
//! Snapshots carry observations, with handles reacquired before use. A vacant
//! leaf is evidence from a single anchored open, never permission to write.

mod tree;

pub use tree::{
    SourceNodeId, SourceObservation, SourceTree, SourceTreeEntry, SourceTreeError,
    inspect_source_tree,
};

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io;
use std::os::windows::{ffi::OsStrExt, fs::MetadataExt, io::AsRawHandle};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use windows::Wdk::Storage::FileSystem::FILE_CASE_SENSITIVE_INFORMATION;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO,
    FileCaseSensitiveInfo, GetFileInformationByHandleEx,
};

use crate::archive::transfer_native::{
    NativeArchiveError, TransferDirectory, is_missing_relative_leaf, split_local_absolute_path,
};
use crate::batch_rename::{
    MAX_WINDOWS_NAME_UTF16_UNITS, MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL,
    compare_paths_windows_ordinal, is_valid_windows_filename_leaf,
};
use crate::file_operations::{
    NativeFileIdentity, ScanCancellation, TransferDirectoryEntry, TransferDirectoryEntryKind,
    TransferEnumerationError, enumerate_transfer_directory, final_path_for_handle,
    has_cloud_placeholder_attributes, native_file_identity, query_replace_basic_info,
};

const MAX_DIRECTORY_LEVELS: usize = 128;
const EXTENDED_PREFIX: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferLeaf(OsString);

impl TransferLeaf {
    pub fn new(name: &OsStr) -> Result<Self, AdmissionError> {
        let units = name
            .encode_wide()
            .take(MAX_WINDOWS_NAME_UTF16_UNITS + 1)
            .count();
        if units > MAX_WINDOWS_NAME_UTF16_UNITS
            || !name.to_str().is_some_and(is_valid_windows_filename_leaf)
        {
            return Err(AdmissionError::InvalidLeaf);
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_os_str(&self) -> &OsStr {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectoryStamp {
    identity: NativeFileIdentity,
    created: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentSnapshot {
    path: Arc<Path>,
    chain: Arc<[DirectoryStamp]>,
}

impl ParentSnapshot {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> NativeFileIdentity {
        self.chain[self.chain.len() - 1].identity
    }

    pub fn ancestor_identities(&self) -> impl Iterator<Item = NativeFileIdentity> + '_ {
        self.chain.iter().map(|stamp| stamp.identity)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryMetadata {
    pub kind: EntryKind,
    pub byte_len: Option<u64>,
    pub attributes: u32,
    pub created: i64,
    pub modified: i64,
    pub changed: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntrySnapshot {
    parent: ParentSnapshot,
    leaf: TransferLeaf,
    identity: NativeFileIdentity,
    metadata: EntryMetadata,
}

impl EntrySnapshot {
    pub fn parent(&self) -> &ParentSnapshot {
        &self.parent
    }

    pub fn leaf(&self) -> &TransferLeaf {
        &self.leaf
    }

    pub fn path(&self) -> PathBuf {
        self.parent.path.join(self.leaf.as_os_str())
    }

    pub fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub fn metadata(&self) -> EntryMetadata {
        self.metadata
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetOccupancy {
    Vacant,
    Occupied(EntrySnapshot),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSnapshot {
    parent: ParentSnapshot,
    requested_leaf: TransferLeaf,
    occupancy: TargetOccupancy,
}

impl TargetSnapshot {
    pub fn parent(&self) -> &ParentSnapshot {
        &self.parent
    }

    pub fn requested_leaf(&self) -> &TransferLeaf {
        &self.requested_leaf
    }

    pub fn occupancy(&self) -> &TargetOccupancy {
        &self.occupancy
    }
}

#[derive(Debug)]
pub enum AdmissionError {
    Cancelled,
    InvalidPath,
    InvalidLeaf,
    Limit,
    ReparsePoint,
    CloudPlaceholder,
    CaseSensitiveDirectory,
    UnsupportedIdentity,
    UnsupportedEntry,
    Alias,
    NotADirectory,
    SourceMissing,
    StaleParent,
    StaleEntry,
    StaleTarget,
    Io(io::Error),
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "Transfer inspection cancelled.",
            Self::InvalidPath => "Choose an absolute local drive path.",
            Self::InvalidLeaf => "The name is outside the supported Windows filename rules.",
            Self::Limit => "The path exceeds the transfer depth or Windows path limit.",
            Self::ReparsePoint => "Transfer inspection stopped at a reparse point.",
            Self::CloudPlaceholder => "Transfer inspection stopped at a cloud placeholder.",
            Self::CaseSensitiveDirectory => "This directory uses unsupported case-sensitive names.",
            Self::UnsupportedIdentity => "The object has no supported stable local identity.",
            Self::UnsupportedEntry => {
                "The object is outside the supported file and directory types."
            }
            Self::Alias => "The resolved name differs from the selected Windows name.",
            Self::NotADirectory => "The selected parent is not a directory.",
            Self::SourceMissing => "The selected source no longer exists.",
            Self::StaleParent => "The parent or its ancestor changed. Review the selection again.",
            Self::StaleEntry => "The source changed. Review the selection again.",
            Self::StaleTarget => "The destination changed. Review the conflict again.",
            Self::Io(error) => {
                return write!(
                    f,
                    "Could not inspect the selected Windows object ({error})."
                );
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for AdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AdmissionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<NativeArchiveError> for AdmissionError {
    fn from(error: NativeArchiveError) -> Self {
        Self::Io(io::Error::new(error.kind(), error.to_string()))
    }
}

fn check_cancel(cancellation: &ScanCancellation) -> Result<(), AdmissionError> {
    if cancellation.is_cancelled() {
        return Err(AdmissionError::Cancelled);
    }
    Ok(())
}

fn checked_parts(path: &Path) -> Result<(PathBuf, Vec<OsString>), AdmissionError> {
    let units: Vec<_> = path
        .as_os_str()
        .encode_wide()
        .take(MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL + 1)
        .collect();
    let plain = units.strip_prefix(&EXTENDED_PREFIX).unwrap_or(&units);
    if plain.len().saturating_add(EXTENDED_PREFIX.len() + 1) > MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL
    {
        return Err(AdmissionError::Limit);
    }
    if plain.len() < 3 || plain[1] != u16::from(b':') || plain[2] != u16::from(b'\\') {
        return Err(AdmissionError::InvalidPath);
    }

    // Path::components normalizes interior dots and repeated separators.
    // Check the original spelling before asking it for path components.
    let remainder = &plain[3..];
    let remainder = remainder
        .strip_suffix(&[u16::from(b'\\')])
        .unwrap_or(remainder);
    if !remainder.is_empty() {
        for part in remainder.split(|unit| *unit == u16::from(b'\\')) {
            if part.is_empty() || part.iter().any(|unit| *unit == u16::from(b'/')) {
                return Err(AdmissionError::InvalidPath);
            }
            if part == [u16::from(b'.')] || part == [u16::from(b'.'); 2] {
                return Err(AdmissionError::InvalidPath);
            }
        }
    } else if plain.len() > 3 {
        return Err(AdmissionError::InvalidPath);
    }

    let (root, names) = split_local_absolute_path(path).map_err(|_| AdmissionError::InvalidPath)?;
    if names.len() > MAX_DIRECTORY_LEVELS {
        return Err(AdmissionError::Limit);
    }
    for name in &names {
        TransferLeaf::new(name)?;
    }
    Ok((root, names))
}

fn check_target_bounds(parent: &ParentSnapshot, leaf: &TransferLeaf) -> Result<(), AdmissionError> {
    let total = parent
        .path
        .join(leaf.as_os_str())
        .as_os_str()
        .encode_wide()
        .count();
    if parent.chain.len() > MAX_DIRECTORY_LEVELS
        || total.saturating_add(EXTENDED_PREFIX.len() + 1) > MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL
    {
        return Err(AdmissionError::Limit);
    }
    Ok(())
}

fn classify_attributes(attributes: u32) -> Result<EntryKind, AdmissionError> {
    if has_cloud_placeholder_attributes(attributes) {
        return Err(AdmissionError::CloudPlaceholder);
    }
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        return Err(AdmissionError::ReparsePoint);
    }
    if attributes & FILE_ATTRIBUTE_DEVICE.0 != 0 {
        return Err(AdmissionError::UnsupportedEntry);
    }
    Ok(if attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
        EntryKind::Directory
    } else {
        EntryKind::File
    })
}

fn check_identity(identity: NativeFileIdentity, volume: u64) -> Result<(), AdmissionError> {
    if identity.file_id() == [0; 16] || identity.volume_serial_number() != volume {
        return Err(AdmissionError::UnsupportedIdentity);
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    compare_paths_windows_ordinal(left, right) == Ordering::Equal
}

fn checked_final_path(file: &File, expected: &Path) -> Result<PathBuf, AdmissionError> {
    let actual = final_path_for_handle(file)?;
    if !same_path(&actual, expected) {
        return Err(AdmissionError::Alias);
    }
    Ok(actual)
}

fn check_case_policy(file: &File) -> Result<(), AdmissionError> {
    let mut info = FILE_CASE_SENSITIVE_INFORMATION::default();
    // SAFETY: file supplies a borrowed directory handle and info is the
    // documented writable structure for the case-sensitive information class.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileCaseSensitiveInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of_val(&info) as u32,
        )
    }
    .map_err(io::Error::other)?;
    if info.Flags != 0 {
        return Err(AdmissionError::CaseSensitiveDirectory);
    }
    Ok(())
}

fn directory_stamp(
    directory: &TransferDirectory,
    volume: u64,
) -> Result<DirectoryStamp, AdmissionError> {
    let identity = directory.verify_identity()?;
    check_identity(identity, volume)?;
    let info = query_replace_basic_info(directory.file())?;
    if classify_attributes(info.FileAttributes)? != EntryKind::Directory {
        return Err(AdmissionError::NotADirectory);
    }
    check_case_policy(directory.file())?;
    Ok(DirectoryStamp {
        identity,
        created: info.CreationTime,
    })
}

struct HeldParent {
    root: TransferDirectory,
    descendants: Vec<TransferDirectory>,
    snapshot: ParentSnapshot,
}

impl HeldParent {
    fn directory(&self) -> &TransferDirectory {
        self.descendants.last().unwrap_or(&self.root)
    }

    fn open(path: &Path, cancellation: &ScanCancellation) -> Result<Self, AdmissionError> {
        Self::open_with_listing(path, cancellation, false)
    }

    fn open_with_listing(
        path: &Path,
        cancellation: &ScanCancellation,
        list: bool,
    ) -> Result<Self, AdmissionError> {
        check_cancel(cancellation)?;
        let (drive, components) = checked_parts(path)?;
        let root = if list && components.is_empty() {
            TransferDirectory::open_listable_root(&drive)?
        } else {
            TransferDirectory::open_root(&drive)?
        };
        let volume = root.identity().volume_serial_number();
        let mut stamps = vec![directory_stamp(&root, volume)?];
        let mut canonical = checked_final_path(root.file(), &drive)?;
        let mut descendants = Vec::<TransferDirectory>::with_capacity(components.len());

        let final_index = components.len().saturating_sub(1);
        for (index, name) in components.into_iter().enumerate() {
            check_cancel(cancellation)?;
            let parent = descendants.last().unwrap_or(&root);
            let expected = canonical.join(&name);
            let candidate = parent.open_relative_metadata(&name)?;
            let info = query_replace_basic_info(&candidate)?;
            if classify_attributes(info.FileAttributes)? != EntryKind::Directory {
                return Err(AdmissionError::NotADirectory);
            }
            let identity = native_file_identity(&candidate)?;
            check_identity(identity, volume)?;
            let child = if list && index == final_index {
                parent.open_listable_child(&name)?
            } else {
                parent.open_child(&name)?
            };
            let stamp = directory_stamp(&child, volume)?;
            if stamp.identity != identity || stamp.created != info.CreationTime {
                return Err(AdmissionError::StaleParent);
            }
            canonical = checked_final_path(child.file(), &expected)?;
            descendants.push(child);
            stamps.push(stamp);
        }

        let held = Self {
            root,
            descendants,
            snapshot: ParentSnapshot {
                path: canonical.into(),
                chain: stamps.into(),
            },
        };
        held.check_chain(cancellation)?;
        Ok(held)
    }

    fn check_chain(&self, cancellation: &ScanCancellation) -> Result<(), AdmissionError> {
        let volume = self.snapshot.identity().volume_serial_number();
        let directories = std::iter::once(&self.root).chain(&self.descendants);
        for (directory, expected) in directories.zip(self.snapshot.chain.iter()) {
            check_cancel(cancellation)?;
            if directory_stamp(directory, volume)? != *expected {
                return Err(AdmissionError::StaleParent);
            }
        }
        checked_final_path(self.directory().file(), self.snapshot.path())?;
        check_cancel(cancellation)?;
        Ok(())
    }

    fn inspect_leaf(
        &self,
        leaf: &TransferLeaf,
        cancellation: &ScanCancellation,
    ) -> Result<TargetOccupancy, AdmissionError> {
        check_cancel(cancellation)?;
        check_target_bounds(&self.snapshot, leaf)?;
        let file = match self.directory().open_relative_metadata(leaf.as_os_str()) {
            Ok(file) => Some(file),
            Err(error) if is_missing_relative_leaf(&error) => None,
            Err(error) => return Err(error.into()),
        };
        let occupancy = if let Some(file) = &file {
            TargetOccupancy::Occupied(self.capture_entry(file, leaf)?)
        } else {
            TargetOccupancy::Vacant
        };
        self.check_chain(cancellation)?;
        Ok(occupancy)
    }

    fn capture_entry(
        &self,
        file: &File,
        requested_leaf: &TransferLeaf,
    ) -> Result<EntrySnapshot, AdmissionError> {
        let metadata = observe_entry_metadata(file)?;
        let identity = native_file_identity(file)?;
        check_identity(identity, self.snapshot.identity().volume_serial_number())?;
        let path = checked_final_path(file, &self.snapshot.path.join(requested_leaf.as_os_str()))?;
        let actual_leaf = path.file_name().ok_or(AdmissionError::Alias)?;
        Ok(EntrySnapshot {
            parent: self.snapshot.clone(),
            leaf: TransferLeaf::new(actual_leaf)?,
            identity,
            metadata,
        })
    }
}

fn observe_entry_metadata(file: &File) -> Result<EntryMetadata, AdmissionError> {
    let info = query_replace_basic_info(file)?;
    let kind = classify_attributes(info.FileAttributes)?;
    if kind == EntryKind::Directory {
        check_case_policy(file)?;
    }
    let metadata = file.metadata()?;
    if (kind == EntryKind::Directory && !metadata.is_dir())
        || (kind == EntryKind::File && !metadata.is_file())
        || metadata.file_attributes() != info.FileAttributes
        || basic_stamp(query_replace_basic_info(file)?) != basic_stamp(info)
    {
        return Err(AdmissionError::StaleEntry);
    }
    Ok(EntryMetadata {
        kind,
        byte_len: (kind == EntryKind::File).then(|| metadata.len()),
        attributes: info.FileAttributes,
        created: info.CreationTime,
        modified: info.LastWriteTime,
        changed: info.ChangeTime,
    })
}

fn basic_stamp(info: FILE_BASIC_INFO) -> (i64, i64, i64, u32) {
    (
        info.CreationTime,
        info.LastWriteTime,
        info.ChangeTime,
        info.FileAttributes,
    )
}

pub fn inspect_parent(
    path: &Path,
    cancellation: &ScanCancellation,
) -> Result<ParentSnapshot, AdmissionError> {
    Ok(HeldParent::open(path, cancellation)?.snapshot)
}

pub fn inspect_source(
    path: &Path,
    cancellation: &ScanCancellation,
) -> Result<EntrySnapshot, AdmissionError> {
    check_cancel(cancellation)?;
    let (mut root, mut components) = checked_parts(path)?;
    let leaf = TransferLeaf(components.pop().ok_or(AdmissionError::InvalidLeaf)?);
    root.extend(components);
    let held = HeldParent::open(&root, cancellation)?;
    let directory_path = path.as_os_str().encode_wide().last() == Some(u16::from(b'\\'));
    match held.inspect_leaf(&leaf, cancellation)? {
        TargetOccupancy::Occupied(entry)
            if directory_path && entry.metadata.kind != EntryKind::Directory =>
        {
            Err(AdmissionError::NotADirectory)
        }
        TargetOccupancy::Occupied(entry) => Ok(entry),
        TargetOccupancy::Vacant => Err(AdmissionError::SourceMissing),
    }
}

pub fn inspect_target(
    parent: &ParentSnapshot,
    leaf: &TransferLeaf,
    cancellation: &ScanCancellation,
) -> Result<TargetSnapshot, AdmissionError> {
    let held = HeldParent::open(parent.path(), cancellation)?;
    if held.snapshot != *parent {
        return Err(AdmissionError::StaleParent);
    }
    let occupancy = held.inspect_leaf(leaf, cancellation)?;
    Ok(TargetSnapshot {
        parent: held.snapshot,
        requested_leaf: leaf.clone(),
        occupancy,
    })
}

/// Checks the observed namespace and metadata. The read-only handles are
/// released on return. Execution must take its own held guards before effects.
pub fn recheck_target_metadata(
    expected: &TargetSnapshot,
    cancellation: &ScanCancellation,
) -> Result<(), AdmissionError> {
    if *expected != inspect_target(expected.parent(), expected.requested_leaf(), cancellation)? {
        return Err(AdmissionError::StaleTarget);
    }
    Ok(())
}

/// Checks identity, type and metadata. Concurrent writes can leave the same
/// metadata visible. Execution must obtain held guards and verify its content.
pub fn recheck_source_metadata(
    expected: &EntrySnapshot,
    cancellation: &ScanCancellation,
) -> Result<(), AdmissionError> {
    let target = inspect_target(expected.parent(), expected.leaf(), cancellation)?;
    match target.occupancy() {
        TargetOccupancy::Occupied(current) if current == expected => Ok(()),
        _ => Err(AdmissionError::StaleEntry),
    }
}

struct ChildObservation {
    name: OsString,
    entry: Result<EntrySnapshot, AdmissionError>,
}

fn inspect_listed_entry(
    listed: &TransferDirectoryEntry,
    open: impl FnOnce(&TransferLeaf) -> Result<TargetOccupancy, AdmissionError>,
) -> Result<EntrySnapshot, AdmissionError> {
    match listed.kind {
        TransferDirectoryEntryKind::CloudPlaceholder => {
            return Err(AdmissionError::CloudPlaceholder);
        }
        TransferDirectoryEntryKind::ReparsePoint => return Err(AdmissionError::ReparsePoint),
        TransferDirectoryEntryKind::Ordinary => {}
    }
    let leaf = TransferLeaf::new(&listed.name)?;
    match open(&leaf)? {
        TargetOccupancy::Occupied(entry) => Ok(entry),
        TargetOccupancy::Vacant => Err(AdmissionError::StaleEntry),
    }
}

fn inspect_children(
    source: &EntrySnapshot,
    max_entries: usize,
    max_name_bytes: usize,
    cancellation: &ScanCancellation,
) -> Result<Vec<ChildObservation>, AdmissionError> {
    check_cancel(cancellation)?;
    if source.metadata.kind != EntryKind::Directory {
        return Err(AdmissionError::NotADirectory);
    }

    let held = HeldParent::open_with_listing(&source.path(), cancellation, true)?;
    let dir = held.directory().file();
    let check_source = || {
        if held.snapshot.chain.split_last().map(|(_, chain)| chain)
            != Some(source.parent.chain.as_ref())
            || held.snapshot.identity() != source.identity
            || !same_path(held.snapshot.path(), &source.path())
            || observe_entry_metadata(dir)? != source.metadata
        {
            return Err(AdmissionError::StaleEntry);
        }
        held.check_chain(cancellation)
    };
    check_source()?;
    let mut names = enumerate_transfer_directory(
        dir,
        held.snapshot.path(),
        max_entries,
        max_name_bytes,
        || cancellation.is_cancelled(),
    )
    .map_err(|failure| match failure {
        TransferEnumerationError::Limit => AdmissionError::Limit,
        TransferEnumerationError::Cancelled => AdmissionError::Cancelled,
        TransferEnumerationError::Io(message) => AdmissionError::Io(io::Error::other(message)),
    })?;

    names.sort_by(|left, right| {
        compare_paths_windows_ordinal(Path::new(&left.name), Path::new(&right.name))
            .then_with(|| left.name.cmp(&right.name))
    });
    let mut entries = Vec::with_capacity(names.len());
    for listed in names {
        check_cancel(cancellation)?;
        let entry = inspect_listed_entry(&listed, |leaf| held.inspect_leaf(leaf, cancellation));
        match entry {
            Err(
                error @ (AdmissionError::StaleEntry
                | AdmissionError::StaleParent
                | AdmissionError::Limit
                | AdmissionError::Cancelled),
            ) => return Err(error),
            entry => entries.push(ChildObservation {
                name: listed.name,
                entry,
            }),
        }
    }
    check_source()?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn transfer_tree_skips_enumeration_only_placeholder_before_opening() {
        let listed = TransferDirectoryEntry {
            name: OsString::from("placeholder.txt"),
            kind: TransferDirectoryEntryKind::CloudPlaceholder,
        };
        let result = inspect_listed_entry(&listed, |_| panic!("placeholder was opened"));
        assert!(matches!(result, Err(AdmissionError::CloudPlaceholder)));
    }

    #[test]
    fn transfer_admission_vacant_leaf_detects_the_new_occupant() {
        let temp = tempfile::tempdir().unwrap();
        let control = ScanCancellation::new();
        let parent = inspect_parent(temp.path(), &control).unwrap();
        let leaf = TransferLeaf::new(OsStr::new("Reviewed File.txt")).unwrap();
        let before = inspect_target(&parent, &leaf, &control).unwrap();
        assert_eq!(before.occupancy(), &TargetOccupancy::Vacant);
        recheck_target_metadata(&before, &control).unwrap();
        let source = temp.path().join(leaf.as_os_str());
        fs::write(&source, b"new occupant").unwrap();

        assert!(matches!(
            recheck_target_metadata(&before, &control),
            Err(AdmissionError::StaleTarget)
        ));
        let after = inspect_target(&parent, &leaf, &control).unwrap();
        let entry = inspect_source(&source, &control).unwrap();
        assert_eq!(entry.metadata().byte_len, Some(12));
        assert_eq!(after.occupancy(), &TargetOccupancy::Occupied(entry));
    }

    #[test]
    fn transfer_admission_same_spelling_with_replaced_parent_is_stale() {
        let temp = tempfile::tempdir().unwrap();
        let selected = temp.path().join("reviewed");
        fs::create_dir(&selected).unwrap();
        let control = ScanCancellation::new();
        let parent = inspect_parent(&selected, &control).unwrap();
        let leaf = TransferLeaf::new(OsStr::new("unused.txt")).unwrap();

        fs::rename(&selected, temp.path().join("earlier")).unwrap();
        fs::create_dir(&selected).unwrap();

        let replacement = inspect_parent(&selected, &control).unwrap();
        assert_ne!(replacement.identity(), parent.identity());
        assert!(matches!(
            inspect_target(&parent, &leaf, &control),
            Err(AdmissionError::StaleParent)
        ));
    }

    #[test]
    fn transfer_admission_hardlink_and_case_collision_bind_native_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("Readme.txt");
        fs::write(&original, b"data").unwrap();
        let alias = temp.path().join("Alias.txt");
        fs::hard_link(&original, &alias).unwrap();

        let control = ScanCancellation::new();
        let left = inspect_source(&original, &control).unwrap();
        let right = inspect_source(&alias, &control).unwrap();
        assert_eq!(left.identity(), right.identity());
        assert_ne!(left.leaf(), right.leaf());
        let requested = TransferLeaf::new(OsStr::new("README.TXT")).unwrap();
        let collision = inspect_target(left.parent(), &requested, &control).unwrap();
        assert_eq!(collision.requested_leaf(), &requested);
        assert_eq!(
            collision.occupancy(),
            &TargetOccupancy::Occupied(left.clone())
        );
        recheck_source_metadata(&left, &control).unwrap();

        fs::write(&original, b"updated").unwrap();
        assert!(matches!(
            recheck_source_metadata(&left, &control),
            Err(AdmissionError::StaleEntry)
        ));
    }

    #[test]
    fn transfer_admission_rejects_normalized_paths_reserved_names_and_limits() {
        for leaf in [
            "",
            ".",
            "..",
            "CON",
            "LPT².txt",
            "a:b",
            "bad.",
            "bad ",
            "a/b",
            "a\\b",
        ] {
            assert!(matches!(
                TransferLeaf::new(OsStr::new(leaf)),
                Err(AdmissionError::InvalidLeaf)
            ));
        }
        for path in [
            r"C:\a\.\b",
            r"C:\a\..\b",
            r"\\server\share",
            r"C:a",
            r"C:\a\\b",
            r"C:/a",
        ] {
            assert!(matches!(
                checked_parts(Path::new(path)),
                Err(AdmissionError::InvalidPath)
            ));
        }
        assert!(checked_parts(Path::new(r"\\?\C:\A Folder\Résumé.txt")).is_ok());
        let mut depth = PathBuf::from(r"C:\");
        depth.extend(std::iter::repeat_n("folder", MAX_DIRECTORY_LEVELS + 1));
        assert!(matches!(checked_parts(&depth), Err(AdmissionError::Limit)));

        let mut width = PathBuf::from(r"C:\");
        let segment = "x".repeat(255);
        width.extend(std::iter::repeat_n(segment, MAX_DIRECTORY_LEVELS));
        assert!(matches!(checked_parts(&width), Err(AdmissionError::Limit)));
    }

    #[test]
    fn transfer_admission_refuses_cloud_attributes_and_respects_cancellation() {
        for attributes in [0x1000, 0x0004_0000, 0x0040_0000] {
            assert!(matches!(
                classify_attributes(attributes),
                Err(AdmissionError::CloudPlaceholder)
            ));
        }
        assert!(matches!(
            classify_attributes(FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0),
            Err(AdmissionError::ReparsePoint)
        ));

        let control = ScanCancellation::new();
        control.cancel();
        assert!(matches!(
            inspect_parent(Path::new(r"C:\"), &control),
            Err(AdmissionError::Cancelled)
        ));
    }
}
