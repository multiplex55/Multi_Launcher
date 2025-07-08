use crate::actions::Action;
use crate::plugin::Plugin;

pub struct WikipediaPlugin;

impl Plugin for WikipediaPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(q) = query.strip_prefix("wiki ") {
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
}

