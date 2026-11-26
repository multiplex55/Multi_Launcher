//! Retained controls for the shared RegexSession substitution result.
use super::regex_tester::RegexWorkspace;
use super::*;
use multi_launcher::regex_tester::SubstitutionEvaluationResult;
use std::time::Instant;
const PREVIEW_BYTES: usize = 16 * 1024;
fn utf8_prefix(text: &str, maximum: usize) -> &str {
    if text.len() <= maximum {
        return text;
    }
    let mut end = maximum;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
fn preview(text: &str) -> &str {
    utf8_prefix(text, PREVIEW_BYTES)
}
pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let enabled = view.open && !view.rehosting && !view.admission.is_fenced();
    let active = view.session.substitution_enabled();
    let receipt = view.mount.receipt.clone();
    let toggle_receipt = receipt.clone();
    let clear_receipt = receipt.clone();
    let owner = cx.entity().downgrade();
    let toggle = Checkbox::new("regex-substitution-enabled")
        .checked(active)
        .disabled(!enabled)
        .accessibility_label("Enable substitution preview")
        .on_change(move |state, _, _, app| {
            let _ = owner.update(app, |view, cx| {
                if !toggle_receipt.is_current()
                    || !view.open
                    || view.rehosting
                    || view.admission.is_fenced()
                {
                    return;
                }
                view.session
                    .set_substitution_enabled(state == CheckboxState::Checked, Instant::now());
                view.pump(cx);
                cx.notify();
            });
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    CheckboxIndicator::new()
                        .checked(active)
                        .size(px(16.0))
                        .border_1()
                        .child(if active { "✓" } else { "" }),
                )
                .child(Label::new("Enable substitution preview")),
        );
    let (status, output) = if !active {
        ("Substitution preview is disabled.".to_owned(), None)
    } else {
        match view.session.substitution_result() {
            None => ("Replacement preview pending…".to_owned(), None),
            Some(SubstitutionEvaluationResult::InvalidPattern(error)) => {
                (format!("Invalid pattern: {}", error.message), None)
            }
            Some(SubstitutionEvaluationResult::Suspended(reason)) => {
                (format!("Replacement preview paused: {reason:?}"), None)
            }
            Some(SubstitutionEvaluationResult::Success(result)) => {
                let text = preview(&result.output);
                (
                    format!(
                        "{} replacements · {} UTF-8 bytes{}",
                        result.replacements_made,
                        result.output.len(),
                        if text.len() < result.output.len() {
                            " · preview truncated"
                        } else {
                            ""
                        }
                    ),
                    Some(text.to_owned()),
                )
            }
        }
    };
    let replacement = &view.session.draft.replacement;
    let replacement_editor = if replacement.len() > view.session.policy().replacement_bytes {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(Label::new(format!(
                "Replacement retained ({} bytes); exceeds the {} byte interactive limit.",
                replacement.len(),
                view.session.policy().replacement_bytes
            )))
            .child(
                Label::new(utf8_prefix(replacement, 128).to_owned())
                    .id("regex-replacement-oversize-preview"),
            )
            .into_any_element()
    } else {
        Textarea::new(&view.mount.replacement)
            .readonly(!enabled)
            .aria_label("Replacement")
            .into_any_element()
    };
    div()
        .id("regex-substitution")
        .flex()
        .flex_col()
        .gap_2()
        .child(Label::new("Substitution"))
        .child(toggle)
        .child(Label::new("Replacement"))
        .child(Label::new(
            "Rust replacement syntax: $1, ${name}, and $$ for a literal dollar.",
        ))
        .child(replacement_editor)
        .child(
            Button::new("regex-replacement-clear")
                .label("Clear replacement")
                .disabled(!enabled)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !clear_receipt.is_current()
                        || !view.open
                        || view.rehosting
                        || view.admission.is_fenced()
                    {
                        return;
                    }
                    if !view.session.draft.replacement.is_empty() {
                        view.session.draft.replacement.clear();
                        view.mount
                            .replacement
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        view.session.mark_replacement_changed(Instant::now());
                        view.pump(cx);
                        cx.notify();
                    }
                })),
        )
        .child(Label::new(status).id("regex-substitution-status"))
        .children(output.map(|output| {
            div()
                .id("regex-substitution-output-scroll")
                .max_h(px(180.0))
                .overflow_y_scroll()
                .child(Label::new(output).id("regex-substitution-output"))
        }))
        .into_any_element()
}
#[cfg(test)]
mod tests {
    use super::{PREVIEW_BYTES, preview};
    #[test]
    fn preview_is_bounded_by_utf8_bytes_without_splitting() {
        let source = format!("{}é!", "a".repeat(PREVIEW_BYTES - 1));
        assert_eq!(preview(&source).len(), PREVIEW_BYTES - 1);
        assert!(source.starts_with(preview(&source)));
    }
}
