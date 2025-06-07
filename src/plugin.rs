use crate::actions::Action;

pub trait Plugin: Send + Sync {
    /// Return actions based on the query string
    fn search(&self, query: &str) -> Vec<Action>;
    /// Name of the plugin
    fn name(&self) -> &str;
}

/// A manager that holds plugins
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn search(&self, query: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        for p in &self.plugins {
            actions.extend(p.search(query));
        }
        actions
    }
}
