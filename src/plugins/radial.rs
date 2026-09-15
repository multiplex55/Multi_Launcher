use crate::actions::Action;
use crate::plugin::Plugin;

pub struct RadialPlugin;

impl RadialPlugin {
    fn action(
        label: impl Into<String>,
        description: impl Into<String>,
        wire: impl Into<String>,
    ) -> Action {
        Action {
            label: label.into(),
            desc: description.into(),
            action: wire.into(),
            args: None,
        }
    }

    fn inventory() -> Vec<Action> {
        let mut actions = vec![
            Self::action("Show default radial menu", "Radial menu", "radial"),
            Self::action("Close radial menu", "Radial menu", "radial close"),
            Self::action("Edit radial menus", "Radial menu", "radial edit"),
            Self::action("Edit radial skins", "Radial menu", "radial skins"),
        ];
        actions.extend(Self::menu_inventory());
        actions
    }

    fn menu_inventory() -> Vec<Action> {
        crate::gui::radial_published_document()
            .menus
            .iter()
            .map(|menu| {
                Self::action(
                    format!("Show radial menu {}", menu.name),
                    format!("Radial menu · exact ID {}", menu.id),
                    format!("radial show {}", menu.id),
                )
            })
            .collect()
    }
}

impl Plugin for RadialPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(query, "radial") else {
            return Vec::new();
        };
        if !rest.is_empty() && !rest.chars().next().is_some_and(char::is_whitespace) {
            return Vec::new();
        }
        let rest = rest.trim();
        if rest.is_empty() {
            return vec![Self::inventory().remove(0)];
        }
        if let Some(target) = crate::common::strip_prefix_ci(rest, "show") {
            if !target.is_empty() && !target.chars().next().is_some_and(char::is_whitespace) {
                return Vec::new();
            }
            let target = target.trim();
            return Self::menu_inventory()
                .into_iter()
                .filter(|action| {
                    target.is_empty()
                        || action.label.to_lowercase().contains(&target.to_lowercase())
                        || action
                            .action
                            .to_lowercase()
                            .contains(&target.to_lowercase())
                })
                .collect();
        }
        Self::inventory()
            .into_iter()
            .filter(|action| action.action == format!("radial {rest}"))
            .collect()
    }

    fn name(&self) -> &str {
        "radial"
    }

    fn description(&self) -> &str {
        "Show, close, and edit radial menus. New profiles start with editable Favorites, Apps, Windows, Macros, Notes, Snippets/Clipboard, Screen Tools, and Dashboard menus (prefix: `radial`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::inventory()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["radial"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_and_help_inventory_expose_typed_radial_commands() {
        let plugin = RadialPlugin;
        let cases = [
            ("radial", "radial"),
            ("RADIAL close", "radial close"),
            ("radial edit", "radial edit"),
            ("radial skins", "radial skins"),
        ];
        for (query, wire) in cases {
            assert_eq!(plugin.search(query)[0].action, wire);
        }
        let menu = crate::gui::radial_published_document().menus[0].clone();
        assert_eq!(
            plugin.search(&format!("radial show {}", menu.name))[0].action,
            format!("radial show {}", menu.id)
        );
        assert!(plugin.commands().len() >= 5);
        assert!(plugin.description().contains("radial"));
        assert!(plugin.search("radials").is_empty());
        assert!(!plugin.search("radial show").is_empty());
        assert!(plugin.search("radial show definitely-missing").is_empty());
    }
}
