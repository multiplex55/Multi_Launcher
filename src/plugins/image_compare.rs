use crate::actions::Action;
use crate::plugin::Plugin;

pub struct ImageComparePlugin;

impl Plugin for ImageComparePlugin {
    fn search(&self, query: &str) -> Vec<Action> {
        if !query.trim().eq_ignore_ascii_case("imgdiff") {
            return Vec::new();
        }
        vec![Action {
            label: "Open Image Comparison".into(),
            desc: "Compare two images by exact RGBA pixels without changing either source".into(),
            action: "imgdiff:open".into(),
            args: None,
        }]
    }

    fn name(&self) -> &str {
        "image_compare"
    }

    fn description(&self) -> &str {
        "Exact, session-only image comparison (prefix: `imgdiff`)"
    }

    fn capabilities(&self) -> &[&str] {
        &["search"]
    }

    fn query_prefixes(&self) -> &[&str] {
        &["imgdiff"]
    }

    fn commands(&self) -> Vec<Action> {
        vec![Action {
            label: "imgdiff".into(),
            desc: "Open Image Comparison".into(),
            action: "query:imgdiff".into(),
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
    fn imgdiff_plugin_is_an_exact_enabled_search_entry_with_typed_open() {
        for query in ["imgdiff", "IMGDIFF", " imgdiff "] {
            let actions = ImageComparePlugin.search(query);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action, "imgdiff:open");
            assert_eq!(
                parse_action(&actions[0]).unwrap(),
                Command::Dialog(DialogCommand::ImageCompare)
            );
        }
        for query in ["imgdiffusion", "imgdiff image.png", "imgdiff:open"] {
            assert!(ImageComparePlugin.search(query).is_empty(), "{query:?}");
        }
        assert_eq!(ImageComparePlugin.name(), "image_compare");
        assert_eq!(ImageComparePlugin.query_prefixes(), ["imgdiff"]);
        assert_eq!(ImageComparePlugin.capabilities(), ["search"]);
        assert_eq!(ImageComparePlugin.commands()[0].action, "query:imgdiff");
    }

    #[test]
    fn imgdiff_builtin_manager_obeys_plugin_and_search_capability_filters() {
        let workspace = tempfile::tempdir().unwrap();
        let mut manager = crate::plugin::PluginManager::new_inert_for_test_at(workspace.path());
        manager.register_builtin_for_settings_test(ImageComparePlugin);

        let catalog = manager
            .settings_catalog_snapshot_if_changed(None)
            .expect("builtin registration publishes its catalog entry");
        let entry = catalog
            .plugins
            .iter()
            .find(|entry| entry.name == "image_compare")
            .expect("the builtin manager contains Image Comparison");
        assert_eq!(entry.origin, crate::plugin::PluginCatalogOrigin::Builtin);
        assert!(entry.enabled);
        assert_eq!(entry.capabilities, vec!["search".to_owned()]);

        let enabled = HashSet::from(["image_compare".to_owned()]);
        let searchable = HashMap::from([("image_compare".to_owned(), vec!["search".to_owned()])]);
        let results = manager.search_filtered("imgdiff", Some(&enabled), Some(&searchable));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "imgdiff:open");

        assert!(
            manager
                .search_filtered("imgdiff", Some(&HashSet::new()), Some(&searchable))
                .is_empty()
        );
        let no_search = HashMap::from([("image_compare".to_owned(), Vec::new())]);
        assert!(
            manager
                .search_filtered("imgdiff", Some(&enabled), Some(&no_search))
                .is_empty()
        );
    }
}
