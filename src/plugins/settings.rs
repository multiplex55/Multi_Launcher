use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        let mut actions = Vec::new();
        if let Some(rest) = crate::common::strip_prefix_ci(q, "settings") {
            if rest.is_empty() || rest.starts_with(' ') {
                actions.push(Action {
                    label: "Open settings".into(),
                    desc: "Show settings panel".into(),
                    action: "settings:dialog".into(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(q, "dashboard") {
            if rest.is_empty() || rest.starts_with(' ') {
                actions.push(Action {
                    label: "Dashboard Settings".into(),
                    desc: "Configure dashboard layout and widgets".into(),
                    action: "dashboard:settings".into(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(q, "theme") {
            if rest.is_empty() || rest.starts_with(' ') {
                actions.push(Action {
                    label: "Theme settings".into(),
                    desc: "Configure launcher theme colors".into(),
                    action: "theme:dialog".into(),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
                });
            }
        }
        actions
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
        vec![
            Action {
                label: "settings".into(),
                desc: "Settings".into(),
                action: "settings:dialog".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "Dashboard Settings".into(),
                desc: "Dashboard settings".into(),
                action: "dashboard:settings".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "Theme settings".into(),
                desc: "Configure launcher theme colors".into(),
                action: "theme:dialog".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
        ]
    }
}
