use crate::actions::Action;

const COMMANDS: [(&str, &str, &str); 10] = [
    ("mm", "Open MultiManager", "mm:open"),
    ("mm settings", "Open MultiManager settings", "mm:settings"),
    ("mm save", "Save MultiManager workspaces", "mm:save"),
    ("mm reload", "Reload MultiManager workspaces", "mm:reload"),
    (
        "mm send all home",
        "Send all MultiManager windows home",
        "mm:send-all-home",
    ),
    (
        "mm reconnect",
        "Reconnect MultiManager windows",
        "mm:reconnect",
    ),
    (
        "mm save bindings",
        "Save MultiManager window bindings",
        "mm:save-bindings",
    ),
    (
        "mm restore bindings",
        "Restore MultiManager window bindings",
        "mm:restore-bindings",
    ),
    (
        "mm recapture all",
        "Recapture all MultiManager workspaces",
        "mm:recapture-all",
    ),
    ("mm import", "Import MultiManager workspaces", "mm:import"),
];

pub fn all_mm_commands() -> Vec<Action> {
    COMMANDS
        .iter()
        .map(|(command, desc, action)| Action {
            label: (*command).into(),
            desc: (*desc).into(),
            action: (*action).into(),
            args: None,
        })
        .collect()
}

pub fn search_mm_commands(query: &str) -> Vec<Action> {
    let q = query.trim();
    let q_lc = q.to_ascii_lowercase();
    let Some(rest) = crate::common::strip_prefix_ci(q, "mm") else {
        return Vec::new();
    };
    if !rest.is_empty() && !rest.starts_with(' ') {
        return Vec::new();
    }

    all_mm_commands()
        .into_iter()
        .filter(|action| action.label.starts_with(&q_lc))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::search_mm_commands;
    use crate::plugins::multi_manager::MultiManagerPlugin;

    #[test]
    fn plugin_commands_include_public_multi_manager_actions() {
        let actions = MultiManagerPlugin::commands();
        let expected = [
            "mm:open",
            "mm:settings",
            "mm:save",
            "mm:reload",
            "mm:send-all-home",
            "mm:reconnect",
            "mm:save-bindings",
            "mm:restore-bindings",
            "mm:recapture-all",
        ];

        for expected_action in expected {
            assert!(
                actions
                    .iter()
                    .any(|action| action.action == expected_action),
                "plugin commands should include {expected_action}"
            );
        }
    }

    #[test]
    fn plugin_commands_match_mm_search_public_actions() {
        let plugin_actions: Vec<_> = MultiManagerPlugin::commands()
            .into_iter()
            .filter(|action| action.action.starts_with("mm:"))
            .map(|action| action.action)
            .collect();
        let search_actions: Vec<_> = search_mm_commands("mm")
            .into_iter()
            .filter(|action| action.action.starts_with("mm:"))
            .map(|action| action.action)
            .collect();

        assert_eq!(plugin_actions, search_actions);
    }

    #[test]
    fn exact_mm_returns_open_first() {
        let actions = search_mm_commands("mm");
        assert_eq!(actions.first().map(|a| a.action.as_str()), Some("mm:open"));
    }

    #[test]
    fn mm_settings_returns_settings() {
        let actions = search_mm_commands("mm settings");
        assert!(actions.iter().any(|a| a.action == "mm:settings"));
    }

    #[test]
    fn mm_save_returns_save() {
        let actions = search_mm_commands("mm save");
        assert!(actions.iter().any(|a| a.action == "mm:save"));
    }

    #[test]
    fn mm_reload_returns_reload() {
        let actions = search_mm_commands("mm reload");
        assert!(actions.iter().any(|a| a.action == "mm:reload"));
    }

    #[test]
    fn mm_recapture_all_returns_recapture_all() {
        let actions = search_mm_commands("mm recapture all");
        assert!(actions.iter().any(|a| a.action == "mm:recapture-all"));
    }

    #[test]
    fn mm_send_all_home_returns_send_all_home() {
        let actions = search_mm_commands("mm send all home");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "mm:send-all-home");
    }

    #[test]
    fn mm_reconnect_returns_reconnect() {
        let actions = search_mm_commands("mm reconnect");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "mm:reconnect");
    }

    #[test]
    fn non_mm_queries_return_no_multi_manager_commands() {
        for query in ["todo list", "mms", "multi manager", ""] {
            assert!(
                search_mm_commands(query)
                    .iter()
                    .all(|action| !action.action.starts_with("mm:")),
                "{query:?} should not return MultiManager commands"
            );
        }
    }
}
