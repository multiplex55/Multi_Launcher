use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ConvertPanelPlugin;

impl Plugin for ConvertPanelPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        const CONV_PREFIX: &str = "conv";
        const CONVERT_PREFIX: &str = "convert";
        if let Some(rest) = crate::common::strip_prefix_ci(q, CONV_PREFIX) {
            if rest.is_empty() || rest.starts_with(' ') {
                return vec![Action {
                    label: "Open converter".into(),
                    desc: "Converter panel".into(),
                    action: "convert:panel".into(),
                    args: None,
                }];
            }
        } else if let Some(rest) = crate::common::strip_prefix_ci(q, CONVERT_PREFIX) {
            if rest.is_empty() || rest.starts_with(' ') {
                return vec![Action {
                    label: "Open converter".into(),
                    desc: "Converter panel".into(),
                    action: "convert:panel".into(),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "convert_panel"
    }

    fn description(&self) -> &str {
        "Open converter panel (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Converter panel".into(),
                action: "query:conv ".into(),
                args: None,
            },
            Action {
                label: "convert".into(),
                desc: "Converter panel".into(),
                action: "query:convert ".into(),
                args: None,
            },
        ]
    }
}
