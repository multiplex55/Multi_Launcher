use crate::actions::Action;
use crate::plugin::Plugin;

pub struct MediaPlugin;

impl Plugin for MediaPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        const PREFIX: &str = "media";
        let rest = match crate::common::strip_prefix_ci(query, PREFIX) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let filter = rest.trim();
        const OPS: [&str; 4] = ["play", "pause", "next", "prev"];
        OPS.iter()
            .filter(|op| filter.is_empty() || op.starts_with(filter))
            .map(|op| Action {
                label: format!("Media {}", op),
                desc: "Media".into(),
                action: format!("media:{}", op),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            })
            .collect()
    }

    fn name(&self) -> &str {
        "media"
    }

    fn description(&self) -> &str {
        "Control media playback (prefix: `media`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "media play".into(),
                desc: "Media".into(),
                action: "query:media play".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "media pause".into(),
                desc: "Media".into(),
                action: "query:media pause".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "media next".into(),
                desc: "Media".into(),
                action: "query:media next".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
            Action {
                label: "media prev".into(),
                desc: "Media".into(),
                action: "query:media prev".into(),
                args: None,
                preview_text: None,
                risk_level: None,
                icon: None,
            },
        ]
    }
}
