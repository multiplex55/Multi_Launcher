//! Windows handle-bound source and destination primitives for archive work.
//!
//! The private guards establish held source/destination identities, no-follow
//! traversal, owned staging, and same-parent no-replace publication for the
//! bounded Stored-entry writer. No extraction UI or command is exposed here.

#![allow(dead_code)] // Native operations remain private until workflow activation.

use super::native_io::{ArchiveIoControl, OverlappedArchiveReader, VerifiedOverlappedFile};
use super::plan::DestinationOccupancy;
use super::{
    ArchiveCatalog, ArchiveLimits, ArchiveRevision, ArchiveSource, CatalogError,
    ValidatedRelativePath,
};
use crate::file_operations::{NativeFileIdentity, ScanCancellation, native_file_identity};
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL: usize = 32_767;
const STAGE_ATTEMPTS: usize = 64;
static NEXT_STAGE_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
const NATIVE_EVIDENCE_LIMIT: usize = 64;

#[cfg(test)]
#[derive(Clone, Debug)]
struct NativeEvidence {
    operation: &'static str,
    nt_status: u32,
    win32_code: u32,
    kind: io::ErrorKind,
    held_identity: Option<NativeFileIdentity>,
}

#[cfg(test)]
thread_local! {
    static NATIVE_EVIDENCE: std::cell::RefCell<(Vec<NativeEvidence>, usize)> =
        const { std::cell::RefCell::new((Vec::new(), 0)) };
}

#[cfg(test)]
fn record_native_evidence(
    operation: &'static str,
    status: windows::Win32::Foundation::NTSTATUS,
    held_identity: Option<NativeFileIdentity>,
) {
    let win32_code = unsafe { windows::Win32::Foundation::RtlNtStatusToDosError(status) };
    let record = NativeEvidence {
        operation,
        nt_status: status.0 as u32,
        win32_code,
        kind: io::Error::from_raw_os_error(win32_code as i32).kind(),
        held_identity,
    };
    NATIVE_EVIDENCE.with(|evidence| {
        let mut evidence = evidence.borrow_mut();
        if evidence.0.len() < NATIVE_EVIDENCE_LIMIT {
            evidence.0.push(record);
        } else {
            evidence.1 += 1;
        }
    });
}

#[cfg(test)]
fn take_native_evidence() -> (Vec<NativeEvidence>, usize) {
    NATIVE_EVIDENCE.with(|evidence| {
        let mut evidence = evidence.borrow_mut();
        (
            std::mem::take(&mut evidence.0),
            std::mem::take(&mut evidence.1),
        )
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeFailure {
    operation: &'static str,
    nt_status: Option<i32>,
    win32_code: Option<u32>,
    kind: io::ErrorKind,
    held_identity: Option<NativeFileIdentity>,
}

impl NativeFailure {
    fn validation(
        operation: &'static str,
        kind: io::ErrorKind,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            operation,
            nt_status: None,
            win32_code: None,
            kind,
            held_identity,
        }
    }

    fn from_io(
        operation: &'static str,
        error: &io::Error,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            operation,
            nt_status: None,
            win32_code: error
                .raw_os_error()
                .and_then(|code| u32::try_from(code).ok()),
            kind: error.kind(),
            held_identity,
        }
    }

    fn from_win32(
        operation: &'static str,
        error: windows::core::Error,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        let code = win32_code(error);
        Self {
            operation,
            nt_status: None,
            win32_code: Some(code),
            kind: io::Error::from_raw_os_error(code as i32).kind(),
            held_identity,
        }
    }

    fn from_ntstatus(
        operation: &'static str,
        status: windows::Win32::Foundation::NTSTATUS,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        // Keep the NTSTATUS before converting it.  This distinction is
        // important for race tests where Win32's broad ErrorKind is not enough.
        let code = unsafe { windows::Win32::Foundation::RtlNtStatusToDosError(status) };
        Self {
            operation,
            nt_status: Some(status.0),
            win32_code: Some(code),
            kind: io::Error::from_raw_os_error(code as i32).kind(),
            held_identity,
        }
    }
}

#[derive(Debug)]
pub(crate) struct NativeArchiveError {
    primary: NativeFailure,
    cleanup_attempted: bool,
    cleanup_failure: Option<Box<NativeFailure>>,
    cleanup_failed: bool,
    unconfirmed_name_effect: bool,
}

impl NativeArchiveError {
    pub(crate) fn kind(&self) -> io::ErrorKind {
        self.primary.kind
    }

    pub(crate) fn cleanup_failed(&self) -> bool {
        self.cleanup_failed
    }
    pub(crate) fn unconfirmed_name_effect(&self) -> bool {
        self.unconfirmed_name_effect
    }

    fn validation(
        operation: &'static str,
        kind: io::ErrorKind,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            primary: NativeFailure::validation(operation, kind, held_identity),
            cleanup_attempted: false,
            cleanup_failure: None,
            cleanup_failed: false,
            unconfirmed_name_effect: false,
        }
    }

    fn from_io(
        operation: &'static str,
        error: io::Error,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            primary: NativeFailure::from_io(operation, &error, held_identity),
            cleanup_attempted: false,
            cleanup_failure: None,
            cleanup_failed: false,
            unconfirmed_name_effect: false,
        }
    }

    fn from_win32(
        operation: &'static str,
        error: windows::core::Error,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            primary: NativeFailure::from_win32(operation, error, held_identity),
            cleanup_attempted: false,
            cleanup_failure: None,
            cleanup_failed: false,
            unconfirmed_name_effect: false,
        }
    }

    fn from_ntstatus(
        operation: &'static str,
        status: windows::Win32::Foundation::NTSTATUS,
        held_identity: Option<NativeFileIdentity>,
    ) -> Self {
        Self {
            primary: NativeFailure::from_ntstatus(operation, status, held_identity),
            cleanup_attempted: false,
            cleanup_failure: None,
            cleanup_failed: false,
            unconfirmed_name_effect: false,
        }
    }

    fn with_unconfirmed_name_effect(mut self) -> Self {
        self.unconfirmed_name_effect = true;
        self
    }

    fn with_cleanup(mut self, cleanup: Result<(), NativeFailure>) -> Self {
        self.cleanup_attempted = true;
        self.cleanup_failure = cleanup.err().map(Box::new);
        self.cleanup_failed = self.cleanup_failure.is_some();
        self
    }

    fn after_cleanup(primary: NativeFailure, cleanup: Result<(), NativeFailure>) -> Self {
        let cleanup_failure = cleanup.err().map(Box::new);
        Self {
            primary,
            cleanup_attempted: true,
            cleanup_failed: cleanup_failure.is_some(),
            cleanup_failure,
            unconfirmed_name_effect: false,
        }
    }

    fn from_failed_cleanup(failure: NativeFailure) -> Self {
        Self {
            primary: failure,
            cleanup_attempted: true,
            cleanup_failure: None,
            cleanup_failed: true,
            unconfirmed_name_effect: false,
        }
    }

    #[cfg(test)]
    fn primary(&self) -> &NativeFailure {
        &self.primary
    }
}

impl fmt::Display for NativeArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_failure(formatter, &self.primary)?;
        if self.cleanup_attempted {
            match &self.cleanup_failure {
                Some(failure) => {
                    formatter.write_str("; owned-stage cleanup failed: ")?;
                    write_failure(formatter, failure)?;
                }
                None if self.cleanup_failed => {
                    formatter.write_str("; owned-stage cleanup failed")?
                }
                None => formatter.write_str("; owned-stage cleanup completed")?,
            }
        }
        Ok(())
    }
}

impl Error for NativeArchiveError {}

fn write_failure(formatter: &mut fmt::Formatter<'_>, failure: &NativeFailure) -> fmt::Result {
    write!(
        formatter,
        "{} failed (NTSTATUS={:?}, Win32={:?}, kind={:?}, held_identity={:?})",
        failure.operation,
        failure.nt_status.map(|status| status as u32),
        failure.win32_code,
        failure.kind,
        failure.held_identity
    )
}

fn win32_code(error: windows::core::Error) -> u32 {
    let code = error.code().0 as u32;
    if code & 0xffff_0000 == 0x8007_0000 {
        code & 0xffff
    } else {
        code
    }
}

struct HeldDirectory {
    file: File,
    identity: NativeFileIdentity,
    parent: Option<Arc<HeldDirectory>>,
    name: OsString,
}

impl HeldDirectory {
    fn verify_identity(&self) -> Result<NativeFileIdentity, NativeArchiveError> {
        let current = native_file_identity(&self.file).map_err(|error| {
            NativeArchiveError::from_io("FILE_ID_INFO(directory)", error, Some(self.identity))
        })?;
        if current != self.identity {
            return Err(NativeArchiveError::validation(
                "held directory identity changed",
                io::ErrorKind::InvalidData,
                Some(current),
            ));
        }
        Ok(current)
    }

    fn open_child(self: &Arc<Self>, component: &OsStr, require_add_file: bool) -> Result<Arc<Self>, NativeArchiveError> {
        self.open_child_with_access(component, require_add_file, require_add_file)
    }

    fn open_child_with_access(
        self: &Arc<Self>, component: &OsStr, require_add_file: bool, require_add_subdirectory: bool,
    ) -> Result<Arc<Self>, NativeArchiveError> {
        self.open_child_with_options(component, require_add_file, require_add_subdirectory, false, false, false)
    }

    fn open_child_with_options(
        self: &Arc<Self>, component: &OsStr, require_add_file: bool,
        require_add_subdirectory: bool, no_recall: bool, list: bool, write_attributes: bool,
    ) -> Result<Arc<Self>, NativeArchiveError> {
        validate_single_component(component, "relative directory component")?;
        let access = windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
            | windows::Win32::Storage::FileSystem::FILE_TRAVERSE
            | windows::Win32::Storage::FileSystem::SYNCHRONIZE
            | if write_attributes {
                windows::Win32::Storage::FileSystem::FILE_WRITE_ATTRIBUTES
            } else { windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0) }
            | if list {
                windows::Win32::Storage::FileSystem::FILE_LIST_DIRECTORY
            } else { windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0) }
            | if require_add_file {
                windows::Win32::Storage::FileSystem::FILE_ADD_FILE
            } else { windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0) }
            | if require_add_subdirectory { windows::Win32::Storage::FileSystem::FILE_ADD_SUBDIRECTORY } else { windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0) };
        let (file, _) = open_relative(
            self,
            component,
            access,
            shared_directory_access(),
            windows::Wdk::Storage::FileSystem::FILE_OPEN,
            // FILE_DIRECTORY_FILE cannot be combined with FILE_OPEN_NO_RECALL.
            // The transfer open validates the returned directory handle below.
            (if no_recall {
                windows::Wdk::Storage::FileSystem::FILE_OPEN_NO_RECALL
            } else {
                windows::Wdk::Storage::FileSystem::FILE_DIRECTORY_FILE
            }) | windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT
                | windows::Wdk::Storage::FileSystem::FILE_SYNCHRONOUS_IO_NONALERT,
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
            "NtCreateFile(FILE_OPEN child directory)",
            || Ok(()),
        )?;
        let metadata = file.metadata().map_err(|error| {
            NativeArchiveError::from_io(
                "File::metadata(child directory)",
                error,
                Some(self.identity),
            )
        })?;
        let attributes = metadata.file_attributes();
        if attributes & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(NativeArchiveError::validation(
                "destination or source component is a reparse point",
                io::ErrorKind::InvalidData,
                native_file_identity(&file).ok(),
            ));
        }
        if !metadata.is_dir() {
            return Err(NativeArchiveError::validation(
                "relative component is not a directory",
                io::ErrorKind::InvalidData,
                native_file_identity(&file).ok(),
            ));
        }
        let identity = native_file_identity(&file).map_err(|error| {
            NativeArchiveError::from_io("FILE_ID_INFO(child directory)", error, Some(self.identity))
        })?;
        if identity.volume_serial_number() != self.identity.volume_serial_number() {
            return Err(NativeArchiveError::validation(
                "relative directory crossed the held local volume",
                io::ErrorKind::InvalidData,
                Some(identity),
            ));
        }
        Ok(Arc::new(Self {
            file,
            identity,
            parent: Some(self.clone()),
            name: component.to_os_string(),
        }))
    }

    fn create_child(self: &Arc<Self>, component: &OsStr) -> Result<Arc<Self>, NativeArchiveError> {
        self.create_child_observed(component, || {})
    }

    fn create_child_observed(
        self: &Arc<Self>,
        component: &OsStr,
        on_created: impl FnOnce(),
    ) -> Result<Arc<Self>, NativeArchiveError> {
        validate_single_component(component, "relative directory component")?;
        self.verify_identity()?;
        let (file, information) = open_relative(
            self,
            component,
            windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
                | windows::Win32::Storage::FileSystem::FILE_TRAVERSE
                | windows::Win32::Storage::FileSystem::FILE_ADD_FILE
                | windows::Win32::Storage::FileSystem::FILE_ADD_SUBDIRECTORY
                | windows::Win32::Storage::FileSystem::SYNCHRONIZE,
            shared_directory_access(),
            windows::Wdk::Storage::FileSystem::FILE_CREATE,
            windows::Wdk::Storage::FileSystem::FILE_DIRECTORY_FILE
                | windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT
                | windows::Wdk::Storage::FileSystem::FILE_SYNCHRONOUS_IO_NONALERT,
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_DIRECTORY,
            "NtCreateFile(FILE_CREATE output directory)",
            || Ok(()),
        )?;
        if information != 2 {
            return Err(NativeArchiveError::validation(
                "NtCreateFile did not report FILE_CREATED for output directory",
                io::ErrorKind::InvalidData,
                Some(self.identity),
            ).with_unconfirmed_name_effect());
        }
        on_created();
        let metadata = file.metadata().map_err(|error| {
            NativeArchiveError::from_io(
                "File::metadata(new output directory)",
                error,
                Some(self.identity),
            )
        })?;
        if !metadata.is_dir()
            || metadata.file_attributes()
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        {
            return Err(NativeArchiveError::validation(
                "new output directory is not an ordinary directory",
                io::ErrorKind::InvalidData,
                native_file_identity(&file).ok(),
            ));
        }
        let identity = native_file_identity(&file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(new output directory)",
                error,
                Some(self.identity),
            )
        })?;
        if identity.volume_serial_number() != self.identity.volume_serial_number() {
            return Err(NativeArchiveError::validation(
                "new output directory crossed the held local volume",
                io::ErrorKind::InvalidData,
                Some(identity),
            ));
        }
        Ok(Arc::new(Self {
            file,
            identity,
            parent: Some(self.clone()),
            name: component.to_os_string(),
        }))
    }
}

fn ensure_child_directory(
    parent: &Arc<HeldDirectory>,
    component: &OsStr,
) -> Result<(Arc<HeldDirectory>, bool), NativeArchiveError> {
    match parent.open_child(component, true) {
        Ok(directory) => Ok((directory, false)),
        Err(error) if is_missing_relative_component(&error) => {
            match parent.create_child(component) {
                Ok(directory) => Ok((directory, true)),
                Err(error) if is_name_collision(&error) => parent
                    .open_child(component, true)
                    .map(|directory| (directory, false)),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn is_missing_relative_component(error: &NativeArchiveError) -> bool {
    matches!(error.primary.win32_code, Some(2 | 3))
        || error.primary.nt_status.is_some_and(|status| {
            status == 0xc000_0034_u32 as i32 || status == 0xc000_003a_u32 as i32
        })
}

fn is_name_collision(error: &NativeArchiveError) -> bool {
    matches!(error.primary.win32_code, Some(80 | 183))
        || error.primary.nt_status == Some(0xc000_0035_u32 as i32)
}

/// A selected destination opened component-by-component without following any
/// reparse point. Its held directory chain is used for every later operation.
pub(crate) struct NativeArchiveDestination {
    root: Arc<HeldDirectory>,
}

/// A reviewed output root. A missing final child is held by its verified parent
/// and is created exclusively only after the extraction is confirmed.
pub(crate) enum NativeArchiveTarget {
    Existing(NativeArchiveDestination),
    Vacant {
        parent: NativeDirectoryHandle,
        leaf: OsString,
    },
}

impl From<NativeArchiveDestination> for NativeArchiveTarget {
    fn from(destination: NativeArchiveDestination) -> Self {
        Self::Existing(destination)
    }
}

impl NativeArchiveTarget {
    pub(crate) fn open(path: &Path) -> Result<Self, NativeArchiveError> {
        match NativeArchiveDestination::open(path) {
            Ok(destination) => return Ok(Self::Existing(destination)),
            Err(error) if is_missing_relative_component(&error) => {}
            Err(error) => return Err(error),
        }
        let parent_path = path.parent().ok_or_else(|| {
            NativeArchiveError::validation("destination has no parent", io::ErrorKind::InvalidInput, None)
        })?;
        let leaf = path.file_name().ok_or_else(|| {
            NativeArchiveError::validation("destination has no final component", io::ErrorKind::InvalidInput, None)
        })?.to_os_string();
        let leaf_text = leaf.to_str().ok_or_else(|| {
            NativeArchiveError::validation("destination name is not valid Unicode", io::ErrorKind::InvalidInput, None)
        })?;
        let safe = super::SafeRelativePath::validate(
            leaf_text,
            super::EntryKind::File,
            &ArchiveLimits::default(),
        ).map_err(|_| {
            NativeArchiveError::validation("invalid destination name", io::ErrorKind::InvalidInput, None)
        })?;
        if safe.component_count() != 1 {
            return Err(NativeArchiveError::validation(
                "destination must be a single child",
                io::ErrorKind::InvalidInput,
                None,
            ));
        }
        let parent = NativeDirectoryHandle {
            directory: open_directory_path(parent_path, true)?,
        };
        if parent.probe_leaf_occupancy(&leaf)?.is_some() {
            return Err(NativeArchiveError::validation(
                "destination appeared during admission",
                io::ErrorKind::AlreadyExists,
                Some(parent.identity()),
            ));
        }
        Ok(Self::Vacant { parent, leaf })
    }

    pub(crate) fn verify_identity(&self) -> Result<NativeFileIdentity, NativeArchiveError> {
        match self {
            Self::Existing(destination) => destination.verify_identity(),
            Self::Vacant { parent, leaf } => {
                let identity = parent.verify_identity()?;
                if parent.probe_leaf_occupancy(leaf)?.is_some() {
                    return Err(NativeArchiveError::validation(
                        "proposed destination is no longer vacant",
                        io::ErrorKind::AlreadyExists,
                        Some(identity),
                    ));
                }
                Ok(identity)
            }
        }
    }

    pub(crate) fn probe_occupancy(
        &self,
        path: &ValidatedRelativePath,
    ) -> Result<DestinationOccupancy, io::ErrorKind> {
        match self {
            Self::Existing(destination) => destination.probe_occupancy(path),
            Self::Vacant { .. } => Ok(DestinationOccupancy::Vacant),
        }
    }

    pub(crate) fn materialize(
        self,
        created_root: &mut bool,
    ) -> Result<NativeArchiveDestination, NativeArchiveError> {
        match self {
            Self::Existing(destination) => {
                destination.verify_identity()?;
                Ok(destination)
            }
            Self::Vacant { parent, leaf } => {
                let root = parent.directory.create_child_observed(&leaf, || *created_root = true)?;
                Ok(NativeArchiveDestination { root })
            }
        }
    }
}

impl NativeArchiveDestination {
    pub(crate) fn open(path: &Path) -> Result<Self, NativeArchiveError> {
        let root = open_directory_path(path, true)?;
        Ok(Self { root })
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.root.identity
    }

    pub(crate) fn contains_ancestor(&self, identity: NativeFileIdentity) -> bool {
        let mut current = Some(self.root.as_ref());
        while let Some(directory) = current {
            if directory.identity == identity {
                return true;
            }
            current = directory.parent.as_deref();
        }
        false
    }

    pub(crate) fn verify_identity(&self) -> Result<NativeFileIdentity, NativeArchiveError> {
        self.root.verify_identity()
    }

    pub(crate) fn probe_occupancy(
        &self,
        path: &ValidatedRelativePath,
    ) -> Result<DestinationOccupancy, io::ErrorKind> {
        let (parent, leaf) = match self.open_parent_for(path) {
            Ok(parent) => parent,
            Err(error) if is_missing_relative_component(&error) => {
                return Ok(DestinationOccupancy::Vacant);
            }
            Err(error) => return Err(error.kind()),
        };
        parent
            .probe_leaf_occupancy(&leaf)
            .map(|occupancy| occupancy.unwrap_or(DestinationOccupancy::Vacant))
            .map_err(|error| error.kind())
    }

    pub(crate) fn open_descendant(
        &self,
        components: &[&str],
    ) -> Result<NativeDirectoryHandle, NativeArchiveError> {
        let mut current = self.root.clone();
        for component in components {
            current = current.open_child(OsStr::new(component), true)?;
        }
        Ok(NativeDirectoryHandle { directory: current })
    }

    pub(crate) fn open_parent_for(
        &self,
        path: &ValidatedRelativePath,
    ) -> Result<(NativeDirectoryHandle, OsString), NativeArchiveError> {
        let mut components = path.components().peekable();
        let mut current = self.root.clone();
        let mut leaf = None;
        while let Some(component) = components.next() {
            if components.peek().is_some() {
                current = current.open_child(OsStr::new(component), true)?;
            } else {
                leaf = Some(OsString::from(component));
            }
        }
        let leaf = leaf.ok_or_else(|| {
            NativeArchiveError::validation(
                "validated output path has no final component",
                io::ErrorKind::InvalidInput,
                Some(current.identity),
            )
        })?;
        validate_single_component(&leaf, "validated output leaf")?;
        Ok((NativeDirectoryHandle { directory: current }, leaf))
    }

    pub(crate) fn open_parent_for_create(
        &self,
        path: &ValidatedRelativePath,
    ) -> Result<(NativeDirectoryHandle, OsString, usize), NativeArchiveError> {
        let mut components = path.components().peekable();
        let mut current = self.root.clone();
        let mut created = 0_usize;
        let mut leaf = None;
        while let Some(component) = components.next() {
            if components.peek().is_some() {
                let (directory, was_created) =
                    ensure_child_directory(&current, OsStr::new(component))?;
                current = directory;
                created += usize::from(was_created);
            } else {
                leaf = Some(OsString::from(component));
            }
        }
        let leaf = leaf.ok_or_else(|| {
            NativeArchiveError::validation(
                "validated output path has no final component",
                io::ErrorKind::InvalidInput,
                Some(current.identity),
            )
        })?;
        validate_single_component(&leaf, "validated output leaf")?;
        Ok((NativeDirectoryHandle { directory: current }, leaf, created))
    }

    pub(crate) fn ensure_directory_path(
        &self,
        path: &ValidatedRelativePath,
    ) -> Result<(NativeDirectoryHandle, usize), NativeArchiveError> {
        let mut current = self.root.clone();
        let mut created = 0_usize;
        for component in path.components() {
            let (directory, was_created) = ensure_child_directory(&current, OsStr::new(component))?;
            current = directory;
            created += usize::from(was_created);
        }
        Ok((NativeDirectoryHandle { directory: current }, created))
    }
}

#[derive(Clone)]
pub(crate) struct NativeDirectoryHandle {
    directory: Arc<HeldDirectory>,
}

impl NativeDirectoryHandle {
    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.directory.identity
    }

    pub(crate) fn verify_identity(&self) -> Result<NativeFileIdentity, NativeArchiveError> {
        self.directory.verify_identity()
    }

    pub(crate) fn open_child(&self, component: &str) -> Result<Self, NativeArchiveError> {
        Ok(Self {
            directory: self.directory.open_child(OsStr::new(component), true)?,
        })
    }

    pub(crate) fn create_stage(&self) -> Result<OwnedStage, NativeArchiveError> {
        OwnedStage::create(self.directory.clone())
    }

    #[cfg(test)]
    fn create_stage_after_validation_for_test(
        &self,
        after_validation: impl FnOnce() -> io::Result<()>,
    ) -> Result<OwnedStage, NativeArchiveError> {
        OwnedStage::create_after_validation_for_test(self.directory.clone(), after_validation)
    }

    fn open_readonly_leaf(&self, leaf: &OsStr) -> Result<File, NativeArchiveError> {
        validate_single_component(leaf, "relative output leaf")?;
        let (file, _) = open_relative(
            &self.directory,
            leaf,
            windows::Win32::Storage::FileSystem::FILE_READ_DATA
                | windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
                | windows::Win32::Storage::FileSystem::SYNCHRONIZE,
            windows::Win32::Storage::FileSystem::FILE_SHARE_READ,
            windows::Wdk::Storage::FileSystem::FILE_OPEN,
            windows::Wdk::Storage::FileSystem::FILE_NON_DIRECTORY_FILE
                | windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT
                | windows::Wdk::Storage::FileSystem::FILE_SYNCHRONOUS_IO_NONALERT,
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
            "NtCreateFile(FILE_OPEN relative file)",
            || Ok(()),
        )?;
        let metadata = file.metadata().map_err(|error| {
            NativeArchiveError::from_io(
                "File::metadata(relative file)",
                error,
                Some(self.directory.identity),
            )
        })?;
        if metadata.file_attributes()
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
            != 0
            || !metadata.is_file()
        {
            return Err(NativeArchiveError::validation(
                "relative file is not an ordinary file",
                io::ErrorKind::InvalidData,
                native_file_identity(&file).ok(),
            ));
        }
        Ok(file)
    }

    fn probe_leaf_occupancy(
        &self,
        leaf: &OsStr,
    ) -> Result<Option<DestinationOccupancy>, NativeArchiveError> {
        validate_single_component(leaf, "validated occupancy leaf")?;
        let (file, _) = match open_relative(
            &self.directory,
            leaf,
            windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
                | windows::Win32::Storage::FileSystem::SYNCHRONIZE,
            windows::Win32::Storage::FileSystem::FILE_SHARE_READ,
            windows::Wdk::Storage::FileSystem::FILE_OPEN,
            windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT
                | windows::Wdk::Storage::FileSystem::FILE_SYNCHRONOUS_IO_NONALERT,
            windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
            "NtCreateFile(FILE_OPEN relative occupancy leaf)",
            || Ok(()),
        ) {
            Ok(opened) => opened,
            Err(error) if is_missing_relative_component(&error) => return Ok(None),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata().map_err(|error| {
            NativeArchiveError::from_io(
                "File::metadata(relative occupancy leaf)",
                error,
                Some(self.directory.identity),
            )
        })?;
        let occupancy = if metadata.file_attributes()
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
            != 0
        {
            DestinationOccupancy::ReparsePoint
        } else if metadata.is_dir() {
            DestinationOccupancy::Directory
        } else if metadata.is_file() {
            DestinationOccupancy::File
        } else {
            DestinationOccupancy::Other
        };
        Ok(Some(occupancy))
    }

    #[cfg(test)]
    fn read_leaf(&self, leaf: &str) -> Result<Vec<u8>, NativeArchiveError> {
        let mut file = self.open_readonly_leaf(OsStr::new(leaf))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|error| {
            NativeArchiveError::from_io(
                "Read relative published file",
                error,
                Some(self.directory.identity),
            )
        })?;
        Ok(bytes)
    }

    #[cfg(test)]
    fn leaf_identity(&self, leaf: &str) -> Result<NativeFileIdentity, NativeArchiveError> {
        let file = self.open_readonly_leaf(OsStr::new(leaf))?;
        native_file_identity(&file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(relative published file)",
                error,
                Some(self.directory.identity),
            )
        })
    }
}

struct OrdinaryFileAdmission {
    file: VerifiedOverlappedFile,
    ancestors: Arc<HeldDirectory>,
    identity: NativeFileIdentity,
    len: u64,
    modified: u64,
    created: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CreationFileEvidence {
    identity: NativeFileIdentity,
    len: u64,
    modified: u64,
    created: u64,
    changed: i64,
}

fn creation_change_time(
    file: &File,
    identity: NativeFileIdentity,
) -> Result<i64, NativeArchiveError> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
    };
    let mut info = FILE_BASIC_INFO::default();
    // SAFETY: the held file handle is live and info is a writable buffer of
    // exactly the size required for FileBasicInfo.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle() as _),
            FileBasicInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    }
    .map_err(|error| {
        NativeArchiveError::from_win32("creation FILE_BASIC_INFO", error, Some(identity))
    })?;
    Ok(info.ChangeTime)
}

pub(crate) struct NativeCreationFile {
    path: PathBuf,
    admission: OrdinaryFileAdmission,
    changed: i64,
}

impl NativeCreationFile {
    pub(crate) fn open(path: &Path) -> Result<Self, NativeArchiveError> {
        let admission = open_ordinary_file(path)?;
        let changed = creation_change_time(admission.file.as_file(), admission.identity)?;
        Ok(Self {
            path: path.to_owned(),
            admission,
            changed,
        })
    }
    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.admission.identity
    }
    pub(crate) fn len(&self) -> u64 {
        self.admission.len
    }
    pub(crate) fn evidence(&self) -> CreationFileEvidence {
        CreationFileEvidence {
            identity: self.admission.identity,
            len: self.admission.len,
            modified: self.admission.modified,
            created: self.admission.created,
            changed: self.changed,
        }
    }
    pub(crate) fn verified_file(&self) -> &VerifiedOverlappedFile {
        &self.admission.file
    }
    pub(crate) fn verify(&self) -> Result<(), NativeArchiveError> {
        let current = open_ordinary_file(&self.path)?;
        let metadata = self.admission.file.as_file().metadata().map_err(|error| {
            NativeArchiveError::from_io("creation source metadata", error, Some(self.identity()))
        })?;
        if current.identity != self.admission.identity
            || current.len != self.admission.len
            || current.modified != self.admission.modified
            || current.created != self.admission.created
            || metadata.len() != self.admission.len
            || metadata.last_write_time() != self.admission.modified
            || metadata.creation_time() != self.admission.created
            || creation_change_time(current.file.as_file(), current.identity)? != self.changed
            || creation_change_time(self.admission.file.as_file(), self.identity())? != self.changed
        {
            return Err(NativeArchiveError::validation(
                "creation source changed",
                io::ErrorKind::InvalidData,
                Some(self.identity()),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CreationDirectoryEvidence {
    identity: NativeFileIdentity,
    modified: u64,
    changed: i64,
}
pub(crate) struct NativeCreationDirectory {
    path: PathBuf,
    directory: Arc<HeldDirectory>,
    evidence: CreationDirectoryEvidence,
}
impl NativeCreationDirectory {
    pub(crate) fn open(path: &Path) -> Result<Self, NativeArchiveError> {
        let directory = open_directory_path(path, false)?;
        let metadata = directory.file.metadata().map_err(|e| {
            NativeArchiveError::from_io("creation directory metadata", e, Some(directory.identity))
        })?;
        Ok(Self {
            path: path.to_owned(),
            evidence: CreationDirectoryEvidence {
                identity: directory.identity,
                modified: metadata.last_write_time(),
                changed: creation_change_time(&directory.file, directory.identity)?,
            },
            directory,
        })
    }
    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.directory.identity
    }
    pub(crate) fn evidence(&self) -> CreationDirectoryEvidence {
        self.evidence
    }
    pub(crate) fn verify(&self) -> Result<(), NativeArchiveError> {
        self.directory.verify_identity()?;
        let current = Self::open(&self.path)?;
        let held = self.directory.file.metadata().map_err(|e| {
            NativeArchiveError::from_io(
                "creation directory metadata",
                e,
                Some(self.directory.identity),
            )
        })?;
        if current.evidence != self.evidence
            || held.last_write_time() != self.evidence.modified
            || creation_change_time(&self.directory.file, self.directory.identity)?
                != self.evidence.changed
        {
            return Err(NativeArchiveError::validation(
                "creation source directory changed",
                io::ErrorKind::InvalidData,
                Some(self.directory.identity),
            ));
        }
        Ok(())
    }
}

fn open_ordinary_file(path: &Path) -> Result<OrdinaryFileAdmission, NativeArchiveError> {
    let (volume_root, mut components) = split_local_absolute_path(path)?;
    let leaf = components.pop().ok_or_else(|| {
        NativeArchiveError::validation(
            "archive source path has no filename",
            io::ErrorKind::InvalidInput,
            None,
        )
    })?;
    let ancestors = open_directory_parts(&volume_root, &components, false)?;
    let (file, _) = open_relative(
        &ancestors,
        &leaf,
        windows::Win32::Storage::FileSystem::FILE_READ_DATA
            | windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
            | windows::Win32::Storage::FileSystem::SYNCHRONIZE,
        windows::Win32::Storage::FileSystem::FILE_SHARE_READ,
        windows::Wdk::Storage::FileSystem::FILE_OPEN,
        windows::Wdk::Storage::FileSystem::FILE_NON_DIRECTORY_FILE
            | windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT,
        windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        "NtCreateFile(FILE_OPEN held archive source)",
        || Ok(()),
    )?;
    let metadata = file.metadata().map_err(|error| {
        NativeArchiveError::from_io(
            "File::metadata(archive source)",
            error,
            Some(ancestors.identity),
        )
    })?;
    let identity = native_file_identity(&file).map_err(|error| {
        NativeArchiveError::from_io(
            "FILE_ID_INFO(archive source)",
            error,
            Some(ancestors.identity),
        )
    })?;
    if metadata.file_attributes()
        & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
        != 0
        || !metadata.is_file()
    {
        return Err(NativeArchiveError::validation(
            "archive source is not an ordinary file",
            io::ErrorKind::InvalidData,
            Some(identity),
        ));
    }
    if identity.volume_serial_number() != ancestors.identity.volume_serial_number() {
        return Err(NativeArchiveError::validation(
            "archive source crossed the held local volume",
            io::ErrorKind::InvalidData,
            Some(identity),
        ));
    }
    let archive_len = metadata.len();
    let modified = metadata.last_write_time();
    let created = metadata.creation_time();
    let file = VerifiedOverlappedFile::verify(file).map_err(|error| {
        NativeArchiveError::validation(
            match error {
                super::native_io::NativeIoStartError::HandleModeQueryFailed { .. } => {
                    "verify overlapped archive source mode"
                }
                super::native_io::NativeIoStartError::SynchronousHandle { .. } => {
                    "archive source was opened synchronously"
                }
                super::native_io::NativeIoStartError::InvalidBufferLength => {
                    "verify overlapped archive source mode"
                }
            },
            io::ErrorKind::InvalidData,
            Some(identity),
        )
    })?;
    Ok(OrdinaryFileAdmission {
        file,
        ancestors,
        identity,
        len: archive_len,
        modified,
        created,
    })
}

/// One opened archive source. Catalog verification reads through an
/// explicit-offset adapter over this held handle; it never reopens the path.
pub(crate) struct NativeArchiveSource {
    file: VerifiedOverlappedFile,
    _ancestors: Arc<HeldDirectory>,
    identity: NativeFileIdentity,
    archive_len: u64,
    revision: ArchiveRevision,
    limits: ArchiveLimits,
    catalog_source: Option<ArchiveSource>,
}

impl NativeArchiveSource {
    pub(crate) fn open(
        path: &Path,
        revision: ArchiveRevision,
        limits: ArchiveLimits,
    ) -> Result<Self, NativeArchiveError> {
        let admitted = open_ordinary_file(path)?;
        let OrdinaryFileAdmission {
            file,
            ancestors,
            identity,
            len: archive_len,
            ..
        } = admitted;
        Ok(Self {
            file,
            _ancestors: ancestors,
            identity,
            archive_len,
            revision,
            limits,
            catalog_source: None,
        })
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub(crate) fn archive_source(&self) -> Option<ArchiveSource> {
        self.catalog_source
    }

    pub(crate) fn archive_len(&self) -> u64 {
        self.archive_len
    }

    pub(crate) fn verified_file(&self) -> &VerifiedOverlappedFile {
        &self.file
    }

    /// Catalog the already-held source file. Callers open the source before
    /// cataloging so a pathname replacement cannot silently rebind metadata
    /// from a prior file to a new object with matching length.
    pub(crate) fn read_catalog(
        &mut self,
        cancellation: &ScanCancellation,
    ) -> Result<ArchiveCatalog, NativeArchiveError> {
        let io = ArchiveIoControl::new(cancellation.clone());
        self.read_catalog_with_io(cancellation, &io)
    }

    pub(crate) fn read_catalog_with_io(
        &mut self,
        cancellation: &ScanCancellation,
        io_control: &ArchiveIoControl,
    ) -> Result<ArchiveCatalog, NativeArchiveError> {
        let file = self.file.as_file();
        let before = native_file_identity(file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(held archive source)",
                error,
                Some(self.identity),
            )
        })?;
        let before_len = file
            .metadata()
            .map_err(|error| {
                NativeArchiveError::from_io(
                    "File::metadata(held archive source)",
                    error,
                    Some(before),
                )
            })?
            .len();
        if before != self.identity || before_len != self.archive_len {
            return Err(NativeArchiveError::validation(
                "held archive source identity or length changed",
                io::ErrorKind::InvalidData,
                Some(before),
            ));
        }
        let catalog = super::read_catalog(
            &mut OverlappedArchiveReader::new(&self.file, io_control, self.archive_len),
            self.revision,
            self.limits,
            cancellation,
        )
        .map_err(|error| catalog_error(error, self.identity))?;
        let file = self.file.as_file();
        let after = native_file_identity(file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(held archive source after catalog)",
                error,
                Some(self.identity),
            )
        })?;
        let after_len = file
            .metadata()
            .map_err(|error| {
                NativeArchiveError::from_io(
                    "File::metadata(held archive source after catalog)",
                    error,
                    Some(after),
                )
            })?
            .len();
        if after != self.identity || after_len != self.archive_len {
            return Err(NativeArchiveError::validation(
                "held source identity or length changed while cataloging",
                io::ErrorKind::InvalidData,
                Some(after),
            ));
        }
        self.catalog_source = Some(catalog.source());
        Ok(catalog)
    }

    pub(crate) fn verify_catalog(
        &mut self,
        expected_catalog: &ArchiveCatalog,
    ) -> Result<(), NativeArchiveError> {
        if self.catalog_source.is_none() {
            return Err(NativeArchiveError::validation(
                "held source must be cataloged before verification",
                io::ErrorKind::InvalidData,
                Some(self.identity),
            ));
        }
        let cancellation = ScanCancellation::new();
        let io = ArchiveIoControl::new(cancellation.clone());
        self.verify_catalog_with_io(expected_catalog, &cancellation, &io)
    }

    pub(crate) fn verify_catalog_with_io(
        &mut self,
        expected_catalog: &ArchiveCatalog,
        cancellation: &ScanCancellation,
        io_control: &ArchiveIoControl,
    ) -> Result<(), NativeArchiveError> {
        if self.catalog_source.is_none() {
            return Err(NativeArchiveError::validation(
                "held source must be cataloged before verification",
                io::ErrorKind::InvalidData,
                Some(self.identity),
            ));
        }
        let actual = self.read_catalog_with_io(cancellation, io_control)?;
        if actual != *expected_catalog {
            return Err(NativeArchiveError::validation(
                "held source no longer matches the selected archive catalog revision",
                io::ErrorKind::InvalidData,
                Some(self.identity),
            ));
        }
        Ok(())
    }
}

fn catalog_error(error: CatalogError, identity: NativeFileIdentity) -> NativeArchiveError {
    let kind = match error {
        CatalogError::Cancelled => io::ErrorKind::Interrupted,
        CatalogError::Allocation => io::ErrorKind::OutOfMemory,
        CatalogError::LimitExceeded(_) => io::ErrorKind::InvalidData,
        CatalogError::Malformed(_) => io::ErrorKind::InvalidData,
        CatalogError::Io(kind) => kind,
    };
    NativeArchiveError::validation("read_catalog on held source", kind, Some(identity))
}

fn shared_directory_access() -> windows::Win32::Storage::FileSystem::FILE_SHARE_MODE {
    windows::Win32::Storage::FileSystem::FILE_SHARE_READ
        | windows::Win32::Storage::FileSystem::FILE_SHARE_WRITE
}

pub(crate) fn split_local_absolute_path(path: &Path) -> Result<(PathBuf, Vec<OsString>), NativeArchiveError> {
    let mut components = path.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => {
                return Err(NativeArchiveError::validation(
                    "native path must be an absolute local drive path",
                    io::ErrorKind::InvalidInput,
                    None,
                ));
            }
        },
        _ => {
            return Err(NativeArchiveError::validation(
                "native path must be an absolute local drive path",
                io::ErrorKind::InvalidInput,
                None,
            ));
        }
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(NativeArchiveError::validation(
            "native path must include a drive root",
            io::ErrorKind::InvalidInput,
            None,
        ));
    }

    let mut volume_root = OsString::from(char::from(drive).to_string());
    volume_root.push(":");
    volume_root.push("\\");
    let mut names = Vec::new();
    for component in components {
        match component {
            Component::Normal(name) => names.push(name.to_os_string()),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(NativeArchiveError::validation(
                    "native path contains a non-normal component",
                    io::ErrorKind::InvalidInput,
                    None,
                ));
            }
        }
    }
    let total_units = path.as_os_str().encode_wide().count();
    if total_units.saturating_add(1) > MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL {
        return Err(NativeArchiveError::validation(
            "native path exceeds the local extended-path bound",
            io::ErrorKind::InvalidInput,
            None,
        ));
    }
    Ok((PathBuf::from(volume_root), names))
}

fn open_directory_path(
    path: &Path,
    require_add_file: bool,
) -> Result<Arc<HeldDirectory>, NativeArchiveError> {
    let (volume_root, components) = split_local_absolute_path(path)?;
    open_directory_parts(&volume_root, &components, require_add_file)
}

fn open_directory_parts(
    volume_root: &Path,
    components: &[OsString],
    require_add_file: bool,
) -> Result<Arc<HeldDirectory>, NativeArchiveError> {
    let mut current = open_volume_root(volume_root, components.is_empty() && require_add_file)?;
    for (index, component) in components.iter().enumerate() {
        let is_final = index + 1 == components.len();
        current = current.open_child(component, is_final && require_add_file)?;
    }
    Ok(current)
}

fn open_volume_root(path: &Path, require_add_file: bool) -> Result<Arc<HeldDirectory>, NativeArchiveError> {
    open_volume_root_with_access(path, require_add_file, require_add_file)
}

fn open_volume_root_with_access(
    path: &Path, require_add_file: bool, require_add_subdirectory: bool,
) -> Result<Arc<HeldDirectory>, NativeArchiveError> {
    open_volume_root_with_options(path, require_add_file, require_add_subdirectory, false, false)
}

fn open_volume_root_with_options(
    path: &Path, require_add_file: bool, require_add_subdirectory: bool, no_recall: bool, list: bool,
) -> Result<Arc<HeldDirectory>, NativeArchiveError> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_TRAVERSE, GetDriveTypeW, OPEN_EXISTING, SYNCHRONIZE,
    };
    use windows::core::PCWSTR;

    let mut wide = extended_path_wide(path)?;
    wide.push(0);
    let mut drive_root_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    drive_root_wide.push(0);
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(drive_root_wide.as_ptr())) };
    // GetDriveTypeW values: unknown=0, no-root=1, remote=4. All other
    // documented drive types are backed by a local volume.
    if !matches!(drive_type, 2 | 3 | 5 | 6) {
        return Err(NativeArchiveError::validation(
            "native archive root is not a local volume",
            io::ErrorKind::InvalidInput,
            None,
        ));
    }
    let mut access = FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE;
    if list { access |= windows::Win32::Storage::FileSystem::FILE_LIST_DIRECTORY; }
    if require_add_file {
        access |= FILE_ADD_FILE;
    }
    if require_add_subdirectory { access |= FILE_ADD_SUBDIRECTORY; }
    // SAFETY: `wide` is an absolute, local, NUL-terminated root path. The
    // handle is opened as the reparse point itself and immediately owned by a
    // `File`. Directory handles intentionally do not share delete access.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            access.0,
            shared_directory_access(),
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
                | if no_recall { windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_NO_RECALL } else { windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0) },
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        NativeArchiveError::from_win32("CreateFileW(open volume root)", error, None)
    })?;
    // SAFETY: CreateFileW returned a valid owned handle transferred exactly
    // once; `File::drop` closes it.
    let file = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| NativeArchiveError::from_io("File::metadata(volume root)", error, None))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY.0 == 0
        || !metadata.is_dir()
    {
        return Err(NativeArchiveError::validation(
            "local volume root is not an ordinary directory",
            io::ErrorKind::InvalidData,
            native_file_identity(&file).ok(),
        ));
    }
    let identity = native_file_identity(&file)
        .map_err(|error| NativeArchiveError::from_io("FILE_ID_INFO(volume root)", error, None))?;
    Ok(Arc::new(HeldDirectory {
        file,
        identity,
        parent: None,
        name: path.as_os_str().to_os_string(),
    }))
}

fn extended_path_wide(path: &Path) -> Result<Vec<u16>, NativeArchiveError> {
    const EXTENDED_PREFIX: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
    if raw.is_empty() || raw.contains(&0) {
        return Err(NativeArchiveError::validation(
            "native path is empty or contains NUL",
            io::ErrorKind::InvalidInput,
            None,
        ));
    }
    let extended = if raw.starts_with(&EXTENDED_PREFIX) {
        raw
    } else {
        let mut output = Vec::with_capacity(EXTENDED_PREFIX.len() + raw.len());
        output.extend(EXTENDED_PREFIX);
        output.extend(raw);
        output
    };
    if extended.len().saturating_add(1) > MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL {
        return Err(NativeArchiveError::validation(
            "native path exceeds the local extended-path bound",
            io::ErrorKind::InvalidInput,
            None,
        ));
    }
    Ok(extended)
}

fn validate_single_component(
    component: &OsStr,
    operation: &'static str,
) -> Result<(), NativeArchiveError> {
    let mut wide: Vec<u16> = component.encode_wide().collect();
    let mut parts = Path::new(component).components();
    let is_normal = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
    if !is_normal
        || wide.is_empty()
        || wide.len() > 255
        || wide
            .iter()
            .any(|unit| matches!(*unit, 0 | 0x2f | 0x5c | 0x3a))
        || wide.last().is_some_and(|unit| matches!(*unit, 0x2e | 0x20))
    {
        wide.clear();
        return Err(NativeArchiveError::validation(
            operation,
            io::ErrorKind::InvalidInput,
            None,
        ));
    }
    Ok(())
}

fn open_relative(
    parent: &HeldDirectory,
    name: &OsStr,
    desired_access: windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS,
    share_access: windows::Win32::Storage::FileSystem::FILE_SHARE_MODE,
    disposition: windows::Wdk::Storage::FileSystem::NTCREATEFILE_CREATE_DISPOSITION,
    create_options: windows::Wdk::Storage::FileSystem::NTCREATEFILE_CREATE_OPTIONS,
    file_attributes: windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES,
    operation: &'static str,
    // Test callers use this boundary to inject a namespace mutation after all
    // validation and immediately before the real native create call.
    before_native_create: impl FnOnce() -> Result<(), NativeArchiveError>,
) -> Result<(File, usize), NativeArchiveError> {
    use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows::Wdk::Storage::FileSystem::NtCreateFile;
    use windows::Win32::Foundation::{HANDLE, UNICODE_STRING};
    use windows::Win32::System::IO::IO_STATUS_BLOCK;
    use windows::core::PWSTR;

    let mut name_wide: Vec<u16> = name.encode_wide().collect();
    let byte_len = name_wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| {
            NativeArchiveError::validation(
                operation,
                io::ErrorKind::InvalidInput,
                Some(parent.identity),
            )
        })?;
    if byte_len == 0 || name_wide.contains(&0) {
        return Err(NativeArchiveError::validation(
            operation,
            io::ErrorKind::InvalidInput,
            Some(parent.identity),
        ));
    }
    let parent_identity = parent.verify_identity()?;
    let parent_attributes = query_attribute_tag(
        &parent.file,
        "NtQueryInformationFile(FileAttributeTagInformation, held parent)",
        parent_identity,
    )
    .map_err(|primary| NativeArchiveError {
        primary,
        cleanup_attempted: false,
        cleanup_failure: None,
        cleanup_failed: false,
        unconfirmed_name_effect: false,
    })?;
    if parent_attributes.FileAttributes
        & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
        != 0
    {
        return Err(NativeArchiveError::validation(
            "held parent became a reparse point before relative operation",
            io::ErrorKind::InvalidData,
            Some(parent_identity),
        ));
    }
    let name_string = UNICODE_STRING {
        Length: byte_len,
        MaximumLength: byte_len,
        Buffer: PWSTR(name_wide.as_mut_ptr()),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: HANDLE(parent.file.as_raw_handle() as _),
        ObjectName: std::ptr::from_ref(&name_string),
        Attributes: 0x40, // OBJ_CASE_INSENSITIVE
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut io_status = IO_STATUS_BLOCK::default();
    let mut handle = HANDLE::default();
    // SAFETY: both names and attributes remain live for the call, and the
    // parent is a retained directory handle. FILE_OPEN_REPARSE_POINT is set by
    // each caller, so the final component is inspected rather than followed.
    before_native_create()?;
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access,
            &attributes,
            &mut io_status,
            None,
            file_attributes,
            share_access,
            disposition,
            create_options,
            None,
            0,
        )
    };
    let io_status_value = unsafe { io_status.Anonymous.Status };
    let evidence_status = if status.0 == 0 {
        io_status_value
    } else {
        status
    };
    #[cfg(test)]
    record_native_evidence(operation, evidence_status, Some(parent.identity));
    // NtCreateFile returns STATUS_SUCCESS or an error for an open/create.
    // Unlike a data ReadFile/WriteFile request, it has no pending completion
    // object; do not accept another nonnegative NTSTATUS as completed work.
    if status.0 != 0 || io_status_value.0 != 0 {
        let error = NativeArchiveError::from_ntstatus(
            operation, evidence_status, Some(parent.identity),
        );
        if !handle.is_invalid() {
            // A returned handle on failure leaves the name effect uncertain.
            let unexpected = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
            drop(unexpected);
            return Err(error.with_unconfirmed_name_effect());
        }
        return Err(if status.0 >= 0 { error.with_unconfirmed_name_effect() } else { error });
    }
    if handle.is_invalid() {
        return Err(NativeArchiveError::validation(
            "NtCreateFile returned success without a valid handle",
            io::ErrorKind::InvalidData,
            Some(parent.identity),
        ).with_unconfirmed_name_effect());
    }
    // SAFETY: NT success returned a new owned file handle; it is transferred
    // once to File and closed by RAII.
    let file = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
    Ok((file, io_status.Information))
}

fn query_attribute_tag(
    file: &File,
    operation: &'static str,
    held_identity: NativeFileIdentity,
) -> Result<windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_TAG_INFO, NativeFailure> {
    use windows::Wdk::Storage::FileSystem::{FileAttributeTagInformation, NtQueryInformationFile};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_TAG_INFO;
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    let mut information = FILE_ATTRIBUTE_TAG_INFO::default();
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtQueryInformationFile(
            HANDLE(file.as_raw_handle() as _),
            &mut io_status,
            std::ptr::from_mut(&mut information).cast(),
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            FileAttributeTagInformation,
        )
    };
    let io_status_value = unsafe { io_status.Anonymous.Status };
    let evidence_status = if status.0 == 0 {
        io_status_value
    } else {
        status
    };
    #[cfg(test)]
    record_native_evidence(operation, evidence_status, Some(held_identity));
    if status.0 != 0 || io_status_value.0 != 0 {
        return Err(NativeFailure::from_ntstatus(
            operation,
            evidence_status,
            Some(held_identity),
        ));
    }
    Ok(information)
}

fn delete_open_file(file: &File, identity: NativeFileIdentity) -> Result<(), NativeFailure> {
    use windows::Wdk::Storage::FileSystem::{FileDispositionInformation, NtSetInformationFile};
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO;
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    let current = native_file_identity(file).map_err(|error| {
        NativeFailure::from_io("FILE_ID_INFO(stage cleanup)", &error, Some(identity))
    })?;
    if current != identity {
        return Err(NativeFailure::validation(
            "owned stage identity changed before cleanup",
            io::ErrorKind::InvalidData,
            Some(current),
        ));
    }
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: BOOLEAN(1),
    };
    let mut io_status = IO_STATUS_BLOCK::default();
    // SAFETY: the buffer matches FILE_DISPOSITION_INFO and the retained handle
    // is the exact create-new stage object whose identity was just verified.
    let status = unsafe {
        NtSetInformationFile(
            HANDLE(file.as_raw_handle() as _),
            &mut io_status,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            FileDispositionInformation,
        )
    };
    let io_status_value = unsafe { io_status.Anonymous.Status };
    let evidence_status = if status.0 == 0 {
        io_status_value
    } else {
        status
    };
    #[cfg(test)]
    record_native_evidence(
        "NtSetInformationFile(FileDispositionInformation)",
        evidence_status,
        Some(current),
    );
    if status.0 != 0 || io_status_value.0 != 0 {
        return Err(NativeFailure::from_ntstatus(
            "NtSetInformationFile(FileDispositionInformation)",
            evidence_status,
            Some(current),
        ));
    }
    Ok(())
}

fn rename_open_file_relative_no_replace(
    file: &File,
    parent: &HeldDirectory,
    leaf: &OsStr,
    identity: NativeFileIdentity,
) -> Result<(), NativeFailure> {
    use std::mem::offset_of;
    use windows::Wdk::Storage::FileSystem::{FileRenameInformation, NtSetInformationFile};
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::FILE_RENAME_INFO;
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    validate_single_component(leaf, "relative rename target").map_err(|error| error.primary)?;
    let name: Vec<u16> = leaf.encode_wide().collect();
    let file_name_length = name
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| {
            NativeFailure::validation(
                "relative rename target length",
                io::ErrorKind::InvalidInput,
                Some(identity),
            )
        })?;
    let mut info = FILE_RENAME_INFO::default();
    info.Anonymous.ReplaceIfExists = BOOLEAN(0);
    info.RootDirectory = HANDLE(parent.file.as_raw_handle() as _);
    info.FileNameLength = file_name_length;
    let name_offset = offset_of!(FILE_RENAME_INFO, FileName);
    let buffer_bytes = name_offset
        .checked_add(file_name_length as usize)
        .ok_or_else(|| {
            NativeFailure::validation(
                "relative rename buffer length",
                io::ErrorKind::InvalidInput,
                Some(identity),
            )
        })?
        .max(std::mem::size_of::<FILE_RENAME_INFO>());
    let mut buffer = vec![0_u64; buffer_bytes.div_ceil(std::mem::size_of::<u64>())];
    let buffer_ptr = buffer.as_mut_ptr().cast::<u8>();
    // SAFETY: the aligned buffer contains the full FILE_RENAME_INFO and
    // UTF-16 leaf. RootDirectory is the held same-parent handle, and
    // ReplaceIfExists=false makes the namespace operation no-replace.
    unsafe {
        std::ptr::copy_nonoverlapping(
            std::ptr::from_ref(&info).cast::<u8>(),
            buffer_ptr,
            name_offset,
        );
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            buffer_ptr.add(name_offset),
            file_name_length as usize,
        );
    }
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            HANDLE(file.as_raw_handle() as _),
            &mut io_status,
            buffer_ptr.cast(),
            u32::try_from(buffer_bytes).map_err(|_| {
                NativeFailure::validation(
                    "relative rename buffer length",
                    io::ErrorKind::InvalidInput,
                    Some(identity),
                )
            })?,
            FileRenameInformation,
        )
    };
    let io_status_value = unsafe { io_status.Anonymous.Status };
    let evidence_status = if status.0 == 0 {
        io_status_value
    } else {
        status
    };
    #[cfg(test)]
    record_native_evidence(
        "NtSetInformationFile(FileRenameInformation, RootDirectory, no-replace)",
        evidence_status,
        Some(identity),
    );
    if status.0 != 0 || io_status_value.0 != 0 {
        return Err(NativeFailure::from_ntstatus(
            "NtSetInformationFile(FileRenameInformation, RootDirectory, no-replace)",
            evidence_status,
            Some(identity),
        ));
    }
    Ok(())
}

fn extended_path_wide_with_nul(path: &Path) -> Result<Vec<u16>, NativeArchiveError> {
    let mut wide = extended_path_wide(path)?;
    wide.push(0);
    Ok(wide)
}

fn default_stage_leaf() -> OsString {
    let serial = NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed);
    OsString::from(format!(".multi-launcher-c06-{}-{serial}.stage", std::process::id()))
}

struct StageCreateObservation {
    leaf: OsString,
    identity: Option<NativeFileIdentity>,
    creation_confirmed: bool,
}

struct TransferStageAdmission<'a> {
    final_leaf: &'a OsStr,
    observation: &'a mut Option<StageCreateObservation>,
}

/// A create-new stage tied to its held parent and file identity.
pub(crate) struct OwnedStage {
    file: VerifiedOverlappedFile,
    parent: Arc<HeldDirectory>,
    parent_identity: NativeFileIdentity,
    leaf: OsString,
    identity: NativeFileIdentity,
    finished: bool,
    #[cfg(test)]
    validation_identity_override: Option<NativeFileIdentity>,
}

impl OwnedStage {
    pub(crate) fn verified_file(&self) -> &VerifiedOverlappedFile {
        &self.file
    }
    fn create(parent: Arc<HeldDirectory>) -> Result<Self, NativeArchiveError> {
        parent.verify_identity()?;
        Self::create_after_parent_validation(parent, || Ok(()))
    }

    #[cfg(test)]
    fn create_after_validation_for_test(
        parent: Arc<HeldDirectory>,
        after_validation: impl FnOnce() -> io::Result<()>,
    ) -> Result<Self, NativeArchiveError> {
        parent.verify_identity()?;
        let parent_identity = parent.identity;
        Self::create_after_parent_validation(parent, || {
            after_validation().map_err(|error| {
                NativeArchiveError::from_io(
                    "test reparse mutation immediately before NtCreateFile",
                    error,
                    Some(parent_identity),
                )
            })
        })
    }

    fn create_after_parent_validation(parent: Arc<HeldDirectory>, before_native_create: impl FnOnce() -> Result<(), NativeArchiveError>) -> Result<Self, NativeArchiveError> {
        Self::create_after_parent_validation_with_access(parent, before_native_create, windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0), windows::Win32::Storage::FileSystem::FILE_SHARE_READ)
    }
    fn create_after_parent_validation_with_access(
        parent: Arc<HeldDirectory>, before_native_create: impl FnOnce() -> Result<(), NativeArchiveError>, extra: windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS, sharing: windows::Win32::Storage::FileSystem::FILE_SHARE_MODE,
    ) -> Result<Self, NativeArchiveError> {
        Self::create_with_options(
            parent, before_native_create, extra, sharing,
            windows::Wdk::Storage::FileSystem::NTCREATEFILE_CREATE_OPTIONS(0),
            None, default_stage_leaf, STAGE_ATTEMPTS,
        )
    }
    fn create_with_options(
        parent: Arc<HeldDirectory>,
        before_native_create: impl FnOnce() -> Result<(), NativeArchiveError>,
        extra: windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS,
        sharing: windows::Win32::Storage::FileSystem::FILE_SHARE_MODE,
        options: windows::Wdk::Storage::FileSystem::NTCREATEFILE_CREATE_OPTIONS,
        mut transfer: Option<TransferStageAdmission<'_>>,
        mut next_leaf: impl FnMut() -> OsString,
        attempts: usize,
    ) -> Result<Self, NativeArchiveError> {
        let mut before_native_create = Some(before_native_create);
        let mut last_collision = None;
        for _ in 0..attempts {
            let leaf = next_leaf();
            if let Some(transfer) = &transfer
                && crate::batch_rename::compare_paths_windows_ordinal(
                    Path::new(&leaf), Path::new(transfer.final_leaf),
                ).is_eq()
            {
                continue;
            }
            let opened = open_relative(
                &parent,
                &leaf,
                windows::Win32::Storage::FileSystem::DELETE
                    | windows::Win32::Storage::FileSystem::FILE_READ_DATA
                    | windows::Win32::Storage::FileSystem::FILE_WRITE_DATA
                    | windows::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES
                    | windows::Win32::Storage::FileSystem::FILE_WRITE_ATTRIBUTES
                    | windows::Win32::Storage::FileSystem::SYNCHRONIZE | extra,
                sharing,
                windows::Wdk::Storage::FileSystem::FILE_CREATE,
                windows::Wdk::Storage::FileSystem::FILE_NON_DIRECTORY_FILE
                    | windows::Wdk::Storage::FileSystem::FILE_OPEN_REPARSE_POINT | options,
                windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(FILE_CREATE owned stage)",
                || match before_native_create.take() {
                    Some(before_native_create) => before_native_create(),
                    None => Ok(()),
                },
            );
            let (file, information) = match opened {
                Ok(opened) => opened,
                Err(error)
                    if !error.unconfirmed_name_effect
                        && (matches!(error.primary.win32_code, Some(80 | 183))
                            || error.primary.nt_status == Some(0xc000_0035_u32 as i32)) =>
                {
                    last_collision = Some(error);
                    continue;
                }
                Err(error) => {
                    if error.unconfirmed_name_effect
                        && let Some(transfer) = &mut transfer
                    {
                        *transfer.observation = Some(StageCreateObservation {
                            leaf,
                            identity: None,
                            creation_confirmed: false,
                        });
                    }
                    return Err(error);
                }
            };

            if let Some(transfer) = &mut transfer {
                *transfer.observation = Some(StageCreateObservation {
                    leaf: leaf.clone(),
                    identity: None,
                    creation_confirmed: information == 2,
                });
            }
            if information != 2 {
                let operation = "NtCreateFile did not report FILE_CREATED for stage";
                if transfer.is_some() {
                    // A contradictory disposition cannot establish transfer ownership.
                    return Err(NativeArchiveError::validation(
                        operation, io::ErrorKind::InvalidData, Some(parent.identity),
                    ));
                }
                let primary = NativeFailure::validation(
                    operation, io::ErrorKind::InvalidData, Some(parent.identity),
                );
                let cleanup = delete_open_file_without_identity(&file);
                return Err(NativeArchiveError::after_cleanup(primary, cleanup));
            }

            let metadata = match file.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    let primary = NativeFailure::from_io(
                        "File::metadata(new stage)",
                        &error,
                        Some(parent.identity),
                    );
                    let cleanup = delete_open_file_without_identity(&file);
                    return Err(NativeArchiveError::after_cleanup(primary, cleanup));
                }
            };
            if !metadata.is_file()
                || metadata.file_attributes()
                    & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                    != 0
            {
                let primary = NativeFailure::validation(
                    "new stage is not an ordinary file",
                    io::ErrorKind::InvalidData,
                    native_file_identity(&file).ok(),
                );
                let cleanup = delete_open_file_without_identity(&file);
                return Err(NativeArchiveError::after_cleanup(primary, cleanup));
            }
            let identity = match native_file_identity(&file) {
                Ok(identity) => identity,
                Err(error) => {
                    let primary = NativeFailure::from_io(
                        "FILE_ID_INFO(new stage)",
                        &error,
                        Some(parent.identity),
                    );
                    let cleanup = delete_open_file_without_identity(&file);
                    return Err(NativeArchiveError::after_cleanup(primary, cleanup));
                }
            };
            if identity.volume_serial_number() != parent.identity.volume_serial_number() {
                let primary = NativeFailure::validation(
                    "new stage is outside the held parent volume",
                    io::ErrorKind::InvalidData,
                    Some(identity),
                );
                let cleanup = delete_open_file_without_identity(&file);
                return Err(NativeArchiveError::after_cleanup(primary, cleanup));
            }
            if transfer.is_some() && identity.file_id() == [0; 16] {
                let primary = NativeFailure::validation(
                    "new stage has no supported file identity",
                    io::ErrorKind::InvalidData,
                    Some(parent.identity),
                );
                return Err(NativeArchiveError::after_cleanup(
                    primary, delete_open_file_without_identity(&file),
                ));
            }
            if let Some(transfer) = &mut transfer
                && let Some(observation) = transfer.observation.as_mut()
            {
                observation.identity = Some(identity);
            }
            if let Err(error) = VerifiedOverlappedFile::verify_before_ownership(&file) {
                let primary = match error {
                    super::native_io::NativeIoStartError::HandleModeQueryFailed { ntstatus } => {
                        NativeFailure::from_ntstatus(
                            "NtQueryInformationFile(FileModeInformation, owned stage)",
                            windows::Win32::Foundation::NTSTATUS(ntstatus),
                            Some(identity),
                        )
                    }
                    super::native_io::NativeIoStartError::SynchronousHandle { .. } => {
                        NativeFailure::validation(
                            "new stage was opened synchronously",
                            io::ErrorKind::InvalidData,
                            Some(identity),
                        )
                    }
                    super::native_io::NativeIoStartError::InvalidBufferLength => {
                        NativeFailure::validation(
                            "verify overlapped owned stage mode",
                            io::ErrorKind::InvalidData,
                            Some(identity),
                        )
                    }
                };
                let cleanup = delete_open_file(&file, identity);
                return Err(NativeArchiveError::after_cleanup(primary, cleanup));
            }
            return Ok(Self {
                file: VerifiedOverlappedFile::from_verified(file),
                parent_identity: parent.identity,
                parent,
                leaf,
                identity,
                finished: false,
                #[cfg(test)]
                validation_identity_override: None,
            });
        }
        Err(last_collision.unwrap_or_else(|| {
            NativeArchiveError::validation(
                "unable to allocate a unique stage name",
                io::ErrorKind::AlreadyExists,
                Some(parent.identity),
            )
        }))
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub(crate) fn write_chunk(
        &self,
        io_control: &ArchiveIoControl,
        offset: u64,
        bytes: &[u8],
    ) -> Result<usize, NativeArchiveError> {
        let file = self.file.as_file();
        let current = native_file_identity(file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(stage before write)",
                error,
                Some(self.identity),
            )
        })?;
        if current != self.identity {
            return Err(NativeArchiveError::validation(
                "owned stage identity changed before write",
                io::ErrorKind::InvalidData,
                Some(current),
            ));
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.len()).map_err(|error| {
            NativeArchiveError::from_io(
                "reserve bounded owned-stage write buffer",
                io::Error::new(io::ErrorKind::OutOfMemory, error),
                Some(current),
            )
        })?;
        owned.extend_from_slice(bytes);
        let bytes = owned.into_boxed_slice();
        io_control
            .write_at(&self.file, offset, bytes)
            .map_err(|error| {
                NativeArchiveError::from_io(
                    "WriteFile(overlapped owned stage)",
                    error,
                    Some(current),
                )
            })
    }

    pub(crate) fn flush_final(&self) -> Result<(), NativeArchiveError> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::FlushFileBuffers;

        let file = self.file.as_file();
        let current = native_file_identity(file).map_err(|error| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(stage before final flush)",
                error,
                Some(self.identity),
            )
        })?;
        if current != self.identity {
            return Err(NativeArchiveError::validation(
                "owned stage identity changed before final flush",
                io::ErrorKind::InvalidData,
                Some(current),
            ));
        }
        unsafe { FlushFileBuffers(HANDLE(file.as_raw_handle() as _)) }.map_err(|error| {
            NativeArchiveError::from_win32("FlushFileBuffers(owned stage)", error, Some(current))
        })
    }

    #[cfg(test)]
    pub(crate) fn write_all_and_sync(&mut self, bytes: &[u8]) -> Result<(), NativeArchiveError> {
        let cancellation = ScanCancellation::new();
        let io = ArchiveIoControl::new(cancellation);
        let mut offset = 0_u64;
        for chunk in bytes.chunks(64 * 1024) {
            let mut completed = 0_usize;
            while completed < chunk.len() {
                let write_offset = offset.checked_add(completed as u64).ok_or_else(|| {
                    NativeArchiveError::validation(
                        "native stage offset overflow",
                        io::ErrorKind::InvalidInput,
                        Some(self.identity),
                    )
                })?;
                let count = self.write_chunk(&io, write_offset, &chunk[completed..])?;
                if count == 0 || count > chunk.len() - completed {
                    return Err(NativeArchiveError::validation(
                        "native stage write made invalid progress",
                        io::ErrorKind::WriteZero,
                        Some(self.identity),
                    ));
                }
                completed += count;
            }
            offset = offset.checked_add(chunk.len() as u64).ok_or_else(|| {
                NativeArchiveError::validation(
                    "native stage offset overflow",
                    io::ErrorKind::InvalidInput,
                    Some(self.identity),
                )
            })?;
        }
        self.flush_final()
    }

    pub(crate) fn publish(self, target_leaf: &OsStr) -> Result<NativeFileIdentity, NativeArchiveError> {
        self.publish_admitted(target_leaf, || true, false).map(|(identity, _)| identity)
    }

    fn publish_admitted(
        mut self,
        target_leaf: &OsStr,
        admit: impl FnOnce() -> bool,
        transfer_cleanup_guard: bool,
    ) -> Result<(NativeFileIdentity, VerifiedOverlappedFile), NativeArchiveError> {
        if let Err(error) = validate_single_component(target_leaf, "relative rename target") {
            return Err(self.fail_and_cleanup(error.primary));
        }
        let file = self.file.as_file();
        let actual_identity = match native_file_identity(file) {
            Ok(identity) => identity,
            Err(error) => {
                let primary = NativeFailure::from_io(
                    "FILE_ID_INFO(stage before publication)",
                    &error,
                    Some(self.identity),
                );
                return Err(self.fail_and_cleanup(primary));
            }
        };
        let expected_identity = {
            #[cfg(test)]
            {
                self.validation_identity_override.unwrap_or(self.identity)
            }
            #[cfg(not(test))]
            {
                self.identity
            }
        };
        if actual_identity != expected_identity {
            let primary = NativeFailure::validation(
                "stage handle identity changed before publication",
                io::ErrorKind::InvalidData,
                Some(actual_identity),
            );
            return Err(self.fail_and_cleanup(primary));
        }
        let parent_identity = match self.parent.verify_identity() {
            Ok(identity) => identity,
            Err(error) => return Err(self.fail_and_cleanup(error.primary)),
        };
        if parent_identity != self.parent_identity {
            let primary = NativeFailure::validation(
                "held stage parent identity changed before publication",
                io::ErrorKind::InvalidData,
                Some(parent_identity),
            );
            return Err(self.fail_and_cleanup(primary));
        }
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                let primary = NativeFailure::from_io(
                    "File::metadata(stage before publication)",
                    &error,
                    Some(actual_identity),
                );
                return Err(self.fail_and_cleanup(primary));
            }
        };
        if !metadata.is_file()
            || metadata.file_attributes()
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        {
            let primary = NativeFailure::validation(
                "stage is not an ordinary file at publication",
                io::ErrorKind::InvalidData,
                Some(actual_identity),
            );
            return Err(self.fail_and_cleanup(primary));
        }
        if !admit() {
            return Err(self.fail_and_cleanup(NativeFailure::validation(
                "publication admission retired",
                io::ErrorKind::Interrupted,
                Some(actual_identity),
            )));
        }
        match rename_open_file_relative_no_replace(file, &self.parent, target_leaf, actual_identity)
        {
            Ok(()) => {
                self.finished = true;
                Ok((actual_identity, self.file.clone()))
            }
            Err(primary) => Err(self.finish_rename_failure(primary, transfer_cleanup_guard)),
        }
    }

    fn finish_rename_failure(
        &mut self,
        primary: NativeFailure,
        transfer_cleanup_guard: bool,
    ) -> NativeArchiveError {
        if !transfer_cleanup_guard || self.proves_private_stage_name() {
            return self.fail_and_cleanup(primary);
        }
        self.finished = true;
        NativeArchiveError {
            primary,
            cleanup_attempted: false,
            cleanup_failure: None,
            cleanup_failed: false,
            unconfirmed_name_effect: true,
        }
    }

    fn proves_private_stage_name(&self) -> bool {
        let Ok(parent_identity) = self.parent.verify_identity() else { return false; };
        if parent_identity != self.parent_identity
            || native_file_identity(self.file.as_file()).ok() != Some(self.identity)
        {
            return false;
        }
        let (Ok(parent_path), Ok(file_path)) = (
            crate::file_operations::final_path_for_handle(&self.parent.file),
            crate::file_operations::final_path_for_handle(self.file.as_file()),
        ) else { return false; };
        crate::batch_rename::compare_paths_windows_ordinal(&parent_path.join(&self.leaf), &file_path).is_eq()
            && self.parent.verify_identity().ok() == Some(self.parent_identity)
            && native_file_identity(self.file.as_file()).ok() == Some(self.identity)
    }

    fn fail_and_cleanup(&mut self, primary: NativeFailure) -> NativeArchiveError {
        let cleanup = delete_open_file(self.file.as_file(), self.identity);
        self.finished = true;
        NativeArchiveError::after_cleanup(primary, cleanup)
    }

    pub(crate) fn abort(mut self) -> Result<(), NativeArchiveError> {
        let cleanup = delete_open_file(self.file.as_file(), self.identity);
        self.finished = true;
        cleanup.map_err(NativeArchiveError::from_failed_cleanup)
    }

    #[cfg(test)]
    fn leaf(&self) -> &OsStr {
        &self.leaf
    }

    #[cfg(test)]
    fn force_validation_identity_mismatch(&mut self) {
        self.validation_identity_override = Some(NativeFileIdentity::from_parts(
            self.identity.volume_serial_number().wrapping_add(1),
            self.identity.file_id(),
        ));
    }
}

impl Drop for OwnedStage {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // Fallback cleanup is handle-bound and only attempted while the file
        // identity still proves ownership. Callers use publish/abort to receive
        // any cleanup error; Drop is only a last-resort path for unwinding.
        if native_file_identity(self.file.as_file()).ok() == Some(self.identity) {
            let _ = delete_open_file(self.file.as_file(), self.identity);
        }
    }
}

fn delete_open_file_without_identity(file: &File) -> Result<(), NativeFailure> {
    use windows::Wdk::Storage::FileSystem::{FileDispositionInformation, NtSetInformationFile};
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO;
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: BOOLEAN(1),
    };
    let mut io_status = IO_STATUS_BLOCK::default();
    // SAFETY: the CREATE_NEW handle is the proof of ownership even if the
    // subsequent metadata/identity query failed. Disposition is set on that
    // same exact handle; no path lookup is performed.
    let status = unsafe {
        NtSetInformationFile(
            HANDLE(file.as_raw_handle() as _),
            &mut io_status,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            FileDispositionInformation,
        )
    };
    let io_status_value = unsafe { io_status.Anonymous.Status };
    let evidence_status = if status.0 == 0 {
        io_status_value
    } else {
        status
    };
    #[cfg(test)]
    record_native_evidence(
        "NtSetInformationFile(FileDispositionInformation, create-new handle)",
        evidence_status,
        None,
    );
    if status.0 != 0 || io_status_value.0 != 0 {
        return Err(NativeFailure::from_ntstatus(
            "NtSetInformationFile(FileDispositionInformation, create-new handle)",
            evidence_status,
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::io::Write as _;
    use std::os::windows::ffi::OsStrExt as _;
    use tempfile::{TempDir, tempdir};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE,
        FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{FSCTL_DELETE_REPARSE_POINT, FSCTL_SET_REPARSE_POINT};
    use windows::core::PCWSTR;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    const REVISION: super::super::ArchiveRevision = super::super::ArchiveRevision::new(71);
    const SOURCE_BYTES: &[u8] = b"M3-A source fixture payload";

    fn write_zip(path: &Path) -> Vec<u8> {
        write_zip_with_payload(path, SOURCE_BYTES)
    }

    fn write_zip_with_payload(path: &Path, payload: &[u8]) -> Vec<u8> {
        let file = File::create(path).expect("create ZIP fixture");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(
                "source.txt",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .expect("start ZIP entry");
        writer.write_all(payload).expect("write ZIP entry");
        writer.finish().expect("finish ZIP fixture");
        fs::read(path).expect("read ZIP fixture bytes")
    }

    fn read_fixture_catalog(path: &Path) -> ArchiveCatalog {
        let mut file = File::open(path).expect("open ZIP fixture for cataloging");
        super::super::read_catalog(
            &mut file,
            REVISION,
            ArchiveLimits::default(),
            &ScanCancellation::new(),
        )
        .expect("catalog ZIP fixture")
    }

    fn nul_terminated(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn set_junction_on_existing_directory(
        link: &Path,
        expected_identity: NativeFileIdentity,
        target: &Path,
    ) -> io::Result<JunctionGuard> {
        let link_wide = nul_terminated(link);
        let mut substitute: Vec<u16> = r"\??\".encode_utf16().collect();
        substitute.extend(target.as_os_str().encode_wide());
        let print_name: Vec<u16> = target.as_os_str().encode_wide().collect();
        let substitute_bytes = substitute.len() * 2;
        let print_bytes = print_name.len() * 2;
        let path_bytes = substitute_bytes + 2 + print_bytes + 2;
        let data_length = 8 + path_bytes;
        let mut data = vec![0_u8; 8 + data_length];
        data[0..4].copy_from_slice(&0xa000_0003_u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
        data[4..6].copy_from_slice(&(data_length as u16).to_le_bytes());
        data[6..8].copy_from_slice(&0_u16.to_le_bytes());
        data[8..10].copy_from_slice(&0_u16.to_le_bytes());
        data[10..12].copy_from_slice(&(substitute_bytes as u16).to_le_bytes());
        data[12..14].copy_from_slice(&((substitute_bytes + 2) as u16).to_le_bytes());
        data[14..16].copy_from_slice(&(print_bytes as u16).to_le_bytes());
        for (index, unit) in substitute.iter().copied().enumerate() {
            let offset = 16 + index * 2;
            data[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let print_start = 16 + substitute_bytes + 2;
        for (index, unit) in print_name.iter().copied().enumerate() {
            let offset = print_start + index * 2;
            data[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }

        // Open the already-admitted directory itself without following a
        // reparse point. The owned handle also performs unwind cleanup on
        // this same object, without resolving or deleting its target path.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(link_wide.as_ptr()),
                FILE_GENERIC_WRITE.0,
                FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0),
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                HANDLE::default(),
            )
        }
        .map_err(|error| io::Error::from_raw_os_error(win32_code(error) as i32))?;
        // SAFETY: CreateFileW returned a valid owned handle. Transfer it to
        // File immediately so every subsequent return path closes it.
        let file = unsafe { File::from_raw_handle(handle.0 as RawHandle) };
        let actual_identity = native_file_identity(&file)?;
        if actual_identity != expected_identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "junction fixture path no longer names the admitted directory",
            ));
        }
        let guard = JunctionGuard { file, active: true };
        let mut bytes_returned = 0_u32;
        let result = unsafe {
            DeviceIoControl(
                HANDLE(guard.file.as_raw_handle() as _),
                FSCTL_SET_REPARSE_POINT,
                Some(data.as_ptr().cast()),
                u32::try_from(data.len()).expect("reparse data bounded"),
                None,
                0,
                Some(&mut bytes_returned),
                None,
            )
        };
        result.map_err(|error| io::Error::from_raw_os_error(win32_code(error) as i32))?;
        Ok(guard)
    }

    struct JunctionGuard {
        file: File,
        active: bool,
    }

    impl JunctionGuard {
        fn clear(&mut self) -> io::Result<()> {
            if !self.active {
                return Ok(());
            }
            let mut data = [0_u8; 8];
            data[0..4].copy_from_slice(&0xa000_0003_u32.to_le_bytes());
            let mut bytes_returned = 0_u32;
            unsafe {
                DeviceIoControl(
                    HANDLE(self.file.as_raw_handle() as _),
                    FSCTL_DELETE_REPARSE_POINT,
                    Some(data.as_ptr().cast()),
                    data.len() as u32,
                    None,
                    0,
                    Some(&mut bytes_returned),
                    None,
                )
            }
            .map_err(|error| io::Error::from_raw_os_error(win32_code(error) as i32))?;
            self.active = false;
            Ok(())
        }
    }

    impl Drop for JunctionGuard {
        fn drop(&mut self) {
            let _ = self.clear();
        }
    }

    fn outside_names(path: &Path) -> io::Result<BTreeSet<OsString>> {
        fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect()
    }

    fn primary_failure<T>(result: &Result<T, NativeArchiveError>) -> Option<NativeFailure> {
        result.as_ref().err().map(|error| error.primary().clone())
    }

    fn emit_native_evidence(case: &str, terminal_facts: &str) {
        let (evidence, truncated) = take_native_evidence();
        println!(
            "M3-A native evidence case={case}; records={evidence:?}; truncated={truncated}; terminal={terminal_facts}"
        );
    }

    fn create_stage_and_publish_after_validation(
        directory: &NativeDirectoryHandle,
        after_validation: impl FnOnce() -> io::Result<()>,
        bytes: &[u8],
        target: &OsStr,
    ) -> (
        Result<NativeFileIdentity, NativeArchiveError>,
        Option<NativeFileIdentity>,
        Option<OsString>,
    ) {
        let mut stage = match directory.create_stage_after_validation_for_test(after_validation) {
            Ok(stage) => stage,
            Err(error) => return (Err(error), None, None),
        };
        let stage_identity = stage.identity();
        let stage_leaf = stage.leaf().to_os_string();
        if let Err(error) = stage.write_all_and_sync(bytes) {
            return (Err(error), Some(stage_identity), Some(stage_leaf));
        }
        (
            stage.publish(target),
            Some(stage_identity),
            Some(stage_leaf),
        )
    }

    fn fixture_tree() -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempdir().expect("create native archive fixture root");
        let selected = temp.path().join("selected");
        let held = temp.path().join("held");
        let outside = temp.path().join("outside");
        fs::create_dir(&selected).expect("create selected directory");
        fs::create_dir(&outside).expect("create outside directory");
        (temp, selected, held, outside)
    }

    fn install_outside_winner(outside: &Path) {
        fs::write(outside.join("sentinel.txt"), b"outside sentinel")
            .expect("write outside sentinel");
        fs::write(outside.join("winner.bin"), b"foreign winner")
            .expect("write outside competing winner");
    }

    #[test]
    fn c06_bound_source_catalogs_on_and_retains_the_same_locked_file() {
        let temp = tempdir().expect("create source fixture root");
        let archive_path = temp.path().join("source.zip");
        let stale_bytes = write_zip_with_payload(&archive_path, b"original payload");
        let stale_catalog = read_fixture_catalog(&archive_path);
        let stale_metadata = fs::metadata(&archive_path).expect("read old source metadata");
        let stale_identity =
            native_file_identity(&File::open(&archive_path).expect("open old source"))
                .expect("identify old source");

        // Replace the path with a distinct file of the same type and length
        // after the path-owned catalog exists. The source owner must then open
        // the current file first and build its catalog on that retained handle.
        let replacement_path = temp.path().join("replacement.zip");
        let replacement_bytes = write_zip_with_payload(&replacement_path, b"replaced payload");
        let replacement_metadata =
            fs::metadata(&replacement_path).expect("read replacement metadata");
        let remove_old = fs::remove_file(&archive_path);
        let install_replacement =
            remove_old.and_then(|()| fs::rename(&replacement_path, &archive_path));
        let replacement_identity =
            native_file_identity(&File::open(&archive_path).expect("open replacement source"))
                .expect("identify replacement source");
        let metadata_equal = stale_bytes.len() == replacement_bytes.len()
            && stale_metadata.len() == replacement_metadata.len()
            && stale_metadata.is_file() == replacement_metadata.is_file();

        let mut source =
            NativeArchiveSource::open(&archive_path, REVISION, ArchiveLimits::default())
                .expect("open replacement before cataloging it");
        let held_identity = source.identity();
        let fresh_catalog = source
            .read_catalog(&ScanCancellation::new())
            .expect("catalog on the retained replacement handle");
        let stale_verification = source.verify_catalog(&stale_catalog);
        let fresh_verification = source.verify_catalog(&fresh_catalog);
        let held_after = source.identity();
        let captured_source = source.archive_source();
        let replacement_length = source.archive_len();
        let archive_source_metadata_equal = stale_catalog.source() == fresh_catalog.source();

        // The source file handle must also prevent a later pathname move from
        // detaching the cataloged bytes from their held physical identity.
        let moved_path = temp.path().join("moved-after-open.zip");
        let rename_after_open = fs::rename(&archive_path, &moved_path);
        let source_still_at_path = archive_path.exists();
        let moved_path_exists = moved_path.exists();
        let source_bytes_after = fs::read(&archive_path).ok();
        let evidence_facts = format!(
            "replacement_installed={} filesystem_metadata_equal={} archive_source_metadata_equal={} identity_changed={} stale_differs={} stale_rejected={} fresh_verified={} held_identity_stable={} move_blocked={} path_unchanged={}",
            install_replacement.is_ok(),
            metadata_equal,
            archive_source_metadata_equal,
            stale_identity != replacement_identity,
            stale_catalog != fresh_catalog,
            stale_verification.is_err(),
            fresh_verification.is_ok(),
            held_identity == held_after && held_after == replacement_identity,
            rename_after_open.is_err(),
            source_still_at_path && !moved_path_exists,
        );
        emit_native_evidence(
            "c06_bound_source_catalogs_on_and_retains_the_same_locked_file",
            &evidence_facts,
        );

        assert!(
            install_replacement.is_ok()
                && metadata_equal
                && archive_source_metadata_equal
                && stale_identity != replacement_identity
                && stale_catalog != fresh_catalog
                && stale_verification.is_err()
                && fresh_verification.is_ok()
                && held_identity == held_after
                && held_after == replacement_identity
                && captured_source == Some(fresh_catalog.source())
                && replacement_length == replacement_bytes.len() as u64
                && rename_after_open.is_err()
                && source_still_at_path
                && !moved_path_exists
                && source_bytes_after.as_deref() == Some(replacement_bytes.as_slice()),
            "open-before-catalog source authority failed: installed={install_replacement:?}, filesystem_metadata_equal={metadata_equal}, archive_source_metadata_equal={archive_source_metadata_equal}, old_identity={stale_identity:?}, new_identity={replacement_identity:?}, stale_verify={stale_verification:?}, fresh_verify={fresh_verification:?}, move_after_open={rename_after_open:?}, path_unchanged={source_still_at_path}, moved={moved_path_exists}, source_len={replacement_length}"
        );
    }

    #[test]
    fn c06_selected_root_supports_descendants_and_competitor_never_overwrites() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        fs::create_dir(selected.join("nested")).expect("create selected descendant");
        let destination =
            NativeArchiveDestination::open(&selected).expect("open selected destination");
        let descendant = destination
            .open_descendant(&["nested"])
            .expect("open held descendant");
        let mut stage = descendant.create_stage().expect("create owned stage");
        let stage_leaf = stage.leaf().to_os_string();
        stage
            .write_all_and_sync(b"staged exact bytes")
            .expect("write and sync stage");

        let competitor = selected.join("nested").join("winner.bin");
        fs::write(&competitor, b"winner already present")
            .expect("create competing winner after stage admission");
        let result = stage.publish(OsStr::new("winner.bin"));
        let failure = primary_failure(&result);
        let winner_after = fs::read(&competitor).ok();
        let stage_after = selected.join("nested").join(stage_leaf).exists();
        let descendant_identity = descendant.verify_identity();
        let published_identity = descendant.leaf_identity("winner.bin");
        let collision_refusal = failure.as_ref().is_some_and(|failure| {
            failure.nt_status == Some(0xc000_0035_u32 as i32)
                || matches!(failure.win32_code, Some(80 | 183))
        });
        emit_native_evidence(
            "c06_selected_root_supports_descendants_and_competitor_never_overwrites",
            &format!(
                "collision_refusal={collision_refusal} winner_unchanged={} stage_removed={} held_identity_stable={}",
                winner_after.as_deref() == Some(b"winner already present".as_slice()),
                !stage_after,
                descendant_identity.as_ref().ok() == Some(&descendant.identity()),
            ),
        );

        assert!(
            collision_refusal
                && result.is_err()
                && winner_after.as_deref() == Some(b"winner already present".as_slice())
                && !stage_after
                && descendant_identity.as_ref().ok() == Some(&descendant.identity())
                && published_identity.is_ok(),
            "no-replace must preserve the admitted competitor and clean only the stage: failure={failure:?}, winner={winner_after:?}, stage_exists={stage_after}, held={descendant_identity:?}, published={published_identity:?}"
        );
    }

    #[test]
    fn c06_root_relative_publication_commits_exact_bytes_without_replacement() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        let destination =
            NativeArchiveDestination::open(&selected).expect("open selected destination");
        let root = destination
            .open_descendant(&[])
            .expect("retain selected root directory");
        let mut stage = root.create_stage().expect("create stage relative to root");
        let bytes = b"exact root-relative publication";
        stage
            .write_all_and_sync(bytes)
            .expect("write and flush exact bytes");
        let expected_identity = stage.identity();
        let result = stage.publish(OsStr::new("published.bin"));
        let failure = primary_failure(&result);
        let actual_bytes = fs::read(selected.join("published.bin")).ok();
        let actual_identity = root.leaf_identity("published.bin");
        let hidden_stages = fs::read_dir(&selected)
            .expect("enumerate destination after publication")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".stage"))
            .count();
        emit_native_evidence(
            "c06_root_relative_publication_commits_exact_bytes_without_replacement",
            &format!(
                "published={} exact_bytes={} identity_matches={} hidden_stages={hidden_stages}",
                result.as_ref().ok() == Some(&expected_identity),
                actual_bytes.as_deref() == Some(bytes.as_slice()),
                actual_identity.as_ref().ok() == Some(&expected_identity),
            ),
        );

        assert!(
            result.as_ref().ok() == Some(&expected_identity)
                && actual_bytes.as_deref() == Some(bytes.as_slice())
                && actual_identity.as_ref().ok() == Some(&expected_identity)
                && hidden_stages == 0,
            "held-root no-replace publication failed: result={result:?}, failure={failure:?}, bytes={actual_bytes:?}, actual_identity={actual_identity:?}, stage_count={hidden_stages}"
        );
    }

    #[test]
    fn c06_stage_identity_failure_reports_cleanup_and_preserves_foreign_files() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        let destination =
            NativeArchiveDestination::open(&selected).expect("open selected destination");
        let root = destination
            .open_descendant(&[])
            .expect("retain selected root directory");
        fs::write(selected.join("winner.bin"), b"foreign winner").expect("create foreign target");
        let mut stage = root.create_stage().expect("create owned stage");
        let stage_leaf = stage.leaf().to_os_string();
        let stage_identity = stage.identity();
        stage
            .write_all_and_sync(b"incomplete stage")
            .expect("write stage bytes");
        stage.force_validation_identity_mismatch();

        let result = stage.publish(OsStr::new("winner.bin"));
        let failure = primary_failure(&result);
        let cleanup_attempted = result
            .as_ref()
            .err()
            .is_some_and(|error| error.cleanup_attempted);
        let cleanup_failure = result
            .as_ref()
            .err()
            .and_then(|error| error.cleanup_failure.clone());
        let winner_after = fs::read(selected.join("winner.bin")).ok();
        let stage_exists = selected.join(stage_leaf).exists();
        let directory_identity = root.verify_identity();
        emit_native_evidence(
            "c06_stage_identity_failure_reports_cleanup_and_preserves_foreign_files",
            &format!(
                "identity_refused={} cleanup_attempted={cleanup_attempted} cleanup_succeeded={} winner_unchanged={} stage_removed={} directory_identity_stable={}",
                failure.as_ref().is_some_and(|failure| {
                    failure.kind == io::ErrorKind::InvalidData
                        && failure.held_identity == Some(stage_identity)
                }),
                cleanup_failure.is_none(),
                winner_after.as_deref() == Some(b"foreign winner".as_slice()),
                !stage_exists,
                directory_identity.as_ref().ok() == Some(&root.identity()),
            ),
        );

        assert!(
            result.is_err()
                && failure.as_ref().is_some_and(|failure| {
                    failure.nt_status.is_none()
                        && failure.win32_code.is_none()
                        && failure.kind == io::ErrorKind::InvalidData
                        && failure.held_identity == Some(stage_identity)
                })
                && cleanup_attempted
                && cleanup_failure.is_none()
                && winner_after.as_deref() == Some(b"foreign winner".as_slice())
                && !stage_exists
                && directory_identity.as_ref().ok() == Some(&root.identity()),
            "stage identity refusal/cleanup contract failed: primary={failure:?}, cleanup_attempted={cleanup_attempted}, cleanup_failure={cleanup_failure:?}, winner={winner_after:?}, stage_exists={stage_exists}, directory={directory_identity:?}"
        );
    }

    #[test]
    fn c06_selected_root_and_relative_operations_confine_reparse_races() {
        let (_temp, selected, _held, outside) = fixture_tree();
        install_outside_winner(&outside);
        let destination =
            NativeArchiveDestination::open(&selected).expect("admit selected root before race");
        let selected_identity = destination.identity();
        let root = destination
            .open_descendant(&[])
            .expect("retain selected root for relative publication");
        let mut junction = None;
        let (result, stage_identity, stage_leaf) = create_stage_and_publish_after_validation(
            &root,
            || {
                junction = Some(set_junction_on_existing_directory(
                    &selected,
                    root.identity(),
                    &outside,
                )?);
                Ok(())
            },
            b"root-relative race output",
            OsStr::new("winner.bin"),
        );

        // Retag the exact admitted directory after open_relative has completed
        // its final identity, attribute, reparse, and name checks, immediately
        // before NtCreateFile(FILE_CREATE). The held identity must stay stable
        // while the selected path routes outside.
        let selected_route = fs::canonicalize(&selected);
        let outside_route = fs::canonicalize(&outside);
        let path_routes_outside = matches!(
            (&selected_route, &outside_route),
            (Ok(selected_route), Ok(outside_route)) if selected_route == outside_route
        );
        let held_identity_after = root.verify_identity();
        let selected_identity_after = destination.verify_identity();
        let held_attributes = query_attribute_tag(
            &root.directory.file,
            "NtQueryInformationFile(FileAttributeTagInformation, root race terminal)",
            selected_identity,
        );
        let held_is_reparse = held_attributes.as_ref().is_ok_and(|attributes| {
            attributes.FileAttributes
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        });
        let outside_winner = fs::read(outside.join("winner.bin")).ok();
        let outside_sentinel = fs::read(outside.join("sentinel.txt")).ok();
        let outside_objects = outside_names(&outside);
        let expected_outside_objects =
            BTreeSet::from([OsString::from("sentinel.txt"), OsString::from("winner.bin")]);
        let outside_objects_match = match &outside_objects {
            Ok(names) => names == &expected_outside_objects,
            Err(_) => false,
        };
        let stage_not_created = stage_identity.is_none() && stage_leaf.is_none();
        let junction_cleanup_ok =
            matches!(junction.as_mut().map(JunctionGuard::clear), Some(Ok(())));
        let selected_route_after_cleanup = fs::canonicalize(&selected);
        let route_restored = matches!(
            (&selected_route_after_cleanup, &outside_route),
            (Ok(selected_route), Ok(outside_route)) if selected_route != outside_route
        );
        let failure = primary_failure(&result);
        let raw_outcome = failure.as_ref().map(|failure| {
            (
                failure.operation,
                failure.nt_status,
                failure.win32_code,
                failure.kind,
                failure.held_identity,
            )
        });
        let safe_refusal = failure.as_ref().is_some_and(|failure| {
            failure.operation == "NtCreateFile(FILE_CREATE owned stage)"
                && failure.nt_status.is_some_and(|status| status < 0)
                && failure.win32_code.is_some()
                && failure.held_identity == Some(selected_identity)
        });
        emit_native_evidence(
            "c06_selected_root_and_relative_operations_confine_reparse_races",
            &format!(
                "raw_outcome={raw_outcome:?} route_outside={path_routes_outside} route_restored={route_restored} held_identity_stable={} held_is_reparse={held_is_reparse} safe_refusal={safe_refusal} stage_not_created={stage_not_created} outside_objects_unchanged={outside_objects_match} winner_unchanged={} sentinel_unchanged={} junction_cleanup={junction_cleanup_ok}",
                held_identity_after.as_ref().ok() == Some(&selected_identity)
                    && selected_identity_after.as_ref().ok() == Some(&selected_identity),
                outside_winner.as_deref() == Some(b"foreign winner".as_slice()),
                outside_sentinel.as_deref() == Some(b"outside sentinel".as_slice()),
            ),
        );

        assert!(
            safe_refusal
                && path_routes_outside
                && held_identity_after.as_ref().ok() == Some(&selected_identity)
                && selected_identity_after.as_ref().ok() == Some(&selected_identity)
                && held_is_reparse
                && result.is_err()
                && stage_not_created
                && outside_winner.as_deref() == Some(b"foreign winner".as_slice())
                && outside_sentinel.as_deref() == Some(b"outside sentinel".as_slice())
                && outside_objects_match
                && junction_cleanup_ok
                && route_restored,
            "selected-root reparse race escaped held authority: raw_outcome={raw_outcome:?}, path_routes_outside={path_routes_outside}, route_restored={route_restored}, held_attributes_error={:?}, held_after={held_identity_after:?}, selected_after={selected_identity_after:?}, held_is_reparse={held_is_reparse}, result_error={:?}, stage_not_created={stage_not_created}, outside_winner={outside_winner:?}, outside_sentinel={outside_sentinel:?}, outside_objects={outside_objects:?}, junction_cleanup={junction_cleanup_ok}",
            held_attributes.as_ref().err(),
            result.as_ref().err().map(|error| error.primary()),
        );
    }

    #[test]
    fn c06_descendant_and_relative_operations_confine_reparse_races() {
        let (_temp, selected, _held, outside) = fixture_tree();
        let child_path = selected.join("child");
        fs::create_dir(&child_path).expect("create selected child before admission");
        install_outside_winner(&outside);
        let destination = NativeArchiveDestination::open(&selected)
            .expect("admit selected root before child race");
        let root_identity = destination.identity();
        let child = destination
            .open_descendant(&["child"])
            .expect("admit held child before race");
        let child_identity = child.identity();
        let mut junction = None;
        let (result, stage_identity, stage_leaf) = create_stage_and_publish_after_validation(
            &child,
            || {
                junction = Some(set_junction_on_existing_directory(
                    &child_path,
                    child.identity(),
                    &outside,
                )?);
                Ok(())
            },
            b"child-relative race output",
            OsStr::new("winner.bin"),
        );

        // The admitted child itself becomes a junction after open_relative's
        // final checks and immediately before NtCreateFile(FILE_CREATE); no
        // directory rename or replacement is involved.
        let child_route = fs::canonicalize(&child_path);
        let outside_route = fs::canonicalize(&outside);
        let path_routes_outside = matches!(
            (&child_route, &outside_route),
            (Ok(child_route), Ok(outside_route)) if child_route == outside_route
        );
        let held_child_after = child.verify_identity();
        let held_root_after = destination.verify_identity();
        let held_attributes = query_attribute_tag(
            &child.directory.file,
            "NtQueryInformationFile(FileAttributeTagInformation, child race terminal)",
            child_identity,
        );
        let held_is_reparse = held_attributes.as_ref().is_ok_and(|attributes| {
            attributes.FileAttributes
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        });
        let outside_winner = fs::read(outside.join("winner.bin")).ok();
        let outside_sentinel = fs::read(outside.join("sentinel.txt")).ok();
        let outside_objects = outside_names(&outside);
        let expected_outside_objects =
            BTreeSet::from([OsString::from("sentinel.txt"), OsString::from("winner.bin")]);
        let outside_objects_match = match &outside_objects {
            Ok(names) => names == &expected_outside_objects,
            Err(_) => false,
        };
        let stage_not_created = stage_identity.is_none() && stage_leaf.is_none();
        let junction_cleanup_ok =
            matches!(junction.as_mut().map(JunctionGuard::clear), Some(Ok(())));
        let child_route_after_cleanup = fs::canonicalize(&child_path);
        let route_restored = matches!(
            (&child_route_after_cleanup, &outside_route),
            (Ok(child_route), Ok(outside_route)) if child_route != outside_route
        );
        let failure = primary_failure(&result);
        let raw_outcome = failure.as_ref().map(|failure| {
            (
                failure.operation,
                failure.nt_status,
                failure.win32_code,
                failure.kind,
                failure.held_identity,
            )
        });
        let safe_refusal = failure.as_ref().is_some_and(|failure| {
            failure.operation == "NtCreateFile(FILE_CREATE owned stage)"
                && failure.nt_status.is_some_and(|status| status < 0)
                && failure.win32_code.is_some()
                && failure.held_identity == Some(child_identity)
        });
        emit_native_evidence(
            "c06_descendant_and_relative_operations_confine_reparse_races",
            &format!(
                "raw_outcome={raw_outcome:?} route_outside={path_routes_outside} route_restored={route_restored} held_identity_stable={} held_is_reparse={held_is_reparse} safe_refusal={safe_refusal} stage_not_created={stage_not_created} outside_objects_unchanged={outside_objects_match} winner_unchanged={} sentinel_unchanged={} junction_cleanup={junction_cleanup_ok}",
                held_child_after.as_ref().ok() == Some(&child_identity)
                    && held_root_after.as_ref().ok() == Some(&root_identity),
                outside_winner.as_deref() == Some(b"foreign winner".as_slice()),
                outside_sentinel.as_deref() == Some(b"outside sentinel".as_slice()),
            ),
        );

        assert!(
            safe_refusal
                && path_routes_outside
                && held_child_after.as_ref().ok() == Some(&child_identity)
                && held_root_after.as_ref().ok() == Some(&root_identity)
                && held_is_reparse
                && result.is_err()
                && stage_not_created
                && outside_winner.as_deref() == Some(b"foreign winner".as_slice())
                && outside_sentinel.as_deref() == Some(b"outside sentinel".as_slice())
                && outside_objects_match
                && junction_cleanup_ok
                && route_restored,
            "descendant reparse race escaped held authority: raw_outcome={raw_outcome:?}, path_routes_outside={path_routes_outside}, route_restored={route_restored}, held_attributes_error={:?}, held_child={held_child_after:?}, held_root={held_root_after:?}, held_is_reparse={held_is_reparse}, result_error={:?}, stage_not_created={stage_not_created}, outside_winner={outside_winner:?}, outside_sentinel={outside_sentinel:?}, outside_objects={outside_objects:?}, junction_cleanup={junction_cleanup_ok}",
            held_attributes.as_ref().err(),
            result.as_ref().err().map(|error| error.primary()),
        );
    }

    #[test]
    fn transfer_admission_refuses_a_retagged_parent_and_never_descends() {
        use crate::file_operations::ScanCancellation;
        use crate::file_transfer::{
            AdmissionError, TargetOccupancy, TransferLeaf, inspect_parent, inspect_source,
            inspect_target, recheck_target_metadata,
        };

        let (_temp, selected, _held, outside) = fixture_tree();
        let sentinel = outside.join("sentinel.txt");
        fs::write(&sentinel, b"outside sentinel").unwrap();
        let cancellation = ScanCancellation::new();
        let parent = inspect_parent(&selected, &cancellation).unwrap();
        let leaf = TransferLeaf::new(OsStr::new("sentinel.txt")).unwrap();
        let approved = inspect_target(&parent, &leaf, &cancellation).unwrap();
        assert_eq!(approved.occupancy(), &TargetOccupancy::Vacant);

        let mut junction =
            set_junction_on_existing_directory(&selected, parent.identity(), &outside).unwrap();
        assert!(matches!(
            inspect_source(&selected.join(leaf.as_os_str()), &cancellation),
            Err(AdmissionError::ReparsePoint)
        ));
        assert!(matches!(
            recheck_target_metadata(&approved, &cancellation),
            Err(AdmissionError::ReparsePoint)
        ));
        junction.clear().unwrap();
        assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
        recheck_target_metadata(&approved, &cancellation).unwrap();
    }

    #[test]
    fn transfer_tree_records_junction_as_unavailable_without_listing_its_target() {
        use crate::file_operations::ScanCancellation;
        use crate::file_transfer::{AdmissionError, SourceObservation, inspect_parent, inspect_source_tree};

        let (_temp, selected, _held, outside) = fixture_tree();
        let outside_file = outside.join("external.txt");
        fs::write(&outside_file, b"external payload").unwrap();
        let local_file = selected.join("local.txt");
        fs::write(&local_file, b"local payload").unwrap();
        let link = selected.join("link");
        fs::create_dir(&link).unwrap();
        let cancellation = ScanCancellation::new();
        let identity = inspect_parent(&link, &cancellation).unwrap().identity();
        let mut junction = set_junction_on_existing_directory(&link, identity, &outside).unwrap();

        let tree = inspect_source_tree(&[selected], &cancellation).unwrap();
        assert_eq!(tree.entries().len(), 3);
        assert_eq!(tree.entries()[1].path().file_name(), Some(OsStr::new("link")));
        assert!(matches!(
            tree.entries()[1].observation(),
            SourceObservation::Unavailable(AdmissionError::ReparsePoint)
        ));
        assert_eq!(tree.entries()[2].path().file_name(), Some(OsStr::new("local.txt")));
        junction.clear().unwrap();
        assert_eq!(fs::read(outside_file).unwrap(), b"external payload");
    }

    #[test]
    fn transfer_execute_native_directory_preflight_has_definite_no_effect() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        let (root, parts) = split_local_absolute_path(&selected).unwrap();
        let mut parent = transfer_native::TransferDirectory::open_root(&root).unwrap();
        for (index, part) in parts.iter().enumerate() {
            parent = if index + 1 == parts.len() {
                parent.open_create_directory_child(part).unwrap()
            } else {
                parent.open_child(part).unwrap()
            };
        }

        let Err(refused) = parent.create_directory(OsStr::new("invalid\\nested")) else {
            panic!("the native boundary accepted a nonliteral directory name");
        };
        assert_eq!(refused.outcome, transfer_native::TransferDirectoryCreateOutcome::NotCreated);
        assert_eq!(fs::read_dir(selected).unwrap().count(), 0);
    }

    #[test]
    fn transfer_copy_native_stage_name_excludes_the_exact_final_name() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        let (root, parts) = split_local_absolute_path(&selected).unwrap();
        let mut parent = transfer_native::TransferDirectory::open_root(&root).unwrap();
        for (index, name) in parts.iter().enumerate() {
            parent = if index + 1 == parts.len() {
                parent.open_create_file_child(name).unwrap()
            } else {
                parent.open_child(name).unwrap()
            };
        }
        let mut names = [OsString::from("FINAL.STAGE"), OsString::from("distinct.stage")].into_iter();
        let stage = parent.create_stage_with_names(
            OsStr::new("Final.Stage"),
            || names.next().expect("reserved and distinct stage names"),
        ).unwrap();
        assert_eq!(stage.leaf(), OsStr::new("distinct.stage"));
        assert!(!selected.join("Final.Stage").exists());
        assert!(selected.join("distinct.stage").exists());
        stage.abort().unwrap();
        assert_eq!(fs::read_dir(selected).unwrap().count(), 0);
    }

    #[test]
    fn transfer_copy_native_uncertain_rename_keeps_the_actual_published_file() {
        let (_temp, selected, _held, _outside) = fixture_tree();
        let directory = NativeArchiveDestination::open(&selected).unwrap().open_descendant(&[]).unwrap();
        let mut stage = directory.create_stage().unwrap();
        let saved_identity = stage.identity();
        stage.write_all_and_sync(b"completed transfer").unwrap();
        let old_private = selected.join(stage.leaf());
        let final_leaf = OsStr::new("published.txt");
        rename_open_file_relative_no_replace(
            stage.file.as_file(), &directory.directory, final_leaf, saved_identity,
        ).unwrap();
        // Inject a failed status at the native result boundary after the
        // real rename. Recovery must leave the held object at its new name.
        let injected = NativeFailure::validation(
            "fixture interrupted rename observation", io::ErrorKind::Other, Some(saved_identity),
        );
        let result = stage.finish_rename_failure(injected, true);
        assert!(result.unconfirmed_name_effect());
        assert!(!result.cleanup_attempted);
        drop(stage);
        assert!(!old_private.exists());
        let published = selected.join(final_leaf);
        assert_eq!(fs::read(&published).unwrap(), b"completed transfer");
        assert_eq!(native_file_identity(&File::open(published).unwrap()).unwrap(), saved_identity);
    }

    #[test]
    fn transfer_admission_distinguishes_definitive_leaf_absence_from_other_failures() {
        use windows::Win32::Foundation::{
            STATUS_ACCESS_DENIED, STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND,
            STATUS_SHARING_VIOLATION,
        };

        let refusal = |status| NativeArchiveError::from_ntstatus("transfer test", status, None);
        assert!(transfer_native::is_missing_relative_leaf(&refusal(
            STATUS_OBJECT_NAME_NOT_FOUND
        )));
        for status in [STATUS_OBJECT_PATH_NOT_FOUND, STATUS_ACCESS_DENIED, STATUS_SHARING_VIOLATION] {
            assert!(!transfer_native::is_missing_relative_leaf(&refusal(status)));
        }
    }

    #[test]
    fn c06_reparse_destination_and_source_ancestors_are_rejected_without_escape() {
        let (root_temp, selected, _held, root_outside) = fixture_tree();
        let archive_path = root_outside.join("source.zip");
        write_zip(&archive_path);
        install_outside_winner(&root_outside);
        let root_identity = NativeArchiveDestination::open(&selected)
            .expect("admit selected root before reparse test")
            .identity();
        let mut root_junction = Some(
            set_junction_on_existing_directory(&selected, root_identity, &root_outside)
                .expect("retag existing selected root as a junction"),
        );
        let destination_result = NativeArchiveDestination::open(&selected);
        let source_result = NativeArchiveSource::open(
            &selected.join("source.zip"),
            REVISION,
            ArchiveLimits::default(),
        );
        let root_names = outside_names(&root_outside);
        let root_sentinel = fs::read(root_outside.join("sentinel.txt")).ok();
        let expected_root_names = BTreeSet::from([
            OsString::from("sentinel.txt"),
            OsString::from("source.zip"),
            OsString::from("winner.bin"),
        ]);
        let root_names_match = matches!(&root_names, Ok(names) if names == &expected_root_names);
        let root_cleanup_ok = matches!(
            root_junction.as_mut().map(JunctionGuard::clear),
            Some(Ok(()))
        );
        drop(root_junction);
        drop(root_temp);

        let (child_temp, child_selected, _child_held, child_outside) = fixture_tree();
        install_outside_winner(&child_outside);
        let child_link = child_selected.join("child-link");
        fs::create_dir(&child_link).expect("create ordinary descendant before retag");
        let ordinary_root = NativeArchiveDestination::open(&child_selected)
            .expect("open ordinary selected root for descendant test");
        let child_identity = ordinary_root
            .open_descendant(&["child-link"])
            .expect("admit ordinary descendant before reparse test")
            .identity();
        let mut child_junction = Some(
            set_junction_on_existing_directory(&child_link, child_identity, &child_outside)
                .expect("retag existing descendant as a junction"),
        );
        let descendant_result = ordinary_root.open_descendant(&["child-link"]);
        let child_names = outside_names(&child_outside);
        let child_sentinel = fs::read(child_outside.join("sentinel.txt")).ok();
        let expected_child_names =
            BTreeSet::from([OsString::from("sentinel.txt"), OsString::from("winner.bin")]);
        let child_names_match = matches!(&child_names, Ok(names) if names == &expected_child_names);
        let child_cleanup_ok = matches!(
            child_junction.as_mut().map(JunctionGuard::clear),
            Some(Ok(()))
        );
        drop(child_junction);
        drop(ordinary_root);
        drop(child_temp);

        let destination_failure = primary_failure(&destination_result);
        let source_failure = primary_failure(&source_result);
        let descendant_failure = primary_failure(&descendant_result);
        emit_native_evidence(
            "c06_reparse_destination_and_source_ancestors_are_rejected_without_escape",
            &format!(
                "root_destination={destination_failure:?} source_ancestor={source_failure:?} root_objects_unchanged={root_names_match} root_sentinel_unchanged={} root_cleanup={root_cleanup_ok} descendant={descendant_failure:?} child_objects_unchanged={child_names_match} child_sentinel_unchanged={} child_cleanup={child_cleanup_ok}",
                root_sentinel.as_deref() == Some(b"outside sentinel".as_slice()),
                child_sentinel.as_deref() == Some(b"outside sentinel".as_slice()),
            ),
        );

        assert!(
            destination_failure
                .as_ref()
                .is_some_and(|failure| failure.kind == io::ErrorKind::InvalidData)
                && source_failure
                    .as_ref()
                    .is_some_and(|failure| failure.kind == io::ErrorKind::InvalidData)
                && descendant_failure
                    .as_ref()
                    .is_some_and(|failure| failure.kind == io::ErrorKind::InvalidData)
                && root_names_match
                && child_names_match
                && root_sentinel.as_deref() == Some(b"outside sentinel".as_slice())
                && child_sentinel.as_deref() == Some(b"outside sentinel".as_slice())
                && root_cleanup_ok
                && child_cleanup_ok,
            "reparse admission or owned cleanup failed: destination={destination_failure:?}, source={source_failure:?}, descendant={descendant_failure:?}, root_names={root_names:?}, child_names={child_names:?}, sentinels={root_sentinel:?}/{child_sentinel:?}, cleanup={root_cleanup_ok}/{child_cleanup_ok}"
        );
    }
}

/// Narrow read-only handle-relative primitives for transfer metadata proofs.
pub(crate) mod transfer_native {
    use super::*;

    pub(crate) use super::{NativeArchiveError, split_local_absolute_path};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum TransferDirectoryCreateOutcome {
        NotCreated,
        Observed,
        Ambiguous,
    }
    #[derive(Debug)]
    pub(crate) struct TransferDirectoryCreateFailure {
        pub(crate) error: NativeArchiveError,
        pub(crate) outcome: TransferDirectoryCreateOutcome,
    }

    pub(crate) struct TransferDirectory(Arc<HeldDirectory>);

    impl TransferDirectory {
        pub(crate) fn open_root(root: &Path) -> Result<Self, NativeArchiveError> {
            Self::open_root_with_list(root, false)
        }
        pub(crate) fn open_listable_root(root: &Path) -> Result<Self, NativeArchiveError> {
            Self::open_root_with_list(root, true)
        }
        fn open_root_with_list(root: &Path, list: bool) -> Result<Self, NativeArchiveError> {
            let (validated_root, components) = split_local_absolute_path(root)?;
            if !components.is_empty() {
                return Err(NativeArchiveError::validation(
                    "transfer root must be a drive root",
                    io::ErrorKind::InvalidInput,
                    None,
                ));
            }
            open_volume_root_with_options(&validated_root, false, false, true, list).map(Self)
        }

        pub(crate) fn file(&self) -> &File {
            &self.0.file
        }
        pub(crate) fn identity(&self) -> NativeFileIdentity {
            self.0.identity
        }
        pub(crate) fn verify_identity(&self) -> Result<NativeFileIdentity, NativeArchiveError> {
            self.0.verify_identity()
        }
        pub(crate) fn open_child(&self, name: &OsStr) -> Result<Self, NativeArchiveError> {
            self.0
                .open_child_with_options(name, false, false, true, false, false)
                .map(Self)
        }
        pub(crate) fn open_listable_child(&self, name: &OsStr) -> Result<Self, NativeArchiveError> {
            self.0
                .open_child_with_options(name, false, false, true, true, false)
                .map(Self)
        }
        /// Opens the final destination directory with create-file access only.
        pub(crate) fn open_create_file_child(
            &self,
            name: &OsStr,
        ) -> Result<Self, NativeArchiveError> {
            self.0
                .open_child_with_options(name, true, false, true, false, false)
                .map(Self)
        }
        /// Grants directory creation only on this final child.
        pub(crate) fn open_create_directory_child(
            &self,
            name: &OsStr,
        ) -> Result<Self, NativeArchiveError> {
            self.0
                .open_child_with_options(name, false, true, true, false, false)
                .map(Self)
        }
        /// Grants metadata updates only on this final child.
        pub(crate) fn open_metadata_write_child(
            &self,
            name: &OsStr,
        ) -> Result<Self, NativeArchiveError> {
            self.0
                .open_child_with_options(name, false, false, true, false, true)
                .map(Self)
        }
        pub(crate) fn open_create_directory_root(root: &Path) -> Result<Self, NativeArchiveError> {
            let (root, parts) = split_local_absolute_path(root)?;
            if !parts.is_empty() {
                return Err(NativeArchiveError::validation(
                    "transfer root must be a drive root",
                    io::ErrorKind::InvalidInput,
                    None,
                ));
            }
            open_volume_root_with_options(&root, false, true, true, false).map(Self)
        }
        /// Exclusively creates a child; failures never remove or merge entries.
        pub(crate) fn create_directory(
            &self,
            name: &OsStr,
        ) -> Result<Self, TransferDirectoryCreateFailure> {
            let mut observed = false;
            self.0
                .create_child_observed(name, || observed = true)
                .map(Self)
                .map_err(|error| {
                    let outcome = if observed {
                        TransferDirectoryCreateOutcome::Observed
                    } else if error.unconfirmed_name_effect {
                        TransferDirectoryCreateOutcome::Ambiguous
                    } else {
                        TransferDirectoryCreateOutcome::NotCreated
                    };
                    TransferDirectoryCreateFailure { error, outcome }
                })
        }
        pub(crate) fn open_create_file_root(root: &Path) -> Result<Self, NativeArchiveError> {
            let (root, parts) = split_local_absolute_path(root)?;
            if !parts.is_empty() {
                return Err(NativeArchiveError::validation(
                    "transfer root must be a drive root",
                    io::ErrorKind::InvalidInput,
                    None,
                ));
            }
            open_volume_root_with_options(&root, true, false, true, false).map(Self)
        }
        pub(crate) fn open_relocation(
            &self,
            leaf: &OsStr,
            expected: NativeFileIdentity,
        ) -> Result<TransferRelocation, NativeArchiveError> {
            use windows::Wdk::Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT,
            };
            use windows::Win32::Storage::FileSystem::{
                DELETE, FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, SYNCHRONIZE,
            };
            validate_single_component(leaf, "transfer relocation source")?;
            let (file, _) = open_relative(
                &self.0,
                leaf,
                DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ,
                FILE_OPEN,
                FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(transfer relocation)",
                || Ok(()),
            )?;
            let relocation = TransferRelocation {
                file,
                identity: expected,
                parent: self.0.clone(),
            };
            relocation.validate()?;
            Ok(relocation)
        }
        pub(crate) fn open_read(&self, name: &OsStr) -> Result<TransferFile, NativeArchiveError> {
            use windows::Wdk::Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT,
            };
            use windows::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ,
                SYNCHRONIZE,
            };
            validate_single_component(name, "transfer source name")?;
            let (file, _) = open_relative(
                &self.0,
                name,
                FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ,
                FILE_OPEN,
                FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT,
                FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(transfer source)",
                || Ok(()),
            )?;
            let metadata = file.metadata().map_err(|e| {
                NativeArchiveError::from_io(
                    "File::metadata(transfer source)",
                    e,
                    Some(self.0.identity),
                )
            })?;
            let identity = native_file_identity(&file).map_err(|e| {
                NativeArchiveError::from_io(
                    "FILE_ID_INFO(transfer source)",
                    e,
                    Some(self.0.identity),
                )
            })?;
            if !metadata.is_file()
                || metadata.file_attributes()
                    & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                    != 0
                || identity.volume_serial_number() != self.0.identity.volume_serial_number()
            {
                return Err(NativeArchiveError::validation(
                    "transfer source is not an ordinary local file",
                    io::ErrorKind::InvalidData,
                    Some(identity),
                ));
            }
            let file = VerifiedOverlappedFile::verify(file).map_err(|_| {
                NativeArchiveError::validation(
                    "transfer source is not verified overlapped",
                    io::ErrorKind::InvalidData,
                    Some(identity),
                )
            })?;
            Ok(TransferFile { file, identity })
        }
        pub(crate) fn create_stage(
            &self,
            final_leaf: &OsStr,
        ) -> Result<TransferStage, TransferStageCreateFailure> {
            self.create_stage_with_names(final_leaf, default_stage_leaf)
        }

        pub(crate) fn create_stage_exact(
            &self,
            final_leaf: &OsStr,
            candidate: &OsStr,
        ) -> Result<TransferStage, TransferStageCreateFailure> {
            validate_single_component(candidate, "transfer stage candidate").map_err(|error| {
                TransferStageCreateFailure {
                    error,
                    unresolved: None,
                }
            })?;
            self.create_stage_with_limit(final_leaf, || candidate.to_os_string(), 1)
        }
        pub(super) fn create_stage_with_names(
            &self,
            final_leaf: &OsStr,
            next_leaf: impl FnMut() -> OsString,
        ) -> Result<TransferStage, TransferStageCreateFailure> {
            self.create_stage_with_limit(final_leaf, next_leaf, STAGE_ATTEMPTS)
        }
        fn create_stage_with_limit(
            &self,
            final_leaf: &OsStr,
            next_leaf: impl FnMut() -> OsString,
            attempts: usize,
        ) -> Result<TransferStage, TransferStageCreateFailure> {
            use windows::Win32::Storage::FileSystem::{FILE_ACCESS_RIGHTS, FILE_SHARE_MODE};
            let mut observation = None;
            let result = (|| {
                validate_single_component(final_leaf, "transfer destination name")?;
                self.0.verify_identity()?;
                OwnedStage::create_with_options(
                    self.0.clone(),
                    || Ok(()),
                    FILE_ACCESS_RIGHTS(0),
                    FILE_SHARE_MODE(0),
                    windows::Wdk::Storage::FileSystem::FILE_WRITE_THROUGH,
                    Some(TransferStageAdmission {
                        final_leaf,
                        observation: &mut observation,
                    }),
                    next_leaf,
                    attempts,
                )
            })();
            result.map(TransferStage).map_err(|error| {
                let unresolved = observation
                    .filter(|observation| error.cleanup_failed() || !observation.creation_confirmed)
                    .map(|observation| TransferStageAttempt {
                        leaf: observation.leaf,
                        identity: observation.identity,
                    });
                TransferStageCreateFailure { error, unresolved }
            })
        }
        pub(crate) fn open_source_zone(
            &self,
            leaf: &OsStr,
            expected: NativeFileIdentity,
        ) -> Result<TransferFile, NativeArchiveError> {
            use windows::Wdk::Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT,
            };
            use windows::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ,
                SYNCHRONIZE,
            };
            let name = zone_name(leaf)?;
            verify_base(&self.0, leaf, expected)?;
            let (file, _) = open_relative(
                &self.0,
                &name,
                FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_READ,
                FILE_OPEN,
                FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT,
                FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(transfer Zone.Identifier read)",
                || Ok(()),
            )?;
            let file = verify_zone_file(file, expected)?;
            verify_base(&self.0, leaf, expected)?;
            Ok(file)
        }
        pub(crate) fn open_relative_metadata(
            &self,
            name: &OsStr,
        ) -> Result<File, NativeArchiveError> {
            use windows::Wdk::Storage::FileSystem::{
                FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT,
                FILE_SYNCHRONOUS_IO_NONALERT,
            };
            use windows::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, SYNCHRONIZE,
            };
            validate_single_component(name, "transfer relative metadata name")?;
            open_relative(
                &self.0,
                name,
                FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                shared_directory_access(),
                FILE_OPEN,
                FILE_OPEN_REPARSE_POINT | FILE_OPEN_NO_RECALL | FILE_SYNCHRONOUS_IO_NONALERT,
                FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(transfer metadata)",
                || Ok(()),
            )
            .map(|(file, _)| file)
        }
    }

    fn zone_name(leaf: &OsStr) -> Result<OsString, NativeArchiveError> {
        validate_single_component(leaf, "transfer Zone.Identifier base name")?;
        let mut name = leaf.to_os_string();
        name.push(":Zone.Identifier:$DATA");
        Ok(name)
    }
    fn verify_base(
        parent: &Arc<HeldDirectory>,
        leaf: &OsStr,
        expected: NativeFileIdentity,
    ) -> Result<(), NativeArchiveError> {
        use windows::Wdk::Storage::FileSystem::{
            FILE_OPEN, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        };
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, SYNCHRONIZE,
        };
        validate_single_component(leaf, "transfer Zone.Identifier base name")?;
        let (file, _) = open_relative(
            parent,
            leaf,
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            shared_directory_access(),
            FILE_OPEN,
            FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            FILE_ATTRIBUTE_NORMAL,
            "NtCreateFile(transfer Zone.Identifier base)",
            || Ok(()),
        )?;
        let metadata = file.metadata().map_err(|e| {
            NativeArchiveError::from_io(
                "File::metadata(transfer Zone.Identifier base)",
                e,
                Some(expected),
            )
        })?;
        let actual = native_file_identity(&file).map_err(|e| {
            NativeArchiveError::from_io(
                "FILE_ID_INFO(transfer Zone.Identifier base)",
                e,
                Some(expected),
            )
        })?;
        if actual != expected
            || !metadata.is_file()
            || metadata.file_attributes()
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        {
            return Err(NativeArchiveError::validation(
                "transfer Zone.Identifier base changed or is not ordinary",
                io::ErrorKind::InvalidData,
                Some(actual),
            ));
        }
        Ok(())
    }
    fn verify_zone_file(
        file: File,
        expected: NativeFileIdentity,
    ) -> Result<TransferFile, NativeArchiveError> {
        let metadata = file.metadata().map_err(|e| {
            NativeArchiveError::from_io(
                "File::metadata(transfer Zone.Identifier)",
                e,
                Some(expected),
            )
        })?;
        let actual = native_file_identity(&file).map_err(|e| {
            NativeArchiveError::from_io("FILE_ID_INFO(transfer Zone.Identifier)", e, Some(expected))
        })?;
        if actual != expected
            || !metadata.is_file()
            || metadata.file_attributes()
                & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0
        {
            return Err(NativeArchiveError::validation(
                "transfer Zone.Identifier does not match the ordinary base",
                io::ErrorKind::InvalidData,
                Some(actual),
            ));
        }
        let file = VerifiedOverlappedFile::verify(file).map_err(|_| {
            NativeArchiveError::validation(
                "transfer Zone.Identifier is not verified overlapped",
                io::ErrorKind::InvalidData,
                Some(actual),
            )
        })?;
        Ok(TransferFile {
            file,
            identity: actual,
        })
    }

    /// Owns the exact source handle and its parent; dropping never deletes it.
    pub(crate) struct TransferRelocation {
        file: File,
        identity: NativeFileIdentity,
        parent: Arc<HeldDirectory>,
    }
    impl TransferRelocation {
        pub(crate) fn file(&self) -> &File {
            &self.file
        }
        pub(crate) fn identity(&self) -> NativeFileIdentity {
            self.identity
        }
        fn validate(&self) -> Result<(), NativeArchiveError> {
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::Storage::FileSystem::{
                FILE_STANDARD_INFO, FileStandardInfo, GetFileInformationByHandleEx,
            };
            self.parent.verify_identity()?;
            let actual = native_file_identity(&self.file).map_err(|e| {
                NativeArchiveError::from_io(
                    "FILE_ID_INFO(transfer relocation)",
                    e,
                    Some(self.identity),
                )
            })?;
            let metadata = self.file.metadata().map_err(|e| {
                NativeArchiveError::from_io("File::metadata(transfer relocation)", e, Some(actual))
            })?;
            let mut standard = FILE_STANDARD_INFO::default();
            unsafe {
                GetFileInformationByHandleEx(
                    HANDLE(self.file.as_raw_handle() as _),
                    FileStandardInfo,
                    std::ptr::from_mut(&mut standard).cast(),
                    std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
                )
            }
            .map_err(|e| {
                NativeArchiveError::from_win32(
                    "FileStandardInfo(transfer relocation)",
                    e,
                    Some(actual),
                )
            })?;
            if actual != self.identity
                || actual.volume_serial_number() != self.parent.identity.volume_serial_number()
                || actual.file_id() == [0; 16]
                || !metadata.is_file()
                || metadata.file_attributes()
                    & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                    != 0
                || crate::file_operations::has_cloud_placeholder_attributes(
                    metadata.file_attributes(),
                )
                || standard.NumberOfLinks != 1
                || standard.DeletePending.0 != 0
            {
                return Err(NativeArchiveError::validation(
                    "transfer relocation is not a single-link ordinary local file",
                    io::ErrorKind::InvalidData,
                    Some(actual),
                ));
            }
            Ok(())
        }
        pub(crate) fn rename_no_replace(
            &self,
            target: &TransferDirectory,
            leaf: &OsStr,
        ) -> Result<(), NativeArchiveError> {
            validate_single_component(leaf, "transfer relocation target")?;
            self.validate()?;
            target.0.verify_identity()?;
            if self.identity.volume_serial_number() != target.0.identity.volume_serial_number() {
                return Err(NativeArchiveError::validation(
                    "transfer relocation destination is on another volume",
                    io::ErrorKind::InvalidInput,
                    Some(self.identity),
                ));
            }
            rename_open_file_relative_no_replace(&self.file, &target.0, leaf, self.identity)
                .map_err(|primary| {
                    let confirmed_collision = primary.nt_status == Some(
                        windows::Win32::Foundation::STATUS_OBJECT_NAME_COLLISION.0,
                    );
                    NativeArchiveError {
                        primary,
                        cleanup_attempted: false,
                        cleanup_failure: None,
                        cleanup_failed: false,
                        unconfirmed_name_effect: !confirmed_collision,
                    }
                })
        }
    }

    pub(crate) struct TransferFile {
        file: VerifiedOverlappedFile,
        identity: NativeFileIdentity,
    }
    impl TransferFile {
        pub(crate) fn file(&self) -> &File {
            self.file.as_file()
        }
        pub(crate) fn identity(&self) -> NativeFileIdentity {
            self.identity
        }
    }
    impl TransferFile {
        pub(crate) fn flush(&self) -> Result<(), NativeArchiveError> {
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::Storage::FileSystem::FlushFileBuffers;
            let actual = native_file_identity(self.file.as_file()).map_err(|e| {
                NativeArchiveError::from_io("FILE_ID_INFO(transfer flush)", e, Some(self.identity))
            })?;
            if actual != self.identity {
                return Err(NativeArchiveError::validation(
                    "transfer file identity changed before flush",
                    io::ErrorKind::InvalidData,
                    Some(actual),
                ));
            }
            unsafe { FlushFileBuffers(HANDLE(self.file.as_file().as_raw_handle() as _)) }.map_err(
                |e| {
                    NativeArchiveError::from_win32(
                        "FlushFileBuffers(transfer file)",
                        e,
                        Some(actual),
                    )
                },
            )
        }
    }
    #[derive(Clone)]
    pub(crate) struct TransferIo(ArchiveIoControl);
    impl TransferIo {
        pub(crate) fn new(cancellation: ScanCancellation) -> Self {
            Self(ArchiveIoControl::new(cancellation))
        }
        pub(crate) fn cancel(&self) {
            self.0.cancel();
        }
        pub(crate) fn is_cancelled(&self) -> bool {
            self.0.is_cancelled()
        }
        pub(crate) fn write_at(
            &self,
            file: &TransferFile,
            offset: u64,
            bytes: Box<[u8]>,
        ) -> io::Result<usize> {
            self.0.write_at(&file.file, offset, bytes)
        }
        pub(crate) fn read_at(
            &self,
            file: &TransferFile,
            offset: u64,
            length: usize,
        ) -> io::Result<(Box<[u8]>, usize)> {
            self.0.read_at(&file.file, offset, length)
        }
    }
    #[derive(Debug)]
    pub(crate) struct TransferStageAttempt {
        pub(crate) leaf: OsString,
        pub(crate) identity: Option<NativeFileIdentity>,
    }

    #[derive(Debug)]
    pub(crate) struct TransferStageCreateFailure {
        pub(crate) error: NativeArchiveError,
        pub(crate) unresolved: Option<TransferStageAttempt>,
    }

    pub(crate) struct TransferStage(OwnedStage);
    impl TransferStage {
        pub(crate) fn leaf(&self) -> &OsStr {
            &self.0.leaf
        }
        pub(crate) fn identity(&self) -> NativeFileIdentity {
            self.0.identity()
        }
        pub(crate) fn file(&self) -> &File {
            self.0.file.as_file()
        }
        pub(crate) fn write(
            &self,
            io: &TransferIo,
            offset: u64,
            bytes: &[u8],
        ) -> Result<usize, NativeArchiveError> {
            self.0.write_chunk(&io.0, offset, bytes)
        }
        pub(crate) fn flush(&self) -> Result<(), NativeArchiveError> {
            self.0.flush_final()
        }
        pub(crate) fn create_zone(&self) -> Result<TransferFile, NativeArchiveError> {
            use windows::Wdk::Storage::FileSystem::{
                FILE_CREATE, FILE_OPEN_NO_RECALL, FILE_OPEN_REPARSE_POINT, FILE_WRITE_THROUGH,
            };
            use windows::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
                FILE_WRITE_DATA, SYNCHRONIZE,
            };
            let expected = self.0.identity;
            let name = zone_name(&self.0.leaf)?;
            verify_base(&self.0.parent, &self.0.leaf, expected)?;
            let (file, information) = open_relative(
                &self.0.parent,
                &name,
                FILE_WRITE_DATA | FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                FILE_SHARE_DELETE,
                FILE_CREATE,
                FILE_OPEN_NO_RECALL | FILE_OPEN_REPARSE_POINT | FILE_WRITE_THROUGH,
                FILE_ATTRIBUTE_NORMAL,
                "NtCreateFile(transfer Zone.Identifier create)",
                || Ok(()),
            )?;
            if information != 2 {
                return Err(NativeArchiveError::validation(
                    "transfer Zone.Identifier was not created",
                    io::ErrorKind::InvalidData,
                    Some(expected),
                ));
            }
            let file = verify_zone_file(file, expected)?;
            verify_base(&self.0.parent, &self.0.leaf, expected)?;
            Ok(file)
        }
        pub(crate) fn abort(self) -> Result<(), NativeArchiveError> {
            self.0.abort()
        }
        pub(crate) fn publish(
            self,
            name: &OsStr,
            admit: impl FnOnce() -> bool,
        ) -> Result<TransferFile, NativeArchiveError> {
            self.0
                .publish_admitted(name, admit, true)
                .map(|(identity, file)| TransferFile { file, identity })
        }
    }

    pub(crate) fn is_missing_relative_leaf(error: &NativeArchiveError) -> bool {
        !error.unconfirmed_name_effect
            && error.primary.nt_status
                == Some(windows::Win32::Foundation::STATUS_OBJECT_NAME_NOT_FOUND.0)
    }
}

/// Narrow handle-relative primitives for guarded PDF publication.
pub(crate) mod print_publication {
    use super::*;
    use windows::Wdk::Storage::FileSystem::{
        FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
    };
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, SYNCHRONIZE,
    };

    fn error(error: NativeArchiveError) -> io::Error {
        io::Error::new(error.kind(), error.to_string())
    }
    fn failure(error: NativeFailure) -> io::Error {
        io::Error::new(error.kind, format!("{error:?}"))
    }
    pub(crate) struct Directory(Arc<HeldDirectory>);
    pub(crate) struct ReadFile {
        file: File,
        _parent: Arc<HeldDirectory>,
    }
    impl io::Read for ReadFile {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.file.read(buffer)
        }
    }
    pub(crate) struct Leaf {
        file: File,
        identity: NativeFileIdentity,
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Revision {
        pub identity: NativeFileIdentity,
        pub size: u64,
        pub creation: i64,
        pub modified: i64,
        pub changed: i64,
        pub attributes: u32,
    }
    pub(crate) fn validate_pdf_attributes(attributes: u32) -> io::Result<()> {
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_SYSTEM,
        };
        crate::file_operations::validate_replace_basic_attributes(attributes)?;
        if attributes
            & (FILE_ATTRIBUTE_HIDDEN.0
                | FILE_ATTRIBUTE_SYSTEM.0
                | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED.0)
            != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "PDF replacement refuses special file attributes",
            ));
        }
        Ok(())
    }
    pub(crate) struct Stage(OwnedStage);
    impl Directory {
        pub(crate) fn open(path: &Path, writable: bool) -> io::Result<Self> {
            {
                let (root, parts) = split_local_absolute_path(path).map_err(error)?;
                let mut current =
                    open_volume_root_with_access(&root, writable && parts.is_empty(), false)
                        .map_err(error)?;
                for (index, part) in parts.iter().enumerate() {
                    current = current
                        .open_child_with_access(part, writable && index + 1 == parts.len(), false)
                        .map_err(error)?;
                }
                Ok(Self(current))
            }
        }
        /// Open a plain asset relative to the held root, retaining every parent
        /// handle until the caller finishes reading.
        pub(crate) fn read_relative_file(&self, path: &Path) -> io::Result<ReadFile> {
            let mut components = path.components().peekable();
            let mut parent = Arc::clone(&self.0);
            while let Some(component) = components.next() {
                let std::path::Component::Normal(name) = component else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid asset path",
                    ));
                };
                if components.peek().is_some() {
                    parent = parent.open_child(name, false).map_err(error)?;
                    continue;
                }
                validate_single_component(name, "print asset leaf").map_err(error)?;
                let (file, _) = open_relative(
                    &parent,
                    name,
                    FILE_READ_ATTRIBUTES
                        | windows::Win32::Storage::FileSystem::FILE_READ_DATA
                        | SYNCHRONIZE,
                    FILE_SHARE_READ,
                    FILE_OPEN,
                    FILE_NON_DIRECTORY_FILE
                        | FILE_OPEN_REPARSE_POINT
                        | FILE_SYNCHRONOUS_IO_NONALERT,
                    FILE_ATTRIBUTE_NORMAL,
                    "open print asset",
                    || Ok(()),
                )
                .map_err(error)?;
                let metadata = file.metadata()?;
                let attributes = metadata.file_attributes();
                if !metadata.is_file()
                    || attributes
                        & (FILE_ATTRIBUTE_REPARSE_POINT.0
                            | FILE_ATTRIBUTE_DIRECTORY.0
                            | windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_OFFLINE.0)
                        != 0
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "print asset is not an available ordinary file",
                    ));
                }
                if native_file_identity(&file)?.volume_serial_number()
                    != parent.identity.volume_serial_number()
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "print asset changed volume",
                    ));
                }
                return Ok(ReadFile {
                    file,
                    _parent: parent,
                });
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty asset path",
            ))
        }
        pub(crate) fn identity(&self) -> io::Result<NativeFileIdentity> {
            self.0.verify_identity().map_err(error)
        }
        pub(crate) fn leaf(&self, name: &OsStr, guard: bool) -> io::Result<Option<Leaf>> {
            let access = FILE_READ_ATTRIBUTES
                | windows::Win32::Storage::FileSystem::FILE_READ_DATA
                | SYNCHRONIZE
                | if guard {
                    DELETE | windows::Win32::Storage::FileSystem::READ_CONTROL
                } else {
                    windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0)
                };
            let sharing = if guard {
                FILE_SHARE_READ
            } else {
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
            };
            let file = match open_relative(
                &self.0,
                name,
                access,
                sharing,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                FILE_ATTRIBUTE_NORMAL,
                "open PDF destination",
                || Ok(()),
            ) {
                Ok((file, _)) => file,
                Err(e) if e.primary.nt_status == Some(0xc000_0034u32 as i32) => return Ok(None),
                Err(e) => return Err(error(e)),
            };
            let metadata = file.metadata()?;
            let attrs = metadata.file_attributes();
            if !metadata.is_file()
                || attrs & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DIRECTORY.0) != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "PDF destination is not an ordinary file",
                ));
            }
            let actual = crate::file_operations::final_path_for_handle(&file)?;
            let actual_leaf = actual.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "destination has no final name")
            })?;
            if !crate::batch_rename::compare_paths_windows_ordinal(
                Path::new(actual_leaf),
                Path::new(name),
            )
            .is_eq()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "destination leaf is an alternate name",
                ));
            }
            let identity = native_file_identity(&file)?;
            Ok(Some(Leaf { file, identity }))
        }
        pub(crate) fn source_leaf(&self, name: &OsStr, guard: bool) -> io::Result<Option<Leaf>> {
            let file = match open_relative(
                &self.0,
                name,
                FILE_READ_ATTRIBUTES
                    | SYNCHRONIZE
                    | if guard {
                        windows::Win32::Storage::FileSystem::FILE_READ_DATA
                    } else {
                        windows::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS(0)
                    },
                FILE_SHARE_READ,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                FILE_ATTRIBUTE_NORMAL,
                "open PDF active source",
                || Ok(()),
            ) {
                Ok((file, _)) => file,
                Err(e) if e.primary.nt_status == Some(0xc000_0034u32 as i32) => return Ok(None),
                Err(e) => return Err(error(e)),
            };
            let meta = file.metadata()?;
            if !meta.is_file()
                || meta.file_attributes()
                    & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DIRECTORY.0)
                    != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "active source is not an ordinary file",
                ));
            }
            let identity = native_file_identity(&file)?;
            Ok(Some(Leaf { file, identity }))
        }
        pub(crate) fn stage(&self, security: bool) -> io::Result<Stage> {
            use windows::Win32::Storage::FileSystem::{
                FILE_ACCESS_RIGHTS, FILE_SHARE_MODE, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
            };
            let extra = if security {
                READ_CONTROL | WRITE_DAC | WRITE_OWNER
            } else {
                FILE_ACCESS_RIGHTS(0)
            };
            self.0.verify_identity().map_err(error)?;
            OwnedStage::create_after_parent_validation_with_access(
                self.0.clone(),
                || Ok(()),
                extra,
                FILE_SHARE_MODE(0),
            )
            .map(Stage)
            .map_err(error)
        }
        pub(crate) fn rename(&self, leaf: &Leaf, name: &OsStr) -> io::Result<()> {
            self.identity()?;
            rename_open_file_relative_no_replace(&leaf.file, &self.0, name, leaf.identity)
                .map_err(failure)
        }
    }
    impl Leaf {
        pub(crate) fn revision(&self) -> io::Result<Revision> {
            if native_file_identity(&self.file)? != self.identity {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "held PDF destination identity changed",
                ));
            }
            let info = crate::file_operations::query_replace_basic_info(&self.file)?;
            let metadata = self.file.metadata()?;
            if !metadata.is_file()
                || info.FileAttributes
                    & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DIRECTORY.0)
                    != 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "PDF destination is not an ordinary file",
                ));
            }
            Ok(Revision {
                identity: self.identity,
                size: metadata.len(),
                creation: info.CreationTime,
                modified: info.LastWriteTime,
                changed: info.ChangeTime,
                attributes: info.FileAttributes,
            })
        }
        pub(crate) fn fingerprint(
            &self,
            cancel: &mut impl FnMut() -> bool,
        ) -> io::Result<(Revision, [u8; 32])> {
            use sha2::{Digest, Sha256};
            use std::io::{Read, Seek, SeekFrom};
            let before = self.revision()?;
            if before.size > crate::notes_markdown::print_layout::MAX_OUTPUT_BYTES as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "existing PDF exceeds the review size limit",
                ));
            }
            let mut input = self.file.try_clone()?;
            input.seek(SeekFrom::Start(0))?;
            let mut hash = Sha256::new();
            let mut total = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancel() {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "PDF review cancelled",
                    ));
                }
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                if total > crate::notes_markdown::print_layout::MAX_OUTPUT_BYTES as u64 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "existing PDF exceeds the review size limit",
                    ));
                }
                hash.update(&buffer[..n]);
            }
            if self.revision()? != before || total != before.size {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "existing PDF changed while reviewed",
                ));
            }
            Ok((before, hash.finalize().into()))
        }
        pub(crate) fn validate_security(&self) -> io::Result<()> {
            let attributes =
                crate::file_operations::query_replace_basic_info(&self.file)?.FileAttributes;
            validate_pdf_attributes(attributes)?;
            crate::file_operations::ensure_unnamed_stream_only(&self.file)
        }
        pub(crate) fn dispose(self) -> io::Result<()> {
            delete_open_file(&self.file, self.identity).map_err(failure)
        }
    }
    impl Stage {
        pub(crate) fn copy_security(&self, original: &Leaf) -> io::Result<()> {
            crate::file_operations::copy_replace_security(&original.file, self.0.file.as_file())
        }
        pub(crate) fn write(
            &self,
            bytes: &[u8],
            cancel: &mut impl FnMut() -> bool,
        ) -> io::Result<()> {
            let io = ArchiveIoControl::new(ScanCancellation::new());
            let mut offset = 0u64;
            for chunk in bytes.chunks(64 * 1024) {
                let mut done = 0;
                while done < chunk.len() {
                    if cancel() {
                        return Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "PDF publication cancelled",
                        ));
                    }
                    let n = self
                        .0
                        .write_chunk(&io, offset + done as u64, &chunk[done..])
                        .map_err(error)?;
                    if n == 0 || n > chunk.len() - done {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "invalid PDF stage write",
                        ));
                    }
                    done += n;
                }
                offset += chunk.len() as u64;
            }
            self.0.flush_final().map_err(error)
        }
        pub(crate) fn name(&self) -> OsString {
            self.0.leaf.clone()
        }
        pub(crate) fn identity(&self) -> NativeFileIdentity {
            self.0.identity()
        }
        pub(crate) fn publish(
            self,
            name: &OsStr,
            admit: impl FnOnce() -> bool,
        ) -> Result<NativeFileIdentity, (io::Error, Option<(OsString, NativeFileIdentity)>)>
        {
            let leaf = self.0.leaf.clone();
            let identity = self.0.identity();
            self.0.publish_admitted(name, admit, false).map(|(identity, _)| identity).map_err(|e| {
                let retained = e.cleanup_failed().then_some((leaf, identity));
                (error(e), retained)
            })
        }
        pub(crate) fn abort(self) -> io::Result<()> {
            self.0.abort().map_err(error)
        }
    }
}
