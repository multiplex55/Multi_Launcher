use crate::actions::Action;
use crate::plugin::Plugin;

pub struct DataPlugin;

impl DataPlugin {
    fn actions() -> Vec<Action> {
        vec![
            action("data", "Open Data & Recovery", "data:dialog"),
            action(
                "data health",
                "Inspect persistent data health",
                "data:health",
            ),
            action("data backup", "Create a data snapshot", "data:backup"),
            action(
                "data folder",
                "Open the application data folder",
                "data:folder",
            ),
        ]
    }
}

impl Plugin for DataPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        let Some(rest) = crate::common::strip_prefix_ci(query, "data") else {
            return Vec::new();
        };
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return Vec::new();
        }
        let requested = rest.trim();
        let actions = Self::actions();
        if requested.is_empty() {
            return actions;
        }
        actions
            .into_iter()
            .filter(|action| {
                action
                    .label
                    .strip_prefix("data ")
                    .is_some_and(|suffix| suffix.eq_ignore_ascii_case(requested))
            })
            .collect()
    }

    fn name(&self) -> &str {
        "data"
    }

    fn description(&self) -> &str {
        "Inspect, back up, and recover Multi Launcher data (prefix: `data`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn commands(&self) -> Vec<Action> {
        Self::actions()
    }

    fn query_prefixes(&self) -> &[&str] {
        &["data"]
    }
}

fn action(label: &str, description: &str, action: &str) -> Action {
    Action {
        label: label.into(),
        desc: description.into(),
        action: action.into(),
        args: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(actions: Vec<Action>) -> Vec<(String, String)> {
        actions
            .into_iter()
            .map(|action| (action.label, action.action))
            .collect()
    }

    #[test]
    fn data_queries_are_exact_case_insensitive_and_non_destructive() {
        assert_eq!(
            view(DataPlugin.search("data")),
            [
                ("data".into(), "data:dialog".into()),
                ("data health".into(), "data:health".into()),
                ("data backup".into(), "data:backup".into()),
                ("data folder".into(), "data:folder".into()),
            ]
        );
        assert_eq!(
            view(DataPlugin.search("DATA HEALTH")),
            [("data health".into(), "data:health".into())]
        );
        assert_eq!(
            view(DataPlugin.search("data backup")),
            [("data backup".into(), "data:backup".into())]
        );
        assert!(DataPlugin.search("database").is_empty());
        assert!(DataPlugin.search("data reset").is_empty());
        assert!(DataPlugin.search("data restore").is_empty());
    }

    #[test]
    fn command_discovery_exposes_only_safe_data_actions() {
        let commands = DataPlugin.commands();
        assert_eq!(commands.len(), 4);
        assert!(
            commands
                .iter()
                .all(|action| action.action.starts_with("data:"))
        );
        assert!(
            commands.iter().all(
                |action| !action.action.contains("reset") && !action.action.contains("restore")
            )
        );
        assert_eq!(DataPlugin.query_prefixes(), ["data"]);
    }
}
