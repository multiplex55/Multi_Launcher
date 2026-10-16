use crate::actions::Action;
use crate::common::strip_prefix_ci;
use crate::paste_sequence::{PASTE_SEQUENCES_FILE, SequenceStore};
use crate::plugin::Plugin;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::{Path, PathBuf};

const PREFIXES: &[&str] = &["paste"];

pub struct PasteSequencePlugin {
    matcher: SkimMatcherV2,
    path: PathBuf,
}

impl Default for PasteSequencePlugin {
    fn default() -> Self {
        Self::new_for_path(PASTE_SEQUENCES_FILE)
    }
}

impl Plugin for PasteSequencePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        let query = query.trim();
        let Some(rest) = strip_prefix_ci(query, "paste") else {
            return Vec::new();
        };
        let rest = rest.trim();
        let needle = if rest.eq_ignore_ascii_case("open") || rest.eq_ignore_ascii_case(":open") {
            ""
        } else {
            rest
        };
        let mut actions = Vec::new();
        if needle.is_empty() {
            actions.push(Action {
                label: "Open Paste Sequences".into(),
                desc: "Create and manage paste sequences".into(),
                action: "paste:open".into(),
                args: None,
            });
        }
        let Ok(snapshot) = SequenceStore::new(&self.path).load() else {
            return actions;
        };

        let mut sequences = snapshot.sequences().iter().collect::<Vec<_>>();
        if !needle.is_empty() {
            sequences.retain(|sequence| self.matcher.fuzzy_match(&sequence.name, needle).is_some());
            sequences.sort_by_key(|sequence| {
                self.matcher
                    .fuzzy_match(&sequence.name, needle)
                    .unwrap_or(i64::MIN)
            });
            sequences.reverse();
        }

        actions.extend(sequences.into_iter().map(|sequence| Action {
            label: sequence.name.clone(),
            desc: "Paste Sequence".into(),
            action: format!("paste:run:{}", sequence.id.get()),
            args: None,
        }));
        actions
    }

    fn name(&self) -> &str {
        "paste_sequence"
    }

    fn description(&self) -> &str {
        "Build and run ordered text paste sequences (prefix: `paste`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        PREFIXES
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "paste".into(),
            desc: "Paste Sequences".into(),
            action: "query:paste".into(),
            args: None,
        }]
    }
}

impl PasteSequencePlugin {
    pub fn new_for_path(path: impl AsRef<Path>) -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
            path: path.as_ref().to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paste_sequence::{PlaybackMode, SequenceDraft, SequenceStore, Step};

    #[test]
    fn search_actions_contain_only_stable_ids_and_command_opens_editor() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(PASTE_SEQUENCES_FILE);
        let store = SequenceStore::new(&path);
        let mut snapshot = store.load().unwrap();
        let id = snapshot
            .upsert(SequenceDraft {
                id: None,
                name: "Daily reply".into(),
                hotkey: None,
                mode: PlaybackMode::Automatic,
                delay_ms: 10,
                steps: vec![Step::LiteralText {
                    text: "secret message".into(),
                }],
            })
            .unwrap();
        store.commit(&mut snapshot).unwrap();

        let plugin = PasteSequencePlugin::new_for_path(&path);
        let actions = plugin.search("paste daily");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, format!("paste:run:{}", id.get()));
        assert_eq!(actions[0].args, None);
        assert!(!actions[0].action.contains("secret message"));
        assert_eq!(plugin.commands()[0].action, "query:paste");

        let root_actions = plugin.search("paste");
        assert_eq!(root_actions[0].action, "paste:open");
        assert!(matches!(
            crate::commands::parse_action(&root_actions[0]),
            Ok(crate::commands::Command::PasteSequence(
                crate::commands::PasteSequenceCommand::Open
            ))
        ));
        assert_eq!(plugin.search("paste:open")[0].action, "paste:open");
    }

    #[test]
    fn malformed_saved_store_still_exposes_typed_open_action() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(PASTE_SEQUENCES_FILE);
        std::fs::write(&path, "{ malformed").unwrap();
        let plugin = PasteSequencePlugin::new_for_path(&path);

        let actions = plugin.search("paste");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "paste:open");
        assert!(matches!(
            crate::commands::parse_action(&actions[0]),
            Ok(crate::commands::Command::PasteSequence(
                crate::commands::PasteSequenceCommand::Open
            ))
        ));
        assert_eq!(plugin.search("paste open")[0].action, "paste:open");
    }
}
