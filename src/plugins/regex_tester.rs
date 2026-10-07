use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RegexTesterPlugin;

impl Plugin for RegexTesterPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("regex") {
            return Vec::new();
        }

        vec![Action {
            label: "Open Regex Tester".into(),
            desc: "Open the local, offline Rust Regex Tester".into(),
            action: "regex:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "regex_tester"
    }

    fn description(&self) -> &str {
        "Local, offline Rust Regex Tester (prefix: `regex`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["regex"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "regex".into(),
            desc: "Open the local, offline Rust Regex Tester".into(),
            action: "query:regex".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_regex_query_opens_local_tester() {
        for query in ["regex", "REGEX", "  ReGeX\t\n"] {
            let actions = RegexTesterPlugin.search(query);
            assert_eq!(actions.len(), 1, "query {query:?}");
            let action = &actions[0];
            assert_eq!(action.label, "Open Regex Tester");
            assert_eq!(action.action, "regex:open");
            assert_eq!(action.desc, "Open the local, offline Rust Regex Tester");
            assert!(action.args.is_none());
        }
    }

    #[test]
    fn unrelated_and_inline_regex_queries_produce_no_actions() {
        for query in ["", "regexfoo", "regex extra", "regex \\w+", "regex:open"] {
            assert!(RegexTesterPlugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn completion_query_discovers_the_same_open_action() {
        let commands = RegexTesterPlugin.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].label, "regex");
        assert_eq!(commands[0].action, "query:regex");
        assert!(commands[0].args.is_none());
        let query = commands[0].action.strip_prefix("query:").unwrap();
        assert_eq!(RegexTesterPlugin.search(query)[0].action, "regex:open");
    }
}
