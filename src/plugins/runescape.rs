use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RunescapeSearchPlugin;

impl Plugin for RunescapeSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(q) = query.strip_prefix("rs ") {
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search RuneScape Wiki for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://runescape.wiki/?search={q}"),
                }];
            }
        }
        if let Some(q) = query.strip_prefix("osrs ") {
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search Old School RuneScape Wiki for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://oldschool.runescape.wiki/?search={q}"),
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "runescape_search"
    }

    fn description(&self) -> &str {
        "Search the RuneScape and Old School RuneScape wikis (prefix: `rs`/`osrs`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}

