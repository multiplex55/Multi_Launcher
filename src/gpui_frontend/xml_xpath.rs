//! Retained XPath controls and virtual results for the XML workspace.
use super::xml_utility::XmlWorkspace;
use super::*;
use gpui_kit::base::Selectable;
use gpui_kit::component::Sizable as _;
use multi_launcher::xml_toolkit_engine::{XmlNodeDetailKind, XmlTreeNodeKind};
use multi_launcher::xml_xpath_model::{self, NavigationDirection, VisibleXPathStatus};

pub(super) fn render(view: &mut XmlWorkspace, cx: &mut Context<XmlWorkspace>) -> AnyElement {
    let mount = view.tree_mount();
    let generation = view.xpath.generation();
    let run_mount = mount.clone();
    let toggle_mount = mount.clone();
    let add_mount = mount.clone();
    let mut panel = div()
        .id("xml-xpath-panel")
        .w_full()
        .flex()
        .flex_col()
        .gap_2()
        .child(Label::new("XPath 1.0"))
        .child(Input::new(&view.xpath_expression).aria_label("XPath expression"))
        .child(
            Button::new("xpath-namespaces-toggle")
                .label("Namespace prefix mappings")
                .on_click(cx.listener(move |view, _, _, cx| {
                    if toggle_mount.is_current() && view.xpath_live() {
                        view.xpath_namespaces_open = !view.xpath_namespaces_open;
                        cx.notify();
                    }
                })),
        );
    if view.xpath.expression_was_clipped() {
        panel = panel.child(format!(
            "XPath expressions are limited to {} bytes; extra text was clipped.",
            multi_launcher::xml_toolkit_engine::MAX_XPATH_EXPRESSION_BYTES
        ));
    }
    if view.xpath.namespaces_were_clipped() {
        panel = panel.child(format!(
            "Namespace mappings are limited to {} bytes; extra text was clipped.",
            multi_launcher::xml_toolkit_engine::MAX_XPATH_NAMESPACE_BYTES
        ));
    }
    if view.xpath_namespaces_open {
        let mut mappings = div()
            .id("xpath-namespaces")
            .max_h(px(150.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1();
        for binding in &view.xpath_bindings {
            let id = binding.id;
            let receipt = mount.clone();
            mappings = mappings.child(
                div()
                    .id(SharedString::from(format!("xpath-namespace-{id}")))
                    .flex()
                    .gap_1()
                    .child(Input::new(&binding.prefix).aria_label("Namespace prefix"))
                    .child(Input::new(&binding.uri).aria_label("Namespace URI"))
                    .child(
                        Button::new(SharedString::from(format!("xpath-remove-{id}")))
                            .label("Remove")
                            .small()
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if receipt.is_current() {
                                    view.xpath_remove_namespace(id, cx);
                                }
                            })),
                    ),
            );
        }
        let can_add = view.xpath_bindings.len() < xml_xpath_model::MAX_NAMESPACE_ROWS;
        mappings = mappings.child(
            Button::new("xpath-add-namespace")
                .label("Add prefix mapping")
                .disabled(!can_add)
                .tab_stop(can_add)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if add_mount.is_current() {
                        view.xpath_add_namespace(window, cx);
                    }
                })),
        );
        panel = panel.child(mappings);
    }
    let snapshot = view.current_snapshot();
    let namespace_error = view.xpath.namespace_error();
    if let Some(error) = &namespace_error {
        panel = panel.child(error.clone());
    }
    let can_run = namespace_error.is_none()
        && snapshot.as_ref().is_some_and(|s| {
            matches!(
                s.tree,
                multi_launcher::xml_toolkit_session::XmlTreeStatus::Ready(_)
            ) && s.parse == multi_launcher::xml_toolkit_session::XmlParseStatus::Ready
        });
    panel = panel.child(
        Button::new("xpath-run")
            .label("Run XPath")
            .disabled(!can_run)
            .tab_stop(can_run)
            .on_click(cx.listener(move |view, _, _, cx| {
                if run_mount.is_current() && view.xpath.generation() == generation {
                    view.xpath_run(cx);
                }
            })),
    );
    let Some(snapshot) = snapshot else {
        return panel.child("No XML source is loaded.").into_any_element();
    };
    match view.xpath.visible_status(&snapshot) {
        VisibleXPathStatus::Empty => panel.child("No XML source is loaded.").into_any_element(),
        VisibleXPathStatus::NotRun => panel
            .child("Run the expression against the current parsed document.")
            .into_any_element(),
        VisibleXPathStatus::Pending => panel.child("Running XPath…").into_any_element(),
        VisibleXPathStatus::Failed(error) => panel
            .child(format!(
                "XPath error: {}",
                multi_launcher::xml_source::xml_error_message(error)
            ))
            .into_any_element(),
        VisibleXPathStatus::ScalarText(text) => {
            let (preview, clipped) =
                xml_xpath_model::bounded_prefix(text, xml_xpath_model::MAX_RESULT_VALUE_CHARS);
            let receipt = mount.clone();
            let can_copy = view.clipboard_available();
            panel
                .child("Scalar result (text)")
                .child(preview.to_owned())
                .when(clipped, |panel| {
                    panel.child(
                        "Scalar display is clipped; explicit copy keeps the complete result.",
                    )
                })
                .child(
                    Button::new("xpath-copy-scalar")
                        .label("Copy scalar text")
                        .disabled(!can_copy)
                        .tab_stop(can_copy)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if receipt.is_current() {
                                view.xpath_copy_scalar(generation, cx);
                            }
                        })),
                )
                .into_any_element()
        }
        VisibleXPathStatus::Nodes(nodes) => {
            let count = nodes.len();
            let selected = view.xpath.selected_result();
            panel = panel.child(format!("{count} matches"));
            let prev =
                xml_xpath_model::navigation_target(count, selected, NavigationDirection::Previous);
            let next =
                xml_xpath_model::navigation_target(count, selected, NavigationDirection::Next);
            let a = mount.clone();
            let b = mount.clone();
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("xpath-previous")
                            .label("Previous")
                            .disabled(prev.is_none())
                            .tab_stop(prev.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if a.is_current() {
                                    if let Some(index) = prev {
                                        view.xpath_navigate(index, generation, cx);
                                    }
                                }
                            })),
                    )
                    .child(
                        Button::new("xpath-next")
                            .label("Next")
                            .disabled(next.is_none())
                            .tab_stop(next.is_some())
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if b.is_current() {
                                    if let Some(index) = next {
                                        view.xpath_navigate(index, generation, cx);
                                    }
                                }
                            })),
                    ),
            );
            let owner = cx.entity().downgrade();
            let row_mount = mount.clone();
            let rows = gpui_kit::uniform_list("xpath-results", count, move |range, _, app| {
                let Some(entity) = owner.upgrade() else {
                    return Vec::<AnyElement>::new();
                };
                let state = entity.read(app);
                if !state.xpath_live() || state.xpath.generation() != generation {
                    return Vec::new();
                }
                let Some(snapshot) = state.current_snapshot() else {
                    return Vec::new();
                };
                let VisibleXPathStatus::Nodes(nodes) = state.xpath.visible_status(&snapshot) else {
                    return Vec::new();
                };
                range
                    .filter_map(|index| {
                        let node = nodes.get(index)?;
                        let target = owner.clone();
                        let receipt = row_mount.clone();
                        Some(
                            div()
                                .id(SharedString::from(format!(
                                    "xpath-row-{generation}-{index}-{:?}",
                                    node.id
                                )))
                                .h(px(32.0))
                                .w_full()
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "xpath-result-{generation}-{index}-{:?}",
                                        node.id
                                    )))
                                    .label(xml_xpath_model::result_label(index, node))
                                    .small()
                                    .tab_stop(true)
                                    .selected(state.xpath.selected_result() == Some(index))
                                    .on_click(
                                        move |_, _, app| {
                                            if receipt.is_current() {
                                                let _ = target.update(app, |view, cx| {
                                                    view.xpath_navigate(index, generation, cx)
                                                });
                                            }
                                        },
                                    ),
                                )
                                .into_any_element(),
                        )
                    })
                    .collect()
            })
            .track_scroll(&view.xpath_scroll)
            .h(px(220.0))
            .w_full();
            panel = panel.child(rows);
            if let Some(status) = view.xpath_copy_status(&snapshot) {
                panel = panel.child(status);
            }
            if let Some((index, node)) =
                selected.and_then(|index| nodes.get(index).map(|node| (index, node)))
            {
                let projected = view
                    .tree
                    .projection()
                    .is_some_and(|tree| tree.node(node.id).is_some());
                panel = panel.child(format!(
                    "Selected result {}: {}",
                    index + 1,
                    xml_xpath_model::result_label(index, node)
                ));
                let (value, clipped) = xml_xpath_model::bounded_prefix(
                    &node.value,
                    xml_xpath_model::MAX_RESULT_VALUE_CHARS,
                );
                if !value.is_empty() || clipped {
                    panel = panel.child(value.to_owned());
                }
                if clipped {
                    panel = panel.child("Value preview is clipped.");
                }
                if !projected {
                    panel = panel.child("This result is outside the bounded tree projection; navigation and tree-routed copy are unavailable.");
                } else {
                    let receipt = mount.clone();
                    let node_id = node.id;
                    let multi_launcher::xml_toolkit_session::XmlXPathStatus::Nodes {
                        request, ..
                    } = &snapshot.xpath
                    else {
                        return panel.into_any_element();
                    };
                    let request = request.clone();
                    panel = panel.child(
                        Button::new("xpath-reveal")
                            .label("Reveal in Tree")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if receipt.is_current() {
                                    view.xpath_reveal(
                                        generation, index, node_id, &request, window, cx,
                                    );
                                }
                            })),
                    );
                    let kinds: Vec<(&str, XmlNodeDetailKind)> = match node.kind {
                        XmlTreeNodeKind::Element => vec![
                            ("Copy text content", XmlNodeDetailKind::ElementText),
                            ("Copy XML fragment", XmlNodeDetailKind::XmlFragment),
                        ],
                        XmlTreeNodeKind::Attribute => {
                            vec![("Copy attribute value", XmlNodeDetailKind::AttributeValue)]
                        }
                        XmlTreeNodeKind::Text
                        | XmlTreeNodeKind::CData
                        | XmlTreeNodeKind::Comment
                        | XmlTreeNodeKind::ProcessingInstruction => {
                            vec![("Copy node value", XmlNodeDetailKind::TextValue)]
                        }
                        _ => Vec::new(),
                    };
                    for (label, kind) in kinds {
                        let receipt = mount.clone();
                        panel = panel.child(
                            Button::new(SharedString::from(format!("xpath-copy-{kind:?}")))
                                .label(label)
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if receipt.is_current() {
                                        view.xpath_copy_node(index, kind, generation, cx);
                                    }
                                })),
                        );
                    }
                }
            }
            panel.into_any_element()
        }
    }
}
