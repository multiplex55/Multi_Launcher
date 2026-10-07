//! Paste-time glyph guard. The full String remains owned by the editor/session;
//! copying, hashing and undo bookkeeping still have input-size-dependent costs.

use eframe::egui::{self, text::LayoutJob};

pub(super) fn oversized_job(text: &str, maximum_bytes: usize) -> Option<LayoutJob> {
    if text.len() <= maximum_bytes {
        return None;
    }
    let mut job = LayoutJob {
        text: text.to_owned(),
        ..Default::default()
    };
    // epaint 0.27 returns an elided empty galley before paragraph/glyph layout.
    job.wrap.max_rows = 0;
    Some(job)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_guard_retains_full_unicode_job_and_elides_before_sections() {
        let text = "🦀".repeat(256 * 1024);
        let job = oversized_job(&text, 64 * 1024).unwrap();
        assert_eq!(job.text, text);
        assert!(job.sections.is_empty());
        assert_eq!(job.wrap.max_rows, 0);
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let galley = ctx.fonts(|fonts| fonts.layout_job(job));
            assert!(galley.elided);
            assert!(galley.rows.is_empty());
            assert_eq!(galley.num_vertices, 0);
            assert_eq!(galley.job.text, text);
        });
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
    ui.fonts(|fonts| fonts.layout_job(job))
}
