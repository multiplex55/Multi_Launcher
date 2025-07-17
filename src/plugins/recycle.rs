use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RecyclePlugin;

impl Plugin for RecyclePlugin {
    #[cfg(target_os = "windows")]
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "rec";
        let trimmed = query.trim_start();
        if trimmed.len() >= PREFIX.len() && trimmed[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            return vec![Action {
                label: "Clean Recycle Bin".into(),
                desc: "Recycle Bin".into(),
                action: "recycle:clean".into(),
                args: None,
            }];
        }
        Vec::new()
    }

    #[cfg(not(target_os = "windows"))]
    fn search(&self, _query: &str) -> Vec<Action> {
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
        #[cfg(target_os = "windows")]
        {
            vec![Action { label: "rec".into(), desc: "recycle".into(), action: "fill:rec".into(), args: None }]
        }
        #[cfg(not(target_os = "windows"))]
        {
            Vec::new()
        }
    }
}

