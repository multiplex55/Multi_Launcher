mod diagnostics;
mod filters;
mod keyboard;
mod results;
use crate::actions::clipboard;
use crate::file_search::actions::{
    ExplorerAction, InvocationTarget, containing_directory, copied_filename,
    execute_explorer_action, nested_search_root, open_configured_terminal_in_directory,
    open_in_configured_editor, open_path, resolve_explorer_action,
};
use crate::file_search::coordinator::{SearchCoordinator, event_id};
use crate::file_search::model::{
    ContentMatch, FileKind, FileSearchResultKey, PathIdentity, SearchBackend, SearchDiagnostic,
    SearchEvent, SearchId, SearchKind, SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::preview::PreviewRequest;
use crate::file_search::settings::{
    FileSearchContentSort, FileSearchFilenameSort, FileSearchSettings, FileSearchUiPreferences,
};
use crate::gui::file_search_preview_dialog::FileSearchPreviewDialogState;
use diagnostics::FileSearchDiagnostics;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(760.0, 560.0);
const ACTIVE_SEARCH_REPAINT_INTERVAL: Duration = Duration::from_millis(50);

pub const FILE_SEARCH_SEARCH_FIELD_ID_SOURCE: &str = "file_search_search_text";
pub const FILE_SEARCH_ROOT_FIELD_ID_SOURCE: &str = "file_search_root_directory";

impl From<FileSearchFilenameSort> for crate::file_search::sorting::FilenameSort {
    fn from(value: FileSearchFilenameSort) -> Self {
        match value {
            FileSearchFilenameSort::Relevance => Self::Relevance,
            FileSearchFilenameSort::FilenameAscending => Self::FilenameAscending,
            FileSearchFilenameSort::FilenameDescending => Self::FilenameDescending,
            FileSearchFilenameSort::FullPathAscending => Self::FullPathAscending,
            FileSearchFilenameSort::ModifiedNewest => Self::ModifiedNewest,
            FileSearchFilenameSort::ModifiedOldest => Self::ModifiedOldest,
            FileSearchFilenameSort::SizeLargest => Self::SizeLargest,
            FileSearchFilenameSort::SizeSmallest => Self::SizeSmallest,
        }
    }
}

impl From<FileSearchContentSort> for crate::file_search::sorting::ContentSort {
    fn from(value: FileSearchContentSort) -> Self {
        match value {
            FileSearchContentSort::DiscoveryOrder => Self::DiscoveryOrder,
            FileSearchContentSort::PathThenLine => Self::PathThenLine,
            FileSearchContentSort::MatchCountDescending => Self::MatchCountDescending,
            FileSearchContentSort::ModifiedNewest => Self::ModifiedNewest,
            FileSearchContentSort::FilenameRelevance => Self::FilenameRelevance,
            FileSearchContentSort::LineNumber => Self::LineNumber,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileSearchMode {
    Filename,
    Content,
}

impl From<FileSearchMode> for SearchKind {
    fn from(value: FileSearchMode) -> Self {
        match value {
            FileSearchMode::Filename => SearchKind::Filename,
            FileSearchMode::Content => SearchKind::Content,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSearchScopeMode {
    Global,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSearchEscapeAction {
    Cancel,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSearchRequestError {
    EmptySearchText,
    EmptyGlobalRoots,
    EmptyDirectoryRoots,
}

pub fn handle_escape_action(status: SearchStatus) -> FileSearchEscapeAction {
    if status == SearchStatus::Running {
        FileSearchEscapeAction::Cancel
    } else {
        FileSearchEscapeAction::Close
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileSearchResultRowId {
    pub search_id: SearchId,
    pub result_key: FileSearchResultKey,
    pub match_index: Option<usize>,
    pub path: PathBuf,
    pub line_number: Option<usize>,
    pub column: Option<usize>,
}

pub fn result_row_id_source(
    search_id: SearchId,
    row_id: &FileSearchResultRowId,
) -> (&'static str, SearchId, FileSearchResultRowId) {
    ("file_search_result_row", search_id, row_id.clone())
}

pub fn omitted_matches_id_source(
    search_id: SearchId,
    result_key: FileSearchResultKey,
    path: &std::path::Path,
) -> (
    &'static str,
    SearchId,
    FileSearchResultKey,
    PathBuf,
    &'static str,
) {
    (
        "file_search_result_row",
        search_id,
        result_key,
        path.to_path_buf(),
        "omitted_matches",
    )
}

pub fn results_scroll_id_source(mode: FileSearchMode) -> (&'static str, FileSearchMode) {
    ("file_search_results_scroll", mode)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSearchResultCounts {
    pub filename_rows: usize,
    pub content_matched_files: usize,
    pub content_displayed_match_rows: usize,
    pub content_truncated_displayed_matches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSearchRowPayload {
    ContentGroupHeader {
        path: PathBuf,
        header: String,
        total_matches: usize,
        truncated: bool,
        modified: Option<std::time::SystemTime>,
    },
    Filename {
        path: PathBuf,
        display_filename: String,
        parent_directory_display: String,
        kind: FileKind,
        size: Option<u64>,
        modified: Option<std::time::SystemTime>,
        rank: crate::file_search::model::FilenameRank,
        match_quality: crate::file_search::model::FilenameMatchQuality,
        filename_match_ranges: Vec<crate::file_search::model::TextMatchRange>,
        path_match_ranges: Vec<crate::file_search::model::TextMatchRange>,
    },
    Content {
        path: PathBuf,
        content_match: ContentMatch,
        content_file_truncated: bool,
        is_last_displayed_match_from_truncated_file: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchResultRow {
    pub id: FileSearchResultRowId,
    pub payload: FileSearchRowPayload,
}

pub fn dedup_search_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for path in paths {
        if seen.insert(crate::file_search::sorting::path_identity(&path)) {
            deduped.push(path);
        }
    }
    deduped
}

pub fn resolve_valid_roots(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dedup_search_paths(paths.into_iter().filter_map(|path| {
        let trimmed = path.to_string_lossy().trim().to_owned();
        if trimmed.is_empty() {
            return None;
        }
        let path = PathBuf::from(trimmed);
        path.canonicalize().ok().filter(|p| p.is_dir())
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedFileSearchResultPayload {
    Filename {
        path: PathBuf,
        kind: FileKind,
    },
    Content {
        path: PathBuf,
        content_match: ContentMatch,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFileSearchResult {
    pub row_id: FileSearchResultRowId,
    pub payload: SelectedFileSearchResultPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenSelectedFileSearchResultAction {
    Explorer(ExplorerAction),
    PreviewContent { request: PreviewRequest },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchContextMenuIntent {
    pub action: FileSearchContextMenuAction,
    pub path: PathBuf,
    pub content_match: Option<ContentMatch>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSearchContextMenuAction {
    OpenInConfiguredEditor,
    Preview,
    OpenFileOrDirectory,
    RevealInExplorer,
    OpenContainingDirectory,
    CopyFullPath,
    CopyFilename,
    CopyMatchingLine,
    OpenTerminal,
    NestedFilenameSearch,
    NestedContentSearch,
    ShowAllMatchesInThisFile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileSearchDialogState {
    pub open: bool,
    pub selected_mode: FileSearchMode,
    pub selected_scope: FileSearchScopeMode,
    pub search_text: String,
    pub custom_roots: Vec<String>,
    pub case_sensitive: bool,
    pub include_hidden: bool,
    pub active_search_id: Option<SearchId>,
    pub current_status: SearchStatus,
    pub backend: Option<SearchBackend>,
    pub results: Vec<SearchResult>,
    pub result_rows: Vec<FileSearchResultRow>,
    pub selected_result: Option<SelectedFileSearchResult>,
    pub warning_error_message: Option<String>,
    pub inaccessible_path_warnings: usize,
    pub diagnostics: FileSearchDiagnostics,
    pub preview_dialog: FileSearchPreviewDialogState,
    pub persisted_window_size: egui::Vec2,
    pub settings: FileSearchSettings,
    pub ui_preferences: FileSearchUiPreferences,
    pub ui_preferences_dirty: bool,
    pub request_search_focus: bool,
    pub request_immediate_repaint: bool,
    pub show_ripgrep_missing_prompt: bool,
    pub ripgrep_missing_prompt_dismissed: bool,
    pub excluded_directory_names_overridden: bool,
    pub last_submitted_request: Option<SearchRequest>,
    pub pending_sort_change: bool,
}

impl Default for FileSearchDialogState {
    fn default() -> Self {
        Self {
            open: false,
            selected_mode: FileSearchMode::Filename,
            selected_scope: FileSearchScopeMode::Global,
            search_text: String::new(),
            custom_roots: Vec::new(),
            case_sensitive: false,
            include_hidden: false,
            active_search_id: None,
            current_status: SearchStatus::Pending,
            backend: None,
            results: Vec::new(),
            result_rows: Vec::new(),
            selected_result: None,
            warning_error_message: None,
            inaccessible_path_warnings: 0,
            diagnostics: FileSearchDiagnostics::default(),
            preview_dialog: FileSearchPreviewDialogState::default(),
            persisted_window_size: DEFAULT_WINDOW_SIZE,
            settings: FileSearchSettings::default(),
            ui_preferences: FileSearchUiPreferences::default(),
            ui_preferences_dirty: false,
            request_search_focus: false,
            request_immediate_repaint: false,
            show_ripgrep_missing_prompt: false,
            ripgrep_missing_prompt_dismissed: false,
            excluded_directory_names_overridden: false,
            last_submitted_request: None,
            pending_sort_change: false,
        }
    }
}

fn validate_ripgrep_selection(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute = path
        .canonicalize()
        .map_err(|err| format!("Selected ripgrep executable is invalid: {err}"))?;
    crate::file_search::ripgrep::resolve_ripgrep_executable(&absolute)
        .map(|_| absolute)
        .map_err(|err| format!("Selected file is not a usable ripgrep executable: {err}"))
}

impl FileSearchDialogState {
    pub fn search_field_id() -> egui::Id {
        egui::Id::new(FILE_SEARCH_SEARCH_FIELD_ID_SOURCE)
    }

    pub fn root_field_id() -> egui::Id {
        egui::Id::new(FILE_SEARCH_ROOT_FIELD_ID_SOURCE)
    }

    pub fn open(&mut self) {
        self.open = true;
        self.request_search_focus = true;
    }

    pub fn open_with_mode(&mut self, mode: FileSearchMode) {
        self.selected_mode = mode;
        self.open();
    }

    pub fn open_and_start(
        &mut self,
        mode: FileSearchMode,
        root: Option<PathBuf>,
        text: String,
        coordinator: &mut SearchCoordinator,
    ) -> Option<SearchId> {
        self.selected_mode = mode;
        if let Some(root) = root {
            self.selected_scope = FileSearchScopeMode::Directory;
            self.custom_roots = vec![root.display().to_string()];
        }
        self.search_text = text;
        self.open();
        let id = self.start_search(coordinator);
        if id.is_some() {
            self.request_immediate_repaint = true;
        }
        id
    }

    pub fn consume_search_focus_request(&mut self) -> bool {
        if self.request_search_focus {
            self.request_search_focus = false;
            true
        } else {
            false
        }
    }

    pub fn requires_repaint_polling(&self) -> bool {
        self.current_status == SearchStatus::Running
    }

    pub fn consume_immediate_repaint_request(&mut self) -> bool {
        if self.request_immediate_repaint {
            self.request_immediate_repaint = false;
            true
        } else {
            false
        }
    }

    pub fn start_search(&mut self, coordinator: &mut SearchCoordinator) -> Option<SearchId> {
        let request = match self.build_search_request() {
            Ok(request) => request,
            Err(error) => {
                self.warning_error_message = Some(error.user_message().to_string());
                return None;
            }
        };
        self.clear_results_and_selection();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.diagnostics.clear();
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend_with_settings(
            &request,
            Some(&self.settings),
        ));
        let submitted_request = request.clone();
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
        self.last_submitted_request = Some(submitted_request);
        self.pending_sort_change = false;
        self.request_immediate_repaint = true;
        Some(id)
    }

    pub fn cancel_search(&mut self, coordinator: &mut SearchCoordinator) {
        if self.active_search_id.is_some() && self.current_status == SearchStatus::Running {
            coordinator.cancel_active();
            self.current_status = SearchStatus::Cancelled;
            self.request_immediate_repaint = true;
        }
    }

    pub fn handle_escape(&mut self, coordinator: &mut SearchCoordinator) -> FileSearchEscapeAction {
        let action = handle_escape_action(self.current_status);
        match action {
            FileSearchEscapeAction::Cancel => self.cancel_search(coordinator),
            FileSearchEscapeAction::Close => self.open = false,
        }
        action
    }

    pub fn drain_events(&mut self, coordinator: &mut SearchCoordinator) -> bool {
        let mut observed_terminal_event = false;
        for event in coordinator.drain_current_events() {
            observed_terminal_event |= self.apply_event(event);
        }
        observed_terminal_event
    }

    pub fn apply_event(&mut self, event: SearchEvent) -> bool {
        if Some(event_id(&event)) != self.active_search_id {
            return false;
        }
        let observed_terminal_event =
            match event {
                SearchEvent::Started { backend, .. } => {
                    self.backend = Some(backend);
                    self.diagnostics.backend = Some(backend);
                    self.current_status = SearchStatus::Running;
                    false
                }
                SearchEvent::BackendFallback {
                    from, to, reason, ..
                } => {
                    self.backend = Some(to);
                    self.diagnostics.backend = Some(to);
                    let warning = if from == SearchBackend::Ripgrep && to == SearchBackend::Native {
                        "ripgrep was not found. Native content search is being used.".to_string()
                    } else {
                        format!("Search backend fallback: {from:?} → {to:?}: {reason}")
                    };
                    self.warning_error_message = Some(warning.clone());
                    self.diagnostics.record(SearchDiagnostic::Warning(format!(
                        "{warning} Reason: {reason}"
                    )));
                    if from == SearchBackend::Ripgrep
                        && to == SearchBackend::Native
                        && !self.ripgrep_missing_prompt_dismissed
                    {
                        self.show_ripgrep_missing_prompt = true;
                    }
                    false
                }
                SearchEvent::Result { result, .. } => {
                    if let SearchResult::ContentFile(content) = &result
                        && content.truncated
                    {
                        self.diagnostics
                            .record(SearchDiagnostic::PerFileContentTruncated {
                                path: content.path.clone(),
                                total_matches: content.total_matches,
                                displayed_matches: content.matches.len(),
                            });
                    }
                    self.push_result(result);
                    false
                }
                SearchEvent::Progress { progress, .. } => {
                    if progress.global_truncated {
                        match self.selected_mode {
                            FileSearchMode::Filename => self.diagnostics.record(
                                SearchDiagnostic::FilenameResultsTruncated {
                                    limit: self
                                        .last_submitted_request
                                        .as_ref()
                                        .map(|r| r.max_results)
                                        .unwrap_or(progress.results_found),
                                },
                            ),
                            FileSearchMode::Content => self.diagnostics.record(
                                SearchDiagnostic::GlobalMatchedFilesTruncated {
                                    limit: self
                                        .last_submitted_request
                                        .as_ref()
                                        .map(|r| r.max_results)
                                        .unwrap_or(progress.results_found),
                                },
                            ),
                        }
                    }
                    false
                }
                SearchEvent::Diagnostic { diagnostic, .. } => {
                    self.diagnostics.record(diagnostic);
                    self.inaccessible_path_warnings = self.diagnostics.inaccessible_paths.len();
                    false
                }
                SearchEvent::Completed { .. } => {
                    self.current_status = SearchStatus::Completed;
                    self.finalize_completed_results();
                    self.pending_sort_change = false;
                    self.request_immediate_repaint = true;
                    true
                }
                SearchEvent::Cancelled { .. } => {
                    self.current_status = SearchStatus::Cancelled;
                    self.request_immediate_repaint = true;
                    true
                }
                SearchEvent::Failed { error, .. } => {
                    self.current_status = SearchStatus::Failed;
                    self.warning_error_message = Some(error);
                    self.request_immediate_repaint = true;
                    true
                }
            };
        self.selection_is_valid();
        observed_terminal_event
    }

    fn selected_result_key(&self) -> Option<FileSearchResultKey> {
        self.selected_result
            .as_ref()
            .map(|s| s.row_id.result_key.clone())
    }

    fn restore_selection_by_key(&mut self, key: Option<FileSearchResultKey>) {
        let Some(key) = key else {
            return;
        };
        if let Some(row) = self
            .result_rows
            .iter()
            .find(|row| row.id.result_key == key)
            .cloned()
        {
            self.select_result(&row);
        } else {
            self.clear_selection();
        }
    }

    pub fn handle_sort_changed(&mut self) {
        self.mark_ui_preferences_dirty();
        if self.current_status == SearchStatus::Running {
            self.pending_sort_change = true;
        } else if self.current_status == SearchStatus::Completed {
            self.finalize_completed_results();
            self.request_immediate_repaint = true;
        }
    }

    fn finalize_completed_results(&mut self) {
        let selected_key = self.selected_result_key();
        let results = std::mem::take(&mut self.results);
        self.results = crate::file_search::sorting::sort_and_dedup_results(
            results,
            self.ui_preferences.filename_sort.into(),
            self.ui_preferences.content_sort.into(),
        );
        self.rebuild_result_rows();
        self.restore_selection_by_key(selected_key);
    }

    fn rebuild_result_rows(&mut self) {
        let results = self.results.clone();
        self.result_rows.clear();
        for result in results {
            self.push_result_row(&result);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_result = None;
    }

    pub fn select_result(&mut self, row: &FileSearchResultRow) {
        let payload = match &row.payload {
            FileSearchRowPayload::Filename { path, kind, .. } => {
                SelectedFileSearchResultPayload::Filename {
                    path: path.clone(),
                    kind: *kind,
                }
            }
            FileSearchRowPayload::Content {
                path,
                content_match,
                ..
            } => SelectedFileSearchResultPayload::Content {
                path: path.clone(),
                content_match: content_match.clone(),
            },
            FileSearchRowPayload::ContentGroupHeader { .. } => return,
        };
        self.selected_result = Some(SelectedFileSearchResult {
            row_id: row.id.clone(),
            payload,
        });
    }

    pub fn selected_result(&self) -> Option<&SelectedFileSearchResult> {
        self.selected_result.as_ref()
    }

    pub fn selection_is_valid(&mut self) -> bool {
        let Some(selected) = &self.selected_result else {
            return false;
        };
        let is_valid = self.result_rows.iter().any(|row| row.id == selected.row_id);
        if !is_valid {
            self.clear_selection();
        }
        is_valid
    }

    pub fn clear_results_and_selection(&mut self) {
        self.results.clear();
        self.result_rows.clear();
        self.clear_selection();
    }

    fn push_result(&mut self, result: SearchResult) {
        self.push_result_row(&result);
        self.results.push(result);
    }

    fn push_result_row(&mut self, result: &SearchResult) {
        let search_id = self.active_search_id.unwrap_or(SearchId(0));
        match result {
            SearchResult::Filename(item) => {
                self.result_rows.push(FileSearchResultRow {
                    id: FileSearchResultRowId {
                        search_id,
                        result_key: FileSearchResultKey::Filename {
                            path: crate::file_search::sorting::path_identity(&item.path),
                        },
                        match_index: None,
                        path: item.path.clone(),
                        line_number: None,
                        column: None,
                    },
                    payload: FileSearchRowPayload::Filename {
                        path: item.path.clone(),
                        display_filename: item.file_name.clone(),
                        parent_directory_display: item
                            .parent_directory
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        kind: item.kind,
                        size: item.size,
                        modified: item.modified,
                        rank: item.rank,
                        match_quality: item.match_quality,
                        filename_match_ranges: item.filename_match_ranges.clone(),
                        path_match_ranges: item.path_match_ranges.clone(),
                    },
                });
            }
            SearchResult::ContentFile(content) => {
                let group =
                    results::content_group_presentation(content, self.ui_preferences.content_sort);
                self.result_rows.push(FileSearchResultRow {
                    id: FileSearchResultRowId {
                        search_id,
                        result_key: FileSearchResultKey::Content {
                            path: crate::file_search::sorting::path_identity(&content.path),
                            line_number: 0,
                            byte_start: 0,
                            byte_end: 0,
                            occurrence: usize::MAX,
                        },
                        match_index: None,
                        path: content.path.clone(),
                        line_number: None,
                        column: None,
                    },
                    payload: FileSearchRowPayload::ContentGroupHeader {
                        path: content.path.clone(),
                        header: group.header,
                        total_matches: content.total_matches,
                        truncated: content.truncated,
                        modified: content.modified,
                    },
                });
                let mut matches = content.matches.clone();
                crate::file_search::sorting::sort_content_matches(&mut matches);
                let last_displayed_match_index = matches.len().checked_sub(1);
                let mut occurrence_counts: std::collections::HashMap<(usize, usize, usize), usize> =
                    std::collections::HashMap::new();
                for (match_index, content_match) in matches.into_iter().enumerate() {
                    let occurrence_key = (
                        content_match.line_number,
                        content_match.byte_start,
                        content_match.byte_end,
                    );
                    let occurrence = *occurrence_counts.entry(occurrence_key).or_insert(0);
                    occurrence_counts.insert(occurrence_key, occurrence + 1);
                    self.result_rows.push(FileSearchResultRow {
                        id: FileSearchResultRowId {
                            search_id,
                            result_key: FileSearchResultKey::Content {
                                path: crate::file_search::sorting::path_identity(&content.path),
                                line_number: content_match.line_number,
                                byte_start: content_match.byte_start,
                                byte_end: content_match.byte_end,
                                occurrence,
                            },
                            match_index: Some(match_index),
                            path: content.path.clone(),
                            line_number: Some(content_match.line_number),
                            column: content_match.column,
                        },
                        payload: FileSearchRowPayload::Content {
                            path: content.path.clone(),
                            content_match,
                            content_file_truncated: content.truncated,
                            is_last_displayed_match_from_truncated_file: content.truncated
                                && Some(match_index) == last_displayed_match_index,
                        },
                    });
                }
            }
        }
    }

    pub fn result_counts(&self) -> FileSearchResultCounts {
        FileSearchResultCounts {
            filename_rows: self.filename_row_count(),
            content_matched_files: self.content_matched_file_count(),
            content_displayed_match_rows: self.content_displayed_match_row_count(),
            content_truncated_displayed_matches: self.content_truncated_displayed_match_count(),
        }
    }

    pub fn filename_row_count(&self) -> usize {
        self.result_rows
            .iter()
            .filter(|row| matches!(row.payload, FileSearchRowPayload::Filename { .. }))
            .count()
    }

    pub fn content_matched_file_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result, SearchResult::ContentFile(_)))
            .count()
    }

    pub fn content_displayed_match_row_count(&self) -> usize {
        self.result_rows
            .iter()
            .filter(|row| matches!(row.payload, FileSearchRowPayload::Content { .. }))
            .count()
    }

    pub fn selectable_result_rows(&self) -> impl Iterator<Item = &FileSearchResultRow> {
        self.result_rows.iter().filter(|row| {
            matches!(
                row.payload,
                FileSearchRowPayload::Filename { .. } | FileSearchRowPayload::Content { .. }
            )
        })
    }

    pub fn content_truncated_displayed_match_count(&self) -> usize {
        self.result_rows
            .iter()
            .filter(|row| {
                matches!(
                    row.payload,
                    FileSearchRowPayload::Content {
                        is_last_displayed_match_from_truncated_file: true,
                        ..
                    }
                )
            })
            .count()
    }

    fn result_count_status_text(&self) -> String {
        let counts = self.result_counts();
        match self.selected_mode {
            FileSearchMode::Filename => format!("Results: {}", counts.filename_rows),
            FileSearchMode::Content => format!(
                "Rows: {} | Matched files: {}",
                counts.content_displayed_match_rows, counts.content_matched_files
            ),
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, coordinator: &mut SearchCoordinator) {
        self.drain_events(coordinator);
        if self.consume_immediate_repaint_request() {
            ctx.request_repaint();
        }
        if self.requires_repaint_polling() {
            ctx.request_repaint_after(ACTIVE_SEARCH_REPAINT_INTERVAL);
        }
        if !self.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.handle_escape(coordinator) == FileSearchEscapeAction::Cancel {
                ctx.request_repaint();
            }
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        let mut open = self.open;
        if let Some(resp) = egui::Window::new("File Search")
            .open(&mut open)
            .default_size(DEFAULT_WINDOW_SIZE)
            .min_size(egui::vec2(520.0, 360.0))
            .resizable(true)
            .show(ctx, |ui| self.contents(ui, coordinator))
        {
            self.persisted_window_size = resp.response.rect.size();
        }
        self.open = open;
    }

    pub fn set_ui_preferences(&mut self, preferences: FileSearchUiPreferences) {
        self.ui_preferences = preferences;
        self.ui_preferences_dirty = false;
    }

    pub fn mark_ui_preferences_dirty(&mut self) {
        self.ui_preferences_dirty = true;
    }

    pub fn update_ui_preferences(
        &mut self,
        update: impl FnOnce(&mut FileSearchUiPreferences),
    ) -> bool {
        let before = self.ui_preferences.clone();
        update(&mut self.ui_preferences);
        if self.ui_preferences != before {
            self.mark_ui_preferences_dirty();
            true
        } else {
            false
        }
    }

    pub fn save_dirty_ui_preferences(&mut self) {
        if !self.ui_preferences_dirty {
            return;
        }
        self.settings.ui_preferences = self.ui_preferences.clone();
        self.ui_preferences_dirty = false;
    }

    fn contents(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        self.handle_result_keyboard_shortcuts(ui, coordinator);
        let previously_selected_mode = self.selected_mode;
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.selected_mode,
                FileSearchMode::Filename,
                "Filename",
            );
            ui.selectable_value(&mut self.selected_mode, FileSearchMode::Content, "Content");
            self.clear_selection_if_mode_changed(previously_selected_mode);
            ui.separator();
            ui.selectable_value(
                &mut self.selected_scope,
                FileSearchScopeMode::Global,
                "Global",
            );
            ui.selectable_value(
                &mut self.selected_scope,
                FileSearchScopeMode::Directory,
                "Directory",
            );
        });
        ui.horizontal(|ui| {
            ui.label("Search");
            let search_response = ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .id_source(Self::search_field_id()),
            );
            if self.consume_search_focus_request() {
                search_response.request_focus();
            }
            if search_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.start_search(coordinator);
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                ui.ctx().request_repaint();
            }
        });
        match self.selected_scope {
            FileSearchScopeMode::Global => {
                let valid_global_roots =
                    resolve_valid_roots(self.settings.global_search_roots.clone());
                ui.label(format!(
                    "Global roots: {} configured",
                    valid_global_roots.len()
                ));
                if valid_global_roots.is_empty() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Configure at least one valid global search root in File Search settings.",
                    );
                }
                egui::CollapsingHeader::new("Global root list")
                    .default_open(false)
                    .show(ui, |ui| {
                        for root in &valid_global_roots {
                            ui.label(root.display().to_string());
                        }
                    });
            }
            FileSearchScopeMode::Directory => {
                ui.vertical(|ui| {
                    if ui.button("Add folder…").clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        self.custom_roots.push(path.display().to_string());
                    }
                    let mut remove = None;
                    let mut submit_search = false;
                    for (idx, root) in self.custom_roots.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label("Root");
                            let root_response = ui.add(
                                egui::TextEdit::singleline(root)
                                    .id_source((Self::root_field_id(), idx)),
                            );
                            if root_response.has_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                submit_search = true;
                                ui.input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                });
                                ui.ctx().request_repaint();
                            }
                            let valid = PathBuf::from(root.trim())
                                .canonicalize()
                                .is_ok_and(|p| p.is_dir());
                            ui.label(if valid { "valid" } else { "invalid" });
                            if ui.button("Remove").clicked() {
                                remove = Some(idx);
                            }
                        });
                    }
                    if submit_search {
                        self.start_search(coordinator);
                    }
                    if let Some(idx) = remove {
                        self.custom_roots.remove(idx);
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.case_sensitive, "Case-sensitive");
            ui.checkbox(&mut self.include_hidden, "Include hidden");
            let search_enabled = !self.search_text.trim().is_empty()
                && match self.selected_scope {
                    FileSearchScopeMode::Global => {
                        !resolve_valid_roots(self.settings.global_search_roots.clone()).is_empty()
                    }
                    FileSearchScopeMode::Directory => {
                        !resolve_valid_roots(self.custom_roots.iter().map(PathBuf::from)).is_empty()
                    }
                };
            if ui
                .add_enabled(search_enabled, egui::Button::new("Search"))
                .clicked()
                && self.start_search(coordinator).is_some()
            {
                ui.ctx().request_repaint();
            }
            if ui.button("Cancel").clicked() {
                self.cancel_search(coordinator);
                ui.ctx().request_repaint();
            }
            if ui
                .add_enabled(self.can_open_selected_result(), egui::Button::new("Open"))
                .clicked()
            {
                self.open_selected_result();
                ui.ctx().request_repaint();
            }
            if ui
                .add_enabled(
                    self.selected_result().is_some(),
                    egui::Button::new("Copy selected"),
                )
                .clicked()
                && let Some(payload) = self.copy_selected_payload()
            {
                self.copy_text_payload("copy selected", payload);
            }
            if ui
                .add_enabled(
                    !self.result_rows.is_empty(),
                    egui::Button::new("Copy all visible"),
                )
                .clicked()
                && let Some(payload) = self.copy_all_visible_results_payload()
            {
                self.copy_text_payload("copy visible results", payload);
            }
            if ui
                .add_enabled(
                    !self.result_rows.is_empty(),
                    egui::Button::new("Export visible results…"),
                )
                .clicked()
            {
                self.export_visible_results_to_file();
            }
        });
        self.filters_ui(ui);
        ui.label(format!(
            "Status: {:?} | {} | Warnings: {}",
            self.current_status,
            self.result_count_status_text(),
            self.diagnostics.warning_count()
        ));
        egui::CollapsingHeader::new("Diagnostics")
            .default_open(false)
            .show(ui, |ui| {
                for line in self.diagnostics.summary_lines() {
                    ui.label(line);
                }
            });
        if let Some(msg) = &self.warning_error_message {
            ui.colored_label(egui::Color32::YELLOW, msg);
        }
        self.ripgrep_missing_prompt_ui(ui, coordinator);
        let results_region_size = ui.available_size();
        ui.allocate_ui_with_layout(results_region_size, *ui.layout(), |ui| {
            egui::ScrollArea::both()
                .id_source(results_scroll_id_source(self.selected_mode))
                .auto_shrink([false, false])
                .show(ui, |ui| match self.selected_mode {
                    FileSearchMode::Filename => self.filename_results(ui, coordinator),
                    FileSearchMode::Content => self.content_results(ui, coordinator),
                });
        });
    }

    fn ripgrep_missing_prompt_ui(
        &mut self,
        ui: &mut egui::Ui,
        coordinator: &mut SearchCoordinator,
    ) {
        if !self.show_ripgrep_missing_prompt || self.ripgrep_missing_prompt_dismissed {
            return;
        }
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::YELLOW, "ripgrep (rg) was not found. Native content search will continue; configure rg for faster future searches.");
            if ui.button("Locate rg.exe").clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("Executable", &["exe", ""]).pick_file() {
                    match validate_ripgrep_selection(&path) {
                        Ok(abs) => {
                            self.settings.ripgrep_executable_path = abs;
                            self.ui_preferences_dirty = true;
                            coordinator.reconfigure_from_settings(self.settings.clone());
                            self.show_ripgrep_missing_prompt = false;
                            self.warning_error_message = Some("ripgrep path saved for future searches.".to_owned());
                        }
                        Err(err) => self.warning_error_message = Some(err),
                    }
                }
            if ui.button("Dismiss").clicked() {
                self.dismiss_ripgrep_missing_prompt();
            }
        });
    }

    pub fn dismiss_ripgrep_missing_prompt(&mut self) {
        self.show_ripgrep_missing_prompt = false;
        self.ripgrep_missing_prompt_dismissed = true;
    }

    pub fn configure_ripgrep_path_for_future_searches(
        &mut self,
        path: PathBuf,
        coordinator: &mut SearchCoordinator,
    ) -> Result<(), String> {
        let abs = validate_ripgrep_selection(&path)?;
        self.settings.ripgrep_executable_path = abs;
        self.ui_preferences_dirty = true;
        coordinator.reconfigure_from_settings(self.settings.clone());
        self.show_ripgrep_missing_prompt = false;
        Ok(())
    }

    fn filename_results(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        let previous_columns = self.ui_preferences.visible_columns.clone();
        results::ensure_filename_columns_visible(&mut self.ui_preferences);
        if self.ui_preferences.visible_columns != previous_columns {
            self.ui_preferences_dirty = true;
        }
        ui.horizontal(|ui| {
            ui.menu_button("Columns", |ui| {
                let mut dirty = false;
                for column in results::OPTIONAL_FILENAME_COLUMNS {
                    let mut visible = self.ui_preferences.visible_columns.contains(column);
                    if ui
                        .checkbox(&mut visible, results::column_label(*column))
                        .changed()
                    {
                        dirty |= results::set_filename_column_visible(
                            &mut self.ui_preferences,
                            *column,
                            visible,
                        );
                    }
                }
                ui.separator();
                if ui.button("Reset to defaults").clicked() {
                    results::reset_filename_columns_to_defaults(&mut self.ui_preferences);
                    dirty = true;
                    ui.close_menu();
                }
                if dirty {
                    self.ui_preferences_dirty = true;
                }
            });
        });

        let rows: Vec<_> = self
            .result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Filename {
                    path,
                    display_filename,
                    parent_directory_display,
                    kind,
                    size,
                    modified,
                    rank,
                    match_quality,
                    filename_match_ranges,
                    path_match_ranges,
                } => Some((
                    row.id.clone(),
                    path.clone(),
                    display_filename.clone(),
                    parent_directory_display.clone(),
                    *kind,
                    *size,
                    *modified,
                    *rank,
                    *match_quality,
                    filename_match_ranges.clone(),
                    path_match_ranges.clone(),
                )),
                FileSearchRowPayload::Content { .. }
                | FileSearchRowPayload::ContentGroupHeader { .. } => None,
            })
            .collect();

        let visible_columns = self.ui_preferences.visible_columns.clone();
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click());
        for column in &visible_columns {
            let width = self
                .ui_preferences
                .column_widths
                .get(column)
                .copied()
                .unwrap_or(match column {
                    crate::file_search::settings::FileSearchColumn::Name => 180,
                    crate::file_search::settings::FileSearchColumn::Directory => 260,
                    crate::file_search::settings::FileSearchColumn::MatchQuality => 110,
                    crate::file_search::settings::FileSearchColumn::Kind => 70,
                    crate::file_search::settings::FileSearchColumn::Size => 80,
                    crate::file_search::settings::FileSearchColumn::Modified => 130,
                    _ => 100,
                });
            table = table.column(Column::initial(width as f32).resizable(true).clip(true));
        }

        table
            .header(20.0, |mut header| {
                for column in &visible_columns {
                    header.col(|ui| {
                        let label = format!(
                            "{}{}",
                            results::column_label(*column),
                            results::filename_sort_indicator(self.ui_preferences.filename_sort, *column)
                        );
                        if ui.button(label).clicked() {
                            if let Some(sort) = results::filename_sort_after_header_click(
                                self.ui_preferences.filename_sort,
                                *column,
                            ) {
                                self.ui_preferences.filename_sort = sort;
                                self.ui_preferences_dirty = true;
                                self.pending_sort_change = true;
                            }
                        }
                    });
                }
            })
            .body(|mut body| {
                for (row_id, path, display_filename, parent_directory_display, kind, size, modified, rank, match_quality, filename_match_ranges, path_match_ranges) in rows {
                    let is_selected = self
                        .selected_result()
                        .map(|selected| selected.row_id == row_id)
                        .unwrap_or(false);
                    let row_payload = FileSearchResultRow {
                        id: row_id.clone(),
                        payload: FileSearchRowPayload::Filename {
                            path: path.clone(),
                            display_filename: display_filename.clone(),
                            parent_directory_display: parent_directory_display.clone(),
                            kind,
                            size,
                            modified,
                            rank,
                            match_quality,
                            filename_match_ranges: filename_match_ranges.clone(),
                            path_match_ranges: path_match_ranges.clone(),
                        },
                    };
                    body.row(22.0, |mut row| {
                        for column in &visible_columns {
                            row.col(|ui| {
                                ui.push_id((result_row_id_source(row_id.search_id, &row_id), *column), |ui| {
                                    let response = match column {
                                        crate::file_search::settings::FileSearchColumn::Name => results::non_wrapping_selectable_label(ui, is_selected, egui::WidgetText::LayoutJob(results::highlighted_job(&display_filename, &filename_match_ranges))),
                                        crate::file_search::settings::FileSearchColumn::Directory => results::non_wrapping_selectable_label(ui, is_selected, egui::WidgetText::LayoutJob(results::highlighted_job(&parent_directory_display, &path_match_ranges))),
                                        crate::file_search::settings::FileSearchColumn::Kind => results::non_wrapping_selectable_label(ui, is_selected, format!("{kind:?}")),
                                        crate::file_search::settings::FileSearchColumn::MatchQuality => results::non_wrapping_selectable_label(ui, is_selected, results::format_match_quality(match_quality)),
                                        crate::file_search::settings::FileSearchColumn::Size => results::non_wrapping_selectable_label(ui, is_selected, size.map(results::format_size).unwrap_or_else(|| "—".to_owned())),
                                        crate::file_search::settings::FileSearchColumn::Modified => results::non_wrapping_selectable_label(ui, is_selected, results::format_optional_modified(modified)),
                                        _ => results::non_wrapping_selectable_label(ui, is_selected, ""),
                                    }
                                    .on_hover_text(path.display().to_string());
                                    let double_clicked = response.double_clicked();
                                    if is_selected {
                                        response.scroll_to_me(Some(egui::Align::Center));
                                    }
                                    if response.clicked() {
                                        self.select_result(&row_payload);
                                    }
                                    response.context_menu(|ui| {
                                        self.result_context_menu(
                                            ui,
                                            &path,
                                            kind == crate::file_search::model::FileKind::Directory,
                                            None,
                                            coordinator,
                                        )
                                    });
                                    if double_clicked {
                                        self.open_result_row(&row_payload);
                                    }
                                });
                            });
                        }
                    });
                }
            });
    }

    fn clear_selection_if_mode_changed(&mut self, previously_selected_mode: FileSearchMode) {
        if self.selected_mode != previously_selected_mode {
            self.clear_selection();
        }
    }

    fn content_results(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        let rows = self.result_rows.clone();
        for display_row in rows {
            match display_row.payload {
                FileSearchRowPayload::ContentGroupHeader { path, header, .. } => {
                    ui.push_id(("content_file_header", &path), |ui| {
                        let response = ui
                            .label(egui::RichText::new(header).strong())
                            .on_hover_text(path.display().to_string());
                        response.context_menu(|ui| {
                            self.result_context_menu(ui, &path, false, None, coordinator)
                        });
                    });
                }
                FileSearchRowPayload::Content {
                    path,
                    content_match,
                    content_file_truncated,
                    is_last_displayed_match_from_truncated_file,
                } => {
                    let row_id = display_row.id;
                    ui.push_id(result_row_id_source(row_id.search_id, &row_id), |ui| {
                        let hover_location = content_match
                            .column
                            .map(|column| {
                                format!("{}:{}", path.display(), column.saturating_add(1))
                            })
                            .unwrap_or_else(|| path.display().to_string());
                        let row_text = results::content_line_label(&content_match);
                        let is_selected = self
                            .selected_result()
                            .map(|selected| selected.row_id == row_id)
                            .unwrap_or(false);
                        let response =
                            results::non_wrapping_selectable_label(ui, is_selected, row_text)
                                .on_hover_text(hover_location);
                        let double_clicked = response.double_clicked();
                        let row_payload = FileSearchResultRow {
                            id: row_id.clone(),
                            payload: FileSearchRowPayload::Content {
                                path: path.clone(),
                                content_match: content_match.clone(),
                                content_file_truncated,
                                is_last_displayed_match_from_truncated_file,
                            },
                        };
                        if is_selected {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                        if response.clicked() {
                            self.select_result(&row_payload);
                        }
                        response.context_menu(|ui| {
                            self.content_result_context_menu(
                                ui,
                                &path,
                                content_match.clone(),
                                coordinator,
                            )
                        });
                        if double_clicked {
                            self.open_result_row(&row_payload);
                        }
                    });
                }
                FileSearchRowPayload::Filename { .. } => {}
            }
        }
    }

    fn content_result_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        path: &std::path::Path,
        content_match: ContentMatch,
        coordinator: &mut SearchCoordinator,
    ) {
        if ui.button("Show all matches in this file").clicked() {
            self.start_file_content_search(path, coordinator);
            ui.ctx().request_repaint();
            ui.close_menu();
        }
        self.result_context_menu(ui, path, false, Some(content_match), coordinator);
    }

    pub fn can_open_selected_result(&self) -> bool {
        self.selected_result.is_some()
    }

    pub fn resolve_open_selected_result_action(
        &mut self,
    ) -> Option<OpenSelectedFileSearchResultAction> {
        let selected = self.selected_result.clone()?;
        match selected.payload {
            SelectedFileSearchResultPayload::Filename { path, kind } => {
                match resolve_explorer_action(&path) {
                    Ok(ExplorerAction::Unsupported { path, reason }) => {
                        self.warning_error_message = Some(format!(
                            "Cannot open {}: {reason} (stored result kind: {kind:?}).",
                            path.display()
                        ));
                        None
                    }
                    Ok(action) => Some(OpenSelectedFileSearchResultAction::Explorer(action)),
                    Err(err) => {
                        self.warning_error_message =
                            Some(format!("Cannot open {}: {err}.", path.display()));
                        None
                    }
                }
            }
            SelectedFileSearchResultPayload::Content {
                path,
                content_match,
            } => {
                let request = self.request_preview(&path, Some(&content_match));
                Some(OpenSelectedFileSearchResultAction::PreviewContent { request })
            }
        }
    }

    pub fn open_selected_result(&mut self) -> Option<OpenSelectedFileSearchResultAction> {
        let action = self.resolve_open_selected_result_action()?;
        match &action {
            OpenSelectedFileSearchResultAction::Explorer(action) => {
                let action = action.clone();
                self.run_result_action("open", || execute_explorer_action(action));
            }
            OpenSelectedFileSearchResultAction::PreviewContent { .. } => {}
        }
        Some(action)
    }

    pub fn open_result_row(
        &mut self,
        row: &FileSearchResultRow,
    ) -> Option<OpenSelectedFileSearchResultAction> {
        self.select_result(row);
        self.open_selected_result()
    }

    fn apply_preview_settings_to_request(&self, request: &mut PreviewRequest) {
        request.max_bytes_full_file_preview = self
            .settings
            .max_full_preview_file_size_bytes
            .try_into()
            .unwrap_or(usize::MAX);
    }

    pub fn request_preview(
        &mut self,
        path: &std::path::Path,
        first_match: Option<&ContentMatch>,
    ) -> PreviewRequest {
        let mut request = first_match
            .map(|content_match| {
                let mut request = PreviewRequest::for_match(
                    path,
                    content_match.line_number,
                    content_match
                        .column
                        .map(|column| column.saturating_add(1))
                        .unwrap_or(1),
                );
                if let Some(selection) = request.selected_match.as_mut() {
                    selection.source_line = Some(content_match.line.clone());
                    selection.match_length = Some(
                        content_match
                            .byte_end
                            .saturating_sub(content_match.byte_start)
                            .max(1),
                    );
                    selection.end_column = Some(
                        selection
                            .start_column
                            .saturating_add(selection.match_length.unwrap_or(1)),
                    );
                }
                request
            })
            .unwrap_or_else(|| PreviewRequest::new(path));
        self.apply_preview_settings_to_request(&mut request);
        if let Some(content_match) = first_match.cloned() {
            self.preview_dialog
                .open_content_match(path, content_match, &self.settings);
        } else {
            self.preview_dialog.open = true;
            self.preview_dialog.current_request = Some(request.clone());
            self.preview_dialog.selected_match = None;
            self.preview_dialog.action_error_message = None;
            self.preview_dialog.pending_auto_scroll = false;
            self.preview_dialog.reset_horizontal_scroll = true;
            self.preview_dialog.load_current_preview();
        }
        request
    }

    pub fn context_menu_intents_for_row(
        row: &FileSearchResultRow,
    ) -> Vec<FileSearchContextMenuIntent> {
        let (path, is_directory, content_match) = match &row.payload {
            FileSearchRowPayload::Filename { path, kind, .. } => (
                path.clone(),
                *kind == crate::file_search::model::FileKind::Directory,
                None,
            ),
            FileSearchRowPayload::Content {
                path,
                content_match,
                ..
            } => (path.clone(), false, Some(content_match.clone())),
            FileSearchRowPayload::ContentGroupHeader { path, .. } => (path.clone(), false, None),
        };

        let mut actions = vec![
            FileSearchContextMenuAction::OpenInConfiguredEditor,
            FileSearchContextMenuAction::Preview,
            FileSearchContextMenuAction::OpenFileOrDirectory,
            FileSearchContextMenuAction::RevealInExplorer,
            FileSearchContextMenuAction::OpenContainingDirectory,
            FileSearchContextMenuAction::CopyFullPath,
            FileSearchContextMenuAction::CopyFilename,
        ];
        if content_match.is_some() {
            actions.push(FileSearchContextMenuAction::CopyMatchingLine);
        }
        actions.extend([
            FileSearchContextMenuAction::OpenTerminal,
            FileSearchContextMenuAction::NestedFilenameSearch,
            FileSearchContextMenuAction::NestedContentSearch,
        ]);
        if content_match.is_some() {
            actions.push(FileSearchContextMenuAction::ShowAllMatchesInThisFile);
        }

        actions
            .into_iter()
            .map(|action| FileSearchContextMenuIntent {
                action,
                path: path.clone(),
                content_match: content_match.clone(),
                is_directory,
            })
            .collect()
    }

    fn start_file_content_search(
        &mut self,
        path: &std::path::Path,
        coordinator: &mut SearchCoordinator,
    ) {
        if self.search_text.trim().is_empty() {
            self.warning_error_message =
                Some("Enter search text before searching this file.".to_string());
            return;
        }
        self.selected_mode = FileSearchMode::Content;
        let mut request = match self.build_search_request() {
            Ok(request) => request,
            Err(error) => {
                self.warning_error_message = Some(error.user_message().to_string());
                return;
            }
        };
        request.scope = SearchScope::Files {
            files: vec![path.to_path_buf()],
        };
        request.max_results = 1;
        self.clear_results_and_selection();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.diagnostics.clear();
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend_with_settings(
            &request,
            Some(&self.settings),
        ));
        let submitted_request = request.clone();
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
        self.last_submitted_request = Some(submitted_request);
        self.pending_sort_change = false;
        self.request_immediate_repaint = true;
    }

    fn result_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        path: &std::path::Path,
        is_directory: bool,
        first_match: Option<ContentMatch>,
        coordinator: &mut SearchCoordinator,
    ) {
        if ui.button("Open in configured editor").clicked() {
            let settings = self.settings.clone();
            self.run_result_action("open in configured editor", || {
                open_in_configured_editor(
                    &settings,
                    InvocationTarget {
                        file: path,
                        line: first_match.as_ref().map(|m| m.line_number),
                        column: first_match
                            .as_ref()
                            .and_then(|m| m.column.map(|c| c.saturating_add(1))),
                    },
                )
            });
            ui.close_menu();
        }
        if ui.button("Preview").clicked() {
            self.request_preview(path, first_match.as_ref());
            ui.close_menu();
        }
        if ui.button("Open file or directory").clicked() {
            self.run_result_action("open", || {
                execute_explorer_action(resolve_explorer_action(path)?)
            });
            ui.close_menu();
        }
        if ui.button("Reveal in Explorer").clicked() {
            self.run_result_action("reveal", || {
                execute_explorer_action(resolve_explorer_action(path)?)
            });
            ui.close_menu();
        }
        if ui.button("Open containing directory").clicked() {
            self.run_result_action("open containing directory", || {
                let dir = containing_directory(path).ok_or_else(|| {
                    anyhow::anyhow!("{} has no containing directory", path.display())
                })?;
                open_path(&dir)
            });
            ui.close_menu();
        }
        if ui.button("Copy full path").clicked() {
            self.run_result_action("copy full path", || {
                clipboard::set_text(&path.display().to_string())?;
                Ok(())
            });
            ui.close_menu();
        }
        if ui.button("Copy filename").clicked() {
            self.run_result_action("copy filename", || {
                let name = copied_filename(path)
                    .ok_or_else(|| anyhow::anyhow!("{} has no filename", path.display()))?;
                clipboard::set_text(&name)?;
                Ok(())
            });
            ui.close_menu();
        }
        if let Some(content_match) = first_match.as_ref()
            && ui.button("Copy matching line").clicked()
        {
            let line = content_match.line.clone();
            self.run_result_action("copy matching line", || {
                clipboard::set_text(&line)?;
                Ok(())
            });
            ui.close_menu();
        }
        if ui.button("Open terminal in containing directory").clicked() {
            let settings = self.settings.clone();
            self.run_result_action("open terminal", || {
                let dir = containing_directory(path).ok_or_else(|| {
                    anyhow::anyhow!("{} has no containing directory", path.display())
                })?;
                open_configured_terminal_in_directory(&settings, &dir)
            });
            ui.close_menu();
        }
        if ui
            .button("Start filename search beneath this directory")
            .clicked()
        {
            self.start_nested_search(path, is_directory, FileSearchMode::Filename, coordinator);
            ui.ctx().request_repaint();
            ui.close_menu();
        }
        if ui
            .button("Start content search beneath this directory")
            .clicked()
        {
            self.start_nested_search(path, is_directory, FileSearchMode::Content, coordinator);
            ui.ctx().request_repaint();
            ui.close_menu();
        }
    }

    fn run_result_action(&mut self, label: &str, action: impl FnOnce() -> anyhow::Result<()>) {
        if let Err(err) = action() {
            self.warning_error_message = Some(format!("Failed to {label}: {err}"));
        }
    }

    fn start_nested_search(
        &mut self,
        path: &std::path::Path,
        is_directory: bool,
        mode: FileSearchMode,
        coordinator: &mut SearchCoordinator,
    ) {
        match nested_search_root(path, is_directory) {
            Some(root) => {
                self.selected_mode = mode;
                self.selected_scope = FileSearchScopeMode::Directory;
                self.custom_roots = vec![root.display().to_string()];
                self.start_search(coordinator);
            }
            None => {
                self.warning_error_message = Some(format!(
                    "Cannot search beneath {} because it has no containing directory.",
                    path.display()
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::coordinator::{CancellationToken, SearchExecutor};
    use crate::file_search::model::{
        ContentFileResult, ContentMatch, FileKind, FilenameRank, FilenameResult, SearchProgress,
    };
    use std::sync::{Arc, Mutex, mpsc};

    struct RecordingExecutor {
        requests: Mutex<Vec<SearchRequest>>,
    }

    impl RecordingExecutor {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
            })
        }
    }

    impl SearchExecutor for RecordingExecutor {
        fn execute(
            &self,
            _id: SearchId,
            request: SearchRequest,
            _token: CancellationToken,
            events: mpsc::Sender<SearchEvent>,
        ) {
            self.requests.lock().unwrap().push(request);
            let _ = events.send(SearchEvent::Completed { id: SearchId(1) });
        }
    }

    fn recorded_request(
        state: &mut FileSearchDialogState,
        executor: &Arc<RecordingExecutor>,
    ) -> SearchRequest {
        let mut coordinator = SearchCoordinator::with_executor(executor.clone());
        let search_id = state
            .start_search(&mut coordinator)
            .expect("search should start");
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(request) = executor.requests.lock().unwrap().last().cloned() {
                return request;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for recorded request for search {search_id:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn opening_with_mode_preselected() {
        let mut state = FileSearchDialogState::default();
        state.open_with_mode(FileSearchMode::Content);
        assert!(state.open);
        assert_eq!(state.selected_mode, FileSearchMode::Content);
    }

    #[test]
    fn dirty_state_changes_only_for_preferences() {
        let mut state = FileSearchDialogState::default();

        state.search_text = "needle".into();
        state.selected_result = None;
        assert!(!state.ui_preferences_dirty);

        let changed = state.update_ui_preferences(|prefs| {
            prefs.whole_word = true;
        });
        assert!(changed);
        assert!(state.ui_preferences_dirty);

        state.save_dirty_ui_preferences();
        assert!(!state.ui_preferences_dirty);
        let unchanged = state.update_ui_preferences(|prefs| {
            prefs.whole_word = true;
        });
        assert!(!unchanged);
        assert!(!state.ui_preferences_dirty);
    }

    #[test]
    fn global_roots_resolve_into_roots_scope() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root-a");
        std::fs::create_dir(&root).unwrap();
        let executor = RecordingExecutor::new();
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            settings: FileSearchSettings {
                global_search_roots: vec![root.clone()],
                ..FileSearchSettings::default()
            },
            ..Default::default()
        };

        let request = recorded_request(&mut state, &executor);

        assert_eq!(
            request.scope,
            SearchScope::Roots {
                roots: vec![root.canonicalize().unwrap()]
            }
        );
    }

    #[test]
    fn empty_global_roots_produce_clear_validation_error() {
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            settings: FileSearchSettings {
                global_search_roots: vec![],
                ..FileSearchSettings::default()
            },
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        assert!(state.start_search(&mut coordinator).is_none());
        assert_eq!(
            state.warning_error_message.as_deref(),
            Some("Configure at least one valid global search root in File Search settings.")
        );
    }

    #[test]
    fn repeated_global_roots_are_deduplicated() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let executor = RecordingExecutor::new();
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            settings: FileSearchSettings {
                global_search_roots: vec![root.clone(), root.clone()],
                ..FileSearchSettings::default()
            },
            ..Default::default()
        };

        let request = recorded_request(&mut state, &executor);

        assert_eq!(
            request.scope,
            SearchScope::Roots {
                roots: vec![root.canonicalize().unwrap()]
            }
        );
    }

    #[test]
    fn custom_root_launcher_action_resolves_to_one_root() {
        let temp = tempfile::tempdir().unwrap();
        let executor = RecordingExecutor::new();
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            selected_scope: FileSearchScopeMode::Directory,
            custom_roots: vec![temp.path().display().to_string()],
            ..Default::default()
        };

        let request = recorded_request(&mut state, &executor);

        assert_eq!(
            request.scope,
            SearchScope::Roots {
                roots: vec![temp.path().canonicalize().unwrap()]
            }
        );
    }

    #[test]
    fn opening_file_search_requests_search_focus_once() {
        let mut state = FileSearchDialogState::default();

        state.open();

        assert!(state.open);
        assert!(state.request_search_focus);
    }

    #[test]
    fn consuming_search_focus_request_clears_flag_after_one_use() {
        let mut state = FileSearchDialogState::default();
        state.open();

        assert!(state.consume_search_focus_request());
        assert!(!state.request_search_focus);
        assert!(!state.consume_search_focus_request());
    }

    #[test]
    fn reopening_file_search_requests_search_focus_again() {
        let mut state = FileSearchDialogState::default();
        state.open();
        assert!(state.consume_search_focus_request());

        state.open_with_mode(FileSearchMode::Content);

        assert!(state.request_search_focus);
        assert_eq!(state.selected_mode, FileSearchMode::Content);
    }

    #[test]
    fn root_field_has_stable_distinct_id_and_does_not_rearm_search_focus() {
        let mut state = FileSearchDialogState::default();
        state.open();
        assert!(state.consume_search_focus_request());

        let root_id = FileSearchDialogState::root_field_id();
        let search_id = FileSearchDialogState::search_field_id();

        assert_ne!(root_id, search_id);
        assert!(!state.request_search_focus);
        assert!(!state.consume_search_focus_request());
    }

    #[test]
    fn opening_file_search_does_not_depend_on_launcher_query_text() {
        let launcher_query = String::from("fs");
        let mut state = FileSearchDialogState::default();

        state.open();

        assert_eq!(launcher_query, "fs");
        assert!(state.request_search_focus);
        assert_ne!(
            FileSearchDialogState::search_field_id(),
            egui::Id::new("query_input")
        );
    }

    #[test]
    fn repaint_polling_only_required_while_running() {
        let mut state = FileSearchDialogState::default();

        state.current_status = SearchStatus::Running;
        assert!(state.requires_repaint_polling());

        for status in [
            SearchStatus::Pending,
            SearchStatus::Completed,
            SearchStatus::Cancelled,
            SearchStatus::Failed,
        ] {
            state.current_status = status;
            assert!(!state.requires_repaint_polling());
        }
    }

    #[test]
    fn starting_search_sets_immediate_repaint_request() {
        let mut state = FileSearchDialogState {
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        let id = state.start_search(&mut coordinator);

        assert!(id.is_some());
        assert!(state.consume_immediate_repaint_request());
        assert!(!state.consume_immediate_repaint_request());
    }

    #[test]
    fn terminal_events_request_immediate_repaint_and_stop_polling() {
        for event in [
            SearchEvent::Completed { id: SearchId(7) },
            SearchEvent::Cancelled { id: SearchId(7) },
            SearchEvent::Failed {
                id: SearchId(7),
                error: "boom".into(),
            },
        ] {
            let mut state = FileSearchDialogState {
                active_search_id: Some(SearchId(7)),
                current_status: SearchStatus::Running,
                ..Default::default()
            };

            assert!(state.apply_event(event));

            assert!(!state.requires_repaint_polling());
            assert!(state.consume_immediate_repaint_request());
        }
    }

    #[test]
    fn cancellation_stops_polling_after_cancelled_event_is_observed() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(7)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        assert!(state.requires_repaint_polling());

        assert!(state.apply_event(SearchEvent::Cancelled { id: SearchId(7) }));

        assert_eq!(state.current_status, SearchStatus::Cancelled);
        assert!(!state.requires_repaint_polling());
    }

    #[test]
    fn starting_search_sets_active_state() {
        let mut state = FileSearchDialogState {
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();
        let id = state.start_search(&mut coordinator);
        assert!(id.is_some());
        assert_eq!(state.active_search_id, id);
        assert_eq!(state.current_status, SearchStatus::Running);
    }

    #[test]
    fn global_filename_search_uses_walkdir_backend_when_everything_is_disabled() {
        let settings = FileSearchSettings {
            everything_enabled: false,
            ..FileSearchSettings::default()
        };
        let mut state = FileSearchDialogState {
            search_text: "foo".into(),
            selected_mode: FileSearchMode::Filename,
            selected_scope: FileSearchScopeMode::Global,
            settings: settings.clone(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::with_settings(settings);

        state.start_search(&mut coordinator);

        assert_eq!(state.backend, Some(SearchBackend::WalkDir));
        assert_eq!(coordinator.last_backend(), None);
    }

    #[test]
    fn enter_in_file_search_field_starts_exactly_one_search() {
        let mut state = FileSearchDialogState {
            open: true,
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        let id = state.start_search(&mut coordinator);

        assert!(id.is_some());
        assert_eq!(coordinator.diagnostics().started, 0);
    }

    #[test]
    fn cancelling_search_updates_status() {
        let mut state = FileSearchDialogState {
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();
        state.start_search(&mut coordinator);
        state.cancel_search(&mut coordinator);
        assert_eq!(state.current_status, SearchStatus::Cancelled);
    }

    #[test]
    fn escape_while_running_cancels_and_leaves_dialog_open() {
        let mut state = FileSearchDialogState {
            open: true,
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();
        state.start_search(&mut coordinator);

        let action = state.handle_escape(&mut coordinator);

        assert_eq!(action, FileSearchEscapeAction::Cancel);
        assert!(state.open);
        assert_eq!(state.current_status, SearchStatus::Cancelled);
    }

    #[test]
    fn escape_while_idle_closes_dialog() {
        let mut state = FileSearchDialogState {
            open: true,
            current_status: SearchStatus::Completed,
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        let action = state.handle_escape(&mut coordinator);

        assert_eq!(action, FileSearchEscapeAction::Close);
        assert!(!state.open);
    }

    #[test]
    fn second_escape_after_cancellation_closes_now_idle_dialog() {
        let mut state = FileSearchDialogState {
            open: true,
            search_text: "foo".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();
        state.start_search(&mut coordinator);

        assert_eq!(
            state.handle_escape(&mut coordinator),
            FileSearchEscapeAction::Cancel
        );
        assert_eq!(
            state.handle_escape(&mut coordinator),
            FileSearchEscapeAction::Close
        );

        assert!(!state.open);
    }

    #[test]
    fn escape_ui_consumes_key_after_file_search_handles_it() {
        let ctx = egui::Context::default();
        let mut state = FileSearchDialogState {
            open: true,
            current_status: SearchStatus::Completed,
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        ctx.begin_frame(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        state.ui(&ctx, &mut coordinator);
        let consumed_by_later_launcher_code =
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let _ = ctx.end_frame();

        assert!(!consumed_by_later_launcher_code);
        assert!(!state.open);
    }

    #[test]
    fn applying_active_events_updates_results_and_status() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(7)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Started {
            id: SearchId(7),
            backend: SearchBackend::WalkDir,
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(7),
            result: SearchResult::Filename(FilenameResult {
                path: "a.txt".into(),
                file_name: "a.txt".into(),
                parent_directory: Some("/tmp".into()),
                kind: FileKind::File,
                size: None,
                modified: None,
                rank: FilenameRank::ExactFilename,
                match_quality: FilenameRank::ExactFilename,
                filename_match_ranges: Vec::new(),
                path_match_ranges: Vec::new(),
                arrival_index: 0,
            }),
        });
        state.apply_event(SearchEvent::Progress {
            id: SearchId(7),
            progress: SearchProgress {
                files_scanned: 2,
                directories_scanned: 5,
                results_found: 1,
                status: SearchStatus::Running,
                global_truncated: false,
            },
        });
        state.apply_event(SearchEvent::Completed { id: SearchId(7) });
        assert_eq!(state.backend, Some(SearchBackend::WalkDir));
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.inaccessible_path_warnings, 0);
        assert_eq!(state.current_status, SearchStatus::Completed);
    }

    #[test]
    fn ignoring_stale_events() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(2)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Failed {
            id: SearchId(1),
            error: "old".into(),
        });
        assert_eq!(state.current_status, SearchStatus::Pending);
        assert!(state.warning_error_message.is_none());
    }

    #[test]
    fn same_filename_in_different_directories_creates_distinct_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        for parent in ["/tmp/a", "/tmp/b"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(1),
                result: SearchResult::Filename(FilenameResult {
                    path: PathBuf::from(parent).join("main.rs"),
                    file_name: "main.rs".into(),
                    parent_directory: Some(parent.into()),
                    kind: FileKind::File,
                    size: None,
                    modified: None,
                    rank: FilenameRank::ExactFilename,
                    match_quality: FilenameRank::ExactFilename,
                    filename_match_ranges: Vec::new(),
                    path_match_ranges: Vec::new(),
                    arrival_index: 0,
                }),
            });
        }

        assert_eq!(state.result_rows.len(), 2);
        assert_ne!(state.result_rows[0].id.path, state.result_rows[1].id.path);
        assert_ne!(
            state.result_rows[0].id.result_key,
            state.result_rows[1].id.result_key
        );
    }

    #[test]
    fn multiple_content_matches_in_one_file_create_multiple_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: true,
            }),
        });

        assert_eq!(state.result_rows.len(), 3);
        assert!(matches!(
            state.result_rows[0].payload,
            FileSearchRowPayload::ContentGroupHeader { .. }
        ));
        assert_eq!(state.result_rows[1].id.match_index, Some(0));
        assert_eq!(state.result_rows[2].id.match_index, Some(1));
        match &state.result_rows[2].payload {
            FileSearchRowPayload::Content {
                is_last_displayed_match_from_truncated_file,
                ..
            } => {
                assert!(*is_last_displayed_match_from_truncated_file);
            }
            _ => panic!("expected content row"),
        }
    }

    #[test]
    fn two_matches_on_same_line_produce_distinct_stable_keys() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(10, "needle needle".into(), 0, 6),
                    ContentMatch::new(10, "needle needle".into(), 0, 6),
                ],
                truncated: false,
            }),
        });

        assert_eq!(state.result_rows.len(), 3);
        assert_ne!(
            state.result_rows[1].id.result_key,
            state.result_rows[2].id.result_key
        );
    }

    #[test]
    fn stable_keys_remain_unchanged_after_sorting_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        for path in ["src/b.rs", "src/a.rs"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(1),
                result: SearchResult::Filename(FilenameResult {
                    path: PathBuf::from(path),
                    file_name: path.to_string(),
                    parent_directory: None,
                    kind: FileKind::File,
                    size: None,
                    modified: None,
                    rank: FilenameRank::ExactFilename,
                    match_quality: FilenameRank::ExactFilename,
                    filename_match_ranges: Vec::new(),
                    path_match_ranges: Vec::new(),
                    arrival_index: 0,
                }),
            });
        }
        let mut rows = state.result_rows.clone();
        let keys_before: std::collections::HashSet<_> =
            rows.iter().map(|row| row.id.result_key.clone()).collect();

        rows.sort_by(|a, b| a.id.path.cmp(&b.id.path));
        let keys_after: std::collections::HashSet<_> =
            rows.iter().map(|row| row.id.result_key.clone()).collect();

        assert_eq!(keys_before, keys_after);
    }

    #[test]
    fn truncated_content_metadata_survives_flattening() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(3)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(3),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 3,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: true,
            }),
        });

        for row in state.selectable_result_rows() {
            match &row.payload {
                FileSearchRowPayload::Content {
                    content_file_truncated,
                    ..
                } => assert!(*content_file_truncated),
                _ => panic!("expected content row"),
            }
        }
    }

    #[test]
    fn only_final_displayed_match_for_truncated_file_is_marked() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(4)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(4),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 4,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                    ContentMatch::new(12, "third needle".into(), 8, 14),
                ],
                truncated: true,
            }),
        });

        let marked: Vec<_> = state
            .selectable_result_rows()
            .map(|row| match &row.payload {
                FileSearchRowPayload::Content {
                    is_last_displayed_match_from_truncated_file,
                    ..
                } => *is_last_displayed_match_from_truncated_file,
                _ => panic!("expected content row"),
            })
            .collect();

        assert_eq!(marked, vec![false, false, true]);
    }

    #[test]
    fn non_truncated_content_rows_are_not_marked() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(5)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(5),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: false,
            }),
        });

        for row in state.selectable_result_rows() {
            match &row.payload {
                FileSearchRowPayload::Content {
                    content_file_truncated,
                    is_last_displayed_match_from_truncated_file,
                    ..
                } => {
                    assert!(!*content_file_truncated);
                    assert!(!*is_last_displayed_match_from_truncated_file);
                }
                _ => panic!("expected content row"),
            }
        }
    }

    #[test]
    fn omitted_indicator_id_differs_from_selectable_row_ids() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(6)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(6),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 3,
                matches: vec![ContentMatch::new(4, "first needle".into(), 6, 12)],
                truncated: true,
            }),
        });

        let row_id = &state.result_rows[1].id;
        let selectable_id = egui::Id::new(result_row_id_source(row_id.search_id, row_id));
        let omitted_id = egui::Id::new(omitted_matches_id_source(
            row_id.search_id,
            row_id.result_key.clone(),
            &row_id.path,
        ));

        assert_ne!(selectable_id, omitted_id);
    }

    #[test]
    fn multiple_matches_on_same_line_create_distinct_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "needle needle".into(), 0, 6),
                    ContentMatch::new(4, "needle needle".into(), 7, 13),
                ],
                truncated: false,
            }),
        });

        assert_eq!(state.result_rows.len(), 3);
        assert_eq!(state.result_rows[1].id.line_number, Some(4));
        assert_eq!(state.result_rows[2].id.line_number, Some(4));
        assert_eq!(state.result_rows[1].id.column, Some(0));
        assert_eq!(state.result_rows[2].id.column, Some(7));
        assert_ne!(state.result_rows[1].id, state.result_rows[2].id);
    }

    #[test]
    fn flattened_rows_preserve_backend_and_match_order() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        for (path, line) in [("b.rs", 20), ("a.rs", 10)] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(1),
                result: SearchResult::ContentFile(ContentFileResult {
                    path: path.into(),
                    file_name: path.to_string(),
                    modified: None,
                    filename_relevance: None,
                    arrival_index: 0,
                    total_matches: 1,
                    matches: vec![ContentMatch::new(line, format!("{path} needle"), 0, 6)],
                    truncated: false,
                }),
            });
        }

        assert_eq!(state.result_rows[0].id.path, PathBuf::from("b.rs"));
        assert_eq!(state.result_rows[1].id.path, PathBuf::from("b.rs"));
        assert_eq!(state.result_rows[2].id.path, PathBuf::from("a.rs"));
        assert_eq!(state.result_rows[3].id.path, PathBuf::from("a.rs"));
        assert_ne!(
            state.result_rows[1].id.result_key,
            state.result_rows[3].id.result_key
        );
    }

    #[test]
    fn flattened_content_row_retains_exact_path_line_column_and_content() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(7)),
            ..Default::default()
        };
        let content_match = ContentMatch::new(12, "hello needle".into(), 6, 12);
        state.apply_event(SearchEvent::Result {
            id: SearchId(7),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 1,
                matches: vec![content_match.clone()],
                truncated: false,
            }),
        });

        assert_eq!(state.result_rows.len(), 2);
        assert_eq!(state.result_rows[1].id.search_id, SearchId(7));
        assert_eq!(state.result_rows[1].id.path, PathBuf::from("src/lib.rs"));
        assert_eq!(state.result_rows[1].id.line_number, Some(12));
        assert_eq!(state.result_rows[1].id.column, Some(6));
        match &state.result_rows[1].payload {
            FileSearchRowPayload::Content {
                path,
                content_match: row_match,
                ..
            } => {
                assert_eq!(path, &PathBuf::from("src/lib.rs"));
                assert_eq!(row_match, &content_match);
            }
            _ => panic!("expected content row"),
        }
    }

    #[test]
    fn id_sources_distinguish_same_filename_in_different_directories() {
        let left = FileSearchResultRowId {
            search_id: SearchId(1),
            result_key: FileSearchResultKey::Filename {
                path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
            },
            match_index: None,
            path: "/tmp/a/main.rs".into(),
            line_number: None,
            column: None,
        };
        let right = FileSearchResultRowId {
            path: "/tmp/b/main.rs".into(),
            ..left.clone()
        };

        assert_ne!(
            result_row_id_source(SearchId(1), &left),
            result_row_id_source(SearchId(1), &right)
        );
    }

    #[test]
    fn id_sources_distinguish_two_matches_in_one_file() {
        let first = FileSearchResultRowId {
            search_id: SearchId(1),
            result_key: FileSearchResultKey::Filename {
                path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
            },
            match_index: Some(0),
            path: "src/lib.rs".into(),
            line_number: Some(4),
            column: Some(0),
        };
        let second = FileSearchResultRowId {
            match_index: Some(1),
            line_number: Some(8),
            column: Some(7),
            ..first.clone()
        };

        assert_ne!(
            result_row_id_source(SearchId(1), &first),
            result_row_id_source(SearchId(1), &second)
        );
    }

    #[test]
    fn id_sources_distinguish_two_matches_on_same_source_line() {
        let first = FileSearchResultRowId {
            search_id: SearchId(1),
            result_key: FileSearchResultKey::Filename {
                path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
            },
            match_index: Some(0),
            path: "src/lib.rs".into(),
            line_number: Some(4),
            column: Some(0),
        };
        let second = FileSearchResultRowId {
            match_index: Some(1),
            column: Some(7),
            ..first.clone()
        };

        assert_ne!(
            result_row_id_source(SearchId(1), &first),
            result_row_id_source(SearchId(1), &second)
        );
    }

    #[test]
    fn id_sources_distinguish_equivalent_rows_from_different_searches() {
        let row = FileSearchResultRowId {
            search_id: SearchId(1),
            result_key: FileSearchResultKey::Filename {
                path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
            },
            match_index: None,
            path: "src/lib.rs".into(),
            line_number: None,
            column: None,
        };
        let other_search_row = FileSearchResultRowId {
            search_id: SearchId(2),
            ..row.clone()
        };

        assert_ne!(
            result_row_id_source(SearchId(1), &row),
            result_row_id_source(SearchId(2), &other_search_row)
        );
    }

    #[test]
    fn preserving_dimensions() {
        let mut state = FileSearchDialogState::default();
        state.persisted_window_size = egui::vec2(900.0, 700.0);
        state.open_with_mode(FileSearchMode::Filename);
        assert_eq!(state.persisted_window_size, egui::vec2(900.0, 700.0));
    }

    #[test]
    fn requesting_preview_records_match_context() {
        let mut state = FileSearchDialogState {
            settings: FileSearchSettings {
                max_full_preview_file_size_bytes: 7 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };
        let path = PathBuf::from("src/lib.rs");
        let content_match = ContentMatch::new(12, "hello needle".into(), 6, 12);

        let request = state.request_preview(&path, Some(&content_match));

        assert_eq!(request.path, path);
        let selected_match = request.selected_match.as_ref().unwrap();
        assert_eq!(selected_match.line, 12);
        assert_eq!(selected_match.start_column, 7);
        assert_eq!(selected_match.source_line.as_deref(), Some("hello needle"));
        assert_eq!(selected_match.match_length, Some(6));
        assert_eq!(selected_match.end_column, Some(13));
        assert_eq!(request.max_bytes_full_file_preview, 7 * 1024 * 1024);
        assert_eq!(state.preview_dialog.current_request, Some(request));
    }

    #[test]
    fn content_row_editor_context_intent_uses_that_rows_line_and_column() {
        let row = FileSearchResultRow {
            id: FileSearchResultRowId {
                search_id: SearchId(1),
                result_key: FileSearchResultKey::Filename {
                    path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
                },
                match_index: Some(1),
                path: "src/lib.rs".into(),
                line_number: Some(42),
                column: Some(9),
            },
            payload: FileSearchRowPayload::Content {
                path: "src/lib.rs".into(),
                content_match: ContentMatch::new(42, "row-specific needle".into(), 9, 15),
                content_file_truncated: false,
                is_last_displayed_match_from_truncated_file: false,
            },
        };

        let intent = FileSearchDialogState::context_menu_intents_for_row(&row)
            .into_iter()
            .find(|intent| intent.action == FileSearchContextMenuAction::OpenInConfiguredEditor)
            .expect("editor intent should be available");

        assert_eq!(intent.path, PathBuf::from("src/lib.rs"));
        let intent_match = intent.content_match.as_ref().unwrap();
        assert_eq!(intent_match.line_number, 42);
        assert_eq!(intent_match.column.map(|column| column + 1), Some(10));
    }

    #[test]
    fn content_row_preview_context_intent_uses_that_rows_line_and_content() {
        let content_match = ContentMatch::new(8, "second row content".into(), 3, 9);
        let row = FileSearchResultRow {
            id: FileSearchResultRowId {
                search_id: SearchId(1),
                result_key: FileSearchResultKey::Filename {
                    path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
                },
                match_index: Some(1),
                path: "src/lib.rs".into(),
                line_number: Some(8),
                column: Some(3),
            },
            payload: FileSearchRowPayload::Content {
                path: "src/lib.rs".into(),
                content_match: content_match.clone(),
                content_file_truncated: false,
                is_last_displayed_match_from_truncated_file: false,
            },
        };

        let intent = FileSearchDialogState::context_menu_intents_for_row(&row)
            .into_iter()
            .find(|intent| intent.action == FileSearchContextMenuAction::Preview)
            .expect("preview intent should be available");

        assert_eq!(intent.path, PathBuf::from("src/lib.rs"));
        assert_eq!(intent.content_match, Some(content_match));
    }

    #[test]
    fn show_all_matches_context_intent_uses_the_rows_path() {
        let row = FileSearchResultRow {
            id: FileSearchResultRowId {
                search_id: SearchId(1),
                result_key: FileSearchResultKey::Filename {
                    path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
                },
                match_index: Some(0),
                path: "src/selected.rs".into(),
                line_number: Some(5),
                column: Some(1),
            },
            payload: FileSearchRowPayload::Content {
                path: "src/selected.rs".into(),
                content_match: ContentMatch::new(5, "selected needle".into(), 1, 7),
                content_file_truncated: false,
                is_last_displayed_match_from_truncated_file: false,
            },
        };

        let intent = FileSearchDialogState::context_menu_intents_for_row(&row)
            .into_iter()
            .find(|intent| intent.action == FileSearchContextMenuAction::ShowAllMatchesInThisFile)
            .expect("show all matches intent should be available");

        assert_eq!(intent.path, PathBuf::from("src/selected.rs"));
    }

    #[test]
    fn filename_context_menu_actions_remain_available_after_flattening() {
        let row = FileSearchResultRow {
            id: FileSearchResultRowId {
                search_id: SearchId(1),
                result_key: FileSearchResultKey::Filename {
                    path: PathIdentity::from_path(std::path::Path::new("/tmp/a/main.rs")),
                },
                match_index: None,
                path: "src/lib.rs".into(),
                line_number: None,
                column: None,
            },
            payload: FileSearchRowPayload::Filename {
                path: "src/lib.rs".into(),
                display_filename: "lib.rs".into(),
                parent_directory_display: "src".into(),
                kind: FileKind::File,
                size: None,
                modified: None,
                rank: crate::file_search::model::FilenameRank::FilenameContains,
                match_quality: crate::file_search::model::FilenameRank::FilenameContains,
                filename_match_ranges: Vec::new(),
                path_match_ranges: Vec::new(),
            },
        };
        let actions: Vec<_> = FileSearchDialogState::context_menu_intents_for_row(&row)
            .into_iter()
            .map(|intent| intent.action)
            .collect();

        assert!(actions.contains(&FileSearchContextMenuAction::OpenInConfiguredEditor));
        assert!(actions.contains(&FileSearchContextMenuAction::Preview));
        assert!(actions.contains(&FileSearchContextMenuAction::OpenFileOrDirectory));
        assert!(actions.contains(&FileSearchContextMenuAction::RevealInExplorer));
        assert!(actions.contains(&FileSearchContextMenuAction::OpenContainingDirectory));
        assert!(actions.contains(&FileSearchContextMenuAction::CopyFullPath));
        assert!(actions.contains(&FileSearchContextMenuAction::CopyFilename));
        assert!(actions.contains(&FileSearchContextMenuAction::OpenTerminal));
        assert!(actions.contains(&FileSearchContextMenuAction::NestedFilenameSearch));
        assert!(actions.contains(&FileSearchContextMenuAction::NestedContentSearch));
        assert!(!actions.contains(&FileSearchContextMenuAction::ShowAllMatchesInThisFile));
    }

    #[test]
    fn context_menu_nested_search_uses_file_parent_or_directory_itself() {
        let mut state = FileSearchDialogState {
            search_text: "needle".into(),
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        state.start_nested_search(
            std::path::Path::new("/tmp/project/file.txt"),
            false,
            FileSearchMode::Filename,
            &mut coordinator,
        );
        assert_eq!(state.custom_roots, vec!["/tmp/project".to_string()]);

        state.start_nested_search(
            std::path::Path::new("/tmp/project/src"),
            true,
            FileSearchMode::Content,
            &mut coordinator,
        );
        assert_eq!(state.custom_roots, vec!["/tmp/project/src".to_string()]);
        assert_eq!(state.selected_mode, FileSearchMode::Content);
    }
    fn filename_search_result(path: &str) -> SearchResult {
        filename_search_result_with_kind(path, FileKind::File)
    }

    fn filename_search_result_with_kind(path: &str, kind: FileKind) -> SearchResult {
        let path_buf = PathBuf::from(path);
        SearchResult::Filename(FilenameResult {
            file_name: path_buf
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            parent_directory: path_buf.parent().map(PathBuf::from),
            path: path_buf,
            kind,
            size: None,
            modified: None,
            rank: FilenameRank::ExactFilename,
            match_quality: FilenameRank::ExactFilename,
            filename_match_ranges: Vec::new(),
            path_match_ranges: Vec::new(),
            arrival_index: 0,
        })
    }

    fn content_search_result(
        path: &str,
        total_matches: usize,
        displayed_matches: usize,
        truncated: bool,
    ) -> SearchResult {
        SearchResult::ContentFile(ContentFileResult {
            path: path.into(),
            file_name: std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string()),
            modified: None,
            filename_relevance: None,
            arrival_index: 0,
            total_matches,
            matches: (0..displayed_matches)
                .map(|index| ContentMatch::new(index + 1, format!("line {index} needle"), 5, 11))
                .collect(),
            truncated,
        })
    }

    #[test]
    fn sorting_is_deferred_while_running_and_applied_on_completion() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: filename_search_result("/tmp/b.txt"),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: filename_search_result("/tmp/a.txt"),
        });
        state.ui_preferences.filename_sort = FileSearchFilenameSort::FilenameAscending;
        state.handle_sort_changed();
        assert!(state.pending_sort_change);
        assert_eq!(state.result_rows[0].id.path, PathBuf::from("/tmp/b.txt"));

        state.apply_event(SearchEvent::Completed { id: SearchId(1) });

        assert!(!state.pending_sort_change);
        assert_eq!(state.result_rows[0].id.path, PathBuf::from("/tmp/a.txt"));
    }

    #[test]
    fn selection_survives_sorting_after_completion() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(2)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(2),
            result: filename_search_result("/tmp/b.txt"),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(2),
            result: filename_search_result("/tmp/a.txt"),
        });
        let selected = state.result_rows[0].clone();
        state.select_result(&selected);
        state.ui_preferences.filename_sort = FileSearchFilenameSort::FilenameAscending;
        state.apply_event(SearchEvent::Completed { id: SearchId(2) });
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/b.txt")
        );
    }

    #[test]
    fn duplicate_filename_paths_produce_one_row_on_completion() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(3)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(3),
            result: filename_search_result("/tmp/dup.txt"),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(3),
            result: filename_search_result("/tmp/dup.txt"),
        });
        assert_eq!(state.filename_row_count(), 2);
        state.apply_event(SearchEvent::Completed { id: SearchId(3) });
        assert_eq!(state.filename_row_count(), 1);
    }

    #[test]
    fn duplicate_content_matches_produce_one_row_on_completion() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(4)),
            current_status: SearchStatus::Running,
            selected_mode: FileSearchMode::Content,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(4),
            result: content_search_result("/tmp/dup.txt", 1, 1, false),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(4),
            result: content_search_result("/tmp/dup.txt", 1, 1, true),
        });
        state.apply_event(SearchEvent::Completed { id: SearchId(4) });
        assert_eq!(state.content_matched_file_count(), 1);
        assert_eq!(state.content_displayed_match_row_count(), 1);
        assert_eq!(state.content_truncated_displayed_match_count(), 1);
    }

    #[test]
    fn repeated_completed_sorting_is_deterministic() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(5)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        for path in ["/tmp/c.txt", "/tmp/a.txt", "/tmp/b.txt"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(5),
                result: filename_search_result(path),
            });
        }
        state.ui_preferences.filename_sort = FileSearchFilenameSort::FilenameAscending;
        state.apply_event(SearchEvent::Completed { id: SearchId(5) });
        let first = state.result_rows.clone();
        state.handle_sort_changed();
        state.handle_sort_changed();
        assert_eq!(state.result_rows, first);
    }

    #[test]
    fn filename_row_count_counts_displayed_filename_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        for path in ["/tmp/a.txt", "/tmp/b.txt"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(1),
                result: filename_search_result(path),
            });
        }

        assert_eq!(state.filename_row_count(), 2);
        assert_eq!(state.result_counts().filename_rows, 2);
    }

    #[test]
    fn content_matched_file_count_counts_grouped_files() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        for path in ["src/lib.rs", "src/main.rs"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(1),
                result: content_search_result(path, 3, 2, false),
            });
        }

        assert_eq!(state.content_matched_file_count(), 2);
        assert_eq!(state.result_counts().content_matched_files, 2);
    }

    #[test]
    fn content_displayed_match_row_count_counts_selectable_rows() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result("src/lib.rs", 3, 2, false),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result("src/main.rs", 1, 1, false),
        });

        assert_eq!(state.content_displayed_match_row_count(), 3);
        assert_eq!(state.result_counts().content_displayed_match_rows, 3);
    }

    #[test]
    fn truncated_displayed_match_count_counts_truncated_file_markers() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result("src/lib.rs", 10, 3, true),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result("src/main.rs", 2, 2, false),
        });

        assert_eq!(state.content_truncated_displayed_match_count(), 1);
        assert_eq!(state.result_counts().content_truncated_displayed_matches, 1);
    }

    fn state_with_selected_filename(path: &str) -> FileSearchDialogState {
        state_with_selected_filename_kind(path, FileKind::File)
    }

    fn state_with_selected_filename_kind(path: &str, kind: FileKind) -> FileSearchDialogState {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: filename_search_result_with_kind(path, kind),
        });
        let row = state.result_rows[0].clone();
        state.select_result(&row);
        state
    }

    fn state_with_selected_content(path: &str) -> FileSearchDialogState {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            selected_mode: FileSearchMode::Content,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result(path, 1, 1, false),
        });
        let row = state.result_rows[0].clone();
        state.select_result(&row);
        state
    }

    #[test]
    fn keyboard_next_previous_selection_clamps_at_boundaries() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(9)),
            ..Default::default()
        };
        for path in ["/tmp/a.txt", "/tmp/b.txt"] {
            state.apply_event(SearchEvent::Result {
                id: SearchId(9),
                result: filename_search_result(path),
            });
        }

        state.move_selection(keyboard::SelectionMove::Next);
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/a.txt")
        );
        state.move_selection(keyboard::SelectionMove::Next);
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/b.txt")
        );
        state.move_selection(keyboard::SelectionMove::Next);
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/b.txt")
        );
        state.move_selection(keyboard::SelectionMove::Previous);
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/a.txt")
        );
        state.move_selection(keyboard::SelectionMove::Previous);
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/a.txt")
        );
    }

    #[test]
    fn keyboard_activation_uses_selected_row_key_after_reordering() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(10)),
            current_status: SearchStatus::Running,
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(10),
            result: filename_search_result("/tmp/b.txt"),
        });
        state.apply_event(SearchEvent::Result {
            id: SearchId(10),
            result: filename_search_result("/tmp/a.txt"),
        });
        let selected_key = state.result_rows[0].id.result_key.clone();
        let selected_row = state.result_rows[0].clone();
        state.select_result(&selected_row);
        state.ui_preferences.filename_sort = FileSearchFilenameSort::FilenameAscending;
        state.apply_event(SearchEvent::Completed { id: SearchId(10) });

        assert_eq!(
            state.selected_result().unwrap().row_id.result_key,
            selected_key
        );
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("/tmp/b.txt")
        );
        let action = state.resolve_open_selected_result_action();
        assert!(action.is_none());
        assert!(
            state
                .warning_error_message
                .as_deref()
                .unwrap()
                .contains("/tmp/b.txt")
        );
    }

    #[test]
    fn escape_cancels_running_search_state() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(11)),
            current_status: SearchStatus::Running,
            open: true,
            ..Default::default()
        };
        let mut coordinator = SearchCoordinator::new();

        assert_eq!(
            state.handle_escape(&mut coordinator),
            FileSearchEscapeAction::Cancel
        );
        assert_eq!(state.current_status, SearchStatus::Cancelled);
        assert!(state.open);
    }

    #[test]
    fn copy_shortcuts_build_expected_payloads() {
        let state = state_with_selected_content("/tmp/match.txt");

        assert_eq!(
            state.copy_selected_path_payload().as_deref(),
            Some("/tmp/match.txt")
        );
        assert_eq!(
            state.copy_selected_match_line_payload().as_deref(),
            Some("line 0 needle")
        );
        assert_eq!(
            state.copy_all_visible_results_payload().as_deref(),
            Some("/tmp/match.txt:1:6: line 0 needle")
        );
    }

    #[test]
    fn open_unavailable_with_no_selection() {
        let state = FileSearchDialogState::default();

        assert!(!state.can_open_selected_result());
    }

    #[test]
    fn filename_file_selection_resolves_to_reveal_behavior() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("needle.txt");
        std::fs::write(&file_path, "needle").unwrap();
        let mut state = state_with_selected_filename_kind(
            &file_path.display().to_string(),
            FileKind::Directory,
        );

        let action = state.resolve_open_selected_result_action();

        assert_eq!(
            action,
            Some(OpenSelectedFileSearchResultAction::Explorer(
                ExplorerAction::RevealFile(file_path)
            ))
        );
        assert!(state.warning_error_message.is_none());
    }

    #[test]
    fn filename_directory_selection_resolves_to_directory_open_behavior() {
        let temp = tempfile::tempdir().unwrap();
        let dir_path = temp.path().join("subdir");
        std::fs::create_dir(&dir_path).unwrap();
        let mut state =
            state_with_selected_filename_kind(&dir_path.display().to_string(), FileKind::File);

        let action = state.resolve_open_selected_result_action();

        assert_eq!(
            action,
            Some(OpenSelectedFileSearchResultAction::Explorer(
                ExplorerAction::OpenDirectory(dir_path)
            ))
        );
        assert!(state.warning_error_message.is_none());
    }

    #[test]
    fn missing_path_preserves_selection_and_returns_visible_error() {
        let temp = tempfile::tempdir().unwrap();
        let missing_path = temp.path().join("missing.txt");
        let mut state = state_with_selected_filename(&missing_path.display().to_string());
        let selected_before = state.selected_result().cloned();

        let action = state.resolve_open_selected_result_action();

        assert!(action.is_none());
        assert_eq!(state.selected_result().cloned(), selected_before);
        assert!(
            state
                .warning_error_message
                .as_deref()
                .is_some_and(|message| message.contains("missing or inaccessible"))
        );
    }

    #[test]
    fn content_selection_resolves_to_preview_behavior() {
        let path = "src/lib.rs";
        let mut state = state_with_selected_content(path);
        state.settings.max_full_preview_file_size_bytes = 9 * 1024 * 1024;

        let action = state.resolve_open_selected_result_action();

        match action {
            Some(OpenSelectedFileSearchResultAction::PreviewContent { request }) => {
                assert_eq!(request.path, PathBuf::from(path));
                assert_eq!(request.selected_match.as_ref().unwrap().line, 1);
                assert_eq!(request.max_bytes_full_file_preview, 9 * 1024 * 1024);
                assert_eq!(state.preview_dialog.current_request, Some(request));
            }
            other => panic!("expected preview action, got {other:?}"),
        }
    }

    #[test]
    fn double_click_uses_same_dispatch_path_as_open() {
        let path = "src/lib.rs";
        let mut open_button_state = state_with_selected_content(path);
        open_button_state.settings.max_full_preview_file_size_bytes = 11 * 1024 * 1024;
        let mut double_click_state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            selected_mode: FileSearchMode::Content,
            settings: FileSearchSettings {
                max_full_preview_file_size_bytes: 11 * 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };
        double_click_state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: content_search_result(path, 1, 1, false),
        });
        let double_clicked_row = double_click_state.result_rows[0].clone();

        let open_action = open_button_state.open_selected_result();
        let double_click_action = double_click_state.open_result_row(&double_clicked_row);

        assert_eq!(double_click_action, open_action);
        assert_eq!(
            double_click_state.preview_dialog.current_request,
            open_button_state.preview_dialog.current_request
        );
    }

    #[test]
    fn starting_a_search_clears_selection() {
        let mut state = state_with_selected_filename("/tmp/a.txt");
        state.search_text = "needle".into();
        let mut coordinator = SearchCoordinator::new();

        state.start_search(&mut coordinator);

        assert!(state.selected_result().is_none());
        assert!(state.result_rows.is_empty());
    }

    #[test]
    fn switching_filename_content_mode_clears_selection() {
        let mut state = state_with_selected_filename("/tmp/a.txt");
        let previous_mode = state.selected_mode;

        state.selected_mode = FileSearchMode::Content;
        state.clear_selection_if_mode_changed(previous_mode);

        assert_eq!(previous_mode, FileSearchMode::Filename);
        assert_eq!(state.selected_mode, FileSearchMode::Content);
        assert!(state.selected_result().is_none());
    }

    #[test]
    fn repaint_without_mode_change_preserves_selection() {
        let mut state = state_with_selected_filename("/tmp/a.txt");
        let selected_before = state.selected_result().cloned();
        let previous_mode = state.selected_mode;

        state.clear_selection_if_mode_changed(previous_mode);

        assert_eq!(state.selected_result().cloned(), selected_before);
    }

    #[test]
    fn appending_new_results_preserves_valid_selection() {
        let mut state = state_with_selected_filename("/tmp/a.txt");
        let selected_before = state.selected_result().cloned();

        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: filename_search_result("/tmp/b.txt"),
        });

        assert_eq!(state.result_rows.len(), 2);
        assert_eq!(state.selected_result().cloned(), selected_before);
    }

    #[test]
    fn invalid_stale_selected_row_is_cleared_after_validation() {
        let mut state = state_with_selected_filename("/tmp/a.txt");
        state.result_rows.clear();

        assert!(!state.selection_is_valid());

        assert!(state.selected_result().is_none());
    }

    #[test]
    fn enter_key_intent_opens_selected_row_by_key() {
        let ctx = egui::Context::default();
        let mut state = state_with_selected_filename("/tmp/a.txt");
        state.open = true;
        state.current_status = SearchStatus::Completed;
        let selected_before = state.selected_result().cloned();
        let mut coordinator = SearchCoordinator::new();

        ctx.begin_frame(egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        state.ui(&ctx, &mut coordinator);
        let _ = ctx.end_frame();

        assert_eq!(state.selected_result().cloned(), selected_before);
        assert_eq!(coordinator.diagnostics().started, 0);
        assert!(
            state
                .warning_error_message
                .as_deref()
                .is_some_and(|message| message.contains("/tmp/a.txt"))
        );
    }

    #[test]
    fn fallback_warning_does_not_mark_successful_search_failed() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(99)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::BackendFallback {
            id: SearchId(99),
            from: SearchBackend::Ripgrep,
            to: SearchBackend::Native,
            reason: "not found".to_string(),
        });
        state.apply_event(SearchEvent::Completed { id: SearchId(99) });
        assert_eq!(state.current_status, SearchStatus::Completed);
        assert!(
            state
                .warning_error_message
                .as_deref()
                .is_some_and(|message| message.contains("Native content search is being used"))
        );
    }

    #[test]
    fn dismissal_suppresses_repeated_ripgrep_missing_prompts_during_session() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(42)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::BackendFallback {
            id: SearchId(42),
            from: SearchBackend::Ripgrep,
            to: SearchBackend::Native,
            reason: "ripgrep missing".to_owned(),
        });
        assert!(state.show_ripgrep_missing_prompt);
        state.dismiss_ripgrep_missing_prompt();
        state.apply_event(SearchEvent::BackendFallback {
            id: SearchId(42),
            from: SearchBackend::Ripgrep,
            to: SearchBackend::Native,
            reason: "ripgrep still missing".to_owned(),
        });
        assert!(!state.show_ripgrep_missing_prompt);
        assert!(state.ripgrep_missing_prompt_dismissed);
    }
    #[test]
    fn content_display_rows_have_one_header_per_file_and_total_match_count() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(9)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(9),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/file_search/ripgrep.rs".into(),
                file_name: "ripgrep.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 7,
                matches: vec![
                    ContentMatch::new(10, "needle".into(), 0, 6),
                    ContentMatch::new(20, "needle".into(), 0, 6),
                ],
                truncated: false,
            }),
        });
        let headers: Vec<_> = state
            .result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::ContentGroupHeader {
                    header,
                    total_matches,
                    ..
                } => Some((header, *total_matches)),
                _ => None,
            })
            .collect();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].1, 7);
        assert!(headers[0].0.contains("7 matches"));
        assert_eq!(state.content_displayed_match_row_count(), 2);
    }

    #[test]
    fn content_matches_stay_under_correct_headers_for_duplicate_filenames() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(10)),
            ..Default::default()
        };
        for (idx, dir, line) in [(0, "src/a", 3), (1, "src/b", 9)] {
            let path = PathBuf::from(dir).join("main.rs");
            state.apply_event(SearchEvent::Result {
                id: SearchId(10),
                result: SearchResult::ContentFile(ContentFileResult {
                    path,
                    file_name: "main.rs".into(),
                    modified: None,
                    filename_relevance: None,
                    arrival_index: idx,
                    total_matches: 1,
                    matches: vec![ContentMatch::new(line, "needle".into(), 0, 6)],
                    truncated: false,
                }),
            });
        }
        assert!(matches!(
            state.result_rows[0].payload,
            FileSearchRowPayload::ContentGroupHeader { .. }
        ));
        assert_eq!(state.result_rows[0].id.path, PathBuf::from("src/a/main.rs"));
        assert_eq!(state.result_rows[1].id.path, PathBuf::from("src/a/main.rs"));
        assert!(matches!(
            state.result_rows[2].payload,
            FileSearchRowPayload::ContentGroupHeader { .. }
        ));
        assert_eq!(state.result_rows[2].id.path, PathBuf::from("src/b/main.rs"));
        assert_eq!(state.result_rows[3].id.path, PathBuf::from("src/b/main.rs"));
        assert_ne!(
            state.result_rows[0].id.result_key,
            state.result_rows[2].id.result_key
        );
    }

    #[test]
    fn truncated_indicator_appears_once_per_group() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(11)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(11),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 5,
                matches: vec![
                    ContentMatch::new(1, "needle".into(), 0, 6),
                    ContentMatch::new(2, "needle".into(), 0, 6),
                ],
                truncated: true,
            }),
        });
        let count = state
            .result_rows
            .iter()
            .filter(|row| match &row.payload {
                FileSearchRowPayload::ContentGroupHeader { header, .. } => {
                    header.contains("truncated")
                }
                _ => false,
            })
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn selected_content_match_survives_group_reorder() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(12)),
            current_status: SearchStatus::Completed,
            ..Default::default()
        };
        for (idx, path, count) in [(0, "b.rs", 1), (1, "a.rs", 9)] {
            state
                .results
                .push(SearchResult::ContentFile(ContentFileResult {
                    path: path.into(),
                    file_name: path.into(),
                    modified: None,
                    filename_relevance: None,
                    arrival_index: idx,
                    total_matches: count,
                    matches: vec![ContentMatch::new(1, format!("{path} needle"), 0, 6)],
                    truncated: false,
                }));
        }
        state.rebuild_result_rows();
        let selected = state
            .result_rows
            .iter()
            .find(|row| {
                row.id.path == PathBuf::from("b.rs")
                    && matches!(row.payload, FileSearchRowPayload::Content { .. })
            })
            .unwrap()
            .clone();
        let selected_key = selected.id.result_key.clone();
        state.select_result(&selected);
        state.ui_preferences.content_sort = FileSearchContentSort::MatchCountDescending;
        state.finalize_completed_results();
        assert_eq!(
            state.selected_result().unwrap().row_id.result_key,
            selected_key
        );
        assert_eq!(
            state.selected_result().unwrap().row_id.path,
            PathBuf::from("b.rs")
        );
    }

    #[test]
    fn keyboard_navigation_skips_content_group_headers() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(13)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(13),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                file_name: "lib.rs".into(),
                modified: None,
                filename_relevance: None,
                arrival_index: 0,
                total_matches: 1,
                matches: vec![ContentMatch::new(8, "needle".into(), 0, 6)],
                truncated: false,
            }),
        });
        state.move_selection(crate::gui::file_search_dialog::keyboard::SelectionMove::Next);
        assert!(matches!(
            state.selected_result().unwrap().payload,
            SelectedFileSearchResultPayload::Content { .. }
        ));
        assert_eq!(state.selected_result().unwrap().row_id.line_number, Some(8));
    }
}
