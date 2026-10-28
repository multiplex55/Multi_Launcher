//! Root-confined filesystem proofs and handle-bound file operations.
//!
//! The batch renamer uses this module to build a complete candidate snapshot.
//! Paths and metadata alone never establish identity: Windows IDs are read
//! from opened handles, and the selected root and each candidate's parent
//! chain are reopened and checked while scanning and again before a later
//! mutation boundary.

use crate::activity::{
    ActivityCenter, ActivityDiagnostic, ActivityIncarnation, ActivityOperation, ActivityOutcome,
    ActivityOwner, ActivityProgress, ActivityProgressUnit, ActivityRetirement, ActivitySlot,
    ActivityTask,
};
use crate::batch_rename::{
    self, EntryInclusion, ExplicitDirectoryRoot, RenameInputSnapshot, RenamePlan, RenamePreview,
    RenameRuleSet, RenameSnapshotEntry, SelectionRevision, SnapshotEntryKind, SnapshotEntryRef,
    SnapshotRevision,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;

pub const MAX_RENAME_SCAN_DIRECTORIES: usize = 4_096;
pub const MAX_RENAME_SCAN_DEPTH: usize = 256;
pub const MAX_RENAME_SCAN_ISSUE_SAMPLES: usize = 64;
const FILE_ATTRIBUTE_OFFLINE_BITS: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN_BITS: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS_BITS: u32 = 0x0040_0000;
pub(crate) const MAX_RENAME_HASH_BYTES_PER_FILE: u64 = 1 << 30;
pub(crate) const MAX_RENAME_HASH_BYTES_PER_BATCH: u64 = 4 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RenameScanGeneration(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenameScanScope {
    DirectChildren,
    Recursive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct NativeFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

impl NativeFileIdentity {
    pub(crate) const fn from_parts(volume_serial_number: u64, file_id: [u8; 16]) -> Self {
        Self {
            volume_serial_number,
            file_id,
        }
    }

    pub fn volume_serial_number(self) -> u64 {
        self.volume_serial_number
    }

    pub fn file_id(self) -> [u8; 16] {
        self.file_id
    }
}

/// Fresh identity evidence for one explicit watch root. The retained proof is
/// re-opened for every reported path; it does not authorize a scan or write.
#[derive(Debug, Clone)]
pub struct NativeWatchRoot {
    proof: RootProof,
}

impl NativeWatchRoot {
    pub fn selected_root(&self) -> &Path {
        &self.proof.selected_path
    }

    pub fn canonical_root(&self) -> &Path {
        &self.proof.canonical_path
    }

    pub fn identity(&self) -> NativeFileIdentity {
        self.proof.identity
    }
}

#[cfg(test)]
pub(crate) fn test_native_watch_root(
    path: &Path,
    volume_serial_number: u64,
    file_id: [u8; 16],
) -> NativeWatchRoot {
    let identity = NativeFileIdentity {
        volume_serial_number,
        file_id,
    };
    NativeWatchRoot {
        proof: RootProof {
            selected_path: path.to_path_buf(),
            canonical_path: path.to_path_buf(),
            identity,
            recursive: false,
            exclude_cloud_placeholders: false,
            generation: RenameScanGeneration(0),
            chain: vec![DirectoryProof {
                lookup_path: path.to_path_buf(),
                canonical_path: path.to_path_buf(),
                identity,
            }],
        },
    }
}

/// Read-only evidence for a single path that a watcher reported under an
/// admitted root. This is a point-in-time observation, not mutation authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWatchFileObservation {
    canonical_path: PathBuf,
    root_identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    length: u64,
    modified: SystemTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeHotFolderInput {
    pub(crate) root_identity: NativeFileIdentity,
    pub(crate) parent_identity: NativeFileIdentity,
    pub(crate) file_identity: NativeFileIdentity,
    pub(crate) length: u64,
    pub(crate) modified: SystemTime,
    pub(crate) sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeHotFolderAction {
    Copy,
    Move,
    Rename,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeHotFolderOutputKind {
    Final,
    OwnedStage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeHotFolderOutput {
    pub(crate) kind: NativeHotFolderOutputKind,
    pub(crate) path: PathBuf,
    pub(crate) identity: NativeFileIdentity,
    pub(crate) length: u64,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) sha256: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeHotFolderFailureKind {
    StaleSource,
    Inaccessible,
    DestinationUnavailable,
    DestinationCollision,
    UnsupportedFidelity,
    Cancelled,
    Io,
    PartialMove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeHotFolderPhase {
    Admission,
    Staging,
    Published,
    SourceRemoval,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeHotFolderError {
    pub(crate) kind: NativeHotFolderFailureKind,
    pub(crate) phase: NativeHotFolderPhase,
    /// A short, path-free reason suitable for a bounded execution receipt.
    pub(crate) reason: &'static str,
    /// Any stage or published payload that could not safely be removed.
    pub(crate) retained_outputs: Vec<NativeHotFolderOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeHotFolderResult {
    pub(crate) source_identity: NativeFileIdentity,
    pub(crate) outputs: Vec<NativeHotFolderOutput>,
    pub(crate) phase: NativeHotFolderPhase,
    pub(crate) partial: bool,
    pub(crate) reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NativeHotFolderProgress {
    pub(crate) phase: NativeHotFolderPhase,
    pub(crate) outputs: Vec<NativeHotFolderOutput>,
}

impl NativeWatchFileObservation {
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn root_identity(&self) -> NativeFileIdentity {
        self.root_identity
    }

    pub fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity
    }

    pub fn file_identity(&self) -> NativeFileIdentity {
        self.file_identity
    }

    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn modified(&self) -> SystemTime {
        self.modified
    }
}

#[cfg(test)]
pub(crate) fn test_native_watch_file_observation(
    canonical_path: &Path,
    root_identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    file_identity: NativeFileIdentity,
    length: u64,
    modified: SystemTime,
) -> NativeWatchFileObservation {
    NativeWatchFileObservation {
        canonical_path: canonical_path.to_path_buf(),
        root_identity,
        parent_identity,
        file_identity,
        length,
        modified,
    }
}

#[cfg(test)]
pub(crate) fn test_native_file_identity(
    volume_serial_number: u64,
    file_id: [u8; 16],
) -> NativeFileIdentity {
    NativeFileIdentity {
        volume_serial_number,
        file_id,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeWatchErrorKind {
    Unsupported,
    InvalidRoot,
    RootChanged,
    ParentChanged,
    OutsideRoot,
    InvalidPath,
    Missing,
    NotRegularFile,
    ReparsePoint,
    Busy,
    Inaccessible,
    UnsupportedIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWatchError {
    kind: NativeWatchErrorKind,
}

impl NativeWatchError {
    pub fn kind(self) -> NativeWatchErrorKind {
        self.kind
    }
}

impl std::fmt::Display for NativeWatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self.kind {
            NativeWatchErrorKind::Unsupported => {
                "native watch admission is unavailable on this platform"
            }
            NativeWatchErrorKind::InvalidRoot => "the explicit watch root is invalid",
            NativeWatchErrorKind::RootChanged => "the watched root identity changed",
            NativeWatchErrorKind::ParentChanged => "the reported file parent identity changed",
            NativeWatchErrorKind::OutsideRoot => "the reported path is outside the watched root",
            NativeWatchErrorKind::InvalidPath => "the reported path is not a supported local path",
            NativeWatchErrorKind::Missing => "the reported file no longer exists",
            NativeWatchErrorKind::NotRegularFile => "the reported path is not an ordinary file",
            NativeWatchErrorKind::ReparsePoint => "the reported path is a reparse point",
            NativeWatchErrorKind::Busy => "the reported file is still busy",
            NativeWatchErrorKind::Inaccessible => "the reported file could not be inspected",
            NativeWatchErrorKind::UnsupportedIdentity => {
                "the filesystem did not provide stable file identity metadata"
            }
        })
    }
}

impl std::error::Error for NativeWatchError {}

/// Open and prove one explicit local root using metadata-only directory
/// handles. This does not enumerate the directory.
pub fn admit_watch_root(path: &Path) -> Result<NativeWatchRoot, NativeWatchError> {
    #[cfg(windows)]
    {
        let root = ExplicitDirectoryRoot::new(path.to_path_buf())
            .map_err(|_| native_watch_error(NativeWatchErrorKind::InvalidRoot))?;
        let mut request = RenameScanRequest::new(root, false, RenameScanGeneration(0));
        request.metadata_only_root = true;
        let admission = NativeRenameScanBackend
            .admit_root(&request)
            .map_err(map_watch_backend_failure)?;
        Ok(NativeWatchRoot {
            proof: admission.proof,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(native_watch_error(NativeWatchErrorKind::Unsupported))
    }
}

/// Re-admit the selected root and require the original volume/file identity.
pub fn refresh_watch_root(root: &NativeWatchRoot) -> Result<NativeWatchRoot, NativeWatchError> {
    let fresh = admit_watch_root(&root.proof.selected_path)?;
    #[cfg(windows)]
    if fresh.proof.identity != root.proof.identity
        || batch_rename::compare_paths_windows_ordinal(
            &fresh.proof.canonical_path,
            &root.proof.canonical_path,
        ) != std::cmp::Ordering::Equal
    {
        return Err(native_watch_error(NativeWatchErrorKind::RootChanged));
    }
    Ok(fresh)
}

/// Admit one exact watcher-reported path beneath its already selected root.
/// `require_read_lock` requests a short-lived read handle that denies writers
/// and deleters while final settle evidence is captured.
pub fn observe_watch_file(
    root: &NativeWatchRoot,
    reported_path: &Path,
    require_read_lock: bool,
) -> Result<NativeWatchFileObservation, NativeWatchError> {
    #[cfg(windows)]
    {
        observe_watch_file_native(root, reported_path, require_read_lock)
    }
    #[cfg(not(windows))]
    {
        let _ = (root, reported_path, require_read_lock);
        Err(native_watch_error(NativeWatchErrorKind::Unsupported))
    }
}

/// Reopen a settled watcher candidate with the same deny-write/delete sharing
/// used by M9's guarded mutation path, hash it in bounded chunks, and require
/// the exact watcher-observed root, parent and file version.
pub(crate) fn fingerprint_hotfolder_file(
    root: &NativeWatchRoot,
    path: &Path,
    expected_root_identity: NativeFileIdentity,
    expected_parent_identity: NativeFileIdentity,
    expected_file_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    cancellation: &ScanCancellation,
) -> Result<NativeHotFolderInput, NativeHotFolderError> {
    #[cfg(windows)]
    {
        let source = prepare_hotfolder_source(
            root,
            path,
            expected_root_identity,
            expected_parent_identity,
            expected_file_identity,
            expected_length,
            expected_modified,
            cancellation,
        )
        .map_err(|error| error.with_phase(NativeHotFolderPhase::Admission))?;
        Ok(NativeHotFolderInput {
            root_identity: source.native.root.identity,
            parent_identity: source.native.parent_identity,
            file_identity: source.identity(),
            length: source.original_state.length,
            modified: source.original_state.modified,
            sha256: source.original_state.content_sha256,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (
            root,
            path,
            expected_root_identity,
            expected_parent_identity,
            expected_file_identity,
            expected_length,
            expected_modified,
            cancellation,
        );
        Err(native_hotfolder_error(
            NativeHotFolderFailureKind::Io,
            "native Hot Folder file actions are available only on Windows",
            Vec::new(),
        ))
    }
}

/// Perform one built-in Hot Folder action through the guarded M9 native file
/// owner. Destination directories arrive as independently admitted roots;
/// the target leaf is always validated before any mutation.
pub(crate) fn execute_hotfolder_native_action(
    root: &NativeWatchRoot,
    source_path: &Path,
    expected: NativeHotFolderInput,
    action: NativeHotFolderAction,
    destination_root: Option<&NativeWatchRoot>,
    target_leaf: &OsStr,
    operation_id: u128,
    progress: &mut dyn FnMut(NativeHotFolderProgress) -> std::io::Result<()>,
    cancellation: &ScanCancellation,
) -> Result<NativeHotFolderResult, NativeHotFolderError> {
    #[cfg(windows)]
    {
        execute_hotfolder_native_action_windows(
            root,
            source_path,
            expected,
            action,
            destination_root,
            target_leaf,
            operation_id,
            progress,
            cancellation,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (
            root,
            source_path,
            expected,
            action,
            destination_root,
            target_leaf,
            operation_id,
            progress,
            cancellation,
        );
        Err(native_hotfolder_error(
            NativeHotFolderFailureKind::Io,
            "native Hot Folder file actions are available only on Windows",
            Vec::new(),
        ))
    }
}

#[cfg(any(windows, test))]
enum CrossVolumeMoveOutcome<Output> {
    Complete(Output),
    Partial {
        output: Output,
        reason: &'static str,
    },
}

#[cfg(any(windows, test))]
fn execute_cross_volume_move_protocol<Stage, Output, Error>(
    create_verified_stage: impl FnOnce() -> Result<Stage, Error>,
    publish: impl FnOnce(&mut Stage) -> Result<Output, Error>,
    fresh_recheck: impl FnOnce(&Stage, &Output) -> bool,
    cancellation_requested: impl Fn() -> bool,
    record_published: impl FnOnce(&Stage, &Output) -> bool,
    record_source_removal: impl FnOnce(&Stage, &Output) -> bool,
    remove_source: impl FnOnce() -> bool,
) -> Result<CrossVolumeMoveOutcome<Output>, Error> {
    let mut stage = create_verified_stage()?;
    let output = publish(&mut stage)?;
    if !record_published(&stage, &output) {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "published destination could not be durably recorded; source remains",
        });
    }
    if cancellation_requested() {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "move cancelled before source removal; both verified files remain",
        });
    }
    if !fresh_recheck(&stage, &output) {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "source or published destination changed before source removal; both remain",
        });
    }
    if cancellation_requested() {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "move cancelled before source removal; both verified files remain",
        });
    }
    if !record_source_removal(&stage, &output) {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "source removal intent could not be durably recorded; both files remain",
        });
    }
    if cancellation_requested() {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "move cancelled before source removal; both verified files remain",
        });
    }
    if !remove_source() {
        return Ok(CrossVolumeMoveOutcome::Partial {
            output,
            reason: "source removal failed; source and verified destination remain",
        });
    }
    Ok(CrossVolumeMoveOutcome::Complete(output))
}

fn native_hotfolder_error(
    kind: NativeHotFolderFailureKind,
    reason: &'static str,
    retained_outputs: Vec<NativeHotFolderOutput>,
) -> NativeHotFolderError {
    NativeHotFolderError {
        kind,
        phase: match kind {
            NativeHotFolderFailureKind::StaleSource
            | NativeHotFolderFailureKind::Inaccessible
            | NativeHotFolderFailureKind::DestinationUnavailable => NativeHotFolderPhase::Admission,
            NativeHotFolderFailureKind::DestinationCollision
            | NativeHotFolderFailureKind::UnsupportedFidelity
            | NativeHotFolderFailureKind::Cancelled
            | NativeHotFolderFailureKind::Io => NativeHotFolderPhase::Staging,
            NativeHotFolderFailureKind::PartialMove => NativeHotFolderPhase::SourceRemoval,
        },
        reason,
        retained_outputs,
    }
}

fn native_watch_error(kind: NativeWatchErrorKind) -> NativeWatchError {
    NativeWatchError { kind }
}

#[cfg(windows)]
fn map_watch_backend_failure(error: BackendFailure) -> NativeWatchError {
    let kind = match error.kind {
        BackendFailureKind::InvalidRoot => NativeWatchErrorKind::InvalidRoot,
        BackendFailureKind::RootChanged => NativeWatchErrorKind::RootChanged,
        BackendFailureKind::ParentChanged => NativeWatchErrorKind::ParentChanged,
        BackendFailureKind::PathEscapedRoot => NativeWatchErrorKind::OutsideRoot,
        BackendFailureKind::UnsupportedIdentity => NativeWatchErrorKind::UnsupportedIdentity,
        BackendFailureKind::Inaccessible | BackendFailureKind::ObjectChanged => {
            NativeWatchErrorKind::Inaccessible
        }
        BackendFailureKind::InvalidEntry => NativeWatchErrorKind::InvalidPath,
        BackendFailureKind::ResourceLimit | BackendFailureKind::Cancelled => {
            NativeWatchErrorKind::Inaccessible
        }
    };
    native_watch_error(kind)
}

#[derive(Debug, Clone)]
pub struct RenameScanRequest {
    root: ExplicitDirectoryRoot,
    scope: RenameScanScope,
    generation: RenameScanGeneration,
    metadata_only_root: bool,
    exclude_cloud_placeholders: bool,
    expected_duplicate_root: Option<DuplicateDiscoveryRootIdentity>,
}

impl RenameScanRequest {
    pub fn new(
        root: ExplicitDirectoryRoot,
        recursive: bool,
        generation: RenameScanGeneration,
    ) -> Self {
        Self {
            root,
            scope: if recursive {
                RenameScanScope::Recursive
            } else {
                RenameScanScope::DirectChildren
            },
            generation,
            metadata_only_root: false,
            exclude_cloud_placeholders: false,
            expected_duplicate_root: None,
        }
    }

    pub fn root(&self) -> &ExplicitDirectoryRoot {
        &self.root
    }

    pub fn scope(&self) -> RenameScanScope {
        self.scope
    }

    pub fn generation(&self) -> RenameScanGeneration {
        self.generation
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanCancellation(Arc<AtomicBool>);

impl ScanCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ContentVerificationMode<'a> {
    /// Stop at the next content chunk when the user or application requests
    /// cancellation, before beginning another atomic namespace operation.
    Cooperative(&'a ScanCancellation),
    /// Finish verifying a payload that the transaction may own at a recovery
    /// path before compensating or recording its terminal state.
    CompleteOwnedPayload,
}

impl ContentVerificationMode<'_> {
    pub(crate) fn is_cancelled(self) -> bool {
        matches!(self, Self::Cooperative(cancellation) if cancellation.is_cancelled())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameScanStatus {
    Complete,
    Partial,
    InvalidRoot,
    Inaccessible,
    Unsupported,
    Cancelled,
    ResourceLimit,
    ChangedDuringScan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameScanIssueKind {
    Inaccessible,
    UnsupportedIdentity,
    ReparsePointExcluded,
    RootChanged,
    ParentChanged,
    PathEscapedRoot,
    InvalidEntry,
    ResourceLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameScanIssue {
    path: Option<PathBuf>,
    kind: RenameScanIssueKind,
    message: String,
}

impl RenameScanIssue {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn kind(&self) -> RenameScanIssueKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameScanSummary {
    directories_scanned: usize,
    directories_admitted: usize,
    entries_seen: usize,
    files_seen: usize,
    directories_seen: usize,
    other_entries_seen: usize,
    reparse_entries_excluded: usize,
    cloud_placeholders_excluded: usize,
    inaccessible_entries: usize,
    issue_count: usize,
    issues_truncated: bool,
    issues: Vec<RenameScanIssue>,
}

impl RenameScanSummary {
    pub fn directories_scanned(&self) -> usize {
        self.directories_scanned
    }

    pub(crate) fn directories_admitted(&self) -> usize {
        self.directories_admitted
    }

    pub fn entries_seen(&self) -> usize {
        self.entries_seen
    }

    pub fn files_seen(&self) -> usize {
        self.files_seen
    }

    pub fn directories_seen(&self) -> usize {
        self.directories_seen
    }

    pub fn other_entries_seen(&self) -> usize {
        self.other_entries_seen
    }

    pub fn reparse_entries_excluded(&self) -> usize {
        self.reparse_entries_excluded
    }

    pub(crate) fn cloud_placeholders_excluded(&self) -> usize {
        self.cloud_placeholders_excluded
    }

    pub fn inaccessible_entries(&self) -> usize {
        self.inaccessible_entries
    }

    pub fn issue_count(&self) -> usize {
        self.issue_count
    }

    pub fn issues_truncated(&self) -> bool {
        self.issues_truncated
    }

    pub fn issues(&self) -> &[RenameScanIssue] {
        &self.issues
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameScanRow {
    entry_ref: SnapshotEntryRef,
    relative_path: PathBuf,
    path: PathBuf,
    kind: SnapshotEntryKind,
    identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    length: Option<u64>,
    modified: Option<SystemTime>,
    reparse_point: bool,
    cloud_placeholder: bool,
}

impl RenameScanRow {
    pub fn entry_ref(&self) -> SnapshotEntryRef {
        self.entry_ref
    }

    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> SnapshotEntryKind {
        self.kind
    }

    pub fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity
    }

    pub fn length(&self) -> Option<u64> {
        self.length
    }

    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }

    pub fn is_reparse_point(&self) -> bool {
        self.reparse_point
    }

    pub(crate) fn is_cloud_placeholder(&self) -> bool {
        self.cloud_placeholder
    }
}

#[derive(Debug, Clone)]
pub struct RenameScanResult {
    generation: RenameScanGeneration,
    scope: RenameScanScope,
    status: RenameScanStatus,
    canonical_root: Option<PathBuf>,
    root_identity: Option<NativeFileIdentity>,
    rows: Vec<RenameScanRow>,
    snapshot: Option<RenameInputSnapshot>,
    proof: Option<Arc<ScanProof>>,
    summary: RenameScanSummary,
}

pub(crate) struct DuplicateDiscoveryRow {
    pub(crate) path: PathBuf,
    pub(crate) kind: SnapshotEntryKind,
    pub(crate) identity: NativeFileIdentity,
    pub(crate) length: Option<u64>,
    pub(crate) modified: Option<SystemTime>,
    pub(crate) cloud_placeholder: bool,
}

pub(crate) struct DuplicateDiscoveryScanResult {
    pub(crate) status: RenameScanStatus,
    pub(crate) canonical_root: Option<PathBuf>,
    pub(crate) root_identity: Option<NativeFileIdentity>,
    pub(crate) rows: Vec<DuplicateDiscoveryRow>,
    pub(crate) summary: RenameScanSummary,
}

#[derive(Debug, Clone)]
pub(crate) struct DuplicateDiscoveryRootIdentity {
    pub(crate) canonical_root: PathBuf,
    pub(crate) identity: NativeFileIdentity,
}

/// Read-only bytes captured from a file row in a complete native scan. The
/// evidence is for a later fresh recheck only; it does not authorize a write.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedReadSnapshot {
    canonical_root: PathBuf,
    root_identity: NativeFileIdentity,
    path: PathBuf,
    identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    length: u64,
    modified: SystemTime,
    bytes: Arc<[u8]>,
    content_sha256: [u8; 32],
    evidence: Arc<VerifiedReadEvidence>,
}

impl VerifiedReadSnapshot {
    pub(crate) fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }

    pub(crate) fn root_identity(&self) -> NativeFileIdentity {
        self.root_identity
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub(crate) fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity
    }

    pub(crate) fn length(&self) -> u64 {
        self.length
    }

    pub(crate) fn modified(&self) -> SystemTime {
        self.modified
    }

    pub(crate) fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }

    pub(crate) fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }

    pub(crate) fn evidence(&self) -> &Arc<VerifiedReadEvidence> {
        &self.evidence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifiedReadFailureKind {
    IncompleteScan,
    InvalidEvidence,
    NotRegularFile,
    Inaccessible,
    Changed,
    TooLarge,
    Cancelled,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedReadFailure {
    kind: VerifiedReadFailureKind,
    message: String,
}

impl VerifiedReadFailure {
    pub(crate) fn kind(&self) -> VerifiedReadFailureKind {
        self.kind
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl RenameScanResult {
    pub fn generation(&self) -> RenameScanGeneration {
        self.generation
    }

    pub fn scope(&self) -> RenameScanScope {
        self.scope
    }

    pub fn status(&self) -> RenameScanStatus {
        self.status
    }

    pub fn canonical_root(&self) -> Option<&Path> {
        self.canonical_root.as_deref()
    }

    pub fn root_identity(&self) -> Option<NativeFileIdentity> {
        self.root_identity
    }

    /// Rows include every discovered sibling, including directories,
    /// excluded reparse points, unselected files and other ineligible names.
    pub fn rows(&self) -> &[RenameScanRow] {
        &self.rows
    }

    pub fn summary(&self) -> &RenameScanSummary {
        &self.summary
    }

    pub(crate) fn into_duplicate_discovery(self) -> DuplicateDiscoveryScanResult {
        DuplicateDiscoveryScanResult {
            status: self.status,
            canonical_root: self.canonical_root,
            root_identity: self.root_identity,
            rows: self
                .rows
                .into_iter()
                .map(|row| DuplicateDiscoveryRow {
                    path: row.path,
                    kind: row.kind,
                    identity: row.identity,
                    length: row.length,
                    modified: row.modified,
                    cloud_placeholder: row.cloud_placeholder,
                })
                .collect(),
            summary: self.summary,
        }
    }

    /// Convert this exact, complete result into the M8-A snapshot only when
    /// it is still the latest scan generation accepted by the owner.
    pub fn accept(
        self,
        current_generation: RenameScanGeneration,
    ) -> Result<VerifiedRenameSnapshot, ScanAcceptanceError> {
        if self.generation != current_generation {
            return Err(ScanAcceptanceError::StaleGeneration {
                scanned: self.generation,
                current: current_generation,
            });
        }
        if self.status != RenameScanStatus::Complete {
            return Err(ScanAcceptanceError::Incomplete(self.status));
        }
        let snapshot = self
            .snapshot
            .ok_or(ScanAcceptanceError::Incomplete(self.status))?;
        let proof = self
            .proof
            .ok_or(ScanAcceptanceError::Incomplete(self.status))?;
        Ok(VerifiedRenameSnapshot {
            snapshot,
            proof,
            generation: self.generation,
            summary: self.summary,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanAcceptanceError {
    StaleGeneration {
        scanned: RenameScanGeneration,
        current: RenameScanGeneration,
    },
    Incomplete(RenameScanStatus),
}

#[derive(Debug, Clone)]
pub struct VerifiedRenameSnapshot {
    snapshot: RenameInputSnapshot,
    proof: Arc<ScanProof>,
    generation: RenameScanGeneration,
    summary: RenameScanSummary,
}

impl VerifiedRenameSnapshot {
    pub fn generation(&self) -> RenameScanGeneration {
        self.generation
    }

    pub fn summary(&self) -> &RenameScanSummary {
        &self.summary
    }

    pub fn selection_revision(&self) -> batch_rename::SelectionRevision {
        self.snapshot.selection_revision()
    }

    pub fn canonical_root(&self) -> &Path {
        &self.proof.root.canonical_path
    }

    pub fn root_identity(&self) -> NativeFileIdentity {
        self.proof.root.identity
    }

    pub fn with_selection(
        &self,
        entry_ref: SnapshotEntryRef,
        selected: bool,
    ) -> Result<Self, batch_rename::SnapshotError> {
        Ok(Self {
            snapshot: self.snapshot.with_selection(entry_ref, selected)?,
            proof: Arc::clone(&self.proof),
            generation: self.generation,
            summary: self.summary.clone(),
        })
    }

    pub fn with_inclusion(
        &self,
        entry_ref: SnapshotEntryRef,
        inclusion: EntryInclusion,
    ) -> Result<Self, batch_rename::SnapshotError> {
        self.with_entry_updates(&[(entry_ref, None, Some(inclusion))])
    }

    /// Apply one complete file-selection/filter interaction while retaining
    /// the native proof that belongs to this accepted scan.
    pub fn with_entry_updates(
        &self,
        updates: &[(SnapshotEntryRef, Option<bool>, Option<EntryInclusion>)],
    ) -> Result<Self, batch_rename::SnapshotError> {
        Ok(Self {
            snapshot: self.snapshot.with_entry_updates(updates)?,
            proof: Arc::clone(&self.proof),
            generation: self.generation,
            summary: self.summary.clone(),
        })
    }

    pub fn preview(&self, rules: &RenameRuleSet) -> VerifiedRenamePreview {
        let request = batch_rename::RenameRequest::new(self.snapshot.clone(), rules.clone());
        VerifiedRenamePreview {
            preview: batch_rename::preview_renames(&request),
            proof: Arc::clone(&self.proof),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedRenamePreview {
    preview: RenamePreview,
    proof: Arc<ScanProof>,
}

impl VerifiedRenamePreview {
    pub fn preview(&self) -> &RenamePreview {
        &self.preview
    }

    pub fn verified_plan(&self) -> Option<VerifiedRenamePlan> {
        self.preview.plan().cloned().map(|plan| VerifiedRenamePlan {
            plan,
            proof: Arc::clone(&self.proof),
        })
    }
}

/// An M8-A plan paired with the private root, parent and file identity proof
/// that produced it. M8-D should require this wrapper at its apply boundary.
#[derive(Debug, Clone)]
pub struct VerifiedRenamePlan {
    plan: RenamePlan,
    proof: Arc<ScanProof>,
}

/// A native file handle that is authorized by a verified M8 scan and remains
/// bound to the source identity and its guarded parent chain for one rename
/// transaction. The handle has DELETE and read rights, and denies other
/// writers and deleters while it is retained.
pub(crate) struct GuardedRenameFile {
    source_ref: SnapshotEntryRef,
    original_path: PathBuf,
    target_path: PathBuf,
    current_path: PathBuf,
    parent_path: PathBuf,
    parent_identity: NativeFileIdentity,
    identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    content_sha256: [u8; 32],
    #[cfg(windows)]
    source: std::fs::File,
    #[cfg(windows)]
    root: RootProof,
    #[cfg(windows)]
    parent_chain: Vec<DirectoryProof>,
    #[cfg(windows)]
    _root_guards: OpenDirectoryChain<std::fs::File>,
    #[cfg(windows)]
    _parent_guards: OpenDirectoryChain<std::fs::File>,
}

impl GuardedRenameFile {
    pub(crate) fn source_ref(&self) -> SnapshotEntryRef {
        self.source_ref
    }

    pub(crate) fn original_path(&self) -> &Path {
        &self.original_path
    }

    pub(crate) fn target_path(&self) -> &Path {
        &self.target_path
    }

    pub(crate) fn current_path(&self) -> &Path {
        &self.current_path
    }

    pub(crate) fn parent_path(&self) -> &Path {
        &self.parent_path
    }

    pub(crate) fn parent_identity(&self) -> NativeFileIdentity {
        self.parent_identity
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.identity
    }

    pub(crate) fn expected_length(&self) -> u64 {
        self.expected_length
    }

    pub(crate) fn expected_modified(&self) -> SystemTime {
        self.expected_modified
    }

    pub(crate) fn content_sha256(&self) -> [u8; 32] {
        self.content_sha256
    }

    /// Atomically rename this exact opened file to a leaf in its verified
    /// parent directory, refusing an existing destination.
    pub(crate) fn rename_to_leaf(
        &mut self,
        leaf: &OsStr,
        verification_mode: ContentVerificationMode<'_>,
    ) -> std::io::Result<PathBuf> {
        #[cfg(windows)]
        {
            let expected_path = self.current_path.clone();
            self.verify_at(&expected_path, verification_mode)?;
            let parent = self
                ._parent_guards
                .last()
                .ok_or_else(|| std::io::Error::other("verified rename parent is missing"))?;
            let backend = NativeRenameScanBackend;
            backend
                .reopen_chain(
                    &self.parent_chain,
                    &self.root,
                    DirectoryOpenAccess::Enumerate,
                )
                .map_err(|failure| std::io::Error::other(failure.message))?;
            rename_open_file_no_replace(&self.source, parent, leaf)?;
            let next_path = self.parent_path.join(leaf);
            self.current_path = next_path.clone();
            self.verify_at(&next_path, verification_mode)?;
            Ok(next_path)
        }
        #[cfg(not(windows))]
        {
            let _ = (leaf, verification_mode);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe rename-by-handle is available only on Windows",
            ))
        }
    }

    pub(crate) fn verify_at(
        &self,
        expected_path: &Path,
        verification_mode: ContentVerificationMode<'_>,
    ) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            let identity = native_file_identity(&self.source)?;
            if identity != self.identity {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "opened rename file identity changed",
                ));
            }
            let metadata = self.source.metadata()?;
            if metadata.len() != self.expected_length
                || metadata.modified().ok() != Some(self.expected_modified)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "opened rename file size or last-modified time changed",
                ));
            }
            let actual_path = final_path_for_handle(&self.source)?;
            if batch_rename::compare_paths_windows_ordinal(&actual_path, expected_path)
                != std::cmp::Ordering::Equal
                || actual_path.file_name() != expected_path.file_name()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "opened rename file moved unexpectedly: expected {}, found {}",
                        expected_path.display(),
                        actual_path.display()
                    ),
                ));
            }
            if hash_file_contents(&self.source, verification_mode)? != self.content_sha256 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "opened rename file content changed",
                ));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (expected_path, verification_mode);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe rename-by-handle is available only on Windows",
            ))
        }
    }
}

pub(crate) struct GuardedRenameBatch {
    pub(crate) files: Vec<GuardedRenameFile>,
}

#[derive(Debug, Clone)]
pub(crate) struct UndoGuardSpec {
    pub(crate) source_ref: SnapshotEntryRef,
    pub(crate) original_path: PathBuf,
    pub(crate) current_path: PathBuf,
    pub(crate) parent_path: PathBuf,
    pub(crate) parent_identity: NativeFileIdentity,
    pub(crate) identity: NativeFileIdentity,
    pub(crate) expected_length: u64,
    pub(crate) expected_modified: SystemTime,
    pub(crate) content_sha256: [u8; 32],
}

impl VerifiedRenamePlan {
    pub fn plan(&self) -> &RenamePlan {
        &self.plan
    }

    pub fn generation(&self) -> RenameScanGeneration {
        self.proof.root.generation
    }

    pub fn root_identity(&self) -> NativeFileIdentity {
        self.proof.root.identity
    }

    pub fn canonical_root(&self) -> &Path {
        &self.proof.root.canonical_path
    }

    pub fn recursive_scope(&self) -> bool {
        self.proof.root.recursive
    }

    pub fn fresh_recheck_input(&self) -> FreshRecheckInput {
        FreshRecheckInput::from_verified_plan(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshCheckStatus {
    Current,
    RootChanged,
    ParentChanged,
    SourceChanged,
    Inaccessible,
    Unsupported,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinationOccupancy {
    Vacant,
    Occupied(Vec<NativeFileIdentity>),
    Unknown(FreshCheckStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshRecheckEntryResult {
    entry_ref: SnapshotEntryRef,
    source_status: FreshCheckStatus,
    observed_source_identity: Option<NativeFileIdentity>,
    observed_source_length: Option<u64>,
    observed_source_modified: Option<SystemTime>,
    destination: DestinationOccupancy,
}

impl FreshRecheckEntryResult {
    pub fn entry_ref(&self) -> SnapshotEntryRef {
        self.entry_ref
    }

    pub fn source_status(&self) -> FreshCheckStatus {
        self.source_status
    }

    pub fn observed_source_identity(&self) -> Option<NativeFileIdentity> {
        self.observed_source_identity
    }

    pub fn observed_source_length(&self) -> Option<u64> {
        self.observed_source_length
    }

    pub fn observed_source_modified(&self) -> Option<SystemTime> {
        self.observed_source_modified
    }

    pub fn destination(&self) -> &DestinationOccupancy {
        &self.destination
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshRecheckResult {
    generation: RenameScanGeneration,
    root_status: FreshCheckStatus,
    entries: Vec<FreshRecheckEntryResult>,
}

impl FreshRecheckResult {
    pub fn generation(&self) -> RenameScanGeneration {
        self.generation
    }

    pub fn root_status(&self) -> FreshCheckStatus {
        self.root_status
    }

    pub fn entries(&self) -> &[FreshRecheckEntryResult] {
        &self.entries
    }
}

#[derive(Debug, Clone)]
pub struct FreshRecheckInput {
    root: RootProof,
    entries: Vec<FreshRecheckCandidate>,
}

impl FreshRecheckInput {
    fn from_verified_plan(plan: &VerifiedRenamePlan) -> Self {
        let entries = plan
            .plan
            .actions()
            .iter()
            .map(|action| {
                // A verified plan can only be constructed from this scan's
                // snapshot, so every action must have a matching native proof.
                let proof = plan
                    .proof
                    .entries
                    .get(&action.source_ref())
                    .expect("verified rename action has a native scan proof");
                FreshRecheckCandidate {
                    entry_ref: action.source_ref(),
                    source_path: action.source_path().to_path_buf(),
                    target_path: action.target_path().to_path_buf(),
                    expected_source_identity: proof.identity,
                    expected_source_length: proof.length,
                    expected_source_modified: proof.modified,
                    path_matches_proof: batch_rename::compare_paths_windows_ordinal(
                        action.source_path(),
                        &proof.path,
                    ) == std::cmp::Ordering::Equal,
                    parent_chain: proof.parent_chain.clone(),
                }
            })
            .collect();
        Self {
            root: plan.proof.root.clone(),
            entries,
        }
    }

    pub fn generation(&self) -> RenameScanGeneration {
        self.root.generation
    }

    pub fn root_identity(&self) -> NativeFileIdentity {
        self.root.identity
    }

    pub fn entries(&self) -> impl Iterator<Item = FreshRecheckInputEntry<'_>> {
        self.entries.iter().map(|entry| FreshRecheckInputEntry {
            entry_ref: entry.entry_ref,
            source_path: &entry.source_path,
            target_path: &entry.target_path,
            expected_source_identity: entry.expected_source_identity,
            expected_source_length: entry.expected_source_length,
            expected_source_modified: entry.expected_source_modified,
            expected_parent_identity: entry
                .parent_chain
                .last()
                .map(|parent| parent.identity)
                .unwrap_or(self.root.identity),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FreshRecheckInputEntry<'a> {
    pub entry_ref: SnapshotEntryRef,
    pub source_path: &'a Path,
    pub target_path: &'a Path,
    pub expected_source_identity: NativeFileIdentity,
    pub expected_source_length: Option<u64>,
    pub expected_source_modified: Option<SystemTime>,
    pub expected_parent_identity: NativeFileIdentity,
}

#[derive(Debug, Clone)]
struct FreshRecheckCandidate {
    entry_ref: SnapshotEntryRef,
    source_path: PathBuf,
    target_path: PathBuf,
    expected_source_identity: NativeFileIdentity,
    expected_source_length: Option<u64>,
    expected_source_modified: Option<SystemTime>,
    path_matches_proof: bool,
    parent_chain: Vec<DirectoryProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectoryProof {
    lookup_path: PathBuf,
    canonical_path: PathBuf,
    identity: NativeFileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RootProof {
    selected_path: PathBuf,
    canonical_path: PathBuf,
    identity: NativeFileIdentity,
    recursive: bool,
    exclude_cloud_placeholders: bool,
    generation: RenameScanGeneration,
    chain: Vec<DirectoryProof>,
}

#[derive(Debug, Clone)]
struct EntryProof {
    path: PathBuf,
    identity: NativeFileIdentity,
    length: Option<u64>,
    modified: Option<SystemTime>,
    parent_chain: Vec<DirectoryProof>,
}

#[derive(Debug, Clone)]
struct ScanProof {
    root: RootProof,
    entries: HashMap<SnapshotEntryRef, EntryProof>,
}

/// Opaque proof retained from the scan that produced a verified content read.
/// It carries the exact root and full parent chain needed by a later mutation
/// boundary without granting authority to mutate by itself.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedReadEvidence {
    proof: Arc<ScanProof>,
    entry_ref: SnapshotEntryRef,
}

struct OpenDirectory<H> {
    lookup_path: PathBuf,
    canonical_path: PathBuf,
    identity: NativeFileIdentity,
    handle: H,
}

type OpenDirectoryChain<H> = Vec<Arc<OpenDirectory<H>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirectoryOpenAccess {
    Metadata,
    Enumerate,
}

struct OpenedObject<H> {
    lookup_path: PathBuf,
    canonical_path: PathBuf,
    identity: NativeFileIdentity,
    kind: SnapshotEntryKind,
    reparse_point: bool,
    length: Option<u64>,
    modified: Option<SystemTime>,
    cloud_placeholder: bool,
    handle: H,
}

impl<H> OpenedObject<H> {
    fn into_directory(self) -> OpenDirectory<H> {
        OpenDirectory {
            lookup_path: self.lookup_path,
            canonical_path: self.canonical_path,
            identity: self.identity,
            handle: self.handle,
        }
    }
}

#[derive(Debug, Clone)]
struct EnumeratedName {
    name: OsString,
    is_directory: bool,
    is_reparse_point: bool,
    cloud_placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendFailureKind {
    InvalidRoot,
    Inaccessible,
    UnsupportedIdentity,
    RootChanged,
    ParentChanged,
    ObjectChanged,
    PathEscapedRoot,
    InvalidEntry,
    ResourceLimit,
    Cancelled,
}

#[derive(Debug, Clone)]
struct BackendFailure {
    kind: BackendFailureKind,
    path: Option<PathBuf>,
    message: String,
}

impl std::fmt::Display for BackendFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl BackendFailure {
    fn new(kind: BackendFailureKind, path: Option<&Path>, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.map(Path::to_path_buf),
            message: message.into(),
        }
    }
}

struct RootAdmission<H> {
    proof: RootProof,
    directories: OpenDirectoryChain<H>,
}

trait RenameScanBackend {
    type Handle;

    fn admit_root(
        &self,
        request: &RenameScanRequest,
    ) -> Result<RootAdmission<Self::Handle>, BackendFailure>;

    fn enumerate_directory(
        &self,
        directory: &OpenDirectory<Self::Handle>,
        remaining_entries: usize,
        remaining_path_bytes: usize,
        cancellation: &ScanCancellation,
    ) -> Result<Vec<EnumeratedName>, BackendFailure>;

    fn open_child(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        parent_chain: &[DirectoryProof],
        root: &RootProof,
        name: &OsStr,
        expected: Option<&EnumeratedName>,
    ) -> Result<OpenedObject<Self::Handle>, BackendFailure>;

    fn acquire_child_directory_for_enumeration(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        parent_chain: &[DirectoryProof],
        root: &RootProof,
        child: &OpenedObject<Self::Handle>,
    ) -> Result<OpenDirectory<Self::Handle>, BackendFailure>;

    fn reopen_chain(
        &self,
        chain: &[DirectoryProof],
        root: &RootProof,
        final_access: DirectoryOpenAccess,
    ) -> Result<OpenDirectoryChain<Self::Handle>, BackendFailure>;
}

#[derive(Debug)]
struct PendingRow {
    relative_path: PathBuf,
    path: PathBuf,
    kind: SnapshotEntryKind,
    identity: NativeFileIdentity,
    parent_identity: NativeFileIdentity,
    parent_chain: Vec<DirectoryProof>,
    length: Option<u64>,
    modified: Option<SystemTime>,
    reparse_point: bool,
    cloud_placeholder: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DuplicateDiscoveryScanLimits {
    pub(crate) max_entries: usize,
    pub(crate) max_files: usize,
    pub(crate) max_directories: usize,
    pub(crate) max_depth: usize,
    pub(crate) max_path_bytes: usize,
}

pub(crate) fn remaining_duplicate_discovery_directory_budget(
    maximum: usize,
    already_admitted: usize,
    roots_reserved_for_later: usize,
) -> usize {
    maximum
        .saturating_sub(already_admitted)
        .saturating_sub(roots_reserved_for_later)
}

#[derive(Debug, Clone, Copy)]
struct ScanBounds {
    max_entries: usize,
    max_files: usize,
    max_directories: usize,
    max_scheduled_directories: Option<usize>,
    max_depth: usize,
    max_path_bytes: usize,
    build_rename_snapshot: bool,
}

fn dff_cloud_attribute_preflight(
    exclude_cloud_placeholders: bool,
    query_attributes: impl FnOnce() -> std::io::Result<u32>,
) -> std::io::Result<()> {
    if !exclude_cloud_placeholders {
        return Ok(());
    }
    let attributes = query_attributes()?;
    if has_cloud_placeholder_attributes(attributes) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "cloud-placeholder object excluded before open",
        ));
    }
    Ok(())
}

impl ScanBounds {
    fn rename_snapshot() -> Self {
        Self {
            max_entries: batch_rename::MAX_SNAPSHOT_ENTRIES,
            max_files: batch_rename::MAX_SNAPSHOT_ENTRIES,
            max_directories: MAX_RENAME_SCAN_DIRECTORIES,
            max_scheduled_directories: None,
            max_depth: MAX_RENAME_SCAN_DEPTH,
            max_path_bytes: usize::MAX,
            build_rename_snapshot: true,
        }
    }

    fn duplicate_discovery(limits: DuplicateDiscoveryScanLimits) -> Self {
        Self {
            max_entries: limits.max_entries,
            max_files: limits.max_files,
            max_directories: limits.max_directories,
            max_scheduled_directories: Some(limits.max_directories),
            max_depth: limits.max_depth,
            max_path_bytes: limits.max_path_bytes,
            build_rename_snapshot: false,
        }
    }
}

struct DirectoryFrame<H> {
    directory: Arc<OpenDirectory<H>>,
    guarded_chain: OpenDirectoryChain<H>,
    relative_path: PathBuf,
    chain: Vec<DirectoryProof>,
    path_bytes: usize,
}

fn path_storage_bytes(path: &Path) -> usize {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count().saturating_mul(2)
    }
    #[cfg(not(windows))]
    {
        path.as_os_str().as_encoded_bytes().len()
    }
}

fn directory_proof_path_bytes(chain: &[DirectoryProof]) -> usize {
    chain.iter().fold(0usize, |total, proof| {
        total
            .saturating_add(path_storage_bytes(&proof.lookup_path))
            .saturating_add(path_storage_bytes(&proof.canonical_path))
    })
}

fn has_cloud_placeholder_attributes(attributes: u32) -> bool {
    attributes
        & (FILE_ATTRIBUTE_OFFLINE_BITS
            | FILE_ATTRIBUTE_RECALL_ON_OPEN_BITS
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS_BITS)
        != 0
}

fn identity_is_supported(identity: NativeFileIdentity) -> bool {
    identity.file_id != [0; 16]
}

fn scan_with_backend<B: RenameScanBackend>(
    backend: &B,
    request: RenameScanRequest,
    cancellation: &ScanCancellation,
) -> RenameScanResult {
    scan_with_backend_bounded(
        backend,
        request,
        cancellation,
        ScanBounds::rename_snapshot(),
    )
}

fn scan_with_backend_bounded<B: RenameScanBackend>(
    backend: &B,
    request: RenameScanRequest,
    cancellation: &ScanCancellation,
    bounds: ScanBounds,
) -> RenameScanResult {
    scan_with_backend_bounded_progress(backend, request, cancellation, bounds, &mut |_| {})
}

fn scan_with_backend_bounded_progress<B: RenameScanBackend>(
    backend: &B,
    request: RenameScanRequest,
    cancellation: &ScanCancellation,
    bounds: ScanBounds,
    progress: &mut dyn FnMut(&RenameScanSummary),
) -> RenameScanResult {
    let mut result = empty_scan_result(&request, RenameScanStatus::InvalidRoot);
    if cancellation.is_cancelled() {
        result.status = RenameScanStatus::Cancelled;
        return result;
    }

    let admission = match backend.admit_root(&request) {
        Ok(admission) => admission,
        Err(error) => {
            result.status = status_for_root_failure(error.kind);
            record_failure(&mut result.summary, error);
            return result;
        }
    };
    result.canonical_root = Some(admission.proof.canonical_path.clone());
    result.root_identity = Some(admission.proof.identity);
    if let Some(expected) = request.expected_duplicate_root.as_ref()
        && (expected.identity != admission.proof.identity
            || batch_rename::compare_paths_windows_ordinal(
                &expected.canonical_root,
                &admission.proof.canonical_path,
            ) != std::cmp::Ordering::Equal)
    {
        result.status = RenameScanStatus::ChangedDuringScan;
        record_failure(
            &mut result.summary,
            BackendFailure::new(
                BackendFailureKind::RootChanged,
                Some(&admission.proof.canonical_path),
                "the admitted root identity changed after duplicate-scan preflight",
            ),
        );
        return result;
    }
    let Some(root_directory) = admission.directories.last() else {
        result.status = RenameScanStatus::InvalidRoot;
        record_issue(
            &mut result.summary,
            RenameScanIssue {
                path: Some(admission.proof.selected_path.clone()),
                kind: RenameScanIssueKind::InvalidEntry,
                message: "the admitted root has no retained directory chain".into(),
            },
        );
        return result;
    };
    if bounds.max_scheduled_directories.is_some() {
        // The selected root itself consumes one admitted directory even
        // before its first enumeration. Keep this separate from scanned
        // progress because an early partial result may still own queued child
        // directory admissions.
        result.summary.directories_admitted = 1;
    }

    let admission_chain_path_bytes = directory_proof_path_bytes(&admission.proof.chain)
        .saturating_mul(2)
        .saturating_add(path_storage_bytes(request.root.as_path()))
        .saturating_add(path_storage_bytes(&admission.proof.selected_path))
        .saturating_add(path_storage_bytes(&admission.proof.canonical_path));
    let root_frame_path_bytes = directory_proof_path_bytes(&admission.proof.chain);
    let mut queued_frame_path_bytes = root_frame_path_bytes;
    let mut discovered_directory_handle_path_bytes = 0usize;
    let mut pending_rows = Vec::new();
    let mut directories = VecDeque::new();
    directories.push_back(DirectoryFrame {
        directory: Arc::clone(root_directory),
        guarded_chain: admission.directories.clone(),
        relative_path: PathBuf::new(),
        chain: admission.proof.chain.clone(),
        path_bytes: root_frame_path_bytes,
    });
    let mut scan_status = RenameScanStatus::Complete;
    let mut result_path_bytes = path_storage_bytes(&admission.proof.canonical_path);
    if admission_chain_path_bytes
        .saturating_add(root_frame_path_bytes)
        .saturating_add(result_path_bytes)
        > bounds.max_path_bytes
    {
        result.status = RenameScanStatus::ResourceLimit;
        record_failure(
            &mut result.summary,
            BackendFailure::new(
                BackendFailureKind::ResourceLimit,
                Some(&admission.proof.canonical_path),
                format!(
                    "discovery path storage limit of {} bytes reached",
                    bounds.max_path_bytes
                ),
            ),
        );
        return result;
    }

    while let Some(frame) = directories.pop_front() {
        queued_frame_path_bytes = queued_frame_path_bytes.saturating_sub(frame.path_bytes);
        let active_frame_path_bytes = frame.path_bytes;
        if cancellation.is_cancelled() {
            scan_status = RenameScanStatus::Cancelled;
            break;
        }
        if result.summary.directories_scanned >= bounds.max_directories {
            scan_status = RenameScanStatus::ResourceLimit;
            record_failure(
                &mut result.summary,
                BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&frame.directory.canonical_path),
                    format!("directory limit of {} reached", bounds.max_directories),
                ),
            );
            break;
        }
        let relative_depth = if bounds.build_rename_snapshot {
            frame.chain.len()
        } else {
            frame
                .relative_path
                .components()
                .filter(|component| !matches!(component, Component::CurDir | Component::RootDir))
                .count()
        };
        if relative_depth > bounds.max_depth {
            scan_status = RenameScanStatus::ResourceLimit;
            record_failure(
                &mut result.summary,
                BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&frame.directory.canonical_path),
                    format!("directory depth limit of {} reached", bounds.max_depth),
                ),
            );
            break;
        }
        if let Err(error) = backend.reopen_chain(
            &frame.chain,
            &admission.proof,
            DirectoryOpenAccess::Metadata,
        ) {
            scan_status = status_for_scan_failure(error.kind);
            record_failure(&mut result.summary, error);
            break;
        }
        let remaining_entries = bounds
            .max_entries
            .saturating_sub(result.summary.entries_seen);
        let fixed_path_bytes = admission_chain_path_bytes
            .saturating_add(result_path_bytes)
            .saturating_add(queued_frame_path_bytes)
            .saturating_add(discovered_directory_handle_path_bytes)
            .saturating_add(active_frame_path_bytes);
        let remaining_path_bytes = bounds.max_path_bytes.saturating_sub(fixed_path_bytes);
        let mut names = match backend.enumerate_directory(
            &frame.directory,
            remaining_entries,
            remaining_path_bytes,
            cancellation,
        ) {
            Ok(names) => names,
            Err(error) => {
                record_failure(&mut result.summary, error.clone());
                scan_status = status_for_directory_failure(
                    error.kind,
                    result.summary.directories_scanned == 0,
                );
                break;
            }
        };
        let names_path_bytes = names.iter().fold(0usize, |total, entry| {
            total.saturating_add(path_storage_bytes(Path::new(&entry.name)))
        });
        if fixed_path_bytes.saturating_add(names_path_bytes) > bounds.max_path_bytes {
            scan_status = RenameScanStatus::ResourceLimit;
            record_failure(
                &mut result.summary,
                BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&frame.directory.canonical_path),
                    format!(
                        "discovery path storage limit of {} bytes reached while enumerating names",
                        bounds.max_path_bytes
                    ),
                ),
            );
            break;
        }
        if cancellation.is_cancelled() {
            scan_status = RenameScanStatus::Cancelled;
            break;
        }
        if let Err(error) = backend.reopen_chain(
            &frame.chain,
            &admission.proof,
            DirectoryOpenAccess::Metadata,
        ) {
            scan_status = status_for_scan_failure(error.kind);
            record_failure(&mut result.summary, error);
            break;
        }
        names.sort_by(|left, right| {
            batch_rename::compare_paths_windows_ordinal(
                Path::new(&left.name),
                Path::new(&right.name),
            )
            .then_with(|| left.name.cmp(&right.name))
        });
        result.summary.directories_scanned += 1;
        progress(&result.summary);

        for entry in names {
            if cancellation.is_cancelled() {
                scan_status = RenameScanStatus::Cancelled;
                break;
            }
            if entry.name == OsStr::new(".") || entry.name == OsStr::new("..") {
                continue;
            }
            if result.summary.entries_seen >= bounds.max_entries {
                scan_status = RenameScanStatus::ResourceLimit;
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&frame.directory.canonical_path),
                        format!("entry limit of {} reached", bounds.max_entries),
                    ),
                );
                break;
            }
            result.summary.entries_seen += 1;
            if result.summary.entries_seen.is_multiple_of(256) {
                progress(&result.summary);
            }
            if admission.proof.exclude_cloud_placeholders && entry.cloud_placeholder {
                result.summary.cloud_placeholders_excluded += 1;
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(frame.directory.canonical_path.join(&entry.name)),
                        kind: RenameScanIssueKind::ReparsePointExcluded,
                        message: "cloud placeholder skipped from directory metadata without opening or requesting hydration".into(),
                    },
                );
                continue;
            }
            if admission.proof.exclude_cloud_placeholders && entry.is_reparse_point {
                result.summary.reparse_entries_excluded += 1;
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(frame.directory.canonical_path.join(&entry.name)),
                        kind: RenameScanIssueKind::ReparsePointExcluded,
                        message:
                            "reparse entry skipped from directory metadata without opening or following it"
                                .into(),
                    },
                );
                continue;
            }
            if admission.proof.recursive
                && entry.is_directory
                && !entry.is_reparse_point
                && let Some(max_scheduled_directories) = bounds.max_scheduled_directories
                && result.summary.directories_admitted >= max_scheduled_directories
            {
                scan_status = RenameScanStatus::ResourceLimit;
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&frame.directory.canonical_path.join(&entry.name)),
                        format!(
                            "scheduled directory limit of {max_scheduled_directories} reached before child acquisition"
                        ),
                    ),
                );
                break;
            }
            let opened = match backend.open_child(
                &frame.directory,
                &frame.chain,
                &admission.proof,
                &entry.name,
                Some(&entry),
            ) {
                Ok(opened) => opened,
                Err(error) => {
                    if error.kind == BackendFailureKind::Inaccessible {
                        result.summary.inaccessible_entries += 1;
                        record_failure(&mut result.summary, error);
                        if scan_status == RenameScanStatus::Complete {
                            scan_status = RenameScanStatus::Partial;
                        }
                        continue;
                    }
                    record_failure(&mut result.summary, error.clone());
                    scan_status = status_for_scan_failure(error.kind);
                    break;
                }
            };
            if !batch_rename::path_is_within_windows_ordinal(
                &admission.proof.canonical_path,
                &opened.canonical_path,
            ) {
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::PathEscapedRoot,
                        Some(&opened.canonical_path),
                        "opened object resolved outside the selected root",
                    ),
                );
                scan_status = RenameScanStatus::ChangedDuringScan;
                break;
            }
            if opened.kind == SnapshotEntryKind::File
                && result.summary.files_seen >= bounds.max_files
            {
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&opened.canonical_path),
                        format!("file limit of {} reached", bounds.max_files),
                    ),
                );
                scan_status = RenameScanStatus::ResourceLimit;
                break;
            }
            let relative_path = frame.relative_path.join(&entry.name);
            let row_path_bytes = path_storage_bytes(&opened.canonical_path)
                .saturating_add(path_storage_bytes(&relative_path));
            let next_path_bytes = result_path_bytes.saturating_add(row_path_bytes);
            let live_path_bytes = admission_chain_path_bytes
                .saturating_add(next_path_bytes)
                .saturating_add(queued_frame_path_bytes)
                .saturating_add(discovered_directory_handle_path_bytes)
                .saturating_add(active_frame_path_bytes)
                .saturating_add(names_path_bytes)
                .saturating_add(path_storage_bytes(&opened.canonical_path))
                .saturating_add(path_storage_bytes(&opened.lookup_path));
            if live_path_bytes > bounds.max_path_bytes {
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&opened.canonical_path),
                        format!(
                            "discovery path storage limit of {} bytes reached",
                            bounds.max_path_bytes
                        ),
                    ),
                );
                scan_status = RenameScanStatus::ResourceLimit;
                break;
            }
            match opened.kind {
                SnapshotEntryKind::File => result.summary.files_seen += 1,
                SnapshotEntryKind::Directory => {
                    result.summary.directories_seen += 1;
                    if bounds.max_scheduled_directories.is_some() {
                        result.summary.directories_admitted =
                            result.summary.directories_admitted.saturating_add(1);
                    }
                }
                SnapshotEntryKind::Other => result.summary.other_entries_seen += 1,
            }
            if opened.kind == SnapshotEntryKind::File
                && (opened.length.is_none() || opened.modified.is_none())
            {
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::UnsupportedIdentity,
                        Some(&opened.canonical_path),
                        "the filesystem did not provide complete file metadata for recheck",
                    ),
                );
                scan_status = RenameScanStatus::Unsupported;
                break;
            }
            if admission.proof.exclude_cloud_placeholders
                && opened.kind == SnapshotEntryKind::Directory
                && !identity_is_supported(opened.identity)
            {
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(opened.canonical_path.clone()),
                        kind: RenameScanIssueKind::UnsupportedIdentity,
                        message:
                            "directory identity is unsupported; its descendants were not traversed"
                                .into(),
                    },
                );
                if scan_status == RenameScanStatus::Complete {
                    scan_status = RenameScanStatus::Partial;
                }
                continue;
            }
            let excluded_cloud_placeholder =
                admission.proof.exclude_cloud_placeholders && opened.cloud_placeholder;
            if opened.reparse_point || excluded_cloud_placeholder {
                if excluded_cloud_placeholder {
                    result.summary.cloud_placeholders_excluded += 1;
                } else {
                    result.summary.reparse_entries_excluded += 1;
                }
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(opened.canonical_path.clone()),
                        kind: RenameScanIssueKind::ReparsePointExcluded,
                        message: if excluded_cloud_placeholder {
                            "cloud placeholder skipped without requesting hydration".into()
                        } else {
                            "reparse point retained as an ineligible sibling blocker; traversal was skipped".into()
                        },
                    },
                );
            }
            if opened.kind == SnapshotEntryKind::Directory
                && admission.proof.recursive
                && !opened.reparse_point
                && !excluded_cloud_placeholder
            {
                let child_chain_path_bytes = directory_proof_path_bytes(&frame.chain)
                    .saturating_add(path_storage_bytes(&opened.lookup_path))
                    .saturating_add(path_storage_bytes(&opened.canonical_path));
                let child_relative_path_bytes = path_storage_bytes(&relative_path);
                let child_directory_handle_path_bytes = path_storage_bytes(&opened.lookup_path)
                    .saturating_add(path_storage_bytes(&opened.canonical_path));
                let child_frame_path_bytes =
                    child_chain_path_bytes.saturating_add(child_relative_path_bytes);
                let live_with_child = admission_chain_path_bytes
                    .saturating_add(result_path_bytes)
                    .saturating_add(row_path_bytes)
                    .saturating_add(queued_frame_path_bytes)
                    .saturating_add(discovered_directory_handle_path_bytes)
                    .saturating_add(active_frame_path_bytes)
                    .saturating_add(names_path_bytes)
                    .saturating_add(path_storage_bytes(&opened.canonical_path))
                    .saturating_add(path_storage_bytes(&opened.lookup_path))
                    .saturating_add(child_frame_path_bytes)
                    .saturating_add(child_directory_handle_path_bytes);
                if live_with_child > bounds.max_path_bytes {
                    record_failure(
                        &mut result.summary,
                        BackendFailure::new(
                            BackendFailureKind::ResourceLimit,
                            Some(&opened.canonical_path),
                            format!(
                                "discovery path storage limit of {} bytes reached",
                                bounds.max_path_bytes
                            ),
                        ),
                    );
                    scan_status = RenameScanStatus::ResourceLimit;
                    break;
                }
                let mut child_chain = frame.chain.clone();
                child_chain.push(DirectoryProof {
                    lookup_path: opened.lookup_path.clone(),
                    canonical_path: opened.canonical_path.clone(),
                    identity: opened.identity,
                });
                let child_directory = match backend.acquire_child_directory_for_enumeration(
                    &frame.directory,
                    &frame.chain,
                    &admission.proof,
                    &opened,
                ) {
                    Ok(child_directory) => child_directory,
                    Err(error) if error.kind == BackendFailureKind::Inaccessible => {
                        result.summary.inaccessible_entries += 1;
                        record_failure(&mut result.summary, error);
                        if scan_status == RenameScanStatus::Complete {
                            scan_status = RenameScanStatus::Partial;
                        }
                        continue;
                    }
                    Err(error) => {
                        record_failure(&mut result.summary, error.clone());
                        scan_status = status_for_scan_failure(error.kind);
                        break;
                    }
                };
                let child_directory = Arc::new(child_directory);
                let mut guarded_chain = frame.guarded_chain.clone();
                guarded_chain.push(Arc::clone(&child_directory));
                discovered_directory_handle_path_bytes = discovered_directory_handle_path_bytes
                    .saturating_add(child_directory_handle_path_bytes);
                queued_frame_path_bytes =
                    queued_frame_path_bytes.saturating_add(child_frame_path_bytes);
                directories.push_back(DirectoryFrame {
                    directory: child_directory,
                    guarded_chain,
                    relative_path: relative_path.clone(),
                    chain: child_chain,
                    path_bytes: child_frame_path_bytes,
                });
            }
            result_path_bytes = next_path_bytes;
            pending_rows.push(PendingRow {
                relative_path,
                path: opened.canonical_path,
                kind: opened.kind,
                identity: opened.identity,
                parent_identity: frame.directory.identity,
                parent_chain: if bounds.build_rename_snapshot {
                    frame.chain.clone()
                } else {
                    Vec::new()
                },
                length: opened.length,
                modified: opened.modified,
                reparse_point: opened.reparse_point,
                cloud_placeholder: opened.cloud_placeholder,
            });
        }
        if scan_status != RenameScanStatus::Complete && scan_status != RenameScanStatus::Partial {
            break;
        }
    }

    if cancellation.is_cancelled() {
        scan_status = RenameScanStatus::Cancelled;
    }
    if matches!(
        scan_status,
        RenameScanStatus::Complete | RenameScanStatus::Partial
    ) && let Err(error) = backend.reopen_chain(
        &admission.proof.chain,
        &admission.proof,
        DirectoryOpenAccess::Metadata,
    ) {
        record_failure(&mut result.summary, error.clone());
        scan_status = status_for_scan_failure(error.kind);
    }

    pending_rows.sort_by(|left, right| {
        batch_rename::compare_paths_windows_ordinal(&left.path, &right.path)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    let mut rows = Vec::with_capacity(pending_rows.len());
    let mut proofs = bounds
        .build_rename_snapshot
        .then(|| HashMap::with_capacity(pending_rows.len()));
    let mut entries = bounds
        .build_rename_snapshot
        .then(|| Vec::with_capacity(pending_rows.len()));
    for (index, pending) in pending_rows.into_iter().enumerate() {
        // The duplicate-discovery adapter has a larger bounded metadata result
        // but does not expose rename references or construct a rename snapshot.
        let reference = SnapshotEntryRef(index as u64);
        let source_order = index as u64;
        if let Some(proofs) = proofs.as_mut() {
            proofs.insert(
                reference,
                EntryProof {
                    path: pending.path.clone(),
                    identity: pending.identity,
                    length: pending.length,
                    modified: pending.modified,
                    parent_chain: pending.parent_chain.clone(),
                },
            );
        }
        if let Some(entries) = entries.as_mut() {
            entries.push(
                RenameSnapshotEntry::new(
                    reference,
                    pending.path.clone(),
                    source_order,
                    EntryInclusion::Included,
                    false,
                )
                .with_kind(pending.kind),
            );
        }
        rows.push(RenameScanRow {
            entry_ref: reference,
            relative_path: pending.relative_path,
            path: pending.path,
            kind: pending.kind,
            identity: pending.identity,
            parent_identity: pending.parent_identity,
            length: pending.length,
            modified: pending.modified,
            reparse_point: pending.reparse_point,
            cloud_placeholder: pending.cloud_placeholder,
        });
    }
    result.rows = rows;
    result.status = scan_status;

    if scan_status == RenameScanStatus::Complete && bounds.build_rename_snapshot {
        let Some(proofs) = proofs else {
            unreachable!("rename scans retain their proofs")
        };
        let Some(entries) = entries else {
            unreachable!("rename scans retain their entries")
        };
        let root = match ExplicitDirectoryRoot::new(admission.proof.canonical_path.clone()) {
            Ok(root) => root,
            Err(error) => {
                result.status = RenameScanStatus::Unsupported;
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(admission.proof.canonical_path.clone()),
                        kind: RenameScanIssueKind::InvalidEntry,
                        message: format!(
                            "canonical root is not representable by the rename model: {error:?}"
                        ),
                    },
                );
                return result;
            }
        };
        let snapshot = match RenameInputSnapshot::new(root, entries)
            .and_then(|snapshot| snapshot.with_recursive(admission.proof.recursive))
        {
            Ok(snapshot) => snapshot
                .with_revisions(SnapshotRevision(request.generation.0), SelectionRevision(0)),
            Err(error) => {
                result.status = RenameScanStatus::Unsupported;
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: result.canonical_root.clone(),
                        kind: RenameScanIssueKind::InvalidEntry,
                        message: format!(
                            "enumerated entries could not be represented by the rename model: {error:?}"
                        ),
                    },
                );
                return result;
            }
        };
        result.snapshot = Some(snapshot);
        result.proof = Some(Arc::new(ScanProof {
            root: admission.proof,
            entries: proofs,
        }));
    }
    result
}

fn empty_scan_result(request: &RenameScanRequest, status: RenameScanStatus) -> RenameScanResult {
    RenameScanResult {
        generation: request.generation,
        scope: request.scope,
        status,
        canonical_root: None,
        root_identity: None,
        rows: Vec::new(),
        snapshot: None,
        proof: None,
        summary: RenameScanSummary::default(),
    }
}

fn record_issue(summary: &mut RenameScanSummary, issue: RenameScanIssue) {
    summary.issue_count = summary.issue_count.saturating_add(1);
    if summary.issues.len() < MAX_RENAME_SCAN_ISSUE_SAMPLES {
        summary.issues.push(issue);
    } else {
        summary.issues_truncated = true;
    }
}

fn record_failure(summary: &mut RenameScanSummary, failure: BackendFailure) {
    let kind = match failure.kind {
        BackendFailureKind::Inaccessible => RenameScanIssueKind::Inaccessible,
        BackendFailureKind::UnsupportedIdentity => RenameScanIssueKind::UnsupportedIdentity,
        BackendFailureKind::RootChanged => RenameScanIssueKind::RootChanged,
        BackendFailureKind::ParentChanged => RenameScanIssueKind::ParentChanged,
        BackendFailureKind::ObjectChanged => RenameScanIssueKind::InvalidEntry,
        BackendFailureKind::PathEscapedRoot => RenameScanIssueKind::PathEscapedRoot,
        BackendFailureKind::InvalidEntry | BackendFailureKind::InvalidRoot => {
            RenameScanIssueKind::InvalidEntry
        }
        BackendFailureKind::ResourceLimit => RenameScanIssueKind::ResourceLimit,
        BackendFailureKind::Cancelled => return,
    };
    record_issue(
        summary,
        RenameScanIssue {
            path: failure.path,
            kind,
            message: failure.message,
        },
    );
}

fn status_for_root_failure(kind: BackendFailureKind) -> RenameScanStatus {
    match kind {
        BackendFailureKind::InvalidRoot | BackendFailureKind::InvalidEntry => {
            RenameScanStatus::InvalidRoot
        }
        BackendFailureKind::Inaccessible => RenameScanStatus::Inaccessible,
        BackendFailureKind::UnsupportedIdentity => RenameScanStatus::Unsupported,
        BackendFailureKind::Cancelled => RenameScanStatus::Cancelled,
        BackendFailureKind::ResourceLimit => RenameScanStatus::ResourceLimit,
        BackendFailureKind::RootChanged
        | BackendFailureKind::ParentChanged
        | BackendFailureKind::ObjectChanged
        | BackendFailureKind::PathEscapedRoot => RenameScanStatus::ChangedDuringScan,
    }
}

fn status_for_directory_failure(
    kind: BackendFailureKind,
    root_directory: bool,
) -> RenameScanStatus {
    match kind {
        BackendFailureKind::Inaccessible if root_directory => RenameScanStatus::Inaccessible,
        BackendFailureKind::Inaccessible => RenameScanStatus::Partial,
        other => status_for_root_failure(other),
    }
}

fn status_for_scan_failure(kind: BackendFailureKind) -> RenameScanStatus {
    match kind {
        BackendFailureKind::Inaccessible => RenameScanStatus::Partial,
        other => status_for_root_failure(other),
    }
}

fn fresh_status_for_failure(kind: BackendFailureKind) -> FreshCheckStatus {
    match kind {
        BackendFailureKind::InvalidRoot
        | BackendFailureKind::RootChanged
        | BackendFailureKind::PathEscapedRoot => FreshCheckStatus::RootChanged,
        BackendFailureKind::ObjectChanged => FreshCheckStatus::SourceChanged,
        BackendFailureKind::ParentChanged => FreshCheckStatus::ParentChanged,
        BackendFailureKind::Inaccessible | BackendFailureKind::InvalidEntry => {
            FreshCheckStatus::Inaccessible
        }
        BackendFailureKind::UnsupportedIdentity => FreshCheckStatus::Unsupported,
        BackendFailureKind::ResourceLimit => FreshCheckStatus::Inaccessible,
        BackendFailureKind::Cancelled => FreshCheckStatus::Cancelled,
    }
}

fn recheck_with_backend<B: RenameScanBackend>(
    backend: &B,
    input: &FreshRecheckInput,
    cancellation: &ScanCancellation,
) -> FreshRecheckResult {
    let (root_status, root_guards) = if cancellation.is_cancelled() {
        (FreshCheckStatus::Cancelled, None)
    } else {
        match backend.reopen_chain(
            &input.root.chain,
            &input.root,
            DirectoryOpenAccess::Metadata,
        ) {
            Ok(guards)
                if guards
                    .last()
                    .is_some_and(|directory| directory.identity == input.root.identity) =>
            {
                (FreshCheckStatus::Current, Some(guards))
            }
            Ok(_) => (FreshCheckStatus::RootChanged, None),
            Err(error) => (fresh_status_for_failure(error.kind), None),
        }
    };
    // Keep the admitted root and every ancestor guarded for this entire
    // read-only recheck. The handles are deliberately not retained in proofs.
    let _root_guards = root_guards;
    if root_status != FreshCheckStatus::Current {
        let entries = input
            .entries
            .iter()
            .map(|entry| FreshRecheckEntryResult {
                entry_ref: entry.entry_ref,
                source_status: root_status,
                observed_source_identity: None,
                observed_source_length: None,
                observed_source_modified: None,
                destination: DestinationOccupancy::Unknown(root_status),
            })
            .collect();
        return FreshRecheckResult {
            generation: input.root.generation,
            root_status,
            entries,
        };
    }

    let mut entries = Vec::with_capacity(input.entries.len());
    for candidate in &input.entries {
        if cancellation.is_cancelled() {
            entries.push(FreshRecheckEntryResult {
                entry_ref: candidate.entry_ref,
                source_status: FreshCheckStatus::Cancelled,
                observed_source_identity: None,
                observed_source_length: None,
                observed_source_modified: None,
                destination: DestinationOccupancy::Unknown(FreshCheckStatus::Cancelled),
            });
            continue;
        }
        let parent_guards = match backend.reopen_chain(
            &candidate.parent_chain,
            &input.root,
            DirectoryOpenAccess::Enumerate,
        ) {
            Ok(parent_guards) if !parent_guards.is_empty() => parent_guards,
            Ok(_) => {
                entries.push(FreshRecheckEntryResult {
                    entry_ref: candidate.entry_ref,
                    source_status: FreshCheckStatus::ParentChanged,
                    observed_source_identity: None,
                    observed_source_length: None,
                    observed_source_modified: None,
                    destination: DestinationOccupancy::Unknown(FreshCheckStatus::ParentChanged),
                });
                continue;
            }
            Err(error) => {
                let status = fresh_status_for_failure(error.kind);
                entries.push(FreshRecheckEntryResult {
                    entry_ref: candidate.entry_ref,
                    source_status: status,
                    observed_source_identity: None,
                    observed_source_length: None,
                    observed_source_modified: None,
                    destination: DestinationOccupancy::Unknown(status),
                });
                continue;
            }
        };
        let Some(parent) = parent_guards.last() else {
            entries.push(FreshRecheckEntryResult {
                entry_ref: candidate.entry_ref,
                source_status: FreshCheckStatus::ParentChanged,
                observed_source_identity: None,
                observed_source_length: None,
                observed_source_modified: None,
                destination: DestinationOccupancy::Unknown(FreshCheckStatus::ParentChanged),
            });
            continue;
        };
        let Some(source_name) = candidate.source_path.file_name() else {
            entries.push(FreshRecheckEntryResult {
                entry_ref: candidate.entry_ref,
                source_status: FreshCheckStatus::SourceChanged,
                observed_source_identity: None,
                observed_source_length: None,
                observed_source_modified: None,
                destination: DestinationOccupancy::Unknown(FreshCheckStatus::SourceChanged),
            });
            continue;
        };
        let source = match backend.open_child(
            parent,
            &candidate.parent_chain,
            &input.root,
            source_name,
            None,
        ) {
            Ok(source) => source,
            Err(error) => {
                let status = match error.kind {
                    BackendFailureKind::RootChanged | BackendFailureKind::PathEscapedRoot => {
                        FreshCheckStatus::RootChanged
                    }
                    BackendFailureKind::ObjectChanged => FreshCheckStatus::SourceChanged,
                    BackendFailureKind::ParentChanged => FreshCheckStatus::ParentChanged,
                    BackendFailureKind::UnsupportedIdentity => FreshCheckStatus::Unsupported,
                    BackendFailureKind::Cancelled => FreshCheckStatus::Cancelled,
                    _ => FreshCheckStatus::SourceChanged,
                };
                entries.push(FreshRecheckEntryResult {
                    entry_ref: candidate.entry_ref,
                    source_status: status,
                    observed_source_identity: None,
                    observed_source_length: None,
                    observed_source_modified: None,
                    destination: DestinationOccupancy::Unknown(status),
                });
                continue;
            }
        };
        let source_status = if source.identity != candidate.expected_source_identity
            || source.kind != SnapshotEntryKind::File
            || source.reparse_point
            || !candidate.path_matches_proof
        {
            FreshCheckStatus::SourceChanged
        } else if candidate.expected_source_length.is_none()
            || candidate.expected_source_modified.is_none()
            || source.length.is_none()
            || source.modified.is_none()
        {
            FreshCheckStatus::Unsupported
        } else if source.length != candidate.expected_source_length
            || source.modified != candidate.expected_source_modified
        {
            FreshCheckStatus::SourceChanged
        } else {
            FreshCheckStatus::Current
        };
        let observed_source_identity = Some(source.identity);
        let observed_source_length = source.length;
        let observed_source_modified = source.modified;

        let destination_name = candidate.target_path.file_name();
        let destination = if source_status != FreshCheckStatus::Current {
            DestinationOccupancy::Unknown(source_status)
        } else if let Some(destination_name) = destination_name {
            match backend.enumerate_directory(
                parent,
                batch_rename::MAX_SNAPSHOT_ENTRIES,
                usize::MAX,
                cancellation,
            ) {
                Ok(names) => {
                    let mut occupants = Vec::new();
                    let mut read_error = None;
                    for name in names.iter().filter(|name| {
                        batch_rename::compare_paths_windows_ordinal(
                            Path::new(&name.name),
                            Path::new(destination_name),
                        ) == std::cmp::Ordering::Equal
                    }) {
                        match backend.open_child(
                            parent,
                            &candidate.parent_chain,
                            &input.root,
                            &name.name,
                            Some(name),
                        ) {
                            Ok(occupant) => occupants.push(occupant.identity),
                            Err(error) => {
                                read_error = Some(fresh_status_for_failure(error.kind));
                                break;
                            }
                        }
                    }
                    if let Some(status) = read_error {
                        DestinationOccupancy::Unknown(status)
                    } else if occupants.is_empty() {
                        DestinationOccupancy::Vacant
                    } else {
                        DestinationOccupancy::Occupied(occupants)
                    }
                }
                Err(error) => DestinationOccupancy::Unknown(fresh_status_for_failure(error.kind)),
            }
        } else {
            DestinationOccupancy::Unknown(FreshCheckStatus::SourceChanged)
        };
        entries.push(FreshRecheckEntryResult {
            entry_ref: candidate.entry_ref,
            source_status,
            observed_source_identity,
            observed_source_length,
            observed_source_modified,
            destination,
        });
    }
    FreshRecheckResult {
        generation: input.root.generation,
        root_status,
        entries,
    }
}

/// Reopen and compare the verified root, parent chains and source file IDs,
/// then report every current occupant of each same-parent destination.
/// This performs no write and does not promise that the result stays fresh.
pub fn recheck_verified_rename_plan(
    plan: &VerifiedRenamePlan,
    cancellation: &ScanCancellation,
) -> FreshRecheckResult {
    recheck_with_backend(
        &NativeRenameScanBackend,
        &plan.fresh_recheck_input(),
        cancellation,
    )
}

/// Acquire and retain native source and parent handles for the exact verified
/// plan. This is a complete preflight: every source, metadata precondition,
/// target and sibling occupancy is checked before the caller may persist a
/// journal or move a file.
pub(crate) fn prepare_guarded_rename_batch(
    plan: &VerifiedRenamePlan,
    cancellation: &ScanCancellation,
) -> Result<GuardedRenameBatch, String> {
    #[cfg(windows)]
    {
        prepare_guarded_rename_batch_native(plan, cancellation)
    }
    #[cfg(not(windows))]
    {
        let _ = (plan, cancellation);
        Err("safe rename-by-handle is available only on Windows".to_owned())
    }
}

/// Reopen a persisted receipt using only its exact recorded paths. The
/// journal is untrusted input; callers must validate path relationships
/// before asking this boundary to inspect the root, parent and candidates.
pub(crate) fn inspect_recorded_rename_file(
    root_path: &Path,
    root_identity: NativeFileIdentity,
    parent_path: &Path,
    parent_identity: NativeFileIdentity,
    expected_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    expected_sha256: [u8; 32],
    candidates: &[PathBuf],
    cancellation: &ScanCancellation,
) -> Result<Vec<PathBuf>, String> {
    #[cfg(windows)]
    {
        let root = verified_root_proof(root_path, root_identity)?;
        let parent_chain = verified_parent_chain(&root, parent_path, parent_identity)?;
        let backend = NativeRenameScanBackend;
        let root_guards = backend
            .reopen_chain(&root.chain, &root, DirectoryOpenAccess::Enumerate)
            .map_err(|error| error.to_string())?;
        let parent_guards = backend
            .reopen_chain(&parent_chain, &root, DirectoryOpenAccess::Enumerate)
            .map_err(|error| error.to_string())?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| "verified receipt parent is missing".to_owned())?;
        let parent_canonical_path = parent.canonical_path.clone();
        let _retained = (root_guards, parent_guards);
        let mut matches = Vec::new();
        for path in candidates {
            if cancellation.is_cancelled() {
                return Err("recovery verification was cancelled".to_owned());
            }
            if path.parent().is_none_or(|actual_parent| {
                batch_rename::compare_paths_windows_ordinal(actual_parent, parent_path)
                    != std::cmp::Ordering::Equal
            }) {
                return Err(format!(
                    "receipt candidate {} is outside its recorded parent",
                    path.display()
                ));
            }
            let Some(leaf) = path.file_name() else {
                return Err(format!(
                    "receipt candidate {} has no filename",
                    path.display()
                ));
            };
            let opened = match open_native_verified_file(&parent_canonical_path.join(leaf)) {
                Ok(opened) => opened,
                Err(error) if native_error_is_not_found(&error) => continue,
                Err(error) => return Err(error.to_string()),
            };
            if opened.identity == expected_identity
                && opened.length == Some(expected_length)
                && opened.modified == Some(expected_modified)
                && hash_file_contents(
                    &opened.handle,
                    ContentVerificationMode::Cooperative(cancellation),
                )
                .map_err(|error| error.to_string())?
                    == expected_sha256
                && batch_rename::compare_paths_windows_ordinal(&opened.canonical_path, path)
                    == std::cmp::Ordering::Equal
                && opened.canonical_path.file_name() == path.file_name()
            {
                matches.push(path.clone());
            }
        }
        Ok(matches)
    }
    #[cfg(not(windows))]
    {
        let _ = (
            root_path,
            root_identity,
            parent_path,
            parent_identity,
            expected_identity,
            expected_length,
            expected_modified,
            expected_sha256,
            candidates,
            cancellation,
        );
        Err("native receipt verification is available only on Windows".to_owned())
    }
}

#[cfg(windows)]
fn prepare_guarded_rename_batch_native(
    plan: &VerifiedRenamePlan,
    cancellation: &ScanCancellation,
) -> Result<GuardedRenameBatch, String> {
    if plan.plan.actions().is_empty() {
        return Err("the confirmed plan contains no changed filenames".to_owned());
    }
    if plan.plan.actions().len() > batch_rename::MAX_SELECTED_ENTRIES {
        return Err("the confirmed plan exceeds the selected-file limit".to_owned());
    }

    let backend = NativeRenameScanBackend;
    let root = &plan.proof.root;
    let root_guards = backend
        .reopen_chain(&root.chain, root, DirectoryOpenAccess::Enumerate)
        .map_err(|error| error.to_string())?;
    if root_guards
        .last()
        .is_none_or(|directory| directory.identity != root.identity)
    {
        return Err("the selected root changed before apply".to_owned());
    }

    let mut files = Vec::with_capacity(plan.plan.actions().len());
    let mut unique_identities = std::collections::HashSet::with_capacity(plan.plan.actions().len());
    let mut total_length = 0_u64;
    for action in plan.plan.actions() {
        if cancellation.is_cancelled() {
            return Err("rename was cancelled before any file was moved".to_owned());
        }
        let entry = plan
            .proof
            .entries
            .get(&action.source_ref())
            .ok_or_else(|| "the confirmed action has no native scan proof".to_owned())?;
        if batch_rename::compare_paths_windows_ordinal(&entry.path, action.source_path())
            != std::cmp::Ordering::Equal
        {
            return Err(format!(
                "source path changed in the confirmed plan: {}",
                action.source_path().display()
            ));
        }
        let parent_path = action
            .source_path()
            .parent()
            .ok_or_else(|| "a rename source has no parent directory".to_owned())?;
        if action.target_path().parent().is_none_or(|target_parent| {
            batch_rename::compare_paths_windows_ordinal(target_parent, parent_path)
                != std::cmp::Ordering::Equal
        }) {
            return Err(format!(
                "cross-directory rename is not supported: {}",
                action.target_path().display()
            ));
        }
        if !batch_rename::path_is_within_windows_ordinal(&root.canonical_path, parent_path) {
            return Err(format!(
                "rename parent escaped the selected root: {}",
                parent_path.display()
            ));
        }

        let parent_guards = backend
            .reopen_chain(&entry.parent_chain, root, DirectoryOpenAccess::Enumerate)
            .map_err(|error| error.to_string())?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| "a verified source parent is missing".to_owned())?;
        if batch_rename::compare_paths_windows_ordinal(&parent.canonical_path, parent_path)
            != std::cmp::Ordering::Equal
            || parent.identity
                != entry
                    .parent_chain
                    .last()
                    .map(|proof| proof.identity)
                    .unwrap_or(root.identity)
        {
            return Err(format!(
                "source parent changed before apply: {}",
                parent_path.display()
            ));
        }

        let opened = open_native_mutation_file(action.source_path()).map_err(|error| {
            format!(
                "could not guard source {}: {error}",
                action.source_path().display()
            )
        })?;
        if opened.kind != SnapshotEntryKind::File
            || opened.reparse_point
            || opened.identity != entry.identity
            || opened.length != entry.length
            || opened.modified != entry.modified
            || entry.length.is_none()
            || entry.modified.is_none()
            || batch_rename::compare_paths_windows_ordinal(
                &opened.canonical_path,
                action.source_path(),
            ) != std::cmp::Ordering::Equal
            || opened.canonical_path.parent().is_none_or(|actual_parent| {
                batch_rename::compare_paths_windows_ordinal(actual_parent, parent_path)
                    != std::cmp::Ordering::Equal
            })
        {
            return Err(format!(
                "source changed since preview: {}",
                action.source_path().display()
            ));
        }
        if !unique_identities.insert(opened.identity) {
            return Err(
                "the confirmed selection contains duplicate native file identities; safe recovery would be ambiguous"
                    .to_owned(),
            );
        }
        let length = entry
            .length
            .ok_or_else(|| "source length is unavailable".to_owned())?;
        let expected_modified = entry
            .modified
            .ok_or_else(|| "source last-modified time is unavailable".to_owned())?;
        if length > MAX_RENAME_HASH_BYTES_PER_FILE {
            return Err(format!(
                "source exceeds the {MAX_RENAME_HASH_BYTES_PER_FILE}-byte content verification limit: {}",
                action.source_path().display()
            ));
        }
        total_length = total_length.saturating_add(length);
        if total_length > MAX_RENAME_HASH_BYTES_PER_BATCH {
            return Err(format!(
                "selected files exceed the {MAX_RENAME_HASH_BYTES_PER_BATCH}-byte content verification limit"
            ));
        }
        let content_sha256 = hash_file_contents(
            &opened.handle,
            ContentVerificationMode::Cooperative(cancellation),
        )
        .map_err(|error| {
            format!(
                "could not fingerprint {}: {error}",
                action.source_path().display()
            )
        })?;
        let parent_path = parent.canonical_path.clone();
        let parent_identity = parent.identity;
        files.push(GuardedRenameFile {
            source_ref: action.source_ref(),
            original_path: action.source_path().to_path_buf(),
            target_path: action.target_path().to_path_buf(),
            current_path: action.source_path().to_path_buf(),
            parent_path,
            parent_identity,
            identity: opened.identity,
            expected_length: length,
            expected_modified,
            content_sha256,
            source: opened.handle,
            root: root.clone(),
            parent_chain: entry.parent_chain.clone(),
            _root_guards: root_guards.clone(),
            _parent_guards: parent_guards,
        });
    }
    check_guarded_destinations(&backend, root, &files)?;
    for file in &files {
        file.verify_at(
            &file.original_path,
            ContentVerificationMode::Cooperative(cancellation),
        )
        .map_err(|error| format!("source changed during preflight: {error}"))?;
    }
    Ok(GuardedRenameBatch { files })
}

#[cfg(windows)]
pub(crate) fn prepare_guarded_undo_batch(
    root_path: &Path,
    root_identity: NativeFileIdentity,
    specs: &[UndoGuardSpec],
    cancellation: &ScanCancellation,
) -> Result<GuardedRenameBatch, String> {
    if specs.is_empty() {
        return Err("the recovery receipt contains no restorable entries".to_owned());
    }
    if specs.len() > batch_rename::MAX_SELECTED_ENTRIES {
        return Err("the recovery receipt exceeds the selected-file limit".to_owned());
    }
    let root = verified_root_proof(root_path, root_identity)?;
    let backend = NativeRenameScanBackend;
    let root_guards = backend
        .reopen_chain(&root.chain, &root, DirectoryOpenAccess::Enumerate)
        .map_err(|error| error.to_string())?;
    let mut files = Vec::with_capacity(specs.len());
    let mut unique_identities = std::collections::HashSet::with_capacity(specs.len());
    let mut total_length = 0_u64;
    for spec in specs {
        if cancellation.is_cancelled() {
            return Err("Undo was cancelled before any file was moved".to_owned());
        }
        if spec.original_path.parent().is_none_or(|parent| {
            batch_rename::compare_paths_windows_ordinal(parent, &spec.parent_path)
                != std::cmp::Ordering::Equal
        }) || spec.current_path.parent().is_none_or(|parent| {
            batch_rename::compare_paths_windows_ordinal(parent, &spec.parent_path)
                != std::cmp::Ordering::Equal
        }) {
            return Err("a recovery entry crossed its recorded parent directory".to_owned());
        }
        if spec.expected_length > MAX_RENAME_HASH_BYTES_PER_FILE {
            return Err(format!(
                "recovery source exceeds the {MAX_RENAME_HASH_BYTES_PER_FILE}-byte content verification limit: {}",
                spec.current_path.display()
            ));
        }
        total_length = total_length.saturating_add(spec.expected_length);
        if total_length > MAX_RENAME_HASH_BYTES_PER_BATCH {
            return Err(format!(
                "recovery batch exceeds the {MAX_RENAME_HASH_BYTES_PER_BATCH}-byte content verification limit"
            ));
        }
        let parent_chain = verified_parent_chain(&root, &spec.parent_path, spec.parent_identity)?;
        let parent_guards = backend
            .reopen_chain(&parent_chain, &root, DirectoryOpenAccess::Enumerate)
            .map_err(|error| error.to_string())?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| "a recovery parent directory is missing".to_owned())?;
        let opened = open_native_mutation_file(&spec.current_path).map_err(|error| {
            format!(
                "could not guard recovery source {}: {error}",
                spec.current_path.display()
            )
        })?;
        let content_sha256 = hash_file_contents(
            &opened.handle,
            ContentVerificationMode::Cooperative(cancellation),
        )
        .map_err(|error| {
            format!(
                "could not fingerprint recovery source {}: {error}",
                spec.current_path.display()
            )
        })?;
        if opened.kind != SnapshotEntryKind::File
            || opened.reparse_point
            || opened.identity != spec.identity
            || opened.length != Some(spec.expected_length)
            || opened.modified != Some(spec.expected_modified)
            || content_sha256 != spec.content_sha256
            || batch_rename::compare_paths_windows_ordinal(
                &opened.canonical_path,
                &spec.current_path,
            ) != std::cmp::Ordering::Equal
            || opened.canonical_path.file_name() != spec.current_path.file_name()
            || parent.identity != spec.parent_identity
        {
            return Err(format!(
                "recovery source identity or bytes changed: {}",
                spec.current_path.display()
            ));
        }
        if !unique_identities.insert(opened.identity) {
            return Err(
                "the recovery receipt contains duplicate native file identities; safe Undo would be ambiguous"
                    .to_owned(),
            );
        }
        files.push(GuardedRenameFile {
            source_ref: spec.source_ref,
            original_path: spec.original_path.clone(),
            target_path: spec.original_path.clone(),
            current_path: spec.current_path.clone(),
            parent_path: parent.canonical_path.clone(),
            parent_identity: parent.identity,
            identity: opened.identity,
            expected_length: spec.expected_length,
            expected_modified: spec.expected_modified,
            content_sha256,
            source: opened.handle,
            root: root.clone(),
            parent_chain,
            _root_guards: root_guards.clone(),
            _parent_guards: parent_guards,
        });
    }
    check_guarded_destinations(&backend, &root, &files)?;
    Ok(GuardedRenameBatch { files })
}

#[cfg(not(windows))]
pub(crate) fn prepare_guarded_undo_batch(
    _root_path: &Path,
    _root_identity: NativeFileIdentity,
    _specs: &[UndoGuardSpec],
    _cancellation: &ScanCancellation,
) -> Result<GuardedRenameBatch, String> {
    Err("safe rename-by-handle is available only on Windows".to_owned())
}

#[cfg(windows)]
fn check_guarded_destinations(
    backend: &NativeRenameScanBackend,
    _root: &RootProof,
    files: &[GuardedRenameFile],
) -> Result<(), String> {
    for file in files {
        let Some(target_leaf) = file.target_path.file_name() else {
            return Err("a verified destination has no filename".to_owned());
        };
        let Some(parent) = file._parent_guards.last() else {
            return Err("a verified destination parent is missing".to_owned());
        };
        let names = backend
            .enumerate_directory(
                parent,
                batch_rename::MAX_SNAPSHOT_ENTRIES,
                usize::MAX,
                &ScanCancellation::new(),
            )
            .map_err(|error| error.to_string())?;
        for name in names.iter().filter(|name| {
            batch_rename::compare_paths_windows_ordinal(
                Path::new(&name.name),
                Path::new(target_leaf),
            ) == std::cmp::Ordering::Equal
        }) {
            let occupied_path = parent.canonical_path.join(&name.name);
            let selected_source = files.iter().find(|source| {
                batch_rename::compare_paths_windows_ordinal(&source.current_path, &occupied_path)
                    == std::cmp::Ordering::Equal
            });
            if let Some(source) = selected_source {
                let occupant = open_native_occupancy_file(&occupied_path)
                    .map_err(|error| error.to_string())?;
                if occupant.identity != source.identity
                    || occupant.kind != SnapshotEntryKind::File
                    || occupant.reparse_point
                    || batch_rename::compare_paths_windows_ordinal(
                        &occupant.canonical_path,
                        &source.current_path,
                    ) != std::cmp::Ordering::Equal
                    || occupant.canonical_path.file_name() != source.current_path.file_name()
                {
                    return Err(format!(
                        "destination occupancy no longer matches the exact selected source identity: {}",
                        occupied_path.display()
                    ));
                }
                continue;
            }
            return Err(format!(
                "destination is occupied by an unselected or unrelated entry: {}",
                occupied_path.display()
            ));
        }
    }
    Ok(())
}

/// Scan one explicitly supplied root. Native identity is currently defined
/// only for supported local Windows filesystems; other platforms fail closed.
pub fn scan_rename_root(
    request: RenameScanRequest,
    cancellation: &ScanCancellation,
) -> RenameScanResult {
    #[cfg(windows)]
    {
        scan_with_backend(&NativeRenameScanBackend, request, cancellation)
    }
    #[cfg(not(windows))]
    {
        let mut result = empty_scan_result(&request, RenameScanStatus::Unsupported);
        record_issue(
            &mut result.summary,
            RenameScanIssue {
                path: Some(request.root.as_path().to_path_buf()),
                kind: RenameScanIssueKind::UnsupportedIdentity,
                message:
                    "native file identity and confined enumeration are available only on Windows"
                        .into(),
            },
        );
        result
    }
}

/// Bounded, metadata-only traversal for duplicate discovery. It deliberately
/// shares the validated no-follow backend with rename scans while selecting
/// independent limits and omitting the rename snapshot/proof allocation.
pub(crate) fn scan_duplicate_discovery_root_with_progress(
    root: ExplicitDirectoryRoot,
    expected_root: DuplicateDiscoveryRootIdentity,
    generation: RenameScanGeneration,
    limits: DuplicateDiscoveryScanLimits,
    cancellation: &ScanCancellation,
    progress: &mut dyn FnMut(&RenameScanSummary),
) -> DuplicateDiscoveryScanResult {
    let mut request = RenameScanRequest::new(root, true, generation);
    request.exclude_cloud_placeholders = true;
    request.expected_duplicate_root = Some(expected_root);
    #[cfg(windows)]
    {
        scan_with_backend_bounded_progress(
            &NativeRenameScanBackend,
            request,
            cancellation,
            ScanBounds::duplicate_discovery(limits),
            progress,
        )
        .into_duplicate_discovery()
    }
    #[cfg(not(windows))]
    {
        let _ = (limits, progress);
        let mut result = empty_scan_result(&request, RenameScanStatus::Unsupported);
        record_issue(
            &mut result.summary,
            RenameScanIssue {
                path: Some(request.root.as_path().to_path_buf()),
                kind: RenameScanIssueKind::UnsupportedIdentity,
                message:
                    "duplicate discovery requires handle-based local file identity, available only on Windows"
                        .into(),
            },
        );
        result.into_duplicate_discovery()
    }
}

/// Exact path evidence for one content read. The root is the canonical root
/// admitted during discovery; identity, size, and modification time are the
/// observations captured from the candidate during that scan.
#[derive(Clone)]
pub(crate) struct DuplicateDiscoveryContentRequest {
    pub(crate) root: DuplicateDiscoveryRootIdentity,
    pub(crate) generation: RenameScanGeneration,
    pub(crate) path: PathBuf,
    pub(crate) identity: NativeFileIdentity,
    pub(crate) length: u64,
    pub(crate) modified: SystemTime,
}

/// A short-lived, no-follow DFF read lease. Its sharing mode permits ordinary
/// readers, writers, and deleters; every read is bracketed by identity,
/// metadata, ancestor, and displayed-path revalidation.
pub(crate) struct DuplicateDiscoveryContentFile {
    file: std::fs::File,
    request: DuplicateDiscoveryContentRequest,
    #[cfg(windows)]
    root_proof: RootProof,
    #[cfg(windows)]
    parent_chain: Vec<DirectoryProof>,
}

impl DuplicateDiscoveryContentFile {
    pub(crate) fn revalidate(&self, cancellation: &ScanCancellation) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            revalidate_duplicate_discovery_content_file(self, cancellation)
        }
        #[cfg(not(windows))]
        {
            let _ = cancellation;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "duplicate-content reads require validated Windows file identity",
            ))
        }
    }
}

impl std::io::Read for DuplicateDiscoveryContentFile {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.file, buffer)
    }
}

pub(crate) fn open_duplicate_discovery_content_file(
    request: DuplicateDiscoveryContentRequest,
    cancellation: &ScanCancellation,
) -> std::io::Result<DuplicateDiscoveryContentFile> {
    #[cfg(windows)]
    {
        open_duplicate_discovery_content_file_windows(request, cancellation)
    }
    #[cfg(not(windows))]
    {
        let _ = (request, cancellation);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "duplicate-content reads require validated Windows file identity",
        ))
    }
}

#[cfg(windows)]
fn open_duplicate_discovery_content_file_windows(
    request: DuplicateDiscoveryContentRequest,
    cancellation: &ScanCancellation,
) -> std::io::Result<DuplicateDiscoveryContentFile> {
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "duplicate-content read was cancelled before opening",
        ));
    }
    if request.path == request.root.canonical_root
        || !batch_rename::path_is_within_windows_ordinal(
            &request.root.canonical_root,
            &request.path,
        )
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "candidate path is not a file below its admitted duplicate-scan root",
        ));
    }
    let explicit_root =
        ExplicitDirectoryRoot::new(request.root.canonical_root.clone()).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid duplicate-scan root: {error:?}"),
            )
        })?;
    let mut scan_request = RenameScanRequest::new(explicit_root, true, request.generation);
    scan_request.exclude_cloud_placeholders = true;
    let admission = NativeRenameScanBackend
        .admit_root(&scan_request)
        .map_err(duplicate_content_backend_error)?;
    if admission.proof.identity != request.root.identity
        || batch_rename::compare_paths_windows_ordinal(
            &admission.proof.canonical_path,
            &request.root.canonical_root,
        ) != std::cmp::Ordering::Equal
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "duplicate-scan root identity or canonical path changed",
        ));
    }

    let relative = request
        .path
        .strip_prefix(&request.root.canonical_root)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "candidate path does not use the admitted root prefix",
            )
        })?;
    let components = relative.components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "candidate path contains an invalid root-relative component",
        ));
    }

    let mut directories = admission.directories;
    let mut parent_chain = admission.proof.chain.clone();
    for component in &components[..components.len() - 1] {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "duplicate-content path validation was cancelled",
            ));
        }
        let parent = directories.last().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "admitted duplicate-content root chain is empty",
            )
        })?;
        let child = NativeRenameScanBackend
            .open_child(
                parent,
                &parent_chain,
                &admission.proof,
                component.as_os_str(),
                None,
            )
            .map_err(duplicate_content_backend_error)?;
        if child.kind != SnapshotEntryKind::Directory || child.reparse_point {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "candidate parent is not a verified plain directory",
            ));
        }
        let child_proof = DirectoryProof {
            lookup_path: child.lookup_path.clone(),
            canonical_path: child.canonical_path.clone(),
            identity: child.identity,
        };
        let directory = NativeRenameScanBackend
            .acquire_child_directory_for_enumeration(
                parent,
                &parent_chain,
                &admission.proof,
                &child,
            )
            .map_err(duplicate_content_backend_error)?;
        drop(child);
        parent_chain.push(child_proof);
        directories.push(Arc::new(directory));
    }

    let parent = directories.last().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "candidate file has no verified parent directory",
        )
    })?;
    if request.path.parent().is_none_or(|actual_parent| {
        batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
            != std::cmp::Ordering::Equal
    }) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "candidate file no longer has its scanned parent directory",
        ));
    }

    let opened = open_native_duplicate_content_leaf(&request.path)?;
    validate_duplicate_content_opened(&opened, &request, &admission.proof, &parent.canonical_path)?;
    NativeRenameScanBackend
        .reopen_chain(
            &parent_chain,
            &admission.proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(duplicate_content_backend_error)?;
    drop(directories);

    Ok(DuplicateDiscoveryContentFile {
        file: opened.handle,
        request,
        root_proof: admission.proof,
        parent_chain,
    })
}

#[cfg(windows)]
fn revalidate_duplicate_discovery_content_file(
    content: &DuplicateDiscoveryContentFile,
    cancellation: &ScanCancellation,
) -> std::io::Result<()> {
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "duplicate-content read was cancelled during revalidation",
        ));
    }
    validate_duplicate_content_handle(&content.file, &content.request, &content.root_proof)?;
    let parent_directories = NativeRenameScanBackend
        .reopen_chain(
            &content.parent_chain,
            &content.root_proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(duplicate_content_backend_error)?;
    let parent = parent_directories.last().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "duplicate-content parent chain could not be reopened",
        )
    })?;
    let opened = open_native_duplicate_content_leaf(&content.request.path)?;
    validate_duplicate_content_opened(
        &opened,
        &content.request,
        &content.root_proof,
        &parent.canonical_path,
    )?;
    NativeRenameScanBackend
        .reopen_chain(
            &content.parent_chain,
            &content.root_proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(duplicate_content_backend_error)?;
    drop(opened);
    drop(parent_directories);
    validate_duplicate_content_handle(&content.file, &content.request, &content.root_proof)
}

#[cfg(windows)]
fn validate_duplicate_content_opened(
    opened: &OpenedObject<std::fs::File>,
    request: &DuplicateDiscoveryContentRequest,
    root: &RootProof,
    parent_path: &Path,
) -> std::io::Result<()> {
    if opened.kind != SnapshotEntryKind::File
        || opened.reparse_point
        || opened.cloud_placeholder
        || opened.identity != request.identity
        || opened.length != Some(request.length)
        || opened.modified != Some(request.modified)
        || batch_rename::compare_paths_windows_ordinal(&opened.canonical_path, &request.path)
            != std::cmp::Ordering::Equal
        || !batch_rename::path_is_within_windows_ordinal(
            &root.canonical_path,
            &opened.canonical_path,
        )
        || opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(actual_parent, parent_path)
                != std::cmp::Ordering::Equal
        })
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "duplicate candidate identity, type, path, size, or modification time changed",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_duplicate_content_handle(
    file: &std::fs::File,
    request: &DuplicateDiscoveryContentRequest,
    root: &RootProof,
) -> std::io::Result<()> {
    use std::os::windows::fs::MetadataExt;

    let metadata = file.metadata()?;
    let attributes = metadata.file_attributes();
    if !metadata.is_file()
        || attributes
            & (windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                | FILE_ATTRIBUTE_OFFLINE_BITS
                | FILE_ATTRIBUTE_RECALL_ON_OPEN_BITS
                | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS_BITS)
            != 0
        || metadata.len() != request.length
        || metadata.modified().ok() != Some(request.modified)
        || native_file_identity(file)? != request.identity
        || batch_rename::compare_paths_windows_ordinal(&final_path_for_handle(file)?, &request.path)
            != std::cmp::Ordering::Equal
        || !batch_rename::path_is_within_windows_ordinal(&root.canonical_path, &request.path)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "duplicate candidate changed during its content read",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn duplicate_content_backend_error(error: BackendFailure) -> std::io::Error {
    let kind = match error.kind {
        BackendFailureKind::Cancelled => std::io::ErrorKind::Interrupted,
        BackendFailureKind::UnsupportedIdentity => std::io::ErrorKind::Unsupported,
        BackendFailureKind::ResourceLimit => std::io::ErrorKind::OutOfMemory,
        BackendFailureKind::InvalidRoot | BackendFailureKind::PathEscapedRoot => {
            std::io::ErrorKind::InvalidData
        }
        _ => std::io::ErrorKind::Other,
    };
    std::io::Error::new(kind, error.message)
}

/// Admit one DFF root without enumerating it. Controllers use this short-lived
/// metadata-only proof to deduplicate different path spellings of the same
/// Windows directory before starting a traversal.
pub(crate) fn inspect_duplicate_discovery_root(
    root: ExplicitDirectoryRoot,
    generation: RenameScanGeneration,
) -> Result<DuplicateDiscoveryRootIdentity, (RenameScanStatus, RenameScanSummary)> {
    let mut request = RenameScanRequest::new(root, true, generation);
    request.metadata_only_root = true;
    request.exclude_cloud_placeholders = true;
    #[cfg(windows)]
    {
        match NativeRenameScanBackend.admit_root(&request) {
            Ok(admission) => Ok(DuplicateDiscoveryRootIdentity {
                canonical_root: admission.proof.canonical_path,
                identity: admission.proof.identity,
            }),
            Err(error) => {
                let status = status_for_root_failure(error.kind);
                let mut summary = RenameScanSummary::default();
                record_failure(&mut summary, error);
                Err((status, summary))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        let mut summary = RenameScanSummary::default();
        record_issue(
            &mut summary,
            RenameScanIssue {
                path: None,
                kind: RenameScanIssueKind::UnsupportedIdentity,
                message:
                    "duplicate discovery requires handle-based local file identity, available only on Windows"
                        .into(),
            },
        );
        Err((RenameScanStatus::Unsupported, summary))
    }
}

/// Capture bytes for one row of a complete scan while rechecking its retained
/// root and parent evidence. This is a read-only snapshot boundary, not a
/// mutation authorization token.
pub(crate) fn read_verified_scan_file(
    scan: &RenameScanResult,
    row: &RenameScanRow,
    max_bytes: u64,
    cancellation: &ScanCancellation,
) -> Result<VerifiedReadSnapshot, VerifiedReadFailure> {
    if scan.status != RenameScanStatus::Complete {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::IncompleteScan,
            "file bytes require a complete confined scan".to_owned(),
        ));
    }
    if cancellation.is_cancelled() {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Cancelled,
            "file read was cancelled".to_owned(),
        ));
    }
    let Some(proof) = scan.proof.as_deref() else {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the complete scan did not retain native evidence".to_owned(),
        ));
    };
    let Some(entry) = proof.entries.get(&row.entry_ref) else {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the file row does not belong to this scan".to_owned(),
        ));
    };
    if row.kind != SnapshotEntryKind::File
        || row.reparse_point
        || entry.path != row.path
        || entry.identity != row.identity
        || entry.length != row.length
        || entry.modified != row.modified
        || row.length.is_none()
        || row.modified.is_none()
    {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the file row does not match its retained scan evidence".to_owned(),
        ));
    }
    let expected_length = entry.length.unwrap_or_default();
    if expected_length > max_bytes {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::TooLarge,
            format!("file length {expected_length} exceeds the {max_bytes}-byte limit"),
        ));
    }

    #[cfg(windows)]
    {
        read_verified_scan_file_native(scan, row, proof, entry, max_bytes, cancellation)
    }
    #[cfg(not(windows))]
    {
        let _ = (scan, row, proof, entry, max_bytes);
        Err(verified_read_failure(
            VerifiedReadFailureKind::Unsupported,
            "native confined content reads are available only on Windows".to_owned(),
        ))
    }
}

/// Reopen the root, every retained parent directory, and the exact source
/// handle for one selected replacement candidate. The returned handle is the
/// only authority to relocate the source and create a separate output stage.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_guarded_replace_file(
    evidence: &VerifiedReadEvidence,
    expected_root: &Path,
    expected_root_identity: NativeFileIdentity,
    expected_path: &Path,
    expected_identity: NativeFileIdentity,
    expected_parent_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    expected_sha256: [u8; 32],
    expected_bytes: &[u8],
    cancellation: &ScanCancellation,
) -> Result<GuardedReplaceFile, VerifiedReplaceFailure> {
    let invalid = |message: &str| VerifiedReplaceFailure {
        kind: VerifiedReplaceFailureKind::InvalidEvidence,
        message: message.to_owned(),
    };
    if cancellation.is_cancelled() {
        return Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::Cancelled,
            message: "replacement was cancelled before source admission".to_owned(),
        });
    }
    if expected_bytes.len() as u64 != expected_length
        || <[u8; 32]>::from(Sha256::digest(expected_bytes)) != expected_sha256
    {
        return Err(invalid(
            "the approved source bytes do not match their captured length and hash",
        ));
    }
    let root = &evidence.proof.root;
    let Some(entry) = evidence.proof.entries.get(&evidence.entry_ref) else {
        return Err(invalid(
            "the replacement source proof has no matching scan entry",
        ));
    };
    let recorded_parent = entry.parent_chain.last();
    if root.identity != expected_root_identity
        || batch_rename::compare_paths_windows_ordinal(&root.canonical_path, expected_root)
            != std::cmp::Ordering::Equal
        || batch_rename::compare_paths_windows_ordinal(&entry.path, expected_path)
            != std::cmp::Ordering::Equal
        || entry.identity != expected_identity
        || entry.length != Some(expected_length)
        || entry.modified != Some(expected_modified)
        || recorded_parent.is_none_or(|parent| {
            parent.identity != expected_parent_identity
                || expected_path.parent().is_none_or(|expected_parent| {
                    batch_rename::compare_paths_windows_ordinal(
                        &parent.canonical_path,
                        expected_parent,
                    ) != std::cmp::Ordering::Equal
                })
        })
    {
        return Err(invalid(
            "the approved source metadata does not match its retained native scan evidence",
        ));
    }

    #[cfg(windows)]
    {
        prepare_guarded_replace_file_native(
            evidence,
            expected_path,
            expected_identity,
            expected_parent_identity,
            expected_length,
            expected_modified,
            expected_sha256,
            expected_bytes,
            cancellation,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (
            expected_path,
            expected_identity,
            expected_parent_identity,
            expected_length,
            expected_modified,
            expected_sha256,
            expected_bytes,
        );
        Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::Unsupported,
            message: "safe replacement writes are available only on Windows".to_owned(),
        })
    }
}

#[cfg(windows)]
fn prepare_guarded_replace_file_native(
    evidence: &VerifiedReadEvidence,
    expected_path: &Path,
    expected_identity: NativeFileIdentity,
    expected_parent_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    expected_sha256: [u8; 32],
    expected_bytes: &[u8],
    cancellation: &ScanCancellation,
) -> Result<GuardedReplaceFile, VerifiedReplaceFailure> {
    let root = &evidence.proof.root;
    let entry = evidence
        .proof
        .entries
        .get(&evidence.entry_ref)
        .ok_or_else(|| VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::InvalidEvidence,
            message: "the replacement source proof has no matching scan entry".to_owned(),
        })?;
    let backend = NativeRenameScanBackend;
    let root_guards = backend
        .reopen_chain(&root.chain, root, DirectoryOpenAccess::Enumerate)
        .map_err(|error| replace_backend_failure(error, true))?;
    if root_guards
        .last()
        .is_none_or(|directory| directory.identity != root.identity)
    {
        return Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::RootChanged,
            message: "the selected replacement root changed since preview".to_owned(),
        });
    }
    let parent_guards = backend
        .reopen_chain(&entry.parent_chain, root, DirectoryOpenAccess::Enumerate)
        .map_err(|error| replace_backend_failure(error, false))?;
    let parent = parent_guards.last().ok_or_else(|| VerifiedReplaceFailure {
        kind: VerifiedReplaceFailureKind::ParentChanged,
        message: "the replacement source parent directory is missing".to_owned(),
    })?;
    if parent.identity != expected_parent_identity
        || expected_path.parent().is_none_or(|expected_parent| {
            batch_rename::compare_paths_windows_ordinal(&parent.canonical_path, expected_parent)
                != std::cmp::Ordering::Equal
        })
    {
        return Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::ParentChanged,
            message: "the replacement source parent directory changed since preview".to_owned(),
        });
    }
    if cancellation.is_cancelled() {
        return Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::Cancelled,
            message: "replacement was cancelled before opening its source".to_owned(),
        });
    }
    let opened = open_native_replace_file(expected_path).map_err(|error| {
        replace_io_failure(
            "could not open the selected source for guarded replacement",
            error,
        )
    })?;
    if opened.kind != SnapshotEntryKind::File
        || opened.reparse_point
        || opened.identity != expected_identity
        || opened.length != Some(expected_length)
        || opened.modified != Some(expected_modified)
        || batch_rename::compare_paths_windows_ordinal(&opened.canonical_path, expected_path)
            != std::cmp::Ordering::Equal
        || opened.canonical_path.file_name() != expected_path.file_name()
        || opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
                != std::cmp::Ordering::Equal
        })
        || !batch_rename::path_is_within_windows_ordinal(
            &root.canonical_path,
            &opened.canonical_path,
        )
    {
        return Err(VerifiedReplaceFailure {
            kind: VerifiedReplaceFailureKind::SourceChanged,
            message: "the selected source identity, path, type, or metadata changed since preview"
                .to_owned(),
        });
    }
    let native = GuardedReplaceHandle {
        current_path: expected_path.to_path_buf(),
        parent_path: parent.canonical_path.clone(),
        parent_identity: parent.identity,
        identity: opened.identity,
        root: root.clone(),
        parent_chain: entry.parent_chain.clone(),
        file: opened.handle,
        root_guards,
        parent_guards,
    };
    ensure_unnamed_stream_only(&native.file).map_err(|error| VerifiedReplaceFailure {
        kind: VerifiedReplaceFailureKind::Unsupported,
        message: format!(
            "the selected source has named streams or its stream set cannot be verified; replacement refused to preserve all original data ({error})"
        ),
    })?;
    let original_state = native
        .verify_at(
            expected_path,
            expected_length,
            Some(expected_modified),
            expected_sha256,
            Some(expected_bytes),
            cancellation,
        )
        .map_err(|error| {
            replace_io_failure(
                "the selected source changed during guarded verification",
                error,
            )
        })?;
    Ok(GuardedReplaceFile {
        original_path: expected_path.to_path_buf(),
        original_state,
        native,
    })
}

#[cfg(windows)]
fn ensure_unnamed_stream_only(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
        WIN32_FIND_STREAM_DATA,
    };
    use windows::core::PCWSTR;

    let exact_path = final_path_for_handle(file)?;
    let mut wide_path = extended_local_path(&exact_path)?;
    wide_path.push(0);
    let mut stream = WIN32_FIND_STREAM_DATA::default();
    let find = unsafe {
        FindFirstStreamW(
            PCWSTR(wide_path.as_ptr()),
            FindStreamInfoStandard,
            &mut stream as *mut WIN32_FIND_STREAM_DATA as *mut std::ffi::c_void,
            0,
        )
    }
    .map_err(windows_error_to_io)?;

    let result = (|| {
        let name_end = stream
            .cStreamName
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(stream.cStreamName.len());
        let first_name = String::from_utf16(&stream.cStreamName[..name_end]).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native stream enumeration returned an invalid UTF-16 name",
            )
        })?;
        if !first_name.eq_ignore_ascii_case("::$DATA") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the source's first stream is not its ordinary unnamed data stream",
            ));
        }
        match unsafe {
            FindNextStreamW(
                find,
                &mut stream as *mut WIN32_FIND_STREAM_DATA as *mut std::ffi::c_void,
            )
        } {
            Ok(()) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the source contains an additional named data stream",
            )),
            Err(error) if error.code().0 as u32 == 0x8007_0026 => Ok(()),
            Err(error) => Err(windows_error_to_io(error)),
        }
    })();
    let close = unsafe { FindClose(find) }.map_err(windows_error_to_io);
    match (result, close) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

#[cfg(windows)]
fn replace_backend_failure(error: BackendFailure, root_check: bool) -> VerifiedReplaceFailure {
    let kind = match error.kind {
        BackendFailureKind::RootChanged => VerifiedReplaceFailureKind::RootChanged,
        BackendFailureKind::Cancelled => VerifiedReplaceFailureKind::Cancelled,
        BackendFailureKind::UnsupportedIdentity => VerifiedReplaceFailureKind::Unsupported,
        BackendFailureKind::Inaccessible => VerifiedReplaceFailureKind::Inaccessible,
        BackendFailureKind::ParentChanged => VerifiedReplaceFailureKind::ParentChanged,
        _ if root_check => VerifiedReplaceFailureKind::RootChanged,
        _ => VerifiedReplaceFailureKind::ParentChanged,
    };
    VerifiedReplaceFailure {
        kind,
        message: error.message,
    }
}

#[cfg(windows)]
fn replace_io_failure(operation: &'static str, error: std::io::Error) -> VerifiedReplaceFailure {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidData => {
            VerifiedReplaceFailureKind::SourceChanged
        }
        std::io::ErrorKind::PermissionDenied => VerifiedReplaceFailureKind::Inaccessible,
        std::io::ErrorKind::Interrupted => VerifiedReplaceFailureKind::Cancelled,
        std::io::ErrorKind::Unsupported => VerifiedReplaceFailureKind::Unsupported,
        _ => VerifiedReplaceFailureKind::Inaccessible,
    };
    VerifiedReplaceFailure {
        kind,
        message: format!("{operation}: {error}"),
    }
}

fn verified_read_failure(kind: VerifiedReadFailureKind, message: String) -> VerifiedReadFailure {
    VerifiedReadFailure { kind, message }
}

#[cfg(windows)]
fn read_verified_scan_file_native(
    scan: &RenameScanResult,
    row: &RenameScanRow,
    proof: &ScanProof,
    entry: &EntryProof,
    max_bytes: u64,
    cancellation: &ScanCancellation,
) -> Result<VerifiedReadSnapshot, VerifiedReadFailure> {
    use std::io::Read;

    let parent_directories = NativeRenameScanBackend
        .reopen_chain(
            &entry.parent_chain,
            &proof.root,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(verified_read_backend_failure)?;
    let Some(parent) = parent_directories.last() else {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the scanned file has no retained parent directory".to_owned(),
        ));
    };
    if parent.identity != row.parent_identity
        || entry.path.parent().is_none_or(|path| {
            batch_rename::compare_paths_windows_ordinal(path, &parent.canonical_path)
                != std::cmp::Ordering::Equal
        })
    {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Changed,
            "the scanned file's parent directory changed".to_owned(),
        ));
    }

    // Retain the reopened ancestor handles while reading. The content handle
    // grants read access only and does not follow a final reparse point.
    let opened = open_native_readonly_content_file(&entry.path)
        .map_err(|error| verified_read_io_failure("open scanned file for read", error))?;
    if opened.kind != SnapshotEntryKind::File
        || opened.reparse_point
        || opened.identity != entry.identity
        || opened.length != entry.length
        || opened.modified != entry.modified
        || batch_rename::compare_paths_windows_ordinal(&opened.canonical_path, &entry.path)
            != std::cmp::Ordering::Equal
        || !batch_rename::path_is_within_windows_ordinal(
            &proof.root.canonical_path,
            &opened.canonical_path,
        )
        || opened.canonical_path.parent().is_none_or(|path| {
            batch_rename::compare_paths_windows_ordinal(path, &parent.canonical_path)
                != std::cmp::Ordering::Equal
        })
    {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Changed,
            "the scanned file identity, path, type, or metadata changed before reading".to_owned(),
        ));
    }
    if cancellation.is_cancelled() {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Cancelled,
            "file read was cancelled".to_owned(),
        ));
    }

    let expected_length = opened.length.ok_or_else(|| {
        verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the scanned file has no captured length".to_owned(),
        )
    })?;
    let capacity = usize::try_from(expected_length).map_err(|_| {
        verified_read_failure(
            VerifiedReadFailureKind::TooLarge,
            "file length cannot be represented in memory".to_owned(),
        )
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|error| {
        verified_read_failure(
            VerifiedReadFailureKind::TooLarge,
            format!("could not reserve bounded file snapshot memory: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut content_reader = opened
        .handle
        .try_clone()
        .map_err(|error| verified_read_io_failure("clone scanned file read handle", error))?;
    loop {
        if cancellation.is_cancelled() {
            return Err(verified_read_failure(
                VerifiedReadFailureKind::Cancelled,
                "file read was cancelled".to_owned(),
            ));
        }
        let read = match content_reader.read(&mut buffer) {
            Ok(read) => read,
            Err(error) => {
                if cancellation.is_cancelled() {
                    return Err(verified_read_failure(
                        VerifiedReadFailureKind::Cancelled,
                        "file read was cancelled".to_owned(),
                    ));
                }
                return Err(verified_read_io_failure("read scanned file", error));
            }
        };
        if read == 0 {
            break;
        }
        let next_length = (bytes.len() as u64)
            .checked_add(read as u64)
            .ok_or_else(|| {
                verified_read_failure(
                    VerifiedReadFailureKind::TooLarge,
                    "file byte count overflowed".to_owned(),
                )
            })?;
        if next_length > max_bytes {
            return Err(verified_read_failure(
                VerifiedReadFailureKind::TooLarge,
                format!("file grew beyond the {max_bytes}-byte limit while reading"),
            ));
        }
        if next_length > expected_length {
            return Err(verified_read_failure(
                VerifiedReadFailureKind::Changed,
                "file length grew while reading".to_owned(),
            ));
        }
        hasher.update(&buffer[..read]);
        bytes.extend_from_slice(&buffer[..read]);
    }

    let metadata_after = opened
        .handle
        .metadata()
        .map_err(|error| verified_read_io_failure("recheck scanned file metadata", error))?;
    let identity_after = native_file_identity(&opened.handle)
        .map_err(|error| verified_read_io_failure("recheck scanned file identity", error))?;
    let path_after = final_path_for_handle(&opened.handle)
        .map_err(|error| verified_read_io_failure("recheck scanned file path", error))?;
    if bytes.len() as u64 != expected_length
        || metadata_after.len() != expected_length
        || metadata_after.modified().ok() != opened.modified
        || identity_after != entry.identity
        || batch_rename::compare_paths_windows_ordinal(&path_after, &entry.path)
            != std::cmp::Ordering::Equal
    {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Changed,
            "file identity, path, length, or modification time changed while reading".to_owned(),
        ));
    }

    let parent_after = NativeRenameScanBackend
        .reopen_chain(
            &entry.parent_chain,
            &proof.root,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(verified_read_backend_failure)?;
    let Some(parent_after) = parent_after.last() else {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Changed,
            "the scanned file's parent disappeared after reading".to_owned(),
        ));
    };
    if parent_after.identity != row.parent_identity
        || entry.path.parent().is_none_or(|path| {
            batch_rename::compare_paths_windows_ordinal(path, &parent_after.canonical_path)
                != std::cmp::Ordering::Equal
        })
    {
        return Err(verified_read_failure(
            VerifiedReadFailureKind::Changed,
            "the scanned file's parent directory changed while reading".to_owned(),
        ));
    }

    let modified = opened.modified.ok_or_else(|| {
        verified_read_failure(
            VerifiedReadFailureKind::InvalidEvidence,
            "the scanned file has no captured modification time".to_owned(),
        )
    })?;
    let content_sha256 = hasher.finalize().into();
    Ok(VerifiedReadSnapshot {
        canonical_root: scan
            .canonical_root
            .clone()
            .unwrap_or_else(|| proof.root.canonical_path.clone()),
        root_identity: proof.root.identity,
        path: entry.path.clone(),
        identity: entry.identity,
        parent_identity: row.parent_identity,
        length: expected_length,
        modified,
        bytes: Arc::from(bytes.into_boxed_slice()),
        content_sha256,
        evidence: Arc::new(VerifiedReadEvidence {
            proof: Arc::clone(scan.proof.as_ref().ok_or_else(|| {
                verified_read_failure(
                    VerifiedReadFailureKind::InvalidEvidence,
                    "the complete scan did not retain native evidence".to_owned(),
                )
            })?),
            entry_ref: row.entry_ref,
        }),
    })
}

#[cfg(windows)]
fn verified_read_backend_failure(error: BackendFailure) -> VerifiedReadFailure {
    let kind = match error.kind {
        BackendFailureKind::Inaccessible => VerifiedReadFailureKind::Inaccessible,
        BackendFailureKind::UnsupportedIdentity => VerifiedReadFailureKind::Unsupported,
        BackendFailureKind::Cancelled => VerifiedReadFailureKind::Cancelled,
        BackendFailureKind::ResourceLimit => VerifiedReadFailureKind::TooLarge,
        _ => VerifiedReadFailureKind::Changed,
    };
    verified_read_failure(kind, error.message)
}

#[cfg(windows)]
fn verified_read_io_failure(operation: &'static str, error: std::io::Error) -> VerifiedReadFailure {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => VerifiedReadFailureKind::Changed,
        std::io::ErrorKind::PermissionDenied => VerifiedReadFailureKind::Inaccessible,
        std::io::ErrorKind::Interrupted => VerifiedReadFailureKind::Inaccessible,
        std::io::ErrorKind::InvalidData => VerifiedReadFailureKind::Changed,
        _ => VerifiedReadFailureKind::Inaccessible,
    };
    verified_read_failure(kind, format!("{operation}: {error}"))
}

#[cfg(windows)]
struct NativeRenameScanBackend;

#[cfg(windows)]
impl RenameScanBackend for NativeRenameScanBackend {
    type Handle = std::fs::File;

    fn admit_root(
        &self,
        request: &RenameScanRequest,
    ) -> Result<RootAdmission<Self::Handle>, BackendFailure> {
        validate_local_root_spelling(request.root.as_path())?;
        let component_count = request
            .root
            .as_path()
            .components()
            .filter(|component| !matches!(component, Component::CurDir | Component::ParentDir))
            .count();
        if component_count > MAX_RENAME_SCAN_DEPTH {
            return Err(BackendFailure::new(
                BackendFailureKind::ResourceLimit,
                Some(request.root.as_path()),
                format!("selected root depth exceeds {MAX_RENAME_SCAN_DEPTH} components"),
            ));
        }
        let mut current = PathBuf::new();
        let mut chain = Vec::new();
        let mut prior_canonical: Option<PathBuf> = None;
        let mut volume_serial = None;
        for component in request.root.as_path().components() {
            match component {
                Component::Prefix(prefix) => current.push(prefix.as_os_str()),
                Component::RootDir => {
                    current.push(component.as_os_str());
                    let opened =
                        open_native_object(&current, false, request.exclude_cloud_placeholders)
                            .map_err(|error| classify_root_io(&current, error))?;
                    if request.exclude_cloud_placeholders && opened.cloud_placeholder {
                        return Err(BackendFailure::new(
                            BackendFailureKind::UnsupportedIdentity,
                            Some(&current),
                            "cloud-placeholder roots and ancestors are not hydrated or traversed",
                        ));
                    }
                    if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root or one of its ancestors is not a plain directory",
                        ));
                    }
                    if request.exclude_cloud_placeholders && !identity_is_supported(opened.identity)
                    {
                        return Err(BackendFailure::new(
                            BackendFailureKind::UnsupportedIdentity,
                            Some(&current),
                            "the selected root or an ancestor has no stable Windows file ID",
                        ));
                    }
                    if let Some(expected_parent) = prior_canonical.as_deref()
                        && opened.canonical_path.parent().is_none_or(|parent| {
                            batch_rename::compare_paths_windows_ordinal(parent, expected_parent)
                                != std::cmp::Ordering::Equal
                        })
                    {
                        return Err(BackendFailure::new(
                            BackendFailureKind::RootChanged,
                            Some(&current),
                            "an ancestor resolved through a different directory chain",
                        ));
                    }
                    volume_serial.get_or_insert(opened.identity.volume_serial_number);
                    if volume_serial != Some(opened.identity.volume_serial_number) {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root crosses a volume boundary through a mount point",
                        ));
                    }
                    prior_canonical = Some(opened.canonical_path.clone());
                    chain.push(DirectoryProof {
                        lookup_path: current.clone(),
                        canonical_path: opened.canonical_path.clone(),
                        identity: opened.identity,
                    });
                }
                Component::Normal(name) => {
                    current.push(name);
                    let opened =
                        open_native_object(&current, false, request.exclude_cloud_placeholders)
                            .map_err(|error| classify_root_io(&current, error))?;
                    if request.exclude_cloud_placeholders && opened.cloud_placeholder {
                        return Err(BackendFailure::new(
                            BackendFailureKind::UnsupportedIdentity,
                            Some(&current),
                            "cloud-placeholder roots and ancestors are not hydrated or traversed",
                        ));
                    }
                    if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root or one of its ancestors is not a plain directory",
                        ));
                    }
                    if request.exclude_cloud_placeholders && !identity_is_supported(opened.identity)
                    {
                        return Err(BackendFailure::new(
                            BackendFailureKind::UnsupportedIdentity,
                            Some(&current),
                            "the selected root or an ancestor has no stable Windows file ID",
                        ));
                    }
                    if let Some(expected_parent) = prior_canonical.as_deref()
                        && opened.canonical_path.parent().is_none_or(|parent| {
                            batch_rename::compare_paths_windows_ordinal(parent, expected_parent)
                                != std::cmp::Ordering::Equal
                        })
                    {
                        return Err(BackendFailure::new(
                            BackendFailureKind::RootChanged,
                            Some(&current),
                            "an ancestor resolved through a different directory chain",
                        ));
                    }
                    if volume_serial != Some(opened.identity.volume_serial_number) {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root crosses a volume boundary through a mount point",
                        ));
                    }
                    prior_canonical = Some(opened.canonical_path.clone());
                    chain.push(DirectoryProof {
                        lookup_path: current.clone(),
                        canonical_path: opened.canonical_path.clone(),
                        identity: opened.identity,
                    });
                }
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(BackendFailure::new(
                        BackendFailureKind::InvalidRoot,
                        Some(request.root.as_path()),
                        "parent traversal is not allowed in an explicit scan root",
                    ));
                }
            }
        }
        let Some(root_proof) = chain.last().cloned() else {
            return Err(BackendFailure::new(
                BackendFailureKind::InvalidRoot,
                Some(request.root.as_path()),
                "the selected root has no local volume directory",
            ));
        };
        let selected_root = request.root.as_path();
        let root = RootProof {
            selected_path: selected_root.to_path_buf(),
            canonical_path: root_proof.canonical_path.clone(),
            identity: root_proof.identity,
            recursive: request.scope == RenameScanScope::Recursive,
            exclude_cloud_placeholders: request.exclude_cloud_placeholders,
            generation: request.generation,
            chain,
        };
        let directories = self.reopen_chain(
            &root.chain,
            &root,
            if request.metadata_only_root {
                DirectoryOpenAccess::Metadata
            } else {
                DirectoryOpenAccess::Enumerate
            },
        )?;
        let Some(directory) = directories.last() else {
            return Err(BackendFailure::new(
                BackendFailureKind::RootChanged,
                Some(&root.canonical_path),
                "the selected root chain could not be retained",
            ));
        };
        if directory.identity != root.identity
            || batch_rename::compare_paths_windows_ordinal(
                &directory.canonical_path,
                &root.canonical_path,
            ) != std::cmp::Ordering::Equal
        {
            return Err(BackendFailure::new(
                BackendFailureKind::RootChanged,
                Some(&root.canonical_path),
                "the selected root changed while it was being admitted",
            ));
        }
        Ok(RootAdmission {
            proof: root,
            directories,
        })
    }

    fn enumerate_directory(
        &self,
        directory: &OpenDirectory<Self::Handle>,
        remaining_entries: usize,
        remaining_path_bytes: usize,
        cancellation: &ScanCancellation,
    ) -> Result<Vec<EnumeratedName>, BackendFailure> {
        enumerate_native_directory(
            directory,
            remaining_entries,
            remaining_path_bytes,
            cancellation,
        )
    }

    fn open_child(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        parent_chain: &[DirectoryProof],
        root: &RootProof,
        name: &OsStr,
        expected: Option<&EnumeratedName>,
    ) -> Result<OpenedObject<Self::Handle>, BackendFailure> {
        let verified_parent_chain =
            self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
        let Some(verified_parent) = verified_parent_chain.last() else {
            return Err(BackendFailure::new(
                BackendFailureKind::ParentChanged,
                Some(&parent.canonical_path),
                "the verified parent chain is empty",
            ));
        };
        if verified_parent.identity != parent.identity
            || batch_rename::compare_paths_windows_ordinal(
                &verified_parent.canonical_path,
                &parent.canonical_path,
            ) != std::cmp::Ordering::Equal
        {
            return Err(BackendFailure::new(
                BackendFailureKind::ParentChanged,
                Some(&parent.canonical_path),
                "the enumerated parent directory was replaced",
            ));
        }
        let lookup_path = parent.canonical_path.join(name);
        let opened = open_native_object(&lookup_path, false, root.exclude_cloud_placeholders)
            .map_err(|error| classify_child_io(&lookup_path, error))?;
        if let Some(expected) = expected
            && (expected.is_reparse_point != opened.reparse_point
                || (!expected.is_reparse_point
                    && expected.is_directory != (opened.kind == SnapshotEntryKind::Directory)))
        {
            return Err(BackendFailure::new(
                BackendFailureKind::ObjectChanged,
                Some(&lookup_path),
                "the sibling changed type or reparse status after directory enumeration",
            ));
        }
        if !batch_rename::path_is_within_windows_ordinal(
            &root.canonical_path,
            &opened.canonical_path,
        ) || opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
                != std::cmp::Ordering::Equal
        }) {
            return Err(BackendFailure::new(
                BackendFailureKind::PathEscapedRoot,
                Some(&opened.canonical_path),
                "the opened sibling did not resolve under its verified parent and root",
            ));
        }
        self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
        Ok(opened)
    }

    fn acquire_child_directory_for_enumeration(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        parent_chain: &[DirectoryProof],
        root: &RootProof,
        child: &OpenedObject<Self::Handle>,
    ) -> Result<OpenDirectory<Self::Handle>, BackendFailure> {
        let verified_parent_chain =
            self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
        let Some(verified_parent) = verified_parent_chain.last() else {
            return Err(BackendFailure::new(
                BackendFailureKind::ParentChanged,
                Some(&parent.canonical_path),
                "the verified parent chain is empty before directory enumeration",
            ));
        };
        if verified_parent.identity != parent.identity
            || batch_rename::compare_paths_windows_ordinal(
                &verified_parent.canonical_path,
                &parent.canonical_path,
            ) != std::cmp::Ordering::Equal
        {
            return Err(BackendFailure::new(
                BackendFailureKind::ParentChanged,
                Some(&parent.canonical_path),
                "the parent changed before child directory enumeration",
            ));
        }
        if child.kind != SnapshotEntryKind::Directory || child.reparse_point {
            return Err(BackendFailure::new(
                BackendFailureKind::ObjectChanged,
                Some(&child.canonical_path),
                "only a verified plain directory can be queued for enumeration",
            ));
        }
        let listed = open_native_object(&child.lookup_path, true, root.exclude_cloud_placeholders)
            .map_err(|error| classify_child_io(&child.lookup_path, error))?;
        if listed.kind != SnapshotEntryKind::Directory
            || listed.reparse_point
            || listed.identity != child.identity
            || batch_rename::compare_paths_windows_ordinal(
                &listed.canonical_path,
                &child.canonical_path,
            ) != std::cmp::Ordering::Equal
            || listed.canonical_path.parent().is_none_or(|actual_parent| {
                batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
                    != std::cmp::Ordering::Equal
            })
            || !batch_rename::path_is_within_windows_ordinal(
                &root.canonical_path,
                &listed.canonical_path,
            )
        {
            return Err(BackendFailure::new(
                BackendFailureKind::ObjectChanged,
                Some(&child.lookup_path),
                "the child directory changed while acquiring its enumeration handle",
            ));
        }
        self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
        Ok(listed.into_directory())
    }

    fn reopen_chain(
        &self,
        chain: &[DirectoryProof],
        root: &RootProof,
        final_access: DirectoryOpenAccess,
    ) -> Result<OpenDirectoryChain<Self::Handle>, BackendFailure> {
        if chain.is_empty() {
            return Err(BackendFailure::new(
                BackendFailureKind::RootChanged,
                Some(&root.selected_path),
                "the verified root chain is empty",
            ));
        }
        let mut previous_canonical: Option<PathBuf> = None;
        let mut directories = Vec::with_capacity(chain.len());
        for (index, expected) in chain.iter().enumerate() {
            let is_final = index + 1 == chain.len();
            let opened = open_native_object(
                &expected.lookup_path,
                is_final && final_access == DirectoryOpenAccess::Enumerate,
                root.exclude_cloud_placeholders,
            )
            .map_err(|error| classify_chain_io(&expected.lookup_path, error, root))?;
            if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
                return Err(BackendFailure::new(
                    if is_within_verified_root(root, &expected.canonical_path) {
                        BackendFailureKind::ParentChanged
                    } else {
                        BackendFailureKind::RootChanged
                    },
                    Some(&expected.lookup_path),
                    "a verified directory was replaced or became a reparse point",
                ));
            }
            if opened.identity != expected.identity
                || batch_rename::compare_paths_windows_ordinal(
                    &opened.canonical_path,
                    &expected.canonical_path,
                ) != std::cmp::Ordering::Equal
            {
                return Err(BackendFailure::new(
                    if root
                        .chain
                        .iter()
                        .any(|root_component| root_component == expected)
                    {
                        BackendFailureKind::RootChanged
                    } else {
                        BackendFailureKind::ParentChanged
                    },
                    Some(&expected.lookup_path),
                    "a verified directory identity changed",
                ));
            }
            if let Some(previous) = previous_canonical.as_deref()
                && opened.canonical_path.parent().is_none_or(|parent| {
                    batch_rename::compare_paths_windows_ordinal(parent, previous)
                        != std::cmp::Ordering::Equal
                })
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::PathEscapedRoot,
                    Some(&opened.canonical_path),
                    "the reopened directory chain no longer has the verified parent",
                ));
            }
            if is_within_verified_root(root, &expected.canonical_path)
                && !batch_rename::path_is_within_windows_ordinal(
                    &root.canonical_path,
                    &opened.canonical_path,
                )
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::PathEscapedRoot,
                    Some(&opened.canonical_path),
                    "a reopened directory resolved outside the selected root",
                ));
            }
            previous_canonical = Some(opened.canonical_path.clone());
            directories.push(Arc::new(opened.into_directory()));
        }
        if directories.is_empty() {
            Err(BackendFailure::new(
                BackendFailureKind::RootChanged,
                Some(&root.selected_path),
                "the verified root directory could not be reopened",
            ))
        } else {
            Ok(directories)
        }
    }
}

enum WorkerJob {
    Scan {
        request: RenameScanRequest,
        cancellation: ScanCancellation,
        reply: mpsc::Sender<RenameScanResult>,
        activity: Option<ActivityTask>,
    },
    Apply {
        plan: VerifiedRenamePlan,
        cancellation: ScanCancellation,
        activity: Option<ActivityTask>,
    },
    Undo {
        transaction_id: String,
        cancellation: ScanCancellation,
        activity: Option<ActivityTask>,
    },
}

impl WorkerJob {
    fn cancellation(&self) -> &ScanCancellation {
        match self {
            Self::Scan { cancellation, .. }
            | Self::Apply { cancellation, .. }
            | Self::Undo { cancellation, .. } => cancellation,
        }
    }

    fn is_scan(&self) -> bool {
        matches!(self, Self::Scan { .. })
    }

    fn activity(&self) -> Option<&ActivityTask> {
        match self {
            Self::Scan { activity, .. }
            | Self::Apply { activity, .. }
            | Self::Undo { activity, .. } => activity.as_ref(),
        }
    }
}

enum ActiveWorker {
    Scan(RenameScanGeneration, ScanCancellation, Option<ActivityTask>),
    Apply(ScanCancellation, Option<ActivityTask>),
    Undo(ScanCancellation, Option<ActivityTask>),
}

impl ActiveWorker {
    fn cancellation(&self) -> &ScanCancellation {
        match self {
            Self::Scan(_, cancellation, _)
            | Self::Apply(cancellation, _)
            | Self::Undo(cancellation, _) => cancellation,
        }
    }

    fn activity(&self) -> Option<&ActivityTask> {
        match self {
            Self::Scan(_, _, activity) | Self::Apply(_, activity) | Self::Undo(_, activity) => {
                activity.as_ref()
            }
        }
    }

    fn is_scan(&self) -> bool {
        matches!(self, Self::Scan(..))
    }
}

struct WorkerState {
    next_generation: u64,
    pending: Option<WorkerJob>,
    active: Option<ActiveWorker>,
    startup_cancellation: ScanCancellation,
    shutdown: bool,
}

/// App-owned, nonblocking view of the shared scan and mutation worker.
#[derive(Debug, Clone, Default)]
pub(crate) struct RenameRuntimeSnapshot {
    pub(crate) journal_ready: bool,
    pub(crate) journal_error: Option<String>,
    pub(crate) operation_active: bool,
    pub(crate) progress: Option<crate::batch_rename_transaction::RenameProgress>,
    pub(crate) receipts: Vec<crate::batch_rename_transaction::RenameRecoveryReceipt>,
    pub(crate) last_message: Option<String>,
}

/// A single background scanner. New submissions cancel the active generation
/// and replace the one pending request, so UI edits cannot create an
/// unbounded queue of filesystem traversals.
pub struct RenameScanCoordinator {
    state: Arc<(Mutex<WorkerState>, Condvar)>,
    runtime: Arc<Mutex<RenameRuntimeSnapshot>>,
    journal_path: Option<PathBuf>,
    worker: Option<JoinHandle<()>>,
    activity_center: Option<ActivityCenter>,
    next_activity_incarnation: AtomicU64,
}

impl RenameScanCoordinator {
    pub fn new() -> std::io::Result<Self> {
        Self::new_internal(None)
    }

    pub(crate) fn new_with_journal(journal_path: PathBuf) -> std::io::Result<Self> {
        Self::new_internal(Some(journal_path))
    }

    fn new_internal(journal_path: Option<PathBuf>) -> std::io::Result<Self> {
        let startup_cancellation = ScanCancellation::new();
        let state = Arc::new((
            Mutex::new(WorkerState {
                next_generation: 1,
                pending: None,
                active: None,
                startup_cancellation: startup_cancellation.clone(),
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let runtime = Arc::new(Mutex::new(RenameRuntimeSnapshot::default()));
        let worker_state = Arc::clone(&state);
        let worker_runtime = Arc::clone(&runtime);
        let worker_journal_path = journal_path.clone();
        let worker = thread::Builder::new()
            .name("batch-rename-worker".into())
            .spawn(move || {
                scan_worker_loop(
                    worker_state,
                    worker_runtime,
                    worker_journal_path,
                    startup_cancellation,
                )
            })?;
        Ok(Self {
            state,
            runtime,
            journal_path,
            worker: Some(worker),
            activity_center: None,
            next_activity_incarnation: AtomicU64::new(1),
        })
    }

    pub(crate) fn attach_activity_center(&mut self, center: ActivityCenter) {
        self.activity_center = Some(center);
    }

    pub fn submit(
        &self,
        root: ExplicitDirectoryRoot,
        recursive: bool,
    ) -> std::io::Result<RenameScanTicket> {
        let (reply, receiver) = mpsc::channel();
        let (lock, wake) = &*self.state;
        let mut state = lock
            .lock()
            .map_err(|_| std::io::Error::other("rename scan worker state was poisoned"))?;
        if state.shutdown {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "rename scan worker is shutting down",
            ));
        }
        if state
            .active
            .as_ref()
            .is_some_and(|active| !active.is_scan())
            || state
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.is_scan())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "a batch rename operation is in progress",
            ));
        }
        let generation = RenameScanGeneration(state.next_generation);
        state.next_generation = state
            .next_generation
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("rename scan generation overflow"))?;
        if let Some(active) = &state.active {
            active.cancellation().cancel();
            if let Some(activity) = active.activity() {
                activity.mark_cancelling();
            }
        }
        if let Some(pending) = state.pending.take() {
            cancel_pending_job(pending);
        }
        let cancellation = ScanCancellation::new();
        let activity = self.activity_center.as_ref().and_then(|center| {
            center.start(
                ActivityOwner::Rename,
                ActivitySlot::Primary,
                ActivityIncarnation::new(generation.0),
                ActivityOperation::ScanRenameTargets,
            )
        });
        if let Some(activity) = activity.as_ref() {
            let cancel = cancellation.clone();
            activity.set_cancel_handler(move || cancel.cancel());
        }
        state.pending = Some(WorkerJob::Scan {
            request: RenameScanRequest::new(root, recursive, generation),
            cancellation: cancellation.clone(),
            reply,
            activity,
        });
        wake.notify_one();
        Ok(RenameScanTicket {
            generation,
            cancellation,
            receiver,
        })
    }

    pub(crate) fn runtime_snapshot(&self) -> RenameRuntimeSnapshot {
        self.runtime
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|_| RenameRuntimeSnapshot {
                journal_ready: false,
                journal_error: Some("batch rename runtime state was poisoned".to_owned()),
                ..RenameRuntimeSnapshot::default()
            })
    }

    pub fn submit_apply(&self, plan: VerifiedRenamePlan) -> std::io::Result<()> {
        let journal_path = self.journal_path.as_ref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "this rename coordinator has no recovery journal",
            )
        })?;
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().map_err(|_| poisoned_worker_state())?;
        if state.shutdown {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "batch rename worker is shutting down",
            ));
        }
        if state.active.is_some() || state.pending.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the shared batch rename worker is busy",
            ));
        }
        let mut runtime = self.runtime.lock().map_err(|_| poisoned_runtime_state())?;
        if runtime.operation_active {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the prior batch rename result is still being published",
            ));
        }
        if !runtime.journal_ready {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                runtime
                    .journal_error
                    .clone()
                    .unwrap_or_else(|| "rename recovery data is not ready".to_owned()),
            ));
        }
        if runtime.receipts.iter().any(|receipt| {
            matches!(
                receipt.status(),
                crate::batch_rename_transaction::RenameTransactionStatus::RecoveryRequired
                    | crate::batch_rename_transaction::RenameTransactionStatus::Applying
                    | crate::batch_rename_transaction::RenameTransactionStatus::Undoing
            )
        }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "an unresolved rename recovery receipt must be handled first",
            ));
        }
        let cancellation = ScanCancellation::new();
        let activity = self.activity_center.as_ref().and_then(|center| {
            center.start(
                ActivityOwner::Rename,
                ActivitySlot::Secondary,
                ActivityIncarnation::new(
                    self.next_activity_incarnation
                        .fetch_add(1, Ordering::Relaxed),
                ),
                ActivityOperation::ApplyRename,
            )
        });
        if let Some(activity) = activity.as_ref() {
            let cancel = cancellation.clone();
            activity.set_cancel_handler(move || cancel.cancel());
        }
        state.pending = Some(WorkerJob::Apply {
            plan,
            cancellation,
            activity,
        });
        runtime.operation_active = true;
        runtime.progress = None;
        runtime.last_message = None;
        wake.notify_one();
        let _ = journal_path;
        Ok(())
    }

    pub fn submit_undo(&self, transaction_id: String) -> std::io::Result<()> {
        if self.journal_path.is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "this rename coordinator has no recovery journal",
            ));
        }
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().map_err(|_| poisoned_worker_state())?;
        if state.shutdown {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "batch rename worker is shutting down",
            ));
        }
        if state.active.is_some() || state.pending.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the shared batch rename worker is busy",
            ));
        }
        let mut runtime = self.runtime.lock().map_err(|_| poisoned_runtime_state())?;
        if runtime.operation_active {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "the prior batch rename result is still being published",
            ));
        }
        if !runtime.journal_ready {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                runtime
                    .journal_error
                    .clone()
                    .unwrap_or_else(|| "rename recovery data is not ready".to_owned()),
            ));
        }
        if !runtime.receipts.iter().any(|receipt| {
            receipt.id() == transaction_id
                && receipt.status().can_undo()
                && receipt.remaining_undo_count() > 0
        }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "the selected rename receipt has no verifiable Undo entries",
            ));
        }
        let cancellation = ScanCancellation::new();
        let activity = self.activity_center.as_ref().and_then(|center| {
            center.start(
                ActivityOwner::Rename,
                ActivitySlot::Secondary,
                ActivityIncarnation::new(
                    self.next_activity_incarnation
                        .fetch_add(1, Ordering::Relaxed),
                ),
                ActivityOperation::UndoRename,
            )
        });
        if let Some(activity) = activity.as_ref() {
            let cancel = cancellation.clone();
            activity.set_cancel_handler(move || cancel.cancel());
        }
        state.pending = Some(WorkerJob::Undo {
            transaction_id,
            cancellation,
            activity,
        });
        runtime.operation_active = true;
        runtime.progress = None;
        runtime.last_message = None;
        wake.notify_one();
        Ok(())
    }

    /// Cancel a pending mutation before its first phase, or ask an active
    /// mutation to stop at the next atomic native operation boundary.
    pub fn cancel_operation(&self) {
        let (lock, _) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            if let Some(active) = &state.active
                && !active.is_scan()
            {
                active.cancellation().cancel();
                if let Some(activity) = active.activity() {
                    activity.mark_cancelling();
                }
            }
            if state
                .pending
                .as_ref()
                .is_some_and(|pending| !pending.is_scan())
                && let Some(pending) = state.pending.take()
            {
                cancel_pending_job(pending);
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.operation_active = false;
                    runtime.progress = None;
                    runtime.last_message =
                        Some("rename was cancelled before any file was moved".to_owned());
                }
            }
        }
    }

    /// Request nonblocking shutdown. Retain this coordinator and poll
    /// `try_reap` until the worker has terminated, or call `shutdown` away
    /// from the UI frame path to join synchronously.
    pub fn request_shutdown(&self) {
        self.signal_shutdown();
    }

    /// Join only after the worker has completed. Returns false while a native
    /// filesystem call is still returning from cancellation.
    pub fn try_reap(&mut self) -> std::io::Result<bool> {
        let Some(worker) = self.worker.as_ref() else {
            return Ok(true);
        };
        if !worker.is_finished() {
            return Ok(false);
        }
        let Some(worker) = self.worker.take() else {
            return Ok(true);
        };
        worker
            .join()
            .map_err(|_| std::io::Error::other("rename scan worker panicked"))?;
        Ok(true)
    }

    /// Request shutdown and wait for worker exit. Call this off the UI frame
    /// path; normal UI close should retain the owner and use `try_reap`.
    pub fn shutdown(mut self) -> std::io::Result<()> {
        self.signal_shutdown();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("rename scan worker panicked"))?;
        }
        Ok(())
    }

    fn signal_shutdown(&self) {
        let (lock, wake) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.shutdown = true;
            state.startup_cancellation.cancel();
            if let Some(active) = &state.active {
                active.cancellation().cancel();
                if let Some(activity) = active.activity() {
                    activity.mark_cancelling();
                }
            }
            if let Some(pending) = state.pending.take() {
                cancel_pending_job(pending);
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.operation_active = false;
                    runtime.progress = None;
                    runtime.last_message =
                        Some("pending rename work was cancelled during shutdown".to_owned());
                }
            }
            wake.notify_all();
        }
    }
}

impl Drop for RenameScanCoordinator {
    fn drop(&mut self) {
        self.signal_shutdown();
        // Drop cannot prove that a blocked filesystem call has terminated.
        // Callers responsible for worker lifetime must retain and reap this
        // owner, or use shutdown from an app-finalization path.
    }
}

pub struct RenameScanTicket {
    generation: RenameScanGeneration,
    cancellation: ScanCancellation,
    receiver: mpsc::Receiver<RenameScanResult>,
}

impl RenameScanTicket {
    pub fn generation(&self) -> RenameScanGeneration {
        self.generation
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn try_recv(&self) -> Result<Option<RenameScanResult>, mpsc::TryRecvError> {
        match self.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(error @ mpsc::TryRecvError::Disconnected) => Err(error),
        }
    }
}

fn cancel_pending_job(job: WorkerJob) {
    match job {
        WorkerJob::Scan {
            request,
            cancellation,
            reply,
            activity,
        } => {
            cancellation.cancel();
            let _ = reply.send(empty_scan_result(&request, RenameScanStatus::Cancelled));
            if let Some(activity) = activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }
        WorkerJob::Apply {
            cancellation,
            activity,
            ..
        }
        | WorkerJob::Undo {
            cancellation,
            activity,
            ..
        } => {
            cancellation.cancel();
            if let Some(activity) = activity {
                activity.finish(
                    ActivityOutcome::Cancelled,
                    None,
                    ActivityRetirement::Confirmed,
                );
            }
        }
    }
}

fn poisoned_worker_state() -> std::io::Error {
    std::io::Error::other("batch rename worker state was poisoned")
}

fn poisoned_runtime_state() -> std::io::Error {
    std::io::Error::other("batch rename runtime state was poisoned")
}

fn scan_worker_loop(
    state: Arc<(Mutex<WorkerState>, Condvar)>,
    runtime: Arc<Mutex<RenameRuntimeSnapshot>>,
    journal_path: Option<PathBuf>,
    startup_cancellation: ScanCancellation,
) {
    if let Some(path) = journal_path.as_deref() {
        match crate::batch_rename_transaction::load_and_reconcile_journal(
            path,
            &startup_cancellation,
        ) {
            Ok(receipts) => {
                if let Ok(mut runtime) = runtime.lock() {
                    runtime.journal_ready = true;
                    runtime.receipts = receipts;
                    runtime.last_message = runtime
                        .receipts
                        .iter()
                        .rev()
                        .find(|receipt| receipt.status().blocks_new_apply())
                        .map(|receipt| receipt.message().to_owned());
                }
            }
            Err(error) => {
                if let Ok(mut runtime) = runtime.lock() {
                    runtime.journal_error = Some(error);
                }
            }
        }
    }
    loop {
        let job = {
            let (lock, wake) = &*state;
            let Ok(state) = lock.lock() else {
                return;
            };
            let mut state =
                match wake.wait_while(state, |state| state.pending.is_none() && !state.shutdown) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            if state.shutdown {
                return;
            }
            let Some(job) = state.pending.take() else {
                continue;
            };
            state.active = Some(match &job {
                WorkerJob::Scan {
                    request,
                    cancellation,
                    activity,
                    ..
                } => ActiveWorker::Scan(request.generation, cancellation.clone(), activity.clone()),
                WorkerJob::Apply {
                    cancellation,
                    activity,
                    ..
                } => ActiveWorker::Apply(cancellation.clone(), activity.clone()),
                WorkerJob::Undo {
                    cancellation,
                    activity,
                    ..
                } => ActiveWorker::Undo(cancellation.clone(), activity.clone()),
            });
            job
        };
        match job {
            WorkerJob::Scan {
                request,
                cancellation,
                reply,
                activity,
            } => {
                let generation = request.generation;
                if let Some(activity) = activity.as_ref() {
                    activity.mark_running();
                }
                let result = scan_rename_root(request.clone(), &cancellation);
                if let Some(activity) = activity.as_ref() {
                    let (outcome, diagnostic) = match result.status {
                        RenameScanStatus::Cancelled => (ActivityOutcome::Cancelled, None),
                        RenameScanStatus::Unsupported => (
                            ActivityOutcome::Failed,
                            Some(ActivityDiagnostic::NativeOperationUnavailable),
                        ),
                        RenameScanStatus::InvalidRoot | RenameScanStatus::Inaccessible => (
                            ActivityOutcome::Failed,
                            Some(ActivityDiagnostic::InvalidInput),
                        ),
                        RenameScanStatus::Complete
                        | RenameScanStatus::Partial
                        | RenameScanStatus::ResourceLimit
                        | RenameScanStatus::ChangedDuringScan => (ActivityOutcome::Completed, None),
                    };
                    activity.finish(outcome, diagnostic, ActivityRetirement::Unavailable);
                }
                let _ = reply.send(result);
                clear_active_worker(
                    &state,
                    |active| matches!(active, ActiveWorker::Scan(active_generation, _, _) if *active_generation == generation),
                );
            }
            WorkerJob::Apply {
                plan,
                cancellation,
                activity,
            } => {
                if let Some(activity) = activity.as_ref() {
                    activity.mark_running();
                }
                let result = if let Some(path) = journal_path.as_deref() {
                    let report_runtime = Arc::clone(&runtime);
                    let report_activity = activity.clone();
                    Some(crate::batch_rename_transaction::execute_apply(
                        plan,
                        path,
                        &cancellation,
                        move |progress| {
                            if let Some(activity) = report_activity.as_ref() {
                                activity.set_progress(Some(ActivityProgress::new(
                                    progress.completed as u64,
                                    Some(progress.total as u64),
                                    ActivityProgressUnit::Files,
                                )));
                            }
                            if let Ok(mut runtime) = report_runtime.lock() {
                                runtime.progress = Some(progress);
                            }
                        },
                    ))
                } else {
                    None
                };
                if let (Some(activity), Some(result)) = (activity.as_ref(), result.as_ref()) {
                    let (outcome, diagnostic) = rename_activity_outcome(result.receipt.status());
                    activity.finish(outcome, diagnostic, ActivityRetirement::Unavailable);
                }
                clear_active_worker(&state, |active| matches!(active, ActiveWorker::Apply(..)));
                if let Some(result) = result {
                    publish_operation_result(&runtime, result);
                } else if let Ok(mut runtime) = runtime.lock() {
                    runtime.operation_active = false;
                    runtime.last_message = Some("rename worker has no recovery journal".to_owned());
                }
            }
            WorkerJob::Undo {
                transaction_id,
                cancellation,
                activity,
            } => {
                if let Some(activity) = activity.as_ref() {
                    activity.mark_running();
                }
                let result = if let Some(path) = journal_path.as_deref() {
                    let report_runtime = Arc::clone(&runtime);
                    let report_activity = activity.clone();
                    Some(crate::batch_rename_transaction::execute_undo(
                        &transaction_id,
                        path,
                        &cancellation,
                        move |progress| {
                            if let Some(activity) = report_activity.as_ref() {
                                activity.set_progress(Some(ActivityProgress::new(
                                    progress.completed as u64,
                                    Some(progress.total as u64),
                                    ActivityProgressUnit::Files,
                                )));
                            }
                            if let Ok(mut runtime) = report_runtime.lock() {
                                runtime.progress = Some(progress);
                            }
                        },
                    ))
                } else {
                    None
                };
                if let (Some(activity), Some(result)) = (activity.as_ref(), result.as_ref()) {
                    let (outcome, diagnostic) = rename_activity_outcome(result.receipt.status());
                    activity.finish(outcome, diagnostic, ActivityRetirement::Unavailable);
                }
                clear_active_worker(&state, |active| matches!(active, ActiveWorker::Undo(..)));
                if let Some(result) = result {
                    publish_operation_result(&runtime, result);
                } else if let Ok(mut runtime) = runtime.lock() {
                    runtime.operation_active = false;
                    runtime.last_message = Some("rename worker has no recovery journal".to_owned());
                }
            }
        }
        let (lock, _) = &*state;
        if let Ok(state) = lock.lock() {
            if state.shutdown {
                return;
            }
        } else {
            return;
        }
    }
}

fn clear_active_worker(
    state: &Arc<(Mutex<WorkerState>, Condvar)>,
    matches: impl FnOnce(&ActiveWorker) -> bool,
) {
    let (lock, _) = &**state;
    if let Ok(mut state) = lock.lock()
        && state.active.as_ref().is_some_and(matches)
    {
        state.active = None;
    }
}

fn publish_operation_result(
    runtime: &Arc<Mutex<RenameRuntimeSnapshot>>,
    result: crate::batch_rename_transaction::RenameOperationResult,
) {
    if let Ok(mut runtime) = runtime.lock() {
        if !result.receipt.entries().is_empty() {
            if let Some(existing) = runtime
                .receipts
                .iter_mut()
                .find(|receipt| receipt.id() == result.receipt.id())
            {
                *existing = result.receipt.clone();
            } else if runtime.receipts.len()
                < crate::batch_rename_transaction::MAX_RETAINED_RENAME_RECEIPTS
            {
                runtime.receipts.push(result.receipt.clone());
            }
        }
        runtime.last_message = Some(result.message);
        runtime.operation_active = false;
    }
}

fn rename_activity_outcome(
    status: crate::batch_rename_transaction::RenameTransactionStatus,
) -> (ActivityOutcome, Option<ActivityDiagnostic>) {
    use crate::batch_rename_transaction::RenameTransactionStatus;
    match status {
        RenameTransactionStatus::Cancelled => (ActivityOutcome::Cancelled, None),
        RenameTransactionStatus::Applied
        | RenameTransactionStatus::Partial
        | RenameTransactionStatus::Undone
        | RenameTransactionStatus::Restored => (ActivityOutcome::Completed, None),
        RenameTransactionStatus::Applying
        | RenameTransactionStatus::Undoing
        | RenameTransactionStatus::RecoveryRequired
        | RenameTransactionStatus::Failed
        | RenameTransactionStatus::Refused => (
            ActivityOutcome::Failed,
            Some(ActivityDiagnostic::ResultUnavailable),
        ),
    }
}

#[cfg(not(windows))]
struct NativeRenameScanBackend;

#[cfg(not(windows))]
impl RenameScanBackend for NativeRenameScanBackend {
    type Handle = ();

    fn admit_root(
        &self,
        request: &RenameScanRequest,
    ) -> Result<RootAdmission<Self::Handle>, BackendFailure> {
        Err(BackendFailure::new(
            BackendFailureKind::UnsupportedIdentity,
            Some(request.root.as_path()),
            "native file identity is not available on this platform",
        ))
    }

    fn enumerate_directory(
        &self,
        directory: &OpenDirectory<Self::Handle>,
        _remaining_entries: usize,
        _remaining_path_bytes: usize,
        _cancellation: &ScanCancellation,
    ) -> Result<Vec<EnumeratedName>, BackendFailure> {
        Err(BackendFailure::new(
            BackendFailureKind::UnsupportedIdentity,
            Some(&directory.canonical_path),
            "native directory enumeration is not available on this platform",
        ))
    }

    fn open_child(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        _parent_chain: &[DirectoryProof],
        _root: &RootProof,
        _name: &OsStr,
        _expected: Option<&EnumeratedName>,
    ) -> Result<OpenedObject<Self::Handle>, BackendFailure> {
        Err(BackendFailure::new(
            BackendFailureKind::UnsupportedIdentity,
            Some(&parent.canonical_path),
            "native file identity is not available on this platform",
        ))
    }

    fn acquire_child_directory_for_enumeration(
        &self,
        parent: &OpenDirectory<Self::Handle>,
        _parent_chain: &[DirectoryProof],
        _root: &RootProof,
        child: &OpenedObject<Self::Handle>,
    ) -> Result<OpenDirectory<Self::Handle>, BackendFailure> {
        Err(BackendFailure::new(
            BackendFailureKind::UnsupportedIdentity,
            Some(&child.canonical_path),
            "native directory identity is not available on this platform",
        ))
    }

    fn reopen_chain(
        &self,
        chain: &[DirectoryProof],
        _root: &RootProof,
        _final_access: DirectoryOpenAccess,
    ) -> Result<OpenDirectoryChain<Self::Handle>, BackendFailure> {
        Err(BackendFailure::new(
            BackendFailureKind::UnsupportedIdentity,
            chain.last().map(|entry| entry.canonical_path.as_path()),
            "native file identity is not available on this platform",
        ))
    }
}

#[cfg(windows)]
fn is_within_verified_root(root: &RootProof, path: &Path) -> bool {
    batch_rename::path_is_within_windows_ordinal(&root.canonical_path, path)
}

#[cfg(windows)]
fn validate_local_root_spelling(path: &Path) -> Result<(), BackendFailure> {
    use std::os::windows::ffi::OsStrExt;

    if !path.is_absolute() {
        return Err(BackendFailure::new(
            BackendFailureKind::InvalidRoot,
            Some(path),
            "the selected root must be an absolute local path",
        ));
    }
    let local_disk = path
        .components()
        .next()
        .is_some_and(|component| match component {
            Component::Prefix(prefix) => matches!(
                prefix.kind(),
                std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
            ),
            _ => false,
        });
    if !local_disk {
        return Err(BackendFailure::new(
            BackendFailureKind::InvalidRoot,
            Some(path),
            "UNC, device and non-disk roots are not supported",
        ));
    }
    if path.as_os_str().encode_wide().any(|unit| unit == 0)
        || path.components().any(|component| match component {
            Component::Normal(name) => name
                .encode_wide()
                .any(|unit| unit == 0 || unit == b':' as u16),
            Component::ParentDir => true,
            _ => false,
        })
    {
        return Err(BackendFailure::new(
            BackendFailureKind::InvalidRoot,
            Some(path),
            "NUL, alternate data stream and parent traversal components are not supported",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn verified_root_proof(
    root_path: &Path,
    expected_identity: NativeFileIdentity,
) -> Result<RootProof, String> {
    let root = ExplicitDirectoryRoot::new(root_path.to_path_buf())
        .map_err(|error| format!("invalid recovery root: {error:?}"))?;
    let request = RenameScanRequest::new(root, false, RenameScanGeneration(0));
    let admission = NativeRenameScanBackend
        .admit_root(&request)
        .map_err(|error| error.to_string())?;
    if admission.proof.identity != expected_identity
        || batch_rename::compare_paths_windows_ordinal(&admission.proof.canonical_path, root_path)
            != std::cmp::Ordering::Equal
    {
        return Err("the recorded rename root identity changed".to_owned());
    }
    Ok(admission.proof)
}

#[cfg(windows)]
fn verified_parent_chain(
    root: &RootProof,
    parent_path: &Path,
    expected_identity: NativeFileIdentity,
) -> Result<Vec<DirectoryProof>, String> {
    if !batch_rename::path_is_within_windows_ordinal(&root.canonical_path, parent_path) {
        return Err("the recorded rename parent escaped its root".to_owned());
    }
    let relative = if batch_rename::compare_paths_windows_ordinal(&root.canonical_path, parent_path)
        == std::cmp::Ordering::Equal
    {
        PathBuf::new()
    } else {
        parent_path
            .strip_prefix(&root.canonical_path)
            .map_err(|_| "the recorded parent is not lexically below its root".to_owned())?
            .to_path_buf()
    };
    let mut chain = root.chain.clone();
    let mut current = root.canonical_path.clone();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("the recorded parent path contains traversal".to_owned());
        };
        current.push(name);
        let opened =
            open_native_object(&current, true, false).map_err(|error| error.to_string())?;
        if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
            return Err(format!(
                "the recorded parent is not a plain directory: {}",
                current.display()
            ));
        }
        let Some(expected_parent) = chain.last() else {
            return Err("the verified root chain is empty".to_owned());
        };
        if opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(
                actual_parent,
                &expected_parent.canonical_path,
            ) != std::cmp::Ordering::Equal
        }) {
            return Err("the recorded parent chain changed".to_owned());
        }
        chain.push(DirectoryProof {
            lookup_path: current.clone(),
            canonical_path: opened.canonical_path,
            identity: opened.identity,
        });
    }
    if chain.last().is_none_or(|directory| {
        directory.identity != expected_identity
            || batch_rename::compare_paths_windows_ordinal(&directory.canonical_path, parent_path)
                != std::cmp::Ordering::Equal
    }) {
        return Err("the recorded parent directory identity changed".to_owned());
    }
    Ok(chain)
}

#[cfg(windows)]
fn observe_watch_file_native(
    root: &NativeWatchRoot,
    reported_path: &Path,
    require_read_lock: bool,
) -> Result<NativeWatchFileObservation, NativeWatchError> {
    let root = refresh_watch_root(root)?;
    validate_local_root_spelling(reported_path)
        .map_err(|_| native_watch_error(NativeWatchErrorKind::InvalidPath))?;
    let relative = relative_watch_path(&root.proof.canonical_path, reported_path)?;
    let Some(leaf) = relative.file_name() else {
        return Err(native_watch_error(NativeWatchErrorKind::InvalidPath));
    };
    if relative.components().count().saturating_sub(1) > MAX_RENAME_SCAN_DEPTH {
        return Err(native_watch_error(NativeWatchErrorKind::InvalidPath));
    }
    let relative_parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent_path = root.proof.canonical_path.join(relative_parent);
    let parent_chain = admit_watch_parent_chain(&root.proof, &parent_path)?;
    let backend = NativeRenameScanBackend;
    let parent_guards = backend
        .reopen_chain(&parent_chain, &root.proof, DirectoryOpenAccess::Metadata)
        .map_err(map_watch_backend_failure)?;
    let Some(parent) = parent_guards.last() else {
        return Err(native_watch_error(NativeWatchErrorKind::ParentChanged));
    };
    let lookup_path = parent.canonical_path.join(leaf);
    let opened = if require_read_lock {
        open_native_verified_file(&lookup_path)
    } else {
        open_native_object(&lookup_path, false, false)
    }
    .map_err(map_watch_file_io)?;
    if opened.reparse_point {
        return Err(native_watch_error(NativeWatchErrorKind::ReparsePoint));
    }
    if opened.kind != SnapshotEntryKind::File {
        return Err(native_watch_error(NativeWatchErrorKind::NotRegularFile));
    }
    if !batch_rename::path_is_within_windows_ordinal(
        &root.proof.canonical_path,
        &opened.canonical_path,
    ) || opened.canonical_path.parent().is_none_or(|actual_parent| {
        batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
            != std::cmp::Ordering::Equal
    }) {
        return Err(native_watch_error(NativeWatchErrorKind::OutsideRoot));
    }
    let length = opened
        .length
        .ok_or_else(|| native_watch_error(NativeWatchErrorKind::UnsupportedIdentity))?;
    let modified = opened
        .modified
        .ok_or_else(|| native_watch_error(NativeWatchErrorKind::UnsupportedIdentity))?;
    let parent_identity = parent.identity;
    backend
        .reopen_chain(&parent_chain, &root.proof, DirectoryOpenAccess::Metadata)
        .map_err(map_watch_backend_failure)?;
    Ok(NativeWatchFileObservation {
        canonical_path: opened.canonical_path,
        root_identity: root.proof.identity,
        parent_identity,
        file_identity: opened.identity,
        length,
        modified,
    })
}

#[cfg(windows)]
fn relative_watch_path(root: &Path, path: &Path) -> Result<PathBuf, NativeWatchError> {
    if !path.is_absolute() {
        return Err(native_watch_error(NativeWatchErrorKind::InvalidPath));
    }
    let root_components = root.components().collect::<Vec<_>>();
    let path_components = path.components().collect::<Vec<_>>();
    if path_components.len() <= root_components.len()
        || !root_components
            .iter()
            .zip(path_components.iter())
            .all(|(root, path)| {
                batch_rename::compare_paths_windows_ordinal(
                    Path::new(root.as_os_str()),
                    Path::new(path.as_os_str()),
                ) == std::cmp::Ordering::Equal
            })
    {
        return Err(native_watch_error(NativeWatchErrorKind::OutsideRoot));
    }
    let mut relative = PathBuf::new();
    for component in path_components.iter().skip(root_components.len()) {
        match component {
            Component::Normal(name) => relative.push(name),
            _ => return Err(native_watch_error(NativeWatchErrorKind::InvalidPath)),
        }
    }
    Ok(relative)
}

#[cfg(windows)]
fn admit_watch_parent_chain(
    root: &RootProof,
    parent_path: &Path,
) -> Result<Vec<DirectoryProof>, NativeWatchError> {
    let relative = if batch_rename::compare_paths_windows_ordinal(&root.canonical_path, parent_path)
        == std::cmp::Ordering::Equal
    {
        PathBuf::new()
    } else {
        relative_watch_path(&root.canonical_path, parent_path)?
    };
    let mut chain = root.chain.clone();
    let mut current = root.canonical_path.clone();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(native_watch_error(NativeWatchErrorKind::InvalidPath));
        };
        current.push(name);
        let opened = open_native_object(&current, false, false).map_err(map_watch_file_io)?;
        if opened.reparse_point {
            return Err(native_watch_error(NativeWatchErrorKind::ReparsePoint));
        }
        if opened.kind != SnapshotEntryKind::Directory {
            return Err(native_watch_error(NativeWatchErrorKind::ParentChanged));
        }
        let Some(expected_parent) = chain.last() else {
            return Err(native_watch_error(NativeWatchErrorKind::RootChanged));
        };
        if opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(
                actual_parent,
                &expected_parent.canonical_path,
            ) != std::cmp::Ordering::Equal
        }) || opened.identity.volume_serial_number() != root.identity.volume_serial_number()
        {
            return Err(native_watch_error(NativeWatchErrorKind::OutsideRoot));
        }
        chain.push(DirectoryProof {
            lookup_path: current.clone(),
            canonical_path: opened.canonical_path,
            identity: opened.identity,
        });
    }
    NativeRenameScanBackend
        .reopen_chain(&chain, root, DirectoryOpenAccess::Metadata)
        .map_err(map_watch_backend_failure)?;
    Ok(chain)
}

#[cfg(windows)]
fn map_watch_file_io(error: std::io::Error) -> NativeWatchError {
    let raw_code = native_raw_os_error(&error);
    let kind = match raw_code {
        Some(2 | 3) => NativeWatchErrorKind::Missing,
        Some(32 | 33) => NativeWatchErrorKind::Busy,
        Some(123 | 161) => NativeWatchErrorKind::InvalidPath,
        _ if error.kind() == std::io::ErrorKind::Unsupported => {
            NativeWatchErrorKind::UnsupportedIdentity
        }
        _ => NativeWatchErrorKind::Inaccessible,
    };
    native_watch_error(kind)
}

#[cfg(windows)]
fn open_native_mutation_file(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_SHARE_READ, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let desired_access = DELETE.0 | FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0;
    // Deny write and delete sharing while the transaction retains this handle.
    // This pins the content fingerprint and namespace entry through each
    // handle-based rename operation.
    let share_mode = FILE_SHARE_READ;
    let flags = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS;
    // SAFETY: the path is a validated local, NUL-terminated UTF-16 path. The
    // handle is opened without following a final reparse point, and is
    // immediately transferred into File for RAII ownership.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            desired_access,
            share_mode,
            None,
            OPEN_EXISTING,
            flags,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(DELETE | FILE_READ_DATA)",
            windows_error_to_io(error),
        )
    })?;
    // SAFETY: CreateFileW returned a valid owned HANDLE which is transferred
    // exactly once to File and closed by Drop.
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    let identity = native_file_identity(&file)
        .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?;
    let canonical_path = final_path_for_handle(&file)
        .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?;
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path,
        identity,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder: false,
        handle: file,
    })
}

#[cfg(windows)]
fn open_native_replace_file(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_SHARE_READ, OPEN_EXISTING, READ_CONTROL,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let desired_access = DELETE.0 | FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0;
    // The retained handle denies write/delete sharing and can read the source
    // security descriptor. The original bytes are never changed in place.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            desired_access,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(DELETE | FILE_READ_DATA | FILE_READ_ATTRIBUTES | READ_CONTROL)",
            windows_error_to_io(error),
        )
    })?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path: final_path_for_handle(&file)
            .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?,
        identity: native_file_identity(&file)
            .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder: false,
        handle: file,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifiedReplaceFailureKind {
    InvalidEvidence,
    RootChanged,
    ParentChanged,
    SourceChanged,
    Inaccessible,
    Unsupported,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedReplaceFailure {
    kind: VerifiedReplaceFailureKind,
    message: String,
}

/// Create a durable, CREATE_NEW appdata backup and verify its exact bytes and
/// native identity before it can authorize source relocation.
pub(crate) fn create_verified_replace_backup(
    path: &Path,
    bytes: &[u8],
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> std::io::Result<VerifiedReplaceBackup> {
    if bytes.len() as u64 > MAX_RENAME_HASH_BYTES_PER_FILE
        || <[u8; 32]>::from(Sha256::digest(bytes)) != expected_sha256
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "replacement backup bytes exceed limits or do not match their approved hash",
        ));
    }
    #[cfg(windows)]
    {
        create_verified_replace_backup_native(path, bytes, expected_sha256, cancellation)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, cancellation);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "safe replacement backups are available only on Windows",
        ))
    }
}

/// Reopen a recorded backup without following a final reparse point, then
/// require its recorded native ID, length, and hash before exposing bytes.
pub(crate) fn open_verified_replace_backup(
    path: &Path,
    expected_identity: NativeFileIdentity,
    expected_length: u64,
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> std::io::Result<VerifiedReplaceBackup> {
    if expected_length > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement backup exceeds the content verification limit",
        ));
    }
    #[cfg(windows)]
    {
        open_verified_replace_backup_native(
            path,
            expected_identity,
            expected_length,
            expected_sha256,
            cancellation,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (
            path,
            expected_identity,
            expected_length,
            expected_sha256,
            cancellation,
        );
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "safe replacement backups are available only on Windows",
        ))
    }
}

impl VerifiedReplaceFailure {
    pub(crate) fn kind(&self) -> VerifiedReplaceFailureKind {
        self.kind
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuardedReplaceState {
    pub(crate) identity: NativeFileIdentity,
    pub(crate) length: u64,
    pub(crate) modified: SystemTime,
    pub(crate) content_sha256: [u8; 32],
}

/// A source opened from retained M8 evidence. Its content remains intact until
/// the separately staged output has been flushed and verified.
pub(crate) struct GuardedReplaceFile {
    original_path: PathBuf,
    original_state: GuardedReplaceState,
    #[cfg(windows)]
    native: GuardedReplaceHandle,
}

/// A CREATE_NEW sibling containing exact, flushed output bytes and the source
/// file's copied security descriptor/properties.
pub(crate) struct GuardedReplaceStage {
    state: GuardedReplaceState,
    #[cfg(windows)]
    native: GuardedReplaceHandle,
}

/// Durable app-owned copy of an original payload, pinned by file ID and SHA.
pub(crate) struct VerifiedReplaceBackup {
    path: PathBuf,
    state: GuardedReplaceState,
    bytes: Arc<[u8]>,
    #[cfg(windows)]
    file: std::fs::File,
}

impl VerifiedReplaceBackup {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn state(&self) -> GuardedReplaceState {
        self.state
    }

    pub(crate) fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
}

#[cfg(windows)]
struct GuardedReplaceHandle {
    current_path: PathBuf,
    parent_path: PathBuf,
    parent_identity: NativeFileIdentity,
    identity: NativeFileIdentity,
    root: RootProof,
    parent_chain: Vec<DirectoryProof>,
    file: std::fs::File,
    root_guards: OpenDirectoryChain<std::fs::File>,
    parent_guards: OpenDirectoryChain<std::fs::File>,
}

impl GuardedReplaceFile {
    pub(crate) fn original_path(&self) -> &Path {
        &self.original_path
    }

    pub(crate) fn current_path(&self) -> &Path {
        #[cfg(windows)]
        return &self.native.current_path;
        #[cfg(not(windows))]
        &self.original_path
    }

    pub(crate) fn identity(&self) -> NativeFileIdentity {
        self.original_state.identity
    }

    pub(crate) fn original_state(&self) -> GuardedReplaceState {
        self.original_state
    }

    pub(crate) fn verify_at(
        &self,
        expected_path: &Path,
        expected_length: u64,
        expected_modified: Option<SystemTime>,
        expected_sha256: [u8; 32],
        expected_bytes: Option<&[u8]>,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<GuardedReplaceState> {
        #[cfg(windows)]
        return self.native.verify_at(
            expected_path,
            expected_length,
            expected_modified,
            expected_sha256,
            expected_bytes,
            cancellation,
        );
        #[cfg(not(windows))]
        {
            let _ = (
                expected_path,
                expected_length,
                expected_modified,
                expected_sha256,
                expected_bytes,
                cancellation,
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement writes are available only on Windows",
            ))
        }
    }

    pub(crate) fn rename_to_leaf(
        &mut self,
        leaf: &OsStr,
        expected_state: GuardedReplaceState,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<PathBuf> {
        #[cfg(windows)]
        return self
            .native
            .rename_to_leaf(leaf, expected_state, cancellation);
        #[cfg(not(windows))]
        {
            let _ = (leaf, expected_state, cancellation);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement writes are available only on Windows",
            ))
        }
    }

    pub(crate) fn delete_if_matches(
        &self,
        expected_state: GuardedReplaceState,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<()> {
        #[cfg(windows)]
        {
            ensure_unnamed_stream_only(&self.native.file)?;
            self.native.delete_if_matches(expected_state, cancellation)
        }
        #[cfg(not(windows))]
        {
            let _ = (expected_state, cancellation);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement cleanup is available only on Windows",
            ))
        }
    }

    pub(crate) fn create_output_stage(
        &self,
        leaf: &OsStr,
        bytes: &[u8],
        sha256: [u8; 32],
        modified: Option<SystemTime>,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<GuardedReplaceStage> {
        #[cfg(windows)]
        {
            self.verify_at(
                &self.native.current_path,
                self.original_state.length,
                Some(self.original_state.modified),
                self.original_state.content_sha256,
                None,
                cancellation,
            )?;
            self.native
                .create_output_stage(leaf, bytes, sha256, modified, cancellation)
        }
        #[cfg(not(windows))]
        {
            let _ = (leaf, bytes, sha256, modified, cancellation);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement writes are available only on Windows",
            ))
        }
    }
}

#[cfg(windows)]
fn prepare_hotfolder_source(
    selected_root: &NativeWatchRoot,
    path: &Path,
    expected_root_identity: NativeFileIdentity,
    expected_parent_identity: NativeFileIdentity,
    expected_file_identity: NativeFileIdentity,
    expected_length: u64,
    expected_modified: SystemTime,
    cancellation: &ScanCancellation,
) -> Result<GuardedReplaceFile, NativeHotFolderError> {
    if cancellation.is_cancelled() {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::Cancelled,
            "file operation cancelled before source admission",
            Vec::new(),
        ));
    }
    if expected_length > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "source file exceeds the bounded content verification limit",
            Vec::new(),
        ));
    }
    let root = refresh_watch_root(selected_root).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "watched root identity changed before action",
            Vec::new(),
        )
    })?;
    if root.identity() != expected_root_identity {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "watched root identity changed before action",
            Vec::new(),
        ));
    }
    let relative = relative_watch_path(root.canonical_root(), path).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source path is outside the admitted watched root",
            Vec::new(),
        )
    })?;
    let parent_path = root
        .canonical_root()
        .join(relative.parent().unwrap_or_else(|| Path::new("")));
    let parent_chain = admit_watch_parent_chain(&root.proof, &parent_path).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source parent identity changed before action",
            Vec::new(),
        )
    })?;
    let backend = NativeRenameScanBackend;
    let root_guards = backend
        .reopen_chain(
            &root.proof.chain,
            &root.proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(|_| {
            native_hotfolder_error(
                NativeHotFolderFailureKind::StaleSource,
                "watched root could not be held during action admission",
                Vec::new(),
            )
        })?;
    let parent_guards = backend
        .reopen_chain(&parent_chain, &root.proof, DirectoryOpenAccess::Metadata)
        .map_err(|_| {
            native_hotfolder_error(
                NativeHotFolderFailureKind::StaleSource,
                "source parent could not be held during action admission",
                Vec::new(),
            )
        })?;
    let Some(parent) = parent_guards.last() else {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source parent evidence is unavailable",
            Vec::new(),
        ));
    };
    if parent.identity != expected_parent_identity {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source parent identity changed before action",
            Vec::new(),
        ));
    }
    let opened = open_native_replace_file(path).map_err(|error| {
        map_hotfolder_io_error(
            error,
            NativeHotFolderFailureKind::StaleSource,
            "source could not be opened with a guarded native handle",
        )
    })?;
    if opened.kind != SnapshotEntryKind::File
        || opened.reparse_point
        || opened.identity != expected_file_identity
        || opened.length != Some(expected_length)
        || opened.modified != Some(expected_modified)
        || batch_rename::compare_paths_windows_ordinal(&opened.canonical_path, path)
            != std::cmp::Ordering::Equal
        || opened.canonical_path.parent().is_none_or(|actual_parent| {
            batch_rename::compare_paths_windows_ordinal(actual_parent, &parent.canonical_path)
                != std::cmp::Ordering::Equal
        })
        || !batch_rename::path_is_within_windows_ordinal(
            root.canonical_root(),
            &opened.canonical_path,
        )
    {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source identity, path, or metadata changed before action",
            Vec::new(),
        ));
    }
    let native = GuardedReplaceHandle {
        current_path: opened.canonical_path.clone(),
        parent_path: parent.canonical_path.clone(),
        parent_identity: parent.identity,
        identity: opened.identity,
        root: root.proof.clone(),
        parent_chain,
        file: opened.handle,
        root_guards,
        parent_guards,
    };
    ensure_unnamed_stream_only(&native.file).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "source has named streams or its stream set cannot be verified",
            Vec::new(),
        )
    })?;
    let original_state = capture_hotfolder_guard(&native, cancellation).map_err(|error| {
        map_hotfolder_io_error(
            error,
            NativeHotFolderFailureKind::StaleSource,
            "source changed during guarded content verification",
        )
    })?;
    Ok(GuardedReplaceFile {
        original_path: opened.canonical_path,
        original_state,
        native,
    })
}

#[cfg(windows)]
fn execute_hotfolder_native_action_windows(
    selected_root: &NativeWatchRoot,
    source_path: &Path,
    expected: NativeHotFolderInput,
    action: NativeHotFolderAction,
    selected_destination: Option<&NativeWatchRoot>,
    target_leaf: &OsStr,
    operation_id: u128,
    progress: &mut dyn FnMut(NativeHotFolderProgress) -> std::io::Result<()>,
    cancellation: &ScanCancellation,
) -> Result<NativeHotFolderResult, NativeHotFolderError> {
    validate_replace_leaf(target_leaf).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "target filename is not one safe path component",
            Vec::new(),
        )
    })?;
    let mut source = prepare_hotfolder_source(
        selected_root,
        source_path,
        expected.root_identity,
        expected.parent_identity,
        expected.file_identity,
        expected.length,
        expected.modified,
        cancellation,
    )?;
    let source_state = source.original_state;
    if source_state.content_sha256 != expected.sha256 {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::StaleSource,
            "source content changed after its admission fingerprint",
            Vec::new(),
        ));
    }

    if action == NativeHotFolderAction::Rename {
        rename_hotfolder_guard_to_leaf(&mut source.native, target_leaf, source_state, cancellation)
            .map_err(|error| {
                map_hotfolder_io_error(
                    error,
                    NativeHotFolderFailureKind::Io,
                    "source could not be renamed without replacement",
                )
            })?;
        let output = hotfolder_output(
            NativeHotFolderOutputKind::Final,
            &source.native,
            source_state,
        );
        let durable = progress(NativeHotFolderProgress {
            phase: NativeHotFolderPhase::Published,
            outputs: vec![output.clone()],
        })
        .is_ok();
        let verified = verify_published_hotfolder_guard(&source.native, source_state).is_ok();
        return Ok(NativeHotFolderResult {
            source_identity: source_state.identity,
            outputs: vec![output],
            phase: if durable && verified {
                NativeHotFolderPhase::Complete
            } else {
                NativeHotFolderPhase::Published
            },
            partial: !durable || !verified,
            reason: (!durable)
                .then_some("published rename could not be durably recorded")
                .or_else(|| (!verified).then_some("renamed file could not be revalidated")),
        });
    }

    let destination = selected_destination.ok_or_else(|| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationUnavailable,
            "an explicit destination directory is required",
            Vec::new(),
        )
    })?;
    let destination = refresh_watch_root(destination).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationUnavailable,
            "destination directory identity changed before action",
            Vec::new(),
        )
    })?;
    if action == NativeHotFolderAction::Move
        && source.native.root.identity.volume_serial_number()
            == destination.identity().volume_serial_number()
    {
        rename_hotfolder_guard_to_root(
            &mut source.native,
            &destination,
            target_leaf,
            source_state,
            cancellation,
        )
        .map_err(|error| {
            map_hotfolder_io_error(
                error,
                NativeHotFolderFailureKind::Io,
                "source could not be moved without replacement",
            )
        })?;
        let output = hotfolder_output(
            NativeHotFolderOutputKind::Final,
            &source.native,
            source_state,
        );
        let durable = progress(NativeHotFolderProgress {
            phase: NativeHotFolderPhase::Published,
            outputs: vec![output.clone()],
        })
        .is_ok();
        let verified = verify_published_hotfolder_guard(&source.native, source_state).is_ok();
        return Ok(NativeHotFolderResult {
            source_identity: source_state.identity,
            outputs: vec![output],
            phase: if durable && verified {
                NativeHotFolderPhase::Complete
            } else {
                NativeHotFolderPhase::Published
            },
            partial: !durable || !verified,
            reason: (!durable)
                .then_some("published move could not be durably recorded")
                .or_else(|| (!verified).then_some("moved file could not be revalidated")),
        });
    }

    if action == NativeHotFolderAction::Move {
        return execute_hotfolder_cross_volume_move(
            &source,
            &destination,
            target_leaf,
            source_state,
            operation_id,
            progress,
            cancellation,
        );
    }

    let mut stage = create_hotfolder_output_stage(
        &source.native,
        &destination,
        target_leaf,
        source_state,
        operation_id,
        progress,
        cancellation,
    )?;
    let staged_state = stage.state;
    let stage_path_before_publish = stage.native.current_path.clone();
    if let Err(error) =
        rename_hotfolder_guard_to_leaf(&mut stage.native, target_leaf, staged_state, cancellation)
    {
        let published = batch_rename::compare_paths_windows_ordinal(
            &stage.native.current_path,
            &stage_path_before_publish,
        ) != std::cmp::Ordering::Equal;
        let retained_outputs = if published {
            vec![hotfolder_output(
                NativeHotFolderOutputKind::Final,
                &stage.native,
                stage.state,
            )]
        } else {
            match delete_owned_hotfolder_stage(&stage.native, cancellation) {
                Ok(()) => Vec::new(),
                Err(_) => vec![hotfolder_output(
                    NativeHotFolderOutputKind::OwnedStage,
                    &stage.native,
                    capture_hotfolder_guard(&stage.native, &ScanCancellation::new())
                        .unwrap_or(stage.state),
                )],
            }
        };
        return Err(map_hotfolder_io_error(
            error,
            NativeHotFolderFailureKind::Io,
            "verified output could not be published without replacement",
        )
        .with_phase(if published {
            NativeHotFolderPhase::Published
        } else {
            NativeHotFolderPhase::Staging
        })
        .with_retained_outputs(retained_outputs));
    }
    let output = hotfolder_output(
        NativeHotFolderOutputKind::Final,
        &stage.native,
        staged_state,
    );
    if progress(NativeHotFolderProgress {
        phase: NativeHotFolderPhase::Published,
        outputs: vec![output.clone()],
    })
    .is_err()
    {
        return Ok(NativeHotFolderResult {
            source_identity: source_state.identity,
            outputs: vec![output],
            phase: NativeHotFolderPhase::Published,
            partial: true,
            reason: Some("published copy could not be durably recorded; source was retained"),
        });
    }
    let output_state = match capture_hotfolder_guard(&stage.native, &ScanCancellation::new()) {
        Ok(state) => state,
        Err(_) => {
            return Ok(NativeHotFolderResult {
                source_identity: source_state.identity,
                outputs: vec![output],
                phase: NativeHotFolderPhase::Published,
                partial: true,
                reason: Some("published output could not be revalidated; source was retained"),
            });
        }
    };
    let output = hotfolder_output(
        NativeHotFolderOutputKind::Final,
        &stage.native,
        output_state,
    );
    Ok(NativeHotFolderResult {
        source_identity: source_state.identity,
        outputs: vec![output],
        phase: NativeHotFolderPhase::Complete,
        partial: false,
        reason: None,
    })
}

#[cfg(windows)]
fn execute_hotfolder_cross_volume_move(
    source: &GuardedReplaceFile,
    destination: &NativeWatchRoot,
    target_leaf: &OsStr,
    source_state: GuardedReplaceState,
    operation_id: u128,
    progress: &mut dyn FnMut(NativeHotFolderProgress) -> std::io::Result<()>,
    cancellation: &ScanCancellation,
) -> Result<NativeHotFolderResult, NativeHotFolderError> {
    use std::cell::{Cell, RefCell};

    let retained_stage = RefCell::new(None::<GuardedReplaceStage>);
    let published_revalidated = Cell::new(false);
    let progress = RefCell::new(progress);
    let outcome = execute_cross_volume_move_protocol(
        || {
            let stage = create_hotfolder_output_stage(
                &source.native,
                destination,
                target_leaf,
                source_state,
                operation_id,
                &mut **progress.borrow_mut(),
                cancellation,
            )?;
            retained_stage.replace(Some(stage));
            Ok(())
        },
        |_| {
            let mut retained = retained_stage.borrow_mut();
            let stage = retained.as_mut().ok_or_else(|| {
                native_hotfolder_error(
                    NativeHotFolderFailureKind::Io,
                    "verified output stage was lost before publication",
                    Vec::new(),
                )
            })?;
            let staged_state = stage.state;
            let stage_path_before_publish = stage.native.current_path.clone();
            if let Err(error) = rename_hotfolder_guard_to_leaf(
                &mut stage.native,
                target_leaf,
                staged_state,
                cancellation,
            ) {
                let published = batch_rename::compare_paths_windows_ordinal(
                    &stage.native.current_path,
                    &stage_path_before_publish,
                ) != std::cmp::Ordering::Equal;
                let retained_outputs = if published {
                    vec![hotfolder_output(
                        NativeHotFolderOutputKind::Final,
                        &stage.native,
                        staged_state,
                    )]
                } else {
                    match delete_owned_hotfolder_stage(&stage.native, cancellation) {
                        Ok(()) => Vec::new(),
                        Err(_) => vec![hotfolder_output(
                            NativeHotFolderOutputKind::OwnedStage,
                            &stage.native,
                            capture_hotfolder_guard(&stage.native, &ScanCancellation::new())
                                .unwrap_or(staged_state),
                        )],
                    }
                };
                return Err(map_hotfolder_io_error(
                    error,
                    NativeHotFolderFailureKind::Io,
                    "verified output could not be published without replacement",
                )
                .with_phase(if published {
                    NativeHotFolderPhase::Published
                } else {
                    NativeHotFolderPhase::Staging
                })
                .with_retained_outputs(retained_outputs));
            }
            published_revalidated
                .set(verify_published_hotfolder_guard(&stage.native, staged_state).is_ok());
            stage.state = staged_state;
            Ok(hotfolder_output(
                NativeHotFolderOutputKind::Final,
                &stage.native,
                staged_state,
            ))
        },
        |_, _| {
            if !published_revalidated.get() {
                return false;
            }
            let retained = retained_stage.borrow();
            let Some(stage) = retained.as_ref() else {
                return false;
            };
            verify_hotfolder_guard(&source.native, source_state, cancellation)
                .and_then(|()| verify_hotfolder_guard(&stage.native, stage.state, cancellation))
                .is_ok()
        },
        || cancellation.is_cancelled(),
        |_, output| {
            let mut recorder = progress.borrow_mut();
            (*recorder)(NativeHotFolderProgress {
                phase: NativeHotFolderPhase::Published,
                outputs: vec![output.clone()],
            })
            .is_ok()
        },
        |_, output| {
            let mut recorder = progress.borrow_mut();
            (*recorder)(NativeHotFolderProgress {
                phase: NativeHotFolderPhase::SourceRemoval,
                outputs: vec![output.clone()],
            })
            .is_ok()
        },
        || delete_hotfolder_guarded(&source.native, source_state, cancellation).is_ok(),
    )?;

    let (output, partial, reason) = match outcome {
        CrossVolumeMoveOutcome::Complete(output) => (output, false, None),
        CrossVolumeMoveOutcome::Partial { output, reason } => (output, true, Some(reason)),
    };
    Ok(NativeHotFolderResult {
        source_identity: source_state.identity,
        outputs: vec![output],
        phase: if partial {
            NativeHotFolderPhase::SourceRemoval
        } else {
            NativeHotFolderPhase::Complete
        },
        partial,
        reason,
    })
}

#[cfg(windows)]
impl NativeHotFolderError {
    fn with_phase(mut self, phase: NativeHotFolderPhase) -> Self {
        self.phase = phase;
        self
    }

    fn with_retained_outputs(mut self, retained_outputs: Vec<NativeHotFolderOutput>) -> Self {
        self.retained_outputs = retained_outputs;
        self
    }
}

#[cfg(windows)]
fn map_hotfolder_io_error(
    error: std::io::Error,
    fallback: NativeHotFolderFailureKind,
    fallback_reason: &'static str,
) -> NativeHotFolderError {
    if rename_error_is_destination_collision(&error) {
        native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationCollision,
            "destination already exists; no file was overwritten",
            Vec::new(),
        )
    } else if error.kind() == std::io::ErrorKind::Interrupted {
        native_hotfolder_error(
            NativeHotFolderFailureKind::Cancelled,
            "file operation cancelled at a safe boundary",
            Vec::new(),
        )
    } else if error.kind() == std::io::ErrorKind::Unsupported {
        native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "filesystem metadata or content fidelity is unsupported",
            Vec::new(),
        )
    } else if error.kind() == std::io::ErrorKind::PermissionDenied {
        let mut mapped = native_hotfolder_error(
            NativeHotFolderFailureKind::Inaccessible,
            "file or directory access was denied",
            Vec::new(),
        );
        mapped.phase = if fallback == NativeHotFolderFailureKind::StaleSource {
            NativeHotFolderPhase::Admission
        } else {
            NativeHotFolderPhase::Staging
        };
        mapped
    } else {
        native_hotfolder_error(fallback, fallback_reason, Vec::new())
    }
}

#[cfg(windows)]
fn capture_hotfolder_guard(
    guard: &GuardedReplaceHandle,
    cancellation: &ScanCancellation,
) -> std::io::Result<GuardedReplaceState> {
    guard.verify_location(&guard.current_path)?;
    let before = guard.file.metadata()?;
    if before.len() > MAX_RENAME_HASH_BYTES_PER_FILE
        || native_file_identity(&guard.file)? != guard.identity
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder file identity or length is outside the verified state",
        ));
    }
    let sha256 = hash_file_contents(
        &guard.file,
        ContentVerificationMode::Cooperative(cancellation),
    )?;
    let after = guard.file.metadata()?;
    if after.len() != before.len()
        || after.modified().ok() != before.modified().ok()
        || native_file_identity(&guard.file)? != guard.identity
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder file changed while its content was verified",
        ));
    }
    guard.verify_location(&guard.current_path)?;
    Ok(GuardedReplaceState {
        identity: guard.identity,
        length: after.len(),
        modified: after.modified()?,
        content_sha256: sha256,
    })
}

#[cfg(windows)]
fn verify_published_hotfolder_guard(
    guard: &GuardedReplaceHandle,
    expected: GuardedReplaceState,
) -> std::io::Result<()> {
    guard.verify_location(&guard.current_path)?;
    let metadata = guard.file.metadata()?;
    if native_file_identity(&guard.file)? != expected.identity
        || metadata.len() != expected.length
        || metadata.modified()? != expected.modified
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "published Hot Folder file identity or metadata changed",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn verify_hotfolder_guard(
    guard: &GuardedReplaceHandle,
    expected: GuardedReplaceState,
    cancellation: &ScanCancellation,
) -> std::io::Result<()> {
    let actual = capture_hotfolder_guard(guard, cancellation)?;
    if actual != expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder file identity, path, metadata or content changed",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn hotfolder_output(
    kind: NativeHotFolderOutputKind,
    guard: &GuardedReplaceHandle,
    state: GuardedReplaceState,
) -> NativeHotFolderOutput {
    NativeHotFolderOutput {
        kind,
        path: guard.current_path.clone(),
        identity: state.identity,
        length: state.length,
        modified: Some(state.modified),
        sha256: Some(state.content_sha256),
    }
}

#[cfg(windows)]
fn rename_hotfolder_guard_to_leaf(
    guard: &mut GuardedReplaceHandle,
    leaf: &OsStr,
    expected: GuardedReplaceState,
    cancellation: &ScanCancellation,
) -> std::io::Result<PathBuf> {
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Hot Folder operation cancelled before rename",
        ));
    }
    validate_replace_leaf(leaf)?;
    verify_hotfolder_guard(guard, expected, cancellation)?;
    let (root_guards, parent_guards) = guard.reopen_guards()?;
    let parent = parent_guards
        .last()
        .ok_or_else(|| std::io::Error::other("Hot Folder source parent guard is missing"))?;
    rename_open_file_no_replace(&guard.file, parent, leaf)?;
    guard.current_path = parent.canonical_path.join(leaf);
    guard.root_guards = root_guards;
    guard.parent_guards = parent_guards;
    Ok(guard.current_path.clone())
}

#[cfg(windows)]
fn rename_hotfolder_guard_to_root(
    guard: &mut GuardedReplaceHandle,
    destination: &NativeWatchRoot,
    leaf: &OsStr,
    expected: GuardedReplaceState,
    cancellation: &ScanCancellation,
) -> std::io::Result<PathBuf> {
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Hot Folder operation cancelled before move",
        ));
    }
    validate_replace_leaf(leaf)?;
    verify_hotfolder_guard(guard, expected, cancellation)?;
    let (source_root_guards, source_parent_guards) = guard.reopen_guards()?;
    let destination = refresh_watch_root(destination)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    if guard.root.identity.volume_serial_number() != destination.identity().volume_serial_number() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "cross-volume move must use verified copy, publish and source removal",
        ));
    }
    let destination_guards = NativeRenameScanBackend
        .reopen_chain(
            &destination.proof.chain,
            &destination.proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(|error| std::io::Error::other(error.message))?;
    let Some(parent) = destination_guards.last() else {
        return Err(std::io::Error::other(
            "Hot Folder destination guard is missing",
        ));
    };
    if parent.identity != destination.identity() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder destination identity changed before move",
        ));
    }
    // Keep both source and destination ancestor chains alive through the
    // handle-based no-replace rename.
    let _source_guards = (source_root_guards, source_parent_guards);
    rename_open_file_no_replace(&guard.file, parent, leaf)?;
    guard.current_path = parent.canonical_path.join(leaf);
    guard.parent_path = parent.canonical_path.clone();
    guard.parent_identity = parent.identity;
    guard.root = destination.proof.clone();
    guard.parent_chain = destination.proof.chain.clone();
    guard.root_guards = destination_guards.clone();
    guard.parent_guards = destination_guards;
    Ok(guard.current_path.clone())
}

#[cfg(windows)]
fn create_hotfolder_output_stage(
    source: &GuardedReplaceHandle,
    destination: &NativeWatchRoot,
    final_leaf: &OsStr,
    source_state: GuardedReplaceState,
    operation_id: u128,
    progress: &mut dyn FnMut(NativeHotFolderProgress) -> std::io::Result<()>,
    cancellation: &ScanCancellation,
) -> Result<GuardedReplaceStage, NativeHotFolderError> {
    validate_replace_leaf(final_leaf).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "destination filename is not one safe path component",
            Vec::new(),
        )
    })?;
    verify_hotfolder_guard(source, source_state, cancellation).map_err(|error| {
        map_hotfolder_io_error(
            error,
            NativeHotFolderFailureKind::StaleSource,
            "source changed before output staging",
        )
    })?;
    ensure_unnamed_stream_only(&source.file).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::UnsupportedFidelity,
            "source has named streams or its stream set cannot be verified",
            Vec::new(),
        )
    })?;
    let destination = refresh_watch_root(destination).map_err(|_| {
        native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationUnavailable,
            "destination directory identity changed before staging",
            Vec::new(),
        )
    })?;
    let destination_guards = NativeRenameScanBackend
        .reopen_chain(
            &destination.proof.chain,
            &destination.proof,
            DirectoryOpenAccess::Metadata,
        )
        .map_err(|_| {
            native_hotfolder_error(
                NativeHotFolderFailureKind::DestinationUnavailable,
                "destination directory could not be held during staging",
                Vec::new(),
            )
        })?;
    let Some(parent) = destination_guards.last() else {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationUnavailable,
            "destination directory evidence is unavailable",
            Vec::new(),
        ));
    };
    if parent.identity != destination.identity() {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationUnavailable,
            "destination directory identity changed before staging",
            Vec::new(),
        ));
    }
    let mut created = None;
    for attempt in 0..8_u8 {
        if cancellation.is_cancelled() {
            return Err(native_hotfolder_error(
                NativeHotFolderFailureKind::Cancelled,
                "file operation cancelled before output staging",
                Vec::new(),
            ));
        }
        let nonce = if attempt == 0 {
            operation_id
        } else {
            rand::random::<u128>()
        };
        let stage_leaf = format!(".ml-hotfolder-{nonce:032x}.stage");
        let stage_path = parent.canonical_path.join(&stage_leaf);
        match create_native_replace_stage(&stage_path) {
            Ok(file) => {
                created = Some((stage_path, stage_leaf, file));
                break;
            }
            Err(error) if rename_error_is_destination_collision(&error) => continue,
            Err(error) => {
                return Err(map_hotfolder_io_error(
                    error,
                    NativeHotFolderFailureKind::Io,
                    "unique output stage could not be created",
                ));
            }
        }
    }
    let Some((stage_path, _stage_leaf, output_file)) = created else {
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::DestinationCollision,
            "unique output stage names were occupied",
            Vec::new(),
        ));
    };
    let identity = native_file_identity(&output_file).map_err(|_| {
        let _ = delete_open_file(&output_file);
        native_hotfolder_error(
            NativeHotFolderFailureKind::Io,
            "new output stage identity could not be verified",
            Vec::new(),
        )
    })?;
    let native = GuardedReplaceHandle {
        current_path: stage_path,
        parent_path: parent.canonical_path.clone(),
        parent_identity: parent.identity,
        identity,
        root: destination.proof.clone(),
        parent_chain: destination.proof.chain.clone(),
        file: output_file,
        root_guards: destination_guards.clone(),
        parent_guards: destination_guards,
    };
    let mut stage = GuardedReplaceStage {
        state: GuardedReplaceState {
            identity,
            length: 0,
            modified: SystemTime::UNIX_EPOCH,
            content_sha256: Sha256::digest([]).into(),
        },
        native,
    };
    if progress(NativeHotFolderProgress {
        phase: NativeHotFolderPhase::Staging,
        outputs: vec![owned_stage_output(&stage.native, Some(stage.state))],
    })
    .is_err()
    {
        let cleanup = delete_owned_hotfolder_stage(&stage.native, &ScanCancellation::new());
        let retained_outputs = if cleanup.is_err() {
            vec![owned_stage_output(&stage.native, None)]
        } else {
            Vec::new()
        };
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::Io,
            "owned stage could not be durably recorded before copying",
            retained_outputs,
        ));
    }
    let initialized = initialize_hotfolder_copy_stage(
        &source.file,
        &stage.native.file,
        source_state,
        cancellation,
    );
    let copied = match initialized {
        Ok(state) => state,
        Err(error) => {
            let cleanup = delete_owned_hotfolder_stage(&stage.native, &ScanCancellation::new());
            let retained_outputs = if cleanup.is_err() {
                vec![owned_stage_output(&stage.native, None)]
            } else {
                Vec::new()
            };
            return Err(map_hotfolder_io_error(
                error,
                NativeHotFolderFailureKind::Io,
                "output stage failed verification or fidelity checks",
            )
            .with_retained_outputs(retained_outputs));
        }
    };
    stage.state = copied;
    if progress(NativeHotFolderProgress {
        phase: NativeHotFolderPhase::Staging,
        outputs: vec![owned_stage_output(&stage.native, Some(stage.state))],
    })
    .is_err()
    {
        let cleanup = delete_owned_hotfolder_stage(&stage.native, &ScanCancellation::new());
        let retained_outputs = if cleanup.is_err() {
            vec![owned_stage_output(&stage.native, Some(stage.state))]
        } else {
            Vec::new()
        };
        return Err(native_hotfolder_error(
            NativeHotFolderFailureKind::Io,
            "verified stage could not be durably recorded before publication",
            retained_outputs,
        ));
    }
    Ok(stage)
}

#[cfg(windows)]
fn initialize_hotfolder_copy_stage(
    source: &std::fs::File,
    output: &std::fs::File,
    expected: GuardedReplaceState,
    cancellation: &ScanCancellation,
) -> std::io::Result<GuardedReplaceState> {
    use std::io::{Read, Seek, SeekFrom, Write};

    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Hot Folder operation cancelled before copying bytes",
        ));
    }
    let source_before = source.metadata()?;
    if source_before.len() != expected.length
        || source_before.modified()? != expected.modified
        || native_file_identity(source)? != expected.identity
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder source changed before its copy began",
        ));
    }
    let source_properties = query_replace_basic_info(source)?;
    validate_replace_basic_attributes(source_properties.FileAttributes)?;
    let mut reader = source.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut writer = output.try_clone()?;
    writer.set_len(0)?;
    writer.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    while total < expected.length {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "Hot Folder operation cancelled while copying bytes",
            ));
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Hot Folder source ended before the captured length",
            ));
        }
        total = total.checked_add(count as u64).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Hot Folder byte count overflowed",
            )
        })?;
        if total > expected.length || total > MAX_RENAME_HASH_BYTES_PER_FILE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Hot Folder source grew beyond the verified copy limit",
            ));
        }
        writer.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    if reader.read(&mut buffer[..1])? != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder source grew while its copy was staged",
        ));
    }
    let source_hash: [u8; 32] = hasher.finalize().into();
    if source_hash != expected.content_sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder source content changed while its copy was staged",
        ));
    }
    writer.flush()?;
    writer.set_len(expected.length)?;
    writer.sync_all()?;
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "Hot Folder operation cancelled after staging bytes",
        ));
    }
    copy_replace_security(source, output)?;
    let staged_properties = query_replace_basic_info(output)?;
    let mut properties = source_properties;
    properties.LastWriteTime = system_time_to_windows_ticks(expected.modified)?;
    properties.ChangeTime = 0;
    set_replace_basic_info(output, properties)?;
    output.sync_all()?;

    let source_after = source.metadata()?;
    if source_after.len() != expected.length
        || source_after.modified()? != expected.modified
        || native_file_identity(source)? != expected.identity
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder source changed before the staged copy completed",
        ));
    }
    let output_metadata = output.metadata()?;
    let output_identity = native_file_identity(output)?;
    let output_hash =
        hash_file_contents(output, ContentVerificationMode::Cooperative(cancellation))?;
    if output_metadata.len() != expected.length
        || output_metadata.modified()? != expected.modified
        || output_hash != expected.content_sha256
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder staged copy did not preserve verified bytes and metadata",
        ));
    }
    let _ = staged_properties;
    Ok(GuardedReplaceState {
        identity: output_identity,
        length: expected.length,
        modified: output_metadata.modified()?,
        content_sha256: output_hash,
    })
}

#[cfg(windows)]
fn delete_owned_hotfolder_stage(
    guard: &GuardedReplaceHandle,
    cancellation: &ScanCancellation,
) -> std::io::Result<()> {
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "stage cleanup cancelled",
        ));
    }
    guard.verify_location(&guard.current_path)?;
    if native_file_identity(&guard.file)? != guard.identity {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Hot Folder stage identity changed before cleanup",
        ));
    }
    delete_open_file(&guard.file)
}

#[cfg(windows)]
fn owned_stage_output(
    guard: &GuardedReplaceHandle,
    state: Option<GuardedReplaceState>,
) -> NativeHotFolderOutput {
    let metadata = guard.file.metadata().ok();
    NativeHotFolderOutput {
        kind: NativeHotFolderOutputKind::OwnedStage,
        path: guard.current_path.clone(),
        identity: guard.identity,
        length: state.map_or_else(
            || metadata.as_ref().map_or(0, std::fs::Metadata::len),
            |state| state.length,
        ),
        modified: state
            .map(|state| state.modified)
            .or_else(|| metadata.and_then(|metadata| metadata.modified().ok())),
        sha256: state.map(|state| state.content_sha256),
    }
}

#[cfg(windows)]
fn delete_hotfolder_guarded(
    guard: &GuardedReplaceHandle,
    expected: GuardedReplaceState,
    cancellation: &ScanCancellation,
) -> std::io::Result<()> {
    verify_hotfolder_guard(guard, expected, cancellation)?;
    delete_open_file(&guard.file)
}

impl GuardedReplaceStage {
    pub(crate) fn path(&self) -> &Path {
        #[cfg(windows)]
        return &self.native.current_path;
        #[cfg(not(windows))]
        Path::new("")
    }

    pub(crate) fn state(&self) -> GuardedReplaceState {
        self.state
    }

    pub(crate) fn verify_at(
        &self,
        expected_path: &Path,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<GuardedReplaceState> {
        #[cfg(windows)]
        return self.native.verify_at(
            expected_path,
            self.state.length,
            Some(self.state.modified),
            self.state.content_sha256,
            None,
            cancellation,
        );
        #[cfg(not(windows))]
        {
            let _ = (expected_path, cancellation);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement writes are available only on Windows",
            ))
        }
    }

    pub(crate) fn rename_to_leaf(
        &mut self,
        leaf: &OsStr,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<PathBuf> {
        #[cfg(windows)]
        return self.native.rename_to_leaf(leaf, self.state, cancellation);
        #[cfg(not(windows))]
        {
            let _ = (leaf, cancellation);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement writes are available only on Windows",
            ))
        }
    }

    pub(crate) fn delete_if_matches(&self, cancellation: &ScanCancellation) -> std::io::Result<()> {
        #[cfg(windows)]
        return self.native.delete_if_matches(self.state, cancellation);
        #[cfg(not(windows))]
        {
            let _ = cancellation;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "safe replacement cleanup is available only on Windows",
            ))
        }
    }
}

#[cfg(windows)]
impl GuardedReplaceHandle {
    fn capture_current_state(&self) -> std::io::Result<GuardedReplaceState> {
        self.verify_location(&self.current_path)?;
        let metadata = self.file.metadata()?;
        let bytes = read_handle_bytes(&self.file, metadata.len(), &ScanCancellation::new())?;
        Ok(GuardedReplaceState {
            identity: self.identity,
            length: metadata.len(),
            modified: metadata.modified()?,
            content_sha256: Sha256::digest(&bytes).into(),
        })
    }

    fn verify_at(
        &self,
        expected_path: &Path,
        expected_length: u64,
        expected_modified: Option<SystemTime>,
        expected_sha256: [u8; 32],
        expected_bytes: Option<&[u8]>,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<GuardedReplaceState> {
        self.verify_location(expected_path)?;
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement verification was cancelled",
            ));
        }
        let metadata = self.file.metadata()?;
        if metadata.len() != expected_length
            || expected_modified.is_some_and(|modified| metadata.modified().ok() != Some(modified))
            || native_file_identity(&self.file)? != self.identity
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement file identity, length, or metadata changed",
            ));
        }
        let bytes = read_handle_bytes(&self.file, expected_length, cancellation)?;
        let sha256: [u8; 32] = Sha256::digest(&bytes).into();
        if sha256 != expected_sha256
            || expected_bytes.is_some_and(|expected| expected != bytes.as_slice())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement file content changed",
            ));
        }
        let metadata_after = self.file.metadata()?;
        if metadata_after.len() != expected_length
            || expected_modified
                .is_some_and(|modified| metadata_after.modified().ok() != Some(modified))
            || native_file_identity(&self.file)? != self.identity
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement file changed while it was verified",
            ));
        }
        Ok(GuardedReplaceState {
            identity: self.identity,
            length: expected_length,
            modified: metadata_after.modified()?,
            content_sha256: sha256,
        })
    }

    fn rename_to_leaf(
        &mut self,
        leaf: &OsStr,
        expected_state: GuardedReplaceState,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<PathBuf> {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement was cancelled before its next rename",
            ));
        }
        validate_replace_leaf(leaf)?;
        if expected_state.identity != self.identity {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "replacement rename state has a different file identity",
            ));
        }
        self.verify_at(
            &self.current_path,
            expected_state.length,
            Some(expected_state.modified),
            expected_state.content_sha256,
            None,
            cancellation,
        )?;
        let (root_guards, parent_guards) = self.reopen_guards()?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| std::io::Error::other("replacement parent guard is missing"))?;
        rename_open_file_no_replace(&self.file, parent, leaf)?;
        self.current_path = parent.canonical_path.join(leaf);
        self.root_guards = root_guards;
        self.parent_guards = parent_guards;
        self.verify_at(
            &self.current_path,
            expected_state.length,
            Some(expected_state.modified),
            expected_state.content_sha256,
            None,
            cancellation,
        )?;
        Ok(self.current_path.clone())
    }

    fn delete_if_matches(
        &self,
        expected_state: GuardedReplaceState,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<()> {
        self.verify_at(
            &self.current_path,
            expected_state.length,
            Some(expected_state.modified),
            expected_state.content_sha256,
            None,
            cancellation,
        )?;
        let (_root_guards, parent_guards) = self.reopen_guards()?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| std::io::Error::other("replacement parent guard is missing"))?;
        if parent.identity != self.parent_identity {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement stage parent changed before cleanup",
            ));
        }
        delete_open_file(&self.file)
    }

    fn create_output_stage(
        &self,
        leaf: &OsStr,
        bytes: &[u8],
        sha256: [u8; 32],
        modified: Option<SystemTime>,
        cancellation: &ScanCancellation,
    ) -> std::io::Result<GuardedReplaceStage> {
        validate_replace_leaf(leaf)?;
        let actual_sha256: [u8; 32] = Sha256::digest(bytes).into();
        if actual_sha256 != sha256 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "replacement output bytes do not match their confirmed hash",
            ));
        }
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement was cancelled before staging output bytes",
            ));
        }
        let (root_guards, parent_guards) = self.reopen_guards()?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| std::io::Error::other("replacement parent guard is missing"))?;
        let path = parent.canonical_path.join(leaf);
        let output = create_native_replace_stage(&path)?;
        let identity = match native_file_identity(&output) {
            Ok(identity) => identity,
            Err(error) => {
                let cleanup = delete_open_file(&output);
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(std::io::Error::other(format!(
                        "could not identify new replacement stage at {} ({error}); the CREATE_NEW stage remains ({cleanup})",
                        path.display()
                    ))),
                };
            }
        };
        let native = GuardedReplaceHandle {
            current_path: path,
            parent_path: parent.canonical_path.clone(),
            parent_identity: parent.identity,
            identity,
            root: self.root.clone(),
            parent_chain: self.parent_chain.clone(),
            file: output,
            root_guards,
            parent_guards,
        };
        let mut stage = GuardedReplaceStage {
            state: GuardedReplaceState {
                identity,
                length: 0,
                modified: SystemTime::UNIX_EPOCH,
                content_sha256: Sha256::digest([]).into(),
            },
            native,
        };
        if let Err(error) = initialize_replace_stage(
            &self.file,
            &stage.native.file,
            bytes,
            modified,
            cancellation,
        ) {
            match stage.native.capture_current_state() {
                Ok(state) => {
                    stage.state = state;
                    if let Err(cleanup) = stage.delete_if_matches(&ScanCancellation::new()) {
                        return Err(std::io::Error::other(format!(
                            "could not prepare replacement stage ({error}); its owned stage remains at {} ({cleanup})",
                            stage.path().display()
                        )));
                    }
                }
                Err(cleanup) => {
                    return Err(std::io::Error::other(format!(
                        "could not prepare replacement stage ({error}); its owned stage remains at {} and could not be verified for cleanup ({cleanup})",
                        stage.path().display()
                    )));
                }
            }
            return Err(error);
        }
        let state = stage.native.verify_at(
            stage.path(),
            bytes.len() as u64,
            modified,
            sha256,
            Some(bytes),
            &ScanCancellation::new(),
        )?;
        stage.state = state;
        Ok(stage)
    }

    fn verify_location(&self, expected_path: &Path) -> std::io::Result<()> {
        let (root_guards, parent_guards) = self.reopen_guards()?;
        let root = root_guards
            .last()
            .ok_or_else(|| std::io::Error::other("replacement root guard is missing"))?;
        let parent = parent_guards
            .last()
            .ok_or_else(|| std::io::Error::other("replacement parent guard is missing"))?;
        let source_path = final_path_for_handle(&self.file)?;
        if root.identity != self.root.identity
            || parent.identity != self.parent_identity
            || batch_rename::compare_paths_windows_ordinal(
                &parent.canonical_path,
                &self.parent_path,
            ) != std::cmp::Ordering::Equal
            || native_file_identity(&self.file)? != self.identity
            || batch_rename::compare_paths_windows_ordinal(&source_path, expected_path)
                != std::cmp::Ordering::Equal
            || source_path.file_name() != expected_path.file_name()
            || source_path.parent().is_none_or(|actual_parent| {
                batch_rename::compare_paths_windows_ordinal(actual_parent, &self.parent_path)
                    != std::cmp::Ordering::Equal
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement root, parent, or file identity changed",
            ));
        }
        Ok(())
    }

    fn reopen_guards(
        &self,
    ) -> std::io::Result<(
        OpenDirectoryChain<std::fs::File>,
        OpenDirectoryChain<std::fs::File>,
    )> {
        let backend = NativeRenameScanBackend;
        let root_guards = backend
            .reopen_chain(&self.root.chain, &self.root, DirectoryOpenAccess::Enumerate)
            .map_err(|error| std::io::Error::other(error.message))?;
        let parent_guards = backend
            .reopen_chain(
                &self.parent_chain,
                &self.root,
                DirectoryOpenAccess::Enumerate,
            )
            .map_err(|error| std::io::Error::other(error.message))?;
        if root_guards
            .last()
            .is_none_or(|root| root.identity != self.root.identity)
            || parent_guards.last().is_none_or(|parent| {
                parent.identity != self.parent_identity
                    || batch_rename::compare_paths_windows_ordinal(
                        &parent.canonical_path,
                        &self.parent_path,
                    ) != std::cmp::Ordering::Equal
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement root or full parent chain changed",
            ));
        }
        Ok((root_guards, parent_guards))
    }
}

#[cfg(windows)]
fn create_verified_replace_backup_native(
    path: &Path,
    bytes: &[u8],
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> std::io::Result<VerifiedReplaceBackup> {
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, FILE_WRITE_DATA,
    };
    use windows::core::PCWSTR;

    validate_replace_leaf(path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "backup path has no filename",
        )
    })?)?;
    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_DATA.0 | FILE_WRITE_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .map_err(windows_error_to_io)?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes()
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
            != 0
        || batch_rename::compare_paths_windows_ordinal(&final_path_for_handle(&file)?, path)
            != std::cmp::Ordering::Equal
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "CREATE_NEW replacement backup did not resolve to its exact ordinary file path",
        ));
    }
    for chunk in bytes.chunks(64 * 1024) {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement was cancelled while writing its durable backup",
            ));
        }
        (&file).write_all(chunk)?;
    }
    file.sync_all()?;
    let identity = native_file_identity(&file)?;
    let verified = read_handle_bytes(&file, bytes.len() as u64, cancellation)?;
    let content_sha256: [u8; 32] = Sha256::digest(&verified).into();
    if verified.as_slice() != bytes || content_sha256 != expected_sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "durable replacement backup failed exact-byte verification",
        ));
    }
    Ok(VerifiedReplaceBackup {
        path: path.to_path_buf(),
        state: GuardedReplaceState {
            identity,
            length: bytes.len() as u64,
            modified: file.metadata()?.modified()?,
            content_sha256,
        },
        bytes: Arc::from(verified),
        file,
    })
}

#[cfg(windows)]
fn open_verified_replace_backup_native(
    path: &Path,
    expected_identity: NativeFileIdentity,
    expected_length: u64,
    expected_sha256: [u8; 32],
    cancellation: &ScanCancellation,
) -> std::io::Result<VerifiedReplaceBackup> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    validate_replace_leaf(path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "backup path has no filename",
        )
    })?)?;
    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .map_err(windows_error_to_io)?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || native_file_identity(&file)? != expected_identity
        || metadata.len() != expected_length
        || batch_rename::compare_paths_windows_ordinal(&final_path_for_handle(&file)?, path)
            != std::cmp::Ordering::Equal
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement backup identity, type, length, or path changed",
        ));
    }
    let bytes = read_handle_bytes(&file, expected_length, cancellation)?;
    let content_sha256: [u8; 32] = Sha256::digest(&bytes).into();
    if content_sha256 != expected_sha256 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement backup content changed",
        ));
    }
    Ok(VerifiedReplaceBackup {
        path: path.to_path_buf(),
        state: GuardedReplaceState {
            identity: expected_identity,
            length: expected_length,
            modified: metadata.modified()?,
            content_sha256,
        },
        bytes: Arc::from(bytes),
        file,
    })
}

#[cfg(windows)]
fn validate_replace_leaf(leaf: &OsStr) -> std::io::Result<()> {
    let path = Path::new(leaf);
    if leaf.is_empty()
        || leaf.to_string_lossy().contains(':')
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "replacement stage name must be one ordinary path component",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn create_native_replace_stage(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ, FILE_WRITE_ATTRIBUTES,
        FILE_WRITE_DATA, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let desired_access = DELETE.0
        | FILE_READ_DATA.0
        | FILE_WRITE_DATA.0
        | FILE_READ_ATTRIBUTES.0
        | FILE_WRITE_ATTRIBUTES.0
        | READ_CONTROL.0
        | WRITE_DAC.0
        | WRITE_OWNER.0;
    // CREATE_NEW ensures a collision is never opened or truncated. The stage
    // handle allows reading, writing, security/property application, and a
    // handle-bound no-replace rename, while denying other writers/deleters.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            desired_access,
            FILE_SHARE_READ,
            None,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(CREATE_NEW replacement stage)",
            windows_error_to_io(error),
        )
    })?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file.metadata()?;
    let attributes = metadata.file_attributes();
    if attributes & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0) != 0
        || !metadata.is_file()
    {
        let cleanup = delete_open_file(&file);
        return Err(std::io::Error::other(match cleanup {
            Ok(()) => "CREATE_NEW replacement stage is not a regular file".to_owned(),
            Err(error) => format!(
                "CREATE_NEW replacement stage is not a regular file and could not be removed ({error})"
            ),
        }));
    }
    Ok(file)
}

#[cfg(windows)]
fn initialize_replace_stage(
    source: &std::fs::File,
    output: &std::fs::File,
    bytes: &[u8],
    modified: Option<SystemTime>,
    cancellation: &ScanCancellation,
) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom, Write};

    let source_properties = query_replace_basic_info(source)?;
    validate_replace_basic_attributes(source_properties.FileAttributes)?;
    if bytes.len() as u64 > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "replacement output exceeds the content staging limit",
        ));
    }
    let mut writer = output.try_clone()?;
    writer.set_len(0)?;
    writer.seek(SeekFrom::Start(0))?;
    for chunk in bytes.chunks(64 * 1024) {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement was cancelled while staging output bytes",
            ));
        }
        writer.write_all(chunk)?;
    }
    writer.flush()?;
    writer.set_len(bytes.len() as u64)?;
    writer.sync_all()?;
    if cancellation.is_cancelled() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "replacement was cancelled after staging output bytes",
        ));
    }

    // Supported file security: owner, primary group, DACL, and DACL-protected
    // state are copied by handle. Failure is returned before source relocation.
    copy_replace_security(source, output)?;

    // Preserve creation/access times and supported basic flags. Apply/restore
    // supplies an exact source write time; ordinary apply keeps the stage's
    // freshly generated write time. Sparse, compressed, encrypted, reparse,
    // readonly, and other special-property files are refused above.
    let staged = query_replace_basic_info(output)?;
    let mut properties = source_properties;
    if let Some(modified) = modified {
        properties.LastWriteTime = system_time_to_windows_ticks(modified)?;
    } else {
        properties.LastWriteTime = staged.LastWriteTime;
    }
    properties.ChangeTime = 0;
    set_replace_basic_info(output, properties)?;
    output.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn validate_replace_basic_attributes(attributes: u32) -> std::io::Result<()> {
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_SYSTEM,
    };

    let supported = FILE_ATTRIBUTE_ARCHIVE.0
        | FILE_ATTRIBUTE_HIDDEN.0
        | FILE_ATTRIBUTE_SYSTEM.0
        | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED.0;
    if attributes != FILE_ATTRIBUTE_NORMAL.0
        && (attributes & !supported != 0 || attributes & FILE_ATTRIBUTE_NORMAL.0 != 0)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "replacement refuses a file with unsupported Windows attributes",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn query_replace_basic_info(
    file: &std::fs::File,
) -> std::io::Result<windows::Win32::Storage::FileSystem::FILE_BASIC_INFO> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
    };

    let mut info = FILE_BASIC_INFO::default();
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle() as _),
            FileBasicInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    }
    .map_err(windows_error_to_io)?;
    Ok(info)
}

#[cfg(windows)]
fn set_replace_basic_info(
    file: &std::fs::File,
    info: windows::Win32::Storage::FileSystem::FILE_BASIC_INFO,
) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, SetFileInformationByHandle,
    };

    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle() as _),
            FileBasicInfo,
            std::ptr::from_ref(&info).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    }
    .map_err(windows_error_to_io)
}

#[cfg(windows)]
fn system_time_to_windows_ticks(time: SystemTime) -> std::io::Result<i64> {
    const WINDOWS_EPOCH_OFFSET_SECONDS: u64 = 11_644_473_600;
    let duration = time.duration_since(std::time::UNIX_EPOCH).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pre-epoch file timestamps cannot be preserved safely",
        )
    })?;
    let seconds = duration
        .as_secs()
        .checked_add(WINDOWS_EPOCH_OFFSET_SECONDS)
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "timestamp overflow")
        })?;
    let ticks = seconds
        .checked_mul(10_000_000)
        .and_then(|ticks| ticks.checked_add(u64::from(duration.subsec_nanos() / 100)))
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "timestamp overflow")
        })?;
    i64::try_from(ticks)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "timestamp overflow"))
}

#[cfg(windows)]
struct ReplaceSecurityDescriptor(windows::Win32::Security::PSECURITY_DESCRIPTOR);

#[cfg(windows)]
impl Drop for ReplaceSecurityDescriptor {
    fn drop(&mut self) {
        use windows::Win32::Foundation::HLOCAL;

        if !self.0.is_invalid() {
            unsafe {
                windows::Win32::Foundation::LocalFree(HLOCAL(self.0.0));
            }
        }
    }
}

#[cfg(windows)]
struct ReplaceSecurityParts {
    owner: windows::Win32::Security::PSID,
    group: windows::Win32::Security::PSID,
    dacl: *mut windows::Win32::Security::ACL,
    dacl_present: windows::Win32::Foundation::BOOL,
    dacl_protected: bool,
}

#[cfg(windows)]
fn query_replace_security(
    file: &std::fs::File,
) -> std::io::Result<(ReplaceSecurityDescriptor, ReplaceSecurityParts)> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{BOOL, HANDLE};
    use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        SE_DACL_PROTECTED,
    };

    let handle = HANDLE(file.as_raw_handle() as _);
    let mut owner = PSID::default();
    let mut group = PSID::default();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            Some(&mut group),
            Some(&mut dacl),
            None,
            Some(&mut descriptor),
        )
    };
    if status.0 != 0 {
        return Err(std::io::Error::from_raw_os_error(status.0 as i32));
    }
    let guard = ReplaceSecurityDescriptor(descriptor);
    if descriptor.is_invalid() || owner.is_invalid() || group.is_invalid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "replacement cannot preserve a missing owner or primary group",
        ));
    }
    let mut dacl_present = BOOL(0);
    let mut descriptor_dacl = std::ptr::null_mut();
    let mut defaulted = BOOL(0);
    unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut descriptor_dacl,
            &mut defaulted,
        )
    }
    .map_err(windows_error_to_io)?;
    if dacl_present.0 == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "replacement cannot preserve a file with no DACL",
        ));
    }
    let mut control = 0_u16;
    let mut revision = 0_u32;
    unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }
        .map_err(windows_error_to_io)?;
    let _ = (defaulted, revision);
    Ok((
        guard,
        ReplaceSecurityParts {
            owner,
            group,
            dacl: descriptor_dacl,
            dacl_present,
            dacl_protected: control & SE_DACL_PROTECTED.0 != 0,
        },
    ))
}

#[cfg(windows)]
fn copy_replace_security(source: &std::fs::File, output: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetSecurityInfo};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, UNPROTECTED_DACL_SECURITY_INFORMATION,
    };

    let (_source_descriptor, source_parts) = query_replace_security(source)?;
    let mut security_information =
        OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    security_information |= if source_parts.dacl_protected {
        PROTECTED_DACL_SECURITY_INFORMATION
    } else {
        UNPROTECTED_DACL_SECURITY_INFORMATION
    };
    let status = unsafe {
        SetSecurityInfo(
            HANDLE(output.as_raw_handle() as _),
            SE_FILE_OBJECT,
            security_information,
            source_parts.owner,
            source_parts.group,
            Some(source_parts.dacl.cast_const()),
            None,
        )
    };
    if status.0 != 0 {
        return Err(std::io::Error::from_raw_os_error(status.0 as i32));
    }
    let (_output_descriptor, output_parts) = query_replace_security(output)?;
    let owner_matches =
        unsafe { windows::Win32::Security::EqualSid(source_parts.owner, output_parts.owner) }
            .is_ok();
    let group_matches =
        unsafe { windows::Win32::Security::EqualSid(source_parts.group, output_parts.group) }
            .is_ok();
    let dacl_matches = unsafe { replace_dacl_equal(&source_parts, &output_parts) };
    if !owner_matches
        || !group_matches
        || !dacl_matches
        || source_parts.dacl_present != output_parts.dacl_present
        || source_parts.dacl_protected != output_parts.dacl_protected
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "replacement stage security does not match the source owner, group, or DACL",
        ));
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn replace_dacl_equal(left: &ReplaceSecurityParts, right: &ReplaceSecurityParts) -> bool {
    if left.dacl.is_null() || right.dacl.is_null() {
        return left.dacl.is_null() && right.dacl.is_null();
    }
    let left_length = unsafe { (*left.dacl).AclSize } as usize;
    let right_length = unsafe { (*right.dacl).AclSize } as usize;
    if left_length != right_length
        || left_length < std::mem::size_of::<windows::Win32::Security::ACL>()
    {
        return false;
    }
    let left_bytes = unsafe { std::slice::from_raw_parts(left.dacl.cast::<u8>(), left_length) };
    let right_bytes = unsafe { std::slice::from_raw_parts(right.dacl.cast::<u8>(), right_length) };
    left_bytes == right_bytes
}

#[cfg(windows)]
fn delete_open_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };

    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: BOOLEAN(1),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle() as _),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(windows_error_to_io)
}

/// Open a content file with read access only. Write and delete sharing are
/// withheld while the snapshot is captured, but the handle has no mutation
/// authority and is never returned from the read boundary.
#[cfg(windows)]
fn open_native_readonly_content_file(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ,
        OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    // SAFETY: `path` is a validated local path from a retained native scan
    // proof, encoded as a NUL-terminated UTF-16 string. The handle grants only
    // data and metadata read access and does not follow a final reparse point.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(read-only content snapshot)",
            windows_error_to_io(error),
        )
    })?;
    // SAFETY: CreateFileW returned a valid owned HANDLE transferred once to
    // File, which closes it when this short-lived snapshot operation ends.
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path: final_path_for_handle(&file)
            .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?,
        identity: native_file_identity(&file)
            .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder: false,
        handle: file,
    })
}

/// Open a recorded payload for identity and byte verification without
/// requiring DELETE access. The short-lived handle denies writers and
/// deleters while its fingerprint is checked, but never authorizes mutation.
#[cfg(windows)]
fn open_native_verified_file(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ,
        OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    // SAFETY: the path is a validated local, NUL-terminated UTF-16 path. The
    // handle does not follow a final reparse point and grants read-only access.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(read-only receipt verification)",
            windows_error_to_io(error),
        )
    })?;
    // SAFETY: CreateFileW returned a valid owned HANDLE which is transferred
    // exactly once to File and closed by Drop.
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path: final_path_for_handle(&file)
            .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?,
        identity: native_file_identity(&file)
            .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder: false,
        handle: file,
    })
}

#[cfg(windows)]
fn open_native_occupancy_file(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    // The selected source handle denies delete and write sharing while this
    // read-only handle allows the source's DELETE access to coexist.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "CreateFileW(FILE_READ_ATTRIBUTES occupancy check)",
            windows_error_to_io(error),
        )
    })?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path: final_path_for_handle(&file)
            .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?,
        identity: native_file_identity(&file)
            .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder: false,
        handle: file,
    })
}

#[cfg(windows)]
fn rename_open_file_no_replace(
    source: &std::fs::File,
    parent: &OpenDirectory<std::fs::File>,
    leaf: &OsStr,
) -> std::io::Result<()> {
    use std::mem::{offset_of, size_of};
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{BOOLEAN, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };

    if Path::new(leaf).components().count() != 1
        || !matches!(
            Path::new(leaf).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "handle-based rename requires a single filename component",
        ));
    }
    let destination_path = parent.canonical_path.join(leaf);
    let name = extended_local_path(&destination_path)?;
    if name.is_empty() || name.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination path is empty or contains NUL",
        ));
    }
    let name_bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "filename too long")
        })?;
    let terminated_name_bytes = (name_bytes as usize)
        .checked_add(size_of::<u16>())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "rename buffer too large")
        })?;
    let mut name = name;
    name.push(0);
    let mut info = FILE_RENAME_INFO::default();
    // This union member is the documented legacy BOOLEAN indicating that an
    // existing target must not be replaced.
    info.Anonymous.ReplaceIfExists = BOOLEAN(0);
    // This NTFS fixture rejected a non-null RootDirectory for a relative
    // filename with ERROR_INVALID_PARAMETER. Keep every verified parent
    // handle alive through this operation, and build the absolute destination
    // only from the held parent's canonical path plus the validated leaf.
    info.RootDirectory = HANDLE::default();
    info.FileNameLength = name_bytes;
    let name_offset = offset_of!(FILE_RENAME_INFO, FileName);
    let buffer_bytes = name_offset
        .checked_add(terminated_name_bytes)
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "rename buffer too large")
        })?;
    let mut buffer = vec![0_u64; buffer_bytes.div_ceil(size_of::<u64>())];
    let buffer_ptr = buffer.as_mut_ptr().cast::<u8>();
    // SAFETY: `info` is a valid initialized FILE_RENAME_INFO prefix and the
    // destination buffer has the exact prefix plus UTF-16 name length.
    unsafe {
        std::ptr::copy_nonoverlapping(
            std::ptr::from_ref(&info).cast::<u8>(),
            buffer_ptr,
            name_offset,
        );
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            buffer_ptr.add(name_offset),
            terminated_name_bytes,
        );
        SetFileInformationByHandle(
            HANDLE(source.as_raw_handle()),
            FileRenameInfo,
            buffer_ptr.cast(),
            u32::try_from(buffer_bytes).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "rename buffer too large")
            })?,
        )
    }
    .map_err(|error| {
        native_contextual_io_error(
            "SetFileInformationByHandle(FileRenameInfo, no replace)",
            windows_error_to_io(error),
        )
    })
}

#[cfg(windows)]
fn read_handle_bytes(
    file: &std::fs::File,
    expected_length: u64,
    cancellation: &ScanCancellation,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    if expected_length > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement source exceeds the content verification limit",
        ));
    }
    let capacity = usize::try_from(expected_length).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement source length cannot be represented in memory",
        )
    })?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity).map_err(|error| {
        std::io::Error::other(format!(
            "could not reserve replacement verification memory: {error}"
        ))
    })?;
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        if cancellation.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "replacement source verification was cancelled",
            ));
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let next_length = (bytes.len() as u64)
            .checked_add(count as u64)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "replacement source byte count overflowed",
                )
            })?;
        if next_length > expected_length {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "replacement source grew while being verified",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    if bytes.len() as u64 != expected_length || file.metadata()?.len() != expected_length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "replacement source length changed while being verified",
        ));
    }
    Ok(bytes)
}

fn hash_file_contents(
    file: &std::fs::File,
    verification_mode: ContentVerificationMode<'_>,
) -> std::io::Result<[u8; 32]> {
    use std::io::{Seek, SeekFrom};

    let length = file.metadata()?.len();
    if length > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeds the content verification limit",
        ));
    }
    let mut clone = file.try_clone()?;
    let previous_position = clone.stream_position()?;
    clone.seek(SeekFrom::Start(0))?;
    let result = hash_reader(&mut clone, length, verification_mode);
    let restore = clone.seek(SeekFrom::Start(previous_position));
    match (result, restore) {
        (Ok(hash), Ok(_)) => Ok(hash),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn hash_reader(
    mut reader: impl std::io::Read,
    expected_length: u64,
    verification_mode: ContentVerificationMode<'_>,
) -> std::io::Result<[u8; 32]> {
    if expected_length > MAX_RENAME_HASH_BYTES_PER_FILE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file exceeds the content verification limit",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        if verification_mode.is_cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "content verification was cancelled",
            ));
        }
        let read = std::io::Read::read(&mut reader, &mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > MAX_RENAME_HASH_BYTES_PER_FILE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file exceeds the content verification limit",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file length changed during content verification",
        ));
    }
    Ok(hasher.finalize().into())
}

#[cfg(windows)]
fn open_native_duplicate_content_leaf(path: &Path) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_READ_DATA, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    dff_cloud_attribute_preflight(true, || query_native_path_attributes(path))
        .map_err(|error| native_contextual_io_error("DFF content cloud preflight", error))?;
    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    // Unlike directory proof handles, the content lease permits ordinary
    // writers and deleters. It pins only the opened object, while the caller
    // independently revalidates that the displayed path still names it.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            FILE_READ_DATA.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL,
            HANDLE::default(),
        )
    }
    .map_err(|error| {
        native_contextual_io_error("CreateFileW(DFF content read)", windows_error_to_io(error))
    })?;
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata(DFF content read)", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let cloud_placeholder = has_cloud_placeholder_attributes(attributes);
    if cloud_placeholder {
        return Err(native_contextual_io_error(
            "DFF content cloud recheck",
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "cloud-placeholder object became ineligible during content open",
            ),
        ));
    }
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path: final_path_for_handle(&file)
            .map_err(|error| native_contextual_io_error("GetFinalPathNameW(DFF content)", error))?,
        identity: native_file_identity(&file)
            .map_err(|error| native_contextual_io_error("FILE_ID_INFO(DFF content)", error))?,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder,
        handle: file,
    })
}

#[cfg(windows)]
fn open_native_object(
    path: &Path,
    enumerate_children: bool,
    exclude_cloud_placeholders: bool,
) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    dff_cloud_attribute_preflight(exclude_cloud_placeholders, || {
        query_native_path_attributes(path)
    })
    .map_err(|error| native_contextual_io_error("GetFileAttributesW", error))?;
    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    let desired_access = FILE_READ_ATTRIBUTES.0
        | if enumerate_children {
            FILE_LIST_DIRECTORY.0
        } else {
            0
        };
    // Keeping delete-sharing disabled makes each open directory a temporary
    // namespace guard. The scanner retains the complete ancestor handle chain
    // through enumeration and child opens, then releases it before returning
    // the path/ID proof to the caller.
    let share_mode = FILE_SHARE_READ | FILE_SHARE_WRITE;
    let flags = FILE_FLAG_OPEN_REPARSE_POINT
        | FILE_FLAG_BACKUP_SEMANTICS
        | if exclude_cloud_placeholders {
            FILE_FLAG_OPEN_NO_RECALL
        } else {
            FILE_FLAGS_AND_ATTRIBUTES(0)
        };
    // SAFETY: the path is a validated local, NUL-terminated UTF-16 path. The
    // API opens an existing object with reparse-point resolution disabled and
    // grants only identity/metadata or directory-list access. The returned
    // handle is immediately transferred into File for RAII ownership.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            desired_access,
            share_mode,
            None,
            OPEN_EXISTING,
            flags,
            HANDLE::default(),
        )
    }
    .map_err(|error| native_contextual_io_error("CreateFileW", windows_error_to_io(error)))?;
    // SAFETY: CreateFileW returned a valid owned HANDLE, which is transferred
    // exactly once to File and closed by its Drop implementation.
    let file = unsafe { std::fs::File::from_raw_handle(handle.0 as RawHandle) };
    let metadata = file
        .metadata()
        .map_err(|error| native_contextual_io_error("File::metadata", error))?;
    let attributes = metadata.file_attributes();
    let reparse_point = attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
    let cloud_placeholder = has_cloud_placeholder_attributes(attributes);
    if exclude_cloud_placeholders && cloud_placeholder {
        return Err(native_contextual_io_error(
            "DFF cloud placeholder recheck",
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "cloud-placeholder object became ineligible during open",
            ),
        ));
    }
    let is_directory = attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0;
    let kind = if reparse_point {
        SnapshotEntryKind::Other
    } else if is_directory {
        SnapshotEntryKind::Directory
    } else if metadata.is_file() {
        SnapshotEntryKind::File
    } else {
        SnapshotEntryKind::Other
    };
    let identity = native_file_identity(&file)
        .map_err(|error| native_contextual_io_error("FILE_ID_INFO query", error))?;
    let canonical_path = final_path_for_handle(&file)
        .map_err(|error| native_contextual_io_error("GetFinalPathNameByHandleW", error))?;
    Ok(OpenedObject {
        lookup_path: path.to_path_buf(),
        canonical_path,
        identity,
        kind,
        reparse_point,
        length: (!is_directory).then_some(metadata.len()),
        modified: metadata.modified().ok(),
        cloud_placeholder,
        handle: file,
    })
}

#[cfg(windows)]
fn query_native_path_attributes(path: &Path) -> std::io::Result<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;
    use windows::core::PCWSTR;

    let mut wide_path = extended_local_path(path)?;
    wide_path.push(0);
    // The DFF caller refuses failed attribute queries before opening and also
    // rechecks attributes from the no-recall handle after opening.
    let attributes = unsafe { GetFileAttributesW(PCWSTR(wide_path.as_ptr())) };
    if attributes == u32::MAX {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(attributes)
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct NativeIoContext {
    operation: &'static str,
    source: std::io::Error,
}

#[cfg(windows)]
impl std::fmt::Display for NativeIoContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} failed: {}", self.operation, self.source)
    }
}

#[cfg(windows)]
impl std::error::Error for NativeIoContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

#[cfg(windows)]
fn native_contextual_io_error(operation: &'static str, source: std::io::Error) -> std::io::Error {
    std::io::Error::new(source.kind(), NativeIoContext { operation, source })
}

#[cfg(windows)]
fn native_raw_os_error(error: &std::io::Error) -> Option<i32> {
    error.raw_os_error().or_else(|| {
        error
            .get_ref()?
            .downcast_ref::<NativeIoContext>()?
            .source
            .raw_os_error()
    })
}

#[cfg(windows)]
fn native_error_is_not_found(error: &std::io::Error) -> bool {
    matches!(native_raw_os_error(error), Some(2 | 3))
}

pub(crate) fn rename_error_is_destination_collision(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    let code = native_raw_os_error(error);
    #[cfg(not(windows))]
    let code = error.raw_os_error();
    matches!(code, Some(80 | 183))
}

#[cfg(windows)]
fn extended_local_path(path: &Path) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    const EXTENDED_PREFIX: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if units.len().saturating_add(1) > batch_rename::MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the path exceeds the Windows extended-path limit",
        ));
    }
    if units.starts_with(&EXTENDED_PREFIX) {
        return Ok(units);
    }
    match path.components().next() {
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_)) => {
            let mut extended = Vec::with_capacity(units.len() + EXTENDED_PREFIX.len());
            extended.extend(EXTENDED_PREFIX);
            extended.extend(units);
            Ok(extended)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only local drive-letter paths can be opened by the rename scanner",
        )),
    }
}

#[cfg(windows)]
fn windows_error_to_io(error: windows::core::Error) -> std::io::Error {
    let code = error.code().0 as u32;
    let code = if code & 0xffff_0000 == 0x8007_0000 {
        code & 0xffff
    } else {
        code
    };
    std::io::Error::from_raw_os_error(code as i32)
}

#[cfg(windows)]
pub(crate) fn native_file_identity(file: &std::fs::File) -> std::io::Result<NativeFileIdentity> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };

    let mut info = FILE_ID_INFO::default();
    // SAFETY: `info` is writable storage of exactly the documented FILE_ID_INFO
    // size, and the borrowed handle remains open for this read-only query.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    }
    .map_err(windows_error_to_io)?;
    Ok(NativeFileIdentity {
        volume_serial_number: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

#[cfg(windows)]
fn final_path_for_handle(file: &std::fs::File) -> std::io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GETFINALPATHNAMEBYHANDLE_FLAGS, GetFinalPathNameByHandleW,
        VOLUME_NAME_DOS,
    };

    let handle = HANDLE(file.as_raw_handle());
    let mut buffer = vec![0_u16; 512];
    loop {
        // SAFETY: the handle is borrowed from a live File and `buffer` is a
        // writable UTF-16 slice whose length is passed by the windows wrapper.
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                &mut buffer,
                GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0),
            )
        } as usize;
        if length == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if length < buffer.len() {
            buffer.truncate(length);
            return Ok(strip_extended_prefix(OsString::from_wide(&buffer)));
        }
        if length > batch_rename::MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the opened path exceeds the Windows path limit",
            ));
        }
        buffer.resize(length.saturating_add(1), 0);
    }
}

#[cfg(windows)]
fn strip_extended_prefix(path: OsString) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const EXTENDED_PREFIX: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    let units = path.encode_wide().collect::<Vec<_>>();
    if let Some(local) = units.strip_prefix(&EXTENDED_PREFIX) {
        PathBuf::from(OsString::from_wide(local))
    } else {
        PathBuf::from(path)
    }
}

#[cfg(windows)]
fn enumerate_native_directory(
    directory: &OpenDirectory<std::fs::File>,
    remaining_entries: usize,
    remaining_path_bytes: usize,
    cancellation: &ScanCancellation,
) -> Result<Vec<EnumeratedName>, BackendFailure> {
    use std::mem::offset_of;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ID_BOTH_DIR_INFO,
        FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo, GetFileInformationByHandleEx,
    };

    const BUFFER_WORDS: usize = 8_192;
    let mut buffer = vec![0_u64; BUFFER_WORDS];
    let mut entries = Vec::new();
    let mut names_path_bytes = 0usize;
    let mut information_class = FileIdBothDirectoryRestartInfo;
    loop {
        if cancellation.is_cancelled() {
            return Err(BackendFailure::new(
                BackendFailureKind::Cancelled,
                Some(&directory.canonical_path),
                "directory scan was cancelled",
            ));
        }
        // SAFETY: the directory handle was opened with FILE_LIST_DIRECTORY;
        // the aligned byte buffer is writable and large enough for the API.
        let query = unsafe {
            GetFileInformationByHandleEx(
                HANDLE(directory.handle.as_raw_handle()),
                information_class,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * std::mem::size_of::<u64>()) as u32,
            )
        };
        if let Err(error) = query {
            if error.code().0 as u32 == 0x8007_0012 {
                break;
            }
            let code = (error.code().0 as u32) & 0xffff;
            let kind = if matches!(code, 1 | 50 | 87) {
                BackendFailureKind::UnsupportedIdentity
            } else {
                BackendFailureKind::Inaccessible
            };
            return Err(BackendFailure::new(
                kind,
                Some(&directory.canonical_path),
                format!("could not enumerate the opened directory handle: {error}"),
            ));
        }
        information_class = FileIdBothDirectoryInfo;
        let bytes = std::mem::size_of_val(buffer.as_slice());
        let mut offset = 0usize;
        loop {
            let header_end = offset
                .checked_add(offset_of!(FILE_ID_BOTH_DIR_INFO, FileName))
                .ok_or_else(|| {
                    BackendFailure::new(
                        BackendFailureKind::UnsupportedIdentity,
                        Some(&directory.canonical_path),
                        "directory entry offset overflowed",
                    )
                })?;
            if header_end > bytes {
                return Err(BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(&directory.canonical_path),
                    "directory information returned a truncated record",
                ));
            }
            // SAFETY: the record header is within the API buffer. Fields are
            // read unaligned because the API defines variable-length records.
            let record = unsafe {
                buffer
                    .as_ptr()
                    .cast::<u8>()
                    .add(offset)
                    .cast::<FILE_ID_BOTH_DIR_INFO>()
            };
            let next_offset =
                unsafe { std::ptr::addr_of!((*record).NextEntryOffset).read_unaligned() } as usize;
            let file_name_length =
                unsafe { std::ptr::addr_of!((*record).FileNameLength).read_unaligned() } as usize;
            let attributes =
                unsafe { std::ptr::addr_of!((*record).FileAttributes).read_unaligned() };
            if !file_name_length.is_multiple_of(std::mem::size_of::<u16>()) {
                return Err(BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(&directory.canonical_path),
                    "directory entry returned an invalid UTF-16 name length",
                ));
            }
            let name_end = header_end.checked_add(file_name_length).ok_or_else(|| {
                BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(&directory.canonical_path),
                    "directory entry name length overflowed",
                )
            })?;
            let record_end = if next_offset == 0 {
                name_end
            } else {
                offset.checked_add(next_offset).ok_or_else(|| {
                    BackendFailure::new(
                        BackendFailureKind::UnsupportedIdentity,
                        Some(&directory.canonical_path),
                        "directory entry next offset overflowed",
                    )
                })?
            };
            if name_end > bytes || record_end > bytes || record_end < name_end {
                return Err(BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(&directory.canonical_path),
                    "directory entry record exceeds the native buffer",
                ));
            }
            // SAFETY: the UTF-16 flexible-array name lies within the checked
            // record bounds and remains valid until copied into OsString.
            let name = unsafe {
                let name_ptr = std::ptr::addr_of!((*record).FileName);
                let name_ptr = name_ptr.cast::<u16>();
                let name_units = std::slice::from_raw_parts(
                    name_ptr,
                    file_name_length / std::mem::size_of::<u16>(),
                );
                OsString::from_wide(name_units)
            };
            if !name.is_empty() && name != OsStr::new(".") && name != OsStr::new("..") {
                if entries.len() >= remaining_entries {
                    return Err(BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&directory.canonical_path),
                        format!("entry limit of {remaining_entries} reached"),
                    ));
                }
                let next_names_path_bytes =
                    names_path_bytes.saturating_add(path_storage_bytes(Path::new(&name)));
                if next_names_path_bytes > remaining_path_bytes {
                    return Err(BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&directory.canonical_path),
                        format!(
                            "discovery path storage limit of {remaining_path_bytes} bytes reached while enumerating names"
                        ),
                    ));
                }
                names_path_bytes = next_names_path_bytes;
                entries.push(EnumeratedName {
                    name,
                    is_directory: attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
                    is_reparse_point: attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
                    cloud_placeholder: has_cloud_placeholder_attributes(attributes),
                });
            }
            if next_offset == 0 {
                break;
            }
            if next_offset < offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)
                || !next_offset.is_multiple_of(std::mem::align_of::<u64>())
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(&directory.canonical_path),
                    "directory entry returned an invalid next-record offset",
                ));
            }
            offset = record_end;
        }
    }
    Ok(entries)
}

#[cfg(windows)]
fn classify_root_io(path: &Path, error: std::io::Error) -> BackendFailure {
    let code = native_raw_os_error(&error).unwrap_or_default() as u32;
    let kind = match code {
        2 | 3 | 267 | 123 => BackendFailureKind::InvalidRoot,
        50 | 1 | 87 => BackendFailureKind::UnsupportedIdentity,
        _ => BackendFailureKind::Inaccessible,
    };
    BackendFailure::new(
        kind,
        Some(path),
        format!("could not admit the selected root: {error}"),
    )
}

#[cfg(windows)]
fn classify_child_io(path: &Path, error: std::io::Error) -> BackendFailure {
    let code = native_raw_os_error(&error).unwrap_or_default() as u32;
    let kind = match code {
        50 | 1 | 87 => BackendFailureKind::UnsupportedIdentity,
        _ => BackendFailureKind::Inaccessible,
    };
    BackendFailure::new(
        kind,
        Some(path),
        format!("could not open the enumerated sibling: {error}"),
    )
}

#[cfg(windows)]
fn classify_chain_io(path: &Path, error: std::io::Error, root: &RootProof) -> BackendFailure {
    let raw_code = native_raw_os_error(&error).unwrap_or_default() as u32;
    let kind = if matches!(raw_code, 5 | 32 | 33) {
        BackendFailureKind::Inaccessible
    } else if root.chain.iter().any(|proof| {
        batch_rename::compare_paths_windows_ordinal(&proof.lookup_path, path)
            == std::cmp::Ordering::Equal
    }) {
        BackendFailureKind::RootChanged
    } else {
        BackendFailureKind::ParentChanged
    };
    BackendFailure::new(
        kind,
        Some(path),
        format!("could not reopen a verified directory: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn m8d_hash_cancellation_stops_between_chunks_and_compensation_finishes() {
        use std::cell::Cell;

        struct CancellingReader<'a> {
            bytes: &'a [u8],
            offset: usize,
            cancellation: &'a ScanCancellation,
            read_calls: &'a Cell<usize>,
        }

        impl std::io::Read for CancellingReader<'_> {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                self.read_calls.set(self.read_calls.get() + 1);
                let count = buffer.len().min(self.bytes.len() - self.offset);
                if count == 0 {
                    return Ok(0);
                }
                buffer[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
                self.offset += count;
                if self.offset >= 64 * 1024 {
                    self.cancellation.cancel();
                }
                Ok(count)
            }
        }

        let bytes = vec![0x5a; 2 * 64 * 1024 + 1];
        let cooperative_token = ScanCancellation::new();
        let cooperative_calls = Cell::new(0);
        let result = hash_reader(
            CancellingReader {
                bytes: &bytes,
                offset: 0,
                cancellation: &cooperative_token,
                read_calls: &cooperative_calls,
            },
            bytes.len() as u64,
            ContentVerificationMode::Cooperative(&cooperative_token),
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(cooperative_calls.get(), 1);

        let compensation_token = ScanCancellation::new();
        compensation_token.cancel();
        let compensation_calls = Cell::new(0);
        let result = hash_reader(
            CancellingReader {
                bytes: &bytes,
                offset: 0,
                cancellation: &compensation_token,
                read_calls: &compensation_calls,
            },
            bytes.len() as u64,
            ContentVerificationMode::CompleteOwnedPayload,
        )
        .unwrap();
        assert_eq!(result, <[u8; 32]>::from(Sha256::digest(&bytes)));
        assert_eq!(compensation_calls.get(), 4);
    }

    #[derive(Debug, Clone)]
    struct FakeObject {
        identity: NativeFileIdentity,
        kind: SnapshotEntryKind,
        reparse_point: bool,
        cloud_placeholder: bool,
        length: Option<u64>,
        modified: Option<SystemTime>,
        children: Vec<EnumeratedName>,
    }

    #[derive(Default)]
    struct FakeBackend {
        objects: Mutex<HashMap<PathBuf, FakeObject>>,
        inaccessible: Mutex<HashSet<PathBuf>>,
        directory_list_inaccessible: Mutex<HashSet<PathBuf>>,
        unsupported: Mutex<HashSet<PathBuf>>,
        mid_open_substitution: Mutex<Option<(PathBuf, u64)>>,
        cloud_before_next_open: Mutex<Option<PathBuf>>,
        resource_limited: AtomicBool,
        child_open_calls: AtomicUsize,
    }

    impl FakeBackend {
        fn identity(id: u64) -> NativeFileIdentity {
            let mut file_id = [0; 16];
            file_id[..8].copy_from_slice(&id.to_le_bytes());
            NativeFileIdentity {
                volume_serial_number: 7,
                file_id,
            }
        }

        fn new_root(path: &Path) -> Self {
            let backend = Self::default();
            backend.insert_directory(path, 1, false);
            backend
        }

        fn insert_directory(&self, path: &Path, id: u64, reparse_point: bool) {
            self.objects.lock().unwrap().insert(
                path.to_path_buf(),
                FakeObject {
                    identity: Self::identity(id),
                    kind: if reparse_point {
                        SnapshotEntryKind::Other
                    } else {
                        SnapshotEntryKind::Directory
                    },
                    reparse_point,
                    cloud_placeholder: false,
                    length: None,
                    modified: None,
                    children: Vec::new(),
                },
            );
        }

        fn insert_file(&self, path: &Path, id: u64) {
            self.objects.lock().unwrap().insert(
                path.to_path_buf(),
                FakeObject {
                    identity: Self::identity(id),
                    kind: SnapshotEntryKind::File,
                    reparse_point: false,
                    cloud_placeholder: false,
                    length: Some(1),
                    modified: Some(std::time::UNIX_EPOCH),
                    children: Vec::new(),
                },
            );
        }

        fn add_file(&self, parent: &Path, name: &str, id: u64) -> PathBuf {
            let path = parent.join(name);
            self.insert_file(&path, id);
            self.objects
                .lock()
                .unwrap()
                .get_mut(parent)
                .unwrap()
                .children
                .push(EnumeratedName {
                    name: OsString::from(name),
                    is_directory: false,
                    is_reparse_point: false,
                    cloud_placeholder: false,
                });
            path
        }

        fn add_directory(&self, parent: &Path, name: &str, id: u64) -> PathBuf {
            let path = parent.join(name);
            self.insert_directory(&path, id, false);
            self.objects
                .lock()
                .unwrap()
                .get_mut(parent)
                .unwrap()
                .children
                .push(EnumeratedName {
                    name: OsString::from(name),
                    is_directory: true,
                    is_reparse_point: false,
                    cloud_placeholder: false,
                });
            path
        }

        fn add_reparse_directory(&self, parent: &Path, name: &str, id: u64) -> PathBuf {
            let path = parent.join(name);
            self.insert_directory(&path, id, true);
            self.objects
                .lock()
                .unwrap()
                .get_mut(parent)
                .unwrap()
                .children
                .push(EnumeratedName {
                    name: OsString::from(name),
                    is_directory: true,
                    is_reparse_point: true,
                    cloud_placeholder: false,
                });
            path
        }

        fn change_identity(&self, path: &Path, id: u64) {
            self.objects.lock().unwrap().get_mut(path).unwrap().identity = Self::identity(id);
        }

        fn change_metadata(&self, path: &Path, length: u64, modified: SystemTime) {
            let mut objects = self.objects.lock().unwrap();
            let object = objects.get_mut(path).unwrap();
            object.length = Some(length);
            object.modified = Some(modified);
        }

        fn mark_inaccessible(&self, path: &Path) {
            self.inaccessible.lock().unwrap().insert(path.to_path_buf());
        }

        fn mark_unsupported(&self, path: &Path) {
            self.unsupported.lock().unwrap().insert(path.to_path_buf());
        }

        fn deny_directory_listing(&self, path: &Path) {
            self.directory_list_inaccessible
                .lock()
                .unwrap()
                .insert(path.to_path_buf());
        }

        fn substitute_parent_during_next_open(&self, path: &Path, new_id: u64) {
            *self.mid_open_substitution.lock().unwrap() = Some((path.to_path_buf(), new_id));
        }

        fn mark_cloud_before_next_open(&self, path: &Path) {
            *self.cloud_before_next_open.lock().unwrap() = Some(path.to_path_buf());
        }

        fn object(&self, path: &Path) -> Result<FakeObject, BackendFailure> {
            if self.inaccessible.lock().unwrap().contains(path) {
                return Err(BackendFailure::new(
                    BackendFailureKind::Inaccessible,
                    Some(path),
                    "injected inaccessible object",
                ));
            }
            if self.unsupported.lock().unwrap().contains(path) {
                return Err(BackendFailure::new(
                    BackendFailureKind::UnsupportedIdentity,
                    Some(path),
                    "injected unsupported file identity",
                ));
            }
            self.objects
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| {
                    BackendFailure::new(
                        BackendFailureKind::Inaccessible,
                        Some(path),
                        "object disappeared after directory enumeration",
                    )
                })
        }
    }

    impl RenameScanBackend for FakeBackend {
        type Handle = PathBuf;

        fn admit_root(
            &self,
            request: &RenameScanRequest,
        ) -> Result<RootAdmission<Self::Handle>, BackendFailure> {
            let selected = request.root.as_path();
            let object = self.object(selected).map_err(|error| {
                BackendFailure::new(
                    BackendFailureKind::InvalidRoot,
                    Some(selected),
                    error.message,
                )
            })?;
            if object.kind != SnapshotEntryKind::Directory || object.reparse_point {
                return Err(BackendFailure::new(
                    BackendFailureKind::InvalidRoot,
                    Some(selected),
                    "fake root is not a plain directory",
                ));
            }
            let chain = vec![DirectoryProof {
                lookup_path: selected.to_path_buf(),
                canonical_path: selected.to_path_buf(),
                identity: object.identity,
            }];
            let proof = RootProof {
                selected_path: selected.to_path_buf(),
                canonical_path: selected.to_path_buf(),
                identity: object.identity,
                recursive: request.scope == RenameScanScope::Recursive,
                exclude_cloud_placeholders: request.exclude_cloud_placeholders,
                generation: request.generation,
                chain,
            };
            let directories =
                self.reopen_chain(&proof.chain, &proof, DirectoryOpenAccess::Enumerate)?;
            Ok(RootAdmission { proof, directories })
        }

        fn enumerate_directory(
            &self,
            directory: &OpenDirectory<Self::Handle>,
            remaining_entries: usize,
            remaining_path_bytes: usize,
            cancellation: &ScanCancellation,
        ) -> Result<Vec<EnumeratedName>, BackendFailure> {
            if cancellation.is_cancelled() {
                return Err(BackendFailure::new(
                    BackendFailureKind::Cancelled,
                    Some(&directory.canonical_path),
                    "cancelled",
                ));
            }
            if self.resource_limited.load(Ordering::Acquire) {
                return Err(BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&directory.canonical_path),
                    "injected resource limit",
                ));
            }
            if self
                .directory_list_inaccessible
                .lock()
                .unwrap()
                .contains(&directory.canonical_path)
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::Inaccessible,
                    Some(&directory.canonical_path),
                    "injected directory-list permission failure",
                ));
            }
            let entries = self.object(&directory.canonical_path)?.children;
            if entries.len() > remaining_entries {
                return Err(BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&directory.canonical_path),
                    "fake directory exceeds the remaining entry budget",
                ));
            }
            let path_bytes = entries.iter().fold(0usize, |total, entry| {
                total.saturating_add(path_storage_bytes(Path::new(&entry.name)))
            });
            if path_bytes > remaining_path_bytes {
                return Err(BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&directory.canonical_path),
                    "fake directory exceeds the remaining path-byte budget",
                ));
            }
            Ok(entries)
        }

        fn open_child(
            &self,
            parent: &OpenDirectory<Self::Handle>,
            parent_chain: &[DirectoryProof],
            root: &RootProof,
            name: &OsStr,
            expected: Option<&EnumeratedName>,
        ) -> Result<OpenedObject<Self::Handle>, BackendFailure> {
            let verified_parent_chain =
                self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
            let Some(verified_parent) = verified_parent_chain.last() else {
                return Err(BackendFailure::new(
                    BackendFailureKind::ParentChanged,
                    Some(&parent.canonical_path),
                    "fake parent chain is empty",
                ));
            };
            if verified_parent.identity != parent.identity {
                return Err(BackendFailure::new(
                    BackendFailureKind::ParentChanged,
                    Some(&parent.canonical_path),
                    "fake parent identity changed",
                ));
            }
            let path = parent.canonical_path.join(name);
            if let Some((changed_parent, new_id)) =
                self.mid_open_substitution.lock().unwrap().take()
            {
                self.change_identity(&changed_parent, new_id);
            }
            let mark_cloud = {
                let mut next = self.cloud_before_next_open.lock().unwrap();
                if next.as_ref() == Some(&path) {
                    next.take();
                    true
                } else {
                    false
                }
            };
            if mark_cloud {
                self.objects
                    .lock()
                    .unwrap()
                    .get_mut(&path)
                    .ok_or_else(|| {
                        BackendFailure::new(
                            BackendFailureKind::ObjectChanged,
                            Some(&path),
                            "injected cloud-state target disappeared",
                        )
                    })?
                    .cloud_placeholder = true;
            }
            let object = self.object(&path)?;
            if root.exclude_cloud_placeholders {
                dff_cloud_attribute_preflight(true, || {
                    Ok(if object.cloud_placeholder {
                        FILE_ATTRIBUTE_OFFLINE_BITS
                    } else {
                        0
                    })
                })
                .map_err(|error| {
                    BackendFailure::new(
                        BackendFailureKind::UnsupportedIdentity,
                        Some(&path),
                        error.to_string(),
                    )
                })?;
            }
            self.child_open_calls.fetch_add(1, Ordering::AcqRel);
            if let Some(expected) = expected
                && (expected.is_reparse_point != object.reparse_point
                    || (!expected.is_reparse_point
                        && expected.is_directory != (object.kind == SnapshotEntryKind::Directory)))
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::ObjectChanged,
                    Some(&path),
                    "fake entry changed after enumeration",
                ));
            }
            self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
            Ok(OpenedObject {
                lookup_path: path.clone(),
                canonical_path: path.clone(),
                identity: object.identity,
                kind: object.kind,
                reparse_point: object.reparse_point,
                length: object.length,
                modified: object.modified,
                cloud_placeholder: object.cloud_placeholder,
                handle: path,
            })
        }

        fn acquire_child_directory_for_enumeration(
            &self,
            parent: &OpenDirectory<Self::Handle>,
            parent_chain: &[DirectoryProof],
            root: &RootProof,
            child: &OpenedObject<Self::Handle>,
        ) -> Result<OpenDirectory<Self::Handle>, BackendFailure> {
            let verified_parent_chain =
                self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
            let Some(verified_parent) = verified_parent_chain.last() else {
                return Err(BackendFailure::new(
                    BackendFailureKind::ParentChanged,
                    Some(&parent.canonical_path),
                    "fake parent chain is empty before child enumeration",
                ));
            };
            if verified_parent.identity != parent.identity {
                return Err(BackendFailure::new(
                    BackendFailureKind::ParentChanged,
                    Some(&parent.canonical_path),
                    "fake parent identity changed before child enumeration",
                ));
            }
            if child.kind != SnapshotEntryKind::Directory || child.reparse_point {
                return Err(BackendFailure::new(
                    BackendFailureKind::ObjectChanged,
                    Some(&child.canonical_path),
                    "fake child is not a plain directory",
                ));
            }
            if self
                .directory_list_inaccessible
                .lock()
                .unwrap()
                .contains(&child.lookup_path)
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::Inaccessible,
                    Some(&child.lookup_path),
                    "injected child directory-list permission failure",
                ));
            }
            let object = self.object(&child.lookup_path)?;
            if object.identity != child.identity
                || object.kind != SnapshotEntryKind::Directory
                || object.reparse_point
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::ObjectChanged,
                    Some(&child.lookup_path),
                    "fake child directory changed before enumeration",
                ));
            }
            self.reopen_chain(parent_chain, root, DirectoryOpenAccess::Metadata)?;
            Ok(OpenDirectory {
                lookup_path: child.lookup_path.clone(),
                canonical_path: child.canonical_path.clone(),
                identity: object.identity,
                handle: child.lookup_path.clone(),
            })
        }

        fn reopen_chain(
            &self,
            chain: &[DirectoryProof],
            root: &RootProof,
            final_access: DirectoryOpenAccess,
        ) -> Result<OpenDirectoryChain<Self::Handle>, BackendFailure> {
            let mut directories = Vec::with_capacity(chain.len());
            for (index, proof) in chain.iter().enumerate() {
                if index + 1 == chain.len()
                    && final_access == DirectoryOpenAccess::Enumerate
                    && self
                        .directory_list_inaccessible
                        .lock()
                        .unwrap()
                        .contains(&proof.lookup_path)
                {
                    return Err(BackendFailure::new(
                        BackendFailureKind::Inaccessible,
                        Some(&proof.lookup_path),
                        "injected directory-list permission failure",
                    ));
                }
                let object = self.object(&proof.lookup_path)?;
                if object.identity != proof.identity
                    || object.kind != SnapshotEntryKind::Directory
                    || object.reparse_point
                {
                    return Err(BackendFailure::new(
                        if root.chain.iter().any(|root_proof| root_proof == proof) {
                            BackendFailureKind::RootChanged
                        } else {
                            BackendFailureKind::ParentChanged
                        },
                        Some(&proof.lookup_path),
                        "fake verified directory identity changed",
                    ));
                }
                directories.push(Arc::new(OpenDirectory {
                    lookup_path: proof.lookup_path.clone(),
                    canonical_path: proof.canonical_path.clone(),
                    identity: object.identity,
                    handle: proof.lookup_path.clone(),
                }));
            }
            if directories.is_empty() {
                Err(BackendFailure::new(
                    BackendFailureKind::RootChanged,
                    Some(&root.selected_path),
                    "fake chain was empty",
                ))
            } else {
                Ok(directories)
            }
        }
    }

    fn test_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary root")
    }

    fn request(root: &Path, recursive: bool, generation: u64) -> RenameScanRequest {
        RenameScanRequest::new(
            ExplicitDirectoryRoot::new(root.to_path_buf()).expect("absolute root"),
            recursive,
            RenameScanGeneration(generation),
        )
    }

    #[test]
    fn dff_discovery_skips_cloud_and_reparse_entries_before_opening_them() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "ordinary.bin", 2);
        backend.add_file(root, "cloud.bin", 3);
        let reparse = backend.add_reparse_directory(root, "linked", 4);
        backend.add_file(&reparse, "outside.bin", 5);
        backend
            .objects
            .lock()
            .unwrap()
            .get_mut(root)
            .unwrap()
            .children
            .iter_mut()
            .find(|entry| entry.name == OsStr::new("cloud.bin"))
            .unwrap()
            .cloud_placeholder = true;

        let mut request = request(root, true, 100);
        request.exclude_cloud_placeholders = true;
        let result = scan_with_backend_bounded(
            &backend,
            request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(DuplicateDiscoveryScanLimits {
                max_entries: 20,
                max_files: 20,
                max_directories: 20,
                max_depth: 20,
                max_path_bytes: usize::MAX,
            }),
        );

        assert_eq!(result.status(), RenameScanStatus::Complete);
        assert_eq!(result.rows().len(), 1);
        assert_eq!(
            result.rows()[0].path().file_name(),
            Some(OsStr::new("ordinary.bin"))
        );
        assert_eq!(result.summary().cloud_placeholders_excluded(), 1);
        assert_eq!(result.summary().reparse_entries_excluded(), 1);
        assert_eq!(backend.child_open_calls.load(Ordering::Acquire), 1);
        assert!(result.snapshot.is_none());
    }

    #[test]
    fn dff_discovery_path_budget_bounds_enumerated_names_before_child_open() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "a-name-longer-than-the-remaining-budget.bin", 2);

        let mut request = request(root, true, 101);
        request.exclude_cloud_placeholders = true;
        let root_bytes = path_storage_bytes(root);
        // The fake root has one proof path, one retained directory handle,
        // one queued/active frame, request/display/canonical paths, and the
        // returned canonical-root path. Leave too little for the enumerated
        // filename so both native and injected backends must stop pre-open.
        let limits = DuplicateDiscoveryScanLimits {
            max_entries: 20,
            max_files: 20,
            max_directories: 20,
            max_depth: 20,
            max_path_bytes: root_bytes.saturating_mul(10).saturating_add(2),
        };
        let result = scan_with_backend_bounded(
            &backend,
            request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(limits),
        );

        assert_eq!(result.status(), RenameScanStatus::ResourceLimit);
        assert!(result.rows().is_empty());
        assert_eq!(backend.child_open_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn dff_discovery_rechecks_cloud_state_before_opening_an_enumerated_child() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        let cloud_path = backend.add_file(root, "became-cloud.bin", 2);
        backend.mark_cloud_before_next_open(&cloud_path);

        let mut request = request(root, true, 102);
        request.exclude_cloud_placeholders = true;
        let result = scan_with_backend_bounded(
            &backend,
            request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(DuplicateDiscoveryScanLimits {
                max_entries: 20,
                max_files: 20,
                max_directories: 20,
                max_depth: 20,
                max_path_bytes: usize::MAX,
            }),
        );

        assert_ne!(result.status(), RenameScanStatus::Complete);
        assert!(result.rows().is_empty());
        assert_eq!(backend.child_open_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn dff_discovery_fails_closed_when_cloud_attribute_query_fails() {
        let mut query_calls = 0;
        let failure = dff_cloud_attribute_preflight(true, || {
            query_calls += 1;
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected attribute-query failure",
            ))
        });
        assert_eq!(query_calls, 1);
        assert_eq!(
            failure.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );

        let mut excluded_query_calls = 0;
        assert!(
            dff_cloud_attribute_preflight(false, || {
                excluded_query_calls += 1;
                Err(std::io::Error::other(
                    "must not be queried for rename scans",
                ))
            })
            .is_ok()
        );
        assert_eq!(excluded_query_calls, 0);
    }

    #[test]
    fn dff_discovery_bounds_queued_directories_before_acquiring_child_handles() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_directory(root, "one", 2);
        backend.add_directory(root, "two", 3);

        let mut request = request(root, true, 103);
        request.exclude_cloud_placeholders = true;
        let result = scan_with_backend_bounded(
            &backend,
            request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(DuplicateDiscoveryScanLimits {
                max_entries: 20,
                max_files: 20,
                max_directories: 1,
                max_depth: 20,
                max_path_bytes: usize::MAX,
            }),
        );

        assert_eq!(result.status(), RenameScanStatus::ResourceLimit);
        assert_eq!(result.summary().directories_scanned(), 1);
        assert_eq!(backend.child_open_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn dff_discovery_partial_root_charges_queued_directories_to_the_next_root() {
        let first_temp = test_root();
        let first_root = first_temp.path();
        let first_backend = FakeBackend::new_root(first_root);
        let healthy = first_backend.add_directory(first_root, "a-healthy", 2);
        first_backend.add_directory(&healthy, "a1-still-queued", 4);
        let blocked = first_backend.add_directory(first_root, "b-blocked", 3);
        first_backend.deny_directory_listing(&blocked);

        let total_directory_limit = 5;
        let first_root_budget =
            remaining_duplicate_discovery_directory_budget(total_directory_limit, 0, 1);
        assert_eq!(first_root_budget, 4, "one slot is reserved for root two");
        let mut first_request = request(first_root, true, 104);
        first_request.exclude_cloud_placeholders = true;
        let first = scan_with_backend_bounded(
            &first_backend,
            first_request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(DuplicateDiscoveryScanLimits {
                max_entries: 20,
                max_files: 20,
                max_directories: first_root_budget,
                max_depth: 20,
                max_path_bytes: usize::MAX,
            }),
        );
        assert_eq!(first.status(), RenameScanStatus::Partial);
        // Scanned counts directory attempts, including the blocked listing.
        assert_eq!(first.summary().directories_scanned(), 3);
        assert_eq!(first.summary().directories_admitted(), 4);

        // The first root stopped after its blocked child while a healthy
        // grandchild remained queued. That admission still consumes the
        // aggregate cap, so root two gets only its own root slot.
        let second_root_budget = remaining_duplicate_discovery_directory_budget(
            total_directory_limit,
            first.summary().directories_admitted(),
            0,
        );
        assert_eq!(second_root_budget, 1);
        let second_temp = test_root();
        let second_root = second_temp.path();
        let second_backend = FakeBackend::new_root(second_root);
        second_backend.add_directory(second_root, "must-not-open", 4);
        let mut second_request = request(second_root, true, 105);
        second_request.exclude_cloud_placeholders = true;
        let second = scan_with_backend_bounded(
            &second_backend,
            second_request,
            &ScanCancellation::new(),
            ScanBounds::duplicate_discovery(DuplicateDiscoveryScanLimits {
                max_entries: 20,
                max_files: 20,
                max_directories: second_root_budget,
                max_depth: 20,
                max_path_bytes: usize::MAX,
            }),
        );
        assert_eq!(second.status(), RenameScanStatus::ResourceLimit);
        assert_eq!(second.summary().directories_scanned(), 1);
        assert_eq!(second.summary().directories_admitted(), 1);
        assert_eq!(second_backend.child_open_calls.load(Ordering::Acquire), 0);
    }

    #[test]
    fn direct_and_recursive_scans_keep_sibling_blockers_and_skip_reparse_children() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "source.txt", 2);
        backend.add_directory(root, "target.txt", 3);
        let nested = backend.add_directory(root, "nested", 4);
        backend.add_file(&nested, "child.txt", 5);
        let reparse = backend.add_reparse_directory(root, "linked", 6);
        backend.add_file(&reparse, "outside.txt", 7);

        let direct = scan_with_backend(&backend, request(root, false, 1), &ScanCancellation::new());
        assert_eq!(direct.status(), RenameScanStatus::Complete);
        assert_eq!(direct.rows().len(), 4);
        assert!(direct.rows().iter().any(|row| row.is_reparse_point()));
        assert!(
            !direct
                .rows()
                .iter()
                .any(|row| row.path() == reparse.join("outside.txt"))
        );
        assert_eq!(direct.summary().reparse_entries_excluded(), 1);

        let accepted = direct.accept(RenameScanGeneration(1)).unwrap();
        assert_eq!(accepted.snapshot.entries().len(), 4);
        assert!(
            accepted
                .snapshot
                .entries()
                .iter()
                .all(|entry| !entry.is_selected())
        );
        let source = accepted
            .snapshot
            .entries()
            .iter()
            .find(|entry| entry.source_path().file_name() == Some(OsStr::new("source.txt")))
            .unwrap();
        let selected = accepted.with_selection(source.entry_ref(), true).unwrap();
        let rules = RenameRuleSet::new(
            batch_rename::ExtensionPolicy::Preserve,
            vec![batch_rename::RenameRule::LiteralReplace {
                find: "source".into(),
                replace_with: "target".into(),
            }],
        );
        let preview = selected.preview(&rules);
        let row = preview
            .preview()
            .rows()
            .iter()
            .find(|row| row.entry_ref() == source.entry_ref())
            .unwrap();
        assert!(matches!(
            row.status(),
            batch_rename::PreviewRowStatus::Conflict(
                batch_rename::ConflictReason::DestinationOccupied { .. }
            )
        ));

        let recursive =
            scan_with_backend(&backend, request(root, true, 2), &ScanCancellation::new());
        assert_eq!(recursive.status(), RenameScanStatus::Complete);
        assert_eq!(recursive.rows().len(), 5);
        assert!(
            recursive
                .rows()
                .iter()
                .any(|row| row.path().file_name() == Some(OsStr::new("child.txt")))
        );
        assert!(
            !recursive
                .rows()
                .iter()
                .any(|row| { row.path().file_name() == Some(OsStr::new("outside.txt")) })
        );
    }

    #[test]
    fn direct_scan_keeps_child_directory_blocker_when_listing_is_denied() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        let restricted = backend.add_directory(root, "restricted", 2);
        backend.add_file(&restricted, "hidden.txt", 3);
        backend.deny_directory_listing(&restricted);

        let direct =
            scan_with_backend(&backend, request(root, false, 31), &ScanCancellation::new());
        assert_eq!(direct.status(), RenameScanStatus::Complete);
        assert!(
            direct.rows().iter().any(|row| {
                row.path() == restricted && row.kind() == SnapshotEntryKind::Directory
            })
        );

        let recursive =
            scan_with_backend(&backend, request(root, true, 32), &ScanCancellation::new());
        assert_eq!(recursive.status(), RenameScanStatus::Partial);
        assert!(
            recursive.rows().iter().any(|row| {
                row.path() == restricted && row.kind() == SnapshotEntryKind::Directory
            })
        );
        assert!(
            recursive
                .rows()
                .iter()
                .all(|row| row.path() != restricted.join("hidden.txt"))
        );
        assert!(matches!(
            recursive.accept(RenameScanGeneration(32)),
            Err(ScanAcceptanceError::Incomplete(RenameScanStatus::Partial))
        ));
    }

    #[test]
    fn scan_detects_parent_substitution_during_child_open() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "source.txt", 2);
        backend.substitute_parent_during_next_open(root, 99);

        let result =
            scan_with_backend(&backend, request(root, false, 33), &ScanCancellation::new());
        assert_eq!(result.status(), RenameScanStatus::ChangedDuringScan);
        assert!(matches!(
            result.accept(RenameScanGeneration(33)),
            Err(ScanAcceptanceError::Incomplete(
                RenameScanStatus::ChangedDuringScan
            ))
        ));
    }

    #[test]
    fn deterministic_rows_keep_exact_unicode_names_independent_of_enumeration_order() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "β.txt", 2);
        backend.add_file(root, "café.txt", 3);
        backend.add_file(root, "a.txt", 4);
        backend.add_file(root, "É.txt", 5);

        let first = scan_with_backend(&backend, request(root, false, 10), &ScanCancellation::new());
        backend
            .objects
            .lock()
            .unwrap()
            .get_mut(root)
            .unwrap()
            .children
            .reverse();
        let second =
            scan_with_backend(&backend, request(root, false, 11), &ScanCancellation::new());
        assert_eq!(first.status(), RenameScanStatus::Complete);
        assert_eq!(second.status(), RenameScanStatus::Complete);
        assert_eq!(
            first
                .rows()
                .iter()
                .map(RenameScanRow::path)
                .collect::<Vec<_>>(),
            second
                .rows()
                .iter()
                .map(RenameScanRow::path)
                .collect::<Vec<_>>(),
        );
        assert!(
            first
                .rows()
                .iter()
                .any(|row| row.path().file_name() == Some(OsStr::new("café.txt")))
        );
        assert!(
            first
                .rows()
                .iter()
                .any(|row| row.path().file_name() == Some(OsStr::new("β.txt")))
        );
        assert!(matches!(
            first.clone().accept(RenameScanGeneration(12)),
            Err(ScanAcceptanceError::StaleGeneration { .. })
        ));
    }

    #[test]
    fn incomplete_cancelled_unsupported_and_limited_scans_cannot_be_accepted() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        let missing = backend.add_file(root, "disappears.txt", 2);
        backend.mark_inaccessible(&missing);
        let partial =
            scan_with_backend(&backend, request(root, false, 1), &ScanCancellation::new());
        assert_eq!(partial.status(), RenameScanStatus::Partial);
        assert!(matches!(
            partial.accept(RenameScanGeneration(1)),
            Err(ScanAcceptanceError::Incomplete(RenameScanStatus::Partial))
        ));

        backend.inaccessible.lock().unwrap().clear();
        backend.mark_unsupported(&missing);
        let unsupported =
            scan_with_backend(&backend, request(root, false, 2), &ScanCancellation::new());
        assert_eq!(unsupported.status(), RenameScanStatus::Unsupported);

        let cancelled = ScanCancellation::new();
        cancelled.cancel();
        let cancelled_result = scan_with_backend(&backend, request(root, false, 3), &cancelled);
        assert_eq!(cancelled_result.status(), RenameScanStatus::Cancelled);

        backend.unsupported.lock().unwrap().clear();
        backend.resource_limited.store(true, Ordering::Release);
        let limited =
            scan_with_backend(&backend, request(root, false, 4), &ScanCancellation::new());
        assert_eq!(limited.status(), RenameScanStatus::ResourceLimit);
    }

    #[test]
    fn fresh_recheck_detects_same_path_replacement_and_root_or_parent_substitution() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        let nested = backend.add_directory(root, "nested", 2);
        let source_path = backend.add_file(&nested, "source.txt", 3);
        let scanned =
            scan_with_backend(&backend, request(root, true, 22), &ScanCancellation::new());
        assert_eq!(scanned.status(), RenameScanStatus::Complete);
        let accepted = scanned.accept(RenameScanGeneration(22)).unwrap();
        let source_ref = accepted
            .snapshot
            .entries()
            .iter()
            .find(|entry| entry.source_path() == source_path)
            .unwrap()
            .entry_ref();
        let selected = accepted.with_selection(source_ref, true).unwrap();
        let rules = RenameRuleSet::new(
            batch_rename::ExtensionPolicy::Preserve,
            vec![batch_rename::RenameRule::Suffix("-renamed".into())],
        );
        let plan = selected.preview(&rules).verified_plan().unwrap();
        let current = recheck_with_backend(
            &backend,
            &plan.fresh_recheck_input(),
            &ScanCancellation::new(),
        );
        assert_eq!(current.root_status(), FreshCheckStatus::Current);
        assert_eq!(
            current.entries()[0].source_status(),
            FreshCheckStatus::Current
        );
        assert_eq!(
            current.entries()[0].destination(),
            &DestinationOccupancy::Vacant
        );

        backend.change_identity(&source_path, 33);
        let replaced = recheck_with_backend(
            &backend,
            &plan.fresh_recheck_input(),
            &ScanCancellation::new(),
        );
        assert_eq!(replaced.root_status(), FreshCheckStatus::Current);
        assert_eq!(
            replaced.entries()[0].source_status(),
            FreshCheckStatus::SourceChanged
        );

        backend.change_identity(&source_path, 3);
        backend.change_identity(&nested, 44);
        let parent_replaced = recheck_with_backend(
            &backend,
            &plan.fresh_recheck_input(),
            &ScanCancellation::new(),
        );
        assert_eq!(parent_replaced.root_status(), FreshCheckStatus::Current);
        assert_eq!(
            parent_replaced.entries()[0].source_status(),
            FreshCheckStatus::ParentChanged
        );

        backend.change_identity(root, 55);
        let root_replaced = recheck_with_backend(
            &backend,
            &plan.fresh_recheck_input(),
            &ScanCancellation::new(),
        );
        assert_eq!(root_replaced.root_status(), FreshCheckStatus::RootChanged);
        assert_eq!(
            root_replaced.entries()[0].source_status(),
            FreshCheckStatus::RootChanged
        );
    }

    #[test]
    fn m8c_verified_bulk_updates_preserve_the_scan_proof_and_revision() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        backend.add_file(root, "first.txt", 2);
        backend.add_file(root, "second.txt", 3);
        let scanned =
            scan_with_backend(&backend, request(root, false, 61), &ScanCancellation::new());
        let accepted = scanned.accept(RenameScanGeneration(61)).unwrap();
        let refs = accepted
            .snapshot
            .entries()
            .iter()
            .map(|entry| entry.entry_ref())
            .collect::<Vec<_>>();

        let updated = accepted
            .with_entry_updates(&[
                (refs[0], Some(true), None),
                (refs[1], None, Some(EntryInclusion::Excluded)),
            ])
            .unwrap();
        assert_eq!(updated.snapshot.selection_revision(), SelectionRevision(1));
        let rules = RenameRuleSet::new(
            batch_rename::ExtensionPolicy::Preserve,
            vec![batch_rename::RenameRule::Suffix("-renamed".into())],
        );
        let plan = updated.preview(&rules).verified_plan().unwrap();
        assert_eq!(plan.plan().actions().len(), 1);
        let fresh = recheck_with_backend(
            &backend,
            &plan.fresh_recheck_input(),
            &ScanCancellation::new(),
        );
        assert_eq!(fresh.root_status(), FreshCheckStatus::Current);
        assert_eq!(
            fresh.entries()[0].source_status(),
            FreshCheckStatus::Current
        );
    }

    #[test]
    fn fresh_recheck_detects_same_identity_size_and_modified_time_changes() {
        let temp = test_root();
        let root = temp.path();
        let backend = FakeBackend::new_root(root);
        let source_path = backend.add_file(root, "source.txt", 2);
        let scanned =
            scan_with_backend(&backend, request(root, false, 41), &ScanCancellation::new());
        let accepted = scanned.accept(RenameScanGeneration(41)).unwrap();
        let source_ref = accepted.snapshot.entries()[0].entry_ref();
        let selected = accepted.with_selection(source_ref, true).unwrap();
        let rules = RenameRuleSet::new(
            batch_rename::ExtensionPolicy::Preserve,
            vec![batch_rename::RenameRule::Suffix("-renamed".into())],
        );
        let plan = selected.preview(&rules).verified_plan().unwrap();
        let input = plan.fresh_recheck_input();

        backend.change_metadata(&source_path, 2, SystemTime::UNIX_EPOCH);
        let changed_size = recheck_with_backend(&backend, &input, &ScanCancellation::new());
        assert_eq!(
            changed_size.entries()[0].source_status(),
            FreshCheckStatus::SourceChanged
        );

        backend.change_metadata(
            &source_path,
            1,
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1),
        );
        let changed_time = recheck_with_backend(&backend, &input, &ScanCancellation::new());
        assert_eq!(
            changed_time.entries()[0].source_status(),
            FreshCheckStatus::SourceChanged
        );

        backend.change_metadata(&source_path, 1, SystemTime::UNIX_EPOCH);
        let current = recheck_with_backend(&backend, &input, &ScanCancellation::new());
        assert_eq!(
            current.entries()[0].source_status(),
            FreshCheckStatus::Current
        );
        assert_eq!(current.entries()[0].observed_source_length(), Some(1));
        assert_eq!(
            current.entries()[0].observed_source_modified(),
            Some(SystemTime::UNIX_EPOCH)
        );
    }

    #[test]
    fn coordinator_shutdown_rejects_new_work_and_supports_final_join() {
        let temp = test_root();
        let root = ExplicitDirectoryRoot::new(temp.path().to_path_buf()).unwrap();
        let coordinator = RenameScanCoordinator::new().unwrap();
        coordinator.request_shutdown();
        assert!(coordinator.submit(root, false).is_err());
        coordinator.shutdown().unwrap();
    }

    #[test]
    fn m10c_fake_cross_volume_move_never_removes_before_recheck_or_on_failure() {
        use std::cell::{Cell, RefCell};

        let events = RefCell::new(Vec::new());
        let source_exists = Cell::new(true);
        let destination_exists = Cell::new(false);
        let outcome = execute_cross_volume_move_protocol(
            || {
                events.borrow_mut().push("copy");
                events.borrow_mut().push("verify");
                Ok::<(), ()>(())
            },
            |_| {
                events.borrow_mut().push("publish");
                destination_exists.set(true);
                Ok::<_, ()>("published output")
            },
            |_, _| {
                events.borrow_mut().push("fresh checks");
                true
            },
            || false,
            |_, _| {
                events.borrow_mut().push("record published");
                true
            },
            |_, _| {
                events.borrow_mut().push("record source removal");
                true
            },
            || {
                events.borrow_mut().push("source removal");
                false
            },
        )
        .unwrap();
        assert_eq!(
            events.borrow().as_slice(),
            &[
                "copy",
                "verify",
                "publish",
                "record published",
                "fresh checks",
                "record source removal",
                "source removal"
            ]
        );
        assert!(matches!(
            outcome,
            CrossVolumeMoveOutcome::Partial {
                output: "published output",
                reason: "source removal failed; source and verified destination remain"
            }
        ));
        assert!(source_exists.get());
        assert!(destination_exists.get());

        let stale_events = RefCell::new(Vec::new());
        let stale_source_exists = Cell::new(true);
        let stale_destination_exists = Cell::new(false);
        let stale = execute_cross_volume_move_protocol(
            || {
                stale_events.borrow_mut().push("copy");
                stale_events.borrow_mut().push("verify");
                Ok::<(), ()>(())
            },
            |_| {
                stale_events.borrow_mut().push("publish");
                stale_destination_exists.set(true);
                Ok::<_, ()>("published output")
            },
            |_, _| {
                stale_events.borrow_mut().push("fresh checks");
                false
            },
            || false,
            |_, _| {
                stale_events.borrow_mut().push("record published");
                true
            },
            |_, _| {
                stale_events.borrow_mut().push("record source removal");
                true
            },
            || {
                stale_events.borrow_mut().push("source removal");
                stale_source_exists.set(false);
                true
            },
        )
        .unwrap();
        assert_eq!(
            stale_events.borrow().as_slice(),
            &[
                "copy",
                "verify",
                "publish",
                "record published",
                "fresh checks"
            ]
        );
        assert!(matches!(
            stale,
            CrossVolumeMoveOutcome::Partial {
                output: "published output",
                reason: "source or published destination changed before source removal; both remain"
            }
        ));
        assert!(stale_source_exists.get());
        assert!(stale_destination_exists.get());

        let progress_failure_events = RefCell::new(Vec::new());
        let retained_source_exists = Cell::new(true);
        let retained_destination_exists = Cell::new(false);
        let progress_failure = execute_cross_volume_move_protocol(
            || {
                progress_failure_events.borrow_mut().push("copy and verify");
                Ok::<(), ()>(())
            },
            |_| {
                progress_failure_events.borrow_mut().push("publish");
                retained_destination_exists.set(true);
                Ok::<_, ()>("published output")
            },
            |_, _| {
                progress_failure_events.borrow_mut().push("fresh checks");
                true
            },
            || false,
            |_, _| {
                progress_failure_events
                    .borrow_mut()
                    .push("record published");
                true
            },
            |_, _| {
                progress_failure_events
                    .borrow_mut()
                    .push("record source removal failed");
                false
            },
            || {
                progress_failure_events.borrow_mut().push("source removal");
                retained_source_exists.set(false);
                true
            },
        )
        .unwrap();
        assert_eq!(
            progress_failure_events.borrow().as_slice(),
            &[
                "copy and verify",
                "publish",
                "record published",
                "fresh checks",
                "record source removal failed"
            ]
        );
        assert!(matches!(
            progress_failure,
            CrossVolumeMoveOutcome::Partial {
                output: "published output",
                reason: "source removal intent could not be durably recorded; both files remain"
            }
        ));
        assert!(retained_source_exists.get());
        assert!(retained_destination_exists.get());
    }

    #[cfg(windows)]
    #[test]
    fn native_scan_reads_only_a_disposable_tree_and_preserves_sibling_occupancy() {
        use std::fs;

        let fixture_parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion");
        fs::create_dir_all(&fixture_parent).expect("disposable fixture parent");
        let temp = tempfile::tempdir_in(fixture_parent)
            .expect("temporary root under the accessible repository workspace");
        fs::write(temp.path().join("source.txt"), b"fixture").unwrap();
        fs::create_dir(temp.path().join("target.txt")).unwrap();
        fs::create_dir(temp.path().join("nested")).unwrap();
        fs::write(temp.path().join("nested").join("child.txt"), b"fixture").unwrap();
        let selected = ExplicitDirectoryRoot::new(temp.path().to_path_buf()).unwrap();
        let result = scan_rename_root(
            RenameScanRequest::new(selected, true, RenameScanGeneration(1)),
            &ScanCancellation::new(),
        );
        assert_eq!(
            result.status(),
            RenameScanStatus::Complete,
            "{:?}",
            result.summary()
        );
        assert_eq!(result.rows().len(), 4);
        assert!(
            result
                .rows()
                .iter()
                .any(|row| row.path().file_name() == Some(OsStr::new("child.txt")))
        );
        assert!(result.root_identity().is_some());
        assert!(
            result
                .rows()
                .iter()
                .all(|row| row.identity().file_id() != [0; 16])
        );
    }

    #[cfg(windows)]
    #[test]
    fn m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture() {
        use std::fs;
        use std::io::Write;

        let fixture_parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("productivity_expansion")
            .join("hotfolder_native");
        fs::create_dir_all(&fixture_parent).expect("disposable fixture parent");
        let temp = tempfile::Builder::new()
            .prefix("case-")
            .tempdir_in(&fixture_parent)
            .expect("temporary root under the accessible repository workspace");
        let watched_path = temp.path().join("incoming");
        let destination_path = temp.path().join("destination");
        fs::create_dir(&watched_path).expect("watched fixture root");
        fs::create_dir(&destination_path).expect("destination fixture root");
        let watched = admit_watch_root(&watched_path).expect("admit fixture root");
        let destination = admit_watch_root(&destination_path).expect("admit destination");
        let cancellation = ScanCancellation::new();

        let fingerprint = |path: &Path| {
            let observed = observe_watch_file(&watched, path, false).expect("observe fixture file");
            fingerprint_hotfolder_file(
                &watched,
                path,
                observed.root_identity(),
                observed.parent_identity(),
                observed.file_identity(),
                observed.length(),
                observed.modified(),
                &cancellation,
            )
            .expect("fingerprint fixture file")
        };

        let bytes = b"native hot folder fixture payload";
        let mut ignore_progress =
            |_progress: NativeHotFolderProgress| -> std::io::Result<()> { Ok(()) };
        let copy_source = watched_path.join("copy-source.bin");
        fs::write(&copy_source, bytes).expect("write copy source");
        let copy_input = fingerprint(&copy_source);
        let copy_result = execute_hotfolder_native_action(
            &watched,
            &copy_source,
            copy_input,
            NativeHotFolderAction::Copy,
            Some(&destination),
            OsStr::new("copied.bin"),
            0x101,
            &mut ignore_progress,
            &cancellation,
        )
        .expect("copy without replacing a destination");
        let copied_path = destination_path.join("copied.bin");
        assert_eq!(fs::read(&copied_path).unwrap(), bytes);
        assert!(copy_source.exists());
        assert_eq!(copy_result.outputs.len(), 1);
        assert_eq!(copy_result.phase, NativeHotFolderPhase::Complete);

        let collision = execute_hotfolder_native_action(
            &watched,
            &copy_source,
            copy_input,
            NativeHotFolderAction::Copy,
            Some(&destination),
            OsStr::new("copied.bin"),
            0x102,
            &mut ignore_progress,
            &cancellation,
        )
        .expect_err("existing destination is a collision");
        assert_eq!(
            collision.kind,
            NativeHotFolderFailureKind::DestinationCollision
        );
        assert_eq!(fs::read(&copied_path).unwrap(), bytes);

        let move_source = watched_path.join("move-source.bin");
        fs::write(&move_source, bytes).expect("write move source");
        let move_result = execute_hotfolder_native_action(
            &watched,
            &move_source,
            fingerprint(&move_source),
            NativeHotFolderAction::Move,
            Some(&destination),
            OsStr::new("moved.bin"),
            0x103,
            &mut ignore_progress,
            &cancellation,
        )
        .expect("same-volume move");
        let moved_path = destination_path.join("moved.bin");
        assert!(!move_source.exists());
        assert_eq!(fs::read(&moved_path).unwrap(), bytes);
        assert_eq!(move_result.outputs[0].identity, move_result.source_identity);

        let rename_source = watched_path.join("rename-source.bin");
        fs::write(&rename_source, bytes).expect("write rename source");
        let rename_result = execute_hotfolder_native_action(
            &watched,
            &rename_source,
            fingerprint(&rename_source),
            NativeHotFolderAction::Rename,
            None,
            OsStr::new("renamed.bin"),
            0x104,
            &mut ignore_progress,
            &cancellation,
        )
        .expect("same-volume rename");
        let renamed_path = watched_path.join("renamed.bin");
        assert!(!rename_source.exists());
        assert_eq!(fs::read(&renamed_path).unwrap(), bytes);
        assert_eq!(
            rename_result.outputs[0].identity,
            rename_result.source_identity
        );

        let stream_source = watched_path.join("stream-source.bin");
        fs::write(&stream_source, bytes).expect("write stream source");
        let stream_path =
            PathBuf::from(format!("{}:hotfolder-test-stream", stream_source.display()));
        if let Ok(mut stream) = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stream_path)
        {
            stream.write_all(b"stream").expect("write named stream");
            drop(stream);
            let observed = observe_watch_file(&watched, &stream_source, false)
                .expect("observe file with named stream");
            let refusal = fingerprint_hotfolder_file(
                &watched,
                &stream_source,
                observed.root_identity(),
                observed.parent_identity(),
                observed.file_identity(),
                observed.length(),
                observed.modified(),
                &cancellation,
            )
            .expect_err("unsupported stream fidelity must be refused before mutation");
            assert_eq!(
                refusal.kind,
                NativeHotFolderFailureKind::UnsupportedFidelity
            );
            assert!(stream_source.exists());
        }
    }
}
