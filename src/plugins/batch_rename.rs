use crate::actions::Action;
use crate::plugin::Plugin;

pub struct BatchRenamePlugin;

impl Plugin for BatchRenamePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("rename") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Batch Renamer".into(),
            desc: "Choose a folder for a read-only filename preview; extensions are protected by default".into(),
            action: "rename:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "batch_rename"
    }

    fn description(&self) -> &str {
        "Choose an explicit folder for a read-only batch filename preview; extensions are protected by default (prefix: `rename`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["rename"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "rename".into(),
            desc: "Preview filenames under an explicitly chosen folder; extensions are protected by default".into(),
            action: "query:rename".into(),
            args: None,
        }]
    }
}
