use crate::actions::Action;
use crate::plugin::Plugin;
use sysinfo::System;

pub struct ProcessesPlugin;

impl Plugin for ProcessesPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        enum Mode {
            Both,
            Kill,
            Switch,
        }

        let (mode, rest) = if let Some(r) = crate::common::strip_prefix_ci(query, "psk") {
            (Mode::Kill, r)
        } else if let Some(r) = crate::common::strip_prefix_ci(query, "pss") {
            (Mode::Switch, r)
        } else if let Some(r) = crate::common::strip_prefix_ci(query, "ps") {
            (Mode::Both, r)
        } else {
            return Vec::new();
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
                    p.name().to_string_lossy().to_lowercase().contains(&filter)
                }
            })
            .flat_map(|p| {
                let name = p.name().to_string_lossy().to_string();
                let pid = p.pid().as_u32();
                let switch_action = Action {
                    label: format!("Switch to {name}"),
                    desc: format!("PID {pid}"),
                    action: format!("process:switch:{pid}"),
                    args: None,
                };
                let kill_action = Action {
                    label: format!("Kill {name}"),
                    desc: format!("PID {pid}"),
                    action: format!("process:kill:{pid}"),
                    args: None,
                };
                match mode {
                    Mode::Both => vec![switch_action, kill_action],
                    Mode::Kill => vec![kill_action],
                    Mode::Switch => vec![switch_action],
                }
            })
            .collect()
    }

    fn name(&self) -> &str {
        "processes"
    }

    fn description(&self) -> &str {
        "Enumerate running processes (prefixes: `ps`, `psk`, `pss`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "ps".into(),
                desc: "Processes".into(),
                action: "query:ps ".into(),
                args: None,
            },
            Action {
                label: "psk".into(),
                desc: "Kill process".into(),
                action: "query:psk ".into(),
                args: None,
            },
            Action {
                label: "pss".into(),
                desc: "Switch process".into(),
                action: "query:pss ".into(),
                args: None,
            },
        ]
    }
}
