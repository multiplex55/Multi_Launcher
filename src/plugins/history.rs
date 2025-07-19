use crate::actions::Action;
use crate::history::get_history;
use crate::plugin::Plugin;

const MAX_HISTORY_RESULTS: usize = 10;

pub struct HistoryPlugin;

impl Plugin for HistoryPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "hi";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        if let Some(clear_rest) = crate::common::strip_prefix_ci(query.trim(), "hi clear") {
            if clear_rest.is_empty() {
                return vec![Action {
                    label: "Clear history".into(),
                    desc: "History".into(),
                    action: "history:clear".into(),
                    args: None,
                }];
            }
        }
        let filter = rest.trim().to_lowercase();
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

    fn commands(&self) -> Vec<Action> {
        vec![
            Action { label: "hi".into(), desc: "History".into(), action: "query:hi".into(), args: None },
            Action { label: "hi clear".into(), desc: "History".into(), action: "query:hi clear".into(), args: None },
        ]
    }
}
