use crate::actions::Action;
use libloading::Library;
use crate::plugins_builtin::{WebSearchPlugin, CalculatorPlugin};
use crate::plugins::unit_convert::UnitConvertPlugin;
use crate::plugins::dropcalc::DropCalcPlugin;
use crate::plugins::clipboard::ClipboardPlugin;
use crate::plugins::shell::ShellPlugin;
use crate::plugins::bookmarks::BookmarksPlugin;
use crate::plugins::runescape::RunescapeSearchPlugin;
use crate::plugins::history::HistoryPlugin;
use crate::plugins::folders::FoldersPlugin;
use crate::plugins::system::SystemPlugin;
use crate::plugins::processes::ProcessesPlugin;
use crate::plugins::sysinfo::SysInfoPlugin;
use crate::plugins::network::NetworkPlugin;
use crate::plugins::help::HelpPlugin;
use crate::plugins::youtube::YoutubePlugin;
use crate::plugins::reddit::RedditPlugin;
use crate::plugins::wikipedia::WikipediaPlugin;
use crate::plugins::weather::WeatherPlugin;
use crate::plugins::timer::TimerPlugin;
use crate::plugins::notes::NotesPlugin;
use crate::plugins::todo::TodoPlugin;
use crate::plugins::snippets::SnippetsPlugin;
use crate::plugins::recycle::RecyclePlugin;
use crate::plugins::tempfile::TempfilePlugin;
use crate::plugins::asciiart::AsciiArtPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::windows::WindowsPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::volume::VolumePlugin;
#[cfg(target_os = "windows")]
use crate::plugins::brightness::BrightnessPlugin;
#[cfg(target_os = "windows")]
use crate::plugins::task_manager::TaskManagerPlugin;

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
    pub fn reload_from_dirs(&mut self, dirs: &[String], clipboard_limit: usize, reset_alarm: bool) {
        self.clear_plugins();
        self.register(Box::new(WebSearchPlugin));
        self.register(Box::new(CalculatorPlugin));
        self.register(Box::new(UnitConvertPlugin));
        self.register(Box::new(DropCalcPlugin));
        self.register(Box::new(RunescapeSearchPlugin));
        self.register(Box::new(YoutubePlugin));
        self.register(Box::new(RedditPlugin));
        self.register(Box::new(WikipediaPlugin));
        self.register(Box::new(ClipboardPlugin::new(clipboard_limit)));
        self.register(Box::new(BookmarksPlugin::default()));
        self.register(Box::new(FoldersPlugin::default()));
        self.register(Box::new(SystemPlugin));
        self.register(Box::new(ProcessesPlugin));
        self.register(Box::new(SysInfoPlugin));
        self.register(Box::new(NetworkPlugin::default()));
        self.register(Box::new(ShellPlugin));
        self.register(Box::new(HistoryPlugin));
        self.register(Box::new(NotesPlugin::default()));
        self.register(Box::new(TodoPlugin::default()));
        self.register(Box::new(SnippetsPlugin::default()));
        self.register(Box::new(RecyclePlugin));
        self.register(Box::new(TempfilePlugin));
        self.register(Box::new(AsciiArtPlugin::default()));
        #[cfg(target_os = "windows")]
        {
            self.register(Box::new(VolumePlugin));
            self.register(Box::new(BrightnessPlugin));
            self.register(Box::new(TaskManagerPlugin));
            self.register(Box::new(WindowsPlugin));
        }
        self.register(Box::new(HelpPlugin));
        self.register(Box::new(TimerPlugin));
        if reset_alarm {
            crate::plugins::timer::reset_alarms_loaded();
        }
        crate::plugins::timer::load_saved_alarms();
        self.register(Box::new(WeatherPlugin));
        for dir in dirs {
            tracing::debug!("loading plugins from {dir}");
            let _ = self.load_dir(dir);
        }
        tracing::debug!(loaded=?self.plugin_names());
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        tracing::debug!("registered plugin {}", plugin.name());
        self.plugins.push(plugin);
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

    /// Collect command shortcuts from all plugins.
    pub fn commands(&self) -> Vec<Action> {
        let mut out = Vec::new();
        for p in &self.plugins {
            out.extend(p.commands());
        }
        out
    }

    pub fn load_dir(&mut self, path: &str) -> anyhow::Result<()> {
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
                let constructor: libloading::Symbol<unsafe extern "C" fn() -> Box<dyn Plugin>> = lib.get(b"create_plugin")?;
                let plugin = constructor();
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
        enabled_plugins: Option<&Vec<String>>,
        enabled_caps: Option<&std::collections::HashMap<String, Vec<String>>>,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        for p in &self.plugins {
            let name = p.name();
            if let Some(list) = enabled_plugins {
                if !list.contains(&name.to_string()) {
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
