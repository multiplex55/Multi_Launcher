use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RunescapeSearchPlugin;

impl Plugin for RunescapeSearchPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const RS_PREFIX: &str = "rs ";
        if query.len() >= RS_PREFIX.len()
            && query[..RS_PREFIX.len()].eq_ignore_ascii_case(RS_PREFIX)
        {
            let q = query[RS_PREFIX.len()..].trim();
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
        if query.len() >= OSRS_PREFIX.len()
            && query[..OSRS_PREFIX.len()].eq_ignore_ascii_case(OSRS_PREFIX)
        {
            let q = query[OSRS_PREFIX.len()..].trim();
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

