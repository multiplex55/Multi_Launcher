//! Root-confined filesystem proofs and handle-bound file operations.
//!
//! The batch renamer uses this module to build a complete candidate snapshot.
//! Paths and metadata alone never establish identity: Windows IDs are read
//! from opened handles, and the selected root and each candidate's parent
//! chain are reopened and checked while scanning and again before a later
//! mutation boundary.

use crate::batch_rename::{
    self, EntryInclusion, ExplicitDirectoryRoot, RenameInputSnapshot, RenamePlan, RenamePreview,
    RenameRuleSet, RenameSnapshotEntry, SelectionRevision, SnapshotEntryKind, SnapshotEntryRef,
    SnapshotRevision,
};
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_RENAME_SCAN_DIRECTORIES: usize = 4_096;
pub const MAX_RENAME_SCAN_DEPTH: usize = 256;
pub const MAX_RENAME_SCAN_ISSUE_SAMPLES: usize = 64;
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
    pub fn volume_serial_number(self) -> u64 {
        self.volume_serial_number
    }

    pub fn file_id(self) -> [u8; 16] {
        self.file_id
    }
}

#[derive(Debug, Clone)]
pub struct RenameScanRequest {
    root: ExplicitDirectoryRoot,
    scope: RenameScanScope,
    generation: RenameScanGeneration,
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
    entries_seen: usize,
    files_seen: usize,
    directories_seen: usize,
    other_entries_seen: usize,
    reparse_entries_excluded: usize,
    inaccessible_entries: usize,
    issue_count: usize,
    issues_truncated: bool,
    issues: Vec<RenameScanIssue>,
}

impl RenameScanSummary {
    pub fn directories_scanned(&self) -> usize {
        self.directories_scanned
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
                .reopen_chain(&self.parent_chain, &self.root, DirectoryOpenAccess::Enumerate)
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
}

struct DirectoryFrame<H> {
    directory: Arc<OpenDirectory<H>>,
    guarded_chain: OpenDirectoryChain<H>,
    relative_path: PathBuf,
    chain: Vec<DirectoryProof>,
}

fn scan_with_backend<B: RenameScanBackend>(
    backend: &B,
    request: RenameScanRequest,
    cancellation: &ScanCancellation,
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

    let mut pending_rows = Vec::new();
    let mut directories = VecDeque::new();
    directories.push_back(DirectoryFrame {
        directory: Arc::clone(root_directory),
        guarded_chain: admission.directories.clone(),
        relative_path: PathBuf::new(),
        chain: admission.proof.chain.clone(),
    });
    let mut scan_status = RenameScanStatus::Complete;

    while let Some(frame) = directories.pop_front() {
        if cancellation.is_cancelled() {
            scan_status = RenameScanStatus::Cancelled;
            break;
        }
        if result.summary.directories_scanned >= MAX_RENAME_SCAN_DIRECTORIES {
            scan_status = RenameScanStatus::ResourceLimit;
            record_failure(
                &mut result.summary,
                BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&frame.directory.canonical_path),
                    format!("directory limit of {MAX_RENAME_SCAN_DIRECTORIES} reached"),
                ),
            );
            break;
        }
        if frame.chain.len() > MAX_RENAME_SCAN_DEPTH {
            scan_status = RenameScanStatus::ResourceLimit;
            record_failure(
                &mut result.summary,
                BackendFailure::new(
                    BackendFailureKind::ResourceLimit,
                    Some(&frame.directory.canonical_path),
                    format!("directory depth limit of {MAX_RENAME_SCAN_DEPTH} reached"),
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
        let remaining_entries =
            batch_rename::MAX_SNAPSHOT_ENTRIES.saturating_sub(result.summary.entries_seen);
        let mut names =
            match backend.enumerate_directory(&frame.directory, remaining_entries, cancellation) {
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

        for entry in names {
            if cancellation.is_cancelled() {
                scan_status = RenameScanStatus::Cancelled;
                break;
            }
            if entry.name == OsStr::new(".") || entry.name == OsStr::new("..") {
                continue;
            }
            if result.summary.entries_seen >= batch_rename::MAX_SNAPSHOT_ENTRIES {
                scan_status = RenameScanStatus::ResourceLimit;
                record_failure(
                    &mut result.summary,
                    BackendFailure::new(
                        BackendFailureKind::ResourceLimit,
                        Some(&frame.directory.canonical_path),
                        format!(
                            "entry limit of {} reached",
                            batch_rename::MAX_SNAPSHOT_ENTRIES
                        ),
                    ),
                );
                break;
            }
            result.summary.entries_seen += 1;
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
            let relative_path = frame.relative_path.join(&entry.name);
            pending_rows.push(PendingRow {
                relative_path: relative_path.clone(),
                path: opened.canonical_path.clone(),
                kind: opened.kind,
                identity: opened.identity,
                parent_identity: frame.directory.identity,
                parent_chain: frame.chain.clone(),
                length: opened.length,
                modified: opened.modified,
                reparse_point: opened.reparse_point,
            });
            match opened.kind {
                SnapshotEntryKind::File => result.summary.files_seen += 1,
                SnapshotEntryKind::Directory => result.summary.directories_seen += 1,
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
            if opened.reparse_point {
                result.summary.reparse_entries_excluded += 1;
                record_issue(
                    &mut result.summary,
                    RenameScanIssue {
                        path: Some(opened.canonical_path.clone()),
                        kind: RenameScanIssueKind::ReparsePointExcluded,
                        message: "reparse point retained as an ineligible sibling blocker; traversal was skipped".into(),
                    },
                );
            }
            if opened.kind == SnapshotEntryKind::Directory
                && admission.proof.recursive
                && !opened.reparse_point
            {
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
                directories.push_back(DirectoryFrame {
                    directory: child_directory,
                    guarded_chain,
                    relative_path,
                    chain: child_chain,
                });
            }
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
    ) {
        if let Err(error) = backend.reopen_chain(
            &admission.proof.chain,
            &admission.proof,
            DirectoryOpenAccess::Metadata,
        ) {
            record_failure(&mut result.summary, error.clone());
            scan_status = status_for_scan_failure(error.kind);
        }
    }

    pending_rows.sort_by(|left, right| {
        batch_rename::compare_paths_windows_ordinal(&left.path, &right.path)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    let mut rows = Vec::with_capacity(pending_rows.len());
    let mut proofs = HashMap::with_capacity(pending_rows.len());
    let mut entries = Vec::with_capacity(pending_rows.len());
    for (index, pending) in pending_rows.into_iter().enumerate() {
        // The row count is capped at MAX_SNAPSHOT_ENTRIES (10,000), so every
        // index is representable in the stable u64 domain reference.
        let reference = SnapshotEntryRef(index as u64);
        let source_order = index as u64;
        rows.push(RenameScanRow {
            entry_ref: reference,
            relative_path: pending.relative_path.clone(),
            path: pending.path.clone(),
            kind: pending.kind,
            identity: pending.identity,
            parent_identity: pending.parent_identity,
            length: pending.length,
            modified: pending.modified,
            reparse_point: pending.reparse_point,
        });
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
        entries.push(
            RenameSnapshotEntry::new(
                reference,
                pending.path,
                source_order,
                EntryInclusion::Included,
                false,
            )
            .with_kind(pending.kind),
        );
    }
    result.rows = rows;
    result.status = scan_status;

    if scan_status == RenameScanStatus::Complete {
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
                return Err(format!("receipt candidate {} has no filename", path.display()));
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
                && batch_rename::compare_paths_windows_ordinal(
                    &opened.canonical_path,
                    path,
                ) == std::cmp::Ordering::Equal
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
            || opened
                .canonical_path
                .parent()
                .is_none_or(|actual_parent| {
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
                batch_rename::compare_paths_windows_ordinal(
                    &source.current_path,
                    &occupied_path,
                ) == std::cmp::Ordering::Equal
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
                    let opened = open_native_object(&current, false)
                        .map_err(|error| classify_root_io(&current, error))?;
                    if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root or one of its ancestors is not a plain directory",
                        ));
                    }
                    if let Some(expected_parent) = prior_canonical.as_deref() {
                        if opened.canonical_path.parent().is_none_or(|parent| {
                            batch_rename::compare_paths_windows_ordinal(parent, expected_parent)
                                != std::cmp::Ordering::Equal
                        }) {
                            return Err(BackendFailure::new(
                                BackendFailureKind::RootChanged,
                                Some(&current),
                                "an ancestor resolved through a different directory chain",
                            ));
                        }
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
                    let opened = open_native_object(&current, false)
                        .map_err(|error| classify_root_io(&current, error))?;
                    if opened.kind != SnapshotEntryKind::Directory || opened.reparse_point {
                        return Err(BackendFailure::new(
                            BackendFailureKind::InvalidRoot,
                            Some(&current),
                            "the selected root or one of its ancestors is not a plain directory",
                        ));
                    }
                    if let Some(expected_parent) = prior_canonical.as_deref() {
                        if opened.canonical_path.parent().is_none_or(|parent| {
                            batch_rename::compare_paths_windows_ordinal(parent, expected_parent)
                                != std::cmp::Ordering::Equal
                        }) {
                            return Err(BackendFailure::new(
                                BackendFailureKind::RootChanged,
                                Some(&current),
                                "an ancestor resolved through a different directory chain",
                            ));
                        }
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
            generation: request.generation,
            chain,
        };
        let directories = self.reopen_chain(&root.chain, &root, DirectoryOpenAccess::Enumerate)?;
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
        cancellation: &ScanCancellation,
    ) -> Result<Vec<EnumeratedName>, BackendFailure> {
        enumerate_native_directory(directory, remaining_entries, cancellation)
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
        let opened = open_native_object(&lookup_path, false)
            .map_err(|error| classify_child_io(&lookup_path, error))?;
        if let Some(expected) = expected {
            if expected.is_reparse_point != opened.reparse_point
                || (!expected.is_reparse_point
                    && expected.is_directory != (opened.kind == SnapshotEntryKind::Directory))
            {
                return Err(BackendFailure::new(
                    BackendFailureKind::ObjectChanged,
                    Some(&lookup_path),
                    "the sibling changed type or reparse status after directory enumeration",
                ));
            }
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
        let listed = open_native_object(&child.lookup_path, true)
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
            if let Some(previous) = previous_canonical.as_deref() {
                if opened.canonical_path.parent().is_none_or(|parent| {
                    batch_rename::compare_paths_windows_ordinal(parent, previous)
                        != std::cmp::Ordering::Equal
                }) {
                    return Err(BackendFailure::new(
                        BackendFailureKind::PathEscapedRoot,
                        Some(&opened.canonical_path),
                        "the reopened directory chain no longer has the verified parent",
                    ));
                }
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
    },
    Apply {
        plan: VerifiedRenamePlan,
        cancellation: ScanCancellation,
    },
    Undo {
        transaction_id: String,
        cancellation: ScanCancellation,
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
}

enum ActiveWorker {
    Scan(RenameScanGeneration, ScanCancellation),
    Apply(ScanCancellation),
    Undo(ScanCancellation),
}

impl ActiveWorker {
    fn cancellation(&self) -> &ScanCancellation {
        match self {
            Self::Scan(_, cancellation)
            | Self::Apply(cancellation)
            | Self::Undo(cancellation) => cancellation,
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
        })
    }

    pub fn submit(
        &self,
        root: ExplicitDirectoryRoot,
        recursive: bool,
    ) -> std::io::Result<RenameScanTicket> {
        let (reply, receiver) = mpsc::channel();
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "rename scan worker state was poisoned",
            )
        })?;
        if state.shutdown {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "rename scan worker is shutting down",
            ));
        }
        if state.active.as_ref().is_some_and(|active| !active.is_scan())
            || state.pending.as_ref().is_some_and(|pending| !pending.is_scan())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "a batch rename operation is in progress",
            ));
        }
        let generation = RenameScanGeneration(state.next_generation);
        state.next_generation = state.next_generation.checked_add(1).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "rename scan generation overflow")
        })?;
        if let Some(active) = &state.active {
            active.cancellation().cancel();
        }
        if let Some(pending) = state.pending.take() {
            cancel_pending_job(pending);
        }
        let cancellation = ScanCancellation::new();
        state.pending = Some(WorkerJob::Scan {
            request: RenameScanRequest::new(root, recursive, generation),
            cancellation: cancellation.clone(),
            reply,
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
        state.pending = Some(WorkerJob::Apply {
            plan,
            cancellation,
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
        state.pending = Some(WorkerJob::Undo {
            transaction_id,
            cancellation,
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
            if let Some(active) = &state.active {
                if !active.is_scan() {
                    active.cancellation().cancel();
                }
            }
            if state.pending.as_ref().is_some_and(|pending| !pending.is_scan())
                && let Some(pending) = state.pending.take()
            {
                pending.cancellation().cancel();
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.operation_active = false;
                    runtime.progress = None;
                    runtime.last_message = Some(
                        "rename was cancelled before any file was moved".to_owned(),
                    );
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
            }
            if let Some(pending) = state.pending.take() {
                pending.cancellation().cancel();
                cancel_pending_job(pending);
                if let Ok(mut runtime) = self.runtime.lock() {
                    runtime.operation_active = false;
                    runtime.progress = None;
                    runtime.last_message = Some(
                        "pending rename work was cancelled during shutdown".to_owned(),
                    );
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
        } => {
            cancellation.cancel();
            let _ = reply.send(empty_scan_result(&request, RenameScanStatus::Cancelled));
        }
        WorkerJob::Apply { cancellation, .. } | WorkerJob::Undo { cancellation, .. } => {
            cancellation.cancel();
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
                        .find(|receipt| {
                            receipt.status().blocks_new_apply()
                        })
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
                    ..
                } => ActiveWorker::Scan(request.generation, cancellation.clone()),
                WorkerJob::Apply { cancellation, .. } => {
                    ActiveWorker::Apply(cancellation.clone())
                }
                WorkerJob::Undo { cancellation, .. } => {
                    ActiveWorker::Undo(cancellation.clone())
                }
            });
            job
        };
        match job {
            WorkerJob::Scan {
                request,
                cancellation,
                reply,
            } => {
                let generation = request.generation;
                let result = scan_rename_root(request.clone(), &cancellation);
                let _ = reply.send(result);
                clear_active_worker(&state, |active| {
                    matches!(active, ActiveWorker::Scan(active_generation, _) if *active_generation == generation)
                });
            }
            WorkerJob::Apply {
                plan,
                cancellation,
            } => {
                let result = if let Some(path) = journal_path.as_deref() {
                    let report_runtime = Arc::clone(&runtime);
                    Some(crate::batch_rename_transaction::execute_apply(
                        plan,
                        path,
                        &cancellation,
                        move |progress| {
                            if let Ok(mut runtime) = report_runtime.lock() {
                                runtime.progress = Some(progress);
                            }
                        },
                    ))
                } else {
                    None
                };
                clear_active_worker(&state, |active| matches!(active, ActiveWorker::Apply(_)));
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
            } => {
                let result = if let Some(path) = journal_path.as_deref() {
                    let report_runtime = Arc::clone(&runtime);
                    Some(crate::batch_rename_transaction::execute_undo(
                        &transaction_id,
                        path,
                        &cancellation,
                        move |progress| {
                            if let Ok(mut runtime) = report_runtime.lock() {
                                runtime.progress = Some(progress);
                            }
                        },
                    ))
                } else {
                    None
                };
                clear_active_worker(&state, |active| matches!(active, ActiveWorker::Undo(_)));
                if let Some(result) = result {
                    publish_operation_result(&runtime, result);
                } else if let Ok(mut runtime) = runtime.lock() {
                    runtime.operation_active = false;
                    runtime.last_message = Some("rename worker has no recovery journal".to_owned());
                }
            }
        }
        let (lock, _) = &*state;
        if let Ok(mut state) = lock.lock() {
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
        || batch_rename::compare_paths_windows_ordinal(
            &admission.proof.canonical_path,
            root_path,
        ) != std::cmp::Ordering::Equal
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
    let relative = if batch_rename::compare_paths_windows_ordinal(
        &root.canonical_path,
        parent_path,
    ) == std::cmp::Ordering::Equal
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
        let opened = open_native_object(&current, true).map_err(|error| error.to_string())?;
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
            || batch_rename::compare_paths_windows_ordinal(
                &directory.canonical_path,
                parent_path,
            ) != std::cmp::Ordering::Equal
    }) {
        return Err("the recorded parent directory identity changed".to_owned());
    }
    Ok(chain)
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
        native_contextual_io_error("CreateFileW(DELETE | FILE_READ_DATA)", windows_error_to_io(error))
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
        native_contextual_io_error("CreateFileW(read-only receipt verification)", windows_error_to_io(error))
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
        || !matches!(Path::new(leaf).components().next(), Some(Component::Normal(_)))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "handle-based rename requires a single filename component",
        ));
    }
    let destination_path = parent.canonical_path.join(leaf);
    let name = extended_local_path(&destination_path)?;
    if name.is_empty() || name.iter().any(|unit| *unit == 0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination path is empty or contains NUL",
        ));
    }
    let name_bytes = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "filename too long"))?;
    let terminated_name_bytes = (name_bytes as usize)
        .checked_add(size_of::<u16>())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "rename buffer too large")
        })?;
    let mut name = name;
    name.push(0);
    let mut info = FILE_RENAME_INFO::default();
    // SAFETY: this union member is the documented legacy BOOLEAN indicating
    // that an existing target must not be replaced.
    unsafe {
        info.Anonymous.ReplaceIfExists = BOOLEAN(0);
    }
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
fn open_native_object(
    path: &Path,
    enumerate_children: bool,
) -> std::io::Result<OpenedObject<std::fs::File>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

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
    let flags = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS;
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
        handle: file,
    })
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
fn native_file_identity(file: &std::fs::File) -> std::io::Result<NativeFileIdentity> {
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
            if file_name_length % std::mem::size_of::<u16>() != 0 {
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
                        format!(
                            "entry limit of {} reached",
                            batch_rename::MAX_SNAPSHOT_ENTRIES
                        ),
                    ));
                }
                entries.push(EnumeratedName {
                    name,
                    is_directory: attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
                    is_reparse_point: attributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
                });
            }
            if next_offset == 0 {
                break;
            }
            if next_offset < offset_of!(FILE_ID_BOTH_DIR_INFO, FileName)
                || next_offset % std::mem::align_of::<u64>() != 0
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
        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::Interrupted
        );
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
        resource_limited: AtomicBool,
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
            let object = self.object(&path)?;
            if let Some(expected) = expected {
                if expected.is_reparse_point != object.reparse_point
                    || (!expected.is_reparse_point
                        && expected.is_directory != (object.kind == SnapshotEntryKind::Directory))
                {
                    return Err(BackendFailure::new(
                        BackendFailureKind::ObjectChanged,
                        Some(&path),
                        "fake entry changed after enumeration",
                    ));
                }
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
        assert_eq!(
            updated.snapshot.selection_revision(),
            SelectionRevision(1)
        );
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
        assert_eq!(fresh.entries()[0].source_status(), FreshCheckStatus::Current);
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
}
