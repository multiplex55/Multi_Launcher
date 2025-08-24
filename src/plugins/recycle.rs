use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RecyclePlugin;

impl Plugin for RecyclePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "rec";
        let trimmed = query.trim_start();
        if crate::common::strip_prefix_ci(trimmed, PREFIX).is_some() {
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

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "rec".into(), desc: "Recycle".into(), action: "query:rec".into(), args: None }]
    }
}

