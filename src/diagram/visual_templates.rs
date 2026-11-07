//! Small executable starter scenes for the bounded native visual authoring model.

use super::visual::{
    DEFAULT_LAYER_ID, VisualAnchor, VisualColor, VisualConnector, VisualConnectorDirection,
    VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument, VisualDocumentError,
    VisualObject, VisualObjectId, VisualObjectKind, VisualPrimitiveShape, VisualScene, VisualStyle,
    VisualTransform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisualAuthoringTemplate {
    Flowchart,
    Architecture,
    MindMap,
    Sequence,
    Dependencies,
}

#[derive(Clone, Copy)]
struct NodeSpec {
    label: &'static str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    shape: VisualPrimitiveShape,
    color: VisualColor,
}

#[derive(Clone, Copy)]
struct EdgeSpec {
    from: usize,
    to: usize,
    label: &'static str,
    route: VisualConnectorRoute,
}

impl VisualAuthoringTemplate {
    pub const ALL: [Self; 5] = [
        Self::Flowchart,
        Self::Architecture,
        Self::MindMap,
        Self::Sequence,
        Self::Dependencies,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Flowchart => "Flowchart",
            Self::Architecture => "Architecture",
            Self::MindMap => "Mind map",
            Self::Sequence => "Sequence-style",
            Self::Dependencies => "Dependencies",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Architecture => "architecture",
            Self::MindMap => "mind_map",
            Self::Sequence => "sequence",
            Self::Dependencies => "dependencies",
        }
    }

    /// Create a fresh unsaved visual document with deterministic identities and
    /// one accepted-content baseline. All templates use only current bounded
    /// primitives, free text, and anchored connectors.
    pub fn create_document(self) -> Result<VisualDiagramDocument, VisualDocumentError> {
        let (title, nodes, edges) = self.specification();
        let mut scene = VisualScene::new(title);
        let mut next_id = 2_u64;
        let title_object = scene
            .objects
            .iter_mut()
            .find(|object| object.id == super::visual::TITLE_OBJECT_ID)
            .expect("VisualScene::new always includes its title object");
        title_object.transform = VisualTransform {
            x: 0.0,
            y: -235.0,
            width: 540.0,
            height: 64.0,
            rotation_degrees: 0.0,
        };
        title_object.style.font_size = 24.0;

        let mut node_ids = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let shape_id = allocate_id(&mut next_id);
            scene.objects.push(VisualObject {
                id: shape_id,
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: node.x,
                    y: node.y,
                    width: node.width,
                    height: node.height,
                    rotation_degrees: 0.0,
                },
                style: VisualStyle {
                    fill: node.color,
                    border: VisualColor::rgba(44, 62, 80, 255),
                    border_width: 2.0,
                    ..VisualStyle::default()
                },
                kind: VisualObjectKind::Primitive { shape: node.shape },
            });
            let label_id = allocate_id(&mut next_id);
            scene.objects.push(VisualObject {
                id: label_id,
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform {
                    x: node.x,
                    y: node.y,
                    width: node.width - 24.0,
                    height: node.height - 24.0,
                    rotation_degrees: 0.0,
                },
                style: VisualStyle {
                    fill: VisualColor::rgba(0, 0, 0, 0),
                    border: VisualColor::rgba(38, 48, 62, 255),
                    border_width: 0.0,
                    font_size: 17.0,
                    ..VisualStyle::default()
                },
                kind: VisualObjectKind::Text {
                    text: node.label.into(),
                },
            });
            node_ids.push(shape_id);
        }

        for edge in edges {
            let from = *node_ids
                .get(edge.from)
                .ok_or(VisualDocumentError::InvalidConnectorGeometry)?;
            let to = *node_ids
                .get(edge.to)
                .ok_or(VisualDocumentError::InvalidConnectorGeometry)?;
            let (from_anchor, to_anchor) = anchors(nodes[edge.from], nodes[edge.to]);
            let id = allocate_id(&mut next_id);
            scene.objects.push(VisualObject {
                id,
                layer_id: DEFAULT_LAYER_ID,
                transform: VisualTransform::default(),
                style: VisualStyle {
                    fill: VisualColor::rgba(0, 0, 0, 0),
                    border: VisualColor::rgba(44, 62, 80, 255),
                    border_width: 2.0,
                    ..VisualStyle::default()
                },
                kind: VisualObjectKind::Connector {
                    connector: VisualConnector {
                        from: VisualConnectorEndpoint {
                            object_id: from,
                            anchor: from_anchor,
                        },
                        to: VisualConnectorEndpoint {
                            object_id: to,
                            anchor: to_anchor,
                        },
                        route: edge.route,
                        direction: VisualConnectorDirection::Forward,
                        label: edge.label.into(),
                    },
                },
            });
        }

        VisualDiagramDocument::from_unsaved_starter_scene(scene)
    }

    fn specification(self) -> (&'static str, Vec<NodeSpec>, Vec<EdgeSpec>) {
        let blue = VisualColor::rgba(194, 224, 250, 255);
        let teal = VisualColor::rgba(193, 235, 224, 255);
        let amber = VisualColor::rgba(255, 229, 176, 255);
        let violet = VisualColor::rgba(223, 210, 247, 255);
        match self {
            Self::Flowchart => (
                "Flowchart starter",
                vec![
                    node(
                        "Start",
                        -205.0,
                        -40.0,
                        144.0,
                        76.0,
                        VisualPrimitiveShape::Ellipse,
                        teal,
                    ),
                    node(
                        "Process",
                        0.0,
                        -40.0,
                        156.0,
                        76.0,
                        VisualPrimitiveShape::Rectangle,
                        blue,
                    ),
                    node(
                        "Finish",
                        205.0,
                        -40.0,
                        144.0,
                        76.0,
                        VisualPrimitiveShape::Ellipse,
                        teal,
                    ),
                ],
                vec![
                    edge(0, 1, "begin", VisualConnectorRoute::Straight),
                    edge(1, 2, "complete", VisualConnectorRoute::Straight),
                ],
            ),
            Self::Architecture => (
                "Architecture starter",
                vec![
                    node(
                        "Client",
                        -225.0,
                        -70.0,
                        142.0,
                        76.0,
                        VisualPrimitiveShape::RoundRect,
                        blue,
                    ),
                    node(
                        "Service",
                        -50.0,
                        -70.0,
                        142.0,
                        76.0,
                        VisualPrimitiveShape::RoundRect,
                        teal,
                    ),
                    node(
                        "API",
                        125.0,
                        -70.0,
                        142.0,
                        76.0,
                        VisualPrimitiveShape::RoundRect,
                        amber,
                    ),
                    node(
                        "Database",
                        125.0,
                        105.0,
                        166.0,
                        82.0,
                        VisualPrimitiveShape::Cylinder,
                        violet,
                    ),
                ],
                vec![
                    edge(0, 1, "request", VisualConnectorRoute::Straight),
                    edge(1, 2, "route", VisualConnectorRoute::Straight),
                    edge(2, 3, "query", VisualConnectorRoute::Straight),
                    edge(3, 1, "result", VisualConnectorRoute::Straight),
                ],
            ),
            Self::MindMap => (
                "Mind map starter",
                vec![
                    node(
                        "Project",
                        0.0,
                        5.0,
                        164.0,
                        82.0,
                        VisualPrimitiveShape::Ellipse,
                        violet,
                    ),
                    node(
                        "Research",
                        -205.0,
                        -85.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        blue,
                    ),
                    node(
                        "Build",
                        205.0,
                        -85.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        teal,
                    ),
                    node(
                        "Review",
                        -205.0,
                        112.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        amber,
                    ),
                    node(
                        "Release",
                        205.0,
                        112.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        teal,
                    ),
                ],
                vec![
                    edge(0, 1, "", VisualConnectorRoute::Curved),
                    edge(0, 2, "", VisualConnectorRoute::Curved),
                    edge(0, 3, "", VisualConnectorRoute::Curved),
                    edge(0, 4, "", VisualConnectorRoute::Curved),
                ],
            ),
            Self::Sequence => (
                "Sequence-style starter",
                vec![
                    node(
                        "User",
                        -205.0,
                        -70.0,
                        146.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        blue,
                    ),
                    node(
                        "App",
                        0.0,
                        -70.0,
                        146.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        teal,
                    ),
                    node(
                        "Store",
                        205.0,
                        -70.0,
                        146.0,
                        72.0,
                        VisualPrimitiveShape::RoundRect,
                        amber,
                    ),
                ],
                vec![
                    edge(0, 1, "submit", VisualConnectorRoute::Straight),
                    edge(1, 2, "save", VisualConnectorRoute::Straight),
                    edge(2, 1, "confirm", VisualConnectorRoute::Curved),
                    edge(1, 0, "show result", VisualConnectorRoute::Curved),
                ],
            ),
            Self::Dependencies => (
                "Dependencies starter",
                vec![
                    node(
                        "Core",
                        0.0,
                        -35.0,
                        154.0,
                        78.0,
                        VisualPrimitiveShape::Diamond,
                        violet,
                    ),
                    node(
                        "Parser",
                        -205.0,
                        120.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::Rectangle,
                        blue,
                    ),
                    node(
                        "Storage",
                        0.0,
                        120.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::Rectangle,
                        teal,
                    ),
                    node(
                        "UI",
                        205.0,
                        120.0,
                        148.0,
                        72.0,
                        VisualPrimitiveShape::Rectangle,
                        amber,
                    ),
                ],
                vec![
                    edge(1, 0, "input", VisualConnectorRoute::Straight),
                    edge(2, 0, "state", VisualConnectorRoute::Straight),
                    edge(3, 0, "view", VisualConnectorRoute::Straight),
                ],
            ),
        }
    }
}

fn anchors(from: NodeSpec, to: NodeSpec) -> (VisualAnchor, VisualAnchor) {
    let delta_x = to.x - from.x;
    let delta_y = to.y - from.y;
    if delta_x.abs() >= delta_y.abs() {
        if delta_x >= 0.0 {
            (VisualAnchor::East, VisualAnchor::West)
        } else {
            (VisualAnchor::West, VisualAnchor::East)
        }
    } else if delta_y >= 0.0 {
        (VisualAnchor::South, VisualAnchor::North)
    } else {
        (VisualAnchor::North, VisualAnchor::South)
    }
}

fn allocate_id(next: &mut u64) -> VisualObjectId {
    let id = VisualObjectId::from_raw(*next);
    *next = next.saturating_add(1);
    id
}

fn node(
    label: &'static str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    shape: VisualPrimitiveShape,
    color: VisualColor,
) -> NodeSpec {
    NodeSpec {
        label,
        x,
        y,
        width,
        height,
        shape,
        color,
    }
}

fn edge(from: usize, to: usize, label: &'static str, route: VisualConnectorRoute) -> EdgeSpec {
    EdgeSpec {
        from,
        to,
        label,
        route,
    }
}

#[cfg(test)]
mod tests {
    use super::VisualAuthoringTemplate;
    use crate::diagram::{
        TITLE_OBJECT_ID, VisualAnchor, VisualObjectKind, VisualPrimitiveShape, project_visual_scene,
    };

    #[test]
    fn visual_authoring_template_catalog_projects_all_five_native_starters() {
        assert_eq!(VisualAuthoringTemplate::ALL.len(), 5);
        for template in VisualAuthoringTemplate::ALL {
            let document = template.create_document().unwrap();
            assert!(
                document.is_dirty(),
                "{} is a new unsaved authoring choice",
                template.name()
            );
            let scene = document.scene();
            assert_eq!(
                scene.label(),
                Some(format!("{} starter", template.name()).as_str())
            );
            assert_eq!(
                scene
                    .objects
                    .iter()
                    .find(|object| object.id == TITLE_OBJECT_ID)
                    .unwrap()
                    .transform
                    .y,
                -235.0
            );
            assert!(scene.objects.iter().any(|object| matches!(
                object.kind,
                crate::diagram::VisualObjectKind::Connector { .. }
            )));
            assert!(
                scene
                    .objects
                    .iter()
                    .filter_map(|object| match &object.kind {
                        VisualObjectKind::Text { text } if object.id != TITLE_OBJECT_ID => {
                            Some((text, object.style.border))
                        }
                        _ => None,
                    })
                    .all(|(_, color)| color.alpha > 0)
            );
            let projection = project_visual_scene(scene).unwrap();
            assert!(!projection.items.is_empty(), "{}", template.name());
            assert!(projection.bounds.is_some(), "{}", template.name());
            assert!(
                projection.items.iter().any(|item| matches!(
                    item.kind,
                    crate::diagram::VisualProjectedKind::Primitive(VisualPrimitiveShape::Cylinder)
                        | crate::diagram::VisualProjectedKind::Primitive(
                            VisualPrimitiveShape::Diamond
                        )
                )) || template == VisualAuthoringTemplate::Flowchart
                    || template == VisualAuthoringTemplate::Sequence
                    || template == VisualAuthoringTemplate::MindMap
            );
        }

        let architecture = VisualAuthoringTemplate::Architecture
            .create_document()
            .unwrap();
        let connector = architecture
            .scene()
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Connector { connector } if connector.label == "query" => {
                    Some(connector)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(connector.from.anchor, VisualAnchor::South);
        assert_eq!(connector.to.anchor, VisualAnchor::North);

        let sequence = VisualAuthoringTemplate::Sequence.create_document().unwrap();
        let response = sequence
            .scene()
            .objects
            .iter()
            .find_map(|object| match &object.kind {
                VisualObjectKind::Connector { connector } if connector.label == "confirm" => {
                    Some(connector)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(response.from.anchor, VisualAnchor::West);
        assert_eq!(response.to.anchor, VisualAnchor::East);

        for template in VisualAuthoringTemplate::ALL {
            let projection =
                project_visual_scene(template.create_document().unwrap().scene()).unwrap();
            assert_eq!(
                projection.connector_route_fallbacks,
                0,
                "{}",
                template.name()
            );
        }
    }
}
