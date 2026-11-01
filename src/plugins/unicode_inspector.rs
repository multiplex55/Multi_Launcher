use crate::actions::Action;
use crate::plugin::{Plugin, PluginQueryPolicy};

/// Payload-free launcher entry for the explicitly session-only Unicode inspector.
pub struct UnicodeInspectorPlugin;

impl Plugin for UnicodeInspectorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let Some(prefix) = query.split_whitespace().next() else {
            return Vec::new();
        };
        if !prefix.eq_ignore_ascii_case("unicode") {
            return Vec::new();
        }

        vec![Action {
            label: "Open Unicode Inspector".into(),
            desc: "Inspect text you explicitly paste or enter; content stays in this session"
                .into(),
            action: "unicode:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "unicode_inspector"
    }

    fn description(&self) -> &str {
        "Inspect Unicode scalars from explicitly supplied text (prefix: `unicode`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["unicode"]
    }

    fn query_policy(&self) -> PluginQueryPolicy {
        PluginQueryPolicy::Literal
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "unicode".into(),
            desc: "Open the session-only Unicode Inspector".into(),
            action: "query:unicode".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_inspector_plugin_has_stable_identity_and_payload_free_prefix_action() {
        let plugin = UnicodeInspectorPlugin;
        assert_eq!(plugin.name(), "unicode_inspector");
        assert_eq!(plugin.capabilities(), &["search"]);
        assert_eq!(plugin.query_prefixes(), &["unicode"]);
        assert_eq!(plugin.query_policy(), PluginQueryPolicy::Literal);

        for query in [
            "unicode",
            "UNICODE",
            "  Unicode exact text 雪\r\nsecond line",
            "unicode text containing secret",
        ] {
            let actions = plugin.search(query);
            assert_eq!(actions.len(), 1, "query {query:?}");
            assert_eq!(actions[0].label, "Open Unicode Inspector");
            assert_eq!(actions[0].action, "unicode:open");
            assert!(actions[0].args.is_none());
            assert!(!actions[0].label.contains("secret"));
            assert!(!actions[0].desc.contains("secret"));
        }

        for query in ["", "unicodes", "not-unicode text", "unicode:open"] {
            assert!(plugin.search(query).is_empty(), "query {query:?}");
        }

        let commands = plugin.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].action, "query:unicode");
        assert!(commands[0].args.is_none());
    }
}
