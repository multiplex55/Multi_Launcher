use crate::actions::Action;
use crate::plugin::Plugin;

pub struct HelpPlugin;

impl Plugin for HelpPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        if q == "help" || q.starts_with("help ") {
            return vec![Action {
                label: "Show command list".into(),
                desc: "Display available command prefixes".into(),
                action: "help:show".into(),
                args: None,
            }];
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
}

