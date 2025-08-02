use crate::actions::Action;
use crate::plugins::asciiart::AsciiArtPlugin;
use crate::plugins::emoji::EmojiPlugin;
use crate::plugins::screenshot::ScreenshotPlugin;
use crate::plugins::text_case::TextCasePlugin;
use crate::plugins::bookmarks::BookmarksPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::brightness::BrightnessPlugin;
use crate::plugins::clipboard::ClipboardPlugin;
use crate::plugins::dropcalc::DropCalcPlugin;
use crate::plugins::folders::FoldersPlugin;
use crate::plugins::help::HelpPlugin;
use crate::plugins::history::HistoryPlugin;
use crate::plugins::media::MediaPlugin;
use crate::plugins::network::NetworkPlugin;
use crate::plugins::notes::NotesPlugin;
use crate::plugins::processes::ProcessesPlugin;
use crate::plugins::recycle::RecyclePlugin;
use crate::plugins::reddit::RedditPlugin;
use crate::plugins::runescape::RunescapeSearchPlugin;
use crate::plugins::shell::ShellPlugin;
use crate::plugins::snippets::SnippetsPlugin;
use crate::plugins::fav::FavPlugin;
use crate::plugins::macros::MacrosPlugin;
use crate::plugins::omni_search::OmniSearchPlugin;
use crate::plugins::sysinfo::SysInfoPlugin;
use crate::plugins::system::SystemPlugin;
use crate::plugins::settings::SettingsPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::task_manager::TaskManagerPlugin;
use crate::plugins::tempfile::TempfilePlugin;
use crate::plugins::timer::TimerPlugin;
use crate::plugins::stopwatch::StopwatchPlugin;
use crate::plugins::todo::TodoPlugin;
use crate::plugins::unit_convert::UnitConvertPlugin;
use crate::plugins::base_convert::BaseConvertPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::volume::VolumePlugin;
use crate::plugins::weather::WeatherPlugin;
use crate::plugins::wikipedia::WikipediaPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::windows::WindowsPlugin;
use crate::plugins::youtube::YoutubePlugin;
use crate::plugins::ip::IpPlugin;
use crate::plugins::timestamp::TimestampPlugin;
use crate::plugins::random::RandomPlugin;
use crate::plugins::lorem::LoremPlugin;
use crate::plugins::convert_panel::ConvertPanelPlugin;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::settings::NetUnit;
use std::collections::HashSet;
use std::sync::Arc;
use libloading::Library;
use serde_json::Value;
use eframe::egui;

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
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    #[allow(dead_code)]
    libs: Vec<libloading::Library>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            libs: Vec::new(),
        }
    }

    /// Remove all registered plugins without unloading libraries.
    pub fn clear_plugins(&mut self) {
        self.plugins.clear();
    }

    /// Rebuild the plugin list, keeping previously loaded libraries alive.
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
        self.libs.clear();
        self.register_with_settings(WebSearchPlugin, plugin_settings);
        self.register_with_settings(CalculatorPlugin, plugin_settings);
        self.register_with_settings(UnitConvertPlugin, plugin_settings);
        self.register_with_settings(BaseConvertPlugin, plugin_settings);
        self.register_with_settings(DropCalcPlugin, plugin_settings);
        self.register_with_settings(RunescapeSearchPlugin, plugin_settings);
        self.register_with_settings(YoutubePlugin, plugin_settings);
        self.register_with_settings(RedditPlugin, plugin_settings);
        self.register_with_settings(WikipediaPlugin, plugin_settings);
        self.register_with_settings(ClipboardPlugin::new(clipboard_limit), plugin_settings);
        self.register_with_settings(BookmarksPlugin::default(), plugin_settings);
        self.register_with_settings(FoldersPlugin::default(), plugin_settings);
        self.register_with_settings(OmniSearchPlugin::new(actions.clone()), plugin_settings);
        self.register_with_settings(SystemPlugin, plugin_settings);
        self.register_with_settings(ProcessesPlugin, plugin_settings);
        self.register_with_settings(SysInfoPlugin, plugin_settings);
        self.register_with_settings(NetworkPlugin::new(net_unit), plugin_settings);
        self.register_with_settings(ShellPlugin, plugin_settings);
        self.register_with_settings(HistoryPlugin, plugin_settings);
        self.register_with_settings(NotesPlugin::default(), plugin_settings);
        self.register_with_settings(TodoPlugin::default(), plugin_settings);
        self.register_with_settings(SnippetsPlugin::default(), plugin_settings);
        self.register_with_settings(MacrosPlugin::default(), plugin_settings);
        self.register_with_settings(FavPlugin::default(), plugin_settings);
        self.register_with_settings(RecyclePlugin, plugin_settings);
        self.register_with_settings(TempfilePlugin, plugin_settings);
        self.register_with_settings(MediaPlugin, plugin_settings);
        self.register_with_settings(AsciiArtPlugin::default(), plugin_settings);
        self.register_with_settings(EmojiPlugin::default(), plugin_settings);
        self.register_with_settings(TextCasePlugin, plugin_settings);
        self.register_with_settings(ScreenshotPlugin, plugin_settings);
        self.register_with_settings(TimestampPlugin, plugin_settings);
        self.register_with_settings(IpPlugin, plugin_settings);
        self.register_with_settings(RandomPlugin::default(), plugin_settings);
        self.register_with_settings(LoremPlugin, plugin_settings);
        self.register_with_settings(ConvertPanelPlugin, plugin_settings);
        #[cfg(target_os = "windows")]
        {
            self.register_with_settings(VolumePlugin, plugin_settings);
            self.register_with_settings(BrightnessPlugin, plugin_settings);
            self.register_with_settings(TaskManagerPlugin, plugin_settings);
            self.register_with_settings(WindowsPlugin, plugin_settings);
        }
        self.register_with_settings(SettingsPlugin, plugin_settings);
        self.register_with_settings(HelpPlugin, plugin_settings);
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
        self.plugins.push(plugin);
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
        self.plugins.iter().map(|p| p.name().to_string()).collect()
    }

    /// Return names, descriptions and capabilities for all plugins.
    pub fn plugin_infos(&self) -> Vec<(String, String, Vec<String>)> {
        self.plugins
            .iter()
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
        for p in &self.plugins {
            if let Some(set) = enabled_plugins {
                if !set.contains(p.name()) {
                    continue;
                }
            }
            out.extend(p.commands());
        }
        out
    }

    /// Collect command shortcuts from all plugins.
    pub fn commands(&self) -> Vec<Action> {
        self.commands_filtered(None)
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Box<dyn Plugin>> {
        self.plugins.iter_mut()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Box<dyn Plugin>> {
        self.plugins.iter()
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

            unsafe {
                let lib = Library::new(entry.path())?;
                let constructor: libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> =
                    lib.get(b"create_plugin")?;
                let mut plugin = constructor();
                if let Some(val) = plugin_settings.get(plugin.name()) {
                    plugin.apply_settings(val);
                }
                let name = plugin.name().to_string();
                self.plugins.push(plugin);
                self.libs.push(lib);
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
        let mut actions = Vec::new();
        for p in &self.plugins {
            let name = p.name();
            if let Some(list) = enabled_plugins {
                if !list.contains(name) {
                    continue;
                }
            }
            if let Some(map) = enabled_caps {
                if let Some(caps) = map.get(name) {
                    if !caps.contains(&"search".to_string()) {
                        continue;
                    }
                }
            }
            actions.extend(p.search(query));
        }
        actions
    }
}
