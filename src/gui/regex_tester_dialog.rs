use crate::regex_tester::model::RegexDraft;
use eframe::egui;

fn viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Regex Tester")
        .with_inner_size([960.0, 680.0])
        .with_min_inner_size([360.0, 240.0])
        .with_resizable(true)
}

/// Session-owned inputs survive closing the utility; persistence is explicit.
#[derive(Default)]
pub struct RegexTesterDialogState {
    pub open: bool,
    pub draft: RegexDraft,
    focus_pattern: bool,
    focus_viewport: bool,
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

    fn body(&mut self, ui: &mut egui::Ui) {
        ui.weak("Local, offline · Rust regex engine");
        ui.label("Regular expression");
        let pattern = ui.add(
            egui::TextEdit::singleline(&mut self.draft.pattern)
                .id(egui::Id::new("regex_tester_pattern"))
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
        if self.focus_pattern {
            pattern.request_focus();
            self.focus_pattern = false;
        }
        ui.label("Test text");
        let editor_size = ui.available_size();
        egui::ScrollArea::both()
            .id_source("regex_tester_text_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_sized(
                    editor_size,
                    egui::TextEdit::multiline(&mut self.draft.test_text)
                        .id(egui::Id::new("regex_tester_test_text"))
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
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
