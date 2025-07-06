use crate::actions::Action;
use libloading::Library;
use crate::plugins_builtin::{WebSearchPlugin, CalculatorPlugin};
use crate::plugins::clipboard::ClipboardPlugin;

pub trait Plugin: Send + Sync {
    /// Return actions based on the query string
    fn search(&self, query: &str) -> Vec<Action>;
    /// Name of the plugin
    fn name(&self) -> &str;
    /// Human readable description of the plugin
    fn description(&self) -> &str;
    /// Capabilities offered by the plugin
    fn capabilities(&self) -> &[&str];
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
    pub fn reload_from_dirs(&mut self, dirs: &[String]) {
        self.clear_plugins();
        self.register(Box::new(WebSearchPlugin));
        self.register(Box::new(CalculatorPlugin));
        self.register(Box::new(ClipboardPlugin::default()));
        for dir in dirs {
            let _ = self.load_dir(dir);
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// Return a list of registered plugin names.
    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name().to_string()).collect()
    }

    /// Return the capabilities for all plugins.
    pub fn plugin_capabilities(&self) -> Vec<(String, Vec<String>)> {
        self.plugins
            .iter()
            .map(|p| {
                (
                    p.name().to_string(),
                    p.capabilities().iter().map(|c| c.to_string()).collect(),
                )
            })
            .collect()
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

    pub fn load_dir(&mut self, path: &str) -> anyhow::Result<()> {
        use std::ffi::OsStr;

        let ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };

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
                self.plugins.push(plugin);
                self.libs.push(lib);
            }
        }
        Ok(())
    }

    /// Load plugins from a directory, enabling only those whose names are
    /// present in `enabled` when provided.
    pub fn load_dir_filtered(
        &mut self,
        path: &str,
        enabled: Option<&Vec<String>>,
    ) -> anyhow::Result<()> {
        use std::ffi::OsStr;

        let ext = if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };

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
                let plugin = constructor();
                if let Some(list) = enabled {
                    if !list.contains(&plugin.name().to_string()) {
                        continue;
                    }
                }
                self.plugins.push(plugin);
                self.libs.push(lib);
            }
        }
        Ok(())
    }

    pub fn search(&self, query: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        for p in &self.plugins {
            actions.extend(p.search(query));
        }
        actions
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
