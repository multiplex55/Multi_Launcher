use crate::actions::Action;
use crate::plugin::Plugin;
use urlencoding::encode;

pub struct RedditPlugin;

impl Plugin for RedditPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "red ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, PREFIX) {
            let q = rest.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search Reddit for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://www.reddit.com/search/?q={}", encode(q)),
                    args: None,
                    preview_text: None,
                    risk_level: None,
                    icon: None,
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
        vec![Action {
            label: "red".into(),
            desc: "Reddit".into(),
            action: "query:red ".into(),
            args: None,
            preview_text: None,
            risk_level: None,
            icon: None,
        }]
    }
}
