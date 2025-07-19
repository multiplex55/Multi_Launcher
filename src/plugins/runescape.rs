use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RunescapeSearchPlugin;

impl Plugin for RunescapeSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const RS_PREFIX: &str = "rs ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, RS_PREFIX) {
            let q = rest.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search RuneScape Wiki for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://runescape.wiki/?search={q}"),
                    args: None,
                }];
            }
        }
        const OSRS_PREFIX: &str = "osrs ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, OSRS_PREFIX) {
            let q = rest.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search Old School RuneScape Wiki for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://oldschool.runescape.wiki/?search={q}"),
                    args: None,
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

    fn commands(&self) -> Vec<Action> {
        vec![
            Action { label: "rs".into(), desc: "Runescape".into(), action: "query:rs ".into(), args: None },
            Action { label: "osrs".into(), desc: "Runescape".into(), action: "query:osrs ".into(), args: None },
        ]
    }
}

