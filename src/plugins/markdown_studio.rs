use crate::actions::Action;
use crate::plugin::Plugin;

/// Exact discoverable opener for the standalone Markdown Studio.
pub struct MarkdownStudioPlugin;

impl Plugin for MarkdownStudioPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("markdown studio") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Markdown Studio".into(),
            desc: "Edit, preview, and export a Markdown document".into(),
            action: "markdown:studio".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "markdown_studio"
    }
    fn description(&self) -> &str {
        "Open Markdown Studio"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "markdown studio".into(),
            desc: "Open Markdown Studio".into(),
            action: "query:markdown studio".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_discoverable_query_opens_markdown_studio() {
        let plugin = MarkdownStudioPlugin;
        assert_eq!(
            plugin.search(" Markdown Studio ")[0].action,
            "markdown:studio"
        );
        assert_eq!(plugin.commands()[0].action, "query:markdown studio");
        assert!(plugin.search("markdown studio help").is_empty());
    }
}
