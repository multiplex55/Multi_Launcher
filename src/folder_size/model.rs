use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::batch_rename::SnapshotEntryKind;
use crate::file_operations::{
    MetadataRootControl, MetadataScanControl, MetadataScanEntry, MetadataScanEntryState,
    MetadataScanObserver, NativeFileIdentity, RenameScanIssue, RenameScanIssueKind,
    RenameScanStatus,
};

pub const MAX_FOLDER_SIZE_ENTRIES: usize = 100_000;
pub const MAX_FOLDER_SIZE_DIRECTORIES: usize = 2_048;
pub const MAX_FOLDER_SIZE_DEPTH: usize = 64;
pub const MAX_FOLDER_SIZE_ISSUE_EXAMPLES: usize = 64;
pub const MAX_FOLDER_SIZE_PATH_BYTES: usize = 32 * 1024 * 1024;
// The 32 MiB aggregate path budget is partitioned as follows: 3 MiB for live
// scanner paths, up to 4 MiB for its 64 path-bearing issue samples (each path
// is bounded by the Windows path limit), 8 MiB each for the final tree and
// cumulative progress row chunks, 4 MiB for accumulator issue paths, and
// 4 MiB for the final issue list. The remaining 1 MiB covers root-path copies
// and bounded path bookkeeping.
pub(crate) const MAX_FOLDER_SIZE_RESULT_PATH_BYTES: usize = 8 * 1024 * 1024;
const MAX_FOLDER_SIZE_ISSUE_PATH_BYTES: usize = 4 * 1024 * 1024;
const MAX_FOLDER_SIZE_ISSUE_MESSAGE_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSizeStatus {
    Complete,
    Partial,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FolderSizeProgress {
    generation: u64,
    publication_id: u64,
    entries_seen: usize,
    files_seen: usize,
    directories_seen: usize,
    directories_scanned: usize,
    reparse_entries_excluded: usize,
    cloud_placeholders_excluded: usize,
    inaccessible_entries: usize,
    total_logical_bytes: u64,
    rows: Option<Arc<FolderSizeProgressRows>>,
    directory_overlay: Arc<Vec<DirectoryUpdate>>,
}

#[derive(Debug)]
pub struct FolderSizeProgressRows {
    previous: Option<Arc<FolderSizeProgressRows>>,
    first_index: usize,
    added: Vec<FolderSizeEntry>,
    len: usize,
}

#[derive(Debug, Clone, Copy)]
struct DirectoryUpdate {
    index: usize,
    identity: Option<NativeFileIdentity>,
    kind: FolderSizeEntryKind,
    logical_bytes: Option<u64>,
    subtree_logical_bytes: u64,
    directory_enumerated: bool,
    incomplete: bool,
    subtree_complete: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FolderSizeProgressEntry<'a> {
    entry: &'a FolderSizeEntry,
    update: Option<&'a DirectoryUpdate>,
}

impl FolderSizeProgressEntry<'_> {
    pub fn path(&self) -> &Path {
        self.entry.path()
    }

    pub fn parent_index(&self) -> Option<usize> {
        self.entry.parent_index()
    }

    pub fn identity(&self) -> Option<NativeFileIdentity> {
        self.update
            .map_or_else(|| self.entry.identity(), |update| update.identity)
    }

    pub fn kind(&self) -> FolderSizeEntryKind {
        self.update
            .map_or_else(|| self.entry.kind(), |update| update.kind)
    }

    pub fn logical_bytes(&self) -> Option<u64> {
        self.update
            .map_or_else(|| self.entry.logical_bytes(), |update| update.logical_bytes)
    }

    pub fn subtree_logical_bytes(&self) -> u64 {
        self.update.map_or_else(
            || self.entry.subtree_logical_bytes(),
            |update| update.subtree_logical_bytes,
        )
    }

    pub fn is_complete(&self) -> bool {
        self.update.map_or_else(
            || self.entry.is_complete(),
            |update| match update.kind {
                FolderSizeEntryKind::Directory => update.subtree_complete,
                FolderSizeEntryKind::File => true,
                FolderSizeEntryKind::Omitted(_) => false,
            },
        )
    }
}

impl FolderSizeProgressRows {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Visit the bounded cumulative rows in the same order as the eventual
    /// immutable result, without cloning the retained prefix.
    fn for_each(
        &self,
        directory_overlay: &[DirectoryUpdate],
        mut visit: impl FnMut(FolderSizeProgressEntry<'_>),
    ) {
        let mut chunks = Vec::new();
        let mut current = Some(self);
        while let Some(chunk) = current {
            chunks.push(chunk);
            current = chunk.previous.as_deref();
        }
        let mut latest_updates = HashMap::new();
        for update in directory_overlay {
            latest_updates.insert(update.index, update);
        }
        for chunk in chunks.into_iter().rev() {
            for (offset, entry) in chunk.added.iter().enumerate() {
                let index = chunk.first_index + offset;
                visit(FolderSizeProgressEntry {
                    entry,
                    update: latest_updates.get(&index).copied(),
                });
            }
        }
    }
}

impl FolderSizeProgress {
    #[cfg(test)]
    pub(crate) fn test_with_total(mut self, total: u64) -> Self {
        self.total_logical_bytes = total;
        self.publication_id = self.publication_id.saturating_add(1);
        self
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Monotonic within a scan generation, including directory subtotal
    /// overlays that do not add rows.
    pub fn publication_id(&self) -> u64 {
        self.publication_id
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

    pub fn directories_scanned(&self) -> usize {
        self.directories_scanned
    }

    pub fn reparse_entries_excluded(&self) -> usize {
        self.reparse_entries_excluded
    }

    pub fn cloud_placeholders_excluded(&self) -> usize {
        self.cloud_placeholders_excluded
    }

    pub fn inaccessible_entries(&self) -> usize {
        self.inaccessible_entries
    }

    pub fn total_logical_bytes(&self) -> u64 {
        self.total_logical_bytes
    }

    pub fn rows(&self) -> Option<&Arc<FolderSizeProgressRows>> {
        self.rows.as_ref()
    }

    pub fn for_each_entry(&self, visit: impl FnMut(FolderSizeProgressEntry<'_>)) {
        if let Some(rows) = self.rows.as_deref() {
            rows.for_each(&self.directory_overlay, visit);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSizeOmission {
    ReparsePoint,
    CloudPlaceholder,
    Inaccessible,
    UnsupportedEntry,
    UnsupportedIdentity,
    RepeatedDirectoryIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSizeEntryKind {
    Directory,
    File,
    Omitted(FolderSizeOmission),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSizeEntry {
    path: PathBuf,
    parent_index: Option<usize>,
    identity: Option<NativeFileIdentity>,
    kind: FolderSizeEntryKind,
    logical_bytes: Option<u64>,
    subtree_logical_bytes: u64,
    directory_enumerated: bool,
    incomplete: bool,
}

impl FolderSizeEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Index of the parent directory in `FolderSizeResult::entries`.
    pub fn parent_index(&self) -> Option<usize> {
        self.parent_index
    }

    pub fn identity(&self) -> Option<NativeFileIdentity> {
        self.identity
    }

    pub fn kind(&self) -> FolderSizeEntryKind {
        self.kind
    }

    /// Logical length for a file. Directory entries use `subtree_logical_bytes`.
    pub fn logical_bytes(&self) -> Option<u64> {
        self.logical_bytes
    }

    pub fn subtree_logical_bytes(&self) -> u64 {
        self.subtree_logical_bytes
    }

    pub fn is_complete(&self) -> bool {
        match self.kind {
            FolderSizeEntryKind::Directory => self.directory_enumerated && !self.incomplete,
            FolderSizeEntryKind::File => true,
            FolderSizeEntryKind::Omitted(_) => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderSizeIssueKind {
    Inaccessible,
    UnsupportedIdentity,
    ReparsePointExcluded,
    RootChanged,
    ParentChanged,
    PathEscapedRoot,
    InvalidEntry,
    ResourceLimit,
    AccountingOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSizeIssue {
    path: Option<PathBuf>,
    kind: FolderSizeIssueKind,
    message: String,
}

impl FolderSizeIssue {
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn kind(&self) -> FolderSizeIssueKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSizeSummary {
    entries_seen: usize,
    files_seen: usize,
    directories_seen: usize,
    directories_admitted: usize,
    directories_scanned: usize,
    reparse_entries_excluded: usize,
    cloud_placeholders_excluded: usize,
    inaccessible_entries: usize,
    issue_count: usize,
    issues_truncated: bool,
}

impl FolderSizeSummary {
    pub fn entries_seen(&self) -> usize {
        self.entries_seen
    }

    pub fn files_seen(&self) -> usize {
        self.files_seen
    }

    pub fn directories_seen(&self) -> usize {
        self.directories_seen
    }

    pub fn directories_admitted(&self) -> usize {
        self.directories_admitted
    }

    pub fn directories_scanned(&self) -> usize {
        self.directories_scanned
    }

    pub fn reparse_entries_excluded(&self) -> usize {
        self.reparse_entries_excluded
    }

    pub fn cloud_placeholders_excluded(&self) -> usize {
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
}

#[derive(Debug, PartialEq, Eq)]
pub struct FolderSizeResult {
    generation: u64,
    root: PathBuf,
    root_identity: Option<NativeFileIdentity>,
    status: FolderSizeStatus,
    total_logical_bytes: u64,
    entries: Vec<FolderSizeEntry>,
    summary: FolderSizeSummary,
    issues: Vec<FolderSizeIssue>,
}

impl FolderSizeResult {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_identity(&self) -> Option<NativeFileIdentity> {
        self.root_identity
    }

    pub fn status(&self) -> FolderSizeStatus {
        self.status
    }

    pub fn total_logical_bytes(&self) -> u64 {
        self.total_logical_bytes
    }

    pub fn entries(&self) -> &[FolderSizeEntry] {
        &self.entries
    }

    pub fn summary(&self) -> &FolderSizeSummary {
        &self.summary
    }

    pub fn issues(&self) -> &[FolderSizeIssue] {
        &self.issues
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    EntryLimit,
    DirectoryLimit,
    DepthLimit,
    PathLimit,
    AccountingOverflow,
    InvalidParent,
    UnsupportedIdentity,
}

pub(crate) struct FolderSizeAccumulator<'a> {
    root: PathBuf,
    root_identity: Option<NativeFileIdentity>,
    entries: Vec<FolderSizeEntry>,
    directory_by_identity: HashMap<NativeFileIdentity, usize>,
    path_bytes: usize,
    directory_count: usize,
    total_logical_bytes: u64,
    stop_reason: Option<StopReason>,
    local_issues: Vec<FolderSizeIssue>,
    local_issue_count: usize,
    local_issue_path_bytes: usize,
    issues_truncated: bool,
    generation: u64,
    progress_sink: &'a mut dyn FnMut(FolderSizeProgress),
    progress_rows: Option<Arc<FolderSizeProgressRows>>,
    published_entries: usize,
    progress_directory_indices: Vec<usize>,
    progress_events_since_publish: usize,
    progress_publication_id: u64,
}

impl<'a> FolderSizeAccumulator<'a> {
    pub(crate) fn new(
        selected_root: PathBuf,
        generation: u64,
        progress_sink: &'a mut dyn FnMut(FolderSizeProgress),
    ) -> Self {
        Self {
            root: selected_root,
            root_identity: None,
            entries: Vec::new(),
            directory_by_identity: HashMap::new(),
            path_bytes: 0,
            directory_count: 0,
            total_logical_bytes: 0,
            stop_reason: None,
            local_issues: Vec::new(),
            local_issue_count: 0,
            local_issue_path_bytes: 0,
            issues_truncated: false,
            generation,
            progress_sink,
            progress_rows: None,
            published_entries: 0,
            progress_directory_indices: Vec::new(),
            progress_events_since_publish: 0,
            progress_publication_id: 0,
        }
    }

    fn admit_root(&mut self, path: &Path, identity: NativeFileIdentity) -> bool {
        if self.entries.len() >= MAX_FOLDER_SIZE_ENTRIES
            || path_size(path).is_none_or(|bytes| bytes > MAX_FOLDER_SIZE_RESULT_PATH_BYTES)
        {
            self.stop_reason = Some(if self.entries.len() >= MAX_FOLDER_SIZE_ENTRIES {
                StopReason::EntryLimit
            } else {
                StopReason::PathLimit
            });
            return false;
        }
        if identity.file_id() == [0; 16] {
            self.stop_reason = Some(StopReason::UnsupportedIdentity);
            return false;
        }
        let Some(bytes) = path_size(path) else {
            self.stop_reason = Some(StopReason::PathLimit);
            return false;
        };
        self.root = path.to_path_buf();
        self.root_identity = Some(identity);
        self.path_bytes = bytes;
        self.directory_count = 1;
        self.directory_by_identity.insert(identity, 0);
        self.entries.push(FolderSizeEntry {
            path: path.to_path_buf(),
            parent_index: None,
            identity: Some(identity),
            kind: FolderSizeEntryKind::Directory,
            logical_bytes: None,
            subtree_logical_bytes: 0,
            directory_enumerated: false,
            incomplete: false,
        });
        self.progress_directory_indices.push(0);
        true
    }

    fn observe_entry(&mut self, event: MetadataScanEntry<'_>) -> MetadataScanControl {
        let Some(&parent_index) = self.directory_by_identity.get(&event.parent_identity) else {
            self.stop_reason = Some(StopReason::InvalidParent);
            return MetadataScanControl::Stop;
        };
        if self.entries[parent_index].path != event.parent_path {
            self.stop_reason = Some(StopReason::InvalidParent);
            return MetadataScanControl::Stop;
        }
        if event.depth > MAX_FOLDER_SIZE_DEPTH {
            self.stop_reason = Some(StopReason::DepthLimit);
            return MetadataScanControl::Stop;
        }
        if self.entries.len() >= MAX_FOLDER_SIZE_ENTRIES {
            self.stop_reason = Some(StopReason::EntryLimit);
            return MetadataScanControl::Stop;
        }
        let Some(path_size) = path_size(event.path) else {
            self.stop_reason = Some(StopReason::PathLimit);
            return MetadataScanControl::Stop;
        };
        let Some(path_bytes) = self.path_bytes.checked_add(path_size) else {
            self.stop_reason = Some(StopReason::PathLimit);
            return MetadataScanControl::Stop;
        };
        if path_bytes > MAX_FOLDER_SIZE_RESULT_PATH_BYTES {
            self.stop_reason = Some(StopReason::PathLimit);
            return MetadataScanControl::Stop;
        }

        if event.state == MetadataScanEntryState::Verified
            && event.kind == SnapshotEntryKind::Directory
        {
            let Some(identity) = event.identity else {
                self.stop_reason = Some(StopReason::InvalidParent);
                return MetadataScanControl::Stop;
            };
            if identity.file_id() == [0; 16] {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::UnsupportedIdentity,
                    Some(identity),
                    path_size,
                );
                self.mark_incomplete(parent_index);
                return MetadataScanControl::SkipDescendants;
            }
            if self.directory_by_identity.contains_key(&identity) {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::RepeatedDirectoryIdentity,
                    Some(identity),
                    path_size,
                );
                self.mark_incomplete(parent_index);
                self.record_local_issue(
                    Some(event.path),
                    FolderSizeIssueKind::UnsupportedIdentity,
                    "a directory identity was encountered again; descendants were skipped to prevent a cycle",
                );
                return MetadataScanControl::SkipDescendants;
            }
            if self.directory_count >= MAX_FOLDER_SIZE_DIRECTORIES {
                self.stop_reason = Some(StopReason::DirectoryLimit);
                return MetadataScanControl::Stop;
            }
            let index = self.push_entry(
                event.path,
                Some(parent_index),
                Some(identity),
                FolderSizeEntryKind::Directory,
                None,
                path_size,
            );
            self.directory_count += 1;
            self.directory_by_identity.insert(identity, index);
            self.progress_directory_indices.push(index);
            return MetadataScanControl::Continue;
        }

        match event.state {
            MetadataScanEntryState::CloudPlaceholderExcluded => {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::CloudPlaceholder,
                    None,
                    path_size,
                );
                self.mark_incomplete(parent_index);
                MetadataScanControl::SkipDescendants
            }
            MetadataScanEntryState::ReparseExcluded => {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::ReparsePoint,
                    None,
                    path_size,
                );
                self.mark_incomplete(parent_index);
                MetadataScanControl::SkipDescendants
            }
            MetadataScanEntryState::Inaccessible => {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::Inaccessible,
                    None,
                    path_size,
                );
                self.mark_incomplete(parent_index);
                MetadataScanControl::SkipDescendants
            }
            MetadataScanEntryState::UnsupportedIdentity => {
                self.push_omission(
                    event.path,
                    Some(parent_index),
                    FolderSizeOmission::UnsupportedIdentity,
                    None,
                    path_size,
                );
                self.mark_incomplete(parent_index);
                MetadataScanControl::SkipDescendants
            }
            MetadataScanEntryState::Verified => match event.kind {
                SnapshotEntryKind::File => {
                    let Some(length) = event.logical_length else {
                        self.stop_reason = Some(StopReason::InvalidParent);
                        self.mark_incomplete(parent_index);
                        return MetadataScanControl::Stop;
                    };
                    if !self.add_file_totals(parent_index, length) {
                        self.stop_reason = Some(StopReason::AccountingOverflow);
                        return MetadataScanControl::Stop;
                    }
                    self.push_entry(
                        event.path,
                        Some(parent_index),
                        event.identity,
                        FolderSizeEntryKind::File,
                        Some(length),
                        path_size,
                    );
                    MetadataScanControl::Continue
                }
                SnapshotEntryKind::Other => {
                    self.push_omission(
                        event.path,
                        Some(parent_index),
                        FolderSizeOmission::UnsupportedEntry,
                        event.identity,
                        path_size,
                    );
                    self.mark_incomplete(parent_index);
                    MetadataScanControl::SkipDescendants
                }
                SnapshotEntryKind::Directory => {
                    self.stop_reason = Some(StopReason::InvalidParent);
                    MetadataScanControl::Stop
                }
            },
        }
    }

    fn push_entry(
        &mut self,
        path: &Path,
        parent_index: Option<usize>,
        identity: Option<NativeFileIdentity>,
        kind: FolderSizeEntryKind,
        logical_bytes: Option<u64>,
        path_bytes: usize,
    ) -> usize {
        self.path_bytes = self.path_bytes.saturating_add(path_bytes);
        let index = self.entries.len();
        self.entries.push(FolderSizeEntry {
            path: path.to_path_buf(),
            parent_index,
            identity,
            kind,
            logical_bytes,
            subtree_logical_bytes: 0,
            directory_enumerated: false,
            incomplete: matches!(kind, FolderSizeEntryKind::Omitted(_)),
        });
        index
    }

    fn push_omission(
        &mut self,
        path: &Path,
        parent_index: Option<usize>,
        reason: FolderSizeOmission,
        identity: Option<NativeFileIdentity>,
        path_bytes: usize,
    ) {
        self.push_entry(
            path,
            parent_index,
            identity,
            FolderSizeEntryKind::Omitted(reason),
            None,
            path_bytes,
        );
    }

    fn add_file_totals(&mut self, parent_index: usize, length: u64) -> bool {
        let Some(total) = self.total_logical_bytes.checked_add(length) else {
            return false;
        };
        let mut cursor = Some(parent_index);
        while let Some(index) = cursor {
            if self.entries[index]
                .subtree_logical_bytes
                .checked_add(length)
                .is_none()
            {
                return false;
            }
            cursor = self.entries[index].parent_index;
        }
        self.total_logical_bytes = total;
        let mut cursor = Some(parent_index);
        while let Some(index) = cursor {
            self.entries[index].subtree_logical_bytes += length;
            cursor = self.entries[index].parent_index;
        }
        true
    }

    fn mark_incomplete(&mut self, mut cursor: usize) {
        loop {
            let entry = &mut self.entries[cursor];
            entry.incomplete = true;
            let Some(parent) = entry.parent_index else {
                break;
            };
            cursor = parent;
        }
    }

    fn mark_enumerated(&mut self, identity: NativeFileIdentity) {
        if let Some(index) = self.directory_by_identity.get(&identity).copied() {
            self.entries[index].directory_enumerated = true;
        }
    }

    fn mark_cloud_placeholder(&mut self, identity: NativeFileIdentity) {
        let Some(index) = self.directory_by_identity.remove(&identity) else {
            self.stop_reason = Some(StopReason::InvalidParent);
            return;
        };
        let Some(parent_index) = self.entries[index].parent_index else {
            self.stop_reason = Some(StopReason::InvalidParent);
            return;
        };
        let entry = &mut self.entries[index];
        entry.identity = None;
        entry.kind = FolderSizeEntryKind::Omitted(FolderSizeOmission::CloudPlaceholder);
        entry.logical_bytes = None;
        entry.directory_enumerated = false;
        entry.incomplete = true;
        self.mark_incomplete(parent_index);
    }

    fn update_progress(&mut self, summary: &crate::file_operations::RenameScanSummary) {
        self.progress_publication_id = self.progress_publication_id.saturating_add(1);
        let first_index = self.published_entries;
        let added = self.entries[first_index..].to_vec();
        self.published_entries = self.entries.len();
        if !added.is_empty() {
            let previous_len = self.progress_rows.as_ref().map_or(0, |rows| rows.len);
            self.progress_rows = Some(Arc::new(FolderSizeProgressRows {
                previous: self.progress_rows.take(),
                first_index,
                len: previous_len.saturating_add(added.len()),
                added,
            }));
        }
        let mut incomplete_subtrees = HashSet::with_capacity(self.directory_count);
        let mut directory_overlay = Vec::with_capacity(self.progress_directory_indices.len());
        for index in self.progress_directory_indices.iter().rev().copied() {
            let Some(entry) = self.entries.get(index) else {
                continue;
            };
            let subtree_complete = entry.directory_enumerated
                && !entry.incomplete
                && !incomplete_subtrees.contains(&index);
            if !subtree_complete && let Some(parent) = entry.parent_index {
                incomplete_subtrees.insert(parent);
            }
            directory_overlay.push(DirectoryUpdate {
                index,
                identity: entry.identity,
                kind: entry.kind,
                logical_bytes: entry.logical_bytes,
                subtree_logical_bytes: entry.subtree_logical_bytes,
                directory_enumerated: entry.directory_enumerated,
                incomplete: entry.incomplete,
                subtree_complete,
            });
        }
        directory_overlay.reverse();
        let progress = FolderSizeProgress {
            generation: self.generation,
            publication_id: self.progress_publication_id,
            entries_seen: summary.entries_seen(),
            files_seen: summary.files_seen(),
            directories_seen: summary.directories_seen(),
            directories_scanned: summary.directories_scanned(),
            reparse_entries_excluded: summary.reparse_entries_excluded(),
            cloud_placeholders_excluded: summary.cloud_placeholders_excluded(),
            inaccessible_entries: summary.inaccessible_entries(),
            total_logical_bytes: self.total_logical_bytes,
            rows: self.progress_rows.clone(),
            directory_overlay: Arc::new(directory_overlay),
        };
        (self.progress_sink)(progress);
    }

    fn record_local_issue(
        &mut self,
        path: Option<&Path>,
        kind: FolderSizeIssueKind,
        message: &str,
    ) {
        self.local_issue_count = self.local_issue_count.saturating_add(1);
        let issue_path_bytes = path.and_then(path_size).unwrap_or(0);
        if self.local_issues.len() < MAX_FOLDER_SIZE_ISSUE_EXAMPLES
            && self
                .local_issue_path_bytes
                .checked_add(issue_path_bytes)
                .is_some_and(|total| total <= MAX_FOLDER_SIZE_ISSUE_PATH_BYTES)
        {
            self.local_issue_path_bytes += issue_path_bytes;
            self.local_issues.push(FolderSizeIssue {
                path: path.map(Path::to_path_buf),
                kind,
                message: bounded_message(message),
            });
        } else {
            self.issues_truncated = true;
        }
    }

    pub(crate) fn finish(
        mut self,
        status: RenameScanStatus,
        summary: &crate::file_operations::RenameScanSummary,
    ) -> FolderSizeResult {
        let mut issues = Vec::with_capacity(
            summary
                .issues()
                .len()
                .saturating_add(self.local_issues.len())
                .min(MAX_FOLDER_SIZE_ISSUE_EXAMPLES),
        );
        let mut issue_path_bytes = 0usize;
        for issue in summary.issues() {
            let path_bytes = issue.path().and_then(path_size).unwrap_or(0);
            if issues.len() < MAX_FOLDER_SIZE_ISSUE_EXAMPLES
                && issue_path_bytes
                    .checked_add(path_bytes)
                    .is_some_and(|total| total <= MAX_FOLDER_SIZE_ISSUE_PATH_BYTES)
            {
                issues.push(convert_issue(issue));
                issue_path_bytes += path_bytes;
            } else {
                self.issues_truncated = true;
            }
            if let Some(path) = issue.path() {
                self.mark_path_incomplete(path);
            }
        }
        let remaining_issue_slots = MAX_FOLDER_SIZE_ISSUE_EXAMPLES.saturating_sub(issues.len());
        for issue in self.local_issues.iter().take(remaining_issue_slots) {
            let path_bytes = issue.path.as_deref().and_then(path_size).unwrap_or(0);
            if issue_path_bytes
                .checked_add(path_bytes)
                .is_some_and(|total| total <= MAX_FOLDER_SIZE_ISSUE_PATH_BYTES)
            {
                issue_path_bytes += path_bytes;
                issues.push(issue.clone());
            } else {
                self.issues_truncated = true;
            }
        }
        let mut issue_count = summary.issue_count().saturating_add(self.local_issue_count);

        if let Some(reason) = self.stop_reason {
            let (kind, message) = match reason {
                StopReason::EntryLimit => (
                    FolderSizeIssueKind::ResourceLimit,
                    "folder-size entry limit reached",
                ),
                StopReason::DirectoryLimit => (
                    FolderSizeIssueKind::ResourceLimit,
                    "folder-size directory limit reached",
                ),
                StopReason::DepthLimit => (
                    FolderSizeIssueKind::ResourceLimit,
                    "folder-size depth limit reached",
                ),
                StopReason::PathLimit => (
                    FolderSizeIssueKind::ResourceLimit,
                    "folder-size path-storage limit reached",
                ),
                StopReason::AccountingOverflow => (
                    FolderSizeIssueKind::AccountingOverflow,
                    "logical folder-size total overflowed",
                ),
                StopReason::InvalidParent => (
                    FolderSizeIssueKind::InvalidEntry,
                    "folder-size metadata hierarchy was inconsistent",
                ),
                StopReason::UnsupportedIdentity => (
                    FolderSizeIssueKind::UnsupportedIdentity,
                    "the admitted root did not provide a supported directory identity",
                ),
            };
            issue_count = issue_count.saturating_add(1);
            if issues.len() < MAX_FOLDER_SIZE_ISSUE_EXAMPLES {
                issues.push(FolderSizeIssue {
                    path: None,
                    kind,
                    message: message.to_owned(),
                });
            } else {
                self.issues_truncated = true;
            }
        }
        let issues_truncated =
            self.issues_truncated || summary.issues_truncated() || issue_count > issues.len();

        for index in (1..self.entries.len()).rev() {
            if !self.entries[index].is_complete()
                && let Some(parent_index) = self.entries[index].parent_index
            {
                self.mark_incomplete(parent_index);
            }
        }
        let tree_is_complete =
            !self.entries.is_empty() && self.entries.iter().all(FolderSizeEntry::is_complete);

        let mut final_status = match status {
            RenameScanStatus::Complete
                if issue_count == 0 && self.stop_reason.is_none() && tree_is_complete =>
            {
                FolderSizeStatus::Complete
            }
            RenameScanStatus::Cancelled => FolderSizeStatus::Cancelled,
            RenameScanStatus::InvalidRoot
            | RenameScanStatus::Inaccessible
            | RenameScanStatus::Unsupported
            | RenameScanStatus::ChangedDuringScan => FolderSizeStatus::Failed,
            RenameScanStatus::Complete
            | RenameScanStatus::Partial
            | RenameScanStatus::ResourceLimit => FolderSizeStatus::Partial,
        };
        if self.stop_reason == Some(StopReason::AccountingOverflow)
            || self.stop_reason == Some(StopReason::InvalidParent)
            || self.stop_reason == Some(StopReason::UnsupportedIdentity)
        {
            final_status = FolderSizeStatus::Failed;
        }
        if self.entries.is_empty() && final_status != FolderSizeStatus::Cancelled {
            final_status = FolderSizeStatus::Failed;
        }
        if !self.entries.is_empty() && final_status != FolderSizeStatus::Complete {
            self.mark_incomplete(0);
        }
        self.update_progress(summary);
        let summary = FolderSizeSummary {
            entries_seen: summary.entries_seen(),
            files_seen: summary.files_seen(),
            directories_seen: summary.directories_seen(),
            directories_admitted: summary.directories_admitted(),
            directories_scanned: summary.directories_scanned(),
            reparse_entries_excluded: summary.reparse_entries_excluded(),
            cloud_placeholders_excluded: summary.cloud_placeholders_excluded(),
            inaccessible_entries: summary.inaccessible_entries(),
            issue_count,
            issues_truncated,
        };
        FolderSizeResult {
            generation: self.generation,
            root: self.root,
            root_identity: self.root_identity,
            status: final_status,
            total_logical_bytes: self.total_logical_bytes,
            entries: self.entries,
            summary,
            issues,
        }
    }

    fn mark_path_incomplete(&mut self, path: &Path) {
        let mut nearest = None;
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.path == path {
                nearest = Some((index, true));
                break;
            }
            if path.starts_with(&entry.path) && nearest.is_none_or(|(_, exact)| !exact) {
                nearest = Some((index, false));
            }
        }
        if let Some((index, _)) = nearest {
            self.mark_incomplete(index);
        } else if !self.entries.is_empty() {
            self.mark_incomplete(0);
        }
    }
}

impl MetadataScanObserver for FolderSizeAccumulator<'_> {
    fn admitted_root(&mut self, path: &Path, identity: NativeFileIdentity) -> MetadataRootControl {
        if self.admit_root(path, identity) {
            MetadataRootControl::Continue
        } else {
            MetadataRootControl::Stop
        }
    }

    fn entry(&mut self, entry: MetadataScanEntry<'_>) -> MetadataScanControl {
        self.observe_entry(entry)
    }

    fn directory_enumeration_completed(&mut self, identity: NativeFileIdentity) {
        self.mark_enumerated(identity);
    }

    fn directory_became_cloud_placeholder(&mut self, identity: NativeFileIdentity) {
        self.mark_cloud_placeholder(identity);
    }

    fn progress(&mut self, summary: &crate::file_operations::RenameScanSummary) {
        // Directory completion and the shared scanner's entry cadence can
        // both arrive rapidly for wide trees. Coalesce those signals; finish
        // always publishes one final snapshot regardless of this counter.
        self.progress_events_since_publish = self.progress_events_since_publish.saturating_add(1);
        if self.progress_events_since_publish >= 16 {
            self.progress_events_since_publish = 0;
            self.update_progress(summary);
        }
    }
}

fn path_size(path: &Path) -> Option<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().count().checked_mul(2)
    }
    #[cfg(not(windows))]
    {
        Some(path.as_os_str().as_encoded_bytes().len())
    }
}

fn bounded_message(message: &str) -> String {
    if message.len() <= MAX_FOLDER_SIZE_ISSUE_MESSAGE_BYTES {
        return message.to_owned();
    }
    let mut end = MAX_FOLDER_SIZE_ISSUE_MESSAGE_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_owned()
}

fn convert_issue(issue: &RenameScanIssue) -> FolderSizeIssue {
    FolderSizeIssue {
        path: issue.path().map(Path::to_path_buf),
        kind: match issue.kind() {
            RenameScanIssueKind::Inaccessible => FolderSizeIssueKind::Inaccessible,
            RenameScanIssueKind::UnsupportedIdentity => FolderSizeIssueKind::UnsupportedIdentity,
            RenameScanIssueKind::ReparsePointExcluded => FolderSizeIssueKind::ReparsePointExcluded,
            RenameScanIssueKind::RootChanged => FolderSizeIssueKind::RootChanged,
            RenameScanIssueKind::ParentChanged => FolderSizeIssueKind::ParentChanged,
            RenameScanIssueKind::PathEscapedRoot => FolderSizeIssueKind::PathEscapedRoot,
            RenameScanIssueKind::InvalidEntry => FolderSizeIssueKind::InvalidEntry,
            RenameScanIssueKind::ResourceLimit => FolderSizeIssueKind::ResourceLimit,
        },
        message: bounded_message(issue.message()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::OsStr;

    fn verified_entry<'a>(
        parent_path: &'a Path,
        path: &'a Path,
        relative_path: &'a Path,
        name: &'a OsStr,
        parent_identity: NativeFileIdentity,
        kind: SnapshotEntryKind,
        identity: Option<NativeFileIdentity>,
        logical_length: Option<u64>,
        depth: usize,
    ) -> MetadataScanEntry<'a> {
        MetadataScanEntry {
            parent_path,
            parent_identity,
            path,
            relative_path,
            name,
            kind,
            identity,
            logical_length,
            depth,
            state: MetadataScanEntryState::Verified,
        }
    }

    #[derive(Clone, Copy)]
    struct ProgressRowState {
        kind: FolderSizeEntryKind,
        subtree_logical_bytes: u64,
        is_complete: bool,
    }

    fn progress_entry(progress: &FolderSizeProgress, path: &Path) -> ProgressRowState {
        let mut found = None;
        progress.for_each_entry(|entry| {
            if entry.path() == path {
                found = Some(ProgressRowState {
                    kind: entry.kind(),
                    subtree_logical_bytes: entry.subtree_logical_bytes(),
                    is_complete: entry.is_complete(),
                });
            }
        });
        found.expect("the admitted row should be in the cumulative progress view")
    }

    #[test]
    fn folder_size_scan_progress_overlays_later_subtotals_and_cloud_omissions() {
        let root = std::env::temp_dir().join("folder-size-progress-overlay");
        let child = root.join("nested");
        let file = child.join("payload.bin");
        let root_identity = NativeFileIdentity::from_parts(1, [1; 16]);
        let child_identity = NativeFileIdentity::from_parts(1, [2; 16]);
        let snapshots = RefCell::new(Vec::new());
        let mut sink = |progress| snapshots.borrow_mut().push(progress);
        let mut accumulator = FolderSizeAccumulator::new(root.clone(), 11, &mut sink);
        let summary = crate::file_operations::RenameScanSummary::default();

        assert!(accumulator.admit_root(&root, root_identity));
        accumulator.mark_enumerated(root_identity);
        accumulator.update_progress(&summary);
        let initially_published = snapshots.borrow().last().unwrap().clone();
        assert_eq!(
            progress_entry(&initially_published, &root).subtree_logical_bytes,
            0
        );
        assert!(progress_entry(&initially_published, &root).is_complete);

        let relative_child = Path::new("nested");
        accumulator.observe_entry(verified_entry(
            &root,
            &child,
            relative_child,
            OsStr::new("nested"),
            root_identity,
            SnapshotEntryKind::Directory,
            Some(child_identity),
            None,
            1,
        ));
        accumulator.update_progress(&summary);
        let child_admitted = snapshots.borrow().last().unwrap().clone();
        assert_eq!(
            progress_entry(&child_admitted, &root).subtree_logical_bytes,
            0
        );
        assert!(!progress_entry(&child_admitted, &root).is_complete);
        assert!(!progress_entry(&child_admitted, &child).is_complete);

        accumulator.observe_entry(verified_entry(
            &child,
            &file,
            Path::new("nested/payload.bin"),
            OsStr::new("payload.bin"),
            child_identity,
            SnapshotEntryKind::File,
            None,
            Some(17),
            2,
        ));
        accumulator.update_progress(&summary);
        let with_file = snapshots.borrow().last().unwrap().clone();
        assert_eq!(progress_entry(&with_file, &root).subtree_logical_bytes, 17);
        assert_eq!(progress_entry(&with_file, &child).subtree_logical_bytes, 17);
        assert!(!progress_entry(&with_file, &child).is_complete);
        // The old snapshot remains immutable while the later snapshot overlays
        // the newly verified descendant subtotal onto the already-published root.
        assert_eq!(
            progress_entry(&initially_published, &root).subtree_logical_bytes,
            0
        );

        accumulator.mark_enumerated(child_identity);
        accumulator.update_progress(&summary);
        let complete = snapshots.borrow().last().unwrap().clone();
        assert!(progress_entry(&complete, &root).is_complete);
        assert!(progress_entry(&complete, &child).is_complete);
        assert_eq!(progress_entry(&complete, &root).subtree_logical_bytes, 17);

        let omission_root = root.join("cloud-case");
        let omission_child = omission_root.join("placeholder");
        let omission_root_identity = NativeFileIdentity::from_parts(2, [3; 16]);
        let omission_child_identity = NativeFileIdentity::from_parts(2, [4; 16]);
        let omission_snapshots = RefCell::new(Vec::new());
        let mut omission_sink = |progress| omission_snapshots.borrow_mut().push(progress);
        let mut omission_accumulator =
            FolderSizeAccumulator::new(omission_root.clone(), 12, &mut omission_sink);
        assert!(omission_accumulator.admit_root(&omission_root, omission_root_identity));
        omission_accumulator.update_progress(&summary);
        omission_accumulator.observe_entry(verified_entry(
            &omission_root,
            &omission_child,
            Path::new("placeholder"),
            OsStr::new("placeholder"),
            omission_root_identity,
            SnapshotEntryKind::Directory,
            Some(omission_child_identity),
            None,
            1,
        ));
        omission_accumulator.update_progress(&summary);
        omission_accumulator.mark_cloud_placeholder(omission_child_identity);
        omission_accumulator.update_progress(&summary);
        let omitted = omission_snapshots.borrow().last().unwrap().clone();
        assert_eq!(
            progress_entry(&omitted, &omission_child).kind,
            FolderSizeEntryKind::Omitted(FolderSizeOmission::CloudPlaceholder)
        );
        assert!(!progress_entry(&omitted, &omission_root).is_complete);
    }

    #[test]
    fn folder_size_scan_progress_batches_redundant_directory_signals() {
        let root = std::env::temp_dir().join("folder-size-progress-batch");
        let root_identity = NativeFileIdentity::from_parts(3, [5; 16]);
        let snapshots = RefCell::new(Vec::new());
        let mut sink = |progress| snapshots.borrow_mut().push(progress);
        let mut accumulator = FolderSizeAccumulator::new(root.clone(), 13, &mut sink);
        let summary = crate::file_operations::RenameScanSummary::default();
        assert!(accumulator.admit_root(&root, root_identity));

        for _ in 0..15 {
            MetadataScanObserver::progress(&mut accumulator, &summary);
        }
        assert!(snapshots.borrow().is_empty());
        MetadataScanObserver::progress(&mut accumulator, &summary);
        assert_eq!(snapshots.borrow().len(), 1);
        for _ in 0..15 {
            MetadataScanObserver::progress(&mut accumulator, &summary);
        }
        assert_eq!(snapshots.borrow().len(), 1);
        MetadataScanObserver::progress(&mut accumulator, &summary);
        assert_eq!(snapshots.borrow().len(), 2);

        // A final snapshot is forced even when the current batch is short.
        let _result = accumulator.finish(RenameScanStatus::Cancelled, &summary);
        assert_eq!(snapshots.borrow().len(), 3);
    }

    #[test]
    fn folder_size_scan_checked_totals_stop_before_ancestor_overflow() {
        let root = std::env::temp_dir().join("folder-size-overflow");
        let first = root.join("first.bin");
        let second = root.join("second.bin");
        let root_identity = NativeFileIdentity::from_parts(4, [6; 16]);
        let mut ignore_progress = |_progress| {};
        let mut accumulator = FolderSizeAccumulator::new(root.clone(), 14, &mut ignore_progress);
        assert!(accumulator.admit_root(&root, root_identity));
        let summary = crate::file_operations::RenameScanSummary::default();

        accumulator.observe_entry(verified_entry(
            &root,
            &first,
            Path::new("first.bin"),
            OsStr::new("first.bin"),
            root_identity,
            SnapshotEntryKind::File,
            None,
            Some(u64::MAX),
            1,
        ));
        let stop = accumulator.observe_entry(verified_entry(
            &root,
            &second,
            Path::new("second.bin"),
            OsStr::new("second.bin"),
            root_identity,
            SnapshotEntryKind::File,
            None,
            Some(1),
            1,
        ));
        assert_eq!(stop, MetadataScanControl::Stop);
        assert_eq!(accumulator.total_logical_bytes, u64::MAX);
        assert_eq!(accumulator.entries[0].subtree_logical_bytes(), u64::MAX);
        assert_eq!(accumulator.entries.len(), 2);

        let result = accumulator.finish(RenameScanStatus::ResourceLimit, &summary);
        assert_eq!(result.status(), FolderSizeStatus::Failed);
        assert_eq!(result.total_logical_bytes(), u64::MAX);
        assert!(
            result
                .issues()
                .iter()
                .any(|issue| { issue.kind() == FolderSizeIssueKind::AccountingOverflow })
        );
    }
}
