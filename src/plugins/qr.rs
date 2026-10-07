use crate::actions::Action;
use crate::plugin::{Plugin, PluginQueryPolicy};

pub struct QrPlugin;

impl Plugin for QrPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(initial_text) = parse_qr_query(query) else {
            return Vec::new();
        };

        let label = if initial_text.is_some() {
            "Generate QR for supplied text"
        } else {
            "Open QR Generator"
        };

        vec![Action {
            label: label.into(),
            desc: "Create a QR code locally from text".into(),
            action: "qr:open".into(),
            args: initial_text,
        }]
    }

    fn name(&self) -> &str {
        "qr"
    }

    fn description(&self) -> &str {
        "Create a QR code locally from text (prefix: `qr`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["qr"]
    }

    fn query_policy(&self) -> PluginQueryPolicy {
        PluginQueryPolicy::Literal
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "qr".into(),
            desc: "Create a QR code locally from text".into(),
            action: "query:qr".into(),
            args: None,
        }]
    }
}

fn parse_qr_query(query: &str) -> Option<Option<String>> {
    let query = query.trim_start();
    let command_end = query.find(char::is_whitespace).unwrap_or(query.len());
    if !query[..command_end].eq_ignore_ascii_case("qr") {
        return None;
    }
    if command_end == query.len() {
        return Some(None);
    }

    let separator_len = query[command_end..].chars().next()?.len_utf8();
    let initial_text = &query[command_end + separator_len..];
    Some((!initial_text.is_empty()).then(|| initial_text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_query_opens_an_empty_generator() {
        for query in ["qr", "QR", "  qR", "\tQr\n"] {
            let actions = QrPlugin.search(query);
            assert_eq!(actions.len(), 1, "query {query:?}");
            let action = &actions[0];
            assert_eq!(action.label, "Open QR Generator");
            assert_eq!(action.action, "qr:open");
            assert!(action.args.is_none());
        }
    }

    #[test]
    fn payload_query_preserves_exact_text_without_leaking_it_to_labels() {
        let payload =
            "hello  \"quoted\" C:\\folder\n日本語 kind:private id:private !kind:other !id:other";
        let query = format!(" \tQR {payload}");
        let actions = QrPlugin.search(&query);

        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.label, "Generate QR for supplied text");
        assert_eq!(action.desc, "Create a QR code locally from text");
        assert_eq!(action.action, "qr:open");
        assert_eq!(action.args.as_deref(), Some(payload));
        assert!(!action.label.contains(payload));
        assert!(!action.desc.contains(payload));
        assert!(!action.action.contains(payload));
    }

    #[test]
    fn consumes_one_separator_and_preserves_the_rest() {
        for (query, expected) in [
            ("qr hello world", Some("hello world")),
            ("qr  leading space", Some(" leading space")),
            ("qr\ttext", Some("text")),
            ("qr\ntext", Some("text")),
            ("qr \ntext", Some("\ntext")),
            ("qr    ", Some("   ")),
            ("qr ", None),
        ] {
            let actions = QrPlugin.search(query);
            assert_eq!(actions.len(), 1, "query {query:?}");
            assert_eq!(actions[0].args.as_deref(), expected, "query {query:?}");
        }
    }

    #[test]
    fn command_prefix_and_discovery_are_stable() {
        assert_eq!(QrPlugin.name(), "qr");
        assert_eq!(QrPlugin.capabilities(), &["search"]);
        assert_eq!(QrPlugin.query_prefixes(), &["qr"]);
        assert_eq!(QrPlugin.query_policy(), PluginQueryPolicy::Literal);

        let commands = QrPlugin.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].label, "qr");
        assert_eq!(commands[0].action, "query:qr");
        assert!(commands[0].args.is_none());
    }

    #[test]
    fn only_the_complete_qr_head_is_recognized() {
        for query in ["", "qrfoo", "qrcode", "qr:open", "other qr text"] {
            assert!(QrPlugin.search(query).is_empty(), "query {query:?}");
        }
    }
}
