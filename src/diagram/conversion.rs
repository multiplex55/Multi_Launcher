//! Explicit, nondestructive conversions between Diagram authoring modes.

use super::model::{NodeId, NodeShape};
use super::scene::{SceneError, SceneMetrics, ScenePoint, build_scene};
use super::session::DiagramSession;
use super::visual::{
    DEFAULT_LAYER_ID, MAX_VISUAL_OBJECTS, TITLE_OBJECT_ID, VisualAnchor, VisualConnector,
    VisualConnectorDirection, VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument,
    VisualDocumentError, VisualLayer, VisualObject, VisualObjectId, VisualObjectKind,
    VisualPrimitiveShape, VisualScene, VisualStyle, VisualTransform,
};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub enum MermaidVisualCopyError {
    NotCurrentNativeFlowchart,
    GroupedFlowchartUnsupported,
    TooManyObjects { required: usize, max: usize },
    Scene(SceneError),
    Visual(VisualDocumentError),
}

impl fmt::Display for MermaidVisualCopyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotCurrentNativeFlowchart => formatter.write_str(
                "conversion requires a valid current Mermaid flowchart in the native supported subset",
            ),
            Self::GroupedFlowchartUnsupported => formatter.write_str(
                "conversion of grouped Mermaid flowcharts is not supported; the source document was left unchanged",
            ),
            Self::TooManyObjects { required, max } => write!(
                formatter,
                "conversion needs {required} visual objects, exceeding the {max}-object limit",
            ),
            Self::Scene(error) => write!(formatter, "could not prepare Mermaid geometry: {error}"),
            Self::Visual(error) => write!(formatter, "converted scene is not a valid visual document: {error}"),
        }
    }
}

impl std::error::Error for MermaidVisualCopyError {}

/// Build a new clean visual copy from the session's current native flowchart.
/// The caller retains the Mermaid session and publishes this document under a
/// different path only after the conversion has passed all bounded checks.
pub fn mermaid_visual_copy(
    session: &DiagramSession,
) -> Result<VisualDiagramDocument, MermaidVisualCopyError> {
    let graph = session
        .current_graph()
        .ok_or(MermaidVisualCopyError::NotCurrentNativeFlowchart)?;
    graph
        .validate()
        .map_err(|error| MermaidVisualCopyError::Scene(SceneError::InvalidGraph(error)))?;
    if !graph.groups.is_empty() || graph.nodes.iter().any(|node| node.group.is_some()) {
        return Err(MermaidVisualCopyError::GroupedFlowchartUnsupported);
    }

    // Scene geometry is the same bounded geometry already used by preview and
    // layout. Conversion never estimates node sizes from source string lengths.
    let object_count = graph
        .nodes
        .len()
        .checked_add(graph.edges.len())
        .and_then(|count| count.checked_add(1))
        .ok_or(MermaidVisualCopyError::TooManyObjects {
            required: usize::MAX,
            max: MAX_VISUAL_OBJECTS,
        })?;
    if object_count > MAX_VISUAL_OBJECTS {
        return Err(MermaidVisualCopyError::TooManyObjects {
            required: object_count,
            max: MAX_VISUAL_OBJECTS,
        });
    }

    let prepared =
        build_scene(graph, SceneMetrics::default()).map_err(MermaidVisualCopyError::Scene)?;

    // The required stable title object remains present but inert so it neither
    // adds visible text nor collides with the copied Mermaid geometry.
    let mut scene = VisualScene::new("");
    if let Some(first) = prepared.nodes.first() {
        let title = scene.objects.first_mut().expect("VisualScene has a title");
        title.transform = VisualTransform {
            x: first.bounds.center().x,
            y: first.bounds.center().y,
            width: 0.1,
            height: 0.1,
            rotation_degrees: 0.0,
        };
    }
    scene.layers = vec![VisualLayer {
        id: DEFAULT_LAYER_ID,
        name: "Layer 1".into(),
    }];
    scene.objects.reserve(object_count.saturating_sub(1));
    let mut next_id = TITLE_OBJECT_ID.get().saturating_add(1);
    let mut object_ids = HashMap::<NodeId, VisualObjectId>::with_capacity(prepared.nodes.len());
    let mut node_centers = HashMap::<NodeId, ScenePoint>::with_capacity(prepared.nodes.len());

    for (node, source_node) in prepared.nodes.iter().zip(&graph.nodes) {
        let object_id = allocate_id(&mut next_id);
        object_ids.insert(node.id.clone(), object_id);
        node_centers.insert(node.id.clone(), node.bounds.center());
        scene.objects.push(VisualObject {
            owned_label: Some(super::visual::VisualOwnedLabel {
                text: source_node.label.clone(),
                style: VisualStyle {
                    font_size: 12.8,
                    ..VisualStyle::default()
                },
            }),
            id: object_id,
            layer_id: DEFAULT_LAYER_ID,
            transform: VisualTransform {
                x: node.bounds.center().x,
                y: node.bounds.center().y,
                width: node.bounds.width(),
                height: node.bounds.height(),
                rotation_degrees: 0.0,
            },
            style: VisualStyle::default(),
            kind: VisualObjectKind::Primitive {
                shape: match node.shape {
                    NodeShape::Rectangle => VisualPrimitiveShape::Rectangle,
                    NodeShape::Rounded => VisualPrimitiveShape::RoundRect,
                    NodeShape::Diamond => VisualPrimitiveShape::Diamond,
                },
            },
        });
    }

    for (edge, source_edge) in prepared.edges.iter().zip(&graph.edges) {
        let from = object_ids.get(&edge.from).copied().ok_or_else(|| {
            MermaidVisualCopyError::Scene(SceneError::InvalidGraph(
                super::model::GraphValidationError::MissingNode(edge.from.clone()),
            ))
        })?;
        let to = object_ids.get(&edge.to).copied().ok_or_else(|| {
            MermaidVisualCopyError::Scene(SceneError::InvalidGraph(
                super::model::GraphValidationError::MissingNode(edge.to.clone()),
            ))
        })?;
        let source_center = *node_centers.get(&edge.from).ok_or_else(|| {
            MermaidVisualCopyError::Scene(SceneError::InvalidGraph(
                super::model::GraphValidationError::MissingNode(edge.from.clone()),
            ))
        })?;
        let target_center = *node_centers.get(&edge.to).ok_or_else(|| {
            MermaidVisualCopyError::Scene(SceneError::InvalidGraph(
                super::model::GraphValidationError::MissingNode(edge.to.clone()),
            ))
        })?;
        let (from_anchor, to_anchor) = anchors_between(source_center, target_center);
        scene.objects.push(VisualObject {
            owned_label: None,
            id: allocate_id(&mut next_id),
            layer_id: DEFAULT_LAYER_ID,
            // Connector geometry is derived from endpoint references.
            transform: VisualTransform::default(),
            style: VisualStyle::default(),
            kind: VisualObjectKind::Connector {
                connector: VisualConnector {
                    manual_route: None,
                    from: VisualConnectorEndpoint {
                        object_id: from,
                        anchor: from_anchor,
                    },
                    to: VisualConnectorEndpoint {
                        object_id: to,
                        anchor: to_anchor,
                    },
                    route: VisualConnectorRoute::Curved,
                    direction: VisualConnectorDirection::Forward,
                    label: source_edge.label.clone().unwrap_or_default(),
                },
            },
        });
    }

    VisualDiagramDocument::from_fresh_scene(scene).map_err(MermaidVisualCopyError::Visual)
}

fn allocate_id(next_id: &mut u64) -> VisualObjectId {
    let id = VisualObjectId::from_raw(*next_id);
    *next_id = next_id.saturating_add(1);
    id
}

fn anchors_between(from: ScenePoint, to: ScenePoint) -> (VisualAnchor, VisualAnchor) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (VisualAnchor::East, VisualAnchor::West)
        } else {
            (VisualAnchor::West, VisualAnchor::East)
        }
    } else if dy >= 0.0 {
        (VisualAnchor::South, VisualAnchor::North)
    } else {
        (VisualAnchor::North, VisualAnchor::South)
    }
}

#[cfg(test)]
mod tests {
    use super::{MermaidVisualCopyError, mermaid_visual_copy};
    use crate::diagram::{
        DiagramSession, VisualConnectorDirection, VisualObjectKind, VisualPrimitiveShape,
    };

    fn session(source: &str) -> DiagramSession {
        let mut session = DiagramSession::new();
        session.replace_source(source).expect("set Mermaid source");
        session
    }

    #[test]
    fn mermaid_visual_copy_preserves_native_geometry_labels_and_edges_without_editing_source() {
        let source =
            "flowchart LR\nA[\"東京#10;Caption\"] -->|stores| B{\"Choice\"}\nB --> C(\"Cached\")\n";
        let session = session(source);
        let revision = session.revision();
        let draft = session.draft().to_owned();
        let copy = mermaid_visual_copy(&session).expect("build separate visual document");
        let scene = copy.scene();

        assert_eq!(session.revision(), revision);
        assert_eq!(session.draft(), draft);
        assert!(!copy.is_dirty());
        assert_eq!(copy.revision(), 0);
        assert_eq!(copy.undo_depth(), 0);
        assert_eq!(
            scene
                .objects
                .iter()
                .filter_map(|object| match &object.kind {
                    VisualObjectKind::Primitive { shape } => Some(*shape),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [
                VisualPrimitiveShape::Rectangle,
                VisualPrimitiveShape::Diamond,
                VisualPrimitiveShape::RoundRect
            ]
        );
        assert!(scene.objects.iter().any(|object| {
            object
                .owned_label
                .as_ref()
                .is_some_and(|label| label.text == "東京\nCaption")
        }));
        let projection = crate::diagram::project_visual_scene(scene).expect("project copy");
        for expected in ["東京", "Caption", "Choice", "Cached", "stores"] {
            assert!(
                projection
                    .items
                    .iter()
                    .any(|item| item.text.as_ref().is_some_and(|text| {
                        text.lines.iter().any(|line| line.text == expected)
                    })),
                "projected copy should retain {expected:?}"
            );
        }
        let connector = scene
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Connector { connector } if connector.label == "stores" => {
                    Some(connector)
                }
                _ => None,
            })
            .expect("labeled connector");
        assert_eq!(connector.direction, VisualConnectorDirection::Forward);
        assert_eq!(copy.validate(), Ok(()));

        let long = "1234567890123456789012345678901234567890";
        let long_session = self::session(&format!("flowchart TD\nA[\"{long}\"]\n"));
        let long_copy = mermaid_visual_copy(&long_session).unwrap();
        let projection = crate::diagram::project_visual_scene(long_copy.scene()).unwrap();
        let label = projection.items.iter().find_map(|item| {
            item.text
                .as_ref()
                .filter(|text| text.lines.iter().any(|line| line.text == long))
        });
        assert!(
            label.is_some(),
            "conversion must not wrap a fitting source label"
        );
    }

    #[test]
    fn mermaid_visual_copy_rejects_grouped_invalid_and_non_native_sources() {
        let grouped = session("flowchart TD\nsubgraph G[\"Group\"]\nA\nend\n");
        assert!(matches!(
            mermaid_visual_copy(&grouped),
            Err(MermaidVisualCopyError::GroupedFlowchartUnsupported)
        ));

        let mut invalid = session("flowchart TD\nA\n");
        invalid.replace_source("flowchart TD\nA -->\n").unwrap();
        assert!(matches!(
            mermaid_visual_copy(&invalid),
            Err(MermaidVisualCopyError::NotCurrentNativeFlowchart)
        ));

        let generic = session("sequenceDiagram\nA->>B: hello\n");
        assert!(matches!(
            mermaid_visual_copy(&generic),
            Err(MermaidVisualCopyError::NotCurrentNativeFlowchart)
        ));
    }

    #[test]
    fn mermaid_visual_copy_refuses_object_limit_without_truncating() {
        let mut source = String::from("flowchart TD\n");
        for index in 0..1024 {
            source.push_str(&format!("N{index:03}\n"));
        }
        let session = session(&source);
        assert!(matches!(
            mermaid_visual_copy(&session),
            Err(MermaidVisualCopyError::TooManyObjects {
                required: 1025,
                max: 1024
            })
        ));
    }
}
