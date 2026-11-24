use super::{CommandHelpProvider, supported_launcher_search_actions};
use multi_launcher::actions::Action;
use std::{collections::HashSet, sync::Arc};

struct FileSearchBarePlugin(multi_launcher::plugins::file_search::FileSearchPlugin);

impl multi_launcher::plugin::Plugin for FileSearchBarePlugin {
    fn name(&self) -> &str {
        "file_search"
    }
    fn description(&self) -> &str {
        "Open local File Search"
    }
    fn capabilities(&self) -> &[&str] {
        &["search"]
    }
    fn query_prefixes(&self) -> &[&str] {
        &["fs"]
    }
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("fs") {
            multi_launcher::plugin::Plugin::search(&self.0, query)
        } else {
            Vec::new()
        }
    }
    fn commands(&self) -> Vec<Action> {
        multi_launcher::plugin::Plugin::commands(&self.0)
            .into_iter()
            .filter(|action| action.label == "fs")
            .collect()
    }
}

/// Providers admitted by GPUI Search, including the owners of watched stores.
pub(super) struct InstalledLauncherProviders {
    pub(super) hash_generator: multi_launcher::plugins::hash_generator::HashGeneratorPlugin,
    settings: multi_launcher::plugins::settings::SettingsPlugin,
    help: multi_launcher::plugins::help::HelpPlugin,
    favorites: multi_launcher::plugins::fav::FavBrowsePlugin,
    file_search: FileSearchBarePlugin,
    #[cfg(windows)]
    downloads_triage: multi_launcher::plugins::downloads_triage::DownloadsTriagePlugin,
    folders: Option<multi_launcher::plugins::folders::FoldersPlugin>,
    bookmarks: Option<multi_launcher::plugins::bookmarks::BookmarksPlugin>,
}

impl InstalledLauncherProviders {
    #[cfg(not(test))]
    pub(super) fn new(enabled_plugins: Option<&HashSet<String>>) -> Self {
        Self::with_providers(
            enabled_plugins
                .is_none_or(|enabled| enabled.contains("folders"))
                .then(multi_launcher::plugins::folders::FoldersPlugin::new),
            enabled_plugins
                .is_none_or(|enabled| enabled.contains("bookmarks"))
                .then(multi_launcher::plugins::bookmarks::BookmarksPlugin::new),
        )
    }

    fn with_providers(
        folders: Option<multi_launcher::plugins::folders::FoldersPlugin>,
        bookmarks: Option<multi_launcher::plugins::bookmarks::BookmarksPlugin>,
    ) -> Self {
        Self {
            hash_generator: multi_launcher::plugins::hash_generator::HashGeneratorPlugin,
            settings: multi_launcher::plugins::settings::SettingsPlugin,
            help: multi_launcher::plugins::help::HelpPlugin,
            favorites: multi_launcher::plugins::fav::FavBrowsePlugin,
            file_search: FileSearchBarePlugin(
                multi_launcher::plugins::file_search::FileSearchPlugin::default(),
            ),
            #[cfg(windows)]
            downloads_triage: multi_launcher::plugins::downloads_triage::DownloadsTriagePlugin,
            folders,
            bookmarks,
        }
    }

    #[cfg(test)]
    pub(super) fn test_without_persisted_providers() -> Self {
        Self::with_providers(None, None)
    }

    pub(super) fn plugins(&self) -> impl Iterator<Item = &dyn multi_launcher::plugin::Plugin> {
        [
            Some(&self.hash_generator as &dyn multi_launcher::plugin::Plugin),
            Some(&self.settings as &dyn multi_launcher::plugin::Plugin),
            Some(&self.help as &dyn multi_launcher::plugin::Plugin),
            Some(&self.favorites as &dyn multi_launcher::plugin::Plugin),
            Some(&self.file_search as &dyn multi_launcher::plugin::Plugin),
            #[cfg(windows)]
            Some(&self.downloads_triage as &dyn multi_launcher::plugin::Plugin),
            self.folders
                .as_ref()
                .map(|p| p as &dyn multi_launcher::plugin::Plugin),
            self.bookmarks
                .as_ref()
                .map(|p| p as &dyn multi_launcher::plugin::Plugin),
        ]
        .into_iter()
        .flatten()
    }

    pub(super) fn current_search_aliases(
        &self,
        targets: &HashSet<&str>,
    ) -> (
        Option<std::collections::HashMap<String, Option<String>>>,
        Option<std::collections::HashMap<String, Option<String>>>,
    ) {
        (
            self.folders
                .as_ref()
                .map(|folder| folder.search_aliases_lc(targets)),
            self.bookmarks
                .as_ref()
                .map(|bookmark| bookmark.search_aliases_lc(targets)),
        )
    }

    pub(super) fn aliased_folder_rows(&self) -> std::collections::HashMap<String, HashSet<String>> {
        self.folders
            .as_ref()
            .map(|folder| folder.aliased_search_rows())
            .unwrap_or_default()
    }

    pub(super) fn cached_commands(&self, enabled_plugins: Option<&HashSet<String>>) -> Vec<Action> {
        self.plugins()
            .into_iter()
            .filter(|provider| {
                enabled_plugins.is_none_or(|enabled| enabled.contains(provider.name()))
            })
            .flat_map(admitted_commands)
            .collect()
    }

    pub(super) fn command_help_providers(&self) -> Arc<[CommandHelpProvider]> {
        let mut providers = self
            .plugins()
            .into_iter()
            .map(|provider| CommandHelpProvider {
                name: provider.name().to_owned(),
                description: provider.description().to_owned(),
                capabilities: provider
                    .capabilities()
                    .iter()
                    .map(|capability| (*capability).to_owned())
                    .collect(),
                commands: admitted_commands(provider),
            })
            .collect::<Vec<_>>();
        providers.sort_by(|left, right| left.name.cmp(&right.name));
        providers.into()
    }
}

fn admitted_commands(provider: &dyn multi_launcher::plugin::Plugin) -> Vec<Action> {
    supported_launcher_search_actions(provider.commands())
        .into_iter()
        .filter(|action| match provider.name() {
            "folders" => matches!(action.action.as_str(), "query:f " | "query:f list "),
            "bookmarks" => action.action == "query:bm list",
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::admitted_commands;
    use multi_launcher::{
        actions::Action,
        plugin::{Plugin, search_filtered_plugins_with_origin},
    };
    use std::collections::{HashMap, HashSet};

    struct Fixture {
        name: &'static str,
        prefix: &'static str,
        rows: Vec<Action>,
        commands: Vec<Action>,
    }
    impl Plugin for Fixture {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "fixture"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
        fn query_prefixes(&self) -> &[&str] {
            std::slice::from_ref(&self.prefix)
        }
        fn commands(&self) -> Vec<Action> {
            self.commands.clone()
        }
        fn search(&self, _: &str) -> Vec<Action> {
            self.rows.clone()
        }
    }
    fn action(label: &str, command: &str) -> Action {
        Action {
            label: label.into(),
            desc: String::new(),
            action: command.into(),
            args: None,
        }
    }

    #[test]
    fn provider_admission_keeps_navigation_and_shared_gate_order() {
        let folder = Fixture {
            name: "folders",
            prefix: "f",
            rows: vec![action("one", "C:\\one"), action("two", "C:\\two")],
            commands: vec![
                action("f", "query:f "),
                action("f list", "query:f list "),
                action("f add", "query:f add "),
                action("f rm", "query:f rm "),
            ],
        };
        let bookmark = Fixture {
            name: "bookmarks",
            prefix: "bm",
            rows: vec![action("site", "https://example.com")],
            commands: vec![
                action("bm", "query:bm "),
                action("bm list", "query:bm list"),
                action("bm add", "query:bm add "),
            ],
        };
        assert_eq!(
            admitted_commands(&folder)
                .iter()
                .map(|a| a.label.as_str())
                .collect::<Vec<_>>(),
            vec!["f", "f list"]
        );
        assert_eq!(
            admitted_commands(&bookmark)
                .iter()
                .map(|a| a.label.as_str())
                .collect::<Vec<_>>(),
            vec!["bm list"]
        );
        let providers: [&dyn Plugin; 2] = [&folder, &bookmark];
        let enabled = HashSet::from(["folders".to_owned(), "bookmarks".to_owned()]);
        let caps = HashMap::from([
            ("folders".to_owned(), vec!["search".to_owned()]),
            ("bookmarks".to_owned(), vec![]),
        ]);
        let rows =
            search_filtered_plugins_with_origin(providers, "F list", Some(&enabled), Some(&caps));
        assert_eq!(
            rows,
            vec![
                ("folders".to_owned(), folder.rows[0].clone()),
                ("folders".to_owned(), folder.rows[1].clone())
            ]
        );
        assert!(
            search_filtered_plugins_with_origin(providers, "bm list", Some(&enabled), Some(&caps))
                .is_empty()
        );
        assert!(
            search_filtered_plugins_with_origin(providers, "f", Some(&HashSet::new()), None)
                .is_empty()
        );
    }
}
