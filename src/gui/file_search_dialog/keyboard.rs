//! Keyboard handling helpers for the file-search dialog.
use super::{
    FileSearchDialogState, FileSearchEscapeAction, FileSearchResultRow, FileSearchRowPayload,
    SelectedFileSearchResultPayload,
};
use crate::file_search::actions::{containing_directory, open_path};
use crate::file_search::coordinator::SearchCoordinator;
use crate::file_search::export;
use crate::file_search::model::SearchResult;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectionMove {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
}

impl FileSearchDialogState {
    pub(super) fn handle_result_keyboard_shortcuts(
        &mut self,
        ui: &mut egui::Ui,
        coordinator: &mut SearchCoordinator,
    ) {
        if ui.ctx().wants_keyboard_input() {
            return;
        }

        let modifiers = ui.input(|i| i.modifiers);
        let command = modifiers.command;
        let activate_containing = command || modifiers.ctrl;

        let movement = ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                Some(SelectionMove::Next)
            } else if i.key_pressed(egui::Key::ArrowUp) {
                Some(SelectionMove::Previous)
            } else if i.key_pressed(egui::Key::PageDown) {
                Some(SelectionMove::PageNext)
            } else if i.key_pressed(egui::Key::PageUp) {
                Some(SelectionMove::PagePrevious)
            } else if i.key_pressed(egui::Key::Home) {
                Some(SelectionMove::First)
            } else if i.key_pressed(egui::Key::End) {
                Some(SelectionMove::Last)
            } else {
                None
            }
        });
        if let Some(movement) = movement {
            self.move_selection(movement);
            ui.ctx().request_repaint();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if activate_containing {
                self.open_selected_containing_directory();
            } else {
                self.open_selected_result();
            }
            ui.ctx().request_repaint();
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.handle_escape(coordinator) == FileSearchEscapeAction::Cancel {
                ui.ctx().request_repaint();
            }
        }

        if command && !modifiers.shift && ui.input(|i| i.key_pressed(egui::Key::C)) {
            if let Some(payload) = self.copy_selected_path_payload() {
                self.copy_text_payload("copy selected path", payload);
            }
        }
        if command && modifiers.shift && ui.input(|i| i.key_pressed(egui::Key::C)) {
            if let Some(payload) = self.copy_all_visible_results_payload() {
                self.copy_text_payload("copy visible results", payload);
            }
        }
        if command && ui.input(|i| i.key_pressed(egui::Key::L)) {
            if let Some(payload) = self.copy_selected_match_line_payload() {
                self.copy_text_payload("copy matching line", payload);
            }
        }
        if command && ui.input(|i| i.key_pressed(egui::Key::F)) {
            self.refine_from_selection(coordinator);
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn move_selection(&mut self, movement: SelectionMove) {
        if self.result_rows.is_empty() {
            self.clear_selection();
            return;
        }
        let current_key = self.selected_result_key();
        let current_idx = current_key.as_ref().and_then(|key| {
            self.result_rows
                .iter()
                .position(|row| row.id.result_key == *key)
        });
        let last = self.result_rows.len() - 1;
        let page = 10;
        let next_idx = match (movement, current_idx) {
            (SelectionMove::First, _) => 0,
            (SelectionMove::Last, _) => last,
            (SelectionMove::Next, Some(idx)) => idx.saturating_add(1).min(last),
            (SelectionMove::Previous, Some(idx)) => idx.saturating_sub(1),
            (SelectionMove::PageNext, Some(idx)) => idx.saturating_add(page).min(last),
            (SelectionMove::PagePrevious, Some(idx)) => idx.saturating_sub(page),
            (SelectionMove::Previous | SelectionMove::PagePrevious, None) => last,
            (SelectionMove::Next | SelectionMove::PageNext, None) => 0,
        };
        let row = self.result_rows[next_idx].clone();
        self.select_result(&row);
    }

    pub(super) fn open_selected_containing_directory(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        self.run_result_action("open containing directory", || {
            let dir = containing_directory(&path)
                .ok_or_else(|| anyhow::anyhow!("{} has no containing directory", path.display()))?;
            open_path(&dir)
        });
    }

    fn selected_path(&self) -> Option<std::path::PathBuf> {
        self.selected_result
            .as_ref()
            .map(|selected| match &selected.payload {
                SelectedFileSearchResultPayload::Filename { path, .. } => path.clone(),
                SelectedFileSearchResultPayload::Content { path, .. } => path.clone(),
            })
    }

    pub(super) fn copy_selected_path_payload(&self) -> Option<String> {
        self.selected_path().map(|path| path.display().to_string())
    }

    pub(super) fn copy_selected_payload(&self) -> Option<String> {
        self.selected_result
            .as_ref()
            .map(|selected| match &selected.payload {
                SelectedFileSearchResultPayload::Filename { path, .. } => {
                    path.display().to_string()
                }
                SelectedFileSearchResultPayload::Content { content_match, .. } => {
                    export::selected_content_match_line(content_match)
                }
            })
    }

    pub(super) fn copy_selected_match_line_payload(&self) -> Option<String> {
        self.selected_result
            .as_ref()
            .and_then(|selected| match &selected.payload {
                SelectedFileSearchResultPayload::Content { content_match, .. } => {
                    Some(export::selected_content_match_line(content_match))
                }
                SelectedFileSearchResultPayload::Filename { .. } => None,
            })
    }

    pub(super) fn copy_all_visible_results_payload(&self) -> Option<String> {
        if self.result_rows.is_empty() {
            return None;
        }
        Some(match self.selected_mode {
            super::FileSearchMode::Filename => {
                let rows = self.visible_filename_export_rows();
                export::all_visible_filename_results(rows.iter())
            }
            super::FileSearchMode::Content => {
                let rows = self.visible_content_export_rows();
                export::all_visible_content_results(rows.iter())
            }
        })
    }

    pub(super) fn export_visible_results_tsv(&self) -> String {
        match self.selected_mode {
            super::FileSearchMode::Filename => {
                let rows = self.visible_filename_export_rows();
                export::filename_results_tsv(rows.iter())
            }
            super::FileSearchMode::Content => {
                let rows = self.visible_content_export_rows();
                export::content_results_tsv(rows.iter())
            }
        }
    }

    pub(super) fn export_visible_results_to_file(&mut self) {
        let default_name = match self.selected_mode {
            super::FileSearchMode::Filename => "filename-results.tsv",
            super::FileSearchMode::Content => "content-results.tsv",
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("TSV", &["tsv"])
            .set_file_name(default_name)
            .save_file()
        else {
            return;
        };
        let payload = self.export_visible_results_tsv();
        self.run_result_action("export visible results", || {
            std::fs::write(&path, payload)?;
            Ok(())
        });
    }

    pub(super) fn copy_text_payload(&mut self, label: &str, payload: String) {
        self.run_result_action(label, || {
            export::set_clipboard_text(&payload)?;
            Ok(())
        });
    }

    fn visible_filename_export_rows(&self) -> Vec<export::FilenameExportRow> {
        self.result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Filename {
                    path,
                    display_filename,
                    parent_directory_display,
                    size,
                    modified,
                    match_quality,
                    ..
                } => Some(export::FilenameExportRow {
                    path: path.clone(),
                    file_name: display_filename.clone(),
                    directory: parent_directory_display.clone(),
                    size: *size,
                    modified: *modified,
                    match_quality: Some(*match_quality),
                }),
                FileSearchRowPayload::Content { .. } => None,
            })
            .collect()
    }

    fn visible_content_export_rows(&self) -> Vec<export::ContentExportRow> {
        self.result_rows
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Content {
                    path,
                    content_match,
                    ..
                } => {
                    let source = self.results.iter().find_map(|result| match result {
                        SearchResult::ContentFile(file) if file.path == *path => Some(file),
                        _ => None,
                    });
                    Some(export::ContentExportRow {
                        path: path.clone(),
                        file_name: source
                            .map(|file| file.file_name.clone())
                            .unwrap_or_else(|| export::file_name_from_path(path)),
                        directory: path
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                        line_number: content_match.line_number,
                        column: content_match.column,
                        line_preview: content_match.line.clone(),
                        modified: source.and_then(|file| file.modified),
                        match_quality: source.and_then(|file| file.filename_relevance),
                    })
                }
                FileSearchRowPayload::Filename { .. } => None,
            })
            .collect()
    }

    pub(super) fn refine_from_selection(&mut self, coordinator: &mut SearchCoordinator) {
        let Some(selected) = self.selected_result.clone() else {
            return;
        };
        match selected.payload {
            SelectedFileSearchResultPayload::Content { path, .. } => {
                self.start_file_content_search(&path, coordinator);
            }
            SelectedFileSearchResultPayload::Filename { path, kind } => {
                self.start_nested_search(
                    &path,
                    kind == crate::file_search::model::FileKind::Directory,
                    self.selected_mode,
                    coordinator,
                );
            }
        }
    }
}
