use crate::actions::Action;
use crate::plugin::Plugin;

pub struct TableUtilityPlugin;

impl Plugin for TableUtilityPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("table") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Table Utility".into(),
            desc: "Inspect CSV or TSV text, filter rows, and export selected results".into(),
            action: "table:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "table_utility"
    }

    fn description(&self) -> &str {
        "Read-only CSV and TSV table utility (prefix: `table`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["table"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "table".into(),
            desc: "Open the Table Utility".into(),
            action: "query:table".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, TableUtilityCommand, parse_action};
    use crate::table_utility::TableUtilityProfile;

    #[test]
    fn table_command_search_is_exact_case_insensitive_and_typed() {
        for query in ["table", "TABLE", " Table "] {
            let actions = TableUtilityPlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "table:open");
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::TableUtility(TableUtilityCommand::Open {
                    profile: TableUtilityProfile::Table,
                })
            );
        }
        for query in ["tables", "table csv", "table:"] {
            assert!(TableUtilityPlugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(TableUtilityPlugin.query_prefixes(), ["table"]);
        assert_eq!(TableUtilityPlugin.capabilities(), ["search"]);
        assert_eq!(TableUtilityPlugin.commands()[0].action, "query:table");
    }
}
