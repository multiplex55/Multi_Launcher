use crate::actions::Action;
use crate::clipboard_modify::store::{SharedClipboardModifierCatalog, shared_default_catalog};
use crate::common::query::{apply_action_filters, split_action_filters};
use crate::plugins::asciiart::AsciiArtPlugin;
use crate::plugins::base_convert::BaseConvertPlugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::brightness::BrightnessPlugin;
use crate::plugins::browser_tabs::BrowserTabsPlugin;
use crate::plugins::calendar::CalendarPlugin;
use crate::plugins::clipboard::ClipboardPlugin;
use crate::plugins::clipboard_modify::ClipboardModifyPlugin;
use crate::plugins::color_picker::ColorPickerPlugin;
use crate::plugins::convert_panel::ConvertPanelPlugin;
use crate::plugins::crop::CropPlugin;
use crate::plugins::data::DataPlugin;
use crate::plugins::diff::DiffPlugin;
use crate::plugins::dropcalc::DropCalcPlugin;
use crate::plugins::emoji::EmojiPlugin;
use crate::plugins::fav::FavPlugin;
use crate::plugins::file_search::FileSearchPlugin;
use crate::plugins::folders::FoldersPlugin;
use crate::plugins::help::HelpPlugin;
use crate::plugins::history::HistoryPlugin;
use crate::plugins::ip::IpPlugin;
use crate::plugins::keys::KeysPlugin;
use crate::plugins::layout::LayoutPlugin;
use crate::plugins::link::LinkPlugin;
use crate::plugins::lorem::LoremPlugin;
use crate::plugins::macros::MacrosPlugin;
use crate::plugins::media::MediaPlugin;
use crate::plugins::missing::MissingPlugin;
use crate::plugins::mkmacro::MkMacroPlugin;
use crate::plugins::mouse_gestures::MouseGesturesPlugin;
use crate::plugins::multi_manager::MultiManagerPlugin;
use crate::plugins::network::NetworkPlugin;
use crate::plugins::note::NotePlugin;
use crate::plugins::omni_search::OmniSearchPlugin;
use crate::plugins::processes::ProcessesPlugin;
use crate::plugins::random::RandomPlugin;
use crate::plugins::recycle::RecyclePlugin;
use crate::plugins::reddit::RedditPlugin;
use crate::plugins::runescape::RunescapeSearchPlugin;
use crate::plugins::screenshot::ScreenshotPlugin;
use crate::plugins::settings::SettingsPlugin;
use crate::plugins::shell::ShellPlugin;
use crate::plugins::snippets::SnippetsPlugin;
use crate::plugins::stopwatch::StopwatchPlugin;
use crate::plugins::sysinfo::SysInfoPlugin;
use crate::plugins::system::SystemPlugin;
use crate::plugins::system_data::SystemDataRuntime;
use crate::plugins::task_manager::TaskManagerPlugin;
use crate::plugins::tempfile::TempfilePlugin;
use crate::plugins::text_case::TextCasePlugin;
use crate::plugins::timer::TimerPlugin;
use crate::plugins::timestamp::TimestampPlugin;
use crate::plugins::todo::TodoPlugin;
use crate::plugins::unit_convert::UnitConvertPlugin;
use crate::plugins::virtual_desktop::VirtualDesktopPlugin;
use crate::plugins::volume::VolumePlugin;
use crate::plugins::weather::WeatherPlugin;
use crate::plugins::wikipedia::WikipediaPlugin;
use crate::plugins::windows::WindowsPlugin;
use crate::plugins::youtube::YoutubePlugin;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::settings::NetUnit;
use crate::window_catalog::WindowCatalog;
use eframe::egui;
use libloading::Library;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};

pub const CAP_GRID_RESULTS_COMPATIBLE: &str = "grid_results_compatible";
pub const CAP_FORCE_LIST_RESULTS: &str = "force_list_results";

pub trait Plugin: Send + Sync {
    /// Return actions based on the query string
    fn search(&self, query: &str) -> Vec<Action>;
    /// Name of the plugin
    fn name(&self) -> &str;
    /// Human readable description of the plugin
    fn description(&self) -> &str;
    /// Capabilities offered by the plugin
    fn capabilities(&self) -> &[&str];
    /// Query shortcuts offered by the plugin
    fn commands(&self) -> Vec<Action> {
        Vec::new()
    }

    /// Optional query head prefixes that should route to this plugin.
    ///
    /// Prefix matching is case-insensitive and uses the first token of the
    /// query. Plugins that return an empty slice are considered global and run
    /// for all queries.
    fn query_prefixes(&self) -> &[&str] {
        &[]
    }

    /// Opt-out of prefix routing and always run this plugin for searches.
    fn always_search(&self) -> bool {
        false
    }

    /// Return default settings for this plugin if any.
    fn default_settings(&self) -> Option<serde_json::Value> {
        None
    }

    /// Update the plugin using the provided settings value.
    fn apply_settings(&mut self, _value: &serde_json::Value) {}

    /// Draw the settings UI for this plugin.
    fn settings_ui(&mut self, _ui: &mut egui::Ui, _value: &mut serde_json::Value) {}
}

/// A manager that holds plugins
#[derive(Default)]
pub(crate) struct PluginSearchUpdates {
    generation: AtomicU64,
    tickets: Mutex<RefreshTicketBook>,
    source_generations: Mutex<HashMap<&'static str, u64>>,
    repaint: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

#[derive(Default)]
struct RefreshTicketBook {
    next: u64,
    sources: HashMap<&'static str, RefreshTicketSource>,
}

#[derive(Default)]
struct RefreshTicketSource {
    active: Option<u64>,
    resolved_through: u64,
    published: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RefreshTicket {
    pub(crate) id: u64,
    pub(crate) start: bool,
}

thread_local! {
    static SEARCH_REFRESH_TICKETS: RefCell<Vec<(&'static str, u64)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn record_search_refresh_ticket(source: &'static str, ticket: u64) {
    SEARCH_REFRESH_TICKETS.with(|tickets| tickets.borrow_mut().push((source, ticket)));
}

pub(crate) fn capture_search_refresh_tickets<T>(
    work: impl FnOnce() -> T,
) -> (T, Vec<(&'static str, u64)>) {
    SEARCH_REFRESH_TICKETS.with(|tickets| tickets.borrow_mut().clear());
    let value = work();
    let tickets = SEARCH_REFRESH_TICKETS.with(|tickets| std::mem::take(&mut *tickets.borrow_mut()));
    (value, tickets)
}

impl PluginSearchUpdates {
    pub(crate) fn schedule_or_join(&self, source: &'static str) -> RefreshTicket {
        let mut book = self
            .tickets
            .lock()
            .expect("refresh ticket registry poisoned");
        if let Some(id) = book.sources.get(source).and_then(|state| state.active) {
            return RefreshTicket { id, start: false };
        }
        book.next = book.next.wrapping_add(1).max(1);
        let id = book.next;
        book.sources.entry(source).or_default().active = Some(id);
        RefreshTicket { id, start: true }
    }

    pub(crate) fn active_ticket(&self, source: &str) -> Option<u64> {
        let source = Self::canonical_source(source);
        self.tickets.lock().ok()?.sources.get(source)?.active
    }

    pub(crate) fn published_ticket(&self, source: &str) -> Option<u64> {
        let source = Self::canonical_source(source);
        self.tickets.lock().ok()?.sources.get(source)?.published
    }

    pub(crate) fn ticket_resolved(&self, source: &str, ticket: u64) -> bool {
        let source = Self::canonical_source(source);
        self.tickets
            .lock()
            .ok()
            .and_then(|book| {
                book.sources
                    .get(source)
                    .map(|state| state.resolved_through >= ticket)
            })
            .unwrap_or(false)
    }

    pub(crate) fn publish_ticket(&self, source: &'static str, ticket: u64) {
        let (committed, _) = self.publish_ticket_with_followup(source, ticket, false);
        if !committed {
            return;
        }
        self.notify(source);
    }

    /// Resolve a publication and, when requested, reserve its successor under the same ticket
    /// lock. The caller can make its local worker state match the returned ticket before invoking
    /// `notify`, so repaint callbacks never observe an idle/ticket handoff gap.
    pub(crate) fn publish_ticket_with_followup(
        &self,
        source: &'static str,
        ticket: u64,
        followup: bool,
    ) -> (bool, Option<RefreshTicket>) {
        let Ok(mut book) = self.tickets.lock() else {
            return (false, None);
        };
        {
            let state = book.sources.entry(source).or_default();
            if state.active != Some(ticket) {
                return (false, None);
            }
            state.active = None;
            state.resolved_through = state.resolved_through.max(ticket);
            state.published = Some(ticket);
        }
        let next = followup.then(|| {
            book.next = book.next.wrapping_add(1).max(1);
            let id = book.next;
            book.sources.entry(source).or_default().active = Some(id);
            RefreshTicket { id, start: true }
        });
        (true, next)
    }

    pub(crate) fn cancel_ticket(&self, source: &'static str, ticket: u64) {
        let cancelled = self.tickets.lock().ok().is_some_and(|mut book| {
            let state = book.sources.entry(source).or_default();
            if state.active != Some(ticket) {
                return false;
            }
            state.active = None;
            state.resolved_through = state.resolved_through.max(ticket);
            true
        });
        if cancelled {
            self.notify(source);
        }
    }

    fn canonical_source(source: &str) -> &str {
        match source {
            "processes" | "sysinfo" | "volume" => "system_data",
            "virtual_desktop" => "windows",
            source => source,
        }
    }

    pub(crate) fn notify(&self, source: &'static str) {
        self.generation.fetch_add(1, Ordering::Release);
        if let Ok(mut generations) = self.source_generations.lock() {
            *generations.entry(source).or_default() += 1;
        }
        let callback = self
            .repaint
            .lock()
            .ok()
            .and_then(|callback| callback.as_ref().map(Arc::clone));
        if let Some(callback) = callback {
            callback();
        }
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn source_generation(&self, source: &str) -> u64 {
        let source = Self::canonical_source(source);
        self.source_generations
            .lock()
            .ok()
            .and_then(|generations| generations.get(source).copied())
            .unwrap_or(0)
    }

    pub(crate) fn set_repaint_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut slot) = self.repaint.lock() {
            *slot = Some(callback);
        }
    }
}

pub struct PluginInternalServices {
    pub clipboard_modifier_catalog: SharedClipboardModifierCatalog,
    pub mkmacro_store: Arc<crate::mkmacro::MkMacroStore>,
    pub window_catalog: Arc<WindowCatalog>,
    pub workspace_catalog: Arc<crate::multi_manager::workspace_catalog::WorkspaceCatalog>,
    search_updates: Arc<PluginSearchUpdates>,
    system_data_runtime: Option<SystemDataRuntime>,
}

pub struct PluginManager {
    plugins: Vec<Arc<PluginSlot>>,
    services: PluginInternalServices,
    next_plugin_epoch: u64,
    deferred_dynamic_reloads: Vec<(PathBuf, Weak<PluginSlot>)>,
}

struct PluginSlot {
    name: String,
    plugin: RwLock<Box<dyn Plugin>>,
    _library: Option<Arc<libloading::Library>>,
    library_path: Option<PathBuf>,
    epoch: u64,
}

#[derive(Clone)]
pub(crate) struct OwnedPluginHandle {
    pub(crate) epoch: u64,
    slot: Arc<PluginSlot>,
}

impl OwnedPluginHandle {
    #[cfg(test)]
    pub(crate) fn for_test(plugin: Box<dyn Plugin>) -> Self {
        Self::for_test_epoch(plugin, 1)
    }

    #[cfg(test)]
    pub(crate) fn for_test_epoch(plugin: Box<dyn Plugin>, epoch: u64) -> Self {
        let slot = Arc::new(PluginSlot {
            name: plugin.name().to_string(),
            plugin: RwLock::new(plugin),
            _library: None,
            library_path: None,
            epoch,
        });
        Self { epoch, slot }
    }

    pub(crate) fn read(&self) -> std::sync::LockResult<RwLockReadGuard<'_, Box<dyn Plugin>>> {
        self.slot.plugin.read()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        let store = Arc::new(
            crate::mkmacro::MkMacroStore::open(".")
                .expect("open mkmacro store")
                .0,
        );
        crate::mkmacro::runtime::set_shared_store(Arc::clone(&store));
        let search_updates = Arc::new(PluginSearchUpdates::default());
        let window_catalog = WindowCatalog::production(Arc::clone(&search_updates));
        Self {
            plugins: Vec::new(),
            services: PluginInternalServices {
                clipboard_modifier_catalog: shared_default_catalog(),
                mkmacro_store: store,
                search_updates,
                window_catalog,
                workspace_catalog: Arc::new(
                    crate::multi_manager::workspace_catalog::WorkspaceCatalog::default(),
                ),
                system_data_runtime: None,
            },
            next_plugin_epoch: 0,
            deferred_dynamic_reloads: Vec::new(),
        }
    }

    /// Creates the manager while reserving chords owned by the launcher.
    pub fn new_with_reserved_hotkeys(reserved: &[(&str, &str)]) -> Self {
        let store = Arc::new(
            crate::mkmacro::MkMacroStore::open(".")
                .expect("open mkmacro store")
                .0,
        );
        crate::mkmacro::runtime::set_shared_store_with_reserved(Arc::clone(&store), reserved);
        let search_updates = Arc::new(PluginSearchUpdates::default());
        let window_catalog = WindowCatalog::production(Arc::clone(&search_updates));
        Self {
            plugins: Vec::new(),
            services: PluginInternalServices {
                clipboard_modifier_catalog: shared_default_catalog(),
                mkmacro_store: store,
                search_updates,
                window_catalog,
                workspace_catalog: Arc::new(
                    crate::multi_manager::workspace_catalog::WorkspaceCatalog::default(),
                ),
                system_data_runtime: None,
            },
            next_plugin_epoch: 0,
            deferred_dynamic_reloads: Vec::new(),
        }
    }

    pub fn internal_services(&self) -> &PluginInternalServices {
        &self.services
    }

    pub fn search_generation(&self) -> u64 {
        self.services.search_updates.generation()
    }

    pub fn search_generation_for(&self, source: &str) -> u64 {
        self.services.search_updates.source_generation(source)
    }

    pub(crate) fn active_search_ticket_for(&self, source: &str) -> Option<u64> {
        self.services.search_updates.active_ticket(source)
    }

    pub(crate) fn search_ticket_resolved(&self, source: &str, ticket: u64) -> bool {
        self.services.search_updates.ticket_resolved(source, ticket)
    }

    pub(crate) fn search_plugin_with_ticket(
        &self,
        name: &str,
        query: &str,
    ) -> Result<(Vec<Action>, Option<u64>), &'static str> {
        let slot = self
            .plugins
            .iter()
            .find(|slot| slot.name == name)
            .ok_or("plugin unavailable")?;
        let plugin = slot.plugin.try_read().map_err(|_| "plugin busy")?;
        let (actions, tickets) = capture_search_refresh_tickets(|| plugin.search(query));
        Ok((
            actions,
            tickets.into_iter().find_map(|(source, ticket)| {
                (PluginSearchUpdates::canonical_source(name) == source).then_some(ticket)
            }),
        ))
    }

    pub(crate) fn search_filtered_with_tickets(
        &self,
        query: &str,
        enabled_plugins: Option<&HashSet<String>>,
        enabled_caps: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> (Vec<Action>, Vec<(&'static str, u64)>) {
        capture_search_refresh_tickets(|| {
            self.search_filtered(query, enabled_plugins, enabled_caps)
        })
    }

    pub fn set_search_repaint_callback(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.services.search_updates.set_repaint_callback(callback);
    }

    pub fn clipboard_modifier_catalog(&self) -> SharedClipboardModifierCatalog {
        Arc::clone(&self.services.clipboard_modifier_catalog)
    }

    pub fn replace_clipboard_modifier_catalog_snapshot(
        &self,
        catalog: crate::clipboard_modify::model::ClipboardModifierCatalog,
    ) {
        *self.services.clipboard_modifier_catalog.write().unwrap() = Arc::new(catalog);
    }

    pub fn with_clipboard_modifier_catalog(catalog: SharedClipboardModifierCatalog) -> Self {
        let store = Arc::new(
            crate::mkmacro::MkMacroStore::open(".")
                .expect("open mkmacro store")
                .0,
        );
        crate::mkmacro::runtime::set_shared_store(Arc::clone(&store));
        let search_updates = Arc::new(PluginSearchUpdates::default());
        let window_catalog = WindowCatalog::production(Arc::clone(&search_updates));
        Self {
            plugins: Vec::new(),
            services: PluginInternalServices {
                clipboard_modifier_catalog: catalog,
                mkmacro_store: store,
                search_updates,
                window_catalog,
                workspace_catalog: Arc::new(
                    crate::multi_manager::workspace_catalog::WorkspaceCatalog::default(),
                ),
                system_data_runtime: None,
            },
            next_plugin_epoch: 0,
            deferred_dynamic_reloads: Vec::new(),
        }
    }

    /// Remove all registered plugins, deferring reload of a dynamic library while an owned plugin
    /// handle still pins its originating slot.
    pub fn clear_plugins(&mut self) {
        for slot in &self.plugins {
            if Arc::strong_count(slot) > 1
                && let Some(path) = slot.library_path.clone()
                && !self
                    .deferred_dynamic_reloads
                    .iter()
                    .any(|(existing, weak)| existing == &path && weak.strong_count() > 0)
            {
                self.deferred_dynamic_reloads
                    .push((path, Arc::downgrade(slot)));
            }
        }
        self.plugins.clear();
    }

    /// Rebuild the plugin list, deferring dynamic paths whose prior plugin slot is still pinned.
    ///
    /// `actions` is the shared list of launcher actions. An [`Arc`] is used so
    /// plugins such as [`OmniSearchPlugin`](crate::plugins::omni_search::OmniSearchPlugin)
    /// can cheaply clone the pointer for read-only access without duplicating
    /// the underlying `Vec`. This keeps the action data consistent across the
    /// application while allowing plugins to inspect it from their own threads.
    pub fn reload_from_dirs(
        &mut self,
        dirs: &[String],
        clipboard_limit: usize,
        net_unit: NetUnit,
        reset_alarm: bool,
        plugin_settings: &std::collections::HashMap<String, Value>,
        actions: Arc<Vec<Action>>,
    ) {
        self.clear_plugins();
        // Drop previously loaded dynamic libraries to avoid accumulating
        // duplicate handles when reloading plugins.
        self.register_with_settings(WebSearchPlugin, plugin_settings);
        self.register_with_settings(CalculatorPlugin::default(), plugin_settings);
        self.register_with_settings(UnitConvertPlugin, plugin_settings);
        self.register_with_settings(BaseConvertPlugin, plugin_settings);
        self.register_with_settings(DropCalcPlugin, plugin_settings);
        self.register_with_settings(RunescapeSearchPlugin, plugin_settings);
        self.register_with_settings(YoutubePlugin, plugin_settings);
        self.register_with_settings(RedditPlugin, plugin_settings);
        self.register_with_settings(WikipediaPlugin, plugin_settings);
        self.register_with_settings(ClipboardPlugin::new(clipboard_limit), plugin_settings);
        let clipboard_modifier_catalog = Arc::clone(&self.services.clipboard_modifier_catalog);
        self.register_with_settings(
            ClipboardModifyPlugin::new(clipboard_modifier_catalog),
            plugin_settings,
        );
        self.register_with_settings(BookmarksPlugin::default(), plugin_settings);
        self.register_with_settings(FoldersPlugin::default(), plugin_settings);
        self.register_with_settings(FileSearchPlugin::default(), plugin_settings);
        self.register_with_settings(DiffPlugin::default(), plugin_settings);
        self.register_with_settings(OmniSearchPlugin::new(actions.clone()), plugin_settings);
        self.register_with_settings(SystemPlugin, plugin_settings);
        if self.services.system_data_runtime.is_none() {
            self.services.system_data_runtime = Some(SystemDataRuntime::start(Arc::clone(
                &self.services.search_updates,
            )));
        }
        let system_data = self
            .services
            .system_data_runtime
            .as_ref()
            .expect("system data runtime initialized")
            .cache();
        self.register_with_settings(ProcessesPlugin::new(system_data.clone()), plugin_settings);
        self.register_with_settings(SysInfoPlugin::new(system_data.clone()), plugin_settings);
        self.register_with_settings(NetworkPlugin::new(net_unit), plugin_settings);
        self.register_with_settings(ShellPlugin, plugin_settings);
        self.register_with_settings(HistoryPlugin, plugin_settings);
        self.register_with_settings(NotePlugin::default(), plugin_settings);
        self.register_with_settings(TodoPlugin::default(), plugin_settings);
        self.register_with_settings(CalendarPlugin, plugin_settings);
        self.register_with_settings(SnippetsPlugin::default(), plugin_settings);
        self.register_with_settings(MacrosPlugin::default(), plugin_settings);
        self.register_with_settings(
            MkMacroPlugin::new(Arc::clone(&self.services.mkmacro_store)),
            plugin_settings,
        );
        self.register_with_settings(KeysPlugin, plugin_settings);
        self.register_with_settings(MouseGesturesPlugin::default(), plugin_settings);
        self.register_with_settings(MultiManagerPlugin, plugin_settings);
        self.register_with_settings(FavPlugin::default(), plugin_settings);
        self.register_with_settings(MissingPlugin, plugin_settings);
        self.register_with_settings(RecyclePlugin, plugin_settings);
        self.register_with_settings(TempfilePlugin, plugin_settings);
        self.register_with_settings(MediaPlugin, plugin_settings);
        self.register_with_settings(AsciiArtPlugin::default(), plugin_settings);
        self.register_with_settings(EmojiPlugin::default(), plugin_settings);
        self.register_with_settings(TextCasePlugin, plugin_settings);
        self.register_with_settings(ScreenshotPlugin, plugin_settings);
        self.register_with_settings(CropPlugin, plugin_settings);
        self.register_with_settings(DataPlugin, plugin_settings);
        self.register_with_settings(TimestampPlugin, plugin_settings);
        self.register_with_settings(
            IpPlugin::with_updates(Arc::clone(&self.services.search_updates)),
            plugin_settings,
        );
        self.register_with_settings(RandomPlugin::default(), plugin_settings);
        self.register_with_settings(LoremPlugin, plugin_settings);
        self.register_with_settings(ConvertPanelPlugin, plugin_settings);
        self.register_with_settings(ColorPickerPlugin::default(), plugin_settings);
        self.register_with_settings(VolumePlugin::new(system_data), plugin_settings);
        self.register_with_settings(BrightnessPlugin, plugin_settings);
        self.register_with_settings(TaskManagerPlugin, plugin_settings);
        self.register_with_settings(
            WindowsPlugin::new(Arc::clone(&self.services.window_catalog)),
            plugin_settings,
        );
        self.register_with_settings(
            VirtualDesktopPlugin::new(
                Arc::clone(&self.services.workspace_catalog),
                Arc::clone(&self.services.window_catalog),
                actions.clone(),
            ),
            plugin_settings,
        );
        self.register_with_settings(
            BrowserTabsPlugin::with_updates(Arc::clone(&self.services.search_updates)),
            plugin_settings,
        );
        self.register_with_settings(SettingsPlugin, plugin_settings);
        self.register_with_settings(HelpPlugin, plugin_settings);
        self.register_with_settings(LayoutPlugin, plugin_settings);
        self.register_with_settings(LinkPlugin, plugin_settings);
        self.register_with_settings(TimerPlugin, plugin_settings);
        self.register_with_settings(StopwatchPlugin::default(), plugin_settings);
        if reset_alarm {
            crate::plugins::timer::reset_alarms_loaded();
        }
        crate::plugins::timer::load_saved_alarms();
        self.register_with_settings(WeatherPlugin, plugin_settings);
        for dir in dirs {
            tracing::debug!("loading plugins from {dir}");
            let _ = self.load_dir(dir, plugin_settings);
        }
        tracing::debug!(loaded=?self.plugin_names());
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        tracing::debug!("registered plugin {}", plugin.name());
        self.register_slot(plugin, None, None);
    }

    fn register_slot(
        &mut self,
        plugin: Box<dyn Plugin>,
        library: Option<Arc<libloading::Library>>,
        library_path: Option<PathBuf>,
    ) {
        self.next_plugin_epoch = self.next_plugin_epoch.wrapping_add(1).max(1);
        let name = plugin.name().to_string();
        self.plugins.push(Arc::new(PluginSlot {
            name,
            plugin: RwLock::new(plugin),
            _library: library,
            library_path,
            epoch: self.next_plugin_epoch,
        }));
    }

    fn register_with_settings<P: Plugin + 'static>(
        &mut self,
        mut plugin: P,
        settings: &std::collections::HashMap<String, Value>,
    ) {
        if let Some(val) = settings.get(plugin.name()) {
            plugin.apply_settings(val);
        }
        self.register(Box::new(plugin));
    }

    /// Return a list of registered plugin names.
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins.iter().map(|slot| slot.name.clone()).collect()
    }

    pub fn deferred_plugin_reload_paths(&self) -> Vec<PathBuf> {
        self.deferred_dynamic_reloads
            .iter()
            .filter(|(_, slot)| slot.strong_count() > 0)
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// Return names, descriptions and capabilities for all plugins.
    pub fn plugin_infos(&self) -> Vec<(String, String, Vec<String>)> {
        self.iter()
            .map(|p| {
                (
                    p.name().to_string(),
                    p.description().to_string(),
                    p.capabilities().iter().map(|c| c.to_string()).collect(),
                )
            })
            .collect()
    }

    /// Collect command shortcuts from plugins filtered by `enabled_plugins`.
    pub fn commands_filtered(&self, enabled_plugins: Option<&HashSet<String>>) -> Vec<Action> {
        let mut out = Vec::new();
        for p in self.iter() {
            if let Some(set) = enabled_plugins
                && !set.contains(p.name())
            {
                continue;
            }
            out.extend(p.commands());
        }
        out
    }

    /// Collect command shortcuts from all plugins.
    pub fn commands(&self) -> Vec<Action> {
        self.commands_filtered(None)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = RwLockWriteGuard<'_, Box<dyn Plugin>>> {
        self.plugins
            .iter()
            .filter_map(|slot| slot.plugin.write().ok())
    }

    pub fn iter(&self) -> impl Iterator<Item = RwLockReadGuard<'_, Box<dyn Plugin>>> {
        self.plugins
            .iter()
            .filter_map(|slot| slot.plugin.read().ok())
    }

    pub(crate) fn owned_plugin(&self, name: &str) -> Option<OwnedPluginHandle> {
        self.plugins.iter().find_map(|slot| {
            let matches = slot.name == name;
            matches.then(|| OwnedPluginHandle {
                epoch: slot.epoch,
                slot: Arc::clone(slot),
            })
        })
    }

    pub(crate) fn try_write_plugin(
        &self,
        name: &str,
    ) -> Result<RwLockWriteGuard<'_, Box<dyn Plugin>>, &'static str> {
        let slot = self
            .plugins
            .iter()
            .find(|slot| slot.name == name)
            .ok_or("plugin unavailable")?;
        slot.plugin.try_write().map_err(|_| "plugin busy")
    }

    pub fn load_dir(
        &mut self,
        path: &str,
        plugin_settings: &std::collections::HashMap<String, Value>,
    ) -> anyhow::Result<()> {
        use std::ffi::OsStr;

        let ext = "dll";

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                continue;
            }
            if entry.path().extension() != Some(OsStr::new(ext)) {
                continue;
            }
            let library_path = entry.path();
            self.deferred_dynamic_reloads
                .retain(|(_, slot)| slot.strong_count() > 0);
            if self
                .deferred_dynamic_reloads
                .iter()
                .any(|(deferred, _)| deferred == &library_path)
            {
                tracing::warn!(
                    path = %library_path.display(),
                    "plugin reload deferred while its previous instance is still active"
                );
                continue;
            }

            unsafe {
                let lib = Arc::new(Library::new(&library_path)?);
                let constructor: libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> =
                    lib.get(b"create_plugin")?;
                let mut plugin = constructor();
                if let Some(val) = plugin_settings.get(plugin.name()) {
                    plugin.apply_settings(val);
                }
                let name = plugin.name().to_string();
                self.register_slot(plugin, Some(lib), Some(library_path));
                tracing::debug!("loaded plugin {name}");
            }
        }
        Ok(())
    }

    /// Search with plugin and capability filters.
    pub fn search_filtered(
        &self,
        query: &str,
        enabled_plugins: Option<&HashSet<String>>,
        enabled_caps: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Vec<Action> {
        let (filtered_query, filters) = split_action_filters(query);
        let query_head = filtered_query
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase);
        let mut actions = Vec::new();
        let perf_enabled = crate::performance::enabled();
        for p in self.iter() {
            let name = p.name();
            if let Some(list) = enabled_plugins
                && !list.contains(name)
            {
                continue;
            }
            if let Some(map) = enabled_caps
                && let Some(caps) = map.get(name)
                && !caps.iter().any(|c| c == "search")
            {
                continue;
            }

            if !p.always_search() {
                let prefixes = p.query_prefixes();
                if !prefixes.is_empty() {
                    let Some(head) = query_head.as_deref() else {
                        continue;
                    };
                    if !prefixes
                        .iter()
                        .any(|prefix| prefix.eq_ignore_ascii_case(head))
                    {
                        continue;
                    }
                }
            }

            let timer = crate::performance::Timer::start_if(perf_enabled);
            let plugin_actions = p.search(&filtered_query);
            timer.finish_plugin(name);
            actions.extend(plugin_actions);
        }
        if filters.include_kinds.is_empty()
            && filters.exclude_kinds.is_empty()
            && filters.include_ids.is_empty()
            && filters.exclude_ids.is_empty()
        {
            actions
        } else {
            apply_action_filters(actions, &filters)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct BlockingPlugin {
        started: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    struct NamedPlugin(&'static str);

    impl Plugin for NamedPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            Vec::new()
        }
        fn name(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            self.0
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }

    struct CompletingTicketPlugin {
        updates: Arc<PluginSearchUpdates>,
    }

    impl Plugin for CompletingTicketPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            let ticket = self.updates.schedule_or_join("completing");
            record_search_refresh_ticket("completing", ticket.id);
            self.updates.publish_ticket("completing", ticket.id);
            Vec::new()
        }
        fn name(&self) -> &str {
            "completing"
        }
        fn description(&self) -> &str {
            "test"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }

    impl Plugin for BlockingPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            self.started.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
            Vec::new()
        }
        fn name(&self) -> &str {
            "blocking"
        }
        fn description(&self) -> &str {
            "blocking"
        }
        fn capabilities(&self) -> &[&str] {
            &["search"]
        }
    }

    #[test]
    fn built_in_search_updates_increment_generation_and_repaint() {
        let manager = PluginManager::new();
        let (repaint_tx, repaint_rx) = std::sync::mpsc::channel();
        manager.set_search_repaint_callback(Arc::new(move || repaint_tx.send(()).unwrap()));
        let before = manager.search_generation();
        let source_before = manager.search_generation_for("test");
        manager.services.search_updates.notify("test");
        repaint_rx.recv().unwrap();
        assert_eq!(manager.search_generation(), before + 1);
        assert_eq!(manager.search_generation_for("test"), source_before + 1);
        assert_eq!(manager.search_generation_for("unrelated"), 0);
    }

    #[test]
    fn refresh_tickets_distinguish_joined_fresh_and_unrelated_publications() {
        let updates = PluginSearchUpdates::default();
        let ticket = updates.schedule_or_join("windows");
        assert!(ticket.start);
        assert_eq!(updates.active_ticket("windows"), Some(ticket.id));
        assert_eq!(updates.published_ticket("windows"), None);
        updates.notify("browser_tabs");
        assert_eq!(updates.active_ticket("windows"), Some(ticket.id));
        assert_eq!(updates.published_ticket("windows"), None);
        updates.publish_ticket("windows", ticket.id);
        assert_eq!(updates.active_ticket("windows"), None);
        assert_eq!(updates.published_ticket("windows"), Some(ticket.id));
    }

    #[test]
    fn superseded_ticket_cannot_publish_or_notify() {
        let updates = PluginSearchUpdates::default();
        let stale = updates.schedule_or_join("windows");
        updates.cancel_ticket("windows", stale.id);
        let current = updates.schedule_or_join("windows");
        updates.publish_ticket("windows", stale.id);
        assert_eq!(updates.generation(), 1);
        assert_eq!(updates.active_ticket("windows"), Some(current.id));
        assert_eq!(updates.published_ticket("windows"), None);
        updates.publish_ticket("windows", current.id);
        assert_eq!(updates.generation(), 2);
        assert_eq!(updates.published_ticket("windows"), Some(current.id));
    }

    #[test]
    fn completed_before_search_returns_still_returns_exact_resolved_ticket() {
        let updates = Arc::new(PluginSearchUpdates::default());
        let mut manager = PluginManager::new();
        manager.services.search_updates = Arc::clone(&updates);
        manager.register(Box::new(CompletingTicketPlugin {
            updates: Arc::clone(&updates),
        }));
        let (_, ticket) = manager
            .search_plugin_with_ticket("completing", "query")
            .unwrap();
        let ticket = ticket.expect("search transaction ticket");
        assert!(manager.search_ticket_resolved("completing", ticket));
        assert_eq!(updates.published_ticket("completing"), Some(ticket));
    }

    #[test]
    fn cancellation_resolves_joined_waiters_and_allows_next_ticket() {
        let updates = PluginSearchUpdates::default();
        let first = updates.schedule_or_join("windows");
        let joined = updates.schedule_or_join("windows");
        assert_eq!(joined.id, first.id);
        assert!(!joined.start);
        updates.cancel_ticket("windows", first.id);
        assert!(updates.ticket_resolved("windows", joined.id));
        let next = updates.schedule_or_join("windows");
        assert!(next.start);
        assert_ne!(next.id, first.id);
    }

    #[test]
    fn plugin_instance_epoch_advances_across_reload() {
        let mut manager = PluginManager::new();
        manager.register(Box::new(NamedPlugin("epoch")));
        let first = manager.owned_plugin("epoch").unwrap().epoch;
        manager.clear_plugins();
        manager.register(Box::new(NamedPlugin("epoch")));
        assert!(manager.owned_plugin("epoch").unwrap().epoch > first);
    }

    #[cfg(windows)]
    #[test]
    fn owned_handle_pins_only_its_originating_library() {
        let first_library = Arc::new(unsafe { Library::new("kernel32.dll").unwrap() });
        let unrelated_library = Arc::new(unsafe { Library::new("kernel32.dll").unwrap() });
        let mut manager = PluginManager::new();
        manager.register_slot(
            Box::new(NamedPlugin("first")),
            Some(Arc::clone(&first_library)),
            Some(PathBuf::from("first.dll")),
        );
        manager.register_slot(
            Box::new(NamedPlugin("unrelated")),
            Some(Arc::clone(&unrelated_library)),
            Some(PathBuf::from("unrelated.dll")),
        );
        let first_handle = manager.owned_plugin("first").unwrap();
        manager.clear_plugins();
        assert_eq!(
            manager.deferred_plugin_reload_paths(),
            vec![PathBuf::from("first.dll")]
        );
        assert_eq!(Arc::strong_count(&first_library), 2);
        assert_eq!(Arc::strong_count(&unrelated_library), 1);
        drop(first_handle);
        assert_eq!(Arc::strong_count(&first_library), 1);
        assert!(manager.deferred_plugin_reload_paths().is_empty());
    }
    #[test]
    fn shared_catalog_handle_preserved_across_reload() {
        let mut manager = PluginManager::new();
        let handle = Arc::clone(&manager.internal_services().clipboard_modifier_catalog);
        manager.reload_from_dirs(
            &[],
            10,
            NetUnit::Auto,
            false,
            &HashMap::new(),
            Arc::new(Vec::new()),
        );
        assert!(Arc::ptr_eq(
            &handle,
            &manager.internal_services().clipboard_modifier_catalog
        ));
    }

    #[test]
    fn blocked_search_settings_frame_reports_busy_without_waiting() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut manager = PluginManager::new();
        manager.register(Box::new(BlockingPlugin {
            started: started_tx,
            release: Mutex::new(release_rx),
        }));
        let handle = manager.owned_plugin("blocking").unwrap();
        let worker = std::thread::spawn(move || {
            let plugin = handle.read().unwrap();
            plugin.search("query");
        });
        started_rx.recv().unwrap();
        let mut rendered_busy = false;
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                if matches!(manager.try_write_plugin("blocking"), Err("plugin busy")) {
                    ui.label("blocking settings are temporarily busy; retry next frame.");
                    rendered_busy = true;
                }
            });
        });
        assert!(rendered_busy);
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(manager.try_write_plugin("blocking").is_ok());
    }
}
