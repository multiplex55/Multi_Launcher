use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::clipboard_dialog::ClipboardDialog;
use crate::cpu_list_dialog::CpuListDialog;
use crate::help_window::HelpWindow;
use crate::history::{self, HistoryEntry};
use crate::indexer;
use crate::launcher::launch_action;
use crate::notes_dialog::NotesDialog;
use crate::plugin::PluginManager;
use crate::plugin_editor::PluginEditor;
use crate::plugins::snippets::{remove_snippet, SNIPPETS_FILE};
use crate::settings::Settings;
use crate::settings_editor::SettingsEditor;
use crate::snippet_dialog::SnippetDialog;
use crate::tempfile_dialog::TempfileDialog;
use crate::timer_dialog::{TimerCompletionDialog, TimerDialog};
use crate::timer_help_window::TimerHelpWindow;
use crate::todo_dialog::TodoDialog;
use crate::usage::{self, USAGE_FILE};
use crate::visibility::apply_visibility;
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
#[cfg(target_os = "windows")]
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

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

enum WatchEvent {
    Actions,
}

pub struct LauncherApp {
    pub actions: Vec<Action>,
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
    plugin_dirs: Option<Vec<String>>,
    index_paths: Option<Vec<String>>,
    enabled_plugins: Option<Vec<String>>,
    enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
    visible_flag: Arc<AtomicBool>,
    restore_flag: Arc<AtomicBool>,
    last_visible: bool,
    offscreen_pos: (f32, f32),
    pub window_size: (i32, i32),
    pub window_pos: (i32, i32),
    focus_query: bool,
    toasts: egui_toast::Toasts,
    pub enable_toasts: bool,
    alias_dialog: crate::alias_dialog::AliasDialog,
    bookmark_alias_dialog: crate::bookmark_alias_dialog::BookmarkAliasDialog,
    tempfile_alias_dialog: crate::tempfile_alias_dialog::TempfileAliasDialog,
    tempfile_dialog: TempfileDialog,
    add_bookmark_dialog: crate::add_bookmark_dialog::AddBookmarkDialog,
    help_window: crate::help_window::HelpWindow,
    timer_help: crate::timer_help_window::TimerHelpWindow,
    timer_dialog: crate::timer_dialog::TimerDialog,
    completion_dialog: crate::timer_dialog::TimerCompletionDialog,
    shell_cmd_dialog: crate::shell_cmd_dialog::ShellCmdDialog,
    snippet_dialog: SnippetDialog,
    notes_dialog: NotesDialog,
    todo_dialog: TodoDialog,
    clipboard_dialog: ClipboardDialog,
    volume_dialog: crate::volume_dialog::VolumeDialog,
    brightness_dialog: crate::brightness_dialog::BrightnessDialog,
    cpu_list_dialog: CpuListDialog,
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
    pub follow_mouse: bool,
    pub static_location_enabled: bool,
    pub static_pos: Option<(i32, i32)>,
    pub static_size: Option<(i32, i32)>,
    pub hide_after_run: bool,
}

impl LauncherApp {
    pub fn add_toast(&mut self, toast: Toast) {
        self.toasts.add(toast);
    }
    pub fn update_paths(
        &mut self,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<Vec<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        offscreen_pos: Option<(i32, i32)>,
        enable_toasts: Option<bool>,
        fuzzy_weight: Option<f32>,
        usage_weight: Option<f32>,
        follow_mouse: Option<bool>,
        static_enabled: Option<bool>,
        static_pos: Option<(i32, i32)>,
        static_size: Option<(i32, i32)>,
        hide_after_run: Option<bool>,
    ) {
        self.plugin_dirs = plugin_dirs;
        self.index_paths = index_paths;
        self.enabled_plugins = enabled_plugins;
        self.enabled_capabilities = enabled_capabilities;
        if let Some((x, y)) = offscreen_pos {
            self.offscreen_pos = (x as f32, y as f32);
        }
        if let Some(v) = enable_toasts {
            self.enable_toasts = v;
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
    }

    pub fn new(
        ctx: &egui::Context,
        actions: Vec<Action>,
        custom_len: usize,
        plugins: PluginManager,
        actions_path: String,
        settings_path: String,
        settings: Settings,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<Vec<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        visible_flag: Arc<AtomicBool>,
        restore_flag: Arc<AtomicBool>,
        help_flag: Arc<AtomicBool>,
    ) -> Self {
        let (tx, rx) = channel();
        let mut watchers = Vec::new();
        let toasts = Toasts::new().anchor(egui::Align2::RIGHT_TOP, [10.0, 10.0]);
        let enable_toasts = settings.enable_toasts;

        if let Ok(mut watcher) = RecommendedWatcher::new(
            {
                let tx = tx.clone();
                move |res: notify::Result<notify::Event>| match res {
                    Ok(event) => {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            let _ = tx.send(WatchEvent::Actions);
                        }
                    }
                    Err(e) => tracing::error!("watch error: {:?}", e),
                }
            },
            Config::default(),
        ) {
            use std::path::Path;
            if watcher
                .watch(Path::new(&actions_path), RecursiveMode::NonRecursive)
                .is_ok()
            {
                watchers.push(watcher);
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

        let settings_editor = SettingsEditor::new(&settings);
        let plugin_editor = PluginEditor::new(&settings);
        let app = Self {
            actions: actions.clone(),
            query: String::new(),
            results: actions,
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
            toasts,
            enable_toasts,
            alias_dialog: crate::alias_dialog::AliasDialog::default(),
            bookmark_alias_dialog: crate::bookmark_alias_dialog::BookmarkAliasDialog::default(),
            tempfile_alias_dialog: crate::tempfile_alias_dialog::TempfileAliasDialog::default(),
            tempfile_dialog: TempfileDialog::default(),
            add_bookmark_dialog: crate::add_bookmark_dialog::AddBookmarkDialog::default(),
            help_window: HelpWindow::default(),
            timer_help: TimerHelpWindow::default(),
            timer_dialog: TimerDialog::default(),
            completion_dialog: TimerCompletionDialog::default(),
            shell_cmd_dialog: crate::shell_cmd_dialog::ShellCmdDialog::default(),
            snippet_dialog: SnippetDialog::default(),
            notes_dialog: NotesDialog::default(),
            todo_dialog: TodoDialog::default(),
            clipboard_dialog: ClipboardDialog::default(),
            volume_dialog: crate::volume_dialog::VolumeDialog::default(),
            brightness_dialog: crate::brightness_dialog::BrightnessDialog::default(),
            cpu_list_dialog: CpuListDialog::default(),
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
            follow_mouse,
            static_location_enabled: static_enabled,
            static_pos,
            static_size,
            hide_after_run: settings.hide_after_run,
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

        app
    }

    pub fn search(&mut self) {
        let mut res: Vec<(Action, f32)> = Vec::new();

        let trimmed = self.query.trim();

        if trimmed.starts_with("g ") {
            let filter = vec!["web_search".to_string()];
            res.extend(
                self.plugins
                    .search_filtered(
                        &self.query,
                        Some(&filter),
                        self.enabled_capabilities.as_ref(),
                    )
                    .into_iter()
                    .map(|a| {
                        let score = if self.query.is_empty() {
                            0.0
                        } else {
                            self.matcher
                                .fuzzy_match(&a.label, &self.query)
                                .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                                .unwrap_or(0) as f32
                                * self.fuzzy_weight
                        };
                        (a, score)
                    }),
            );
        } else {
            if self.query.is_empty() {
                res.extend(self.actions.iter().cloned().map(|a| (a, 0.0)));
            } else {
                for a in &self.actions {
                    let s1 = self.matcher.fuzzy_match(&a.label, &self.query);
                    let s2 = self.matcher.fuzzy_match(&a.desc, &self.query);
                    if let Some(score) = s1.max(s2) {
                        res.push((a.clone(), score as f32 * self.fuzzy_weight));
                    }
                }
            }

            res.extend(
                self.plugins
                    .search_filtered(
                        &self.query,
                        self.enabled_plugins.as_ref(),
                        self.enabled_capabilities.as_ref(),
                    )
                    .into_iter()
                    .map(|a| {
                        let score = if self.query.is_empty() {
                            0.0
                        } else {
                            self.matcher
                                .fuzzy_match(&a.label, &self.query)
                                .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                                .unwrap_or(0) as f32
                                * self.fuzzy_weight
                        };
                        (a, score)
                    }),
            );
        }

        for (a, score) in res.iter_mut() {
            *score += self.usage.get(&a.action).cloned().unwrap_or(0) as f32 * self.usage_weight;
        }

        res.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        self.results = res.into_iter().map(|(a, _)| a).collect();
        self.selected = None;
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

    #[cfg(target_os = "windows")]
    pub fn unregister_all_hotkeys(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
        let mut registered_hotkeys = self.registered_hotkeys.lock().unwrap();
        for id in registered_hotkeys.values() {
            unsafe {
                let _ = UnregisterHotKey(None, *id as i32);
            }
        }
        registered_hotkeys.clear();
    }

    #[cfg(not(target_os = "windows"))]
    pub fn unregister_all_hotkeys(&self) {}
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use egui::*;

        tracing::debug!("LauncherApp::update called");
        if self.enable_toasts {
            self.toasts.show(ctx);
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
                    ui.menu_button("Commands", |ui| {
                        if ui.button("Edit Commands").clicked() {
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
                    if ui.button("Timer Plugin Help").clicked() {
                        self.timer_help.open = true;
                    }
                });
            });
        });

        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WatchEvent::Actions => {
                    if let Ok(mut acts) = load_actions(&self.actions_path) {
                        let custom_len = acts.len();
                        if let Some(paths) = &self.index_paths {
                            acts.extend(indexer::index_paths(paths));
                        }
                        self.actions = acts;
                        self.custom_len = custom_len;
                        self.search();
                        tracing::info!("actions reloaded");
                    }
                }
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("🚀 LNCHR");
            if let Some(err) = &self.error {
                ui.colored_label(Color32::RED, err);
            }

            scale_ui(ui, self.query_scale, |ui| {
                let input = ui.text_edit_singleline(&mut self.query);
                if just_became_visible || self.focus_query {
                    input.request_focus();
                    self.focus_query = false;
                }
                if input.changed() {
                    self.search();
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    self.handle_key(egui::Key::ArrowDown);
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    self.handle_key(egui::Key::ArrowUp);
                }

                let mut launch_idx: Option<usize> = None;
                if ctx.input(|i| i.key_pressed(egui::Key::Enter))
                    && !self.bookmark_alias_dialog.open
                    && !self.tempfile_alias_dialog.open
                    && !self.tempfile_dialog.open
                    && !self.notes_dialog.open
                {
                    launch_idx = self.handle_key(egui::Key::Enter);
                }

                if let Some(i) = launch_idx {
                    if let Some(a) = self.results.get(i) {
                        let a = a.clone();
                        let current = self.query.clone();
                        let mut refresh = false;
                        let mut set_focus = false;
                        if a.action == "help:show" {
                            self.help_window.open = true;
                        } else if a.action == "timer:dialog:timer" {
                            self.timer_dialog.open_timer();
                        } else if a.action == "timer:dialog:alarm" {
                            self.timer_dialog.open_alarm();
                        } else if a.action == "shell:dialog" {
                            self.shell_cmd_dialog.open();
                        } else if a.action == "note:dialog" {
                            self.notes_dialog.open();
                        } else if a.action == "bookmark:dialog" {
                            self.add_bookmark_dialog.open();
                        } else if a.action == "snippet:dialog" {
                            self.snippet_dialog.open();
                        } else if a.action == "todo:dialog" {
                            self.todo_dialog.open();
                        } else if a.action == "clipboard:dialog" {
                            self.clipboard_dialog.open();
                        } else if a.action == "tempfile:dialog" {
                            self.tempfile_dialog.open();
                        } else if a.action == "volume:dialog" {
                            self.volume_dialog.open();
                        } else if a.action == "brightness:dialog" {
                            self.brightness_dialog.open();
                        } else if let Some(n) = a.action.strip_prefix("sysinfo:cpu_list:") {
                            if let Ok(count) = n.parse::<usize>() {
                                self.cpu_list_dialog.open(count);
                            }
                        } else if let Err(e) = launch_action(&a) {
                            self.error = Some(format!("Failed: {e}"));
                            self.error_time = Some(Instant::now());
                            if self.enable_toasts {
                                self.toasts.add(Toast {
                                    text: format!("Failed: {e}").into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                        } else {
                            if self.enable_toasts {
                                let msg = if a.action == "recycle:clean" {
                                    "Emptied Recycle Bin".to_string()
                                } else if a.action.starts_with("clipboard:") {
                                    format!("Copied {}", a.label)
                                } else {
                                    format!("Launched {}", a.label)
                                };
                                self.toasts.add(Toast {
                                    text: msg.into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                            if a.action != "help:show" {
                                let _ = history::append_history(
                                    HistoryEntry {
                                        query: current,
                                        action: a.clone(),
                                    },
                                    self.history_limit,
                                );
                                let count = self.usage.entry(a.action.clone()).or_insert(0);
                                *count += 1;
                            }
                            if a.action.starts_with("bookmark:add:") {
                                self.query.clear();
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("bookmark:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("folder:add:") {
                                self.query.clear();
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("folder:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("todo:add:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("todo:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("todo:done:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action == "todo:clear" {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("tempfile:remove:") {
                                refresh = true;
                                set_focus = true;
                            } else if a.action.starts_with("tempfile:alias:") {
                                refresh = true;
                                set_focus = true;
                            }
                            if self.hide_after_run
                                && !a.action.starts_with("bookmark:add:")
                                && !a.action.starts_with("bookmark:remove:")
                                && !a.action.starts_with("folder:add:")
                                && !a.action.starts_with("folder:remove:")
                                && !a.action.starts_with("calc:")
                            {
                                self.visible_flag.store(false, Ordering::SeqCst);
                            }
                        }
                        if refresh {
                            self.search();
                        }
                        if set_focus {
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
                        let alias_list = crate::plugins::folders::load_folders(
                            crate::plugins::folders::FOLDERS_FILE,
                        )
                        .unwrap_or_default();
                        let custom = alias_list
                            .iter()
                            .map(|f| f.path.clone())
                            .collect::<std::collections::HashSet<_>>();
                        let alias_map = alias_list
                            .into_iter()
                            .map(|f| (f.path, f.alias))
                            .collect::<std::collections::HashMap<_, _>>();
                        let bm_list = crate::plugins::bookmarks::load_bookmarks(
                            crate::plugins::bookmarks::BOOKMARKS_FILE,
                        )
                        .unwrap_or_default();
                        let bm_custom = bm_list
                            .iter()
                            .map(|b| b.url.clone())
                            .collect::<std::collections::HashSet<_>>();
                        let show_full = self
                            .enabled_capabilities
                            .as_ref()
                            .and_then(|m| m.get("folders"))
                            .map(|caps| caps.contains(&"show_full_path".to_string()))
                            .unwrap_or(false);
                        for (idx, a) in self.results.iter().enumerate() {
                            let aliased = alias_map.get(&a.action).and_then(|v| v.as_ref());
                            let show_path = show_full || aliased.is_none();
                            let text = if show_path {
                                format!("{} : {}", a.label, a.desc)
                            } else {
                                a.label.clone()
                            };
                            let mut resp = ui.selectable_label(self.selected == Some(idx), text);
                            let menu_resp = resp.on_hover_text(&a.action);
                            let custom_idx = self
                                .actions
                                .iter()
                                .take(self.custom_len)
                                .position(|act| act.action == a.action && act.label == a.label);
                            if custom.contains(&a.action) && !a.action.starts_with("folder:") {
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
                                                self.toasts.add(Toast {
                                                    text: format!("Removed folder {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(3.0),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if bm_custom.contains(&a.action) {
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
                                                self.toasts.add(Toast {
                                                    text: format!("Removed bookmark {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(3.0),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
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
                                                self.toasts.add(Toast {
                                                    text: format!("Removed snippet {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(3.0),
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
                                                self.toasts.add(Toast {
                                                    text: format!("Removed file {}", a.label)
                                                        .into(),
                                                    kind: ToastKind::Success,
                                                    options: ToastOptions::default()
                                                        .duration_in_seconds(3.0),
                                                });
                                            }
                                        }
                                        ui.close_menu();
                                    }
                                });
                            } else if a.desc == "Note"
                                && (a.action.starts_with("note:copy:")
                                    || a.action.starts_with("note:remove:"))
                            {
                                let idx_str = a.action.rsplit(':').next().unwrap_or("");
                                if let Ok(note_idx) = idx_str.parse::<usize>() {
                                    let note_label = a.label.clone();
                                    menu_resp.clone().context_menu(|ui| {
                                        if ui.button("Edit Note").clicked() {
                                            self.notes_dialog.open_edit(note_idx);
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove Note").clicked() {
                                            if let Err(e) = crate::plugins::notes::remove_note(
                                                crate::plugins::notes::QUICK_NOTES_FILE,
                                                note_idx,
                                            ) {
                                                self.error =
                                                    Some(format!("Failed to remove note: {e}"));
                                            } else {
                                                refresh = true;
                                                set_focus = true;
                                                if self.enable_toasts {
                                                    self.toasts.add(Toast {
                                                        text: format!(
                                                            "Removed note {}",
                                                            note_label
                                                        )
                                                        .into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(3.0),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                }
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
                                                    self.toasts.add(Toast {
                                                        text: format!("Removed entry {}", cb_label)
                                                            .into(),
                                                        kind: ToastKind::Success,
                                                        options: ToastOptions::default()
                                                            .duration_in_seconds(3.0),
                                                    });
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                            if let Some(idx_act) = custom_idx {
                                menu_resp.clone().context_menu(|ui| {
                                    if ui.button("Edit Command").clicked() {
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
                                let a = a.clone();
                                let current = self.query.clone();
                                if a.action == "help:show" {
                                    self.help_window.open = true;
                                } else if a.action == "timer:dialog:timer" {
                                    self.timer_dialog.open_timer();
                                } else if a.action == "timer:dialog:alarm" {
                                    self.timer_dialog.open_alarm();
                                } else if a.action == "shell:dialog" {
                                    self.shell_cmd_dialog.open();
                                } else if a.action == "note:dialog" {
                                    self.notes_dialog.open();
                                } else if a.action == "bookmark:dialog" {
                                    self.add_bookmark_dialog.open();
                                } else if a.action == "snippet:dialog" {
                                    self.snippet_dialog.open();
                                } else if a.action == "todo:dialog" {
                                    self.todo_dialog.open();
                                } else if a.action == "clipboard:dialog" {
                                    self.clipboard_dialog.open();
                                } else if a.action == "tempfile:dialog" {
                                    self.tempfile_dialog.open();
                                } else if a.action == "volume:dialog" {
                                    self.volume_dialog.open();
                                } else if a.action == "brightness:dialog" {
                                    self.brightness_dialog.open();
                                } else if let Some(n) = a.action.strip_prefix("sysinfo:cpu_list:") {
                                    if let Ok(count) = n.parse::<usize>() {
                                        self.cpu_list_dialog.open(count);
                                    }
                                } else if let Err(e) = launch_action(&a) {
                                    self.error = Some(format!("Failed: {e}"));
                                    self.error_time = Some(Instant::now());
                                    if self.enable_toasts {
                                        self.toasts.add(Toast {
                                            text: format!("Failed: {e}").into(),
                                            kind: ToastKind::Error,
                                            options: ToastOptions::default()
                                                .duration_in_seconds(3.0),
                                        });
                                    }
                                } else {
                                    if self.enable_toasts {
                                        let msg = if a.action == "recycle:clean" {
                                            "Emptied Recycle Bin".to_string()
                                        } else if a.action.starts_with("clipboard:") {
                                            format!("Copied {}", a.label)
                                        } else {
                                            format!("Launched {}", a.label)
                                        };
                                        self.toasts.add(Toast {
                                            text: msg.into(),
                                            kind: ToastKind::Success,
                                            options: ToastOptions::default()
                                                .duration_in_seconds(3.0),
                                        });
                                    }
                                    if a.action != "help:show" {
                                        let _ = history::append_history(
                                            HistoryEntry {
                                                query: current,
                                                action: a.clone(),
                                            },
                                            self.history_limit,
                                        );
                                        let count = self.usage.entry(a.action.clone()).or_insert(0);
                                        *count += 1;
                                    }
                                    if a.action.starts_with("bookmark:add:") {
                                        self.query.clear();
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("bookmark:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("folder:add:") {
                                        self.query.clear();
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("folder:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("todo:add:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("todo:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("todo:done:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action == "todo:clear" {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("tempfile:remove:") {
                                        refresh = true;
                                        set_focus = true;
                                    } else if a.action.starts_with("tempfile:alias:") {
                                        refresh = true;
                                        set_focus = true;
                                    }
                                    if self.hide_after_run
                                        && !a.action.starts_with("bookmark:add:")
                                        && !a.action.starts_with("bookmark:remove:")
                                        && !a.action.starts_with("folder:add:")
                                        && !a.action.starts_with("folder:remove:")
                                        && !a.action.starts_with("calc:")
                                    {
                                        self.visible_flag.store(false, Ordering::SeqCst);
                                    }
                                }
                                self.selected = Some(idx);
                            }
                        }
                        if refresh {
                            self.search();
                        }
                        if set_focus {
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
        let mut timer_help = std::mem::take(&mut self.timer_help);
        timer_help.ui(ctx);
        self.timer_help = timer_help;
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
        let mut notes_dlg = std::mem::take(&mut self.notes_dialog);
        notes_dlg.ui(ctx, self);
        self.notes_dialog = notes_dlg;
        let mut todo_dlg = std::mem::take(&mut self.todo_dialog);
        todo_dlg.ui(ctx, self);
        self.todo_dialog = todo_dlg;
        let mut cb_dlg = std::mem::take(&mut self.clipboard_dialog);
        cb_dlg.ui(ctx, self);
        self.clipboard_dialog = cb_dlg;
        let mut vol_dlg = std::mem::take(&mut self.volume_dialog);
        vol_dlg.ui(ctx, self);
        self.volume_dialog = vol_dlg;
        let mut bright_dlg = std::mem::take(&mut self.brightness_dialog);
        bright_dlg.ui(ctx, self);
        self.brightness_dialog = bright_dlg;
        let mut cpu_dlg = std::mem::take(&mut self.cpu_list_dialog);
        cpu_dlg.ui(ctx, self);
        self.cpu_list_dialog = cpu_dlg;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.unregister_all_hotkeys();
        self.visible_flag.store(false, Ordering::SeqCst);
        self.last_visible = false;
        if let Ok(mut settings) = crate::settings::Settings::load(&self.settings_path) {
            settings.window_size = Some(self.window_size);
            let _ = settings.save(&self.settings_path);
        }
        let _ = usage::save_usage(USAGE_FILE, &self.usage);
        #[cfg(not(test))]
        std::process::exit(0);
    }
}
