use crate::actions::Action;
use crate::plugin::Plugin;

/// Exact discoverable opener for the interactive Diagram Studio.
pub struct DiagramStudioPlugin;

impl Plugin for DiagramStudioPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("diagram studio") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Diagram Studio".into(),
            desc: "Edit a Mermaid flowchart visually or by source".into(),
            action: "diagram:studio".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "diagram_studio"
    }

    fn description(&self) -> &str {
        "Open the Mermaid Diagram Studio"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "diagram studio".into(),
            desc: "Open the Mermaid Diagram Studio".into(),
            action: "query:diagram studio".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::DiagramStudioPlugin;
    use crate::plugin::Plugin;

    #[test]
    fn exact_discoverable_query_opens_diagram_studio() {
        let plugin = DiagramStudioPlugin;
        let result = plugin.search("  DIAGRAM STUDIO  ");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, "diagram:studio");
        assert_eq!(plugin.commands()[0].action, "query:diagram studio");
        assert!(plugin.search("diagram studio help").is_empty());
    }
}
