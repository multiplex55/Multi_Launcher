//! Read-only, virtualized presentation state for the accepted XML document.
//!
//! This module never owns or walks an MSXML object. It consumes the immutable
//! projection and routes explicit selection/detail intents back to the async
//! XML session.

use crate::gui::jump_mode::TargetKind;
use crate::gui::xml_toolkit_dialog::XmlJumpTargets;
use crate::xml_toolkit_engine::{
    MAX_XML_TREE_PREVIEW_CHARS, XmlNodeDetailKind, XmlNodeId, XmlTreeNodeKind, XmlTreeNodeSnapshot,
    XmlTreeProjection, XmlTreeProjectionLimit,
};
use crate::xml_toolkit_session::{
    XmlDetailIntent, XmlDetailRequestId, XmlDetailStatus, XmlNodeDetailOutput,
    XmlNodeDetailRequest, XmlNodeSelection, XmlSelectionRequestId, XmlSessionSnapshot,
    XmlSourceRevision, XmlTreeStatus,
};
use eframe::egui;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_TREE_LABEL_CHARS: usize = 256;
const MAX_DETAIL_DISPLAY_CHARS: usize = 4096;
const TREE_ROW_HEIGHT: f32 = 20.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TreeRowIdentity {
    Node(XmlNodeId),
    MoreAttributes(XmlNodeId),
    MoreChildren(XmlNodeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VisibleTreeRow {
    identity: TreeRowIdentity,
    node: Option<XmlNodeId>,
    depth: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct XmlToolkitTreeIntents {
    pub selection: Option<XmlNodeSelection>,
    pub detail: Option<XmlNodeDetailRequest>,
}

/// Cached view-only state. A source revision or parsed document change drops
/// selection, expansion, visible rows, and pending copy authority together.
#[derive(Clone, Debug, Default)]
pub(crate) struct XmlToolkitTreeViewState {
    revision: Option<XmlSourceRevision>,
    projection: Option<Arc<XmlTreeProjection>>,
    selected: Option<XmlNodeSelection>,
    expanded: HashSet<XmlNodeId>,
    visible_rows: Vec<VisibleTreeRow>,
    next_selection_request: u64,
    next_detail_request: u64,
    latest_detail_request: Option<XmlNodeDetailRequest>,
    latest_copy_request: Option<XmlNodeDetailRequest>,
    pending_scroll_to: Option<XmlNodeId>,
    #[cfg(test)]
    visible_rows_rebuilds: usize,
    #[cfg(test)]
    rendered_rows: usize,
}

impl XmlToolkitTreeViewState {
    /// Synchronize the cached tree to the session's accepted source. This is
    /// cheap and idempotent during ordinary egui frames.
    pub(crate) fn sync(&mut self, snapshot: &XmlSessionSnapshot) {
        if self.revision != snapshot.revision {
            self.clear_document_state();
            self.revision = snapshot.revision;
        }

        let projection = match &snapshot.tree {
            XmlTreeStatus::Ready(projection) => Some(Arc::clone(projection)),
            XmlTreeStatus::Empty | XmlTreeStatus::Pending | XmlTreeStatus::Failed(_) => None,
        };
        let same_projection = self
            .projection
            .as_ref()
            .zip(projection.as_ref())
            .is_some_and(|(current, incoming)| {
                current.document == incoming.document && Arc::ptr_eq(current, incoming)
            });
        if same_projection {
            return;
        }

        let same_document = self
            .projection
            .as_ref()
            .zip(projection.as_ref())
            .is_some_and(|(current, incoming)| current.document == incoming.document);
        if !same_document {
            self.clear_document_state();
        }
        self.projection = projection;
        if let Some(projection) = &self.projection {
            self.expanded.insert(projection.root);
            self.rebuild_visible_rows();
        } else {
            self.visible_rows.clear();
            self.selected = None;
            self.latest_detail_request = None;
            self.latest_copy_request = None;
        }
    }

    fn clear_document_state(&mut self) {
        self.projection = None;
        self.selected = None;
        self.expanded.clear();
        self.visible_rows.clear();
        self.latest_detail_request = None;
        self.latest_copy_request = None;
        self.pending_scroll_to = None;
    }

    pub(crate) fn projection(&self) -> Option<&Arc<XmlTreeProjection>> {
        self.projection.as_ref()
    }

    pub(crate) fn selected(&self) -> Option<XmlNodeSelection> {
        self.selected
    }

    /// Revoke any pending or ready copy authorization while keeping the
    /// current tree selection and expansion state intact.
    pub(crate) fn revoke_copy_authority(&mut self) {
        self.latest_copy_request = None;
        if self
            .latest_detail_request
            .is_some_and(|request| request.intent == XmlDetailIntent::Copy)
        {
            self.latest_detail_request = None;
        }
    }

    /// Select a projected node, open its ancestors, and request scrolling to
    /// its stable identity. Returns `None` for nodes outside the bounded tree
    /// projection so callers can present an honest navigation refusal.
    pub(crate) fn reveal_node(&mut self, node: XmlNodeId) -> Option<XmlToolkitTreeIntents> {
        self.prepare_reveal(node)?;
        let selection = self.select_node(node)?;
        let detail = default_detail_kind(
            self.projection
                .as_ref()?
                .node(node)
                .map(|snapshot| snapshot.kind),
        )
        .and_then(|kind| self.request_detail(kind, XmlDetailIntent::Inspect));
        Some(XmlToolkitTreeIntents {
            selection: Some(selection),
            detail,
        })
    }

    /// Route an explicit query-result copy through the same selection and
    /// detail guard used by the tree's own copy buttons.
    pub(crate) fn request_node_copy(
        &mut self,
        node: XmlNodeId,
        kind: XmlNodeDetailKind,
    ) -> Option<XmlToolkitTreeIntents> {
        self.prepare_reveal(node)?;
        let selection = self.select_node(node)?;
        let detail = self.request_detail(kind, XmlDetailIntent::Copy)?;
        Some(XmlToolkitTreeIntents {
            selection: Some(selection),
            detail: Some(detail),
        })
    }

    fn prepare_reveal(&mut self, node: XmlNodeId) -> Option<()> {
        self.revoke_copy_authority();
        let projection = self.projection.as_ref()?;
        projection.node(node)?;
        let mut ancestor = Some(node);
        let mut opened = false;
        let mut traversed = 0usize;
        while let Some(id) = ancestor {
            traversed = traversed.saturating_add(1);
            if traversed > projection.nodes.len() {
                return None;
            }
            let snapshot = projection.node(id)?;
            if id != node {
                opened |= self.expanded.insert(id);
            }
            ancestor = snapshot.parent;
        }
        if opened {
            self.rebuild_visible_rows();
        }
        self.pending_scroll_to = Some(node);
        Some(())
    }

    pub(crate) fn select_node(&mut self, node: XmlNodeId) -> Option<XmlNodeSelection> {
        let projection = self.projection.as_ref()?;
        if projection.node(node).is_none() {
            return None;
        }
        if self.selected.is_some_and(|selected| {
            selected.document == projection.document && selected.node == node
        }) {
            return self.selected;
        }
        let request = self.next_selection_request;
        let Some(next_request) = self.next_selection_request.checked_add(1) else {
            self.selected = None;
            self.latest_detail_request = None;
            self.latest_copy_request = None;
            return None;
        };
        self.next_selection_request = next_request;
        let selection = XmlNodeSelection {
            revision: self.revision?,
            document: projection.document,
            node,
            request: XmlSelectionRequestId::from_counter(request),
        };
        self.selected = Some(selection);
        self.latest_detail_request = None;
        self.latest_copy_request = None;
        Some(selection)
    }

    /// Create an async detail operation for the current selection. Copy
    /// authority exists only for an explicit `Copy` request and is revoked by
    /// every later selection, inspection, or document invalidation.
    pub(crate) fn request_detail(
        &mut self,
        kind: XmlNodeDetailKind,
        intent: XmlDetailIntent,
    ) -> Option<XmlNodeDetailRequest> {
        let selection = self.selected?;
        if self.projection.as_ref()?.node(selection.node).is_none() {
            return None;
        }
        let detail_request = self.next_detail_request;
        let Some(next_request) = self.next_detail_request.checked_add(1) else {
            self.latest_detail_request = None;
            self.latest_copy_request = None;
            return None;
        };
        self.next_detail_request = next_request;
        let request = XmlNodeDetailRequest {
            revision: selection.revision,
            document: selection.document,
            node: selection.node,
            selection_request: selection.request,
            detail_request: XmlDetailRequestId::from_counter(detail_request),
            kind,
            intent,
        };
        self.latest_detail_request = Some(request);
        self.latest_copy_request = (intent == XmlDetailIntent::Copy).then_some(request);
        Some(request)
    }

    /// Return a complete detail value only while it still belongs to the
    /// current explicitly requested copy action. Callers must publish this
    /// value to the clipboard explicitly; rendering never does so.
    pub(crate) fn take_valid_copy_completion(
        &mut self,
        snapshot: &XmlSessionSnapshot,
    ) -> Option<Arc<XmlNodeDetailOutput>> {
        let request = self.latest_copy_request?;
        let selected = self.selected?;
        let projected_document = self
            .projection
            .as_ref()
            .map(|projection| projection.document);
        let snapshot_document = match &snapshot.tree {
            XmlTreeStatus::Ready(projection) => Some(projection.document),
            _ => None,
        };
        if request.intent != XmlDetailIntent::Copy
            || self.latest_detail_request != Some(request)
            || self.revision != Some(request.revision)
            || snapshot.revision != Some(request.revision)
            || projected_document != Some(request.document)
            || snapshot_document != Some(request.document)
            || selected.revision != request.revision
            || selected.document != request.document
            || selected.node != request.node
            || selected.request != request.selection_request
        {
            return None;
        }
        match &snapshot.detail {
            XmlDetailStatus::Ready(output) if output.request == request => {
                self.latest_copy_request = None;
                Some(Arc::clone(output))
            }
            _ => None,
        }
    }

    pub(crate) fn toggle_expansion(&mut self, node: XmlNodeId) -> bool {
        let Some(projection) = self.projection.as_ref() else {
            return false;
        };
        let Some(snapshot) = projection.node(node) else {
            return false;
        };
        if snapshot.attributes.is_empty()
            && snapshot.children.is_empty()
            && !snapshot.has_more_attributes
            && !snapshot.has_more_children
        {
            return false;
        }
        if !self.expanded.remove(&node) {
            self.expanded.insert(node);
        }
        self.rebuild_visible_rows();
        true
    }

    fn rebuild_visible_rows(&mut self) {
        self.visible_rows.clear();
        let Some(projection) = self.projection.as_ref() else {
            return;
        };
        let mut pending = vec![(TreeRowIdentity::Node(projection.root), 0usize)];
        while let Some((identity, depth)) = pending.pop() {
            let TreeRowIdentity::Node(node_id) = identity else {
                self.visible_rows.push(VisibleTreeRow {
                    identity,
                    node: None,
                    depth,
                });
                continue;
            };
            let Some(node) = projection.node(node_id) else {
                continue;
            };
            self.visible_rows.push(VisibleTreeRow {
                identity,
                node: Some(node_id),
                depth,
            });
            if !self.expanded.contains(&node_id) {
                continue;
            }

            // Push reverse display order so attributes, their omission marker,
            // children, and the child omission marker appear in source order.
            if node.has_more_children {
                pending.push((TreeRowIdentity::MoreChildren(node_id), depth + 1));
            }
            pending.extend(
                node.children
                    .iter()
                    .rev()
                    .copied()
                    .map(|id| (TreeRowIdentity::Node(id), depth + 1)),
            );
            if node.has_more_attributes {
                pending.push((TreeRowIdentity::MoreAttributes(node_id), depth + 1));
            }
            pending.extend(
                node.attributes
                    .iter()
                    .rev()
                    .copied()
                    .map(|id| (TreeRowIdentity::Node(id), depth + 1)),
            );
        }
        #[cfg(test)]
        {
            self.visible_rows_rebuilds += 1;
        }
    }

    /// Render the cached projection using virtual rows. Returned intents are
    /// generated only on the first egui pass, avoiding duplicate operations
    /// from a discarded multipass layout.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
    ) -> XmlToolkitTreeIntents {
        self.show_scoped(ui, snapshot, &mut XmlJumpTargets::new(None), true)
    }

    pub(crate) fn show_scoped(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
        targets: &mut XmlJumpTargets<'_>,
        authorized_pass: bool,
    ) -> XmlToolkitTreeIntents {
        self.sync(snapshot);
        #[cfg(test)]
        {
            self.rendered_rows = 0;
        }
        let first_pass = ui.ctx().current_pass_index() == 0;
        let mut intents = XmlToolkitTreeIntents::default();
        let Some(projection) = self.projection.as_ref().cloned() else {
            match &snapshot.tree {
                XmlTreeStatus::Empty => {
                    ui.weak("Enter XML source to browse its tree.");
                }
                XmlTreeStatus::Pending => {
                    ui.weak("Parsing XML tree…");
                }
                XmlTreeStatus::Failed(error) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("XML tree unavailable: {error:?}"),
                    );
                }
                XmlTreeStatus::Ready(_) => {}
            }
            return intents;
        };

        let row_height = ui.spacing().interact_size.y.max(TREE_ROW_HEIGHT);
        let mut toggle = None;
        let mut selection = None;
        let mut focus_selection = None;
        let mut copy_kind = None;
        let available_height = ui.available_height().max(0.0);
        let details_height = (available_height * 0.35)
            .clamp(110.0, 240.0)
            .min(available_height);
        let tree_height =
            (available_height - details_height - ui.spacing().item_spacing.y).max(0.0);
        let scroll_target = if first_pass && authorized_pass {
            self.pending_scroll_to.take()
        } else {
            None
        };
        let scroll_row = scroll_target.and_then(|target| {
            self.visible_rows
                .iter()
                .position(|row| row.node == Some(target))
        });
        let row_spacing = ui.spacing().item_spacing.y;
        let viewport_height = tree_height;
        let mut tree_scroll = egui::ScrollArea::both()
            .id_salt("xml_toolkit_tree_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(tree_height);
        if let Some(row) = scroll_row {
            tree_scroll = tree_scroll.vertical_scroll_offset(tree_row_scroll_offset(
                row,
                row_height,
                row_spacing,
                viewport_height,
                self.visible_rows.len(),
            ));
        }
        tree_scroll.show_rows(ui, row_height, self.visible_rows.len(), |ui, range| {
            #[cfg(test)]
            let mut rendered_rows = 0usize;
            for index in range {
                #[cfg(test)]
                {
                    rendered_rows += 1;
                }
                let row = self.visible_rows[index];
                ui.push_id(("xml_tree_row", row.identity), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(row.depth as f32 * 12.0);
                        let Some(node_id) = row.node else {
                            let marker = match row.identity {
                                TreeRowIdentity::MoreAttributes(_) => {
                                    "… attributes omitted by projection limit"
                                }
                                TreeRowIdentity::MoreChildren(_) => {
                                    "… child nodes omitted by projection limit"
                                }
                                TreeRowIdentity::Node(_) => "… omitted by projection limit",
                            };
                            ui.weak(marker);
                            return;
                        };
                        let Some(node) = projection.node(node_id) else {
                            return;
                        };
                        let can_expand = !node.attributes.is_empty()
                            || !node.children.is_empty()
                            || node.has_more_attributes
                            || node.has_more_children;
                        let expanded = self.expanded.contains(&node_id);
                        if can_expand {
                            let response = ui.small_button(if expanded { "▾" } else { "▸" });
                            targets.button(
                                ui,
                                &response,
                                format!("tree/{:?}/node/{node_id:?}/toggle", snapshot.revision),
                                "Expand or collapse XML node",
                            );
                            if response.clicked() {
                                toggle = Some(node_id);
                            }
                        } else {
                            ui.add_space(ui.spacing().interact_size.x);
                        }
                        let label = tree_node_label(node);
                        let response = ui.selectable_label(
                            self.selected
                                .is_some_and(|selected| selected.node == node_id),
                            egui::RichText::new(label).monospace(),
                        );
                        if targets.selectable(
                            ui,
                            &response,
                            format!("tree/{:?}/node/{node_id:?}/select", snapshot.revision),
                            "XML tree node",
                            TargetKind::Action,
                        ) {
                            focus_selection = Some(node_id);
                        }
                        if response.clicked() {
                            selection = Some(node_id);
                        }
                    });
                });
            }
            #[cfg(test)]
            {
                self.rendered_rows += rendered_rows;
            }
        });

        if projection.truncated.is_some() {
            ui.weak(match projection.truncated {
                Some(XmlTreeProjectionLimit::NodeCount) => {
                    "Tree projection reached its node limit."
                }
                Some(XmlTreeProjectionLimit::MetadataBytes) => {
                    "Tree projection reached its metadata limit."
                }
                None => "",
            });
        }
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("xml_toolkit_tree_details_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(details_height)
            .show(ui, |ui| {
                let selected_identity = self
                    .selected
                    .map(|selection| (selection.revision, selection.document, selection.node));
                ui.push_id(("xml_tree_details", selected_identity), |ui| {
                    self.show_selected_details(ui, snapshot, &projection, &mut copy_kind, targets);
                });
            });

        if first_pass && authorized_pass {
            if let Some(node) = toggle {
                self.toggle_expansion(node);
            }
            if let Some(node) = selection.or(focus_selection)
                && let Some(selected) = self.select_node(node)
            {
                intents.selection = Some(selected);
                self.pending_scroll_to = Some(node);
                if focus_selection.is_none()
                    && let Some(kind) =
                        default_detail_kind(projection.node(node).map(|snapshot| snapshot.kind))
                {
                    intents.detail = self.request_detail(kind, XmlDetailIntent::Inspect);
                }
            }
            if selection.is_none()
                && let Some(kind) = copy_kind
            {
                intents.detail = self.request_detail(kind, XmlDetailIntent::Copy);
            }
        }
        intents
    }

    fn show_selected_details(
        &self,
        ui: &mut egui::Ui,
        snapshot: &XmlSessionSnapshot,
        projection: &XmlTreeProjection,
        copy_kind: &mut Option<XmlNodeDetailKind>,
        targets: &mut XmlJumpTargets<'_>,
    ) {
        let Some(selection) = self.selected else {
            ui.weak("Select a node to inspect it.");
            return;
        };
        let Some(node) = projection.node(selection.node) else {
            ui.weak("The selected node is unavailable.");
            return;
        };
        ui.label(format!(
            "Node: {}",
            bounded_chars(&node.qualified_name, MAX_TREE_LABEL_CHARS)
        ));
        ui.label(format!("Kind: {}", node_kind_label(node.kind)));
        if !node.local_name.is_empty() && node.local_name != node.qualified_name {
            ui.label(format!(
                "Local name: {}",
                bounded_chars(&node.local_name, MAX_TREE_LABEL_CHARS)
            ));
        }
        if !node.prefix.is_empty() {
            ui.label(format!(
                "Prefix: {}",
                bounded_chars(&node.prefix, MAX_TREE_LABEL_CHARS)
            ));
        }
        if !node.namespace_uri.is_empty() {
            ui.label(format!(
                "Namespace: {}",
                bounded_chars(&node.namespace_uri, MAX_TREE_LABEL_CHARS)
            ));
        }
        if !node.preview.is_empty() || node.preview_truncated {
            ui.label("Value preview:");
            let preview = xml_escaped_display(&node.preview, MAX_XML_TREE_PREVIEW_CHARS);
            ui.add(egui::Label::new(egui::RichText::new(preview).monospace()).wrap());
            if node.preview_truncated {
                ui.weak("Preview is clipped. Use an explicit copy request for the complete value.");
            }
        }

        match node.kind {
            XmlTreeNodeKind::Element => {
                ui.horizontal(|ui| {
                    let copy_text = ui.small_button("Copy text content");
                    targets.button(
                        ui,
                        &copy_text,
                        format!(
                            "tree/{:?}/node/{:?}/copy-text",
                            snapshot.revision, selection.node
                        ),
                        "Copy XML text",
                    );
                    if copy_text.clicked() {
                        *copy_kind = Some(XmlNodeDetailKind::ElementText);
                    }
                    let copy_xml = ui.small_button("Copy XML fragment");
                    targets.button(
                        ui,
                        &copy_xml,
                        format!(
                            "tree/{:?}/node/{:?}/copy-xml",
                            snapshot.revision, selection.node
                        ),
                        "Copy XML fragment",
                    );
                    if copy_xml.clicked() {
                        *copy_kind = Some(XmlNodeDetailKind::XmlFragment);
                    }
                });
            }
            kind if default_detail_kind(Some(kind)).is_some() && {
                let response = ui.small_button("Copy complete value");
                targets.button(
                    ui,
                    &response,
                    format!(
                        "tree/{:?}/node/{:?}/copy-value",
                        snapshot.revision, selection.node
                    ),
                    "Copy XML value",
                );
                response.clicked()
            } =>
            {
                *copy_kind = default_detail_kind(Some(kind));
            }
            _ => {}
        }
        match &snapshot.detail {
            XmlDetailStatus::Pending(request) if request_matches_selection(*request, selection) => {
                ui.weak(match request.intent {
                    XmlDetailIntent::Inspect => "Loading selected node value…",
                    XmlDetailIntent::Copy => "Preparing complete value for explicit copy…",
                });
            }
            XmlDetailStatus::Ready(output)
                if request_matches_selection(output.request, selection)
                    && output.request.intent == XmlDetailIntent::Inspect =>
            {
                let (value, truncated) = bounded_prefix(&output.value, MAX_DETAIL_DISPLAY_CHARS);
                ui.add(egui::Label::new(egui::RichText::new(value).monospace()).wrap());
                if truncated {
                    ui.weak(
                        "Displayed value is clipped; the async result retains the complete value.",
                    );
                }
            }
            XmlDetailStatus::Failed { request, error }
                if request_matches_selection(*request, selection) =>
            {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("Node detail unavailable: {error:?}"),
                );
            }
            XmlDetailStatus::Ready(output)
                if request_matches_selection(output.request, selection)
                    && output.request.intent == XmlDetailIntent::Copy =>
            {
                ui.weak(
                    "The complete value is ready for the requesting caller to publish explicitly.",
                );
            }
            _ => {}
        }
    }

    #[cfg(test)]
    fn visible_rows(&self) -> &[VisibleTreeRow] {
        &self.visible_rows
    }

    #[cfg(test)]
    pub(crate) fn is_expanded_for_test(&self, node: XmlNodeId) -> bool {
        self.expanded.contains(&node)
    }

    #[cfg(test)]
    pub(crate) fn pending_scroll_for_test(&self) -> Option<XmlNodeId> {
        self.pending_scroll_to
    }
}

fn tree_node_label(node: &XmlTreeNodeSnapshot) -> String {
    let kind = node_kind_label(node.kind);
    let name = if node.qualified_name.is_empty() {
        "#unnamed"
    } else {
        node.qualified_name.as_str()
    };
    let name = xml_escaped_display(name, MAX_TREE_LABEL_CHARS);
    if node.preview.is_empty() {
        format!("{kind}  {name}")
    } else {
        let preview = xml_escaped_display(&node.preview, MAX_XML_TREE_PREVIEW_CHARS);
        format!("{kind}  {name}  {preview}")
    }
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

fn default_detail_kind(kind: Option<XmlTreeNodeKind>) -> Option<XmlNodeDetailKind> {
    match kind? {
        XmlTreeNodeKind::Element => Some(XmlNodeDetailKind::XmlFragment),
        XmlTreeNodeKind::Attribute => Some(XmlNodeDetailKind::AttributeValue),
        XmlTreeNodeKind::Text
        | XmlTreeNodeKind::CData
        | XmlTreeNodeKind::Comment
        | XmlTreeNodeKind::ProcessingInstruction => Some(XmlNodeDetailKind::TextValue),
        XmlTreeNodeKind::Document | XmlTreeNodeKind::Other => None,
    }
}

fn request_matches_selection(request: XmlNodeDetailRequest, selection: XmlNodeSelection) -> bool {
    request.revision == selection.revision
        && request.document == selection.document
        && request.node == selection.node
        && request.selection_request == selection.request
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

fn xml_escaped_display(value: &str, max_chars: usize) -> String {
    let mut display = String::new();
    for ch in value.chars().take(max_chars) {
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

fn tree_row_scroll_offset(
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
    use crate::xml_toolkit_engine::{XmlDocumentId, XmlTreeNodeSnapshot, XmlTreeProjection};
    use crate::xml_toolkit_session::{
        XmlDetailStatus, XmlOutputStatus, XmlParseStatus, XmlSessionSnapshot, XmlTreeStatus,
        XmlXPathStatus,
    };

    fn fixture(document_number: u64) -> (Arc<XmlTreeProjection>, XmlNodeId, XmlNodeId) {
        let document = XmlDocumentId::for_test(document_number);
        let root = XmlNodeId::for_test(document, 1);
        let repeated_one = XmlNodeId::for_test(document, 2);
        let repeated_two = XmlNodeId::for_test(document, 3);
        let leaf = XmlNodeId::for_test(document, 4);
        (
            Arc::new(XmlTreeProjection::from_nodes(
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
                        children: vec![repeated_one, repeated_two],
                        has_more_attributes: false,
                        has_more_children: false,
                    },
                    XmlTreeNodeSnapshot {
                        id: repeated_one,
                        parent: Some(root),
                        kind: XmlTreeNodeKind::Element,
                        qualified_name: "item".into(),
                        local_name: "item".into(),
                        prefix: String::new(),
                        namespace_uri: "urn:test".into(),
                        preview: String::new(),
                        preview_truncated: false,
                        attributes: Vec::new(),
                        children: vec![leaf],
                        has_more_attributes: false,
                        has_more_children: false,
                    },
                    XmlTreeNodeSnapshot {
                        id: repeated_two,
                        parent: Some(root),
                        kind: XmlTreeNodeKind::Element,
                        qualified_name: "item".into(),
                        local_name: "item".into(),
                        prefix: String::new(),
                        namespace_uri: "urn:test".into(),
                        preview: String::new(),
                        preview_truncated: false,
                        attributes: Vec::new(),
                        children: Vec::new(),
                        has_more_attributes: true,
                        has_more_children: true,
                    },
                    XmlTreeNodeSnapshot {
                        id: leaf,
                        parent: Some(repeated_one),
                        kind: XmlTreeNodeKind::Text,
                        qualified_name: "#text".into(),
                        local_name: "#text".into(),
                        prefix: String::new(),
                        namespace_uri: String::new(),
                        preview: "text 🙂 & <".into(),
                        preview_truncated: false,
                        attributes: Vec::new(),
                        children: Vec::new(),
                        has_more_attributes: false,
                        has_more_children: false,
                    },
                ],
                Some(XmlTreeProjectionLimit::NodeCount),
            )),
            repeated_one,
            repeated_two,
        )
    }

    fn snapshot(revision: XmlSourceRevision, tree: XmlTreeStatus) -> XmlSessionSnapshot {
        XmlSessionSnapshot {
            revision: Some(revision),
            parse: if matches!(tree, XmlTreeStatus::Ready(_)) {
                XmlParseStatus::Ready
            } else {
                XmlParseStatus::Empty
            },
            output: XmlOutputStatus::Empty,
            tree,
            detail: XmlDetailStatus::Empty,
            xpath: XmlXPathStatus::NotRun,
            schema: Default::default(),
        }
    }

    #[test]
    fn m7c_tree_state_clears_stale_copy_and_caches_virtual_rows() {
        let revision = XmlSourceRevision::for_test(1);
        let (projection, first, second) = fixture(1);
        let mut state = XmlToolkitTreeViewState::default();
        state.sync(&snapshot(
            revision,
            XmlTreeStatus::Ready(Arc::clone(&projection)),
        ));
        assert_eq!(state.visible_rows().len(), 3);
        let initial_rebuilds = state.visible_rows_rebuilds;

        // Repeated frame synchronization does not rewalk the immutable tree.
        for _ in 0..8 {
            state.sync(&snapshot(
                revision,
                XmlTreeStatus::Ready(Arc::clone(&projection)),
            ));
        }
        assert_eq!(state.visible_rows_rebuilds, initial_rebuilds);
        assert!(state.toggle_expansion(first));
        assert!(
            state
                .visible_rows()
                .iter()
                .any(|row| row.node == Some(XmlNodeId::for_test(projection.document, 4,)))
        );
        assert_eq!(state.visible_rows_rebuilds, initial_rebuilds + 1);
        assert!(state.toggle_expansion(second));
        let more_attributes = state
            .visible_rows()
            .iter()
            .position(|row| row.identity == TreeRowIdentity::MoreAttributes(second))
            .unwrap();
        let more_children = state
            .visible_rows()
            .iter()
            .position(|row| row.identity == TreeRowIdentity::MoreChildren(second))
            .unwrap();
        assert!(more_attributes < more_children);

        let selection = state.select_node(first).unwrap();
        assert_eq!(selection.document, projection.document);
        let copy = state
            .request_detail(XmlNodeDetailKind::XmlFragment, XmlDetailIntent::Copy)
            .unwrap();
        let completed = XmlNodeDetailOutput {
            request: copy,
            value: "<item xmlns=\"urn:test\">complete</item>".into(),
        };
        let mut copied_snapshot = snapshot(revision, XmlTreeStatus::Ready(Arc::clone(&projection)));
        copied_snapshot.detail = XmlDetailStatus::Ready(Arc::new(completed));
        let (other_projection, _, _) = fixture(2);
        let mut other_document_snapshot =
            snapshot(revision, XmlTreeStatus::Ready(other_projection));
        other_document_snapshot.detail = copied_snapshot.detail.clone();
        assert!(
            state
                .take_valid_copy_completion(&other_document_snapshot)
                .is_none()
        );
        let completed = state
            .take_valid_copy_completion(&copied_snapshot)
            .expect("only the current explicit copy can be consumed");
        assert_eq!(completed.value, "<item xmlns=\"urn:test\">complete</item>");
        assert!(state.take_valid_copy_completion(&copied_snapshot).is_none());

        // A different repeated sibling has a distinct actual node identity and
        // selection token; the old request cannot authorize clipboard output.
        assert_eq!(state.select_node(second).unwrap().node, second);
        assert!(state.take_valid_copy_completion(&copied_snapshot).is_none());

        let next_revision = XmlSourceRevision::for_test(2);
        state.sync(&snapshot(next_revision, XmlTreeStatus::Empty));
        assert!(state.projection().is_none());
        assert!(state.selected().is_none());
        assert!(state.visible_rows().is_empty());
        assert!(state.take_valid_copy_completion(&copied_snapshot).is_none());
    }

    #[test]
    fn m7c_tree_ui_renders_only_the_visible_virtual_rows() {
        let document = XmlDocumentId::for_test(7);
        let root = XmlNodeId::for_test(document, 1);
        let children: Vec<_> = (2..=2001)
            .map(|ordinal| XmlNodeId::for_test(document, ordinal))
            .collect();
        let mut nodes = Vec::with_capacity(children.len() + 1);
        nodes.push(XmlTreeNodeSnapshot {
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
            children: children.clone(),
            has_more_attributes: false,
            has_more_children: false,
        });
        nodes.extend(
            children
                .iter()
                .enumerate()
                .map(|(index, id)| XmlTreeNodeSnapshot {
                    id: *id,
                    parent: Some(root),
                    kind: XmlTreeNodeKind::Element,
                    qualified_name: format!("item-{index}"),
                    local_name: format!("item-{index}"),
                    prefix: String::new(),
                    namespace_uri: String::new(),
                    preview: String::new(),
                    preview_truncated: false,
                    attributes: Vec::new(),
                    children: Vec::new(),
                    has_more_attributes: false,
                    has_more_children: false,
                }),
        );
        let projection = Arc::new(XmlTreeProjection::from_nodes(document, root, nodes, None));
        let revision = XmlSourceRevision::for_test(7);
        let source = snapshot(revision, XmlTreeStatus::Ready(projection));
        let mut state = XmlToolkitTreeViewState::default();
        state.sync(&source);
        state.select_node(children[0]).unwrap();
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let output = context.run_ui(input, |ui| {
            let _ = state.show(ui, &source);
        });
        assert_eq!(state.visible_rows().len(), 2001);
        assert!(state.rendered_rows > 0);
        assert!(state.rendered_rows < state.visible_rows().len());
        fn contains_text(shape: &egui::epaint::Shape, needle: &str) -> bool {
            match shape {
                egui::epaint::Shape::Text(text) => text.galley.job.text.contains(needle),
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().any(|shape| contains_text(shape, needle))
                }
                _ => false,
            }
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| contains_text(&shape.shape, "Copy text content"))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| contains_text(&shape.shape, "Copy XML fragment"))
        );
        output.drop_without_applying_deltas();
    }
}
