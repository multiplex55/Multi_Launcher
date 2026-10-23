use crate::actions::Action;
use crate::plugin::Plugin;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        // Only trim before the command token. The search suffix is transport
        // data for the Settings editor, so keep its exact whitespace and text.
        let q = query.trim_start();
        let mut actions = Vec::new();
        if let Some(rest) = crate::common::strip_prefix_ci(q, "settings")
            && has_command_token_boundary(rest)
        {
            let search_query = rest.strip_prefix(' ').unwrap_or_default();
            if search_query.trim().is_empty() {
                actions.push(Action {
                    label: "Open settings".into(),
                    desc: "Show settings panel".into(),
                    action: "settings:dialog".into(),
                    args: None,
                });
            } else {
                actions.push(Action {
                    label: "Search Settings".into(),
                    desc: "Search application settings".into(),
                    action: "settings:search".into(),
                    args: Some(search_query.to_owned()),
                });
            }
        }
        if let Some(rest) = crate::common::strip_prefix_ci(q, "dashboard")
            && has_command_token_boundary(rest)
        {
            actions.push(Action {
                label: "Dashboard Settings".into(),
                desc: "Configure dashboard layout and widgets".into(),
                action: "dashboard:settings".into(),
                args: None,
            });
        }
        if let Some(rest) = crate::common::strip_prefix_ci(q, "theme")
            && has_command_token_boundary(rest)
        {
            actions.push(Action {
                label: "Theme settings".into(),
                desc: "Configure launcher theme colors".into(),
                action: "theme:dialog".into(),
                args: None,
            });
        }
        actions
    }

    fn name(&self) -> &str {
        "settings"
    }

    fn description(&self) -> &str {
        "Open settings panel (prefix: `settings`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![
            Action {
                label: "settings".into(),
                desc: "Settings".into(),
                action: "settings:dialog".into(),
                args: None,
            },
            Action {
                label: "Dashboard Settings".into(),
                desc: "Dashboard settings".into(),
                action: "dashboard:settings".into(),
                args: None,
            },
            Action {
                label: "Theme settings".into(),
                desc: "Configure launcher theme colors".into(),
                action: "theme:dialog".into(),
                args: None,
            },
        ]
    }
}

fn has_command_token_boundary(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(' ') || rest.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_search_entry_preserves_suffix_and_keeps_legacy_prefixes_exact() {
        let plugin = SettingsPlugin;
        let query = "  SeTtInGs   font: \"Δelta\"\tlauncher  ";
        let actions = plugin.search(query);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "settings:search");
        assert_eq!(
            actions[0].args.as_deref(),
            Some("  font: \"Δelta\"\tlauncher  ")
        );

        for whitespace_only in [
            "settings",
            "settings ",
            "settings  \t\u{2003}",
            "settings\t",
            "settings\u{2003}",
            " \u{2003}settings\t",
        ] {
            let actions = plugin.search(whitespace_only);
            assert_eq!(actions.len(), 1, "{whitespace_only:?}");
            assert_eq!(actions[0].action, "settings:dialog");
            assert!(actions[0].args.is_none());
        }

        for lookalike in [
            "settingsish font",
            "settings\tfont",
            "settings\u{2003}font",
            "settings:search",
        ] {
            assert!(
                plugin.search(lookalike).is_empty(),
                "lookalike prefix must not dispatch Settings search: {lookalike:?}"
            );
        }

        for (query, action) in [
            ("dashboard   ", "dashboard:settings"),
            ("dashboard\t", "dashboard:settings"),
            ("dashboard\u{2003}", "dashboard:settings"),
            ("theme  ", "theme:dialog"),
            ("theme\t", "theme:dialog"),
            ("theme\u{2003}", "theme:dialog"),
        ] {
            let actions = plugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, action);
            assert!(actions[0].args.is_none());
        }
    }
}
