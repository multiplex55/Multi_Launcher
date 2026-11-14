//! Deterministic initial graph layout using the shared scene geometry.

use super::model::FlowDirection;
use super::model::{
    DiagramDocument, GraphValidationError, GroupId, MAX_POSITION_ABS, NodeId, Position,
};
use super::scene::{SceneError, SceneMetrics, build_scene};
use super::session::NodePosition;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

#[derive(Debug)]
pub enum DiagramLayoutError {
    Scene(SceneError),
    MissingPreparedNode(NodeId),
    PositionOutOfBounds { node: NodeId, max_abs: f32 },
    InvalidGraph(GraphValidationError),
    InvalidComponentGraph,
}

impl fmt::Display for DiagramLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scene(error) => error.fmt(formatter),
            Self::MissingPreparedNode(id) => {
                write!(formatter, "scene geometry is missing node {id}")
            }
            Self::PositionOutOfBounds { node, max_abs } => write!(
                formatter,
                "layout position for {node} exceeds the {max_abs}-unit coordinate limit"
            ),
            Self::InvalidGraph(error) => error.fmt(formatter),
            Self::InvalidComponentGraph => {
                formatter.write_str("could not order the bounded graph components")
            }
        }
    }
}

impl std::error::Error for DiagramLayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Scene(error) => Some(error),
            Self::InvalidGraph(error) => Some(error),
            _ => None,
        }
    }
}

/// Computes stable node positions without changing IDs, labels, shapes,
/// groups, direction, or edges. Components are ranked after collapsing cycles;
/// disconnected components begin at rank zero and all ties use stable IDs.
pub fn layout_positions(graph: &DiagramDocument) -> Result<Vec<NodePosition>, DiagramLayoutError> {
    graph
        .validate()
        .map_err(|error| DiagramLayoutError::Scene(SceneError::InvalidGraph(error)))?;
    if graph.nodes.is_empty() {
        return Ok(Vec::new());
    }

    let metrics = SceneMetrics::default();
    let mut extent_graph = graph.clone();
    for node in &mut extent_graph.nodes {
        node.position = Position::default();
    }
    let extent_scene = build_scene(&extent_graph, metrics).map_err(DiagramLayoutError::Scene)?;
    let group_cross_extents = extent_scene
        .groups
        .iter()
        .map(|group| {
            let (min, max) = if matches!(
                extent_scene.direction,
                FlowDirection::TopDown | FlowDirection::TopBottom
            ) {
                (group.bounds.min.x, group.bounds.max.x)
            } else {
                (group.bounds.min.y, group.bounds.max.y)
            };
            (group.id.clone(), ((-min).max(0.0), max.max(0.0)))
        })
        .collect::<HashMap<GroupId, _>>();
    let extents = extent_scene
        .nodes
        .into_iter()
        .map(|node| (node.id, (node.bounds.width(), node.bounds.height())))
        .collect::<HashMap<_, _>>();

    let mut sorted_indices = (0..graph.nodes.len()).collect::<Vec<_>>();
    sorted_indices
        .sort_unstable_by(|left, right| graph.nodes[*left].id.cmp(&graph.nodes[*right].id));
    let mut node_index = HashMap::with_capacity(graph.nodes.len());
    for (index, node) in graph.nodes.iter().enumerate() {
        node_index.insert(node.id.clone(), index);
    }

    let mut adjacency = vec![Vec::new(); graph.nodes.len()];
    let mut reverse = vec![Vec::new(); graph.nodes.len()];
    for edge in &graph.edges {
        let from = *node_index
            .get(&edge.from)
            .ok_or_else(|| DiagramLayoutError::MissingPreparedNode(edge.from.clone()))?;
        let to = *node_index
            .get(&edge.to)
            .ok_or_else(|| DiagramLayoutError::MissingPreparedNode(edge.to.clone()))?;
        adjacency[from].push(to);
        reverse[to].push(from);
    }
    for neighbors in adjacency.iter_mut().chain(reverse.iter_mut()) {
        neighbors
            .sort_unstable_by(|left, right| graph.nodes[*left].id.cmp(&graph.nodes[*right].id));
        neighbors.dedup();
    }

    let (components, component_of) =
        strongly_connected_components(&adjacency, &reverse, &sorted_indices);
    let component_ranks = rank_components(graph, &components, &component_of, &adjacency)?;
    let node_ranks = component_of
        .iter()
        .map(|component| {
            component_ranks
                .get(*component)
                .copied()
                .ok_or(DiagramLayoutError::InvalidComponentGraph)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let max_rank = node_ranks.iter().copied().max().unwrap_or(0);

    let vertical = matches!(
        graph.direction,
        FlowDirection::TopDown | FlowDirection::TopBottom
    );
    let primary_extent = |index: usize| -> Result<f64, DiagramLayoutError> {
        let (width, height) = extents
            .get(&graph.nodes[index].id)
            .copied()
            .ok_or_else(|| {
                DiagramLayoutError::MissingPreparedNode(graph.nodes[index].id.clone())
            })?;
        Ok(if vertical { height } else { width })
    };
    let cross_extent = |index: usize| -> Result<f64, DiagramLayoutError> {
        let (width, height) = extents
            .get(&graph.nodes[index].id)
            .copied()
            .ok_or_else(|| {
                DiagramLayoutError::MissingPreparedNode(graph.nodes[index].id.clone())
            })?;
        Ok(if vertical { width } else { height })
    };

    let max_primary = sorted_indices
        .iter()
        .map(|index| primary_extent(*index))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(0.0_f64, f64::max);
    let rank_step = max_primary + metrics.group_padding + metrics.group_header_gap;
    let within_lane_gap = metrics.group_header_gap + metrics.group_padding * 0.5;
    let between_lane_gap = metrics.group_padding + metrics.group_header_gap;

    let mut lanes: BTreeMap<Option<GroupId>, BTreeMap<usize, Vec<usize>>> = BTreeMap::new();
    for index in &sorted_indices {
        let node = &graph.nodes[*index];
        lanes
            .entry(node.group.clone())
            .or_default()
            .entry(node_ranks[*index])
            .or_default()
            .push(*index);
    }

    let mut lane_spans = Vec::with_capacity(lanes.len());
    for (lane, rows) in &lanes {
        let mut widest_row = 0.0_f64;
        let mut widest_node = 0.0_f64;
        for row in rows.values() {
            let mut row_width = 0.0;
            for (position, index) in row.iter().enumerate() {
                if position > 0 {
                    row_width += within_lane_gap;
                }
                let extent = cross_extent(*index)?;
                widest_node = widest_node.max(extent);
                row_width += extent;
            }
            widest_row = widest_row.max(row_width);
        }
        let extra_row_extent = (widest_row / 2.0 - widest_node / 2.0).max(0.0);
        let (before, after) = lane
            .as_ref()
            .and_then(|id| group_cross_extents.get(id))
            .map_or((widest_row / 2.0, widest_row / 2.0), |(before, after)| {
                (before + extra_row_extent, after + extra_row_extent)
            });
        lane_spans.push((lane.clone(), rows.clone(), before, after));
    }

    let total_cross = lane_spans
        .iter()
        .map(|(_, _, before, after)| before + after)
        .sum::<f64>()
        + between_lane_gap * lane_spans.len().saturating_sub(1) as f64;
    let mut lane_cursor = -total_cross / 2.0;
    let mut positions = vec![None; graph.nodes.len()];
    for (_lane, rows, before, after) in lane_spans {
        let lane_center = lane_cursor + before;
        for (rank, row) in rows {
            let row_width = row
                .iter()
                .enumerate()
                .try_fold(0.0, |width, (position, index)| {
                    let gap = if position == 0 { 0.0 } else { within_lane_gap };
                    Ok::<_, DiagramLayoutError>(width + gap + cross_extent(*index)?)
                })?;
            let mut cross_cursor = lane_center - row_width / 2.0;
            for (row_index, index) in row.iter().enumerate() {
                let extent = cross_extent(*index)?;
                let cross = cross_cursor + extent / 2.0;
                let primary = (rank as f64 - max_rank as f64 / 2.0) * rank_step;
                let (x, y) = if vertical {
                    (cross, primary)
                } else {
                    (primary, cross)
                };
                let node_id = graph.nodes[*index].id.clone();
                positions[*index] = Some(bounded_position(node_id, x, y)?);
                cross_cursor += extent;
                if row_index + 1 < row.len() {
                    cross_cursor += within_lane_gap;
                }
            }
        }
        lane_cursor += before + after + between_lane_gap;
    }

    let output = sorted_indices
        .into_iter()
        .map(|index| {
            Ok(NodePosition {
                id: graph.nodes[index].id.clone(),
                position: positions[index].ok_or(DiagramLayoutError::InvalidComponentGraph)?,
            })
        })
        .collect::<Result<Vec<_>, DiagramLayoutError>>()?;
    let mut candidate = graph.clone();
    for move_to in &output {
        let index = candidate
            .node_index(&move_to.id)
            .ok_or_else(|| DiagramLayoutError::MissingPreparedNode(move_to.id.clone()))?;
        candidate.nodes[index].position = move_to.position;
    }
    candidate
        .validate()
        .map_err(DiagramLayoutError::InvalidGraph)?;
    build_scene(&candidate, metrics).map_err(DiagramLayoutError::Scene)?;
    Ok(output)
}

fn bounded_position(node: NodeId, x: f64, y: f64) -> Result<Position, DiagramLayoutError> {
    if !x.is_finite()
        || !y.is_finite()
        || x.abs() > f64::from(MAX_POSITION_ABS)
        || y.abs() > f64::from(MAX_POSITION_ABS)
    {
        return Err(DiagramLayoutError::PositionOutOfBounds {
            node,
            max_abs: MAX_POSITION_ABS,
        });
    }
    Position::new(x as f32, y as f32).map_err(DiagramLayoutError::InvalidGraph)
}

fn strongly_connected_components(
    adjacency: &[Vec<usize>],
    reverse: &[Vec<usize>],
    sorted_indices: &[usize],
) -> (Vec<Vec<usize>>, Vec<usize>) {
    let mut visited = vec![false; adjacency.len()];
    let mut finish_order = Vec::with_capacity(adjacency.len());
    for &root in sorted_indices {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut frames = vec![(root, 0_usize)];
        loop {
            let next = {
                let Some((node, next_child)) = frames.last_mut() else {
                    break;
                };
                if *next_child < adjacency[*node].len() {
                    let child = adjacency[*node][*next_child];
                    *next_child += 1;
                    Some(child)
                } else {
                    None
                }
            };
            if let Some(child) = next {
                if !visited[child] {
                    visited[child] = true;
                    frames.push((child, 0));
                }
            } else if let Some((finished, _)) = frames.pop() {
                finish_order.push(finished);
            } else {
                break;
            }
        }
    }

    let mut component_of = vec![usize::MAX; adjacency.len()];
    let mut components = Vec::new();
    for root in finish_order.into_iter().rev() {
        if component_of[root] != usize::MAX {
            continue;
        }
        let component_id = components.len();
        let mut component = Vec::new();
        let mut stack = vec![root];
        component_of[root] = component_id;
        while let Some(node) = stack.pop() {
            component.push(node);
            for &parent in reverse[node].iter().rev() {
                if component_of[parent] == usize::MAX {
                    component_of[parent] = component_id;
                    stack.push(parent);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    (components, component_of)
}

fn rank_components(
    graph: &DiagramDocument,
    components: &[Vec<usize>],
    component_of: &[usize],
    adjacency: &[Vec<usize>],
) -> Result<Vec<usize>, DiagramLayoutError> {
    let mut keys = Vec::with_capacity(components.len());
    for component in components {
        let key = component
            .iter()
            .filter_map(|index| graph.nodes.get(*index))
            .map(|node| node.id.clone())
            .min()
            .ok_or(DiagramLayoutError::InvalidComponentGraph)?;
        keys.push(key);
    }

    let mut successors = vec![BTreeSet::new(); components.len()];
    let mut indegree = vec![0_usize; components.len()];
    for (from, neighbors) in adjacency.iter().enumerate() {
        let source_component = component_of[from];
        for &to in neighbors {
            let target_component = component_of[to];
            if source_component != target_component
                && successors[source_component].insert(target_component)
            {
                indegree[target_component] += 1;
            }
        }
    }

    let mut ready = BTreeSet::new();
    for component in 0..components.len() {
        if indegree[component] == 0 {
            ready.insert((keys[component].clone(), component));
        }
    }
    let mut ranks = vec![0_usize; components.len()];
    let mut processed = 0;
    while let Some((key, component)) = ready.iter().next().cloned() {
        ready.remove(&(key, component));
        processed += 1;
        for successor in &successors[component] {
            ranks[*successor] = ranks[*successor].max(ranks[component] + 1);
            indegree[*successor] -= 1;
            if indegree[*successor] == 0 {
                ready.insert((keys[*successor].clone(), *successor));
            }
        }
    }
    if processed != components.len() {
        return Err(DiagramLayoutError::InvalidComponentGraph);
    }
    Ok(ranks)
}

#[cfg(test)]
mod tests {
    use super::layout_positions;
    use crate::diagram::{
        DiagramDocumentController, DiagramTemplate, GraphOperation, GroupId, NodeId, NodePosition,
        Position, PreviewState, parse_mermaid,
    };

    const GROUPED_CYCLE: &str = "flowchart LR\nsubgraph AlphaLane[\"Alpha lane with a deliberately long header that wraps across multiple lines\"]\nA[\"A\"]\nB[\"B\"]\nend\nsubgraph BetaLane[\"Beta lane\"]\nC[\"C\"]\nD[\"D\"]\nend\nU[\"Unconnected\"]\nA --> B\nB --> A\nB --> C\nC --> D\n";

    fn node_position(graph: &crate::diagram::DiagramDocument, id: &str) -> Position {
        graph
            .nodes
            .iter()
            .find(|node| node.id.as_str() == id)
            .unwrap()
            .position
    }

    fn rects_overlap(left: crate::diagram::SceneRect, right: crate::diagram::SceneRect) -> bool {
        left.min.x < right.max.x
            && left.max.x > right.min.x
            && left.min.y < right.max.y
            && left.max.y > right.min.y
    }

    #[test]
    fn diagram_layout_is_deterministic_for_cycles_disconnected_nodes_and_group_lanes() {
        let graph = parse_mermaid(GROUPED_CYCLE).unwrap();
        let first = layout_positions(&graph).unwrap();
        let second = layout_positions(&graph).unwrap();
        assert_eq!(first, second);

        let mut laid_out = graph.clone();
        for movement in &first {
            let index = laid_out.node_index(&movement.id).unwrap();
            laid_out.nodes[index].position = movement.position;
        }
        let a = node_position(&laid_out, "A");
        let b = node_position(&laid_out, "B");
        let c = node_position(&laid_out, "C");
        let d = node_position(&laid_out, "D");
        let unconnected = node_position(&laid_out, "U");
        assert_eq!(a.x, b.x, "nodes in one cycle share a rank");
        assert!(c.x > b.x);
        assert!(d.x > c.x);
        assert_eq!(unconnected.x, a.x, "disconnected nodes use rank zero");
        assert_ne!(a.y, c.y, "different groups occupy separate lanes");
        assert_eq!(laid_out.groups[0].id, GroupId::new("AlphaLane").unwrap());
        assert_eq!(laid_out.groups[1].id, GroupId::new("BetaLane").unwrap());

        for direction in [
            crate::diagram::FlowDirection::TopDown,
            crate::diagram::FlowDirection::LeftRight,
        ] {
            let mut directed = graph.clone();
            directed.direction = direction;
            let positions = layout_positions(&directed).unwrap();
            for movement in &positions {
                let index = directed.node_index(&movement.id).unwrap();
                directed.nodes[index].position = movement.position;
            }
            let scene =
                crate::diagram::build_scene(&directed, crate::diagram::SceneMetrics::default())
                    .unwrap();
            let alpha = scene
                .groups
                .iter()
                .find(|group| group.id.as_str() == "AlphaLane")
                .unwrap();
            let beta = scene
                .groups
                .iter()
                .find(|group| group.id.as_str() == "BetaLane")
                .unwrap();
            let unconnected = scene
                .nodes
                .iter()
                .find(|node| node.id.as_str() == "U")
                .unwrap();
            assert!(!rects_overlap(alpha.bounds, beta.bounds));
            assert!(!rects_overlap(alpha.bounds, unconnected.bounds));
            assert!(!rects_overlap(beta.bounds, unconnected.bounds));
        }
    }

    #[test]
    fn diagram_layout_obeys_vertical_direction_and_only_changes_positions() {
        let graph = parse_mermaid("flowchart TD\nA[\"Alpha\"]\nB[\"Beta\"]\nA --> B\n").unwrap();
        let positions = layout_positions(&graph).unwrap();
        let mut laid_out = graph.clone();
        for movement in &positions {
            let index = laid_out.node_index(&movement.id).unwrap();
            laid_out.nodes[index].position = movement.position;
        }
        assert!(node_position(&laid_out, "B").y > node_position(&laid_out, "A").y);
        assert_eq!(laid_out.direction, graph.direction);
        assert_eq!(laid_out.edges, graph.edges);
        assert_eq!(laid_out.groups, graph.groups);
        for old in &graph.nodes {
            let new = laid_out
                .nodes
                .iter()
                .find(|node| node.id == old.id)
                .unwrap();
            assert_eq!(new.id, old.id);
            assert_eq!(new.label, old.label);
            assert_eq!(new.shape, old.shape);
            assert_eq!(new.group, old.group);
        }
    }

    #[test]
    fn diagram_document_layout_is_one_undoable_position_edit_and_repeat_is_noop() {
        let mut document = DiagramDocumentController::new();
        document
            .replace_source("flowchart LR\nA[\"A\"]\nB[\"B\"]\nC[\"C\"]\nA --> B\nB --> C\n")
            .unwrap();
        document
            .apply_transaction(&[GraphOperation::MoveNodes {
                positions: vec![
                    NodePosition {
                        id: NodeId::new("A").unwrap(),
                        position: Position::default(),
                    },
                    NodePosition {
                        id: NodeId::new("B").unwrap(),
                        position: Position::default(),
                    },
                    NodePosition {
                        id: NodeId::new("C").unwrap(),
                        position: Position::default(),
                    },
                ],
            }])
            .unwrap();
        let original = document.session().unwrap().current_graph().unwrap().clone();
        let before_revision = document.session().unwrap().revision();
        let before_undo = document.session().unwrap().undo_depth();
        assert!(document.auto_layout().unwrap());
        let laid_out = document.session().unwrap().current_graph().unwrap().clone();
        assert_eq!(document.session().unwrap().revision(), before_revision + 1);
        assert_eq!(document.session().unwrap().undo_depth(), before_undo + 1);
        assert!(
            laid_out
                .nodes
                .iter()
                .any(|node| node.position != Position::default())
        );
        assert!(!document.auto_layout().unwrap());
        assert_eq!(document.session().unwrap().revision(), before_revision + 1);
        assert_eq!(document.session().unwrap().undo_depth(), before_undo + 1);
        assert!(document.undo().unwrap());
        assert_eq!(
            document.session().unwrap().current_graph().unwrap(),
            &original
        );
        assert!(document.redo().unwrap());
        assert_eq!(
            document.session().unwrap().current_graph().unwrap(),
            &laid_out
        );
    }

    #[test]
    fn diagram_document_layout_rejects_invalid_source_and_pending_replacement() {
        let mut document = DiagramDocumentController::new();
        document.replace_source("%%{unsupported}%%\n").unwrap();
        assert_eq!(
            document.session().unwrap().status().preview,
            PreviewState::Stale
        );
        assert!(document.auto_layout().is_err());

        document
            .replace_source(DiagramTemplate::Linear.source())
            .unwrap();
        document
            .request_template(DiagramTemplate::Decision)
            .unwrap();
        assert!(document.auto_layout().is_err());
        document
            .resolve_pending(super::super::TransitionDecision::Cancel)
            .unwrap();
    }
}
