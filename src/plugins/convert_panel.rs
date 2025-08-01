use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ConvertPanelPlugin;

impl Plugin for ConvertPanelPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let q = query.trim();
        if q.eq_ignore_ascii_case("conv") || q.eq_ignore_ascii_case("convert") {
            return vec![Action {
                label: "Open converter".into(),
                desc: "Unit convert".into(),
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
        "Open converter panel (prefix: `conv` or `convert`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "conv".into(),
                desc: "Unit convert".into(),
                action: "query:conv".into(),
                args: None,
            },
            Action {
                label: "convert".into(),
                desc: "Unit convert".into(),
                action: "query:convert".into(),
                args: None,
            },
        ]
    }
}
