use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RedditPlugin;

impl Plugin for RedditPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(q) = query.strip_prefix("red ") {
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search Reddit for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://www.reddit.com/search/?q={q}"),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "reddit"
    }

    fn description(&self) -> &str {
        "Search Reddit (prefix: `red`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
}
