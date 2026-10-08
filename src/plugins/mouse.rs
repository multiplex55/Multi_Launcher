use crate::actions::Action;
use crate::commands::{CoordinateToolCommand, parse_mouse_wire};
use crate::plugin::Plugin;

pub struct MousePlugin;

impl MousePlugin {
    fn action(label: impl Into<String>, command: impl Into<String>) -> Action {
        Action {
            label: label.into(),
            desc: "Mouse coordinates, one-shot click-to-copy picking, and independent crosshair"
                .into(),
            action: command.into(),
            args: None,
        }
    }

    fn inventory() -> Vec<Action> {
        [
            ("mouse settings", "mouse:settings"),
            ("mouse coords toggle", "mouse:coords:toggle"),
            ("mouse coords on", "mouse:coords:on"),
            ("mouse coords off", "mouse:coords:off"),
            ("mouse coords copy", "mouse:coords:copy"),
            ("mouse coords pick", "mouse:coords:pick"),
            ("mouse coords cancel", "mouse:coords:cancel"),
            ("mouse coords freeze", "mouse:coords:freeze"),
            ("mouse coords unfreeze", "mouse:coords:unfreeze"),
            ("mouse crosshair toggle", "mouse:crosshair:toggle"),
            ("mouse crosshair on", "mouse:crosshair:on"),
            ("mouse crosshair off", "mouse:crosshair:off"),
            ("mouse help", "mouse:help"),
        ]
        .into_iter()
        .map(|(label, command)| Self::action(label, command))
        .collect()
    }

    fn command_action(query: &str) -> Option<Action> {
        let mut parts = query.split_whitespace();
        let family = parts.next()?;
        if !family.eq_ignore_ascii_case("mouse") {
            return None;
        }
        let operation = parts.map(str::to_ascii_lowercase).collect::<Vec<_>>();
        if operation.is_empty() {
            return None;
        }

        let label = format!("mouse {}", operation.join(" "));
        let wire = format!("mouse:{}", operation.join(":"));
        let action = Self::action(label, wire);
        match parse_mouse_wire(&action) {
            Some(CoordinateToolCommand::Invalid { .. }) | None => None,
            Some(_) => Some(action),
        }
    }
}

impl Plugin for MousePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        if query.eq_ignore_ascii_case("mouse") {
            return Self::inventory();
        }
        if query.eq_ignore_ascii_case("mouse coords") {
            return Self::inventory()
                .into_iter()
                .filter(|action| action.action.starts_with("mouse:coords:"))
                .collect();
        }
        if query.eq_ignore_ascii_case("mouse crosshair") {
            return Self::inventory()
                .into_iter()
                .filter(|action| action.action.starts_with("mouse:crosshair:"))
                .collect();
        }
        Self::command_action(query).into_iter().collect()
    }

    fn name(&self) -> &str {
        "mouse"
    }

    fn description(&self) -> &str {
        "Mouse coordinate HUD, click-to-copy picking, and independent crosshair (prefix: `mouse`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::inventory()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["mouse"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_commands_are_exact_case_insensitive_and_bounded() {
        let plugin = MousePlugin;
        for (query, expected) in [
            ("mouse settings", "mouse:settings"),
            ("MOUSE HELP", "mouse:help"),
            ("mouse coords toggle", "mouse:coords:toggle"),
            ("Mouse CoOrDs On", "mouse:coords:on"),
            ("mouse coords space CLIENT", "mouse:coords:space:client"),
            (
                "mouse coords offset -512 +512",
                "mouse:coords:offset:-512:+512",
            ),
            ("mouse coords pick", "mouse:coords:pick"),
            (
                "mouse crosshair color #Aa00FF",
                "mouse:crosshair:color:#aa00ff",
            ),
            ("MOUSE CROSSHAIR GAP 128", "mouse:crosshair:gap:128"),
            ("MOUSE CROSSHAIR GUIDES OFF", "mouse:crosshair:guides:off"),
        ] {
            let results = plugin.search(query);
            assert_eq!(results.len(), 1, "{query:?}");
            assert_eq!(results[0].action, expected);
            assert!(results[0].label.starts_with("mouse "));
        }

        for query in [
            "mousex coords toggle",
            "mouse unknown",
            "mouse coords offset 513 0",
            "mouse coords offset 1",
            "mouse coords pick extra",
            "mouse coords cancel extra",
            "mouse crosshair thickness 0",
            "mouse crosshair length 257",
            "mouse crosshair gap",
            "mouse crosshair gap 1 extra",
            "mouse crosshair gap -1",
            "mouse crosshair gap 129",
            "mouse crosshair gap Infinity",
            "mouse crosshair opacity NaN",
            "mouse crosshair color red",
            "mouse crosshair guides maybe",
            "mouse crosshair help trailing",
            "coord pick",
            "crosshair opacity 1.1",
        ] {
            assert!(plugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn plain_mouse_discovery_puts_settings_first_and_keeps_inventory_concise() {
        let plugin = MousePlugin;
        assert_eq!(plugin.name(), "mouse");
        assert_eq!(plugin.query_prefixes(), ["mouse"]);

        let results = plugin.search("MOUSE");
        assert_eq!(results.len(), 13);
        assert_eq!(results[0].label, "mouse settings");
        assert_eq!(results[0].action, "mouse:settings");
        assert_eq!(results, plugin.commands());
        for required in [
            "mouse:coords:toggle",
            "mouse:coords:on",
            "mouse:coords:off",
            "mouse:coords:copy",
            "mouse:coords:pick",
            "mouse:coords:cancel",
            "mouse:coords:freeze",
            "mouse:coords:unfreeze",
            "mouse:crosshair:toggle",
            "mouse:crosshair:on",
            "mouse:crosshair:off",
            "mouse:help",
        ] {
            assert!(results.iter().any(|action| action.action == required));
        }
        assert!(
            results
                .iter()
                .all(|action| !action.action.starts_with("coord:")
                    && !action.action.starts_with("crosshair:"))
        );
        assert!(plugin.search("mouse coords offset 16 24").len() == 1);
        assert!(plugin.search("mouse crosshair color #ff0000").len() == 1);
        assert!(plugin.search("mouse crosshair gap 0").len() == 1);
        let coords = plugin.search("mouse coords");
        assert_eq!(coords.len(), 8);
        assert!(
            coords
                .iter()
                .all(|action| action.action.starts_with("mouse:coords:"))
        );
        let crosshair = plugin.search("mouse crosshair");
        assert_eq!(crosshair.len(), 3);
        assert!(
            crosshair
                .iter()
                .all(|action| action.action.starts_with("mouse:crosshair:"))
        );
    }
}
