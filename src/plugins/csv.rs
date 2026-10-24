use crate::actions::Action;
use crate::plugin::Plugin;

pub struct CsvPlugin;

impl Plugin for CsvPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("csv") {
            return Vec::new();
        }
        vec![Action {
            label: "Open CSV Viewer".into(),
            desc: "Inspect a comma-delimited file with strict CSV parsing".into(),
            action: "csv:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "csv"
    }

    fn description(&self) -> &str {
        "Strict CSV inspection (prefix: `csv`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["csv"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "csv".into(),
            desc: "Open the strict CSV viewer".into(),
            action: "query:csv".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TableUtilityCommand, parse_action};
    use crate::table_utility::TableUtilityProfile;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn csv_plugin_is_an_exact_enabled_search_entry_with_typed_open() {
        for query in ["csv", "CSV", " Csv "] {
            let actions = CsvPlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "csv:open");
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::TableUtility(TableUtilityCommand::Open {
                    profile: TableUtilityProfile::Csv,
                })
            );
        }
        for query in ["csvs", "csv data.csv", "csv:"] {
            assert!(CsvPlugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(CsvPlugin.name(), "csv");
        assert_eq!(CsvPlugin.query_prefixes(), ["csv"]);
        assert_eq!(CsvPlugin.capabilities(), ["search"]);
        assert_eq!(CsvPlugin.commands()[0].action, "query:csv");

        let workspace = tempfile::tempdir().unwrap();
        let mut manager = crate::plugin::PluginManager::new_inert_for_test_at(workspace.path());
        manager.register_builtin_for_settings_test(CsvPlugin);
        let catalog = manager
            .settings_catalog_snapshot_if_changed(None)
            .expect("builtin registration publishes its catalog entry");
        let entry = catalog
            .plugins
            .iter()
            .find(|entry| entry.name == "csv")
            .expect("the built-in manager contains CSV");
        assert!(entry.enabled);
        assert_eq!(entry.capabilities, vec!["search".to_owned()]);

        let enabled = HashSet::from(["csv".to_owned()]);
        let searchable = HashMap::from([("csv".to_owned(), vec!["search".to_owned()])]);
        assert_eq!(
            manager
                .search_filtered("csv", Some(&enabled), Some(&searchable))
                .iter()
                .map(|action| action.action.as_str())
                .collect::<Vec<_>>(),
            ["csv:open"]
        );
        assert!(
            manager
                .search_filtered("csv", Some(&HashSet::new()), Some(&searchable))
                .is_empty()
        );
        let no_search = HashMap::from([("csv".to_owned(), Vec::new())]);
        assert!(
            manager
                .search_filtered("csv", Some(&enabled), Some(&no_search))
                .is_empty()
        );
    }
}
