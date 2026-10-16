use crate::actions::Action;
use crate::plugin::Plugin;

pub struct HashGeneratorPlugin;

impl Plugin for HashGeneratorPlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("hash") {
            return Vec::new();
        }

        vec![Action {
            label: "Open Hash Generator".into(),
            desc: "Hash text, loaded clipboard text, or a local file with SHA-256, SHA-512, or legacy SHA-1".into(),
            action: "hash:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "hash_generator"
    }

    fn description(&self) -> &str {
        "Calculate SHA-256, SHA-512, and legacy SHA-1 checksums (prefix: `hash`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["hash"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "hash".into(),
            desc: "Open the Hash Generator".into(),
            action: "query:hash".into(),
            args: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, HashGeneratorCommand, parse_action};

    #[test]
    fn hash_prefix_opens_the_typed_generator_command() {
        for query in ["hash", " HASH ", "\thAsH\n"] {
            let actions = HashGeneratorPlugin.search(query);
            assert_eq!(actions.len(), 1, "{query:?}");
            assert_eq!(actions[0].action, "hash:open");
            assert!(actions[0].args.is_none());
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::HashGenerator(HashGeneratorCommand::Open)
            );
        }
        for query in ["hashfoo", "hash file.txt", "hash:open"] {
            assert!(HashGeneratorPlugin.search(query).is_empty(), "{query:?}");
        }
    }

    #[test]
    fn quick_help_exposes_a_single_hash_entry() {
        let commands = HashGeneratorPlugin.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].action, "query:hash");
        assert_eq!(HashGeneratorPlugin.search("hash")[0].action, "hash:open");
    }
}
