use crate::actions::Action;
use std::collections::HashSet;

/// Renderer-neutral metadata used by Command Help and the egui Help window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandHelpProvider {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub commands: Vec<Action>,
}

pub fn matches_filter(
    filter: &str,
    name: &str,
    description: &str,
    capabilities: &[String],
    commands: &[Action],
) -> bool {
    if filter.is_empty() {
        return true;
    }
    let filter = filter.to_lowercase();
    name.to_lowercase().contains(&filter)
        || description.to_lowercase().contains(&filter)
        || capabilities
            .iter()
            .any(|capability| capability.to_lowercase().contains(&filter))
        || commands.iter().any(|command| {
            command.label.to_lowercase().contains(&filter)
                || command.desc.to_lowercase().contains(&filter)
                || command.action.to_lowercase().contains(&filter)
        })
}

pub fn command_prefixes(commands: &[Action]) -> Vec<String> {
    let mut prefixes = HashSet::new();
    for command in commands {
        if let Some(prefix) = command.label.split_whitespace().next() {
            prefixes.insert(prefix.to_owned());
        }
    }
    let mut prefixes = prefixes.into_iter().collect::<Vec<_>>();
    prefixes.sort();
    prefixes
}

pub fn example_line(command: &Action) -> String {
    command
        .action
        .strip_prefix("query:")
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_else(|| command.label.clone())
}

#[cfg(test)]
mod tests {
    use super::{command_prefixes, example_line, matches_filter};
    use crate::actions::Action;

    fn action(label: &str, description: &str, action: &str) -> Action {
        Action {
            label: label.to_owned(),
            desc: description.to_owned(),
            action: action.to_owned(),
            args: None,
        }
    }

    #[test]
    fn command_help_filter_is_case_folded_literal_text_without_trimming() {
        let commands = [
            action("Hash Open", "Open the generator", "hash:open"),
            action("Set note", "Apply a saved query", "query:  note  "),
        ];
        let capabilities = vec!["search".to_owned()];
        assert!(matches_filter(
            "hAsH",
            "hash_generator",
            "desc",
            &capabilities,
            &commands
        ));
        assert!(matches_filter(
            "SearCH",
            "hash_generator",
            "desc",
            &capabilities,
            &[]
        ));
        assert!(matches_filter("OPEN", "plugin", "desc", &[], &commands));
        assert!(matches_filter(
            "query:  note  ",
            "plugin",
            "desc",
            &[],
            &commands
        ));
        assert!(!matches_filter(" OPEN ", "plugin", "desc", &[], &commands));
        assert!(!matches_filter(
            "note query",
            "plugin",
            "desc",
            &[],
            &commands
        ));
        assert!(matches_filter("", "plugin", "desc", &[], &[]));
    }

    #[test]
    fn command_help_prefixes_and_examples_keep_the_egui_rules() {
        let commands = [
            action("note:create", "Create a note", "query:note"),
            action("note:find item", "Find a note", "query:  note:find item  "),
            action("Open Hash", "Open the generator", "hash:open"),
        ];
        assert_eq!(
            command_prefixes(&commands),
            ["Open", "note:create", "note:find"]
        );
        assert_eq!(example_line(&commands[0]), "note");
        assert_eq!(example_line(&commands[1]), "note:find item");
        assert_eq!(example_line(&commands[2]), "Open Hash");
    }
}
