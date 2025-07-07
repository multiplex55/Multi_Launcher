use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SystemPlugin;

impl Plugin for SystemPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("sys") {
            return Vec::new();
        }
        let filter = query.strip_prefix("sys").unwrap_or("").trim();
        const OPTIONS: [&str; 4] = ["shutdown", "reboot", "lock", "logoff"];
        OPTIONS
            .iter()
            .filter(|o| filter.is_empty() || o.starts_with(filter))
            .map(|o| Action {
                label: format!("System {}", o),
                desc: "System".into(),
                action: format!("system:{}", o),
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
}
