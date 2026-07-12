use crate::file_search::actions::{InvocationTarget, open_in_configured_editor};
use crate::file_search::model::ContentMatch;
use crate::file_search::preview::{FilePreview, PreviewCache, PreviewKind, PreviewRequest};
use crate::file_search::settings::FileSearchSettings;
use eframe::egui;
use std::path::{Path, PathBuf};

const DEFAULT_PREVIEW_WINDOW_SIZE: egui::Vec2 = egui::vec2(860.0, 620.0);

pub fn preview_window_id_source() -> &'static str {
    "file_search_preview_window"
}

pub fn preview_scroll_id_source(path: impl Into<PathBuf>) -> (&'static str, PathBuf) {
    ("file_search_preview_scroll", path.into())
}

pub fn preview_line_id_source(
    path: impl Into<PathBuf>,
    line_number: usize,
) -> (&'static str, PathBuf, usize) {
    ("file_search_preview_line", path.into(), line_number)
}

pub struct FileSearchPreviewDialogState {
    pub open: bool,
    pub current_request: Option<PreviewRequest>,
    pub loaded_preview: Option<FilePreview>,
    pub selected_match: Option<ContentMatch>,
    pub loading: bool,
    pub load_error_message: Option<String>,
    pub action_error_message: Option<String>,
    pub pending_auto_scroll: bool,
    pub persisted_window_size: egui::Vec2,
    pub reset_horizontal_scroll: bool,
    preview_cache: PreviewCache,
}

impl std::fmt::Debug for FileSearchPreviewDialogState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSearchPreviewDialogState")
            .field("open", &self.open)
            .field("current_request", &self.current_request)
            .field("loaded_preview", &self.loaded_preview)
            .field("selected_match", &self.selected_match)
            .field("loading", &self.loading)
            .field("load_error_message", &self.load_error_message)
            .field("action_error_message", &self.action_error_message)
            .field("pending_auto_scroll", &self.pending_auto_scroll)
            .field("persisted_window_size", &self.persisted_window_size)
            .field("reset_horizontal_scroll", &self.reset_horizontal_scroll)
            .finish()
    }
}

impl Clone for FileSearchPreviewDialogState {
    fn clone(&self) -> Self {
        Self {
            open: self.open,
            current_request: self.current_request.clone(),
            loaded_preview: self.loaded_preview.clone(),
            selected_match: self.selected_match.clone(),
            loading: self.loading,
            load_error_message: self.load_error_message.clone(),
            action_error_message: self.action_error_message.clone(),
            pending_auto_scroll: self.pending_auto_scroll,
            persisted_window_size: self.persisted_window_size,
            reset_horizontal_scroll: self.reset_horizontal_scroll,
            preview_cache: PreviewCache::default(),
        }
    }
}

impl PartialEq for FileSearchPreviewDialogState {
    fn eq(&self, other: &Self) -> bool {
        self.open == other.open
            && self.current_request == other.current_request
            && self.loaded_preview == other.loaded_preview
            && self.selected_match == other.selected_match
            && self.loading == other.loading
            && self.load_error_message == other.load_error_message
            && self.action_error_message == other.action_error_message
            && self.pending_auto_scroll == other.pending_auto_scroll
            && self.persisted_window_size == other.persisted_window_size
            && self.reset_horizontal_scroll == other.reset_horizontal_scroll
    }
}

impl Default for FileSearchPreviewDialogState {
    fn default() -> Self {
        Self {
            open: false,
            current_request: None,
            loaded_preview: None,
            selected_match: None,
            loading: false,
            load_error_message: None,
            action_error_message: None,
            pending_auto_scroll: false,
            persisted_window_size: DEFAULT_PREVIEW_WINDOW_SIZE,
            reset_horizontal_scroll: false,
            preview_cache: PreviewCache::default(),
        }
    }
}

impl FileSearchPreviewDialogState {
    pub fn open_content_match(
        &mut self,
        path: impl AsRef<Path>,
        content_match: ContentMatch,
        settings: &FileSearchSettings,
    ) {
        let mut request = PreviewRequest::for_match(
            path.as_ref(),
            content_match.line_number,
            content_match
                .column
                .map(|column| column.saturating_add(1))
                .unwrap_or(1),
        );
        request.max_bytes_full_file_preview = settings.max_content_search_file_size_bytes as usize;
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

        self.open = true;
        self.current_request = Some(request.clone());
        self.selected_match = Some(content_match);
        self.loaded_preview = None;
        self.loading = true;
        self.load_error_message = None;
        self.action_error_message = None;
        self.reset_horizontal_scroll = true;
        self.pending_auto_scroll = true;
        self.load_current_preview();
    }

    pub fn load_current_preview(&mut self) {
        let Some(request) = self.current_request.clone() else {
            self.loaded_preview = None;
            self.loading = false;
            return;
        };
        let preview = self.preview_cache.preview(&request);
        self.load_error_message = preview.error.as_ref().map(|error| error.message.clone());
        self.loaded_preview = Some(preview);
        self.loading = false;
    }

    pub fn ui(&mut self, ctx: &egui::Context, settings: &FileSearchSettings) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut size_to_persist = None;
        egui::Window::new("File preview")
            .id(egui::Id::new(preview_window_id_source()))
            .open(&mut open)
            .default_size(self.persisted_window_size)
            .show(ctx, |ui| {
                size_to_persist = Some(ui.available_size());
                self.render_contents(ui, settings);
            });
        self.open = open;
        if let Some(size) = size_to_persist.filter(|size| size.x > 0.0 && size.y > 0.0) {
            self.persisted_window_size = size;
        }
    }

    fn render_contents(&mut self, ui: &mut egui::Ui, settings: &FileSearchSettings) {
        let Some(request) = self.current_request.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.label(request.path.display().to_string());
            if ui.button("Open in editor").clicked() {
                let target = InvocationTarget {
                    file: &request.path,
                    line: self.selected_match.as_ref().map(|m| m.line_number),
                    column: self
                        .selected_match
                        .as_ref()
                        .and_then(|m| m.column.map(|c| c.saturating_add(1))),
                };
                if let Err(error) = open_in_configured_editor(settings, target) {
                    self.action_error_message = Some(error.to_string());
                }
            }
        });
        if let Some(error) = &self.action_error_message {
            ui.colored_label(egui::Color32::RED, error);
        }
        if let Some(error) = &self.load_error_message {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        if self.loading {
            ui.spinner();
            return;
        }
        let Some(preview) = &self.loaded_preview else {
            return;
        };
        match preview.kind {
            PreviewKind::Text => {
                egui::ScrollArea::both()
                    .id_source(preview_scroll_id_source(&request.path))
                    .show(ui, |ui| {
                        for line in &preview.lines {
                            let response =
                                ui.monospace(format!("{:>6}  {}", line.line_number, line.text));
                            if self.pending_auto_scroll
                                && Some(line.line_number)
                                    == self.selected_match.as_ref().map(|m| m.line_number)
                            {
                                response.scroll_to_me(Some(egui::Align::Center));
                                self.pending_auto_scroll = false;
                            }
                        }
                    });
            }
            _ => {
                ui.label("Preview is not available as text.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_first_match_sets_open_current_request_and_match() {
        let mut state = FileSearchPreviewDialogState::default();
        let path = PathBuf::from("src/lib.rs");
        let content_match = ContentMatch::new(12, "hello needle".into(), 6, 12);
        state.open_content_match(&path, content_match.clone(), &FileSearchSettings::default());
        assert!(state.open);
        assert_eq!(state.current_request.as_ref().map(|r| &r.path), Some(&path));
        assert_eq!(state.selected_match, Some(content_match));
    }

    #[test]
    fn opening_second_match_replaces_request_and_match() {
        let mut state = FileSearchPreviewDialogState::default();
        let settings = FileSearchSettings::default();
        state.open_content_match(
            "first.rs",
            ContentMatch::new(1, "first".into(), 0, 5),
            &settings,
        );
        let second = ContentMatch::new(42, "second needle".into(), 7, 13);
        state.open_content_match("second.rs", second.clone(), &settings);
        assert_eq!(
            state.current_request.as_ref().map(|r| r.path.clone()),
            Some(PathBuf::from("second.rs"))
        );
        assert_eq!(state.selected_match, Some(second));
    }

    #[test]
    fn action_error_clears_on_new_preview() {
        let mut state = FileSearchPreviewDialogState::default();
        state.action_error_message = Some("old error".into());
        state.open_content_match(
            "next.rs",
            ContentMatch::new(3, "next".into(), 0, 4),
            &FileSearchSettings::default(),
        );
        assert_eq!(state.action_error_message, None);
    }

    #[test]
    fn pending_auto_scroll_resets_on_new_preview() {
        let mut state = FileSearchPreviewDialogState::default();
        state.pending_auto_scroll = false;
        state.open_content_match(
            "next.rs",
            ContentMatch::new(3, "next".into(), 0, 4),
            &FileSearchSettings::default(),
        );
        assert!(state.pending_auto_scroll);
    }

    #[test]
    fn preview_id_sources_are_stable_and_path_scoped() {
        let path = PathBuf::from("src/lib.rs");
        assert_eq!(preview_window_id_source(), "file_search_preview_window");
        assert_eq!(
            preview_scroll_id_source(&path),
            ("file_search_preview_scroll", path.clone())
        );
        assert_ne!(
            preview_scroll_id_source("src/lib.rs"),
            preview_scroll_id_source("src/main.rs")
        );
        assert_ne!(
            preview_line_id_source(&path, 1),
            preview_line_id_source(&path, 2)
        );
    }
}
