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
const MAX_VISIBLE_ISSUES: usize = 8;
const ROW_SIZE_WIDTH: f32 = 86.0;
const ROW_SHARE_WIDTH: f32 = 64.0;
const ROW_STATUS_WIDTH: f32 = 166.0;
const ROW_COLUMN_GAP: f32 = 10.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FolderSizeDialogEffect {
    CopyPath(String),
    OpenPath(PathBuf),
    FocusPanel,
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
                .take(MAX_VISIBLE_ISSUES)
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
        self.children = sorted_child_indices(&self.rows);
    }

    fn row(&self, index: usize) -> Option<&FolderSizeRow> {
        self.rows.get(index)
    }

    fn contains_row(&self, generation: u64, index: usize) -> bool {
        projection_contains_row(self.key, self.rows.len(), generation, index)
    }
}

fn sorted_child_indices(rows: &[FolderSizeRow]) -> Vec<Vec<usize>> {
    let mut children = vec![Vec::new(); rows.len()];
    for (child_index, row) in rows.iter().enumerate() {
        if let Some(parent_index) = row.parent_index
            && let Some(parent_children) = children.get_mut(parent_index)
        {
            parent_children.push(child_index);
        }
    }
    for child_indices in &mut children {
        child_indices.sort_by(|left, right| {
            row_sort_measurement(&rows[*right])
                .cmp(&row_sort_measurement(&rows[*left]))
                .then_with(|| rows[*left].sort_key.cmp(&rows[*right].sort_key))
                .then_with(|| rows[*left].name.cmp(&rows[*right].name))
                .then_with(|| left.cmp(right))
        });
    }
    children
}

fn row_sort_measurement(row: &FolderSizeRow) -> Option<u64> {
    match row.kind {
        FolderSizeEntryKind::Directory => Some(row.subtree_logical_bytes),
        FolderSizeEntryKind::File => row.logical_bytes,
        FolderSizeEntryKind::Omitted(_) => None,
    }
}

fn visible_child_indices(
    children: &[Vec<usize>],
    current_index: Option<usize>,
    root_index: Option<usize>,
) -> &[usize] {
    current_index
        .or(root_index)
        .and_then(|index| children.get(index))
        .map(Vec::as_slice)
        .unwrap_or(&[])
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
    let reserved = ROW_SIZE_WIDTH + ROW_SHARE_WIDTH + ROW_STATUS_WIDTH + ROW_COLUMN_GAP * 3.0;
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
        .unwrap_or_else(|| path.as_os_str())
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

#[derive(Clone)]
enum FolderSizeAction {
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
}

pub(crate) struct FolderSizeDialogState {
    pub(crate) open: bool,
    session: FolderSizeSession,
    selected_root: Option<ExplicitDirectoryRoot>,
    scan_root: Option<ExplicitDirectoryRoot>,
    recursive: bool,
    current_directory: Option<(u64, usize)>,
    current_directory_path: Option<PathBuf>,
    selected_entry: Option<(u64, usize)>,
    projection: Option<Arc<FolderSizeProjection>>,
    pending: Option<PendingFolderSizePass>,
    notice: Option<String>,
    #[cfg(test)]
    test_folder_choice: Option<Option<PathBuf>>,
    #[cfg(test)]
    test_folder_picker_calls: usize,
    #[cfg(test)]
    test_action_rects: Vec<(String, egui::Rect, egui::Rect)>,
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
            session,
            selected_root: None,
            scan_root: None,
            recursive: true,
            current_directory: None,
            current_directory_path: None,
            selected_entry: None,
            projection: None,
            pending: None,
            notice: None,
            #[cfg(test)]
            test_folder_choice: None,
            #[cfg(test)]
            test_folder_picker_calls: 0,
            #[cfg(test)]
            test_action_rects: Vec::new(),
        }
    }

    /// Opening or focusing Folder Size never starts a scan.
    pub(crate) fn open(&mut self) {
        self.open = true;
    }

    pub(crate) fn request_close(&mut self, ctx: &egui::Context) {
        if self.open {
            self.stage(ctx, vec![FolderSizeAction::Close]);
        }
    }

    pub(crate) fn stage_prefill(&mut self, ctx: &egui::Context, path: PathBuf) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.stage(ctx, vec![FolderSizeAction::Prefill(path)]);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.open = false;
        self.pending = None;
        self.session.close();
    }

    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.pending.is_some() || self.session.view().is_busy()
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        root_visible: bool,
        input_allowed: bool,
        first_ui_pass: bool,
    ) {
        let view = self.session.view();
        self.sync_projection(&view);
        if !self.open {
            return;
        }
        #[cfg(test)]
        self.test_action_rects.clear();

        let pass = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let commit_allowed = input_allowed
            && root_visible
            && first_ui_pass
            && ctx.viewport_id() == egui::ViewportId::ROOT
            && !egui::Popup::is_any_open(ctx);
        let mut window_open = self.open;
        let projection = self.projection.clone();
        let selected_root = self.selected_root.clone();
        let scan_root = self.scan_root.clone();
        let selected_entry = self.selected_entry;
        let current_directory = self.current_directory;
        let current_directory_path = self.current_directory_path.clone();
        let mut recursive = self.recursive;
        let original_recursive = recursive;
        let notice = self.notice.clone();
        let mut staged_actions = Vec::new();

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
                |ui| {
                    let controls_allowed = commit_allowed && ui.is_visible() && !ui.is_sizing_pass();
                    ui.add_enabled_ui(controls_allowed, |ui| {
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
                            self.test_action_rects.push((
                                "scan".into(),
                                scan.rect,
                                ui.clip_rect(),
                            ));
                            if scan.clicked()
                                && let Some(root) = selected_root.clone()
                            {
                                staged_actions.push(FolderSizeAction::Scan {
                                    root,
                                    session_generation: view.generation(),
                                    recursive,
                                });
                            }
                            let refresh =
                                ui.add_enabled(scan_enabled, egui::Button::new("Refresh"));
                            #[cfg(test)]
                            self.test_action_rects.push((
                                "refresh".into(),
                                refresh.rect,
                                ui.clip_rect(),
                            ));
                            if refresh.clicked()
                                && let Some(root) = selected_root.clone()
                            {
                                staged_actions.push(FolderSizeAction::Refresh {
                                    root,
                                    session_generation: view.generation(),
                                    recursive,
                                });
                            }
                            let cancel =
                                ui.add_enabled(view.is_busy(), egui::Button::new("Cancel"));
                            #[cfg(test)]
                            self.test_action_rects.push((
                                "cancel".into(),
                                cancel.rect,
                                ui.clip_rect(),
                            ));
                            if cancel.clicked() {
                                staged_actions.push(FolderSizeAction::Cancel {
                                    session_generation: view.generation(),
                                });
                            }

                            let selected_target = selected_entry.filter(|(generation, index)| {
                                projection.as_ref().is_some_and(|projection| {
                                    projection.contains_row(*generation, *index)
                                })
                            });
                            let can_copy_or_open = selected_target.is_some() || selected_root.is_some();
                            if ui
                                .add_enabled(can_copy_or_open, egui::Button::new("Copy Path"))
                                .clicked()
                            {
                                if let Some((generation, index)) = selected_target {
                                    staged_actions
                                        .push(FolderSizeAction::CopyRow { generation, index });
                                } else if let Some(root) = selected_root.clone() {
                                    staged_actions.push(FolderSizeAction::CopyRoot(root));
                                }
                            }
                            if ui
                                .add_enabled(can_copy_or_open, egui::Button::new("Reveal/Open"))
                                .clicked()
                            {
                                if let Some((generation, index)) = selected_target {
                                    staged_actions
                                        .push(FolderSizeAction::OpenRow { generation, index });
                                } else if let Some(root) = selected_root.clone() {
                                    staged_actions.push(FolderSizeAction::OpenRoot(root));
                                }
                            }
                            if ui.checkbox(&mut recursive, "Include subfolders").changed() {
                                staged_actions.push(FolderSizeAction::SetRecursive(recursive));
                            }
                        });

                        if recursive != original_recursive {
                            if let Some(FolderSizeAction::Scan {
                                recursive: scan_recursive,
                                ..
                            }) =
                                staged_actions.iter_mut().find(|action| {
                                    matches!(action, FolderSizeAction::Scan { .. })
                                })
                            {
                                *scan_recursive = recursive;
                            }
                            if let Some(FolderSizeAction::Refresh {
                                recursive: refresh_recursive,
                                ..
                            }) =
                                staged_actions.iter_mut().find(|action| {
                                    matches!(action, FolderSizeAction::Refresh { .. })
                                })
                            {
                                *refresh_recursive = recursive;
                            }
                        }

                        if let Some(root) = selected_root.as_ref() {
                            ui.label(format!("Root: {}", root.as_path().display()));
                        } else {
                            ui.label("Choose a folder to begin. Opening Folder Size does not start a scan.");
                        }
                        if let Some(message) = notice.as_deref() {
                            ui.colored_label(ui.visuals().warn_fg_color, message);
                        }

                        let scan_matches_selection = scan_root.is_some()
                            && scan_root == selected_root;
                        if scan_matches_selection {
                            show_scan_status(ui, &view, projection.as_deref());
                        }
                        if let Some(projection) = projection.as_deref() {
                            show_folder_rows(
                                ui,
                                projection,
                                selected_root.as_ref(),
                                current_directory,
                                current_directory_path.as_deref(),
                                selected_entry,
                                commit_allowed,
                                &mut staged_actions,
                            );
                        } else if selected_root.is_some() {
                            ui.label("No scan results yet. Choose Scan when you are ready.");
                        }
                    });
                },
            );

        if !window_open && commit_allowed {
            staged_actions.push(FolderSizeAction::Close);
        }
        self.stage_with_provenance(pass, staged_actions);
    }

    pub(crate) fn on_ui_frame_complete(
        &mut self,
        ctx: &egui::Context,
    ) -> Vec<FolderSizeDialogEffect> {
        let view = self.session.view();
        self.sync_projection(&view);
        let Some(pending) = self.pending.take() else {
            return Vec::new();
        };
        if !pending.provenance.was_retained(ctx) {
            if ctx.cumulative_frame_nr_for(pending.provenance.viewport_id)
                > pending.provenance.frame_nr
            {
                // The originating input frame completed without retaining this
                // pass. A discarded click has no authority in the next frame.
                self.pending = None;
            } else {
                self.pending = Some(pending);
            }
            return Vec::new();
        }

        let mut effects = Vec::new();
        let had_actions = !pending.actions.is_empty();
        for action in pending.actions {
            let current = self.session.view();
            self.sync_projection(&current);
            match action {
                FolderSizeAction::ChooseFolder => {
                    if !self.revalidate_root_pass(ctx, pending.provenance) {
                        continue;
                    }
                    if let Some(path) = self.pick_folder() {
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
                FolderSizeAction::Select { generation, index } => {
                    if self
                        .projection
                        .as_ref()
                        .is_some_and(|projection| projection.contains_row(generation, index))
                    {
                        self.selected_entry = Some((generation, index));
                    }
                }
                FolderSizeAction::Drill { generation, index } => {
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
                    if current.is_busy() {
                        let _ = self.session.cancel();
                    }
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
            Ok(()) => "Path copied to the clipboard.".into(),
            Err(error) => error,
        });
    }

    pub(crate) fn path_action_completed(&mut self, result: Result<(), String>) {
        self.notice = Some(match result {
            Ok(()) => "Path opened in File Explorer.".into(),
            Err(error) => error,
        });
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
                    actions,
                });
            }
        }
    }

    fn revalidate_root_pass(
        &self,
        ctx: &egui::Context,
        provenance: ViewportPassProvenance,
    ) -> bool {
        provenance.viewport_id == egui::ViewportId::ROOT && provenance.was_retained(ctx)
    }

    fn pick_folder(&mut self) -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(choice) = self.test_folder_choice.take() {
            self.test_folder_picker_calls = self.test_folder_picker_calls.saturating_add(1);
            return choice;
        }

        #[cfg(not(test))]
        {
            rfd::FileDialog::new().pick_folder()
        }
        #[cfg(test)]
        {
            None
        }
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
    pub(crate) fn selected_root_for_test(&self) -> Option<&ExplicitDirectoryRoot> {
        self.selected_root.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn session_view_for_test(&self) -> crate::folder_size::FolderSizeSessionView {
        self.session.view()
    }
}

fn show_scan_status(
    ui: &mut egui::Ui,
    view: &crate::folder_size::FolderSizeSessionView,
    projection: Option<&FolderSizeProjection>,
) {
    ui.separator();
    ui.label(status_label(view.status()));
    if let Some(projection) = projection {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Subtotal {} • {} entries seen ({} files) • {} directories enumerated • {} child directories seen",
                format_bytes(projection.total_logical_bytes),
                projection.counts.entries,
                projection.counts.files,
                projection.counts.scanned,
                projection.counts.directories,
            ));
            if let Some(admitted) = projection.counts.directories_admitted {
                ui.label(format!("{admitted} directories admitted in total"));
            }
            if projection.counts.reparse > 0
                || projection.counts.cloud > 0
                || projection.counts.inaccessible > 0
            {
                ui.label(format!(
                    "Skipped: {} reparse, {} cloud, {} inaccessible",
                    projection.counts.reparse,
                    projection.counts.cloud,
                    projection.counts.inaccessible,
                ));
            }
            if projection.counts.issues > 0 {
                ui.label(format!(
                    "{} issue(s){}",
                    projection.counts.issues,
                    if projection.counts.issues_truncated { " (some details omitted)" } else { "" },
                ));
            }
        });
        if let Some(status) = projection.status {
            ui.label(match status {
                FolderSizeStatus::Complete => "Result: Complete",
                FolderSizeStatus::Partial => {
                    "Result: Partial; incomplete branches are marked below."
                }
                FolderSizeStatus::Cancelled => "Result: Cancelled; showing verified entries only.",
                FolderSizeStatus::Failed => "Result: Failed; verified subtotal is retained.",
            });
        }
        for (path, kind, message) in &projection.issues {
            ui.small(match path {
                Some(path) => format!("{}: {message}", path.display()),
                None => format!("{}: {message}", issue_label(*kind)),
            });
        }
    }
    if let Some(error) = view.error() {
        ui.colored_label(ui.visuals().error_fg_color, error.to_string());
    }
}

fn show_folder_rows(
    ui: &mut egui::Ui,
    projection: &FolderSizeProjection,
    selected_root: Option<&ExplicitDirectoryRoot>,
    current_directory: Option<(u64, usize)>,
    current_directory_path: Option<&Path>,
    selected_entry: Option<(u64, usize)>,
    commit_allowed: bool,
    staged_actions: &mut Vec<FolderSizeAction>,
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
            if ui.button("Up").clicked() && commit_allowed {
                staged_actions.push(FolderSizeAction::Back { generation, index });
            }
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
    ui.separator();
    ui.horizontal(|ui| {
        let name_width = row_name_width(ui.available_width());
        ui.add_sized(
            egui::vec2(name_width, ROW_HEIGHT),
            egui::Label::new("Name").truncate(),
        );
        ui.add_space(ROW_COLUMN_GAP);
        ui.add_sized(
            egui::vec2(ROW_SIZE_WIDTH, ROW_HEIGHT),
            egui::Label::new("Logical size").truncate(),
        );
        ui.add_space(ROW_COLUMN_GAP);
        ui.add_sized(
            egui::vec2(ROW_SHARE_WIDTH, ROW_HEIGHT),
            egui::Label::new("Share").truncate(),
        );
        ui.add_space(ROW_COLUMN_GAP);
        ui.add_sized(
            egui::vec2(ROW_STATUS_WIDTH, ROW_HEIGHT),
            egui::Label::new("Status").truncate(),
        );
    });

    let child_indices =
        visible_child_indices(&projection.children, current_index, projection.root_index);
    if child_indices.is_empty() {
        ui.weak("No measured entries in this folder yet.");
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
                        if response.clicked() && commit_allowed {
                            staged_actions.push(FolderSizeAction::Select { generation, index });
                        }
                        if response.double_clicked()
                            && commit_allowed
                            && row.kind == FolderSizeEntryKind::Directory
                        {
                            staged_actions.push(FolderSizeAction::Drill { generation, index });
                        }
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
        let children = sorted_child_indices(&rows);

        assert_eq!(
            visible_child_indices(&children, None, Some(0)),
            &[2, 4, 1, 5]
        );
        assert_eq!(visible_child_indices(&children, Some(2), Some(0)), &[3]);
        assert!(visible_child_indices(&children, Some(usize::MAX), Some(0)).is_empty());

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
