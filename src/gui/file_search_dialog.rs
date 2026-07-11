use crate::actions::clipboard;
use crate::file_search::actions::{
    containing_directory, copied_filename, nested_search_root,
    open_configured_terminal_in_directory, open_in_configured_editor, open_path, reveal_path,
    InvocationTarget,
};
use crate::file_search::coordinator::{event_id, SearchCoordinator};
use crate::file_search::model::{
    ContentFileResult, ContentMatch, SearchBackend, SearchEvent, SearchId, SearchKind,
    SearchRequest, SearchResult, SearchScope, SearchStatus,
};
use crate::file_search::preview::PreviewRequest;
use crate::file_search::settings::FileSearchSettings;
use eframe::egui;
use std::collections::BTreeMap;
use std::path::PathBuf;

const DEFAULT_WINDOW_SIZE: egui::Vec2 = egui::vec2(760.0, 560.0);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentResultGroup {
    pub path: PathBuf,
    pub total_matches: usize,
    pub first_line: Option<String>,
    pub matches: Vec<ContentMatch>,
    pub truncated: bool,
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
    pub content_result_groups: BTreeMap<PathBuf, ContentResultGroup>,
    pub warning_error_message: Option<String>,
    pub inaccessible_path_warnings: usize,
    pub last_preview_request: Option<PreviewRequest>,
    pub persisted_window_size: egui::Vec2,
    pub settings: FileSearchSettings,
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
            content_result_groups: BTreeMap::new(),
            warning_error_message: None,
            inaccessible_path_warnings: 0,
            last_preview_request: None,
            persisted_window_size: DEFAULT_WINDOW_SIZE,
            settings: FileSearchSettings::default(),
        }
    }
}

impl FileSearchDialogState {
    pub fn open_with_mode(&mut self, mode: FileSearchMode) {
        self.selected_mode = mode;
        self.open = true;
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
        self.content_result_groups.clear();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend(&request));
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
        Some(id)
    }

    pub fn cancel_search(&mut self, coordinator: &mut SearchCoordinator) {
        if self.active_search_id.is_some() && self.current_status == SearchStatus::Running {
            coordinator.cancel_active();
            self.current_status = SearchStatus::Cancelled;
        }
    }

    pub fn drain_events(&mut self, coordinator: &mut SearchCoordinator) {
        for event in coordinator.drain_events_including_stale() {
            self.apply_event(event);
        }
    }

    pub fn apply_event(&mut self, event: SearchEvent) {
        if Some(event_id(&event)) != self.active_search_id {
            return;
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
            SearchEvent::Completed { .. } => self.current_status = SearchStatus::Completed,
            SearchEvent::Cancelled { .. } => self.current_status = SearchStatus::Cancelled,
            SearchEvent::Failed { error, .. } => {
                self.current_status = SearchStatus::Failed;
                self.warning_error_message = Some(error);
            }
        }
    }

    fn push_result(&mut self, result: SearchResult) {
        if let SearchResult::ContentFile(content) = &result {
            self.upsert_content_group(content);
        }
        self.results.push(result);
    }

    fn upsert_content_group(&mut self, content: &ContentFileResult) {
        self.content_result_groups.insert(
            content.path.clone(),
            ContentResultGroup {
                path: content.path.clone(),
                total_matches: content.total_matches,
                first_line: content.matches.first().map(|m| m.line.clone()),
                matches: content.matches.clone(),
                truncated: content.truncated,
            },
        );
    }

    pub fn ui(&mut self, ctx: &egui::Context, coordinator: &mut SearchCoordinator) {
        self.drain_events(coordinator);
        if !self.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.current_status == SearchStatus::Running {
                self.cancel_search(coordinator);
            } else {
                self.open = false;
            }
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
            let search_response = ui.text_edit_singleline(&mut self.search_text);
            if search_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.start_search(coordinator);
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Root");
            let root_response = ui.text_edit_singleline(&mut self.root_directory);
            if root_response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.start_search(coordinator);
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
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
                self.start_search(coordinator);
            }
            if ui.button("Cancel").clicked() {
                self.cancel_search(coordinator);
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
        let items: Vec<_> = self
            .results
            .iter()
            .filter_map(|result| match result {
                SearchResult::Filename(item) => Some(item.clone()),
                SearchResult::ContentFile(_) => None,
            })
            .collect();
        for item in items {
            let response = ui
                .horizontal(|ui| {
                    ui.label(&item.file_name);
                    ui.label(format!("{:?}", item.kind));
                    ui.label(
                        item.parent_directory
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    );
                })
                .response;
            response.context_menu(|ui| {
                self.result_context_menu(
                    ui,
                    &item.path,
                    item.kind == crate::file_search::model::FileKind::Directory,
                    None,
                    coordinator,
                )
            });
        }
    }

    fn content_results(&mut self, ui: &mut egui::Ui, coordinator: &mut SearchCoordinator) {
        let groups: Vec<_> = self.content_result_groups.values().cloned().collect();
        for group in groups {
            let response = egui::CollapsingHeader::new(format!(
                "{} ({} matches) - {}{}",
                group.path.display(),
                group.total_matches,
                group.first_line.as_deref().unwrap_or(""),
                if group.truncated {
                    " … truncated"
                } else {
                    ""
                }
            ))
            .show(ui, |ui| {
                for content_match in &group.matches {
                    let column = content_match
                        .column
                        .map(|column| format!(":{}", column.saturating_add(1)))
                        .unwrap_or_default();
                    ui.label(format!(
                        "{}{}: {}",
                        content_match.line_number, column, content_match.line
                    ));
                }
                if group.truncated {
                    ui.weak("Additional matches omitted. Use the context menu to search only this file.");
                }
            })
            .header_response;
            let first_match = group.matches.first().cloned();
            response.context_menu(|ui| {
                self.content_result_context_menu(ui, &group.path, first_match.as_ref(), coordinator)
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
        self.content_result_groups.clear();
        self.warning_error_message = None;
        self.inaccessible_path_warnings = 0;
        self.current_status = SearchStatus::Running;
        self.backend = Some(SearchCoordinator::select_backend(&request));
        let id = coordinator.start_search(request);
        self.active_search_id = Some(id);
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
            ui.close_menu();
        }
        if ui
            .button("Start content search beneath this directory")
            .clicked()
        {
            self.start_nested_search(path, is_directory, FileSearchMode::Content, coordinator);
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
    fn content_events_group_matches() {
        let mut state = FileSearchDialogState {
            active_search_id: Some(SearchId(1)),
            ..Default::default()
        };
        state.apply_event(SearchEvent::Result {
            id: SearchId(1),
            result: SearchResult::ContentFile(ContentFileResult {
                path: "src/lib.rs".into(),
                total_matches: 1,
                matches: vec![ContentMatch::new(4, "needle".into(), 0, 6)],
                truncated: false,
            }),
        });
        assert_eq!(state.content_result_groups.len(), 1);
        assert_eq!(
            state
                .content_result_groups
                .values()
                .next()
                .unwrap()
                .total_matches,
            1
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
