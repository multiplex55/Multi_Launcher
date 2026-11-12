use crate::batch_rename::ExplicitDirectoryRoot;
use crate::folder_size::{
    FolderSizeEntryKind, FolderSizeIssueKind, FolderSizeOmission, FolderSizeProgress,
    FolderSizeResult, FolderSizeSession, FolderSizeSessionStatus, FolderSizeStatus,
};
use crate::gui::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use crate::gui::utility_host::ViewportPassProvenance;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const WINDOW_TITLE: &str = "Folder Size";
const UTILITY_KEY: &str = "folder-size";
const ROW_HEIGHT: f32 = 26.0;
const ROW_TYPE_WIDTH: f32 = 64.0;
const ROW_SIZE_WIDTH: f32 = 86.0;
const ROW_SHARE_WIDTH: f32 = 64.0;
const ROW_STATUS_WIDTH: f32 = 166.0;
const ROW_COLUMN_GAP: f32 = 10.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FolderSizeDialogEffect {
    CopyPath(String),
    OpenPath(PathBuf),
    Reveal(FolderSizeRevealRequest),
    FocusPanel,
    ShowRoot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FolderSizeRevealRequest {
    key: ProjectionKey,
    index: usize,
    directory: Option<(u64, usize)>,
    host: super::utility_host::HostIdentity,
    epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortColumn { Name, Type, Size, Share, Status }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection { Ascending, Descending }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KindFilter { All, Files, Folders }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VisibleKey { projection: ProjectionKey, directory: usize, column: SortColumn, direction: SortDirection, filter: KindFilter }
struct VisibleRows { key: VisibleKey, indices: Arc<Vec<usize>> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FolderSizeRevealRoute { OpenRoot, RevealEntry }
pub(crate) fn reveal_route(path: &Path) -> FolderSizeRevealRoute {
    if path.parent().is_none_or(|parent| parent.as_os_str().is_empty()) {
        FolderSizeRevealRoute::OpenRoot
    } else { FolderSizeRevealRoute::RevealEntry }
}

#[derive(Clone)]
enum ProjectionSource {
    Progress(FolderSizeProgress),
    Result(Arc<FolderSizeResult>),
}

impl ProjectionSource {
    fn path_at(&self, target_index: usize) -> Option<PathBuf> {
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
enum ProjectionVersion {
    Progress(u64),
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectionKey {
    generation: u64,
    version: ProjectionVersion,
}

#[derive(Clone)]
struct FolderSizeRow {
    parent_index: Option<usize>,
    name: String,
    sort_key: String,
    kind: FolderSizeEntryKind,
    logical_bytes: Option<u64>,
    subtree_logical_bytes: u64,
    complete: bool,
}

#[derive(Clone, Copy, Default)]
struct FolderSizeCounts {
    entries: usize,
    files: usize,
    directories: usize,
    directories_admitted: Option<usize>,
    scanned: usize,
    reparse: usize,
    cloud: usize,
    inaccessible: usize,
    issues: usize,
    issues_truncated: bool,
}

#[derive(Clone)]
struct FolderSizeProjection {
    key: ProjectionKey,
    source: ProjectionSource,
    rows: Vec<FolderSizeRow>,
    children: Vec<Vec<usize>>,
    root_index: Option<usize>,
    total_logical_bytes: u64,
    counts: FolderSizeCounts,
    status: Option<FolderSizeStatus>,
    issues: Vec<(Option<PathBuf>, FolderSizeIssueKind, String)>,
}

impl FolderSizeProjection {
    fn from_progress(progress: &FolderSizeProgress) -> Self {
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

    fn from_result(result: Arc<FolderSizeResult>) -> Self {
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

    fn row(&self, index: usize) -> Option<&FolderSizeRow> {
        self.rows.get(index)
    }

    fn contains_row(&self, generation: u64, index: usize) -> bool {
        projection_contains_row(self.key, self.rows.len(), generation, index)
    }
}

fn child_indices(rows: &[FolderSizeRow]) -> Vec<Vec<usize>> {
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
fn compare_rows(rows: &[FolderSizeRow], left: usize, right: usize, column: SortColumn, direction: SortDirection, total: u64) -> std::cmp::Ordering {
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
fn filter_matches(filter: KindFilter, kind: FolderSizeEntryKind) -> bool {
    match filter { KindFilter::All => true, KindFilter::Files => kind == FolderSizeEntryKind::File, KindFilter::Folders => kind == FolderSizeEntryKind::Directory }
}

fn projection_contains_row(
    key: ProjectionKey,
    row_count: usize,
    generation: u64,
    index: usize,
) -> bool {
    key.generation == generation && index < row_count
}

fn row_measurement(row: &FolderSizeRow, total_logical_bytes: u64) -> (Option<u64>, Option<f64>) {
    let row_bytes = row_sort_measurement(row);
    let share = (total_logical_bytes > 0)
        .then_some(row_bytes)
        .flatten()
        .map(|bytes| (bytes as f64 / total_logical_bytes as f64) * 100.0);
    (row_bytes, share)
}

fn row_name_width(available_width: f32) -> f32 {
    let reserved = ROW_TYPE_WIDTH + ROW_SIZE_WIDTH + ROW_SHARE_WIDTH + ROW_STATUS_WIDTH + ROW_COLUMN_GAP * 4.0;
    (available_width - reserved)
        .max(72.0)
        .min((available_width * 0.56).max(0.0))
}

fn status_label(status: FolderSizeSessionStatus) -> &'static str {
    match status {
        FolderSizeSessionStatus::Idle => "Ready",
        FolderSizeSessionStatus::Scanning => "Scanning…",
        FolderSizeSessionStatus::RefreshQueued => {
            "Refresh queued; current native call is settling…"
        }
        FolderSizeSessionStatus::Cancelled => "Cancelled — last verified subtotal is frozen.",
        FolderSizeSessionStatus::Complete => "Complete",
        FolderSizeSessionStatus::Partial => "Partial — some entries could not be measured.",
        FolderSizeSessionStatus::Failed => "Failed — no complete result was produced.",
        FolderSizeSessionStatus::Closed => "Session closed",
    }
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

use super::utility_host::{HostedCopyCapture, HostedCopyReceipt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FolderView {
    Table,
    Treemap,
    Both,
}

#[derive(Clone, Copy, PartialEq)]
struct TreemapPress {
    projection: ProjectionKey,
    directory: Option<(u64, usize)>,
    filter: KindFilter,
    rect: egui::Rect,
    area: egui::Rect,
    row: usize,
    host: super::utility_host::HostIdentity,
    epoch: u64,
}

#[derive(Clone)]
enum FolderSizeAction {
    SetView(FolderView),
    CancelTreemapPress,
    TreemapPress(TreemapPress),
    TreemapRelease(TreemapPress, bool),
    Host(super::utility_host::UtilityPresentation),
    ChooseFolder,
    Prefill(PathBuf),
    SetRecursive(bool),
    Scan {
        root: ExplicitDirectoryRoot,
        session_generation: u64,
        recursive: bool,
    },
    Refresh {
        root: ExplicitDirectoryRoot,
        session_generation: u64,
        recursive: bool,
    },
    Cancel {
        session_generation: u64,
    },
    SetSort(SortColumn),
    SetFilter(KindFilter),
    RevealRow { generation: u64, index: usize },
    OpenRowMenu { generation: u64, index: usize, popup: egui::Id },
    Select {
        generation: u64,
        index: usize,
    },
    Drill {
        generation: u64,
        index: usize,
    },
    Back {
        generation: u64,
        index: usize,
    },
    CopyRoot(ExplicitDirectoryRoot),
    CopyRow {
        generation: u64,
        index: usize,
    },
    OpenRoot(ExplicitDirectoryRoot),
    OpenRow {
        generation: u64,
        index: usize,
    },
    Close,
}

struct PendingFolderSizePass {
    provenance: ViewportPassProvenance,
    actions: Vec<FolderSizeAction>,
    owner: ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    native: bool,
    info: Option<egui::ViewportInfo>,
    projection: Option<Arc<FolderSizeProjection>>,
}

#[derive(Clone, Copy, PartialEq)]
struct TreemapKey {
    projection: ProjectionKey,
    directory: usize,
    filter: KindFilter,
    width: u64,
    height: u64,
}

#[derive(Default)]
struct TreemapCache {
    value: Option<(TreemapKey, Arc<crate::folder_size::treemap::Layout>)>,
}

impl TreemapCache {
    fn layout(
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
pub(crate) struct FolderSizeDialogState {
    pub(crate) open: bool,
    host: super::utility_host::UtilityHostState,
    presentation_prepared: bool,
    host_epoch: u64,
    host_focus: bool,
    host_os_close_pending: bool,
    prefill_pending: Option<PendingFolderSizePass>,
    session: FolderSizeSession,
    selected_root: Option<ExplicitDirectoryRoot>,
    scan_root: Option<ExplicitDirectoryRoot>,
    recursive: bool,
    current_directory: Option<(u64, usize)>,
    current_directory_path: Option<PathBuf>,
    selected_entry: Option<(u64, usize)>,
    row_menu: Option<(ProjectionKey, usize, egui::Id, super::utility_host::HostIdentity, u64, Option<(u64, usize)>)>,
    projection: Option<Arc<FolderSizeProjection>>,
    sort_column: SortColumn,
    sort_direction: SortDirection,
    filter: KindFilter,
    visible_rows: Option<VisibleRows>,
    treemap_cache: TreemapCache,
    folder_view: FolderView,
    treemap_press: Option<TreemapPress>,
    pending: Option<PendingFolderSizePass>,
    notice: Option<String>,
    #[cfg(test)]
    test_folder_choice: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_folder_picker_calls: usize,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
    #[cfg(test)]
    test_action_enabled: Vec<(&'static str, bool)>,
}

impl FolderSizeDialogState {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        Self::build(ctx, None)
    }

    pub(crate) fn new_with_activity_center(
        ctx: &egui::Context,
        activity_center: crate::activity::ActivityCenter,
    ) -> Self {
        Self::build(ctx, Some(activity_center))
    }

    fn build(
        ctx: &egui::Context,
        activity_center: Option<crate::activity::ActivityCenter>,
    ) -> Self {
        ctx.add_plugin(HostedCopyCapture::default());
        let wake_context = ctx.clone();
        let session = match activity_center {
            Some(activity_center) => FolderSizeSession::new_with_activity_center(
                move || wake_context.request_repaint_of(egui::ViewportId::ROOT),
                activity_center,
            ),
            None => FolderSizeSession::new(move || {
                wake_context.request_repaint_of(egui::ViewportId::ROOT);
            }),
        };
        Self {
            open: false,
            host: super::utility_host::UtilityHostState::docked(),
            presentation_prepared: false,
            host_epoch: 0,
            host_focus: false,
            host_os_close_pending: false,
            prefill_pending: None,
            session,
            selected_root: None,
            scan_root: None,
            recursive: true,
            current_directory: None,
            current_directory_path: None,
            selected_entry: None,
            row_menu: None,
            projection: None,
            sort_column: SortColumn::Size,
            sort_direction: SortDirection::Descending,
            filter: KindFilter::All,
            visible_rows: None,
            treemap_cache: TreemapCache::default(),
            folder_view: FolderView::Table,
            treemap_press: None,
            pending: None,
            notice: None,
            #[cfg(test)]
            test_folder_choice: None,
            #[cfg(test)]
            test_folder_picker_calls: 0,
            #[cfg(test)]
            test_action_rects: Vec::new(),
            #[cfg(test)]
            test_action_enabled: Vec::new(),
        }
    }

    /// Opening or focusing Folder Size never starts a scan.
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
    }

    pub(crate) fn request_close(&mut self, ctx: &egui::Context) {
        if self.open {
            self.stage(ctx, vec![FolderSizeAction::Close]);
        }
    }

    #[cfg(test)]
    pub(super) fn test_host_focus_pending(&self) -> bool { self.host_focus }

    pub(super) fn open_from_file_search(
        &mut self,
        ctx: &egui::Context,
        path: PathBuf,
        receipt: super::file_search_window_host::FileSearchOpenReceipt,
    ) {
        let was_open = self.open;
        self.open();
        self.prepare_open_presentation(ctx);
        let key =
            crate::settings::UtilityToolKey::parse("folder-size").expect("stable utility key");
        let remembered = super::utility_presentation::UtilityPresentationManager::for_context(ctx)
            .and_then(|manager| manager.preference(key))
            .map(Into::into);
        let desired = super::utility_host::resolve_child_presentation(
            was_open.then(|| self.host.presentation()),
            remembered,
            self.host.presentation(),
            &super::utility_host::ChildOpenOrigin::Parent(receipt),
        );
        self.host.transition(desired);
        self.request_host_focus();
        if desired == super::utility_host::UtilityPresentation::Popout {
            super::utility_host::remember_presentation(ctx, key, desired);
        }
        let current = self.session.view();
        match ExplicitDirectoryRoot::new(path) {
            Ok(root) if self.selected_root.as_ref() != Some(&root) => {
                self.apply_selected_root(root, &current)
            }
            Ok(_) => {}
            Err(error) => {
                self.notice = Some(format!("The File Search folder is not usable: {error:?}."))
            }
        }
    }

    pub(super) fn stage_file_search_prefill(
        &mut self,
        pass: ViewportPassProvenance,
        path: PathBuf,
    ) {
        self.prefill_pending = Some(PendingFolderSizePass {
            provenance: pass,
            owner: pass,
            host: self.host.identity(),
            epoch: self.host_epoch,
            native: false,
            info: None,
            projection: None,
            actions: vec![FolderSizeAction::Prefill(path)],
        });
    }

    pub(crate) fn stage_prefill(&mut self, ctx: &egui::Context, path: PathBuf) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage_file_search_prefill(
                ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                path,
            );
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.open = false;
        self.pending = None;
        self.session.close();
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.pending.is_some()
            || self.prefill_pending.is_some()
            || self.session.view().is_busy()
            || (self.open
                && self.host.presentation() == super::utility_host::UtilityPresentation::Popout)
    }

    fn render_host_body(
        &mut self,
        ctx: &egui::Context,
        input_allowed: bool,
        first_ui_pass: bool,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> (Vec<FolderSizeAction>, bool) {
        let view = self.session.view();
        self.sync_projection(&view);
        if !self.open {
            return (Vec::new(), false);
        }
        #[cfg(test)]
        self.test_action_rects.clear();
        #[cfg(test)]
        self.test_action_enabled.clear();

        let pass_allowed = input_allowed && first_ui_pass && !ctx.will_discard();
        let commit_allowed = pass_allowed && !egui::Popup::is_any_open(ctx);
        self.sync_visible_rows();
        let mut window_open = self.open;
        let projection = self.projection.clone();
        let selected_root = self.selected_root.clone();
        let scan_root = self.scan_root.clone();
        let selected_entry = self.selected_entry;
        let visible_indices = self.visible_rows.as_ref().map(|rows| Arc::clone(&rows.indices)).unwrap_or_default();
        let sort_column = self.sort_column;
        let sort_direction = self.sort_direction;
        let filter = self.filter;
        let folder_view = self.folder_view;
        let host_identity = self.host.identity();
        let host_epoch = self.host_epoch;
        let previous_treemap_press = self.treemap_press;
        let owned_popup = self.row_menu.map(|(_, _, popup, _, _, _)| popup);
        let current_directory = self.current_directory;
        let current_directory_path = self.current_directory_path.clone();
        let mut recursive = self.recursive;
        let original_recursive = recursive;
        let notice = self.notice.clone();
        let mut staged_actions = Vec::new();

        let mut draw = |ui: &mut egui::Ui| {
            let controls_allowed = commit_allowed && ui.is_visible() && !ui.is_sizing_pass();
            if let Some(presentation) = presentation {
                let (button, next) = super::utility_host::presentation_button(
                    ui,
                    presentation,
                    input_allowed && ui.is_visible() && !ui.is_sizing_pass(),
                );
                #[cfg(test)]
                self.test_action_rects.push((
                    "host-presentation".into(),
                    button.rect,
                    ui.clip_rect(),
                ));
                #[cfg(test)]
                self.test_action_enabled
                    .push(("host-presentation", button.enabled()));
                if button.clicked() && controls_allowed {
                    staged_actions.push(FolderSizeAction::Host(next));
                }
            }
            ui.add_enabled_ui(
                input_allowed && ui.is_visible() && !ui.is_sizing_pass(),
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let choose = ui.button("Choose folder…");
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "choose_folder".into(),
                            choose.rect,
                            ui.clip_rect(),
                        ));
                        if choose.clicked() {
                            staged_actions.push(FolderSizeAction::ChooseFolder);
                        }
                        let scan_enabled = selected_root.is_some();
                        let scan = ui.add_enabled(scan_enabled, egui::Button::new("Scan"));
                        #[cfg(test)]
                        self.test_action_rects
                            .push(("scan".into(), scan.rect, ui.clip_rect()));
                        #[cfg(test)]
                        self.test_action_enabled.push(("scan", scan.enabled()));
                        if scan.clicked()
                            && let Some(root) = selected_root.clone()
                        {
                            staged_actions.push(FolderSizeAction::Scan {
                                root,
                                session_generation: view.generation(),
                                recursive,
                            });
                        }
                        let refresh = ui.add_enabled(scan_enabled, egui::Button::new("Refresh"));
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "refresh".into(),
                            refresh.rect,
                            ui.clip_rect(),
                        ));
                        #[cfg(test)]
                        self.test_action_enabled
                            .push(("refresh", refresh.enabled()));
                        if refresh.clicked()
                            && let Some(root) = selected_root.clone()
                        {
                            staged_actions.push(FolderSizeAction::Refresh {
                                root,
                                session_generation: view.generation(),
                                recursive,
                            });
                        }
                        let cancel = ui.add_enabled(view.is_busy(), egui::Button::new("Cancel"));
                        #[cfg(test)]
                        self.test_action_rects
                            .push(("cancel".into(), cancel.rect, ui.clip_rect()));
                        #[cfg(test)]
                        self.test_action_enabled.push(("cancel", cancel.enabled()));
                        if cancel.clicked() {
                            staged_actions.push(FolderSizeAction::Cancel {
                                session_generation: view.generation(),
                            });
                        }

                        let selected_target = selected_entry.filter(|(generation, index)| {
                            projection.as_ref().is_some_and(|projection| {
                                projection.contains_row(*generation, *index) && visible_indices.contains(index)
                            })
                        });
                        let can_copy_or_open = selected_target.is_some() || selected_root.is_some();
                        let copy = ui.add_enabled(can_copy_or_open, egui::Button::new("Copy Path"));
                        #[cfg(test)]
                        self.test_action_rects
                            .push(("copy".into(), copy.rect, ui.clip_rect()));
                        if copy.clicked() {
                            if let Some((generation, index)) = selected_target {
                                staged_actions
                                    .push(FolderSizeAction::CopyRow { generation, index });
                            } else if let Some(root) = selected_root.clone() {
                                staged_actions.push(FolderSizeAction::CopyRoot(root));
                            }
                        }
                        if ui
                            .add_enabled(can_copy_or_open, egui::Button::new("Reveal in Explorer"))
                            .clicked()
                        {
                            if let Some((generation, index)) = selected_target {
                                staged_actions
                                    .push(FolderSizeAction::OpenRow { generation, index });
                            } else if let Some(root) = selected_root.clone() {
                                staged_actions.push(FolderSizeAction::OpenRoot(root));
                            }
                        }
                        let recursive_response = ui.checkbox(&mut recursive, "Include subfolders");
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "recursive".into(),
                            recursive_response.rect,
                            ui.clip_rect(),
                        ));
                        if recursive_response.changed() {
                            staged_actions.push(FolderSizeAction::SetRecursive(recursive));
                        }
                    });

                    if recursive != original_recursive {
                        if let Some(FolderSizeAction::Scan {
                            recursive: scan_recursive,
                            ..
                        }) = staged_actions
                            .iter_mut()
                            .find(|action| matches!(action, FolderSizeAction::Scan { .. }))
                        {
                            *scan_recursive = recursive;
                        }
                        if let Some(FolderSizeAction::Refresh {
                            recursive: refresh_recursive,
                            ..
                        }) = staged_actions
                            .iter_mut()
                            .find(|action| matches!(action, FolderSizeAction::Refresh { .. }))
                        {
                            *refresh_recursive = recursive;
                        }
                    }

                    if let Some(root) = selected_root.as_ref() {
                        let _label = ui.label(format!("Root: {}", root.as_path().display()));
                        #[cfg(test)]
                        self.test_action_rects.push((
                            "root-label".into(),
                            _label.rect,
                            ui.clip_rect(),
                        ));
                    } else {
                        ui.label(
                            "Choose a folder to begin. Opening Folder Size does not start a scan.",
                        );
                    }
                    if let Some(message) = notice.as_deref() {
                        ui.colored_label(ui.visuals().warn_fg_color, message);
                    }

                    let scan_matches_selection = scan_root.is_some() && scan_root == selected_root;
                    if scan_matches_selection {
                        show_scan_status(ui, &view, projection.as_deref(),
                            #[cfg(test)] &mut self.test_action_rects);
                    }
                    if let Some(projection) = projection.as_deref() {
                        ui.horizontal_wrapped(|ui| {
                            for (label, value) in [
                                ("Table", FolderView::Table),
                                ("Treemap", FolderView::Treemap),
                                ("Both", FolderView::Both),
                            ] {
                                let response = ui.selectable_label(folder_view == value, label);
                                #[cfg(test)]
                                self.test_action_rects.push((
                                    format!("view/{label}"),
                                    response.rect,
                                    ui.clip_rect(),
                                ));
                                if response.clicked() && commit_allowed {
                                    staged_actions.push(FolderSizeAction::SetView(value));
                                }
                            }
                        });
                        macro_rules! rows {
                            ($ui:expr, $show:expr) => {
                                show_folder_rows(
                                    $ui,
                                    projection,
                                    selected_root.as_ref(),
                                    current_directory,
                                    current_directory_path.as_deref(),
                                    selected_entry,
                                    &visible_indices,
                                    sort_column,
                                    sort_direction,
                                    filter,
                                    owned_popup,
                                    commit_allowed,
                                    pass_allowed,
                                    $show,
                                    &mut staged_actions,
                                    #[cfg(test)]
                                    &mut self.test_action_rects,
                                )
                            };
                        }
                        macro_rules! chart {
                            ($ui:expr) => {{
                                let size = treemap_view_size($ui);
                                if let Some(layout) =
                                    self.treemap_layout(size.x as f64, size.y as f64)
                                {
                                    show_treemap(
                                        $ui,
                                        projection,
                                        &layout,
                                        size,
                                        selected_entry,
                                        current_directory,
                                        filter,
                                        host_identity,
                                        host_epoch,
                                        previous_treemap_press,
                                        owned_popup,
                                        commit_allowed,
                                        pass_allowed,
                                        &mut staged_actions,
                                        #[cfg(test)]
                                        &mut self.test_action_rects,
                                    );
                                }
                            }};
                        }
                        if folder_view == FolderView::Both && treemap_view_size(ui).x >= 850.0 {
                            ui.columns(2, |columns| {
                                for column in columns.iter_mut() {
                                    column.set_clip_rect(
                                        column.clip_rect().intersect(column.max_rect()),
                                    );
                                }
                                rows!(&mut columns[0], true);
                                chart!(&mut columns[1]);
                            });
                        } else {
                            if folder_view == FolderView::Both {
                                let height = (ui.available_height() * 0.45).max(100.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), height),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                                        rows!(ui, true);
                                    },
                                );
                                chart!(ui);
                            } else {
                                rows!(ui, folder_view == FolderView::Table);
                                if folder_view == FolderView::Treemap {
                                    chart!(ui);
                                }
                            }
                        }
                    } else if selected_root.is_some() {
                        ui.label("No scan results yet. Choose Scan when you are ready.");
                    }
                },
            );
        };
        if let Some(ui) = child_ui {
            draw(ui);
        } else {
            egui::Window::new(WINDOW_TITLE)
                .id(egui::Id::new(Some(WINDOW_TITLE)))
                .collapsible(false)
                .resizable(true)
                .default_size((900.0, 620.0))
                .min_size((560.0, 360.0))
                .max_size(ctx.content_rect().size())
                .show_utility(
                    ctx,
                    UTILITY_KEY,
                    egui::Id::new(WINDOW_TITLE),
                    Some(&mut window_open),
                    crate::gui::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        crate::gui::utility_dialog_size::UtilityWindowSizeDefault::outer(
                            egui::vec2(900.0, 620.0),
                        ),
                        egui::vec2(560.0, 360.0),
                        [None, None],
                    )
                    .with_maximum(ctx.content_rect().size()),
                    |ui| draw(ui),
                );
        }
        if !commit_allowed {
            staged_actions.retain(|action| pass_allowed && matches!(action, FolderSizeAction::RevealRow { .. }));
        }
        (staged_actions, !window_open && commit_allowed)
    }
    pub(super) fn owns_row_menu(&self, ctx: &egui::Context) -> bool {
        self.row_menu.is_some_and(|(key, index, popup, host, epoch, directory)| {
            self.open && self.host.identity() == host && self.host_epoch == epoch
                && self.current_directory == directory
                && self.projection.as_ref().is_some_and(|p| {
                    p.contains_row(key.generation, index)
                        && p.row(index).is_some_and(|row| {
                            row.parent_index == directory.map(|(_, parent)| parent).or(p.root_index)
                                && filter_matches(self.filter, row.kind)
                        })
                })
                && egui::Popup::is_id_open(ctx, popup)
        })
    }
    pub(super) fn owns_row_menu_layer(&self, ctx: &egui::Context, layer: egui::LayerId) -> bool {
        self.owns_row_menu(ctx) && self.row_menu.is_some_and(|(_, _, popup, _, _, _)| layer.id == popup)
    }
    pub(super) fn prepare_open_presentation(&mut self, ctx: &egui::Context) {
        if self.open && !self.presentation_prepared {
            let key =
                crate::settings::UtilityToolKey::parse("folder-size").expect("stable utility key");
            super::utility_host::restore_remembered_presentation(ctx, key, &mut self.host);
            self.presentation_prepared = true;
        }
    }

    pub(super) fn is_popped_out(&self) -> bool {
        self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }

    pub(super) fn remember_final_presentation(&self, ctx: &egui::Context) {
        if self.open {
            let key =
                crate::settings::UtilityToolKey::parse("folder-size").expect("stable utility key");
            super::utility_host::remember_presentation(ctx, key, self.host.presentation());
        }
    }

    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then_some(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new(Some(WINDOW_TITLE)),
        ))
    }
    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.host.bootstrap_pending()
    }
    pub(crate) fn awaiting_root_focus(&self) -> bool {
        self.host_focus
            && self.host.presentation() == super::utility_host::UtilityPresentation::Docked
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host_focus = true;
        self.host.request_focus();
    }
    pub(super) fn presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    pub(crate) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("folder-size-viewport")
    }
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(UTILITY_KEY)
                .expect("stable folder-size key"),
            viewport_id: Self::viewport(),
            owner_generation: self.host_epoch,
            host_generation: host.generation,
        }
    }
    fn retire_host(&mut self, ctx: &egui::Context) {
        if self.host.presentation() == super::utility_host::UtilityPresentation::Popout {
            let identity = self.native_identity(self.host.identity());
            super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
            ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
        }
        self.host.retire_to_docked();
        self.presentation_prepared = false;
        self.host_os_close_pending = false;
        self.host_focus = false;
    }
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        input_allowed: bool,
        first: bool,
    ) {
        self.show_hosted(
            ctx,
            root_visible,
            input_allowed,
            input_allowed,
            first,
            false,
            true,
        );
    }
    pub(crate) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        root_owner: bool,
        application_input_allowed: bool,
        first: bool,
        pinned: bool,
        shortcut_allowed: bool,
    ) {
        self.prepare_open_presentation(ctx);
        use super::utility_host::UtilityPresentation;
        let view = self.session.view();
        self.sync_projection(&view);
        if !self.open {
            return;
        }
        if pinned {
            self.host_os_close_pending = false;
        }
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut owner = None;
        let mut native = false;
        let mut info = None;
        let mut staged = None;
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_owner && (!egui::Popup::is_any_open(ctx) || self.owns_row_menu(ctx));
                let shortcut = first
                    && allowed
                    && shortcut_allowed
                    && !pinned
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let (actions, close) =
                    self.render_host_body(ctx, allowed, first, None, Some(host.host));
                staged = Some((actions, close || shortcut));
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title(WINDOW_TITLE)
                .with_inner_size([900.0, 620.0])
                .with_min_inner_size([560.0, 360.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let c = child.ctx().clone();
                let is_native =
                    class == egui::ViewportClass::Immediate && c.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner = Some(ViewportPassProvenance::current(&c, c.viewport_id()));
                    native = is_native;
                    if is_native {
                        info = Some(child.input(|i| i.viewport().clone()));
                    }
                } else {
                    owner = None;
                    info = None;
                    staged = None;
                }
                let allowed = if is_native {
                    application_input_allowed
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && (!egui::Popup::is_any_open(&c) || self.owns_row_menu(&c))
                        && super::jump_mode::ui::focused_response_layer(&c)
                            .is_none_or(|l| l == egui::LayerId::background() || self.owns_row_menu_layer(&c, l))
                } else {
                    root_owner && (!egui::Popup::is_any_open(&c) || self.owns_row_menu(&c))
                };
                if is_native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.host_os_close_pending = true;
                    }
                }
                let shortcut = child_first
                    && allowed
                    && !pinned
                    && (is_native || shortcut_allowed)
                    && c.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                if is_native {
                    let receipt = HostedCopyReceipt {
                        root,
                        owner: ViewportPassProvenance::current(&c, c.viewport_id()),
                        host,
                        epoch: self.host_epoch,
                        eligible: child_first && allowed && !c.will_discard(),
                    };
                    c.with_plugin(|plugin: &mut HostedCopyCapture| {
                        plugin.armed.insert(c.viewport_id(), receipt);
                    });
                    egui::CentralPanel::default().show(child, |ui| {
                        staged = Some(self.render_host_body(
                            &c,
                            allowed,
                            child_first,
                            Some(ui),
                            Some(host.host),
                        ));
                    });
                } else {
                    staged = Some(self.render_host_body(
                        &c,
                        allowed,
                        child_first,
                        None,
                        Some(host.host),
                    ));
                }
                if let Some((_, close)) = &mut staged {
                    *close |= shortcut || (is_native && !pinned && self.host_os_close_pending);
                }
            });
        }
        if let (Some(owner), Some((mut actions, close))) = (owner, staged) {
            if close && !pinned {
                actions = vec![FolderSizeAction::Close];
            } else if let Some(next) = actions.iter().find_map(|action| {
                if let FolderSizeAction::Host(next) = action {
                    Some(*next)
                } else {
                    None
                }
            }) {
                actions = vec![FolderSizeAction::Host(next)];
            }
            if first && !ctx.will_discard() {
                if let Some(previous) = self.pending.as_ref()
                    && previous
                        .actions
                        .iter()
                        .any(|action| matches!(action, FolderSizeAction::Close))
                {
                    return;
                }
                self.pending = Some(PendingFolderSizePass {
                    provenance: root,
                    owner,
                    host,
                    epoch: self.host_epoch,
                    native,
                    info,
                    projection: self.projection.clone(),
                    actions,
                });
            }
        }
    }

    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<FolderSizeDialogEffect> {
        let view = self.session.view();
        self.sync_projection(&view);
        if self.pending.as_ref().is_some_and(|pending| {
            pending.provenance.was_retained(ctx)
                && pending.owner.was_retained(ctx)
                && pending
                    .actions
                    .iter()
                    .any(|action| matches!(action, FolderSizeAction::Close))
        }) {
            self.prefill_pending = None;
        }
        let prefill = self.prefill_pending.is_some();
        let Some(pending) = self.prefill_pending.take().or_else(|| self.pending.take()) else {
            return Vec::new();
        };
        if !pending.provenance.was_retained(ctx)
            || !pending.owner.was_retained(ctx)
            || (!prefill
                && (pending.epoch != self.host_epoch || pending.host != self.host.identity()))
        {
            if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                > pending.provenance.frame_nr
            {
                // The originating input frame completed without retaining this
                // pass. A discarded click has no authority in the next frame.
                self.pending = None;
            } else {
                if prefill {
                    self.prefill_pending = Some(pending);
                } else {
                    self.pending = Some(pending);
                }
            }
            return Vec::new();
        }

        if !prefill {
            let identity = self.native_identity(pending.host);
            if let Some(info) = pending.info.as_ref() {
                super::utility_dialog_size::apply_native_utility_geometry_restore(
                    ctx,
                    egui::ViewportClass::Immediate,
                    true,
                    WINDOW_TITLE,
                    identity,
                    info,
                    egui::vec2(900.0, 620.0),
                    Some(egui::vec2(560.0, 360.0)),
                    None,
                );
                if self.host_focus {
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
                }
            } else if self.host_focus && let Some(layer) = self.root_layer() {
                ctx.move_to_top(layer);
            }
            self.host_focus = false;
            if pending.host.host == super::utility_host::UtilityPresentation::Popout {
                self.host.complete_bootstrap(
                    pending.host,
                    (!pending.native).then(|| {
                        egui::LayerId::new(egui::Order::Middle, egui::Id::new(Some(WINDOW_TITLE)))
                    }),
                    None,
                );
            }
        }
        let mut effects = Vec::new();
        if !prefill {
            let copies = ctx
                .with_plugin(|plugin: &mut HostedCopyCapture| {
                    let mut accepted = Vec::new();
                    let mut other = Vec::new();
                    for (receipt, copies) in plugin.captured.drain(..) {
                        if receipt.owner.viewport_id != pending.owner.viewport_id {
                            other.push((receipt, copies));
                        } else if receipt.root == pending.provenance
                            && receipt.owner == pending.owner
                            && receipt.host == pending.host
                            && receipt.epoch == pending.epoch
                        {
                            accepted.extend(copies);
                        }
                    }
                    plugin.captured = other;
                    accepted
                })
                .unwrap_or_default();
            let local_close_or_move = pending.actions.iter().any(|action| {
                matches!(action, FolderSizeAction::Close | FolderSizeAction::Host(_))
            });
            if !local_close_or_move {
                effects.extend(copies.into_iter().map(FolderSizeDialogEffect::CopyPath));
            }
        }
        let had_actions = !pending.actions.is_empty();
        for action in pending.actions {
            let current = self.session.view();
            self.sync_projection(&current);
            let row_action = matches!(
                action,
                FolderSizeAction::Select { .. }
                    | FolderSizeAction::RevealRow { .. } | FolderSizeAction::OpenRowMenu { .. }
                    | FolderSizeAction::SetSort(_) | FolderSizeAction::SetFilter(_) | FolderSizeAction::TreemapPress(_) | FolderSizeAction::TreemapRelease(_, _)
                    | FolderSizeAction::Drill { .. }
                    | FolderSizeAction::Back { .. }
                    | FolderSizeAction::CopyRow { .. }
                    | FolderSizeAction::OpenRow { .. }
            );
            if row_action
                && pending.projection.as_ref().is_some_and(|origin| {
                    !self
                        .projection
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(origin, current))
                })
            {
                continue;
            }
            match action {
                FolderSizeAction::SetView(view) => {
                    self.folder_view = view;
                    self.treemap_press = None;
                }
                FolderSizeAction::CancelTreemapPress => {
                    self.treemap_press = None;
                }
                FolderSizeAction::TreemapPress(press) => {
                    if self.folder_view != FolderView::Table
                        && press.host == self.host.identity()
                        && press.epoch == self.host_epoch
                        && press.directory == self.current_directory
                        && press.filter == self.filter
                        && self
                            .projection
                            .as_ref()
                            .is_some_and(|p| p.key == press.projection)
                    {
                        self.treemap_press = Some(press);
                    }
                }
                FolderSizeAction::TreemapRelease(press, double) => {
                    let accepted = self.treemap_press.take() == Some(press)
                        && self.folder_view != FolderView::Table
                        && press.host == self.host.identity()
                        && press.epoch == self.host_epoch
                        && press.directory == self.current_directory
                        && press.filter == self.filter
                        && self
                            .projection
                            .as_ref()
                            .is_some_and(|p| p.key == press.projection)
                        && self
                            .visible_rows
                            .as_ref()
                            .is_some_and(|rows| rows.indices.contains(&press.row));
                    if accepted {
                        self.select_row(press.projection.generation, press.row);
                        if double {
                            self.drill_row(press.projection.generation, press.row);
                        }
                    }
                }
                FolderSizeAction::Host(next) => {
                    self.treemap_press = None;
                    if self.host.transition(next) {
                        let key =
                            crate::settings::UtilityToolKey::parse("folder-size").expect("stable utility key");
                        super::utility_host::remember_presentation(ctx, key, next);
                        if next == super::utility_host::UtilityPresentation::Docked {
                            let identity = self.native_identity(pending.host);
                            super::utility_dialog_size::close_native_utility_geometry_viewport(
                                ctx, identity,
                            );
                            ctx.send_viewport_cmd_to(
                                identity.viewport_id,
                                egui::ViewportCommand::Close,
                            );
                            effects.push(FolderSizeDialogEffect::ShowRoot);
                        }
                        self.host_focus = true;
                    }
                }
                FolderSizeAction::ChooseFolder => {
                    if !pending.provenance.was_retained(ctx) || !pending.owner.was_retained(ctx) {
                        continue;
                    }
                    let selected_before = self.selected_root.clone();
                    let generation_before = current.generation();
                    let identity = pending.native.then(|| self.native_identity(pending.host));
                    #[cfg(windows)]
                    let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, identity);
                    #[cfg(not(windows))]
                    let hwnd = None;
                    if let Some(path) = self.pick_folder(hwnd) {
                        if !self.open
                            || self.host_epoch != pending.epoch
                            || self.host.identity() != pending.host
                            || self.selected_root != selected_before
                            || self.session.view().generation() != generation_before
                        {
                            continue;
                        }
                        #[cfg(all(windows, not(test)))]
                        if hwnd.is_none()
                            || hwnd
                                != super::utility_dialog_size::utility_picker_hwnd(ctx, identity)
                        {
                            continue;
                        }
                        match ExplicitDirectoryRoot::new(path) {
                            Ok(root) => {
                                self.apply_selected_root(root, &current);
                                self.notice = None;
                            }
                            Err(error) => {
                                self.notice =
                                    Some(format!("The selected folder is not usable: {error:?}."));
                            }
                        }
                    }
                }
                FolderSizeAction::Prefill(path) => {
                    match ExplicitDirectoryRoot::new(path) {
                        Ok(root) => {
                            if self.selected_root.as_ref() != Some(&root) {
                                self.apply_selected_root(root, &current);
                            }
                        }
                        Err(error) => {
                            self.notice =
                                Some(format!("The File Search folder is not usable: {error:?}."));
                        }
                    }
                    effects.push(FolderSizeDialogEffect::FocusPanel);
                }
                FolderSizeAction::SetRecursive(recursive) => {
                    self.recursive = recursive;
                }
                FolderSizeAction::Scan {
                    root,
                    session_generation,
                    recursive,
                }
                | FolderSizeAction::Refresh {
                    root,
                    session_generation,
                    recursive,
                } => {
                    if current.generation() != session_generation
                        || self.selected_root.as_ref() != Some(&root)
                    {
                        continue;
                    }
                    match self.session.refresh(root, recursive) {
                        Ok(_) => {
                            self.scan_root = self.selected_root.clone();
                            self.current_directory = None;
                            self.current_directory_path = None;
                            self.selected_entry = None;
                            self.projection = None;
                            self.notice = None;
                        }
                        Err(error) => self.notice = Some(error.to_string()),
                    }
                }
                FolderSizeAction::Cancel { session_generation } => {
                    if current.generation() == session_generation && current.is_busy() {
                        match self.session.cancel() {
                            Ok(true) => {
                                self.notice = Some(
                                    "Scan cancelled. Showing the last verified subtotal.".into(),
                                );
                            }
                            Ok(false) => {}
                            Err(error) => self.notice = Some(error.to_string()),
                        }
                    }
                }
                FolderSizeAction::SetSort(column) => {
                    if self.sort_column == column {
                        self.sort_direction = if self.sort_direction == SortDirection::Ascending { SortDirection::Descending } else { SortDirection::Ascending };
                    } else {
                        self.sort_column = column;
                        self.sort_direction = SortDirection::Ascending;
                    }
                    self.visible_rows = None;
                    self.sync_visible_rows();
                }
                FolderSizeAction::SetFilter(filter) => {
                    self.filter = filter;
                    self.visible_rows = None;
                    self.sync_visible_rows();
                }
                FolderSizeAction::OpenRowMenu { generation, index, popup } => {
                    if let Some(projection) = self.projection.as_ref()
                        && projection.contains_row(generation, index)
                        && self.visible_rows.as_ref().is_some_and(|rows| rows.indices.contains(&index))
                    {
                        self.selected_entry = Some((generation, index));
                        self.row_menu = Some((projection.key, index, popup, pending.host, pending.epoch, self.current_directory));
                    }
                }
                FolderSizeAction::RevealRow { generation, index } => {
                    self.sync_visible_rows();
                    if self.open && self.visible_rows.as_ref().is_some_and(|rows| rows.indices.contains(&index))
                        && let Some(projection) = self.projection.as_ref()
                    {
                        if projection.source.path_at(index).is_none() {
                            self.notice = Some("Cannot reveal: this row has no retained path.".into());
                            continue;
                        }
                        self.selected_entry = Some((generation, index));
                        effects.push(FolderSizeDialogEffect::Reveal(FolderSizeRevealRequest {
                            key: projection.key, index, directory: self.current_directory,
                            host: pending.host, epoch: pending.epoch,
                        }));
                    }
                }
                FolderSizeAction::Select { generation, index } => self.select_row(generation, index),
                FolderSizeAction::Drill { generation, index } => self.drill_row(generation, index),
                FolderSizeAction::Back { generation, index } => {
                    if self.current_directory == Some((generation, index))
                        && let Some(projection) = self.projection.as_ref()
                        && projection.contains_row(generation, index)
                    {
                        self.current_directory = projection
                            .row(index)
                            .and_then(|row| row.parent_index)
                            .map(|parent| (generation, parent));
                        self.selected_entry = None;
                        self.current_directory_path =
                            self.current_directory
                                .and_then(|(parent_generation, parent_index)| {
                                    (parent_generation == generation)
                                        .then(|| projection.source.path_at(parent_index))
                                        .flatten()
                                });
                    }
                }
                FolderSizeAction::CopyRoot(root) => {
                    if self.selected_root.as_ref() == Some(&root) {
                        effects.push(FolderSizeDialogEffect::CopyPath(
                            root.as_path().display().to_string(),
                        ));
                    }
                }
                FolderSizeAction::CopyRow { generation, index } => {
                    if let Some(path) = self.current_row_path(&current, generation, index) {
                        effects.push(FolderSizeDialogEffect::CopyPath(path.display().to_string()));
                    }
                }
                FolderSizeAction::OpenRoot(root) => {
                    if self.selected_root.as_ref() == Some(&root) {
                        effects.push(FolderSizeDialogEffect::OpenPath(
                            root.as_path().to_path_buf(),
                        ));
                    }
                }
                FolderSizeAction::OpenRow { generation, index } => {
                    if let Some(path) = self.current_row_path(&current, generation, index) {
                        effects.push(FolderSizeDialogEffect::OpenPath(path));
                    }
                }
                FolderSizeAction::Close => {
                    self.remember_final_presentation(ctx);
                    if current.is_busy() {
                        let _ = self.session.cancel();
                    }
                    self.retire_host(ctx);
                    self.open = false;
                    self.selected_entry = None;
                    self.current_directory = None;
                    self.current_directory_path = None;
                    self.notice = None;
                }
            }
        }
        if had_actions {
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
        if prefill && self.pending.is_some() {
            effects.extend(self.on_ui_frame_complete(ctx));
        }
        effects
    }

    fn apply_selected_root(
        &mut self,
        root: ExplicitDirectoryRoot,
        current: &crate::folder_size::FolderSizeSessionView,
    ) {
        if self.selected_root.as_ref() != Some(&root) {
            if matches!(
                current.status(),
                FolderSizeSessionStatus::Scanning | FolderSizeSessionStatus::RefreshQueued
            ) {
                let _ = self.session.cancel();
            }
            self.scan_root = None;
            self.selected_root = Some(root);
            self.current_directory = None;
            self.current_directory_path = None;
            self.selected_entry = None;
            self.projection = None;
            self.notice = None;
        }
    }

    pub(crate) fn copy_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Copied to the clipboard.".into(),
            Err(error) => error,
        });
    }

    pub(crate) fn path_action_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Path opened in File Explorer.".into(),
            Err(error) => error,
        });
    }

    fn select_row(&mut self, generation: u64, index: usize) {
        if self
            .projection
            .as_ref()
            .is_some_and(|projection| projection.contains_row(generation, index))
        {
            self.selected_entry = Some((generation, index));
        }
    }

    fn drill_row(&mut self, generation: u64, index: usize) {
        if let Some(projection) = self.projection.as_ref()
            && projection.contains_row(generation, index)
            && projection
                .row(index)
                .is_some_and(|row| row.kind == FolderSizeEntryKind::Directory)
            && let Some(path) = projection.source.path_at(index)
        {
            self.selected_entry = Some((generation, index));
            self.current_directory = Some((generation, index));
            self.current_directory_path = Some(path.clone());
            self.notice = Some(format!("Showing {}", path.display()));
        }
    }

    // Called only when the treemap is requested; table rendering does no layout work.
    fn treemap_layout(
        &mut self,
        width: f64,
        height: f64,
    ) -> Option<Arc<crate::folder_size::treemap::Layout>> {
        let projection = self.projection.as_ref()?;
        let directory = self
            .current_directory
            .filter(|(generation, _)| *generation == projection.key.generation)
            .map(|(_, index)| index)
            .or(projection.root_index)?;
        self.treemap_cache.layout(
            projection.key,
            &projection.rows,
            &projection.children,
            directory,
            self.filter,
            width,
            height,
        )
    }
    fn sync_visible_rows(&mut self) {
        let Some(projection) = self.projection.as_ref() else { self.visible_rows = None; return; };
        let Some(directory) = self.current_directory.filter(|(generation, _)| *generation == projection.key.generation)
            .map(|(_, index)| index).or(projection.root_index) else { self.visible_rows = None; return; };
        let key = VisibleKey { projection: projection.key, directory, column: self.sort_column, direction: self.sort_direction, filter: self.filter };
        if self.visible_rows.as_ref().is_some_and(|cache| cache.key == key) { return; }
        let mut indices = projection.children.get(directory).into_iter().flatten().copied()
            .filter(|index| filter_matches(self.filter, projection.rows[*index].kind)).collect::<Vec<_>>();
        indices.sort_by(|a, b| compare_rows(&projection.rows, *a, *b, self.sort_column, self.sort_direction, projection.total_logical_bytes));
        if self.selected_entry.is_some_and(|(generation, index)| generation != key.projection.generation || (!indices.contains(&index) && self.current_directory != Some((generation, index)))) {
            self.selected_entry = None;
        }
        self.visible_rows = Some(VisibleRows { key, indices: Arc::new(indices) });
    }

    pub(crate) fn take_reveal_path_for_dispatch(&mut self, request: &FolderSizeRevealRequest) -> Result<Option<PathBuf>, String> {
        let view = self.session.view();
        self.sync_projection(&view);
        self.sync_visible_rows();
        if !self.open || self.host_epoch != request.epoch || self.host.identity() != request.host
            || self.current_directory != request.directory || self.selected_entry != Some((request.key.generation, request.index))
            || self.projection.as_ref().map(|p| p.key) != Some(request.key)
            || !self.visible_rows.as_ref().is_some_and(|rows| rows.indices.contains(&request.index)) {
            return Ok(None);
        }
        let Some(path) = self.current_row_path(&view, request.key.generation, request.index) else { return Ok(None); };
        match path.try_exists() {
            Ok(true) => Ok(Some(path)),
            Ok(false) => Err(format!("Missing path: {}", path.display())),
            Err(error) => Err(format!("Cannot access {}: {error}", path.display())),
        }
    }

    pub(crate) fn reveal_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result { Ok(()) => "Revealed in File Explorer.".into(), Err(error) => format!("Could not reveal selected path: {error}") });
    }

    fn current_row_path(
        &self,
        view: &crate::folder_size::FolderSizeSessionView,
        generation: u64,
        index: usize,
    ) -> Option<PathBuf> {
        let published_generation = if view.result_is_current() {
            Some(view.generation())
        } else {
            view.progress().and_then(|progress| {
                (view.progress_is_current() || view.status() == FolderSizeSessionStatus::Cancelled)
                    .then_some(progress.generation())
            })
        };
        if published_generation != Some(generation)
            || self.scan_root.as_ref() != self.selected_root.as_ref()
        {
            return None;
        }
        self.projection
            .as_ref()
            .filter(|projection| projection.contains_row(generation, index))
            .and_then(|projection| projection.row(index))?;
        self.projection
            .as_ref()
            .filter(|projection| projection.contains_row(generation, index))
            .and_then(|projection| projection.source.path_at(index))
    }

    fn sync_projection(&mut self, view: &crate::folder_size::FolderSizeSessionView) {
        let next_key = if self.scan_root.as_ref() != self.selected_root.as_ref() {
            None
        } else if view.result_is_current() {
            Some(ProjectionKey {
                generation: view.generation(),
                version: ProjectionVersion::Result,
            })
        } else if let Some(progress) = view.progress()
            && (view.progress_is_current() || view.status() == FolderSizeSessionStatus::Cancelled)
        {
            Some(ProjectionKey {
                generation: progress.generation(),
                version: ProjectionVersion::Progress(progress.publication_id()),
            })
        } else {
            None
        };
        if self.projection.as_ref().map(|projection| projection.key) == next_key {
            return;
        }
        let old_generation = self
            .projection
            .as_ref()
            .map(|projection| projection.key.generation);
        let new_generation = next_key.map(|key| key.generation);
        if old_generation != new_generation {
            self.selected_entry = None;
            self.current_directory = None;
            self.current_directory_path = None;
        }
        self.projection = next_key.and_then(|key| {
            if key.version == ProjectionVersion::Result {
                view.result()
                    .filter(|result| result.generation() == key.generation)
                    .cloned()
                    .map(FolderSizeProjection::from_result)
                    .map(Arc::new)
            } else {
                view.progress()
                    .filter(|progress| {
                        progress.generation() == key.generation
                            && progress.publication_id()
                                == match key.version {
                                    ProjectionVersion::Progress(publication_id) => publication_id,
                                    ProjectionVersion::Result => 0,
                                }
                    })
                    .map(FolderSizeProjection::from_progress)
                    .map(Arc::new)
            }
        });
        if let Some((generation, index)) = self.current_directory
            && !self.projection.as_ref().is_some_and(|projection| {
                projection.contains_row(generation, index)
                    && projection
                        .row(index)
                        .is_some_and(|row| row.kind == FolderSizeEntryKind::Directory)
            })
        {
            self.current_directory = None;
            self.current_directory_path = None;
        }
        if let Some((generation, index)) = self.selected_entry
            && !self
                .projection
                .as_ref()
                .is_some_and(|projection| projection.contains_row(generation, index))
        {
            self.selected_entry = None;
        }
    }

    fn stage(&mut self, ctx: &egui::Context, actions: Vec<FolderSizeAction>) {
        self.stage_with_provenance(
            ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
            actions,
        );
    }

    fn stage_with_provenance(
        &mut self,
        provenance: ViewportPassProvenance,
        actions: Vec<FolderSizeAction>,
    ) {
        if actions.is_empty() {
            return;
        }
        if actions
            .iter()
            .any(|action| matches!(action, FolderSizeAction::Close))
        {
            self.pending = Some(PendingFolderSizePass {
                provenance,
                owner: provenance,
                host: self.host.identity(),
                epoch: self.host_epoch,
                native: false,
                info: None,
                projection: None,
                actions: vec![FolderSizeAction::Close],
            });
            return;
        }
        match self.pending.as_mut() {
            Some(pending) if pending.provenance == provenance => {
                if !pending
                    .actions
                    .iter()
                    .any(|action| matches!(action, FolderSizeAction::Close))
                {
                    pending.actions.extend(actions);
                }
            }
            _ => {
                self.pending = Some(PendingFolderSizePass {
                    provenance,
                    owner: provenance,
                    host: self.host.identity(),
                    epoch: self.host_epoch,
                    native: false,
                    info: None,
                    projection: None,
                    actions,
                });
            }
        }
    }

    fn pick_folder(&mut self, hwnd: Option<usize>) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(choice) = self.test_folder_choice.take() {
            self.test_folder_picker_calls = self.test_folder_picker_calls.saturating_add(1);
            return choice;
        }

        #[cfg(not(test))]
        {
            #[cfg(windows)]
            {
                let Some(hwnd) = hwnd.and_then(|h| std::num::NonZeroIsize::new(h as isize)) else {
                    self.notice = Some(
                        "Cannot verify the folder picker window. Focus Folder Size and try again."
                            .into(),
                    );
                    return None;
                };
                rfd::FileDialog::new()
                    .set_parent(&super::add_action_dialog::NativeDialogParent(hwnd))
                    .pick_folder()
            }
            #[cfg(not(windows))]
            {
                let _ = hwnd;
                rfd::FileDialog::new().pick_folder()
            }
        }
        #[cfg(test)]
        {
            let _ = hwnd;
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn age_treemap_press_publication_for_test(&mut self) {
        self.treemap_press
            .as_mut()
            .expect("retained tile press")
            .projection
            .version = ProjectionVersion::Progress(0);
    }

    #[cfg(test)]
    pub(crate) fn age_row_menu_publication_for_test(&mut self) {
        let (key, ..) = self.row_menu.as_mut().expect("retained row menu");
        key.version = ProjectionVersion::Progress(0);
    }

    #[cfg(test)]
    pub(crate) fn inject_long_issues_for_test(&mut self) {
        let projection = Arc::make_mut(self.projection.as_mut().unwrap());
        projection.counts.issues = 20;
        projection.counts.issues_truncated = true;
        projection.issues = (0..20).map(|index| (None, FolderSizeIssueKind::Inaccessible, format!("issue {index}: {}", "long detail ".repeat(30)))).collect();
    }

    #[cfg(test)]
    pub(crate) fn visible_cache_for_test(&self) -> Option<Arc<Vec<usize>>> {
        self.visible_rows.as_ref().map(|rows| Arc::clone(&rows.indices))
    }

    #[cfg(test)]
    pub(crate) fn set_folder_choice_for_test(&mut self, choice: Option<PathBuf>) {
        self.test_folder_choice = Some(choice);
    }

    #[cfg(test)]
    pub(crate) fn folder_picker_calls_for_test(&self) -> usize {
        self.test_folder_picker_calls
    }

    #[cfg(test)]
    pub(crate) fn test_action_rect(&self, name: &str) -> Option<(egui::Rect, egui::Rect)> {
        self.test_action_rects
            .iter()
            .find(|(candidate, _, _)| candidate == name)
            .map(|(_, rect, clip)| (*rect, *clip))
    }

    #[cfg(test)]
    pub(crate) fn test_action_enabled(&self, name: &str) -> Option<bool> {
        self.test_action_enabled
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, enabled)| *enabled)
    }

    #[cfg(test)]
    pub(super) fn test_presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    #[cfg(test)]
    pub(crate) fn test_navigation_state(
        &self,
    ) -> (
        bool,
        Option<(u64, usize)>,
        Option<(u64, usize)>,
        Option<PathBuf>,
    ) {
        (
            self.recursive,
            self.selected_entry,
            self.current_directory,
            self.current_directory_path.clone(),
        )
    }
    #[cfg(test)]
    pub(crate) fn selected_root_for_test(&self) -> Option<&ExplicitDirectoryRoot> {
        self.selected_root.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn session_view_for_test(&self) -> crate::folder_size::FolderSizeSessionView {
        self.session.view()
    }
    #[cfg(test)]
    pub(crate) fn set_session_for_test(&mut self, session: FolderSizeSession) {
        self.session = session;
    }
}

fn show_scan_status(
    ui: &mut egui::Ui,
    view: &crate::folder_size::FolderSizeSessionView,
    projection: Option<&FolderSizeProjection>,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.label(status_label(view.status()));
        if let Some(projection) = projection {
            ui.label(format!("Subtotal {} • {} entries • {} issues", format_bytes(projection.total_logical_bytes), projection.counts.entries, projection.counts.issues));
        }
    });
    if let Some(projection) = projection {
        let _details = egui::CollapsingHeader::new("Scan details").default_open(false).show(ui, |ui| {
            let _scroll = egui::ScrollArea::vertical().id_salt("folder-size-details").max_height(100.0).show(ui, |ui| {
                ui.label(format!("{} files • {} directories seen • {} enumerated", projection.counts.files, projection.counts.directories, projection.counts.scanned));
                if let Some(admitted) = projection.counts.directories_admitted { ui.label(format!("{admitted} directories admitted")); }
                ui.label(format!("Skipped: {} reparse, {} cloud, {} inaccessible", projection.counts.reparse, projection.counts.cloud, projection.counts.inaccessible));
                if let Some(status) = projection.status {
                    ui.label(match status {
                        FolderSizeStatus::Complete => "Result complete.",
                        FolderSizeStatus::Partial => "Partial result: incomplete branches are marked below.",
                        FolderSizeStatus::Cancelled => "Cancelled: showing verified entries only.",
                        FolderSizeStatus::Failed => "Failed: verified subtotal is retained.",
                    });
                }
                if projection.counts.issues_truncated { ui.label("Some issue details were omitted by the scanner."); }
                for (path, kind, message) in &projection.issues {
                    ui.small(match path { Some(path) => format!("{}: {message}", path.display()), None => format!("{}: {message}", issue_label(*kind)) });
                }
            });
            #[cfg(test)] test_action_rects.push(("details-body".into(), _scroll.inner_rect, ui.clip_rect()));
        });
        #[cfg(test)] test_action_rects.push(("details".into(), _details.header_response.rect, ui.clip_rect()));
    }
    if let Some(error) = view.error() {
        ui.colored_label(ui.visuals().error_fg_color, error.to_string());
    }
}

fn treemap_view_size(ui: &egui::Ui) -> egui::Vec2 {
    let available = ui.available_size();
    let visible = ui.clip_rect().max - ui.cursor().min;
    egui::vec2(
        available.x.min(visible.x).max(1.0),
        (available.y.min(visible.y) - 30.0).clamp(1.0, 350.0),
    )
}

fn show_treemap(
    ui: &mut egui::Ui,
    projection: &FolderSizeProjection,
    layout: &crate::folder_size::treemap::Layout,
    size: egui::Vec2,
    selected: Option<(u64, usize)>,
    directory: Option<(u64, usize)>,
    filter: KindFilter,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    previous_press: Option<TreemapPress>,
    owned_popup: Option<egui::Id>,
    input_allowed: bool,
    menu_allowed: bool,
    actions: &mut Vec<FolderSizeAction>,
    #[cfg(test)] rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    #[cfg(test)]
    rects.push(("treemap-area".into(), area, ui.clip_rect()));
    let painter = ui.painter().with_clip_rect(area.intersect(ui.clip_rect()));
    let mut tiny = 0usize;
    let mut unlabelled = 0usize;
    let mut press_still_current = previous_press.is_none();
    let mut fresh_press = false;
    for tile in &layout.tiles {
        let r = tile.rect;
        let rect = egui::Rect::from_min_size(
            area.min + egui::vec2(r.x as f32, r.y as f32),
            egui::vec2(r.width as f32, r.height as f32),
        )
        .intersect(area);
        if rect.width() < 2.0 || rect.height() < 2.0 {
            tiny += 1;
            continue;
        }
        if rect.width() < 80.0 || rect.height() < 32.0 {
            unlabelled += 1;
        }
        let Some(row) = projection.row(tile.row) else {
            continue;
        };
        let is_selected = selected == Some((projection.key.generation, tile.row));
        let response = ui.interact(
            rect,
            ui.id()
                .with(("treemap", projection.key.generation, tile.row)),
            egui::Sense::click(),
        );
        painter.rect_filled(
            rect,
            0.0,
            if is_selected {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.inactive.bg_fill
            },
        );
        painter.rect_stroke(
            rect,
            0.0,
            if is_selected {
                ui.visuals().selection.stroke
            } else {
                ui.visuals().widgets.inactive.bg_stroke
            },
            egui::StrokeKind::Inside,
        );
        let share = tile.bytes as f64 * 100.0 / layout.total as f64;
        let text = format!(
            "{}{}\n{} · {:.1}%",
            if is_selected { "✓ " } else { "" },
            row.name,
            format_bytes(tile.bytes),
            share
        );
        painter.with_clip_rect(rect.intersect(area)).text(
            rect.min + egui::vec2(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::TextStyle::Small.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        #[cfg(test)]
        rects.push((
            format!("tile/{}", row.name),
            rect,
            area.intersect(ui.clip_rect()),
        ));
        let response = response.on_hover_ui(|ui| {
            let path = projection
                .source
                .path_at(tile.row)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| row.name.clone());
            ui.label(format!(
                "{}\n{} · {} · {}{}",
                path,
                if row.kind == FolderSizeEntryKind::Directory {
                    "Folder"
                } else {
                    "File"
                },
                row_status(row),
                format_bytes(tile.bytes),
                if tile.provisional {
                    " (provisional subtotal)"
                } else {
                    ""
                }
            ));
        });
        let press = TreemapPress {
            projection: projection.key,
            directory,
            filter,
            rect,
            area,
            row: tile.row,
            host,
            epoch,
        };
        if previous_press == Some(press) {
            press_still_current = true;
        }
        let primary_pressed = ui.input(|input| input.events.iter().any(|event| matches!(event,
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. }
                if rect.contains(*pos) && ui.clip_rect().contains(*pos))));
        if input_allowed
            && primary_pressed
            && (response.is_pointer_button_down_on() || response.clicked())
        {
            actions.push(FolderSizeAction::TreemapPress(press));
            fresh_press = true;
        }
        folder_row_context(
            &response,
            row,
            projection.key.generation,
            tile.row,
            owned_popup,
            input_allowed,
            menu_allowed,
            actions,
            #[cfg(test)]
            rects,
        );
        if input_allowed && response.clicked() {
            actions.push(FolderSizeAction::TreemapRelease(
                press,
                response.double_clicked(),
            ));
        }
    }
    if input_allowed && !press_still_current && !fresh_press {
        actions.push(FolderSizeAction::CancelTreemapPress);
    }
    if layout.tiles.is_empty() && layout.zero_count == 0 {
        let _empty = ui.weak("No retained measured children here. For an unscanned folder, enable Include subfolders and Refresh, or choose this folder as the scan root.");
        #[cfg(test)]
        rects.push(("treemap-empty".into(), _empty.rect, ui.clip_rect()));
    }
    ui.weak(format!("Displayed measured total: {} bytes · {} tiny · {} with clipped labels · {} zero · {} unknown · {} skipped · {} provisional", layout.total, tiny, unlabelled, layout.zero_count, layout.unknown_count, layout.omitted_count, layout.provisional_count));
}
fn folder_row_context(
    response: &egui::Response,
    _row: &FolderSizeRow,
    generation: u64,
    index: usize,
    owned_popup: Option<egui::Id>,
    commit_allowed: bool,
    menu_allowed: bool,
    actions: &mut Vec<FolderSizeAction>,
    #[cfg(test)] rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    if response.secondary_clicked() && commit_allowed {
        actions.push(FolderSizeAction::OpenRowMenu {
            generation,
            index,
            popup: egui::Popup::default_response_id(response),
        });
    }
    let popup = egui::Popup::default_response_id(response);
    if response.context_menu_opened() && owned_popup != Some(popup) && !response.secondary_clicked()
    {
        egui::Popup::close_id(&response.ctx, popup);
    }
    response.context_menu(|ui| {
        let copy = ui.button("Copy path");
        #[cfg(test)]
        rects.push((format!("copy-row/{}", _row.name), copy.rect, ui.clip_rect()));
        if copy.clicked() && menu_allowed {
            actions.push(FolderSizeAction::CopyRow { generation, index });
            ui.close();
        }
        let reveal = ui.button("Reveal in File Explorer");
        #[cfg(test)]
        rects.push((format!("reveal/{}", _row.name), reveal.rect, ui.clip_rect()));
        if reveal.clicked() && menu_allowed {
            actions.push(FolderSizeAction::RevealRow { generation, index });
            ui.close();
        }
    });
}

fn show_folder_rows(
    ui: &mut egui::Ui,
    projection: &FolderSizeProjection,
    selected_root: Option<&ExplicitDirectoryRoot>,
    current_directory: Option<(u64, usize)>,
    current_directory_path: Option<&Path>,
    selected_entry: Option<(u64, usize)>,
    child_indices: &[usize],
    sort_column: SortColumn,
    sort_direction: SortDirection,
    filter: KindFilter,
    owned_popup: Option<egui::Id>,
    commit_allowed: bool,
    menu_allowed: bool,
    show_table: bool,
    staged_actions: &mut Vec<FolderSizeAction>,
    #[cfg(test)] test_action_rects: &mut Vec<(String, egui::Rect, egui::Rect)>,
) {
    let generation = projection.key.generation;
    let current_index = current_directory
        .filter(|(current_generation, _)| *current_generation == generation)
        .map(|(_, index)| index)
        .or(projection.root_index);
    let current_path =
        current_directory_path.or_else(|| selected_root.map(ExplicitDirectoryRoot::as_path));
    ui.horizontal(|ui| {
        if let Some((generation, index)) = current_directory
            && generation == projection.key.generation
        {
            let up = ui.button("Up");
            #[cfg(test)] test_action_rects.push(("up".into(), up.rect, ui.clip_rect()));
            if up.clicked() && commit_allowed { staged_actions.push(FolderSizeAction::Back { generation, index }); }
        }
        if let Some(path) = current_path {
            let full_path = path.display().to_string();
            let path_width = ui.available_width();
            ui.add_sized(
                egui::vec2(path_width, ROW_HEIGHT),
                egui::Label::new(full_path.clone()).truncate(),
            )
            .on_hover_text(full_path);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Show:");
        for (label, value) in [("All", KindFilter::All), ("Files", KindFilter::Files), ("Folders", KindFilter::Folders)] {
            let response = ui.selectable_label(filter == value, label);
            #[cfg(test)] test_action_rects.push((format!("filter/{label}"), response.rect, ui.clip_rect()));
            if response.clicked() && commit_allowed { staged_actions.push(FolderSizeAction::SetFilter(value)); }
        }
    });
    if !show_table { return; }
    ui.separator();
    ui.horizontal(|ui| {
        let name_width = row_name_width(ui.available_width());
        for (label, column, width) in [
            ("Name", SortColumn::Name, name_width), ("Type", SortColumn::Type, ROW_TYPE_WIDTH),
            ("Logical size", SortColumn::Size, ROW_SIZE_WIDTH), ("Share", SortColumn::Share, ROW_SHARE_WIDTH),
            ("Status", SortColumn::Status, ROW_STATUS_WIDTH),
        ] {
            let arrow = if sort_column != column { "" } else if sort_direction == SortDirection::Ascending { " ↑" } else { " ↓" };
            let response = ui.add_sized(egui::vec2(width, ROW_HEIGHT), egui::Button::new(format!("{label}{arrow}")));
            #[cfg(test)] test_action_rects.push((format!("sort/{label}"), response.rect, ui.clip_rect()));
            if response.clicked() && commit_allowed { staged_actions.push(FolderSizeAction::SetSort(column)); }
            ui.add_space(ROW_COLUMN_GAP);
        }
    });
    if child_indices.is_empty() {
        ui.weak(if filter == KindFilter::All { "No measured entries in this folder yet." } else { "No entries match this filter." });
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(("folder-size-rows", generation, current_index))
        .auto_shrink([false, false])
        .show_rows(ui, ROW_HEIGHT, child_indices.len(), |ui, visible| {
            for position in visible {
                let index = child_indices[position];
                let Some(row) = projection.row(index) else {
                    continue;
                };
                let selected = selected_entry == Some((generation, index));
                ui.push_id((generation, index), |ui| {
                    ui.horizontal(|ui| {
                        let name_width = row_name_width(ui.available_width());
                        let response = ui
                            .add_sized(
                                egui::vec2(name_width, ROW_HEIGHT),
                                egui::Button::selectable(selected, row.name.as_str()).truncate(),
                            )
                            .on_hover_text(&row.name);
                        #[cfg(test)]
                        test_action_rects.push((
                            format!("row/{}", row.name),
                            response.rect,
                            ui.clip_rect(),
                        ));
                        if response.clicked() && commit_allowed {
                            staged_actions.push(FolderSizeAction::Select { generation, index });
                        }
                        folder_row_context(&response, row, generation, index, owned_popup,
                            commit_allowed, menu_allowed, staged_actions,
                            #[cfg(test)] test_action_rects);
                        if response.double_clicked()
                            && commit_allowed
                            && row.kind == FolderSizeEntryKind::Directory
                        {
                            staged_actions.push(FolderSizeAction::Drill { generation, index });
                        }
                        ui.add_space(ROW_COLUMN_GAP);
                        ui.add_sized(egui::vec2(ROW_TYPE_WIDTH, ROW_HEIGHT), egui::Label::new(match row.kind {
                            FolderSizeEntryKind::File => "File", FolderSizeEntryKind::Directory => "Folder", FolderSizeEntryKind::Omitted(_) => "Skipped",
                        }).truncate());
                        ui.add_space(ROW_COLUMN_GAP);
                        let (row_bytes, share) =
                            row_measurement(row, projection.total_logical_bytes);
                        ui.add_sized(
                            egui::vec2(ROW_SIZE_WIDTH, ROW_HEIGHT),
                            egui::Label::new(
                                row_bytes.map(format_bytes).unwrap_or_else(|| "—".into()),
                            )
                            .truncate(),
                        );
                        ui.add_space(ROW_COLUMN_GAP);
                        ui.add_sized(
                            egui::vec2(ROW_SHARE_WIDTH, ROW_HEIGHT),
                            egui::Label::new(
                                share
                                    .map(|percent| format!("{percent:.1}%"))
                                    .unwrap_or_else(|| "—".into()),
                            )
                            .truncate(),
                        );
                        ui.add_space(ROW_COLUMN_GAP);
                        let status = row_status(row);
                        ui.add_sized(
                            egui::vec2(ROW_STATUS_WIDTH, ROW_HEIGHT),
                            egui::Label::new(status.clone()).truncate(),
                        )
                        .on_hover_text(status);
                    });
                });
            }
        });
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut amount = bytes as f64;
    let mut unit = 0usize;
    while amount >= 1024.0 && unit + 1 < UNITS.len() {
        amount /= 1024.0;
        unit += 1;
    }
    format!("{amount:.1} {}", UNITS[unit])
}

fn row_status(row: &FolderSizeRow) -> String {
    match row.kind {
        FolderSizeEntryKind::Directory if row.complete => "Complete".into(),
        FolderSizeEntryKind::Directory => "Incomplete".into(),
        FolderSizeEntryKind::File => "Measured".into(),
        FolderSizeEntryKind::Omitted(reason) => format!("Skipped: {}", omission_label(reason)),
    }
}

fn omission_label(omission: FolderSizeOmission) -> &'static str {
    match omission {
        FolderSizeOmission::ReparsePoint => "reparse point",
        FolderSizeOmission::CloudPlaceholder => "cloud placeholder",
        FolderSizeOmission::Inaccessible => "inaccessible",
        FolderSizeOmission::UnsupportedEntry => "unsupported entry",
        FolderSizeOmission::UnsupportedIdentity => "unsupported identity",
        FolderSizeOmission::RepeatedDirectoryIdentity => "repeated directory identity",
    }
}

fn issue_label(kind: FolderSizeIssueKind) -> &'static str {
    match kind {
        FolderSizeIssueKind::Inaccessible => "Inaccessible",
        FolderSizeIssueKind::UnsupportedIdentity => "Unsupported identity",
        FolderSizeIssueKind::ReparsePointExcluded => "Reparse point excluded",
        FolderSizeIssueKind::RootChanged => "Root changed",
        FolderSizeIssueKind::ParentChanged => "Parent changed",
        FolderSizeIssueKind::PathEscapedRoot => "Path escaped root",
        FolderSizeIssueKind::InvalidEntry => "Invalid entry",
        FolderSizeIssueKind::ResourceLimit => "Resource limit",
        FolderSizeIssueKind::AccountingOverflow => "Accounting overflow",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        name: &str,
        parent_index: Option<usize>,
        kind: FolderSizeEntryKind,
        logical_bytes: Option<u64>,
        subtree_logical_bytes: u64,
    ) -> FolderSizeRow {
        FolderSizeRow {
            parent_index,
            name: name.to_owned(),
            sort_key: name.to_lowercase(),
            kind,
            logical_bytes,
            subtree_logical_bytes,
            complete: true,
        }
    }

    #[test]
    fn folder_size_treemap_cache_tracks_projection_directory_filter_and_size_without_scans() {
        let root = crate::folder_size::disposable_scan_fixture_root();
        let (session, control) = FolderSizeSession::with_held_test_scan(|| {});
        session
            .refresh(
                ExplicitDirectoryRoot::new(root.path().to_path_buf()).unwrap(),
                true,
            )
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while control.started().is_empty() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(control.started().len(), 1);
        let mut rows = vec![
            row("root", None, FolderSizeEntryKind::Directory, None, 30),
            row("folder", Some(0), FolderSizeEntryKind::Directory, None, 20),
            row("direct", Some(0), FolderSizeEntryKind::File, Some(10), 0),
            row("nested", Some(1), FolderSizeEntryKind::File, Some(20), 0),
            row("unknown", Some(0), FolderSizeEntryKind::File, None, 0),
        ];
        rows[1].complete = false;
        let children = vec![vec![1, 2, 4], vec![3], vec![], vec![], vec![]];
        let mut cache = TreemapCache::default();
        let key = ProjectionKey {
            generation: 1,
            version: ProjectionVersion::Progress(1),
        };
        let first = cache
            .layout(key, &rows, &children, 0, KindFilter::All, 300.0, 200.0)
            .unwrap();
        assert_eq!(
            (first.total, first.unknown_count, first.provisional_count),
            (30, 1, 1)
        );
        assert_eq!(
            first.tiles.iter().map(|tile| tile.row).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let same = cache
            .layout(key, &rows, &children, 0, KindFilter::All, 300.0, 200.0)
            .unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        let newer = ProjectionKey {
            version: ProjectionVersion::Progress(2),
            ..key
        };
        let changed = cache
            .layout(newer, &rows, &children, 0, KindFilter::All, 300.0, 200.0)
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        let result = ProjectionKey {
            version: ProjectionVersion::Result,
            ..key
        };
        let complete = cache
            .layout(result, &rows, &children, 0, KindFilter::All, 300.0, 200.0)
            .unwrap();
        assert!(!Arc::ptr_eq(&changed, &complete));
        let files = cache
            .layout(result, &rows, &children, 0, KindFilter::Files, 300.0, 200.0)
            .unwrap();
        assert_eq!(files.total, 10);
        let nested = cache
            .layout(result, &rows, &children, 1, KindFilter::All, 300.0, 200.0)
            .unwrap();
        assert_eq!(nested.total, 20);
        let resized = cache
            .layout(result, &rows, &children, 1, KindFilter::All, 200.0, 200.0)
            .unwrap();
        assert!(!Arc::ptr_eq(&nested, &resized));
        assert_eq!(control.started().len(), 1);
        control.release();
        session.close();
    }

    #[test]
    fn folder_size_dialog_projection_sorts_drills_and_rejects_stale_generation_selection() {
        let rows = vec![
            row("root", None, FolderSizeEntryKind::Directory, None, 16),
            row("zeta.bin", Some(0), FolderSizeEntryKind::File, Some(5), 0),
            row("Alpha", Some(0), FolderSizeEntryKind::Directory, None, 11),
            row(
                "nested.bin",
                Some(2),
                FolderSizeEntryKind::File,
                Some(11),
                0,
            ),
            row("beta.bin", Some(0), FolderSizeEntryKind::File, Some(5), 0),
            row(
                "unknown.bin",
                Some(0),
                FolderSizeEntryKind::Omitted(FolderSizeOmission::Inaccessible),
                None,
                0,
            ),
        ];
        let children = child_indices(&rows);
        let mut visible = children[0].clone();
        visible.sort_by(|a, b| compare_rows(&rows, *a, *b, SortColumn::Size, SortDirection::Descending, 16));
        assert_eq!(visible, [2, 4, 1, 5]);
        assert_eq!(children[2], [3]);
        for column in [SortColumn::Name, SortColumn::Type, SortColumn::Size, SortColumn::Share, SortColumn::Status] {
            for direction in [SortDirection::Ascending, SortDirection::Descending] {
                let mut sorted = children[0].clone();
                sorted.sort_by(|a, b| compare_rows(&rows, *a, *b, column, direction, 16));
                assert_eq!(sorted.len(), 4);
                if matches!(column, SortColumn::Size | SortColumn::Share) { assert_eq!(sorted.last(), Some(&5)); }
                for pair in sorted.windows(2) {
                    assert_ne!(compare_rows(&rows, pair[0], pair[1], column, direction, 16), std::cmp::Ordering::Greater);
                }
            }
        }
        let mut zero_share = children[0].clone();
        zero_share.sort_by(|a, b| compare_rows(&rows, *a, *b, SortColumn::Share, SortDirection::Descending, 0));
        assert_eq!(zero_share, [2, 4, 5, 1]);
        assert!(filter_matches(KindFilter::All, rows[5].kind));
        assert!(!filter_matches(KindFilter::Files, rows[5].kind));
        assert!(!filter_matches(KindFilter::Folders, rows[5].kind));
        assert!(filter_matches(KindFilter::Files, rows[1].kind));
        assert!(filter_matches(KindFilter::Folders, rows[2].kind));

        let current = ProjectionKey {
            generation: 12,
            version: ProjectionVersion::Progress(3),
        };
        assert!(projection_contains_row(current, rows.len(), 12, 3));
        assert!(!projection_contains_row(current, rows.len(), 11, 3));
        assert!(!projection_contains_row(
            current,
            rows.len(),
            12,
            rows.len()
        ));
    }

    #[test]
    fn folder_size_dialog_reveal_root_route_is_explicit() {
        assert_eq!(reveal_route(Path::new(std::path::MAIN_SEPARATOR_STR)), FolderSizeRevealRoute::OpenRoot);
        assert_eq!(reveal_route(Path::new("folder").join("file").as_path()), FolderSizeRevealRoute::RevealEntry);
    }

    #[test]
    fn folder_size_dialog_zero_total_has_no_share_and_cancel_status_is_frozen() {
        let file = row(
            "payload.bin",
            Some(0),
            FolderSizeEntryKind::File,
            Some(9),
            0,
        );
        assert_eq!(row_measurement(&file, 0), (Some(9), None));
        assert_eq!(row_measurement(&file, 18), (Some(9), Some(50.0)));

        assert_eq!(
            status_label(FolderSizeSessionStatus::Cancelled),
            "Cancelled — last verified subtotal is frozen."
        );
    }

    #[test]
    fn folder_size_retained_close_suppresses_same_frame_prefill_focus() {
        let ctx = egui::Context::default();
        let mut state = FolderSizeDialogState::new(&ctx);
        state.open();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let pass = ViewportPassProvenance::current(ui.ctx(), egui::ViewportId::ROOT);
            state.stage_file_search_prefill(pass, PathBuf::from("unused-prefill"));
            state.stage_with_provenance(pass, vec![FolderSizeAction::Close]);
        });
        let effects = state.on_ui_frame_complete(&ctx);
        output.drop_without_applying_deltas();
        assert!(!state.open);
        assert!(!effects.contains(&FolderSizeDialogEffect::FocusPanel));
        assert!(state.prefill_pending.is_none());
    }

    #[test]
    fn folder_size_dialog_close_wins_same_pass_arbitration() {
        let ctx = egui::Context::default();
        let mut state = FolderSizeDialogState::new(&ctx);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let provenance = ViewportPassProvenance::current(ui.ctx(), egui::ViewportId::ROOT);
            state.stage_with_provenance(provenance, vec![FolderSizeAction::Close]);
            state.stage_with_provenance(provenance, vec![FolderSizeAction::ChooseFolder]);
            let pending = state.pending.as_ref().expect("close intent remains staged");
            assert_eq!(pending.actions.len(), 1);
            assert!(matches!(
                pending.actions.first(),
                Some(FolderSizeAction::Close)
            ));
        });
        output.drop_without_applying_deltas();
    }
}
