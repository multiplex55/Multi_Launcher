//! Presentation metadata only. The workflow remains the sole result-text owner.
use crate::ocr::selection::{OcrGeneration, OcrPresentation};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OcrViewIntent {
    Copy,
    Recapture,
    Close,
}

#[derive(Clone, Copy)]
pub(super) struct OcrViewEvent {
    pub generation: OcrGeneration,
    pub intent: OcrViewIntent,
}

#[derive(Default)]
pub(super) struct OcrView {
    generation: Option<OcrGeneration>,
    editor_focused: bool,
    pub(super) feedback: Option<Result<&'static str, String>>,
}

pub(super) fn editor_id(generation: OcrGeneration) -> egui::Id {
    egui::Id::new(("screen_ocr_editor", generation))
}

impl OcrView {
    pub(super) fn clear(&mut self, ctx: &egui::Context) {
        if let Some(generation) = self.generation {
            ctx.memory_mut(|memory| memory.surrender_focus(editor_id(generation)));
            // TextEdit undo history can retain old text even after the workflow
            // releases its String. Discard that state with the transient view.
            ctx.data_mut(|data| {
                data.remove::<egui::text_edit::TextEditState>(editor_id(generation))
            });
        }
        *self = Self::default();
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        generation: OcrGeneration,
        state: &mut OcrPresentation,
    ) -> Option<OcrViewEvent> {
        if self.generation != Some(generation) {
            self.clear(ctx);
            self.generation = Some(generation);
        }
        let mut open = true;
        let mut intent = None;
        egui::Window::new("Screen Region OCR")
            .id(egui::Id::new("screen_region_ocr"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size((560., 360.))
            .min_size((320., 220.))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let copyable =
                        matches!(state,OcrPresentation::Result(text) if !text.trim().is_empty());
                    if ui
                        .add_enabled(copyable, egui::Button::new("Copy All"))
                        .clicked()
                    {
                        intent = Some(OcrViewIntent::Copy);
                    }
                    if ui.button("Re-capture").clicked() {
                        intent = Some(OcrViewIntent::Recapture);
                    }
                    if ui.button("Close").clicked() {
                        intent = Some(OcrViewIntent::Close);
                    }
                });
                if let Some(feedback) = &self.feedback {
                    match feedback {
                        Ok(message) => {
                            ui.label(*message);
                        }
                        Err(message) => {
                            ui.colored_label(ui.visuals().error_fg_color, message);
                        }
                    }
                }
                match state {
                    OcrPresentation::Recognizing => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Recognizing text...");
                        });
                    }
                    OcrPresentation::Result(text) => {
                        // egui hides a new Window during its initial sizing frame.
                        // Keep focus pending until the editor is actually visible,
                        // then grant it before TextEdit consumes this frame's keys.
                        let grant_focus =
                            ui.is_visible() && ui.is_enabled() && !self.editor_focused;
                        if grant_focus {
                            ctx.memory_mut(|memory| memory.request_focus(editor_id(generation)));
                        }
                        let editor_size = ui.available_size();
                        egui::ScrollArea::both()
                            .id_source(("screen_ocr_scroll", generation))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let response = ui.add_sized(
                                    editor_size,
                                    egui::TextEdit::multiline(text)
                                        .id(editor_id(generation))
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(10),
                                );
                                if grant_focus && response.has_focus() {
                                    self.editor_focused = true;
                                }
                                if response.changed() {
                                    self.feedback = None;
                                }
                            });
                    }
                    OcrPresentation::NoText => {
                        ui.label("No text was recognized.");
                    }
                    OcrPresentation::Error(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, &error.message);
                    }
                }
            });
        if !open {
            intent = Some(OcrViewIntent::Close);
        }
        intent.map(|intent| OcrViewEvent { generation, intent })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::selection::OcrSelectionController;
    use std::time::Instant;
    fn generation() -> OcrGeneration {
        OcrSelectionController::default()
            .request(Instant::now())
            .unwrap()
            .unwrap()
    }

    #[test]
    fn ocr_view_visible_editor_enter_edits_workflow_text_and_focus_is_one_shot() {
        let ctx = egui::Context::default();
        let generation = generation();
        let mut view = OcrView::default();
        let mut state = OcrPresentation::Result("first\nsecond".into());
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("query_input")));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            view.show(ctx, generation, &mut state);
        });
        assert!(
            !view.editor_focused,
            "focus stays pending during hidden sizing"
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 600.),
                )),
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ctx| {
                assert!(view.show(ctx, generation, &mut state).is_none());
            },
        );
        assert!(
            matches!(&state,OcrPresentation::Result(text) if text.len()=="first\nsecond".len()+1 && text.matches('\n').count()==2)
        );
        let other = egui::Id::new("other_control");
        ctx.memory_mut(|memory| memory.request_focus(other));
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            view.show(ctx, generation, &mut state);
        });
        assert!(!ctx.memory(|memory| memory.has_focus(editor_id(generation))));
    }
}
