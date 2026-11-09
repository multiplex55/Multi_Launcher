//! Native graph and source-rendered Mermaid starter documents.

use super::rendering::{DiagramFamily, generic_preview_family};
use super::session::{DiagramSession, DiagramSessionError, PreviewState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagramTemplate {
    Linear,
    Decision,
    Process,
    Mindmap,
    Sequence,
    Class,
    State,
    EntityRelationship,
    Gantt,
    Timeline,
    UserJourney,
    Pie,
    GitGraph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramTemplateCapability {
    NativeGraph,
    SourceRendered(DiagramFamily),
}

impl DiagramTemplate {
    pub const ALL: [Self; 13] = [
        Self::Linear,
        Self::Decision,
        Self::Process,
        Self::Mindmap,
        Self::Sequence,
        Self::Class,
        Self::State,
        Self::EntityRelationship,
        Self::Gantt,
        Self::Timeline,
        Self::UserJourney,
        Self::Pie,
        Self::GitGraph,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Decision => "Decision",
            Self::Process => "Process",
            Self::Mindmap => "Mindmap",
            Self::Sequence => "Sequence",
            Self::Class => "Class",
            Self::State => "State",
            Self::EntityRelationship => "Entity Relationship",
            Self::Gantt => "Gantt",
            Self::Timeline => "Timeline",
            Self::UserJourney => "User Journey",
            Self::Pie => "Pie",
            Self::GitGraph => "GitGraph",
        }
    }

    pub const fn capability(self) -> DiagramTemplateCapability {
        match self {
            Self::Linear | Self::Decision | Self::Process => DiagramTemplateCapability::NativeGraph,
            Self::Mindmap => DiagramTemplateCapability::SourceRendered(DiagramFamily::Mindmap),
            Self::Sequence => DiagramTemplateCapability::SourceRendered(DiagramFamily::Sequence),
            Self::Class => DiagramTemplateCapability::SourceRendered(DiagramFamily::Class),
            Self::State => DiagramTemplateCapability::SourceRendered(DiagramFamily::State),
            Self::EntityRelationship => {
                DiagramTemplateCapability::SourceRendered(DiagramFamily::EntityRelationship)
            }
            Self::Gantt => DiagramTemplateCapability::SourceRendered(DiagramFamily::Gantt),
            Self::Timeline => DiagramTemplateCapability::SourceRendered(DiagramFamily::Timeline),
            Self::UserJourney => {
                DiagramTemplateCapability::SourceRendered(DiagramFamily::UserJourney)
            }
            Self::Pie => DiagramTemplateCapability::SourceRendered(DiagramFamily::Pie),
            Self::GitGraph => DiagramTemplateCapability::SourceRendered(DiagramFamily::GitGraph),
        }
    }

    pub const fn family(self) -> DiagramFamily {
        match self.capability() {
            DiagramTemplateCapability::NativeGraph => DiagramFamily::Flowchart,
            DiagramTemplateCapability::SourceRendered(family) => family,
        }
    }

    /// Mermaid source is the portable definition and remains the session's
    /// exact source of truth after installation.
    pub const fn source(self) -> &'static str {
        match self {
            Self::Linear => {
                "flowchart LR\nStart[\"Start\"]\nStep[\"Step\"]\nFinish[\"Finish\"]\nStart --> Step\nStep --> Finish\n"
            }
            Self::Decision => {
                "flowchart TD\nStart[\"Start\"]\nDecision{\"Decision\"}\nYes[\"Yes\"]\nNo[\"No\"]\nFinish[\"Finish\"]\nStart --> Decision\nDecision -->|Yes| Yes\nDecision -->|No| No\nYes --> Finish\nNo --> Finish\n"
            }
            Self::Process => {
                "flowchart TD\nInput[\"Input\"]\nPrepare[\"Prepare\"]\nProcess[\"Process\"]\nReview{\"Review\"}\nOutput[\"Output\"]\nInput --> Prepare\nPrepare --> Process\nProcess --> Review\nReview -->|Accept| Output\nReview -->|Revise| Prepare\n"
            }
            Self::Mindmap => {
                "mindmap\n  root((Product launch))\n    Research\n      Customer interviews\n      Competitor review\n    Build\n      Prototype\n      Usability test\n    Release\n"
            }
            Self::Sequence => {
                "sequenceDiagram\n    participant User\n    participant App\n    participant Store\n    User->>App: Submit order\n    App->>Store: Save order\n    Store-->>App: Confirm saved\n    App-->>User: Show confirmation\n"
            }
            Self::Class => {
                "classDiagram\nclass Customer {\n  +String name\n  +placeOrder()\n}\nclass Order {\n  +String number\n  +submit()\n}\nCustomer \"1\" --> \"many\" Order : places\n"
            }
            Self::State => {
                "stateDiagram-v2\n    [*] --> Draft\n    Draft --> Review : submit\n    Review --> Draft : request changes\n    Review --> Published : approve\n    Published --> [*]\n"
            }
            Self::EntityRelationship => {
                "erDiagram\n    CUSTOMER ||--o{ ORDER : places\n    ORDER ||--|{ LINE_ITEM : contains\n    PRODUCT ||--o{ LINE_ITEM : appears in\n"
            }
            Self::Gantt => {
                "gantt\n    title Product launch\n    dateFormat YYYY-MM-DD\n    section Plan\n    Research :done, research, 2026-01-01, 5d\n    Prototype :active, prototype, after research, 4d\n    section Deliver\n    Release :release, after prototype, 2d\n"
            }
            Self::Timeline => {
                "timeline\n    title Product milestones\n    section Discovery\n    2024 : Interviews\n    2025 : Prototype\n    section Delivery\n    2026 : Public launch\n"
            }
            Self::UserJourney => {
                "journey\n    title New customer onboarding\n    section Discover\n      Find the service: 5: Visitor\n      Compare plans: 3: Visitor\n    section Start\n      Create an account: 4: Visitor, Support\n      Complete first task: 5: Customer\n"
            }
            Self::Pie => {
                "pie showData\n    title Monthly budget\n    \"Housing\" : 45\n    \"Food\" : 25\n    \"Transport\" : 15\n    \"Savings\" : 15\n"
            }
            Self::GitGraph => {
                "gitGraph\n    commit id: \"Plan\"\n    branch feature\n    commit id: \"Build feature\"\n    checkout main\n    commit id: \"Prepare release\"\n    merge feature\n"
            }
        }
    }

    /// Build a fresh template with its captured source color envelope.
    pub fn create_styled_session(self, style: &super::style::MermaidStyle) -> Result<DiagramSession, DiagramTemplateError> {
        self.create_session_with_style(Some(style))
    }

    pub fn create_session(self) -> Result<DiagramSession, DiagramTemplateError> {
        self.create_session_with_style(None)
    }

    pub fn create_session_with_style(self, style: Option<&super::style::MermaidStyle>) -> Result<DiagramSession, DiagramTemplateError> {
        let source = match style {
            Some(style) => super::style::edit_mermaid_style(self.source(), style).map_err(|_| DiagramTemplateError::InvalidDefinition(self))?,
            None => self.source().to_owned(),
        };
        let session = DiagramSession::from_fresh_source(source).map_err(DiagramTemplateError::Session)?;
        let valid_for_capability = match self.capability() {
            DiagramTemplateCapability::NativeGraph => {
                session.status().preview == PreviewState::Current
                    && session.current_graph().is_some()
            }
            DiagramTemplateCapability::SourceRendered(family) => {
                generic_preview_family(session.draft()) == Some(family)
                    && session.current_graph().is_none()
            }
        };
        if !valid_for_capability {
            return Err(DiagramTemplateError::InvalidDefinition(self));
        }
        Ok(session)
    }
}

#[derive(Debug)]
pub enum DiagramTemplateError {
    Session(DiagramSessionError),
    InvalidDefinition(DiagramTemplate),
}

impl std::fmt::Display for DiagramTemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(error) => error.fmt(formatter),
            Self::InvalidDefinition(template) => {
                write!(
                    formatter,
                    "the built-in {} template is invalid for its declared capability",
                    template.name()
                )
            }
        }
    }
}

impl std::error::Error for DiagramTemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            Self::InvalidDefinition(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagramTemplate, DiagramTemplateCapability};
    use crate::diagram::{
        DiagramFamily, FlowDirection, NodeShape, PreviewState, RasterSize, parse_mermaid,
        render_mermaid_preview,
    };

    #[test]
    fn native_diagram_templates_remain_current_editable_graphs() {
        for template in [
            DiagramTemplate::Linear,
            DiagramTemplate::Decision,
            DiagramTemplate::Process,
        ] {
            assert_eq!(
                template.capability(),
                DiagramTemplateCapability::NativeGraph
            );
            assert!(
                parse_mermaid(template.source()).is_ok(),
                "{}",
                template.name()
            );
            let session = template.create_session().unwrap();
            assert_eq!(session.status().preview, PreviewState::Current);
            assert!(session.current_graph().is_some());
            assert_eq!(session.draft(), template.source());
            assert!(session.is_dirty());
        }

        let linear = DiagramTemplate::Linear.create_session().unwrap();
        let graph = linear.current_graph().unwrap();
        assert_eq!(graph.direction, FlowDirection::LeftRight);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);

        for template in [DiagramTemplate::Decision, DiagramTemplate::Process] {
            let session = template.create_session().unwrap();
            let graph = session.current_graph().unwrap();
            assert_eq!(graph.direction, FlowDirection::TopDown);
            assert_eq!(graph.edges.len(), 5);
            assert!(
                graph
                    .nodes
                    .iter()
                    .any(|node| node.shape == NodeShape::Diamond)
            );
        }
    }

    #[test]
    fn generic_diagram_templates_keep_exact_source_without_claiming_native_graphs() {
        for template in DiagramTemplate::ALL {
            let DiagramTemplateCapability::SourceRendered(family) = template.capability() else {
                continue;
            };
            let session = template.create_session().unwrap();
            assert_eq!(template.family(), family);
            assert_eq!(session.draft(), template.source());
            assert!(session.current_graph().is_none(), "{}", template.name());
            assert_ne!(session.status().preview, PreviewState::Current);
            assert!(session.is_dirty());
        }
    }

    #[test]
    fn all_thirteen_templates_render_through_the_production_adapter() {
        let raster_size = RasterSize::new(640, 420).unwrap();
        let expected = [
            (DiagramTemplate::Linear, DiagramFamily::Flowchart, "Start"),
            (
                DiagramTemplate::Decision,
                DiagramFamily::Flowchart,
                "Decision",
            ),
            (
                DiagramTemplate::Process,
                DiagramFamily::Flowchart,
                "Process",
            ),
            (
                DiagramTemplate::Mindmap,
                DiagramFamily::Mindmap,
                "Product launch",
            ),
            (
                DiagramTemplate::Sequence,
                DiagramFamily::Sequence,
                "Submit order",
            ),
            (DiagramTemplate::Class, DiagramFamily::Class, "Customer"),
            (DiagramTemplate::State, DiagramFamily::State, "Draft"),
            (
                DiagramTemplate::EntityRelationship,
                DiagramFamily::EntityRelationship,
                "CUSTOMER",
            ),
            (
                DiagramTemplate::Gantt,
                DiagramFamily::Gantt,
                "Product launch",
            ),
            (
                DiagramTemplate::Timeline,
                DiagramFamily::Timeline,
                "Product milestones",
            ),
            (
                DiagramTemplate::UserJourney,
                DiagramFamily::UserJourney,
                "New customer onboarding",
            ),
            (DiagramTemplate::Pie, DiagramFamily::Pie, "Monthly budget"),
            (DiagramTemplate::GitGraph, DiagramFamily::GitGraph, "Plan"),
        ];

        assert_eq!(DiagramTemplate::ALL.len(), expected.len());
        assert_eq!(
            DiagramTemplate::ALL,
            expected.map(|(template, _, _)| template)
        );
        let text_uses_color = |svg: &str, label: &str, color: &str| {
            svg.match_indices(label).any(|(offset, _)| {
                let before = &svg[..offset];
                before.rfind("<text").and_then(|start| before[start..].split('>').next())
                    .is_some_and(|element| element.contains(&format!("fill=\"{color}\"")))
            })
        };
        for (template, expected_family, label) in expected {
            let output =
                render_mermaid_preview(template.source(), raster_size).unwrap_or_else(|error| {
                    panic!("{} template failed to render: {error}", template.name())
                });
            assert_eq!(output.family, expected_family, "{}", template.name());
            assert!(
                output.svg.contains(label),
                "{} SVG does not contain useful template text {label:?}",
                template.name()
            );
            assert!(output.natural_width.is_finite() && output.natural_width > 0.0);
            assert!(output.natural_height.is_finite() && output.natural_height > 0.0);
            let colors = "%%{init: {\"themeVariables\":{\"primaryColor\":\"#123456\",\"primaryBorderColor\":\"#345678\",\"primaryTextColor\":\"#f1f2f3\",\"lineColor\":\"#789abc\",\"background\":\"#09111a\",\"secondaryColor\":\"#234567\",\"tertiaryColor\":\"#456789\",\"timelineEventColors\":[\"#b10001\",\"#b20002\"],\"pieColors\":[\"#c10001\",\"#c20002\"],\"gitColors\":[\"#d10001\",\"#d20002\"],\"gitBranchLabelColors\":[\"#e10001\",\"#e20002\"]}}}%%\n";
            let styled_source = format!("{colors}{}", template.source());
            let styled = render_mermaid_preview(&styled_source, raster_size)
                .unwrap_or_else(|error| panic!("{} styled template failed: {error}", template.name()));
            assert_eq!(styled.family, expected_family);
            let expected_fill = match template {
                DiagramTemplate::EntityRelationship => "#234567",
                DiagramTemplate::Gantt => "#345678",
                DiagramTemplate::Timeline => "#b10001",
                DiagramTemplate::Pie => "#c10001",
                DiagramTemplate::GitGraph => "#d10001",
                _ => "#123456",
            };
            let attribute = format!("fill=\"{expected_fill}\"");
            if template != DiagramTemplate::Gantt {
                assert!(styled.svg.split('<').any(|tag| {
                    ["rect ", "path ", "circle ", "ellipse ", "polygon "].iter().any(|shape| tag.starts_with(shape))
                        && tag.split('>').next().is_some_and(|element| element.contains(&attribute))
                }), "{} has no shape filled {expected_fill}", template.name());
            } else {
                let task_fills = |svg: &str| svg.split('<').filter(|tag| tag.starts_with("rect ") && tag.contains("rx=\"3\"") && tag.contains("stroke-width=\"1\""))
                    .filter_map(|tag| tag.split("fill=\"").nth(1)?.split('\"').next().map(str::to_owned)).collect::<Vec<_>>();
                let original = task_fills(&output.svg);
                let changed = task_fills(&styled.svg);
                assert!(!original.is_empty() && original != changed, "sectioned Gantt task fills must change");
                assert!(styled.svg.contains("stroke=\"#345678\""));
            }
            let dark_source = format!("%%{{init: {{\"theme\":\"dark\"}}}}%%\n{}", template.source());
            let session = template.create_styled_session(&crate::diagram::MermaidStyle { theme: Some(crate::diagram::MermaidTheme::Dark), ..Default::default() }).unwrap();
            assert_eq!(session.draft(), dark_source);
            let dark = render_mermaid_preview(session.draft(), raster_size).unwrap();
            let preset_fill = match template {
                DiagramTemplate::EntityRelationship => "#302f3d",
                DiagramTemplate::Pie => "#0b0000",
                DiagramTemplate::GitGraph => "#797d7d",
                _ => "#1f2020",
            };
            if template == DiagramTemplate::Gantt {
                let task_fills = |svg: &str| svg.split('<').filter(|tag| tag.starts_with("rect ") && tag.contains("rx=\"3\"") && tag.contains("stroke-width=\"1\""))
                    .filter_map(|tag| tag.split("fill=\"").nth(1)?.split('\"').next().map(str::to_owned)).collect::<Vec<_>>();
                assert_ne!(task_fills(&output.svg), task_fills(&dark.svg), "dark Gantt task fill");
            } else {
                let attribute = format!("fill=\"{preset_fill}\"");
                assert!(dark.svg.to_ascii_lowercase().split('<').any(|tag| {
                    ["rect ", "path ", "circle ", "ellipse ", "polygon "].iter().any(|shape| tag.starts_with(shape)) && tag.split('>').next().is_some_and(|element| element.contains(&attribute))
                }), "{} dark preset lacks shape fill {preset_fill}", template.name());
            }
            if template == DiagramTemplate::GitGraph {
                assert!(styled.svg.contains("class=\"commit-arrows\""));
                assert!(styled.svg.contains("stroke=\"#d10001\"") || styled.svg.contains("stroke=\"#d20002\""));
                assert!(text_uses_color(&styled.svg, "main", "#e10001"));
            }
            if template == DiagramTemplate::Pie {
                assert!(styled.svg.split('<').any(|tag| tag.starts_with("path ") && tag.contains("fill=\"#c10001\"")), "pie slice fill");
                assert!(styled.svg.split('<').any(|tag| tag.starts_with("rect ") && tag.contains("fill=\"#c10001\"")), "pie legend marker fill");
                assert!(styled.svg.contains("fill=\"#c20002\""));
                assert!(text_uses_color(&styled.svg, "Housing", "#f1f2f3"));
            }
            if template == DiagramTemplate::Sequence {
                assert!(styled.svg.contains("stroke=\"#345678\"") && styled.svg.contains("User"));
            }
            if template == DiagramTemplate::Timeline {
                assert!(styled.svg.contains("fill=\"#b20002\""));
                assert!(styled.raster.rgba.chunks_exact(4).any(|p| p[..3] == [0xb1, 0, 1]));
            }
            assert_eq!((output.raster.width, output.raster.height), (640, 420));
            assert_eq!(output.raster.rgba.len(), 640 * 420 * 4);
            let visible_pixels = output
                .raster
                .rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .count();
            let foreground_pixels = output
                .raster
                .rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0 && pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
                .count();
            assert!(
                visible_pixels > 500,
                "{} has no meaningful pixels",
                template.name()
            );
            assert!(
                foreground_pixels > 100,
                "{} has no readable foreground",
                template.name()
            );
        }
    }

    #[test]
    fn diagram_templates_are_independent_fresh_sessions() {
        let mut first = DiagramTemplate::Decision.create_session().unwrap();
        let second = DiagramTemplate::Decision.create_session().unwrap();
        assert_eq!(first.revision(), 1);
        assert_eq!(second.revision(), 1);
        first.undo().unwrap();
        assert_eq!(first.draft(), "");
        assert_eq!(second.draft(), DiagramTemplate::Decision.source());
    }
}
