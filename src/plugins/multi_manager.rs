use crate::actions::Action;
use crate::plugin::Plugin;

pub struct MultiManagerPlugin;

impl MultiManagerPlugin {
    pub fn commands() -> Vec<Action> {
        vec![
            Action {
                label: "mm".into(),
                desc: "MultiManager".into(),
                action: "query:mm".into(),
                args: None,
            },
            Action {
                label: "mm settings".into(),
                desc: "MultiManager settings".into(),
                action: "query:mm settings".into(),
                args: None,
            },
        ]
    }
}

impl Plugin for MultiManagerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        crate::multi_manager::commands::search_mm_commands(query)
    }

    fn name(&self) -> &str {
        "MultiManager"
    }

    fn description(&self) -> &str {
        "Manage window workspaces, home/target positions, hotkeys, and rotations"
    }

    fn capabilities(&self) -> &[&str] {
        &["workspace_manager", "window_manager", "hotkeys"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::commands()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["mm"]
    }
}
