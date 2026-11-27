use super::{CommandHelpProvider, supported_launcher_search_actions};
use multi_launcher::actions::Action;
use std::{collections::HashSet, sync::Arc};

pub(super) fn clipboard_catalog_is_current(
    shared: &multi_launcher::clipboard_modify::runtime::SharedClipboardModifierCatalog,
    expected: &Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>,
) -> bool {
    shared.read().ok().is_some_and(|live| Arc::ptr_eq(&live, expected))
}

struct CalculatorMetadata(multi_launcher::plugins_builtin::CalculatorPlugin);

pub(super) struct NoteSnapshot {
    pub version: u64,
    pub by_slug: std::collections::HashMap<String, Arc<multi_launcher::plugins::note::Note>>,
}
#[derive(Default)]
pub(super) struct NoteSnapshots(std::sync::Mutex<Option<Arc<NoteSnapshot>>>);
impl NoteSnapshots {
    pub fn current(&self) -> Option<Arc<NoteSnapshot>> {
        let mut slot = self.0.lock().ok()?;
        let version = multi_launcher::plugins::note::note_version();
        if let Some(snapshot) = slot.as_ref().filter(|snapshot| snapshot.version == version) { return Some(snapshot.clone()); }
        let (version, notes) = multi_launcher::plugins::note::note_cache_snapshot_with_version().ok()?;
        if multi_launcher::plugins::note::note_version() != version { return None; }
        let snapshot = Arc::new(NoteSnapshot { version, by_slug: notes.into_iter().map(|note| (note.slug.clone(), Arc::new(note))).collect() });
        *slot = Some(snapshot.clone()); Some(snapshot)
    }
}

#[derive(Default)]
pub(super) struct QuickNotesBare {
    owner: Arc<std::sync::Mutex<Option<Arc<multi_launcher::plugins::note::NotePlugin>>>>,
    snapshots: Arc<NoteSnapshots>,
    projected: std::sync::Mutex<Option<(String, u64, std::collections::HashMap<String, (Action, Arc<multi_launcher::plugins::note::Note>)>)>>,
}
impl multi_launcher::plugin::Plugin for QuickNotesBare {
    fn name(&self) -> &str { "notes" }
    fn description(&self) -> &str { "Notes (prefix: `note`)" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["note", "notes"] }
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("note") || query.trim().eq_ignore_ascii_case("notes") {
            vec![multi_launcher::plugins::note::quick_notes_action()]
        } else {
            let command = query.trim().split_whitespace().nth(1).unwrap_or("").to_ascii_lowercase();
            if !matches!(command.as_str(), "list" | "open" | "search") { return Vec::new(); }
            self.owner.lock().ok().and_then(|owner| owner.clone()).map_or_else(Vec::new, |plugin| {
                let Some(snapshot) = self.snapshots.current() else { return Vec::new(); };
                let version = snapshot.version;
                let rows: Vec<Action> = plugin.search(query).into_iter().filter(|action| matches!(multi_launcher::commands::parse_action(action), Ok(multi_launcher::commands::Command::Note(multi_launcher::commands::NoteCommand::Open { .. })))).collect();
                if multi_launcher::plugins::note::note_version() != version { return Vec::new(); }
                let mut projected = std::collections::HashMap::new();
                for action in &rows {
                    if let Ok(multi_launcher::commands::Command::Note(multi_launcher::commands::NoteCommand::Open { slug })) = multi_launcher::commands::parse_action(action)
                        && let Some(note) = snapshot.by_slug.get(&slug) {
                        projected.insert(action.action.clone(), (action.clone(), note.clone()));
                    }
                }
                if let Ok(mut slot) = self.projected.lock() { *slot = Some((query.to_owned(), version, projected)); }
                rows
            })
        }
    }
    fn commands(&self) -> Vec<Action> { vec![multi_launcher::plugins::note::quick_notes_command()] }
}

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

struct ClipboardModifyBare {
    catalog: multi_launcher::clipboard_modify::runtime::SharedClipboardModifierCatalog,
    ready: std::sync::atomic::AtomicBool,
    projected: std::sync::Mutex<Option<(String, Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>)>>,
}
impl multi_launcher::plugin::Plugin for ClipboardModifyBare {
    fn name(&self) -> &str { "clipboard_modify" }
    fn description(&self) -> &str { "Builds clipboard modification pipelines with prefix `cm`" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["cm"] }
    fn search(&self, query: &str) -> Vec<Action> {
        if self.ready.load(std::sync::atomic::Ordering::Acquire) {
            if let Ok(catalog) = self.catalog.read() {
                let catalog = catalog.clone();
                let rows = multi_launcher::plugins::clipboard_modify::search_with_catalog(query, &catalog);
                if let Ok(mut projected) = self.projected.lock() { *projected = Some((query.to_owned(), catalog)); }
                return rows;
            }
        }
        multi_launcher::plugins::clipboard_modify::bare_modify_opener(query).or_else(|| multi_launcher::plugins::clipboard_modify::bare_undo_action(query)).into_iter().collect()
    }
    fn commands(&self) -> Vec<Action> { multi_launcher::plugins::clipboard_modify::admitted_modify_commands() }
}

struct TempfileBare;
impl multi_launcher::plugin::Plugin for TempfileBare {
    fn name(&self) -> &str { "tempfile" }
    fn description(&self) -> &str { "Create a temporary file" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["tmp"] }
    fn search(&self, query: &str) -> Vec<Action> {
        if query.trim().eq_ignore_ascii_case("tmp") {
            multi_launcher::plugin::Plugin::search(&multi_launcher::plugins::tempfile::TempfilePlugin, query)
        } else { Vec::new() }
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "tmp".into(), desc: "Tempfile".into(), action: "query:tmp".into(), args: None }]
    }
}

pub(super) fn tempfile_rows(query: &str, enabled: Option<&HashSet<String>>, caps: Option<&std::collections::HashMap<String, Vec<String>>>) -> Vec<Action> {
    search_routed([&TempfileBare as &dyn multi_launcher::plugin::Plugin], query, enabled, caps)
        .into_iter().map(|(_, action)| action).collect()
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

#[derive(Default)]
struct SnippetsBare {
    owner: std::sync::Mutex<Option<Arc<multi_launcher::plugins::snippets::SnippetSearchOwner>>>,
    projected: std::sync::Mutex<Option<(String, multi_launcher::plugins::snippets::SnippetSearchSnapshot)>>,
    plain_aliases: std::sync::Mutex<Option<(Arc<Vec<multi_launcher::plugins::snippets::SnippetEntry>>, HashSet<String>)>>,
}
impl multi_launcher::plugin::Plugin for SnippetsBare {
    fn name(&self) -> &str { "snippets" }
    fn description(&self) -> &str { "Search saved text snippets (prefix: `cs`)" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["cs"] }
    fn search(&self, query: &str) -> Vec<Action> {
        if let Some(action) = multi_launcher::plugins::snippets::bare_dialog_opener(query) { return vec![action]; }
        let Some(owner) = self.owner.lock().ok().and_then(|owner| owner.clone()) else { return Vec::new(); };
        let snapshot = owner.snapshot();
        if !snapshot.healthy { return Vec::new(); }
        let actions = multi_launcher::plugins::snippets::search_with_entries(query, &snapshot.entries);
        let has_run = actions.iter().any(|action| matches!(multi_launcher::commands::parse_action(action), Ok(multi_launcher::commands::Command::Storage(multi_launcher::commands::StorageCommand::SnippetRun(_)))));
        let plain = if !has_run { HashSet::new() } else if let Ok(mut cache) = self.plain_aliases.lock() {
            if cache.as_ref().is_none_or(|(entries, _)| !Arc::ptr_eq(entries, &snapshot.entries)) {
                let aliases = snapshot.entries.iter().filter_map(|entry| {
                    snippet_without_prompt(entry).then(|| entry.alias.clone())
                }).collect();
                *cache = Some((snapshot.entries.clone(), aliases));
            }
            cache.as_ref().map(|(_, aliases)| aliases.clone()).unwrap_or_default()
        } else { HashSet::new() };
        let rows = filter_projected_snippet_actions(actions, &snapshot.entries, Some(&plain));
        if let Ok(mut projected) = self.projected.lock() { *projected = Some((query.to_owned(), snapshot)); }
        rows
    }
    fn commands(&self) -> Vec<Action> {
        vec![Action { label: "cs".into(), desc: "Snippet".into(), action: "query:cs".into(), args: None }]
    }
}

fn snippet_without_prompt(entry: &multi_launcher::plugins::snippets::SnippetEntry) -> bool {
    matches!(multi_launcher::snippet_prompt::prepare(entry),
        Ok(multi_launcher::snippet_prompt::PreparedInvocation::Plain(_)
            | multi_launcher::snippet_prompt::PreparedInvocation::Automatic(_)
            | multi_launcher::snippet_prompt::PreparedInvocation::Prompted(_)))
}

fn projected_snippet_actions(query: &str, entries: &[multi_launcher::plugins::snippets::SnippetEntry], plain: Option<&HashSet<String>>) -> Vec<Action> {
    filter_projected_snippet_actions(multi_launcher::plugins::snippets::search_with_entries(query, entries), entries, plain)
}

fn filter_projected_snippet_actions(actions: Vec<Action>, entries: &[multi_launcher::plugins::snippets::SnippetEntry], plain: Option<&HashSet<String>>) -> Vec<Action> {
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for entry in entries { *counts.entry(&entry.alias).or_default() += 1; }
    actions.into_iter()
        .filter(|action| match multi_launcher::commands::parse_action(action) {
            Ok(multi_launcher::commands::Command::Storage(multi_launcher::commands::StorageCommand::SnippetEdit(alias))) => {
                counts.get(alias.as_str()) == Some(&1)
            }
            Ok(multi_launcher::commands::Command::Storage(multi_launcher::commands::StorageCommand::SnippetRun(alias))) => {
                counts.get(alias.as_str()) == Some(&1) && plain.is_none_or(|plain| plain.contains(&alias))
            }
            _ => false,
        })
        .collect()
}

struct SnippetsProjection<'a>(&'a [multi_launcher::plugins::snippets::SnippetEntry]);
impl multi_launcher::plugin::Plugin for SnippetsProjection<'_> {
    fn name(&self) -> &str { "snippets" }
    fn description(&self) -> &str { "Search saved text snippets (prefix: `cs`)" }
    fn capabilities(&self) -> &[&str] { &["search"] }
    fn query_prefixes(&self) -> &[&str] { &["cs"] }
    fn search(&self, query: &str) -> Vec<Action> {
        projected_snippet_actions(query, self.0, None)
    }
}

pub(super) fn snippets_named_rows(query: &str, entries: &[multi_launcher::plugins::snippets::SnippetEntry], enabled: Option<&HashSet<String>>, caps: Option<&std::collections::HashMap<String, Vec<String>>>) -> Vec<Action> {
    let projection = SnippetsProjection(entries);
    search_routed([&projection as &dyn multi_launcher::plugin::Plugin], query, enabled, caps).into_iter().map(|(_, action)| action).collect()
}

pub(super) fn snippets_rows(
    query: &str,
    enabled: Option<&HashSet<String>>,
    caps: Option<&std::collections::HashMap<String, Vec<String>>>,
) -> Vec<Action> {
    search_routed([&SnippetsBare::default() as &dyn multi_launcher::plugin::Plugin], query, enabled, caps)
        .into_iter().map(|(_, action)| action).collect()
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
    snippets: SnippetsBare,
    tempfile: TempfileBare,
    clipboard_modify: ClipboardModifyBare,
    file_search: FileSearchBarePlugin,
    folder_size: multi_launcher::plugins::folder_size::FolderSizePlugin,
    duplicate_finder: multi_launcher::plugins::duplicate_finder::DuplicateFinderPlugin,
    quick_notes: QuickNotesBare,
    #[cfg(windows)]
    file_lock: multi_launcher::plugins::file_lock::FileLockPlugin,
    calculator: CalculatorMetadata,
    #[cfg(windows)]
    windows: WindowsMetadata,
    date_arithmetic: multi_launcher::plugins::date_arithmetic::DateArithmeticPlugin,
    base_convert: multi_launcher::plugins::base_convert::BaseConvertPlugin,
    unit_convert: multi_launcher::plugins::unit_convert::UnitConvertPlugin,
    #[cfg(windows)]
    downloads_triage: multi_launcher::plugins::downloads_triage::DownloadsTriagePlugin,
    folders: Option<Arc<multi_launcher::plugins::folders::FoldersPlugin>>,
    bookmarks: Option<Arc<multi_launcher::plugins::bookmarks::BookmarksPlugin>>,
    pub(super) omni: super::omni_search::Provider,
    pub(super) omni_wake: Arc<super::HashWakeSignal>,
    pub(super) notes_wake: Arc<super::HashWakeSignal>,
    pub(super) notes_error: Arc<std::sync::Mutex<Option<String>>>,
}

impl Drop for InstalledLauncherProviders {
    fn drop(&mut self) { self.omni_wake.close(); self.notes_wake.close(); }
}

impl InstalledLauncherProviders {
    pub(super) fn note_source(&self, effective: &str, action: &Action) -> Option<(u64, Arc<multi_launcher::plugins::note::Note>)> {
        let projection = self.quick_notes.projected.lock().ok()?;
        let (query, version, rows) = projection.as_ref()?;
        if query != effective || *version != multi_launcher::plugins::note::note_version() { return None; }
        let (expected, note) = rows.get(&action.action)?;
        (expected == action).then(|| (*version, note.clone()))
    }
    pub(super) fn notes_owner(&self) -> Arc<std::sync::Mutex<Option<Arc<multi_launcher::plugins::note::NotePlugin>>>> {
        self.quick_notes.owner.clone()
    }
    pub(super) fn install_snippets_owner(&self, owner: Arc<multi_launcher::plugins::snippets::SnippetSearchOwner>) {
        if let Ok(mut current) = self.snippets.owner.lock() { *current = Some(owner); }
    }
    pub(super) fn snippets_owner(&self) -> Option<Arc<multi_launcher::plugins::snippets::SnippetSearchOwner>> {
        self.snippets.owner.lock().ok()?.clone()
    }
    pub(super) fn take_snippets_projection(&self) -> Option<(String, multi_launcher::plugins::snippets::SnippetSearchSnapshot)> {
        self.snippets.projected.lock().ok()?.take()
    }
    pub(super) fn clipboard_modify_ready(&self) {
        self.clipboard_modify.ready.store(true, std::sync::atomic::Ordering::Release);
    }
    pub(super) fn take_clipboard_modify_projection(&self) -> Option<(String, Arc<multi_launcher::clipboard_modify::model::ClipboardModifierCatalog>)> {
        self.clipboard_modify.projected.lock().ok()?.take()
    }
    pub(super) fn clipboard_modify_catalog(&self) -> multi_launcher::clipboard_modify::runtime::SharedClipboardModifierCatalog {
        self.clipboard_modify.catalog.clone()
    }
    #[cfg(not(test))]
    pub(super) fn new(enabled_plugins: Option<&HashSet<String>>) -> Self {
        let wake = Arc::new(super::HashWakeSignal::default());
        let folder_wake = wake.clone();
        let bookmark_wake = wake.clone();
        Self::with_providers(
            enabled_plugins
                .is_none_or(|enabled| enabled.contains("folders") || enabled.contains("omni_search"))
                .then(|| multi_launcher::plugins::folders::FoldersPlugin::new_with_wake(Arc::new(move || folder_wake.notify()))),
            enabled_plugins
                .is_none_or(|enabled| enabled.contains("bookmarks") || enabled.contains("omni_search"))
                .then(|| multi_launcher::plugins::bookmarks::BookmarksPlugin::new_with_wake(Arc::new(move || bookmark_wake.notify()))),
            enabled_plugins.is_none_or(|enabled| enabled.contains("favorites")),
            wake,
        )
    }

    fn with_providers(
        folders: Option<multi_launcher::plugins::folders::FoldersPlugin>,
        bookmarks: Option<multi_launcher::plugins::bookmarks::BookmarksPlugin>,
        watch_favorites: bool,
        omni_wake: Arc<super::HashWakeSignal>,
    ) -> Self {
        let folders = folders.map(Arc::new);
        let bookmarks = bookmarks.map(Arc::new);
        let quick_notes = QuickNotesBare::default();
        let omni = super::omni_search::Provider::new(folders.clone(), bookmarks.clone(), quick_notes.owner.clone(), quick_notes.snapshots.clone());
        Self {
            omni,
            omni_wake,
            notes_wake: Arc::new(super::HashWakeSignal::default()),
            notes_error: Arc::new(std::sync::Mutex::new(None)),
            hash_generator: multi_launcher::plugins::hash_generator::HashGeneratorPlugin,
            settings: multi_launcher::plugins::settings::SettingsPlugin,
            help: multi_launcher::plugins::help::HelpPlugin,
            favorites: super::favorite_search::Provider::new(watch_favorites),
            activity: multi_launcher::plugins::activity::ActivityPlugin,
            json_utility: multi_launcher::plugins::json_utility::JsonUtilityPlugin,
            xml_toolkit: multi_launcher::plugins::xml_toolkit::XmlToolkitPlugin,
            regex_tester: multi_launcher::plugins::regex_tester::RegexTesterPlugin,
            clipboard: ClipboardBare,
            snippets: SnippetsBare::default(),
            tempfile: TempfileBare,
            clipboard_modify: ClipboardModifyBare {
                catalog: multi_launcher::clipboard_modify::runtime::shared_default_catalog(),
                ready: std::sync::atomic::AtomicBool::new(false),
                projected: std::sync::Mutex::new(None),
            },
            file_search: FileSearchBarePlugin(
                multi_launcher::plugins::file_search::FileSearchPlugin::default(),
            ),
            folder_size: multi_launcher::plugins::folder_size::FolderSizePlugin,
            duplicate_finder: multi_launcher::plugins::duplicate_finder::DuplicateFinderPlugin,
            quick_notes,
            #[cfg(windows)]
            file_lock: multi_launcher::plugins::file_lock::FileLockPlugin,
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
        Self::with_providers(None, None, false, Arc::new(super::HashWakeSignal::default()))
    }

    pub(super) fn plugins<'a>(
        &'a self,
    ) -> impl Iterator<Item = &'a dyn multi_launcher::plugin::Plugin> {
        [
            Some(&self.hash_generator as &dyn multi_launcher::plugin::Plugin),
            Some(&self.omni as &dyn multi_launcher::plugin::Plugin),
            Some(&self.settings as &dyn multi_launcher::plugin::Plugin),
            Some(&self.help as &dyn multi_launcher::plugin::Plugin),
            Some(&self.favorites as &dyn multi_launcher::plugin::Plugin),
            Some(&self.activity as &dyn multi_launcher::plugin::Plugin),
            Some(&self.json_utility as &dyn multi_launcher::plugin::Plugin),
            Some(&self.xml_toolkit as &dyn multi_launcher::plugin::Plugin),
            Some(&self.regex_tester as &dyn multi_launcher::plugin::Plugin),
            Some(&self.clipboard as &dyn multi_launcher::plugin::Plugin),
            Some(&self.snippets as &dyn multi_launcher::plugin::Plugin),
            Some(&self.tempfile as &dyn multi_launcher::plugin::Plugin),
            Some(&self.clipboard_modify as &dyn multi_launcher::plugin::Plugin),
            Some(&self.file_search as &dyn multi_launcher::plugin::Plugin),
            Some(&self.folder_size as &dyn multi_launcher::plugin::Plugin),
            Some(&self.duplicate_finder as &dyn multi_launcher::plugin::Plugin),
            Some(&self.quick_notes as &dyn multi_launcher::plugin::Plugin),
            #[cfg(windows)]
            Some(&self.file_lock as &dyn multi_launcher::plugin::Plugin),
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
                .map(|p| p.as_ref() as &dyn multi_launcher::plugin::Plugin),
            self.bookmarks
                .as_ref()
                .map(|p| p.as_ref() as &dyn multi_launcher::plugin::Plugin),
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

    pub(super) fn bookmark_aliases(&self) -> Option<std::collections::HashMap<String, Option<String>>> {
        let bookmarks = self.bookmarks.as_ref()?;
        let rows = multi_launcher::plugin::Plugin::search(bookmarks.as_ref(), "bm list");
        let targets = rows.iter().map(|row| row.action.as_str()).collect::<HashSet<_>>();
        Some(bookmarks.search_aliases_lc(&targets))
    }
    pub(super) fn bookmark_row_is_current(&self, query: &str, action: &Action, enabled: Option<&HashSet<String>>,
        caps: Option<&std::collections::HashMap<String, Vec<String>>>) -> bool {
        self.bookmarks.as_ref().is_some_and(|provider| search_routed([provider.as_ref() as &dyn multi_launcher::plugin::Plugin], query, enabled, caps)
            .iter().any(|(name, candidate)| name == "bookmarks" && candidate == action))
    }

    pub(super) fn folder_aliases(&self) -> Option<std::collections::HashMap<String, Option<String>>> {
        let folder = self.folders.as_ref()?;
        let rows = multi_launcher::plugin::Plugin::search(folder.as_ref(), "f list");
        let targets = rows.iter().map(|row| row.action.as_str()).collect::<HashSet<_>>();
        Some(folder.search_aliases_lc(&targets))
    }

    pub(super) fn folder_row_is_current(&self, query: &str, action: &Action, enabled: Option<&HashSet<String>>,
        caps: Option<&std::collections::HashMap<String, Vec<String>>>) -> bool {
        self.folders.as_ref().is_some_and(|provider| search_routed([provider.as_ref() as &dyn multi_launcher::plugin::Plugin], query, enabled, caps)
            .iter().any(|(name, candidate)| name == "folders" && candidate == action))
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
            "folders" => matches!(action.action.as_str(), "query:f " | "query:f list " | "query:f add " | "query:f rm "),
            "bookmarks" => matches!(action.action.as_str(), "query:bm " | "query:bm list" | "query:bm add " | "query:bm rm "),
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn note_provider_projects_body_search_to_exact_stock_open() {
        use multi_launcher::plugin::Plugin;
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("ML_NOTES_DIR");
        let old_templates = std::env::var_os("ML_NOTE_TEMPLATES_DIR");
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()); std::env::set_var("ML_NOTE_TEMPLATES_DIR", dir.path().join("templates")); }
        std::fs::write(dir.path().join("alpha.md"), "# Alpha\n\nunique body needle").unwrap();
        let provider = super::QuickNotesBare::default();
        *provider.owner.lock().unwrap() = Some(std::sync::Arc::new(multi_launcher::plugins::note::NotePlugin::new()));
        let rows = provider.search("note search needle");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "note:open:alpha");
        assert!(provider.search("note new alpha").is_empty());
        let stored = provider.projected.lock().unwrap();
        let (_, version, projection) = stored.as_ref().unwrap();
        assert_eq!(*version, multi_launcher::plugins::note::note_version());
        assert_eq!(projection.get("note:open:alpha").unwrap().1.path, dir.path().join("alpha.md"));
        drop(stored);
        let omni = super::super::omni_search::Provider::new(None, None, provider.owner.clone(), provider.snapshots.clone());
        let app = multi_launcher::actions::Action { label: "Alpha App".into(), desc: "App".into(), action: "alpha.exe".into(), args: None };
        let catalog = multi_launcher::plugins::omni_search::OmniAppCatalog::build(std::sync::Arc::new(vec![app.clone()]), 7, || true).unwrap();
        omni.install(Some(std::sync::Arc::new(catalog)));
        let enabled = HashSet::from(["omni_search".to_owned()]);
        let routed = super::search_routed([&omni as &dyn Plugin, &provider as &dyn Plugin], "o", Some(&enabled), None);
        assert!(routed.iter().all(|(name, _)| name == "omni_search"));
        let note_action = routed.iter().find(|(_, action)| action.action == "note:open:alpha").unwrap().1.clone();
        let projection = omni.projection("o").unwrap();
        assert!(projection.note_for(&note_action).is_some());
        assert!(projection.source_for(&app).is_some());
        std::fs::write(dir.path().join("alpha.md"), "# Alpha\n\nchanged").unwrap();
        multi_launcher::plugins::note::refresh_cache().unwrap();
        assert!(projection.source_for(&note_action).is_none());
        assert!(projection.source_for(&app).is_some());
        omni.configure(Some(&serde_json::json!({"include_notes": false})));
        assert!(omni.search("o").iter().all(|action| action.action != "note:open:alpha"));
        drop(omni); drop(provider);
        if let Some(old) = old { unsafe { std::env::set_var("ML_NOTES_DIR", old); } }
        else { unsafe { std::env::remove_var("ML_NOTES_DIR"); } }
        if let Some(old) = old_templates { unsafe { std::env::set_var("ML_NOTE_TEMPLATES_DIR", old); } }
        else { unsafe { std::env::remove_var("ML_NOTE_TEMPLATES_DIR"); } }
    }

    use super::admitted_commands;
    use multi_launcher::{
        actions::Action,
        plugin::{Plugin, search_filtered_plugins_with_origin},
    };
    use std::collections::{HashMap, HashSet};

    #[test]
    fn tempfile_bare_provider_admits_only_enabled_dialog() {
        let enabled = HashSet::from(["tempfile".to_owned()]);
        let rows = super::tempfile_rows("TmP", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, "tempfile:dialog");
        assert!(super::tempfile_rows("tmp new secret", Some(&enabled), None).is_empty());
        assert!(super::tempfile_rows("tmp", Some(&HashSet::new()), None).is_empty());
    }

    #[test]
    fn snippets_bare_provider_admits_only_stock_dialog_with_search_capability() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["snippets".to_owned()]);
        for query in ["cs", " CS "] {
            let rows = super::search_routed(providers.plugins(), query, Some(&enabled), None);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, "snippets");
            assert_eq!(rows[0].1, multi_launcher::plugins::snippets::bare_dialog_opener(query).unwrap());
        }
        for query in ["cs edit sample", "cs add sample text", "csfoo"] {
            assert!(super::search_routed(providers.plugins(), query, Some(&enabled), None).is_empty());
        }
        let no_search = HashMap::from([("snippets".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers.plugins(), "cs", Some(&enabled), Some(&no_search)).is_empty());
    }

    #[test]
    fn snippets_named_projection_keeps_stock_filters_and_rejects_mutating_overload() {
        use multi_launcher::plugins::snippets::SnippetEntry;
        let rows = vec![SnippetEntry { alias: "alpha".into(), text: "text".into(), hide_contents: false,
            prompt_for_fields: false, fields: Vec::new(), tags: vec!["work".into()] }];
        let enabled = HashSet::from(["snippets".to_owned()]);
        let projected = super::snippets_named_rows("cs edit tag:work !tag:private -id:unrelated", &rows, Some(&enabled), None);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].action, "snippet:edit:alpha");
        assert!(super::snippets_named_rows("cs edit alpha replacement", &rows, Some(&enabled), None).is_empty());
        assert!(super::snippets_named_rows("cs edit alpha", &rows, Some(&HashSet::new()), None).is_empty());
    }

    #[test]
    fn snippets_named_projection_excludes_duplicates_even_if_only_one_matches_filter() {
        use multi_launcher::plugins::snippets::SnippetEntry;
        let rows = vec![
            SnippetEntry { alias: "same".into(), text: "first".into(), hide_contents: false, prompt_for_fields: false, fields: Vec::new(), tags: vec!["work".into()] },
            SnippetEntry { alias: "same".into(), text: "second".into(), hide_contents: false, prompt_for_fields: false, fields: Vec::new(), tags: vec!["other".into()] },
        ];
        assert!(super::snippets_named_rows("cs edit tag:work", &rows, None, None).is_empty());
    }
    #[test]
    fn snippet_run_admission_includes_all_supported_modes() {
        use multi_launcher::plugins::snippets::SnippetEntry;
        let entry = |text: &str, prompted| SnippetEntry {
            alias: "a".into(), text: text.into(), hide_contents: false,
            prompt_for_fields: prompted, fields: Vec::new(), tags: Vec::new(),
        };
        assert!(super::snippet_without_prompt(&entry("plain", false)));
        assert!(super::snippet_without_prompt(&entry("{{sys.clipboard}}", false)));
        assert!(super::snippet_without_prompt(&entry("{{name}}", true)));
    }

    #[test]
    fn snippets_plain_projection_admits_only_unique_safe_run_actions() {
        use multi_launcher::plugins::snippets::SnippetEntry;
        let rows = vec![SnippetEntry { alias: "alpha".into(), text: "text".into(), hide_contents: false,
            prompt_for_fields: false, fields: Vec::new(), tags: vec!["work".into()] }];
        let allowed = HashSet::from(["alpha".to_owned()]);
        let projected = super::projected_snippet_actions("cs list tag:work", &rows, Some(&allowed));
        assert_eq!(projected.len(), 1);
        assert!(matches!(multi_launcher::commands::parse_action(&projected[0]), Ok(multi_launcher::commands::Command::Storage(multi_launcher::commands::StorageCommand::SnippetRun(_)))));
        assert!(super::projected_snippet_actions("cs list tag:work", &rows, Some(&HashSet::new())).is_empty());
        assert!(super::projected_snippet_actions("cs edit alpha replacement", &rows, Some(&allowed)).is_empty());
    }

    #[test]
    fn clipboard_catalog_receipt_rejects_same_id_with_changed_body() {
        use std::sync::{Arc, RwLock};
        use multi_launcher::clipboard_modify::model::{ClipboardModifierCatalog, ClipboardTemplate};
        let old = Arc::new(ClipboardModifierCatalog::new(vec![ClipboardTemplate {
            id: "same-id".into(), label: "Same".into(), aliases: vec![],
            template: "before {{clipboard}}".into(), processor: None,
        }], vec![]).unwrap());
        let shared = Arc::new(RwLock::new(old.clone()));
        assert!(super::clipboard_catalog_is_current(&shared, &old));
        let mut replacement = (*old).clone();
        replacement.templates[0].template = "after {{clipboard}}".into();
        *shared.write().unwrap() = Arc::new(replacement);
        assert!(!super::clipboard_catalog_is_current(&shared, &old));
    }

    #[test]
    fn clipboard_provider_projects_cold_custom_catalog_and_effective_filtered_query() {
        use std::sync::{Arc, Mutex, RwLock, atomic::AtomicBool};
        use multi_launcher::clipboard_modify::model::{ClipboardModifierCatalog, ClipboardTemplate};
        let catalog = Arc::new(ClipboardModifierCatalog::new(vec![ClipboardTemplate {
            id: "custom-owned".into(), label: "Custom owned".into(), aliases: vec![],
            template: "owned {{clipboard}}".into(), processor: None,
        }], vec![]).unwrap());
        let provider = super::ClipboardModifyBare {
            catalog: Arc::new(RwLock::new(catalog.clone())), ready: AtomicBool::new(false), projected: Mutex::new(None),
        };
        assert!(provider.search("cm template custom-owned").is_empty());
        provider.ready.store(true, std::sync::atomic::Ordering::Release);
        let enabled = HashSet::from(["clipboard_modify".to_owned()]);
        let rows = super::search_routed([&provider as &dyn Plugin], "cm uppercase -id:unrelated", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1.action, "clipboard_modify:execute");
        let (effective, used) = provider.projected.lock().unwrap().take().unwrap();
        assert_eq!(effective.trim(), "cm uppercase");
        assert!(Arc::ptr_eq(&used, &catalog));
        assert_eq!(provider.search("cm template custom-owned")[0].action, "clipboard_modify:execute");
    }

    #[test]
    fn installed_folder_size_provider_obeys_exact_query_and_capabilities() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["folder_size".to_owned()]);
        let rows = super::search_routed(providers.plugins(), "FolderSize", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "folder_size");
        let filtered = super::search_routed(providers.plugins(), "kind:foldersize foldersize", Some(&enabled), None);
        assert_eq!(filtered.len(), 1);
        assert!(super::super::folder_size::current_provider("kind:foldersize foldersize", &filtered[0].1, Some(&enabled), None));
        assert!(super::super::folder_size::current_provider("FolderSize", &rows[0].1, Some(&enabled), None));
        assert!(super::search_routed(providers.plugins(), "foldersize extra", Some(&enabled), None).is_empty());
        assert!(super::search_routed(providers.plugins(), "foldersize", Some(&HashSet::new()), None).is_empty());
        let disabled = HashMap::from([("folder_size".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers.plugins(), "foldersize", Some(&enabled), Some(&disabled)).is_empty());
    }

    #[test]
    fn installed_duplicate_finder_provider_obeys_exact_query_and_capabilities() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["duplicate_finder".to_owned()]);
        let rows = super::search_routed(providers.plugins(), "DFF", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "duplicate_finder");
        assert!(super::super::duplicate_finder::current_provider("DFF", &rows[0].1, Some(&enabled), None));
        let filtered = super::search_routed(providers.plugins(), "kind:dff dff", Some(&enabled), None);
        assert_eq!(filtered.len(), 1);
        assert!(super::super::duplicate_finder::current_provider("kind:dff dff", &filtered[0].1, Some(&enabled), None));
        assert!(super::search_routed(providers.plugins(), "dff extra", Some(&enabled), None).is_empty());
        assert!(super::search_routed(providers.plugins(), "dff", Some(&HashSet::new()), None).is_empty());
        let disabled = HashMap::from([("duplicate_finder".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers.plugins(), "dff", Some(&enabled), Some(&disabled)).is_empty());
        assert_eq!(providers.plugins().find(|plugin| plugin.name() == "duplicate_finder").unwrap().commands()[0].action, "query:dff");
    }

    #[cfg(windows)]
    #[test]
    fn installed_file_lock_provider_obeys_exact_query_and_capabilities() {
        let providers = super::InstalledLauncherProviders::test_without_persisted_providers();
        let enabled = HashSet::from(["file_lock".to_owned()]);
        let rows = super::search_routed(providers.plugins(), "FileLock", Some(&enabled), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "file_lock");
        assert!(super::super::file_lock::current_provider("FileLock", &rows[0].1, Some(&enabled), None));
        let filtered = super::search_routed(providers.plugins(), "kind:filelock filelock", Some(&enabled), None);
        assert_eq!(filtered.len(), 1);
        assert!(super::super::file_lock::current_provider("kind:filelock filelock", &filtered[0].1, Some(&enabled), None));
        assert!(super::search_routed(providers.plugins(), "filelock extra", Some(&enabled), None).is_empty());
        assert!(super::search_routed(providers.plugins(), "filelock", Some(&HashSet::new()), None).is_empty());
        let disabled = HashMap::from([("file_lock".to_owned(), Vec::new())]);
        assert!(super::search_routed(providers.plugins(), "filelock", Some(&enabled), Some(&disabled)).is_empty());
    }

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
            vec!["f", "f list", "f add", "f rm"]
        );
        assert_eq!(
            admitted_commands(&bookmark)
                .iter()
                .map(|a| a.label.as_str())
                .collect::<Vec<_>>(),
            vec!["bm", "bm list", "bm add"]
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
