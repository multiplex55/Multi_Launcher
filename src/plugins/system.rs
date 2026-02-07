use crate::actions::{Action, ActionRiskLevel};
use crate::plugin::Plugin;

pub struct SystemPlugin;

impl Plugin for SystemPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "sys";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let filter = rest.trim();
        const OPTIONS: [&str; 4] = ["shutdown", "reboot", "lock", "logoff"];
        OPTIONS
            .iter()
            .filter(|o| filter.is_empty() || o.starts_with(filter))
            .map(|o| Action {
                label: format!("System {}", o),
                desc: "System".into(),
                action: format!("system:{}", o),
                args: None,
                preview_text: Some(format!("Performs a system-level {o} operation.")),
                risk_level: Some(match *o {
                    "shutdown" | "reboot" => ActionRiskLevel::Critical,
                    "logoff" => ActionRiskLevel::High,
                    _ => ActionRiskLevel::Medium,
                }),
                icon: Some("power".into()),
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
        vec![Action {
            label: "sys".into(),
            desc: "System".into(),
            action: "query:sys ".into(),
            args: None,
            preview_text: Some("Search system power actions.".into()),
            risk_level: Some(ActionRiskLevel::Low),
            icon: Some("power".into()),
        }]
    }
}
