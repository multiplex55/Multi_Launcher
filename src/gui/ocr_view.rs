//! Presentation metadata only. The workflow remains the sole result-text owner.
use crate::mkmacro::{DiagnosticKind, ExecutionDiagnostic};
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

fn error_guidance(error: &ExecutionDiagnostic) -> &'static str {
    // These contexts come from the shared pipeline/policy boundaries. Keep the
    // original diagnostic in the workflow; never classify its message text.
    if error
        .context
        .get("ocr_pipeline_operation")
        .map(String::as_str)
        == Some("capture region")
    {
        "Could not capture the selected region. Re-capture the region and try again."
    } else if error.kind == DiagnosticKind::UnsupportedOperation
        && error.context.get("operation").map(String::as_str)
            == Some("resolve English OCR language")
    {
        "No English OCR language is installed. Install an English language pack in Windows Settings > Time & language > Language & region, then try again."
    } else if matches!(
        error.kind,
        DiagnosticKind::UnsupportedOperation | DiagnosticKind::RuntimeUnavailable
    ) {
        "Local OCR is unavailable. Check Windows OCR support, then try again."
    } else {
        "Could not recognize text in the selected region. Re-capture a clear, readable region and try again."
    }
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
                    if let OcrPresentation::Result(text) = state
                        && ui
                            .add_enabled(!text.trim().is_empty(), egui::Button::new("Copy All"))
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
                        ui.label("No text was recognized in the selected region.");
                    }
                    OcrPresentation::Error(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, error_guidance(error));
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

    fn rendered_text(state: &mut OcrPresentation) -> String {
        fn append(shape: &egui::epaint::Shape, text: &mut String) {
            match shape {
                egui::epaint::Shape::Text(shape) => {
                    text.push_str(shape.galley.text());
                    text.push('\n');
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        append(shape, text);
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let mut view = OcrView::default();
        let generation = generation();
        let mut output = String::new();
        for _ in 0..2 {
            let frame = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    assert!(view.show(ctx, generation, state).is_none());
                },
            );
            output.clear();
            for shape in frame.shapes {
                append(&shape.shape, &mut output);
            }
        }
        output
    }

    #[test]
    fn ocr_view_empty_progress_and_failures_hide_copy_and_keep_recovery_actions() {
        let cases = [
            (
                OcrPresentation::NoText,
                "No text was recognized in the selected region.",
            ),
            (OcrPresentation::Recognizing, "Recognizing text..."),
            (
                OcrPresentation::Error(ExecutionDiagnostic::new(
                    DiagnosticKind::UnsupportedOperation,
                    "backend detail",
                )),
                "Local OCR is unavailable. Check Windows OCR support, then try again.",
            ),
            (
                OcrPresentation::Error(
                    ExecutionDiagnostic::new(
                        DiagnosticKind::UnsupportedOperation,
                        "backend detail",
                    )
                    .context("operation", "resolve English OCR language"),
                ),
                "No English OCR language is installed. Install an English language pack in Windows Settings > Time & language > Language & region, then try again.",
            ),
            (
                OcrPresentation::Error(
                    ExecutionDiagnostic::new(DiagnosticKind::Backend, "backend detail")
                        .context("ocr_pipeline_operation", "capture region"),
                ),
                "Could not capture the selected region. Re-capture the region and try again.",
            ),
            (
                OcrPresentation::Error(
                    ExecutionDiagnostic::new(DiagnosticKind::Backend, "backend detail")
                        .context("ocr_pipeline_operation", "recognize tile"),
                ),
                "Could not recognize text in the selected region. Re-capture a clear, readable region and try again.",
            ),
        ];
        for (mut state, expected) in cases {
            let text = rendered_text(&mut state);
            assert!(text.contains(expected), "{text}");
            assert!(!text.contains("Copy All"));
            assert!(text.contains("Re-capture") && text.contains("Close"));
            assert!(!text.contains("backend detail"));
        }
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
