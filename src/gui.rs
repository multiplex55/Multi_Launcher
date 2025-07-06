use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::plugin_editor::PluginEditor;
use crate::settings_editor::SettingsEditor;
use crate::settings::Settings;
use crate::launcher::launch_action;
use crate::plugin::PluginManager;
use crate::indexer;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config, EventKind};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use crate::visibility::apply_visibility;
use std::collections::HashMap;

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
    window_size: (i32, i32),
}

impl LauncherApp {
    pub fn update_paths(
        &mut self,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
        enabled_plugins: Option<Vec<String>>,
        enabled_capabilities: Option<std::collections::HashMap<String, Vec<String>>>,
        offscreen_pos: Option<(i32, i32)>,
    ) {
        self.plugin_dirs = plugin_dirs;
        self.index_paths = index_paths;
        self.enabled_plugins = enabled_plugins;
        self.enabled_capabilities = enabled_capabilities;
        if let Some((x, y)) = offscreen_pos {
            self.offscreen_pos = (x as f32, y as f32);
        }
    }

    pub fn new(
        ctx: &egui::Context,
        actions: Vec<Action>,
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
    ) -> Self {
        let (tx, rx) = channel();
        let mut watchers = Vec::new();

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
        };

        tracing::debug!("initial viewport visible: {}", initial_visible);
        apply_visibility(initial_visible, ctx, offscreen_pos);

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
        let mut res: Vec<Action> = if self.query.is_empty() {
            self.actions.clone()
        } else {
            self.actions
                .iter()
                .filter(|a| {
                    self.matcher.fuzzy_match(&a.label, &self.query).is_some()
                        || self.matcher.fuzzy_match(&a.desc, &self.query).is_some()
                })
                .cloned()
                .collect()
        };

        // append plugin results respecting enabled plugin settings
        res.extend(self.plugins.search_filtered(
            &self.query,
            self.enabled_plugins.as_ref(),
            self.enabled_capabilities.as_ref(),
        ));

        // sort by usage count if available
        res.sort_by_key(|a| std::cmp::Reverse(self.usage.get(&a.action).cloned().unwrap_or(0)));

        self.results = res;
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
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.window_size = (rect.width() as i32, rect.height() as i32);
        }
        let do_restore = self.restore_flag.swap(false, Ordering::SeqCst);
        if do_restore {
            tracing::debug!("Restoring window on restore_flag");
            apply_visibility(true, ctx, self.offscreen_pos);
            #[cfg(target_os = "windows")]
            if let Some(hwnd) = crate::window_manager::get_hwnd(frame) {
                crate::window_manager::force_restore_and_foreground(hwnd);
            }
        }

        let should_be_visible = self.visible_flag.load(Ordering::SeqCst);
        let just_became_visible = !self.last_visible && should_be_visible;
        if self.last_visible != should_be_visible {
            tracing::debug!("gui thread -> visible: {}", should_be_visible);
            apply_visibility(should_be_visible, ctx, self.offscreen_pos);
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
                    if ui.button("Force Hide").clicked() {
                        apply_visibility(false, ctx, self.offscreen_pos);
                        self.visible_flag.store(false, Ordering::SeqCst);
                        self.last_visible = false;
                    }
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
            });
        });

        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WatchEvent::Actions => {
                    if let Ok(mut acts) = load_actions(&self.actions_path) {
                        if let Some(paths) = &self.index_paths {
                            acts.extend(indexer::index_paths(paths));
                        }
                        self.actions = acts;
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

            let input = ui.text_edit_singleline(&mut self.query);
            if just_became_visible {
                input.request_focus();
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
                    let action_str = a.action.clone();
                    if let Err(e) = launch_action(a) {
                        self.error = Some(format!("Failed: {e}"));
                    } else {
                        let count = self.usage.entry(action_str.clone()).or_insert(0);
                        *count += 1;
                        if action_str.starts_with("bookmark:add:") {
                            self.query.clear();
                            self.search();
                        } else if action_str.starts_with("bookmark:remove:") {
                            self.search();
                        }
                    }
                }
            }

            ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for (idx, a) in self.results.iter().enumerate() {
                    let label = format!("{} : {}", a.label, a.desc);
                    if ui.selectable_label(self.selected == Some(idx), label).clicked() {
                        let action_str = a.action.clone();
                        if let Err(e) = launch_action(a) {
                            self.error = Some(format!("Failed: {e}"));
                        } else {
                            let count = self.usage.entry(action_str.clone()).or_insert(0);
                            *count += 1;
                            if action_str.starts_with("bookmark:add:") {
                                self.query.clear();
                                self.search();
                            } else if action_str.starts_with("bookmark:remove:") {
                                self.search();
                            }
                        }
                        self.selected = Some(idx);
                    }
                }
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
