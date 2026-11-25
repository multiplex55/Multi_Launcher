//! Renderer-neutral XPath request and navigation state.
use crate::xml_toolkit_engine::{
    XmlEngineError, XmlNodeDetailKind, XmlNodeSnapshot, XmlTreeNodeKind,
    MAX_XPATH_EXPRESSION_BYTES, MAX_XPATH_NAMESPACE_BYTES,
};
use crate::xml_toolkit_session::{
    XmlNamespaceContextRevision, XmlSessionSnapshot, XmlSourceRevision, XmlTreeStatus,
    XmlXPathQueryRequest, XmlXPathRequestId, XmlXPathStatus,
};
use crate::xml_tree_model::{XmlToolkitTreeIntents, XmlToolkitTreeViewState};
use std::sync::Arc;

pub const MAX_NAMESPACE_ROWS: usize = 64;
pub const MAX_RESULT_LABEL_CHARS: usize = 192;
pub const MAX_RESULT_VALUE_CHARS: usize = 240;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NamespaceBindingEditorRow {
    pub(crate) id: u64,
    pub(crate) prefix: String,
    pub(crate) uri: String,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NamespaceRowAction {
    Add,
    Remove(usize),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct XmlToolkitXPathIntents {
    /// Apply this first when both invalidation and a new request are returned.
    pub invalidate: Option<XmlSourceRevision>,
    pub query: Option<XmlXPathQueryRequest>,
    pub tree: XmlToolkitTreeIntents,
    /// A scalar copy is separately authorized and completed through
    /// `take_valid_scalar_copy_completion` by the caller.
    pub scalar_copy: Option<XmlXPathQueryRequest>,
}

#[derive(Clone, Debug, Default)]
pub struct XmlToolkitXPathViewState {
    pub(crate) expression: String,
    pub(crate) namespaces: Vec<NamespaceBindingEditorRow>,
    pub(crate) pending_namespace_action: Option<NamespaceRowAction>,
    pub(crate) namespace_context_revision: u64,
    pub(crate) next_namespace_row: u64,
    pub(crate) query_view_generation: u64,
    pub(crate) next_request: u64,
    pub(crate) revision: Option<XmlSourceRevision>,
    pub(crate) active_request: Option<XmlXPathQueryRequest>,
    pub(crate) selected_result: Option<usize>,
    pub(crate) pending_result_scroll_to: Option<usize>,
    pub(crate) latest_scalar_copy_request: Option<XmlXPathQueryRequest>,
    pub(crate) navigation_unavailable: bool,
    pub(crate) expression_was_clipped: bool,
    pub(crate) namespaces_were_clipped: bool,
    #[cfg(test)]
    pub(crate) visible_rows: usize,
    #[cfg(test)]
    pub(crate) rendered_rows: usize,
    #[cfg(test)]
    pub(crate) max_rendered_result_row_height: f32,
    #[cfg(test)]
    pub(crate) result_scroll_requests: usize,
    #[cfg(test)]
    pub(crate) namespace_action_commits: usize,
}

impl XmlToolkitXPathViewState {
    /// Drop local navigation/copy authority as soon as the accepted source
    /// revision changes, even before a new parse completes.
    pub fn sync(&mut self, snapshot: &XmlSessionSnapshot) {
        if self.revision != snapshot.revision {
            self.revision = snapshot.revision;
            self.query_view_generation = self.query_view_generation.wrapping_add(1);
            self.active_request = None;
            self.selected_result = None;
            self.pending_result_scroll_to = None;
            self.latest_scalar_copy_request = None;
            self.navigation_unavailable = false;
        }
    }

    /// Completes only the latest explicit scalar-copy intent while its query,
    /// document, and source revision still match the displayed result.
    pub fn take_valid_scalar_copy_completion(
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

    pub fn is_current_request(
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

    pub fn visible_status<'a>(&self, snapshot: &'a XmlSessionSnapshot) -> VisibleXPathStatus<'a> {
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

    pub fn invalidate_local_query(&mut self, tree: &mut XmlToolkitTreeViewState) {
        self.query_view_generation = self.query_view_generation.wrapping_add(1);
        self.active_request = None;
        self.selected_result = None;
        self.pending_result_scroll_to = None;
        self.latest_scalar_copy_request = None;
        self.navigation_unavailable = false;
        tree.revoke_copy_authority();
    }

    pub fn navigate_to_result(
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

    pub fn request_result_copy(
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

    pub fn authorize_scalar_copy(
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

    pub fn next_query_request(
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
    pub fn expression_was_clipped(&self) -> bool {
        self.expression_was_clipped
    }
    pub fn namespaces_were_clipped(&self) -> bool {
        self.namespaces_were_clipped
    }
    pub fn expression(&self) -> &str {
        &self.expression
    }
    pub fn generation(&self) -> u64 {
        self.query_view_generation
    }
    pub fn selected_result(&self) -> Option<usize> {
        self.selected_result
    }
    pub fn namespace_rows(&self) -> impl Iterator<Item = (u64, &str, &str)> {
        self.namespaces
            .iter()
            .map(|row| (row.id, row.prefix.as_str(), row.uri.as_str()))
    }
    pub fn edit_expression(
        &mut self,
        mut value: String,
        tree: &mut XmlToolkitTreeViewState,
    ) -> bool {
        let clipped = value.len() > MAX_XPATH_EXPRESSION_BYTES;
        truncate_utf8(&mut value, MAX_XPATH_EXPRESSION_BYTES);
        if value == self.expression && clipped == self.expression_was_clipped {
            return false;
        }
        self.expression = value;
        self.expression_was_clipped = clipped;
        self.invalidate_local_query(tree);
        true
    }
    pub fn add_namespace(&mut self, tree: &mut XmlToolkitTreeViewState) -> Option<u64> {
        if self.namespaces.len() >= MAX_NAMESPACE_ROWS {
            return None;
        }
        let id = self.next_namespace_row;
        self.next_namespace_row = self.next_namespace_row.checked_add(1)?;
        self.namespaces.push(NamespaceBindingEditorRow {
            id,
            ..Default::default()
        });
        self.namespaces_were_clipped = cap_namespace_editor(&mut self.namespaces);
        self.namespace_context_revision = self.namespace_context_revision.saturating_add(1);
        self.invalidate_local_query(tree);
        Some(id)
    }
    pub fn remove_namespace(&mut self, id: u64, tree: &mut XmlToolkitTreeViewState) -> bool {
        let Some(index) = self.namespaces.iter().position(|row| row.id == id) else {
            return false;
        };
        self.namespaces.remove(index);
        self.namespace_context_revision = self.namespace_context_revision.saturating_add(1);
        self.invalidate_local_query(tree);
        true
    }
    pub fn edit_namespace(
        &mut self,
        id: u64,
        prefix: String,
        uri: String,
        tree: &mut XmlToolkitTreeViewState,
    ) -> bool {
        let Some(row) = self.namespaces.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        if row.prefix == prefix && row.uri == uri {
            return false;
        }
        row.prefix = prefix;
        row.uri = uri;
        self.namespaces_were_clipped = cap_namespace_editor(&mut self.namespaces);
        self.namespace_context_revision = self.namespace_context_revision.saturating_add(1);
        self.invalidate_local_query(tree);
        true
    }
    pub fn namespace_error(&self) -> Option<String> {
        let bindings: Vec<_> = self
            .namespaces
            .iter()
            .map(|row| (row.prefix.clone(), row.uri.clone()))
            .collect();
        crate::xml_toolkit_engine::validate_xpath_namespaces(&bindings)
            .err()
            .map(|error| match error {
                XmlEngineError::XPath(message) => message,
                error => format!("{error:?}"),
            })
    }
    pub fn run(
        &mut self,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
    ) -> Result<XmlXPathQueryRequest, String> {
        let bindings: Vec<_> = self
            .namespaces
            .iter()
            .map(|row| (row.prefix.clone(), row.uri.clone()))
            .collect();
        crate::xml_toolkit_engine::validate_xpath_namespaces(&bindings)
            .map_err(|error| format!("{error:?}"))?;
        let XmlTreeStatus::Ready(projection) = &snapshot.tree else {
            return Err("XML tree is not ready.".into());
        };
        if snapshot.parse != crate::xml_toolkit_session::XmlParseStatus::Ready {
            return Err("XML source is not ready.".into());
        }
        let revision = snapshot
            .revision
            .ok_or_else(|| "XML source is not ready.".to_owned())?;
        self.invalidate_local_query(tree);
        let request = self
            .next_query_request(revision, projection.document)
            .ok_or_else(|| "XPath request limit reached.".to_owned())?;
        self.active_request = Some(request.clone());
        Ok(request)
    }
    pub fn current_selected_node(
        &self,
        snapshot: &XmlSessionSnapshot,
        index: usize,
        node: crate::xml_toolkit_engine::XmlNodeId,
        request: &XmlXPathQueryRequest,
    ) -> bool {
        if self.selected_result != Some(index) || !self.is_current_request(snapshot, request) {
            return false;
        }
        match &snapshot.xpath {
            XmlXPathStatus::Nodes {
                request: published,
                nodes,
            } => published == request && nodes.get(index).is_some_and(|result| result.id == node),
            _ => false,
        }
    }
    pub fn navigate(
        &mut self,
        index: usize,
        snapshot: &XmlSessionSnapshot,
        tree: &mut XmlToolkitTreeViewState,
    ) -> Option<XmlToolkitTreeIntents> {
        let VisibleXPathStatus::Nodes(nodes) = self.visible_status(snapshot) else {
            return None;
        };
        self.navigate_to_result(index, nodes, tree)
    }
}

pub enum VisibleXPathStatus<'a> {
    Empty,
    NotRun,
    Pending,
    Nodes(&'a [XmlNodeSnapshot]),
    ScalarText(&'a str),
    Failed(&'a XmlEngineError),
}

#[derive(Clone, Copy)]
pub enum NavigationDirection {
    Previous,
    Next,
}

pub fn navigation_target(
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

pub fn result_label(index: usize, node: &XmlNodeSnapshot) -> String {
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

pub fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

pub(crate) fn cap_namespace_editor(rows: &mut [NamespaceBindingEditorRow]) -> bool {
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

pub fn xml_escaped_display(value: &str) -> String {
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
