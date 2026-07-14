//! Keyboard handling helpers for the file-search dialog.
use super::{
    FileSearchDialogState, FileSearchEscapeAction, FileSearchResultRow, FileSearchRowPayload,
    SelectedFileSearchResultPayload,
};
use crate::actions::clipboard;
use crate::file_search::actions::{containing_directory, open_path};
use crate::file_search::coordinator::SearchCoordinator;
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

    pub(super) fn copy_selected_match_line_payload(&self) -> Option<String> {
        self.selected_result
            .as_ref()
            .and_then(|selected| match &selected.payload {
                SelectedFileSearchResultPayload::Content { content_match, .. } => {
                    Some(content_match.line.clone())
                }
                SelectedFileSearchResultPayload::Filename { .. } => None,
            })
    }

    pub(super) fn copy_all_visible_results_payload(&self) -> Option<String> {
        if self.result_rows.is_empty() {
            return None;
        }
        Some(
            self.result_rows
                .iter()
                .map(visible_result_line)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn copy_text_payload(&mut self, label: &str, payload: String) {
        self.run_result_action(label, || {
            clipboard::set_text(&payload)?;
            Ok(())
        });
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

fn visible_result_line(row: &FileSearchResultRow) -> String {
    match &row.payload {
        FileSearchRowPayload::Filename { path, .. } => path.display().to_string(),
        FileSearchRowPayload::Content {
            path,
            content_match,
            ..
        } => format!(
            "{}:{}:{}: {}",
            path.display(),
            content_match.line_number,
            content_match
                .column
                .map(|column| column.saturating_add(1))
                .unwrap_or(1),
            content_match.line
        ),
    }
}
