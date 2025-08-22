mod add_action_dialog;
mod add_bookmark_dialog;
mod alias_dialog;
mod bookmark_alias_dialog;
mod brightness_dialog;
mod clipboard_dialog;
mod convert_panel;
mod cpu_list_dialog;
mod fav_dialog;
mod image_panel;
mod macro_dialog;
mod note_panel;
mod notes_dialog;
mod shell_cmd_dialog;
mod snippet_dialog;
mod tempfile_alias_dialog;
mod tempfile_dialog;
mod timer_dialog;
mod toast_log_dialog;
mod todo_dialog;
mod todo_view_dialog;
mod unused_assets_dialog;
mod volume_dialog;

pub use add_action_dialog::AddActionDialog;
pub use add_bookmark_dialog::AddBookmarkDialog;
pub use alias_dialog::AliasDialog;
pub use bookmark_alias_dialog::BookmarkAliasDialog;
pub use brightness_dialog::BrightnessDialog;
pub use brightness_dialog::BRIGHTNESS_QUERIES;
pub use clipboard_dialog::ClipboardDialog;
pub use convert_panel::ConvertPanel;
pub use cpu_list_dialog::CpuListDialog;
pub use fav_dialog::FavDialog;
pub use image_panel::ImagePanel;
pub use macro_dialog::MacroDialog;
#[cfg(target_os = "windows")]
pub use note_panel::{build_nvim_command, extract_links, show_wiki_link, NotePanel};
#[cfg(not(target_os = "windows"))]
pub use note_panel::{extract_links, show_wiki_link, NotePanel};
pub use notes_dialog::NotesDialog;
pub use shell_cmd_dialog::ShellCmdDialog;
pub use snippet_dialog::SnippetDialog;
pub use tempfile_alias_dialog::TempfileAliasDialog;
pub use tempfile_dialog::TempfileDialog;
pub use timer_dialog::{TimerCompletionDialog, TimerDialog};
pub use toast_log_dialog::ToastLogDialog;
pub use todo_dialog::TodoDialog;
pub use todo_view_dialog::TodoViewDialog;
pub use unused_assets_dialog::UnusedAssetsDialog;
pub use volume_dialog::VolumeDialog;

use crate::actions::folders;
use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::help_window::HelpWindow;
use crate::history::{self, HistoryEntry};
use crate::indexer;
use crate::launcher::launch_action;
use crate::plugin::PluginManager;
use crate::plugin_editor::PluginEditor;
use crate::plugins::note::{NoteExternalOpen, NotePluginSettings};
use crate::plugins::snippets::{remove_snippet, SNIPPETS_FILE};
use crate::settings::Settings;
use crate::settings_editor::SettingsEditor;
use crate::toast_log::{append_toast_log, TOAST_LOG_FILE};
use crate::usage::{self, USAGE_FILE};
use crate::visibility::apply_visibility;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use fst::{IntoStreamer, Map, MapBuilder, Streamer};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use url::Url;

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

#[derive(Clone)]
pub enum WatchEvent {
    Actions,
    Folders,
    Bookmarks,
    Recycle(Result<(), String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestWatchEvent {
    Actions,
    Folders,
    Bookmarks,
}

impl From<WatchEvent> for TestWatchEvent {
    fn from(value: WatchEvent) -> Self {
        match value {
            WatchEvent::Actions => TestWatchEvent::Actions,
            WatchEvent::Folders => TestWatchEvent::Folders,
            WatchEvent::Bookmarks => TestWatchEvent::Bookmarks,
            WatchEvent::Recycle(_) => unreachable!(),
        }
    }
}

fn watch_file(
    path: &Path,
    tx: Sender<WatchEvent>,
    event: WatchEvent,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<notify::Event>| match res {
            Ok(ev) => {
                if matches!(
                    ev.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.send(event.clone());
                }
            }
            Err(e) => tracing::error!("watch error: {:?}", e),
        },
        Config::default(),
    )?;
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .or_else(|_| {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            watcher.watch(parent, RecursiveMode::NonRecursive)
        })?;
    Ok(watcher)
}

fn push_toast(toasts: &mut Toasts, toast: Toast) {
    append_toast_log(toast.text.text());
    toasts.add(toast);
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
    FavDialog,
    NotesDialog,
    UnusedAssetsDialog,
    NotePanel,
    ImagePanel,
    TodoDialog,
    TodoViewDialog,
    ClipboardDialog,
    ConvertPanel,
    VolumeDialog,
    BrightnessDialog,
    CpuListDialog,
    ToastLogDialog,
    Editor,
    Settings,
    Plugins,
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
    fav_dialog: bool,
    notes_dialog: bool,
    unused_assets_dialog: bool,
    note_panel: bool,
    image_panel: bool,
    todo_dialog: bool,
    todo_view_dialog: bool,
    clipboard_dialog: bool,
    convert_panel: bool,
    volume_dialog: bool,
    brightness_dialog: bool,
    cpu_list_dialog: bool,
    toast_log_dialog: bool,
    editor: bool,
    settings: bool,
    plugins: bool,
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
    action_cache: Vec<(String, String)>,
    command_cache: Vec<Action>,
    completion_index: Option<Map<Vec<u8>>>,
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
    #[cfg(target_os = "windows")]
    pub registered_hotkeys: Mutex<HashMap<String, usize>>,
    pub show_editor: bool,
    pub show_settings: bool,
    pub show_plugins: bool,
    pub actions_path: String,
    pub editor: ActionsEditor,
    pub settings_editor: SettingsEditor,
    pub plugin_editor: PluginEditor,
    pub settings_path: String,
    /// Hold watchers so the `RecommendedWatcher` instances remain active.
    #[allow(dead_code)] // required to keep watchers alive
    watchers: Vec<RecommendedWatcher>,
    rx: Receiver<WatchEvent>,
    folder_aliases: HashMap<String, Option<String>>,
    bookmark_aliases: HashMap<String, Option<String>>,
    plugin_dirs: Option<Vec<String>>,
    index_paths: Option<Vec<String>>,
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
    fav_dialog: FavDialog,
    notes_dialog: NotesDialog,
    unused_assets_dialog: UnusedAssetsDialog,
    note_panels: Vec<NotePanel>,
    image_panels: Vec<ImagePanel>,
    todo_dialog: TodoDialog,
    todo_view_dialog: TodoViewDialog,
    clipboard_dialog: ClipboardDialog,
    convert_panel: ConvertPanel,
    volume_dialog: VolumeDialog,
    brightness_dialog: BrightnessDialog,
    cpu_list_dialog: CpuListDialog,
    toast_log_dialog: ToastLogDialog,
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
    pub page_jump: usize,
    pub note_panel_default_size: (f32, f32),
    pub note_save_on_close: bool,
    pub note_always_overwrite: bool,
    pub note_images_as_links: bool,
    pub note_external_editor: Option<String>,
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
    pub query_autocomplete: bool,
    pub net_refresh: f32,
    pub net_unit: crate::settings::NetUnit,
    pub screenshot_dir: Option<String>,
    pub screenshot_save_file: bool,
    last_timer_update: Instant,
    last_net_update: Instant,
    last_stopwatch_update: Instant,
    last_search_query: String,
    last_results_valid: bool,
    last_timer_query: bool,
    last_stopwatch_query: bool,
    pending_query: Option<String>,
    pub vim_mode: bool,
}

impl LauncherApp {
    pub fn update_action_cache(&mut self) {
        self.action_cache = self
            .actions
            .iter()
            .map(|a| (a.label.to_lowercase(), a.desc.to_lowercase()))
            .collect();
        self.update_completion_index();
    }

    pub fn update_command_cache(&mut self) {
        let mut cmds = self
            .plugins
            .commands_filtered(self.enabled_plugins.as_ref());
        cmds.sort_by_cached_key(|a| a.label.to_lowercase());
        self.command_cache = cmds;
        self.update_completion_index();
    }

    fn update_completion_index(&mut self) {
        let mut entries: Vec<String> = Vec::new();
        entries.extend(self.command_cache.iter().map(|a| a.label.to_lowercase()));
        for a in self.actions.iter() {
            entries.push(format!("app {}", a.label.to_lowercase()));
        }
        entries.sort();
        entries.dedup();
        let mut builder = MapBuilder::memory();
        for (i, k) in entries.iter().enumerate() {
            if let Err(e) = builder.insert(k, i as u64) {
                tracing::warn!(key = %k, ?e, "failed to insert key into completion index");
            }
        }
        let map = Map::new(builder.into_inner().unwrap()).unwrap();
        self.completion_index = Some(map);
        self.update_suggestions();
    }

    fn update_suggestions(&mut self) {
        self.suggestions.clear();
        self.autocomplete_index = 0;
        if !self.query_autocomplete || self.query.is_empty() {
            return;
        }
        if let Some(ref index) = self.completion_index {
            let q = self.query.to_lowercase();
            let mut stream = index.range().ge(q.as_str()).into_stream();
            while let Some((k, _)) = stream.next() {
                let key = std::str::from_utf8(k).unwrap();
                if !key.starts_with(&q) {
                    break;
                }
                if key != q {
                    self.suggestions.push(key.to_string());
                }
                if self.suggestions.len() >= 5 {
                    break;
                }
            }
        }
    }

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

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
        self.error_time = Some(Instant::now());
    }
    pub fn update_paths(
        &mut self,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<HashSet<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        offscreen_pos: Option<(i32, i32)>,
        enable_toasts: Option<bool>,
        toast_duration: Option<f32>,
        fuzzy_weight: Option<f32>,
        usage_weight: Option<f32>,
        follow_mouse: Option<bool>,
        static_enabled: Option<bool>,
        static_pos: Option<(i32, i32)>,
        static_size: Option<(i32, i32)>,
        hide_after_run: Option<bool>,
        timer_refresh: Option<f32>,
        disable_timer_updates: Option<bool>,
        preserve_command: Option<bool>,
        query_autocomplete: Option<bool>,
        net_refresh: Option<f32>,
        net_unit: Option<crate::settings::NetUnit>,
        screenshot_dir: Option<String>,
        screenshot_save_file: Option<bool>,
        always_on_top: Option<bool>,
        page_jump: Option<usize>,
        note_panel_default_size: Option<(f32, f32)>,
        note_save_on_close: Option<bool>,
        note_always_overwrite: Option<bool>,
        note_images_as_links: Option<bool>,
        note_more_limit: Option<usize>,
        note_external_editor: Option<String>,
    ) {
        self.plugin_dirs = plugin_dirs;
        self.index_paths = index_paths;
        self.enabled_plugins = enabled_plugins;
        if self.enabled_plugins.is_some() {
            self.update_command_cache();
        }
        self.enabled_capabilities = enabled_capabilities;
        if let Some((x, y)) = offscreen_pos {
            self.offscreen_pos = (x as f32, y as f32);
        }
        if let Some(v) = enable_toasts {
            self.enable_toasts = v;
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
        if let Some(v) = follow_mouse {
            self.follow_mouse = v;
        }
        if let Some(v) = static_enabled {
            self.static_location_enabled = v;
        }
        if static_pos.is_some() {
            self.static_pos = static_pos;
        }
        if static_size.is_some() {
            self.static_size = static_size;
        }
        if let Some(v) = hide_after_run {
            self.hide_after_run = v;
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
        if let Some(v) = always_on_top {
            self.always_on_top = v;
        }
        if let Some(v) = page_jump {
            self.page_jump = v;
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
        if let Some(v) = note_more_limit {
            self.note_more_limit = v;
        }
        if note_external_editor.is_some() {
            self.note_external_editor = note_external_editor;
        }
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
        let toast_duration = settings.toast_duration;
        use std::path::Path;

        let folder_aliases =
            crate::plugins::folders::load_folders(crate::plugins::folders::FOLDERS_FILE)
                .unwrap_or_else(|_| crate::plugins::folders::default_folders())
                .into_iter()
                .map(|f| (f.path, f.alias))
                .collect::<HashMap<_, _>>();
        let bookmark_aliases =
            crate::plugins::bookmarks::load_bookmarks(crate::plugins::bookmarks::BOOKMARKS_FILE)
                .unwrap_or_default()
                .into_iter()
                .map(|b| (b.url, b.alias))
                .collect::<HashMap<_, _>>();

        #[cfg(not(test))]
        match watch_file(Path::new(&actions_path), tx.clone(), WatchEvent::Actions) {
            Ok(w) => watchers.push(w),
            Err(e) => {
                tracing::error!("watch error: {:?}", e);
                push_toast(
                    &mut toasts,
                    Toast {
                        text: format!("Failed to watch {}", actions_path).into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
                push_toast(
                    &mut toasts,
                    Toast {
                        text: format!("Failed to watch {}", actions_path).into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch folders.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch folders.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch bookmarks.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
                push_toast(
                    &mut toasts,
                    Toast {
                        text: "Failed to watch bookmarks.json".into(),
                        kind: ToastKind::Error,
                        options: ToastOptions::default().duration_in_seconds(toast_duration as f64),
                    },
                );
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
        let static_enabled = settings.static_location_enabled;

        let note_external_open = settings
            .plugin_settings
            .get("note")
            .and_then(|v| serde_json::from_value::<NotePluginSettings>(v.clone()).ok())
            .map(|s| s.external_open)
            .unwrap_or(NoteExternalOpen::Neither);

        let settings_editor = SettingsEditor::new_with_plugins(&settings);
        let plugin_editor = PluginEditor::new(&settings);
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
            #[cfg(target_os = "windows")]
            registered_hotkeys: Mutex::new(HashMap::new()),
            show_editor: false,
            show_settings: false,
            show_plugins: false,
            actions_path,
            editor: ActionsEditor::default(),
            settings_editor,
            plugin_editor,
            settings_path,
            watchers,
            rx,
            folder_aliases,
            bookmark_aliases,
            plugin_dirs,
            index_paths,
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
            fav_dialog: FavDialog::default(),
            notes_dialog: NotesDialog::default(),
            unused_assets_dialog: UnusedAssetsDialog::default(),
            note_panels: Vec::new(),
            image_panels: Vec::new(),
            todo_dialog: TodoDialog::default(),
            todo_view_dialog: TodoViewDialog::default(),
            clipboard_dialog: ClipboardDialog::default(),
            convert_panel: ConvertPanel::default(),
            volume_dialog: VolumeDialog::default(),
            brightness_dialog: BrightnessDialog::default(),
            cpu_list_dialog: CpuListDialog::default(),
            toast_log_dialog: ToastLogDialog::default(),
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
            page_jump: settings.page_jump,
            note_panel_default_size: settings.note_panel_default_size,
            note_save_on_close: settings.note_save_on_close,
            note_always_overwrite: settings.note_always_overwrite,
            note_images_as_links: settings.note_images_as_links,
            note_external_editor: settings.note_external_editor.clone(),
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
            query_autocomplete: settings.query_autocomplete,
            net_refresh: settings.net_refresh,
            net_unit: settings.net_unit,
            screenshot_dir: settings.screenshot_dir.clone(),
            screenshot_save_file: settings.screenshot_save_file,
            last_timer_update: Instant::now(),
            last_net_update: Instant::now(),
            last_stopwatch_update: Instant::now(),
            last_search_query: String::new(),
            last_results_valid: false,
            last_timer_query: false,
            last_stopwatch_query: false,
            pending_query: None,
            action_cache: Vec::new(),
            command_cache: Vec::new(),
            completion_index: None,
            suggestions: Vec::new(),
            autocomplete_index: 0,
            vim_mode: false,
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

        #[cfg(target_os = "windows")]
        {
            use crate::global_hotkey::Hotkey as WinHotkey;
            if let Some(ref seq) = settings.hotkey {
                if let Ok(mut hk) = WinHotkey::new(seq) {
                    hk.register(&app, 1);
                }
            }
            if let Some(ref seq) = settings.quit_hotkey {
                if let Ok(mut hk) = WinHotkey::new(seq) {
                    hk.register(&app, 2);
                }
            }
        }

        app.update_action_cache();
        app.update_command_cache();
        app.search();
        app
    }

    pub fn search(&mut self) {
        if self.last_results_valid && self.query == self.last_search_query {
            self.selected = None;
            return;
        }

        let trimmed = self.query.trim();
        let trimmed_lc = trimmed.to_lowercase();
        self.last_timer_query =
            trimmed.starts_with("timer list") || trimmed.starts_with("alarm list");
        self.last_stopwatch_query = trimmed.starts_with("sw list");
        if trimmed.is_empty() {
            let mut res = self.command_cache.clone();
            for a in self.actions.iter() {
                res.push(Action {
                    label: format!("app {}", a.label),
                    desc: a.desc.clone(),
                    action: a.action.clone(),
                    args: a.args.clone(),
                });
            }
            self.results = res;
            self.selected = None;
            return;
        }

        let mut res: Vec<(Action, f32)> = Vec::new();

        let search_actions =
            trimmed_lc == APP_PREFIX || trimmed_lc.starts_with(&format!("{} ", APP_PREFIX));
        let action_query = if search_actions {
            if trimmed_lc == APP_PREFIX {
                "".to_string()
            } else {
                trimmed.splitn(2, ' ').nth(1).unwrap_or("").to_string()
            }
        } else {
            String::new()
        };
        let action_query_lc = action_query.to_lowercase();

        if trimmed_lc.starts_with("g ") {
            res.extend(self.search_plugins(trimmed, &trimmed_lc));
        } else {
            if search_actions {
                res.extend(self.search_actions(&action_query, &action_query_lc));
            }
            res.extend(self.search_plugins(trimmed, &trimmed_lc));
        }

        self.apply_usage_weight(&mut res);

        res.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        self.results = res.into_iter().map(|(a, _)| a).collect();
        self.selected = None;
        self.last_search_query = self.query.clone();
        self.last_results_valid = true;
        self.update_suggestions();
    }

    fn search_actions(&self, query: &str, query_lc: &str) -> Vec<(Action, f32)> {
        let mut res = Vec::new();
        if query.is_empty() {
            res.extend(self.actions.iter().cloned().map(|a| (a, 0.0)));
        } else {
            for (i, a) in self.actions.iter().enumerate() {
                let (ref label_lc, ref desc_lc) = self.action_cache[i];
                if self.fuzzy_weight <= 0.0 {
                    let alias_match = self
                        .folder_aliases
                        .get(&a.action)
                        .or_else(|| self.bookmark_aliases.get(&a.action))
                        .and_then(|v| v.as_ref())
                        .map(|s| s.to_lowercase().contains(query_lc))
                        .unwrap_or(false);
                    let label_match = label_lc.contains(query_lc);
                    let desc_match = desc_lc.contains(query_lc);
                    if label_match || desc_match || alias_match {
                        let score = if alias_match { 1.0 } else { 0.0 };
                        res.push((a.clone(), score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, query);
                    let s2 = self.matcher.fuzzy_match(&a.desc, query);
                    if let Some(score) = s1.max(s2) {
                        res.push((a.clone(), score as f32 * self.fuzzy_weight));
                    }
                }
            }
        }
        res
    }

    fn search_plugins(&self, trimmed: &str, trimmed_lc: &str) -> Vec<(Action, f32)> {
        let mut res = Vec::new();
        if trimmed_lc.starts_with("g ") {
            let filter = std::collections::HashSet::from(["web_search".to_string()]);
            let plugin_results = self.plugins.search_filtered(
                &self.query,
                Some(&filter),
                self.enabled_capabilities.as_ref(),
            );
            let query_term = trimmed_lc.splitn(2, ' ').nth(1).unwrap_or("");
            for a in plugin_results {
                let label_lc = a.label.to_lowercase();
                let desc_lc = a.desc.to_lowercase();
                if self.fuzzy_weight <= 0.0 {
                    if query_term.is_empty() {
                        res.push((a, 0.0));
                    } else {
                        let alias_match = self
                            .folder_aliases
                            .get(&a.action)
                            .or_else(|| self.bookmark_aliases.get(&a.action))
                            .and_then(|v| v.as_ref())
                            .map(|s| s.to_lowercase().contains(query_term))
                            .unwrap_or(false);
                        let label_match = label_lc.contains(query_term);
                        let desc_match = desc_lc.contains(query_term);
                        if label_match || desc_match || alias_match {
                            let score = if alias_match { 1.0 } else { 0.0 };
                            res.push((a, score));
                        }
                    }
                } else {
                    let score = if self.query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, &self.query)
                            .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
            return res;
        }

        let plugin_results = self.plugins.search_filtered(
            &self.query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        );

        if plugin_results.is_empty() && !trimmed.is_empty() {
            for a in self
                .plugins
                .commands_filtered(self.enabled_plugins.as_ref())
            {
                let label_lc = a.label.to_lowercase();
                let desc_lc = a.desc.to_lowercase();
                if self.fuzzy_weight <= 0.0 {
                    let alias_match = self
                        .folder_aliases
                        .get(&a.action)
                        .or_else(|| self.bookmark_aliases.get(&a.action))
                        .and_then(|v| v.as_ref())
                        .map(|s| s.to_lowercase().contains(trimmed_lc))
                        .unwrap_or(false);
                    let label_match = label_lc.contains(trimmed_lc);
                    let desc_match = desc_lc.contains(trimmed_lc);
                    if label_match || desc_match || alias_match {
                        let score = if alias_match { 1.0 } else { 0.0 };
                        res.push((a, score));
                    }
                } else {
                    let s1 = self.matcher.fuzzy_match(&a.label, trimmed);
                    let s2 = self.matcher.fuzzy_match(&a.desc, trimmed);
                    if let Some(score) = s1.max(s2) {
                        res.push((a, score as f32 * self.fuzzy_weight));
                    }
                }
            }
        } else {
            let tail = trimmed_lc.splitn(2, " ").nth(1).unwrap_or("");
            let mut query_term = tail.splitn(3, " ").nth(1).unwrap_or("").to_string();
            if query_term.is_empty() {
                let parts: Vec<&str> = tail.split_whitespace().collect();
                if parts.len() == 1 && !SUBCOMMANDS.contains(&parts[0]) {
                    query_term = parts[0].to_string();
                } else if parts.len() > 1 {
                    query_term = parts[1..].join(" ");
                }
            }
            let query_term_lc = query_term.to_lowercase();
            for a in plugin_results {
                let label_lc = a.label.to_lowercase();
                let desc_lc = a.desc.to_lowercase();
                if self.fuzzy_weight <= 0.0 {
                    if query_term_lc.is_empty() {
                        res.push((a, 0.0));
                    } else {
                        let alias_match = self
                            .folder_aliases
                            .get(&a.action)
                            .or_else(|| self.bookmark_aliases.get(&a.action))
                            .and_then(|v| v.as_ref())
                            .map(|s| s.to_lowercase().contains(&query_term_lc))
                            .unwrap_or(false);
                        let label_match = label_lc.contains(&query_term_lc);
                        let desc_match = desc_lc.contains(&query_term_lc);
                        if label_match || desc_match || alias_match {
                            let score = if alias_match { 1.0 } else { 0.0 };
                            res.push((a, score));
                        }
                    }
                } else {
                    let score = if self.query.is_empty() {
                        0.0
                    } else {
                        self.matcher
                            .fuzzy_match(&a.label, &self.query)
                            .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                            .unwrap_or(0) as f32
                            * self.fuzzy_weight
                    };
                    res.push((a, score));
                }
            }
        }

        res
    }

    fn apply_usage_weight(&self, res: &mut Vec<(Action, f32)>) {
        for (a, score) in res.iter_mut() {
            *score += self.usage.get(&a.action).cloned().unwrap_or(0) as f32 * self.usage_weight;
        }
    }

    #[cfg_attr(test, allow(dead_code))]
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

    /// Handle a keyboard navigation key. Returns the index of a selected
    /// action when `Enter` is pressed and a selection is available.
    pub fn handle_key(&mut self, key: egui::Key) -> Option<usize> {
        match key {
            egui::Key::ArrowDown => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if i < max => Some(i + 1),
                        Some(i) => Some(i),
                        None => Some(0),
                    };
                }
                None
            }
            egui::Key::ArrowUp => {
                if !self.results.is_empty() {
                    let max = self.results.len() - 1;
                    self.selected = match self.selected {
                        Some(i) if i > 0 => Some(i - 1),
                        Some(i) => Some(i.min(max)),
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

    fn any_panel_open(&self) -> bool {
        self.alias_dialog.open
            || self.bookmark_alias_dialog.open
            || self.tempfile_alias_dialog.open
            || self.tempfile_dialog.open
            || self.add_bookmark_dialog.open
            || self.timer_dialog.open
            || self.shell_cmd_dialog.open
            || self.snippet_dialog.open
            || self.macro_dialog.open
            || self.fav_dialog.open
            || self.notes_dialog.open
            || self.unused_assets_dialog.open
            || !self.note_panels.is_empty()
            || !self.image_panels.is_empty()
            || self.todo_dialog.open
            || self.todo_view_dialog.open
            || self.clipboard_dialog.open
            || self.convert_panel.open
            || self.volume_dialog.open
            || self.brightness_dialog.open
            || self.cpu_list_dialog.open
            || self.toast_log_dialog.open
            || self.help_window.open
            || self.help_window.overlay_open
            || self.show_editor
            || self.show_settings
            || self.show_plugins
    }

    #[cfg(target_os = "windows")]
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

    #[cfg(not(target_os = "windows"))]
    pub fn unregister_all_hotkeys(&self) {}

    /// Return the currently configured screenshot directory, if any.
    pub fn get_screenshot_dir(&self) -> Option<&str> {
        self.screenshot_dir.as_deref()
    }

    /// Whether screenshots copied to the clipboard are also saved to disk.
    pub fn get_screenshot_save_file(&self) -> bool {
        self.screenshot_save_file
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
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut panel) = self.note_panels.pop() {
                    if self.note_save_on_close {
                        panel.save(self);
                    }
                }
                self.panel_states.note_panel = false;
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
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
            Panel::FavDialog => {
                self.fav_dialog.open = false;
                self.panel_states.fav_dialog = false;
            }
            Panel::NotesDialog => {
                self.notes_dialog.open = false;
                self.panel_states.notes_dialog = false;
            }
            Panel::UnusedAssetsDialog => {
                self.unused_assets_dialog.open = false;
                self.panel_states.unused_assets_dialog = false;
            }
            Panel::NotePanel => {
                if let Some(mut panel) = self.note_panels.pop() {
                    if self.note_save_on_close {
                        panel.save(self);
                    }
                }
                self.panel_states.note_panel = false;
            }
            Panel::ImagePanel => {
                let _ = self.image_panels.pop();
                self.panel_states.image_panel = false;
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
            Panel::FavDialog => self.fav_dialog.open = true,
            Panel::NotesDialog => self.notes_dialog.open = true,
            Panel::UnusedAssetsDialog => self.unused_assets_dialog.open = true,
            Panel::NotePanel => {}
            Panel::ImagePanel => {}
            Panel::TodoDialog => self.todo_dialog.open = true,
            Panel::TodoViewDialog => self.todo_view_dialog.open = true,
            Panel::ClipboardDialog => self.clipboard_dialog.open = true,
            Panel::ConvertPanel => self.convert_panel.open = true,
            Panel::VolumeDialog => self.volume_dialog.open = true,
            Panel::BrightnessDialog => self.brightness_dialog.open = true,
            Panel::CpuListDialog => self.cpu_list_dialog.open = true,
            Panel::ToastLogDialog => self.toast_log_dialog.open = true,
            Panel::Editor => self.show_editor = true,
            Panel::Settings => self.show_settings = true,
            Panel::Plugins => self.show_plugins = true,
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
                self.set_error(format!("Failed to save: {e}"));
            }
        }
    }

    fn update_panel_stack(&mut self) {
        macro_rules! check {
            ($cond:expr, $field:ident, $kind:expr) => {
                if $cond && !self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_stack.push($kind);
                    self.panel_states.$field = true;
                } else if !$cond && self.panel_states.$field {
                    self.panel_stack.retain(|p| *p != $kind);
                    self.panel_states.$field = false;
                }
            };
        }

        check!(self.alias_dialog.open, alias_dialog, Panel::AliasDialog);
        check!(
            self.bookmark_alias_dialog.open,
            bookmark_alias_dialog,
            Panel::BookmarkAliasDialog
        );
        check!(
            self.tempfile_alias_dialog.open,
            tempfile_alias_dialog,
            Panel::TempfileAliasDialog
        );
        check!(
            self.tempfile_dialog.open,
            tempfile_dialog,
            Panel::TempfileDialog
        );
        check!(
            self.add_bookmark_dialog.open,
            add_bookmark_dialog,
            Panel::AddBookmarkDialog
        );
        check!(
            self.help_window.overlay_open,
            help_overlay,
            Panel::HelpOverlay
        );
        check!(self.help_window.open, help_window, Panel::HelpWindow);
        check!(self.timer_dialog.open, timer_dialog, Panel::TimerDialog);
        check!(
            self.completion_dialog.open,
            completion_dialog,
            Panel::CompletionDialog
        );
        check!(
            self.shell_cmd_dialog.open,
            shell_cmd_dialog,
            Panel::ShellCmdDialog
        );
        check!(
            self.snippet_dialog.open,
            snippet_dialog,
            Panel::SnippetDialog
        );
        check!(self.macro_dialog.open, macro_dialog, Panel::MacroDialog);
        check!(self.fav_dialog.open, fav_dialog, Panel::FavDialog);
        check!(self.notes_dialog.open, notes_dialog, Panel::NotesDialog);
        check!(
            self.unused_assets_dialog.open,
            unused_assets_dialog,
            Panel::UnusedAssetsDialog
        );
        check!(!self.note_panels.is_empty(), note_panel, Panel::NotePanel);
        check!(
            !self.image_panels.is_empty(),
            image_panel,
            Panel::ImagePanel
        );
        check!(self.todo_dialog.open, todo_dialog, Panel::TodoDialog);
        check!(
            self.todo_view_dialog.open,
            todo_view_dialog,
            Panel::TodoViewDialog
        );
        check!(
            self.clipboard_dialog.open,
            clipboard_dialog,
            Panel::ClipboardDialog
        );
        check!(self.convert_panel.open, convert_panel, Panel::ConvertPanel);
        check!(self.volume_dialog.open, volume_dialog, Panel::VolumeDialog);
        check!(
            self.brightness_dialog.open,
            brightness_dialog,
            Panel::BrightnessDialog
        );
        check!(
            self.cpu_list_dialog.open,
            cpu_list_dialog,
            Panel::CpuListDialog
        );
        check!(
            self.toast_log_dialog.open,
            toast_log_dialog,
            Panel::ToastLogDialog
        );
        check!(self.show_editor, editor, Panel::Editor);
        check!(self.show_settings, settings, Panel::Settings);
        check!(self.show_plugins, plugins, Panel::Plugins);
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use egui::*;

        // tracing::debug!("LauncherApp::update called");
        if self.enable_toasts {
            self.toasts.show(ctx);
        }
        if let Some(pending) = self.pending_query.take() {
            self.query = pending;
            self.search();
            self.focus_input();
        }
        if let (Some(t), Some(_)) = (self.error_time, self.error.as_ref()) {
            if t.elapsed().as_secs_f32() >= 3.0 {
                self.error = None;
                self.error_time = None;
            }
        }
        if self
            .enabled_capabilities
            .as_ref()
            .and_then(|m| m.get("timer"))
            .map(|c| c.contains(&"completion_dialog".to_string()))
            .unwrap_or(true)
        {
            for msg in crate::plugins::timer::take_finished_messages() {
                self.completion_dialog.open_message(msg);
            }
        }
        for msg in crate::plugins::macros::take_step_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for msg in crate::plugins::browser_tabs::take_cache_messages() {
            if self.enable_toasts {
                push_toast(
                    &mut self.toasts,
                    Toast {
                        text: msg.into(),
                        kind: ToastKind::Info,
                        options: ToastOptions::default()
                            .duration_in_seconds(self.toast_duration as f64),
                    },
                );
            }
        }
        for err in crate::plugins::macros::take_error_messages() {
            tracing::debug!("{err}");
        }

        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        self.handle_dropped_files(dropped);
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window_size = (rect.width() as i32, rect.height() as i32);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = (rect.min.x as i32, rect.min.y as i32);
        }
        let do_restore = self.restore_flag.swap(false, Ordering::SeqCst);
        if self.visible_flag.load(Ordering::SeqCst) && self.help_flag.swap(false, Ordering::SeqCst)
        {
            self.help_window.overlay_open = !self.help_window.overlay_open;
        } else {
            // reset any queued toggle when window not visible
            self.help_flag.store(false, Ordering::SeqCst);
        }
        if do_restore {
            tracing::debug!("Restoring window on restore_flag");
            apply_visibility(
                true,
                ctx,
                self.offscreen_pos,
                self.follow_mouse,
                self.static_location_enabled,
                self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                self.static_size.map(|(w, h)| (w as f32, h as f32)),
                (self.window_size.0 as f32, self.window_size.1 as f32),
            );
            #[cfg(target_os = "windows")]
            if let Some(hwnd) = crate::window_manager::get_hwnd(_frame) {
                crate::window_manager::force_restore_and_foreground(hwnd);
            }
        }

        let should_be_visible = self.visible_flag.load(Ordering::SeqCst);
        let just_became_visible = !self.last_visible && should_be_visible;
        if self.last_visible != should_be_visible {
            tracing::debug!("gui thread -> visible: {}", should_be_visible);
            apply_visibility(
                should_be_visible,
                ctx,
                self.offscreen_pos,
                self.follow_mouse,
                self.static_location_enabled,
                self.static_pos.map(|(x, y)| (x as f32, y as f32)),
                self.static_size.map(|(w, h)| (w as f32, h as f32)),
                (self.window_size.0 as f32, self.window_size.1 as f32),
            );
            self.last_visible = should_be_visible;
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("Apps", |ui| {
                        if ui.button("Edit Apps").clicked() {
                            self.show_editor = !self.show_editor;
                        }
                        if ui.button("Edit Plugins").clicked() {
                            self.show_plugins = !self.show_plugins;
                        }
                    });
                    if ui.button("Close Application").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.unregister_all_hotkeys();
                        self.visible_flag.store(false, Ordering::SeqCst);
                        self.last_visible = false;
                        #[cfg(not(test))]
                        std::process::exit(0);
                    }
                });
                ui.menu_button("Settings", |ui| {
                    if ui.button("Edit Settings").clicked() {
                        self.show_settings = !self.show_settings;
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Command List").clicked() {
                        self.help_window.open = true;
                    }
                    if ui.button("Open Toast Log").clicked() {
                        if std::fs::OpenOptions::new()
                            .create(true)
                            .write(true)
                            .open(TOAST_LOG_FILE)
                            .is_err()
                        {
                            self.set_error("Failed to create log".into());
                        } else if let Err(e) = open::that(TOAST_LOG_FILE) {
                            self.set_error(format!("Failed to open log: {e}"));
                        }
                    }
                    if ui.button("View Toast Log").clicked() {
                        self.toast_log_dialog.open();
                    }
                });
                for panel in self.pinned_panels.clone() {
                    let label = format!("{:?}", panel);
                    if ui.button(label).clicked() {
                        if self.panel_stack.last() == Some(&panel) {
                            self.toggle_pin(panel);
                        } else {
                            self.focus_panel(panel);
                        }
                    }
                }
            });
        });

        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WatchEvent::Actions => {
                    if let Ok(mut acts) = load_actions(&self.actions_path) {
                        let custom_len = acts.len();
                        if let Some(paths) = &self.index_paths {
                            match indexer::index_paths(paths) {
                                Ok(idx) => acts.extend(idx),
                                Err(e) => {
                                    tracing::error!(error = %e, "failed to index paths");
                                    self.set_error(format!("Failed to index paths: {e}"));
                                }
                            }
                        }
                        self.actions = Arc::new(acts);
                        self.custom_len = custom_len;
                        self.update_action_cache();
                        self.search();
                        tracing::info!("actions reloaded");
                    }
                }
                WatchEvent::Folders => {
                    self.folder_aliases = crate::plugins::folders::load_folders(
                        crate::plugins::folders::FOLDERS_FILE,
                    )
                    .unwrap_or_else(|_| crate::plugins::folders::default_folders())
                    .into_iter()
                    .map(|f| (f.path, f.alias))
                    .collect();
                }
                WatchEvent::Bookmarks => {
                    self.bookmark_aliases = crate::plugins::bookmarks::load_bookmarks(
                        crate::plugins::bookmarks::BOOKMARKS_FILE,
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .map(|b| (b.url, b.alias))
                    .collect();
                }
                WatchEvent::Recycle(res) => match res {
                    Ok(()) => {
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: "Emptied Recycle Bin".into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                    Err(e) => {
                        let msg = format!("Failed to empty recycle bin: {e}");
                        self.set_error(msg.clone());
                        if self.enable_toasts {
                            push_toast(
                                &mut self.toasts,
                                Toast {
                                    text: msg.into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                },
                            );
                        }
                    }
                },
            }
        }

        let trimmed = self.query.trim().to_string();
        self.maybe_refresh_timer_list();
        self.maybe_refresh_stopwatch_list();
        if trimmed.eq_ignore_ascii_case("net")
            && self.last_net_update.elapsed().as_secs_f32() >= self.net_refresh
        {
            self.search();
            self.last_net_update = Instant::now();
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("🚀 Multi Lnchr");
            if let Some(err) = &self.error {
                ui.colored_label(Color32::RED, err);
            }

            scale_ui(ui, self.query_scale, |ui| {
                let input_id = egui::Id::new("query_input");

                if self.move_cursor_end {
                    if ui.ctx().memory(|m| m.has_focus(input_id)) {
                        let len = self.query.chars().count();
                        tracing::debug!("moving cursor to end: {len}");
                        ui.ctx().data_mut(|data| {
                            let state = data
                                .get_persisted_mut_or_default::<egui::widgets::text_edit::TextEditState>(
                                    input_id,
                                );
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(len),
                            )));
                        });
                        #[cfg(target_os = "windows")]
                        crate::window_manager::send_end_key();
                        self.move_cursor_end = false;
                        tracing::debug!("move_cursor_end cleared after moving");
                    } else {
                        tracing::debug!("cursor not moved - input not focused");
                    }
                }

                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id_source(input_id)
                        .desired_width(f32::INFINITY),
                );
                if just_became_visible || self.focus_query {
                    input.request_focus();
                    self.focus_query = false;
                }

                if input.changed() {
                    self.autocomplete_index = 0;
                    self.search();
                }

                if self.query_autocomplete && !self.suggestions.is_empty() {
                    ui.vertical(|ui| {
                        for s in &self.suggestions {
                            ui.colored_label(Color32::GRAY, s);
                        }
                    });
                }

                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    if self.any_panel_open() {
                        if self.close_front_dialog() {
                            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        }
                    } else {
                        self.visible_flag.store(false, Ordering::SeqCst);
                    }
                }

                if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::W)) {
                    if self.any_panel_open() {
                        if self.close_front_dialog() {
                            ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
                        }
                    }
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    self.handle_key(egui::Key::ArrowDown);
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    self.handle_key(egui::Key::ArrowUp);
                }

                if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
                    self.handle_key(egui::Key::PageDown);
                }

                if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
                    self.handle_key(egui::Key::PageUp);
                }

                let tab = ctx.input(|i| i.key_pressed(egui::Key::Tab));
                let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                let mut accepted_suggestion = false;
                if self.query_autocomplete
                    && !self.suggestions.is_empty()
                    && (tab || enter && self.selected.is_none())
                {
                    let idx = if tab { self.autocomplete_index } else { 0 };
                    let next = if tab {
                        (self.autocomplete_index + 1) % self.suggestions.len()
                    } else {
                        0
                    };
                    if let Some(s) = self.suggestions.get(idx).cloned() {
                        if s != self.query.to_lowercase() {
                            self.query = s;
                            self.move_cursor_end = true;
                            self.search();
                            accepted_suggestion = true;
                            if tab {
                                self.autocomplete_index = next;
                            }
                        }
                    }
                }
                if accepted_suggestion {
                    ctx.input_mut(|i| {
                        if tab {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                        }
                        if enter {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                        }
                    });
                }

                let mut launch_idx: Option<usize> = None;
                if !accepted_suggestion
                    && enter
                    && !self.bookmark_alias_dialog.open
                    && !self.tempfile_alias_dialog.open
                    && !self.tempfile_dialog.open
                    && !self.shell_cmd_dialog.open
                    && !self.notes_dialog.open
                    && !self.todo_dialog.open
                    && !self.todo_view_dialog.open
                    && self.note_panels.is_empty()
                    && self.image_panels.is_empty()
                {
                    launch_idx = self.handle_key(egui::Key::Enter);
                }

                if let Some(i) = launch_idx {
                    if let Some(a) = self.results.get(i) {
                        let a = a.clone();
                        let current = self.query.clone();
                        let mut refresh = false;
                        let mut set_focus = false;
                        if let Some(new_q) = a.action.strip_prefix("query:") {
                            tracing::debug!("query action via Enter: {new_q}");
                            self.query = new_q.to_string();
                            self.last_timer_query = new_q.starts_with("timer list")
                                || new_q.starts_with("alarm list");
                            self.search();
                            set_focus = true;
                            tracing::debug!("move_cursor_end set via Enter key");
                            self.move_cursor_end = true;
                        } else if a.action == "help:show" {
                            self.help_window.open = true;
                        } else if a.action == "timer:dialog:timer" {
                            self.timer_dialog.open_timer();
                        } else if a.action == "timer:dialog:alarm" {
                            self.timer_dialog.open_alarm();
                        } else if a.action == "shell:dialog" {
                            self.shell_cmd_dialog.open();
                        } else if a.action == "note:dialog" {
                            self.notes_dialog.open();
                        } else if a.action == "note:unused_assets" {
                            self.unused_assets_dialog.open();
                        } else if a.action == "bookmark:dialog" {
                            self.add_bookmark_dialog.open();
                        } else if a.action == "snippet:dialog" {
                            self.snippet_dialog.open();
                        } else if let Some(alias) = a.action.strip_prefix("snippet:edit:") {
                            self.snippet_dialog.open_edit(alias);
                        } else if a.action == "macro:dialog" {
                            self.macro_dialog.open();
                        } else if let Some(label) = a.action.strip_prefix("fav:dialog:") {
                            if label.is_empty() {
                                self.fav_dialog.open();
                            } else {
                                self.fav_dialog.open_edit(label);
                            }
                        } else if a.action == "todo:dialog" {
                            self.todo_dialog.open();
                        } else if a.action == "todo:view" {
                            self.todo_view_dialog.open();
                        } else if let Some(idx) = a.action.strip_prefix("todo:edit:") {
                            if let Ok(i) = idx.parse::<usize>() {
                                self.todo_view_dialog.open_edit(i);
                            }
                        } else if a.action == "clipboard:dialog" {
                            self.clipboard_dialog.open();
                        } else if let Some(slug) = a.action.strip_prefix("note:open:") {
                            let slug = slug.to_string();
                            self.open_note_panel(&slug, None);
                        } else if let Some(rest) = a.action.strip_prefix("note:new:") {
                            let mut parts = rest.splitn(2, ':');
                            let slug = parts.next().unwrap_or("").to_string();
                            let template = parts.next().map(|s| s.to_string());
                            self.open_note_panel(&slug, template.as_deref());
                        } else if a.action == "note:tags" {
                            self.open_note_tags();
                            set_focus = true;
                        } else if let Some(link) = a.action.strip_prefix("note:link:") {
                            self.open_note_link(link);
                        } else if let Some(slug) = a.action.strip_prefix("note:remove:") {
                            self.delete_note(slug);
                        } else if a.action == "convert:panel" {
                            self.convert_panel.open();
                        } else if a.action == "tempfile:dialog" {
                            self.tempfile_dialog.open();
                        } else if a.action == "settings:dialog" {
                            self.show_settings = true;
                        } else if a.action == "volume:dialog" {
                            self.volume_dialog.open();
                        } else if a.action == "brightness:dialog" {
                            self.brightness_dialog.open();
                        } else if let Some(n) = a.action.strip_prefix("sysinfo:cpu_list:") {
                            if let Ok(count) = n.parse::<usize>() {
                                self.cpu_list_dialog.open(count);
                            }
                        } else if a.action.starts_with("tab:switch:") {
                            if self.enable_toasts {
                                push_toast(
                                    &mut self.toasts,
                                    Toast {
                                        text: format!("Switching to {}", a.label).into(),
                                        kind: ToastKind::Info,
                                        options: ToastOptions::default()
                                            .duration_in_seconds(self.toast_duration as f64),
                                    },
                                );
                            }
                            let act = a.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = launch_action(&act) {
                                    tracing::error!(?e, "failed to switch tab");
                                }
                            });
                            if a.action != "help:show" {
                                let _ = history::append_history(
                                    HistoryEntry {
                                        query: current.clone(),
                                        query_lc: String::new(),
                                        action: a.clone(),
                                    },
                                    self.history_limit,
                                );
                                let count = self.usage.entry(a.action.clone()).or_insert(0);
                                *count += 1;
                            }
                        } else if let Err(e) = launch_action(&a) {
                            if a.desc == "Fav" && !a.action.starts_with("fav:") {
                                tracing::error!(?e, fav=%a.label, "failed to run favorite");
                            }
                            self.set_error(format!("Failed: {e}"));
                            if self.enable_toasts {
                                push_toast(&mut self.toasts, Toast {
                                    text: format!("Failed: {e}").into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                });
                            }
                        } else {
                            if a.desc == "Fav" && !a.action.starts_with("fav:") {
                                tracing::info!(fav=%a.label, command=%a.action, "ran favorite");
                            }
                            if self.enable_toasts && a.action != "recycle:clean" {
                                let msg = if a.action.starts_with("clipboard:") {
                                    format!("Copied {}", a.label)
                                } else {
                                    format!("Launched {}", a.label)
                                };
                                push_toast(&mut self.toasts, Toast {
                                    text: msg.into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                });
                            }
                            if a.action != "help:show" {
                                let _ = history::append_history(
                                    HistoryEntry {
                                        query: current.clone(),
                                        query_lc: String::new(),
                                        action: a.clone(),
                                    },
                                    self.history_limit,
                                );
                                let count = self.usage.entry(a.action.clone()).or_insert(0);
                                *count += 1;
                            }
                            if a.action == "note:reload" {
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    push_toast(
                                        &mut self.toasts,
                                        Toast {
                                            text: "Reloaded notes".into(),
                                            kind: ToastKind::Success,
                                            options: ToastOptions::default()
                                                .duration_in_seconds(self.toast_duration as f64),
                                        },
                                    );
                                }
                            } else if a.action.starts_with("bookmark:add:") {
                                if self.preserve_command {
                                    self.query = "bm add ".into();
                                } else {
                                    self.query.clear();
                                }
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("bookmark:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("folder:add:") {
                                if self.preserve_command {
                                    self.query = "f add ".into();
                                } else {
                                    self.query.clear();
                                }
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("folder:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("fav:add:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("fav:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("todo:add:") {
                                if self.preserve_command {
                                    self.query = "todo add ".into();
                                } else {
                                    self.query.clear();
                                }
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    if let Some(text) = a
                                        .action
                                        .strip_prefix("todo:add:")
                                        .and_then(|r| r.split('|').next())
                                    {
                                        push_toast(&mut self.toasts, Toast {
                                            text: format!("Added todo {text}").into(),
                                            kind: ToastKind::Success,
                                            options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                        });
                                    }
                                }
                            } else if a.action.starts_with("todo:remove:") {
                                refresh = true;
                                set_focus = true;
                                if current.starts_with("note list") {
                                    self.pending_query = Some(current.clone());
                                }
                                if self.enable_toasts {
                                    let label =
                                        a.label.strip_prefix("Remove todo ").unwrap_or(&a.label);
                                    push_toast(&mut self.toasts, Toast {
                                        text: format!("Removed todo {label}").into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action.starts_with("todo:done:") {
                                refresh = true;
                                set_focus = true;
                                // Re-run the current query so the todo list reflects the
                                // updated completion state immediately.
                                self.pending_query = Some(current.clone());
                                if self.enable_toasts {
                                    let label = a
                                        .label
                                        .trim_start_matches("[x] ")
                                        .trim_start_matches("[ ] ");
                                    push_toast(&mut self.toasts, Toast {
                                        text: format!("Toggled todo {label}").into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action.starts_with("todo:pset:") {
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    push_toast(&mut self.toasts, Toast {
                                        text: "Updated todo priority".into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action.starts_with("todo:tag:") {
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    push_toast(&mut self.toasts, Toast {
                                        text: "Updated todo tags".into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action == "todo:clear" {
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    push_toast(&mut self.toasts, Toast {
                                        text: "Cleared completed todos".into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action.starts_with("snippet:remove:") {
                                refresh = true;
                                set_focus = true;
                                if self.enable_toasts {
                                    push_toast(&mut self.toasts, Toast {
                                        text: format!("Removed snippet {}", a.label).into(),
                                        kind: ToastKind::Success,
                                        options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                    });
                                }
                            } else if a.action.starts_with("tempfile:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("tempfile:alias:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action == "tempfile:new"
                                || a.action.starts_with("tempfile:new:")
                            {
                                if self.preserve_command {
                                    self.query = "tmp new ".into();
                                } else {
                                    self.query.clear();
                                }
                                set_focus = true;
                            } else if a.action.starts_with("timer:cancel:")
                                && current.starts_with("timer rm")
                            {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("timer:pause:")
                                && current.starts_with("timer pause")
                            {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("timer:resume:")
                                && current.starts_with("timer resume")
                            {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("timer:start:")
                                && current.starts_with("timer add")
                            {
                                if self.preserve_command {
                                    self.query = "timer add ".into();
                                } else {
                                    self.query.clear();
                                }
                                set_focus = true;
                            }
                            if self.hide_after_run
                                && !a.action.starts_with("bookmark:add:")
                                && !a.action.starts_with("bookmark:remove:")
                                && !a.action.starts_with("folder:add:")
                                && !a.action.starts_with("folder:remove:")
                                && !a.action.starts_with("snippet:remove:")
                                && !a.action.starts_with("fav:add:")
                                && !a.action.starts_with("fav:remove:")
                                && !a.action.starts_with("calc:")
                                && !a.action.starts_with("todo:done:")
                            {
                                self.visible_flag.store(false, Ordering::SeqCst);
                            }
                        }
                        if refresh {
                            // Ensure file removals update the visible results list
                            self.last_results_valid = false;
                            self.search();
                        }
                        if set_focus {
                            self.focus_input();
                        } else if self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
                        {
                            self.focus_input();
                        }
                    }
                }
            });

            let area_height = ui.available_height();
            ScrollArea::vertical()
                .max_height(area_height)
                .show(ui, |ui| {
                    scale_ui(ui, self.list_scale, |ui| {
                        let mut refresh = false;
                        let mut set_focus = false;
                        let mut clicked_query: Option<String> = None;
                        let show_full = self
                            .enabled_capabilities
                            .as_ref()
                            .and_then(|m| m.get("folders"))
                            .map(|caps| caps.contains(&"show_full_path".to_string()))
                            .unwrap_or(false);
                        for idx in 0..self.results.len() {
                            let a = self.results[idx].clone();
                            let aliased = self
                                .folder_aliases
                                .get(&a.action)
                                .and_then(|v| v.as_ref());
                            let show_path = show_full || aliased.is_none();
                            let text = if show_path {
                                format!("{} : {}", a.label, a.desc)
                            } else {
                                a.label.clone()
                            };
                            let mut resp = ui.add_sized(
                                [ui.available_width(), 0.0],
                                egui::SelectableLabel::new(self.selected == Some(idx), text),
                            );
                            let tooltip = if a.desc == "Timer"
                                && a.action.starts_with("timer:show:")
                            {
                                if let Ok(id) = a.action[11..].parse::<u64>() {
                                    if let Some(ts) = crate::plugins::timer::timer_start_ts(id) {
                                        format!("Started {}", crate::plugins::timer::format_ts(ts))
                                    } else {
                                        a.action.clone()
                                    }
                                } else {
                                    a.action.clone()
                                }
                            } else {
                                a.action.clone()
                            };
                            let menu_resp = resp.on_hover_text(tooltip);
                            let custom_idx = self
                                .actions
                                .iter()
                                .take(self.custom_len)
                                .position(|act| act.action == a.action && act.label == a.label);
                            if self.folder_aliases.contains_key(&a.action)
                                && !a.action.starts_with("folder:")
                            {
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Set Alias").clicked() {
                                        self.alias_dialog.open(&a.action);
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove Folder").clicked() {
                                        if let Err(e) = crate::plugins::folders::remove_folder(
                                            crate::plugins::folders::FOLDERS_FILE,
                                            &a.action,
                                        ) {
                                            self.error =
                                                Some(format!("Failed to remove folder: {e}"));
                                        } else {
                                            refresh = true;
                                            set_focus = true;
                                            if self.enable_toasts {
                                                push_toast(&mut self.toasts, Toast {
                                                    text: format!("Removed folder {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(self.toast_duration as f64),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if self.bookmark_aliases.contains_key(&a.action) {
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Set Alias").clicked() {
                                        self.bookmark_alias_dialog.open(&a.action);
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove Bookmark").clicked() {
                                        if let Err(e) = crate::plugins::bookmarks::remove_bookmark(
                                            crate::plugins::bookmarks::BOOKMARKS_FILE,
                                            &a.action,
                                        ) {
                                            self.error =
                                                Some(format!("Failed to remove bookmark: {e}"));
                                        } else {
                                            refresh = true;
                                            set_focus = true;
                                            if self.enable_toasts {
                                                push_toast(&mut self.toasts, Toast {
                                                    text: format!("Removed bookmark {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(self.toast_duration as f64),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if a.desc == "Timer" && a.action.starts_with("timer:show:") {
                                if let Ok(id) = a.action[11..].parse::<u64>() {
                                    let query = self.query.trim().to_string();
                                    menu_resp.clone().context_menu(|ui| {
                                        if ui.button("Pause Timer").clicked() {
                                            crate::plugins::timer::pause_timer(id);
                                            if query.starts_with("timer list") {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Paused timer {}", a.label)
                                                            .into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove Timer").clicked() {
                                            crate::plugins::timer::cancel_timer(id);
                                            if query.starts_with("timer list") {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Removed timer {}", a.label)
                                                            .into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                }
                            } else if a.desc == "Stopwatch" && a.action.starts_with("stopwatch:show:") {
                                if let Ok(id) = a.action["stopwatch:show:".len()..].parse::<u64>() {
                                    let query = self.query.trim().to_string();
                                    menu_resp.clone().context_menu(|ui| {
                                        if ui.button("Pause Stopwatch").clicked() {
                                            crate::plugins::stopwatch::pause_stopwatch(id);
                                            if query.starts_with("sw list") {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Paused stopwatch {}", a.label).into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Resume Stopwatch").clicked() {
                                            crate::plugins::stopwatch::resume_stopwatch(id);
                                            if query.starts_with("sw list") {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Resumed stopwatch {}", a.label).into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Stop Stopwatch").clicked() {
                                            crate::plugins::stopwatch::stop_stopwatch(id);
                                            if query.starts_with("sw list") {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Stopped stopwatch {}", a.label).into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("Copy Time").clicked() {
                                            if let Some(time) =
                                                crate::plugins::stopwatch::format_elapsed(id)
                                            {
                                                if let Err(e) =
                                                    crate::actions::clipboard::set_text(&time)
                                                {
                                                    self.error =
                                                        Some(format!("Failed to copy time: {e}"));
                                                } else if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Copied {time}").into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                }
                            } else if a.desc == "Snippet" {
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Edit Snippet").clicked() {
                                        self.snippet_dialog.open_edit(&a.label);
                                        ui.close_menu();
                                    }
                                    if ui.button("Remove Snippet").clicked() {
                                        if let Err(e) = remove_snippet(SNIPPETS_FILE, &a.label) {
                                            self.error =
                                                Some(format!("Failed to remove snippet: {e}"));
                                        } else {
                                            refresh = true;
                                            set_focus = true;
                                            if self.enable_toasts {
                                                push_toast(&mut self.toasts, Toast {
                                                    text: format!("Removed snippet {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(self.toast_duration as f64),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if a.desc == "Tempfile" && !a.action.starts_with("tempfile:") {
                                let file_path = a.action.clone();
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Set Alias").clicked() {
                                        self.tempfile_alias_dialog.open(&file_path);
                                        ui.close_menu();
                                    }
                                    if ui.button("Delete File").clicked() {
                                        if let Err(e) = crate::plugins::tempfile::remove_file(
                                            std::path::Path::new(&file_path),
                                        ) {
                                            self.error =
                                                Some(format!("Failed to delete file: {e}"));
                                        } else {
                                            refresh = true;
                                            set_focus = true;
                                            if self.enable_toasts {
                                                push_toast(&mut self.toasts, Toast {
                                                    text: format!("Removed file {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(self.toast_duration as f64),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if a.desc == "Note"
                                && a.action.starts_with("note:open:")
                            {
                                let slug = a.action.rsplit(':').next().unwrap_or("").to_string();
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Edit Note").clicked() {
                                        self.open_note_panel(&slug, None);
                                        ui.close_menu();
                                    }
                                    #[cfg(target_os = "windows")]
                                    if ui.button("Open in Notepad").clicked() {
                                        match crate::plugins::note::load_notes() {
                                            Ok(notes) => {
                                                if let Some(note) =
                                                    notes.iter().find(|n| n.slug == slug)
                                                {
                                                    if let Err(e) = std::process::Command::new(
                                                        "notepad.exe",
                                                    )
                                                    .arg(&note.path)
                                                    .spawn()
                                                    {
                                                        self.error = Some(e.to_string());
                                                    }
                                                } else {
                                                    self.error =
                                                        Some("Note not found".to_string());
                                                }
                                            }
                                            Err(e) => {
                                                self.error = Some(e.to_string());
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                    if ui.button("Open in Neovim").clicked() {
                                        if self.open_note_in_neovim(
                                            &slug,
                                            crate::plugins::note::load_notes,
                                            |path| {
                                                std::process::Command::new("nvim")
                                                    .arg(path)
                                                    .spawn()
                                                    .map(|_| ())
                                            },
                                        ) {
                                            ui.close_menu();
                                        }
                                    }
                                    if ui.button("Remove Note").clicked() {
                                        self.delete_note(&slug);
                                        refresh = true;
                                        set_focus = true;
                                        ui.close_menu();
                                    }
                                });
                            } else if a.desc == "Clipboard"
                                && a.action.starts_with("clipboard:copy:")
                            {
                                let idx_str = a.action.rsplit(':').next().unwrap_or("");
                                if let Ok(cb_idx) = idx_str.parse::<usize>() {
                                    let cb_label = a.label.clone();
                                    menu_resp.clone().context_menu(|ui| {
                                        if ui.button("Edit Entry").clicked() {
                                            self.clipboard_dialog.open_edit(cb_idx);
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove Entry").clicked() {
                                            if let Err(e) = crate::plugins::clipboard::remove_entry(
                                                crate::plugins::clipboard::CLIPBOARD_FILE,
                                                cb_idx,
                                            ) {
                                                self.error =
                                                    Some(format!("Failed to remove entry: {e}"));
                                            } else {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    push_toast(&mut self.toasts, Toast {
                                                        text: format!("Removed entry {}", cb_label)
                                                            .into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(self.toast_duration as f64),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                }
                            } else if a.desc == "Todo" && a.action.starts_with("todo:done:") {
                                let idx_str = a.action.rsplit(':').next().unwrap_or("");
                                if let Ok(todo_idx) = idx_str.parse::<usize>() {
                                    menu_resp.clone().context_menu(|ui| {
                                        if ui.button("Edit Todo").clicked() {
                                            self.todo_view_dialog.open_edit(todo_idx);
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                            if let Some(idx_act) = custom_idx {
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Edit App").clicked() {
                                        self.editor.open_edit(idx_act, &self.actions[idx_act]);
                                        self.show_editor = true;
                                        ui.close_menu();
                                    }
                                });
                            }
                            resp = menu_resp;
                            if self.selected == Some(idx) {
                                resp.scroll_to_me(Some(egui::Align::Center));
                            }
                            if resp.clicked() {
                                let current = self.query.clone();
                                if let Some(new_q) = a.action.strip_prefix("query:") {
                                    tracing::debug!("query action via click: {new_q}");
                                    clicked_query = Some(new_q.to_string());
                                    set_focus = true;
                                    tracing::debug!("move_cursor_end set via mouse click");
                                    self.move_cursor_end = true;
                                } else if a.action == "help:show" {
                                    self.help_window.open = true;
                                } else if a.action == "timer:dialog:timer" {
                                    self.timer_dialog.open_timer();
                                } else if a.action == "timer:dialog:alarm" {
                                    self.timer_dialog.open_alarm();
                                } else if a.action == "shell:dialog" {
                                    self.shell_cmd_dialog.open();
                                } else if a.action == "note:dialog" {
                                    self.notes_dialog.open();
                                } else if a.action == "note:unused_assets" {
                                    self.unused_assets_dialog.open();
                                } else if a.action == "bookmark:dialog" {
                                    self.add_bookmark_dialog.open();
                                } else if a.action == "snippet:dialog" {
                            self.snippet_dialog.open();
                        } else if let Some(alias) = a.action.strip_prefix("snippet:edit:") {
                            self.snippet_dialog.open_edit(alias);
                        } else if a.action == "macro:dialog" {
                            self.macro_dialog.open();
                        } else if let Some(label) = a.action.strip_prefix("fav:dialog:") {
                            if label.is_empty() {
                                self.fav_dialog.open();
                            } else {
                                self.fav_dialog.open_edit(label);
                            }
                        } else if a.action == "todo:dialog" {
                            self.todo_dialog.open();
                        } else if a.action == "todo:view" {
                            self.todo_view_dialog.open();
                        } else if let Some(idx) = a.action.strip_prefix("todo:edit:") {
                            if let Ok(i) = idx.parse::<usize>() {
                                self.todo_view_dialog.open_edit(i);
                            }
                        } else if a.action == "clipboard:dialog" {
                            self.clipboard_dialog.open();
                        } else if let Some(slug) = a.action.strip_prefix("note:open:") {
                            let slug = slug.to_string();
                            self.open_note_panel(&slug, None);
                        } else if let Some(rest) = a.action.strip_prefix("note:new:") {
                            let mut parts = rest.splitn(2, ':');
                            let slug = parts.next().unwrap_or("").to_string();
                            let template = parts.next().map(|s| s.to_string());
                            self.open_note_panel(&slug, template.as_deref());
                        } else if a.action == "note:tags" {
                            self.open_note_tags();
                            set_focus = true;
                        } else if let Some(link) = a.action.strip_prefix("note:link:") {
                            self.open_note_link(link);
                        } else if let Some(slug) = a.action.strip_prefix("note:remove:") {
                            self.delete_note(slug);
                        } else if a.action == "convert:panel" {
                            self.convert_panel.open();
                        } else if a.action == "tempfile:dialog" {
                            self.tempfile_dialog.open();
                        } else if a.action == "settings:dialog" {
                            self.show_settings = true;
                        } else if a.action == "volume:dialog" {
                            self.volume_dialog.open();
                        } else if a.action == "brightness:dialog" {
                            self.brightness_dialog.open();
                        } else if let Some(n) = a.action.strip_prefix("sysinfo:cpu_list:") {
                            if let Ok(count) = n.parse::<usize>() {
                                self.cpu_list_dialog.open(count);
                            }
                        } else if let Err(e) = launch_action(&a) {
                                    if a.desc == "Fav" && !a.action.starts_with("fav:") {
                                        tracing::error!(?e, fav=%a.label, "failed to run favorite");
                                    }
                                    self.error = Some(format!("Failed: {e}"));
                                    self.error_time = Some(Instant::now());
                                    if self.enable_toasts {
                                        push_toast(&mut self.toasts, Toast {
                                            text: format!("Failed: {e}").into(),
                                            kind: ToastKind::Error,
                                            options: ToastOptions::default()
                                                .duration_in_seconds(self.toast_duration as f64),
                                        });
                                    }
                                } else {
                                    if a.desc == "Fav" && !a.action.starts_with("fav:") {
                                        tracing::info!(fav=%a.label, command=%a.action, "ran favorite");
                                    }
                            if self.enable_toasts && a.action != "recycle:clean" {
                                let msg = if a.action.starts_with("clipboard:") {
                                    format!("Copied {}", a.label)
                                } else {
                                    format!("Launched {}", a.label)
                                };
                                push_toast(&mut self.toasts, Toast {
                                    text: msg.into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default()
                                        .duration_in_seconds(self.toast_duration as f64),
                                });
                            }
                                    if a.action != "help:show" {
                                        let _ = history::append_history(
                                            HistoryEntry {
                                                query: current.clone(),
                                                query_lc: String::new(),
                                                action: a.clone(),
                                            },
                                            self.history_limit,
                                        );
                                        let count = self.usage.entry(a.action.clone()).or_insert(0);
                                        *count += 1;
                                    }
                                    if a.action.starts_with("bookmark:add:") {
                                        if self.preserve_command {
                                            self.query = "bm add ".into();
                                        } else {
                                            self.query.clear();
                                        }
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("bookmark:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("folder:add:") {
                                        if self.preserve_command {
                                            self.query = "f add ".into();
                                        } else {
                                            self.query.clear();
                                        }
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("folder:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("fav:add:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("fav:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("todo:add:") {
                                        if self.preserve_command {
                                            self.query = "todo add ".into();
                                        } else {
                                            self.query.clear();
                                        }
                                        refresh = true;
                                        set_focus = true;
                                        if self.enable_toasts {
                                            if let Some(text) = a
                                                .action
                                                .strip_prefix("todo:add:")
                                                .and_then(|r| r.split('|').next())
                                            {
                                                push_toast(&mut self.toasts, Toast {
                                                    text: format!("Added todo {text}").into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                                });
                                            }
                                        }
                                    } else if a.action.starts_with("todo:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                        if current.starts_with("note list") {
                                            clicked_query = Some(current.clone());
                                        }
                                        if self.enable_toasts {
                                            let label = a
                                                .label
                                                .strip_prefix("Remove todo ")
                                                .unwrap_or(&a.label);
                                            push_toast(&mut self.toasts, Toast {
                                                text: format!("Removed todo {label}").into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action.starts_with("todo:done:") {
                                        refresh = true;
                                        set_focus = true;
                                        // Re-run the current query so the visible list refreshes
                                        // with the toggled completion state.
                                        clicked_query = Some(current.clone());
                                        if self.enable_toasts {
                                            let label = a
                                                .label
                                                .trim_start_matches("[x] ")
                                                .trim_start_matches("[ ] ");
                                            push_toast(&mut self.toasts, Toast {
                                                text: format!("Toggled todo {label}").into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action.starts_with("todo:pset:") {
                                        refresh = true;
                                        set_focus = true;
                                        if self.enable_toasts {
                                            push_toast(&mut self.toasts, Toast {
                                                text: "Updated todo priority".into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action.starts_with("todo:tag:") {
                                        refresh = true;
                                        set_focus = true;
                                        if self.enable_toasts {
                                            push_toast(&mut self.toasts, Toast {
                                                text: "Updated todo tags".into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default().duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action == "todo:clear" {
                                        refresh = true;
                                        set_focus = true;
                                        if self.enable_toasts {
                                            push_toast(&mut self.toasts, Toast {
                                                text: "Cleared completed todos".into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action.starts_with("snippet:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                        if self.enable_toasts {
                                            push_toast(&mut self.toasts, Toast {
                                                text: format!("Removed snippet {}", a.label).into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(self.toast_duration as f64),
                                            });
                                        }
                                    } else if a.action.starts_with("tempfile:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("tempfile:alias:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action == "tempfile:new"
                                        || a.action.starts_with("tempfile:new:")
                                    {
                                        if self.preserve_command {
                                            self.query = "tmp new ".into();
                                        } else {
                                            self.query.clear();
                                        }
                                        set_focus = true;
                                    }
                                    if self.hide_after_run
                                        && !a.action.starts_with("bookmark:add:")
                                        && !a.action.starts_with("bookmark:remove:")
                                        && !a.action.starts_with("folder:add:")
                                        && !a.action.starts_with("folder:remove:")
                                        && !a.action.starts_with("snippet:remove:")
                                        && !a.action.starts_with("fav:add:")
                                        && !a.action.starts_with("fav:remove:")
                                        && !a.action.starts_with("calc:")
                                        && !a.action.starts_with("todo:done:")
                                    {
                                        self.visible_flag.store(false, Ordering::SeqCst);
                                    }
                                }
                                self.selected = Some(idx);
                            }
                        }
                        if let Some(new_q) = clicked_query {
                            self.pending_query = Some(new_q);
                        }
                        if refresh {
                            // Ensure file removals update the visible results list
                            self.last_results_valid = false;
                            self.search();
                        }
                        if set_focus {
                            self.focus_input();
                        } else if self.visible_flag.load(Ordering::SeqCst) && !self.any_panel_open()
                        {
                            self.focus_input();
                        }
                    });
                });
        });
        let show_editor = self.show_editor;
        if show_editor {
            let mut editor = std::mem::take(&mut self.editor);
            editor.ui(ctx, self);
            self.editor = editor;
        }
        let show_settings = self.show_settings;
        if show_settings {
            let mut ed = std::mem::take(&mut self.settings_editor);
            ed.ui(ctx, self);
            self.settings_editor = ed;
        }
        let show_plugin = self.show_plugins;
        if show_plugin {
            let mut ed = std::mem::take(&mut self.plugin_editor);
            ed.ui(ctx, self);
            self.plugin_editor = ed;
        }
        let mut dlg = std::mem::take(&mut self.alias_dialog);
        dlg.ui(ctx, self);
        self.alias_dialog = dlg;
        let mut bm_dlg = std::mem::take(&mut self.bookmark_alias_dialog);
        bm_dlg.ui(ctx, self);
        self.bookmark_alias_dialog = bm_dlg;
        let mut tf_dlg = std::mem::take(&mut self.tempfile_alias_dialog);
        tf_dlg.ui(ctx, self);
        self.tempfile_alias_dialog = tf_dlg;
        let mut create_tf = std::mem::take(&mut self.tempfile_dialog);
        create_tf.ui(ctx, self);
        self.tempfile_dialog = create_tf;
        let mut add_bm_dlg = std::mem::take(&mut self.add_bookmark_dialog);
        add_bm_dlg.ui(ctx, self);
        self.add_bookmark_dialog = add_bm_dlg;
        let mut help = std::mem::take(&mut self.help_window);
        help.ui(ctx, self);
        self.help_window = help;
        let mut timer_dlg = std::mem::take(&mut self.timer_dialog);
        timer_dlg.ui(ctx, self);
        self.timer_dialog = timer_dlg;
        let mut comp = std::mem::take(&mut self.completion_dialog);
        comp.ui(ctx);
        self.completion_dialog = comp;
        let mut shell_dlg = std::mem::take(&mut self.shell_cmd_dialog);
        shell_dlg.ui(ctx, self);
        self.shell_cmd_dialog = shell_dlg;
        let mut snip_dlg = std::mem::take(&mut self.snippet_dialog);
        snip_dlg.ui(ctx, self);
        self.snippet_dialog = snip_dlg;
        let mut macro_dlg = std::mem::take(&mut self.macro_dialog);
        macro_dlg.ui(ctx, self);
        self.macro_dialog = macro_dlg;
        let mut fav_dlg = std::mem::take(&mut self.fav_dialog);
        fav_dlg.ui(ctx, self);
        self.fav_dialog = fav_dlg;
        let mut notes_dlg = std::mem::take(&mut self.notes_dialog);
        notes_dlg.ui(ctx, self);
        self.notes_dialog = notes_dlg;
        let mut assets_dlg = std::mem::take(&mut self.unused_assets_dialog);
        assets_dlg.ui(ctx, self);
        self.unused_assets_dialog = assets_dlg;
        let mut i = 0;
        while i < self.note_panels.len() {
            let mut panel = self.note_panels.remove(i);
            panel.ui(ctx, self);
            if panel.open {
                self.note_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.image_panels.len() {
            let mut panel = self.image_panels.remove(i);
            panel.ui(ctx);
            if panel.open {
                self.image_panels.insert(i, panel);
                i += 1;
            }
        }
        let mut todo_dlg = std::mem::take(&mut self.todo_dialog);
        todo_dlg.ui(ctx, self);
        self.todo_dialog = todo_dlg;
        let mut todo_view = std::mem::take(&mut self.todo_view_dialog);
        todo_view.ui(ctx, self);
        self.todo_view_dialog = todo_view;
        let mut cb_dlg = std::mem::take(&mut self.clipboard_dialog);
        cb_dlg.ui(ctx, self);
        self.clipboard_dialog = cb_dlg;
        let mut conv_panel = std::mem::take(&mut self.convert_panel);
        conv_panel.ui(ctx, self);
        self.convert_panel = conv_panel;
        let mut vol_dlg = std::mem::take(&mut self.volume_dialog);
        vol_dlg.ui(ctx, self);
        self.volume_dialog = vol_dlg;
        let mut bright_dlg = std::mem::take(&mut self.brightness_dialog);
        bright_dlg.ui(ctx, self);
        self.brightness_dialog = bright_dlg;
        let mut cpu_dlg = std::mem::take(&mut self.cpu_list_dialog);
        cpu_dlg.ui(ctx, self);
        self.cpu_list_dialog = cpu_dlg;
        let mut toast_dlg = std::mem::take(&mut self.toast_log_dialog);
        toast_dlg.ui(ctx, self);
        self.toast_log_dialog = toast_dlg;
        self.enforce_pinned();
        self.update_panel_stack();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.unregister_all_hotkeys();
        self.visible_flag.store(false, Ordering::SeqCst);
        self.last_visible = false;
        if let Ok(mut settings) = crate::settings::Settings::load(&self.settings_path) {
            settings.window_size = Some(self.window_size);
            settings.pinned_panels = self.pinned_panels.clone();
            let _ = settings.save(&self.settings_path);
        }
        let _ = usage::save_usage(USAGE_FILE, &self.usage);
        #[cfg(not(test))]
        std::process::exit(0);
    }
}

impl LauncherApp {
    pub fn watch_receiver(&self) -> &Receiver<WatchEvent> {
        &self.rx
    }

    /// Open a note panel for the given slug, optionally using a template for new notes.
    pub fn open_note_panel(&mut self, slug: &str, template: Option<&str>) {
        use crate::plugins::note::{extract_alias, get_template, load_notes, Note};
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
                        let filled = tpl.replace("{{title}}", &title).replace("{{date}}", slug);
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
                let alias = extract_alias(&content);
                Note {
                    title,
                    path: std::path::PathBuf::new(),
                    content,
                    tags: Vec::new(),
                    links: Vec::new(),
                    slug: String::new(),
                    alias,
                }
            });
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
        self.note_panels.push(NotePanel::from_note(note));
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
            self.set_error(format!("Image not found: {}", path.display()));
            return;
        }
        if image::ImageFormat::from_path(path).is_err() {
            self.set_error(format!("Unsupported image format: {}", path.display()));
            return;
        }
        self.image_panels.push(ImagePanel::new(path.to_path_buf()));
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
                    Err(e) => self.set_error(format!("Failed to open link: {e}")),
                },
                _ => {
                    if self.enable_toasts {
                        push_toast(
                            &mut self.toasts,
                            Toast {
                                text: format!("Invalid link: {link}").into(),
                                kind: ToastKind::Error,
                                options: ToastOptions::default()
                                    .duration_in_seconds(self.toast_duration as f64),
                            },
                        );
                    }
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
                        self.error = Some(e.to_string());
                    }
                } else {
                    self.error = Some("Note not found".to_string());
                }
            }
            Err(e) => {
                self.error = Some(e.to_string());
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
                        self.set_error(format!("Failed to remove note: {e}"));
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
                    self.set_error("Note not found".into());
                }
            }
            Err(e) => self.set_error(format!("Failed to load notes: {e}")),
        }
        self.focus_input();
    }

    /// Process dropped files or directories.
    pub fn handle_dropped_files(&mut self, files: Vec<egui::DroppedFile>) {
        for file in files {
            if let Some(path) = file.path {
                if path.is_dir() {
                    if let Err(e) = folders::add(path.to_str().unwrap_or_default()) {
                        self.set_error(format!("Failed to add folder: {e}"));
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
    rx.try_recv().ok().map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::slug::reset_slug_lookup,
        plugin::PluginManager,
        plugins::note::{append_note, load_notes, save_notes, NotePlugin},
        settings::Settings,
        toast_log::TOAST_LOG_FILE,
    };
    use eframe::egui;
    use image::RgbaImage;
    use once_cell::sync::Lazy;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
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

    #[test]
    fn tab_cycles_through_suggestions() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        app.query_autocomplete = true;
        app.suggestions = vec!["one".into(), "two".into(), "three".into()];

        let expected = ["one", "two", "three", "one"];
        for exp in expected {
            let idx = app.autocomplete_index;
            let next = (app.autocomplete_index + 1) % app.suggestions.len();
            let s = app.suggestions[idx].clone();
            if s != app.query.to_lowercase() {
                app.query = s;
            }
            app.autocomplete_index = next;
            assert_eq!(app.query, exp);
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
        std::env::set_var("ML_NOTES_DIR", &notes_dir);
        std::env::set_var("HOME", dir.path());
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
}
