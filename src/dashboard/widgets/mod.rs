use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugin::PluginManager;
use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;

mod active_timers;
mod browser_tabs;
mod calendar;
mod clipboard_recent;
mod clipboard_snippets;
mod command_history;
mod context_links;
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
mod query_suggestions;
mod quick_tools;
mod recent_commands;
mod recent_notes;
mod recycle_bin;
mod registry;
mod render;
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
pub use context_links::ContextLinksWidget;
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
pub use registry::{WidgetDescriptor, WidgetFactory, WidgetMetadata, WidgetRegistry};
pub use render::{RefreshMode, TimedCache};
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

#[derive(Debug, Clone)]
pub struct WidgetAction {
    pub action: Action,
    pub query_override: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct WidgetSettingsUiResult {
    pub changed: bool,
    pub error: Option<String>,
}

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

pub type SettingsUiFn =
    fn(&mut egui::Ui, &mut Value, &WidgetSettingsContext<'_>) -> WidgetSettingsUiResult;

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

pub(crate) use query_suggestions::query_suggestions;
pub(crate) use render::{
    default_refresh_throttle_secs, edit_typed_settings, find_plugin, gesture_focus_action,
    gesture_toggle_action, merge_json, plugin_names, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule,
};
