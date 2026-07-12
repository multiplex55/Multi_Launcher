use crate::actions::clipboard;
use crate::file_search::actions::{
    containing_directory, copied_filename, execute_explorer_action, nested_search_root,
    open_configured_terminal_in_directory, open_in_configured_editor, open_path,
    resolve_explorer_action, ExplorerAction, InvocationTarget,
};
use crate::file_search::coordinator::{event_id, SearchCoordinator};
use crate::file_search::model::{
    ContentMatch, FileKind, SearchBackend, SearchEvent, SearchId, SearchKind, SearchRequest,
    SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::preview::PreviewRequest;
use crate::file_search::settings::FileSearchSettings;
use crate::gui::file_search_preview_dialog::FileSearchPreviewDialogState;
use eframe::egui::{self, WidgetText};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(760.0, 560.0);
const ACTIVE_SEARCH_REPAINT_INTERVAL: Duration = Duration::from_millis(50);

pub const FILE_SEARCH_SEARCH_FIELD_ID_SOURCE: &str = "file_search_search_text";
pub const FILE_SEARCH_ROOT_FIELD_ID_SOURCE: &str = "file_search_root_directory";

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
    pub backend_result_index: usize,
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
    backend_result_index: usize,
    path: &std::path::Path,
) -> (&'static str, SearchId, usize, PathBuf, &'static str) {
    (
        "file_search_result_row",
        search_id,
        backend_result_index,
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
    Filename {
        path: PathBuf,
        display_filename: String,
        parent_directory_display: String,
        kind: FileKind,
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
    pub root_directory: String,
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
    pub preview_dialog: FileSearchPreviewDialogState,
    pub persisted_window_size: egui::Vec2,
    pub settings: FileSearchSettings,
    pub request_search_focus: bool,
    pub request_immediate_repaint: bool,
}

impl Default for FileSearchDialogState {
    fn default() -> Self {
        Self {
            open: false,
            selected_mode: FileSearchMode::Filename,
            selected_scope: FileSearchScopeMode::Global,
            search_text: String::new(),
            root_directory: String::new(),
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
            preview_dialog: FileSearchPreviewDialogState::default(),
            persisted_window_size: DEFAULT_WINDOW_SIZE,
            settings: FileSearchSettings::default(),
            request_search_focus: false,
            request_immediate_repaint: false,
        }
    }
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
            self.root_directory = root.display().to_string();
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
        let text = self.search_text.trim();
        if text.is_empty() {
            self.warning_error_message = Some("Enter search text before searching.".to_string());
            return None;
        }
        let scope = match self.selected_scope {
            FileSearchScopeMode::Global => SearchScope::Global,
            FileSearchScopeMode::Directory => {
                let root = self.root_directory.trim();
                if root.is_empty() {
                    self.warning_error_message = Some("Choose a root directory first.".to_string());
                    return None;
                }
                SearchScope::Directory {
                    root: PathBuf::from(root),
                }
            }
        };
        let request = SearchRequest {
            kind: self.selected_mode.into(),
            scope,
            text: text.to_string(),
            case_sensitive: self.case_sensitive,
            include_hidden_files: self.include_hidden,
            max_results: self.settings.max_search_results.max(1),
            max_file_size_bytes: self.settings.max_content_search_file_size_bytes.max(1),
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: self.settings.excluded_directory_names.clone(),
        };
        self.clear_results_and_selection();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend_with_settings(
            &request,
            Some(&self.settings),
        ));
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
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
        let observed_terminal_event = match event {
            SearchEvent::Started { backend, .. } => {
                self.backend = Some(backend);
                self.current_status = SearchStatus::Running;
                false
            }
            SearchEvent::Result { result, .. } => {
                self.push_result(result);
                false
            }
            SearchEvent::Progress { progress, .. } => {
                self.inaccessible_path_warnings = progress
                    .directories_scanned
                    .saturating_sub(progress.files_scanned)
                    .try_into()
                    .unwrap_or(usize::MAX);
                false
            }
            SearchEvent::Completed { .. } => {
                self.current_status = SearchStatus::Completed;
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
        let backend_result_index = self.results.len();
        let search_id = self.active_search_id.unwrap_or(SearchId(0));
        match &result {
            SearchResult::Filename(item) => {
                self.result_rows.push(FileSearchResultRow {
                    id: FileSearchResultRowId {
                        search_id,
                        backend_result_index,
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
                    },
                });
            }
            SearchResult::ContentFile(content) => {
                let last_displayed_match_index = content.matches.len().checked_sub(1);
                for (match_index, content_match) in content.matches.iter().cloned().enumerate() {
                    self.result_rows.push(FileSearchResultRow {
                        id: FileSearchResultRowId {
                            search_id,
                            backend_result_index,
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
        self.results.push(result);
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

    fn contents(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
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
        ui.horizontal(|ui| {
            ui.label("Root");
            let root_response = ui.add(
                egui::TextEdit::singleline(&mut self.root_directory)
                    .id_source(Self::root_field_id()),
            );
            if root_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.start_search(coordinator);
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                ui.ctx().request_repaint();
            }
            if ui.button("Pick…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.root_directory = path.display().to_string();
                    self.selected_scope = FileSearchScopeMode::Directory;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.case_sensitive, "Case-sensitive");
            ui.checkbox(&mut self.include_hidden, "Include hidden");
            if ui.button("Search").clicked() {
                if self.start_search(coordinator).is_some() {
                    ui.ctx().request_repaint();
                }
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
        });
        ui.label(format!(
            "Backend: {} | Status: {:?} | {} | Inaccessible-path warnings: {}",
            self.backend
                .map(|b| format!("{b:?}"))
                .unwrap_or_else(|| "not selected".to_string()),
            self.current_status,
            self.result_count_status_text(),
            self.inaccessible_path_warnings
        ));
        if let Some(msg) = &self.warning_error_message {
            ui.colored_label(egui::Color32::YELLOW, msg);
        }
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

    fn filename_results(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        let rows: Vec<_> = self
            .result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Filename {
                    path,
                    display_filename,
                    parent_directory_display,
                    kind,
                } => Some((
                    row.id.clone(),
                    path.clone(),
                    display_filename.clone(),
                    parent_directory_display.clone(),
                    *kind,
                )),
                FileSearchRowPayload::Content { .. } => None,
            })
            .collect();
        for (row_id, path, display_filename, parent_directory_display, kind) in rows {
            ui.push_id(result_row_id_source(row_id.search_id, &row_id), |ui| {
                let row_text = format!(
                    "{} | {:?} | {}",
                    display_filename, kind, parent_directory_display
                );
                let is_selected = self
                    .selected_result()
                    .map(|selected| selected.row_id == row_id)
                    .unwrap_or(false);
                let response = non_wrapping_selectable_label(ui, is_selected, row_text)
                    .on_hover_text(path.display().to_string());
                let double_clicked = response.double_clicked();
                if response.clicked() {
                    self.select_result(&FileSearchResultRow {
                        id: row_id.clone(),
                        payload: FileSearchRowPayload::Filename {
                            path: path.clone(),
                            display_filename: display_filename.clone(),
                            parent_directory_display: parent_directory_display.clone(),
                            kind,
                        },
                    });
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
                    self.open_result_row(&FileSearchResultRow {
                        id: row_id.clone(),
                        payload: FileSearchRowPayload::Filename {
                            path: path.clone(),
                            display_filename: display_filename.clone(),
                            parent_directory_display: parent_directory_display.clone(),
                            kind,
                        },
                    });
                }
            });
        }
    }

    fn clear_selection_if_mode_changed(&mut self, previously_selected_mode: FileSearchMode) {
        if self.selected_mode != previously_selected_mode {
            self.clear_selection();
        }
    }

    fn content_results(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        let rows: Vec<_> = self
            .result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Content {
                    path,
                    content_match,
                    content_file_truncated,
                    is_last_displayed_match_from_truncated_file,
                } => Some((
                    row.id.clone(),
                    path.clone(),
                    content_match.clone(),
                    *content_file_truncated,
                    *is_last_displayed_match_from_truncated_file,
                )),
                FileSearchRowPayload::Filename { .. } => None,
            })
            .collect();
        for (
            row_id,
            path,
            content_match,
            content_file_truncated,
            is_last_displayed_match_from_truncated_file,
        ) in rows
        {
            ui.push_id(result_row_id_source(row_id.search_id, &row_id), |ui| {
                let column = content_match
                    .column
                    .map(|column| format!(":{}", column.saturating_add(1)))
                    .unwrap_or_default();
                let row_text = format!(
                    "{} | {}{} | {}{}",
                    path.display(),
                    content_match.line_number,
                    column,
                    content_match.line,
                    if is_last_displayed_match_from_truncated_file {
                        " … truncated"
                    } else {
                        ""
                    }
                );
                let is_selected = self
                    .selected_result()
                    .map(|selected| selected.row_id == row_id)
                    .unwrap_or(false);
                let response = non_wrapping_selectable_label(ui, is_selected, row_text)
                    .on_hover_text(path.display().to_string());
                let double_clicked = response.double_clicked();
                if response.clicked() {
                    self.select_result(&FileSearchResultRow {
                        id: row_id.clone(),
                        payload: FileSearchRowPayload::Content {
                            path: path.clone(),
                            content_match: content_match.clone(),
                            content_file_truncated,
                            is_last_displayed_match_from_truncated_file,
                        },
                    });
                }
                response.context_menu(|ui| {
                    self.content_result_context_menu(ui, &path, content_match.clone(), coordinator)
                });
                if double_clicked {
                    self.open_result_row(&FileSearchResultRow {
                        id: row_id.clone(),
                        payload: FileSearchRowPayload::Content {
                            path: path.clone(),
                            content_match: content_match.clone(),
                            content_file_truncated,
                            is_last_displayed_match_from_truncated_file,
                        },
                    });
                }
            });
            if is_last_displayed_match_from_truncated_file {
                ui.push_id(
                    omitted_matches_id_source(row_id.search_id, row_id.backend_result_index, &path),
                    |ui| {
                        ui.label(
                            egui::RichText::new("Additional matches in this file were omitted.")
                                .weak(),
                        );
                    },
                );
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
        let text = self.search_text.trim();
        if text.is_empty() {
            self.warning_error_message =
                Some("Enter search text before searching this file.".to_string());
            return;
        }
        let request = SearchRequest {
            kind: SearchKind::Content,
            scope: SearchScope::File {
                path: path.to_path_buf(),
            },
            text: text.to_string(),
            case_sensitive: self.case_sensitive,
            include_hidden_files: self.include_hidden,
            max_results: 1,
            max_file_size_bytes: self.settings.max_content_search_file_size_bytes.max(1),
            included_extensions: Vec::new(),
            excluded_extensions: Vec::new(),
            excluded_directory_names: Vec::new(),
        };
        self.selected_mode = FileSearchMode::Content;
        self.clear_results_and_selection();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend_with_settings(
            &request,
            Some(&self.settings),
        ));
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
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
        if let Some(content_match) = first_match.as_ref() {
            if ui.button("Copy matching line").clicked() {
                let line = content_match.line.clone();
                self.run_result_action("copy matching line", || {
                    clipboard::set_text(&line)?;
                    Ok(())
                });
                ui.close_menu();
            }
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
                self.root_directory = root.display().to_string();
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

fn non_wrapping_selectable_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<WidgetText>,
) -> egui::Response {
    let text = text.into();
    let button_padding = ui.spacing().button_padding;
    let total_extra = button_padding + button_padding;
    let galley = text.into_galley(ui, Some(false), f32::INFINITY, egui::TextStyle::Button);
    let mut desired_size = total_extra + galley.size();
    desired_size.y = desired_size.y.max(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_at_least(desired_size, egui::Sense::click());

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, selected, galley.text())
    });

    if ui.is_rect_visible(response.rect) {
        let text_pos = ui
            .layout()
            .align_size_within_rect(galley.size(), rect.shrink2(button_padding))
            .min;
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.highlighted() || response.has_focus() {
            let rect = rect.expand(visuals.expansion);
            ui.painter().rect(
                rect,
                visuals.rounding,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
            );
        }
        ui.painter().galley(text_pos, galley, visuals.text_color());
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{
        ContentFileResult, ContentMatch, FileKind, FilenameRank, FilenameResult, SearchProgress,
    };

    #[test]
    fn opening_with_mode_preselected() {
        let mut state = FileSearchDialogState::default();
        state.open_with_mode(FileSearchMode::Content);
        assert!(state.open);
        assert_eq!(state.selected_mode, FileSearchMode::Content);
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
    fn global_filename_search_uses_ripgrep_backend_when_everything_is_disabled() {
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

        assert_eq!(state.backend, Some(SearchBackend::Ripgrep));
        assert_eq!(coordinator.last_backend(), Some(SearchBackend::Ripgrep));
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
        assert_eq!(coordinator.diagnostics().started, 1);
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
            }),
        });
        state.apply_event(SearchEvent::Progress {
            id: SearchId(7),
            progress: SearchProgress {
                files_scanned: 2,
                directories_scanned: 5,
                results_found: 1,
                status: SearchStatus::Running,
            },
        });
        state.apply_event(SearchEvent::Completed { id: SearchId(7) });
        assert_eq!(state.backend, Some(SearchBackend::WalkDir));
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.inaccessible_path_warnings, 3);
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
                }),
            });
        }

        assert_eq!(state.result_rows.len(), 2);
        assert_ne!(state.result_rows[0].id.path, state.result_rows[1].id.path);
        assert_ne!(
            state.result_rows[0].id.backend_result_index,
            state.result_rows[1].id.backend_result_index
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
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: true,
            }),
        });

        assert_eq!(state.result_rows.len(), 2);
        assert_eq!(state.result_rows[0].id.match_index, Some(0));
        assert_eq!(state.result_rows[1].id.match_index, Some(1));
        match &state.result_rows[1].payload {
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
    fn truncated_content_metadata_survives_flattening() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(3)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(3),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                total_matches: 3,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: true,
            }),
        });

        for row in &state.result_rows {
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
            .result_rows
            .iter()
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
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "first needle".into(), 6, 12),
                    ContentMatch::new(8, "second needle".into(), 7, 13),
                ],
                truncated: false,
            }),
        });

        for row in &state.result_rows {
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
                total_matches: 3,
                matches: vec![ContentMatch::new(4, "first needle".into(), 6, 12)],
                truncated: true,
            }),
        });

        let row_id = &state.result_rows[0].id;
        let selectable_id = egui::Id::new(result_row_id_source(row_id.search_id, row_id));
        let omitted_id = egui::Id::new(omitted_matches_id_source(
            row_id.search_id,
            row_id.backend_result_index,
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
                total_matches: 2,
                matches: vec![
                    ContentMatch::new(4, "needle needle".into(), 0, 6),
                    ContentMatch::new(4, "needle needle".into(), 7, 13),
                ],
                truncated: false,
            }),
        });

        assert_eq!(state.result_rows.len(), 2);
        assert_eq!(state.result_rows[0].id.line_number, Some(4));
        assert_eq!(state.result_rows[1].id.line_number, Some(4));
        assert_eq!(state.result_rows[0].id.column, Some(0));
        assert_eq!(state.result_rows[1].id.column, Some(7));
        assert_ne!(state.result_rows[0].id, state.result_rows[1].id);
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
                    total_matches: 1,
                    matches: vec![ContentMatch::new(line, format!("{path} needle"), 0, 6)],
                    truncated: false,
                }),
            });
        }

        assert_eq!(state.result_rows[0].id.path, PathBuf::from("b.rs"));
        assert_eq!(state.result_rows[1].id.path, PathBuf::from("a.rs"));
        assert_eq!(state.result_rows[0].id.backend_result_index, 0);
        assert_eq!(state.result_rows[1].id.backend_result_index, 1);
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
                total_matches: 1,
                matches: vec![content_match.clone()],
                truncated: false,
            }),
        });

        assert_eq!(state.result_rows.len(), 1);
        assert_eq!(state.result_rows[0].id.search_id, SearchId(7));
        assert_eq!(state.result_rows[0].id.path, PathBuf::from("src/lib.rs"));
        assert_eq!(state.result_rows[0].id.line_number, Some(12));
        assert_eq!(state.result_rows[0].id.column, Some(6));
        match &state.result_rows[0].payload {
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
            backend_result_index: 0,
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
            backend_result_index: 0,
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
            backend_result_index: 0,
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
            backend_result_index: 0,
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
                backend_result_index: 0,
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
                backend_result_index: 0,
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
                backend_result_index: 0,
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
                backend_result_index: 0,
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
        assert_eq!(state.root_directory, "/tmp/project");

        state.start_nested_search(
            std::path::Path::new("/tmp/project/src"),
            true,
            FileSearchMode::Content,
            &mut coordinator,
        );
        assert_eq!(state.root_directory, "/tmp/project/src");
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
            total_matches,
            matches: (0..displayed_matches)
                .map(|index| ContentMatch::new(index + 1, format!("line {index} needle"), 5, 11))
                .collect(),
            truncated,
        })
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
        assert!(state
            .warning_error_message
            .as_deref()
            .is_some_and(|message| message.contains("missing or inaccessible")));
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
    fn enter_key_intent_does_not_open_selected_row() {
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
        assert!(state.warning_error_message.is_none());
    }
}
