mod actions;
mod add_action_dialog;
mod add_bookmark_dialog;
mod alias_dialog;
mod bookmark_alias_dialog;
mod brightness_dialog;
mod calendar_event_details;
mod calendar_event_editor;
mod calendar_popover;
mod clipboard_dialog;
mod clipboard_modify_dialog;
mod confirmation_modal;
mod convert_panel;
mod cpu_list_dialog;
mod dashboard_editor_dialog;
mod fav_dialog;
mod file_search_dialog;
pub mod file_search_preview_dialog;
mod image_panel;
mod macro_dialog;
mod mouse_gesture_settings_dialog;
mod mouse_gestures_dialog;
mod multi_manager_actions;
mod note_graph_dialog;
mod note_panel;
mod notes_dialog;
mod render;
mod screenshot_editor;
mod search;
mod shell_cmd_dialog;
mod snippet_dialog;
mod state;
mod tempfile_alias_dialog;
mod tempfile_dialog;
mod theme_settings_dialog;
mod timer_dialog;
mod toast_log_dialog;
mod todo_dialog;
mod todo_view_dialog;
mod unused_assets_dialog;
pub(crate) mod volume_data;
mod volume_dialog;
mod watch;

pub use add_action_dialog::AddActionDialog;
pub use add_bookmark_dialog::AddBookmarkDialog;
pub use alias_dialog::AliasDialog;
pub use bookmark_alias_dialog::BookmarkAliasDialog;
pub use brightness_dialog::BRIGHTNESS_QUERIES;
pub use brightness_dialog::BrightnessDialog;
pub use calendar_event_details::CalendarEventDetails;
pub use calendar_event_editor::CalendarEventEditor;
pub use calendar_popover::CalendarPopover;
pub use clipboard_dialog::ClipboardDialog;

pub use clipboard_modify_dialog::{ClipboardModifyDialogSection, ClipboardModifyDialogState};
pub use convert_panel::ConvertPanel;
pub use cpu_list_dialog::CpuListDialog;
pub use fav_dialog::FavDialog;
pub use file_search_dialog::{
    FileSearchDialogState, FileSearchMode, FileSearchScopeMode, FileSearchUiCommand,
};
pub use file_search_preview_dialog::FileSearchPreviewDialogState;
pub use image_panel::ImagePanel;
pub use macro_dialog::MacroDialog;
pub use mouse_gesture_settings_dialog::MouseGestureSettingsDialog;
pub use mouse_gestures_dialog::{GestureRecorder, MgGesturesDialog, RecorderConfig};
pub use note_graph_dialog::NoteGraphDialog;
pub use note_panel::{
    NotePanel, build_nvim_command, build_wezterm_command, extract_links, show_wiki_link,
    spawn_external,
};
pub use notes_dialog::NotesDialog;
pub use screenshot_editor::{
    MarkupArrow, MarkupHistory, MarkupLayer, MarkupRect, MarkupStroke, MarkupText, MarkupTool,
    ScreenshotEditor, render_markup_layers,
};
pub use shell_cmd_dialog::ShellCmdDialog;
pub use snippet_dialog::SnippetDialog;
pub use tempfile_alias_dialog::TempfileAliasDialog;
pub use tempfile_dialog::TempfileDialog;
pub use theme_settings_dialog::ThemeSettingsDialogState;
pub use timer_dialog::{TimerCompletionDialog, TimerDialog};
pub use toast_log_dialog::ToastLogDialog;
pub use todo_dialog::TodoDialog;
pub use todo_view_dialog::{TodoViewDialog, todo_view_layout_sizes, todo_view_window_constraints};
pub use unused_assets_dialog::UnusedAssetsDialog;
pub use volume_dialog::VolumeDialog;

use crate::actions::folders;
use crate::actions::{Action, load_actions};
use crate::actions_editor::ActionsEditor;
use crate::clipboard_modify::coordinator::{
    ImmediateCompletionEvent, ImmediateExecutionCoordinator, ImmediateRequestMetadata,
};
use crate::clipboard_modify::runtime::{ClipboardModifyRuntime, clipboard_service};
use crate::common::query::{ActionFilterMetadata, action_matches_filters, split_action_filters};
use crate::dashboard::config::DashboardConfig;
use crate::dashboard::widgets::{WidgetRegistry, WidgetSettingsContext};
use crate::dashboard::{
    Dashboard, DashboardContext, DashboardDataCache, DashboardEvent, WidgetActivation,
};
use crate::file_search::coordinator::SearchCoordinator;
use crate::help_window::HelpWindow;
use crate::history::{self, HISTORY_PINS_FILE, HistoryEntry, HistoryPin};
use crate::indexer;
use crate::launcher::launch_action;
use crate::mouse_gestures::db::{GESTURES_FILE, load_gestures, save_gestures};
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use crate::multi_manager::state::MultiManagerState;
use crate::multi_manager::ui::{MultiManagerDialog, MultiManagerSettingsDialog};
use crate::plugin::{CAP_FORCE_LIST_RESULTS, CAP_GRID_RESULTS_COMPATIBLE, PluginManager};
use crate::plugin_editor::PluginEditor;
use crate::plugins::clipboard_modify::ClipboardModifyPluginSettings;
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::plugins::snippets::{SNIPPETS_FILE, remove_snippet};
use crate::settings::{MultiManagerSettings, NoteSettings, QueryResultsLayoutSettings, Settings};
use crate::settings_editor::SettingsEditor;
use crate::toast_log::{TOAST_LOG_FILE, append_toast_log};
use crate::usage::{self, USAGE_FILE};
use crate::visibility::apply_visibility;
use chrono::NaiveDate;
use confirmation_modal::{ConfirmationModal, ConfirmationResult, DestructiveAction};
use dashboard_editor_dialog::DashboardEditorDialog;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use fst::{IntoStreamer, Map, MapBuilder, Streamer};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
#[cfg(test)]
use search::{COMPLETION_REBUILD_DEBOUNCE, NOTE_SEARCH_DEBOUNCE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use url::Url;
use watch::watch_file;

pub use state::{ActivationSource, ClipboardModifyGuiEvent, TestWatchEvent, WatchEvent};
pub(crate) use state::{PendingConfirmAction, ResultContextMenuKind, UiErrorEvent};

const SUBCOMMANDS: &[&str] = &[
    "add", "rm", "list", "clear", "open", "new", "alias", "set", "pause", "resume", "cancel",
    "edit", "ma",
];

/// Prefix used to search user saved applications.
pub const APP_PREFIX: &str = "app";
fn scale_ui<R>(ui: &mut egui::Ui, scale: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        if (scale - 1.0).abs() > f32::EPSILON {
            let mut style: egui::Style = (*ui.ctx().style()).clone();
            style.spacing.item_spacing *= scale;
            style.spacing.interact_size *= scale;
            for font in style.text_styles.values_mut() {
                font.size *= scale;
            }
            ui.set_style(style);
        }
        add_contents(ui)
    })
    .inner
}

fn push_toast(toasts: &mut Toasts, toast: Toast) {
    append_toast_log(toast.text.text());
    toasts.add(toast);
}

fn normalize_static_window_config(
    follow_mouse: bool,
    static_location_enabled: bool,
    static_pos: Option<(i32, i32)>,
    static_size: Option<(i32, i32)>,
) -> (bool, Option<(i32, i32)>, Option<(i32, i32)>) {
    if follow_mouse {
        (false, None, None)
    } else {
        (static_location_enabled, static_pos, static_size)
    }
}

static APP_EVENT_TXS: Lazy<Mutex<Vec<Sender<WatchEvent>>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn register_event_sender(tx: Sender<WatchEvent>) {
    if let Ok(mut guard) = APP_EVENT_TXS.lock() {
        guard.push(tx);
    }
}

pub fn send_event(ev: WatchEvent) {
    if let Ok(mut guard) = APP_EVENT_TXS.lock() {
        guard.retain(|tx| tx.send(ev.clone()).is_ok());
    }
}

#[cfg(not(test))]
fn open_link(url: &str) -> std::io::Result<()> {
    open::that(url)
}

#[cfg(test)]
fn open_link(_url: &str) -> std::io::Result<()> {
    OPEN_LINK_COUNT.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[cfg(test)]
pub static OPEN_LINK_COUNT: AtomicUsize = AtomicUsize::new(0);

#[allow(dead_code)]
pub static EXECUTE_ACTION_COUNT: AtomicUsize = AtomicUsize::new(0);

static EXECUTE_ACTION_HOOK: Lazy<
    Mutex<Option<Box<dyn Fn(&Action) -> anyhow::Result<()> + Send + Sync>>>,
> = Lazy::new(|| Mutex::new(None));

pub fn set_execute_action_hook(
    hook: Option<Box<dyn Fn(&Action) -> anyhow::Result<()> + Send + Sync>>,
) {
    if let Ok(mut guard) = EXECUTE_ACTION_HOOK.lock() {
        *guard = hook;
    }
}

fn execute_action(action: &Action) -> anyhow::Result<()> {
    if let Ok(guard) = EXECUTE_ACTION_HOOK.lock()
        && let Some(ref hook) = *guard
    {
        return hook(action);
    }
    launch_action(action)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    AliasDialog,
    BookmarkAliasDialog,
    TempfileAliasDialog,
    TempfileDialog,
    AddBookmarkDialog,
    HelpOverlay,
    HelpWindow,
    TimerDialog,
    CompletionDialog,
    ShellCmdDialog,
    SnippetDialog,
    MacroDialog,
    MouseGesturesDialog,
    MouseGestureSettingsDialog,
    ThemeSettingsDialog,
    FavDialog,
    FileSearchDialog,
    NotesDialog,
    NoteGraphDialog,
    UnusedAssetsDialog,
    NotePanel,
    ImagePanel,
    ScreenshotEditor,
    TodoDialog,
    TodoViewDialog,
    ClipboardDialog,
    ClipboardModifyDialog,
    ConvertPanel,
    VolumeDialog,
    BrightnessDialog,
    CpuListDialog,
    ToastLogDialog,
    CalendarPopover,
    CalendarEventEditor,
    CalendarEventDetails,
    Editor,
    Settings,
    Plugins,
    MultiManagerDialog,
    MultiManagerSettingsDialog,
}

#[derive(Default)]
struct PanelStates {
    alias_dialog: bool,
    bookmark_alias_dialog: bool,
    tempfile_alias_dialog: bool,
    tempfile_dialog: bool,
    add_bookmark_dialog: bool,
    help_overlay: bool,
    help_window: bool,
    timer_dialog: bool,
    completion_dialog: bool,
    shell_cmd_dialog: bool,
    snippet_dialog: bool,
    macro_dialog: bool,
    mouse_gestures_dialog: bool,
    mouse_gesture_settings_dialog: bool,
    theme_settings_dialog: bool,
    fav_dialog: bool,
    file_search_dialog: bool,
    notes_dialog: bool,
    note_graph_dialog: bool,
    unused_assets_dialog: bool,
    note_panel: bool,
    image_panel: bool,
    screenshot_editor: bool,
    todo_dialog: bool,
    todo_view_dialog: bool,
    clipboard_dialog: bool,
    clipboard_modify_dialog: bool,
    convert_panel: bool,
    volume_dialog: bool,
    brightness_dialog: bool,
    cpu_list_dialog: bool,
    toast_log_dialog: bool,
    calendar_popover: bool,
    calendar_event_editor: bool,
    calendar_event_details: bool,
    editor: bool,
    settings: bool,
    plugins: bool,
    multi_manager_dialog: bool,
    multi_manager_settings_dialog: bool,
}

/// Primary GUI state for Multi Launcher.
///
/// The application may create multiple windows or helper threads. To keep the
/// available actions consistent across those components, `LauncherApp` holds
/// them in an [`Arc<Vec<Action>>`]. Cloning the `Arc` only replicates the
/// pointer, allowing cheap, thread-safe sharing without duplicating the vector
/// itself.
pub struct LauncherApp {
    /// Shared list of all actions available to the launcher.
    ///
    /// The list is wrapped in an [`Arc`] so windows or background tasks can
    /// access it without cloning the underlying `Vec`. Cloning this field only
    /// duplicates the pointer, keeping the action data itself shared. When
    /// actions are edited the entire `Arc` is replaced with a new one.
    pub actions: Arc<Vec<Action>>,
    action_cache: Vec<CachedSearchEntry>,
    action_filter_metadata: Vec<ActionFilterMetadata>,
    actions_by_id: HashMap<String, Action>,
    command_cache: Vec<Action>,
    command_search_cache: Vec<CachedSearchEntry>,
    completion_index: Option<Map<Vec<u8>>>,
    action_completion_dirty: bool,
    command_completion_dirty: bool,
    completion_rebuild_after: Option<Instant>,
    suggestions: Vec<String>,
    autocomplete_index: usize,
    pub query: String,
    pub results: Vec<Action>,
    pub matcher: SkimMatcherV2,
    pub error: Option<String>,
    error_time: Option<Instant>,
    pub plugins: PluginManager,
    pub selected: Option<usize>,
    pub usage: HashMap<String, u32>,
    pub registered_hotkeys: Mutex<HashMap<String, usize>>,
    pub show_editor: bool,
    pub show_settings: bool,
    pub show_plugins: bool,
    pub actions_path: String,
    pub editor: ActionsEditor,
    pub settings_editor: SettingsEditor,
    pub plugin_editor: PluginEditor,
    pub settings_path: String,
    pub multi_manager: MultiManagerState,
    pub multi_manager_settings: MultiManagerSettings,
    pub launcher_hwnd: Option<usize>,
    pub multi_manager_dialog: MultiManagerDialog,
    pub multi_manager_settings_dialog: MultiManagerSettingsDialog,
    /// Hold watchers so the `RecommendedWatcher` instances remain active.
    #[allow(dead_code)] // required to keep watchers alive
    watchers: Vec<RecommendedWatcher>,
    pub dashboard: Dashboard,
    dashboard_data_cache: DashboardDataCache,
    pub dashboard_enabled: bool,
    pub dashboard_show_when_empty: bool,
    pub dashboard_path: String,
    pub dashboard_default_location: Option<String>,
    pub reduce_dashboard_work_when_unfocused: bool,
    pub show_dashboard_diagnostics: bool,
    pub dashboard_editor: DashboardEditorDialog,
    pub show_dashboard_editor: bool,
    rx: Receiver<WatchEvent>,
    folder_aliases: HashMap<String, Option<String>>,
    folder_aliases_lc: HashMap<String, Option<String>>,
    bookmark_aliases: HashMap<String, Option<String>>,
    bookmark_aliases_lc: HashMap<String, Option<String>>,
    plugin_dirs: Option<Vec<String>>,
    index_paths: Option<Vec<String>>,
    max_indexed_items: Option<usize>,
    enabled_plugins: Option<HashSet<String>>,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    visible_flag: Arc<AtomicBool>,
    restore_flag: Arc<AtomicBool>,
    last_visible: bool,
    offscreen_pos: (f32, f32),
    pub window_size: (i32, i32),
    pub window_pos: (i32, i32),
    focus_query: bool,
    move_cursor_end: bool,
    toasts: egui_toast::Toasts,
    pub enable_toasts: bool,
    pub show_inline_errors: bool,
    pub show_error_toasts: bool,
    pub toast_duration: f32,
    alias_dialog: AliasDialog,
    bookmark_alias_dialog: BookmarkAliasDialog,
    tempfile_alias_dialog: TempfileAliasDialog,
    tempfile_dialog: TempfileDialog,
    add_bookmark_dialog: AddBookmarkDialog,
    help_window: crate::help_window::HelpWindow,
    timer_dialog: TimerDialog,
    completion_dialog: TimerCompletionDialog,
    shell_cmd_dialog: ShellCmdDialog,
    snippet_dialog: SnippetDialog,
    macro_dialog: MacroDialog,
    mouse_gestures_dialog: MgGesturesDialog,
    mouse_gesture_settings_dialog: MouseGestureSettingsDialog,
    theme_settings_dialog_open: bool,
    theme_settings_dialog: ThemeSettingsDialogState,
    fav_dialog: FavDialog,
    file_search_dialog: FileSearchDialogState,
    file_search_coordinator: SearchCoordinator,
    notes_dialog: NotesDialog,
    note_graph_dialog: NoteGraphDialog,
    unused_assets_dialog: UnusedAssetsDialog,
    note_panels: Vec<NotePanel>,
    image_panels: Vec<ImagePanel>,
    screenshot_editors: Vec<ScreenshotEditor>,
    todo_dialog: TodoDialog,
    todo_view_dialog: TodoViewDialog,
    clipboard_dialog: ClipboardDialog,
    pub clipboard_modify_runtime: ClipboardModifyRuntime,
    pub clipboard_modify_dialog: ClipboardModifyDialogState,
    pub clipboard_modify_config_diagnostic: Option<String>,
    pending_clipboard_modify_immediate: HashMap<u64, ImmediateRequestMetadata>,
    clipboard_modify_immediate: ImmediateExecutionCoordinator<
        crate::clipboard_modify::clipboard::ProductionClipboardService,
    >,
    clipboard_modify_events: Vec<ImmediateCompletionEvent>,
    convert_panel: ConvertPanel,
    volume_dialog: VolumeDialog,
    brightness_dialog: BrightnessDialog,
    cpu_list_dialog: CpuListDialog,
    toast_log_dialog: ToastLogDialog,
    calendar_popover: CalendarPopover,
    calendar_event_editor: CalendarEventEditor,
    calendar_event_details: CalendarEventDetails,
    panel_stack: Vec<Panel>,
    panel_states: PanelStates,
    pinned_panels: Vec<Panel>,
    pub help_flag: Arc<AtomicBool>,
    pub hotkey_str: Option<String>,
    pub quit_hotkey_str: Option<String>,
    pub help_hotkey_str: Option<String>,
    pub query_scale: f32,
    pub list_scale: f32,
    /// Number of user defined commands at the start of `actions`.
    pub custom_len: usize,
    pub history_limit: usize,
    pub clipboard_limit: usize,
    pub fuzzy_weight: f32,
    pub usage_weight: f32,
    pub match_exact: bool,
    pub page_jump: usize,
    pub query_results_layout: QueryResultsLayoutSettings,
    resolved_grid_layout: bool,
    pub note_settings: NoteSettings,
    pub note_panel_default_size: (f32, f32),
    pub note_save_on_close: bool,
    pub note_always_overwrite: bool,
    pub note_images_as_links: bool,
    pub note_show_details: bool,
    pub note_external_open: NoteExternalOpen,
    pub note_font_size: f32,
    pub note_more_limit: usize,
    pub follow_mouse: bool,
    pub static_location_enabled: bool,
    pub static_pos: Option<(i32, i32)>,
    pub static_size: Option<(i32, i32)>,
    pub hide_after_run: bool,
    pub always_on_top: bool,
    pub timer_refresh: f32,
    pub disable_timer_updates: bool,
    pub preserve_command: bool,
    pub clear_query_after_run: bool,
    pub require_confirm_destructive: bool,
    pub query_autocomplete: bool,
    pub net_refresh: f32,
    pub net_unit: crate::settings::NetUnit,
    pub screenshot_dir: Option<String>,
    pub screenshot_save_file: bool,
    pub screenshot_auto_save: bool,
    pub screenshot_use_editor: bool,
    pub calendar_popover_open: bool,
    pub calendar_editor_open: bool,
    pub calendar_details_open: bool,
    pub calendar_selected_date: Option<NaiveDate>,
    pub calendar_selected_event: Option<String>,
    last_timer_update: Instant,
    last_net_update: Instant,
    last_stopwatch_update: Instant,
    last_search_query: String,
    last_results_valid: bool,
    last_timer_query: bool,
    last_stopwatch_query: bool,
    last_note_search_change: Option<Instant>,
    pending_query: Option<String>,
    confirm_modal: ConfirmationModal,
    pending_confirm: Option<PendingConfirmAction>,
    pub vim_mode: bool,
    pub file_search_window_open: bool,
    pub file_search_selected_kind: crate::file_search::model::SearchKind,
    pub file_search_root: Option<std::path::PathBuf>,
    pub file_search_text: String,
    pub file_search_active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CachedSearchEntry {
    label_lc: String,
    desc_lc: String,
    action_lc: String,
}

impl CachedSearchEntry {
    fn from_action(action: &Action) -> Self {
        Self {
            label_lc: action.label.to_lowercase(),
            desc_lc: action.desc.to_lowercase(),
            action_lc: action.action.to_lowercase(),
        }
    }
}

impl LauncherApp {
    pub fn plugin_enabled(&self, name: &str) -> bool {
        match &self.enabled_plugins {
            Some(set) => set.contains(name),
            None => true,
        }
    }

    pub fn enabled_plugins_list(&self) -> Option<Vec<String>> {
        self.enabled_plugins
            .as_ref()
            .map(|set| set.iter().cloned().collect())
    }
    pub fn add_toast(&mut self, toast: Toast) {
        push_toast(&mut self.toasts, toast);
    }

    pub fn add_error_toast(&mut self, msg: impl Into<String>) {
        if self.enable_toasts && self.show_error_toasts {
            self.add_toast(Toast {
                text: msg.into().into(),
                kind: ToastKind::Error,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
            });
        }
    }

    fn set_inline_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.error_time = Some(Instant::now());
    }

    pub fn report_error(&mut self, context: &'static str, err: impl Display) {
        self.report_error_message(context, err.to_string());
    }

    pub fn report_ui_error(&mut self, err: UiErrorEvent) {
        self.report_error_message(err.context, err.message);
    }

    pub fn report_error_message(&mut self, context: &'static str, msg: impl Into<String>) {
        let msg = msg.into();
        tracing::error!(context, error = %msg);
        append_toast_log(&format!("[error:{context}] {msg}"));
        if self.show_inline_errors {
            self.set_inline_error(msg.clone());
        }
        if self.enable_toasts && self.show_error_toasts {
            self.add_toast(Toast {
                text: msg.into(),
                kind: ToastKind::Error,
                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
            });
        }
    }

    fn should_render_inline_error(&self) -> bool {
        self.show_inline_errors && self.error.is_some()
    }

    fn open_settings_dialog(&mut self) {
        if !self.show_settings {
            match Settings::load(&self.settings_path) {
                Ok(settings) => {
                    self.settings_editor = SettingsEditor::new_with_plugins(&settings);
                }
                Err(e) => {
                    let msg = format!("Failed to load settings: {e}");
                    self.report_error_message("settings_dialog.load", msg);
                }
            }
        }
        self.show_settings = true;
    }

    /// Opens the standalone Mouse Gesture Settings dialog.
    ///
    /// This is exposed so non-`gui` modules (e.g. `settings_editor`) can
    /// trigger it without reaching into private `LauncherApp` fields.
    pub fn open_mouse_gesture_settings_dialog(&mut self) {
        self.mouse_gesture_settings_dialog.open();
    }

    pub fn open_clipboard_modify_dialog(&mut self) {
        self.clipboard_modify_dialog.open_section(
            ClipboardModifyDialogSection::Modify,
            &crate::clipboard_modify::runtime::clipboard_service(),
        );
    }

    pub fn open_clipboard_modify_config_file(&mut self) {
        let path = self.clipboard_modify_runtime.store.path.clone();
        if !path.exists() {
            let model = crate::clipboard_modify::config::default_model();
            if let Err(err) = crate::clipboard_modify::config::save_model_atomic(&path, &model) {
                self.report_error_message(
                    "clipboard_modify.config.create",
                    format!("Failed to create Clipboard Modify config: {err}"),
                );
                return;
            }
        }
        if let Err(err) = open::that(&path) {
            self.report_error_message(
                "clipboard_modify.config.open",
                format!("Failed to open {}: {err}", path.display()),
            );
        }
    }

    pub fn reload_clipboard_modify_config(&mut self) {
        match self.clipboard_modify_runtime.reload_now() {
            Ok(catalog) => {
                self.refresh_clipboard_modify_catalog(catalog, true);
                self.handle_clipboard_modify_gui_event(
                    ClipboardModifyGuiEvent::ConfigurationReloadSuccess,
                );
            }
            Err(err) => {
                self.handle_clipboard_modify_gui_event(
                    ClipboardModifyGuiEvent::ConfigurationReloadFailure(err.to_string()),
                );
            }
        }
    }

    pub fn reset_clipboard_modify_config_to_factory_defaults(&mut self) {
        match self.clipboard_modify_runtime.reset_to_factory_defaults() {
            Ok(catalog) => self.refresh_clipboard_modify_catalog(catalog, true),
            Err(err) => self.report_error_message("clipboard_modify.config.reset", err.to_string()),
        }
    }

    pub fn apply_file_search_settings(
        &mut self,
        mut settings: crate::file_search::settings::FileSearchSettings,
    ) {
        if self.file_search_dialog.ui_preferences_dirty {
            settings.ui_preferences = self.file_search_dialog.ui_preferences.clone();
        }
        for diagnostic in settings.validate() {
            tracing::warn!(%diagnostic, "file_search settings warning during runtime apply");
        }
        self.file_search_dialog
            .cancel_search(&mut self.file_search_coordinator);
        self.file_search_coordinator
            .reconfigure_from_settings(settings.clone());
        self.file_search_dialog.case_sensitive = settings.case_sensitive;
        self.file_search_dialog.include_hidden = settings.include_hidden_files;
        self.file_search_dialog
            .set_ui_preferences(settings.ui_preferences.clone());
        self.file_search_dialog.settings = settings;
    }

    pub(crate) fn save_file_search_ui_preferences_if_dirty(&mut self) {
        if self.file_search_dialog.ui_preferences_dirty {
            self.handle_file_search_ui_command(FileSearchUiCommand::PersistPreferences(
                self.file_search_dialog.ui_preferences.clone(),
            ));
        }
    }

    pub(crate) fn handle_file_search_ui_command(&mut self, command: FileSearchUiCommand) {
        match command {
            FileSearchUiCommand::PersistPreferences(preferences) => {
                self.file_search_dialog.settings.ui_preferences = preferences.clone();
                self.file_search_dialog.set_ui_preferences(preferences);
                match crate::settings::Settings::load(&self.settings_path) {
                    Ok(mut settings) => {
                        let mut cfg = settings
                            .plugin_settings
                            .get("file_search")
                            .and_then(|value| serde_json::from_value(value.clone()).ok())
                            .unwrap_or_else(
                                crate::file_search::settings::FileSearchSettings::default,
                            );
                        cfg.ui_preferences = self.file_search_dialog.ui_preferences.clone();
                        if let Ok(value) = serde_json::to_value(&cfg) {
                            settings
                                .plugin_settings
                                .insert("file_search".to_owned(), value.clone());
                            self.settings_editor
                                .set_plugin_setting_value("file_search", value);
                            if let Err(error) = settings.save(&self.settings_path) {
                                self.report_error_message(
                                    "file_search.preferences.save",
                                    format!("Failed to save file-search preferences: {error}"),
                                );
                            }
                        }
                    }
                    Err(error) => self.report_error_message(
                        "file_search.preferences.load",
                        format!("Failed to load settings: {error}"),
                    ),
                }
            }
            FileSearchUiCommand::ConfigureRipgrep(path) => {
                match crate::settings::Settings::load(&self.settings_path) {
                    Ok(mut settings) => {
                        let mut cfg = settings
                            .plugin_settings
                            .get("file_search")
                            .and_then(|value| serde_json::from_value(value.clone()).ok())
                            .unwrap_or_else(
                                crate::file_search::settings::FileSearchSettings::default,
                            );
                        cfg.ripgrep_executable_path = path;
                        if let Ok(value) = serde_json::to_value(&cfg) {
                            settings
                                .plugin_settings
                                .insert("file_search".to_owned(), value.clone());
                            self.settings_editor
                                .set_plugin_setting_value("file_search", value);
                            match settings.save(&self.settings_path) {
                                Ok(()) => {
                                    self.apply_file_search_settings(cfg);
                                    self.file_search_dialog.warning_error_message =
                                        Some("ripgrep path saved for future searches.".to_owned());
                                }
                                Err(error) => self.report_error_message(
                                    "file_search.ripgrep.save",
                                    format!("Failed to save ripgrep path: {error}"),
                                ),
                            }
                        }
                    }
                    Err(error) => self.report_error_message(
                        "file_search.ripgrep.load",
                        format!("Failed to load settings: {error}"),
                    ),
                }
            }
        }
    }

    pub(crate) fn merge_file_search_ui_preferences_into_settings(
        &mut self,
        settings: &mut crate::settings::Settings,
    ) {
        self.file_search_dialog.save_dirty_ui_preferences();
        if let Ok(value) = serde_json::to_value(&self.file_search_dialog.settings) {
            settings
                .plugin_settings
                .insert("file_search".to_owned(), value);
        }
    }

    pub fn open_theme_settings_dialog(&mut self) {
        self.theme_settings_dialog_open = true;
        self.theme_settings_dialog.request_reload();
    }

    pub fn is_theme_settings_dialog_open(&self) -> bool {
        self.theme_settings_dialog_open
    }

    pub fn close_theme_settings_dialog(&mut self) {
        self.theme_settings_dialog_open = false;
        self.update_panel_stack();
    }
    pub fn update_paths(
        &mut self,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        offscreen_pos: Option<(i32, i32)>,
        enable_toasts: Option<bool>,
        show_inline_errors: Option<bool>,
        show_error_toasts: Option<bool>,
        toast_duration: Option<f32>,
        fuzzy_weight: Option<f32>,
        usage_weight: Option<f32>,
        match_exact: Option<bool>,
        follow_mouse: Option<bool>,
        static_enabled: Option<bool>,
        static_pos: Option<(i32, i32)>,
        static_size: Option<(i32, i32)>,
        hide_after_run: Option<bool>,
        clear_query_after_run: Option<bool>,
        require_confirm_destructive: Option<bool>,
        timer_refresh: Option<f32>,
        disable_timer_updates: Option<bool>,
        preserve_command: Option<bool>,
        query_autocomplete: Option<bool>,
        net_refresh: Option<f32>,
        net_unit: Option<crate::settings::NetUnit>,
        screenshot_dir: Option<String>,
        screenshot_save_file: Option<bool>,
        screenshot_use_editor: Option<bool>,
        screenshot_auto_save: Option<bool>,
        always_on_top: Option<bool>,
        page_jump: Option<usize>,
        note_settings: Option<NoteSettings>,
        note_panel_default_size: Option<(f32, f32)>,
        note_save_on_close: Option<bool>,
        note_always_overwrite: Option<bool>,
        note_images_as_links: Option<bool>,
        note_show_details: Option<bool>,
        note_more_limit: Option<usize>,
        show_dashboard_diagnostics: Option<bool>,
    ) {
        self.plugin_dirs = plugin_dirs;
        self.index_paths = index_paths;
        self.enabled_plugins = enabled_plugins;

        // Keep MG hook in lockstep with whether the plugin is enabled in the UI/settings.
        crate::plugins::mouse_gestures::sync_enabled_plugins(self.enabled_plugins.as_ref());
        self.update_command_cache();
        self.enabled_capabilities = enabled_capabilities;
        if let Some((x, y)) = offscreen_pos {
            self.offscreen_pos = (x as f32, y as f32);
        }
        if let Some(v) = enable_toasts {
            self.enable_toasts = v;
        }
        if let Some(v) = show_inline_errors {
            self.show_inline_errors = v;
        }
        if let Some(v) = show_error_toasts {
            self.show_error_toasts = v;
        }
        if let Some(v) = toast_duration {
            self.toast_duration = v;
        }
        if let Some(v) = fuzzy_weight {
            self.fuzzy_weight = v;
        }
        if let Some(v) = usage_weight {
            self.usage_weight = v;
        }
        if let Some(v) = match_exact {
            self.match_exact = v;
        }
        if let Some(v) = follow_mouse {
            self.follow_mouse = v;
        }
        let requested_static_enabled = static_enabled.unwrap_or(self.static_location_enabled);
        let requested_static_pos = static_pos.or(self.static_pos);
        let requested_static_size = static_size.or(self.static_size);
        let (normalized_static_enabled, normalized_static_pos, normalized_static_size) =
            normalize_static_window_config(
                self.follow_mouse,
                requested_static_enabled,
                requested_static_pos,
                requested_static_size,
            );
        self.static_location_enabled = normalized_static_enabled;
        self.static_pos = normalized_static_pos;
        self.static_size = normalized_static_size;
        if let Some(v) = hide_after_run {
            self.hide_after_run = v;
        }
        if let Some(v) = clear_query_after_run {
            self.clear_query_after_run = v;
        }
        if let Some(v) = require_confirm_destructive {
            self.require_confirm_destructive = v;
        }
        if let Some(v) = timer_refresh {
            self.timer_refresh = v;
        }
        if let Some(v) = disable_timer_updates {
            self.disable_timer_updates = v;
        }
        if let Some(v) = preserve_command {
            self.preserve_command = v;
        }
        if let Some(v) = query_autocomplete {
            self.query_autocomplete = v;
            if !v {
                self.suggestions.clear();
            } else {
                self.update_suggestions();
            }
        }
        if let Some(v) = net_refresh {
            self.net_refresh = v;
        }
        if let Some(v) = net_unit {
            self.net_unit = v;
        }
        if screenshot_dir.is_some() {
            self.screenshot_dir = screenshot_dir;
        }
        if let Some(v) = screenshot_save_file {
            self.screenshot_save_file = v;
        }
        if let Some(v) = screenshot_use_editor {
            self.screenshot_use_editor = v;
        }
        if let Some(v) = screenshot_auto_save {
            self.screenshot_auto_save = v;
        }
        if let Some(v) = always_on_top {
            self.always_on_top = v;
        }
        if let Some(v) = page_jump {
            self.page_jump = v;
        }
        if let Some(v) = note_settings {
            self.note_settings = v;
        }
        if let Some(v) = note_panel_default_size {
            self.note_panel_default_size = v;
        }
        if let Some(v) = note_save_on_close {
            self.note_save_on_close = v;
        }
        if let Some(v) = note_always_overwrite {
            self.note_always_overwrite = v;
        }
        if let Some(v) = note_images_as_links {
            self.note_images_as_links = v;
        }
        if let Some(v) = note_show_details {
            self.note_show_details = v;
        }
        if let Some(v) = note_more_limit {
            self.note_more_limit = v;
        }
        if let Some(v) = show_dashboard_diagnostics {
            self.show_dashboard_diagnostics = v;
        }
        self.recompute_query_results_layout();
        crate::plugins::mouse_gestures::sync_enabled_plugins(self.enabled_plugins.as_ref());
    }

    pub fn new(
        ctx: &egui::Context,
        actions: Arc<Vec<Action>>,
        custom_len: usize,
        plugins: PluginManager,
        actions_path: String,
        settings_path: String,
        settings: Settings,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        visible_flag: Arc<AtomicBool>,
        restore_flag: Arc<AtomicBool>,
        help_flag: Arc<AtomicBool>,
    ) -> Self {
        let (tx, rx) = channel();
        register_event_sender(tx.clone());
        let mut watchers = Vec::new();
        let mut toasts = Toasts::new().anchor(egui::Align2::RIGHT_TOP, [10.0, 10.0]);
        let enable_toasts = settings.enable_toasts;
        let show_inline_errors = settings.show_inline_errors;
        let show_error_toasts = settings.show_error_toasts;
        let toast_duration = settings.toast_duration;
        use std::path::Path;

        let dashboard_path = DashboardConfig::path_for(
            settings
                .dashboard
                .config_path
                .as_deref()
                .unwrap_or("dashboard.json"),
        );
        let dashboard_registry = WidgetRegistry::with_defaults();
        let dashboard_event_cb = std::sync::Arc::new({
            let tx = tx.clone();
            move |ev: DashboardEvent| {
                let _ = tx.send(WatchEvent::Dashboard(ev));
            }
        });
        let mut dashboard = Dashboard::new(
            &dashboard_path,
            dashboard_registry.clone(),
            Some(dashboard_event_cb),
        );
        dashboard.attach_watcher();

        let (folder_aliases, folder_aliases_lc) = Self::folder_alias_maps();
        let (bookmark_aliases, bookmark_aliases_lc) = Self::bookmark_alias_maps();
        let clipboard_modify_catalog = plugins.clipboard_modifier_catalog();
        let (clipboard_modify_runtime, loaded_clipboard_modify) =
            ClipboardModifyRuntime::new(Path::new(&settings_path), clipboard_modify_catalog);
        let clipboard_modify_config_diagnostic = loaded_clipboard_modify.state.startup_diagnostic();

        #[cfg(not(test))]
        match watch_file(Path::new(&actions_path), tx.clone(), WatchEvent::Actions) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: format!("Failed to watch {}", actions_path).into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            if Path::new(&actions_path).exists() {
                if let Ok(w) = watch_file(Path::new(&actions_path), tx.clone(), WatchEvent::Actions)
                {
                    watchers.push(w);
                }
            } else {
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: format!("Failed to watch {}", actions_path).into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        #[cfg(not(test))]
        match watch_file(
            Path::new(crate::plugins::folders::FOLDERS_FILE),
            tx.clone(),
            WatchEvent::Folders,
        ) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch folders.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            let path = Path::new(crate::plugins::folders::FOLDERS_FILE);
            if path.exists() {
                if let Ok(w) = watch_file(path, tx.clone(), WatchEvent::Folders) {
                    watchers.push(w);
                }
            } else {
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch folders.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        #[cfg(not(test))]
        match watch_file(
            Path::new(crate::plugins::bookmarks::BOOKMARKS_FILE),
            tx.clone(),
            WatchEvent::Bookmarks,
        ) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch bookmarks.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        #[cfg(test)]
        {
            let path = Path::new(crate::plugins::bookmarks::BOOKMARKS_FILE);
            if path.exists() {
                if let Ok(w) = watch_file(path, tx.clone(), WatchEvent::Bookmarks) {
                    watchers.push(w);
                }
            } else {
                if enable_toasts && show_error_toasts {
                    push_toast(
                        &mut toasts,
                        Toast {
                            text: "Failed to watch bookmarks.json".into(),
                            kind: ToastKind::Error,
                            options: ToastOptions::default()
                                .duration_in_seconds(toast_duration as f64),
                        },
                    );
                }
            }
        }

        let notes_dir = crate::plugins::note::notes_dir();
        let _ = std::fs::create_dir_all(&notes_dir);

        #[cfg(not(test))]
        for (path, event) in [
            (
                Path::new(crate::plugins::clipboard::CLIPBOARD_FILE),
                WatchEvent::Clipboard,
            ),
            (
                Path::new(crate::plugins::snippets::SNIPPETS_FILE),
                WatchEvent::Snippets,
            ),
            (
                Path::new(crate::plugins::todo::TODO_FILE),
                WatchEvent::Todos,
            ),
            (
                Path::new(crate::plugins::fav::FAV_FILE),
                WatchEvent::Favorites,
            ),
            (notes_dir.as_path(), WatchEvent::Notes),
            (
                Path::new(crate::mouse_gestures::db::GESTURES_FILE),
                WatchEvent::Gestures,
            ),
            (
                Path::new(crate::mouse_gestures::usage::GESTURES_USAGE_FILE),
                WatchEvent::Gestures,
            ),
        ] {
            match watch_file(path, tx.clone(), event) {
                Ok(w) => watchers.push(w),
                Err(e) => tracing::error!("watch error: {:?}", e),
            }
        }

        #[cfg(test)]
        for (path, event) in [
            (
                Path::new(crate::plugins::clipboard::CLIPBOARD_FILE),
                WatchEvent::Clipboard,
            ),
            (
                Path::new(crate::plugins::snippets::SNIPPETS_FILE),
                WatchEvent::Snippets,
            ),
            (
                Path::new(crate::plugins::todo::TODO_FILE),
                WatchEvent::Todos,
            ),
            (
                Path::new(crate::plugins::fav::FAV_FILE),
                WatchEvent::Favorites,
            ),
            (notes_dir.as_path(), WatchEvent::Notes),
            (
                Path::new(crate::mouse_gestures::db::GESTURES_FILE),
                WatchEvent::Gestures,
            ),
            (
                Path::new(crate::mouse_gestures::usage::GESTURES_USAGE_FILE),
                WatchEvent::Gestures,
            ),
        ] {
            if path.exists() {
                if let Ok(w) = watch_file(path, tx.clone(), event) {
                    watchers.push(w);
                }
            }
        }

        let initial_visible = visible_flag.load(Ordering::SeqCst);

        let offscreen_pos = {
            let (x, y) = settings.offscreen_pos.unwrap_or((2000, 2000));
            (x as f32, y as f32)
        };
        let win_size = settings.window_size.unwrap_or((400, 220));
        let query_scale = settings.query_scale.unwrap_or(1.0).min(5.0);
        let list_scale = settings.list_scale.unwrap_or(1.0).min(5.0);
        let static_pos = settings.static_pos;
        let static_size = settings.static_size;
        let follow_mouse = settings.follow_mouse;
        let (static_enabled, static_pos, static_size) = normalize_static_window_config(
            follow_mouse,
            settings.static_location_enabled,
            static_pos,
            static_size,
        );

        let note_external_open = settings
            .plugin_settings
            .get("note")
            .and_then(|v| serde_json::from_value::<NotePluginSettings>(v.clone()).ok())
            .map(|s| s.external_open)
            .unwrap_or(NoteExternalOpen::Wezterm);

        let file_search_settings = settings
            .plugin_settings
            .get("file_search")
            .and_then(|v| {
                serde_json::from_value::<crate::file_search::settings::FileSearchSettings>(
                    v.clone(),
                )
                .ok()
            })
            .unwrap_or_default();

        let clipboard_modify_settings = settings
            .plugin_settings
            .get("clipboard_modify")
            .and_then(|v| serde_json::from_value::<ClipboardModifyPluginSettings>(v.clone()).ok())
            .unwrap_or_default();

        let settings_editor = SettingsEditor::new_with_plugins(&settings);
        let multi_manager =
            MultiManagerState::load_or_default(&settings.multi_manager, &settings_path);
        let plugin_editor = PluginEditor::new(&settings);
        let actions_by_id = actions
            .iter()
            .map(|a| (a.action.clone(), a.clone()))
            .collect::<HashMap<_, _>>();
        let dashboard_data_cache = DashboardDataCache::new();
        dashboard_data_cache.refresh_all(&plugins);
        let mut app = Self {
            actions: Arc::clone(&actions),
            query: String::new(),
            results: (*actions).clone(),
            matcher: SkimMatcherV2::default(),
            error: None,
            error_time: None,
            plugins,
            selected: None,
            usage: usage::load_usage(USAGE_FILE).unwrap_or_default(),
            registered_hotkeys: Mutex::new(HashMap::new()),
            show_editor: false,
            show_settings: false,
            show_plugins: false,
            actions_path,
            editor: ActionsEditor::default(),
            settings_editor,
            plugin_editor,
            settings_path,
            multi_manager,
            multi_manager_settings: settings.multi_manager.clone(),
            launcher_hwnd: None,
            multi_manager_dialog: MultiManagerDialog::default(),
            multi_manager_settings_dialog: MultiManagerSettingsDialog::default(),
            watchers,
            dashboard,
            dashboard_data_cache,
            dashboard_enabled: settings.dashboard.enabled,
            dashboard_show_when_empty: settings.dashboard.show_when_query_empty,
            dashboard_path: dashboard_path.to_string_lossy().to_string(),
            dashboard_default_location: settings.dashboard.default_location.clone(),
            reduce_dashboard_work_when_unfocused: settings.reduce_dashboard_work_when_unfocused,
            show_dashboard_diagnostics: settings.show_dashboard_diagnostics,
            dashboard_editor: DashboardEditorDialog::default(),
            show_dashboard_editor: false,
            rx,
            folder_aliases,
            folder_aliases_lc,
            bookmark_aliases,
            bookmark_aliases_lc,
            plugin_dirs,
            index_paths,
            max_indexed_items: settings.max_indexed_items,
            enabled_plugins,
            enabled_capabilities,
            visible_flag: visible_flag.clone(),
            restore_flag: restore_flag.clone(),
            last_visible: initial_visible,
            offscreen_pos,
            window_size: win_size,
            window_pos: (0, 0),
            focus_query: false,
            move_cursor_end: false,
            toasts,
            enable_toasts,
            show_inline_errors,
            show_error_toasts,
            toast_duration,
            alias_dialog: AliasDialog::default(),
            bookmark_alias_dialog: BookmarkAliasDialog::default(),
            tempfile_alias_dialog: TempfileAliasDialog::default(),
            tempfile_dialog: TempfileDialog::default(),
            add_bookmark_dialog: AddBookmarkDialog::default(),
            help_window: HelpWindow {
                show_examples: settings.show_examples,
                ..Default::default()
            },
            timer_dialog: TimerDialog::default(),
            completion_dialog: TimerCompletionDialog::default(),
            shell_cmd_dialog: ShellCmdDialog::default(),
            snippet_dialog: SnippetDialog::default(),
            macro_dialog: MacroDialog::default(),
            mouse_gestures_dialog: MgGesturesDialog::default(),
            mouse_gesture_settings_dialog: MouseGestureSettingsDialog::default(),
            theme_settings_dialog_open: false,
            theme_settings_dialog: ThemeSettingsDialogState::default(),
            fav_dialog: FavDialog::default(),
            file_search_dialog: FileSearchDialogState {
                settings: file_search_settings.clone(),
                ui_preferences: file_search_settings.ui_preferences.clone(),
                case_sensitive: file_search_settings.case_sensitive,
                include_hidden: file_search_settings.include_hidden_files,
                ..FileSearchDialogState::default()
            },
            file_search_coordinator: SearchCoordinator::from_settings(file_search_settings),
            notes_dialog: NotesDialog::default(),
            note_graph_dialog: NoteGraphDialog::default(),
            unused_assets_dialog: UnusedAssetsDialog::default(),
            note_panels: Vec::new(),
            image_panels: Vec::new(),
            screenshot_editors: Vec::new(),
            todo_dialog: TodoDialog::default(),
            todo_view_dialog: TodoViewDialog::default(),
            clipboard_dialog: ClipboardDialog::default(),
            clipboard_modify_runtime,
            clipboard_modify_dialog: ClipboardModifyDialogState::with_initial_size(
                clipboard_modify_settings.dialog_width,
                clipboard_modify_settings.dialog_height,
            ),
            clipboard_modify_config_diagnostic,
            pending_clipboard_modify_immediate: HashMap::new(),
            clipboard_modify_immediate: ImmediateExecutionCoordinator::new(clipboard_service()),
            clipboard_modify_events: Vec::new(),
            convert_panel: ConvertPanel::default(),
            volume_dialog: VolumeDialog::default(),
            brightness_dialog: BrightnessDialog::default(),
            cpu_list_dialog: CpuListDialog::default(),
            toast_log_dialog: ToastLogDialog::default(),
            calendar_popover: CalendarPopover::default(),
            calendar_event_editor: CalendarEventEditor::default(),
            calendar_event_details: CalendarEventDetails::default(),
            panel_stack: Vec::new(),
            panel_states: PanelStates::default(),
            pinned_panels: settings.pinned_panels.clone(),
            help_flag: help_flag.clone(),
            hotkey_str: settings.hotkey.clone(),
            quit_hotkey_str: settings.quit_hotkey.clone(),
            help_hotkey_str: settings.help_hotkey.clone(),
            query_scale,
            list_scale,
            custom_len,
            history_limit: settings.history_limit,
            clipboard_limit: settings.clipboard_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            match_exact: settings.match_exact,
            page_jump: settings.page_jump,
            query_results_layout: settings.query_results_layout.clone(),
            resolved_grid_layout: false,
            note_settings: settings.note.clone(),
            note_panel_default_size: settings.note_panel_default_size,
            note_save_on_close: settings.note_save_on_close,
            note_always_overwrite: settings.note_always_overwrite,
            note_images_as_links: settings.note_images_as_links,
            note_show_details: settings.note_show_details,
            note_external_open,
            note_font_size: 16.0,
            note_more_limit: settings.note_more_limit,
            follow_mouse,
            static_location_enabled: static_enabled,
            static_pos,
            static_size,
            hide_after_run: settings.hide_after_run,
            always_on_top: settings.always_on_top,
            timer_refresh: settings.timer_refresh,
            disable_timer_updates: settings.disable_timer_updates,
            preserve_command: settings.preserve_command,
            clear_query_after_run: settings.clear_query_after_run,
            require_confirm_destructive: settings.require_confirm_destructive,
            query_autocomplete: settings.query_autocomplete,
            net_refresh: settings.net_refresh,
            net_unit: settings.net_unit,
            screenshot_dir: settings.screenshot_dir.clone(),
            screenshot_save_file: settings.screenshot_save_file,
            screenshot_auto_save: settings.screenshot_auto_save,
            screenshot_use_editor: settings.screenshot_use_editor,
            calendar_popover_open: false,
            calendar_editor_open: false,
            calendar_details_open: false,
            calendar_selected_date: None,
            calendar_selected_event: None,
            last_timer_update: Instant::now(),
            last_net_update: Instant::now(),
            last_stopwatch_update: Instant::now(),
            last_search_query: String::new(),
            last_results_valid: false,
            last_timer_query: false,
            last_stopwatch_query: false,
            last_note_search_change: None,
            pending_query: None,
            confirm_modal: ConfirmationModal::default(),
            pending_confirm: None,
            action_cache: Vec::new(),
            action_filter_metadata: Vec::new(),
            actions_by_id,
            command_cache: Vec::new(),
            command_search_cache: Vec::new(),
            completion_index: None,
            action_completion_dirty: false,
            command_completion_dirty: false,
            completion_rebuild_after: None,
            suggestions: Vec::new(),
            autocomplete_index: 0,
            vim_mode: false,
            file_search_window_open: false,
            file_search_selected_kind: crate::file_search::model::SearchKind::Filename,
            file_search_root: None,
            file_search_text: String::new(),
            file_search_active: false,
        };

        tracing::debug!("initial viewport visible: {}", initial_visible);
        apply_visibility(
            initial_visible,
            ctx,
            offscreen_pos,
            follow_mouse,
            static_enabled,
            static_pos.map(|(x, y)| (x as f32, y as f32)),
            static_size.map(|(w, h)| (w as f32, h as f32)),
            (win_size.0 as f32, win_size.1 as f32),
        );

        app.enforce_pinned();
        app.update_panel_stack();

        {
            use crate::global_hotkey::Hotkey as WinHotkey;
            if let Some(ref seq) = settings.hotkey
                && let Ok(mut hk) = WinHotkey::new(seq)
            {
                hk.register(&app, 1);
            }
            if let Some(ref seq) = settings.quit_hotkey
                && let Ok(mut hk) = WinHotkey::new(seq)
            {
                hk.register(&app, 2);
            }
        }

        app.update_action_cache();
        app.update_command_cache();
        app.rebuild_completion_index_now();
        app.search();
        crate::plugins::mouse_gestures::sync_enabled_plugins(app.enabled_plugins.as_ref());
        app.recompute_query_results_layout();
        app
    }

    pub fn maybe_refresh_timer_list(&mut self) {
        let trimmed = self.query.trim();
        if (trimmed.starts_with("timer list")
            || trimmed.starts_with("alarm list")
            || self.last_timer_query)
            && !self.disable_timer_updates
            && self.last_timer_update.elapsed().as_secs_f32() >= self.timer_refresh
        {
            self.last_results_valid = false;
            self.search();
            self.last_timer_update = Instant::now();
        }
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn maybe_refresh_stopwatch_list(&mut self) {
        let trimmed = self.query.trim();
        if (trimmed.starts_with("sw list") || self.last_stopwatch_query)
            && !crate::plugins::stopwatch::running_stopwatches().is_empty()
            && self.last_stopwatch_update.elapsed().as_secs_f32()
                >= crate::plugins::stopwatch::refresh_rate()
        {
            self.last_results_valid = false;
            self.search();
            self.last_stopwatch_update = Instant::now();
        }
    }

    pub(crate) fn accept_suggestion(&mut self, tab: bool) -> bool {
        if !self.query_autocomplete || self.suggestions.is_empty() {
            return false;
        }
        let suggestion = if tab {
            self.suggestions.get(self.autocomplete_index).cloned()
        } else {
            self.suggestions.first().cloned()
        };
        if let Some(s) = suggestion
            && s != self.query.to_lowercase()
        {
            let old_suggestions = self.suggestions.clone();
            let old_index = self.autocomplete_index;
            self.query = s;
            self.move_cursor_end = true;
            self.search();
            if tab && !old_suggestions.is_empty() {
                self.suggestions = old_suggestions;
                self.autocomplete_index = (old_index + 1) % self.suggestions.len();
            }
            return true;
        }
        false
    }

    fn should_use_grid_layout(&self) -> bool {
        if !self.query_results_layout.enabled {
            return false;
        }

        // Global grid mode when capability gating is disabled.
        if !self.query_results_layout.respect_plugin_capability {
            return true;
        }

        let trimmed = self.query.trim();
        if trimmed.is_empty() {
            return false;
        }

        let (filtered_query, _) = split_action_filters(trimmed);
        let query_head = filtered_query
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase);

        let mut prefixed_matches = Vec::new();
        for plugin in self.plugins.iter() {
            if let Some(enabled) = self.enabled_plugins.as_ref()
                && !enabled.contains(plugin.name())
            {
                continue;
            }

            let prefixes = plugin.query_prefixes();
            if prefixes.is_empty() {
                continue;
            }
            let Some(head) = query_head.as_deref() else {
                continue;
            };
            if prefixes
                .iter()
                .any(|prefix| prefix.eq_ignore_ascii_case(head))
            {
                prefixed_matches.push(plugin.as_ref());
            }
        }

        // No plugin-prefixed context: use the configured grid layout.
        if prefixed_matches.is_empty() {
            return true;
        }

        // Ambiguous/mixed plugin context: safely fall back to list mode.
        if prefixed_matches.len() != 1 {
            return false;
        }

        let plugin = prefixed_matches[0];
        if self
            .query_results_layout
            .plugin_opt_out
            .iter()
            .any(|name| name.eq_ignore_ascii_case(plugin.name()))
        {
            return false;
        }

        let capabilities = plugin.capabilities();
        if capabilities.contains(&CAP_FORCE_LIST_RESULTS) {
            return false;
        }
        capabilities.contains(&CAP_GRID_RESULTS_COMPATIBLE)
    }

    pub fn recompute_query_results_layout(&mut self) {
        self.resolved_grid_layout = self.should_use_grid_layout();
    }

    /// Handle a keyboard navigation key. Returns the index of a selected
    /// action when `Enter` is pressed and a selection is available.
    pub fn handle_key(&mut self, key: egui::Key) -> Option<usize> {
        let cols = self.query_results_layout.cols.max(1);
        let move_to = |current: usize, delta: isize, max: usize| -> usize {
            current.saturating_add_signed(delta).min(max)
        };

        match key {
            egui::Key::ArrowDown | egui::Key::Num2 => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if self.resolved_grid_layout => {
                            Some(move_to(i, cols as isize, max))
                        }
                        Some(i) if i < max => Some(i + 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowUp | egui::Key::Num8 => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if self.resolved_grid_layout => {
                            Some(move_to(i, -(cols as isize), max))
                        }
                        Some(i) if i > 0 => Some(i - 1),
                        Some(i) => Some(i.min(max)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowRight | egui::Key::Num6 => {
                if self.resolved_grid_layout && !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if i < max => Some(i + 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowLeft | egui::Key::Num4 => {
                if self.resolved_grid_layout && !self.results.is_empty() {
                    self.selected = match self.selected {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::PageDown => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) => Some(i.saturating_add(self.page_jump).min(max)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::PageUp => {
                if !self.results.is_empty() {
                    self.selected = match self.selected {
                        Some(i) => Some(i.saturating_sub(self.page_jump)),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::Enter => {
                if let Some(i) = self.selected {
                    Some(i)
                } else if self.results.len() == 1 {
                    Some(0)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn focus_input(&mut self) {
        self.focus_query = true;
    }

    fn resolve_pin_action(&self, pin: &HistoryPin) -> Option<Action> {
        if let Some(action) = self.actions_by_id.get(&pin.action_id) {
            return Some(action.clone());
        }

        let commands = self
            .plugins
            .commands_filtered(self.enabled_plugins.as_ref());
        if let Some(action) = commands.into_iter().find(|action| {
            action.action == pin.action_id && action.args.as_deref() == pin.args.as_deref()
        }) {
            return Some(action);
        }

        let snapshot = self.dashboard_data_cache.snapshot();
        if let Some(action) = snapshot.processes.iter().find(|action| {
            action.action == pin.action_id && action.args.as_deref() == pin.args.as_deref()
        }) {
            return Some(action.clone());
        }

        if let Some(fav) = snapshot
            .favorites
            .iter()
            .find(|fav| fav.action == pin.action_id && fav.args.as_deref() == pin.args.as_deref())
        {
            return Some(Action {
                label: fav.label.clone(),
                desc: "Fav".into(),
                action: fav.action.clone(),
                args: fav.args.clone(),
            });
        }

        if let Some(slug) = pin.action_id.strip_prefix("note:open:")
            && let Some(note) = snapshot.notes.iter().find(|note| note.slug == slug)
        {
            return Some(Action {
                label: note.alias.as_ref().unwrap_or(&note.title).clone(),
                desc: "Note".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("clipboard:copy:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(entry) = snapshot.clipboard_history.get(idx)
        {
            return Some(Action {
                label: entry.clone(),
                desc: "Clipboard".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:done:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:edit:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("{} {}", if todo.done { "[x]" } else { "[ ]" }, todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(idx) = pin
            .action_id
            .strip_prefix("todo:remove:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(todo) = snapshot.todos.get(idx)
        {
            return Some(Action {
                label: format!("Remove todo {}", todo.text),
                desc: "Todo".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        for snippet in snapshot.snippets.iter() {
            if pin.action_id == format!("clipboard:{}", snippet.text) {
                return Some(Action {
                    label: snippet.alias.clone(),
                    desc: "Snippet".into(),
                    action: pin.action_id.clone(),
                    args: None,
                });
            }
        }

        if let Some(alias) = pin.action_id.strip_prefix("snippet:edit:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Edit snippet {alias}"),
                desc: "Snippet".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        if let Some(alias) = pin.action_id.strip_prefix("snippet:remove:")
            && snapshot.snippets.iter().any(|s| s.alias == alias)
        {
            return Some(Action {
                label: format!("Remove snippet {alias}"),
                desc: "Snippet".into(),
                action: pin.action_id.clone(),
                args: None,
            });
        }

        None
    }

    pub fn set_last_search_query(&mut self, s: String) {
        self.last_search_query = s;
    }

    pub fn set_last_timer_update(&mut self, t: Instant) {
        self.last_timer_update = t;
    }

    pub fn set_last_stopwatch_update(&mut self, t: Instant) {
        self.last_stopwatch_update = t;
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn last_timer_update(&self) -> Instant {
        self.last_timer_update
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn last_stopwatch_update(&self) -> Instant {
        self.last_stopwatch_update
    }

    pub fn get_last_search_query(&self) -> &str {
        &self.last_search_query
    }

    pub fn last_timer_query_flag(&self) -> bool {
        self.last_timer_query
    }

    pub fn last_stopwatch_query_flag(&self) -> bool {
        self.last_stopwatch_query
    }

    pub fn visible_flag_state(&self) -> bool {
        self.visible_flag.load(Ordering::SeqCst)
    }

    pub fn restore_flag_state(&self) -> bool {
        self.restore_flag.load(Ordering::SeqCst)
    }

    pub fn should_show_dashboard(&self, trimmed: &str) -> bool {
        self.dashboard_enabled && self.dashboard_show_when_empty && trimmed.trim().is_empty()
    }

    pub fn move_cursor_end_flag(&self) -> bool {
        self.move_cursor_end
    }

    const TRACKED_PANELS: [Panel; 40] = [
        Panel::AliasDialog,
        Panel::BookmarkAliasDialog,
        Panel::TempfileAliasDialog,
        Panel::TempfileDialog,
        Panel::AddBookmarkDialog,
        Panel::HelpOverlay,
        Panel::HelpWindow,
        Panel::TimerDialog,
        Panel::CompletionDialog,
        Panel::ShellCmdDialog,
        Panel::SnippetDialog,
        Panel::MacroDialog,
        Panel::MouseGesturesDialog,
        Panel::MouseGestureSettingsDialog,
        Panel::ThemeSettingsDialog,
        Panel::FavDialog,
        Panel::FileSearchDialog,
        Panel::NotesDialog,
        Panel::NoteGraphDialog,
        Panel::UnusedAssetsDialog,
        Panel::NotePanel,
        Panel::ImagePanel,
        Panel::ScreenshotEditor,
        Panel::TodoDialog,
        Panel::TodoViewDialog,
        Panel::ClipboardDialog,
        Panel::ClipboardModifyDialog,
        Panel::ConvertPanel,
        Panel::VolumeDialog,
        Panel::BrightnessDialog,
        Panel::CpuListDialog,
        Panel::ToastLogDialog,
        Panel::CalendarPopover,
        Panel::CalendarEventEditor,
        Panel::CalendarEventDetails,
        Panel::Editor,
        Panel::Settings,
        Panel::Plugins,
        Panel::MultiManagerDialog,
        Panel::MultiManagerSettingsDialog,
    ];

    fn is_panel_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::AliasDialog => self.alias_dialog.open,
            Panel::BookmarkAliasDialog => self.bookmark_alias_dialog.open,
            Panel::TempfileAliasDialog => self.tempfile_alias_dialog.open,
            Panel::TempfileDialog => self.tempfile_dialog.open,
            Panel::AddBookmarkDialog => self.add_bookmark_dialog.open,
            Panel::HelpOverlay => self.help_window.overlay_open,
            Panel::HelpWindow => self.help_window.open,
            Panel::TimerDialog => self.timer_dialog.open,
            Panel::CompletionDialog => self.completion_dialog.open,
            Panel::ShellCmdDialog => self.shell_cmd_dialog.open,
            Panel::SnippetDialog => self.snippet_dialog.open,
            Panel::MacroDialog => self.macro_dialog.open,
            Panel::MouseGesturesDialog => self.mouse_gestures_dialog.open,
            Panel::MouseGestureSettingsDialog => self.mouse_gesture_settings_dialog.open,
            Panel::ThemeSettingsDialog => self.theme_settings_dialog_open,
            Panel::FavDialog => self.fav_dialog.open,
            Panel::FileSearchDialog => self.file_search_dialog.open,
            Panel::NotesDialog => self.notes_dialog.open,
            Panel::NoteGraphDialog => self.note_graph_dialog.open,
            Panel::UnusedAssetsDialog => self.unused_assets_dialog.open,
            Panel::NotePanel => !self.note_panels.is_empty(),
            Panel::ImagePanel => !self.image_panels.is_empty(),
            Panel::ScreenshotEditor => !self.screenshot_editors.is_empty(),
            Panel::TodoDialog => self.todo_dialog.open,
            Panel::TodoViewDialog => self.todo_view_dialog.open,
            Panel::ClipboardDialog => self.clipboard_dialog.open,
            Panel::ClipboardModifyDialog => self.clipboard_modify_dialog.open,
            Panel::ConvertPanel => self.convert_panel.open,
            Panel::VolumeDialog => self.volume_dialog.open,
            Panel::BrightnessDialog => self.brightness_dialog.open,
            Panel::CpuListDialog => self.cpu_list_dialog.open,
            Panel::ToastLogDialog => self.toast_log_dialog.open,
            Panel::CalendarPopover => self.calendar_popover_open,
            Panel::CalendarEventEditor => self.calendar_editor_open,
            Panel::CalendarEventDetails => self.calendar_details_open,
            Panel::Editor => self.show_editor,
            Panel::Settings => self.show_settings,
            Panel::Plugins => self.show_plugins,
            Panel::MultiManagerDialog => self.multi_manager_dialog.open,
            Panel::MultiManagerSettingsDialog => self.multi_manager_settings_dialog.open,
        }
    }

    fn any_panel_open(&self) -> bool {
        Self::TRACKED_PANELS
            .iter()
            .copied()
            .any(|panel| self.is_panel_open(panel))
    }

    pub fn unregister_all_hotkeys(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
        if let Ok(mut registered_hotkeys) = self.registered_hotkeys.lock() {
            for id in registered_hotkeys.values() {
                unsafe {
                    let _ = UnregisterHotKey(None, *id as i32);
                }
            }
            registered_hotkeys.clear();
        } else {
            tracing::error!("failed to lock registered_hotkeys");
        }
    }

    fn handle_screenshot_launch_result(
        &mut self,
        result: anyhow::Result<crate::plugins::screenshot::ScreenshotLaunchOutcome>,
    ) -> bool {
        match result {
            Ok(crate::plugins::screenshot::ScreenshotLaunchOutcome::Completed) => true,
            Ok(crate::plugins::screenshot::ScreenshotLaunchOutcome::Cancelled) => false,
            Err(e) => {
                self.report_error_message("launcher", format!("Failed: {e}"));
                false
            }
        }
    }

    /// Return the currently configured screenshot directory, if any.
    pub fn get_screenshot_dir(&self) -> Option<&str> {
        self.screenshot_dir.as_deref()
    }

    /// Whether screenshots copied to the clipboard are also saved to disk.
    pub fn get_screenshot_save_file(&self) -> bool {
        self.screenshot_save_file
    }

    /// Whether screenshots are saved automatically after editing.
    pub fn get_screenshot_auto_save(&self) -> bool {
        self.screenshot_auto_save
    }

    pub fn get_screenshot_use_editor(&self) -> bool {
        self.screenshot_use_editor
    }

    /// Close the top-most open dialog if any is visible.
    /// Returns `true` when a dialog was closed.
    pub fn close_front_dialog(&mut self) -> bool {
        let panel = match self.panel_stack.pop() {
            Some(p) => p,
            None => return false,
        };
        if self.pinned_panels.contains(&panel) {
            self.panel_stack.push(panel);
            return false;
        }
        match panel {
            Panel::AliasDialog => {
                self.alias_dialog.open = false;
                self.panel_states.alias_dialog = false;
            }
            Panel::BookmarkAliasDialog => {
                self.bookmark_alias_dialog.open = false;
                self.panel_states.bookmark_alias_dialog = false;
            }
            Panel::TempfileAliasDialog => {
                self.tempfile_alias_dialog.open = false;
                self.panel_states.tempfile_alias_dialog = false;
            }
            Panel::TempfileDialog => {
                self.tempfile_dialog.open = false;
                self.panel_states.tempfile_dialog = false;
            }
            Panel::AddBookmarkDialog => {
                self.add_bookmark_dialog.open = false;
                self.panel_states.add_bookmark_dialog = false;
            }
            Panel::HelpOverlay => {
                self.help_window.overlay_open = false;
                self.panel_states.help_overlay = false;
            }
            Panel::HelpWindow => {
                self.help_window.open = false;
                self.panel_states.help_window = false;
            }
            Panel::TimerDialog => {
                self.timer_dialog.open = false;
                self.panel_states.timer_dialog = false;
            }
            Panel::CompletionDialog => {
                self.completion_dialog.open = false;
                self.panel_states.completion_dialog = false;
            }
            Panel::ShellCmdDialog => {
                self.shell_cmd_dialog.open = false;
                self.panel_states.shell_cmd_dialog = false;
            }
            Panel::SnippetDialog => {
                self.snippet_dialog.open = false;
                self.panel_states.snippet_dialog = false;
            }
            Panel::MacroDialog => {
                self.macro_dialog.open = false;
                self.panel_states.macro_dialog = false;
            }
            Panel::MouseGesturesDialog => {
                self.mouse_gestures_dialog.open = false;
                self.panel_states.mouse_gestures_dialog = false;
            }
            Panel::MouseGestureSettingsDialog => {
                self.mouse_gesture_settings_dialog.open = false;
                self.panel_states.mouse_gesture_settings_dialog = false;
            }
            Panel::ThemeSettingsDialog => {
                self.theme_settings_dialog_open = false;
                self.panel_states.theme_settings_dialog = false;
            }
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::FileSearchDialog => {
                self.file_search_dialog.open = false;
                self.panel_states.file_search_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::NoteGraphDialog => {
                self.note_graph_dialog.open = false;
                self.panel_states.note_graph_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut panel) = self.note_panels.pop()
                    && self.note_save_on_close
                {
                    panel.save(self);
                }
                self.panel_states.note_panel = false;
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
            }
            Panel::ScreenshotEditor => {
                let _ = self.screenshot_editors.pop();
                self.panel_states.screenshot_editor = false;
            }
            Panel::TodoDialog => {
                self.todo_dialog.open = false;
                self.panel_states.todo_dialog = false;
            }
            Panel::TodoViewDialog => {
                self.todo_view_dialog.open = false;
                self.panel_states.todo_view_dialog = false;
            }
            Panel::ClipboardDialog => {
                self.clipboard_dialog.open = false;
                self.panel_states.clipboard_dialog = false;
            }
            Panel::ClipboardModifyDialog => {
                self.clipboard_modify_dialog.open = false;
                self.clipboard_modify_dialog.cleanup_after_close();
                self.panel_states.clipboard_modify_dialog = false;
            }
            Panel::ConvertPanel => {
                self.convert_panel.open = false;
                self.panel_states.convert_panel = false;
            }
            Panel::VolumeDialog => {
                self.volume_dialog.open = false;
                self.panel_states.volume_dialog = false;
            }
            Panel::BrightnessDialog => {
                self.brightness_dialog.open = false;
                self.panel_states.brightness_dialog = false;
            }
            Panel::CpuListDialog => {
                self.cpu_list_dialog.open = false;
                self.panel_states.cpu_list_dialog = false;
            }
            Panel::ToastLogDialog => {
                self.toast_log_dialog.open = false;
                self.panel_states.toast_log_dialog = false;
            }
            Panel::CalendarPopover => {
                self.calendar_popover_open = false;
                self.panel_states.calendar_popover = false;
            }
            Panel::CalendarEventEditor => {
                self.calendar_editor_open = false;
                self.panel_states.calendar_event_editor = false;
            }
            Panel::CalendarEventDetails => {
                self.calendar_details_open = false;
                self.panel_states.calendar_event_details = false;
            }
            Panel::Editor => {
                self.show_editor = false;
                self.panel_states.editor = false;
            }
            Panel::Settings => {
                self.show_settings = false;
                self.panel_states.settings = false;
            }
            Panel::Plugins => {
                self.show_plugins = false;
                self.panel_states.plugins = false;
            }
            Panel::MultiManagerDialog => {
                self.multi_manager_dialog.open = false;
                self.panel_states.multi_manager_dialog = false;
            }
            Panel::MultiManagerSettingsDialog => {
                self.multi_manager_settings_dialog.open = false;
                self.panel_states.multi_manager_settings_dialog = false;
            }
        }
        true
    }

    fn force_close_panel(&mut self, panel: Panel) {
        match panel {
            Panel::AliasDialog => {
                self.alias_dialog.open = false;
                self.panel_states.alias_dialog = false;
            }
            Panel::BookmarkAliasDialog => {
                self.bookmark_alias_dialog.open = false;
                self.panel_states.bookmark_alias_dialog = false;
            }
            Panel::TempfileAliasDialog => {
                self.tempfile_alias_dialog.open = false;
                self.panel_states.tempfile_alias_dialog = false;
            }
            Panel::TempfileDialog => {
                self.tempfile_dialog.open = false;
                self.panel_states.tempfile_dialog = false;
            }
            Panel::AddBookmarkDialog => {
                self.add_bookmark_dialog.open = false;
                self.panel_states.add_bookmark_dialog = false;
            }
            Panel::HelpOverlay => {
                self.help_window.overlay_open = false;
                self.panel_states.help_overlay = false;
            }
            Panel::HelpWindow => {
                self.help_window.open = false;
                self.panel_states.help_window = false;
            }
            Panel::TimerDialog => {
                self.timer_dialog.open = false;
                self.panel_states.timer_dialog = false;
            }
            Panel::CompletionDialog => {
                self.completion_dialog.open = false;
                self.panel_states.completion_dialog = false;
            }
            Panel::ShellCmdDialog => {
                self.shell_cmd_dialog.open = false;
                self.panel_states.shell_cmd_dialog = false;
            }
            Panel::SnippetDialog => {
                self.snippet_dialog.open = false;
                self.panel_states.snippet_dialog = false;
            }
            Panel::MacroDialog => {
                self.macro_dialog.open = false;
                self.panel_states.macro_dialog = false;
            }
            Panel::MouseGesturesDialog => {
                self.mouse_gestures_dialog.open = false;
                self.panel_states.mouse_gestures_dialog = false;
            }
            Panel::MouseGestureSettingsDialog => {
                self.mouse_gesture_settings_dialog.open = false;
                self.panel_states.mouse_gesture_settings_dialog = false;
            }
            Panel::ThemeSettingsDialog => {
                self.theme_settings_dialog_open = false;
                self.panel_states.theme_settings_dialog = false;
            }
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::FileSearchDialog => {
                self.file_search_dialog.open = false;
                self.panel_states.file_search_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::NoteGraphDialog => {
                self.note_graph_dialog.open = false;
                self.panel_states.note_graph_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut panel) = self.note_panels.pop()
                    && self.note_save_on_close
                {
                    panel.save(self);
                }
                self.panel_states.note_panel = false;
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
            }
            Panel::ScreenshotEditor => {
                let _ = self.screenshot_editors.pop();
                self.panel_states.screenshot_editor = false;
            }
            Panel::TodoDialog => {
                self.todo_dialog.open = false;
                self.panel_states.todo_dialog = false;
            }
            Panel::TodoViewDialog => {
                self.todo_view_dialog.open = false;
                self.panel_states.todo_view_dialog = false;
            }
            Panel::ClipboardDialog => {
                self.clipboard_dialog.open = false;
                self.panel_states.clipboard_dialog = false;
            }
            Panel::ClipboardModifyDialog => {
                self.clipboard_modify_dialog.open = false;
                self.clipboard_modify_dialog.cleanup_after_close();
                self.panel_states.clipboard_modify_dialog = false;
            }
            Panel::ConvertPanel => {
                self.convert_panel.open = false;
                self.panel_states.convert_panel = false;
            }
            Panel::VolumeDialog => {
                self.volume_dialog.open = false;
                self.panel_states.volume_dialog = false;
            }
            Panel::BrightnessDialog => {
                self.brightness_dialog.open = false;
                self.panel_states.brightness_dialog = false;
            }
            Panel::CpuListDialog => {
                self.cpu_list_dialog.open = false;
                self.panel_states.cpu_list_dialog = false;
            }
            Panel::ToastLogDialog => {
                self.toast_log_dialog.open = false;
                self.panel_states.toast_log_dialog = false;
            }
            Panel::CalendarPopover => {
                self.calendar_popover_open = false;
                self.panel_states.calendar_popover = false;
            }
            Panel::CalendarEventEditor => {
                self.calendar_editor_open = false;
                self.panel_states.calendar_event_editor = false;
            }
            Panel::CalendarEventDetails => {
                self.calendar_details_open = false;
                self.panel_states.calendar_event_details = false;
            }
            Panel::Editor => {
                self.show_editor = false;
                self.panel_states.editor = false;
            }
            Panel::Settings => {
                self.show_settings = false;
                self.panel_states.settings = false;
            }
            Panel::Plugins => {
                self.show_plugins = false;
                self.panel_states.plugins = false;
            }
            Panel::MultiManagerDialog => {
                self.multi_manager_dialog.open = false;
                self.panel_states.multi_manager_dialog = false;
            }
            Panel::MultiManagerSettingsDialog => {
                self.multi_manager_settings_dialog.open = false;
                self.panel_states.multi_manager_settings_dialog = false;
            }
        }
        self.panel_stack.retain(|p| *p != panel);
    }

    fn ensure_open(&mut self, panel: Panel) {
        match panel {
            Panel::AliasDialog => self.alias_dialog.open = true,
            Panel::BookmarkAliasDialog => self.bookmark_alias_dialog.open = true,
            Panel::TempfileAliasDialog => self.tempfile_alias_dialog.open = true,
            Panel::TempfileDialog => self.tempfile_dialog.open = true,
            Panel::AddBookmarkDialog => self.add_bookmark_dialog.open = true,
            Panel::HelpOverlay => self.help_window.overlay_open = true,
            Panel::HelpWindow => self.help_window.open = true,
            Panel::TimerDialog => self.timer_dialog.open = true,
            Panel::CompletionDialog => self.completion_dialog.open = true,
            Panel::ShellCmdDialog => self.shell_cmd_dialog.open = true,
            Panel::SnippetDialog => self.snippet_dialog.open = true,
            Panel::MacroDialog => self.macro_dialog.open = true,
            Panel::MouseGesturesDialog => self.mouse_gestures_dialog.open = true,
            Panel::MouseGestureSettingsDialog => self.mouse_gesture_settings_dialog.open(),
            Panel::ThemeSettingsDialog => self.open_theme_settings_dialog(),
            Panel::FavDialog => self.fav_dialog.open = true,
            Panel::FileSearchDialog => self.file_search_dialog.open(),
            Panel::NotesDialog => self.notes_dialog.open = true,
            Panel::NoteGraphDialog => self.note_graph_dialog.open = true,
            Panel::UnusedAssetsDialog => self.unused_assets_dialog.open = true,
            Panel::NotePanel => {}
            Panel::ImagePanel => {}
            Panel::ScreenshotEditor => {}
            Panel::TodoDialog => self.todo_dialog.open = true,
            Panel::TodoViewDialog => self.todo_view_dialog.open = true,
            Panel::ClipboardDialog => self.clipboard_dialog.open = true,
            Panel::ClipboardModifyDialog => self
                .clipboard_modify_dialog
                .open_section(ClipboardModifyDialogSection::Modify, &clipboard_service()),
            Panel::ConvertPanel => self.convert_panel.open = true,
            Panel::VolumeDialog => self.volume_dialog.open = true,
            Panel::BrightnessDialog => self.brightness_dialog.open = true,
            Panel::CpuListDialog => self.cpu_list_dialog.open = true,
            Panel::ToastLogDialog => self.toast_log_dialog.open = true,
            Panel::CalendarPopover => self.calendar_popover_open = true,
            Panel::CalendarEventEditor => self.calendar_editor_open = true,
            Panel::CalendarEventDetails => self.calendar_details_open = true,
            Panel::Editor => self.show_editor = true,
            Panel::Settings => self.open_settings_dialog(),
            Panel::Plugins => self.show_plugins = true,
            Panel::MultiManagerDialog => self.multi_manager_dialog.open = true,
            Panel::MultiManagerSettingsDialog => self.multi_manager_settings_dialog.open = true,
        }
        if !self.panel_stack.contains(&panel) {
            self.panel_stack.push(panel);
        }
    }

    fn focus_panel(&mut self, panel: Panel) {
        self.panel_stack.retain(|p| *p != panel);
        self.panel_stack.push(panel);
        self.ensure_open(panel);
    }

    fn enforce_pinned(&mut self) {
        let pinned = self.pinned_panels.clone();
        for panel in pinned {
            self.ensure_open(panel);
        }
    }

    fn toggle_pin(&mut self, panel: Panel) {
        if self.pinned_panels.contains(&panel) {
            self.pinned_panels.retain(|p| *p != panel);
            self.force_close_panel(panel);
        } else {
            self.pinned_panels.push(panel);
            self.focus_panel(panel);
        }
        self.save_pinned_panels();
    }

    fn save_pinned_panels(&mut self) {
        if let Ok(mut s) = Settings::load(&self.settings_path) {
            s.pinned_panels = self.pinned_panels.clone();
            if let Err(e) = s.save(&self.settings_path) {
                self.report_error_message("launcher", format!("Failed to save: {e}"));
            }
        }
    }

    fn update_panel_stack(&mut self) {
        macro_rules! check {
            ($field:ident, $kind:expr) => {{
                let is_open = self.is_panel_open($kind);
                if is_open && !self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_stack.push($kind);
                    self.panel_states.$field = true;
                } else if !is_open && self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_states.$field = false;
                }
            }};
        }

        check!(alias_dialog, Panel::AliasDialog);
        check!(bookmark_alias_dialog, Panel::BookmarkAliasDialog);
        check!(tempfile_alias_dialog, Panel::TempfileAliasDialog);
        check!(tempfile_dialog, Panel::TempfileDialog);
        check!(add_bookmark_dialog, Panel::AddBookmarkDialog);
        check!(help_overlay, Panel::HelpOverlay);
        check!(help_window, Panel::HelpWindow);
        check!(timer_dialog, Panel::TimerDialog);
        check!(completion_dialog, Panel::CompletionDialog);
        check!(shell_cmd_dialog, Panel::ShellCmdDialog);
        check!(snippet_dialog, Panel::SnippetDialog);
        check!(macro_dialog, Panel::MacroDialog);
        check!(mouse_gestures_dialog, Panel::MouseGesturesDialog);
        check!(
            mouse_gesture_settings_dialog,
            Panel::MouseGestureSettingsDialog
        );
        check!(theme_settings_dialog, Panel::ThemeSettingsDialog);
        check!(fav_dialog, Panel::FavDialog);
        check!(file_search_dialog, Panel::FileSearchDialog);
        check!(notes_dialog, Panel::NotesDialog);
        check!(note_graph_dialog, Panel::NoteGraphDialog);
        check!(unused_assets_dialog, Panel::UnusedAssetsDialog);
        check!(note_panel, Panel::NotePanel);
        check!(image_panel, Panel::ImagePanel);
        check!(screenshot_editor, Panel::ScreenshotEditor);
        check!(todo_dialog, Panel::TodoDialog);
        check!(todo_view_dialog, Panel::TodoViewDialog);
        check!(clipboard_dialog, Panel::ClipboardDialog);
        check!(clipboard_modify_dialog, Panel::ClipboardModifyDialog);
        check!(convert_panel, Panel::ConvertPanel);
        check!(volume_dialog, Panel::VolumeDialog);
        check!(brightness_dialog, Panel::BrightnessDialog);
        check!(cpu_list_dialog, Panel::CpuListDialog);
        check!(toast_log_dialog, Panel::ToastLogDialog);
        check!(calendar_popover, Panel::CalendarPopover);
        check!(calendar_event_editor, Panel::CalendarEventEditor);
        check!(calendar_event_details, Panel::CalendarEventDetails);
        check!(editor, Panel::Editor);
        check!(settings, Panel::Settings);
        check!(plugins, Panel::Plugins);
        check!(multi_manager_dialog, Panel::MultiManagerDialog);
        check!(
            multi_manager_settings_dialog,
            Panel::MultiManagerSettingsDialog
        );
    }
}

impl LauncherApp {
    pub fn watch_receiver(&self) -> &Receiver<WatchEvent> {
        &self.rx
    }

    /// Open a note panel for the given slug, optionally using a template for new notes.
    pub fn open_note_panel(&mut self, slug: &str, template: Option<&str>) {
        use crate::plugins::note::{
            Note, expand_template_variables, extract_aliases, get_template, load_notes,
        };
        let note = load_notes()
            .unwrap_or_default()
            .into_iter()
            .find(|n| {
                n.slug == slug
                    || n.alias
                        .as_ref()
                        .map(|a| a.eq_ignore_ascii_case(slug))
                        .unwrap_or(false)
            })
            .unwrap_or_else(|| {
                let title = slug.replace('-', " ");
                let content = if let Some(tpl_name) = template {
                    if let Some(tpl) = get_template(tpl_name) {
                        let filled =
                            expand_template_variables(&tpl, &title, slug, chrono::Local::now());
                        if filled.starts_with("# ") {
                            filled
                        } else {
                            format!("# {}\n\n{}", title, filled)
                        }
                    } else {
                        format!("# {}\n\n", title)
                    }
                } else {
                    format!("# {}\n\n", title)
                };
                let aliases = extract_aliases(&content);
                let alias = aliases.first().cloned();
                Note {
                    title,
                    path: std::path::PathBuf::new(),
                    content,
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: String::new(),
                    alias,
                    aliases,
                    entity_refs: Vec::new(),
                }
            });
        if let Some(existing_idx) = self
            .note_panels
            .iter()
            .position(|panel| panel.note_slug() == note.slug)
        {
            let panel = self.note_panels.remove(existing_idx);
            self.note_panels.push(panel);
            self.update_panel_stack();
            return;
        }

        let word_count = note.content.split_whitespace().count();
        if self.enable_toasts {
            push_toast(
                &mut self.toasts,
                Toast {
                    text: format!(
                        "Opened note ({} words) – press Esc or Cmd+W to close",
                        word_count
                    )
                    .into(),
                    kind: ToastKind::Info,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),
                },
            );
        }
        let mut panel = NotePanel::from_note_with_details_and_settings(
            note,
            self.note_show_details,
            &self.note_settings,
        );
        panel.load_collapsed_sections_state(self);
        self.note_panels.push(panel);
        // Allow keyboard shortcuts like Esc/Cmd+W to immediately close the panel
        self.update_panel_stack();
    }

    pub fn push_note_panel(&mut self, panel: NotePanel) {
        self.note_panels.push(panel);
        self.update_panel_stack();
    }

    /// Open an image viewer panel for the given file path.
    pub fn open_image_panel(&mut self, path: &Path) {
        if !path.exists() {
            self.report_error_message("launcher", format!("Image not found: {}", path.display()));
            return;
        }
        if image::ImageFormat::from_path(path).is_err() {
            self.report_error_message(
                "launcher",
                format!("Unsupported image format: {}", path.display()),
            );
            return;
        }
        self.image_panels.push(ImagePanel::new(path.to_path_buf()));
        self.update_panel_stack();
    }

    /// Open the screenshot editor for a captured image.
    pub fn open_screenshot_editor(&mut self, img: image::RgbaImage, clip: bool, tool: MarkupTool) {
        use chrono::Local;
        let dir = crate::plugins::screenshot::screenshot_dir();
        let _ = std::fs::create_dir_all(&dir);
        let filename = format!(
            "multi_launcher_{}.png",
            Local::now().format("%Y%m%d_%H%M%S")
        );
        let path = dir.join(filename);
        self.screenshot_editors.push(ScreenshotEditor::new(
            img,
            path,
            clip,
            self.screenshot_auto_save,
            tool,
        ));
        self.update_panel_stack();
    }

    /// Update query to show available note tags.
    pub fn open_note_tags(&mut self) {
        self.query = "note tags".into();
        self.search();
        self.focus_input();
        if self.enable_toasts {
            push_toast(
                &mut self.toasts,
                Toast {
                    text: "Showing note tags – press Esc to exit".into(),
                    kind: ToastKind::Info,
                    options: ToastOptions::default()
                        .duration_in_seconds(self.toast_duration as f64),
                },
            );
        }
    }

    pub fn open_calendar_popover(&mut self, date: Option<NaiveDate>) {
        let today = chrono::Local::now().naive_local().date();
        self.calendar_selected_date = Some(date.unwrap_or(today));
        self.calendar_selected_event = None;
        self.calendar_popover_open = true;
    }

    pub fn open_calendar_editor(
        &mut self,
        event: Option<crate::plugins::calendar::CalendarEvent>,
        split_scope: Option<(
            crate::gui::calendar_event_details::RecurrenceScope,
            chrono::NaiveDateTime,
        )>,
    ) {
        if let Some(event) = event {
            self.calendar_event_editor.open(Some(event), split_scope);
        } else {
            let date = self
                .calendar_selected_date
                .unwrap_or_else(|| chrono::Local::now().naive_local().date());
            self.calendar_event_editor.open_new(date);
        }
        self.calendar_editor_open = true;
    }

    /// Filter the note list to only show notes containing the given tag.
    pub fn filter_notes_by_tag(&mut self, tag: &str) {
        self.query = format!("note list #{tag}");
        self.search();
        self.focus_input();
    }

    /// Open a link collected from notes in the system browser.
    pub fn open_note_link(&mut self, link: &str) {
        let url = if link.starts_with("www.") {
            format!("https://{link}")
        } else {
            link.to_string()
        };
        if link.starts_with("www.") || link.contains("://") {
            match Url::parse(&url) {
                Ok(url) if url.scheme() == "https" => match open_link(url.as_str()) {
                    Ok(_) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: "Opened note link".into(),
                                    kind: ToastKind::Info,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        self.report_error_message("launcher", format!("Failed to open link: {e}"))
                    }
                },
                _ => {
                    self.add_error_toast(format!("Invalid link: {link}"));
                }
            }
        } else {
            self.open_note_panel(link, None);
        }
    }

    /// Resolve a note by slug and open it in Neovim using injected callbacks.
    ///
    /// `load` should mirror [`load_notes`](crate::plugins::note::load_notes) and
    /// is invoked once to fetch the available notes. `spawn` is called with the
    /// resolved path and should launch `nvim`. Both closures are `FnOnce` to
    /// support unit testing of this helper.
    ///
    /// Any error from either step is stored in [`self.error`](Self::error); the
    /// boolean return value is always `true` and currently unused.
    pub fn open_note_in_neovim<L, S>(&mut self, slug: &str, load: L, spawn: S) -> bool
    where
        L: FnOnce() -> anyhow::Result<Vec<crate::plugins::note::Note>>,
        S: FnOnce(&std::path::Path) -> std::io::Result<()>,
    {
        match load() {
            Ok(notes) => {
                if let Some(note) = notes.iter().find(|n| n.slug == slug) {
                    if let Err(e) = spawn(&note.path) {
                        self.report_error_message("launcher", e.to_string());
                    }
                } else {
                    self.report_error_message("launcher", "Note not found".to_string());
                }
            }
            Err(e) => {
                self.report_error_message("launcher", e.to_string());
            }
        }
        true
    }

    /// Delete a note by its slug identifier.
    pub fn delete_note(&mut self, slug: &str) {
        use crate::plugins::note::{load_notes, remove_note};
        match load_notes() {
            Ok(notes) => {
                if let Some((idx, note)) = notes.into_iter().enumerate().find(|(_, n)| {
                    n.slug == slug
                        || n.alias
                            .as_ref()
                            .map(|a| a.eq_ignore_ascii_case(slug))
                            .unwrap_or(false)
                }) {
                    let word_count = note.content.split_whitespace().count();
                    if let Err(e) = remove_note(idx) {
                        self.report_error_message(
                            "launcher",
                            format!("Failed to remove note: {e}"),
                        );
                    } else {
                        let msg = format!(
                            "Removed note {} ({} words)",
                            note.alias.as_ref().unwrap_or(&note.title),
                            word_count
                        );
                        append_toast_log(&msg);
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: msg.clone().into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                        if self.query.trim_start().starts_with("note list") {
                            self.pending_query = Some(self.query.clone());
                            self.search();
                        }
                        self.notes_dialog.open();
                    }
                } else {
                    self.report_error_message("launcher", "Note not found");
                }
            }
            Err(e) => self.report_error_message("launcher", format!("Failed to load notes: {e}")),
        }
        self.focus_input();
    }

    /// Process dropped files or directories.
    pub fn handle_dropped_files(&mut self, files: Vec<egui::DroppedFile>) {
        for file in files {
            if let Some(path) = file.path {
                if path.is_dir() {
                    if let Err(e) = folders::add(path.to_str().unwrap_or_default()) {
                        self.report_error_message("launcher", format!("Failed to add folder: {e}"));
                    }
                    if let Some(p) = path.to_str() {
                        self.alias_dialog.open(p);
                    }
                } else if let Some(p) = path.to_str() {
                    self.show_editor = true;
                    self.editor.open_add_with_path(p);
                }
            }
        }
    }
}

pub fn recv_test_event(rx: &Receiver<WatchEvent>) -> Option<TestWatchEvent> {
    while let Ok(ev) = rx.try_recv() {
        match ev {
            WatchEvent::Actions | WatchEvent::Folders | WatchEvent::Bookmarks => {
                return Some(ev.into());
            }
            WatchEvent::Dashboard(_)
            | WatchEvent::Clipboard
            | WatchEvent::Snippets
            | WatchEvent::Notes
            | WatchEvent::Todos
            | WatchEvent::Favorites
            | WatchEvent::Gestures
            | WatchEvent::ExecuteAction(_)
            | WatchEvent::ClipboardModify(_) => {
                continue;
            }
            WatchEvent::Recycle(_) => return Some(ev.into()),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::slug::reset_slug_lookup,
        dashboard::config::OverflowMode,
        dashboard::layout::NormalizedSlot,
        plugin::PluginManager,
        plugins::note::{NotePlugin, append_note, load_notes, save_notes},
        settings::Settings,
        toast_log::TOAST_LOG_FILE,
    };
    use eframe::egui;
    use image::RgbaImage;
    use once_cell::sync::Lazy;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[derive(Clone)]
    struct TestPlugin {
        name: &'static str,
        caps: Vec<&'static str>,
        prefixes: Vec<&'static str>,
    }

    impl crate::plugin::Plugin for TestPlugin {
        fn search(&self, _query: &str) -> Vec<Action> {
            vec![Action {
                label: "plugin".into(),
                desc: "test".into(),
                action: "plugin:test".into(),
                args: None,
            }]
        }

        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "test"
        }

        fn capabilities(&self) -> &[&str] {
            &self.caps
        }

        fn query_prefixes(&self) -> &[&str] {
            &self.prefixes
        }
    }

    struct ExactFilterPlugin;

    impl crate::plugin::Plugin for ExactFilterPlugin {
        fn search(&self, query: &str) -> Vec<Action> {
            let query = query.trim().to_ascii_lowercase();
            if query == "note today" {
                return vec![Action {
                    label: "Create 2025 02 23".into(),
                    desc: "Note".into(),
                    action: "note:new:2025-02-23".into(),
                    args: None,
                }];
            }
            if query.starts_with("note search ") {
                return vec![Action {
                    label: "Alpha note".into(),
                    desc: "Note".into(),
                    action: "note:open:alpha".into(),
                    args: None,
                }];
            }
            if query.starts_with("note ") {
                return vec![Action {
                    label: "note search".into(),
                    desc: "Note".into(),
                    action: "query:note search ".into(),
                    args: None,
                }];
            }
            Vec::new()
        }

        fn name(&self) -> &str {
            "exact-filter-plugin"
        }

        fn description(&self) -> &str {
            "Exact filter test plugin"
        }

        fn capabilities(&self) -> &[&str] {
            &[]
        }

        fn query_prefixes(&self) -> &[&str] {
            &["note"]
        }
    }

    #[test]
    fn file_search_dialog_counts_as_any_panel_open() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        app.file_search_dialog.open = true;

        assert!(app.any_panel_open());
    }

    #[test]
    fn tracked_openable_panels_count_as_any_panel_open() {
        let ctx = egui::Context::default();
        let openable_panels = LauncherApp::TRACKED_PANELS.iter().copied().filter(|panel| {
            !matches!(
                panel,
                Panel::NotePanel | Panel::ImagePanel | Panel::ScreenshotEditor
            )
        });

        for panel in openable_panels {
            let mut app = new_app(&ctx);
            app.ensure_open(panel);
            assert!(
                app.any_panel_open(),
                "expected {panel:?} to count as an open panel"
            );
        }
    }

    #[test]
    fn inline_error_visibility_respects_setting() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.report_error("test.inline_error_visibility", "boom");
        app.show_inline_errors = true;
        assert!(app.should_render_inline_error());

        app.show_inline_errors = false;
        assert!(!app.should_render_inline_error());
    }

    #[test]
    fn report_error_respects_inline_and_toast_settings() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;
        app.show_error_toasts = true;
        app.show_inline_errors = true;

        app.report_error("test.report_error", "first");
        assert_eq!(app.error.as_deref(), Some("first"));
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert_eq!(log.matches("[error:test.report_error] first").count(), 1);
        assert_eq!(log.matches("first").count(), 2);

        app.error = None;
        app.show_inline_errors = false;
        app.show_error_toasts = false;
        app.report_error("test.report_error", "second");
        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert_eq!(log.matches("[error:test.report_error] second").count(), 1);
        assert_eq!(log.matches("second").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn screenshot_cancel_does_not_report_failure_or_toast() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;
        app.show_error_toasts = true;
        app.show_inline_errors = true;

        let before_log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap_or_default();
        let handled = app.handle_screenshot_launch_result(Ok(
            crate::plugins::screenshot::ScreenshotLaunchOutcome::Cancelled,
        ));
        let after_log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap_or_default();

        assert!(!handled);
        assert!(app.error.is_none());
        assert_eq!(before_log, after_log);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn report_error_records_when_ui_is_disabled() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = false;
        app.show_error_toasts = false;
        app.show_inline_errors = false;

        app.report_error("test.recording", "record me");

        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert!(log.contains("[error:test.recording] record me"));
        assert_eq!(log.matches("record me").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn recycle_error_path_uses_unified_reporting() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = false;
        app.show_error_toasts = false;
        app.show_inline_errors = false;

        send_event(WatchEvent::Recycle(Err("boom".into())));
        app.process_watch_events();

        assert!(app.error.is_none());
        let log = std::fs::read_to_string(TOAST_LOG_FILE).unwrap();
        assert!(log.contains("[error:recycle.empty] Failed to empty recycle bin: boom"));
        assert_eq!(log.matches("Failed to empty recycle bin: boom").count(), 1);

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn action_search_remains_case_insensitive_with_cached_aliases() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.fuzzy_weight = 0.0;
        app.actions = Arc::new(vec![Action {
            label: "Sample Action".into(),
            desc: "Demo description".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();
        app.bookmark_aliases
            .insert("demo:action".into(), Some("MiXeDCaSe Alias".into()));
        app.bookmark_aliases_lc
            .insert("demo:action".into(), Some("mixedcase alias".into()));

        app.query = "app SAMPLE".into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));

        app.query = "app MIXEDCASE".into();
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn exact_display_match_uses_pre_normalized_query_substring() {
        let cached = CachedSearchEntry {
            label_lc: "testingeve123".into(),
            desc_lc: String::new(),
            action_lc: String::new(),
        };
        assert!(LauncherApp::matches_exact_display_text(
            &cached,
            &"Eve".to_lowercase()
        ));
        assert!(LauncherApp::matches_exact_display_text(&cached, "eve"));
        assert!(!LauncherApp::matches_exact_display_text(&cached, "night"));
    }

    #[test]
    fn action_and_command_search_cache_is_normalized() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![Action {
            label: "MiXeD Label".into(),
            desc: "MiXeD Desc".into(),
            action: "Action:ID".into(),
            args: None,
        }]);
        app.update_action_cache();

        assert_eq!(app.action_cache.len(), 1);
        assert_eq!(app.action_cache[0].label_lc, "mixed label");
        assert_eq!(app.action_cache[0].desc_lc, "mixed desc");
        assert_eq!(app.action_cache[0].action_lc, "action:id");

        app.plugins.register(Box::new(ExactFilterPlugin));
        app.update_command_cache();
        assert_eq!(app.command_cache.len(), app.command_search_cache.len());
        for (action, cached) in app
            .command_cache
            .iter()
            .zip(app.command_search_cache.iter())
        {
            assert_eq!(cached.label_lc, action.label.to_lowercase());
            assert_eq!(cached.desc_lc, action.desc.to_lowercase());
            assert_eq!(cached.action_lc, action.action.to_lowercase());
        }
    }

    #[test]
    fn match_exact_overrides_fuzzy_scoring() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.fuzzy_weight = 1.0;
        app.actions = Arc::new(vec![Action {
            label: "testing123".into(),
            desc: "Demo description".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();

        app.query = "app tstng123".into();
        app.match_exact = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));

        app.query = "app tstng123".into();
        app.match_exact = true;
        app.last_results_valid = false;
        app.search();
        assert!(!app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn exact_mode_keeps_plugin_resolved_results_but_filters_query_suggestions() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.match_exact = true;
        app.plugins.register(Box::new(ExactFilterPlugin));

        app.query = "note today".into();
        app.search();
        assert!(
            app.results
                .iter()
                .any(|a| a.action == "note:new:2025-02-23")
        );

        app.query = "note search alpha".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "note:open:alpha"));

        app.query = "note zz".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.is_empty());
    }

    #[test]
    fn update_paths_refreshes_command_search_cache_for_plugin_reload() {
        struct CommandPlugin;

        impl crate::plugin::Plugin for CommandPlugin {
            fn search(&self, _query: &str) -> Vec<Action> {
                Vec::new()
            }

            fn name(&self) -> &str {
                "command-plugin"
            }

            fn description(&self) -> &str {
                "command plugin"
            }

            fn capabilities(&self) -> &[&str] {
                &[]
            }

            fn commands(&self) -> Vec<Action> {
                vec![Action {
                    label: "PlUgIn Command".into(),
                    desc: "PlUgIn Desc".into(),
                    action: "Plugin:Command".into(),
                    args: None,
                }]
            }
        }

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.plugins.register(Box::new(CommandPlugin));

        app.update_paths(
            None, // plugin_dirs
            None, // index_paths
            None, // enabled_plugins
            None, // enabled_capabilities
            None, // offscreen_pos
            None, // enable_toasts
            None, // show_inline_errors
            None, // show_error_toasts
            None, // toast_duration
            None, // fuzzy_weight
            None, // usage_weight
            None, // match_exact
            None, // follow_mouse
            None, // static_enabled
            None, // static_pos
            None, // static_size
            None, // hide_after_run
            None, // clear_query_after_run
            None, // require_confirm_destructive
            None, // timer_refresh
            None, // disable_timer_updates
            None, // preserve_command
            None, // query_autocomplete
            None, // net_refresh
            None, // net_unit
            None, // screenshot_dir
            None, // screenshot_save_file
            None, // screenshot_use_editor
            None, // screenshot_auto_save
            None, // always_on_top
            None, // page_jump
            None, // note_settings
            None, // note_panel_default_size
            None, // note_save_on_close
            None, // note_always_overwrite
            None, // note_images_as_links
            None, // note_show_details
            None, // note_more_limit
            None, // show_dashboard_diagnostics
        );

        assert_eq!(app.command_cache.len(), 1);
        assert_eq!(app.command_search_cache.len(), 1);
        assert_eq!(app.command_search_cache[0].label_lc, "plugin command");
        assert_eq!(app.command_search_cache[0].desc_lc, "plugin desc");
        assert_eq!(app.command_search_cache[0].action_lc, "plugin:command");
    }

    #[test]
    fn update_paths_refreshes_note_settings() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        assert!(app.note_settings.split_view_enabled);

        let mut note_settings = app.note_settings.clone();
        note_settings.split_view_enabled = false;

        app.update_paths(
            None,                // plugin_dirs
            None,                // index_paths
            None,                // enabled_plugins
            None,                // enabled_capabilities
            None,                // offscreen_pos
            None,                // enable_toasts
            None,                // show_inline_errors
            None,                // show_error_toasts
            None,                // toast_duration
            None,                // fuzzy_weight
            None,                // usage_weight
            None,                // match_exact
            None,                // follow_mouse
            None,                // static_enabled
            None,                // static_pos
            None,                // static_size
            None,                // hide_after_run
            None,                // clear_query_after_run
            None,                // require_confirm_destructive
            None,                // timer_refresh
            None,                // disable_timer_updates
            None,                // preserve_command
            None,                // query_autocomplete
            None,                // net_refresh
            None,                // net_unit
            None,                // screenshot_dir
            None,                // screenshot_save_file
            None,                // screenshot_use_editor
            None,                // screenshot_auto_save
            None,                // always_on_top
            None,                // page_jump
            Some(note_settings), // note_settings
            None,                // note_panel_default_size
            None,                // note_save_on_close
            None,                // note_always_overwrite
            None,                // note_images_as_links
            None,                // note_show_details
            None,                // note_more_limit
            None,                // show_dashboard_diagnostics
        );

        assert!(!app.note_settings.split_view_enabled);
    }

    #[test]
    fn malformed_note_new_action_reports_error_and_search_recovers() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.actions = Arc::new(vec![Action {
            label: "Sample Action".into(),
            desc: "Demo".into(),
            action: "demo:action".into(),
            args: None,
        }]);
        app.update_action_cache();

        app.activate_action(
            Action {
                label: "Malformed note".into(),
                desc: "Note".into(),
                action: "note:new:2025-02-23:today:extra".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(
            app.error
                .as_ref()
                .is_some_and(|msg| msg.contains("Malformed note action"))
        );

        app.query = "app sample".into();
        app.last_results_valid = false;
        app.search();
        assert!(app.results.iter().any(|a| a.action == "demo:action"));
    }

    #[test]
    fn watch_events_refresh_alias_and_lowercase_alias_caches() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let folders_json = serde_json::json!([{
            "label": "Folder",
            "path": "/tmp/folder-one",
            "alias": "MiXeDFolder"
        }]);
        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&folders_json).unwrap(),
        )
        .unwrap();

        let bookmarks_json = serde_json::json!([{
            "url": "https://example.com",
            "alias": "MiXeDBookmark"
        }]);
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&bookmarks_json).unwrap(),
        )
        .unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        assert_eq!(
            app.folder_aliases_lc.get("/tmp/folder-one"),
            Some(&Some("mixedfolder".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("mixedbookmark".into()))
        );

        let updated_folders_json = serde_json::json!([{
            "label": "Folder",
            "path": "/tmp/folder-one",
            "alias": "NewAlias"
        }]);
        std::fs::write(
            crate::plugins::folders::FOLDERS_FILE,
            serde_json::to_string_pretty(&updated_folders_json).unwrap(),
        )
        .unwrap();
        send_event(WatchEvent::Folders);
        app.process_watch_events();
        assert_eq!(
            app.folder_aliases.get("/tmp/folder-one"),
            Some(&Some("NewAlias".into()))
        );
        assert_eq!(
            app.folder_aliases_lc.get("/tmp/folder-one"),
            Some(&Some("newalias".into()))
        );

        let updated_bookmarks_json = serde_json::json!([{
            "url": "https://example.com",
            "alias": "OtherAlias"
        }]);
        std::fs::write(
            crate::plugins::bookmarks::BOOKMARKS_FILE,
            serde_json::to_string_pretty(&updated_bookmarks_json).unwrap(),
        )
        .unwrap();
        send_event(WatchEvent::Bookmarks);
        app.process_watch_events();
        assert_eq!(
            app.bookmark_aliases.get("https://example.com"),
            Some(&Some("OtherAlias".into()))
        );
        assert_eq!(
            app.bookmark_aliases_lc.get("https://example.com"),
            Some(&Some("otheralias".into()))
        );

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn watch_event_bursts_delay_completion_rebuild_until_debounce_window() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        std::fs::write(
            "actions.json",
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "label": "Initial App",
                    "desc": "demo",
                    "action": "initial:app",
                    "args": null
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.rebuild_completion_index_now();
        assert!(app.completion_index.is_some());

        std::fs::write(
            "actions.json",
            serde_json::to_string_pretty(&serde_json::json!([
                {
                    "label": "Updated App",
                    "desc": "demo",
                    "action": "updated:app",
                    "args": null
                }
            ]))
            .unwrap(),
        )
        .unwrap();

        send_event(WatchEvent::Actions);
        send_event(WatchEvent::Actions);
        app.process_watch_events();

        assert!(app.completion_index.is_none());
        assert!(app.action_completion_dirty);

        let scheduled = app
            .completion_rebuild_after
            .expect("rebuild should be scheduled");
        app.maybe_rebuild_completion_index(scheduled - Duration::from_millis(1));
        assert!(app.completion_index.is_none());

        app.maybe_rebuild_completion_index(scheduled + Duration::from_millis(1));
        assert!(app.completion_index.is_some());
        assert!(!app.action_completion_dirty);
        assert!(!app.command_completion_dirty);
        assert!(app.completion_rebuild_after.is_none());

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn completion_suggestions_clear_until_rebuild_and_match_latest_entries() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;

        app.actions = Arc::new(vec![Action {
            label: "Old App".into(),
            desc: "demo".into(),
            action: "old:app".into(),
            args: None,
        }]);
        app.update_action_cache();
        app.rebuild_completion_index_now();

        app.query = "app ".into();
        app.update_suggestions();
        assert!(app.suggestions.iter().any(|s| s == "app old app"));

        app.actions = Arc::new(vec![Action {
            label: "New App".into(),
            desc: "demo".into(),
            action: "new:app".into(),
            args: None,
        }]);
        app.update_action_cache();

        assert!(app.completion_index.is_none());
        assert!(app.suggestions.is_empty());

        app.maybe_rebuild_completion_index(
            Instant::now() + COMPLETION_REBUILD_DEBOUNCE + Duration::from_millis(1),
        );

        assert!(app.suggestions.iter().all(|s| s != "app old app"));
        assert!(app.suggestions.iter().any(|s| s == "app new app"));
    }

    #[test]
    fn open_note_panel_reuses_existing_panel_for_same_slug() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let dir = tempdir().unwrap();
        let prev = std::env::var("ML_NOTES_DIR").ok();
        unsafe { std::env::set_var("ML_NOTES_DIR", dir.path()) };

        append_note("Second Note", "body").unwrap();
        app.open_note_panel("second-note", None);
        app.open_note_panel("second-note", None);

        assert_eq!(app.note_panels.len(), 1);

        if let Some(prev) = prev {
            unsafe { std::env::set_var("ML_NOTES_DIR", prev) };
        } else {
            unsafe { std::env::remove_var("ML_NOTES_DIR") };
        }
    }

    #[test]
    fn open_note_link_valid_and_invalid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("https://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 1);
        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("www.example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 1);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("http://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("ftp://example.com");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);

        super::OPEN_LINK_COUNT.store(0, Ordering::SeqCst);
        app.open_note_link("internal-note");
        assert_eq!(super::OPEN_LINK_COUNT.load(Ordering::SeqCst), 0);
        assert_eq!(app.note_panels.len(), 1);
    }

    #[test]
    fn tab_cycles_through_suggestions() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;
        app.suggestions = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];

        for expected in ["alpha", "beta", "gamma", "alpha"] {
            assert!(app.accept_suggestion(true));
            assert_eq!(app.query, expected);
            assert_eq!(
                app.suggestions,
                vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string(),]
            );
        }
    }

    #[test]
    fn pinned_panel_prevents_close() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.pinned_panels = vec![Panel::ClipboardDialog];
        let dir = tempdir().unwrap();
        let actions_path = dir.path().join("actions.json");
        let settings_path = dir.path().join("settings.json");
        let mut app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            actions_path.to_string_lossy().to_string(),
            settings_path.to_string_lossy().to_string(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(app.clipboard_dialog.open);
        assert!(!app.close_front_dialog());
        assert!(app.clipboard_dialog.open);
        app.toggle_pin(Panel::ClipboardDialog);
        assert!(!app.pinned_panels.contains(&Panel::ClipboardDialog));
        app.clipboard_dialog.open = true;
        app.update_panel_stack();
        assert!(app.close_front_dialog());
        assert!(!app.clipboard_dialog.open);
    }

    #[test]
    fn diagnostics_widget_layout_enables_context_flag() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "diagnostics".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        assert!(app.has_diagnostics_widget());
        assert!(app.show_dashboard_diagnostics || app.has_diagnostics_widget());
    }

    #[test]
    fn diagnostics_context_includes_snapshot_when_widget_present() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "diagnostics".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        let has_diagnostics_widget = app.has_diagnostics_widget();
        let show_diagnostics_widget = app.show_dashboard_diagnostics || has_diagnostics_widget;
        let diagnostics = if app.show_dashboard_diagnostics || has_diagnostics_widget {
            Some(app.dashboard.diagnostics_snapshot())
        } else {
            None
        };

        assert!(show_diagnostics_widget);
        assert!(diagnostics.is_some());
    }

    #[test]
    fn diagnostics_context_omits_snapshot_when_disabled_and_missing_widget() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.show_dashboard_diagnostics = false;
        app.dashboard.slots = vec![NormalizedSlot {
            id: None,
            widget: "notes".to_string(),
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 1,
            settings: json!({}),
            overflow: OverflowMode::Scroll,
        }];

        let has_diagnostics_widget = app.has_diagnostics_widget();
        let show_diagnostics_widget = app.show_dashboard_diagnostics || has_diagnostics_widget;
        let diagnostics = if app.show_dashboard_diagnostics || has_diagnostics_widget {
            Some(app.dashboard.diagnostics_snapshot())
        } else {
            None
        };

        assert!(!show_diagnostics_widget);
        assert!(diagnostics.is_none());
    }

    #[test]
    fn image_panel_closes_with_escape() {
        let ctx = egui::Context::default();
        let dir = tempdir().unwrap();
        let path = dir.path().join("img.png");
        RgbaImage::new(1, 1).save(&path).unwrap();

        let mut app = new_app(&ctx);
        app.open_image_panel(&path);
        assert_eq!(app.image_panels.len(), 1);
        assert!(app.close_front_dialog());
        assert!(app.image_panels.is_empty());
    }

    #[test]
    fn open_note_in_neovim_resolves_and_handles_errors() {
        use crate::plugins::note::Note;
        use anyhow::anyhow;
        use std::cell::Cell;

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);

        let note_path = tempdir().unwrap().path().join("alpha.md");
        let load_called = Cell::new(false);
        let spawn_called = Cell::new(false);

        let closed = app.open_note_in_neovim(
            "alpha",
            || {
                load_called.set(true);
                Ok(vec![Note {
                    title: "alpha".into(),
                    path: note_path.clone(),
                    content: String::new(),
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: "alpha".into(),
                    alias: None,
                    aliases: Vec::new(),
                    entity_refs: Vec::new(),
                }])
            },
            |p| {
                spawn_called.set(true);
                assert_eq!(p, &note_path);
                Ok(())
            },
        );
        assert!(closed);
        assert!(load_called.get());
        assert!(spawn_called.get());
        assert!(app.error.is_none());

        spawn_called.set(false);
        app.error = None;
        let closed = app.open_note_in_neovim(
            "missing",
            || Ok(Vec::new()),
            |_| {
                spawn_called.set(true);
                Ok(())
            },
        );
        assert!(closed);
        assert!(!spawn_called.get());
        assert_eq!(app.error.as_deref(), Some("Note not found"));

        spawn_called.set(false);
        app.error = None;
        let closed = app.open_note_in_neovim(
            "alpha",
            || Err(anyhow!("load failure")),
            |_| {
                spawn_called.set(true);
                Ok(())
            },
        );
        assert!(closed);
        assert!(!spawn_called.get());
        assert!(app.error.as_ref().unwrap().contains("load failure"));
    }

    #[test]
    fn delete_note_uses_alias_and_logs_message() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\nAlias: special-name\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.plugins.register(Box::new(NotePlugin::default()));
        app.enable_toasts = true;

        app.query = "note list".into();
        app.search();

        app.delete_note("special-name");
        assert!(load_notes().unwrap().is_empty());
        if let Some(p) = app.pending_query.take() {
            app.query = p;
        }
        app.last_results_valid = false;
        app.search();
        assert!(!app.results.iter().any(|a| a.action == "note:open:alpha"));
        let log_path = dir.path().join(TOAST_LOG_FILE);
        let log = std::fs::read_to_string(log_path).unwrap();
        assert!(log.contains("Removed note special-name"));

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn destructive_note_delete_is_queued_when_confirmation_required() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = true;

        app.activate_action(
            Action {
                label: "Delete note".into(),
                desc: "Notes".into(),
                action: "note:remove:alpha".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(app.pending_confirm.is_some());
        assert_eq!(load_notes().unwrap().len(), 1);

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn destructive_note_delete_executes_only_after_confirmation() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        let orig_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        std::env::set_current_dir(dir.path()).unwrap();
        save_notes(&[]).unwrap();
        reset_slug_lookup();
        append_note("alpha", "# alpha\n\ncontent").unwrap();

        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.require_confirm_destructive = true;

        app.activate_action(
            Action {
                label: "Delete note".into(),
                desc: "Notes".into(),
                action: "note:remove:alpha".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert_eq!(load_notes().unwrap().len(), 1);
        let pending = app.pending_confirm.take().expect("pending confirm action");
        app.activate_action_confirmed(pending.action, pending.query_override, pending.source);
        assert!(load_notes().unwrap().is_empty());

        std::env::set_current_dir(orig_dir).unwrap();
    }

    #[test]
    fn note_graph_dialog_action_opens_dialog() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        assert!(!app.note_graph_dialog.open);

        app.activate_action(
            Action {
                label: "Open note graph".into(),
                desc: "Note".into(),
                action: "note:graph_dialog".into(),
                args: Some(r#"{"include_tags":["foo"],"root":"alpha"}"#.into()),
            },
            None,
            ActivationSource::Enter,
        );

        assert!(app.note_graph_dialog.open);
    }

    #[test]
    fn note_search_debounce_respects_delay() {
        let start = Instant::now();
        assert!(!LauncherApp::note_search_debounce_ready(
            Some(start),
            start,
            NOTE_SEARCH_DEBOUNCE,
        ));
        assert!(!LauncherApp::note_search_debounce_ready(
            Some(start),
            start + Duration::from_millis(999),
            NOTE_SEARCH_DEBOUNCE,
        ));
        assert!(LauncherApp::note_search_debounce_ready(
            Some(start),
            start + NOTE_SEARCH_DEBOUNCE,
            NOTE_SEARCH_DEBOUNCE,
        ));
    }
    #[test]
    fn launcher_new_shares_file_search_settings_with_dialog_and_coordinator() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        let file_search_settings = crate::file_search::settings::FileSearchSettings {
            global_search_roots: vec![std::path::PathBuf::from("/tmp/search-root")],
            excluded_directory_names: vec!["skip-me".to_owned()],
            max_search_results: 123,
            max_matches_per_content_file: 7,
            max_content_search_file_size_bytes: 456_789,
            include_hidden_files: true,
            case_sensitive: true,
            everything_executable_path: std::path::PathBuf::from("custom-everything.exe"),
            ripgrep_executable_path: std::path::PathBuf::from("custom-rg"),
            everything_enabled: false,
            ..crate::file_search::settings::FileSearchSettings::default()
        };
        settings.plugin_settings.insert(
            "file_search".to_owned(),
            serde_json::to_value(file_search_settings.clone()).expect("serialize settings"),
        );

        let app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        assert_eq!(app.file_search_dialog.settings, file_search_settings);
        assert_eq!(
            app.file_search_dialog.case_sensitive,
            file_search_settings.case_sensitive
        );
        assert_eq!(
            app.file_search_dialog.include_hidden,
            file_search_settings.include_hidden_files
        );
        assert_eq!(
            app.file_search_coordinator.production_settings(),
            Some(&file_search_settings)
        );
    }

    #[test]
    fn launcher_new_normalizes_conflicting_follow_mouse_static_config() {
        let ctx = egui::Context::default();
        let mut settings = Settings::default();
        settings.follow_mouse = true;
        settings.static_location_enabled = true;
        settings.static_pos = Some((10, 20));
        settings.static_size = Some((500, 400));

        let app = LauncherApp::new(
            &ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );

        assert!(app.follow_mouse);
        assert!(!app.static_location_enabled);
        assert_eq!(app.static_pos, None);
        assert_eq!(app.static_size, None);
    }

    #[test]
    fn grid_context_menu_eligibility_uses_result_actions() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.resolved_grid_layout = true;
        app.bookmark_aliases
            .insert("https://example.com".into(), Some("Example".into()));
        app.results = vec![Action {
            label: "Example".into(),
            desc: "Bookmark".into(),
            action: "https://example.com".into(),
            args: None,
        }];

        let kind = app.result_context_menu_kind(&app.results[0]);
        assert_eq!(kind, ResultContextMenuKind::Bookmark);
    }

    #[test]
    fn context_menu_parity_bookmark_between_list_and_grid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "Docs".into(),
            desc: "Bookmark".into(),
            action: "https://docs.rs".into(),
            args: None,
        };
        app.bookmark_aliases
            .insert(action.action.clone(), Some("docs".into()));

        app.resolved_grid_layout = false;
        let list_kind = app.result_context_menu_kind(&action);
        app.resolved_grid_layout = true;
        let grid_kind = app.result_context_menu_kind(&action);

        assert_eq!(list_kind, ResultContextMenuKind::Bookmark);
        assert_eq!(grid_kind, list_kind);
    }

    #[test]
    fn context_menu_parity_todo_between_list_and_grid() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let action = Action {
            label: "[ ] parity".into(),
            desc: "Todo".into(),
            action: "todo:done:7".into(),
            args: None,
        };

        app.resolved_grid_layout = false;
        let list_kind = app.result_context_menu_kind(&action);
        app.resolved_grid_layout = true;
        let grid_kind = app.result_context_menu_kind(&action);

        assert_eq!(list_kind, ResultContextMenuKind::Todo { idx: 7 });
        assert_eq!(grid_kind, list_kind);
    }
    #[test]
    fn handle_key_grid_navigation_arrows_and_numpad() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_results_layout.enabled = true;
        app.query_results_layout.cols = 3;
        app.query_results_layout.rows = 2;
        app.resolved_grid_layout = true;
        app.results = (0..8)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.selected = Some(4);
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(5));
        app.handle_key(egui::Key::ArrowLeft);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(4));

        app.handle_key(egui::Key::Num6);
        assert_eq!(app.selected, Some(5));
        app.handle_key(egui::Key::Num4);
        assert_eq!(app.selected, Some(4));
        app.handle_key(egui::Key::Num8);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::Num2);
        assert_eq!(app.selected, Some(4));

        app.selected = Some(7);
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(7));
    }

    #[test]
    fn handle_key_list_mode_remains_compatible() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.resolved_grid_layout = false;
        app.results = (0..4)
            .map(|i| Action {
                label: format!("A{i}"),
                desc: "d".into(),
                action: format!("act:{i}"),
                args: None,
            })
            .collect();

        app.selected = Some(1);
        app.handle_key(egui::Key::ArrowDown);
        assert_eq!(app.selected, Some(2));
        app.handle_key(egui::Key::ArrowUp);
        assert_eq!(app.selected, Some(1));
        app.handle_key(egui::Key::ArrowRight);
        assert_eq!(app.selected, Some(1));
    }

    #[test]
    fn launcher_enter_activation_requires_query_focus_and_closed_file_search() {
        assert!(!LauncherApp::launcher_enter_activation_enabled(
            false, false
        ));
        assert!(LauncherApp::launcher_enter_activation_enabled(true, false));
        assert!(!LauncherApp::launcher_enter_activation_enabled(true, true));
    }

    #[test]
    fn fs_query_text_does_not_imply_launcher_keyboard_ownership() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "fs".into();
        app.results = vec![Action {
            label: "File Search".into(),
            desc: "Open file search".into(),
            action: crate::file_search::actions::OPEN_ACTION.into(),
            args: None,
        }];
        app.selected = Some(0);

        assert!(!LauncherApp::launcher_query_keyboard_enabled(false));
        assert!(!LauncherApp::launcher_enter_activation_enabled(
            false,
            app.file_search_dialog.open
        ));
    }

    #[test]
    fn open_file_search_disables_launcher_enter_activation() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "fs".into();
        app.file_search_dialog.open = true;

        assert!(!LauncherApp::launcher_enter_activation_enabled(
            true,
            app.file_search_dialog.open
        ));
    }

    #[test]
    fn open_file_search_disables_launcher_wide_escape_handling() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.file_search_dialog.open = true;
        app.visible_flag.store(true, Ordering::SeqCst);

        assert!(!LauncherApp::launcher_escape_handling_enabled(
            app.file_search_dialog.open
        ));
        assert!(app.visible_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn launcher_query_is_not_refocused_while_file_search_remains_open() {
        assert!(!LauncherApp::launcher_query_focus_should_be_requested(
            true, false, true
        ));
        assert!(!LauncherApp::launcher_query_focus_should_be_requested(
            false, true, true
        ));
        assert!(LauncherApp::launcher_query_focus_should_be_requested(
            false, true, false
        ));
    }

    #[test]
    fn arrow_page_tab_navigation_requires_query_focus() {
        assert!(!LauncherApp::launcher_query_keyboard_enabled(false));
        assert!(LauncherApp::launcher_query_keyboard_enabled(true));
    }

    #[test]
    fn grid_layout_defaults_to_enabled_for_non_prefixed_queries() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "hello world".into();
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        app.recompute_query_results_layout();
        assert!(app.resolved_grid_layout);
    }

    #[test]
    fn grid_layout_selection_respects_plugin_capability_and_opt_out() {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query = "note hi".into();
        app.query_results_layout.enabled = true;
        app.query_results_layout.respect_plugin_capability = true;

        app.plugins.register(Box::new(TestPlugin {
            name: "note",
            caps: vec![CAP_GRID_RESULTS_COMPATIBLE],
            prefixes: vec!["note"],
        }));
        app.recompute_query_results_layout();
        assert!(app.resolved_grid_layout);

        app.query_results_layout.plugin_opt_out = vec!["note".into()];
        app.recompute_query_results_layout();
        assert!(!app.resolved_grid_layout);

        app.query_results_layout.plugin_opt_out.clear();
        app.plugins.clear_plugins();
        app.plugins.register(Box::new(TestPlugin {
            name: "note",
            caps: vec![CAP_FORCE_LIST_RESULTS],
            prefixes: vec!["note"],
        }));
        app.recompute_query_results_layout();
        assert!(!app.resolved_grid_layout);
    }

    #[test]
    fn failing_action_reports_error_and_launcher_stays_responsive() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.enable_toasts = true;

        set_execute_action_hook(Some(Box::new(|_| Err(anyhow::anyhow!("injected failure")))));

        app.activate_action(
            Action {
                label: "Broken".into(),
                desc: "Test".into(),
                action: "exec:broken".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );

        assert!(
            app.error
                .as_ref()
                .is_some_and(|msg| msg.contains("injected failure"))
        );
        assert!(app.error_time.is_some());

        set_execute_action_hook(Some(Box::new(|_| Ok(()))));
        app.activate_action(
            Action {
                label: "Healthy".into(),
                desc: "Test".into(),
                action: "exec:ok".into(),
                args: None,
            },
            None,
            ActivationSource::Enter,
        );
        assert!(app.error.is_some());

        set_execute_action_hook(None);
    }
}
