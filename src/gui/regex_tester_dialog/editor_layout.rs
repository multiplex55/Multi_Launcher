//! Paste-time glyph guard. The full String remains owned by the editor/session;
//! copying, hashing and undo bookkeeping still have input-size-dependent costs.

use eframe::egui::{
    self,
    text::{ByteIndex, LayoutJob},
};

pub(super) fn oversized_job(text: &str, maximum_bytes: usize) -> Option<LayoutJob> {
    if text.len() <= maximum_bytes {
        return None;
    }
    let mut job = LayoutJob {
        text: text.to_owned(),
        ..Default::default()
    };
    // egui 0.36 validates section coverage before its max_rows == 0 early return.
    // This full-range section keeps the intentionally glyph-free job valid.
    job.sections.push(egui::text::LayoutSection {
        leading_space: 0.0,
        byte_range: ByteIndex::ZERO..ByteIndex(text.len()),
        format: Default::default(),
    });
    job.wrap.max_rows = 0;
    Some(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_guard_retains_full_unicode_job_with_one_valid_full_text_section() {
        let text = "🦀".repeat(256 * 1024);
        let job = oversized_job(&text, 64 * 1024).unwrap();
        assert_eq!(job.text, text);
        assert_eq!(job.sections.len(), 1);
        assert_eq!(
            job.sections[0].byte_range,
            ByteIndex::ZERO..ByteIndex(text.len())
        );
        assert_eq!(job.sections[0].format, egui::TextFormat::default());
        assert_eq!(job.wrap.max_rows, 0);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            assert!(galley.elided);
            assert!(galley.rows.is_empty());
            assert_eq!(galley.num_vertices, 0);
            assert_eq!(galley.job.text, text);
        });
        output.drop_without_applying_deltas();
        assert!(oversized_job("é🦀", 6).is_none());
    }
}

pub(super) fn plain(
    ui: &egui::Ui,
    text: &str,
    width: f32,
    maximum_bytes: usize,
    font: egui::FontId,
    singleline: bool,
) -> std::sync::Arc<egui::Galley> {
    let job = oversized_job(text, maximum_bytes).unwrap_or_else(|| {
        if singleline {
            LayoutJob::simple_singleline(text.to_owned(), font, ui.visuals().text_color())
        } else {
            LayoutJob::simple(text.to_owned(), font, ui.visuals().text_color(), width)
        }
    });
    ui.ctx().fonts_mut(|fonts| fonts.layout_job(job))
}
