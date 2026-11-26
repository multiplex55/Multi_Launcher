//! Bounded syntax tree and checked constructor controls.
use super::regex_tester::RegexWorkspace;
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::regex_tester::{
    EvaluationResult, ExplanationResult, RegexSyntaxNodeRef, RegexSyntaxVisualizationResult,
    constructor::{self, Constructor},
    policy::MAX_PATTERN_BYTES,
};
use std::collections::HashSet;

pub(super) fn reveal_delta(top: f32, bottom: f32, viewport_top: f32, viewport_bottom: f32) -> f32 {
    if top < viewport_top {
        viewport_top - top
    } else if bottom > viewport_bottom {
        viewport_bottom - bottom
    } else {
        0.
    }
}

#[derive(Default)]
pub(super) struct SyntaxState {
    pub selected: Option<RegexSyntaxNodeRef>,
    collapsed: HashSet<RegexSyntaxNodeRef>,
}
pub(super) fn reconcile(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) {
    let Some(tree) = view
        .session
        .visualization()
        .and_then(|result| result.tree())
    else {
        view.syntax.selected = None;
        view.syntax.collapsed.clear();
        return;
    };
    view.syntax
        .collapsed
        .retain(|reference| tree.resolve(reference).is_some());
    let editor = view.mount.pattern.read(cx);
    view.syntax.selected = if editor.value().as_ref() == tree.source()
        && view.session.draft.pattern == tree.source()
        && view.session.draft.flags == tree.flags()
    {
        tree.best_node_for_selection(editor.selected_range())
    } else {
        None
    };
}
fn tree_current(view: &RegexWorkspace, cx: &App) -> bool {
    view.session
        .visualization()
        .and_then(|result| result.tree())
        .is_some_and(|tree| {
            tree.source() == view.session.draft.pattern
                && tree.flags() == view.session.draft.flags
                && view.mount.pattern.read(cx).value().as_ref() == tree.source()
        })
}
fn current(view: &RegexWorkspace, receipt: &utility_mount::MountReceipt, revision: u64) -> bool {
    receipt.is_current()
        && view.open
        && !view.rehosting
        && !view.admission.is_fenced()
        && !view.presets.has_confirmation()
        && view.inspectors.is_syntax()
        && view.inspectors.revision == revision
}
pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let receipt = view.mount.receipt.clone();
    let revision = view.inspectors.revision;
    let editor = view.mount.pattern.read(cx);
    let selection = editor.selected_range();
    let source = editor.value();
    let flags = view.session.draft.flags;
    let valid = source.as_ref() == view.session.draft.pattern.as_str()
        && source.len() <= MAX_PATTERN_BYTES
        && source.get(selection.clone()).is_some();
    let snapshot: Option<Arc<str>> = valid.then(|| Arc::from(source.as_ref()));
    let mut panel = div()
        .id("regex-syntax")
        .flex()
        .flex_col()
        .gap_2()
        .child(Label::new("Construct"));
    let mut palette = div().flex().flex_wrap().gap_1();
    for (constructor, label) in constructor::ALL {
        let mount = receipt.clone();
        let expected_source = snapshot.clone();
        let expected_selection = selection.clone();
        let enabled =
            valid && !(constructor == Constructor::EscapeSelection && selection.is_empty());
        palette = palette.child(
            Button::new(SharedString::from(format!(
                "regex-constructor-{constructor:?}"
            )))
            .label(label)
            .disabled(!enabled)
            .on_click(cx.listener(move |view, _, window, cx| {
                let Some(expected_source) = expected_source.as_deref() else {
                    return;
                };
                if !current(view, &mount, revision)
                    || view.session.draft.pattern != expected_source
                    || view.session.draft.flags != flags
                {
                    return;
                }
                let editor = view.mount.pattern.read(cx);
                if editor.value().as_ref() != expected_source
                    || editor.selected_range() != expected_selection
                {
                    return;
                }
                let Some((replacement, caret)) = constructor::replacement(
                    expected_source,
                    expected_selection.clone(),
                    constructor,
                ) else {
                    return;
                };
                if replacement.len() > MAX_PATTERN_BYTES || replacement == expected_source {
                    return;
                }
                let end_suffix = expected_source.len() - expected_selection.end;
                let Some(inserted) = replacement
                    .get(expected_selection.start..replacement.len() - end_suffix)
                    .map(str::to_owned)
                else {
                    return;
                };
                view.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel);
                view.clipboard_epochs.main_changed(true);
                view.clipboard_feedback = None;
                view.session.draft.pattern = replacement;
                view.mount.pattern.update(cx, |input, cx| {
                    input.set_selected_range(expected_selection.clone(), cx);
                    input.replace(inserted, window, cx);
                    input.set_selected_range(caret, cx);
                });
                view.mount.pattern.focus_handle(cx).focus(window, cx);
                view.reveal_pattern(window, cx);
                view.finish_prepared_change(cx);
            })),
        );
    }
    panel = panel.child(palette);
    let tree = match view.session.visualization() {
        Some(RegexSyntaxVisualizationResult::Available(tree)) => tree,
        Some(RegexSyntaxVisualizationResult::Unavailable(reason)) => {
            return panel
                .child(Label::new(format!("Syntax unavailable: {reason:?}")))
                .into_any_element();
        }
        None => {
            let message = match view.session.result() {
                Some(EvaluationResult::InvalidPattern(error)) => {
                    format!("Current pattern is invalid: {}", error.message)
                }
                Some(EvaluationResult::Suspended(reason)) => {
                    format!("Syntax unavailable while evaluation is suspended: {reason:?}")
                }
                _ => "Syntax appears after the current pattern and flags finish evaluation.".into(),
            };
            return panel.child(Label::new(message)).into_any_element();
        }
    };
    let rows: Vec<_> = tree
        .nodes()
        .iter()
        .filter(|node| {
            let mut parent = node.parent();
            while let Some(id) = parent {
                let Some(reference) = tree.node_ref(id) else {
                    return false;
                };
                if view.syntax.collapsed.contains(&reference) {
                    return false;
                }
                parent = tree.node(id).and_then(|node| node.parent());
            }
            true
        })
        .filter_map(|node| {
            tree.node_ref(node.id()).map(|reference| {
                (
                    reference,
                    node.display_label().to_owned(),
                    node.depth(),
                    !node.children().is_empty(),
                )
            })
        })
        .collect();
    let rows = Arc::new(rows);
    let count = rows.len();
    let owner = cx.entity().downgrade();
    let row_receipt = receipt.clone();
    let list = gpui_kit::uniform_list("regex-syntax-tree", count, move |range, _, app| {
        let Some(entity) = owner.upgrade() else {
            return Vec::<AnyElement>::new();
        };
        let view = entity.read(app);
        if !current(view, &row_receipt, revision) || !tree_current(view, app) {
            return Vec::new();
        }
        range
            .filter_map(|index| {
                let (reference, label, depth, has_children) = rows.get(index)?;
                let tree = view.session.visualization()?.tree()?;
                tree.resolve(reference)?;
                let is_collapsed = view.syntax.collapsed.contains(reference);
                let selected = view.syntax.selected.as_ref() == Some(reference);
                let target = owner.clone();
                let select_receipt = row_receipt.clone();
                let selected_ref = reference.clone();
                let toggle_target = owner.clone();
                let toggle_receipt = row_receipt.clone();
                let toggle_ref = reference.clone();
                let id = reference.id().get();
                let mut row = div()
                    .id(SharedString::from(format!(
                        "regex-syntax-node-{}-{id}",
                        reference.generation()
                    )))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .pl(px((*depth as f32 * 12.).min(160.)));
                if *has_children {
                    row = row.child(
                        Button::new(SharedString::from(format!("regex-syntax-expand-{id}")))
                            .small()
                            .label(if is_collapsed { "▸" } else { "▾" })
                            .on_click(move |_, _, app| {
                                let _ = toggle_target.update(app, |view, cx| {
                                    if current(view, &toggle_receipt, revision)
                                        && tree_current(view, cx)
                                        && view
                                            .session
                                            .visualization()
                                            .and_then(|r| r.tree())
                                            .and_then(|t| t.resolve(&toggle_ref))
                                            .is_some()
                                    {
                                        if !view.syntax.collapsed.remove(&toggle_ref) {
                                            view.syntax.collapsed.insert(toggle_ref.clone());
                                        }
                                        cx.notify();
                                    }
                                });
                            }),
                    );
                }
                row = row.child(
                    Button::new(SharedString::from(format!("regex-syntax-select-{id}")))
                        .small()
                        .selected(selected)
                        .label(label.clone())
                        .on_click(move |_, window, app| {
                            let _ = target.update(app, |view, cx| {
                                if !current(view, &select_receipt, revision)
                                    || !tree_current(view, cx)
                                {
                                    return;
                                }
                                let Some(span) = view
                                    .session
                                    .visualization()
                                    .and_then(|r| r.tree())
                                    .and_then(|t| t.resolve(&selected_ref))
                                    .map(|n| n.focus_span().as_range())
                                else {
                                    return;
                                };
                                view.syntax.selected = Some(selected_ref.clone());
                                view.mount
                                    .pattern
                                    .update(cx, |input, cx| input.set_selected_range(span, cx));
                                view.mount.pattern.focus_handle(cx).focus(window, cx);
                                view.reveal_pattern(window, cx);
                                cx.notify();
                            });
                        }),
                );
                Some(row.into_any_element())
            })
            .collect()
    })
    .h(px(224.));
    panel = panel.child(list);
    if let Some(reference) = view.syntax.selected.as_ref() {
        if let Some(ExplanationResult::Success { explanations }) = view.session.explanation() {
            if let Some(node) = tree.resolve(reference) {
                for index in node.explanation_indices() {
                    if let Some(explanation) = explanations.get(*index) {
                        panel = panel.child(
                            Label::new(format!(
                                "{}: {}",
                                explanation.label, explanation.description
                            ))
                            .id(SharedString::from(format!(
                                "regex-syntax-explanation-{}-{}-{index}",
                                reference.generation(),
                                reference.id().get()
                            ))),
                        );
                    }
                }
                if !node.explanation_indices().is_empty() {
                    let expected = reference.clone();
                    let mount = receipt.clone();
                    panel = panel.child(
                        Button::new("regex-syntax-open-explanation")
                            .label("Open explanation list")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if current(view, &mount, revision)
                                    && tree_current(view, cx)
                                    && view.syntax.selected.as_ref() == Some(&expected)
                                    && view
                                        .session
                                        .visualization()
                                        .and_then(|r| r.tree())
                                        .and_then(|tree| tree.resolve(&expected))
                                        .is_some()
                                {
                                    view.inspectors.open_explanation(&view.clipboard_epochs);
                                    cx.notify();
                                }
                            })),
                    );
                }
            }
        }
    }
    panel.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::reveal_delta;
    #[test]
    fn reveal_moves_only_as_needed_to_show_pattern() {
        assert_eq!(reveal_delta(25., 45., 0., 100.), 0.);
        assert_eq!(reveal_delta(-15., 5., 0., 100.), 15.);
        assert_eq!(reveal_delta(95., 115., 0., 100.), -15.);
    }
}
