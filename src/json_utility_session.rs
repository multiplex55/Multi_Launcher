//! Renderer-neutral JSON editor, projection worker, and tree/search state.
mod tree_api;
pub use tree_api::{TreeCopyKind, TreeCopyRequest, TreeDetails, TreeMatch, TreeRow, TreeStamp};
use crate::clipboard_modify::clipboard::{ClipboardBackend, ClipboardError};
use crate::json_transform::{JsonDocument, JsonNodePath, JsonPathSegment, JsonSearchFields, JsonSearchMatch, JsonTransformError, JsonTreeNodeId, JsonTreeProjection, JsonValueRef};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

pub(crate) const TREE_DETAIL_MAX_CHARS: usize = 4_096;
pub(crate) const TREE_KEY_MAX_CHARS: usize = 96;
pub(crate) const TREE_ROW_PREVIEW_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, Copy)]
pub enum Transform {
    Format,
    Minify,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum JsonUtilityView {
    #[default]
    Text,
    Tree,
}

#[derive(Debug)]
pub(crate) struct JsonTreeSnapshot {
    pub(crate) document: JsonDocument,
    pub(crate) projection: JsonTreeProjection,
}

impl JsonTreeSnapshot {
    pub(crate) fn parse(text: &str) -> Result<Self, JsonTransformError> {
        JsonDocument::parse(text).map(Self::from_document)
    }

    pub(crate) fn from_document(document: JsonDocument) -> Self {
        let projection = document.tree_projection();
        Self {
            document,
            projection,
        }
    }
}

#[derive(Debug)]
struct JsonSearchRequest {
    pub(crate) generation: u64,
    pub(crate) revision: u64,
    pub(crate) query: String,
    pub(crate) fields: JsonSearchFields,
    pub(crate) snapshot: Arc<JsonTreeSnapshot>,
}

#[derive(Debug)]
struct JsonSearchCompletion {
    pub(crate) generation: u64,
    pub(crate) revision: u64,
    pub(crate) matches: Vec<JsonSearchMatch>,
}

#[derive(Debug)]
enum JsonWorkerRequest {
    Projection(ProjectionRequest),
    Search(JsonSearchRequest),
}

#[derive(Debug)]
enum JsonWorkerCompletion {
    Projection(ProjectionCompletion),
    Search(JsonSearchCompletion),
}

impl JsonWorkerRequest {
    pub(crate) fn build(self) -> JsonWorkerCompletion {
        match self {
            Self::Projection(request) => JsonWorkerCompletion::Projection(request.build()),
            Self::Search(request) => JsonWorkerCompletion::Search(JsonSearchCompletion {
                generation: request.generation,
                revision: request.revision,
                matches: request
                    .snapshot
                    .document
                    .search(&request.query, request.fields),
            }),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProjectionSource {
    EditedText(String),
    InitialClipboard(String),
    ParsedDocument(JsonDocument),
}

#[derive(Debug)]
pub(crate) struct ProjectionRequest {
    pub(crate) generation: u64,
    pub(crate) source: ProjectionSource,
}

#[derive(Debug)]
pub(crate) struct ProjectionCompletion {
    pub(crate) generation: u64,
    pub(crate) initial_clipboard_text: Option<String>,
    pub(crate) result: Result<Arc<JsonTreeSnapshot>, JsonTransformError>,
}

impl ProjectionRequest {
    pub(crate) fn build(self) -> ProjectionCompletion {
        let (result, initial_clipboard_text) = match self.source {
            ProjectionSource::EditedText(text) => {
                (JsonTreeSnapshot::parse(&text).map(Arc::new), None)
            }
            ProjectionSource::InitialClipboard(text) => {
                (JsonTreeSnapshot::parse(&text).map(Arc::new), Some(text))
            }
            ProjectionSource::ParsedDocument(document) => (
                Ok(Arc::new(JsonTreeSnapshot::from_document(document))),
                None,
            ),
        };
        ProjectionCompletion {
            generation: self.generation,
            initial_clipboard_text,
            result,
        }
    }
}

#[derive(Debug, Default)]
struct TreeWorkerState {
    pub(crate) current_generation: u64,
    pub(crate) current_search_revision: Option<u64>,
    pub(crate) pending: Option<JsonWorkerRequest>,
    pub(crate) completed: Option<JsonWorkerCompletion>,
    pub(crate) shutdown: bool,
}

impl TreeWorkerState {
    pub(crate) fn queue(&mut self, request: JsonWorkerRequest) {
        match &request {
            JsonWorkerRequest::Projection(request) => {
                self.current_generation = request.generation;
                self.current_search_revision = None;
            }
            JsonWorkerRequest::Search(request) => {
                self.current_generation = request.generation;
                self.current_search_revision = Some(request.revision);
            }
        }
        self.completed = None;
        self.pending = Some(request);
    }

    pub(crate) fn invalidate(&mut self, generation: u64) {
        self.current_generation = generation;
        self.current_search_revision = None;
        self.pending = None;
        self.completed = None;
    }

    pub(crate) fn publish_if_current(&mut self, completion: JsonWorkerCompletion) -> bool {
        let is_current = match &completion {
            JsonWorkerCompletion::Projection(completion) => {
                self.current_generation == completion.generation
            }
            JsonWorkerCompletion::Search(completion) => {
                self.current_generation == completion.generation
                    && self.current_search_revision == Some(completion.revision)
            }
        };
        if self.shutdown || !is_current || self.pending.is_some() {
            return false;
        }
        self.completed = Some(completion);
        true
    }
}

/// One dialog-owned worker. It keeps at most one active and one coalesced
/// pending build, and publishes at most one completion back to the UI.
#[derive(Debug)]
struct JsonTreeWorker {
    state: Arc<(Mutex<TreeWorkerState>, Condvar)>,
}

impl JsonTreeWorker {
    pub(crate) fn new(notify: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        let state = Arc::new((Mutex::new(TreeWorkerState::default()), Condvar::new()));
        let worker_state = Arc::clone(&state);

        thread::Builder::new()
            .name("json-tree-projection".to_owned())
            .spawn(move || {
                loop {
                    let request = {
                        let (lock, wake) = &*worker_state;
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        while state.pending.is_none() && !state.shutdown {
                            state = wake.wait(state).unwrap_or_else(|error| error.into_inner());
                        }
                        if state.shutdown {
                            return;
                        }
                        let Some(request) = state.pending.take() else {
                            continue;
                        };
                        request
                    };

                    let completion = request.build();
                    let (lock, wake) = &*worker_state;
                    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                    if state.shutdown {
                        return;
                    }
                    let published = state.publish_if_current(completion);
                    drop(state);
                    if published {
                        notify();
                    }
                    wake.notify_one();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { state })
    }

    pub(crate) fn submit(&self, request: JsonWorkerRequest) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.queue(request);
        wake.notify_one();
    }

    pub(crate) fn invalidate(&self, generation: u64) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.invalidate(generation);
        wake.notify_one();
    }

    pub(crate) fn poll(&self) -> Option<JsonWorkerCompletion> {
        let (lock, _) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.completed.take()
    }
}

impl Drop for JsonTreeWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.shutdown = true;
        state.pending = None;
        state.completed = None;
        wake.notify_one();
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibleTreeRow {
    pub(crate) id: JsonTreeNodeId,
    pub(crate) depth: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedNodeDetails {
    pub(crate) path: String,
    pub(crate) kind: &'static str,
    pub(crate) value: String,
    pub(crate) value_truncated: bool,
}

#[derive(Debug, Default)]
pub(crate) struct JsonTreeSearchState {
    pub(crate) query: String,
    pub(crate) fields: JsonSearchFields,
    pub(crate) matches: Vec<JsonSearchMatch>,
    pub(crate) match_by_node: HashMap<JsonTreeNodeId, (usize, JsonSearchMatch)>,
    pub(crate) active_match: Option<usize>,
    pub(crate) revision: u64,
    pub(crate) requested: Option<(u64, u64)>,
    pub(crate) result_generation: Option<u64>,
}

impl JsonTreeSearchState {
    pub(crate) fn invalidate_results(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.matches.clear();
        self.match_by_node.clear();
        self.active_match = None;
        self.requested = None;
        self.result_generation = None;
    }

    pub(crate) fn reset(&mut self) {
        self.query.clear();
        self.fields = JsonSearchFields::Both;
        self.invalidate_results();
    }

    pub(crate) fn needs_search(&self, generation: u64) -> bool {
        !self.query.is_empty()
            && self.requested != Some((generation, self.revision))
            && self.result_generation != Some(generation)
    }

    pub(crate) fn mark_requested(&mut self, generation: u64) {
        self.requested = Some((generation, self.revision));
    }

    pub(crate) fn is_pending(&self, generation: u64) -> bool {
        self.requested == Some((generation, self.revision))
            && self.result_generation != Some(generation)
    }

    fn accept(&mut self, completion: JsonSearchCompletion) -> bool {
        if completion.revision != self.revision
            || self.requested != Some((completion.generation, completion.revision))
        {
            return false;
        }
        self.requested = None;
        self.result_generation = Some(completion.generation);
        self.matches = completion.matches;
        self.match_by_node = self
            .matches
            .iter()
            .copied()
            .enumerate()
            .map(|(index, matched)| (matched.node(), (index, matched)))
            .collect();
        self.active_match = (!self.matches.is_empty()).then_some(0);
        true
    }
}

#[derive(Debug, Default)]
pub(crate) struct JsonTreeViewState {
    pub(crate) view: JsonUtilityView,
    pub(crate) snapshot: Option<Arc<JsonTreeSnapshot>>,
    pub(crate) snapshot_generation: Option<u64>,
    pub(crate) selected: Option<JsonTreeNodeId>,
    pub(crate) selected_path: Option<JsonNodePath>,
    pub(crate) selected_path_restore: Option<JsonNodePath>,
    pub(crate) selected_details: Option<SelectedNodeDetails>,
    pub(crate) expanded: HashSet<JsonTreeNodeId>,
    pub(crate) expanded_paths_restore: Option<HashSet<JsonNodePath>>,
    pub(crate) visible_rows: Vec<VisibleTreeRow>,
    pub(crate) target_revision: u64,
    pub(crate) search: JsonTreeSearchState,
    pub(crate) pending_scroll_to: Option<JsonTreeNodeId>,
    #[cfg(test)]
    pub(crate) visible_rows_rebuilds: usize,
}

impl JsonTreeViewState {
    pub(crate) fn current_snapshot(&self, generation: u64) -> Option<&JsonTreeSnapshot> {
        self.snapshot
            .as_ref()
            .filter(|_| self.snapshot_generation == Some(generation))
            .map(Arc::as_ref)
    }

    pub(crate) fn current_snapshot_handle(&self, generation: u64) -> Option<Arc<JsonTreeSnapshot>> {
        self.snapshot
            .as_ref()
            .filter(|_| self.snapshot_generation == Some(generation))
            .cloned()
    }

    pub(crate) fn invalidate(&mut self, preserve_paths: bool) {
        if preserve_paths {
            if let Some(snapshot) = &self.snapshot {
                if self.selected_path_restore.is_none()
                    && let Some(selected) = self.selected
                {
                    self.selected_path_restore = snapshot.projection.path(selected);
                }
                let expanded_paths = self.expanded_paths_restore.get_or_insert_with(HashSet::new);
                for id in &self.expanded {
                    if let Some(path) = snapshot.projection.path(*id) {
                        expanded_paths.insert(path);
                    }
                }
            }
        } else {
            self.selected_path_restore = None;
            self.expanded_paths_restore = None;
        }
        self.snapshot = None;
        self.snapshot_generation = None;
        self.selected = None;
        self.selected_path = None;
        self.selected_details = None;
        self.expanded.clear();
        self.visible_rows.clear();
        self.search.invalidate_results();
        self.pending_scroll_to = None;
    }

    pub(crate) fn install(&mut self, generation: u64, snapshot: impl Into<Arc<JsonTreeSnapshot>>) {
        let snapshot = snapshot.into();
        let selection_restore = self.selected_path_restore.take();
        let expansion_restore = std::mem::take(&mut self.expanded_paths_restore);
        let root = snapshot.projection.root();
        let mut expanded = HashSet::new();
        if let Some(paths) = expansion_restore {
            for path in paths {
                if let Some(id) = node_id_for_path(&snapshot.projection, &path)
                    && snapshot
                        .projection
                        .node(id)
                        .is_some_and(|node| node.child_count() > 0)
                {
                    expanded.insert(id);
                }
            }
        } else {
            expanded.insert(root);
        }
        let selected = selection_restore
            .as_ref()
            .and_then(|path| node_id_for_path(&snapshot.projection, path))
            .or(Some(root));
        let selected_path = selected.and_then(|id| snapshot.projection.path(id));
        let details = selected.and_then(|id| selected_node_details(&snapshot, id));

        self.snapshot = Some(snapshot);
        self.snapshot_generation = Some(generation);
        self.expanded = expanded;
        self.selected = selected;
        self.selected_path = selected_path;
        self.selected_details = details;
        self.rebuild_visible_rows();
    }

    pub(crate) fn select(&mut self, id: JsonTreeNodeId) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Some(path) = snapshot.projection.path(id) else {
            return;
        };
        self.selected_details = selected_node_details(snapshot, id);
        self.selected = Some(id);
        self.selected_path = Some(path);
    }

    pub(crate) fn reveal_and_select(&mut self, id: JsonTreeNodeId) {
        let mut ancestor = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.projection.node(id))
            .and_then(|node| node.parent());
        let mut expanded_changed = false;
        while let Some(parent) = ancestor {
            let Some(node) = self
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.projection.node(parent))
            else {
                break;
            };
            if node.child_count() > 0 {
                expanded_changed |= self.expanded.insert(parent);
            }
            ancestor = node.parent();
        }
        if expanded_changed {
            self.rebuild_visible_rows();
        }
        self.select(id);
        self.pending_scroll_to = Some(id);
    }

    pub(crate) fn toggle_expanded(&mut self, id: JsonTreeNodeId) {
        let has_children = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.projection.node(id))
            .is_some_and(|node| node.child_count() > 0);
        if !has_children {
            return;
        }
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.rebuild_visible_rows();
    }

    pub(crate) fn set_branch_expanded(&mut self, root: JsonTreeNodeId, expanded: bool) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let mut stack = vec![root];
        let mut changed = false;
        while let Some(id) = stack.pop() {
            let Some(node) = snapshot.projection.node(id) else {
                continue;
            };
            if !node.children().is_empty() {
                changed |= if expanded {
                    self.expanded.insert(id)
                } else {
                    self.expanded.remove(&id)
                };
            }
            stack.extend(node.children().iter().copied());
        }
        if changed {
            self.rebuild_visible_rows();
        }
    }

    pub(crate) fn rebuild_visible_rows(&mut self) {
        self.target_revision = self.target_revision.wrapping_add(1);
        let Some(snapshot) = &self.snapshot else {
            self.visible_rows.clear();
            return;
        };
        let root = snapshot.projection.root();
        let mut rows = Vec::new();
        let mut stack = vec![(root, 0usize)];
        while let Some((id, depth)) = stack.pop() {
            let Some(node) = snapshot.projection.node(id) else {
                continue;
            };
            rows.push(VisibleTreeRow { id, depth });
            if self.expanded.contains(&id) {
                stack.extend(
                    node.children()
                        .iter()
                        .rev()
                        .copied()
                        .map(|child| (child, depth + 1)),
                );
            }
        }
        self.visible_rows = rows;
        #[cfg(test)]
        {
            self.visible_rows_rebuilds += 1;
        }
    }
}

pub(crate) fn node_id_for_path(
    projection: &JsonTreeProjection,
    path: &JsonNodePath,
) -> Option<JsonTreeNodeId> {
    let mut current = projection.root();
    for segment in path.segments() {
        let node = projection.node(current)?;
        current = node.children().iter().copied().find(|child| {
            projection
                .node(*child)
                .is_some_and(|candidate| candidate.segment() == Some(segment))
        })?;
    }
    Some(current)
}

pub(crate) fn selected_node_details(
    snapshot: &JsonTreeSnapshot,
    id: JsonTreeNodeId,
) -> Option<SelectedNodeDetails> {
    let path = snapshot.projection.path(id)?;
    let node = snapshot.projection.node(id)?;
    let (value, value_truncated) = match snapshot.document.resolve_path(&path)? {
        JsonValueRef::Null => ("null".to_owned(), false),
        JsonValueRef::Boolean(value) => (value.to_string(), false),
        JsonValueRef::Number(value) => (value.to_string(), false),
        JsonValueRef::String(value) => {
            let truncated = value.chars().count() > TREE_DETAIL_MAX_CHARS;
            (bounded_display(value, TREE_DETAIL_MAX_CHARS), truncated)
        }
        JsonValueRef::Array { len } => (format!("Array ({len} items)"), false),
        JsonValueRef::Object { len } => (format!("Object ({len} items)"), false),
    };
    Some(SelectedNodeDetails {
        path: bounded_display(&path.display_path(), TREE_DETAIL_MAX_CHARS),
        kind: node_kind_label(node.kind()),
        value,
        value_truncated,
    })
}

pub(crate) fn bounded_display(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

pub(crate) fn tree_row_preview(value: &str) -> String {
    let mut result = String::new();
    let mut result_chars = 0;
    for character in value.chars() {
        let escaped = character.escape_debug().to_string();
        let escaped_chars = escaped.chars().count();
        if result_chars + escaped_chars > TREE_ROW_PREVIEW_MAX_CHARS {
            result.push('…');
            break;
        }
        result.push_str(&escaped);
        result_chars += escaped_chars;
    }
    result
}

pub(crate) fn node_kind_label(kind: crate::json_transform::JsonNodeKind) -> &'static str {
    use crate::json_transform::JsonNodeKind;
    match kind {
        JsonNodeKind::Null => "Null",
        JsonNodeKind::Boolean => "Boolean",
        JsonNodeKind::Number => "Number",
        JsonNodeKind::String => "String",
        JsonNodeKind::Array => "Array",
        JsonNodeKind::Object => "Object",
    }
}

pub(crate) fn node_segment_label(segment: Option<&JsonPathSegment>) -> String {
    match segment {
        None => "(root)".to_owned(),
        Some(JsonPathSegment::ArrayIndex(index)) => format!("[{index}]"),
        Some(JsonPathSegment::ObjectKey(key)) => {
            let key = bounded_display(key, TREE_KEY_MAX_CHARS);
            format!("{key:?}")
        }
    }
}

#[derive(Debug, Default)]
pub struct JsonEditorState {
    pub(crate) text: String,
    pub(crate) parse_error: Option<JsonTransformError>,
    pub(crate) feedback: Option<Result<String, String>>,
    pub(crate) generation: u64,
    pub(crate) tree: JsonTreeViewState,
    worker: Option<JsonTreeWorker>,
    pub(crate) queued_projection: Option<ProjectionSource>,
    pub(crate) requested_generation: Option<u64>,
    pub(crate) clipboard_load_generation: Option<u64>,
    pub(crate) tree_worker_error: Option<String>,
    #[cfg(test)]
    pub(crate) projection_builds_started: usize,
    #[cfg(test)]
    pub(crate) search_builds_started: usize,
}

impl JsonEditorState {
    /// Replace the authored source and invalidate its derived state atomically.
    pub fn set_source(&mut self, text: String) {
        if self.text != text {
            self.text = text;
            self.source_edited();
        }
    }

    pub fn source(&self) -> &str { &self.text }
    pub fn parse_error(&self) -> Option<&JsonTransformError> { self.parse_error.as_ref() }
    pub fn feedback(&self) -> Option<&Result<String, String>> { self.feedback.as_ref() }
    pub fn worker_error(&self) -> Option<&str> { self.tree_worker_error.as_deref() }
    pub fn loading_clipboard(&self) -> bool { self.is_loading_clipboard() }

    pub fn initialize(&mut self, clipboard: &impl ClipboardBackend) {
        self.begin_initialize();
        self.finish_initialize(clipboard.read_text());
    }

    pub fn begin_initialize(&mut self) {
        self.reset_feedback();
        self.text.clear();
        self.tree.search.reset();
        self.invalidate_source(false);
    }

    /// Retire a stale initial clipboard projection without replacing Tree search controls.
    pub fn cancel_initial_clipboard(&mut self) {
        self.invalidate_source(false);
    }

    pub fn finish_initialize(&mut self, result: Result<String, ClipboardError>) {
        match result {
            Ok(text) if !text.trim().is_empty() => {
                self.clipboard_load_generation = Some(self.generation);
                self.queued_projection = Some(ProjectionSource::InitialClipboard(text));
            }
            _ => {}
        }
    }

    pub(crate) fn reset_feedback(&mut self) {
        self.parse_error = None;
        self.feedback = None;
    }

    pub(crate) fn is_loading_clipboard(&self) -> bool {
        self.clipboard_load_generation == Some(self.generation)
    }

    pub(crate) fn invalidate_source(&mut self, preserve_paths: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.tree.invalidate(preserve_paths);
        self.queued_projection = None;
        self.requested_generation = None;
        self.clipboard_load_generation = None;
        self.tree_worker_error = None;
        if let Some(worker) = &self.worker {
            worker.invalidate(self.generation);
        }
    }

    pub(crate) fn source_edited(&mut self) {
        self.reset_feedback();
        self.invalidate_source(true);
        if !self.text.is_empty() {
            self.queued_projection = Some(ProjectionSource::EditedText(self.text.clone()));
        }
    }

    pub fn clear(&mut self) {
        self.reset_feedback();
        self.text.clear();
        self.invalidate_source(false);
    }

    pub fn reload(&mut self, clipboard: &impl ClipboardBackend) {
        self.finish_reload(clipboard.read_text());
    }

    pub fn finish_reload(&mut self, result: Result<String, ClipboardError>) {
        self.reset_feedback();
        match result {
            Ok(text) => {
                self.text = text.clone();
                self.invalidate_source(false);
                if !text.is_empty() {
                    self.queued_projection = Some(ProjectionSource::EditedText(text));
                }
            }
            Err(error) => {
                self.feedback = Some(Err(format!("Could not read clipboard: {error}")));
            }
        }
    }

    pub fn transform(&mut self, transform: Transform) {
        self.reset_feedback();
        let document = match JsonDocument::parse(&self.text) {
            Ok(document) => document,
            Err(error) => {
                self.parse_error = Some(error);
                return;
            }
        };
        let result = match transform {
            Transform::Format => document.pretty(),
            Transform::Minify => document.minify(),
        };
        match result {
            Ok(text) => {
                if text != self.text {
                    self.text = text;
                    self.invalidate_source(true);
                    self.queued_projection = Some(ProjectionSource::ParsedDocument(document));
                }
            }
            Err(error) => self.parse_error = Some(error),
        }
    }

    pub fn copy(&mut self, clipboard: &impl ClipboardBackend) {
        self.reset_feedback();
        let is_currently_valid = self.tree.current_snapshot(self.generation).is_some();
        if !is_currently_valid && let Err(error) = JsonDocument::parse(&self.text) {
            self.parse_error = Some(error);
            return;
        }
        self.feedback = Some(
            clipboard
                .write_text(&self.text)
                .map(|()| "Copied JSON to clipboard".to_owned())
                .map_err(|error| format!("Could not copy JSON: {error}")),
        );
    }

    pub fn accept_copy_feedback(&mut self, completed: &Self) {
        self.parse_error = completed.parse_error.clone();
        self.feedback = completed.feedback.clone();
    }

    pub(crate) fn copy_tree_value(&mut self, clipboard: &impl ClipboardBackend, subtree: bool) {
        self.copy_tree_with(clipboard, |document, path| {
            if subtree {
                document.copy_subtree(path)
            } else {
                document.copy_value(path)
            }
        });
    }

    pub(crate) fn copy_tree_path(&mut self, clipboard: &impl ClipboardBackend) {
        self.copy_tree_with(clipboard, |_, path| Ok(Some(path.to_json_pointer())));
    }

    pub(crate) fn copy_tree_with(
        &mut self,
        clipboard: &impl ClipboardBackend,
        make_text: impl FnOnce(
            &JsonDocument,
            &JsonNodePath,
        ) -> Result<Option<String>, JsonTransformError>,
    ) {
        let Some(snapshot) = self.tree.current_snapshot(self.generation) else {
            return;
        };
        let Some(id) = self.tree.selected else {
            return;
        };
        let Some(path) = snapshot.projection.path(id) else {
            return;
        };
        let text = match make_text(&snapshot.document, &path) {
            Ok(Some(text)) => text,
            Ok(None) => return,
            Err(error) => {
                self.feedback = Some(Err(format!("Could not copy JSON node: {error}")));
                return;
            }
        };
        self.feedback = Some(
            clipboard
                .write_text(&text)
                .map(|()| "Copied JSON node to clipboard".to_owned())
                .map_err(|error| format!("Could not copy JSON node: {error}")),
        );
    }

    pub(crate) fn move_search_match(&mut self, previous: bool) {
        let count = self.tree.search.matches.len();
        if count == 0 {
            return;
        }
        let active = self.tree.search.active_match;
        let next = match (active, previous) {
            (Some(index), true) => (index + count - 1) % count,
            (Some(index), false) => (index + 1) % count,
            (None, true) => count - 1,
            (None, false) => 0,
        };
        self.tree.search.active_match = Some(next);
        if let Some(node) = self
            .tree
            .search
            .matches
            .get(next)
            .map(JsonSearchMatch::node)
        {
            self.tree.reveal_and_select(node);
        }
    }

    pub(crate) fn queue_current_text_if_needed(&mut self) {
        if self.queued_projection.is_none()
            && self.requested_generation != Some(self.generation)
            && self.tree.current_snapshot(self.generation).is_none()
            && self.parse_error.is_none()
            && self.clipboard_load_generation != Some(self.generation)
            && !self.text.is_empty()
        {
            self.queued_projection = Some(ProjectionSource::EditedText(self.text.clone()));
        }
    }

    pub fn service_projection(&mut self, notify: impl Fn() -> Arc<dyn Fn() + Send + Sync>) {
        if let Some(completion) = self.worker.as_ref().and_then(JsonTreeWorker::poll) {
            self.accept_worker_completion(completion);
        }
        self.queue_current_text_if_needed();
        if let Some(source) = self.queued_projection.take() {
            if self.worker.is_none() {
                match JsonTreeWorker::new(notify()) {
                    Ok(worker) => self.worker = Some(worker),
                    Err(error) => {
                        self.tree_worker_error =
                            Some(format!("Could not start JSON tree worker: {error}"));
                        self.requested_generation = Some(self.generation);
                        self.clipboard_load_generation = None;
                        return;
                    }
                }
            }
            let is_initial_clipboard = matches!(&source, ProjectionSource::InitialClipboard(_));
            if is_initial_clipboard {
                self.clipboard_load_generation = Some(self.generation);
            }
            self.requested_generation = Some(self.generation);
            if let Some(worker) = &self.worker {
                #[cfg(test)]
                {
                    self.projection_builds_started += 1;
                }
                worker.submit(JsonWorkerRequest::Projection(ProjectionRequest {
                    generation: self.generation,
                    source,
                }));
            }
        }
        self.queue_search_if_needed(&notify);
    }

    fn queue_search_if_needed(&mut self, notify: &impl Fn() -> Arc<dyn Fn() + Send + Sync>) {
        let generation = self.generation;
        let Some(snapshot) = self.tree.current_snapshot_handle(generation) else {
            return;
        };
        if !self.tree.search.needs_search(generation) {
            return;
        }
        if self.worker.is_none() {
            match JsonTreeWorker::new(notify()) {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.tree_worker_error =
                        Some(format!("Could not start JSON search worker: {error}"));
                    return;
                }
            }
        }
        let request = JsonSearchRequest {
            generation,
            revision: self.tree.search.revision,
            query: self.tree.search.query.clone(),
            fields: self.tree.search.fields,
            snapshot,
        };
        self.tree.search.mark_requested(generation);
        if let Some(worker) = &self.worker {
            #[cfg(test)]
            {
                self.search_builds_started += 1;
            }
            worker.submit(JsonWorkerRequest::Search(request));
        }
    }

    fn accept_worker_completion(&mut self, completion: JsonWorkerCompletion) {
        match completion {
            JsonWorkerCompletion::Projection(completion) => self.accept_completion(completion),
            JsonWorkerCompletion::Search(completion) => {
                if self.tree.search.accept(completion)
                    && let Some(index) = self.tree.search.active_match
                    && let Some(node) = self
                        .tree
                        .search
                        .matches
                        .get(index)
                        .map(JsonSearchMatch::node)
                {
                    self.tree.reveal_and_select(node);
                }
            }
        }
    }

    pub(crate) fn accept_completion(&mut self, completion: ProjectionCompletion) {
        if completion.generation != self.generation {
            return;
        }
        let is_initial_clipboard = completion.initial_clipboard_text.is_some()
            || self.clipboard_load_generation == Some(self.generation);
        self.clipboard_load_generation = None;
        self.tree_worker_error = None;
        match completion.result {
            Ok(snapshot) => {
                if let Some(text) = completion.initial_clipboard_text {
                    self.text = text;
                }
                self.parse_error = None;
                self.tree.install(self.generation, snapshot);
            }
            Err(_error) if is_initial_clipboard => {
                self.text.clear();
                self.parse_error = None;
                self.tree.invalidate(false);
            }
            Err(error) => {
                self.parse_error = Some(error);
                self.tree.invalidate(true);
            }
        }
    }

    pub fn close(&mut self) {
        self.reset_feedback();
        self.invalidate_source(false);
    }

    #[cfg(test)]
    pub(crate) fn complete_queued_projection_for_test(&mut self) {
        if let Some(source) = self.queued_projection.take() {
            self.requested_generation = Some(self.generation);
            self.projection_builds_started += 1;
            let completion = ProjectionRequest {
                generation: self.generation,
                source,
            }
            .build();
            self.accept_completion(completion);
        }
    }

    #[cfg(test)]
    pub(crate) fn complete_search_for_test(&mut self) {
        let generation = self.generation;
        let Some(snapshot) = self.tree.current_snapshot_handle(generation) else {
            return;
        };
        if !self.tree.search.needs_search(generation) {
            return;
        }
        let request = JsonWorkerRequest::Search(JsonSearchRequest {
            generation,
            revision: self.tree.search.revision,
            query: self.tree.search.query.clone(),
            fields: self.tree.search.fields,
            snapshot,
        });
        self.tree.search.mark_requested(generation);
        self.search_builds_started += 1;
        self.accept_worker_completion(request.build());
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonEditorState, JsonTreeWorker, JsonWorkerCompletion, JsonWorkerRequest, ProjectionRequest, ProjectionSource, TreeWorkerState};
    use crate::json_transform::JsonSearchFields;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[test]
    fn source_updates_are_atomic_and_idle_service_does_not_make_callback() {
        let mut editor = JsonEditorState::default();
        editor.service_projection(|| panic!("idle session must not create a callback"));
        editor.set_source("{}".into());
        let generation = editor.generation;
        assert_eq!(editor.source(), "{}");
        editor.set_source("{}".into());
        assert_eq!(editor.generation, generation);
        editor.set_source("[]".into());
        assert_eq!(editor.generation, generation.wrapping_add(1));
    }

    #[test]
    fn cancelling_initial_clipboard_preserves_tree_search_controls() {
        let mut editor = JsonEditorState::default();
        editor.begin_initialize();
        editor.finish_initialize(Ok("{}".into()));
        editor.tree_set_search("needle".into(), JsonSearchFields::Keys);
        editor.cancel_initial_clipboard();
        assert_eq!(editor.tree_search_query(), "needle");
        assert_eq!(editor.tree_search_fields(), JsonSearchFields::Keys);
        assert!(!editor.loading_clipboard());
    }

    #[test]
    fn worker_discards_stale_results_and_coalesces_pending_source() {
        let mut state = TreeWorkerState::default();
        state.queue(JsonWorkerRequest::Projection(ProjectionRequest {
            generation: 1,
            source: ProjectionSource::EditedText("[]".to_owned()),
        }));
        state.queue(JsonWorkerRequest::Projection(ProjectionRequest {
            generation: 2,
            source: ProjectionSource::EditedText("{}".to_owned()),
        }));
        assert!(matches!(
            state.pending.as_ref().unwrap(),
            JsonWorkerRequest::Projection(ProjectionRequest { generation: 2, .. })
        ));
        let stale = JsonWorkerRequest::Projection(ProjectionRequest {
            generation: 1,
            source: ProjectionSource::EditedText("[]".to_owned()),
        })
        .build();
        assert!(!state.publish_if_current(stale));
        assert!(state.completed.is_none());
        assert!(matches!(
            state.pending.as_ref().unwrap(),
            JsonWorkerRequest::Projection(ProjectionRequest { generation: 2, .. })
        ));
        state.invalidate(3);
        let stale_after_close = JsonWorkerRequest::Projection(ProjectionRequest {
            generation: 2,
            source: ProjectionSource::EditedText("{}".to_owned()),
        })
        .build();
        assert!(!state.publish_if_current(stale_after_close));
        assert!(state.pending.is_none());
    }

    #[test]
    fn projection_completion_notifies_supplied_frontend() {
        let (sender, receiver) = mpsc::channel();
        let worker = JsonTreeWorker::new(Arc::new(move || { let _ = sender.send(()); })).unwrap();
        worker.submit(JsonWorkerRequest::Projection(ProjectionRequest {
            generation: 1,
            source: ProjectionSource::EditedText("{}".into()),
        }));
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(worker.poll(), Some(JsonWorkerCompletion::Projection(completion)) if completion.generation == 1 && completion.result.is_ok()));
    }
}
