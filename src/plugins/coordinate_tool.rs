use crate::actions::Action;
use crate::commands::{CoordinateToolCommand, parse_coordinate_tool_wire};
use crate::plugin::Plugin;

pub struct CoordinateToolPlugin;

impl CoordinateToolPlugin {
    fn action(label: impl Into<String>, command: impl Into<String>) -> Action {
        Action {
            label: label.into(),
            desc: "Physical mouse coordinates, one-shot pick, and independent crosshair".into(),
            action: command.into(),
            args: None,
        }
    }

    fn inventory() -> Vec<Action> {
        [
            ("Toggle coordinate HUD", "coord:toggle"),
            ("Enable coordinate HUD", "coord:on"),
            ("Disable coordinate HUD", "coord:off"),
            ("Use desktop coordinates", "coord:space:desktop"),
            ("Use monitor coordinates", "coord:space:monitor"),
            ("Use client coordinates", "coord:space:client"),
            ("Compact coordinate HUD", "coord:compact"),
            ("Detailed coordinate HUD", "coord:detailed"),
            ("Set coordinate HUD offset to 16,24", "coord:offset:16:24"),
            ("Freeze coordinate sample", "coord:freeze"),
            ("Unfreeze coordinate sample", "coord:unfreeze"),
            ("Copy coordinates", "coord:copy"),
            ("Pick coordinates at the next click", "coord:pick"),
            ("Cancel coordinate pick", "coord:cancel"),
            ("Coordinate HUD help", "coord:help"),
            ("Toggle crosshair", "crosshair:toggle"),
            ("Enable crosshair", "crosshair:on"),
            ("Disable crosshair", "crosshair:off"),
            ("Set crosshair color to red", "crosshair:color:#ff0000"),
            ("Set crosshair thickness to 2", "crosshair:thickness:2"),
            ("Set crosshair length to 12", "crosshair:length:12"),
            ("Set crosshair opacity to 1.0", "crosshair:opacity:1.0"),
            ("Enable crosshair guides", "crosshair:guides:on"),
            ("Disable crosshair guides", "crosshair:guides:off"),
            ("Enable crosshair contrast", "crosshair:contrast:on"),
            ("Disable crosshair contrast", "crosshair:contrast:off"),
            ("Crosshair help", "crosshair:help"),
        ]
        .into_iter()
        .map(|(label, command)| Self::action(label, command))
        .collect()
    }

    fn help_action(family: &str) -> Action {
        if family.eq_ignore_ascii_case("crosshair") {
            Self::action("Crosshair help", "crosshair:help")
        } else {
            Self::action("Coordinate HUD help", "coord:help")
        }
    }

    fn command_action(query: &str) -> Option<Action> {
        let mut parts = query.split_whitespace();
        let family = parts.next()?;
        let canonical_family = if family.eq_ignore_ascii_case("coord") {
            "coord"
        } else if family.eq_ignore_ascii_case("crosshair") {
            "crosshair"
        } else {
            return None;
        };
        let operation = parts.collect::<Vec<_>>().join(":").to_ascii_lowercase();
        let wire = if operation.is_empty() {
            format!("{canonical_family}:toggle")
        } else {
            format!("{canonical_family}:{operation}")
        };
        let action = Self::action(query.trim(), wire);
        match parse_coordinate_tool_wire(&action) {
            Some(CoordinateToolCommand::Invalid { .. }) | None => None,
            Some(_) => Some(action),
        }
    }
}

impl Plugin for CoordinateToolPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        Self::command_action(query.trim())
            .map(|action| {
                if action.action == "coord:help" || action.action == "crosshair:help" {
                    vec![Self::help_action(
                        if action.action.starts_with("crosshair:") {
                            "crosshair"
                        } else {
                            "coord"
                        },
                    )]
                } else {
                    vec![action]
                }
            })
            .unwrap_or_default()
    }

    fn name(&self) -> &str {
        "coordinate_tool"
    }

    fn description(&self) -> &str {
        "Physical-pixel coordinate HUD, one-shot click-to-copy picking, and independent crosshair (prefixes: `coord`, `crosshair`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::inventory()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["coord", "crosshair"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_are_exact_case_insensitive_and_bounded() {
        let plugin = CoordinateToolPlugin;
        for (query, expected) in [
            ("coord", "coord:toggle"),
            ("COORD ON", "coord:on"),
            ("coord space CLIENT", "coord:space:client"),
            ("coord offset -512 +512", "coord:offset:-512:+512"),
            ("coord pick", "coord:pick"),
            ("COORD cancel", "coord:cancel"),
            ("crosshair color #Aa00FF", "crosshair:color:#aa00ff"),
            ("CROSSHAIR guides OFF", "crosshair:guides:off"),
        ] {
            let results = plugin.search(query);
            assert_eq!(results.len(), 1, "{query:?}");
            assert_eq!(results[0].action, expected.to_ascii_lowercase());
        }

        for query in [
            "coordx",
            "coord unknown",
            "coord offset 513 0",
            "coord offset 1",
            "coord pick extra",
            "coord cancel extra",
            "crosshair thickness 0",
            "crosshair length 257",
            "crosshair opacity NaN",
            "crosshair color red",
            "crosshair guides maybe",
            "crosshair help trailing",
        ] {
            assert!(plugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn inventory_and_metadata_expose_passive_and_capture_controls() {
        let plugin = CoordinateToolPlugin;
        assert_eq!(plugin.query_prefixes(), ["coord", "crosshair"]);
        assert!(
            plugin
                .description()
                .to_ascii_lowercase()
                .contains("physical-pixel")
        );
        let commands = plugin.commands();
        for required in [
            "coord:help",
            "coord:copy",
            "coord:pick",
            "coord:cancel",
            "coord:space:client",
            "coord:offset:16:24",
            "crosshair:help",
            "crosshair:guides:on",
        ] {
            assert!(
                commands.iter().any(|action| action.action == required),
                "{required}"
            );
        }
        assert_eq!(plugin.search("coord help")[0].action, "coord:help");
        assert_eq!(plugin.search("crosshair help")[0].action, "crosshair:help");
    }
}
