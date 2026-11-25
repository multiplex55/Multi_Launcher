//! Checked public tree projections for renderer frontends.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeStamp {
    generation: u64,
    target: u64,
    search: u64,
    selected: Option<JsonTreeNodeId>,
    result_generation: Option<u64>,
    active_match: Option<usize>,
}
impl TreeStamp { pub fn source_generation(self) -> u64 { self.generation } }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeMatch { Key, Value, Both }
#[derive(Clone, Debug)]
pub struct TreeRow {
    pub id: JsonTreeNodeId,
    pub depth: usize,
    pub label: String,
    pub expandable: bool,
    pub expanded: bool,
    pub selected: bool,
    pub matched: Option<TreeMatch>,
    pub active_match: bool,
}
#[derive(Clone, Debug)]
pub struct TreeDetails {
    pub path: String,
    pub kind: &'static str,
    pub value: String,
    pub truncated: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeCopyKind {
    Value,
    Subtree,
    Pointer,
}
pub struct TreeCopyRequest {
    snapshot: Arc<JsonTreeSnapshot>,
    path: JsonNodePath,
    kind: TreeCopyKind,
    stamp: TreeStamp,
}
impl TreeCopyRequest {
    pub fn execute(&self, clipboard: &impl ClipboardBackend) -> Result<(), String> {
        let text = match self.kind {
            TreeCopyKind::Value => self.snapshot.document.copy_value(&self.path),
            TreeCopyKind::Subtree => self.snapshot.document.copy_subtree(&self.path),
            TreeCopyKind::Pointer => Ok(Some(self.path.to_json_pointer())),
        }
        .map_err(|error| format!("Could not copy JSON node: {error}"))?;
        let text = text.ok_or_else(|| "Selected JSON node is unavailable".to_owned())?;
        clipboard
            .write_text(&text)
            .map_err(|error| format!("Could not copy JSON node: {error}"))
    }
}
impl JsonEditorState {
    pub fn tree_stamp(&self) -> Option<TreeStamp> {
        self.tree.current_snapshot(self.generation)?;
        Some(TreeStamp {
            generation: self.generation,
            target: self.tree.target_revision,
            search: self.tree.search.revision,
            selected: self.tree.selected,
            result_generation: self.tree.search.result_generation,
            active_match: self.tree.search.active_match,
        })
    }
    pub fn tree_row_count(&self) -> usize {
        if self.tree_stamp().is_some() {
            self.tree.visible_rows.len()
        } else {
            0
        }
    }
    pub fn tree_row(&self, index: usize) -> Option<TreeRow> {
        let snapshot = self.tree.current_snapshot(self.generation)?;
        let row = *self.tree.visible_rows.get(index)?;
        let node = snapshot.projection.node(row.id)?;
        let hit = self.tree.search.match_by_node.get(&row.id);
        Some(TreeRow {
            id: row.id,
            depth: row.depth,
            label: format!(
                "{}: {}",
                node_segment_label(node.segment()),
                tree_row_preview(node.preview())
            ),
            expandable: node.child_count() > 0,
            expanded: self.tree.expanded.contains(&row.id),
            selected: self.tree.selected == Some(row.id),
            matched: hit.and_then(|(_, hit)| match (hit.key_matched(), hit.value_matched()) {
                (true, true) => Some(TreeMatch::Both), (true, false) => Some(TreeMatch::Key),
                (false, true) => Some(TreeMatch::Value), _ => None,
            }),
            active_match: hit.is_some_and(|(index, _)| Some(*index) == self.tree.search.active_match),
        })
    }
    pub fn tree_details(&self) -> Option<TreeDetails> {
        self.tree_stamp()?;
        let details = self.tree.selected_details.as_ref()?;
        Some(TreeDetails {
            path: details.path.clone(),
            kind: details.kind,
            value: details.value.clone(),
            truncated: details.value_truncated,
        })
    }
    pub fn tree_select(&mut self, stamp: TreeStamp, id: JsonTreeNodeId) -> bool {
        if self.tree_stamp() != Some(stamp)
            || !self.tree.visible_rows.iter().any(|row| row.id == id)
        {
            return false;
        }
        self.tree.select(id);
        true
    }
    pub fn tree_toggle(&mut self, stamp: TreeStamp, id: JsonTreeNodeId) -> bool {
        if self.tree_stamp() != Some(stamp)
            || !self.tree.visible_rows.iter().any(|row| row.id == id)
        {
            return false;
        }
        self.tree.toggle_expanded(id);
        true
    }
    pub fn tree_expand_selected(&mut self, stamp: TreeStamp, expanded: bool) -> bool {
        if self.tree_stamp() != Some(stamp) {
            return false;
        }
        let Some(id) = self.tree.selected else {
            return false;
        };
        self.tree.set_branch_expanded(id, expanded);
        true
    }
    pub fn tree_set_search(&mut self, query: String, fields: JsonSearchFields) {
        if self.tree.search.query != query || self.tree.search.fields != fields {
            self.tree.search.query = query;
            self.tree.search.fields = fields;
            self.tree.search.invalidate_results();
        }
    }
    pub fn tree_search_query(&self) -> &str {
        &self.tree.search.query
    }
    pub fn tree_search_fields(&self) -> JsonSearchFields {
        self.tree.search.fields
    }
    pub fn tree_search_status(&self) -> (usize, Option<usize>, bool) {
        (
            self.tree.search.matches.len(),
            self.tree.search.active_match,
            self.tree.search.is_pending(self.generation),
        )
    }
    pub fn tree_move_match(&mut self, stamp: TreeStamp, previous: bool) -> Option<usize> {
        if self.tree_stamp() != Some(stamp)
            || self.tree.search.is_pending(self.generation)
        {
            return None;
        }
        self.move_search_match(previous);
        let id = self.tree.pending_scroll_to.take()?;
        self.tree.visible_rows.iter().position(|row| row.id == id)
    }
    pub fn tree_take_reveal(&mut self) -> Option<usize> {
        let id = self.tree.pending_scroll_to.take()?;
        self.tree.visible_rows.iter().position(|row| row.id == id)
    }
    pub fn tree_copy_request(
        &self,
        stamp: TreeStamp,
        kind: TreeCopyKind,
    ) -> Option<TreeCopyRequest> {
        if self.tree_stamp()? != stamp {
            return None;
        }
        let snapshot = self.tree.current_snapshot_handle(self.generation)?;
        let path = snapshot.projection.path(self.tree.selected?)?;
        Some(TreeCopyRequest {
            snapshot,
            path,
            kind,
            stamp,
        })
    }
    pub fn finish_tree_copy(
        &mut self,
        request: &TreeCopyRequest,
        result: Result<(), String>,
    ) -> bool {
        if self.tree_stamp() != Some(request.stamp) {
            return false;
        }
        self.feedback = Some(result.map(|()| "Copied JSON node to clipboard".to_owned()));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::clipboard::ClipboardError;
    use std::sync::Mutex;
    struct Clipboard(Mutex<Vec<String>>);
    impl ClipboardBackend for Clipboard {
        fn read_text(&self) -> Result<String, ClipboardError> {
            Err(ClipboardError::NonText)
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.0.lock().unwrap().push(text.into());
            Ok(())
        }
    }
    #[test]
    fn checked_tree_selection_search_and_copy_receipts_reject_stale_state() {
        let mut editor = JsonEditorState::default();
        editor.set_source(r#"{"name":"Café","nested":{"name":"two"}}"#.into());
        editor.complete_queued_projection_for_test();
        let root = editor.tree_stamp().unwrap();
        assert_eq!(editor.tree_row_count(), 3);
        let name = editor.tree_row(1).unwrap();
        assert!(editor.tree_select(root, name.id));
        assert!(!editor.tree_select(root, name.id));
        let selected = editor.tree_stamp().unwrap();
        let copy = editor
            .tree_copy_request(selected, TreeCopyKind::Pointer)
            .unwrap();
        let clipboard = Clipboard(Mutex::new(Vec::new()));
        assert!(copy.execute(&clipboard).is_ok());
        assert_eq!(clipboard.0.lock().unwrap().as_slice(), ["/name"]);
        editor.tree_set_search("two".into(), JsonSearchFields::Values);
        assert!(!editor.finish_tree_copy(&copy, Ok(())));
        editor.complete_search_for_test();
        assert_eq!(editor.tree_search_status().0, 1);
        assert!(editor.tree_take_reveal().is_some());
        assert!(editor.tree_details().unwrap().path.contains("nested"));
        let active_row = (0..editor.tree_row_count()).filter_map(|index| editor.tree_row(index))
            .find(|row| row.active_match).unwrap();
        assert_eq!(active_row.matched, Some(TreeMatch::Value));
        editor.tree_set_search("name".into(), JsonSearchFields::Keys);
        editor.complete_search_for_test();
        let navigation = editor.tree_stamp().unwrap();
        assert!(editor.tree_move_match(navigation, false).is_some());
        assert!(editor.tree_move_match(navigation, false).is_none());
        let current = editor.tree_stamp().unwrap();
        editor.set_source("[]".into());
        assert!(
            editor
                .tree_copy_request(current, TreeCopyKind::Value)
                .is_none()
        );
    }
}
