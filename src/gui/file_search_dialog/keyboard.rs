//! Keyboard handling helpers for the file-search dialog.
use super::{
    FileSearchDialogState, FileSearchEscapeAction, FileSearchRowPayload,
    SelectedFileSearchResultPayload,
};
use crate::file_search::actions::{
    InvocationTarget, containing_directory, execute_explorer_action, open_in_configured_editor,
    open_path, resolve_explorer_action,
};
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
        let ctx = ui.ctx();
        let modifiers = ctx.input(|i| i.modifiers);
        let (text_field_focused, search_submit_field_focused) = ctx.memory(|m| {
            let focused = m.focused();
            let search_submit = focused.is_some_and(|id| {
                id == Self::search_field_id()
                    || (0..self.custom_roots.len()).any(|idx| id == Self::root_text_field_id(idx))
            });
            let any_text = search_submit || focused == Some(Self::refinement_field_id());
            (any_text, search_submit)
        });
        let menu_or_combo_active = ctx.wants_keyboard_input() && !text_field_focused;

        if modifiers.ctrl && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.focus_search_field();
            ctx.request_repaint();
            return;
        }
        if modifiers.ctrl && ctx.input(|i| i.key_pressed(egui::Key::L)) {
            self.focus_root_field();
            ctx.request_repaint();
            return;
        }

        if !menu_or_combo_active {
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.select_next_visible_result();
                ctx.request_repaint();
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.select_previous_visible_result();
                ctx.request_repaint();
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !menu_or_combo_active {
            if search_submit_field_focused {
                self.start_search(coordinator);
                ctx.request_repaint();
            } else if !text_field_focused {
                if modifiers.ctrl {
                    self.open_selected_in_editor();
                } else if modifiers.alt {
                    self.reveal_selected_in_explorer();
                } else {
                    self.open_selected_result();
                }
                ctx.request_repaint();
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && self.handle_escape(coordinator) == FileSearchEscapeAction::Cancel
        {
            ctx.request_repaint();
        }

        if modifiers.ctrl
            && ctx.input(|i| i.key_pressed(egui::Key::C))
            && !(text_field_focused && ctx.wants_keyboard_input())
        {
            if modifiers.shift {
                self.copy_selected_matching_line();
            } else {
                self.copy_selected_path();
            }
        }
    }

    pub(super) fn move_selection(&mut self, movement: SelectionMove) {
        let selectable_rows: Vec<_> = self.visible_selectable_result_rows().collect();
        if selectable_rows.is_empty() {
            self.clear_selection();
            return;
        }
        let current_key = self.selected_result_key();
        let current_idx = current_key.as_ref().and_then(|key| {
            selectable_rows
                .iter()
                .position(|row| row.id.result_key == *key)
        });
        let last = selectable_rows.len() - 1;
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
        let row = selectable_rows[next_idx].clone();
        self.select_result(&row);
    }

    pub(super) fn select_next_visible_result(&mut self) {
        self.move_selection(SelectionMove::Next);
    }

    pub(super) fn select_previous_visible_result(&mut self) {
        self.move_selection(SelectionMove::Previous);
    }

    pub(super) fn open_selected_in_editor(&mut self) {
        let Some(selected) = self.selected_result.clone() else {
            return;
        };
        let (path, line, column) = match selected.payload {
            SelectedFileSearchResultPayload::Filename { path, .. } => (path, None, None),
            SelectedFileSearchResultPayload::Content {
                path,
                content_match,
            } => (
                path,
                Some(content_match.line_number),
                content_match.column.map(|c| c.saturating_add(1)),
            ),
        };
        let settings = self.settings.clone();
        self.run_result_action("open in configured editor", || {
            open_in_configured_editor(
                &settings,
                InvocationTarget {
                    file: &path,
                    line,
                    column,
                },
            )
        });
    }

    pub(super) fn reveal_selected_in_explorer(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        self.run_result_action("reveal", || {
            execute_explorer_action(resolve_explorer_action(&path)?)
        });
    }

    pub(super) fn copy_selected_path(&mut self) {
        if let Some(payload) = self.copy_selected_path_payload() {
            self.copy_text_payload("copy selected path", payload);
        }
    }

    pub(super) fn copy_selected_matching_line(&mut self) {
        if let Some(payload) = self.copy_selected_match_line_payload() {
            self.copy_text_payload("copy matching line", payload);
        }
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

    pub(super) fn copy_all_visible_results_payload(&self) -> Result<String, String> {
        let payload = match self.selected_mode {
            super::FileSearchMode::Filename => {
                let rows = self.visible_filename_export_rows();
                export::all_visible_filename_results(rows.iter())
            }
            super::FileSearchMode::Content => {
                let rows = self.visible_content_export_rows();
                export::all_visible_content_results(rows.iter())
            }
        };
        export::non_empty_export(payload)
    }

    pub(super) fn export_visible_results_tsv(&self) -> Result<String, String> {
        let has_rows = self.visible_selectable_result_rows().next().is_some();
        if !has_rows {
            return Err("There are no visible file-search results to export.".to_string());
        }
        Ok(match self.selected_mode {
            super::FileSearchMode::Filename => {
                let rows = self.visible_filename_export_rows();
                export::filename_results_tsv(rows.iter())
            }
            super::FileSearchMode::Content => {
                let rows = self.visible_content_export_rows();
                export::content_results_tsv(rows.iter())
            }
        })
    }

    pub(super) fn copy_visible_full_paths_payload(&self) -> Result<String, String> {
        let rows = self.visible_selectable_result_rows().collect::<Vec<_>>();
        export::visible_full_paths(rows.iter().map(|row| row.id.path.as_path()))
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
        let payload = match self.export_visible_results_tsv() {
            Ok(payload) => payload,
            Err(err) => {
                self.warning_error_message = Some(err);
                return;
            }
        };
        self.run_result_action("export visible results", || {
            std::fs::write(&path, payload.as_bytes())?;
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
        self.visible_result_rows()
            .iter()
            .filter_map(|row| match &row.payload {
                FileSearchRowPayload::Filename {
                    path,
                    display_filename,
                    parent_directory_display,
                    kind,
                    size,
                    modified,
                    match_quality,
                    ..
                } => Some(export::FilenameExportRow {
                    path: path.clone(),
                    file_name: display_filename.clone(),
                    directory: parent_directory_display.clone(),
                    kind: *kind,
                    size: *size,
                    modified: *modified,
                    match_quality: Some(*match_quality),
                }),
                FileSearchRowPayload::Content { .. }
                | FileSearchRowPayload::ContentGroupHeader { .. } => None,
            })
            .collect()
    }

    fn visible_content_export_rows(&self) -> Vec<export::ContentExportRow> {
        self.visible_result_rows()
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
                        total_matches_in_file: source.map(|file| file.total_matches).unwrap_or(1),
                        file_truncated: source.map(|file| file.truncated).unwrap_or(false),
                    })
                }
                FileSearchRowPayload::Filename { .. }
                | FileSearchRowPayload::ContentGroupHeader { .. } => None,
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
