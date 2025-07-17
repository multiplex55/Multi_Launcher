use crate::actions::Action;
use crate::plugin::Plugin;

pub struct WikipediaPlugin;

impl Plugin for WikipediaPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "wiki ";
        if query.len() >= PREFIX.len()
            && query[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        {
            let q = query[PREFIX.len()..].trim();
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search Wikipedia for {q}"),
                    desc: "Web search".into(),
                    action: format!(
                        "https://en.wikipedia.org/wiki/Special:Search?search={q}"
                    ),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "wikipedia"
    }

    fn description(&self) -> &str {
        "Search Wikipedia (prefix: `wiki`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "wiki".into(), desc: "wikipedia".into(), action: "fill:wiki ".into(), args: None }]
    }
}

