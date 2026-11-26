//! Retained Regex learning inspectors backed by the shared catalog and session.
use super::regex_tester::RegexWorkspace;
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::regex_tester::{
    BUILT_IN_EXAMPLES, EvaluationResult, ExplanationResult, ReferenceCategory, RegexExample,
    search_reference,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Explanation,
    Reference,
    Examples,
    Syntax,
}
#[derive(Default)]
pub(super) struct InspectorState {
    section: Option<Section>,
    pub reference_query: String,
    pub examples_query: String,
    category: Option<ReferenceCategory>,
    pub revision: u64,
}
impl InspectorState {
    pub fn is_reference(&self) -> bool {
        self.section == Some(Section::Reference)
    }
    pub fn is_examples(&self) -> bool {
        self.section == Some(Section::Examples)
    }
    pub fn open_explanation(&mut self, epochs: &super::regex_clipboard::Epochs) {
        self.revoke(epochs);
        self.section = Some(Section::Explanation);
    }
    pub fn is_syntax(&self) -> bool {
        self.section == Some(Section::Syntax)
    }
    pub fn revoke(&mut self, epochs: &super::regex_clipboard::Epochs) {
        self.revision = self.revision.wrapping_add(1);
        epochs.reference.fetch_add(1, Ordering::AcqRel);
    }
}
fn example_matches(example: &RegexExample, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || [example.name, example.description, example.pattern]
            .iter()
            .any(|s| s.to_lowercase().contains(&query))
}
fn current(
    view: &RegexWorkspace,
    mount: &utility_mount::MountReceipt,
    revision: u64,
    section: Section,
) -> bool {
    mount.is_current()
        && view.open
        && !view.rehosting
        && !view.admission.is_fenced()
        && !view.presets.has_confirmation()
        && view.inspectors.section == Some(section)
        && view.inspectors.revision == revision
}
fn bounded(text: &str, maximum: usize) -> &str {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let mount = view.mount.receipt.clone();
    let revision = view.inspectors.revision;
    let enabled = view.open
        && !view.rehosting
        && !view.admission.is_fenced()
        && !view.presets.has_confirmation();
    let mut panel = div().id("regex-inspectors").flex().flex_col().gap_2();
    let mut tabs = div().flex().gap_2();
    for (section, label, key) in [
        (Section::Explanation, "Explanation", "explanation"),
        (Section::Reference, "Reference", "reference"),
        (Section::Examples, "Examples", "examples"),
        (Section::Syntax, "Syntax Structure", "syntax"),
    ] {
        let receipt = mount.clone();
        let prefix = if view.inspectors.section == Some(section) {
            "▾"
        } else {
            "▸"
        };
        tabs = tabs.child(
            Button::new(SharedString::from(format!("regex-inspector-{key}")))
                .label(format!("{prefix} {label}"))
                .disabled(!enabled)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if receipt.is_current()
                        && view.open
                        && !view.rehosting
                        && !view.admission.is_fenced()
                        && !view.presets.has_confirmation()
                        && view.inspectors.revision == revision
                    {
                        view.inspectors.revoke(&view.clipboard_epochs);
                        if view
                            .mount
                            .reference_filter
                            .focus_handle(cx)
                            .is_focused(window)
                            || view
                                .mount
                                .examples_filter
                                .focus_handle(cx)
                                .is_focused(window)
                        {
                            view.focus_anchor(window, cx);
                        }
                        view.inspectors.section = if view.inspectors.section == Some(section) {
                            None
                        } else {
                            Some(section)
                        };
                        if view.inspectors.is_syntax() {
                            super::regex_syntax::reconcile(view, cx);
                        }
                        cx.notify();
                    }
                })),
        );
    }
    panel = panel.child(tabs);
    match view.inspectors.section {
        None => {}
        Some(Section::Syntax) => {
            panel = panel.child(super::regex_syntax::render(view, cx));
        }
        Some(Section::Explanation) => {
            let mut content = div()
                .id("regex-explanation")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(250.))
                .overflow_y_scroll();
            match view.session.explanation() {
                Some(ExplanationResult::Success { explanations }) if explanations.is_empty() => {
                    content = content.child(Label::new("No syntax to explain."));
                }
                Some(ExplanationResult::Success { explanations }) => {
                    for (index, item) in explanations.iter().enumerate() {
                        content = content.child(
                            Label::new(format!(
                                "{}–{}  {}  {}: {}",
                                item.span.start_byte(),
                                item.span.end_byte(),
                                item.token,
                                item.label,
                                item.description
                            ))
                            .id(SharedString::from(format!("regex-explanation-{index}"))),
                        );
                    }
                }
                Some(ExplanationResult::InvalidPattern(error)) => {
                    content =
                        content.child(Label::new(format!("Invalid pattern: {}", error.message)));
                }
                None => {
                    let message = match view.session.result() {
                        Some(EvaluationResult::InvalidPattern(error)) => {
                            format!("Invalid pattern: {}", error.message)
                        }
                        Some(EvaluationResult::Suspended(reason)) => {
                            format!("Evaluation suspended: {reason:?}")
                        }
                        Some(EvaluationResult::Success { .. }) => "Explanation unavailable.".into(),
                        None => "Explanation pending…".into(),
                    };
                    content = content.child(Label::new(message));
                }
            }
            panel = panel.child(content);
        }
        Some(Section::Reference) => {
            panel = panel.child(
                Input::new(&view.mount.reference_filter)
                    .disabled(!enabled)
                    .aria_label("Search reference"),
            );
            let mut categories = div().flex().flex_wrap().gap_1();
            for category in
                std::iter::once(None).chain(ReferenceCategory::ALL.into_iter().map(Some))
            {
                let receipt = mount.clone();
                let label = category.map_or("All categories", ReferenceCategory::title);
                categories = categories.child(
                    Button::new(SharedString::from(format!(
                        "regex-reference-category-{category:?}"
                    )))
                    .label(label)
                    .selected(view.inspectors.category == category)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if current(view, &receipt, revision, Section::Reference) {
                            view.inspectors.revoke(&view.clipboard_epochs);
                            view.inspectors.category = category;
                            cx.notify();
                        }
                    })),
                );
            }
            panel = panel.child(categories);
            let mut list = div()
                .id("regex-reference-list")
                .flex()
                .flex_col()
                .gap_2()
                .max_h(px(300.))
                .overflow_y_scroll();
            let mut found = false;
            for entry in
                search_reference(&view.inspectors.reference_query, view.inspectors.category)
            {
                found = true;
                let append = mount.clone();
                let copy = mount.clone();
                let mut row = div()
                    .id(SharedString::from(format!(
                        "regex-reference-row-{}",
                        entry.id
                    )))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new(format!("{} — {}", entry.title, entry.syntax)))
                    .child(Label::new(format!(
                        "{:?}: {}",
                        entry.syntax_kind, entry.description
                    )));
                if let Some(example) = entry.example {
                    row = row.child(Label::new(format!(
                        "Example: {}   Text: {}",
                        example.pattern, example.text
                    )));
                }
                row = row
                    .child(
                        Button::new(SharedString::from(format!(
                            "regex-reference-append-{}",
                            entry.id
                        )))
                        .label("Append syntax")
                        .disabled(!enabled)
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if !current(view, &append, revision, Section::Reference)
                                    || !search_reference(
                                        &view.inspectors.reference_query,
                                        view.inspectors.category,
                                    )
                                    .any(|item| item.id == entry.id)
                                {
                                    return;
                                }
                                let previous = view.session.draft.pattern.clone();
                                if view.mount.pattern.read(cx).value().as_ref() != previous.as_str()
                                {
                                    return;
                                }
                                view.clipboard_feedback = None;
                                view.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel);
                                view.clipboard_epochs.main_changed(true);
                                view.session.draft.pattern.push_str(entry.syntax);
                                view.mount.pattern.update(cx, |input, cx| {
                                    input.set_selected_range(previous.len()..previous.len(), cx);
                                    input.replace(entry.syntax, window, cx);
                                });
                                view.mount.pattern.focus_handle(cx).focus(window, cx);
                                view.finish_prepared_change(cx);
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "regex-reference-copy-{}",
                            entry.id
                        )))
                        .label("Copy syntax")
                        .disabled(!enabled || view.clipboard_pending)
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                if current(view, &copy, revision, Section::Reference)
                                    && search_reference(
                                        &view.inspectors.reference_query,
                                        view.inspectors.category,
                                    )
                                    .any(|item| item.id == entry.id)
                                {
                                    view.clipboard_action(
                                        super::regex_clipboard::Action::Reference(entry.syntax),
                                        window,
                                        cx,
                                    );
                                }
                            },
                        )),
                    );
                list = list.child(row);
            }
            if !found {
                list = list.child(Label::new("No reference entries match these filters."));
            }
            panel = panel.child(list);
        }
        Some(Section::Examples) => {
            panel = panel.child(
                Input::new(&view.mount.examples_filter)
                    .disabled(!enabled)
                    .aria_label("Search examples"),
            );
            let mut list = div()
                .id("regex-examples-list")
                .flex()
                .flex_col()
                .gap_2()
                .max_h(px(300.))
                .overflow_y_scroll();
            let mut found = false;
            for example in BUILT_IN_EXAMPLES
                .iter()
                .filter(|entry| example_matches(entry, &view.inspectors.examples_query))
            {
                found = true;
                let receipt = mount.clone();
                let mut row = div()
                    .id(SharedString::from(format!(
                        "regex-example-row-{}",
                        example.id
                    )))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new(example.name))
                    .child(Label::new(example.description))
                    .child(Label::new(example.pattern))
                    .child(Label::new(format!(
                        "Sample: {}",
                        bounded(example.sample_text, 200)
                    )));
                if let Some(replacement) = example.replacement {
                    row = row.child(Label::new(format!(
                        "Replacement: {}",
                        bounded(replacement, 120)
                    )));
                }
                row = row.child(
                    Button::new(SharedString::from(format!(
                        "regex-example-load-{}",
                        example.id
                    )))
                    .label("Load example")
                    .disabled(!enabled)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !current(view, &receipt, revision, Section::Examples)
                            || !example_matches(example, &view.inspectors.examples_query)
                        {
                            return;
                        }
                        view.clipboard_feedback = None;
                        let cases = view.session.draft.test_cases.clone();
                        let mut draft = example.to_draft();
                        draft.test_cases = cases;
                        view.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel);
                        view.clipboard_epochs.source.fetch_add(1, Ordering::AcqRel);
                        view.clipboard_epochs
                            .replacement
                            .fetch_add(1, Ordering::AcqRel);
                        view.clipboard_epochs.main_changed(true);
                        view.session.draft = draft;
                        let pattern = view.session.draft.pattern.clone();
                        let text = view.session.draft.test_text.clone();
                        let replacement = view.session.draft.replacement.clone();
                        view.mount
                            .pattern
                            .update(cx, |input, cx| input.set_value(pattern, window, cx));
                        view.mount
                            .text
                            .update(cx, |input, cx| input.set_value(text, window, cx));
                        view.mount
                            .replacement
                            .update(cx, |input, cx| input.set_value(replacement, window, cx));
                        view.mount.pattern.focus_handle(cx).focus(window, cx);
                        view.finish_prepared_change(cx);
                    })),
                );
                list = list.child(row);
            }
            if !found {
                list = list.child(Label::new("No examples match your search."));
            }
            panel = panel.child(list);
        }
    }
    panel.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{bounded, example_matches};
    use multi_launcher::regex_tester::BUILT_IN_EXAMPLES;
    #[test]
    fn example_search_is_case_insensitive_and_preview_keeps_utf8_boundaries() {
        let example = &BUILT_IN_EXAMPLES[0];
        assert!(example_matches(example, &example.name.to_uppercase()));
        assert!(example_matches(example, example.pattern));
        assert!(!example_matches(example, "not a real example 98234"));
        assert_eq!(bounded("aé中", 2), "a");
        assert_eq!(bounded("aé中", 3), "aé");
    }
}
