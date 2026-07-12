use crate::actions::clipboard;
use crate::file_search::actions::{
    containing_directory, copied_filename, nested_search_root,
    open_configured_terminal_in_directory, open_in_configured_editor, open_path, reveal_path,
    InvocationTarget,
};
use crate::file_search::coordinator::{event_id, SearchCoordinator};
use crate::file_search::model::{
    ContentFileResult, ContentMatch, FileKind, SearchBackend, SearchEvent, SearchId, SearchKind,
    SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::preview::PreviewRequest;
use crate::file_search::settings::FileSearchSettings;
use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(760.0, 560.0);
const ACTIVE_SEARCH_REPAINT_INTERVAL: Duration = Duration::from_millis(50);

pub const FILE_SEARCH_SEARCH_FIELD_ID_SOURCE: &str = "file_search_search_text";
pub const FILE_SEARCH_ROOT_FIELD_ID_SOURCE: &str = "file_search_root_directory";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchResultRowId {
    pub search_id: SearchId,
    pub backend_result_index: usize,
    pub match_index: Option<usize>,
    pub path: PathBuf,
    pub line_number: Option<usize>,
    pub column: Option<usize>,
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
        is_last_displayed_match_from_truncated_file: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSearchResultRow {
    pub id: FileSearchResultRowId,
    pub payload: FileSearchRowPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFileSearchResult {
    pub row_id: FileSearchResultRowId,
    pub path: PathBuf,
    pub match_context: Option<ContentMatch>,
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
    pub warning_error_message: Option<String>,
    pub inaccessible_path_warnings: usize,
    pub last_preview_request: Option<PreviewRequest>,
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
            warning_error_message: None,
            inaccessible_path_warnings: 0,
            last_preview_request: None,
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
        self.results.clear();
        self.result_rows.clear();
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
        match event {
            SearchEvent::Started { backend, .. } => {
                self.backend = Some(backend);
                self.current_status = SearchStatus::Running;
            }
            SearchEvent::Result { result, .. } => self.push_result(result),
            SearchEvent::Progress { progress, .. } => {
                self.inaccessible_path_warnings = progress
                    .directories_scanned
                    .saturating_sub(progress.files_scanned)
                    .try_into()
                    .unwrap_or(usize::MAX);
            }
            SearchEvent::Completed { .. } => {
                self.current_status = SearchStatus::Completed;
                self.request_immediate_repaint = true;
                return true;
            }
            SearchEvent::Cancelled { .. } => {
                self.current_status = SearchStatus::Cancelled;
                self.request_immediate_repaint = true;
                return true;
            }
            SearchEvent::Failed { error, .. } => {
                self.current_status = SearchStatus::Failed;
                self.warning_error_message = Some(error);
                self.request_immediate_repaint = true;
                return true;
            }
        }
        false
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
                            is_last_displayed_match_from_truncated_file: content.truncated
                                && Some(match_index) == last_displayed_match_index,
                        },
                    });
                }
            }
        }
        self.results.push(result);
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
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.selected_mode,
                FileSearchMode::Filename,
                "Filename",
            );
            ui.selectable_value(&mut self.selected_mode, FileSearchMode::Content, "Content");
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
        });
        ui.label(format!(
            "Backend: {} | Status: {:?} | Results: {} | Inaccessible-path warnings: {}",
            self.backend
                .map(|b| format!("{b:?}"))
                .unwrap_or_else(|| "not selected".to_string()),
            self.current_status,
            self.results.len(),
            self.inaccessible_path_warnings
        ));
        if let Some(msg) = &self.warning_error_message {
            ui.colored_label(egui::Color32::YELLOW, msg);
        }
        egui::ScrollArea::vertical().show(ui, |ui| match self.selected_mode {
            FileSearchMode::Filename => self.filename_results(ui, coordinator),
            FileSearchMode::Content => self.content_results(ui, coordinator),
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
                    path.clone(),
                    display_filename.clone(),
                    parent_directory_display.clone(),
                    *kind,
                )),
                FileSearchRowPayload::Content { .. } => None,
            })
            .collect();
        for (path, display_filename, parent_directory_display, kind) in rows {
            let response = ui
                .horizontal(|ui| {
                    ui.label(display_filename);
                    ui.label(format!("{:?}", kind));
                    ui.label(parent_directory_display);
                })
                .response;
            response.context_menu(|ui| {
                self.result_context_menu(
                    ui,
                    &path,
                    kind == crate::file_search::model::FileKind::Directory,
                    None,
                    coordinator,
                )
            });
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
                    is_last_displayed_match_from_truncated_file,
                } => Some((
                    path.clone(),
                    content_match.clone(),
                    *is_last_displayed_match_from_truncated_file,
                )),
                FileSearchRowPayload::Filename { .. } => None,
            })
            .collect();
        for (path, content_match, is_last_displayed_match_from_truncated_file) in rows {
            let column = content_match
                .column
                .map(|column| format!(":{}", column.saturating_add(1)))
                .unwrap_or_default();
            let response = ui
                .horizontal(|ui| {
                    ui.label(format!(
                        "{}:{}{}: {}{}",
                        path.display(),
                        content_match.line_number,
                        column,
                        content_match.line,
                        if is_last_displayed_match_from_truncated_file {
                            " … truncated"
                        } else {
                            ""
                        }
                    ));
                })
                .response;
            response.context_menu(|ui| {
                self.content_result_context_menu(ui, &path, Some(&content_match), coordinator)
            });
        }
    }

    fn content_result_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        path: &std::path::Path,
        first_match: Option<&ContentMatch>,
        coordinator: &mut SearchCoordinator,
    ) {
        if ui.button("Show all matches in this file").clicked() {
            self.start_file_content_search(path, coordinator);
            ui.ctx().request_repaint();
            ui.close_menu();
        }
        self.result_context_menu(ui, path, false, first_match, coordinator);
    }

    pub fn request_preview(
        &mut self,
        path: &std::path::Path,
        first_match: Option<&ContentMatch>,
    ) -> PreviewRequest {
        let request = first_match
            .map(|content_match| {
                PreviewRequest::for_match(
                    path,
                    content_match.line_number,
                    content_match
                        .column
                        .map(|column| column.saturating_add(1))
                        .unwrap_or(1),
                )
            })
            .unwrap_or_else(|| PreviewRequest::new(path));
        self.last_preview_request = Some(request.clone());
        request
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
        self.results.clear();
        self.result_rows.clear();
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
        first_match: Option<&ContentMatch>,
        coordinator: &mut SearchCoordinator,
    ) {
        if ui.button("Open in configured editor").clicked() {
            let settings = self.settings.clone();
            self.run_result_action("open in configured editor", || {
                open_in_configured_editor(
                    &settings,
                    InvocationTarget {
                        file: path,
                        line: first_match.map(|m| m.line_number),
                        column: first_match.and_then(|m| m.column.map(|c| c.saturating_add(1))),
                    },
                )
            });
            ui.close_menu();
        }
        if ui.button("Preview").clicked() {
            self.request_preview(path, first_match);
            ui.close_menu();
        }
        if ui.button("Open file or directory").clicked() {
            self.run_result_action("open", || open_path(path));
            ui.close_menu();
        }
        if ui.button("Reveal in Explorer").clicked() {
            self.run_result_action("reveal", || reveal_path(path));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::model::{
        ContentMatch, FileKind, FilenameRank, FilenameResult, SearchProgress,
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
    fn preserving_dimensions() {
        let mut state = FileSearchDialogState::default();
        state.persisted_window_size = egui::vec2(900.0, 700.0);
        state.open_with_mode(FileSearchMode::Filename);
        assert_eq!(state.persisted_window_size, egui::vec2(900.0, 700.0));
    }

    #[test]
    fn requesting_preview_records_match_context() {
        let mut state = FileSearchDialogState::default();
        let path = PathBuf::from("src/lib.rs");
        let content_match = ContentMatch::new(12, "hello needle".into(), 6, 12);

        let request = state.request_preview(&path, Some(&content_match));

        assert_eq!(request.path, path);
        assert_eq!(request.selected_match.unwrap().line, 12);
        assert_eq!(request.selected_match.unwrap().column, 7);
        assert_eq!(state.last_preview_request, Some(request));
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
}
