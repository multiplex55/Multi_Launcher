use crate::actions::Action;
use crate::commands::JsonUtilityIntent;
use crate::plugin::Plugin;

pub struct JsonUtilityPlugin;

impl Plugin for JsonUtilityPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let mut parts = query.split_whitespace();
        if !parts
            .next()
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("json"))
        {
            return Vec::new();
        }

        let intent = match (parts.next(), parts.next()) {
            (None, None) => JsonUtilityIntent::General,
            (Some(operation), None) if operation.eq_ignore_ascii_case("format") => {
                JsonUtilityIntent::Format
            }
            (Some(operation), None) if operation.eq_ignore_ascii_case("pretty") => {
                JsonUtilityIntent::Format
            }
            (Some(operation), None) if operation.eq_ignore_ascii_case("minify") => {
                JsonUtilityIntent::Minify
            }
            _ => return Vec::new(),
        };

        let (label, description, action) = match intent {
            JsonUtilityIntent::General => (
                "Open JSON Utility",
                "Open the JSON formatter and minifier",
                "json_utility:open",
            ),
            JsonUtilityIntent::Format => (
                "Format JSON",
                "Open the JSON utility in format mode",
                "json_utility:format",
            ),
            JsonUtilityIntent::Minify => (
                "Minify JSON",
                "Open the JSON utility in minify mode",
                "json_utility:minify",
            ),
        };

        vec![Action {
            label: label.into(),
            desc: description.into(),
            action: action.into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "json_utility"
    }

    fn description(&self) -> &str {
        "Format and minify strict JSON (prefix: `json`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["json"]
    }

    fn commands(&self) -> Vec<Action> {
        ["json", "json format", "json pretty", "json minify"]
            .into_iter()
            .map(|query| Action {
                label: query.into(),
                desc: "JSON formatter and minifier".into(),
                action: format!("query:{query}"),
                args: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{JsonUtilityCommand, parse_action};

    #[test]
    fn json_query_family_maps_to_general_format_alias_and_minify_intents() {
        for (query, expected) in [
            ("json", JsonUtilityIntent::General),
            ("json format", JsonUtilityIntent::Format),
            ("json pretty", JsonUtilityIntent::Format),
            ("json minify", JsonUtilityIntent::Minify),
            ("JSON FORMAT", JsonUtilityIntent::Format),
        ] {
            let actions = JsonUtilityPlugin.search(query);
            assert_eq!(actions.len(), 1, "query {query:?}");
            let command = parse_action(&actions[0]).unwrap();
            assert_eq!(
                command,
                crate::commands::Command::JsonUtility(JsonUtilityCommand::Open {
                    intent: expected
                })
            );
        }
    }

    #[test]
    fn unsupported_json_queries_do_not_produce_utility_actions() {
        for query in [
            "jsonfoo",
            "json format extra",
            "json formatter",
            "json pretty minify",
        ] {
            assert!(JsonUtilityPlugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn command_catalog_exposes_all_supported_queries() {
        let commands = JsonUtilityPlugin.commands();
        let queries = commands
            .iter()
            .map(|action| action.action.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            queries,
            [
                "query:json",
                "query:json format",
                "query:json pretty",
                "query:json minify"
            ]
        );
    }
}
