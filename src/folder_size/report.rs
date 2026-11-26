//! Renderer-neutral Folder Size report projection and navigation.
use std::{path::{Path, PathBuf}, sync::Arc};
use crate::batch_rename::ExplicitDirectoryRoot;
use super::{FolderSizeEntryKind, FolderSizeIssueKind, FolderSizeOmission, FolderSizeProgress, FolderSizeResult, FolderSizeStatus, FolderSizeSessionStatus, FolderSizeSessionView};
pub use super::treemap::{Layout as TreemapLayout, Rect as TreemapRect, Tile as TreemapTile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn { Name, Type, Size, Share, Status }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection { Ascending, Descending }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindFilter { All, Files, Folders }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VisibleKey { pub(crate) projection: ProjectionKey, pub(crate) directory: usize, pub(crate) column: SortColumn, pub(crate) direction: SortDirection, pub(crate) filter: KindFilter }
pub(crate) struct VisibleRows { pub(crate) key: VisibleKey, pub(crate) indices: Arc<Vec<usize>> }


#[derive(Clone)]
pub enum ProjectionSource {
    Progress(FolderSizeProgress),
    Result(Arc<FolderSizeResult>),
}
impl ProjectionSource {
    pub fn path_at(&self, target_index: usize) -> Option<PathBuf> {
        match self {
            Self::Result(result) => result
                .entries()
                .get(target_index)
                .map(|entry| entry.path().to_path_buf()),
            Self::Progress(progress) => {
                let mut path = None;
                let mut index = 0usize;
                progress.for_each_entry(|entry| {
                    if index == target_index {
                        path = Some(entry.path().to_path_buf());
                    }
                    index = index.saturating_add(1);
                });
                path
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionVersion {
    Progress(u64),
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionKey {
    pub(crate) generation: u64,
    pub(crate) version: ProjectionVersion,
}

#[derive(Clone)]
pub struct FolderSizeRow {
    pub(crate) parent_index: Option<usize>,
    pub(crate) name: String,
    pub(crate) sort_key: String,
    pub(crate) kind: FolderSizeEntryKind,
    pub(crate) logical_bytes: Option<u64>,
    pub(crate) subtree_logical_bytes: u64,
    pub(crate) complete: bool,
}

#[derive(Clone, Copy, Default)]
pub struct FolderSizeCounts {
    pub(crate) entries: usize,
    pub(crate) files: usize,
    pub(crate) directories: usize,
    pub(crate) directories_admitted: Option<usize>,
    pub(crate) scanned: usize,
    pub(crate) reparse: usize,
    pub(crate) cloud: usize,
    pub(crate) inaccessible: usize,
    pub(crate) issues: usize,
    pub(crate) issues_truncated: bool,
}

#[derive(Clone)]
pub struct FolderSizeProjection {
    pub(crate) key: ProjectionKey,
    pub(crate) source: ProjectionSource,
    pub(crate) rows: Vec<FolderSizeRow>,
    pub(crate) children: Vec<Vec<usize>>,
    pub(crate) root_index: Option<usize>,
    pub(crate) total_logical_bytes: u64,
    pub(crate) counts: FolderSizeCounts,
    pub(crate) status: Option<FolderSizeStatus>,
    pub(crate) issues: Vec<(Option<PathBuf>, FolderSizeIssueKind, String)>,
}

impl FolderSizeProjection {
    pub fn from_progress(progress: &FolderSizeProgress) -> Self {
        let mut rows = Vec::new();
        progress.for_each_entry(|entry| {
            rows.push(project_row(
                entry.path(),
                entry.parent_index(),
                entry.kind(),
                entry.logical_bytes(),
                entry.subtree_logical_bytes(),
                entry.is_complete(),
            ));
        });
        let mut projection = Self {
            key: ProjectionKey {
                generation: progress.generation(),
                version: ProjectionVersion::Progress(progress.publication_id()),
            },
            source: ProjectionSource::Progress(progress.clone()),
            children: vec![Vec::new(); rows.len()],
            root_index: rows.iter().position(|row| row.parent_index.is_none()),
            rows,
            total_logical_bytes: progress.total_logical_bytes(),
            counts: FolderSizeCounts {
                entries: progress.entries_seen(),
                files: progress.files_seen(),
                directories: progress.directories_seen(),
                directories_admitted: None,
                scanned: progress.directories_scanned(),
                reparse: progress.reparse_entries_excluded(),
                cloud: progress.cloud_placeholders_excluded(),
                inaccessible: progress.inaccessible_entries(),
                ..FolderSizeCounts::default()
            },
            status: None,
            issues: Vec::new(),
        };
        projection.finish_index();
        projection
    }

    pub fn from_result(result: Arc<FolderSizeResult>) -> Self {
        let rows = result
            .entries()
            .iter()
            .map(|entry| {
                project_row(
                    entry.path(),
                    entry.parent_index(),
                    entry.kind(),
                    entry.logical_bytes(),
                    entry.subtree_logical_bytes(),
                    entry.is_complete(),
                )
            })
            .collect::<Vec<_>>();
        let summary = result.summary();
        let mut projection = Self {
            key: ProjectionKey {
                generation: result.generation(),
                version: ProjectionVersion::Result,
            },
            source: ProjectionSource::Result(Arc::clone(&result)),
            children: vec![Vec::new(); rows.len()],
            root_index: rows.iter().position(|row| row.parent_index.is_none()),
            rows,
            total_logical_bytes: result.total_logical_bytes(),
            counts: FolderSizeCounts {
                entries: summary.entries_seen(),
                files: summary.files_seen(),
                directories: summary.directories_seen(),
                directories_admitted: Some(summary.directories_admitted()),
                scanned: summary.directories_scanned(),
                reparse: summary.reparse_entries_excluded(),
                cloud: summary.cloud_placeholders_excluded(),
                inaccessible: summary.inaccessible_entries(),
                issues: summary.issue_count(),
                issues_truncated: summary.issues_truncated(),
            },
            status: Some(result.status()),
            issues: result
                .issues()
                .iter()
                .map(|issue| {
                    (
                        issue.path().map(Path::to_path_buf),
                        issue.kind(),
                        issue.message().to_owned(),
                    )
                })
                .collect(),
        };
        projection.finish_index();
        projection
    }

    fn finish_index(&mut self) {
        self.children = child_indices(&self.rows);
    }

    pub fn row(&self, index: usize) -> Option<&FolderSizeRow> {
        self.rows.get(index)
    }

    pub fn contains_row(&self, generation: u64, index: usize) -> bool {
        projection_contains_row(self.key, self.rows.len(), generation, index)
    }
}

pub(crate) fn child_indices(rows: &[FolderSizeRow]) -> Vec<Vec<usize>> {
    let mut children = vec![Vec::new(); rows.len()];
    for (index, row) in rows.iter().enumerate() {
        if let Some(parent) = row.parent_index.and_then(|p| children.get_mut(p)) { parent.push(index); }
    }
    children
}

fn kind_rank(kind: FolderSizeEntryKind) -> u8 {
    match kind { FolderSizeEntryKind::File => 0, FolderSizeEntryKind::Directory => 1, FolderSizeEntryKind::Omitted(_) => 2 }
}
fn status_rank(row: &FolderSizeRow) -> (u8, &'static str) {
    match row.kind {
        FolderSizeEntryKind::Directory if row.complete => (0, ""),
        FolderSizeEntryKind::Directory => (1, ""),
        FolderSizeEntryKind::File => (2, ""),
        FolderSizeEntryKind::Omitted(reason) => (3, omission_label(reason)),
    }
}
fn row_sort_measurement(row: &FolderSizeRow) -> Option<u64> {
    match row.kind {
        FolderSizeEntryKind::Directory => Some(row.subtree_logical_bytes),
        FolderSizeEntryKind::File => row.logical_bytes,
        FolderSizeEntryKind::Omitted(_) => None,
    }
}
pub(crate) fn compare_rows(rows: &[FolderSizeRow], left: usize, right: usize, column: SortColumn, direction: SortDirection, total: u64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (&rows[left], &rows[right]);
    let reverse = |value: Ordering| if direction == SortDirection::Descending { value.reverse() } else { value };
    let primary = match column {
        SortColumn::Name => reverse(a.sort_key.cmp(&b.sort_key).then_with(|| a.name.cmp(&b.name))),
        SortColumn::Type => reverse(kind_rank(a.kind).cmp(&kind_rank(b.kind))),
        SortColumn::Status => reverse(status_rank(a).cmp(&status_rank(b))),
        SortColumn::Share if total == 0 => Ordering::Equal,
        SortColumn::Size | SortColumn::Share => match (row_sort_measurement(a), row_sort_measurement(b)) {
            (Some(a), Some(b)) => reverse(a.cmp(&b)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
    };
    primary.then_with(|| a.sort_key.cmp(&b.sort_key)).then_with(|| a.name.cmp(&b.name)).then_with(|| left.cmp(&right))
}
pub(crate) fn filter_matches(filter: KindFilter, kind: FolderSizeEntryKind) -> bool {
    match filter { KindFilter::All => true, KindFilter::Files => kind == FolderSizeEntryKind::File, KindFilter::Folders => kind == FolderSizeEntryKind::Directory }
}

pub(crate) fn projection_contains_row(
    key: ProjectionKey,
    row_count: usize,
    generation: u64,
    index: usize,
) -> bool {
    key.generation == generation && index < row_count
}

pub(crate) fn row_measurement(row: &FolderSizeRow, total_logical_bytes: u64) -> (Option<u64>, Option<f64>) {
    let row_bytes = row_sort_measurement(row);
    let share = (total_logical_bytes > 0)
        .then_some(row_bytes)
        .flatten()
        .map(|bytes| (bytes as f64 / total_logical_bytes as f64) * 100.0);
    (row_bytes, share)
}

fn project_row(
    path: &Path,
    parent_index: Option<usize>,
    kind: FolderSizeEntryKind,
    logical_bytes: Option<u64>,
    subtree_logical_bytes: u64,
    complete: bool,
) -> FolderSizeRow {
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned();
    FolderSizeRow {
        parent_index,
        sort_key: name.to_lowercase(),
        name,
        kind,
        logical_bytes,
        subtree_logical_bytes,
        complete,
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TreemapKey {
    pub(crate) projection: ProjectionKey,
    pub(crate) directory: usize,
    pub(crate) filter: KindFilter,
    width: u64,
    height: u64,
}

#[derive(Default)]
pub(crate) struct TreemapCache {
    value: Option<(TreemapKey, Arc<crate::folder_size::treemap::Layout>)>,
}

impl TreemapCache {
    pub(crate) fn layout(
        &mut self,
        projection: ProjectionKey,
        rows: &[FolderSizeRow],
        children: &[Vec<usize>],
        directory: usize,
        filter: KindFilter,
        width: f64,
        height: f64,
    ) -> Option<Arc<crate::folder_size::treemap::Layout>> {
        use crate::folder_size::treemap::{self, Item, Measurement};
        let key = TreemapKey {
            projection,
            directory,
            filter,
            width: width.to_bits(),
            height: height.to_bits(),
        };
        if let Some((cached_key, layout)) = &self.value
            && *cached_key == key
        {
            return Some(layout.clone());
        }
        let indices = children.get(directory)?;
        let items = indices.iter().filter_map(|&index| {
            let row = rows.get(index)?;
            if !filter_matches(filter, row.kind) {
                return None;
            }
            let measurement = match row.kind {
                FolderSizeEntryKind::Omitted(_) => Measurement::Omitted,
                _ => row_sort_measurement(row).map_or(Measurement::Unknown, Measurement::Bytes),
            };
            Some(Item {
                row: index,
                measurement,
                provisional: matches!(row.kind, FolderSizeEntryKind::Directory) && !row.complete,
            })
        });
        let layout = Arc::new(treemap::layout(items, width, height)?);
        self.value = Some((key, layout.clone()));
        Some(layout)
    }
}

pub(crate) fn omission_label(omission: FolderSizeOmission) -> &'static str {
    match omission {
        FolderSizeOmission::ReparsePoint => "reparse point",
        FolderSizeOmission::CloudPlaceholder => "cloud placeholder",
        FolderSizeOmission::Inaccessible => "inaccessible",
        FolderSizeOmission::UnsupportedEntry => "unsupported entry",
        FolderSizeOmission::UnsupportedIdentity => "unsupported identity",
        FolderSizeOmission::RepeatedDirectoryIdentity => "repeated directory identity",
    }
}

/// Retained, renderer-neutral navigation and projection for one scan session.
pub struct FolderSizeReport {
    pub(crate) current_directory: Option<(u64, usize)>,
    pub(crate) current_directory_path: Option<PathBuf>,
    pub(crate) selected_entry: Option<(u64, usize)>,
    pub(crate) projection: Option<Arc<FolderSizeProjection>>,
    pub(crate) sort_column: SortColumn,
    pub(crate) sort_direction: SortDirection,
    pub(crate) filter: KindFilter,
    pub(crate) visible_rows: Option<VisibleRows>,
    pub(crate) treemap_cache: TreemapCache,
}

impl Default for FolderSizeReport {
    fn default() -> Self {
        Self {
            current_directory: None, current_directory_path: None, selected_entry: None,
            projection: None, sort_column: SortColumn::Size, sort_direction: SortDirection::Descending,
            filter: KindFilter::All, visible_rows: None, treemap_cache: TreemapCache::default(),
        }
    }
}

impl FolderSizeReport {
    pub fn projection(&self) -> Option<&Arc<FolderSizeProjection>> { self.projection.as_ref() }
    pub fn selected(&self) -> Option<(u64, usize)> { self.selected_entry }
    pub fn directory(&self) -> Option<(u64, usize)> { self.current_directory }
    pub fn directory_path(&self) -> Option<&Path> { self.current_directory_path.as_deref() }
    pub fn sort_column(&self) -> SortColumn { self.sort_column }
    pub fn sort_direction(&self) -> SortDirection { self.sort_direction }
    pub fn filter(&self) -> KindFilter { self.filter }
    pub fn visible_rows(&self) -> Option<&Arc<Vec<usize>>> { self.visible_rows.as_ref().map(|rows| &rows.indices) }

    pub fn set_sort(&mut self, column: SortColumn) {
        if self.sort_column == column {
            self.sort_direction = if self.sort_direction == SortDirection::Ascending { SortDirection::Descending } else { SortDirection::Ascending };
        } else { self.sort_column = column; self.sort_direction = SortDirection::Ascending; }
        self.visible_rows = None;
        self.sync_visible_rows();
    }
    pub fn set_filter(&mut self, filter: KindFilter) {
        self.filter = filter;
        self.visible_rows = None;
        self.sync_visible_rows();
    }
    pub fn clear(&mut self) {
        self.current_directory = None; self.current_directory_path = None; self.selected_entry = None;
        self.projection = None; self.visible_rows = None;
    }
    pub fn select(&mut self, generation: u64, index: usize) -> bool {
        if self.projection.as_ref().is_some_and(|projection| projection.contains_row(generation, index)) {
            self.selected_entry = Some((generation, index)); true
        } else { false }
    }
    pub fn drill(&mut self, generation: u64, index: usize) -> Option<PathBuf> {
        let projection = self.projection.as_ref()?;
        if !projection.contains_row(generation, index)
            || !projection.row(index).is_some_and(|row| row.kind == FolderSizeEntryKind::Directory) { return None; }
        let path = projection.source.path_at(index)?;
        self.selected_entry = Some((generation, index)); self.current_directory = Some((generation, index));
        self.current_directory_path = Some(path.clone());
        self.sync_visible_rows();
        Some(path)
    }
    pub fn up(&mut self, generation: u64, index: usize) -> bool {
        let Some(projection) = self.projection.as_ref() else { return false; };
        if self.current_directory != Some((generation, index)) || !projection.contains_row(generation, index) { return false; }
        self.current_directory = projection.row(index).and_then(|row| row.parent_index).map(|parent| (generation, parent));
        self.selected_entry = None;
        self.current_directory_path = self.current_directory.and_then(|(_, parent)| projection.source.path_at(parent));
        self.sync_visible_rows();
        true
    }
    pub fn path_for_current_row(&self, view: &FolderSizeSessionView, accepted_root: Option<&ExplicitDirectoryRoot>, current_root: Option<&ExplicitDirectoryRoot>, issued: ProjectionKey, index: usize) -> Option<PathBuf> {
        if accepted_root.is_none() || accepted_root != current_root || publication_key(view) != Some(issued) { return None; }
        let projection = self.projection.as_ref().filter(|projection| projection.key == issued && projection.contains_row(issued.generation, index))?;
        projection.source.path_at(index)
    }
    pub fn treemap(&mut self, width: f64, height: f64) -> Option<Arc<TreemapLayout>> {
        let projection = self.projection.as_ref()?;
        let directory = self.current_directory.filter(|(generation, _)| *generation == projection.key.generation)
            .map(|(_, index)| index).or(projection.root_index)?;
        self.treemap_cache.layout(projection.key, &projection.rows, &projection.children, directory, self.filter, width, height)
    }
    pub fn sync_visible_rows(&mut self) {
        let Some(projection) = self.projection.as_ref() else { self.visible_rows = None; return; };
        let Some(directory) = self.current_directory.filter(|(generation, _)| *generation == projection.key.generation)
            .map(|(_, index)| index).or(projection.root_index) else { self.visible_rows = None; return; };
        let key = VisibleKey { projection: projection.key, directory, column: self.sort_column, direction: self.sort_direction, filter: self.filter };
        if self.visible_rows.as_ref().is_some_and(|cache| cache.key == key) { return; }
        let mut indices = projection.children.get(directory).into_iter().flatten().copied()
            .filter(|index| filter_matches(self.filter, projection.rows[*index].kind)).collect::<Vec<_>>();
        indices.sort_by(|a, b| compare_rows(&projection.rows, *a, *b, self.sort_column, self.sort_direction, projection.total_logical_bytes));
        if self.selected_entry.is_some_and(|(generation, index)| generation != key.projection.generation
            || (!indices.contains(&index) && self.current_directory != Some((generation, index)))) { self.selected_entry = None; }
        self.visible_rows = Some(VisibleRows { key, indices: Arc::new(indices) });
    }
    pub fn sync(&mut self, view: &FolderSizeSessionView, accepted_root: Option<&ExplicitDirectoryRoot>, current_root: Option<&ExplicitDirectoryRoot>) {
        let next_key = if accepted_root == current_root { publication_key(view) } else { None };
        if self.projection.as_ref().map(|projection| projection.key) == next_key { return; }
        if self.projection.as_ref().map(|projection| projection.key.generation) != next_key.map(|key| key.generation) {
            self.selected_entry = None; self.current_directory = None; self.current_directory_path = None;
        }
        self.projection = next_key.and_then(|key| {
            if key.version == ProjectionVersion::Result {
                view.result().filter(|result| result.generation() == key.generation).cloned().map(FolderSizeProjection::from_result).map(Arc::new)
            } else {
                view.progress().filter(|progress| progress.generation() == key.generation
                    && progress.publication_id() == match key.version { ProjectionVersion::Progress(id) => id, ProjectionVersion::Result => 0 })
                    .map(FolderSizeProjection::from_progress).map(Arc::new)
            }
        });
        if let Some((generation, index)) = self.current_directory
            && !self.projection.as_ref().is_some_and(|projection| projection.contains_row(generation, index)
                && projection.row(index).is_some_and(|row| row.kind == FolderSizeEntryKind::Directory)) {
            self.current_directory = None; self.current_directory_path = None;
        }
        if let Some((generation, index)) = self.selected_entry
            && !self.projection.as_ref().is_some_and(|projection| projection.contains_row(generation, index)) { self.selected_entry = None; }
        self.visible_rows = None;
    }
}

fn publication_key(view: &FolderSizeSessionView) -> Option<ProjectionKey> {
    if view.result_is_current() {
        Some(ProjectionKey { generation: view.generation(), version: ProjectionVersion::Result })
    } else if let Some(progress) = view.progress()
        && (view.progress_is_current() || view.status() == FolderSizeSessionStatus::Cancelled) {
        Some(ProjectionKey { generation: progress.generation(), version: ProjectionVersion::Progress(progress.publication_id()) })
    } else { None }
}

impl ProjectionKey {
    pub fn generation(self) -> u64 { self.generation }
    pub fn version(self) -> ProjectionVersion { self.version }
}
impl FolderSizeRow {
    pub fn name(&self) -> &str { &self.name }
    pub fn kind(&self) -> FolderSizeEntryKind { self.kind }
    pub fn parent_index(&self) -> Option<usize> { self.parent_index }
    pub fn complete(&self) -> bool { self.complete }
    pub fn measurement(&self, total: u64) -> (Option<u64>, Option<f64>) { row_measurement(self, total) }
}
impl FolderSizeProjection {
    pub fn key(&self) -> ProjectionKey { self.key }
    pub fn rows(&self) -> &[FolderSizeRow] { &self.rows }
    pub fn root_index(&self) -> Option<usize> { self.root_index }
    pub fn total_logical_bytes(&self) -> u64 { self.total_logical_bytes }
    pub fn counts(&self) -> FolderSizeCounts { self.counts }
    pub fn status(&self) -> Option<FolderSizeStatus> { self.status }
    pub fn issues(&self) -> &[(Option<PathBuf>, FolderSizeIssueKind, String)] { &self.issues }
    pub fn path_at(&self, index: usize) -> Option<PathBuf> { self.source.path_at(index) }
}
impl FolderSizeCounts {
    pub fn entries(self) -> usize { self.entries }
    pub fn files(self) -> usize { self.files }
    pub fn directories(self) -> usize { self.directories }
    pub fn directories_admitted(self) -> Option<usize> { self.directories_admitted }
    pub fn scanned(self) -> usize { self.scanned }
    pub fn reparse(self) -> usize { self.reparse }
    pub fn cloud(self) -> usize { self.cloud }
    pub fn inaccessible(self) -> usize { self.inaccessible }
    pub fn issues(self) -> usize { self.issues }
    pub fn issues_truncated(self) -> bool { self.issues_truncated }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn report_navigation_and_path_require_current_publication() {
        let fixture = crate::folder_size::disposable_scan_fixture_root();
        let directory = fixture.path().join("nested");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("child.txt"), b"abc").unwrap();
        let root = ExplicitDirectoryRoot::new(fixture.path().to_path_buf()).unwrap();
        let session = super::super::FolderSizeSession::new(|| {});
        session.refresh(root.clone(), true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let view = loop {
            let view = session.view();
            if view.result_is_current() { break view; }
            assert!(Instant::now() < deadline, "scan did not finish");
            std::thread::yield_now();
        };
        let mut report = FolderSizeReport::default();
        report.sync(&view, Some(&root), Some(&root));
        report.sync_visible_rows();
        let projection = report.projection().unwrap();
        let key = projection.key();
        let publication = session.publication_guard();
        assert!(publication.is_current(key));
        let child_dir = projection.rows().iter().position(|row| row.name() == "nested").unwrap();
        assert_eq!(report.path_for_current_row(&view, Some(&root), Some(&root), key, child_dir), Some(directory));
        let stale = ProjectionKey { version: ProjectionVersion::Progress(0), ..key };
        assert!(report.path_for_current_row(&view, Some(&root), Some(&root), stale, child_dir).is_none());
        // The retained projection may itself lag the live session publication.
        Arc::make_mut(report.projection.as_mut().unwrap()).key = stale;
        assert!(report.path_for_current_row(&view, Some(&root), Some(&root), stale, child_dir).is_none());
        Arc::make_mut(report.projection.as_mut().unwrap()).key = key;
        assert!(report.path_for_current_row(&view, Some(&root), None, key, child_dir).is_none());
        assert!(report.path_for_current_row(&view, None, None, key, child_dir).is_none());
        assert!(report.drill(key.generation(), child_dir).is_some());
        assert_eq!(report.visible_rows().unwrap().len(), 1);
        assert!(report.up(key.generation(), child_dir));
        assert!(report.visible_rows().unwrap().contains(&child_dir));
        session.refresh(root.clone(), true).unwrap();
        assert!(!publication.is_current(key));
        session.close();
        assert!(!publication.is_current(key));
    }
}
