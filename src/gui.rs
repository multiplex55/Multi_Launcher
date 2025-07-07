use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::plugin_editor::PluginEditor;
use crate::settings_editor::SettingsEditor;
use crate::settings::Settings;
use crate::launcher::launch_action;
use crate::history::{self, HistoryEntry};
use crate::plugin::PluginManager;
use crate::indexer;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config, EventKind};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use eframe::egui;
use egui_toast::{Toasts, Toast, ToastKind, ToastOptions};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use crate::visibility::apply_visibility;
use std::collections::HashMap;
use crate::help_window::HelpWindow;

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
    #[allow(dead_code)]
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
    help_window: crate::help_window::HelpWindow,
    pub help_flag: Arc<AtomicBool>,
    pub hotkey_str: Option<String>,
    pub quit_hotkey_str: Option<String>,
    pub help_hotkey_str: Option<String>,
    pub query_scale: f32,
    pub list_scale: f32,
    /// Number of user defined commands at the start of `actions`.
    pub custom_len: usize,
    pub history_limit: usize,
    pub fuzzy_weight: f32,
    pub usage_weight: f32,
    pub follow_mouse: bool,
    pub static_location_enabled: bool,
    pub static_pos: Option<(i32, i32)>,
    pub static_size: Option<(i32, i32)>,
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
            if watcher.watch(Path::new(&actions_path), RecursiveMode::NonRecursive).is_ok() {
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
            plugins,
            selected: None,
            usage: HashMap::new(),
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
            help_window: HelpWindow::default(),
            help_flag: help_flag.clone(),
            hotkey_str: settings.hotkey.clone(),
            quit_hotkey_str: settings.quit_hotkey.clone(),
            help_hotkey_str: settings.help_hotkey.clone(),
            query_scale,
            list_scale,
            custom_len,
            history_limit: settings.history_limit,
            fuzzy_weight: settings.fuzzy_weight,
            usage_weight: settings.usage_weight,
            follow_mouse,
            static_location_enabled: static_enabled,
            static_pos,
            static_size,
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

        res.extend(self.plugins.search_filtered(
            &self.query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        ).into_iter().map(|a| {
            let score = if self.query.is_empty() {
                0.0
            } else {
                self
                    .matcher
                    .fuzzy_match(&a.label, &self.query)
                    .max(self.matcher.fuzzy_match(&a.desc, &self.query))
                    .unwrap_or(0) as f32 * self.fuzzy_weight
            };
            (a, score)
        }));

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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        use egui::*;

        tracing::debug!("LauncherApp::update called");
        if self.enable_toasts {
            self.toasts.show(ctx);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window_size = (rect.width() as i32, rect.height() as i32);
        }
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = (rect.min.x as i32, rect.min.y as i32);
        }
        let do_restore = self.restore_flag.swap(false, Ordering::SeqCst);
        if self.help_flag.swap(false, Ordering::SeqCst) {
            self.help_window.overlay_open = true;
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
            );
            #[cfg(target_os = "windows")]
            if let Some(hwnd) = crate::window_manager::get_hwnd(frame) {
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
                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
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
                        } else if let Err(e) = launch_action(&a) {
                            self.error = Some(format!("Failed: {e}"));
                            if self.enable_toasts {
                                self.toasts.add(Toast {
                                    text: format!("Failed: {e}").into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                        } else {
                            if self.enable_toasts {
                                self.toasts.add(Toast {
                                    text: format!("Launched {}", a.label).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                            if a.action != "help:show" {
                                let _ = history::append_history(
                                    HistoryEntry { query: current, action: a.clone() },
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
            ScrollArea::vertical().max_height(area_height).show(ui, |ui| {
                scale_ui(ui, self.list_scale, |ui| {
                    let mut refresh = false;
                    let mut set_focus = false;
                    let alias_list = crate::plugins::folders::load_folders(crate::plugins::folders::FOLDERS_FILE)
                        .unwrap_or_default();
                let custom = alias_list
                    .iter()
                    .map(|f| f.path.clone())
                    .collect::<std::collections::HashSet<_>>();
                let alias_map = alias_list
                    .into_iter()
                    .map(|f| (f.path, f.alias))
                    .collect::<std::collections::HashMap<_, _>>();
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
                    let mut menu_resp = resp.on_hover_text(&a.action);
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
                        });
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
                        } else if let Err(e) = launch_action(&a) {
                            self.error = Some(format!("Failed: {e}"));
                            if self.enable_toasts {
                                self.toasts.add(Toast {
                                    text: format!("Failed: {e}").into(),
                                    kind: ToastKind::Error,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                        } else {
                            if self.enable_toasts {
                                self.toasts.add(Toast {
                                    text: format!("Launched {}", a.label).into(),
                                    kind: ToastKind::Success,
                                    options: ToastOptions::default().duration_in_seconds(3.0),
                                });
                            }
                            if a.action != "help:show" {
                                let _ = history::append_history(
                                    HistoryEntry { query: current, action: a.clone() },
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
        let mut help = std::mem::take(&mut self.help_window);
        help.ui(ctx, self);
        self.help_window = help;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.unregister_all_hotkeys();
        self.visible_flag.store(false, Ordering::SeqCst);
        self.last_visible = false;
        if let Ok(mut settings) = crate::settings::Settings::load(&self.settings_path) {
            settings.window_size = Some(self.window_size);
            let _ = settings.save(&self.settings_path);
        }
        #[cfg(not(test))]
        std::process::exit(0);
    }
}
