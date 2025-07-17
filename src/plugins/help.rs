use crate::actions::Action;
use crate::plugin::Plugin;

pub struct HelpPlugin;

impl Plugin for HelpPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        const PREFIX: &str = "help";
        const PREFIX_SPACE: &str = "help ";
        if q.eq_ignore_ascii_case(PREFIX)
            || (q.len() >= PREFIX_SPACE.len()
                && q[..PREFIX_SPACE.len()].eq_ignore_ascii_case(PREFIX_SPACE))
        {
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

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "help".into(),
            desc: "Help".into(),
            action: "query:help".into(),
            args: None,
        }]
    }
}

