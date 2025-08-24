use crate::actions::Action;
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
        }]
    }
}

