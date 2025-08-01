use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ConvertPanelPlugin;

impl Plugin for ConvertPanelPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        if let Some(rest) = crate::common::strip_prefix_ci(q, "conv")
            .or_else(|| crate::common::strip_prefix_ci(q, "convert"))
        {
            if rest.is_empty() || rest.starts_with(' ') {
                return vec![Action {
                    label: "Open convert panel".into(),
                    desc: "Show convert panel".into(),
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
        "Open convert panel (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Convert panel".into(),
                action: "query:conv".into(),
                args: None,
            },
            Action {
                label: "convert".into(),
                desc: "Convert panel".into(),
                action: "query:convert".into(),
                args: None,
            },
        ]
    }
}

