//! Small executable starter scenes for the bounded native visual authoring model.

use super::visual::{
    DEFAULT_LAYER_ID, VisualAnchor, VisualColor, VisualConnector, VisualConnectorDirection,
    VisualConnectorEndpoint, VisualConnectorRoute, VisualDiagramDocument, VisualDocumentError,
    VisualObject, VisualObjectId, VisualObjectKind, VisualOwnedLabel, VisualPrimitiveShape, VisualScene, VisualStyle,
    VisualSwimlaneOrientation, VisualTransform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisualAuthoringTemplate {
    Flowchart,
    Architecture,
    MindMap,
    Sequence,
    Dependencies,
    OrganizationChart,
    NetworkTopology,
    DecisionTree,
    SwimlaneWorkflow,
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
    pub const ALL: [Self; 9] = [
        Self::Flowchart,
        Self::Architecture,
        Self::MindMap,
        Self::Sequence,
        Self::Dependencies,
        Self::OrganizationChart,
        Self::NetworkTopology,
        Self::DecisionTree,
        Self::SwimlaneWorkflow,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Flowchart => "Flowchart",
            Self::Architecture => "Architecture",
            Self::MindMap => "Mind map",
            Self::Sequence => "Sequence-style",
            Self::Dependencies => "Dependencies",
            Self::OrganizationChart => "Organization Chart",
            Self::NetworkTopology => "Network Topology",
            Self::DecisionTree => "Decision Tree",
            Self::SwimlaneWorkflow => "Swimlane Workflow",
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Flowchart => "flowchart",
            Self::Architecture => "architecture",
            Self::MindMap => "mind_map",
            Self::Sequence => "sequence",
            Self::Dependencies => "dependencies",
            Self::OrganizationChart => "organization_chart",
            Self::NetworkTopology => "network_topology",
            Self::DecisionTree => "decision_tree",
            Self::SwimlaneWorkflow => "swimlane_workflow",
        }
    }

    /// Create a fresh unsaved visual document with deterministic identities and
    /// one accepted-content baseline. All templates use only current bounded
    /// labeled primitives, title text, optional swimlanes, and anchored connectors.
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

        let mut lane_ids = Vec::new();
        let mut lane_objects = Vec::new();
        if self == Self::SwimlaneWorkflow {
            for (label, y) in [("Request", -110.0), ("Delivery", 0.0), ("Review", 110.0)] {
                let id = allocate_id(&mut next_id);
                lane_objects.push(VisualObject {
                    id,
                    layer_id: DEFAULT_LAYER_ID,
                    transform: VisualTransform { x: 0.0, y, width: 720.0, height: 94.0, rotation_degrees: 0.0 },
                    style: VisualStyle {
                        fill: VisualColor::rgba(239, 244, 249, 255),
                        border: VisualColor::rgba(104, 121, 140, 255),
                        font_size: 14.0,
                        ..VisualStyle::default()
                    },
                    kind: VisualObjectKind::Swimlane { orientation: VisualSwimlaneOrientation::Horizontal, label: label.into() },
                    owned_label: None,
                });
                lane_ids.push(id);
            }
        }
        let mut node_ids = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            if let Some(lane) = lane_objects.get(index) {
                scene.objects.push(lane.clone());
            }
            let shape_id = allocate_id(&mut next_id);
            scene.objects.push(VisualObject {
                owned_label: Some(VisualOwnedLabel {
                    text: node.label.into(),
                    style: VisualStyle {
                        fill: VisualColor::rgba(0, 0, 0, 0),
                        border: VisualColor::rgba(38, 48, 62, 255),
                        border_width: 0.0,
                        font_size: 17.0,
                        ..VisualStyle::default()
                    },
                }),
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
            if let Some(lane) = lane_ids.get(index) {
                scene.parent_by_child.insert(shape_id, *lane);
            }
            node_ids.push(shape_id);
        }

        for edge in edges {
            let from = *node_ids
                .get(edge.from)
                .ok_or(VisualDocumentError::InvalidConnectorGeometry)?;
            let to = *node_ids
                .get(edge.to)
                .ok_or(VisualDocumentError::InvalidConnectorGeometry)?;
            let (from_anchor, to_anchor) = if matches!(self, Self::OrganizationChart | Self::DecisionTree) {
                (VisualAnchor::South, VisualAnchor::North)
            } else {
                anchors(nodes[edge.from], nodes[edge.to])
            };
            let id = allocate_id(&mut next_id);
            scene.objects.push(VisualObject {
                owned_label: None,
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
                        manual_route: None,
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
            Self::OrganizationChart => (
                "Organization Chart starter",
                vec![
                    node("Director", 0.0, -130.0, 150.0, 62.0, VisualPrimitiveShape::RoundRect, violet),
                    node("Engineering", -180.0, -5.0, 150.0, 62.0, VisualPrimitiveShape::RoundRect, blue),
                    node("Operations", 180.0, -5.0, 150.0, 62.0, VisualPrimitiveShape::RoundRect, teal),
                    node("Platform", -270.0, 125.0, 135.0, 58.0, VisualPrimitiveShape::Rectangle, blue),
                    node("Product", -90.0, 125.0, 135.0, 58.0, VisualPrimitiveShape::Rectangle, blue),
                    node("Finance", 90.0, 125.0, 135.0, 58.0, VisualPrimitiveShape::Rectangle, teal),
                    node("People", 270.0, 125.0, 135.0, 58.0, VisualPrimitiveShape::Rectangle, teal),
                ],
                vec![edge(0, 1, "", VisualConnectorRoute::Straight), edge(0, 2, "", VisualConnectorRoute::Straight),
                    edge(1, 3, "", VisualConnectorRoute::Straight), edge(1, 4, "", VisualConnectorRoute::Straight),
                    edge(2, 5, "", VisualConnectorRoute::Straight), edge(2, 6, "", VisualConnectorRoute::Straight)],
            ),
            Self::NetworkTopology => (
                "Network Topology starter",
                vec![
                    node("Router", 0.0, 0.0, 145.0, 68.0, VisualPrimitiveShape::RoundRect, violet),
                    node("Internet", 0.0, -145.0, 150.0, 64.0, VisualPrimitiveShape::Ellipse, blue),
                    node("Office", -240.0, -25.0, 150.0, 64.0, VisualPrimitiveShape::Rectangle, teal),
                    node("Servers", 240.0, -25.0, 150.0, 64.0, VisualPrimitiveShape::Cylinder, amber),
                    node("Monitoring", 0.0, 145.0, 155.0, 64.0, VisualPrimitiveShape::Rectangle, blue),
                ],
                vec![edge(1, 0, "WAN", VisualConnectorRoute::Straight), edge(0, 2, "LAN", VisualConnectorRoute::Straight),
                    edge(0, 3, "service", VisualConnectorRoute::Straight), edge(0, 4, "metrics", VisualConnectorRoute::Straight),
                    edge(4, 3, "observe", VisualConnectorRoute::Straight)],
            ),
            Self::DecisionTree => (
                "Decision Tree starter",
                vec![
                    node("Ready?", 0.0, -130.0, 145.0, 72.0, VisualPrimitiveShape::Diamond, amber),
                    node("Approved?", -190.0, 0.0, 145.0, 72.0, VisualPrimitiveShape::Diamond, violet),
                    node("Urgent?", 190.0, 0.0, 145.0, 72.0, VisualPrimitiveShape::Diamond, violet),
                    node("Launch", -270.0, 130.0, 130.0, 58.0, VisualPrimitiveShape::RoundRect, teal),
                    node("Review", -90.0, 130.0, 130.0, 58.0, VisualPrimitiveShape::RoundRect, blue),
                    node("Escalate", 90.0, 130.0, 130.0, 58.0, VisualPrimitiveShape::RoundRect, amber),
                    node("Wait", 270.0, 130.0, 130.0, 58.0, VisualPrimitiveShape::RoundRect, blue),
                ],
                vec![edge(0, 1, "yes", VisualConnectorRoute::Straight), edge(0, 2, "no", VisualConnectorRoute::Straight),
                    edge(1, 3, "yes", VisualConnectorRoute::Straight), edge(1, 4, "no", VisualConnectorRoute::Straight),
                    edge(2, 5, "yes", VisualConnectorRoute::Straight), edge(2, 6, "no", VisualConnectorRoute::Straight)],
            ),
            Self::SwimlaneWorkflow => (
                "Swimlane Workflow starter",
                vec![
                    node("Submit request", -220.0, -97.0, 150.0, 50.0, VisualPrimitiveShape::RoundRect, blue),
                    node("Deliver work", 0.0, 13.0, 150.0, 50.0, VisualPrimitiveShape::Rectangle, teal),
                    node("Approve result", 220.0, 123.0, 150.0, 50.0, VisualPrimitiveShape::RoundRect, amber),
                ],
                vec![edge(0, 1, "assign", VisualConnectorRoute::Straight), edge(1, 2, "review", VisualConnectorRoute::Straight)],
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
    fn visual_authoring_template_catalog_projects_all_nine_native_starters() {
        assert_eq!(VisualAuthoringTemplate::ALL.len(), 9);
        let slugs = VisualAuthoringTemplate::ALL.map(VisualAuthoringTemplate::slug);
        assert_eq!(&slugs[..5], &["flowchart", "architecture", "mind_map", "sequence", "dependencies"]);
        let names = VisualAuthoringTemplate::ALL.map(VisualAuthoringTemplate::name);
        assert_eq!(&names[..5], &["Flowchart", "Architecture", "Mind map", "Sequence-style", "Dependencies"]);
        assert_eq!(slugs.iter().collect::<std::collections::BTreeSet<_>>().len(), 9);
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
            assert!(scene.objects.iter().any(|object| object.owned_label.is_some()));
            assert!(scene.assets.is_empty());
            assert!(
                scene
                    .objects
                    .iter()
                    .filter_map(|object| object.owned_label.as_ref().map(|label| (&label.text, label.style.border)))
                    .all(|(_, color)| color.alpha > 0)
            );
            let projection = project_visual_scene(scene).unwrap();
            assert!(!projection.items.is_empty(), "{}", template.name());
            let bounds = projection.bounds.expect("template has finite bounds");
            assert!([bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y].iter().all(|value| value.is_finite()));
            let labels = scene.objects.iter().filter_map(|object| object.owned_label.as_ref().map(|label| label.text.as_str())).collect::<Vec<_>>();
            assert!(labels.len() >= 3 && labels.iter().all(|label| !label.trim().is_empty()));
            let connectors = scene.objects.iter().filter_map(|object| match &object.kind { VisualObjectKind::Connector { connector } => Some(connector), _ => None }).collect::<Vec<_>>();
            match template {
                VisualAuthoringTemplate::OrganizationChart => {
                    assert!(labels.contains(&"Director") && labels.contains(&"Platform"));
                    assert_eq!(connectors.len(), 6);
                    assert_eq!(connectors.iter().filter(|edge| edge.from.object_id == connectors[0].from.object_id).count(), 2);
                }
                VisualAuthoringTemplate::NetworkTopology => {
                    assert!(labels.contains(&"Router") && labels.contains(&"Servers"));
                    assert_eq!(connectors.len(), 5);
                    assert!(scene.objects.iter().any(|object| matches!(object.kind, VisualObjectKind::Primitive { shape: VisualPrimitiveShape::Cylinder })));
                }
                VisualAuthoringTemplate::DecisionTree => {
                    assert!(labels.contains(&"Ready?") && labels.contains(&"Launch"));
                    assert_eq!(connectors.len(), 6);
                    assert!(connectors.iter().any(|edge| edge.label == "yes") && connectors.iter().any(|edge| edge.label == "no"));
                }
                VisualAuthoringTemplate::SwimlaneWorkflow => {
                    assert_eq!(scene.objects.iter().filter(|object| matches!(object.kind, VisualObjectKind::Swimlane { .. })).count(), 3);
                    assert_eq!(scene.parent_by_child.len(), 3);
                    for (child, parent) in &scene.parent_by_child {
                        let child_index = scene.objects.iter().position(|object| object.id == *child).unwrap();
                        assert_eq!(scene.objects[child_index - 1].id, *parent);
                        let lane = scene.objects[child_index - 1].transform;
                        let member = scene.objects[child_index].transform;
                        let header_bottom = lane.y - lane.height / 2.0 + (lane.height * 0.28).min(28.0);
                        assert!(member.y - member.height / 2.0 > header_bottom);
                    }
                    assert!(connectors.iter().all(|edge| scene.parent_by_child.get(&edge.from.object_id) != scene.parent_by_child.get(&edge.to.object_id)));
                }
                _ => {}
            }
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

    #[test]
    fn visual_authoring_new_template_edit_save_reopen_and_undo() {
        use crate::diagram::{DiagramDocumentController, TransitionOutcome, VisualOperation};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("decision-tree.mld");
        let mut document = DiagramDocumentController::new();
        assert_eq!(document.request_visual_template(VisualAuthoringTemplate::DecisionTree).unwrap(), TransitionOutcome::Applied);
        let mut node = document.visual_scene().unwrap().objects.iter().find(|object| object.owned_label.is_some()).unwrap().clone();
        let id = node.id;
        let original = node.owned_label.as_ref().unwrap().text.clone();
        node.owned_label.as_mut().unwrap().text = "Ready to ship?".into();
        node.transform.x += 25.0;
        document.apply_visual_transaction(&[VisualOperation::ReplaceObject(node)]).unwrap();
        document.save_as(&path).unwrap();
        let mut reopened = DiagramDocumentController::new();
        assert_eq!(reopened.request_open(&path).unwrap(), TransitionOutcome::Applied);
        let find = |document: &DiagramDocumentController| document.visual_scene().unwrap().objects.iter().find(|object| object.id == id).unwrap().owned_label.as_ref().unwrap().text.clone();
        assert_eq!(find(&reopened), "Ready to ship?");
        assert!(reopened.undo().unwrap());
        assert_eq!(find(&reopened), original);
        assert!(reopened.is_dirty());
    }
}
