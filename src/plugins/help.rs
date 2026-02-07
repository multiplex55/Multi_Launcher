use crate::actions::Action;
use crate::plugin::Plugin;

pub struct HelpPlugin;

impl Plugin for HelpPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(q, "help") {
            if rest.is_empty() || rest.starts_with(' ') {
                return vec![Action {
                    label: "Show command list".into(),
                    desc: "Display available command prefixes".into(),
                    action: "help:show".into(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "List available commands (prefix: `help`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "help".into(),
            desc: "Help".into(),
            action: "query:help".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
