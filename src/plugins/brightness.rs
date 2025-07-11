use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BrightnessPlugin;

impl Plugin for BrightnessPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if trimmed.eq_ignore_ascii_case("bright") {
            return vec![Action {
                label: "bright: edit brightness".into(),
                desc: "Brightness".into(),
                action: "brightness:dialog".into(),
                args: None,
            }];
        }
        if let Some(rest) = trimmed.strip_prefix("bright ") {
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

    fn name(&self) -> &str { "brightness" }

    fn description(&self) -> &str {
        "Adjust display brightness (prefix: `bright`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
