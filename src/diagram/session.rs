//! Pure source/graph synchronization, bounded transactions, and edit history.

use super::codec::{DiagramCodecError, decode_session, encode_session};
use super::mermaid::{
    MAX_MERMAID_SOURCE_BYTES, MermaidDiagnostic, MermaidFormatError, format_mermaid, parse_mermaid,
};
use super::model::{
    DiagramDocument, DiagramEdge, DiagramGroup, DiagramNode, EdgeId, FlowDirection,
    GraphValidationError, GroupId, MAX_DIAGRAM_NODES, MAX_LABEL_BYTES, MAX_POSITION_ABS, NodeId,
    NodeShape, Position,
};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

pub const MAX_DIAGRAM_TRANSACTION_OPERATIONS: usize = 256;
pub const MAX_DIAGRAM_HISTORY_ENTRIES: usize = 64;
pub const MAX_DIAGRAM_HISTORY_BYTES: usize = 4 * 1024 * 1024;

const ANCHOR_GRID_SIDE: usize = 64;
const ANCHOR_GRID_SLOTS: usize = ANCHOR_GRID_SIDE * ANCHOR_GRID_SIDE;
const ANCHOR_SPACING: f32 = 180.0;
const MIN_ANCHOR_SEPARATION: f64 = 120.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionSnapshot {
    #[serde(deserialize_with = "deserialize_bounded_source")]
    pub(super) draft: String,
    pub(super) last_good: Option<LastGood>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LastGood {
    #[serde(deserialize_with = "deserialize_bounded_source")]
    pub(super) source: String,
    pub(super) graph: DiagramDocument,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryEntry {
    pub(super) before: SessionSnapshot,
    pub(super) after: SessionSnapshot,
    #[serde(skip)]
    pub(super) encoded_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewState {
    Current,
    Stale,
    NoPreview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionStatus {
    pub preview: PreviewState,
    pub diagnostic: Option<MermaidDiagnostic>,
}

#[derive(Clone, Debug)]
pub struct SavedContentSnapshot(SessionSnapshot);

#[derive(Clone, Debug, Default)]
pub(super) struct EditHistory {
    pub(super) undo: VecDeque<HistoryEntry>,
    pub(super) redo: VecDeque<HistoryEntry>,
    pub(super) encoded_bytes: usize,
}

impl EditHistory {
    fn record(&mut self, entry: HistoryEntry) {
        for redo_entry in self.redo.drain(..) {
            self.encoded_bytes = self.encoded_bytes.saturating_sub(redo_entry.encoded_bytes);
        }
        self.encoded_bytes = self.encoded_bytes.saturating_add(entry.encoded_bytes);
        self.undo.push_back(entry);
        while self.undo.len().saturating_add(self.redo.len()) > MAX_DIAGRAM_HISTORY_ENTRIES
            || self.encoded_bytes > MAX_DIAGRAM_HISTORY_BYTES
        {
            let Some(oldest) = self.undo.pop_front() else {
                break;
            };
            self.encoded_bytes = self.encoded_bytes.saturating_sub(oldest.encoded_bytes);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodePosition {
    pub id: NodeId,
    pub position: Position,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphOperation {
    AddNode {
        id: NodeId,
        label: String,
        shape: NodeShape,
        position: Position,
        group: Option<GroupId>,
    },
    RemoveNode {
        id: NodeId,
    },
    SetNodeLabel {
        id: NodeId,
        label: String,
    },
    SetNodeShape {
        id: NodeId,
        shape: NodeShape,
    },
    SetNodeGroup {
        id: NodeId,
        group: Option<GroupId>,
    },
    MoveNodes {
        positions: Vec<NodePosition>,
    },
    AddEdge {
        from: NodeId,
        to: NodeId,
        label: Option<String>,
    },
    RemoveEdge {
        id: EdgeId,
    },
    SetEdgeLabel {
        id: EdgeId,
        label: Option<String>,
    },
    ReconnectEdge {
        id: EdgeId,
        from: NodeId,
        to: NodeId,
    },
    AddGroup {
        id: GroupId,
        label: String,
    },
    RemoveGroup {
        id: GroupId,
    },
    SetGroupLabel {
        id: GroupId,
        label: String,
    },
    SetDirection(FlowDirection),
}

#[derive(Debug)]
pub enum DiagramSessionError {
    SourceTooLarge { max_bytes: usize },
    VisualEditsBlocked,
    RevisionExhausted,
    TooManyOperations { max_operations: usize },
    TooManyMoveTargets { max_targets: usize },
    DuplicateMoveTarget(NodeId),
    NodeNotFound(NodeId),
    EdgeNotFound(EdgeId),
    GroupNotFound(GroupId),
    AnchorSpaceExhausted,
    HistoryEntryTooLarge { max_bytes: usize },
    HistorySerializationFailed,
    SemanticMismatch,
    Graph(GraphValidationError),
    Format(MermaidFormatError),
}

impl fmt::Display for DiagramSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { max_bytes } => {
                write!(formatter, "source exceeds the {max_bytes}-byte limit")
            }
            Self::VisualEditsBlocked => {
                formatter.write_str("visual edits are unavailable while the source is invalid")
            }
            Self::RevisionExhausted => formatter.write_str("diagram revision is exhausted"),
            Self::TooManyOperations { max_operations } => write!(
                formatter,
                "transaction exceeds the {max_operations}-operation limit"
            ),
            Self::TooManyMoveTargets { max_targets } => write!(
                formatter,
                "move exceeds the {max_targets}-node target limit"
            ),
            Self::DuplicateMoveTarget(id) => {
                write!(formatter, "node {id} occurs more than once in a move")
            }
            Self::NodeNotFound(id) => write!(formatter, "node {id} was not found"),
            Self::EdgeNotFound(id) => write!(formatter, "edge {} was not found", id.get()),
            Self::GroupNotFound(id) => write!(formatter, "group {id} was not found"),
            Self::AnchorSpaceExhausted => {
                formatter.write_str("no separated bounded node anchor is available")
            }
            Self::HistoryEntryTooLarge { max_bytes } => write!(
                formatter,
                "edit is too large to record within the {max_bytes}-byte history limit"
            ),
            Self::HistorySerializationFailed => {
                formatter.write_str("failed to measure encoded diagram history")
            }
            Self::SemanticMismatch => {
                formatter.write_str("formatted Mermaid does not match the candidate graph")
            }
            Self::Graph(error) => error.fmt(formatter),
            Self::Format(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DiagramSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Graph(error) => Some(error),
            Self::Format(error) => Some(error),
            _ => None,
        }
    }
}

/// Pure editor state. Filesystem effects belong to the later persistence layer.
#[derive(Clone, Debug)]
pub struct DiagramSession {
    pub(super) current: SessionSnapshot,
    pub(super) revision: u64,
    /// Monotonic for the lifetime of the session; history snapshots never restore it.
    pub(super) next_edge_id: u64,
    pub(super) saved_baseline: SessionSnapshot,
    pub(super) history: EditHistory,
}

impl Default for DiagramSession {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramSession {
    pub fn new() -> Self {
        let initial = SessionSnapshot {
            draft: String::new(),
            last_good: Some(LastGood {
                source: String::new(),
                graph: DiagramDocument::default(),
            }),
        };
        Self {
            current: initial.clone(),
            revision: 0,
            next_edge_id: 1,
            saved_baseline: initial,
            history: EditHistory::default(),
        }
    }

    /// Creates a new untitled session whose first edit is the supplied raw
    /// source. Invalid Mermaid remains an exact NoPreview draft; it does not
    /// inherit the blank session's empty last-good graph as a stale preview.
    pub(crate) fn from_fresh_source(source: impl AsRef<str>) -> Result<Self, DiagramSessionError> {
        let source = source.as_ref();
        if source.len() > MAX_MERMAID_SOURCE_BYTES {
            return Err(DiagramSessionError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            });
        }
        if source.is_empty() {
            return Ok(Self::new());
        }

        let mut session = Self::new();
        let before = session.current.clone();
        let after = match parse_mermaid(source) {
            Ok(parsed) => SessionSnapshot {
                draft: source.to_owned(),
                last_good: Some(session.reconcile_source(parsed, source)?),
            },
            Err(_) => SessionSnapshot {
                draft: source.to_owned(),
                last_good: None,
            },
        };
        let next_revision = session.next_revision()?;
        let next_edge_id = after
            .last_good
            .as_ref()
            .map_or(session.next_edge_id, |last_good| {
                session.next_edge_id.max(last_good.graph.next_edge_id)
            });
        let entry = make_history_entry(before, after.clone())?;

        session.history.record(entry);
        session.current = after;
        session.revision = next_revision;
        session.next_edge_id = next_edge_id;
        Ok(session)
    }

    pub fn draft(&self) -> &str {
        &self.current.draft
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn next_edge_id(&self) -> u64 {
        self.next_edge_id
    }

    pub fn preview_graph(&self) -> Option<&DiagramDocument> {
        self.current
            .last_good
            .as_ref()
            .map(|last_good| &last_good.graph)
    }

    pub fn last_good_source(&self) -> Option<&str> {
        self.current
            .last_good
            .as_ref()
            .map(|last_good| last_good.source.as_str())
    }

    pub fn status(&self) -> SessionStatus {
        match parse_mermaid(&self.current.draft) {
            Ok(parsed) => {
                let current = self.current.last_good.as_ref().is_some_and(|last_good| {
                    last_good.source == self.current.draft
                        && semantically_equivalent(&parsed, &last_good.graph)
                });
                SessionStatus {
                    preview: if current {
                        PreviewState::Current
                    } else if self.current.last_good.is_some() {
                        PreviewState::Stale
                    } else {
                        PreviewState::NoPreview
                    },
                    diagnostic: None,
                }
            }
            Err(diagnostic) => SessionStatus {
                preview: if self.current.last_good.is_some() {
                    PreviewState::Stale
                } else {
                    PreviewState::NoPreview
                },
                diagnostic: Some(diagnostic),
            },
        }
    }

    pub fn current_graph(&self) -> Option<&DiagramDocument> {
        (self.status().preview == PreviewState::Current)
            .then(|| {
                self.current
                    .last_good
                    .as_ref()
                    .map(|last_good| &last_good.graph)
            })
            .flatten()
    }

    pub fn is_dirty(&self) -> bool {
        !same_saved_content(&self.current, &self.saved_baseline)
    }

    pub fn capture_saved_content(&self) -> SavedContentSnapshot {
        SavedContentSnapshot(self.current.clone())
    }

    /// Acknowledges exactly the captured content. Later edits remain dirty.
    pub fn acknowledge_saved_content(&mut self, saved: &SavedContentSnapshot) {
        self.saved_baseline = saved.0.clone();
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.history.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.history.redo.len()
    }

    /// Replaces the raw source draft. Invalid in-bounds text is retained exactly.
    pub fn replace_source(&mut self, source: impl AsRef<str>) -> Result<bool, DiagramSessionError> {
        let source = source.as_ref();
        if source.len() > MAX_MERMAID_SOURCE_BYTES {
            return Err(DiagramSessionError::SourceTooLarge {
                max_bytes: MAX_MERMAID_SOURCE_BYTES,
            });
        }
        if source == self.current.draft {
            return Ok(false);
        }
        let next_revision = self.next_revision()?;
        let before = self.current.clone();
        let after = match parse_mermaid(source) {
            Ok(parsed) => {
                let last_good = self.reconcile_source(parsed, source)?;
                SessionSnapshot {
                    draft: source.to_owned(),
                    last_good: Some(last_good),
                }
            }
            Err(_) => SessionSnapshot {
                draft: source.to_owned(),
                last_good: self.current.last_good.clone(),
            },
        };
        let entry = make_history_entry(before, after.clone())?;
        let next_edge_id = after
            .last_good
            .as_ref()
            .map_or(self.next_edge_id, |last_good| {
                self.next_edge_id.max(last_good.graph.next_edge_id)
            });

        self.history.record(entry);
        self.current = after;
        self.revision = next_revision;
        self.next_edge_id = next_edge_id;
        Ok(true)
    }

    /// Atomically applies a bounded batch of visual graph operations.
    pub fn apply_transaction(
        &mut self,
        operations: &[GraphOperation],
    ) -> Result<bool, DiagramSessionError> {
        if operations.len() > MAX_DIAGRAM_TRANSACTION_OPERATIONS {
            return Err(DiagramSessionError::TooManyOperations {
                max_operations: MAX_DIAGRAM_TRANSACTION_OPERATIONS,
            });
        }
        if operations.is_empty() {
            return Ok(false);
        }
        let Some(last_good) = self.current.last_good.as_ref() else {
            return Err(DiagramSessionError::VisualEditsBlocked);
        };
        let parsed = parse_mermaid(&self.current.draft)
            .map_err(|_| DiagramSessionError::VisualEditsBlocked)?;
        if last_good.source != self.current.draft
            || !semantically_equivalent(&parsed, &last_good.graph)
        {
            return Err(DiagramSessionError::VisualEditsBlocked);
        }
        preflight_operations(operations)?;

        let next_revision = self.next_revision()?;
        let before = self.current.clone();
        let old_graph = &last_good.graph;
        let mut candidate = old_graph.clone();
        candidate.next_edge_id = candidate.next_edge_id.max(self.next_edge_id);
        apply_operations(&mut candidate, operations)?;
        candidate.validate().map_err(DiagramSessionError::Graph)?;
        if same_graph_content(&candidate, old_graph) {
            return Ok(false);
        }

        let canonical_source = format_mermaid(&candidate).map_err(DiagramSessionError::Format)?;
        let reparsed =
            parse_mermaid(&canonical_source).map_err(|_| DiagramSessionError::SemanticMismatch)?;
        if !semantically_equivalent(&candidate, &reparsed) {
            return Err(DiagramSessionError::SemanticMismatch);
        }
        let after = SessionSnapshot {
            draft: canonical_source.clone(),
            last_good: Some(LastGood {
                source: canonical_source,
                graph: candidate.clone(),
            }),
        };
        let entry = make_history_entry(before, after.clone())?;
        let next_edge_id = self.next_edge_id.max(candidate.next_edge_id);

        self.history.record(entry);
        self.current = after;
        self.revision = next_revision;
        self.next_edge_id = next_edge_id;
        Ok(true)
    }

    pub fn undo(&mut self) -> Result<bool, DiagramSessionError> {
        if self.history.undo.is_empty() {
            return Ok(false);
        }
        let next_revision = self.next_revision()?;
        let Some(entry) = self.history.undo.pop_back() else {
            return Ok(false);
        };
        self.current = entry.before.clone();
        self.history.redo.push_back(entry);
        self.revision = next_revision;
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, DiagramSessionError> {
        if self.history.redo.is_empty() {
            return Ok(false);
        }
        let next_revision = self.next_revision()?;
        let Some(entry) = self.history.redo.pop_back() else {
            return Ok(false);
        };
        self.current = entry.after.clone();
        self.history.undo.push_back(entry);
        self.revision = next_revision;
        Ok(true)
    }

    pub fn encode(&self) -> Result<Vec<u8>, DiagramCodecError> {
        encode_session(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DiagramCodecError> {
        decode_session(bytes)
    }

    fn next_revision(&self) -> Result<u64, DiagramSessionError> {
        self.revision
            .checked_add(1)
            .ok_or(DiagramSessionError::RevisionExhausted)
    }

    fn reconcile_source(
        &self,
        mut parsed: DiagramDocument,
        source: &str,
    ) -> Result<LastGood, DiagramSessionError> {
        let prior = self.current.last_good.as_ref();
        let parser_next_edge_id = parsed.next_edge_id;
        let preserve_first_edge_ids = self.next_edge_id == 1;
        parsed.next_edge_id = if preserve_first_edge_ids {
            parser_next_edge_id
        } else {
            self.next_edge_id.max(parser_next_edge_id)
        };

        if let Some(prior) = prior {
            reconcile_nodes(&mut parsed, &prior.graph)?;
            reconcile_edges(&mut parsed, &prior.graph, preserve_first_edge_ids)?;
        } else if !preserve_first_edge_ids {
            allocate_unmatched_edges(&mut parsed)?;
        }

        parsed.validate().map_err(DiagramSessionError::Graph)?;
        let reparsed = parse_mermaid(source).map_err(|_| DiagramSessionError::SemanticMismatch)?;
        if !semantically_equivalent(&parsed, &reparsed) {
            return Err(DiagramSessionError::SemanticMismatch);
        }
        Ok(LastGood {
            source: source.to_owned(),
            graph: parsed,
        })
    }
}

fn deserialize_bounded_source<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct SourceVisitor;

    impl<'de> Visitor<'de> for SourceVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                formatter,
                "source text of at most {MAX_MERMAID_SOURCE_BYTES} bytes"
            )
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > MAX_MERMAID_SOURCE_BYTES {
                return Err(E::custom("diagram source exceeds its byte limit"));
            }
            Ok(value.to_owned())
        }

        fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(value)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value.len() > MAX_MERMAID_SOURCE_BYTES {
                return Err(E::custom("diagram source exceeds its byte limit"));
            }
            Ok(value)
        }
    }

    deserializer.deserialize_str(SourceVisitor)
}

fn make_history_entry(
    before: SessionSnapshot,
    after: SessionSnapshot,
) -> Result<HistoryEntry, DiagramSessionError> {
    let mut entry = HistoryEntry {
        before,
        after,
        encoded_bytes: 0,
    };
    entry.encoded_bytes = serde_json::to_vec(&entry)
        .map_err(|_| DiagramSessionError::HistorySerializationFailed)?
        .len();
    if entry.encoded_bytes > MAX_DIAGRAM_HISTORY_BYTES {
        return Err(DiagramSessionError::HistoryEntryTooLarge {
            max_bytes: MAX_DIAGRAM_HISTORY_BYTES,
        });
    }
    Ok(entry)
}

fn same_saved_content(left: &SessionSnapshot, right: &SessionSnapshot) -> bool {
    left.draft == right.draft
        && match (&left.last_good, &right.last_good) {
            (None, None) => true,
            (Some(left), Some(right)) => {
                left.source == right.source && same_graph_content(&left.graph, &right.graph)
            }
            _ => false,
        }
}

fn same_graph_content(left: &DiagramDocument, right: &DiagramDocument) -> bool {
    left.direction == right.direction
        && left.nodes == right.nodes
        && left.edges == right.edges
        && left.groups == right.groups
}

/// Compare only graph data represented by the admitted Mermaid subset.
pub(super) fn semantically_equivalent(left: &DiagramDocument, right: &DiagramDocument) -> bool {
    let left_nodes: HashMap<&str, (&str, NodeShape, Option<&str>)> = left
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                (
                    node.label.as_str(),
                    node.shape,
                    node.group.as_ref().map(GroupId::as_str),
                ),
            )
        })
        .collect();
    let right_nodes: HashMap<&str, (&str, NodeShape, Option<&str>)> = right
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                (
                    node.label.as_str(),
                    node.shape,
                    node.group.as_ref().map(GroupId::as_str),
                ),
            )
        })
        .collect();
    if left.direction != right.direction || left_nodes != right_nodes {
        return false;
    }

    let left_groups: HashMap<&str, &str> = left
        .groups
        .iter()
        .map(|group| (group.id.as_str(), group.label.as_str()))
        .collect();
    let right_groups: HashMap<&str, &str> = right
        .groups
        .iter()
        .map(|group| (group.id.as_str(), group.label.as_str()))
        .collect();
    if left_groups != right_groups {
        return false;
    }

    let mut left_edges = edge_semantics(left);
    let mut right_edges = edge_semantics(right);
    left_edges.sort_unstable();
    right_edges.sort_unstable();
    left_edges == right_edges
}

fn edge_semantics(graph: &DiagramDocument) -> Vec<(&str, &str, Option<&str>)> {
    graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_deref()))
        .collect()
}

fn reconcile_nodes(
    parsed: &mut DiagramDocument,
    prior: &DiagramDocument,
) -> Result<(), DiagramSessionError> {
    let old_positions: HashMap<NodeId, Position> = prior
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node.position))
        .collect();
    let mut occupied = Vec::with_capacity(parsed.nodes.len());
    for node in &mut parsed.nodes {
        if let Some(position) = old_positions.get(&node.id) {
            node.position = *position;
            occupied.push(*position);
        }
    }
    let candidates = anchor_candidates();
    let mut next_candidate = 0;
    for node in &mut parsed.nodes {
        if old_positions.contains_key(&node.id) {
            continue;
        }
        let position = deterministic_anchor(&candidates, &mut next_candidate, &occupied)?;
        node.position = position;
        occupied.push(position);
    }
    Ok(())
}

fn anchor_candidates() -> Vec<Position> {
    let half_side = (ANCHOR_GRID_SIDE / 2) as i32;
    let lower_bound = -half_side;
    let upper_bound = half_side;
    let mut candidates = Vec::with_capacity(ANCHOR_GRID_SLOTS);
    candidates.push(Position { x: 0.0, y: 0.0 });

    for radius in 1..=half_side {
        for x in -radius..=radius {
            push_anchor_candidate(x, -radius, lower_bound, upper_bound, &mut candidates);
        }
        for y in (-radius + 1)..=radius {
            push_anchor_candidate(radius, y, lower_bound, upper_bound, &mut candidates);
        }
        for x in (-radius..radius).rev() {
            push_anchor_candidate(x, radius, lower_bound, upper_bound, &mut candidates);
        }
        for y in ((-radius + 1)..radius).rev() {
            push_anchor_candidate(-radius, y, lower_bound, upper_bound, &mut candidates);
        }
    }
    debug_assert_eq!(candidates.len(), ANCHOR_GRID_SLOTS);
    candidates
}

fn push_anchor_candidate(
    x: i32,
    y: i32,
    lower_bound: i32,
    upper_bound: i32,
    candidates: &mut Vec<Position>,
) {
    if (lower_bound..upper_bound).contains(&x) && (lower_bound..upper_bound).contains(&y) {
        candidates.push(Position {
            x: x as f32 * ANCHOR_SPACING,
            y: y as f32 * ANCHOR_SPACING,
        });
    }
}

fn deterministic_anchor(
    candidates: &[Position],
    next_candidate: &mut usize,
    occupied: &[Position],
) -> Result<Position, DiagramSessionError> {
    while let Some(candidate) = candidates.get(*next_candidate).copied() {
        *next_candidate += 1;
        let separated = occupied.iter().all(|position| {
            let dx = f64::from(candidate.x) - f64::from(position.x);
            let dy = f64::from(candidate.y) - f64::from(position.y);
            dx.mul_add(dx, dy * dy) >= MIN_ANCHOR_SEPARATION * MIN_ANCHOR_SEPARATION
        });
        if separated {
            return Ok(candidate);
        }
    }
    Err(DiagramSessionError::AnchorSpaceExhausted)
}

fn reconcile_edges(
    parsed: &mut DiagramDocument,
    prior: &DiagramDocument,
    preserve_new_parser_ids: bool,
) -> Result<(), DiagramSessionError> {
    let mut exact_matches: HashMap<(NodeId, NodeId, Option<String>), VecDeque<usize>> =
        HashMap::with_capacity(prior.edges.len());
    let mut endpoint_matches: HashMap<(NodeId, NodeId), VecDeque<usize>> =
        HashMap::with_capacity(prior.edges.len());
    for (index, edge) in prior.edges.iter().enumerate() {
        exact_matches
            .entry((edge.from.clone(), edge.to.clone(), edge.label.clone()))
            .or_default()
            .push_back(index);
        endpoint_matches
            .entry((edge.from.clone(), edge.to.clone()))
            .or_default()
            .push_back(index);
    }

    let mut used = vec![false; prior.edges.len()];
    let mut assigned = vec![None; parsed.edges.len()];
    for (new_index, edge) in parsed.edges.iter().enumerate() {
        let key = (edge.from.clone(), edge.to.clone(), edge.label.clone());
        if let Some(index) = exact_matches.get_mut(&key).and_then(VecDeque::pop_front) {
            used[index] = true;
            assigned[new_index] = Some(prior.edges[index].id);
        }
    }
    for (new_index, edge) in parsed.edges.iter().enumerate() {
        if assigned[new_index].is_some() {
            continue;
        }
        let key = (edge.from.clone(), edge.to.clone());
        if let Some(queue) = endpoint_matches.get_mut(&key) {
            while let Some(index) = queue.pop_front() {
                if !used[index] {
                    used[index] = true;
                    assigned[new_index] = Some(prior.edges[index].id);
                    break;
                }
            }
        }
    }

    allocate_edge_assignments(parsed, assigned, preserve_new_parser_ids)
}

fn allocate_unmatched_edges(parsed: &mut DiagramDocument) -> Result<(), DiagramSessionError> {
    let assigned = vec![None; parsed.edges.len()];
    allocate_edge_assignments(parsed, assigned, false)
}

fn allocate_edge_assignments(
    parsed: &mut DiagramDocument,
    assigned: Vec<Option<EdgeId>>,
    preserve_new_parser_ids: bool,
) -> Result<(), DiagramSessionError> {
    for index in 0..parsed.edges.len() {
        if let Some(id) = assigned[index] {
            parsed.edges[index].id = id;
        } else if !preserve_new_parser_ids {
            let id = parsed
                .allocate_edge_id()
                .map_err(DiagramSessionError::Graph)?;
            parsed.edges[index].id = id;
        }
    }
    Ok(())
}

fn preflight_operations(operations: &[GraphOperation]) -> Result<(), DiagramSessionError> {
    for operation in operations {
        match operation {
            GraphOperation::AddNode {
                label, position, ..
            } => {
                validate_operation_label(label)?;
                validate_operation_position(*position)?;
            }
            GraphOperation::SetNodeLabel { label, .. }
            | GraphOperation::AddGroup { label, .. }
            | GraphOperation::SetGroupLabel { label, .. } => validate_operation_label(label)?,
            GraphOperation::MoveNodes { positions } => {
                if positions.len() > MAX_DIAGRAM_NODES {
                    return Err(DiagramSessionError::TooManyMoveTargets {
                        max_targets: MAX_DIAGRAM_NODES,
                    });
                }
                let mut moved = HashSet::with_capacity(positions.len());
                for target in positions {
                    validate_operation_position(target.position)?;
                    if !moved.insert(target.id.clone()) {
                        return Err(DiagramSessionError::DuplicateMoveTarget(target.id.clone()));
                    }
                }
            }
            GraphOperation::AddEdge { label, .. } | GraphOperation::SetEdgeLabel { label, .. } => {
                if let Some(label) = label {
                    validate_operation_label(label)?;
                }
            }
            GraphOperation::RemoveNode { .. }
            | GraphOperation::SetNodeShape { .. }
            | GraphOperation::SetNodeGroup { .. }
            | GraphOperation::RemoveEdge { .. }
            | GraphOperation::ReconnectEdge { .. }
            | GraphOperation::RemoveGroup { .. }
            | GraphOperation::SetDirection(_) => {}
        }
    }
    Ok(())
}

fn validate_operation_label(label: &str) -> Result<(), DiagramSessionError> {
    if label.len() > MAX_LABEL_BYTES || label.contains('\0') {
        return Err(DiagramSessionError::Graph(
            GraphValidationError::InvalidLabel,
        ));
    }
    Ok(())
}

fn validate_operation_position(position: Position) -> Result<(), DiagramSessionError> {
    Position::new(position.x, position.y)
        .map(|_| ())
        .map_err(DiagramSessionError::Graph)
}

fn apply_operations(
    graph: &mut DiagramDocument,
    operations: &[GraphOperation],
) -> Result<(), DiagramSessionError> {
    for operation in operations {
        match operation {
            GraphOperation::AddNode {
                id,
                label,
                shape,
                position,
                group,
            } => graph.nodes.push(DiagramNode {
                id: id.clone(),
                label: label.clone(),
                shape: *shape,
                position: *position,
                group: group.clone(),
            }),
            GraphOperation::RemoveNode { id } => {
                let Some(index) = graph.node_index(id) else {
                    return Err(DiagramSessionError::NodeNotFound(id.clone()));
                };
                graph.nodes.remove(index);
                graph
                    .edges
                    .retain(|edge| &edge.from != id && &edge.to != id);
            }
            GraphOperation::SetNodeLabel { id, label } => {
                let Some(node) = graph.nodes.iter_mut().find(|node| &node.id == id) else {
                    return Err(DiagramSessionError::NodeNotFound(id.clone()));
                };
                node.label.clone_from(label);
            }
            GraphOperation::SetNodeShape { id, shape } => {
                let Some(node) = graph.nodes.iter_mut().find(|node| &node.id == id) else {
                    return Err(DiagramSessionError::NodeNotFound(id.clone()));
                };
                node.shape = *shape;
            }
            GraphOperation::SetNodeGroup { id, group } => {
                let Some(node) = graph.nodes.iter_mut().find(|node| &node.id == id) else {
                    return Err(DiagramSessionError::NodeNotFound(id.clone()));
                };
                node.group = group.clone();
            }
            GraphOperation::MoveNodes { positions } => {
                if positions.len() > MAX_DIAGRAM_NODES {
                    return Err(DiagramSessionError::TooManyMoveTargets {
                        max_targets: MAX_DIAGRAM_NODES,
                    });
                }
                let mut moved = HashSet::with_capacity(positions.len());
                for target in positions {
                    if !moved.insert(target.id.clone()) {
                        return Err(DiagramSessionError::DuplicateMoveTarget(target.id.clone()));
                    }
                    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == target.id)
                    else {
                        return Err(DiagramSessionError::NodeNotFound(target.id.clone()));
                    };
                    node.position = target.position;
                }
            }
            GraphOperation::AddEdge { from, to, label } => {
                let id = graph
                    .allocate_edge_id()
                    .map_err(DiagramSessionError::Graph)?;
                graph.edges.push(DiagramEdge {
                    id,
                    from: from.clone(),
                    to: to.clone(),
                    label: label.clone(),
                });
            }
            GraphOperation::RemoveEdge { id } => {
                let Some(index) = graph.edges.iter().position(|edge| edge.id == *id) else {
                    return Err(DiagramSessionError::EdgeNotFound(*id));
                };
                graph.edges.remove(index);
            }
            GraphOperation::SetEdgeLabel { id, label } => {
                let Some(edge) = graph.edges.iter_mut().find(|edge| edge.id == *id) else {
                    return Err(DiagramSessionError::EdgeNotFound(*id));
                };
                edge.label.clone_from(label);
            }
            GraphOperation::ReconnectEdge { id, from, to } => {
                let Some(edge) = graph.edges.iter_mut().find(|edge| edge.id == *id) else {
                    return Err(DiagramSessionError::EdgeNotFound(*id));
                };
                edge.from = from.clone();
                edge.to = to.clone();
            }
            GraphOperation::AddGroup { id, label } => graph.groups.push(DiagramGroup {
                id: id.clone(),
                label: label.clone(),
            }),
            GraphOperation::RemoveGroup { id } => {
                let Some(index) = graph.groups.iter().position(|group| &group.id == id) else {
                    return Err(DiagramSessionError::GroupNotFound(id.clone()));
                };
                graph.groups.remove(index);
                for node in &mut graph.nodes {
                    if node.group.as_ref() == Some(id) {
                        node.group = None;
                    }
                }
            }
            GraphOperation::SetGroupLabel { id, label } => {
                let Some(group) = graph.groups.iter_mut().find(|group| &group.id == id) else {
                    return Err(DiagramSessionError::GroupNotFound(id.clone()));
                };
                group.label.clone_from(label);
            }
            GraphOperation::SetDirection(direction) => graph.direction = *direction,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DiagramSession, DiagramSessionError, GraphOperation, MAX_DIAGRAM_HISTORY_BYTES,
        MAX_DIAGRAM_HISTORY_ENTRIES, NodePosition, PreviewState, SessionStatus,
    };
    use crate::diagram::{FlowDirection, GroupId, NodeId, Position};

    const TWO_PARALLEL_EDGES: &str =
        "flowchart LR\nA[\"Alpha\"]\nB[\"Beta\"]\nA -->|one| B\nA -->|two| B\n";

    fn node_position(id: &str, x: f32, y: f32) -> NodePosition {
        NodePosition {
            id: NodeId::new(id).unwrap(),
            position: Position::new(x, y).unwrap(),
        }
    }

    fn baseline(session: &DiagramSession) -> Vec<u8> {
        session.encode().unwrap()
    }

    #[test]
    fn diagram_session_fresh_invalid_source_is_exact_no_preview_and_codec_roundtrips() {
        let source = "\u{feff}%% keep these bytes\r\nflowchart TD\r\nA --> B\r\n";
        let session = DiagramSession::from_fresh_source(source).unwrap();
        assert_eq!(session.draft(), source);
        assert_eq!(session.status().preview, PreviewState::NoPreview);
        assert_eq!(session.last_good_source(), None);
        assert_eq!(session.revision(), 1);
        assert_eq!(session.undo_depth(), 1);
        assert!(session.is_dirty());

        let mut decoded = DiagramSession::decode(&session.encode().unwrap()).unwrap();
        assert_eq!(decoded.draft(), source);
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
        assert_eq!(decoded.last_good_source(), None);
        assert_eq!(decoded.revision(), 1);
        assert_eq!(decoded.undo_depth(), 1);
        assert!(!decoded.is_dirty());
        assert!(decoded.undo().unwrap());
        assert_eq!(decoded.draft(), "");
        assert_eq!(decoded.status().preview, PreviewState::Current);
        assert!(decoded.redo().unwrap());
        assert_eq!(decoded.draft(), source);
        assert_eq!(decoded.status().preview, PreviewState::NoPreview);
    }

    #[test]
    fn diagram_session_fresh_supported_source_has_one_undoable_initial_edit() {
        let source = "flowchart LR\r\nA[\"Alpha\"]\r\nB[\"Beta\"]\r\nA --> B\r\n";
        let mut session = DiagramSession::from_fresh_source(source).unwrap();
        assert_eq!(session.draft(), source);
        assert_eq!(session.status().preview, PreviewState::Current);
        assert_eq!(session.revision(), 1);
        assert_eq!(session.undo_depth(), 1);
        assert!(session.is_dirty());
        assert_eq!(session.current_graph().unwrap().nodes.len(), 2);

        assert!(session.undo().unwrap());
        assert_eq!(session.draft(), "");
        assert_eq!(session.status().preview, PreviewState::Current);
        assert_eq!(session.revision(), 2);
        assert!(session.redo().unwrap());
        assert_eq!(session.draft(), source);
        assert_eq!(session.status().preview, PreviewState::Current);
    }

    #[test]
    fn diagram_session_reconciles_layout_parallel_edges_and_new_node_anchors() {
        let edited_source = "flowchart LR\nA[\"Alpha edited\"]\nB[\"Beta\"]\nC[\"Gamma\"]\nA -->|two| B\nA -->|renamed| B\nA --> C\n";
        let mut left = DiagramSession::new();
        let mut right = DiagramSession::new();
        for session in [&mut left, &mut right] {
            session.replace_source(TWO_PARALLEL_EDGES).unwrap();
            session
                .apply_transaction(&[GraphOperation::MoveNodes {
                    positions: vec![node_position("A", 0.0, 0.0), node_position("B", 400.0, 0.0)],
                }])
                .unwrap();
        }
        let previous = left.preview_graph().unwrap().clone();
        let previous_one = previous
            .edges
            .iter()
            .find(|edge| edge.label.as_deref() == Some("one"))
            .unwrap()
            .id;
        let previous_two = previous
            .edges
            .iter()
            .find(|edge| edge.label.as_deref() == Some("two"))
            .unwrap()
            .id;

        left.replace_source(edited_source).unwrap();
        right.replace_source(edited_source).unwrap();
        let graph = left.current_graph().unwrap();
        assert_eq!(graph.nodes[0].position, Position::default());
        assert_eq!(graph.nodes[1].position, Position::new(400.0, 0.0).unwrap());
        let added_node = graph
            .nodes
            .iter()
            .find(|node| node.id.as_str() == "C")
            .unwrap();
        assert_ne!(added_node.position, Position::default());
        assert_eq!(
            added_node.position,
            right
                .current_graph()
                .unwrap()
                .nodes
                .iter()
                .find(|node| node.id.as_str() == "C")
                .unwrap()
                .position
        );
        assert_eq!(
            graph
                .edges
                .iter()
                .find(|edge| edge.label.as_deref() == Some("two"))
                .unwrap()
                .id,
            previous_two
        );
        assert_eq!(
            graph
                .edges
                .iter()
                .find(|edge| edge.label.as_deref() == Some("renamed"))
                .unwrap()
                .id,
            previous_one
        );
        assert!(
            graph
                .edges
                .iter()
                .find(|edge| edge.to.as_str() == "C")
                .unwrap()
                .id
                .get()
                > previous_two.get()
        );
    }

    #[test]
    fn diagram_session_initial_anchors_are_compact_centered_and_preserve_origin() {
        let source = "flowchart TD\nA\nB\nC\n";
        let mut fresh = DiagramSession::new();
        fresh.replace_source(source).unwrap();
        let fresh_graph = fresh.current_graph().unwrap();
        let fresh_positions = fresh_graph
            .nodes
            .iter()
            .map(|node| node.position)
            .collect::<Vec<_>>();
        assert_eq!(fresh_positions[0], Position::default());
        for (index, left) in fresh_positions.iter().enumerate() {
            assert!(left.x.abs() <= 180.0 && left.y.abs() <= 180.0);
            for right in fresh_positions.iter().skip(index + 1) {
                let dx = f64::from(left.x) - f64::from(right.x);
                let dy = f64::from(left.y) - f64::from(right.y);
                assert!(dx.mul_add(dx, dy * dy) >= 120.0_f64.powi(2));
            }
        }

        let mut survivor = DiagramSession::new();
        survivor.replace_source("flowchart TD\nA\n").unwrap();
        let origin = survivor.current_graph().unwrap().nodes[0].position;
        assert_eq!(origin, Position::default());
        survivor.replace_source(source).unwrap();
        let graph = survivor.current_graph().unwrap();
        assert_eq!(graph.nodes[0].position, origin);
        assert!(
            graph
                .nodes
                .iter()
                .skip(1)
                .all(|node| node.position.x.abs() <= 360.0 && node.position.y.abs() <= 360.0)
        );
    }

    #[test]
    fn diagram_session_retains_invalid_draft_and_blocks_every_visual_edit() {
        let mut session = DiagramSession::new();
        session.replace_source(TWO_PARALLEL_EDGES).unwrap();
        let unsupported = "%% keep exactly\r\nflowchart LR\r\nA --> B\r\n";
        session.replace_source(unsupported).unwrap();
        let revision = session.revision();
        let history = session.undo_depth();
        let allocator = session.next_edge_id();
        let status = session.status();
        assert_eq!(session.draft(), unsupported);
        assert_eq!(session.last_good_source(), Some(TWO_PARALLEL_EDGES));
        assert_eq!(status.preview, PreviewState::Stale);
        assert!(status.diagnostic.is_some());
        assert!(session.current_graph().is_none());
        assert!(matches!(
            session.apply_transaction(&[GraphOperation::SetDirection(FlowDirection::LeftRight)]),
            Err(DiagramSessionError::VisualEditsBlocked)
        ));
        assert_eq!(session.revision(), revision);
        assert_eq!(session.undo_depth(), history);
        assert_eq!(session.next_edge_id(), allocator);
        assert_eq!(session.draft(), unsupported);

        let decoded = DiagramSession::decode(&session.encode().unwrap()).unwrap();
        assert_eq!(decoded.draft(), unsupported);
        assert_eq!(decoded.status().preview, PreviewState::Stale);
        let mut decoded = decoded;
        assert!(decoded.undo().unwrap());
        assert_eq!(decoded.status().preview, PreviewState::Current);
        assert!(decoded.redo().unwrap());
        assert_eq!(decoded.draft(), unsupported);
    }

    #[test]
    fn diagram_session_rejects_oversized_operation_labels_before_publication() {
        let mut session = DiagramSession::new();
        session.replace_source(TWO_PARALLEL_EDGES).unwrap();
        let before = baseline(&session);
        let huge_label = "x".repeat(crate::diagram::MAX_LABEL_BYTES + 1);
        assert!(matches!(
            session.apply_transaction(&[GraphOperation::SetNodeLabel {
                id: NodeId::new("A").unwrap(),
                label: huge_label,
            }]),
            Err(DiagramSessionError::Graph(_))
        ));
        assert_eq!(baseline(&session), before);

        let too_many_operations = vec![
            GraphOperation::SetDirection(FlowDirection::LeftRight);
            crate::diagram::MAX_DIAGRAM_TRANSACTION_OPERATIONS + 1
        ];
        assert!(matches!(
            session.apply_transaction(&too_many_operations),
            Err(DiagramSessionError::TooManyOperations { .. })
        ));
        assert_eq!(baseline(&session), before);

        let too_many_targets: Vec<_> = (0..=crate::diagram::MAX_DIAGRAM_NODES)
            .map(|index| NodePosition {
                id: NodeId::new(format!("N{index}")).unwrap(),
                position: Position::default(),
            })
            .collect();
        assert!(matches!(
            session.apply_transaction(&[GraphOperation::MoveNodes {
                positions: too_many_targets,
            }]),
            Err(DiagramSessionError::TooManyMoveTargets { .. })
        ));
        assert_eq!(baseline(&session), before);

        assert!(matches!(
            session.apply_transaction(&[GraphOperation::MoveNodes {
                positions: vec![NodePosition {
                    id: NodeId::new("A").unwrap(),
                    position: Position {
                        x: f32::NAN,
                        y: 0.0
                    },
                }],
            }]),
            Err(DiagramSessionError::Graph(_))
        ));
        assert_eq!(baseline(&session), before);
    }

    #[test]
    fn diagram_session_transactions_are_atomic_and_noops_keep_redo() {
        let mut session = DiagramSession::new();
        session.replace_source(TWO_PARALLEL_EDGES).unwrap();
        session
            .apply_transaction(&[GraphOperation::SetNodeLabel {
                id: NodeId::new("A").unwrap(),
                label: "Changed".into(),
            }])
            .unwrap();
        session.undo().unwrap();
        assert_eq!(session.redo_depth(), 1);
        let before_noop = baseline(&session);
        assert!(
            !session
                .apply_transaction(&[GraphOperation::SetDirection(FlowDirection::LeftRight)])
                .unwrap()
        );
        assert_eq!(baseline(&session), before_noop);

        let before_failure = baseline(&session);
        let unused_id = GroupId::new("Temp").unwrap();
        assert!(
            session
                .apply_transaction(&[
                    GraphOperation::AddGroup {
                        id: unused_id,
                        label: "Temporary".into(),
                    },
                    GraphOperation::AddEdge {
                        from: NodeId::new("missing").unwrap(),
                        to: NodeId::new("A").unwrap(),
                        label: None,
                    },
                ])
                .is_err()
        );
        assert_eq!(baseline(&session), before_failure);
        assert_eq!(session.redo_depth(), 1);
    }

    #[test]
    fn diagram_session_history_depth_stays_within_its_combined_entry_limit() {
        let mut session = DiagramSession::new();
        for index in 0..(MAX_DIAGRAM_HISTORY_ENTRIES + 3) {
            session
                .replace_source(format!("flowchart TD\nNode{index}\n"))
                .unwrap();
        }
        assert_eq!(session.undo_depth(), MAX_DIAGRAM_HISTORY_ENTRIES);
        for _ in 0..MAX_DIAGRAM_HISTORY_ENTRIES {
            assert!(session.undo().unwrap());
        }
        assert!(!session.can_undo());
        assert_eq!(session.draft(), "flowchart TD\nNode2\n");
        assert!(session.can_redo());
        assert!(session.encode().is_ok());
    }

    #[test]
    fn diagram_session_trims_history_to_its_exact_encoded_byte_budget() {
        let label_a = "a".repeat(crate::diagram::MAX_LABEL_BYTES);
        let label_b = "b".repeat(crate::diagram::MAX_LABEL_BYTES);
        let mut source = String::from("flowchart TD\n");
        for index in 0..12 {
            source.push_str(&format!("N{index}[\"{label_a}\"]\n"));
        }
        let mut session = DiagramSession::new();
        session.replace_source(source).unwrap();
        for index in 0..32 {
            session
                .apply_transaction(&[GraphOperation::SetNodeLabel {
                    id: NodeId::new("N0").unwrap(),
                    label: if index % 2 == 0 {
                        label_b.clone()
                    } else {
                        label_a.clone()
                    },
                }])
                .unwrap();
        }
        assert!(session.history.encoded_bytes <= MAX_DIAGRAM_HISTORY_BYTES);
        assert!(session.undo_depth() < MAX_DIAGRAM_HISTORY_ENTRIES);
        assert!(session.undo_depth() > 0);
        assert!(session.encode().is_ok());
    }

    #[test]
    fn diagram_session_batch_edits_preserve_reconnect_ids_and_clean_references() {
        let mut session = DiagramSession::new();
        session
            .replace_source("flowchart TD\nA\nB\nC\nA --> B\n")
            .unwrap();
        let edge_id = session.current_graph().unwrap().edges[0].id;
        session
            .apply_transaction(&[
                GraphOperation::AddGroup {
                    id: GroupId::new("G").unwrap(),
                    label: "Group".into(),
                },
                GraphOperation::SetGroupLabel {
                    id: GroupId::new("G").unwrap(),
                    label: "Updated group".into(),
                },
                GraphOperation::AddNode {
                    id: NodeId::new("D").unwrap(),
                    label: "New node".into(),
                    shape: crate::diagram::NodeShape::Diamond,
                    position: Position::new(800.0, 200.0).unwrap(),
                    group: Some(GroupId::new("G").unwrap()),
                },
                GraphOperation::SetNodeGroup {
                    id: NodeId::new("A").unwrap(),
                    group: Some(GroupId::new("G").unwrap()),
                },
                GraphOperation::SetNodeShape {
                    id: NodeId::new("C").unwrap(),
                    shape: crate::diagram::NodeShape::Rounded,
                },
                GraphOperation::SetEdgeLabel {
                    id: edge_id,
                    label: Some("updated edge".into()),
                },
                GraphOperation::ReconnectEdge {
                    id: edge_id,
                    from: NodeId::new("C").unwrap(),
                    to: NodeId::new("B").unwrap(),
                },
            ])
            .unwrap();
        let graph = session.current_graph().unwrap();
        assert_eq!(graph.edges[0].id, edge_id);
        assert_eq!(graph.edges[0].from.as_str(), "C");
        assert_eq!(graph.nodes[0].group.as_ref().unwrap().as_str(), "G");
        assert_eq!(graph.edges[0].label.as_deref(), Some("updated edge"));
        session
            .apply_transaction(&[
                GraphOperation::RemoveGroup {
                    id: GroupId::new("G").unwrap(),
                },
                GraphOperation::RemoveNode {
                    id: NodeId::new("B").unwrap(),
                },
            ])
            .unwrap();
        let graph = session.current_graph().unwrap();
        assert!(graph.groups.is_empty());
        assert!(graph.nodes.iter().all(|node| node.group.is_none()));
        assert!(graph.edges.is_empty());
        graph.validate().unwrap();
    }

    #[test]
    fn diagram_session_allocator_survives_undo_and_saved_ack_is_snapshot_specific() {
        let mut session = DiagramSession::new();
        session.replace_source("flowchart TD\nA\nB\n").unwrap();
        let saved = session.capture_saved_content();
        session
            .apply_transaction(&[GraphOperation::AddEdge {
                from: NodeId::new("A").unwrap(),
                to: NodeId::new("B").unwrap(),
                label: None,
            }])
            .unwrap();
        let first_id = session.current_graph().unwrap().edges[0].id;
        let delayed_save = session.capture_saved_content();
        session.undo().unwrap();
        assert!(session.is_dirty());
        session.acknowledge_saved_content(&saved);
        assert!(!session.is_dirty());
        session.redo().unwrap();
        assert!(session.is_dirty());
        session.acknowledge_saved_content(&delayed_save);
        assert!(!session.is_dirty());
        session
            .apply_transaction(&[GraphOperation::RemoveEdge { id: first_id }])
            .unwrap();
        session
            .apply_transaction(&[GraphOperation::AddEdge {
                from: NodeId::new("B").unwrap(),
                to: NodeId::new("A").unwrap(),
                label: None,
            }])
            .unwrap();
        let second_id = session.current_graph().unwrap().edges[0].id;
        assert!(second_id.get() > first_id.get());
        let captured = session.capture_saved_content();
        session
            .apply_transaction(&[GraphOperation::SetDirection(FlowDirection::LeftRight)])
            .unwrap();
        session.acknowledge_saved_content(&captured);
        assert!(session.is_dirty());
    }

    #[test]
    fn diagram_session_status_tracks_current_and_stale_supported_sources() {
        let mut session = DiagramSession::new();
        assert_eq!(
            session.status(),
            SessionStatus {
                preview: PreviewState::Current,
                diagnostic: None,
            }
        );
        session.replace_source("flowchart LR\nA\n").unwrap();
        assert_eq!(session.status().preview, PreviewState::Current);
        session.replace_source("flowchart LR\nB\n").unwrap();
        assert_eq!(session.status().preview, PreviewState::Current);
        assert_eq!(session.preview_graph().unwrap().nodes[0].id.as_str(), "B");
    }

    #[test]
    fn diagram_session_semantics_ignore_layout_identity_and_vector_order() {
        let source = "flowchart LR\nA\nB\nA -->|first| B\nA -->|second| B\n";
        let graph = crate::diagram::parse_mermaid(source).unwrap();
        let mut reordered = graph.clone();
        reordered.nodes.reverse();
        reordered.edges.reverse();
        reordered.edges.swap(0, 1);
        reordered.nodes[0].position = Position::new(240.0, -80.0).unwrap();
        let first_id = reordered.edges[0].id;
        reordered.edges[0].id = reordered.edges[1].id;
        reordered.edges[1].id = first_id;
        assert!(super::semantically_equivalent(&graph, &reordered));

        reordered.direction = FlowDirection::TopDown;
        assert!(!super::semantically_equivalent(&graph, &reordered));
    }
}
