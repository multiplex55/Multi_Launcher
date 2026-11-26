//! Read-only launcher boundary over the existing duplicate discovery session.
use super::{model::{DiscoverySnapshot, DiscoveryStatus}, projection::{build_result_rows, ResultRow},
    session::{DuplicateFinderSession, DuplicateFinderSessionError, DuplicateFinderSessionView}};
use crate::activity::ActivityCenter;
use std::{path::{Path, PathBuf}, sync::Arc};

pub const MAX_ROOTS: usize = super::discovery::MAX_SELECTED_ROOTS;
pub const MAX_ROOT_PATH_BYTES: usize = super::discovery::MAX_ROOT_SELECTION_PATH_BYTES;

/// One owned scanner session. All scanning remains explicit and read-only.
pub struct DuplicateFinder { inner: DuplicateFinderSession }
impl DuplicateFinder {
    pub fn new(wake: impl Fn() + Send + Sync + 'static, activity: ActivityCenter) -> Self {
        let mut inner = DuplicateFinderSession::with_wake(Arc::new(wake));
        inner.attach_activity_center(activity);
        Self { inner }
    }
    pub fn start(&self, roots: Vec<PathBuf>) -> Result<u64, DuplicateFinderSessionError> { self.inner.start_scan(roots) }
    pub fn cancel(&self) -> Result<bool, DuplicateFinderSessionError> { self.inner.cancel() }
    pub fn close(&self) { self.inner.close(); }
    pub fn reopen(&self) -> Result<(), DuplicateFinderSessionError> { self.inner.reopen() }
    pub fn view(&self) -> DuplicateFinderView { DuplicateFinderView { inner: self.inner.view() } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicateFinderStatus { Idle, Scanning, Complete, Partial, Cancelled, Failed, Closed }
impl From<DiscoveryStatus> for DuplicateFinderStatus {
    fn from(value: DiscoveryStatus) -> Self { match value {
        DiscoveryStatus::Idle => Self::Idle, DiscoveryStatus::Scanning => Self::Scanning,
        DiscoveryStatus::Complete => Self::Complete, DiscoveryStatus::Partial => Self::Partial,
        DiscoveryStatus::Cancelled => Self::Cancelled, DiscoveryStatus::Failed => Self::Failed,
        DiscoveryStatus::Closed => Self::Closed,
    } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DuplicateFinderTotals {
    pub verified_groups: usize,
    pub verified_files: usize,
    pub logical_duplicate_bytes: u64,
    pub identity_unverified_matches: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DuplicateFinderCounts {
    pub files_seen: usize,
    pub directories_scanned: usize,
    pub inaccessible: usize,
    pub reparse_excluded: usize,
    pub cloud_excluded: usize,
}

#[derive(Clone, Debug)]
pub struct DuplicateFinderProgress {
    pub stage: &'static str,
    pub roots_total: usize,
    pub roots_completed: usize,
    pub files_seen: usize,
    pub directories_scanned: usize,
    pub current_root: Option<PathBuf>,
    pub files_completed: Option<usize>,
    pub files_total: Option<usize>,
    pub bytes_processed: Option<u64>,
}

pub struct DuplicateFinderView { inner: DuplicateFinderSessionView }
impl DuplicateFinderView {
    pub fn generation(&self) -> u64 { self.inner.snapshot.generation }
    pub fn status(&self) -> DuplicateFinderStatus { self.inner.snapshot.status.into() }
    pub fn busy(&self) -> bool { self.inner.busy }
    pub fn closed(&self) -> bool { self.inner.closed }
    pub fn progress(&self) -> Option<DuplicateFinderProgress> {
        use super::model::DiscoveryStage;
        self.inner.progress.as_ref().or(self.inner.snapshot.progress.as_ref()).map(|progress| DuplicateFinderProgress {
            stage: match progress.stage { DiscoveryStage::Traversing => "Traversing", DiscoveryStage::Hashing => "Hashing",
                DiscoveryStage::Comparing => "Comparing", DiscoveryStage::Complete => "Complete" },
            roots_total: progress.roots_total, roots_completed: progress.roots_completed,
            files_seen: progress.counts.files_seen, directories_scanned: progress.counts.directories_scanned,
            current_root: progress.current_root.clone(),
            files_completed: progress.content.map(|content| content.files_completed),
            files_total: progress.content.map(|content| content.files_total),
            bytes_processed: progress.content.map(|content| content.bytes_processed),
        })
    }
    pub fn matches_report(&self, report: &DuplicateFinderReport) -> bool { Arc::ptr_eq(&self.inner.snapshot, &report.snapshot) }
    pub fn report(&self) -> DuplicateFinderReport {
        let mut rows = Vec::new();
        let truncated = build_result_rows(&self.inner.snapshot, &mut rows);
        DuplicateFinderReport { snapshot: self.inner.snapshot.clone(), rows, truncated }
    }
}

pub struct DuplicateFinderReport {
    snapshot: Arc<DiscoverySnapshot>,
    rows: Vec<ResultRow>,
    truncated: bool,
}
pub enum DuplicateFinderRow<'a> {
    VerifiedGroup { bytes: u64, files: usize, duplicate_bytes: u64 },
    VerifiedFile { path: &'a Path, bytes: u64 },
    UnverifiedGroup { bytes: u64 },
    UnverifiedKnownFile { path: &'a Path, bytes: u64 },
    UnverifiedPath { path: &'a Path, bytes: u64 },
}
impl DuplicateFinderReport {
    pub fn generation(&self) -> u64 { self.snapshot.generation }
    pub fn len(&self) -> usize { self.rows.len() }
    pub fn is_empty(&self) -> bool { self.rows.is_empty() }
    pub fn truncated(&self) -> bool { self.truncated }
    pub fn totals(&self) -> DuplicateFinderTotals {
        let totals = self.snapshot.duplicate_totals;
        DuplicateFinderTotals { verified_groups: totals.verified_groups, verified_files: totals.verified_files,
            logical_duplicate_bytes: totals.logical_duplicate_bytes, identity_unverified_matches: totals.identity_unverified_matches }
    }
    pub fn counts(&self) -> DuplicateFinderCounts {
        let counts = self.snapshot.counts;
        DuplicateFinderCounts { files_seen: counts.files_seen, directories_scanned: counts.directories_scanned,
            inaccessible: counts.inaccessible_entries, reparse_excluded: counts.reparse_entries_excluded,
            cloud_excluded: counts.cloud_placeholders_excluded }
    }
    pub fn roots(&self) -> impl Iterator<Item = (&Path, String)> {
        self.snapshot.roots.iter().map(|root| (root.selected_path.as_path(), format!("{:?}", root.status)))
    }
    pub fn diagnostics(&self) -> impl Iterator<Item = (Option<&Path>, &str)> {
        self.snapshot.diagnostics.iter().map(|diagnostic| (diagnostic.path.as_deref(), diagnostic.message.as_str()))
    }
    pub fn diagnostics_truncated(&self) -> bool { self.snapshot.diagnostics_truncated }
    pub fn row(&self, index: usize) -> Option<DuplicateFinderRow<'_>> {
        let row = *self.rows.get(index)?;
        match row {
            ResultRow::VerifiedHeader(group) => {
                let group = self.snapshot.verified_duplicate_groups.get(group)?;
                let size = self.snapshot.size_groups.get(group.size_group_index)?.size_bytes;
                Some(DuplicateFinderRow::VerifiedGroup { bytes: size, files: group.known_files.len(), duplicate_bytes: group.logical_duplicate_bytes })
            }
            ResultRow::VerifiedMember { group, member } => {
                let group = self.snapshot.verified_duplicate_groups.get(group)?;
                let size = self.snapshot.size_groups.get(group.size_group_index)?;
                let verified = group.known_files.get(member)?;
                let alias = size.known_files.get(verified.known_file_index)?.aliases.get(verified.alias_index)?;
                Some(DuplicateFinderRow::VerifiedFile { path: &alias.path, bytes: size.size_bytes })
            }
            ResultRow::UnverifiedHeader(group) => {
                let group = self.snapshot.identity_unverified_matches.get(group)?;
                Some(DuplicateFinderRow::UnverifiedGroup { bytes: self.snapshot.size_groups.get(group.size_group_index)?.size_bytes })
            }
            ResultRow::UnverifiedKnownMember { group, member } => {
                let group = self.snapshot.identity_unverified_matches.get(group)?;
                let size = self.snapshot.size_groups.get(group.size_group_index)?;
                let verified = group.known_files.get(member)?;
                let alias = size.known_files.get(verified.known_file_index)?.aliases.get(verified.alias_index)?;
                Some(DuplicateFinderRow::UnverifiedKnownFile { path: &alias.path, bytes: size.size_bytes })
            }
            ResultRow::UnverifiedPath { group, path } => {
                let group = self.snapshot.identity_unverified_matches.get(group)?;
                let size = self.snapshot.size_groups.get(group.size_group_index)?;
                let candidate = size.unverified_paths.get(*group.unverified_path_indices.get(path)?)?;
                Some(DuplicateFinderRow::UnverifiedPath { path: &candidate.path, bytes: candidate.size_bytes })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DuplicateFinderReport, DuplicateFinderRow, build_result_rows};
    use crate::duplicate_finder::{DiscoverySnapshot, FileAlias, IdentityUnverifiedMatch, IdentityUnverifiedPath,
        KnownFile, KnownFileIdentity, SizeGroup, VerifiedDuplicateGroup, VerifiedKnownFile};
    use std::{path::PathBuf, sync::Arc};

    #[test]
    fn verified_rows_use_exact_alias_and_leave_unverified_totals_separate() {
        let mut snapshot = DiscoverySnapshot::idle(8);
        snapshot.size_groups.push(SizeGroup { size_bytes: 4, known_files: vec![KnownFile {
            identity: KnownFileIdentity { volume_serial_number: 1, file_id: [2; 16] }, size_bytes: 4, modified: None,
            aliases: vec![FileAlias { root_index: 0, path: PathBuf::from("old") },
                FileAlias { root_index: 0, path: PathBuf::from("verified") }],
        }, KnownFile { identity: KnownFileIdentity { volume_serial_number: 1, file_id: [3; 16] },
            size_bytes: 4, modified: None, aliases: vec![FileAlias { root_index: 0, path: PathBuf::from("other") }] }],
            unverified_paths: vec![IdentityUnverifiedPath { root_index: 0, size_bytes: 4,
            observed_identity: Default::default(), modified: None, path: PathBuf::from("unknown") }] });
        let member = VerifiedKnownFile { known_file_index: 0, alias_index: 1 };
        snapshot.verified_duplicate_groups.push(VerifiedDuplicateGroup { size_group_index: 0, sha256: [0; 32],
            known_files: vec![member, VerifiedKnownFile { known_file_index: 1, alias_index: 0 }], logical_duplicate_bytes: 4 });
        snapshot.identity_unverified_matches.push(IdentityUnverifiedMatch { size_group_index: 0, sha256: [0; 32], known_files: vec![member], unverified_path_indices: vec![0] });
        snapshot.duplicate_totals.verified_groups = 1;
        snapshot.duplicate_totals.verified_files = 2;
        snapshot.duplicate_totals.logical_duplicate_bytes = 4;
        snapshot.duplicate_totals.identity_unverified_matches = 1;
        let mut rows = Vec::new();
        let truncated = build_result_rows(&snapshot, &mut rows);
        let report = DuplicateFinderReport { snapshot: Arc::new(snapshot), rows, truncated };
        assert!(matches!(report.row(1), Some(DuplicateFinderRow::VerifiedFile { path, .. }) if path == std::path::Path::new("verified")));
        assert!(matches!(report.row(4), Some(DuplicateFinderRow::UnverifiedKnownFile { path, .. }) if path == std::path::Path::new("verified")));
        assert!(matches!(report.row(5), Some(DuplicateFinderRow::UnverifiedPath { path, .. }) if path == std::path::Path::new("unknown")));
        assert_eq!(report.totals().verified_groups, 1);
        assert_eq!(report.totals().verified_files, 2);
        assert_eq!(report.totals().logical_duplicate_bytes, 4);
        assert_eq!(report.totals().identity_unverified_matches, 1);
    }
}
