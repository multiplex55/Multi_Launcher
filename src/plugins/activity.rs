use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ActivityPlugin;

impl Plugin for ActivityPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("activity") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Activity".into(),
            desc: "View bounded, payload-free snapshots of background work".into(),
            action: "activity:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "activity"
    }

    fn description(&self) -> &str {
        "Inspect bounded activity and worker retirement facts (prefix: `activity`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["activity"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "activity".into(),
            desc: "Open the Activity view".into(),
            action: "query:activity".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DialogCommand, parse_action};

    #[test]
    fn activity_prefix_routes_to_the_payload_free_typed_dialog() {
        for query in ["activity", " ACTIVITY ", "\tActivity\n"] {
            let actions = ActivityPlugin.search(query);
            assert_eq!(actions.len(), 1, "{query:?}");
            assert_eq!(actions[0].action, "activity:open");
            assert_eq!(actions[0].args, None);
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::Activity)
            );
        }
        for query in ["activityx", "activity now", "activity:open"] {
            assert!(ActivityPlugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(ActivityPlugin.commands()[0].action, "query:activity");
    }

    #[test]
    fn activity_rejects_unknown_actions_and_payloads_without_echoing_them() {
        let sentinel = "ACTIVITY_PRIVATE_PAYLOAD_SENTINEL";
        for (action, args, expected) in [
            (
                "activity:open",
                Some(sentinel),
                "Activity does not accept a path or content payload",
            ),
            ("activity:unknown", None, "unknown Activity command"),
        ] {
            let error = parse_action(&Action {
                label: "Activity".into(),
                desc: String::new(),
                action: action.into(),
                args: args.map(str::to_owned),
            })
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains(sentinel), "{error}");
        }
    }
}
