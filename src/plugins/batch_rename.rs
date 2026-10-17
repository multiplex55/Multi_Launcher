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
            desc: "Choose a folder, review a read-only filename preview, then confirm guarded Apply; extensions are protected and safe Undo is available".into(),
            action: "rename:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "batch_rename"
    }

    fn description(&self) -> &str {
        "Choose an explicit folder, preview ordered filename rules, then confirm guarded Apply or safe Undo; extensions are protected by default (prefix: `rename`)"
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
            desc: "Choose an explicit root; numbering follows deterministic path order, extensions stay protected by default, and Apply rechecks stale sources and occupied names before staging. Existing files are never overwritten; partial or cancelled work stays in a recovery receipt for safe Undo.".into(),
            action: "query:rename".into(),
            args: None,
        }]
    }
}
