//! Hash-specific, accessible progress presentation.
use gpui_kit::{InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use gpui_kit::component::{label::Label, progress::Progress};
use multi_launcher::{gui::theme::ThemeMetrics, hash_generator::HashProgress};

#[derive(Debug, PartialEq)]
struct Presentation {
    percent: Option<f32>,
    status: String,
}

fn presentation(bytes: u64, total: Option<u64>) -> Presentation {
    match total {
        Some(total) => {
            let percent = if total == 0 {
                100
            } else {
                ((u128::from(bytes) * 100) / u128::from(total)).min(100) as u32
            };
            Presentation {
                percent: Some(percent as f32),
                status: format!("Hashing · {bytes} / {total} bytes ({percent}%)"),
            }
        }
        None => Presentation {
            percent: None,
            status: format!("Hashing · {bytes} bytes processed (total unknown)"),
        },
    }
}

pub(super) fn render(progress: HashProgress, metrics: ThemeMetrics) -> impl IntoElement {
    let presentation = presentation(progress.bytes_processed, progress.total_bytes);
    div()
        .id("hash-progress")
        .flex()
        .flex_col()
        .gap_1()
        .w_full()
        .child(
            Progress::new("hash-progress-bar")
                .accessibility_label(presentation.status.clone())
                .loading(presentation.percent.is_none())
                .value(presentation.percent.unwrap_or(0.0)),
        )
        .child(
            Label::new(presentation.status)
                .id("hash-progress-status")
                .text_size(px(metrics.small_text_size))
                .line_height(px(metrics.line_height)),
        )
}

#[cfg(test)]
mod tests {
    use super::presentation;

    #[test]
    fn progress_presents_unknown_zero_partial_and_clamped_totals() {
        let unknown = presentation(7, None);
        assert_eq!(unknown.percent, None);
        assert_eq!(unknown.status, "Hashing · 7 bytes processed (total unknown)");
        let zero = presentation(0, Some(0));
        assert_eq!(zero.percent, Some(100.0));
        assert_eq!(zero.status, "Hashing · 0 / 0 bytes (100%)");
        let partial = presentation(25, Some(100));
        assert_eq!(partial.percent, Some(25.0));
        assert_eq!(partial.status, "Hashing · 25 / 100 bytes (25%)");
        let one_short = presentation(99_999_999, Some(100_000_000));
        assert_eq!(one_short.percent, Some(99.0));
        assert_eq!(one_short.status, "Hashing · 99999999 / 100000000 bytes (99%)");
        let over = presentation(150, Some(100));
        assert_eq!(over.percent, Some(100.0));
        assert_eq!(over.status, "Hashing · 150 / 100 bytes (100%)");
    }
}
