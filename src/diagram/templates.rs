//! Validated starter graphs for Diagram Studio.

use super::session::{DiagramSession, DiagramSessionError, PreviewState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagramTemplate {
    Linear,
    Decision,
    Process,
}

impl DiagramTemplate {
    pub const ALL: [Self; 3] = [Self::Linear, Self::Decision, Self::Process];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Decision => "Decision",
            Self::Process => "Process",
        }
    }

    /// Mermaid source is the template's portable definition and remains the
    /// session's source of truth after installation.
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
        }
    }

    pub fn create_session(self) -> Result<DiagramSession, DiagramTemplateError> {
        let session = DiagramSession::from_fresh_source(self.source())
            .map_err(DiagramTemplateError::Session)?;
        if session.status().preview != PreviewState::Current {
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
                    "the built-in {} template is invalid",
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
    use super::DiagramTemplate;
    use crate::diagram::{FlowDirection, NodeShape, PreviewState, parse_mermaid};

    #[test]
    fn diagram_templates_are_valid_current_graphs_with_unique_starter_content() {
        let mut sources = Vec::new();
        for template in DiagramTemplate::ALL {
            let source = template.source();
            assert!(
                parse_mermaid(source).is_ok(),
                "{} template",
                template.name()
            );
            let session = template.create_session().unwrap();
            assert_eq!(session.status().preview, PreviewState::Current);
            assert!(session.current_graph().is_some());
            assert_eq!(session.draft(), source);
            assert!(session.is_dirty());
            let graph = session.current_graph().unwrap();
            match template {
                DiagramTemplate::Linear => {
                    assert_eq!(graph.direction, FlowDirection::LeftRight);
                    assert_eq!(graph.nodes.len(), 3);
                    assert_eq!(graph.edges.len(), 2);
                }
                DiagramTemplate::Decision | DiagramTemplate::Process => {
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
            sources.push(source);
        }
        assert_ne!(sources[0], sources[1]);
        assert_ne!(sources[1], sources[2]);
        assert_ne!(sources[0], sources[2]);
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
