use super::{CommandHelpProvider, supported_launcher_search_actions};
use multi_launcher::actions::Action;
use std::{collections::HashSet, sync::Arc};

struct CalculatorMetadata(multi_launcher::plugins_builtin::CalculatorPlugin);

impl multi_launcher::plugin::Plugin for CalculatorMetadata {
    fn name(&self) -> &str { "calculator" }
    fn description(&self) -> &str { "Evaluate inline expressions and browse Calculator history" }
    fn capabilities(&self) -> &[&str] { self.0.capabilities() }
    fn search(&self, _: &str) -> Vec<Action> { Vec::new() }
    fn commands(&self) -> Vec<Action> {
        self.0.commands().into_iter().filter(|action| matches!(action.action.as_str(), "query:= " | "query:= history" | "query:calc list")).collect()
    }
}

#[cfg(windows)]
struct WindowsMetadata;
#[cfg(windows)]
impl multi_launcher::plugin::Plugin for WindowsMetadata {
    fn name(&self) -> &str { "windows" }
    fn description(&self) -> &str { "Switch windows (prefix: `win`)" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["win"] }
    fn search(&self, _: &str) -> Vec<Action> { Vec::new() }
    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "win".into(), desc: "Windows".into(), action: "query:win ".into(), args: None }]
    }
}

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

struct ClipboardBare;
impl multi_launcher::plugin::Plugin for ClipboardBare {
    fn name(&self) -> &str { "clipboard" }
    fn description(&self) -> &str { "Provides clipboard history search (prefix: `cb`)" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["cb"] }
    fn search(&self, query: &str) -> Vec<Action> { multi_launcher::plugins::clipboard::bare_history_opener(query).into_iter().collect() }
    fn commands(&self) -> Vec<Action> { vec![Action { label: "cb".into(), desc: "Clipboard".into(), action: "query:cb".into(), args: None }] }
}

/// Providers admitted by GPUI Search, including the owners of watched stores.
pub(super) struct InstalledLauncherProviders {
    pub(super) hash_generator: multi_launcher::plugins::hash_generator::HashGeneratorPlugin,
    settings: multi_launcher::plugins::settings::SettingsPlugin,
    help: multi_launcher::plugins::help::HelpPlugin,
    favorites: super::favorite_search::Provider,
    activity: multi_launcher::plugins::activity::ActivityPlugin,
    json_utility: multi_launcher::plugins::json_utility::JsonUtilityPlugin,
    xml_toolkit: multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin,
    regex_tester: multi_launcher::plugins::regex_tester::RegexTesterPlugin,
    clipboard: ClipboardBare,
    file_search: FileSearchBarePlugin,
    calculator: CalculatorMetadata,
    #[cfg(windows)]
    windows: WindowsMetadata,
    date_arithmetic: multi_launcher::plugins::date_arithmetic::DateArithmeticPlugin,
    base_convert: multi_launcher::plugins::base_convert::BaseConvertPlugin,
    unit_convert: multi_launcher::plugins::unit_convert::UnitConvertPlugin,
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
            enabled_plugins.is_none_or(|enabled| enabled.contains("favorites")),
        )
    }

    fn with_providers(
        folders: Option<multi_launcher::plugins::folders::FoldersPlugin>,
        bookmarks: Option<multi_launcher::plugins::bookmarks::BookmarksPlugin>,
        watch_favorites: bool,
    ) -> Self {
        Self {
            hash_generator: multi_launcher::plugins::hash_generator::HashGeneratorPlugin,
            settings: multi_launcher::plugins::settings::SettingsPlugin,
            help: multi_launcher::plugins::help::HelpPlugin,
            favorites: super::favorite_search::Provider::new(watch_favorites),
            activity: multi_launcher::plugins::activity::ActivityPlugin,
            json_utility: multi_launcher::plugins::json_utility::JsonUtilityPlugin,
            xml_toolkit: multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin,
            regex_tester: multi_launcher::plugins::regex_tester::RegexTesterPlugin,
            clipboard: ClipboardBare,
            file_search: FileSearchBarePlugin(
                multi_launcher::plugins::file_search::FileSearchPlugin::default(),
            ),
            calculator: CalculatorMetadata(multi_launcher::plugins_builtin::CalculatorPlugin::default()),
            #[cfg(windows)]
            windows: WindowsMetadata,
            date_arithmetic: multi_launcher::plugins::date_arithmetic::DateArithmeticPlugin,
            base_convert: multi_launcher::plugins::base_convert::BaseConvertPlugin,
            unit_convert: multi_launcher::plugins::unit_convert::UnitConvertPlugin,
            #[cfg(windows)]
            downloads_triage: multi_launcher::plugins::downloads_triage::DownloadsTriagePlugin,
            folders,
            bookmarks,
        }
    }

    #[cfg(test)]
    pub(super) fn test_without_persisted_providers() -> Self {
        Self::with_providers(None, None, false)
    }

    pub(super) fn plugins<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a dyn multi_launcher::plugin::Plugin> {
        [
            Some(&self.hash_generator as &dyn multi_launcher::plugin::Plugin),
            Some(&self.settings as &dyn multi_launcher::plugin::Plugin),
            Some(&self.help as &dyn multi_launcher::plugin::Plugin),
            Some(&self.favorites as &dyn multi_launcher::plugin::Plugin),
            Some(&self.activity as &dyn multi_launcher::plugin::Plugin),
            Some(&self.json_utility as &dyn multi_launcher::plugin::Plugin),
            Some(&self.xml_toolkit as &dyn multi_launcher::plugin::Plugin),
            Some(&self.regex_tester as &dyn multi_launcher::plugin::Plugin),
            Some(&self.clipboard as &dyn multi_launcher::plugin::Plugin),
            Some(&self.file_search as &dyn multi_launcher::plugin::Plugin),
            Some(&self.calculator as &dyn multi_launcher::plugin::Plugin),
            #[cfg(windows)]
            Some(&self.windows as &dyn multi_launcher::plugin::Plugin),
            Some(&self.date_arithmetic as &dyn multi_launcher::plugin::Plugin),
            Some(&self.unit_convert as &dyn multi_launcher::plugin::Plugin),
            Some(&self.base_convert as &dyn multi_launcher::plugin::Plugin),
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
        .chain(
            WebProvider::ALL
                .into_iter()
                .map(|provider| -> &'a dyn multi_launcher::plugin::Plugin { provider.plugin() }),
        )
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
        let mut commands: Vec<_> = self.plugins()
            .into_iter()
            .filter(|provider| {
                enabled_plugins.is_none_or(|enabled| enabled.contains(provider.name()))
            })
            .flat_map(admitted_commands)
            .collect();
        commands.sort_by_cached_key(|action| action.label.to_lowercase());
        commands
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WebProvider {
    Google,
    Reddit,
    Runescape,
    Weather,
    Wikipedia,
    Youtube,
}

impl WebProvider {
    const ALL: [Self; 6] = [
        Self::Google,
        Self::Reddit,
        Self::Runescape,
        Self::Weather,
        Self::Wikipedia,
        Self::Youtube,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "web_search" => Some(Self::Google),
            "reddit" => Some(Self::Reddit),
            "runescape_search" => Some(Self::Runescape),
            "weather" => Some(Self::Weather),
            "wikipedia" => Some(Self::Wikipedia),
            "youtube" => Some(Self::Youtube),
            _ => None,
        }
    }

    pub(super) fn plugin(self) -> &'static dyn multi_launcher::plugin::Plugin {
        match self {
            Self::Google => &multi_launcher::plugins_builtin::WebSearchPlugin,
            Self::Reddit => &multi_launcher::plugins::reddit::RedditPlugin,
            Self::Runescape => &multi_launcher::plugins::runescape::RunescapeSearchPlugin,
            Self::Weather => &multi_launcher::plugins::weather::WeatherPlugin,
            Self::Wikipedia => &multi_launcher::plugins::wikipedia::WikipediaPlugin,
            Self::Youtube => &multi_launcher::plugins::youtube::YoutubePlugin,
        }
    }

    pub fn is_current(
        self,
        query: &str,
        action: &Action,
        enabled: Option<&HashSet<String>>,
        caps: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> bool {
        search_routed([self.plugin()], query, enabled, caps)
            .iter()
            .any(|(name, row)| Self::from_name(name) == Some(self) && row == action)
    }
}

/// Preserve the foreground launcher's exclusive, raw `g ` route.
pub(super) fn search_routed<'a>(
    plugins: impl IntoIterator<Item = &'a dyn multi_launcher::plugin::Plugin>,
    query: &str,
    enabled: Option<&HashSet<String>>,
    caps: Option<&std::collections::HashMap<String, Vec<String>>>,
) -> Vec<(String, Action)> {
    let google_only = query.trim().to_lowercase().starts_with("g ")
        .then(|| HashSet::from(["web_search".to_owned()]));
    multi_launcher::plugin::search_filtered_plugins_with_origin(
        plugins, query, google_only.as_ref().or(enabled), caps,
    )
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

    #[test]
    fn installed_activity_provider_uses_stock_search_gates() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["activity".to_owned()]);
        let rows = super::search_routed(providers.plugins(), "activity", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "activity");
        assert!(super::super::activity::provider_is_current("activity", &rows[0].1, Some(&enabled), None));
        let caps = HashMap::from([("activity".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers.plugins(), "activity", Some(&enabled), Some(&caps)).is_empty());
    }

    #[test]
    fn installed_web_providers_use_shared_search_and_capability_gates() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        for (name, query) in [("reddit", "red cat"), ("runescape_search", "rs cat"),
            ("runescape_search", "osrs cat"), ("weather", "weather London"),
            ("wikipedia", "wiki cat"), ("youtube", "yt cat")] {
            let enabled = HashSet::from([name.to_owned()]);
            let caps = HashMap::from([(name.to_owned(), vec!["search".to_owned()])]);
            let rows = search_filtered_plugins_with_origin(providers.plugins(), query, Some(&enabled), Some(&caps));
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, name);
            assert!(super::WebProvider::from_name(name).unwrap().is_current(query, &rows[0].1, Some(&enabled), Some(&caps)));
            assert!(!super::WebProvider::from_name(name).unwrap().is_current(query, &rows[0].1, Some(&HashSet::new()), Some(&caps)));
            let disabled = HashMap::from([(name.to_owned(), Vec::new())]);
            assert!(!super::WebProvider::from_name(name).unwrap().is_current(query, &rows[0].1, Some(&enabled), Some(&disabled)));
        }
        assert_eq!(super::WebProvider::from_name("web_search"), Some(super::WebProvider::Google));
    }

    #[test]
    fn convert_cached_commands_follow_stable_egui_order() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["base_convert".to_owned(), "unit_convert".to_owned()]);
        let commands = providers.cached_commands(Some(&enabled));
        let rows = commands.iter().map(|action| (action.label.as_str(), action.desc.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(rows, [
            ("conv", "Unit convert"),
            ("conv", "Base Convert"),
            ("convert", "Unit convert"),
            ("convert", "Base Convert"),
        ]);
    }

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
    fn google_exclusive_route_preserves_shared_query_and_capability_policy() {
        let google = multi_launcher::plugins_builtin::WebSearchPlugin;
        let competitor = Fixture {
            name: "competitor", prefix: "g", rows: vec![action("competitor", "other")], commands: Vec::new(),
        };
        let providers: [&dyn Plugin; 2] = [&google, &competitor];
        let disabled = HashSet::from(["competitor".to_owned()]);
        let query = "g Café+Tea";
        let rows = super::search_routed(providers, query, Some(&disabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "web_search");
        assert_eq!(rows[0].1.action, "https://www.google.com/search?q=Caf%C3%A9%2BTea");
        assert!(super::WebProvider::Google.is_current(query, &rows[0].1, Some(&disabled), None));
        let no_search = HashMap::from([("web_search".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers, query, Some(&disabled), Some(&no_search)).is_empty());
        assert!(!super::WebProvider::Google.is_current(query, &rows[0].1, Some(&disabled), Some(&no_search)));
        assert!(super::search_routed(providers, "G cat", Some(&disabled), None).is_empty());
        let tab = super::search_routed(providers, "g\tcat", Some(&disabled), None);
        assert_eq!(tab.len(), 1);
        assert_eq!(tab[0].0, "competitor");
        let enabled = HashSet::from(["web_search".to_owned()]);
        assert!(super::search_routed(providers, "kind:https g cat", Some(&disabled), None).is_empty());
        let filtered = super::search_routed(providers, "kind:https g cat", Some(&enabled), None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.action, "https://www.google.com/search?q=cat");
        assert!(super::search_routed(providers, "g ", Some(&enabled), None).is_empty());
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
