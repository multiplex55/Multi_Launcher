use crate::regex_tester::{CaptureGroup, CaptureValue, RegexMatch};
use eframe::egui;

#[derive(Clone, Copy)]
pub(super) enum CopyTarget {
    Match,
    Capture(usize),
}

pub(super) fn copy_text(matched: &RegexMatch, target: CopyTarget) -> Option<&str> {
    match target {
        CopyTarget::Match => Some(&matched.text),
        CopyTarget::Capture(index) => match &matched.captures.get(index)?.value {
            CaptureValue::Matched { text, .. } => Some(text),
            CaptureValue::Unmatched => None,
        },
    }
}

pub(super) fn capture_label(capture: &CaptureGroup) -> String {
    match &capture.name {
        Some(name) => format!("Capture #{} ({name})", capture.group_index),
        None => format!("Capture #{}", capture.group_index),
    }
}

pub(super) fn byte_label(span: crate::regex_tester::ByteSpan) -> String {
    format!(
        "UTF-8 bytes {}..{} (end exclusive)",
        span.start_byte(),
        span.end_byte()
    )
}

fn value(ui: &mut egui::Ui, text: &str, id: &str) {
    egui::ScrollArea::both()
        .id_source(id)
        .max_height(120.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut borrowed = text;
            ui.add(
                egui::TextEdit::multiline(&mut borrowed)
                    .id(egui::Id::new(("regex_inspection_value", id)))
                    .font(egui::TextStyle::Monospace)
                    .desired_width(ui.available_width())
                    .desired_rows(3),
            );
        });
}

/// Only the explicitly expanded match and selected capture are laid out. Other
/// capture values remain borrowed and are never cloned for presentation.
pub(super) fn show(
    ui: &mut egui::Ui,
    matched: &RegexMatch,
    selected_capture: &mut usize,
) -> Option<CopyTarget> {
    let mut copy = None;
    ui.strong(format!("Match {}", matched.id.index() + 1));
    ui.label(format!(
        "Line {}, column {} (Unicode scalars)",
        matched.location.line, matched.location.column
    ));
    ui.label(byte_label(matched.span));
    if matched.text.is_empty() {
        ui.weak("Zero-width match (empty text)");
    }
    if ui.button("Copy match").clicked() {
        copy = Some(CopyTarget::Match);
    }
    egui::CollapsingHeader::new("Full match text")
        .id_source("regex_inspection_match_text")
        .show(ui, |ui| {
            value(ui, &matched.text, "regex_match_value_scroll")
        });
    if matched.captures.is_empty() {
        return copy;
    }
    *selected_capture = (*selected_capture).min(matched.captures.len() - 1);
    egui::ComboBox::from_id_source("regex_inspection_capture")
        .width(ui.available_width().min(220.0))
        .selected_text(capture_label(&matched.captures[*selected_capture]))
        .show_ui(ui, |ui| {
            for (index, capture) in matched.captures.iter().enumerate() {
                ui.selectable_value(selected_capture, index, capture_label(capture));
            }
        });
    let capture = &matched.captures[*selected_capture];
    match &capture.value {
        CaptureValue::Unmatched => {
            ui.weak("Unmatched optional capture");
        }
        CaptureValue::Matched { text, span } => {
            ui.label(byte_label(*span));
            if text.is_empty() {
                ui.weak("Matched empty capture");
            }
            egui::CollapsingHeader::new("Full capture text")
                .id_source(("regex_inspection_capture_text", capture.group_index))
                .show(ui, |ui| value(ui, text, "regex_capture_value_scroll"));
        }
    }
    if ui
        .add_enabled(
            copy_text(matched, CopyTarget::Capture(*selected_capture)).is_some(),
            egui::Button::new("Copy capture"),
        )
        .clicked()
    {
        copy = Some(CopyTarget::Capture(*selected_capture));
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex_tester::{EvaluationResult, RegexFlags, evaluate};

    #[test]
    fn inspection_distinguishes_named_numbered_unmatched_empty_and_exact_byte_spans() {
        let source = "é🦀";
        let EvaluationResult::Success { matches, .. } =
            evaluate(r"(?P<unicode>é)(🦀)(z)?()", &RegexFlags::default(), source)
        else {
            panic!("success required")
        };
        let matched = &matches[0];
        assert_eq!(capture_label(&matched.captures[0]), "Capture #1 (unicode)");
        assert_eq!(capture_label(&matched.captures[1]), "Capture #2");
        assert_eq!(byte_label(matched.span), "UTF-8 bytes 0..6 (end exclusive)");
        assert_eq!(copy_text(matched, CopyTarget::Match), Some(source));
        assert_eq!(copy_text(matched, CopyTarget::Capture(0)), Some("é"));
        assert_eq!(copy_text(matched, CopyTarget::Capture(2)), None);
        assert_eq!(copy_text(matched, CopyTarget::Capture(3)), Some(""));
        let ctx = egui::Context::default();
        let mut selected = 2;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                show(ui, matched, &mut selected);
            });
        });
        assert_eq!(matched.text, source);
        assert_eq!(selected, 2);
    }
}
