use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SystemPlugin;

impl Plugin for SystemPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "sys";
        if query.len() < PREFIX.len() || !query[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            return Vec::new();
        }
        let filter = query[PREFIX.len()..].trim();
        const OPTIONS: [&str; 4] = ["shutdown", "reboot", "lock", "logoff"];
        OPTIONS
            .iter()
            .filter(|o| filter.is_empty() || o.starts_with(filter))
            .map(|o| Action {
                label: format!("System {}", o),
                desc: "System".into(),
                action: format!("system:{}", o),
                args: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "system"
    }

    fn description(&self) -> &str {
        "Execute system actions like shutdown or reboot (prefix: `sys`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "sys".into(), desc: "system".into(), action: "fill:sys ".into(), args: None }]
    }
}
