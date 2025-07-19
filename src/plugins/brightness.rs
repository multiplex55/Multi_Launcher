use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrightnessPlugin;

impl Plugin for BrightnessPlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(trimmed, "bright") {
            if rest.trim().is_empty() {
                return vec![Action {
                    label: "bright: edit brightness".into(),
                    desc: "Brightness".into(),
                    action: "brightness:dialog".into(),
                    args: None,
                }];
            }
            let rest = rest.trim();
            if let Ok(val) = rest.parse::<u8>() {
                if val <= 100 {
                    return vec![Action {
                        label: format!("Set brightness to {val}%"),
                        desc: "Brightness".into(),
                        action: format!("brightness:set:{val}"),
                        args: None,
                    }];
                }
            }
        }
        Vec::new()
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
        Vec::new()
    }

    fn name(&self) -> &str { "brightness" }

    fn description(&self) -> &str {
        "Adjust display brightness (prefix: `bright`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "bright".into(),
            desc: "Brightness".into(),
            action: "query:bright ".into(),
            args: None,
        }]
    }
}
