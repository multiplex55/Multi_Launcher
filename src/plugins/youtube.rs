use crate::actions::Action;
use crate::plugin::Plugin;

pub struct YoutubePlugin;

impl Plugin for YoutubePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(q) = query.strip_prefix("yt ") {
            let q = q.trim();
            if !q.is_empty() {
                return vec![Action {
                    label: format!("Search YouTube for {q}"),
                    desc: "Web search".into(),
                    action: format!(
                        "https://www.youtube.com/results?search_query={q}"
                    ),
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
}
