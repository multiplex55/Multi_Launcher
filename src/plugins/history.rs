use crate::actions::Action;
use crate::history::get_history;
use crate::plugin::Plugin;

const MAX_HISTORY_RESULTS: usize = 10;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "hi";
        if query.len() < PREFIX.len() || !query[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
            return Vec::new();
        }
        if query.trim().eq_ignore_ascii_case("hi clear") {
            return vec![Action {
                label: "Clear history".into(),
                desc: "History".into(),
                action: "history:clear".into(),
                args: None,
            }];
        }
        let filter = query[PREFIX.len()..].trim().to_lowercase();
        get_history()
            .into_iter()
            .enumerate()
            .filter(|(_, entry)| entry.query.to_lowercase().contains(&filter))
            .take(MAX_HISTORY_RESULTS)
            .map(|(idx, entry)| Action {
                label: entry.query,
                desc: "History".into(),
                action: format!("history:{idx}"),
                args: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "history"
    }

    fn description(&self) -> &str {
        "Search previously executed queries (prefix: `hi`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
