use crate::actions::Action;
use crate::plugin::Plugin;

#[derive(Default)]
pub struct KeysPlugin;

impl Plugin for KeysPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();

        // Accept `keys ...` and `key ...` prefixes (case-insensitive).
        let spec = crate::common::strip_prefix_ci(q, "keys")
            .or_else(|| crate::common::strip_prefix_ci(q, "key"));

        let Some(spec) = spec else {
            return Vec::new();
        };

        let spec = spec.trim();
        if spec.is_empty() {
            // If user typed just `keys`, show a small hint.
            return vec![Action {
                label: "keys <combo>".into(),
                desc: "Keys".into(),
                action: "query:keys ".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            }];
        }

        vec![Action {
            label: format!("Send keys: {spec}"),
            desc: "Keys".into(),
            action: format!("keys:{spec}"),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }

    fn name(&self) -> &str {
        "keys"
    }

    fn description(&self) -> &str {
        "Send keystrokes via SendInput (prefix: `keys` / `key`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "keys".into(),
            desc: "Keys".into(),
            action: "query:keys ".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
