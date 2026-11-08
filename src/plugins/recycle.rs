use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RecyclePlugin;

impl Plugin for RecyclePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("rec") {
            return vec![Action {
                label: "Clean Recycle Bin".into(),
                desc: "Recycle Bin".into(),
                action: "recycle:clean".into(),
                args: None,
            }];
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "recycle"
    }

    fn description(&self) -> &str {
        "Empty the recycle bin (prefix: `rec`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["rec"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "rec".into(),
            desc: "Recycle".into(),
            action: "query:rec".into(),
            args: None,
        }]
    }
}
