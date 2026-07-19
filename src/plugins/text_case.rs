use crate::actions::Action;
use crate::plugin::Plugin;
use crate::text_transform::case::legacy;

pub struct TextCasePlugin;

impl Plugin for TextCasePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "case ";
        if let Some(rest) = crate::common::strip_prefix_ci(query.trim_start(), PREFIX) {
            return legacy::transform_query(rest)
                .into_iter()
                .map(|result| Action {
                    label: result.label.clone(),
                    desc: result.desc.into(),
                    action: format!("clipboard:{}", result.label),
                    args: None,
                })
                .collect();
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "text_case"
    }
    fn description(&self) -> &str {
        "Convert text cases (prefix: `case`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "case <text>".into(),
            desc: "Text Case".into(),
            action: "query:case ".into(),
            args: None,
        }]
    }
}
