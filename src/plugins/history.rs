use crate::actions::Action;
use crate::plugin::Plugin;
use crate::history::get_history;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.starts_with("hi") {
            return Vec::new();
        }
        let filter = query.strip_prefix("hi").unwrap_or("").trim();
        get_history()
            .into_iter()
            .enumerate()
            .filter(|(_, entry)| entry.query.contains(filter))
            .map(|(idx, entry)| Action {
                label: entry.query,
                desc: "History".into(),
                action: format!("history:{idx}"),
            })
            .collect()
    }

    fn name(&self) -> &str {
        "history"
    }

    fn description(&self) -> &str {
        "Search previously executed queries"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
