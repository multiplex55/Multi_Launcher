//! Bounded virtual XML tree and checked detail actions.
use super::xml_utility::XmlWorkspace;
use super::*;
use gpui_kit::base::Selectable;
use gpui_kit::component::Sizable as _;
use multi_launcher::xml_toolkit_engine::{
    XmlNodeDetailKind, XmlTreeNodeKind, XmlTreeProjectionLimit,
};
use multi_launcher::xml_toolkit_session::{XmlDetailIntent, XmlDetailStatus, XmlTreeStatus};
use multi_launcher::xml_tree_model::{self, TreeRowIdentity};

pub(super) fn render(view: &mut XmlWorkspace, cx: &mut Context<XmlWorkspace>) -> AnyElement {
    let Some(snapshot) = view.current_snapshot() else {
        return div()
            .child("Enter XML source to browse its tree.")
            .into_any_element();
    };
    let Some(revision) = snapshot.revision else {
        return div().into_any_element();
    };
    let owner = cx.entity().downgrade();
    let mount = view.tree_mount();
    let row_mount = mount.clone();
    let rows = gpui_kit::uniform_list(
        "xml-tree-rows",
        view.tree.row_count(),
        move |range, _, app| {
            let Some(entity) = owner.upgrade() else {
                return Vec::<AnyElement>::new();
            };
            let state = entity.read(app);
            if !state.tree_live()
                || state
                    .current_snapshot()
                    .is_none_or(|s| s.revision != Some(revision))
            {
                return Vec::new();
            }
            range
                .filter_map(|index| {
                    let (identity, node, depth) = state.tree.row(index)?;
                    let id = format!("{revision:?}-{identity:?}");
                    let mut row = div()
                        .id(SharedString::from(format!("xml-row-{id}")))
                        .w_full()
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .gap_1()
                        .pl(px((depth.min(32) * 12) as f32));
                    let Some(node) = node else {
                        let message = match identity {
                            TreeRowIdentity::MoreAttributes(_) => {
                                "… attributes omitted by projection limit"
                            }
                            TreeRowIdentity::MoreChildren(_) => {
                                "… child nodes omitted by projection limit"
                            }
                            TreeRowIdentity::Node(_) => "… omitted by projection limit",
                        };
                        return Some(row.child(message).into_any_element());
                    };
                    let projected = state.tree.projection()?.node(node)?;
                    let expandable = !projected.attributes.is_empty()
                        || !projected.children.is_empty()
                        || projected.has_more_attributes
                        || projected.has_more_children;
                    if expandable {
                        let target = owner.clone();
                        let receipt = row_mount.clone();
                        row = row.child(
                            Button::new(SharedString::from(format!("xml-toggle-{id}")))
                                .label(if state.tree.is_expanded(node) {
                                    "▾"
                                } else {
                                    "▸"
                                })
                                .small()
                                .tab_stop(true)
                                .on_click(move |_, _, app| {
                                    if receipt.is_current() {
                                        let _ = target.update(app, |view, cx| {
                                            view.tree_toggle(node, revision, cx)
                                        });
                                    }
                                }),
                        );
                    }
                    let target = owner.clone();
                    let receipt = row_mount.clone();
                    Some(
                        row.child(
                            Button::new(SharedString::from(format!("xml-select-{id}")))
                                .label(xml_tree_model::tree_node_label(projected))
                                .small()
                                .tab_stop(true)
                                .selected(state.tree.selected().is_some_and(|s| s.node == node))
                                .on_click(move |_, _, app| {
                                    if receipt.is_current() {
                                        let _ = target.update(app, |view, cx| {
                                            view.tree_select(node, revision, cx)
                                        });
                                    }
                                }),
                        )
                        .into_any_element(),
                    )
                })
                .collect()
        },
    )
    .track_scroll(&view.tree_scroll)
    .h(px(220.0))
    .w_full();
    let mut panel = div()
        .id("xml-tree-panel")
        .w_full()
        .flex()
        .flex_col()
        .gap_2();
    panel = match &snapshot.tree {
        XmlTreeStatus::Empty => panel.child("Enter XML source to browse its tree."),
        XmlTreeStatus::Pending => panel.child("Parsing XML tree…"),
        XmlTreeStatus::Failed(error) => panel.child(format!(
            "XML tree unavailable: {}",
            multi_launcher::xml_source::xml_error_message(error)
        )),
        XmlTreeStatus::Ready(projection) => {
            let mut panel = panel.child(rows);
            if let Some(limit) = projection.truncated {
                panel = panel.child(match limit {
                    XmlTreeProjectionLimit::NodeCount => "Tree projection reached its node limit.",
                    XmlTreeProjectionLimit::MetadataBytes => {
                        "Tree projection reached its metadata limit."
                    }
                });
            }
            panel
        }
    };
    let Some(selection) = view.tree.selected() else {
        return panel
            .child("Select a node to inspect it.")
            .into_any_element();
    };
    let Some(node) = view.tree.projection().and_then(|p| p.node(selection.node)) else {
        return panel
            .child("The selected node is unavailable.")
            .into_any_element();
    };
    let mut detail = div()
        .id("xml-tree-detail")
        .max_h(px(240.0))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap_1()
        .child(format!(
            "Node: {}",
            xml_tree_model::bounded_chars(
                &node.qualified_name,
                xml_tree_model::MAX_TREE_LABEL_CHARS
            )
        ))
        .child(format!(
            "Kind: {}",
            xml_tree_model::node_kind_label(node.kind)
        ));
    if !node.local_name.is_empty() && node.local_name != node.qualified_name {
        detail = detail.child(format!(
            "Local name: {}",
            xml_tree_model::bounded_chars(&node.local_name, xml_tree_model::MAX_TREE_LABEL_CHARS)
        ));
    }
    if !node.prefix.is_empty() {
        detail = detail.child(format!(
            "Prefix: {}",
            xml_tree_model::bounded_chars(&node.prefix, xml_tree_model::MAX_TREE_LABEL_CHARS)
        ));
    }
    if !node.namespace_uri.is_empty() {
        detail = detail.child(format!(
            "Namespace: {}",
            xml_tree_model::bounded_chars(
                &node.namespace_uri,
                xml_tree_model::MAX_TREE_LABEL_CHARS
            )
        ));
    }
    if !node.preview.is_empty() || node.preview_truncated {
        detail = detail.child(format!(
            "Value preview: {}",
            xml_tree_model::xml_escaped_display(
                &node.preview,
                multi_launcher::xml_toolkit_engine::MAX_XML_TREE_PREVIEW_CHARS
            )
        ));
    }
    let kinds: Vec<(&str, XmlNodeDetailKind)> = if node.kind == XmlTreeNodeKind::Element {
        vec![
            ("Copy text content", XmlNodeDetailKind::ElementText),
            ("Copy XML fragment", XmlNodeDetailKind::XmlFragment),
        ]
    } else {
        xml_tree_model::default_detail_kind(Some(node.kind))
            .map(|kind| vec![("Copy complete value", kind)])
            .unwrap_or_default()
    };
    for (label, kind) in kinds {
        let receipt = mount.clone();
        detail = detail.child(
            Button::new(SharedString::from(format!("xml-detail-copy-{kind:?}")))
                .label(label)
                .small()
                .tab_stop(true)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if receipt.is_current() {
                        view.tree_detail(selection, kind, XmlDetailIntent::Copy, cx);
                    }
                })),
        );
    }
    match &snapshot.detail {
        XmlDetailStatus::Pending(request)
            if xml_tree_model::request_matches_selection(*request, selection) =>
        {
            detail = detail.child(if request.intent == XmlDetailIntent::Copy {
                "Preparing complete value for explicit copy…"
            } else {
                "Loading selected node value…"
            })
        }
        XmlDetailStatus::Ready(output)
            if output.request.intent == XmlDetailIntent::Inspect
                && xml_tree_model::request_matches_selection(output.request, selection) =>
        {
            let (value, clipped) = xml_tree_model::bounded_prefix(
                &output.value,
                xml_tree_model::MAX_DETAIL_DISPLAY_CHARS,
            );
            detail = detail.child(value.to_owned());
            if clipped {
                detail = detail.child(
                    "Displayed value is clipped; the async result retains the complete value.",
                );
            }
        }
        XmlDetailStatus::Failed { request, error }
            if xml_tree_model::request_matches_selection(*request, selection) =>
        {
            detail = detail.child(format!(
                "Node detail unavailable: {}",
                multi_launcher::xml_source::xml_error_message(error)
            ))
        }
        _ => {}
    }
    panel.child(detail).into_any_element()
}
