use crate::regex_tester::session::EvaluatedText;
use eframe::egui::{self, text::LayoutJob};

fn colors(visuals: &egui::Visuals) -> (egui::Color32, egui::Color32) {
    (
        visuals.selection.bg_fill.gamma_multiply(0.35),
        visuals.selection.bg_fill,
    )
}

pub(super) fn layout(text: &str, view: &EvaluatedText<'_>, ui: &egui::Ui, width: f32) -> LayoutJob {
    let format = egui::TextFormat {
        font_id: egui::TextStyle::Monospace.resolve(ui.style()),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let mut job = LayoutJob {
        text: text.to_owned(),
        ..Default::default()
    };
    job.wrap.max_width = width;
    let (normal, active) = colors(ui.visuals());
    let mut cursor = 0;
    let mut append = |range, background| {
        let mut section_format = format.clone();
        section_format.background = background;
        job.sections.push(egui::text::LayoutSection {
            leading_space: 0.0,
            byte_range: range,
            format: section_format,
        });
    };
    if view.applies_to(text) {
        for (index, matched) in view.matches.iter().enumerate() {
            let span = matched.span.as_range();
            if span.is_empty() {
                continue;
            }
            if cursor < span.start {
                append(cursor..span.start, egui::Color32::TRANSPARENT);
            }
            append(
                span.clone(),
                if view.selected_index == Some(index) {
                    active
                } else {
                    normal
                },
            );
            cursor = span.end;
        }
    }
    if cursor < text.len() || cursor == 0 {
        append(cursor..text.len(), egui::Color32::TRANSPARENT);
    }
    job
}

/// Convert sorted UTF-8 byte positions to egui's scalar-character positions in
/// one source walk. CR and LF are separate scalars, just as egui counts them.
fn zero_width_indices(text: &str, view: &EvaluatedText<'_>) -> Vec<(usize, bool)> {
    if !view.applies_to(text) {
        return Vec::new();
    }
    let mut chars = text.char_indices().enumerate().peekable();
    let mut consumed = 0;
    view.matches
        .iter()
        .enumerate()
        .filter(|(_, matched)| matched.span.is_empty())
        .map(|(index, matched)| {
            let byte = matched.span.start_byte();
            while chars.peek().is_some_and(|(_, (offset, _))| *offset < byte) {
                chars.next();
                consumed += 1;
            }
            (
                chars.peek().map_or(consumed, |(index, _)| *index),
                view.selected_index == Some(index),
            )
        })
        .collect()
}

fn marker_rects(galley: &egui::Galley, indices: &[(usize, bool)]) -> Vec<(egui::Rect, bool)> {
    let mut row_index = 0;
    let mut row_start = 0;
    let mut markers = Vec::with_capacity(indices.len());
    for &(character, active) in indices {
        while row_index + 1 < galley.rows.len() {
            let row = &galley.rows[row_index];
            let end = row_start + row.char_count_including_newline();
            // At a soft wrap boundary choose the following row. At a hard
            // newline the cursor before '\n' belongs to the preceding row.
            if character < end {
                break;
            }
            row_start = end;
            row_index += 1;
        }
        if let Some(row) = galley.rows.get(row_index) {
            let x = row.x_offset(character.saturating_sub(row_start));
            markers.push((
                egui::Rect::from_min_max(egui::pos2(x, row.min_y()), egui::pos2(x, row.max_y())),
                active,
            ));
        }
    }
    markers
}

pub(super) fn paint_markers(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &str,
    view: &EvaluatedText<'_>,
) {
    if !view.applies_to(text) || output.galley.job.text != text {
        return;
    }
    let painter = ui
        .painter()
        .with_clip_rect(ui.clip_rect().intersect(output.text_clip_rect));
    let (normal, active) = colors(ui.visuals());
    for (rect, selected) in marker_rects(&output.galley, &zero_width_indices(text, view)) {
        let rect = rect.translate(output.galley_pos.to_vec2());
        painter.line_segment(
            [rect.min, rect.max],
            egui::Stroke::new(
                if selected { 3.0_f32 } else { 2.0_f32 },
                if selected { active } else { normal },
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regex_tester::{RegexSession, session::EVALUATION_DEBOUNCE};
    use std::time::Instant;

    fn evaluated(pattern: &str, text: &str) -> RegexSession {
        let mut session = RegexSession::default();
        session.draft.pattern = pattern.into();
        session.draft.test_text = text.into();
        let now = Instant::now();
        session.mark_changed(now);
        session.tick(now + EVALUATION_DEBOUNCE);
        session
    }

    #[test]
    fn sections_preserve_adjacent_unicode_and_multiline_source_and_active_style() {
        for (pattern, text) in [(".", "é🦀"), (".+", "éé\nab")] {
            let ctx = egui::Context::default();
            let mut session = evaluated(pattern, text);
            let (source, view) = session.text_edit_parts();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let job = layout(source, &view, ui, 80.0);
                    assert_eq!(job.text, text);
                    let mut end = 0;
                    for section in &job.sections {
                        assert_eq!(section.byte_range.start, end);
                        assert!(text.is_char_boundary(section.byte_range.start));
                        assert!(text.is_char_boundary(section.byte_range.end));
                        end = section.byte_range.end;
                    }
                    assert_eq!(end, text.len());
                    let backgrounds = job
                        .sections
                        .iter()
                        .filter(|section| section.format.background != egui::Color32::TRANSPARENT)
                        .map(|section| section.format.background)
                        .collect::<Vec<_>>();
                    assert_eq!(backgrounds.len(), 2);
                    assert_ne!(backgrounds[0], backgrounds[1]);
                });
            });
            assert_eq!(source, text);
        }
    }

    #[test]
    fn same_frame_multibyte_edit_and_revision_mismatch_discard_all_stale_highlights() {
        let ctx = egui::Context::default();
        let mut session = evaluated("a", "aaé");
        let (text, mut view) = session.text_edit_parts();
        text.insert_str(0, "文");
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let job = layout(text, &view, ui, 100.0);
                assert_eq!(job.text, "文aaé");
                assert_eq!(job.sections.len(), 1);
                assert_eq!(
                    job.sections[0].format.background,
                    egui::Color32::TRANSPARENT
                );
                assert!(zero_width_indices(text, &view).is_empty());
                view.revision += 1;
                let job = layout(view.source.unwrap(), &view, ui, 100.0);
                assert_eq!(job.sections.len(), 1);
                assert_eq!(
                    job.sections[0].format.background,
                    egui::Color32::TRANSPARENT
                );
            });
        });
    }

    #[test]
    fn zero_width_markers_map_unicode_crlf_eof_empty_and_soft_wrap_in_one_row_walk() {
        for text in ["", "é\r\n🦀\n", "abcdefghi"] {
            for width in [400.0, 15.0] {
                let ctx = egui::Context::default();
                let mut session = evaluated("", text);
                let (source, view) = session.text_edit_parts();
                let indices = zero_width_indices(source, &view);
                assert_eq!(
                    indices.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
                    (0..=text.chars().count()).collect::<Vec<_>>()
                );
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let galley =
                            ui.fonts(|fonts| fonts.layout_job(layout(source, &view, ui, width)));
                        let markers = marker_rects(&galley, &indices);
                        assert_eq!(markers.len(), indices.len());
                        for ((rect, _), (index, _)) in markers.iter().zip(&indices) {
                            let expected = galley.pos_from_ccursor(egui::text::CCursor {
                                index: *index,
                                prefer_next_row: true,
                            });
                            assert_eq!(*rect, expected);
                        }
                    });
                });
            }
        }
    }
}
