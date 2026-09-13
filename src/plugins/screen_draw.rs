use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ScreenDrawPlugin;

impl ScreenDrawPlugin {
    fn action(label: &str, action: &str) -> Action {
        Action {
            label: label.into(),
            desc: "Screen Draw".into(),
            action: action.into(),
            args: None,
        }
    }

    fn actions() -> Vec<Action> {
        vec![
            Self::action("Screen Draw", "screen_draw:start"),
            Self::action("Open Screen Draw toolbar", "screen_draw:toolbar"),
            Self::action("Screen Draw new capture", "screen_draw:new_capture"),
            Self::action("Screen Draw Ghost mode", "screen_draw:ghost"),
            Self::action("Finish Screen Draw", "screen_draw:done"),
            Self::action("Clear Screen Draw annotations", "screen_draw:clear"),
            Self::action("Close Screen Draw", "screen_draw:close"),
        ]
    }
}

impl Plugin for ScreenDrawPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        let rest = ["sd", "sa"]
            .into_iter()
            .filter_map(|prefix| crate::common::strip_prefix_ci(query, prefix))
            .find(|rest| rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace));
        let Some(rest) = rest else {
            return Vec::new();
        };

        match rest.trim().to_ascii_lowercase().as_str() {
            "" | "start" => vec![Self::action("Screen Draw", "screen_draw:start")],
            "toolbar" | "open" => vec![Self::action(
                "Open Screen Draw toolbar",
                "screen_draw:toolbar",
            )],
            "new" | "new capture" | "capture" | "refresh" => vec![Self::action(
                "Screen Draw new capture",
                "screen_draw:new_capture",
            )],
            "ghost" => vec![Self::action("Screen Draw Ghost mode", "screen_draw:ghost")],
            "done" | "finish" => vec![Self::action("Finish Screen Draw", "screen_draw:done")],
            "clear" => vec![Self::action(
                "Clear Screen Draw annotations",
                "screen_draw:clear",
            )],
            "close" => vec![Self::action("Close Screen Draw", "screen_draw:close")],
            _ => Vec::new(),
        }
    }

    fn name(&self) -> &str {
        "screen_draw"
    }

    fn description(&self) -> &str {
        "Draw over a frozen desktop (prefixes: `sd`, `sa`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::actions()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["sd", "sa"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_and_alias_prefixes_start_without_runtime_side_effects() {
        let plugin = ScreenDrawPlugin;
        for query in ["sd", "SD", "sa", "SA"] {
            let results = plugin.search(query);
            assert_eq!(results.len(), 1, "{query}");
            assert_eq!(results[0].action, "screen_draw:start");
        }
    }

    #[test]
    fn static_subcommands_have_stable_typed_actions() {
        let plugin = ScreenDrawPlugin;
        let cases = [
            ("sd toolbar", "screen_draw:toolbar"),
            ("sa new capture", "screen_draw:new_capture"),
            ("sd ghost", "screen_draw:ghost"),
            ("sd done", "screen_draw:done"),
            ("sd clear", "screen_draw:clear"),
            ("sd close", "screen_draw:close"),
        ];
        for (query, expected) in cases {
            assert_eq!(plugin.search(query)[0].action, expected);
        }
        assert!(plugin.search("screen draw").is_empty());
        assert!(plugin.search("sd unknown").is_empty());
        assert!(plugin.search("sddone").is_empty());
        assert!(plugin.search("saclose").is_empty());
    }

    #[test]
    fn command_inventory_exposes_every_typed_operation() {
        let plugin = ScreenDrawPlugin;
        assert_eq!(
            plugin
                .commands()
                .into_iter()
                .map(|action| action.action)
                .collect::<Vec<_>>(),
            [
                "screen_draw:start",
                "screen_draw:toolbar",
                "screen_draw:new_capture",
                "screen_draw:ghost",
                "screen_draw:done",
                "screen_draw:clear",
                "screen_draw:close",
            ]
        );
    }
}
