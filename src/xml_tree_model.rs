//! Renderer-neutral retained XML tree projection and checked detail intents.
use crate::xml_toolkit_engine::{
    XmlNodeDetailKind, XmlNodeId, XmlTreeNodeKind, XmlTreeNodeSnapshot, XmlTreeProjection,
    MAX_XML_TREE_PREVIEW_CHARS,
};
use crate::xml_toolkit_session::{
    XmlDetailIntent, XmlDetailRequestId, XmlDetailStatus, XmlNodeDetailOutput,
    XmlNodeDetailRequest, XmlNodeSelection, XmlSelectionRequestId, XmlSessionSnapshot,
    XmlSourceRevision, XmlTreeStatus,
};
use std::{collections::HashSet, sync::Arc};

pub const MAX_TREE_LABEL_CHARS: usize = 256;
pub const MAX_DETAIL_DISPLAY_CHARS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TreeRowIdentity {
    Node(XmlNodeId),
    MoreAttributes(XmlNodeId),
    MoreChildren(XmlNodeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisibleTreeRow {
    pub(crate) identity: TreeRowIdentity,
    pub(crate) node: Option<XmlNodeId>,
    pub(crate) depth: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct XmlToolkitTreeIntents {
    pub selection: Option<XmlNodeSelection>,
    pub detail: Option<XmlNodeDetailRequest>,
}

/// Cached view-only state. A source revision or parsed document change drops
/// selection, expansion, visible rows, and pending copy authority together.
#[derive(Clone, Debug, Default)]
pub struct XmlToolkitTreeViewState {
    pub(crate) revision: Option<XmlSourceRevision>,
    pub(crate) projection: Option<Arc<XmlTreeProjection>>,
    pub(crate) selected: Option<XmlNodeSelection>,
    pub(crate) expanded: HashSet<XmlNodeId>,
    pub(crate) visible_rows: Vec<VisibleTreeRow>,
    pub(crate) next_selection_request: u64,
    pub(crate) next_detail_request: u64,
    pub(crate) latest_detail_request: Option<XmlNodeDetailRequest>,
    pub(crate) latest_copy_request: Option<XmlNodeDetailRequest>,
    pub(crate) pending_scroll_to: Option<XmlNodeId>,
    #[cfg(test)]
    pub(crate) visible_rows_rebuilds: usize,
    #[cfg(test)]
    pub(crate) rendered_rows: usize,
}

impl XmlToolkitTreeViewState {
    /// Synchronize the cached tree to the session's accepted source. This is
    /// cheap and idempotent during ordinary egui frames.
    pub fn sync(&mut self, snapshot: &XmlSessionSnapshot) {
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

    pub fn projection(&self) -> Option<&Arc<XmlTreeProjection>> {
        self.projection.as_ref()
    }

    pub fn selected(&self) -> Option<XmlNodeSelection> {
        self.selected
    }

    /// Revoke any pending or ready copy authorization while keeping the
    /// current tree selection and expansion state intact.
    pub fn revoke_copy_authority(&mut self) {
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
    pub fn reveal_node(&mut self, node: XmlNodeId) -> Option<XmlToolkitTreeIntents> {
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
    pub fn request_node_copy(
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

    pub fn select_node(&mut self, node: XmlNodeId) -> Option<XmlNodeSelection> {
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
    pub fn request_detail(
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
    pub fn take_valid_copy_completion(
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

    pub fn toggle_expansion(&mut self, node: XmlNodeId) -> bool {
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

    pub fn visible_contains(&self, node: XmlNodeId) -> bool {
        self.visible_rows.iter().any(|row| row.node == Some(node))
    }

    pub fn row_count(&self) -> usize {
        self.visible_rows.len()
    }
    pub fn row(&self, index: usize) -> Option<(TreeRowIdentity, Option<XmlNodeId>, usize)> {
        self.visible_rows
            .get(index)
            .map(|row| (row.identity, row.node, row.depth))
    }
    pub fn is_expanded(&self, node: XmlNodeId) -> bool {
        self.expanded.contains(&node)
    }
}

pub fn tree_node_label(node: &XmlTreeNodeSnapshot) -> String {
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

pub fn node_kind_label(kind: XmlTreeNodeKind) -> &'static str {
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

pub fn default_detail_kind(kind: Option<XmlTreeNodeKind>) -> Option<XmlNodeDetailKind> {
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

pub fn request_matches_selection(
    request: XmlNodeDetailRequest,
    selection: XmlNodeSelection,
) -> bool {
    request.revision == selection.revision
        && request.document == selection.document
        && request.node == selection.node
        && request.selection_request == selection.request
}

pub fn bounded_chars(value: &str, max_chars: usize) -> &str {
    value
        .char_indices()
        .nth(max_chars)
        .map_or(value, |(index, _)| &value[..index])
}

pub fn bounded_prefix(value: &str, max_chars: usize) -> (&str, bool) {
    let mut chars = value.char_indices();
    if let Some((index, _)) = chars.nth(max_chars) {
        (&value[..index], true)
    } else {
        (value, false)
    }
}

pub fn xml_escaped_display(value: &str, max_chars: usize) -> String {
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
