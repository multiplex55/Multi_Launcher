use crate::actions::Action;
use libloading::Library;

pub trait Plugin: Send + Sync {
    /// Return actions based on the query string
    fn search(&self, query: &str) -> Vec<Action>;
    /// Name of the plugin
    fn name(&self) -> &str;
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

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
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

    pub fn search(&self, query: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        for p in &self.plugins {
            actions.extend(p.search(query));
        }
        actions
    }
}
