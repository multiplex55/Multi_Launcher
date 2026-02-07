use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::mouse_gestures::engine::DirMode;
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use crate::plugin::PluginManager;
use eframe::egui;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

mod active_timers;
mod browser_tabs;
mod calendar;
mod clipboard_recent;
mod clipboard_snippets;
mod command_history;
mod diagnostics;
mod frequent_commands;
mod gesture_cheat_sheet;
mod gesture_health;
mod gesture_recent;
mod layouts;
mod notes_graph;
mod notes_recent;
mod notes_tags;
mod now_playing;
mod pinned_commands;
mod pinned_query_results;
mod plugin_home;
mod process_list;
mod query_list;
mod quick_tools;
mod recent_commands;
mod recent_notes;
mod recycle_bin;
mod scratchpad;
mod snippets_favorites;
mod stopwatch;
mod system_actions;
mod system_controls;
mod system_status;
mod tempfiles;
mod todo;
mod todo_focus;
mod volume;
mod weather_site;
mod window_list;
mod windows_overview;

pub use active_timers::ActiveTimersWidget;
pub use browser_tabs::BrowserTabsWidget;
pub use calendar::CalendarWidget;
pub use clipboard_recent::ClipboardRecentWidget;
pub use clipboard_snippets::ClipboardSnippetsWidget;
pub use command_history::CommandHistoryWidget;
pub use diagnostics::DiagnosticsWidget;
pub use frequent_commands::FrequentCommandsWidget;
pub use gesture_cheat_sheet::GestureCheatSheetWidget;
pub use gesture_health::GestureHealthWidget;
pub use gesture_recent::GestureRecentWidget;
pub use layouts::LayoutsWidget;
pub use notes_graph::NotesGraphWidget;
pub use notes_recent::NotesRecentWidget;
pub use notes_tags::NotesTagsWidget;
pub use now_playing::NowPlayingWidget;
pub use pinned_commands::PinnedCommandsWidget;
pub use pinned_query_results::PinnedQueryResultsWidget;
pub use plugin_home::PluginHomeWidget;
pub use process_list::ProcessesWidget;
pub use query_list::QueryListWidget;
pub use quick_tools::QuickToolsWidget;
pub use recent_commands::RecentCommandsWidget;
pub use recent_notes::RecentNotesWidget;
pub use recycle_bin::RecycleBinWidget;
pub use scratchpad::ScratchpadWidget;
pub use snippets_favorites::SnippetsFavoritesWidget;
pub use stopwatch::StopwatchWidget;
pub use system_actions::SystemWidget;
pub use system_controls::SystemControlsWidget;
pub use system_status::SystemStatusWidget;
pub use tempfiles::TempfilesWidget;
pub use todo::TodoWidget;
pub use todo_focus::TodoFocusWidget;
pub use volume::VolumeWidget;
pub use weather_site::WeatherSiteWidget;
pub use window_list::WindowsWidget;
pub use windows_overview::WindowsOverviewWidget;

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
    pub plugin_infos: Option<&'a [(String, String, Vec<String>)]>,
    pub plugin_commands: Option<&'a [Action]>,
    pub actions: Option<&'a [Action]>,
    pub usage: Option<&'a std::collections::HashMap<String, u32>>,
    pub default_location: Option<&'a str>,
    pub enabled_plugins: Option<&'a HashSet<String>>,
}

impl<'a> WidgetSettingsContext<'a> {
    pub fn empty() -> Self {
        Self {
            plugins: None,
            plugin_infos: None,
            plugin_commands: None,
            actions: None,
            usage: None,
            default_location: None,
            enabled_plugins: None,
        }
    }
}

/// Handler used to render widget settings.
pub type SettingsUiFn =
    fn(&mut egui::Ui, &mut Value, &WidgetSettingsContext<'_>) -> WidgetSettingsUiResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WidgetMetadata {
    pub name: String,
    pub has_settings_ui: bool,
}

/// Widget trait implemented by all dashboard widgets.
/// Checklist:
/// - Render reads from snapshots/caches only (no IO, no plugin queries, no locks with IO).
/// - Schedule heavy refresh work via timers/background jobs and swap in new snapshots.
/// - Keep render deterministic and fast; use diagnostics to flag slow refreshes.
pub trait Widget: Send {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction>;

    fn on_config_updated(&mut self, _settings: &Value) {}

    fn header_ui(
        &mut self,
        _ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
    ) -> Option<WidgetAction> {
        None
    }
}

/// Descriptor for building widgets from JSON settings.
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
            "command_history",
            WidgetFactory::new(CommandHistoryWidget::new)
                .with_settings_ui(CommandHistoryWidget::settings_ui),
        );
        reg.register("diagnostics", WidgetFactory::new(DiagnosticsWidget::new));
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
            WidgetFactory::new(RecentNotesWidget::new)
                .with_settings_ui(RecentNotesWidget::settings_ui),
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
            WidgetFactory::new(BrowserTabsWidget::new)
                .with_settings_ui(BrowserTabsWidget::settings_ui),
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
            WidgetFactory::new(NowPlayingWidget::new)
                .with_settings_ui(NowPlayingWidget::settings_ui),
        );
        reg.register(
            "snippets_favorites",
            WidgetFactory::new(SnippetsFavoritesWidget::new)
                .with_settings_ui(SnippetsFavoritesWidget::settings_ui),
        );
        reg.register(
            "scratchpad",
            WidgetFactory::new(ScratchpadWidget::new)
                .with_settings_ui(ScratchpadWidget::settings_ui),
        );
        reg.register(
            "notes_recent",
            WidgetFactory::new(NotesRecentWidget::new)
                .with_settings_ui(NotesRecentWidget::settings_ui),
        );
        reg.register(
            "notes_tags",
            WidgetFactory::new(NotesTagsWidget::new).with_settings_ui(NotesTagsWidget::settings_ui),
        );
        reg.register(
            "notes_graph",
            WidgetFactory::new(NotesGraphWidget::new)
                .with_settings_ui(NotesGraphWidget::settings_ui),
        );
        reg.register(
            "todo_focus",
            WidgetFactory::new(TodoFocusWidget::new).with_settings_ui(TodoFocusWidget::settings_ui),
        );
        reg.register(
            "quick_tools",
            WidgetFactory::new(QuickToolsWidget::new)
                .with_settings_ui(QuickToolsWidget::settings_ui),
        );
        reg.register(
            "recycle_bin",
            WidgetFactory::new(RecycleBinWidget::new)
                .with_settings_ui(RecycleBinWidget::settings_ui),
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

pub(crate) fn merge_json(base: &Value, updates: &Value) -> Value {
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

pub(crate) fn plugin_names(ctx: &WidgetSettingsContext<'_>) -> Vec<String> {
    if let Some(infos) = ctx.plugin_infos {
        let mut names: Vec<String> = infos
            .iter()
            .filter(|(name, _, _)| {
                ctx.enabled_plugins
                    .map(|set| set.contains(name))
                    .unwrap_or(true)
            })
            .map(|(name, _, _)| name.clone())
            .collect();
        names.sort();
        names.dedup();
        return names;
    }
    if let Some(manager) = ctx.plugins {
        let mut names = manager.plugin_names();
        if let Some(enabled) = ctx.enabled_plugins {
            names.retain(|name| enabled.contains(name));
        }
        names.sort();
        names
    } else {
        Vec::new()
    }
}

pub(crate) fn find_plugin<'a>(
    ctx: &'a DashboardContext<'a>,
    name: &str,
) -> Option<&'a dyn crate::plugin::Plugin> {
    ctx.plugins
        .iter()
        .find_map(|p| if p.name() == name { Some(&**p) } else { None })
}

fn collect_query_suggestions(out: &mut Vec<String>, actions: &[Action], prefixes: &[String]) {
    for action in actions {
        let label = action.label.trim();
        let label_lower = label.to_lowercase();
        if prefixes.iter().any(|p| label_lower.starts_with(p)) {
            if !out.iter().any(|s| s.eq_ignore_ascii_case(label)) {
                out.push(label.to_string());
            }
            continue;
        }
        if let Some(query) = action.action.strip_prefix("query:") {
            let q_lower = query.to_lowercase();
            if prefixes.iter().any(|p| q_lower.starts_with(p))
                && !out.iter().any(|s| s.eq_ignore_ascii_case(query))
            {
                out.push(query.to_string());
            }
        }
    }
}

pub(crate) fn query_suggestions(
    ctx: &WidgetSettingsContext<'_>,
    plugin_prefixes: &[&str],
    defaults: &[&str],
) -> Vec<String> {
    let mut out = Vec::new();
    let prefixes: Vec<String> = plugin_prefixes.iter().map(|p| p.to_lowercase()).collect();
    if let Some(cmds) = ctx.plugin_commands {
        collect_query_suggestions(&mut out, cmds, &prefixes);
    }
    if let Some(actions) = ctx.actions {
        collect_query_suggestions(&mut out, actions, &prefixes);
    }
    for def in defaults {
        if !out.iter().any(|s| s.eq_ignore_ascii_case(def)) {
            out.push(def.to_string());
        }
    }
    out
}

pub(crate) fn gesture_focus_action(
    label: &str,
    tokens: &str,
    dir_mode: DirMode,
    binding_idx: Option<usize>,
) -> WidgetAction {
    let args = GestureFocusArgs {
        label: label.to_string(),
        tokens: tokens.to_string(),
        dir_mode,
        binding_idx,
    };
    WidgetAction {
        action: Action {
            label: label.to_string(),
            desc: "Mouse gestures".into(),
            action: "mg:dialog:focus".into(),
            args: serde_json::to_string(&args).ok(),
        },
        query_override: None,
    }
}

pub(crate) fn gesture_toggle_action(
    label: &str,
    tokens: &str,
    dir_mode: DirMode,
    enabled: bool,
) -> WidgetAction {
    let args = GestureToggleArgs {
        label: label.to_string(),
        tokens: tokens.to_string(),
        dir_mode,
        enabled,
    };
    WidgetAction {
        action: Action {
            label: format!("Toggle {label}"),
            desc: "Mouse gestures".into(),
            action: "mg:toggle".into(),
            args: serde_json::to_string(&args).ok(),
        },
        query_override: None,
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

#[derive(Debug, Clone)]
pub struct TimedCache<T> {
    pub data: T,
    pub last_refresh: Instant,
    pub interval: Duration,
}

impl<T> TimedCache<T> {
    pub fn new(data: T, interval: Duration) -> Self {
        Self {
            data,
            last_refresh: Instant::now() - interval,
            interval,
        }
    }

    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.interval
    }

    pub fn refresh(&mut self, update: impl FnOnce(&mut T)) {
        update(&mut self.data);
        self.last_refresh = Instant::now();
    }

    pub fn touch(&mut self) {
        self.last_refresh = Instant::now();
    }

    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    pub fn invalidate(&mut self) {
        self.last_refresh = Instant::now() - self.interval;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    Auto,
    Manual,
    Throttled,
}

impl Default for RefreshMode {
    fn default() -> Self {
        RefreshMode::Auto
    }
}

impl std::fmt::Display for RefreshMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshMode::Auto => write!(f, "Auto"),
            RefreshMode::Manual => write!(f, "Manual"),
            RefreshMode::Throttled => write!(f, "Throttled"),
        }
    }
}

pub(crate) fn default_refresh_throttle_secs() -> f32 {
    5.0
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RefreshSchedule {
    pub interval: Duration,
    pub mode: RefreshMode,
    pub throttle: Duration,
}

impl RefreshSchedule {
    pub fn effective_interval(&self) -> Duration {
        match self.mode {
            RefreshMode::Throttled => self.interval.max(self.throttle),
            _ => self.interval,
        }
    }
}

pub(crate) fn refresh_schedule(
    interval: Duration,
    refresh_mode: RefreshMode,
    manual_refresh_only: bool,
    throttle_secs: f32,
) -> RefreshSchedule {
    let mode = if manual_refresh_only && refresh_mode == RefreshMode::Auto {
        RefreshMode::Manual
    } else {
        refresh_mode
    };
    RefreshSchedule {
        interval,
        mode,
        throttle: Duration::from_secs_f32(throttle_secs.max(0.0)),
    }
}

pub(crate) fn run_refresh_schedule(
    ctx: &DashboardContext<'_>,
    schedule: RefreshSchedule,
    refresh_pending: &mut bool,
    last_refresh: &mut Instant,
) -> bool {
    if ctx.reduce_dashboard_work_when_unfocused
        && (!ctx.dashboard_visible || !ctx.dashboard_focused)
    {
        *last_refresh = Instant::now();
        return false;
    }

    let elapsed = last_refresh.elapsed();
    let should_auto = match schedule.mode {
        RefreshMode::Auto => elapsed >= schedule.interval,
        RefreshMode::Manual => false,
        RefreshMode::Throttled => elapsed >= schedule.effective_interval(),
    };
    let should_refresh = *refresh_pending || should_auto;
    if !should_refresh {
        return false;
    }
    if schedule.mode == RefreshMode::Throttled && elapsed < schedule.throttle {
        return false;
    }
    *refresh_pending = false;
    true
}

pub(crate) fn refresh_settings_ui(
    ui: &mut egui::Ui,
    seconds: &mut f32,
    refresh_mode: &mut RefreshMode,
    throttle_secs: &mut f32,
    manual_refresh_only: Option<&mut bool>,
    tooltip: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Refresh every");
        let resp = ui
            .add(
                egui::DragValue::new(seconds)
                    .clamp_range(1.0..=300.0)
                    .speed(0.5),
            )
            .on_hover_text(tooltip);
        changed |= resp.changed();
        ui.label("seconds");
    });
    ui.horizontal(|ui| {
        ui.label("Refresh mode");
        let selected = refresh_mode.to_string();
        egui::ComboBox::from_id_source(ui.id().with("refresh_mode"))
            .selected_text(selected)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Auto, "Auto")
                    .changed();
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Manual, "Manual")
                    .changed();
                changed |= ui
                    .selectable_value(refresh_mode, RefreshMode::Throttled, "Throttled")
                    .changed();
            });
    });
    if *refresh_mode == RefreshMode::Throttled {
        ui.horizontal(|ui| {
            ui.label("Minimum interval");
            changed |= ui
                .add(
                    egui::DragValue::new(throttle_secs)
                        .clamp_range(1.0..=300.0)
                        .speed(0.5),
                )
                .changed();
            ui.label("seconds");
        });
    }
    if let Some(manual_refresh_only) = manual_refresh_only {
        if *manual_refresh_only && *refresh_mode == RefreshMode::Auto {
            *refresh_mode = RefreshMode::Manual;
            changed = true;
        }
        if *manual_refresh_only != (*refresh_mode == RefreshMode::Manual) {
            *manual_refresh_only = *refresh_mode == RefreshMode::Manual;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_json_preserves_unknown_fields() {
        let base = json!({"known": 1, "extra": {"keep": true}});
        let updates = json!({"known": 2});
        let merged = merge_json(&base, &updates);
        assert_eq!(merged["known"], json!(2));
        assert_eq!(merged["extra"], json!({"keep": true}));
    }

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
}
