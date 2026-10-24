use crate::actions::Action;
use crate::plugin::Plugin;

pub struct DuplicateFinderPlugin;

impl Plugin for DuplicateFinderPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("dff") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Duplicate Finder".into(),
            desc: "Find byte-identical files under folders you explicitly select".into(),
            action: "dff:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "duplicate_finder"
    }

    fn description(&self) -> &str {
        "Read-only duplicate discovery for explicitly selected folders (prefix: `dff`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["dff"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "dff".into(),
            desc: "Open Duplicate Finder".into(),
            action: "query:dff".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, DialogCommand, parse_action};
    use std::collections::{HashMap, HashSet};

    #[test]
    fn dff_command_plugin_is_exact_and_routes_to_the_typed_dialog() {
        for query in ["dff", "DFF", " dff "] {
            let actions = DuplicateFinderPlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "dff:open");
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::DuplicateFinder)
            );
        }
        for query in ["dffx", "dff duplicates", "dff:open"] {
            assert!(DuplicateFinderPlugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(DuplicateFinderPlugin.name(), "duplicate_finder");
        assert_eq!(DuplicateFinderPlugin.query_prefixes(), ["dff"]);
        assert_eq!(DuplicateFinderPlugin.capabilities(), ["search"]);
        assert_eq!(DuplicateFinderPlugin.commands()[0].action, "query:dff");
    }

    #[test]
    fn dff_command_builtin_obeys_enablement_and_search_capability() {
        let workspace = tempfile::tempdir().unwrap();
        let mut manager = crate::plugin::PluginManager::new_inert_for_test_at(workspace.path());
        manager.register_builtin_for_settings_test(DuplicateFinderPlugin);

        let catalog = manager
            .settings_catalog_snapshot_if_changed(None)
            .expect("the builtin catalog entry is published");
        let entry = catalog
            .plugins
            .iter()
            .find(|entry| entry.name == "duplicate_finder")
            .expect("the manager registers Duplicate Finder");
        assert!(entry.enabled);
        assert_eq!(entry.capabilities, vec!["search"]);

        let enabled = HashSet::from(["duplicate_finder".to_owned()]);
        let searchable =
            HashMap::from([("duplicate_finder".to_owned(), vec!["search".to_owned()])]);
        assert_eq!(
            manager.search_filtered("dff", Some(&enabled), Some(&searchable))[0].action,
            "dff:open"
        );
        assert!(
            manager
                .search_filtered("dff", Some(&HashSet::new()), Some(&searchable))
                .is_empty()
        );
        let unavailable = HashMap::from([("duplicate_finder".to_owned(), Vec::new())]);
        assert!(
            manager
                .search_filtered("dff", Some(&enabled), Some(&unavailable))
                .is_empty()
        );
    }
}
