use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::System;

pub struct ProcessesPlugin;

impl Plugin for ProcessesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "ps";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let filter = rest.trim().to_lowercase();
        let system = System::new_all();
        system
            .processes()
            .values()
            .filter(|p| {
                if filter.is_empty() {
                    true
                } else {
                    p.name()
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter)
                }
            })
            .flat_map(|p| {
                let name = p.name().to_string_lossy().to_string();
                let pid = p.pid().as_u32();
                vec![
                    Action {
                        label: format!("Switch to {name}"),
                        desc: format!("PID {pid}"),
                        action: format!("process:switch:{pid}"),
                        args: None,
                    },
                    Action {
                        label: format!("Kill {name}"),
                        desc: format!("PID {pid}"),
                        action: format!("process:kill:{pid}"),
                        args: None,
                    },
                ]
            })
            .collect()
    }

    fn name(&self) -> &str {
        "processes"
    }

    fn description(&self) -> &str {
        "Enumerate running processes (prefix: `ps`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "ps".into(), desc: "Processes".into(), action: "query:ps ".into(), args: None }]
    }
}

