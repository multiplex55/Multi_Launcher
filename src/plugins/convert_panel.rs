use crate::actions::Action;
use crate::plugin::Plugin;

/// Plugin exposing the interactive convert panel.
pub struct ConvertPanelPlugin;

impl Plugin for ConvertPanelPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let trimmed = query.trim();
        if crate::common::strip_prefix_ci(trimmed, "convert").is_some()
            || crate::common::strip_prefix_ci(trimmed, "conv").is_some()
        {
            return vec![Action {
                label: "conv: open convert panel".into(),
                desc: "Convert".into(),
                action: "convert:panel".into(),
                args: None,
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "convert_panel"
    }

    fn description(&self) -> &str {
        "Open the conversion panel (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Convert".into(),
                action: "query:conv ".into(),
                args: None,
            },
            Action {
                label: "convert".into(),
                desc: "Convert".into(),
                action: "query:convert ".into(),
                args: None,
            },
        ]
    }
}

