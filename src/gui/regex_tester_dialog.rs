use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::regex_tester::{EvaluationLimit, EvaluationResult, MatchCompleteness, RegexSession};
use eframe::egui;
use std::sync::Arc;
use std::time::Instant;
mod clipboard;
mod editor_layout;
mod examples;
mod explanation;
mod highlighting;
mod history;
mod inspection;
mod presets;
#[cfg(test)]
mod profiling;
mod reference;
mod substitution;

fn copy_text(
    clipboard: &dyn ClipboardBackend,
    text: &str,
    success: &'static str,
) -> Result<&'static str, String> {
    clipboard
        .write_text(text)
        .map(|()| success)
        .map_err(|error| format!("Could not copy: {}", clipboard::error_message(&error)))
}

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Regex Tester")
        .with_inner_size([960.0, 680.0])
        .with_min_inner_size([360.0, 240.0])
        .with_resizable(true)
}

fn utf8_prefix(text: &str, maximum_bytes: usize) -> &str {
    let mut end = text.len().min(maximum_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Fixed allocations stop editor scroll extents from growing the native window.
fn bounded_area(ui: &mut egui::Ui, rect: egui::Rect, id: &str, render: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_rect(rect, egui::Sense::hover());
    let mut child = ui.child_ui_with_id_source(rect, egui::Layout::top_down(egui::Align::Min), id);
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    render(&mut child);
}

#[derive(Clone, Copy, Debug)]
struct CoreLayout {
    header: egui::Rect,
    validation: Option<egui::Rect>,
    editor: egui::Rect,
    information: Option<egui::Rect>,
    status: egui::Rect,
}

impl CoreLayout {
    fn new(bounds: egui::Rect, information_open: bool, invalid_pattern: bool) -> Self {
        let header_height = 100.0_f32.min(bounds.height() * 0.45);
        let status_height = 24.0_f32.min(bounds.height() * 0.15);
        let gap = 8.0_f32.min(bounds.height() * 0.03);
        let header =
            egui::Rect::from_min_size(bounds.min, egui::vec2(bounds.width(), header_height));
        let status = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), bounds.bottom() - status_height),
            bounds.max,
        );
        let validation = invalid_pattern.then(|| {
            egui::Rect::from_min_size(
                egui::pos2(bounds.left(), header.bottom()),
                egui::vec2(bounds.width(), 80.0_f32.min(bounds.height() * 0.16)),
            )
        });
        let content_top = validation.map_or(header.bottom(), |area| area.bottom());
        let content = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), content_top + gap),
            egui::pos2(bounds.right(), status.top() - gap),
        );
        let (editor, information) = if !information_open {
            (content, None)
        } else if bounds.width() >= 680.0 {
            let info_width = (bounds.width() * 0.3).clamp(220.0, 300.0);
            let split = content.right() - info_width;
            (
                egui::Rect::from_min_max(content.min, egui::pos2(split - gap, content.bottom())),
                Some(egui::Rect::from_min_max(
                    egui::pos2(split, content.top()),
                    content.max,
                )),
            )
        } else {
            let info_height = (content.height() * 0.3).min(110.0);
            let split = content.bottom() - info_height;
            (
                egui::Rect::from_min_max(content.min, egui::pos2(content.right(), split - gap)),
                Some(egui::Rect::from_min_max(
                    egui::pos2(content.left(), split),
                    content.max,
                )),
            )
        };
        Self {
            header,
            validation,
            editor,
            information,
            status,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum InformationSection {
    #[default]
    Matches,
    MatchDetails,
    Explanation,
    Reference,
    Examples,
    History,
    Presets,
    Substitution,
    Clipboard,
}

/// Session-owned inputs survive closing the utility; persistence is explicit.
pub struct RegexTesterDialogState {
    pub open: bool,
    pub session: RegexSession,
    focus_pattern: bool,
    focus_viewport: bool,
    information_open: bool,
    scroll_to_selected: bool,
    information_section: InformationSection,
    reference: reference::ReferenceState,
    examples: examples::ExamplesState,
    history: Option<history::HistoryState>,
    presets: Option<presets::PresetState>,
    selected_capture: usize,
    clipboard: Arc<dyn ClipboardBackend>,
    copy_feedback: Option<Result<&'static str, String>>,
}

impl Default for RegexTesterDialogState {
    fn default() -> Self {
        Self::with_clipboard(Arc::new(ArboardClipboardBackend))
    }
}

impl RegexTesterDialogState {
    pub fn with_clipboard(clipboard: Arc<dyn ClipboardBackend>) -> Self {
        Self {
            open: false,
            session: RegexSession::default(),
            focus_pattern: false,
            focus_viewport: false,
            information_open: true,
            scroll_to_selected: false,
            information_section: InformationSection::default(),
            reference: reference::ReferenceState::default(),
            examples: examples::ExamplesState::default(),
            history: None,
            presets: None,
            selected_capture: 0,
            clipboard,
            copy_feedback: None,
        }
    }

    pub fn open(&mut self) {
        self.session.ensure_initial_evaluation(Instant::now());
        self.focus_viewport = true;
        if !self.open {
            self.focus_pattern = true;
            self.open = true;
        }
    }

    /// Explicit production storage configuration; default construction does no IO.
    pub fn with_storage_paths(
        history_path: impl Into<std::path::PathBuf>,
        preset_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        let mut dialog = Self::default();
        dialog.history = Some(history::HistoryState::open(history_path));
        dialog.presets = Some(presets::PresetState::open(preset_path));
        dialog
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("regex_tester_viewport"),
            viewport_builder(),
            |child, class| {
                let independent = class == egui::ViewportClass::Immediate;
                if independent && self.focus_viewport {
                    child.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.focus_viewport = false;
                let close = child.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        || input.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        || (independent && input.viewport().close_requested())
                });
                if close {
                    self.open = false;
                } else if independent {
                    egui::CentralPanel::default().show(child, |ui| self.body(ui));
                } else {
                    // Standalone egui contexts embed viewports. Keep that fallback
                    // within its parent rather than changing root-window geometry.
                    let size = child.input(|input| input.screen_rect().size());
                    let available = (size - egui::vec2(24.0, 48.0)).max(egui::vec2(1.0, 1.0));
                    let mut open = self.open;
                    egui::Window::new("Regex Tester")
                        .id(egui::Id::new("regex_tester_embedded_window"))
                        .open(&mut open)
                        .collapsible(false)
                        .resizable(true)
                        .default_size(available.min(egui::vec2(960.0, 680.0)))
                        .min_size(available.min(egui::vec2(360.0, 240.0)))
                        .max_size(available)
                        .show(child, |ui| self.body(ui));
                    self.open = open;
                }
                if independent && !self.open {
                    child.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
    }

    fn body(&mut self, ui: &mut egui::Ui) -> CoreLayout {
        let now = Instant::now();
        self.session.ensure_initial_evaluation(now);
        let bounds = ui.available_rect_before_wrap();
        let had_error = self.validation_error().is_some();
        // Recompute after the toggle so collapsing frees editor space this frame.
        let header = CoreLayout::new(bounds, self.information_open, had_error).header;
        bounded_area(ui, header, "regex_tester_header", |ui| {
            self.pattern_area(ui)
        });
        let layout = CoreLayout::new(bounds, self.information_open, had_error);
        bounded_area(ui, layout.editor, "regex_tester_editor_area", |ui| {
            self.text_area(ui)
        });
        if self.session.tick(now) {
            self.record_history();
            self.scroll_to_selected = false;
            ui.ctx().request_repaint();
        }
        if let Some(validation) = layout.validation {
            bounded_area(ui, validation, "regex_tester_validation", |ui| {
                self.validation_area(ui)
            });
        }
        if had_error != self.validation_error().is_some() {
            ui.ctx().request_repaint();
        }
        if let Some(information) = layout.information {
            bounded_area(ui, information, "regex_tester_information_area", |ui| {
                self.information_area(ui)
            });
        }
        bounded_area(ui, layout.status, "regex_tester_status", |ui| {
            self.status_area(ui)
        });
        if let Some(delay) = self.session.pending_delay(Instant::now()) {
            ui.ctx().request_repaint_after(delay);
        }
        layout
    }

    fn pattern_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.weak("Engine: Rust regex · all matches");
            let label = if self.information_open {
                "Hide info"
            } else {
                "Show info"
            };
            if ui
                .button(label)
                .on_hover_text("Show or hide match information")
                .clicked()
            {
                self.information_open = !self.information_open;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Pattern");
            if self.session.draft.pattern.len() > self.session.policy().pattern_bytes {
                self.oversized_pattern_area(ui);
                return;
            }
            if self.session.draft.pattern.len() > crate::regex_tester::policy::NORMAL_PATTERN_BYTES {
                ui.colored_label(ui.visuals().warn_fg_color, "Large").on_hover_text("Pattern exceeds the normal 1 KiB range; interactive evaluation remains bounded at 4 KiB.");
            }
            ui.label(egui::RichText::new("/").monospace().size(18.0));
            let suffix = format!("/{}", self.session.draft.flags.suffix());
            let suffix_width = ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(
                        suffix.clone(),
                        egui::FontId::monospace(18.0),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x
            });
            let width =
                (ui.available_width() - suffix_width - ui.spacing().item_spacing.x).max(1.0);
            let maximum_bytes = self.session.policy().pattern_bytes;
            let mut layouter = |ui: &egui::Ui, text: &str, width: f32| {
                editor_layout::plain(ui, text, width, maximum_bytes, egui::FontId::monospace(18.0), true)
            };
            let pattern = ui.add_sized(
                [width, 28.0],
                egui::TextEdit::singleline(&mut self.session.draft.pattern)
                    .id(egui::Id::new("regex_tester_pattern"))
                    .font(egui::FontId::monospace(18.0))
                    .layouter(&mut layouter)
                    .hint_text("Regular expression"),
            );
            if pattern.changed() {
                self.mark_changed();
            }
            if self.focus_pattern {
                pattern.request_focus();
                self.focus_pattern = false;
            }
            ui.label(egui::RichText::new(suffix).monospace().size(18.0));
        });
        ui.horizontal(|ui| self.flags_area(ui));
    }

    fn flags_area(&mut self, ui: &mut egui::Ui) -> [egui::Response; 5] {
        ui.label("Flags");
        let responses = [
            (
                "i",
                &mut self.session.draft.flags.case_insensitive,
                "Case insensitive",
            ),
            (
                "m",
                &mut self.session.draft.flags.multi_line,
                "Multiline: ^ and $ match line boundaries",
            ),
            (
                "s",
                &mut self.session.draft.flags.dot_matches_new_line,
                "Dot matches newline",
            ),
            (
                "u",
                &mut self.session.draft.flags.unicode,
                "Unicode character classes and matching",
            ),
            (
                "x",
                &mut self.session.draft.flags.ignore_whitespace,
                "Ignore pattern whitespace and allow comments",
            ),
        ]
        .map(|(letter, enabled, hint)| ui.checkbox(enabled, letter).on_hover_text(hint));
        if responses.iter().any(egui::Response::changed) {
            self.mark_changed();
        }
        responses
    }

    fn oversized_pattern_area(&mut self, ui: &mut egui::Ui) {
        let width = (ui.available_width() - 100.0).max(1.0);
        let mut preview = utf8_prefix(&self.session.draft.pattern, 128);
        ui.add_sized(
            [width, 28.0],
            egui::TextEdit::singleline(&mut preview)
                .id(egui::Id::new("regex_tester_pattern_preview"))
                .font(egui::TextStyle::Monospace)
                .interactive(false),
        ).on_hover_text(format!(
            "Read-only preview: first 128 bytes or fewer. Pattern is {} bytes; editing and evaluation pause above {} bytes. Original pattern is retained.",
            self.session.draft.pattern.len(), self.session.policy().pattern_bytes
        ));
        if ui.button("Clear pattern").clicked() {
            self.session.draft.pattern.clear();
            self.mark_changed();
            self.focus_pattern = true;
        }
    }

    fn text_area(&mut self, ui: &mut egui::Ui) {
        ui.label("Input / test text");
        if self.session.draft.test_text.len() > self.session.policy().text_bytes {
            self.oversized_text_area(ui);
            return;
        }
        if self.session.draft.test_text.len() > crate::regex_tester::policy::NORMAL_TEXT_BYTES {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Large input: above the normal 16 KiB range.",
            );
        }
        let maximum_bytes = self.session.policy().text_bytes;
        let editor_size = ui.available_size().max(egui::vec2(1.0, 1.0));
        egui::ScrollArea::both()
            .id_source("regex_tester_text_scroll")
            .auto_shrink([false, false])
            .max_height(editor_size.y)
            .show(ui, |ui| {
                let changed = {
                    let (text, view) = self.session.text_edit_parts();
                    let mut layouter = |ui: &egui::Ui, text: &str, width: f32| {
                        ui.fonts(|fonts| {
                            fonts.layout_job(
                                editor_layout::oversized_job(text, maximum_bytes).unwrap_or_else(
                                    || highlighting::layout(text, &view, ui, width),
                                ),
                            )
                        })
                    };
                    let output = egui::TextEdit::multiline(text)
                        .id(egui::Id::new("regex_tester_test_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(1)
                        .desired_width(editor_size.x)
                        .min_size(editor_size)
                        .layouter(&mut layouter)
                        .hint_text("Type or paste text to test")
                        .show(ui);
                    highlighting::paint_markers(ui, &output, text, &view);
                    if std::mem::take(&mut self.scroll_to_selected) {
                        if let Some(caret) = highlighting::active_caret(&output, text, &view) {
                            ui.scroll_to_rect(caret.expand(4.0), Some(egui::Align::Center));
                        }
                    }
                    output.response.changed()
                };
                if changed {
                    self.mark_changed();
                }
            });
    }

    fn mark_changed(&mut self) {
        self.scroll_to_selected = false;
        self.copy_feedback = None;
        self.session.mark_changed(Instant::now());
    }

    fn request_match_scroll(&mut self, ui: &egui::Ui) {
        self.scroll_to_selected = true;
        ui.ctx().request_repaint();
    }

    fn information_area(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_source("regex_information_section")
            .width(ui.available_width().min(220.0))
            .selected_text(match self.information_section {
                InformationSection::Matches => "Matches",
                InformationSection::MatchDetails => "Selected match",
                InformationSection::Explanation => "Explanation",
                InformationSection::Reference => "Reference",
                InformationSection::Examples => "Examples",
                InformationSection::History => "History",
                InformationSection::Presets => "Presets",
                InformationSection::Substitution => "Substitution",
                InformationSection::Clipboard => "Clipboard",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Matches,
                    "Matches",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::MatchDetails,
                    "Selected match",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Explanation,
                    "Explanation",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Reference,
                    "Reference",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Examples,
                    "Examples",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::History,
                    "History",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Presets,
                    "Presets",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Substitution,
                    "Substitution",
                );
                ui.selectable_value(
                    &mut self.information_section,
                    InformationSection::Clipboard,
                    "Clipboard",
                );
            });
        if let Some(feedback) = &self.copy_feedback {
            match feedback {
                Ok(message) => {
                    ui.weak(*message);
                }
                Err(message) => {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
            }
        }
        if self.information_section == InformationSection::MatchDetails {
            self.match_details_area(ui);
            return;
        }
        if self.information_section == InformationSection::Explanation {
            if let Some(result) = self.session.explanation() {
                explanation::show(ui, result);
            } else {
                // Compiler validation is authoritative; pending revisions have
                // no accessible stale analysis.
                ui.weak(self.summary());
            }
            return;
        }
        if self.information_section == InformationSection::Reference {
            if let Some(action) = self.reference.show(ui) {
                self.reference_action(action);
            }
            return;
        }
        if self.information_section == InformationSection::Examples {
            if let Some(example) = self.examples.show(ui) {
                self.load_example(example);
            }
            return;
        }
        if self.information_section == InformationSection::History {
            if let Some(history) = &mut self.history {
                let retryable = matches!(
                    self.session.result(),
                    Some(EvaluationResult::Success { .. })
                ) && !self.session.draft.pattern.is_empty();
                if let Some(action) = history.show(ui, retryable) {
                    self.history_action(action);
                }
            } else {
                ui.weak("History storage is not configured.");
            }
            return;
        }
        if self.information_section == InformationSection::Presets {
            if let Some(presets) = &mut self.presets {
                if let Some(action) = presets.show(ui) {
                    self.preset_action(action);
                }
            } else {
                ui.weak("Preset storage is not configured.");
            }
            return;
        }
        if self.information_section == InformationSection::Substitution {
            if substitution::show(ui, &mut self.session) {
                self.copy_feedback = None;
            }
            return;
        }
        if self.information_section == InformationSection::Clipboard {
            if let Some(action) = clipboard::show(ui, &self.session, self.selected_capture) {
                self.clipboard_action(action);
            }
            return;
        }

        let count = self.session.matches().len();
        if count == 0 {
            ui.weak(self.summary());
            return;
        }
        let mut selected = None;
        let row_height = ui.spacing().interact_size.y;
        egui::ScrollArea::vertical()
            .id_source("regex_tester_information_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show_rows(ui, row_height, count, |ui, rows| {
                ui.style_mut().wrap = Some(false);
                for index in rows {
                    let matched = &self.session.matches()[index];
                    let preview = utf8_prefix(&matched.text, 80)
                        .replace('\r', "␍")
                        .replace('\n', "↵");
                    let label = format!(
                        "#{} · {}:{} · {}",
                        index + 1,
                        matched.location.line,
                        matched.location.column,
                        if preview.is_empty() {
                            "(zero width)"
                        } else {
                            &preview
                        }
                    );
                    if ui
                        .selectable_label(self.session.selected_index() == Some(index), label)
                        .clicked()
                    {
                        selected = Some(index);
                    }
                }
            });
        if let Some(index) = selected {
            self.session.select_match(index);
            self.request_match_scroll(ui);
        }
    }

    fn match_details_area(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.session.selected_index() else {
            ui.weak(self.summary());
            return;
        };
        let mut copy = None;
        egui::ScrollArea::vertical()
            .id_source("regex_match_details_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                copy = inspection::show(
                    ui,
                    &self.session.matches()[index],
                    &mut self.selected_capture,
                );
            });
        if let Some(target) = copy {
            self.copy_selected(target);
        }
    }

    fn copy_selected(&mut self, target: inspection::CopyTarget) {
        let text = self
            .session
            .selected_index()
            .and_then(|index| self.session.matches().get(index))
            .and_then(|matched| inspection::copy_text(matched, target));
        self.copy_feedback = Some(match text {
            Some(text) => copy_text(
                self.clipboard.as_ref(),
                text,
                match target {
                    inspection::CopyTarget::Match => "Copied match",
                    inspection::CopyTarget::Capture(_) => "Copied capture",
                },
            ),
            None => Err("The selected match or capture has no matched value".into()),
        });
    }

    fn clipboard_action(&mut self, action: clipboard::ClipboardAction) {
        use clipboard::ClipboardAction as Action;
        match action {
            Action::ImportText => match self.clipboard.read_text() {
                Ok(text) => {
                    self.session.draft.test_text = text;
                    self.mark_changed();
                    self.copy_feedback = Some(Ok("Imported clipboard text"));
                }
                Err(error) => {
                    self.copy_feedback = Some(Err(format!(
                        "Could not import: {}",
                        clipboard::error_message(&error)
                    )));
                }
            },
            Action::CopyMatch => self.copy_selected(inspection::CopyTarget::Match),
            Action::CopyCapture(index) => {
                self.copy_selected(inspection::CopyTarget::Capture(index))
            }
            Action::CopyPattern => {
                self.copy_feedback = Some(copy_text(
                    self.clipboard.as_ref(),
                    &self.session.draft.pattern,
                    "Copied pattern",
                ));
            }
            Action::CopyAllMatches => {
                let matches = self.session.matches();
                self.copy_feedback = Some(if matches.is_empty() {
                    Err("No current matches to copy.".into())
                } else {
                    let text = clipboard::all_matches(matches);
                    copy_text(self.clipboard.as_ref(), &text, "Copied displayed matches")
                });
            }
            Action::CopyReplacementResult => {
                self.copy_feedback =
                    Some(match self.session.substitution_result() {
                        Some(crate::regex_tester::SubstitutionEvaluationResult::Success(
                            result,
                        )) if self.session.substitution_enabled() => copy_text(
                            self.clipboard.as_ref(),
                            &result.output,
                            "Copied replacement result",
                        ),
                        _ => Err("No complete current replacement result to copy.".into()),
                    });
            }
            Action::CopyMatchInformation => {
                self.copy_feedback = Some(
                    match self
                        .session
                        .selected_index()
                        .and_then(|index| self.session.matches().get(index))
                    {
                        Some(matched) => {
                            let text = clipboard::match_information(matched);
                            copy_text(self.clipboard.as_ref(), &text, "Copied match information")
                        }
                        None => Err("No current selected match information to copy.".into()),
                    },
                );
            }
        }
    }

    fn reference_action(&mut self, action: reference::ReferenceAction) {
        match action {
            reference::ReferenceAction::Append(entry) => {
                self.session.draft.pattern.push_str(entry.syntax);
                self.mark_changed();
            }
            reference::ReferenceAction::Copy(entry) => {
                self.copy_feedback = Some(copy_text(
                    self.clipboard.as_ref(),
                    entry.syntax,
                    "Copied syntax",
                ));
            }
        }
    }

    fn load_example(&mut self, example: &crate::regex_tester::RegexExample) {
        self.session.draft = example.to_draft();
        self.mark_changed();
    }

    fn record_history(&mut self) {
        if let Some(history) = &mut self.history {
            history.record_success(&self.session);
        }
    }

    fn history_action(&mut self, action: history::HistoryAction) {
        let Some(history) = &mut self.history else {
            return;
        };
        match action {
            history::HistoryAction::Load(index) => {
                if let Some(entry) = history.store.entries().get(index) {
                    self.session.draft.pattern.clone_from(&entry.pattern);
                    self.session.draft.flags = entry.flags;
                    self.mark_changed();
                }
            }
            history::HistoryAction::Reload => {
                let _ = history.store.reload();
            }
            history::HistoryAction::Retry => {
                history.retry(&self.session);
            }
        }
    }

    fn preset_action(&mut self, action: presets::PresetAction) {
        if self
            .presets
            .as_mut()
            .is_some_and(|presets| presets.handle(action, &mut self.session.draft))
        {
            self.mark_changed();
        }
    }

    fn validation_error(&self) -> Option<&str> {
        match self.session.result() {
            Some(EvaluationResult::InvalidPattern(error)) => Some(&error.message),
            _ => None,
        }
    }

    fn validation_area(&self, ui: &mut egui::Ui) {
        let Some(message) = self.validation_error() else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_source("regex_tester_validation_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(message).color(ui.visuals().error_fg_color),
                    )
                    .wrap(true),
                );
            });
    }

    fn oversized_text_area(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_source("regex_tester_oversized_preview_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.colored_label(ui.visuals().warn_fg_color, format!(
                    "Text is {} bytes; live editing and evaluation pause above {} bytes. Original text is retained.",
                    self.session.draft.test_text.len(), self.session.policy().text_bytes
                ));
                if ui.button("Clear text to resume editing").clicked() {
                    self.session.draft.test_text.clear();
                    self.mark_changed();
                    return;
                }
                ui.weak("Read-only preview: first 4096 bytes or fewer");
                ui.add(egui::Label::new(egui::RichText::new(self.preview_text()).monospace()).wrap(true));
            });
    }

    fn preview_text(&self) -> &str {
        utf8_prefix(&self.session.draft.test_text, 4096)
    }

    fn summary(&self) -> String {
        match self.session.result() {
            None => "Waiting for edits to settle".into(),
            Some(EvaluationResult::Success {
                matches,
                completeness: MatchCompleteness::Complete,
            }) => format!("{} matches", matches.len()),
            Some(EvaluationResult::Success {
                matches,
                completeness: MatchCompleteness::Truncated { at_least, .. },
            }) => format!("At least {at_least} matches; showing {}", matches.len()),
            Some(EvaluationResult::InvalidPattern(_)) => "Invalid pattern".into(),
            Some(EvaluationResult::Suspended(limit)) => {
                let reason = match limit.limit {
                    EvaluationLimit::PatternBytes => "pattern size",
                    EvaluationLimit::TextBytes => "text size",
                    EvaluationLimit::CaptureGroups => "capture group count",
                    EvaluationLimit::StoredMatches => "stored match count",
                    EvaluationLimit::MaterializedBytes => "result size",
                    EvaluationLimit::ReplacementBytes => "replacement size",
                    EvaluationLimit::ReplacementOutputBytes => "replacement output size",
                };
                format!("Evaluation paused: {reason} exceeds {}", limit.maximum)
            }
        }
    }

    fn navigation_summary(&self) -> String {
        let Some(index) = self.session.selected_index() else {
            return self.summary();
        };
        let displayed = if matches!(
            self.session.result(),
            Some(EvaluationResult::Success {
                completeness: MatchCompleteness::Truncated { .. },
                ..
            })
        ) {
            " displayed"
        } else {
            ""
        };
        format!(
            "Match {} of {}{displayed}",
            index + 1,
            self.session.matches().len()
        )
    }

    fn status_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let enabled = !self.session.matches().is_empty();
            if ui
                .add_enabled(enabled, egui::Button::new("Previous"))
                .clicked()
                && self.session.previous_match()
            {
                self.request_match_scroll(ui);
            }
            if ui.add_enabled(enabled, egui::Button::new("Next")).clicked()
                && self.session.next_match()
            {
                self.request_match_scroll(ui);
            }
            ui.weak(self.navigation_summary())
                .on_hover_text(self.summary());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_tester_same_frame_paste_retains_full_unicode_buffers_in_all_three_editors() {
        let text = "🦀".repeat(256 * 1024);
        for (editor, id) in [
            (0, "regex_tester_pattern"),
            (1, "regex_tester_test_text"),
            (2, "regex_substitution_replacement"),
        ] {
            let mut dialog = RegexTesterDialogState::default();
            dialog.information_section = InformationSection::Substitution;
            let now = Instant::now();
            dialog.session.set_substitution_enabled(true, now);
            dialog
                .session
                .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            let ctx = egui::Context::default();
            let run = |dialog: &mut RegexTesterDialogState, events: Vec<egui::Event>| {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(960.0, 680.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            dialog.body(ui);
                        });
                    },
                );
            };
            run(&mut dialog, Vec::new());
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
            run(&mut dialog, vec![egui::Event::Paste(text.clone())]);
            let buffer = |dialog: &RegexTesterDialogState| {
                match editor {
                    0 => dialog.session.draft.pattern.as_str(),
                    1 => dialog.session.draft.test_text.as_str(),
                    _ => dialog.session.draft.replacement.as_str(),
                }
                .to_owned()
            };
            assert_eq!(buffer(&dialog), text, "editor {editor} dropped pasted data");
            assert!(dialog.session.pending_delay(Instant::now()).is_none());
            if editor == 2 {
                assert!(matches!(
                    dialog.session.substitution_result(),
                    Some(crate::regex_tester::SubstitutionEvaluationResult::Suspended(_))
                ));
            } else {
                assert!(matches!(
                    dialog.session.result(),
                    Some(EvaluationResult::Suspended(_))
                ));
            }
            run(&mut dialog, Vec::new());
            assert_eq!(buffer(&dialog), text);
            assert!(
                !dialog
                    .session
                    .tick(Instant::now() + std::time::Duration::from_secs(1))
            );
            // Explicit recovery preserves normal editing through the same ID.
            match editor {
                0 => dialog.session.draft.pattern.clear(),
                1 => dialog.session.draft.test_text.clear(),
                _ => dialog.session.draft.replacement.clear(),
            }
            if editor == 2 {
                dialog.session.mark_replacement_changed(Instant::now());
            } else {
                dialog.session.mark_changed(Instant::now());
            }
            run(&mut dialog, Vec::new());
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
            run(&mut dialog, vec![egui::Event::Paste("é".into())]);
            assert_eq!(buffer(&dialog), "é");
        }
    }

    #[test]
    fn regex_tester_clipboard_full_replacement_copy_requires_current_success_and_render_has_no_io()
    {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.information_section = InformationSection::Clipboard;
        let now = Instant::now();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(600);
        dialog.session.draft.replacement = "🦀".repeat(10);
        dialog.session.set_substitution_enabled(true, now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let Some(crate::regex_tester::SubstitutionEvaluationResult::Success(result)) =
            dialog.session.substitution_result()
        else {
            panic!("expected complete result");
        };
        let full_output = result.output.clone();
        assert!(full_output.len() > substitution::RESULT_PREVIEW_BYTES);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        assert_eq!(*backend.contents.lock().unwrap(), full_output);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        dialog.session.draft.replacement = "Z".into();
        dialog.session.mark_replacement_changed(now);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.set_substitution_enabled(false, now);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.set_substitution_enabled(true, now);
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.replacement = "X".repeat(4000);
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(matches!(
            dialog.session.substitution_result(),
            Some(crate::regex_tester::SubstitutionEvaluationResult::Suspended(_))
        ));
        dialog.clipboard_action(clipboard::ClipboardAction::CopyReplacementResult);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        dialog.open();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            );
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_clipboard_import_reads_once_preserves_other_fields_and_handles_errors_and_limits()
     {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = "/é/".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.draft.test_text = "old private text".into();
        dialog.session.draft.replacement = "$1 replacement".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let old = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        *backend.contents.lock().unwrap() = "é🦀\ninput".into();
        dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
        assert_eq!(dialog.session.draft.test_text, "é🦀\ninput");
        assert_eq!(dialog.session.draft.pattern, old.pattern);
        assert_eq!(dialog.session.draft.flags, old.flags);
        assert_eq!(dialog.session.draft.replacement, old.replacement);
        assert_eq!(dialog.session.revision(), revision + 1);
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert_eq!(*backend.contents.lock().unwrap(), "é🦀\ninput");
        let imported = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        for error in [
            ClipboardError::NonText,
            ClipboardError::Busy("held by another app".into()),
        ] {
            *backend.read_error.lock().unwrap() = Some(error);
            dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
            assert_eq!(dialog.session.draft, imported);
            assert_eq!(dialog.session.revision(), revision);
            assert!(dialog.copy_feedback.as_ref().unwrap().is_err());
            assert_eq!(*backend.contents.lock().unwrap(), "é🦀\ninput");
        }
        assert_eq!(backend.reads.load(Ordering::SeqCst), 3);
        *backend.read_error.lock().unwrap() = None;
        let oversized = "é🦀".repeat(dialog.session.policy().text_bytes);
        *backend.contents.lock().unwrap() = oversized.clone();
        dialog.clipboard_action(clipboard::ClipboardAction::ImportText);
        assert_eq!(dialog.session.draft.test_text, oversized);
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(now).is_none());
        assert_eq!(backend.reads.load(Ordering::SeqCst), 4);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        assert_eq!(
            clipboard::error_message(&ClipboardError::NonText),
            "Clipboard does not contain text."
        );
    }

    #[test]
    fn regex_tester_clipboard_copies_exact_values_information_and_rejects_stale_or_absent_values() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.draft.pattern = "/é/".into();
        dialog.clipboard_action(clipboard::ClipboardAction::CopyPattern);
        assert_eq!(*backend.contents.lock().unwrap(), "/é/");
        dialog.session.draft.pattern = "(?P<letter>é)(🦀)(z)?()".into();
        dialog.session.draft.test_text = "é🦀".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        for (action, expected) in [
            (clipboard::ClipboardAction::CopyMatch, "é🦀"),
            (clipboard::ClipboardAction::CopyCapture(0), "é"),
            (clipboard::ClipboardAction::CopyCapture(3), ""),
            (clipboard::ClipboardAction::CopyAllMatches, "é🦀"),
        ] {
            dialog.clipboard_action(action);
            assert_eq!(*backend.contents.lock().unwrap(), expected);
        }
        dialog.clipboard_action(clipboard::ClipboardAction::CopyMatchInformation);
        let information = backend.contents.lock().unwrap().clone();
        for label in [
            "Line 1, column 1 (Unicode scalars)",
            "UTF-8 bytes 0..6 (end exclusive)",
            "Text:\né🦀",
            "Capture #1 (letter)",
            "Capture #2",
            "Unmatched optional capture",
            "Matched empty capture",
        ] {
            assert!(information.contains(label), "missing {label}");
        }
        let writes = backend.attempts.load(Ordering::SeqCst);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyCapture(2));
        assert_eq!(backend.attempts.load(Ordering::SeqCst), writes);
        assert_eq!(dialog.session.draft, draft);
        dialog.session.draft.pattern = "é|🦀".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(*backend.contents.lock().unwrap(), "é\n🦀");
        dialog.session.draft.pattern = "^|$".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(*backend.contents.lock().unwrap(), "\n");
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert_eq!(
            clipboard::all_matches_label(&dialog.session),
            "Copy displayed matches"
        );
        dialog.clipboard_action(clipboard::ClipboardAction::CopyAllMatches);
        assert_eq!(
            *backend.contents.lock().unwrap(),
            vec!["a"; 1000].join("\n")
        );
        let writes = backend.attempts.load(Ordering::SeqCst);
        dialog.session.mark_changed(now);
        for action in [
            clipboard::ClipboardAction::CopyMatch,
            clipboard::ClipboardAction::CopyCapture(0),
            clipboard::ClipboardAction::CopyAllMatches,
            clipboard::ClipboardAction::CopyMatchInformation,
        ] {
            dialog.clipboard_action(action);
        }
        assert_eq!(backend.attempts.load(Ordering::SeqCst), writes);
        *backend.contents.lock().unwrap() = "keep existing clipboard".into();
        backend.fail.store(true, Ordering::SeqCst);
        dialog.clipboard_action(clipboard::ClipboardAction::CopyPattern);
        assert_eq!(*backend.contents.lock().unwrap(), "keep existing clipboard");
        assert!(
            dialog
                .copy_feedback
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Clipboard is busy")
        );
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_substitution_bounds_readonly_preview_and_retains_oversized_replacement() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.information_section = InformationSection::Substitution;
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(600);
        dialog.session.draft.replacement = "🦀".repeat(10);
        let now = Instant::now();
        dialog.session.set_substitution_enabled(true, now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let Some(crate::regex_tester::SubstitutionEvaluationResult::Success(result)) =
            dialog.session.substitution_result()
        else {
            panic!("expected replacement result");
        };
        assert_eq!(result.output.len(), 24_000);
        assert_eq!(
            substitution::preview(&result.output).len(),
            substitution::RESULT_PREVIEW_BYTES
        );
        assert_eq!(substitution::preview("é🦀"), "é🦀");
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for oversized in [false, true] {
            if oversized {
                dialog.session.draft.replacement =
                    "é🦀".repeat(dialog.session.policy().replacement_bytes);
                dialog.session.mark_replacement_changed(now);
            }
            let retained_replacement = dialog.session.draft.replacement.clone();
            for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let bounds = ui.available_rect_before_wrap();
                            dialog.body(ui);
                            assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                            assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                        });
                    },
                );
            }
            assert_eq!(dialog.session.draft.pattern, draft.pattern);
            assert_eq!(dialog.session.draft.flags, draft.flags);
            assert_eq!(dialog.session.draft.test_text, draft.test_text);
            assert_eq!(dialog.session.draft.replacement, retained_replacement);
            assert_eq!(dialog.session.revision(), revision);
            assert_eq!(dialog.session.matches().len(), 600);
            assert!(dialog.session.pending_delay(now).is_none());
        }
    }

    #[test]
    fn regex_tester_presets_load_once_and_management_render_reopen_preserve_document() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("history.json");
        let preset_path = dir.path().join("presets.json");
        let mut store = crate::regex_tester::PresetStore::open(&preset_path);
        let id = store
            .create(crate::regex_tester::PresetInput {
                name: "Pattern only".into(),
                pattern: "é".into(),
                flags: crate::regex_tester::RegexFlags {
                    case_insensitive: true,
                    ..Default::default()
                },
                sample_text: None,
                replacement: None,
            })
            .unwrap();
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        dialog.clipboard = backend.clone();
        dialog.information_section = InformationSection::Presets;
        let now = Instant::now();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "current private test".into();
        dialog.session.draft.replacement = "current replacement".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let revision = dialog.session.revision();
        dialog.preset_action(presets::PresetAction::CancelDelete);
        dialog.preset_action(presets::PresetAction::Reload);
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.session.result().is_some());
        dialog.scroll_to_selected = true;
        dialog.copy_feedback = Some(Ok("Old feedback"));
        dialog.preset_action(presets::PresetAction::Load(id));
        assert_eq!(dialog.session.revision(), revision + 1);
        assert_eq!(dialog.session.draft.pattern, "é");
        assert!(dialog.session.draft.flags.case_insensitive);
        assert_eq!(dialog.session.draft.test_text, "current private test");
        assert_eq!(dialog.session.draft.replacement, "current replacement");
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert!(!dialog.scroll_to_selected);
        assert!(dialog.copy_feedback.is_none());
        // Settle before checking rendering isolation, as body intentionally
        // performs the initial/pending evaluation once.
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            );
        }
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.information_section == InformationSection::Presets);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_configured_history_records_restarts_and_loads_only_pattern_flags() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("regex_history.json");
        let preset_path = dir.path().join("regex_presets.json");
        let mut dialog = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        assert!(!history_path.exists());
        assert!(!preset_path.exists());
        assert!(dialog.presets.is_some());
        assert!(RegexTesterDialogState::default().history.is_none());
        let now = Instant::now();
        dialog.session.draft.pattern = "é".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.draft.test_text = "secret É".into();
        dialog.session.draft.replacement = "secret replacement".into();
        dialog
            .session
            .mark_changed(now - std::time::Duration::from_secs(1));
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                dialog.body(ui);
            });
        });
        assert!(history_path.exists());
        assert!(!preset_path.exists());
        let mut restarted = RegexTesterDialogState::with_storage_paths(&history_path, &preset_path);
        assert_eq!(
            restarted.history.as_ref().unwrap().store.entries()[0].pattern,
            "é"
        );
        restarted.session.draft.pattern = "a".into();
        restarted.session.draft.test_text = "retained test text".into();
        restarted.session.draft.replacement = "retained replacement".into();
        restarted.session.mark_changed(now);
        restarted
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let revision = restarted.session.revision();
        restarted.scroll_to_selected = true;
        restarted.copy_feedback = Some(Ok("Old feedback"));
        restarted.history_action(history::HistoryAction::Load(0));
        assert_eq!(restarted.session.draft.pattern, "é");
        assert!(restarted.session.draft.flags.case_insensitive);
        assert_eq!(restarted.session.draft.test_text, "retained test text");
        assert_eq!(restarted.session.draft.replacement, "retained replacement");
        assert_eq!(restarted.session.revision(), revision + 1);
        assert!(restarted.session.result().is_none());
        assert!(restarted.session.explanation().is_none());
        assert!(!restarted.scroll_to_selected);
        assert!(restarted.copy_feedback.is_none());
        restarted.information_section = InformationSection::History;
        restarted.open();
        restarted.open = false;
        restarted.open();
        assert_eq!(restarted.session.draft.pattern, "é");
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        restarted.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            );
        }
    }

    #[test]
    fn regex_tester_examples_load_exact_editable_drafts_and_clear_stale_state_once() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let unicode = crate::regex_tester::RegexExample {
            id: "test-unicode-replacement",
            name: "Unicode capture",
            description: "Test complete draft assignment",
            pattern: "(?P<letter>é)(🦀)",
            flags: crate::regex_tester::RegexFlags::default(),
            sample_text: "é🦀",
            replacement: Some("$2:${letter}:$1"),
        };
        for example in [
            &unicode,
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "url")
                .unwrap(),
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "key-value")
                .unwrap(),
            crate::regex_tester::BUILT_IN_EXAMPLES
                .iter()
                .find(|entry| entry.id == "email-like")
                .unwrap(),
        ] {
            dialog.scroll_to_selected = true;
            dialog.copy_feedback = Some(Ok("Old feedback"));
            let revision = dialog.session.revision();
            dialog.load_example(example);
            assert_eq!(dialog.session.draft, example.to_draft());
            assert_eq!(dialog.session.revision(), revision + 1);
            assert!(dialog.session.result().is_none());
            assert!(dialog.session.explanation().is_none());
            assert!(!dialog.scroll_to_selected);
            assert!(dialog.copy_feedback.is_none());
            dialog
                .session
                .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            assert!(matches!(
                dialog.session.result(),
                Some(EvaluationResult::Success { .. })
            ));
        }
        assert!(dialog.session.draft.replacement.is_empty());
        dialog.session.draft.pattern.push_str("/é");
        dialog.mark_changed();
        let edited = dialog.session.draft.clone();
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert_eq!(dialog.session.draft, edited);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_examples_search_render_and_reopen_have_no_document_side_effects() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::Examples;
        for (query, id) in [
            ("HTTP(S)-LIKE URL", "url"),
            ("STANDARDS-COMPLETE", "email-like"),
            ("(?P<key>", "key-value"),
        ] {
            dialog.examples.query = query.into();
            assert!(dialog.examples.entries().any(|entry| entry.id == id));
        }
        dialog.examples.query = "not-a-catalog-entry".into();
        assert_eq!(dialog.examples.entries().count(), 0);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for query in ["", "not-a-catalog-entry"] {
            dialog.examples.query = query.into();
            for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let bounds = ui.available_rect_before_wrap();
                            dialog.body(ui);
                            assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                            assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                        });
                    },
                );
            }
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        dialog.examples.query = "key=value".into();
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert!(dialog.information_section == InformationSection::Examples);
        assert_eq!(dialog.examples.query, "key=value");
    }

    #[test]
    fn regex_tester_reference_filters_render_and_reopen_preserve_draft_without_clipboard_io() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.ensure_initial_evaluation(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::Reference;
        dialog.reference.query = "DiGiT".into();
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::CharacterClasses);
        assert!(
            dialog
                .reference
                .entries()
                .any(|entry| entry.id == "class-digit")
        );
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::Anchors);
        assert!(
            dialog
                .reference
                .entries()
                .all(|entry| entry.category == crate::regex_tester::ReferenceCategory::Anchors)
        );
        dialog.reference.query = "not-a-catalog-entry".into();
        assert_eq!(dialog.reference.entries().count(), 0);
        dialog.reference.category = None;
        dialog.reference.query.clear();
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        dialog.body(ui);
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                },
            );
        }
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 0);
        dialog.reference.query = "DiGiT".into();
        dialog.reference.category = Some(crate::regex_tester::ReferenceCategory::CharacterClasses);
        dialog.open();
        dialog.open = false;
        dialog.open();
        assert!(dialog.information_section == InformationSection::Reference);
        assert_eq!(dialog.reference.query, "DiGiT");
        assert_eq!(
            dialog.reference.category,
            Some(crate::regex_tester::ReferenceCategory::CharacterClasses)
        );
    }

    #[test]
    fn regex_tester_reference_append_and_copy_are_exact_explicit_actions() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        let now = Instant::now();
        dialog.session.draft.pattern = "é/".into();
        dialog.session.draft.test_text = "é/123".into();
        dialog.session.draft.replacement = "$1".into();
        dialog.session.draft.flags.case_insensitive = true;
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let revision = dialog.session.revision();
        let digit = crate::regex_tester::QUICK_REFERENCE
            .iter()
            .find(|entry| entry.id == "class-digit")
            .unwrap();
        dialog.reference_action(reference::ReferenceAction::Copy(digit));
        assert_eq!(*backend.contents.lock().unwrap(), r"\d");
        assert_eq!(dialog.copy_feedback, Some(Ok("Copied syntax")));
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(dialog.session.revision(), revision);
        dialog.reference_action(reference::ReferenceAction::Append(digit));
        assert_eq!(dialog.session.draft.pattern, "é/\\d");
        assert_eq!(dialog.session.draft.test_text, draft.test_text);
        assert_eq!(dialog.session.draft.flags, draft.flags);
        assert_eq!(dialog.session.draft.replacement, draft.replacement);
        assert_eq!(dialog.session.revision(), revision + 1);
        assert!(dialog.session.result().is_none());
        assert!(dialog.session.explanation().is_none());
        assert!(dialog.copy_feedback.is_none());
        let fragment = crate::regex_tester::QUICK_REFERENCE
            .iter()
            .find(|entry| entry.syntax == "*")
            .unwrap();
        dialog.session.draft.pattern.clear();
        dialog.reference_action(reference::ReferenceAction::Append(fragment));
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_some());
        assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn regex_tester_explanation_view_tracks_invalid_pending_and_valid_state() {
        let now = Instant::now();
        let mut dialog = RegexTesterDialogState::default();
        dialog.information_section = InformationSection::Explanation;
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_some());
        assert!(dialog.session.explanation().is_none());
        dialog.session.draft.pattern = "(?i:é)+".into();
        dialog.session.mark_changed(now);
        assert!(dialog.validation_error().is_none());
        assert!(dialog.session.explanation().is_none());
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.session.explanation().is_some());
        let draft = dialog.session.draft.clone();
        let ctx = egui::Context::default();
        for size in [egui::vec2(360.0, 240.0), egui::vec2(960.0, 680.0)] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let bounds = ui.available_rect_before_wrap();
                    dialog.body(ui);
                    assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                    assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                });
            });
        }
        assert_eq!(dialog.session.draft, draft);
        assert!(dialog.session.pending_delay(now).is_none());
    }
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct FakeClipboard {
        reads: AtomicUsize,
        read_error: Mutex<Option<ClipboardError>>,
        attempts: AtomicUsize,
        writes: Mutex<Vec<String>>,
        contents: Mutex<String>,
        fail: AtomicBool,
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if let Some(error) = &*self.read_error.lock().unwrap() {
                return Err(error.clone());
            }
            Ok(self.contents.lock().unwrap().clone())
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            if self.fail.load(Ordering::Relaxed) {
                return Err(ClipboardError::Busy("blocked".into()));
            }
            self.writes.lock().unwrap().push(text.into());
            *self.contents.lock().unwrap() = text.into();
            Ok(())
        }
    }

    #[test]
    fn regex_tester_explicit_match_capture_copy_handles_unicode_empty_unmatched_and_failure() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = r"(?P<word>é)(🦀)(z)?()".into();
        dialog.session.draft.test_text = "é🦀".into();
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        dialog.copy_selected(inspection::CopyTarget::Match);
        dialog.copy_selected(inspection::CopyTarget::Capture(0));
        dialog.copy_selected(inspection::CopyTarget::Capture(3));
        assert_eq!(*backend.writes.lock().unwrap(), ["é🦀", "é", ""]);
        assert!(matches!(dialog.copy_feedback, Some(Ok("Copied capture"))));
        dialog.copy_selected(inspection::CopyTarget::Capture(2));
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 3);
        assert!(matches!(dialog.copy_feedback, Some(Err(_))));
        *backend.contents.lock().unwrap() = "existing clipboard".into();
        backend.fail.store(true, Ordering::Relaxed);
        dialog.copy_selected(inspection::CopyTarget::Match);
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 4);
        assert_eq!(*backend.contents.lock().unwrap(), "existing clipboard");
        assert!(
            dialog
                .copy_feedback
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains("Could not copy")
        );
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(backend.reads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn regex_tester_open_render_and_inspection_never_access_clipboard_implicitly() {
        let backend = Arc::new(FakeClipboard::default());
        let mut dialog = RegexTesterDialogState::with_clipboard(backend.clone());
        dialog.session.draft.pattern = "(a)".into();
        dialog.session.draft.test_text = "a".into();
        dialog.open();
        dialog
            .session
            .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        dialog.information_section = InformationSection::MatchDetails;
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| dialog.show(ctx));
        assert_eq!(backend.reads.load(Ordering::Relaxed), 0);
        assert_eq!(backend.attempts.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn regex_tester_native_geometry_is_large_and_resizable() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(960.0, 680.0)));
        assert_eq!(builder.min_inner_size, Some(egui::vec2(360.0, 240.0)));
        assert_eq!(builder.resizable, Some(true));
    }

    #[test]
    fn regex_tester_navigation_scroll_is_one_shot_and_preserves_editor_cursor_and_source() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "one".into();
        dialog.session.draft.test_text = "one\n".repeat(200);
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        let draft = dialog.session.draft.clone();
        let render = |dialog: &mut RegexTesterDialogState| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
            });
        };
        render(&mut dialog);
        let id = egui::Id::new("regex_tester_test_text");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        let selection = egui::text::CCursorRange::one(egui::text::CCursor::new(2));
        state.cursor.set_char_range(Some(selection));
        state.store(&ctx, id);
        assert!(dialog.session.previous_match());
        dialog.scroll_to_selected = true;
        assert_eq!(dialog.navigation_summary(), "Match 200 of 200");
        render(&mut dialog);
        assert!(!dialog.scroll_to_selected);
        assert_eq!(dialog.session.draft, draft);
        assert_eq!(
            egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range(),
            Some(selection)
        );
        render(&mut dialog);
        assert!(!dialog.scroll_to_selected);
        dialog.scroll_to_selected = true;
        dialog.mark_changed();
        assert!(!dialog.scroll_to_selected);
    }

    #[test]
    fn regex_tester_navigation_summary_qualifies_truncated_rows_as_displayed() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert_eq!(dialog.navigation_summary(), "Match 1 of 1000 displayed");
        assert_eq!(dialog.summary(), "At least 1001 matches; showing 1000");
    }

    #[test]
    fn regex_tester_inline_validation_clears_while_pending_and_after_fixing() {
        let now = Instant::now();
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "[".into();
        dialog.session.mark_changed(now);
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(
            dialog
                .validation_error()
                .unwrap()
                .contains("unclosed character class")
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(360.0, 240.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let bounds = ui.available_rect_before_wrap();
                let layout = dialog.body(ui);
                assert!(bounds.contains_rect(layout.validation.unwrap()));
                assert!(bounds.contains_rect(layout.editor));
                assert!(layout.editor.height() > 25.0);
                assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
            });
        });
        dialog.session.draft.pattern = "a".into();
        dialog.session.mark_changed(now);
        assert!(dialog.validation_error().is_none());
        assert!(dialog.session.result().is_none());
        dialog
            .session
            .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
        assert!(dialog.validation_error().is_none());
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Success { .. })
        ));
    }

    #[test]
    fn regex_tester_inline_validation_preserves_rust_unsupported_construct_messages() {
        let now = Instant::now();
        for (pattern, expected) in [("(?=a)", "look-around"), (r"(a)\1", "backreferences")] {
            let mut dialog = RegexTesterDialogState::default();
            dialog.session.draft.pattern = pattern.into();
            dialog.session.mark_changed(now);
            dialog
                .session
                .tick(now + crate::regex_tester::session::EVALUATION_DEBOUNCE);
            let Some(EvaluationResult::InvalidPattern(error)) = dialog.session.result() else {
                panic!("invalid pattern required")
            };
            assert_eq!(dialog.validation_error(), Some(error.message.as_str()));
            assert!(dialog.validation_error().unwrap().contains(expected));
        }
    }

    #[test]
    fn regex_tester_idle_frames_do_not_reschedule_and_summary_reports_truncation() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "a".into();
        dialog.session.draft.test_text = "a".repeat(1001);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        });
        assert!(
            dialog
                .session
                .tick(Instant::now() + crate::regex_tester::session::EVALUATION_DEBOUNCE)
        );
        assert_eq!(dialog.summary(), "At least 1001 matches; showing 1000");
        let revision = dialog.session.revision();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        });
        assert_eq!(dialog.session.revision(), revision);
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
    }

    #[test]
    fn regex_tester_oversized_text_is_retained_with_bounded_unicode_preview() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.test_text = "🦀".repeat(dialog.session.policy().text_bytes);
        let original = dialog.session.draft.test_text.clone();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
        });
        assert_eq!(dialog.session.draft.test_text, original);
        assert!(dialog.preview_text().len() <= 4096);
        assert!(original.starts_with(dialog.preview_text()));
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
    }

    #[test]
    fn regex_tester_oversized_unicode_pattern_is_retained_with_bounded_preview() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "文🦀".repeat(dialog.session.policy().pattern_bytes);
        dialog.session.draft.flags.case_insensitive = true;
        let original = dialog.session.draft.clone();
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| dialog.body(ui));
            });
        }
        assert_eq!(dialog.session.draft, original);
        assert!(utf8_prefix(&original.pattern, 128).len() <= 128);
        assert!(
            original
                .pattern
                .starts_with(utf8_prefix(&original.pattern, 128))
        );
        assert!(matches!(
            dialog.session.result(),
            Some(EvaluationResult::Suspended(_))
        ));
        assert!(dialog.session.pending_delay(Instant::now()).is_none());
    }

    #[test]
    fn regex_tester_layout_keeps_long_inputs_within_normal_and_narrow_viewports() {
        for size in [egui::vec2(960.0, 680.0), egui::vec2(360.0, 240.0)] {
            for information_open in [true, false] {
                let ctx = egui::Context::default();
                let mut dialog = RegexTesterDialogState::default();
                dialog.information_open = information_open;
                dialog.session.draft.pattern = "path/to/file".repeat(100);
                dialog.session.draft.test_text = "a long editable line ".repeat(1000);
                let draft = dialog.session.draft.clone();
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let bounds = ui.available_rect_before_wrap();
                        let layout = dialog.body(ui);
                        for area in [
                            Some(layout.header),
                            Some(layout.editor),
                            layout.information,
                            Some(layout.status),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            assert!(
                                bounds.contains_rect(area),
                                "{size:?}: {area:?} outside {bounds:?}"
                            );
                        }
                        assert!(layout.editor.width() > 100.0);
                        assert!(layout.editor.height() > 25.0);
                        if let Some(info) = layout.information {
                            assert!(!layout.editor.intersects(info));
                            if size.x > 680.0 {
                                assert!(info.left() > layout.editor.right());
                            } else {
                                assert!(info.top() > layout.editor.bottom());
                            }
                        }
                        assert!(ui.min_rect().right() <= bounds.right() + 0.1);
                        assert!(ui.min_rect().bottom() <= bounds.bottom() + 0.1);
                    });
                });
                assert_eq!(dialog.session.draft, draft);
            }
        }
    }

    #[test]
    fn regex_tester_information_collapse_restores_editor_space_and_survives_reopen() {
        let mut dialog = RegexTesterDialogState::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 680.0));
        let expanded = CoreLayout::new(bounds, dialog.information_open, false);
        dialog.information_open = false;
        dialog.open();
        dialog.open = false;
        dialog.open();
        let collapsed = CoreLayout::new(bounds, dialog.information_open, false);
        assert!(expanded.information.is_some());
        assert!(collapsed.information.is_none());
        assert!(collapsed.editor.width() > expanded.editor.width());
    }

    #[test]
    fn regex_tester_flag_controls_leave_slashes_in_raw_pattern() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "https?://example.com/a/b".into();
        let mut positions = Vec::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    positions = dialog
                        .flags_area(ui)
                        .map(|response| response.rect.center())
                        .to_vec();
                });
            });
        });
        for position in positions {
            let input = egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| dialog.flags_area(ui));
                });
            });
        }
        assert_eq!(dialog.session.draft.flags.suffix(), "imsx");
        assert_eq!(dialog.session.draft.pattern, "https?://example.com/a/b");
    }

    #[test]
    fn regex_tester_open_is_idempotent_and_preserves_session_draft() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.session.draft.pattern = "(?P<word>\\w+)".into();
        dialog.session.draft.test_text = "hello".into();
        dialog.session.draft.replacement = "$word".into();
        let draft = dialog.session.draft.clone();
        dialog.open();
        assert!(dialog.focus_pattern);
        dialog.focus_pattern = false;
        dialog.focus_viewport = false;
        dialog.open();
        assert!(!dialog.focus_pattern);
        assert!(dialog.focus_viewport);
        dialog.open = false;
        dialog.open();
        assert!(dialog.focus_pattern);
        assert_eq!(dialog.session.draft, draft);
    }

    #[test]
    fn regex_tester_embedded_shell_focuses_once_and_escape_closes() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.open();
        let _ = ctx.run(egui::RawInput::default(), |ctx| dialog.show(ctx));
        assert!(!dialog.focus_pattern);
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(input, |ctx| dialog.show(ctx));
        assert!(!dialog.open);
    }
}
