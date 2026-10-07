use crate::regex_tester::model::RegexDraft;
use eframe::egui;

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Regex Tester")
        .with_inner_size([960.0, 680.0])
        .with_min_inner_size([360.0, 240.0])
        .with_resizable(true)
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
    editor: egui::Rect,
    information: Option<egui::Rect>,
    status: egui::Rect,
}

impl CoreLayout {
    fn new(bounds: egui::Rect, information_open: bool) -> Self {
        let header_height = 100.0_f32.min(bounds.height() * 0.45);
        let status_height = 24.0_f32.min(bounds.height() * 0.15);
        let gap = 8.0_f32.min(bounds.height() * 0.03);
        let header =
            egui::Rect::from_min_size(bounds.min, egui::vec2(bounds.width(), header_height));
        let status = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), bounds.bottom() - status_height),
            bounds.max,
        );
        let content = egui::Rect::from_min_max(
            egui::pos2(bounds.left(), header.bottom() + gap),
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
            editor,
            information,
            status,
        }
    }
}

/// Session-owned inputs survive closing the utility; persistence is explicit.
pub struct RegexTesterDialogState {
    pub open: bool,
    pub draft: RegexDraft,
    focus_pattern: bool,
    focus_viewport: bool,
    information_open: bool,
}

impl Default for RegexTesterDialogState {
    fn default() -> Self {
        Self {
            open: false,
            draft: RegexDraft::default(),
            focus_pattern: false,
            focus_viewport: false,
            information_open: true,
        }
    }
}

impl RegexTesterDialogState {
    pub fn open(&mut self) {
        self.focus_viewport = true;
        if !self.open {
            self.focus_pattern = true;
            self.open = true;
        }
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
        let bounds = ui.available_rect_before_wrap();
        // Recompute after the toggle so collapsing frees editor space this frame.
        let header = CoreLayout::new(bounds, self.information_open).header;
        bounded_area(ui, header, "regex_tester_header", |ui| {
            self.pattern_area(ui)
        });
        let layout = CoreLayout::new(bounds, self.information_open);
        bounded_area(ui, layout.editor, "regex_tester_editor_area", |ui| {
            self.text_area(ui)
        });
        if let Some(information) = layout.information {
            bounded_area(ui, information, "regex_tester_information_area", |ui| {
                self.information_area(ui)
            });
        }
        bounded_area(ui, layout.status, "regex_tester_status", Self::status_area);
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
            ui.label(egui::RichText::new("/").monospace().size(18.0));
            let suffix = format!("/{}", self.draft.flags.suffix());
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
            let pattern = ui.add_sized(
                [width, 28.0],
                egui::TextEdit::singleline(&mut self.draft.pattern)
                    .id(egui::Id::new("regex_tester_pattern"))
                    .font(egui::FontId::monospace(18.0))
                    .hint_text("Regular expression"),
            );
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
        [
            (
                "i",
                &mut self.draft.flags.case_insensitive,
                "Case insensitive",
            ),
            (
                "m",
                &mut self.draft.flags.multi_line,
                "Multiline: ^ and $ match line boundaries",
            ),
            (
                "s",
                &mut self.draft.flags.dot_matches_new_line,
                "Dot matches newline",
            ),
            (
                "u",
                &mut self.draft.flags.unicode,
                "Unicode character classes and matching",
            ),
            (
                "x",
                &mut self.draft.flags.ignore_whitespace,
                "Ignore pattern whitespace and allow comments",
            ),
        ]
        .map(|(letter, enabled, hint)| ui.checkbox(enabled, letter).on_hover_text(hint))
    }

    fn text_area(&mut self, ui: &mut egui::Ui) {
        ui.label("Test text");
        let editor_size = ui.available_size().max(egui::vec2(1.0, 1.0));
        egui::ScrollArea::both()
            .id_source("regex_tester_text_scroll")
            .auto_shrink([false, false])
            .max_height(editor_size.y)
            .show(ui, |ui| {
                ui.add_sized(
                    editor_size,
                    egui::TextEdit::multiline(&mut self.draft.test_text)
                        .id(egui::Id::new("regex_tester_test_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(1)
                        .desired_width(editor_size.x)
                        .hint_text("Type or paste text to test"),
                );
            });
    }

    fn information_area(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_source("regex_tester_information_scroll")
            .auto_shrink([false, false])
            .max_height(ui.available_height().max(1.0))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.strong("Match information");
                ui.weak("No evaluated results.");
            });
    }

    fn status_area(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.weak("Not evaluated");
            ui.add_enabled(false, egui::Button::new("Previous"));
            ui.add_enabled(false, egui::Button::new("Next"));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_tester_native_geometry_is_large_and_resizable() {
        let builder = viewport_builder();
        assert_eq!(builder.inner_size, Some(egui::vec2(960.0, 680.0)));
        assert_eq!(builder.min_inner_size, Some(egui::vec2(360.0, 240.0)));
        assert_eq!(builder.resizable, Some(true));
    }

    #[test]
    fn regex_tester_layout_keeps_long_inputs_within_normal_and_narrow_viewports() {
        for size in [egui::vec2(960.0, 680.0), egui::vec2(360.0, 240.0)] {
            for information_open in [true, false] {
                let ctx = egui::Context::default();
                let mut dialog = RegexTesterDialogState::default();
                dialog.information_open = information_open;
                dialog.draft.pattern = "path/to/file".repeat(100);
                dialog.draft.test_text = "a long editable line ".repeat(1000);
                let draft = dialog.draft.clone();
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
                assert_eq!(dialog.draft, draft);
            }
        }
    }

    #[test]
    fn regex_tester_information_collapse_restores_editor_space_and_survives_reopen() {
        let mut dialog = RegexTesterDialogState::default();
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 680.0));
        let expanded = CoreLayout::new(bounds, dialog.information_open);
        dialog.information_open = false;
        dialog.open();
        dialog.open = false;
        dialog.open();
        let collapsed = CoreLayout::new(bounds, dialog.information_open);
        assert!(expanded.information.is_some());
        assert!(collapsed.information.is_none());
        assert!(collapsed.editor.width() > expanded.editor.width());
    }

    #[test]
    fn regex_tester_flag_controls_leave_slashes_in_raw_pattern() {
        let ctx = egui::Context::default();
        let mut dialog = RegexTesterDialogState::default();
        dialog.draft.pattern = "https?://example.com/a/b".into();
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
        assert_eq!(dialog.draft.flags.suffix(), "imsx");
        assert_eq!(dialog.draft.pattern, "https?://example.com/a/b");
    }

    #[test]
    fn regex_tester_open_is_idempotent_and_preserves_session_draft() {
        let mut dialog = RegexTesterDialogState::default();
        dialog.draft.pattern = "(?P<word>\\w+)".into();
        dialog.draft.test_text = "hello".into();
        dialog.draft.replacement = "$word".into();
        let draft = dialog.draft.clone();
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
        assert_eq!(dialog.draft, draft);
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
