use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

mod frequent_commands;
mod note_meta;
mod notes_open;
mod plugin_home;
mod recent_commands;
mod todo_summary;
mod weather_site;

pub use frequent_commands::FrequentCommandsWidget;
pub use note_meta::NoteMetaWidget;
pub use notes_open::NotesOpenWidget;
pub use plugin_home::PluginHomeWidget;
pub use recent_commands::RecentCommandsWidget;
pub use todo_summary::TodoSummaryWidget;
pub use weather_site::WeatherSiteWidget;

/// Result of a widget activation.
#[derive(Debug, Clone)]
pub struct WidgetAction {
    pub action: Action,
    pub query_override: Option<String>,
}

/// Widget trait implemented by all dashboard widgets.
pub trait Widget: Send {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction>;
}

/// Factory for building widgets from JSON settings.
#[derive(Clone)]
pub struct WidgetFactory {
    ctor: std::sync::Arc<dyn Fn(&Value) -> Box<dyn Widget> + Send + Sync>,
}

impl WidgetFactory {
    pub fn new<T: Widget + Default + 'static, C: DeserializeOwned + Default + 'static>(
        build: fn(C) -> T,
    ) -> Self {
        Self {
            ctor: std::sync::Arc::new(move |v| {
                let cfg = serde_json::from_value::<C>(v.clone()).unwrap_or_default();
                Box::new(build(cfg))
            }),
        }
    }

    pub fn create(&self, settings: &Value) -> Box<dyn Widget> {
        (self.ctor)(settings)
    }
}

#[derive(Clone, Default)]
pub struct WidgetRegistry {
    map: HashMap<String, WidgetFactory>,
}

impl WidgetRegistry {
    pub fn with_defaults() -> Self {
        let mut reg = Self::default();
        reg.register("weather_site", WidgetFactory::new(WeatherSiteWidget::new));
        reg.register("notes_open", WidgetFactory::new(NotesOpenWidget::new));
        reg.register("note_meta", WidgetFactory::new(NoteMetaWidget::new));
        reg.register(
            "recent_commands",
            WidgetFactory::new(RecentCommandsWidget::new),
        );
        reg.register(
            "frequent_commands",
            WidgetFactory::new(FrequentCommandsWidget::new),
        );
        reg.register("todo_summary", WidgetFactory::new(TodoSummaryWidget::new));
        reg.register("plugin_home", WidgetFactory::new(PluginHomeWidget::new));
        reg
    }

    pub fn register(&mut self, name: &str, factory: WidgetFactory) {
        self.map.insert(name.to_string(), factory);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn create(&self, name: &str, settings: &Value) -> Option<Box<dyn Widget>> {
        self.map.get(name).map(|f| f.create(settings))
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.map.keys().cloned().collect();
        names.sort();
        names
    }
}
