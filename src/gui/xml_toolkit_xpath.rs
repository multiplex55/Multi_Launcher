//! Egui renderer for the shared XPath model.
use crate::gui::jump_mode::TargetKind;
use crate::gui::xml_toolkit_dialog::XmlJumpTargets;
use crate::gui::xml_toolkit_tree::{XmlToolkitTreeIntents, XmlToolkitTreeViewState};
use crate::xml_toolkit_engine::{MAX_XPATH_EXPRESSION_BYTES, MAX_XPATH_NAMESPACE_BYTES, XmlEngineError, XmlNodeDetailKind, XmlNodeSnapshot, XmlTreeNodeKind, validate_xpath_namespaces};
use crate::xml_toolkit_session::{XmlNamespaceContextRevision, XmlParseStatus, XmlSessionSnapshot, XmlSourceRevision, XmlTreeStatus, XmlXPathQueryRequest, XmlXPathRequestId, XmlXPathStatus};
use crate::xml_xpath_model::{NamespaceBindingEditorRow, NamespaceRowAction, VisibleXPathStatus, NavigationDirection, MAX_NAMESPACE_ROWS, MAX_RESULT_LABEL_CHARS, MAX_RESULT_VALUE_CHARS, navigation_target, result_label, node_kind_label, bounded_chars, bounded_prefix, truncate_utf8, cap_namespace_editor, xml_escaped_display};
pub(crate) use crate::xml_xpath_model::{XmlToolkitXPathIntents, XmlToolkitXPathViewState};
use eframe::egui;
use std::sync::Arc;
const XPATH_RESULT_ROW_HEIGHT: f32 = 22.0;
impl XmlToolkitXPathViewState {
    /// Draw the XPath editor and bounded virtual result list. Query work,
    /// session invalidation, and explicit clipboard publication are returned
    /// to the caller as intents; no synchronous engine or clipboard operation
    /// occurs in this view.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
    ) -> XmlToolkitXPathIntents {
        self.show_scoped(ui, snapshot, tree, &mut XmlJumpTargets::new(None), true)
    }

    pub(crate) fn show_scoped(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
        targets: &mut XmlJumpTargets<'_>,
        authorized_pass: bool,
    ) -> XmlToolkitXPathIntents {
        self.sync(snapshot);
        tree.sync(snapshot);
        #[cfg(test)]
        {
            self.visible_rows = 0;
            self.rendered_rows = 0;
            self.max_rendered_result_row_height = 0.0;
        }
        let first_pass = ui.ctx().current_pass_index() == 0;
        let mut intents = XmlToolkitXPathIntents::default();
        let mut expression_edited = false;
        let mut namespaces_edited = false;
        let mut run_clicked = false;
        let mut previous_clicked = false;
        let mut next_clicked = false;
        let mut selected_row = None;
        let mut focus_selected_row = None;
        let mut scalar_copy_clicked = false;
        let mut node_copy_clicked = None;

        ui.heading("XPath 1.0");
        ui.horizontal(|ui| {
            ui.label("Expression");
            let expression = ui.add(
                egui::TextEdit::singleline(&mut self.expression)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .hint_text("//item[@id='example']"),
            );
            targets.text_entry(ui, &expression, "xpath/expression", "XPath expression");
            expression_edited |= expression.changed();
        });
        if self.expression.len() > MAX_XPATH_EXPRESSION_BYTES {
            truncate_utf8(&mut self.expression, MAX_XPATH_EXPRESSION_BYTES);
            self.expression_was_clipped = true;
            expression_edited = true;
        } else if expression_edited {
            self.expression_was_clipped = false;
        }
        if self.expression_was_clipped {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "XPath expressions are limited to {MAX_XPATH_EXPRESSION_BYTES} bytes; extra text was clipped."
                ),
            );
        }

        let namespaces_header = ui.collapsing("Namespace prefix mappings", |ui| {
            ui.weak(
                "XPath has no default element namespace. Add a prefix and URI, then use that prefix in the expression.",
            );
            egui::ScrollArea::vertical()
                .id_salt("xml_toolkit_xpath_namespace_rows")
                .auto_shrink([false, false])
                .max_height(150.0)
                .show(ui, |ui| {
                    for (index, binding) in self.namespaces.iter_mut().enumerate() {
                        ui.push_id(("xml_xpath_namespace", binding.id), |ui| ui.horizontal(|ui| {
                            ui.label("Prefix");
                            let prefix = ui.add(
                                    egui::TextEdit::singleline(&mut binding.prefix)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(96.0),
                                );
                            targets.text_entry(
                                ui,
                                &prefix,
                                format!("xpath/namespace/{}/prefix", binding.id),
                                "Namespace prefix",
                            );
                            namespaces_edited |= prefix.changed();
                            ui.label("URI");
                            let uri = ui.add(
                                    egui::TextEdit::singleline(&mut binding.uri)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(240.0),
                                );
                            targets.text_entry(
                                ui,
                                &uri,
                                format!("xpath/namespace/{}/uri", binding.id),
                                "Namespace URI",
                            );
                            namespaces_edited |= uri.changed();
                            let remove = ui.small_button("Remove");
                            targets.button(
                                ui,
                                &remove,
                                format!("xpath/namespace/{}/remove", binding.id),
                                "Remove namespace mapping",
                            );
                            let remove_clicked = remove.clicked();
                            if first_pass && authorized_pass && remove_clicked {
                                self.pending_namespace_action =
                                    Some(NamespaceRowAction::Remove(index));
                            }
                        }));
                    }
                });
            ui.horizontal(|ui| {
                if self.namespaces.len() < MAX_NAMESPACE_ROWS {
                    let add = ui.small_button("Add prefix mapping");
                    targets.button(
                        ui,
                        &add,
                        "xpath/namespace/add",
                        "Add namespace mapping",
                    );
                    let add_clicked = add.clicked();
                    if first_pass && authorized_pass && add_clicked {
                        self.pending_namespace_action = Some(NamespaceRowAction::Add);
                    }
                } else {
                    ui.weak(format!("At most {MAX_NAMESPACE_ROWS} mappings are supported."));
                }
            });
        });
        targets.button(
            ui,
            &namespaces_header.header_response,
            "xpath/namespaces/toggle",
            "XPath namespace mappings",
        );

        if first_pass && authorized_pass {
            match self.pending_namespace_action.take() {
                Some(NamespaceRowAction::Add) if self.namespaces.len() < MAX_NAMESPACE_ROWS => {
                    let id = self.next_namespace_row;
                    self.next_namespace_row = self.next_namespace_row.wrapping_add(1);
                    self.namespaces.push(NamespaceBindingEditorRow {
                        id,
                        ..NamespaceBindingEditorRow::default()
                    });
                    namespaces_edited = true;
                    #[cfg(test)]
                    {
                        self.namespace_action_commits += 1;
                    }
                }
                Some(NamespaceRowAction::Remove(index)) if index < self.namespaces.len() => {
                    self.namespaces.remove(index);
                    namespaces_edited = true;
                    #[cfg(test)]
                    {
                        self.namespace_action_commits += 1;
                    }
                }
                Some(NamespaceRowAction::Add | NamespaceRowAction::Remove(_)) | None => {}
            }
        }

        if cap_namespace_editor(&mut self.namespaces) {
            self.namespaces_were_clipped = true;
            namespaces_edited = true;
        }
        if self.namespaces_were_clipped {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "Namespace mappings are limited to {MAX_XPATH_NAMESPACE_BYTES} bytes; extra text was clipped."
                ),
            );
        }

        if first_pass && authorized_pass {
            if namespaces_edited {
                self.namespace_context_revision = self.namespace_context_revision.saturating_add(1);
            }
            if expression_edited || namespaces_edited {
                self.invalidate_local_query(tree);
                intents.invalidate = snapshot.revision;
            }
        }

        let namespace_map: Vec<_> = self
            .namespaces
            .iter()
            .map(|binding| (binding.prefix.clone(), binding.uri.clone()))
            .collect();
        let namespace_validation = validate_xpath_namespaces(&namespace_map);
        if let Err(XmlEngineError::XPath(message)) = &namespace_validation {
            ui.colored_label(ui.visuals().error_fg_color, message);
        }
        let document = match &snapshot.tree {
            XmlTreeStatus::Ready(projection) if snapshot.parse == XmlParseStatus::Ready => {
                Some(projection.document)
            }
            _ => None,
        };
        ui.horizontal(|ui| {
            let run = ui.add_enabled(
                document.is_some()
                    && self.expression.len() <= MAX_XPATH_EXPRESSION_BYTES
                    && namespace_validation.is_ok(),
                egui::Button::new("Run XPath"),
            );
            targets.button(
                ui,
                &run,
                format!(
                    "xpath/{:?}/{}/run",
                    snapshot.revision, self.query_view_generation
                ),
                "Run XPath",
            );
            run_clicked = run.clicked();
            match self.visible_status(snapshot) {
                VisibleXPathStatus::Pending => {
                    ui.weak("Running XPath…");
                }
                VisibleXPathStatus::Nodes(nodes) if !nodes.is_empty() => {
                    ui.label(format!("{} matches", nodes.len()));
                    let previous = ui.add_enabled(!nodes.is_empty(), egui::Button::new("Previous"));
                    targets.button(
                        ui,
                        &previous,
                        format!(
                            "xpath/{:?}/{}/previous",
                            snapshot.revision, self.query_view_generation
                        ),
                        "Previous XPath result",
                    );
                    previous_clicked = previous.clicked();
                    let next = ui.add_enabled(!nodes.is_empty(), egui::Button::new("Next"));
                    targets.button(
                        ui,
                        &next,
                        format!(
                            "xpath/{:?}/{}/next",
                            snapshot.revision, self.query_view_generation
                        ),
                        "Next XPath result",
                    );
                    next_clicked = next.clicked();
                }
                VisibleXPathStatus::Nodes(_) => {
                    ui.label("0 matches");
                    ui.add_enabled(false, egui::Button::new("Previous"));
                    ui.add_enabled(false, egui::Button::new("Next"));
                }
                VisibleXPathStatus::ScalarText(_) => {
                    ui.label("Scalar result (text)");
                }
                VisibleXPathStatus::Failed(_) => {
                    ui.label("XPath failed");
                }
                VisibleXPathStatus::Empty | VisibleXPathStatus::NotRun => {
                    ui.weak("Run the expression against the current parsed document.");
                    ui.add_enabled(false, egui::Button::new("Previous"));
                    ui.add_enabled(false, egui::Button::new("Next"));
                }
            }
        });

        match self.visible_status(snapshot) {
            VisibleXPathStatus::Empty => {
                ui.weak("No XML source is loaded.");
            }
            VisibleXPathStatus::NotRun => {}
            VisibleXPathStatus::Pending => {
                ui.weak("Waiting for the XML worker…");
            }
            VisibleXPathStatus::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("XPath error: {error:?}"),
                );
            }
            VisibleXPathStatus::ScalarText(text) => {
                ui.label("Scalar result (MSXML exposes XPath scalar values as text):");
                let (preview, truncated) = bounded_prefix(text, MAX_RESULT_VALUE_CHARS);
                ui.add(egui::Label::new(egui::RichText::new(preview).monospace()).wrap());
                if truncated {
                    ui.weak("Scalar display is clipped; explicit copy keeps the complete bounded result.");
                }
                let copy_scalar = ui.add_enabled(true, egui::Button::new("Copy scalar text"));
                targets.button(
                    ui,
                    &copy_scalar,
                    format!(
                        "xpath/{:?}/{}/copy-scalar",
                        snapshot.revision, self.query_view_generation
                    ),
                    "Copy XPath scalar",
                );
                scalar_copy_clicked = copy_scalar.clicked();
            }
            VisibleXPathStatus::Nodes(nodes) => {
                if nodes.is_empty() {
                    ui.weak("The query completed with zero matching nodes.");
                } else {
                    if previous_clicked {
                        selected_row = navigation_target(
                            nodes.len(),
                            self.selected_result,
                            NavigationDirection::Previous,
                        );
                    } else if next_clicked {
                        selected_row = navigation_target(
                            nodes.len(),
                            self.selected_result,
                            NavigationDirection::Next,
                        );
                    }
                    let selected = self.selected_result.filter(|index| *index < nodes.len());
                    let tree_projection = match &snapshot.tree {
                        XmlTreeStatus::Ready(projection) => Some(projection.as_ref()),
                        _ => None,
                    };
                    let result_viewport_height = (ui.available_height() * 0.45).clamp(100.0, 280.0);
                    let scroll_to = if first_pass && authorized_pass {
                        self.pending_result_scroll_to.take()
                    } else {
                        None
                    };
                    #[cfg(test)]
                    if scroll_to.is_some() {
                        self.result_scroll_requests += 1;
                    }
                    let mut result_scroll = egui::ScrollArea::vertical()
                        .id_salt("xml_toolkit_xpath_results")
                        .auto_shrink([false, false])
                        .max_height(result_viewport_height);
                    if let Some(index) = scroll_to {
                        result_scroll =
                            result_scroll.vertical_scroll_offset(result_row_scroll_offset(
                                index,
                                XPATH_RESULT_ROW_HEIGHT,
                                ui.spacing().item_spacing.y,
                                result_viewport_height,
                                nodes.len(),
                            ));
                    }
                    result_scroll.show_rows(
                        ui,
                        XPATH_RESULT_ROW_HEIGHT,
                        nodes.len(),
                        |ui, range| {
                            #[cfg(test)]
                            let mut rendered = 0usize;
                            for index in range {
                                #[cfg(test)]
                                {
                                    rendered += 1;
                                }
                                let node = &nodes[index];
                                let label = result_label(index, node);
                                let (response, focus_selected) = ui
                                    .push_id(
                                        ("xml_xpath_result", self.query_view_generation, node.id),
                                        |ui| {
                                            let response = ui.add_sized(
                                                egui::vec2(
                                                    ui.available_width(),
                                                    XPATH_RESULT_ROW_HEIGHT,
                                                ),
                                                egui::Button::selectable(
                                                    selected == Some(index),
                                                    egui::RichText::new(label).monospace(),
                                                )
                                                .truncate(),
                                            );
                                            let focus_selected = targets.selectable(
                                                ui,
                                                &response,
                                                format!(
                                                    "xpath/{:?}/{}/node/{:?}/select",
                                                    snapshot.revision,
                                                    self.query_view_generation,
                                                    node.id
                                                ),
                                                "XPath result node",
                                                TargetKind::Action,
                                            );
                                            (response, focus_selected)
                                        },
                                    )
                                    .inner;
                                #[cfg(test)]
                                {
                                    self.max_rendered_result_row_height = self
                                        .max_rendered_result_row_height
                                        .max(response.rect.height());
                                }
                                if focus_selected {
                                    focus_selected_row = Some(index);
                                }
                                if response.clicked() {
                                    selected_row = Some(index);
                                }
                            }
                            #[cfg(test)]
                            {
                                self.rendered_rows += rendered;
                            }
                        },
                    );
                    #[cfg(test)]
                    {
                        self.visible_rows = nodes.len();
                    }

                    if let Some((index, node)) =
                        selected.and_then(|index| nodes.get(index).map(|node| (index, node)))
                    {
                        ui.separator();
                        ui.label(format!(
                            "Selected result {}: {} {}",
                            index + 1,
                            node_kind_label(node.kind),
                            bounded_chars(&node.qualified_name, MAX_RESULT_LABEL_CHARS)
                        ));
                        let (preview, truncated) =
                            bounded_prefix(&node.value, MAX_RESULT_VALUE_CHARS);
                        if !preview.is_empty() || truncated {
                            ui.add(
                                egui::Label::new(egui::RichText::new(preview).monospace()).wrap(),
                            );
                        }
                        let projected = tree_projection
                            .is_some_and(|projection| projection.node(node.id).is_some());
                        if !projected {
                            ui.weak(
                                "This result is outside the bounded tree projection; navigation and tree-routed copy are unavailable.",
                            );
                        } else {
                            if self.navigation_unavailable {
                                ui.weak("The selected result could not be revealed in the tree.");
                            }
                            ui.push_id(
                                (
                                    "xml_xpath_selected_result",
                                    self.query_view_generation,
                                    node.id,
                                ),
                                |ui| {
                                    ui.horizontal(|ui| match node.kind {
                                        XmlTreeNodeKind::Element => {
                                            let copy_text = ui.small_button("Copy element text");
                                            targets.button(
                                                ui,
                                                &copy_text,
                                                format!(
                                                    "xpath/{:?}/{}/node/{:?}/copy-text",
                                                    snapshot.revision,
                                                    self.query_view_generation,
                                                    node.id
                                                ),
                                                "Copy XPath node text",
                                            );
                                            if copy_text.clicked() {
                                                node_copy_clicked =
                                                    Some((index, XmlNodeDetailKind::ElementText));
                                            }
                                            let copy_xml = ui.small_button("Copy XML fragment");
                                            targets.button(
                                                ui,
                                                &copy_xml,
                                                format!(
                                                    "xpath/{:?}/{}/node/{:?}/copy-xml",
                                                    snapshot.revision,
                                                    self.query_view_generation,
                                                    node.id
                                                ),
                                                "Copy XPath XML fragment",
                                            );
                                            if copy_xml.clicked() {
                                                node_copy_clicked =
                                                    Some((index, XmlNodeDetailKind::XmlFragment));
                                            }
                                        }
                                        XmlTreeNodeKind::Attribute => {
                                            let copy_attribute =
                                                ui.small_button("Copy attribute value");
                                            targets.button(
                                                ui,
                                                &copy_attribute,
                                                format!(
                                                    "xpath/{:?}/{}/node/{:?}/copy-attribute",
                                                    snapshot.revision,
                                                    self.query_view_generation,
                                                    node.id
                                                ),
                                                "Copy XPath attribute",
                                            );
                                            if copy_attribute.clicked() {
                                                node_copy_clicked = Some((
                                                    index,
                                                    XmlNodeDetailKind::AttributeValue,
                                                ));
                                            }
                                        }
                                        XmlTreeNodeKind::Text
                                        | XmlTreeNodeKind::CData
                                        | XmlTreeNodeKind::Comment
                                        | XmlTreeNodeKind::ProcessingInstruction => {
                                            let copy_value = ui.small_button("Copy node value");
                                            targets.button(
                                                ui,
                                                &copy_value,
                                                format!(
                                                    "xpath/{:?}/{}/node/{:?}/copy-value",
                                                    snapshot.revision,
                                                    self.query_view_generation,
                                                    node.id
                                                ),
                                                "Copy XPath node value",
                                            );
                                            if copy_value.clicked() {
                                                node_copy_clicked =
                                                    Some((index, XmlNodeDetailKind::TextValue));
                                            }
                                        }
                                        XmlTreeNodeKind::Document | XmlTreeNodeKind::Other => {
                                            ui.weak(
                                                "This XPath node kind has no tree copy action.",
                                            );
                                        }
                                    });
                                },
                            );
                        }
                    }
                }
            }
        }

        if first_pass && authorized_pass {
            if run_clicked {
                tree.revoke_copy_authority();
                self.active_request = None;
                self.selected_result = None;
                self.pending_result_scroll_to = None;
                self.latest_scalar_copy_request = None;
                self.navigation_unavailable = false;
                if let (Some(revision), Some(document)) = (snapshot.revision, document)
                    && let Some(request) = self.next_query_request(revision, document)
                {
                    self.query_view_generation = self.query_view_generation.wrapping_add(1);
                    self.active_request = Some(request.clone());
                    intents.query = Some(request);
                }
            }
            if let Some(index) = focus_selected_row
                && let VisibleXPathStatus::Nodes(nodes) = self.visible_status(snapshot)
                && let Some(node) = nodes.get(index)
                && let Some(selection) = tree.select_node(node.id)
            {
                // Jump Select is a focus/selection handoff only: it doesn't
                // reveal ancestors, request detail computation, or copy.
                self.selected_result = Some(index);
                self.pending_result_scroll_to = Some(index);
                intents.tree.selection = Some(selection);
            } else if let Some(index) = selected_row
                && let VisibleXPathStatus::Nodes(nodes) = self.visible_status(snapshot)
                && let Some(tree_intents) = self.navigate_to_result(index, nodes, tree)
            {
                intents.tree = tree_intents;
            }
            if let Some((index, kind)) = node_copy_clicked
                && let Some(tree_intents) = self.request_result_copy(index, kind, snapshot, tree)
            {
                intents.tree = tree_intents;
            }
            if scalar_copy_clicked {
                intents.scalar_copy = self.authorize_scalar_copy(snapshot);
            }
        }
        intents
    }
}

fn result_row_scroll_offset(
    row: usize,
    row_height: f32,
    row_spacing: f32,
    viewport_height: f32,
    total_rows: usize,
) -> f32 {
    let stride = row_height + row_spacing;
    let content_height = (stride * total_rows as f32 - row_spacing).max(0.0);
    let max_scroll = (content_height - viewport_height).max(0.0);
    let row_center = row as f32 * stride + row_height * 0.5;
    (row_center - viewport_height * 0.5).clamp(0.0, max_scroll)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_toolkit_engine::{
        XmlDocumentId, XmlNodeId, XmlTreeNodeSnapshot, XmlTreeProjection,
    };
    use crate::xml_toolkit_session::{
        XmlDetailStatus, XmlNodeDetailOutput, XmlOutputStatus, XmlSourceRevision,
    };

    fn fixture(
        number: u64,
    ) -> (
        Arc<XmlTreeProjection>,
        XmlNodeId,
        XmlNodeId,
        XmlNodeId,
        XmlNodeId,
        XmlNodeId,
    ) {
        let document = XmlDocumentId::for_test(number);
        let root = XmlNodeId::for_test(document, 1);
        let group = XmlNodeId::for_test(document, 2);
        let first = XmlNodeId::for_test(document, 3);
        let second = XmlNodeId::for_test(document, 4);
        let attribute = XmlNodeId::for_test(document, 5);
        let projection = Arc::new(XmlTreeProjection::from_nodes(
            document,
            root,
            vec![
                XmlTreeNodeSnapshot {
                    id: root,
                    parent: None,
                    kind: XmlTreeNodeKind::Document,
                    qualified_name: "#document".into(),
                    local_name: "#document".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: vec![group],
                    has_more_attributes: false,
                    has_more_children: false,
                },
                XmlTreeNodeSnapshot {
                    id: group,
                    parent: Some(root),
                    kind: XmlTreeNodeKind::Element,
                    qualified_name: "group".into(),
                    local_name: "group".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: vec![first, second],
                    has_more_attributes: false,
                    has_more_children: false,
                },
                XmlTreeNodeSnapshot {
                    id: first,
                    parent: Some(group),
                    kind: XmlTreeNodeKind::Element,
                    qualified_name: "item".into(),
                    local_name: "item".into(),
                    prefix: String::new(),
                    namespace_uri: "urn:items".into(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                },
                XmlTreeNodeSnapshot {
                    id: second,
                    parent: Some(group),
                    kind: XmlTreeNodeKind::Element,
                    qualified_name: "item".into(),
                    local_name: "item".into(),
                    prefix: String::new(),
                    namespace_uri: "urn:items".into(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: vec![attribute],
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                },
                XmlTreeNodeSnapshot {
                    id: attribute,
                    parent: Some(second),
                    kind: XmlTreeNodeKind::Attribute,
                    qualified_name: "id".into(),
                    local_name: "id".into(),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: "second".into(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                },
            ],
            None,
        ));
        (projection, root, group, first, second, attribute)
    }

    fn make_request(
        revision: XmlSourceRevision,
        document: XmlDocumentId,
        request_id: u64,
        expression: &str,
        namespaces: Vec<(String, String)>,
    ) -> XmlXPathQueryRequest {
        XmlXPathQueryRequest {
            revision,
            document,
            request: XmlXPathRequestId::from_counter(request_id),
            expression: expression.into(),
            namespace_context_revision: XmlNamespaceContextRevision::from_counter(0),
            namespaces,
        }
    }

    fn scalar_snapshot(
        revision: XmlSourceRevision,
        projection: Arc<XmlTreeProjection>,
        request: XmlXPathQueryRequest,
        value: &str,
    ) -> XmlSessionSnapshot {
        XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(projection),
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::ScalarText {
                request,
                text: Arc::from(value),
            },
            schema: Default::default(),
        }
    }

    #[test]
    fn xpath_model_namespaces_rerun_and_full_scalar_are_current() {
        let revision = XmlSourceRevision::for_test(12);
        let (projection, _, _, _, _, _) = fixture(12);
        let seed = make_request(revision, projection.document, 99, "//p:item", Vec::new());
        let base = scalar_snapshot(revision, Arc::clone(&projection), seed, "old");
        let mut tree = XmlToolkitTreeViewState::default();
        tree.sync(&base);
        let mut view = XmlToolkitXPathViewState::default();
        view.sync(&base);
        assert!(view.edit_expression("//p:item".into(), &mut tree));
        let id = view.add_namespace(&mut tree).unwrap();
        assert!(view.edit_namespace(id, "p".into(), "urn:items".into(), &mut tree));
        let duplicate = view.add_namespace(&mut tree).unwrap();
        assert!(view.edit_namespace(duplicate, "p".into(), "urn:other".into(), &mut tree));
        assert!(view.namespace_error().is_some());
        assert!(view.remove_namespace(duplicate, &mut tree));
        assert!(view.namespace_error().is_none());
        let first = view.run(&base, &mut tree).unwrap();
        let second = view.run(&base, &mut tree).unwrap();
        assert_ne!(first.request, second.request);
        let stale = scalar_snapshot(revision, Arc::clone(&projection), first, "stale");
        assert!(matches!(view.visible_status(&stale), VisibleXPathStatus::NotRun));
        let full = "é🦀".repeat(1_000);
        let current = scalar_snapshot(revision, projection, second, &full);
        assert!(view.authorize_scalar_copy(&current).is_some());
        assert_eq!(view.take_valid_scalar_copy_completion(&current).as_deref(), Some(full.as_str()));
        assert!(view.take_valid_scalar_copy_completion(&current).is_none());
        assert!(view.remove_namespace(id, &mut tree));
        assert!(matches!(view.visible_status(&current), VisibleXPathStatus::NotRun));
    }

    fn run_view_frame(
        context: &egui::Context,
        view: &mut XmlToolkitXPathViewState,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
        size: egui::Vec2,
    ) {
        run_view_frame_with_discard(context, view, snapshot, tree, size, false);
    }

    fn run_view_frame_with_discard(
        context: &egui::Context,
        view: &mut XmlToolkitXPathViewState,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
        size: egui::Vec2,
        discard_first_pass: bool,
    ) {
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = view.show(ui, snapshot, tree);
                    if discard_first_pass && ui.ctx().current_pass_index() == 0 {
                        ui.ctx().request_discard("test namespace row action");
                    }
                });
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn m7d_namespace_row_actions_commit_once_when_egui_discards_a_pass() {
        let revision = XmlSourceRevision::for_test(6);
        let (projection, ..) = fixture(6);
        let snapshot = XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(projection),
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::NotRun,
            schema: Default::default(),
        };
        let mut view = XmlToolkitXPathViewState {
            revision: Some(revision),
            pending_namespace_action: Some(NamespaceRowAction::Add),
            ..Default::default()
        };
        let mut tree = XmlToolkitTreeViewState::default();
        let context = egui::Context::default();
        run_view_frame_with_discard(
            &context,
            &mut view,
            &snapshot,
            &mut tree,
            egui::vec2(500.0, 600.0),
            true,
        );
        assert_eq!(view.namespaces.len(), 1);
        assert_eq!(view.namespace_action_commits, 1);

        view.pending_namespace_action = Some(NamespaceRowAction::Remove(0));
        run_view_frame_with_discard(
            &context,
            &mut view,
            &snapshot,
            &mut tree,
            egui::vec2(500.0, 600.0),
            true,
        );
        assert!(view.namespaces.is_empty());
        assert_eq!(view.namespace_action_commits, 2);
    }

    #[test]
    fn m7d_navigation_reveals_the_exact_projected_attribute_owner() {
        let revision = XmlSourceRevision::for_test(7);
        let (projection, _, group, first, second, attribute) = fixture(7);
        let request = make_request(
            revision,
            projection.document,
            1,
            "//p:item/@id",
            vec![("p".into(), "urn:items".into())],
        );
        let queried_attribute = XmlNodeSnapshot {
            id: attribute,
            kind: XmlTreeNodeKind::Attribute,
            qualified_name: "id".into(),
            value: "second".into(),
        };
        let snapshot = XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(Arc::clone(&projection)),
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::Nodes {
                request: request.clone(),
                nodes: Arc::new(vec![queried_attribute.clone()]),
            },
            schema: Default::default(),
        };
        let mut view = XmlToolkitXPathViewState {
            expression: request.expression.clone(),
            namespace_context_revision: 0,
            revision: Some(revision),
            active_request: Some(request.clone()),
            ..Default::default()
        };
        let mut tree = XmlToolkitTreeViewState::default();
        tree.sync(&snapshot);

        assert_ne!(first, second);
        let intents = view
            .navigate_to_result(0, std::slice::from_ref(&queried_attribute), &mut tree)
            .expect("the projected attribute can be revealed");
        assert_eq!(
            tree.selected().map(|selected| selected.node),
            Some(attribute)
        );
        assert!(tree.is_expanded_for_test(group));
        assert!(tree.is_expanded_for_test(second));
        assert_eq!(tree.pending_scroll_for_test(), Some(attribute));
        assert_eq!(intents.selection.unwrap().node, attribute);
        assert!(matches!(
            intents.detail,
            Some(request)
                if request.node == attribute
                    && request.kind == XmlNodeDetailKind::AttributeValue
                    && request.intent == crate::xml_toolkit_session::XmlDetailIntent::Inspect
        ));
        assert!(view.current_selected_node(&snapshot, 0, attribute, &request));
        assert!(!view.current_selected_node(&snapshot, 1, attribute, &request));
        assert!(!view.current_selected_node(&snapshot, 0, first, &request));
        let stale = make_request(revision, projection.document, 9, "//p:item/@id", vec![("p".into(), "urn:items".into())]);
        assert!(!view.current_selected_node(&snapshot, 0, attribute, &stale));
        tree.select_node(first).unwrap();
        assert!(view.current_selected_node(&snapshot, 0, attribute, &request));
        view.navigate(0, &snapshot, &mut tree).unwrap();
        assert_eq!(tree.selected().unwrap().node, attribute);
    }

    #[test]
    fn m7d_copy_authority_is_explicit_current_and_scalar_values_stay_text() {
        let revision = XmlSourceRevision::for_test(8);
        let (projection, _, _, _, _, attribute) = fixture(8);
        let request = make_request(revision, projection.document, 3, "false()", vec![]);
        let snapshot = scalar_snapshot(revision, Arc::clone(&projection), request.clone(), "false");
        let mut view = XmlToolkitXPathViewState {
            expression: request.expression.clone(),
            revision: Some(revision),
            active_request: Some(request.clone()),
            ..Default::default()
        };
        assert!(view.take_valid_scalar_copy_completion(&snapshot).is_none());
        assert_eq!(view.authorize_scalar_copy(&snapshot), Some(request.clone()));
        assert_eq!(
            view.take_valid_scalar_copy_completion(&snapshot).as_deref(),
            Some("false")
        );
        assert!(view.take_valid_scalar_copy_completion(&snapshot).is_none());

        let mut stale_view = XmlToolkitXPathViewState {
            expression: request.expression.clone(),
            revision: Some(revision),
            active_request: Some(request.clone()),
            latest_scalar_copy_request: Some(request.clone()),
            ..Default::default()
        };
        stale_view.expression.push('x');
        assert!(
            stale_view
                .take_valid_scalar_copy_completion(&snapshot)
                .is_none()
        );

        let node_request = make_request(revision, projection.document, 4, "//@id", vec![]);
        let node_snapshot = XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(Arc::clone(&projection)),
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::Nodes {
                request: node_request.clone(),
                nodes: Arc::new(vec![XmlNodeSnapshot {
                    id: attribute,
                    kind: XmlTreeNodeKind::Attribute,
                    qualified_name: "id".into(),
                    value: "second".into(),
                }]),
            },
            schema: Default::default(),
        };
        let node_view = XmlToolkitXPathViewState {
            expression: node_request.expression.clone(),
            revision: Some(revision),
            active_request: Some(node_request),
            selected_result: Some(0),
            ..Default::default()
        };
        let mut tree = XmlToolkitTreeViewState::default();
        tree.sync(&node_snapshot);
        let copy = node_view
            .request_result_copy(
                0,
                XmlNodeDetailKind::AttributeValue,
                &node_snapshot,
                &mut tree,
            )
            .expect("explicit query-result copy uses the guarded tree path");
        let copy_request = copy.detail.expect("copy routes through async detail");
        assert_eq!(copy_request.node, attribute);
        assert_eq!(
            copy_request.intent,
            crate::xml_toolkit_session::XmlDetailIntent::Copy
        );
        let mut completed_snapshot = node_snapshot.clone();
        completed_snapshot.detail = XmlDetailStatus::Ready(Arc::new(XmlNodeDetailOutput {
            request: copy_request,
            value: "second".into(),
        }));
        let copied = tree
            .take_valid_copy_completion(&completed_snapshot)
            .expect("only the current explicit copy completion is available");
        assert_eq!(copied.value, "second");
    }

    #[test]
    fn m7d_navigation_wraps_only_node_results_and_result_rows_are_virtualized() {
        assert_eq!(
            navigation_target(3, None, NavigationDirection::Next),
            Some(0)
        );
        assert_eq!(
            navigation_target(3, None, NavigationDirection::Previous),
            Some(2)
        );
        assert_eq!(
            navigation_target(3, Some(2), NavigationDirection::Next),
            Some(0)
        );
        assert_eq!(
            navigation_target(3, Some(0), NavigationDirection::Previous),
            Some(2)
        );
        assert_eq!(navigation_target(0, None, NavigationDirection::Next), None);

        let revision = XmlSourceRevision::for_test(9);
        let (projection, ..) = fixture(9);
        let request = make_request(revision, projection.document, 9, "//p:item", vec![]);
        let mut nodes = Vec::new();
        for index in 0..2_000 {
            nodes.push(XmlNodeSnapshot {
                id: XmlNodeId::for_test(projection.document, 10_000 + index),
                kind: XmlTreeNodeKind::Element,
                qualified_name: if index == 0 {
                    "long-name".repeat(64)
                } else {
                    format!("item-{index}")
                },
                value: if index == 0 {
                    "<&value ".repeat(64)
                } else {
                    "value".into()
                },
            });
        }
        let scalar_projection = Arc::clone(&projection);
        let scalar_document = projection.document;
        let snapshot = XmlSessionSnapshot {
            revision: Some(revision),
            parse: XmlParseStatus::Ready,
            output: XmlOutputStatus::Empty,
            tree: XmlTreeStatus::Ready(projection),
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::Nodes {
                request: request.clone(),
                nodes: Arc::new(nodes),
            },
            schema: Default::default(),
        };
        let mut view = XmlToolkitXPathViewState {
            expression: request.expression.clone(),
            revision: Some(revision),
            active_request: Some(request),
            selected_result: Some(0),
            ..Default::default()
        };
        let mut tree = XmlToolkitTreeViewState::default();
        let context = egui::Context::default();
        run_view_frame(
            &context,
            &mut view,
            &snapshot,
            &mut tree,
            egui::vec2(240.0, 600.0),
        );
        assert_eq!(view.visible_rows, 2_000);
        assert!(view.rendered_rows > 0);
        assert!(view.rendered_rows < view.visible_rows);
        assert!(view.max_rendered_result_row_height <= XPATH_RESULT_ROW_HEIGHT);
        assert_eq!(view.result_scroll_requests, 0);
        view.pending_result_scroll_to = Some(1_999);
        run_view_frame(
            &context,
            &mut view,
            &snapshot,
            &mut tree,
            egui::vec2(240.0, 600.0),
        );
        assert!(view.pending_result_scroll_to.is_none());
        assert_eq!(view.result_scroll_requests, 1);
        run_view_frame(
            &context,
            &mut view,
            &snapshot,
            &mut tree,
            egui::vec2(240.0, 600.0),
        );
        assert_eq!(view.result_scroll_requests, 1);

        let scalar_request = make_request(revision, scalar_document, 10, "false()", vec![]);
        let scalar = scalar_snapshot(revision, scalar_projection, scalar_request.clone(), "false");
        let mut scalar_view = XmlToolkitXPathViewState {
            expression: scalar_request.expression.clone(),
            revision: Some(revision),
            active_request: Some(scalar_request),
            ..Default::default()
        };
        let mut scalar_tree = XmlToolkitTreeViewState::default();
        let no_copy = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(240.0, 600.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let intents = scalar_view.show(ui, &scalar, &mut scalar_tree);
                    assert!(intents.scalar_copy.is_none());
                });
            },
        );
        assert!(
            scalar_view
                .take_valid_scalar_copy_completion(&scalar)
                .is_none()
        );
        no_copy.drop_without_applying_deltas();
    }
}
