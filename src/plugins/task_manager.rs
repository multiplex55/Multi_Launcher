use crate::actions::{Action, ActionRiskLevel};
use crate::plugin::Plugin;

pub struct TaskManagerPlugin;

impl Plugin for TaskManagerPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim_start();
        if crate::common::strip_prefix_ci(trimmed, "tm").is_some() {
            return vec![Action {
                label: "Open Task Manager".into(),
                desc: "Task Manager".into(),
                action: "shell:taskmgr".into(),
                args: None,
                preview_text: Some(
                    "Opens Windows Task Manager to inspect and end running tasks.".into(),
                ),
                risk_level: Some(ActionRiskLevel::Medium),
                icon: Some("task_manager".into()),
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "task_manager"
    }

    fn description(&self) -> &str {
        "Open the Windows Task Manager (prefix: `tm`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "tm".into(),
            desc: "Task Manager".into(),
            action: "query:tm".into(),
            args: None,
            preview_text: Some("Query shortcut for Task Manager commands.".into()),
            risk_level: Some(ActionRiskLevel::Low),
            icon: Some("task_manager".into()),
        }]
    }
}
