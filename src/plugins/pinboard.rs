use crate::actions::Action;
use crate::plugin::Plugin;

pub struct PinboardPlugin;

impl Plugin for PinboardPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        let capture = query.eq_ignore_ascii_case("pinboard")
            || query.eq_ignore_ascii_case("pinboard capture");
        let list =
            query.eq_ignore_ascii_case("pinboard") || query.eq_ignore_ascii_case("pinboard list");
        let live =
            query.eq_ignore_ascii_case("pinboard") || query.eq_ignore_ascii_case("pinboard live");

        let mut actions = Vec::with_capacity(3);
        if capture {
            actions.push(Action {
                label: "Capture Screenshot Pin".into(),
                desc: "Select a screen region and keep it as an in-memory reference pin".into(),
                action: "pinboard:capture".into(),
                args: None,
            });
        }
        if list {
            actions.push(Action {
                label: "Show Screenshot Pins".into(),
                desc: "Open the pin list to recover click-through pins or close them".into(),
                action: "pinboard:list".into(),
                args: None,
            });
        }
        if live {
            actions.push(Action {
                label: "Capture Live Window Pin".into(),
                desc: "Select a crop inside one external window for a live reference pin".into(),
                action: "pinboard:live".into(),
                args: None,
            });
        }
        actions
    }

    fn name(&self) -> &str {
        "pinboard"
    }

    fn description(&self) -> &str {
        "Capture region-only, session-only screenshot pins with `pinboard capture`, or select a window-attached live reference with `pinboard live`. Pins are topmost by default, pass-through pins are recovered from the pin list, and explicit Copy/Save actions are available. The session limit is 8 pins and 256 MiB; display size fits the owning child viewport's DPI."
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["pinboard"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "pinboard".into(),
            desc: "Capture static or live reference pins, or recover session-only pins".into(),
            action: "query:pinboard".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, PinboardCommand, parse_action};

    #[test]
    fn exact_pinboard_queries_expose_capture_and_recovery_actions() {
        for query in ["pinboard", " PINBOARD ", "pInBoArD"] {
            let actions = PinboardPlugin.search(query);
            assert_eq!(actions.len(), 3, "{query:?}");
            assert_eq!(actions[0].action, "pinboard:capture");
            assert_eq!(actions[1].action, "pinboard:list");
            assert_eq!(actions[2].action, "pinboard:live");
        }

        for (query, wire, command) in [
            (
                "pinboard capture",
                "pinboard:capture",
                PinboardCommand::Capture,
            ),
            ("pinboard list", "pinboard:list", PinboardCommand::List),
            ("pinboard live", "pinboard:live", PinboardCommand::Live),
        ] {
            let actions = PinboardPlugin.search(query);
            assert_eq!(actions.len(), 1, "{query:?}");
            assert_eq!(actions[0].action, wire);
            assert!(actions[0].args.is_none());
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Pinboard(command)
            );
        }

        for query in [
            "pinboard capture extra",
            "pinboard list now",
            "pinboard live extra",
            "pinboard: capture",
            "pinboardish",
        ] {
            assert!(PinboardPlugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn plugin_help_discloses_all_pinboard_paths() {
        assert_eq!(PinboardPlugin.commands()[0].action, "query:pinboard");
        let description = PinboardPlugin.description();
        for detail in [
            "region-only",
            "session-only",
            "topmost",
            "pin list",
            "pinboard live",
            "256 MiB",
        ] {
            assert!(description.contains(detail), "missing {detail:?}");
        }
    }
}
