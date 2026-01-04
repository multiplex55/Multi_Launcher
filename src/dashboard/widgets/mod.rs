use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugin::PluginManager;
use eframe::egui;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
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

/// Result of editing widget settings.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct WidgetSettingsUiResult {
    pub changed: bool,
    pub error: Option<String>,
}

/// Context available to widget settings UIs.
#[derive(Clone, Copy)]
pub struct WidgetSettingsContext<'a> {
    pub plugins: Option<&'a PluginManager>,
    pub actions: Option<&'a [Action]>,
    pub usage: Option<&'a std::collections::HashMap<String, u32>>,
    pub default_location: Option<&'a str>,
}

impl<'a> WidgetSettingsContext<'a> {
    pub fn empty() -> Self {
        Self {
            plugins: None,
            actions: None,
            usage: None,
            default_location: None,
        }
    }
}

/// Handler used to render widget settings.
pub type SettingsUiFn =
    fn(&mut egui::Ui, &mut Value, &WidgetSettingsContext<'_>) -> WidgetSettingsUiResult;

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
    default_settings: Value,
    settings_ui: Option<SettingsUiFn>,
}

impl WidgetFactory {
    pub fn new<
        T: Widget + Default + 'static,
        C: DeserializeOwned + Serialize + Default + 'static,
    >(
        build: fn(C) -> T,
    ) -> Self {
        Self {
            ctor: std::sync::Arc::new(move |v| {
                let cfg = serde_json::from_value::<C>(v.clone()).unwrap_or_default();
                Box::new(build(cfg))
            }),
            default_settings: serde_json::to_value(C::default()).unwrap_or_else(|_| json!({})),
            settings_ui: None,
        }
    }

    pub fn with_settings_ui(mut self, settings_ui: SettingsUiFn) -> Self {
        self.settings_ui = Some(settings_ui);
        self
    }

    pub fn default_settings(&self) -> Value {
        self.default_settings.clone()
    }

    pub fn settings_ui(&self) -> Option<SettingsUiFn> {
        self.settings_ui
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
        reg.register(
            "weather_site",
            WidgetFactory::new(WeatherSiteWidget::new)
                .with_settings_ui(WeatherSiteWidget::settings_ui),
        );
        reg.register(
            "plugin_home",
            WidgetFactory::new(PluginHomeWidget::new)
                .with_settings_ui(PluginHomeWidget::settings_ui),
        );
        reg.register(
            "notes_open",
            WidgetFactory::new(NotesOpenWidget::new).with_settings_ui(NotesOpenWidget::settings_ui),
        );
        reg.register(
            "note_meta",
            WidgetFactory::new(NoteMetaWidget::new).with_settings_ui(NoteMetaWidget::settings_ui),
        );
        reg.register(
            "recent_commands",
            WidgetFactory::new(RecentCommandsWidget::new)
                .with_settings_ui(RecentCommandsWidget::settings_ui),
        );
        reg.register(
            "frequent_commands",
            WidgetFactory::new(FrequentCommandsWidget::new)
                .with_settings_ui(FrequentCommandsWidget::settings_ui),
        );
        reg.register(
            "todo_summary",
            WidgetFactory::new(TodoSummaryWidget::new)
                .with_settings_ui(TodoSummaryWidget::settings_ui),
        );
        reg
    }

    pub fn register(&mut self, name: &str, factory: WidgetFactory) {
        self.map.insert(name.to_string(), factory);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn create(&self, name: &str, settings: &Value) -> Option<Box<dyn Widget>> {
        let settings = if settings.is_null() {
            self.default_settings(name)
                .unwrap_or_else(|| Value::Object(Default::default()))
        } else {
            settings.clone()
        };
        self.map.get(name).map(|f| f.create(&settings))
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.map.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn default_settings(&self, name: &str) -> Option<Value> {
        self.map.get(name).map(|f| f.default_settings())
    }

    pub fn settings_ui_fn(&self, name: &str) -> Option<SettingsUiFn> {
        self.map.get(name).and_then(|f| f.settings_ui())
    }

    pub fn render_settings_ui(
        &self,
        name: &str,
        ui: &mut egui::Ui,
        settings: &mut Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> Option<WidgetSettingsUiResult> {
        let factory = self.map.get(name)?;
        let render = factory.settings_ui()?;
        if settings.is_null() {
            *settings = factory.default_settings();
        }
        Some(render(ui, settings, ctx))
    }
}

fn merge_json(base: &Value, updates: &Value) -> Value {
    match (base, updates) {
        (Value::Object(a), Value::Object(b)) => {
            let mut merged = a.clone();
            for (k, v) in b {
                merged.insert(k.clone(), v.clone());
            }
            Value::Object(merged)
        }
        _ => updates.clone(),
    }
}

pub(crate) fn edit_typed_settings<C: DeserializeOwned + Serialize + Default>(
    ui: &mut egui::Ui,
    value: &mut Value,
    ctx: &WidgetSettingsContext<'_>,
    render: impl FnOnce(&mut egui::Ui, &mut C, &WidgetSettingsContext<'_>) -> bool,
) -> WidgetSettingsUiResult {
    let mut changed = false;
    let mut error = None;
    if value.is_null() {
        *value = serde_json::to_value(C::default()).unwrap_or_else(|_| json!({}));
        changed = true;
    }

    let original = value.clone();
    let mut cfg: C = match serde_json::from_value(original.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            error = Some(format!("Failed to parse settings: {e}"));
            C::default()
        }
    };

    let before = serde_json::to_value(&cfg).unwrap_or_else(|_| json!({}));
    let ui_changed = render(ui, &mut cfg, ctx);
    changed |= ui_changed;
    let serialized = serde_json::to_value(&cfg).unwrap_or_else(|_| json!({}));

    // Preserve unknown fields by merging on top of the original value.
    let merged = merge_json(&original, &serialized);
    if merged != *value {
        *value = merged;
        changed = true;
    } else if ui_changed && serialized != before {
        changed = true;
    }

    WidgetSettingsUiResult { changed, error }
}
