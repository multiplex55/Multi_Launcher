use crate::file_search::actions::{execute_explorer_action, resolve_explorer_action};
use crate::file_search::model::ContentMatch;
use crate::file_search::preview::{
    preview_file, FilePreview, PreviewCoverage, PreviewKind, PreviewLoadOutcome,
    PreviewLoadStateMachine, PreviewLoadingState, PreviewRequest,
};
use crate::file_search::settings::FileSearchSettings;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const DEFAULT_PREVIEW_WINDOW_SIZE: egui::Vec2 = egui::vec2(860.0, 620.0);
const PREVIEW_ROW_HEIGHT: f32 = 18.0;

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
    load_state: PreviewLoadStateMachine,
    preview_result_rx: Option<Receiver<(u64, PreviewLoadOutcome)>>,
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
            load_state: self.load_state.clone(),
            preview_result_rx: None,
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
            load_state: PreviewLoadStateMachine::default(),
            preview_result_rx: None,
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
        request.max_bytes_full_file_preview = settings
            .max_full_preview_file_size_bytes
            .try_into()
            .unwrap_or(usize::MAX);
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
        self.start_loading_current_preview();
    }

    pub fn load_current_preview(&mut self) {
        self.start_loading_current_preview();
    }

    pub fn start_loading_current_preview(&mut self) {
        let Some(request) = self.current_request.clone() else {
            self.loaded_preview = None;
            self.loading = false;
            self.load_state.clear();
            self.preview_result_rx = None;
            return;
        };

        self.loaded_preview = None;
        self.loading = true;
        self.load_error_message = None;
        let request_id = self.load_state.begin_request();
        let (tx, rx) = mpsc::channel();
        self.preview_result_rx = Some(rx);
        thread::spawn(move || {
            let preview = preview_file(&request);
            let outcome = preview
                .error
                .as_ref()
                .map(|error| PreviewLoadOutcome::Failed(error.message.clone()))
                .unwrap_or_else(|| PreviewLoadOutcome::Loaded(preview));
            let _ = tx.send((request_id, outcome));
        });
    }

    fn poll_preview_result(&mut self) {
        let Some(rx) = self.preview_result_rx.take() else {
            return;
        };
        let mut keep_rx = true;
        while let Ok((request_id, outcome)) = rx.try_recv() {
            let accepted = self.load_state.complete_request(request_id, outcome);
            if accepted {
                match &self.load_state.state {
                    PreviewLoadingState::Loaded { preview, .. } => {
                        self.load_error_message = None;
                        self.loaded_preview = Some(preview.clone());
                        self.loading = false;
                    }
                    PreviewLoadingState::Failed { error, .. } => {
                        self.load_error_message = Some(error.clone());
                        self.loaded_preview = None;
                        self.loading = false;
                    }
                    _ => {}
                }
                keep_rx = false;
            }
        }
        if keep_rx {
            self.preview_result_rx = Some(rx);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, settings: &FileSearchSettings) {
        if !self.open {
            return;
        }
        self.poll_preview_result();
        if self.load_state.is_loading() {
            ctx.request_repaint();
        }
        let mut open = self.open;
        let mut size_to_persist = None;
        let response = egui::Window::new("File preview")
            .id(egui::Id::new(preview_window_id_source()))
            .open(&mut open)
            .default_size(self.persisted_window_size)
            .resizable(true)
            .show(ctx, |ui| {
                self.render_contents(ui, settings);
            });
        if let Some(response) = response {
            size_to_persist = Some(response.response.rect.size());
        }
        self.open = open;
        if let Some(size) = size_to_persist.filter(|size| size.x > 0.0 && size.y > 0.0) {
            self.persisted_window_size = size;
        }
    }

    fn render_contents(&mut self, ui: &mut egui::Ui, settings: &FileSearchSettings) {
        let Some(request) = self.current_request.clone() else {
            return;
        };
        self.render_header(ui, &request, settings);
        ui.separator();
        if self.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading…");
            });
            return;
        }
        let Some(preview) = self.loaded_preview.clone() else {
            return;
        };
        match preview.kind {
            PreviewKind::Text => self.render_text_preview(ui, &preview),
            _ => {
                ui.label("Preview is not available as text.");
            }
        }
    }

    fn render_header(
        &mut self,
        ui: &mut egui::Ui,
        request: &PreviewRequest,
        _settings: &FileSearchSettings,
    ) {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(request.path.display().to_string()).strong());
            ui.horizontal_wrapped(|ui| {
                if let Some(line) = self.selected_match.as_ref().map(|m| m.line_number) {
                    ui.label(format!("Selected line: {line}"));
                }
                if let Some(preview) = &self.loaded_preview {
                    ui.label(format!("Status: {}", preview_status(preview)));
                    if should_show_file_size(preview) {
                        ui.label(format!(
                            "Size: {}",
                            human_file_size(preview.metadata.len_bytes)
                        ));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Open in Explorer").clicked() {
                    match resolve_explorer_action(&request.path).and_then(execute_explorer_action) {
                        Ok(()) => self.action_error_message = None,
                        Err(error) => self.action_error_message = Some(error.to_string()),
                    }
                }
                if ui.button("Copy Full Path").clicked() {
                    ui.ctx().output_mut(|output| {
                        output.copied_text = request.path.display().to_string()
                    });
                    self.action_error_message = None;
                }
                if ui.button("Copy Matching Line").clicked() {
                    if let Some(line) = self.selected_match.as_ref().map(|m| m.line.clone()) {
                        ui.ctx().output_mut(|output| output.copied_text = line);
                        self.action_error_message = None;
                    } else {
                        self.action_error_message =
                            Some("No matching-line text is available to copy.".to_string());
                    }
                }
            });
        });
        if let Some(error) = &self.action_error_message {
            ui.colored_label(egui::Color32::RED, error);
        }
        if let Some(error) = &self.load_error_message {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        if let Some(preview) = &self.loaded_preview {
            for warning in &preview.warnings {
                ui.colored_label(egui::Color32::YELLOW, warning);
            }
        }
    }

    fn render_text_preview(&mut self, ui: &mut egui::Ui, preview: &FilePreview) {
        let selected_line = self.selected_match.as_ref().map(|m| m.line_number);
        let scroll_target = self
            .pending_auto_scroll
            .then(|| {
                selected_line.and_then(|line| displayed_index_for_source_line(&preview.lines, line))
            })
            .flatten();
        let line_number_width = line_number_column_width(
            preview
                .lines
                .iter()
                .map(|l| l.line_number)
                .max()
                .unwrap_or(0),
        );
        let path = preview.path.clone();
        let row_count = preview.lines.len();

        egui::ScrollArea::both()
            .id_source(preview_scroll_id_source(path))
            .auto_shrink([false, false])
            .show_rows(ui, PREVIEW_ROW_HEIGHT, row_count, |ui, row_range| {
                ui.style_mut().wrap = Some(false);
                for row_index in row_range {
                    let Some(line) = preview.lines.get(row_index) else {
                        continue;
                    };
                    let is_selected = Some(line.line_number) == selected_line;
                    let fill = is_selected.then(|| ui.visuals().selection.bg_fill);
                    let inner = egui::Frame::none()
                        .fill(fill.unwrap_or(egui::Color32::TRANSPARENT))
                        .show(ui, |ui| {
                            ui.set_min_height(PREVIEW_ROW_HEIGHT);
                            ui.horizontal(|ui| {
                                ui.monospace(format!(
                                    "{:>width$}",
                                    line.line_number,
                                    width = line_number_width
                                ));
                                ui.monospace("  ");
                                ui.monospace(&line.text);
                            });
                        });
                    if scroll_target == Some(row_index) {
                        inner.response.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            });
        if self.pending_auto_scroll {
            self.pending_auto_scroll = false;
        }
    }
}

fn displayed_index_for_source_line(
    lines: &[crate::file_search::preview::PreviewLine],
    source_line: usize,
) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.line_number == source_line)
}

fn line_number_column_width(max_line_number: usize) -> usize {
    max_line_number.to_string().len().max(4)
}

fn preview_status(preview: &FilePreview) -> String {
    let coverage = match preview.coverage {
        PreviewCoverage::Complete => "complete",
        PreviewCoverage::BeginningOnly => "beginning only",
        PreviewCoverage::MatchContextOnly => "match context",
        PreviewCoverage::BinaryUnsupported => "binary unsupported",
        PreviewCoverage::Unsupported => "unsupported",
        PreviewCoverage::ReadError => "read error",
    };
    match (preview.displayed_start_line, preview.displayed_end_line) {
        (Some(start), Some(end)) => format!("{coverage}; showing lines {start}-{end}"),
        _ => coverage.to_string(),
    }
}

fn should_show_file_size(preview: &FilePreview) -> bool {
    preview.metadata.len_bytes > 0 && !matches!(preview.coverage, PreviewCoverage::Complete)
}

fn human_file_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_search::preview::PreviewLine;

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

    #[test]
    fn selected_source_line_maps_to_displayed_vector_index() {
        let lines = vec![
            preview_line(40),
            preview_line(41),
            preview_line(42),
            preview_line(43),
        ];

        assert_eq!(displayed_index_for_source_line(&lines, 42), Some(2));
    }

    #[test]
    fn missing_selected_source_line_returns_no_scroll_target() {
        let lines = vec![preview_line(10), preview_line(11), preview_line(13)];

        assert_eq!(displayed_index_for_source_line(&lines, 12), None);
    }

    #[test]
    fn line_number_width_handles_large_line_numbers() {
        assert_eq!(line_number_column_width(9), 4);
        assert_eq!(line_number_column_width(1_000), 4);
        assert_eq!(line_number_column_width(1_000_000), 7);
    }

    fn preview_line(line_number: usize) -> PreviewLine {
        PreviewLine {
            line_number,
            text: format!("line {line_number}"),
            match_ranges: Vec::new(),
        }
    }
}
