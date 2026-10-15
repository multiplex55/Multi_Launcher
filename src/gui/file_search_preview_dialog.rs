use crate::file_search::actions::{execute_explorer_action, resolve_explorer_action};
use crate::file_search::model::ContentMatch;
use crate::file_search::preview::{
    FilePreview, PreviewCoverage, PreviewKind, PreviewLine, PreviewLoadOutcome,
    PreviewLoadStateMachine, PreviewLoadingState, PreviewRequest, preview_file,
};
use crate::file_search::settings::FileSearchSettings;
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use eframe::egui;
use eframe::egui::{FontId, RichText, TextStyle};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

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
            PreviewKind::Text => {
                self.render_text_preview(ui, &preview);
            }
            _ => {
                widgets::semantic_status(
                    ui,
                    if preview.kind == PreviewKind::Error {
                        StatusTone::Error
                    } else {
                        StatusTone::Warning
                    },
                    "Text preview unavailable",
                    Some("Preview is not available as text."),
                );
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
            widgets::compact_section_heading(ui, "Preview", None);
            let path = request.path.display().to_string();
            ui.add(egui::Label::new(RichText::new(&path).strong()).wrap());
            ui.horizontal_wrapped(|ui| {
                if let Some(line) = self.selected_match.as_ref().map(|m| m.line_number) {
                    widgets::secondary_help(ui, &format!("Selected line: {line}"));
                }
                if let Some(preview) = &self.loaded_preview {
                    widgets::semantic_status(
                        ui,
                        preview_status_tone(preview),
                        "Preview status",
                        Some(&preview_status(preview)),
                    );
                    if should_show_file_size(preview) {
                        widgets::secondary_help(
                            ui,
                            &format!("Size: {}", human_file_size(preview.metadata.len_bytes)),
                        );
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                let open = ui.button("Open in Explorer");
                if widgets::with_button_emphasis(ui, open, ButtonEmphasis::Primary).clicked() {
                    match resolve_explorer_action(&request.path).and_then(execute_explorer_action) {
                        Ok(()) => self.action_error_message = None,
                        Err(error) => self.action_error_message = Some(error.to_string()),
                    }
                }
                if ui.button("Copy Full Path").clicked() {
                    ui.ctx().output_mut(|output| {
                        output.commands.push(egui::OutputCommand::CopyText(
                            request.path.display().to_string(),
                        ));
                    });
                    self.action_error_message = None;
                }
                if ui.button("Copy Matching Line").clicked() {
                    if let Some(line) = self.selected_match.as_ref().map(|m| m.line.clone()) {
                        ui.ctx().output_mut(|output| {
                            output.commands.push(egui::OutputCommand::CopyText(line));
                        });
                        self.action_error_message = None;
                    } else {
                        self.action_error_message =
                            Some("No matching-line text is available to copy.".to_string());
                    }
                }
            });
        });
        if let Some(error) = &self.action_error_message {
            widgets::semantic_status(ui, StatusTone::Error, "Action failed", Some(error));
        }
        if let Some(error) = &self.load_error_message {
            widgets::semantic_status(ui, StatusTone::Error, "Preview failed", Some(error));
        }
        if let Some(preview) = &self.loaded_preview {
            for warning in &preview.warnings {
                widgets::semantic_status(ui, StatusTone::Warning, "Preview warning", Some(warning));
            }
        }
    }

    fn render_text_preview(
        &mut self,
        ui: &mut egui::Ui,
        preview: &FilePreview,
    ) -> egui::scroll_area::ScrollAreaOutput<()> {
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
        let (font_id, row_height) = preview_row_metrics(ui);

        let output = egui::ScrollArea::both()
            .id_salt(preview_scroll_id_source(path))
            .auto_shrink([false, false])
            .show_rows(ui, row_height, row_count, |ui, row_range| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for row_index in row_range {
                    let Some(line) = preview.lines.get(row_index) else {
                        continue;
                    };
                    let is_selected = Some(line.line_number) == selected_line;
                    let response = render_preview_line(
                        ui,
                        line,
                        line_number_width,
                        &font_id,
                        row_height,
                        is_selected,
                    );
                    if scroll_target == Some(row_index) {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            });
        if self.pending_auto_scroll {
            self.pending_auto_scroll = false;
        }
        output
    }
}

fn preview_row_metrics(ui: &mut egui::Ui) -> (FontId, f32) {
    let font_id = resolved_preview_font(ui);
    let extra_line_spacing = ui.spacing().extra_text_line_spacing;
    let height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + extra_line_spacing)
        .max(ui.spacing().interact_size.y);
    (font_id, height)
}

fn render_preview_line(
    ui: &mut egui::Ui,
    line: &PreviewLine,
    line_number_width: usize,
    font_id: &FontId,
    row_height: f32,
    selected: bool,
) -> egui::Response {
    let fill = selected.then(|| ui.visuals().selection.bg_fill);
    egui::Frame::NONE
        .fill(fill.unwrap_or(egui::Color32::TRANSPARENT))
        .show(ui, |ui| {
            ui.set_min_height(row_height);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{:>width$}",
                        line.line_number,
                        width = line_number_width
                    ))
                    .font(font_id.clone()),
                );
                ui.label(RichText::new("  ").font(font_id.clone()));
                ui.label(RichText::new(&line.text).font(font_id.clone()));
            });
        })
        .response
}

fn resolved_preview_font(ui: &egui::Ui) -> FontId {
    let style = ui.style();
    style.override_font_id.clone().unwrap_or_else(|| {
        style
            .override_text_style
            .clone()
            .unwrap_or(TextStyle::Monospace)
            .resolve(style)
    })
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

fn preview_status_tone(preview: &FilePreview) -> StatusTone {
    match preview.coverage {
        PreviewCoverage::Complete => StatusTone::Success,
        PreviewCoverage::BeginningOnly | PreviewCoverage::MatchContextOnly => StatusTone::Warning,
        PreviewCoverage::BinaryUnsupported
        | PreviewCoverage::Unsupported
        | PreviewCoverage::ReadError => StatusTone::Error,
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

    #[test]
    fn scaled_preview_rows_use_measured_font_height_without_changing_request_payload() {
        let path = PathBuf::from(
            r"G:\projects\a-long-preview-path\with-several\nested\directories\and\source.rs",
        );
        let selected_match = ContentMatch::new(42, "needle in source".into(), 10, 16);
        let request = PreviewRequest::for_match(&path, 42, 10);
        let preview = FilePreview {
            path: path.clone(),
            kind: PreviewKind::Text,
            lines: vec![
                preview_line(40),
                preview_line(41),
                preview_line(42),
                preview_line(43),
            ],
            metadata: crate::file_search::preview::PreviewMetadata {
                is_directory: false,
                len_bytes: 48,
                modified: None,
                readonly: false,
                sampled_bytes: 48,
            },
            binary_or_unsupported: None,
            error: None,
            coverage: PreviewCoverage::Complete,
            displayed_start_line: Some(40),
            displayed_end_line: Some(43),
            warnings: Vec::new(),
            lossy_utf8: false,
            cache_hit: false,
        };
        let mut state = FileSearchPreviewDialogState {
            current_request: Some(request.clone()),
            loaded_preview: Some(preview.clone()),
            selected_match: Some(selected_match.clone()),
            pending_auto_scroll: true,
            ..Default::default()
        };
        let mut rendered = None;
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(520.0, 440.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.style_mut().override_font_id =
                    Some(FontId::new(25.0, egui::FontFamily::Monospace));
                let (font_id, row_height) = preview_row_metrics(ui);
                let vertical_spacing = ui.spacing().item_spacing.y;
                let interaction_height = ui.spacing().interact_size.y;
                let row_rects = ui
                    .vertical(|ui| {
                        let first = render_preview_line(
                            ui,
                            &preview.lines[0],
                            4,
                            &font_id,
                            row_height,
                            false,
                        );
                        let second = render_preview_line(
                            ui,
                            &preview.lines[1],
                            4,
                            &font_id,
                            row_height,
                            false,
                        );
                        [first.rect, second.rect]
                    })
                    .inner;
                state.render_header(ui, &request, &FileSearchSettings::default());
                ui.separator();
                let scroll = state.render_text_preview(ui, &preview);
                rendered = Some((
                    font_id,
                    row_height,
                    vertical_spacing,
                    interaction_height,
                    row_rects,
                    scroll.content_size,
                    scroll.inner_rect,
                ));
            },
        );
        output.drop_without_applying_deltas();

        let (
            font_id,
            row_height,
            vertical_spacing,
            interaction_height,
            row_rects,
            content_size,
            inner_rect,
        ) = rendered.expect("preview header and rows render");
        assert_eq!(font_id.size, 25.0);
        assert!(row_height > 0.0);
        assert!(row_height >= interaction_height);
        assert!((row_rects[0].height() - row_height).abs() <= 0.5);
        assert!(
            ((row_rects[1].min.y - row_rects[0].min.y) - (row_height + vertical_spacing)).abs()
                <= 0.5,
            "actual row rects follow the measured row height plus layout spacing"
        );
        assert!(inner_rect.is_positive());
        assert!(content_size.x > 0.0);
        assert!(
            content_size.y + 1.0 >= row_height * preview.lines.len() as f32,
            "the scroll content includes every measured row"
        );
        assert_eq!(state.current_request, Some(request));
        assert_eq!(state.selected_match, Some(selected_match));
        assert_eq!(state.loaded_preview, Some(preview));
        assert!(!state.pending_auto_scroll);
    }

    fn preview_line(line_number: usize) -> PreviewLine {
        PreviewLine {
            line_number,
            text: format!("line {line_number}"),
            match_ranges: Vec::new(),
        }
    }
}
