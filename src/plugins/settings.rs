use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(q, "settings") {
            if rest.is_empty() || rest.starts_with(' ') {
                return vec![Action {
                    label: "Open settings".into(),
                    desc: "Show settings panel".into(),
                    action: "settings:dialog".into(),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "settings"
    }

    fn description(&self) -> &str {
        "Open settings panel (prefix: `settings`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "settings".into(),
            desc: "Settings".into(),
            action: "query:settings".into(),
            args: None,
        }]
    }
}
