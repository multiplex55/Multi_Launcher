use crate::actions::Action;
use crate::plugin::Plugin;
use urlencoding::encode;

pub struct YoutubePlugin;

impl Plugin for YoutubePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "yt ";
        if let Some(rest) = crate::common::strip_prefix_ci(query, PREFIX) {
            let q = rest.trim();
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search YouTube for {q}"),
                    desc: "Web search".into(),
                    action: format!("https://www.youtube.com/results?search_query={}", encode(q)),
                    args: None,
                }];
            }
        }
        Vec::new()
    }

    fn name(&self) -> &str {
        "youtube"
    }

    fn description(&self) -> &str {
        "Search YouTube (prefix: `yt`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "yt".into(),
            desc: "YouTube".into(),
            action: "query:yt ".into(),
            args: None,
        }]
    }
}
