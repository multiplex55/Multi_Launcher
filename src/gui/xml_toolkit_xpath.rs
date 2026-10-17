//! XPath query controls and result navigation for the XML toolkit.
//!
//! The view only edits query state and returns bounded requests/intents. The
//! session executes XPath on its existing XML worker; result navigation is
//! routed through the stable node identities owned by the XML tree view.

use crate::gui::xml_toolkit_tree::{XmlToolkitTreeIntents, XmlToolkitTreeViewState};
use crate::xml_toolkit_engine::{
    MAX_XPATH_EXPRESSION_BYTES, MAX_XPATH_NAMESPACE_BYTES, XmlEngineError, XmlNodeDetailKind,
    XmlNodeSnapshot, XmlTreeNodeKind, validate_xpath_namespaces,
};
use crate::xml_toolkit_session::{
    XmlNamespaceContextRevision, XmlParseStatus, XmlSessionSnapshot, XmlSourceRevision,
    XmlTreeStatus, XmlXPathQueryRequest, XmlXPathRequestId, XmlXPathStatus,
};
use eframe::egui;
use std::sync::Arc;

const MAX_NAMESPACE_ROWS: usize = 64;
const MAX_RESULT_LABEL_CHARS: usize = 192;
const MAX_RESULT_VALUE_CHARS: usize = 240;
const XPATH_RESULT_ROW_HEIGHT: f32 = 22.0;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NamespaceBindingEditorRow {
    prefix: String,
    uri: String,
}

#[derive(Clone, Copy, Debug)]
enum NamespaceRowAction {
    Add,
    Remove(usize),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct XmlToolkitXPathIntents {
    /// Apply this first when both invalidation and a new request are returned.
    pub invalidate: Option<XmlSourceRevision>,
    pub query: Option<XmlXPathQueryRequest>,
    pub tree: XmlToolkitTreeIntents,
    /// A scalar copy is separately authorized and completed through
    /// `take_valid_scalar_copy_completion` by the caller.
    pub scalar_copy: Option<XmlXPathQueryRequest>,
}

#[derive(Debug, Default)]
pub(crate) struct XmlToolkitXPathViewState {
    expression: String,
    namespaces: Vec<NamespaceBindingEditorRow>,
    pending_namespace_action: Option<NamespaceRowAction>,
    namespace_context_revision: u64,
    next_request: u64,
    revision: Option<XmlSourceRevision>,
    active_request: Option<XmlXPathQueryRequest>,
    selected_result: Option<usize>,
    pending_result_scroll_to: Option<usize>,
    latest_scalar_copy_request: Option<XmlXPathQueryRequest>,
    navigation_unavailable: bool,
    expression_was_clipped: bool,
    namespaces_were_clipped: bool,
    #[cfg(test)]
    visible_rows: usize,
    #[cfg(test)]
    rendered_rows: usize,
    #[cfg(test)]
    max_rendered_result_row_height: f32,
    #[cfg(test)]
    result_scroll_requests: usize,
    #[cfg(test)]
    namespace_action_commits: usize,
}

impl XmlToolkitXPathViewState {
    /// Drop local navigation/copy authority as soon as the accepted source
    /// revision changes, even before a new parse completes.
    pub(crate) fn sync(&mut self, snapshot: &XmlSessionSnapshot) {
        if self.revision != snapshot.revision {
            self.revision = snapshot.revision;
            self.active_request = None;
            self.selected_result = None;
            self.pending_result_scroll_to = None;
            self.latest_scalar_copy_request = None;
            self.navigation_unavailable = false;
        }
    }

    /// Completes only the latest explicit scalar-copy intent while its query,
    /// document, and source revision still match the displayed result.
    pub(crate) fn take_valid_scalar_copy_completion(
        &mut self,
        snapshot: &XmlSessionSnapshot,
    ) -> Option<Arc<str>> {
        let request = self.latest_scalar_copy_request.clone()?;
        if !self.is_current_request(snapshot, &request) {
            self.latest_scalar_copy_request = None;
            return None;
        }
        match &snapshot.xpath {
            XmlXPathStatus::ScalarText {
                request: completed,
                text,
            } if completed == &request => {
                self.latest_scalar_copy_request = None;
                Some(Arc::clone(text))
            }
            _ => None,
        }
    }

    fn is_current_request(
        &self,
        snapshot: &XmlSessionSnapshot,
        request: &XmlXPathQueryRequest,
    ) -> bool {
        self.active_request.as_ref() == Some(request)
            && self.revision == Some(request.revision)
            && snapshot.revision == Some(request.revision)
            && request.expression == self.expression
            && request.namespace_context_revision
                == XmlNamespaceContextRevision::from_counter(self.namespace_context_revision)
    }

    fn visible_status<'a>(&self, snapshot: &'a XmlSessionSnapshot) -> VisibleXPathStatus<'a> {
        if self.revision != snapshot.revision {
            return VisibleXPathStatus::NotRun;
        }
        match &snapshot.xpath {
            XmlXPathStatus::Empty => VisibleXPathStatus::Empty,
            XmlXPathStatus::Pending(request) if self.is_current_request(snapshot, request) => {
                VisibleXPathStatus::Pending
            }
            XmlXPathStatus::Nodes { request, nodes }
                if self.is_current_request(snapshot, request) =>
            {
                VisibleXPathStatus::Nodes(nodes)
            }
            XmlXPathStatus::ScalarText { request, text }
                if self.is_current_request(snapshot, request) =>
            {
                VisibleXPathStatus::ScalarText(text)
            }
            XmlXPathStatus::Failed { request, error }
                if self.is_current_request(snapshot, request) =>
            {
                VisibleXPathStatus::Failed(error)
            }
            XmlXPathStatus::NotRun
            | XmlXPathStatus::Pending(_)
            | XmlXPathStatus::Nodes { .. }
            | XmlXPathStatus::ScalarText { .. }
            | XmlXPathStatus::Failed { .. } => VisibleXPathStatus::NotRun,
        }
    }

    fn invalidate_local_query(&mut self, tree: &mut XmlToolkitTreeViewState) {
        self.active_request = None;
        self.selected_result = None;
        self.pending_result_scroll_to = None;
        self.latest_scalar_copy_request = None;
        self.navigation_unavailable = false;
        tree.revoke_copy_authority();
    }

    fn navigate_to_result(
        &mut self,
        index: usize,
        nodes: &[XmlNodeSnapshot],
        tree: &mut XmlToolkitTreeViewState,
    ) -> Option<XmlToolkitTreeIntents> {
        let node = nodes.get(index)?;
        self.selected_result = Some(index);
        self.pending_result_scroll_to = Some(index);
        match tree.reveal_node(node.id) {
            Some(intents) => {
                self.navigation_unavailable = false;
                Some(intents)
            }
            None => {
                self.navigation_unavailable = true;
                None
            }
        }
    }

    fn request_result_copy(
        &self,
        index: usize,
        kind: XmlNodeDetailKind,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
    ) -> Option<XmlToolkitTreeIntents> {
        let VisibleXPathStatus::Nodes(nodes) = self.visible_status(snapshot) else {
            return None;
        };
        if self.selected_result != Some(index) {
            return None;
        }
        let node = nodes.get(index)?;
        if tree.projection()?.node(node.id).is_none() {
            return None;
        }
        tree.request_node_copy(node.id, kind)
    }

    fn authorize_scalar_copy(
        &mut self,
        snapshot: &XmlSessionSnapshot,
    ) -> Option<XmlXPathQueryRequest> {
        let request = self.active_request.as_ref()?.clone();
        if !self.is_current_request(snapshot, &request)
            || !matches!(
                self.visible_status(snapshot),
                VisibleXPathStatus::ScalarText(_)
            )
        {
            return None;
        }
        self.latest_scalar_copy_request = Some(request.clone());
        Some(request)
    }

    fn next_query_request(
        &mut self,
        revision: XmlSourceRevision,
        document: crate::xml_toolkit_engine::XmlDocumentId,
    ) -> Option<XmlXPathQueryRequest> {
        let request = self.next_request;
        self.next_request = self.next_request.checked_add(1)?;
        Some(XmlXPathQueryRequest {
            revision,
            document,
            request: XmlXPathRequestId::from_counter(request),
            expression: self.expression.clone(),
            namespace_context_revision: XmlNamespaceContextRevision::from_counter(
                self.namespace_context_revision,
            ),
            namespaces: self
                .namespaces
                .iter()
                .map(|binding| (binding.prefix.clone(), binding.uri.clone()))
                .collect(),
        })
    }

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
        let mut scalar_copy_clicked = false;
        let mut node_copy_clicked = None;

        ui.heading("XPath 1.0");
        ui.horizontal(|ui| {
            ui.label("Expression");
            expression_edited |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.expression)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .hint_text("//item[@id='example']"),
                )
                .changed();
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

        ui.collapsing("Namespace prefix mappings", |ui| {
            ui.weak(
                "XPath has no default element namespace. Add a prefix and URI, then use that prefix in the expression.",
            );
            egui::ScrollArea::vertical()
                .id_salt("xml_toolkit_xpath_namespace_rows")
                .auto_shrink([false, false])
                .max_height(150.0)
                .show(ui, |ui| {
                    for (index, binding) in self.namespaces.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label("Prefix");
                            namespaces_edited |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut binding.prefix)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(96.0),
                                )
                                .changed();
                            ui.label("URI");
                            namespaces_edited |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut binding.uri)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(240.0),
                                )
                                .changed();
                            let remove_clicked = ui.small_button("Remove").clicked();
                            if first_pass && remove_clicked {
                                self.pending_namespace_action =
                                    Some(NamespaceRowAction::Remove(index));
                            }
                        });
                    }
                });
            ui.horizontal(|ui| {
                if self.namespaces.len() < MAX_NAMESPACE_ROWS {
                    let add_clicked = ui.small_button("Add prefix mapping").clicked();
                    if first_pass && add_clicked {
                        self.pending_namespace_action = Some(NamespaceRowAction::Add);
                    }
                } else {
                    ui.weak(format!("At most {MAX_NAMESPACE_ROWS} mappings are supported."));
                }
            });
        });

        if first_pass {
            match self.pending_namespace_action.take() {
                Some(NamespaceRowAction::Add) if self.namespaces.len() < MAX_NAMESPACE_ROWS => {
                    self.namespaces.push(NamespaceBindingEditorRow::default());
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

        if first_pass {
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
            run_clicked = ui
                .add_enabled(
                    document.is_some()
                        && self.expression.len() <= MAX_XPATH_EXPRESSION_BYTES
                        && namespace_validation.is_ok(),
                    egui::Button::new("Run XPath"),
                )
                .clicked();
            match self.visible_status(snapshot) {
                VisibleXPathStatus::Pending => {
                    ui.weak("Running XPath…");
                }
                VisibleXPathStatus::Nodes(nodes) if !nodes.is_empty() => {
                    ui.label(format!("{} matches", nodes.len()));
                    previous_clicked = ui
                        .add_enabled(!nodes.is_empty(), egui::Button::new("Previous"))
                        .clicked();
                    next_clicked = ui
                        .add_enabled(!nodes.is_empty(), egui::Button::new("Next"))
                        .clicked();
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
                scalar_copy_clicked = ui
                    .add_enabled(true, egui::Button::new("Copy scalar text"))
                    .clicked();
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
                    let scroll_to = if first_pass {
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
                                let response = ui.add_sized(
                                    egui::vec2(ui.available_width(), XPATH_RESULT_ROW_HEIGHT),
                                    egui::Button::selectable(
                                        selected == Some(index),
                                        egui::RichText::new(label).monospace(),
                                    )
                                    .truncate(),
                                );
                                #[cfg(test)]
                                {
                                    self.max_rendered_result_row_height = self
                                        .max_rendered_result_row_height
                                        .max(response.rect.height());
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
                            ui.horizontal(|ui| match node.kind {
                                XmlTreeNodeKind::Element => {
                                    if ui.small_button("Copy element text").clicked() {
                                        node_copy_clicked =
                                            Some((index, XmlNodeDetailKind::ElementText));
                                    }
                                    if ui.small_button("Copy XML fragment").clicked() {
                                        node_copy_clicked =
                                            Some((index, XmlNodeDetailKind::XmlFragment));
                                    }
                                }
                                XmlTreeNodeKind::Attribute => {
                                    if ui.small_button("Copy attribute value").clicked() {
                                        node_copy_clicked =
                                            Some((index, XmlNodeDetailKind::AttributeValue));
                                    }
                                }
                                XmlTreeNodeKind::Text
                                | XmlTreeNodeKind::CData
                                | XmlTreeNodeKind::Comment
                                | XmlTreeNodeKind::ProcessingInstruction => {
                                    if ui.small_button("Copy node value").clicked() {
                                        node_copy_clicked =
                                            Some((index, XmlNodeDetailKind::TextValue));
                                    }
                                }
                                XmlTreeNodeKind::Document | XmlTreeNodeKind::Other => {
                                    ui.weak("This XPath node kind has no tree copy action.");
                                }
                            });
                        }
                    }
                }
            }
        }

        if first_pass {
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
                    self.active_request = Some(request.clone());
                    intents.query = Some(request);
                }
            }
            if let Some(index) = selected_row
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

enum VisibleXPathStatus<'a> {
    Empty,
    NotRun,
    Pending,
    Nodes(&'a [XmlNodeSnapshot]),
    ScalarText(&'a str),
    Failed(&'a XmlEngineError),
}

#[derive(Clone, Copy)]
enum NavigationDirection {
    Previous,
    Next,
}

fn navigation_target(
    count: usize,
    current: Option<usize>,
    direction: NavigationDirection,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, direction) {
        (None, NavigationDirection::Previous) => count - 1,
        (None, NavigationDirection::Next) => 0,
        (Some(index), NavigationDirection::Previous) => (index % count + count - 1) % count,
        (Some(index), NavigationDirection::Next) => (index + 1) % count,
    })
}

fn result_label(index: usize, node: &XmlNodeSnapshot) -> String {
    let mut label = format!(
        "{:>5}. {} {}",
        index + 1,
        node_kind_label(node.kind),
        bounded_chars(&node.qualified_name, MAX_RESULT_LABEL_CHARS)
    );
    if !node.value.is_empty() {
        label.push_str("  ");
        label.push_str(&xml_escaped_display(bounded_chars(
            &node.value,
            MAX_RESULT_VALUE_CHARS,
        )));
    }
    label
}

fn node_kind_label(kind: XmlTreeNodeKind) -> &'static str {
    match kind {
        XmlTreeNodeKind::Document => "Document",
        XmlTreeNodeKind::Element => "Element",
        XmlTreeNodeKind::Attribute => "Attribute",
        XmlTreeNodeKind::Text => "Text",
        XmlTreeNodeKind::CData => "CDATA",
        XmlTreeNodeKind::Comment => "Comment",
        XmlTreeNodeKind::ProcessingInstruction => "Processing instruction",
        XmlTreeNodeKind::Other => "Other",
    }
}

fn bounded_chars(value: &str, max_chars: usize) -> &str {
    value
        .char_indices()
        .nth(max_chars)
        .map_or(value, |(index, _)| &value[..index])
}

fn bounded_prefix(value: &str, max_chars: usize) -> (&str, bool) {
    let mut chars = value.char_indices();
    if let Some((index, _)) = chars.nth(max_chars) {
        (&value[..index], true)
    } else {
        (value, false)
    }
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn cap_namespace_editor(rows: &mut [NamespaceBindingEditorRow]) -> bool {
    let mut remaining = MAX_XPATH_NAMESPACE_BYTES.saturating_sub(rows.len().saturating_mul(10));
    let mut clipped = false;
    for row in rows {
        let prefix_len = row.prefix.len();
        truncate_utf8(&mut row.prefix, remaining);
        clipped |= row.prefix.len() != prefix_len;
        remaining = remaining.saturating_sub(row.prefix.len());

        let uri_len = row.uri.len();
        truncate_utf8(&mut row.uri, remaining);
        clipped |= row.uri.len() != uri_len;
        remaining = remaining.saturating_sub(row.uri.len());
    }
    clipped
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

fn xml_escaped_display(value: &str) -> String {
    let mut display = String::new();
    for ch in value.chars() {
        match ch {
            '&' => display.push_str("&amp;"),
            '<' => display.push_str("&lt;"),
            '>' => display.push_str("&gt;"),
            '"' => display.push_str("&quot;"),
            '\'' => display.push_str("&apos;"),
            '\n' => display.push_str("\\n"),
            '\r' => display.push_str("\\r"),
            '\t' => display.push_str("\\t"),
            ch if ch.is_control() => display.push('�'),
            ch => display.push(ch),
        }
    }
    display
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
            active_request: Some(request),
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
