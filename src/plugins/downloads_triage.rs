use crate::actions::Action;
use crate::plugin::Plugin;

pub struct DownloadsTriagePlugin;

impl Plugin for DownloadsTriagePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !["downloads", "downloads triage", "downloads triage center"]
            .iter()
            .any(|name| query.trim().eq_ignore_ascii_case(name))
        {
            return Vec::new();
        }
        vec![Action {
            label: "Open Downloads Triage".into(),
            desc: "Review and organize downloaded files".into(),
            action: "downloads_triage:open".into(),
            args: None,
        }]
    }
    fn name(&self) -> &str {
        <Self as crate::plugin::BuiltinIdentity>::ID
    }
    fn description(&self) -> &str {
        "Review and organize downloaded files (prefix: `downloads`)"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["downloads"]
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "downloads".into(),
            desc: "Open Downloads Triage".into(),
            action: "query:downloads".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DialogCommand, parse_action};
    #[test]
    fn downloads_triage_plugin_queries_route_to_typed_dialog() {
        for query in ["downloads", " DOWNLOADS TRIAGE ", "Downloads Triage Center"] {
            let actions = DownloadsTriagePlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].args, None);
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::DownloadsTriage)
            );
        }
        for query in [
            "download",
            "downloads now",
            "downloads triage center now",
            "downloads_triage:open",
        ] {
            assert!(DownloadsTriagePlugin.search(query).is_empty());
        }
        assert_eq!(DownloadsTriagePlugin.query_prefixes(), ["downloads"]);
    }
    #[test]
    fn downloads_triage_parser_rejects_payload_and_unknown_without_echo() {
        let sentinel = "PRIVATE_SENTINEL";
        for (action, args) in [
            ("downloads_triage:open", Some(sentinel)),
            ("downloads_triage:other", None),
        ] {
            let error = parse_action(&Action {
                label: String::new(),
                desc: String::new(),
                action: action.into(),
                args: args.map(str::to_owned),
            })
            .unwrap_err()
            .to_string();
            assert!(!error.contains(sentinel));
            assert!(error.contains("Downloads Triage"));
        }
    }
}
