use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RedditPlugin;

impl Plugin for RedditPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "red ";
        if query.len() >= PREFIX.len()
            && query[..PREFIX.len()].eq_ignore_ascii_case(PREFIX)
        {
            let q = query[PREFIX.len()..].trim();
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

    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "red".into(), desc: "reddit".into(), action: "fill:red ".into(), args: None }]
    }
}
