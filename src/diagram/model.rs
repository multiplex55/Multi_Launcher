//! Canonical bounded flowchart graph data.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

pub const MAX_NODE_ID_BYTES: usize = 128;
pub const MAX_GROUP_ID_BYTES: usize = 128;
pub const MAX_LABEL_BYTES: usize = 4 * 1024;
pub const MAX_DIAGRAM_NODES: usize = 1_024;
pub const MAX_DIAGRAM_EDGES: usize = 2_048;
pub const MAX_DIAGRAM_GROUPS: usize = 128;
pub const MAX_POSITION_ABS: f32 = 1_000_000.0;

/// Mermaid flowchart direction. `TB` is retained separately from `TD`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowDirection {
    #[default]
    TopDown,
    TopBottom,
    LeftRight,
}

impl FlowDirection {
    pub const fn as_mermaid(self) -> &'static str {
        match self {
            Self::TopDown => "TD",
            Self::TopBottom => "TB",
            Self::LeftRight => "LR",
        }
    }
}

/// Shapes admitted by the documented Mermaid subset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeShape {
    #[default]
    Rectangle,
    Rounded,
    Diamond,
}

/// Stable Mermaid node identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, GraphValidationError> {
        let value = value.as_ref();
        validate_identifier(value, MAX_NODE_ID_BYTES)?;
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable explicit ID for a flat subgraph.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GroupId(String);

impl GroupId {
    pub fn new(value: impl AsRef<str>) -> Result<Self, GraphValidationError> {
        let value = value.as_ref();
        validate_identifier(value, MAX_GROUP_ID_BYTES)?;
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Monotonically allocated identity for a graph edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(u64);

impl EdgeId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub fn new(x: f32, y: f32) -> Result<Self, GraphValidationError> {
        let position = Self { x, y };
        validate_position(position)?;
        Ok(position)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramGroup {
    pub id: GroupId,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramNode {
    pub id: NodeId,
    pub label: String,
    pub shape: NodeShape,
    pub position: Position,
    pub group: Option<GroupId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramEdge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub label: Option<String>,
}

/// Ordered, renderer-independent graph with its edge-ID high-water mark.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagramDocument {
    pub direction: FlowDirection,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub groups: Vec<DiagramGroup>,
    /// The next edge ID to allocate. Deleting an edge never lowers this value.
    pub next_edge_id: u64,
}

impl Default for DiagramDocument {
    fn default() -> Self {
        Self {
            direction: FlowDirection::TopDown,
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            next_edge_id: 1,
        }
    }
}

impl DiagramDocument {
    /// Validate all graph references and resource bounds before publication.
    pub fn validate(&self) -> Result<(), GraphValidationError> {
        if self.nodes.len() > MAX_DIAGRAM_NODES {
            return Err(GraphValidationError::TooManyNodes {
                max: MAX_DIAGRAM_NODES,
            });
        }
        if self.edges.len() > MAX_DIAGRAM_EDGES {
            return Err(GraphValidationError::TooManyEdges {
                max: MAX_DIAGRAM_EDGES,
            });
        }
        if self.groups.len() > MAX_DIAGRAM_GROUPS {
            return Err(GraphValidationError::TooManyGroups {
                max: MAX_DIAGRAM_GROUPS,
            });
        }
        if self.next_edge_id == 0 {
            return Err(GraphValidationError::InvalidAllocator);
        }

        let mut group_ids = HashSet::with_capacity(self.groups.len());
        let mut group_names = HashSet::with_capacity(self.groups.len());
        for group in &self.groups {
            validate_identifier(group.id.as_str(), MAX_GROUP_ID_BYTES)?;
            validate_label(&group.label)?;
            if !group_ids.insert(group.id.clone()) {
                return Err(GraphValidationError::DuplicateGroup(group.id.clone()));
            }
            group_names.insert(group.id.as_str().to_owned());
        }

        let mut node_ids = HashSet::with_capacity(self.nodes.len());
        for node in &self.nodes {
            validate_identifier(node.id.as_str(), MAX_NODE_ID_BYTES)?;
            validate_label(&node.label)?;
            validate_position(node.position)?;
            if group_names.contains(node.id.as_str()) {
                return Err(GraphValidationError::NodeGroupIdCollision(
                    node.id.as_str().to_owned(),
                ));
            }
            if !node_ids.insert(node.id.clone()) {
                return Err(GraphValidationError::DuplicateNode(node.id.clone()));
            }
            if let Some(group) = &node.group
                && !group_ids.contains(group)
            {
                return Err(GraphValidationError::MissingGroup(group.clone()));
            }
        }

        let mut edge_ids = HashSet::with_capacity(self.edges.len());
        let mut greatest_edge_id = 0;
        for edge in &self.edges {
            if edge.id.0 == 0 || !edge_ids.insert(edge.id) {
                return Err(GraphValidationError::DuplicateEdge(edge.id));
            }
            greatest_edge_id = greatest_edge_id.max(edge.id.0);
            if !node_ids.contains(&edge.from) {
                return Err(GraphValidationError::MissingNode(edge.from.clone()));
            }
            if !node_ids.contains(&edge.to) {
                return Err(GraphValidationError::MissingNode(edge.to.clone()));
            }
            if let Some(label) = &edge.label {
                validate_label(label)?;
            }
        }
        if self.next_edge_id <= greatest_edge_id {
            return Err(GraphValidationError::InvalidAllocator);
        }
        Ok(())
    }

    /// Allocate a new edge ID without ever reusing a previous high-water mark.
    pub fn allocate_edge_id(&mut self) -> Result<EdgeId, GraphValidationError> {
        if self.next_edge_id == 0
            || self
                .edges
                .iter()
                .map(|edge| edge.id.0)
                .max()
                .is_some_and(|greatest| self.next_edge_id <= greatest)
        {
            return Err(GraphValidationError::InvalidAllocator);
        }
        let next = self
            .next_edge_id
            .checked_add(1)
            .ok_or(GraphValidationError::EdgeIdExhausted)?;
        let edge_id = EdgeId(self.next_edge_id);
        self.next_edge_id = next;
        Ok(edge_id)
    }

    pub fn node_index(&self, id: &NodeId) -> Option<usize> {
        self.nodes.iter().position(|node| &node.id == id)
    }

    /// Deterministic lookup for a validated document.
    pub fn node_map(&self) -> HashMap<&NodeId, &DiagramNode> {
        self.nodes.iter().map(|node| (&node.id, node)).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphValidationError {
    InvalidIdentifier,
    NodeGroupIdCollision(String),
    DuplicateNode(NodeId),
    DuplicateGroup(GroupId),
    DuplicateEdge(EdgeId),
    MissingNode(NodeId),
    MissingGroup(GroupId),
    InvalidLabel,
    InvalidPosition,
    InvalidAllocator,
    EdgeIdExhausted,
    TooManyNodes { max: usize },
    TooManyEdges { max: usize },
    TooManyGroups { max: usize },
}

impl fmt::Display for GraphValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier => formatter.write_str("invalid graph identifier"),
            Self::NodeGroupIdCollision(id) => {
                write!(formatter, "node and subgraph IDs cannot both be {id:?}")
            }
            Self::DuplicateNode(id) => write!(formatter, "duplicate node ID {id}"),
            Self::DuplicateGroup(id) => write!(formatter, "duplicate group ID {id}"),
            Self::DuplicateEdge(id) => write!(formatter, "duplicate or zero edge ID {}", id.get()),
            Self::MissingNode(id) => write!(formatter, "edge refers to missing node {id}"),
            Self::MissingGroup(id) => write!(formatter, "node refers to missing group {id}"),
            Self::InvalidLabel => formatter.write_str("label is too long or contains a NUL byte"),
            Self::InvalidPosition => {
                formatter.write_str("position must be finite and within the coordinate limit")
            }
            Self::InvalidAllocator => {
                formatter.write_str("edge allocator is not above all allocated IDs")
            }
            Self::EdgeIdExhausted => formatter.write_str("edge ID allocator is exhausted"),
            Self::TooManyNodes { max } => write!(formatter, "diagram exceeds the {max}-node limit"),
            Self::TooManyEdges { max } => write!(formatter, "diagram exceeds the {max}-edge limit"),
            Self::TooManyGroups { max } => {
                write!(formatter, "diagram exceeds the {max}-group limit")
            }
        }
    }
}

impl std::error::Error for GraphValidationError {}

fn validate_identifier(value: &str, max_bytes: usize) -> Result<(), GraphValidationError> {
    if value.len() > max_bytes || value == "end" {
        return Err(GraphValidationError::InvalidIdentifier);
    }
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    let valid_rest = bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    let has_repeated_hyphens = value.as_bytes().windows(2).any(|pair| pair == b"--");
    if !valid_start || !valid_rest || has_repeated_hyphens {
        return Err(GraphValidationError::InvalidIdentifier);
    }
    Ok(())
}

fn validate_label(value: &str) -> Result<(), GraphValidationError> {
    if value.len() > MAX_LABEL_BYTES || value.contains('\0') {
        return Err(GraphValidationError::InvalidLabel);
    }
    Ok(())
}

fn validate_position(position: Position) -> Result<(), GraphValidationError> {
    if !position.x.is_finite()
        || !position.y.is_finite()
        || position.x.abs() > MAX_POSITION_ABS
        || position.y.abs() > MAX_POSITION_ABS
    {
        return Err(GraphValidationError::InvalidPosition);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DiagramDocument, DiagramEdge, DiagramGroup, DiagramNode, EdgeId, FlowDirection,
        GraphValidationError, GroupId, MAX_GROUP_ID_BYTES, MAX_NODE_ID_BYTES, NodeId, NodeShape,
        Position,
    };

    #[test]
    fn diagram_model_validates_ids_references_and_bounded_positions() {
        let a = NodeId::new("A").unwrap();
        let b = NodeId::new("B").unwrap();
        let group_id = GroupId::new("Decision").unwrap();
        let mut graph = DiagramDocument {
            direction: FlowDirection::LeftRight,
            nodes: vec![
                DiagramNode {
                    id: a.clone(),
                    label: "Start 🧭".into(),
                    shape: NodeShape::Rectangle,
                    position: Position::new(12.5, -4.0).unwrap(),
                    group: Some(group_id.clone()),
                },
                DiagramNode {
                    id: b.clone(),
                    label: "Finish".into(),
                    shape: NodeShape::Diamond,
                    position: Position::default(),
                    group: None,
                },
            ],
            edges: vec![DiagramEdge {
                id: EdgeId(7),
                from: a,
                to: b,
                label: Some("go".into()),
            }],
            groups: vec![DiagramGroup {
                id: group_id,
                label: "Group".into(),
            }],
            next_edge_id: 8,
        };
        assert!(graph.validate().is_ok());
        assert_eq!(graph.allocate_edge_id().unwrap().get(), 8);
        assert_eq!(graph.next_edge_id, 9);

        graph.nodes[0].position.x = f32::INFINITY;
        assert_eq!(graph.validate(), Err(GraphValidationError::InvalidPosition));
    }

    #[test]
    fn diagram_model_rejects_invalid_ids_dangling_edges_and_lowered_allocator() {
        assert!(matches!(
            NodeId::new("bad id"),
            Err(GraphValidationError::InvalidIdentifier)
        ));
        assert_eq!(
            NodeId::new("A".repeat(MAX_NODE_ID_BYTES + 1)),
            Err(GraphValidationError::InvalidIdentifier)
        );
        assert_eq!(
            GroupId::new("G".repeat(MAX_GROUP_ID_BYTES + 1)),
            Err(GraphValidationError::InvalidIdentifier)
        );
        assert_eq!(
            NodeId::new("end"),
            Err(GraphValidationError::InvalidIdentifier)
        );
        assert_eq!(
            GroupId::new("end"),
            Err(GraphValidationError::InvalidIdentifier)
        );
        assert_eq!(
            NodeId::new("A--B"),
            Err(GraphValidationError::InvalidIdentifier)
        );
        assert_eq!(
            GroupId::new("G--1"),
            Err(GraphValidationError::InvalidIdentifier)
        );
        let a = NodeId::new("A").unwrap();
        let missing = NodeId::new("Missing").unwrap();
        let mut graph = DiagramDocument::default();
        graph.nodes.push(DiagramNode {
            id: a.clone(),
            label: "A".into(),
            shape: NodeShape::Rectangle,
            position: Position::default(),
            group: None,
        });
        graph.edges.push(DiagramEdge {
            id: EdgeId(1),
            from: a,
            to: missing.clone(),
            label: None,
        });
        assert_eq!(
            graph.validate(),
            Err(GraphValidationError::MissingNode(missing))
        );

        let mut valid = DiagramDocument::default();
        let a = NodeId::new("A").unwrap();
        let b = NodeId::new("B").unwrap();
        let group_id = GroupId::new("Group").unwrap();
        valid.groups.push(DiagramGroup {
            id: group_id.clone(),
            label: "Group".into(),
        });
        valid.nodes.extend([
            DiagramNode {
                id: a.clone(),
                label: "A".into(),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: Some(group_id.clone()),
            },
            DiagramNode {
                id: b.clone(),
                label: "B".into(),
                shape: NodeShape::Rectangle,
                position: Position::default(),
                group: None,
            },
        ]);
        valid.edges.push(DiagramEdge {
            id: EdgeId(4),
            from: a.clone(),
            to: b.clone(),
            label: None,
        });
        valid.next_edge_id = 5;
        assert!(valid.validate().is_ok());

        let mut colliding_namespace = valid.clone();
        let node_group_id = GroupId::new("A").unwrap();
        colliding_namespace.groups[0].id = node_group_id.clone();
        colliding_namespace.nodes[0].group = Some(node_group_id);
        assert_eq!(
            colliding_namespace.validate(),
            Err(GraphValidationError::NodeGroupIdCollision("A".into()))
        );

        let mut duplicate_node = valid.clone();
        let duplicate = duplicate_node.nodes[0].clone();
        duplicate_node.nodes.push(duplicate);
        assert_eq!(
            duplicate_node.validate(),
            Err(GraphValidationError::DuplicateNode(a.clone()))
        );

        let mut missing_group = valid.clone();
        let missing_group_id = GroupId::new("Missing").unwrap();
        missing_group.nodes[0].group = Some(missing_group_id.clone());
        assert_eq!(
            missing_group.validate(),
            Err(GraphValidationError::MissingGroup(missing_group_id))
        );

        let mut duplicate_group = valid.clone();
        let duplicate = duplicate_group.groups[0].clone();
        duplicate_group.groups.push(duplicate);
        assert_eq!(
            duplicate_group.validate(),
            Err(GraphValidationError::DuplicateGroup(group_id))
        );

        let mut duplicate_edge = valid.clone();
        let duplicate = duplicate_edge.edges[0].clone();
        duplicate_edge.edges.push(duplicate);
        assert_eq!(
            duplicate_edge.validate(),
            Err(GraphValidationError::DuplicateEdge(EdgeId(4)))
        );

        let mut lowered = valid.clone();
        lowered.next_edge_id = 4;
        assert_eq!(
            lowered.validate(),
            Err(GraphValidationError::InvalidAllocator)
        );

        let mut zero_allocator = DiagramDocument::default();
        zero_allocator.next_edge_id = 0;
        assert_eq!(
            zero_allocator.validate(),
            Err(GraphValidationError::InvalidAllocator)
        );
    }
}
