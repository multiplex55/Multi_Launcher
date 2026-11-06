use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RecordingPlugin;

impl RecordingPlugin {
    fn action(label: &str, command: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Silent screen recording".into(),
            action: format!("record:{command}"),
            args: None,
        }
    }

    fn actions() -> Vec<Action> {
        vec![
            Self::action("Open Screen Recorder", "open"),
            Self::action("Stop Screen Recording", "stop"),
        ]
    }
}

impl Plugin for RecordingPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("record") {
            Self::actions()
        } else {
            Vec::new()
        }
    }

    fn name(&self) -> &str {
        "recording"
    }

    fn description(&self) -> &str {
        "Open and stop silent GIF or MP4 screen recordings"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::actions()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["record"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_query_is_exact_case_insensitive_and_lists_only_recording_controls() {
        let plugin = RecordingPlugin;
        for query in ["record", "RECORD", "Record"] {
            let actions = plugin.search(query);
            assert_eq!(
                actions
                    .iter()
                    .map(|action| action.action.as_str())
                    .collect::<Vec<_>>(),
                ["record:open", "record:stop"]
            );
        }
        for query in ["recorder", "record open", "record:open", "xrecord"] {
            assert!(plugin.search(query).is_empty(), "{query}");
        }
    }
}
