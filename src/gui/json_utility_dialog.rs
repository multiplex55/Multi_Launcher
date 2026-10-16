use crate::clipboard_modify::clipboard::{ArboardClipboardBackend, ClipboardBackend};
use crate::commands::JsonUtilityIntent;
use crate::json_transform::{
    JsonDocument, JsonNodePath, JsonPathSegment, JsonSearchFields, JsonSearchMatch,
    JsonTransformError, JsonTreeNodeId, JsonTreeProjection, JsonValueRef,
};
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

const TREE_DETAIL_MAX_CHARS: usize = 4_096;
const TREE_KEY_MAX_CHARS: usize = 96;
const TREE_ROW_PREVIEW_MAX_CHARS: usize = 120;
const TREE_DETAILS_PANEL_MAX_HEIGHT: f32 = 96.0;

#[derive(Debug, Clone, Copy)]
enum Transform {
    Format,
    Minify,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum JsonUtilityView {
    #[default]
    Text,
    Tree,
}

#[derive(Debug)]
struct JsonTreeSnapshot {
    document: JsonDocument,
    projection: JsonTreeProjection,
}

impl JsonTreeSnapshot {
    fn parse(text: &str) -> Result<Self, JsonTransformError> {
        JsonDocument::parse(text).map(Self::from_document)
    }

    fn from_document(document: JsonDocument) -> Self {
        let projection = document.tree_projection();
        Self {
            document,
            projection,
        }
    }
}

#[derive(Debug)]
struct JsonSearchRequest {
    generation: u64,
    revision: u64,
    query: String,
    fields: JsonSearchFields,
    snapshot: Arc<JsonTreeSnapshot>,
}

#[derive(Debug)]
struct JsonSearchCompletion {
    generation: u64,
    revision: u64,
    matches: Vec<JsonSearchMatch>,
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
    fn build(self) -> JsonWorkerCompletion {
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
enum ProjectionSource {
    EditedText(String),
    InitialClipboard(String),
    ParsedDocument(JsonDocument),
}

#[derive(Debug)]
struct ProjectionRequest {
    generation: u64,
    source: ProjectionSource,
}

#[derive(Debug)]
struct ProjectionCompletion {
    generation: u64,
    initial_clipboard_text: Option<String>,
    result: Result<Arc<JsonTreeSnapshot>, JsonTransformError>,
}

impl ProjectionRequest {
    fn build(self) -> ProjectionCompletion {
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
    current_generation: u64,
    current_search_revision: Option<u64>,
    pending: Option<JsonWorkerRequest>,
    completed: Option<JsonWorkerCompletion>,
    shutdown: bool,
}

impl TreeWorkerState {
    fn queue(&mut self, request: JsonWorkerRequest) {
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

    fn invalidate(&mut self, generation: u64) {
        self.current_generation = generation;
        self.current_search_revision = None;
        self.pending = None;
        self.completed = None;
    }

    fn publish_if_current(&mut self, completion: JsonWorkerCompletion) -> bool {
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
    fn new(ctx: &egui::Context) -> Result<Self, String> {
        let state = Arc::new((Mutex::new(TreeWorkerState::default()), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let repaint_ctx = ctx.clone();
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
                        repaint_ctx.request_repaint();
                    }
                    wake.notify_one();
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { state })
    }

    fn submit(&self, request: JsonWorkerRequest) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.queue(request);
        wake.notify_one();
    }

    fn invalidate(&self, generation: u64) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.invalidate(generation);
        wake.notify_one();
    }

    fn poll(&self) -> Option<JsonWorkerCompletion> {
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
struct VisibleTreeRow {
    id: JsonTreeNodeId,
    depth: usize,
}

#[derive(Debug, Clone)]
struct SelectedNodeDetails {
    path: String,
    kind: &'static str,
    value: String,
    value_truncated: bool,
}

#[derive(Debug, Default)]
struct JsonTreeSearchState {
    query: String,
    fields: JsonSearchFields,
    matches: Vec<JsonSearchMatch>,
    match_by_node: HashMap<JsonTreeNodeId, (usize, JsonSearchMatch)>,
    active_match: Option<usize>,
    revision: u64,
    requested: Option<(u64, u64)>,
    result_generation: Option<u64>,
}

impl JsonTreeSearchState {
    fn invalidate_results(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.matches.clear();
        self.match_by_node.clear();
        self.active_match = None;
        self.requested = None;
        self.result_generation = None;
    }

    fn reset(&mut self) {
        self.query.clear();
        self.fields = JsonSearchFields::Both;
        self.invalidate_results();
    }

    fn needs_search(&self, generation: u64) -> bool {
        !self.query.is_empty()
            && self.requested != Some((generation, self.revision))
            && self.result_generation != Some(generation)
    }

    fn mark_requested(&mut self, generation: u64) {
        self.requested = Some((generation, self.revision));
    }

    fn is_pending(&self, generation: u64) -> bool {
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
struct JsonTreeViewState {
    view: JsonUtilityView,
    snapshot: Option<Arc<JsonTreeSnapshot>>,
    snapshot_generation: Option<u64>,
    selected: Option<JsonTreeNodeId>,
    selected_path: Option<JsonNodePath>,
    selected_path_restore: Option<JsonNodePath>,
    selected_details: Option<SelectedNodeDetails>,
    expanded: HashSet<JsonTreeNodeId>,
    expanded_paths_restore: Option<HashSet<JsonNodePath>>,
    visible_rows: Vec<VisibleTreeRow>,
    search: JsonTreeSearchState,
    pending_scroll_to: Option<JsonTreeNodeId>,
    #[cfg(test)]
    visible_rows_rebuilds: usize,
}

impl JsonTreeViewState {
    fn current_snapshot(&self, generation: u64) -> Option<&JsonTreeSnapshot> {
        self.snapshot
            .as_ref()
            .filter(|_| self.snapshot_generation == Some(generation))
            .map(Arc::as_ref)
    }

    fn current_snapshot_handle(&self, generation: u64) -> Option<Arc<JsonTreeSnapshot>> {
        self.snapshot
            .as_ref()
            .filter(|_| self.snapshot_generation == Some(generation))
            .cloned()
    }

    fn invalidate(&mut self, preserve_paths: bool) {
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

    fn install(&mut self, generation: u64, snapshot: impl Into<Arc<JsonTreeSnapshot>>) {
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

    fn select(&mut self, id: JsonTreeNodeId) {
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

    fn reveal_and_select(&mut self, id: JsonTreeNodeId) {
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

    fn toggle_expanded(&mut self, id: JsonTreeNodeId) {
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

    fn set_branch_expanded(&mut self, root: JsonTreeNodeId, expanded: bool) {
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

    fn rebuild_visible_rows(&mut self) {
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

fn node_id_for_path(
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

fn selected_node_details(
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

fn bounded_display(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

fn tree_row_preview(value: &str) -> String {
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

fn tree_panel_heights(available_height: f32, spacing: f32) -> (f32, f32) {
    let available_height = available_height.max(0.0);
    let details_height = (available_height * 0.3)
        .clamp(48.0, TREE_DETAILS_PANEL_MAX_HEIGHT)
        .min(available_height);
    let tree_height = (available_height - details_height - spacing.max(0.0) - 1.0).max(0.0);
    (tree_height, details_height)
}

fn tree_search_scroll_offset(
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

fn node_kind_label(kind: crate::json_transform::JsonNodeKind) -> &'static str {
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

fn node_segment_label(segment: Option<&JsonPathSegment>) -> String {
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
struct JsonEditorState {
    text: String,
    parse_error: Option<JsonTransformError>,
    feedback: Option<Result<String, String>>,
    generation: u64,
    tree: JsonTreeViewState,
    worker: Option<JsonTreeWorker>,
    queued_projection: Option<ProjectionSource>,
    requested_generation: Option<u64>,
    clipboard_load_generation: Option<u64>,
    tree_worker_error: Option<String>,
    #[cfg(test)]
    projection_builds_started: usize,
    #[cfg(test)]
    search_builds_started: usize,
}

impl JsonEditorState {
    fn initialize(&mut self, clipboard: &impl ClipboardBackend) {
        self.reset_feedback();
        self.text.clear();
        self.tree.search.reset();
        self.invalidate_source(false);
        match clipboard.read_text() {
            Ok(text) if !text.trim().is_empty() => {
                self.clipboard_load_generation = Some(self.generation);
                self.queued_projection = Some(ProjectionSource::InitialClipboard(text));
            }
            _ => {}
        }
    }

    fn reset_feedback(&mut self) {
        self.parse_error = None;
        self.feedback = None;
    }

    fn is_loading_clipboard(&self) -> bool {
        self.clipboard_load_generation == Some(self.generation)
    }

    fn invalidate_source(&mut self, preserve_paths: bool) {
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

    fn source_edited(&mut self) {
        self.reset_feedback();
        self.invalidate_source(true);
        if !self.text.is_empty() {
            self.queued_projection = Some(ProjectionSource::EditedText(self.text.clone()));
        }
    }

    fn clear(&mut self) {
        self.reset_feedback();
        self.text.clear();
        self.invalidate_source(false);
    }

    fn reload(&mut self, clipboard: &impl ClipboardBackend) {
        self.reset_feedback();
        match clipboard.read_text() {
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

    fn transform(&mut self, transform: Transform) {
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

    fn copy(&mut self, clipboard: &impl ClipboardBackend) {
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

    fn copy_tree_value(&mut self, clipboard: &impl ClipboardBackend, subtree: bool) {
        self.copy_tree_with(clipboard, |document, path| {
            if subtree {
                document.copy_subtree(path)
            } else {
                document.copy_value(path)
            }
        });
    }

    fn copy_tree_path(&mut self, clipboard: &impl ClipboardBackend) {
        self.copy_tree_with(clipboard, |_, path| Ok(Some(path.to_json_pointer())));
    }

    fn copy_tree_with(
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

    fn move_search_match(&mut self, previous: bool) {
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

    fn queue_current_text_if_needed(&mut self) {
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

    fn service_projection(&mut self, ctx: &egui::Context) {
        if let Some(completion) = self.worker.as_ref().and_then(JsonTreeWorker::poll) {
            self.accept_worker_completion(completion);
        }
        self.queue_current_text_if_needed();
        if let Some(source) = self.queued_projection.take() {
            if self.worker.is_none() {
                match JsonTreeWorker::new(ctx) {
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
        self.queue_search_if_needed(ctx);
    }

    fn queue_search_if_needed(&mut self, ctx: &egui::Context) {
        let generation = self.generation;
        let Some(snapshot) = self.tree.current_snapshot_handle(generation) else {
            return;
        };
        if !self.tree.search.needs_search(generation) {
            return;
        }
        if self.worker.is_none() {
            match JsonTreeWorker::new(ctx) {
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

    fn accept_completion(&mut self, completion: ProjectionCompletion) {
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

    fn close(&mut self) {
        self.reset_feedback();
        self.invalidate_source(false);
    }

    #[cfg(test)]
    fn complete_queued_projection_for_test(&mut self) {
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
    fn complete_search_for_test(&mut self) {
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

#[derive(Debug)]
pub struct JsonUtilityDialogState {
    pub open: bool,
    pub intent: JsonUtilityIntent,
    editor: JsonEditorState,
    focus_editor: bool,
    #[cfg(test)]
    tree_panel_layout: Option<(f32, f32, f32, f32)>,
    #[cfg(test)]
    rendered_tree_rows: usize,
}

impl Default for JsonUtilityDialogState {
    fn default() -> Self {
        Self {
            open: false,
            intent: JsonUtilityIntent::General,
            editor: JsonEditorState::default(),
            focus_editor: false,
            #[cfg(test)]
            tree_panel_layout: None,
            #[cfg(test)]
            rendered_tree_rows: 0,
        }
    }
}

impl JsonUtilityDialogState {
    pub fn open(&mut self, intent: JsonUtilityIntent) {
        self.open_with_clipboard(intent, &ArboardClipboardBackend);
    }

    fn open_with_clipboard(
        &mut self,
        intent: JsonUtilityIntent,
        clipboard: &impl ClipboardBackend,
    ) {
        self.intent = intent;
        self.editor.initialize(clipboard);
        self.focus_editor = true;
        self.open = true;
    }

    pub(crate) fn reopen_existing(&mut self) {
        self.open = true;
        self.editor.queue_current_text_if_needed();
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.focus_editor = false;
        self.editor.close();
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        self.editor.service_projection(ctx);
        let mut open = self.open;
        egui::Window::new("JSON Utility")
            .id(egui::Id::new("json_utility_dialog"))
            .collapsible(false)
            .resizable(true)
            .default_size((700.0, 480.0))
            .min_size((340.0, 220.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let actions = if self.intent == JsonUtilityIntent::Minify {
                        [("Minify", Transform::Minify), ("Format", Transform::Format)]
                    } else {
                        [("Format", Transform::Format), ("Minify", Transform::Minify)]
                    };
                    for (label, transform) in actions {
                        if ui
                            .add_enabled(
                                !self.editor.is_loading_clipboard(),
                                egui::Button::new(label),
                            )
                            .clicked()
                        {
                            self.editor.transform(transform);
                        }
                    }
                    if ui
                        .add_enabled(
                            !self.editor.is_loading_clipboard(),
                            egui::Button::new("Copy Result"),
                        )
                        .clicked()
                    {
                        self.editor.copy(&ArboardClipboardBackend);
                    }
                    if ui.button("Paste from Clipboard").clicked() {
                        self.editor.reload(&ArboardClipboardBackend);
                    }
                    if ui.button("Clear").clicked() {
                        self.editor.clear();
                        self.focus_editor = true;
                    }
                });
                if let Some(error) = &self.editor.parse_error {
                    ui.colored_label(ui.visuals().error_fg_color, &error.message);
                } else if let Some(error) = &self.editor.tree_worker_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if let Some(feedback) = &self.editor.feedback {
                    match feedback {
                        Ok(message) => {
                            ui.label(message);
                        }
                        Err(message) => {
                            ui.colored_label(ui.visuals().error_fg_color, message);
                        }
                    }
                } else if self.editor.is_loading_clipboard() {
                    ui.weak("Checking clipboard JSON…");
                } else {
                    ui.weak("Strict JSON · Copy Result writes only when requested");
                }

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.editor.tree.view, JsonUtilityView::Text, "Text");
                    ui.selectable_value(&mut self.editor.tree.view, JsonUtilityView::Tree, "Tree");
                });
                ui.separator();
                match self.editor.tree.view {
                    JsonUtilityView::Text => {
                        let editor_size = ui.available_size();
                        egui::ScrollArea::both()
                            .id_salt("json_utility_editor_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let response = ui.add_sized(
                                    editor_size,
                                    egui::TextEdit::multiline(&mut self.editor.text)
                                        .id(egui::Id::new("json_utility_editor"))
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(12)
                                        .hint_text("Paste or type JSON here"),
                                );
                                if self.focus_editor {
                                    response.request_focus();
                                    self.focus_editor = false;
                                }
                                if response.changed() {
                                    self.editor.source_edited();
                                }
                            });
                    }
                    JsonUtilityView::Tree => self.show_tree(ui),
                }
            });
        self.open = open;
        if self.open {
            self.editor.service_projection(ctx);
        } else {
            self.close();
        }
    }

    fn show_tree(&mut self, ui: &mut egui::Ui) {
        #[cfg(test)]
        {
            self.tree_panel_layout = None;
            self.rendered_tree_rows = 0;
        }
        let generation = self.editor.generation;
        let ready = self.editor.tree.current_snapshot(generation).is_some();
        let mut search_inputs_changed = false;
        let mut previous_match = false;
        let mut next_match = false;
        ui.horizontal_wrapped(|ui| {
            ui.label("Search");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.editor.tree.search.query)
                    .desired_width(220.0)
                    .hint_text("Keys or values"),
            );
            search_inputs_changed |= response.changed();
            if ui.small_button("Clear search").clicked() {
                self.editor.tree.search.query.clear();
                search_inputs_changed = true;
            }
            let fields = &mut self.editor.tree.search.fields;
            let previous_fields = *fields;
            ui.selectable_value(fields, JsonSearchFields::Keys, "Keys");
            ui.selectable_value(fields, JsonSearchFields::Values, "Values");
            ui.selectable_value(fields, JsonSearchFields::Both, "Both");
            search_inputs_changed |= *fields != previous_fields;

            let search = &self.editor.tree.search;
            let pending = search.is_pending(generation);
            let has_matches = !search.matches.is_empty();
            if ui
                .add_enabled(
                    ready && has_matches && !pending,
                    egui::Button::new("Previous"),
                )
                .clicked()
            {
                previous_match = true;
            }
            if ui
                .add_enabled(ready && has_matches && !pending, egui::Button::new("Next"))
                .clicked()
            {
                next_match = true;
            }
            if pending {
                ui.weak("Searching…");
            } else if let Some(index) = search.active_match {
                ui.label(format!("{} / {} matches", index + 1, search.matches.len()));
            } else {
                ui.label(format!("{} matches", search.matches.len()));
            }
        });
        if search_inputs_changed {
            self.editor.tree.search.invalidate_results();
        } else if previous_match {
            self.editor.move_search_match(true);
        } else if next_match {
            self.editor.move_search_match(false);
        }
        ui.weak("Literal and case-sensitive · keys are decoded object names · values are decoded strings and scalar JSON text");

        let selected = self.editor.tree.selected;
        let mut copy_action = None;
        let mut branch_action = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    ready && selected.is_some(),
                    egui::Button::new("Expand Branch"),
                )
                .clicked()
            {
                branch_action = selected.map(|id| (id, true));
            }
            if ui
                .add_enabled(
                    ready && selected.is_some(),
                    egui::Button::new("Collapse Branch"),
                )
                .clicked()
            {
                branch_action = selected.map(|id| (id, false));
            }
            if ui
                .add_enabled(ready && selected.is_some(), egui::Button::new("Copy Value"))
                .clicked()
            {
                copy_action = Some(TreeCopyAction::Value);
            }
            if ui
                .add_enabled(
                    ready && selected.is_some(),
                    egui::Button::new("Copy Subtree"),
                )
                .clicked()
            {
                copy_action = Some(TreeCopyAction::Subtree);
            }
            if ui
                .add_enabled(
                    ready && selected.is_some(),
                    egui::Button::new("Copy JSON Pointer"),
                )
                .clicked()
            {
                copy_action = Some(TreeCopyAction::Path);
            }
        });

        let mut select_action = None;
        let mut toggle_action = None;
        let (tree_height, details_height) =
            tree_panel_heights(ui.available_height(), ui.spacing().item_spacing.y);
        let scroll_target = self.editor.tree.pending_scroll_to.take();
        let snapshot = self.editor.tree.current_snapshot(generation);
        if let Some(snapshot) = snapshot {
            let row_height = ui.spacing().interact_size.y.max(20.0);
            let row_spacing = ui.spacing().item_spacing.y;
            let target_row = scroll_target.and_then(|target| {
                self.editor
                    .tree
                    .visible_rows
                    .iter()
                    .position(|row| row.id == target)
            });
            let mut tree_scroll = egui::ScrollArea::both()
                .id_salt("json_utility_tree_scroll")
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .max_height(tree_height);
            if let Some(row) = target_row {
                tree_scroll = tree_scroll.vertical_scroll_offset(tree_search_scroll_offset(
                    row,
                    row_height,
                    row_spacing,
                    tree_height,
                    self.editor.tree.visible_rows.len(),
                ));
            }
            #[cfg(test)]
            let mut rendered_rows = 0;
            tree_scroll.show_rows(
                ui,
                row_height,
                self.editor.tree.visible_rows.len(),
                |ui, range| {
                    for index in range {
                        #[cfg(test)]
                        {
                            rendered_rows += 1;
                        }
                        let row = self.editor.tree.visible_rows[index];
                        let Some(node) = snapshot.projection.node(row.id) else {
                            continue;
                        };
                        let preview = tree_row_preview(node.preview());
                        let label = format!(
                            "{}    {}    {}",
                            node_segment_label(node.segment()),
                            node_kind_label(node.kind()),
                            preview
                        );
                        let has_children = node.child_count() > 0;
                        let expanded = self.editor.tree.expanded.contains(&row.id);
                        let hit = self.editor.tree.search.match_by_node.get(&row.id);
                        let label = if let Some((index, _)) = hit {
                            let background = if self.editor.tree.search.active_match == Some(*index)
                            {
                                ui.visuals().selection.bg_fill
                            } else {
                                ui.visuals().faint_bg_color
                            };
                            egui::RichText::new(label)
                                .monospace()
                                .background_color(background)
                        } else {
                            egui::RichText::new(label).monospace()
                        };
                        ui.push_id(("json_tree_node", generation, row.id.index()), |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(row.depth as f32 * 12.0);
                                if has_children {
                                    let toggle = if expanded { "▾" } else { "▸" };
                                    if ui.small_button(toggle).clicked() {
                                        toggle_action = Some(row.id);
                                    }
                                } else {
                                    ui.add_space(ui.spacing().interact_size.x);
                                }
                                let response = ui.selectable_label(
                                    self.editor.tree.selected == Some(row.id),
                                    label,
                                );
                                let response = if let Some((_, matched)) = hit {
                                    let fields =
                                        match (matched.key_matched(), matched.value_matched()) {
                                            (true, true) => "Matched key and scalar value",
                                            (true, false) => "Matched object key",
                                            (false, true) => "Matched scalar value",
                                            (false, false) => "Matched JSON node",
                                        };
                                    response.on_hover_text(fields)
                                } else {
                                    response
                                };
                                if response.clicked() {
                                    select_action = Some(row.id);
                                }
                            });
                        });
                    }
                },
            );
            #[cfg(test)]
            {
                self.rendered_tree_rows = rendered_rows;
            }
        }
        ui.separator();
        let _details_scroll = egui::ScrollArea::vertical()
            .id_salt("json_utility_tree_details_scroll")
            .auto_shrink([false, false])
            .min_scrolled_height(0.0)
            .max_height(details_height)
            .show(ui, |ui| {
                if ready {
                    if let Some(details) = &self.editor.tree.selected_details {
                        ui.label(format!("JSON Pointer: {}", details.path));
                        ui.label(format!("Type: {}", details.kind));
                        ui.label("Value:");
                        ui.add(
                            egui::Label::new(egui::RichText::new(&details.value).monospace())
                                .wrap(),
                        );
                        if details.value_truncated {
                            ui.weak("Display is capped; copy the value for its complete contents.");
                        }
                    }
                } else if self.editor.is_loading_clipboard() {
                    ui.weak("Checking clipboard JSON…");
                } else if let Some(error) = &self.editor.parse_error {
                    ui.colored_label(ui.visuals().error_fg_color, &error.message);
                } else if let Some(error) = &self.editor.tree_worker_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if self.editor.text.is_empty() {
                    ui.weak("Enter JSON in the Text tab to browse its tree.");
                } else {
                    ui.weak("Parsing strict JSON…");
                }
            });
        #[cfg(test)]
        {
            self.tree_panel_layout = Some((
                tree_height,
                details_height,
                _details_scroll.inner_rect.height(),
                _details_scroll.content_size.y,
            ));
        }

        if let Some(id) = select_action {
            self.editor.tree.select(id);
        }
        if let Some(id) = toggle_action {
            self.editor.tree.toggle_expanded(id);
        }
        if let Some((id, expanded)) = branch_action {
            self.editor.tree.set_branch_expanded(id, expanded);
        }
        match copy_action {
            Some(TreeCopyAction::Value) => {
                self.editor.copy_tree_value(&ArboardClipboardBackend, false)
            }
            Some(TreeCopyAction::Subtree) => {
                self.editor.copy_tree_value(&ArboardClipboardBackend, true)
            }
            Some(TreeCopyAction::Path) => self.editor.copy_tree_path(&ArboardClipboardBackend),
            None => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TreeCopyAction {
    Value,
    Subtree,
    Path,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::Mutex;

    struct FakeClipboard {
        text: Mutex<Result<String, ClipboardError>>,
        writes: Mutex<Vec<String>>,
        fail_write: bool,
    }

    impl FakeClipboard {
        fn new(text: &str) -> Self {
            Self {
                text: Mutex::new(Ok(text.to_owned())),
                writes: Mutex::new(Vec::new()),
                fail_write: false,
            }
        }
    }

    impl ClipboardBackend for FakeClipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            self.text.lock().unwrap().clone()
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            if self.fail_write {
                return Err(ClipboardError::Busy("occupied".into()));
            }
            self.writes.lock().unwrap().push(text.to_owned());
            Ok(())
        }
    }

    fn initialize_for_test(dialog: &mut JsonUtilityDialogState, clipboard: &FakeClipboard) {
        dialog.open_with_clipboard(JsonUtilityIntent::General, clipboard);
        dialog.editor.complete_queued_projection_for_test();
    }

    fn select_pointer(editor: &mut JsonEditorState, pointer: &str) {
        let snapshot = editor.tree.current_snapshot(editor.generation).unwrap();
        let path = snapshot.document.resolve_pointer(pointer).unwrap().unwrap();
        let id = node_id_for_path(&snapshot.projection, &path).unwrap();
        editor.tree.select(id);
    }

    #[test]
    fn each_open_loads_only_valid_strict_clipboard_json_and_keeps_intent() {
        let clipboard = FakeClipboard::new(" {\"z\":1} ");
        let mut dialog = JsonUtilityDialogState::default();
        dialog.open_with_clipboard(JsonUtilityIntent::Minify, &clipboard);
        assert!(dialog.open);
        assert_eq!(dialog.intent, JsonUtilityIntent::Minify);
        assert!(dialog.editor.text.is_empty());
        dialog.editor.complete_queued_projection_for_test();
        assert_eq!(dialog.editor.text, " {\"z\":1} ");
        assert!(
            dialog
                .editor
                .tree
                .current_snapshot(dialog.editor.generation)
                .is_some()
        );
        dialog.editor.transform(Transform::Format);
        *clipboard.text.lock().unwrap() = Ok("[1,]".into());
        dialog.open_with_clipboard(JsonUtilityIntent::Format, &clipboard);
        assert_eq!(dialog.intent, JsonUtilityIntent::Format);
        assert!(dialog.editor.text.is_empty());
        dialog.editor.complete_queued_projection_for_test();
        assert!(dialog.editor.text.is_empty());
        assert!(dialog.editor.parse_error.is_none());
        *clipboard.text.lock().unwrap() = Err(ClipboardError::NonText);
        dialog.open_with_clipboard(JsonUtilityIntent::General, &clipboard);
        assert!(dialog.editor.text.is_empty());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn format_and_minify_share_one_editable_session_without_copying() {
        let clipboard = FakeClipboard::new(" { \"z\": [1,true], \"a\": null } ");
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        editor.transform(Transform::Format);
        assert_eq!(
            editor.text,
            "{\n  \"z\": [\n    1,\n    true\n  ],\n  \"a\": null\n}"
        );
        editor.complete_queued_projection_for_test();
        editor.transform(Transform::Minify);
        assert_eq!(editor.text, r#"{"z":[1,true],"a":null}"#);
        assert!(editor.parse_error.is_none());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn parse_failure_preserves_exact_input_and_location_and_never_copies() {
        let clipboard = FakeClipboard::new("{\n  \"value\": }\n");
        let mut editor = JsonEditorState::default();
        editor.reload(&clipboard);
        editor.complete_queued_projection_for_test();
        let original = editor.text.clone();
        for transform in [Transform::Format, Transform::Minify] {
            editor.transform(transform);
            assert_eq!(editor.text, original);
            let error = editor.parse_error.as_ref().unwrap();
            assert_eq!(error.line, Some(2));
            assert!(error.column.is_some_and(|column| column > 0));
            assert!(error.message.contains("line 2 column"));
        }
        editor.copy(&clipboard);
        assert_eq!(editor.text, original);
        assert!(editor.parse_error.is_some());
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn clear_and_explicit_reload_reset_errors_and_allow_invalid_input() {
        let clipboard = FakeClipboard::new("not JSON");
        let mut editor = JsonEditorState::default();
        editor.reload(&clipboard);
        editor.complete_queued_projection_for_test();
        assert_eq!(editor.text, "not JSON");
        editor.transform(Transform::Format);
        assert!(editor.parse_error.is_some());
        editor.clear();
        assert!(editor.text.is_empty());
        assert!(editor.parse_error.is_none());
        editor.reload(&clipboard);
        assert_eq!(editor.text, "not JSON");
        assert!(editor.parse_error.is_none());
        *clipboard.text.lock().unwrap() = Err(ClipboardError::NonText);
        editor.reload(&clipboard);
        assert_eq!(editor.text, "not JSON");
        assert!(matches!(editor.feedback, Some(Err(_))));
    }

    #[test]
    fn copy_validates_and_copies_current_text_only_on_explicit_action() {
        let mut clipboard = FakeClipboard::new("  [ 1, true ]  ");
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        editor.copy(&clipboard);
        assert_eq!(*clipboard.writes.lock().unwrap(), vec!["  [ 1, true ]  "]);
        assert!(matches!(editor.feedback, Some(Ok(_))));
        clipboard.fail_write = true;
        editor.copy(&clipboard);
        assert!(matches!(editor.feedback, Some(Err(_))));
        assert_eq!(clipboard.writes.lock().unwrap().len(), 1);
        assert_eq!(editor.text, "  [ 1, true ]  ");
    }

    #[test]
    fn tree_copy_actions_are_explicit_and_keep_string_value_vs_subtree_semantics() {
        let clipboard = FakeClipboard::new(r#"{"a/b~c":"hello","nested":{"ok":true}}"#);
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        assert!(clipboard.writes.lock().unwrap().is_empty());

        select_pointer(&mut editor, "/a~1b~0c");
        editor.copy_tree_value(&clipboard, false);
        editor.copy_tree_value(&clipboard, true);
        editor.copy_tree_path(&clipboard);
        assert_eq!(
            *clipboard.writes.lock().unwrap(),
            vec!["hello", "\"hello\"", "/a~1b~0c"]
        );

        select_pointer(&mut editor, "/nested");
        editor.copy_tree_value(&clipboard, false);
        editor.copy_tree_value(&clipboard, true);
        assert_eq!(
            clipboard.writes.lock().unwrap()[3..],
            ["{\"ok\":true}", "{\"ok\":true}"]
        );
    }

    #[test]
    fn tree_search_caches_matches_wraps_and_reveals_collapsed_results() {
        let clipboard = FakeClipboard::new(
            r#"{"first":"needle","outer":{"deep":{"target":"needle"}},"needle":{"found":"needle"}}"#,
        );
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        editor.tree.search.query = "needle".to_owned();
        editor.tree.search.fields = JsonSearchFields::Both;
        editor.tree.search.invalidate_results();
        assert!(editor.tree.search.needs_search(editor.generation));
        editor.complete_search_for_test();

        assert_eq!(editor.tree.search.matches.len(), 4);
        assert!(!editor.tree.search.needs_search(editor.generation));
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            "/first"
        );
        assert_eq!(editor.tree.search.active_match, Some(0));

        editor.move_search_match(false);
        let target = editor.tree.selected.unwrap();
        assert_eq!(
            editor
                .tree
                .snapshot
                .as_ref()
                .unwrap()
                .projection
                .path(target)
                .unwrap()
                .to_json_pointer(),
            "/outer/deep/target"
        );
        assert!(editor.tree.pending_scroll_to == Some(target));
        assert!(editor.tree.visible_rows.iter().any(|row| row.id == target));
        assert!(editor.tree.expanded.len() >= 3);

        editor.move_search_match(true);
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            "/first"
        );
        editor.move_search_match(true);
        assert_eq!(editor.tree.search.active_match, Some(3));
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            "/needle/found"
        );
        editor.move_search_match(false);
        assert_eq!(editor.tree.search.active_match, Some(0));

        let prior_selection = editor.tree.selected_path.clone();
        editor.tree.search.query = "first".to_owned();
        editor.tree.search.invalidate_results();
        assert!(editor.tree.search.matches.is_empty());
        assert!(editor.tree.search.match_by_node.is_empty());
        assert_eq!(editor.tree.search.active_match, None);
        assert_eq!(editor.tree.selected_path, prior_selection);
        assert!(editor.tree.search.needs_search(editor.generation));
        editor.complete_search_for_test();
        assert_eq!(editor.tree.search.matches.len(), 1);
        assert_eq!(
            editor.tree.search.matches[0].node(),
            editor.tree.selected.unwrap()
        );
    }

    #[test]
    fn source_edits_and_invalid_json_clear_search_state_immediately() {
        let clipboard = FakeClipboard::new(r#"{"needle":"needle"}"#);
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        editor.tree.search.query = "needle".to_owned();
        editor.tree.search.invalidate_results();
        editor.complete_search_for_test();
        assert_eq!(editor.tree.search.matches.len(), 1);

        let old_generation = editor.generation;
        editor.text = r#"{"broken": }"#.to_owned();
        editor.source_edited();
        assert!(editor.generation > old_generation);
        assert!(editor.tree.search.matches.is_empty());
        assert!(editor.tree.search.match_by_node.is_empty());
        assert_eq!(editor.tree.search.active_match, None);
        assert!(!editor.tree.search.is_pending(editor.generation));
        editor.complete_queued_projection_for_test();
        assert!(editor.parse_error.is_some());
        assert!(editor.tree.current_snapshot(editor.generation).is_none());
        assert!(editor.tree.search.matches.is_empty());
        editor.move_search_match(false);
        assert!(editor.tree.search.matches.is_empty());
    }

    #[test]
    fn tree_search_uses_virtual_row_stride_and_reveals_the_selected_row() {
        let context = egui::Context::default();
        let mut rendered_rows = None;
        let target_row = 150;
        let total_rows = 200;
        context
            .run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let row_height = 20.0;
                    let row_spacing = ui.spacing().item_spacing.y;
                    let viewport_height = 100.0;
                    let offset = tree_search_scroll_offset(
                        target_row,
                        row_height,
                        row_spacing,
                        viewport_height,
                        total_rows,
                    );
                    let stride = row_height + row_spacing;
                    let target_top = target_row as f32 * stride - offset;
                    assert!(target_top >= 0.0);
                    assert!(target_top + row_height <= viewport_height);

                    egui::ScrollArea::vertical()
                        .id_salt("json_search_virtual_row_test")
                        .max_height(viewport_height)
                        .vertical_scroll_offset(offset)
                        .show_rows(ui, row_height, total_rows, |_, rows| {
                            rendered_rows = Some(rows);
                        });
                });
            })
            .drop_without_applying_deltas();
        assert!(rendered_rows.is_some_and(|rows| rows.contains(&target_row)));
    }

    #[test]
    fn pending_invalid_and_stale_snapshots_cannot_enable_tree_copy() {
        let clipboard = FakeClipboard::new(r#"{"value":1}"#);
        let mut editor = JsonEditorState::default();
        editor.reload(&clipboard);
        let stale_generation = editor.generation;
        let stale = ProjectionRequest {
            generation: stale_generation,
            source: ProjectionSource::EditedText(r#"{"value":1}"#.to_owned()),
        }
        .build();
        editor.text = "not JSON".to_owned();
        editor.source_edited();
        editor.accept_completion(stale);
        assert!(editor.tree.current_snapshot(editor.generation).is_none());
        assert!(clipboard.writes.lock().unwrap().is_empty());
        editor.complete_queued_projection_for_test();
        assert!(editor.parse_error.is_some());
        editor.copy_tree_value(&clipboard, false);
        editor.copy_tree_path(&clipboard);
        assert!(clipboard.writes.lock().unwrap().is_empty());
    }

    #[test]
    fn row_previews_escape_line_breaks_and_remain_bounded() {
        let preview = tree_row_preview("first\nsecond\tvalue");
        assert_eq!(preview, "first\\nsecond\\tvalue");
        assert!(!preview.contains('\n'));

        let long_preview = tree_row_preview(&"x".repeat(TREE_ROW_PREVIEW_MAX_CHARS + 50));
        assert!(long_preview.chars().count() <= TREE_ROW_PREVIEW_MAX_CHARS + 1);
        assert!(long_preview.ends_with('…'));
    }

    #[test]
    fn projection_and_visible_rows_rebuild_only_for_source_or_expansion_changes() {
        let clipboard = FakeClipboard::new(r#"{"a":{"b":1},"list":[2]}"#);
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        assert_eq!(editor.tree.visible_rows_rebuilds, 1);
        assert_eq!(editor.projection_builds_started, 1);
        let generation = editor.generation;
        editor.tree.view = JsonUtilityView::Tree;
        select_pointer(&mut editor, "/a");
        assert_eq!(editor.generation, generation);
        assert_eq!(editor.tree.visible_rows_rebuilds, 1);
        assert_eq!(editor.projection_builds_started, 1);

        let id = editor.tree.selected.unwrap();
        editor.tree.toggle_expanded(id);
        assert_eq!(editor.tree.visible_rows_rebuilds, 2);
        editor.tree.toggle_expanded(id);
        assert_eq!(editor.tree.visible_rows_rebuilds, 3);

        editor.text = "{ \"a\": {\"b\": 1}, \"list\": [2] }".to_owned();
        editor.source_edited();
        assert!(editor.tree.visible_rows.is_empty());
        editor.complete_queued_projection_for_test();
        assert_eq!(editor.tree.visible_rows_rebuilds, 4);
        assert_eq!(editor.projection_builds_started, 2);
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            "/a"
        );
    }

    #[test]
    fn expansion_and_selection_restore_by_typed_path_and_drop_missing_paths() {
        let clipboard = FakeClipboard::new(r#"{"a":{"b":1},"0":"key"}"#);
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        select_pointer(&mut editor, "/a/b");
        let a_path = editor
            .tree
            .snapshot
            .as_ref()
            .unwrap()
            .document
            .resolve_pointer("/a")
            .unwrap()
            .unwrap();
        let previous_a_id =
            node_id_for_path(&editor.tree.snapshot.as_ref().unwrap().projection, &a_path).unwrap();
        editor.tree.expanded.insert(previous_a_id);

        editor.text = r#"{"a":{"b":2},"new":true}"#.to_owned();
        editor.source_edited();
        editor.complete_queued_projection_for_test();
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            "/a/b"
        );
        let snapshot = editor.tree.current_snapshot(editor.generation).unwrap();
        let restored_a_id = node_id_for_path(&snapshot.projection, &a_path).unwrap();
        assert!(editor.tree.expanded.contains(&restored_a_id));

        editor.text = r#"{"a":0}"#.to_owned();
        editor.source_edited();
        editor.complete_queued_projection_for_test();
        assert_eq!(
            editor
                .tree
                .selected_path
                .as_ref()
                .unwrap()
                .to_json_pointer(),
            ""
        );
    }

    #[test]
    fn collapsed_root_remains_collapsed_after_harmless_source_change() {
        let clipboard = FakeClipboard::new(r#"{"a":1}"#);
        let mut editor = JsonEditorState::default();
        editor.initialize(&clipboard);
        editor.complete_queued_projection_for_test();
        let root = editor
            .tree
            .current_snapshot(editor.generation)
            .unwrap()
            .projection
            .root();
        assert!(editor.tree.expanded.contains(&root));

        editor.tree.toggle_expanded(root);
        assert!(editor.tree.expanded.is_empty());
        assert_eq!(editor.tree.visible_rows.len(), 1);

        editor.text = " { \"a\": 1 } ".to_owned();
        editor.source_edited();
        editor.complete_queued_projection_for_test();
        assert!(editor.tree.expanded.is_empty());
        assert_eq!(editor.tree.visible_rows.len(), 1);
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
    fn egui_tree_details_remain_bounded_when_preview_is_long() {
        let mut dialog = JsonUtilityDialogState::default();
        dialog.open = true;
        dialog.editor.tree.view = JsonUtilityView::Tree;
        let long_value = "x".repeat(TREE_DETAIL_MAX_CHARS + 1);
        dialog.editor.text = format!(r#"{{"value":"{long_value}"}}"#);
        let snapshot = JsonTreeSnapshot::parse(&dialog.editor.text).unwrap();
        let path = snapshot
            .document
            .resolve_pointer("/value")
            .unwrap()
            .unwrap();
        let selected = node_id_for_path(&snapshot.projection, &path).unwrap();
        dialog
            .editor
            .tree
            .install(dialog.editor.generation, snapshot);
        dialog.editor.tree.select(selected);

        let context = egui::Context::default();
        context
            .run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert!(dialog.open);
        assert_eq!(dialog.editor.tree.view, JsonUtilityView::Tree);
        assert!(
            dialog
                .editor
                .tree
                .selected_details
                .as_ref()
                .unwrap()
                .value_truncated
        );
        let (tree_height, details_max_height, details_height, details_content_height) =
            dialog.tree_panel_layout.unwrap();
        assert!(tree_height > 0.0);
        assert!(details_max_height <= TREE_DETAILS_PANEL_MAX_HEIGHT);
        assert!(details_height <= details_max_height + 0.1);
        assert!(details_content_height > details_height);
    }

    #[test]
    fn large_tree_dialog_virtualizes_rows_and_keeps_idle_snapshots_cached() {
        const ARRAY_LEN: usize = 2_048;
        let mut values: Vec<String> = (0..ARRAY_LEN).map(|index| index.to_string()).collect();
        values[ARRAY_LEN - 1] = format!(r#""{}needle""#, "long-value-".repeat(128));
        let clipboard = FakeClipboard::new(&format!("[{}]", values.join(",")));
        let mut dialog = JsonUtilityDialogState::default();
        initialize_for_test(&mut dialog, &clipboard);
        dialog.editor.tree.view = JsonUtilityView::Tree;
        dialog.editor.tree.search.query = "needle".to_owned();
        dialog.editor.tree.search.fields = JsonSearchFields::Values;
        dialog.editor.tree.search.invalidate_results();
        dialog.editor.complete_search_for_test();

        let generation = dialog.editor.generation;
        let projection_builds = dialog.editor.projection_builds_started;
        let search_builds = dialog.editor.search_builds_started;
        let row_rebuilds = dialog.editor.tree.visible_rows_rebuilds;
        let visible_row_count = dialog.editor.tree.visible_rows.len();
        assert_eq!(visible_row_count, ARRAY_LEN + 1);
        assert_eq!(dialog.editor.tree.search.matches.len(), 1);
        let selected = dialog.editor.tree.selected.unwrap();
        assert!(
            dialog
                .editor
                .tree
                .search
                .match_by_node
                .contains_key(&selected)
        );
        assert_eq!(
            dialog
                .editor
                .tree
                .snapshot
                .as_ref()
                .unwrap()
                .projection
                .path(selected)
                .unwrap()
                .to_json_pointer(),
            "/2047"
        );

        let context = egui::Context::default();
        context
            .run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        let first_frame_rows = dialog.rendered_tree_rows;
        assert!(first_frame_rows > 0);
        assert!(first_frame_rows <= 64, "rendered {first_frame_rows} rows");
        assert!(first_frame_rows < visible_row_count);

        context
            .run_ui(egui::RawInput::default(), |ctx| dialog.show(ctx))
            .drop_without_applying_deltas();
        assert!(dialog.rendered_tree_rows <= 64);
        assert_eq!(dialog.editor.generation, generation);
        assert_eq!(dialog.editor.projection_builds_started, projection_builds);
        assert_eq!(dialog.editor.search_builds_started, search_builds);
        assert_eq!(dialog.editor.tree.visible_rows_rebuilds, row_rebuilds);
    }
}
