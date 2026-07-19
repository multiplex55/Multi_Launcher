use super::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetMetadata {
    pub name: String,
    pub has_settings_ui: bool,
}

#[derive(Clone)]
pub struct WidgetDescriptor {
    ctor: std::sync::Arc<dyn Fn(&Value) -> Box<dyn Widget> + Send + Sync>,
    default_settings: std::sync::Arc<dyn Fn() -> Value + Send + Sync>,
    settings_ui: Option<SettingsUiFn>,
}

pub type WidgetFactory = WidgetDescriptor;

impl WidgetDescriptor {
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
            default_settings: std::sync::Arc::new(|| {
                serde_json::to_value(C::default()).unwrap_or_else(|_| json!({}))
            }),
            settings_ui: None,
        }
    }

    pub fn with_settings_ui(mut self, settings_ui: SettingsUiFn) -> Self {
        self.settings_ui = Some(settings_ui);
        self
    }
    pub fn default_settings(&self) -> Value {
        (self.default_settings)()
    }
    pub fn settings_ui(&self) -> Option<SettingsUiFn> {
        self.settings_ui
    }
    pub fn create(&self, settings: &Value) -> Box<dyn Widget> {
        (self.ctor)(settings)
    }
    pub fn metadata(&self, name: &str) -> WidgetMetadata {
        WidgetMetadata {
            name: name.to_string(),
            has_settings_ui: self.settings_ui.is_some(),
        }
    }
}

#[derive(Clone, Default)]
pub struct WidgetRegistry {
    map: HashMap<String, WidgetDescriptor>,
}

impl WidgetRegistry {
    pub fn with_defaults() -> Self {
        let mut reg = Self::default();
        register_defaults(&mut reg);
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
    pub fn metadata(&self) -> Vec<WidgetMetadata> {
        let mut meta: Vec<WidgetMetadata> = self
            .map
            .iter()
            .map(|(name, descriptor)| descriptor.metadata(name))
            .collect();
        meta.sort_by(|a, b| a.name.cmp(&b.name));
        meta
    }
    pub fn metadata_for(&self, name: &str) -> Option<WidgetMetadata> {
        self.map.get(name).map(|d| d.metadata(name))
    }
    pub fn default_settings(&self, name: &str) -> Option<Value> {
        self.map.get(name).map(|f| f.default_settings())
    }
    pub fn descriptor(&self, name: &str) -> Option<&WidgetDescriptor> {
        self.map.get(name)
    }
    pub fn settings_ui_fn(&self, name: &str) -> Option<SettingsUiFn> {
        self.map.get(name).and_then(|f| f.settings_ui())
    }
    pub fn render_settings_ui(
        &self,
        name: &str,
        ui: &mut eframe::egui::Ui,
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

fn register_defaults(reg: &mut WidgetRegistry) {
    reg.register(
        "weather_site",
        WidgetFactory::new(WeatherSiteWidget::new).with_settings_ui(WeatherSiteWidget::settings_ui),
    );
    reg.register(
        "plugin_home",
        WidgetFactory::new(PluginHomeWidget::new).with_settings_ui(PluginHomeWidget::settings_ui),
    );
    reg.register(
        "command_history",
        WidgetFactory::new(CommandHistoryWidget::new)
            .with_settings_ui(CommandHistoryWidget::settings_ui),
    );
    reg.register("diagnostics", WidgetFactory::new(DiagnosticsWidget::new));
    reg.register("context_links", WidgetFactory::new(ContextLinksWidget::new));
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
        "gesture_cheat_sheet",
        WidgetFactory::new(GestureCheatSheetWidget::new)
            .with_settings_ui(GestureCheatSheetWidget::settings_ui),
    );
    reg.register(
        "gesture_recent",
        WidgetFactory::new(GestureRecentWidget::new)
            .with_settings_ui(GestureRecentWidget::settings_ui),
    );
    reg.register(
        "gesture_health",
        WidgetFactory::new(GestureHealthWidget::new),
    );
    reg.register(
        "todo",
        WidgetFactory::new(TodoWidget::new).with_settings_ui(TodoWidget::settings_ui),
    );
    reg.register(
        "layouts",
        WidgetFactory::new(LayoutsWidget::new).with_settings_ui(LayoutsWidget::settings_ui),
    );
    reg.register(
        "recent_notes",
        WidgetFactory::new(RecentNotesWidget::new).with_settings_ui(RecentNotesWidget::settings_ui),
    );
    reg.register(
        "pinned_commands",
        WidgetFactory::new(PinnedCommandsWidget::new)
            .with_settings_ui(PinnedCommandsWidget::settings_ui),
    );
    reg.register(
        "pinned_query_results",
        WidgetFactory::new(PinnedQueryResultsWidget::new)
            .with_settings_ui(PinnedQueryResultsWidget::settings_ui),
    );
    reg.register(
        "query_list",
        WidgetFactory::new(QueryListWidget::new).with_settings_ui(QueryListWidget::settings_ui),
    );
    reg.register(
        "timers",
        WidgetFactory::new(ActiveTimersWidget::new)
            .with_settings_ui(ActiveTimersWidget::settings_ui),
    );
    reg.register(
        "clipboard_snippets",
        WidgetFactory::new(ClipboardSnippetsWidget::new)
            .with_settings_ui(ClipboardSnippetsWidget::settings_ui),
    );
    reg.register(
        "clipboard_recent",
        WidgetFactory::new(ClipboardRecentWidget::new)
            .with_settings_ui(ClipboardRecentWidget::settings_ui),
    );
    reg.register(
        "browser_tabs",
        WidgetFactory::new(BrowserTabsWidget::new).with_settings_ui(BrowserTabsWidget::settings_ui),
    );
    reg.register(
        "calendar",
        WidgetFactory::new(CalendarWidget::new).with_settings_ui(CalendarWidget::settings_ui),
    );
    reg.register(
        "processes",
        WidgetFactory::new(ProcessesWidget::new).with_settings_ui(ProcessesWidget::settings_ui),
    );
    reg.register(
        "windows",
        WidgetFactory::new(WindowsWidget::new).with_settings_ui(WindowsWidget::settings_ui),
    );
    reg.register(
        "windows_overview",
        WidgetFactory::new(WindowsOverviewWidget::new)
            .with_settings_ui(WindowsOverviewWidget::settings_ui),
    );
    reg.register(
        "system",
        WidgetFactory::new(SystemWidget::new).with_settings_ui(SystemWidget::settings_ui),
    );
    reg.register(
        "system_controls",
        WidgetFactory::new(SystemControlsWidget::new)
            .with_settings_ui(SystemControlsWidget::settings_ui),
    );
    reg.register(
        "system_status",
        WidgetFactory::new(SystemStatusWidget::new)
            .with_settings_ui(SystemStatusWidget::settings_ui),
    );
    reg.register(
        "now_playing",
        WidgetFactory::new(NowPlayingWidget::new).with_settings_ui(NowPlayingWidget::settings_ui),
    );
    reg.register(
        "snippets_favorites",
        WidgetFactory::new(SnippetsFavoritesWidget::new)
            .with_settings_ui(SnippetsFavoritesWidget::settings_ui),
    );
    reg.register(
        "scratchpad",
        WidgetFactory::new(ScratchpadWidget::new).with_settings_ui(ScratchpadWidget::settings_ui),
    );
    reg.register(
        "notes_recent",
        WidgetFactory::new(NotesRecentWidget::new).with_settings_ui(NotesRecentWidget::settings_ui),
    );
    reg.register(
        "notes_tags",
        WidgetFactory::new(NotesTagsWidget::new).with_settings_ui(NotesTagsWidget::settings_ui),
    );
    reg.register(
        "notes_graph",
        WidgetFactory::new(NotesGraphWidget::new).with_settings_ui(NotesGraphWidget::settings_ui),
    );
    reg.register(
        "todo_focus",
        WidgetFactory::new(TodoFocusWidget::new).with_settings_ui(TodoFocusWidget::settings_ui),
    );
    reg.register(
        "quick_tools",
        WidgetFactory::new(QuickToolsWidget::new).with_settings_ui(QuickToolsWidget::settings_ui),
    );
    reg.register(
        "recycle_bin",
        WidgetFactory::new(RecycleBinWidget::new).with_settings_ui(RecycleBinWidget::settings_ui),
    );
    reg.register(
        "tempfiles",
        WidgetFactory::new(TempfilesWidget::new).with_settings_ui(TempfilesWidget::settings_ui),
    );
    reg.register(
        "volume",
        WidgetFactory::new(VolumeWidget::new).with_settings_ui(VolumeWidget::settings_ui),
    );
    reg.register(
        "stopwatch",
        WidgetFactory::new(StopwatchWidget::new).with_settings_ui(StopwatchWidget::settings_ui),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reports_settings_ui() {
        let descriptor = WidgetDescriptor::new(WeatherSiteWidget::new);
        let descriptor_with_ui = WidgetDescriptor::new(WeatherSiteWidget::new)
            .with_settings_ui(WeatherSiteWidget::settings_ui);
        let meta_without = descriptor.metadata("test");
        let meta_with = descriptor_with_ui.metadata("test");
        assert!(!meta_without.has_settings_ui);
        assert!(meta_with.has_settings_ui);
    }

    #[test]
    fn registry_defaults_are_deterministic_and_include_expected_widgets() {
        let registry = WidgetRegistry::with_defaults();
        let names = registry.names();
        assert!(names.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(registry.contains("weather_site"));
        assert!(registry.contains("todo"));
        assert!(registry.contains("windows_overview"));
        assert_eq!(names.first().map(String::as_str), Some("browser_tabs"));
        assert_eq!(names.last().map(String::as_str), Some("windows_overview"));
    }
}
