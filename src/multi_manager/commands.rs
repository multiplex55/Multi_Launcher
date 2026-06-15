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
    ("mm reconnect", "Reconnect MultiManager windows", "mm:reconnect"),
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

pub fn search_mm_commands(query: &str) -> Vec<Action> {
    let q = query.trim();
    let q_lc = q.to_ascii_lowercase();
    let Some(rest) = crate::common::strip_prefix_ci(q, "mm") else {
        return Vec::new();
    };
    if !rest.is_empty() && !rest.starts_with(' ') {
        return Vec::new();
    }

    COMMANDS
        .iter()
        .filter(|(command, _, _)| command.starts_with(&q_lc))
        .map(|(command, desc, action)| Action {
            label: (*command).into(),
            desc: (*desc).into(),
            action: (*action).into(),
            args: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::search_mm_commands;

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
        assert!(actions.iter().any(|a| a.action == "mm:send-all-home"));
    }

    #[test]
    fn mm_reconnect_returns_reconnect() {
        let actions = search_mm_commands("mm reconnect");
        assert!(actions.iter().any(|a| a.action == "mm:reconnect"));
    }

    #[test]
    fn non_mm_input_returns_no_result() {
        assert!(search_mm_commands("todo list").is_empty());
        assert!(search_mm_commands("mms").is_empty());
    }
}
