use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::settings_editor::SettingsEditor;
use crate::settings::Settings;
use crate::launcher::launch_action;
use crate::plugin::PluginManager;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::indexer;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config, EventKind};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;

enum WatchEvent {
    Actions,
    Plugins,
}

pub struct LauncherApp {
    pub actions: Vec<Action>,
    pub query: String,
    pub results: Vec<Action>,
    pub matcher: SkimMatcherV2,
    pub error: Option<String>,
    pub plugins: PluginManager,
    pub usage: HashMap<String, u32>,
    pub registered_hotkeys: Mutex<HashMap<String, usize>>,
    pub show_editor: bool,
    pub show_settings: bool,
    pub actions_path: String,
    pub editor: ActionsEditor,
    pub settings_editor: SettingsEditor,
    pub settings_path: String,
    #[allow(dead_code)]
    watchers: Vec<RecommendedWatcher>,
    rx: Receiver<WatchEvent>,
    plugin_dirs: Option<Vec<String>>,
    index_paths: Option<Vec<String>>,
    visible_flag: Arc<AtomicBool>,
    last_visible: bool,
}

impl LauncherApp {
    pub fn update_paths(&mut self, plugin_dirs: Option<Vec<String>>, index_paths: Option<Vec<String>>) {
        self.plugin_dirs = plugin_dirs;
        self.index_paths = index_paths;
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
        visible_flag: Arc<AtomicBool>,
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

        if let Some(dirs) = &plugin_dirs {
            for dir in dirs {
                let dir_clone = dir.clone();
                if let Ok(mut watcher) = RecommendedWatcher::new(
                    {
                        let tx = tx.clone();
                        move |res: notify::Result<notify::Event>| match res {
                            Ok(event) => {
                                if matches!(
                                    event.kind,
                                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                                ) {
                                    let _ = tx.send(WatchEvent::Plugins);
                                }
                            }
                            Err(e) => tracing::error!("watch error: {:?}", e),
                        }
                    },
                    Config::default(),
                ) {
                    use std::path::Path;
                    if watcher.watch(Path::new(&dir_clone), RecursiveMode::Recursive).is_ok() {
                        watchers.push(watcher);
                    }
                }
            }
        }

        let initial_visible = visible_flag.load(Ordering::SeqCst);

        let app = Self {
            actions: actions.clone(),
            query: String::new(),
            results: actions,
            matcher: SkimMatcherV2::default(),
            error: None,
            plugins,
            usage: HashMap::new(),
            registered_hotkeys: Mutex::new(HashMap::new()),
            show_editor: false,
            show_settings: false,
            actions_path,
            editor: ActionsEditor::default(),
            settings_editor: SettingsEditor::new(&settings),
            settings_path,
            watchers,
            rx,
            plugin_dirs,
            index_paths,
            visible_flag: visible_flag.clone(),
            last_visible: initial_visible,
        };

        tracing::debug!("initial viewport visible: {}", initial_visible);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(initial_visible));

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

        // append plugin results
        res.extend(self.plugins.search(&self.query));

        // sort by usage count if available
        res.sort_by_key(|a| std::cmp::Reverse(self.usage.get(&a.action).cloned().unwrap_or(0)));

        self.results = res;
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

        let should_be_visible = self.visible_flag.load(Ordering::SeqCst);
        tracing::debug!(
            should_be_visible=?should_be_visible,
            last_visible=?self.last_visible
        );
        let just_became_visible = !self.last_visible && should_be_visible;
        if self.last_visible != should_be_visible {
            tracing::debug!("gui thread -> visible: {}", should_be_visible);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(should_be_visible));
            tracing::debug!("ViewportCommand::Visible({}) sent", should_be_visible);
            self.last_visible = should_be_visible;
        }

        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("Commands", |ui| {
                        if ui.button("Edit Commands").clicked() {
                            self.show_editor = !self.show_editor;
                        }
                    });
                    if ui.button("Force Hide").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        ctx.request_repaint();
                        self.visible_flag.store(false, Ordering::SeqCst);
                        self.last_visible = false;
                    }
                    if ui.button("Close Application").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.visible_flag.store(false, Ordering::SeqCst);
                        self.last_visible = false;
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
                WatchEvent::Plugins => {
                    let mut plugins = PluginManager::new();
                    plugins.register(Box::new(WebSearchPlugin));
                    plugins.register(Box::new(CalculatorPlugin));
                    if let Some(dirs) = &self.plugin_dirs {
                        for dir in dirs {
                            if let Err(e) = plugins.load_dir(dir) {
                                tracing::error!("Failed to load plugins from {}: {}", dir, e);
                            }
                        }
                    }
                    self.plugins = plugins;
                    self.search();
                    tracing::info!("plugins reloaded");
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

            if self.results.len() == 1 && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Err(e) = launch_action(&self.results[0]) {
                    self.error = Some(format!("Failed: {e}"));
                } else {
                    let a = &self.results[0];
                    let count = self.usage.entry(a.action.clone()).or_insert(0);
                    *count += 1;
                }
            }

            ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                for a in self.results.iter() {
                    if ui.button(format!("{} : {}", a.label, a.desc)).clicked() {
                        if let Err(e) = launch_action(a) {
                            self.error = Some(format!("Failed: {e}"));
                        } else {
                            let count = self.usage.entry(a.action.clone()).or_insert(0);
                            *count += 1;
                        }
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
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.unregister_all_hotkeys();
        self.visible_flag.store(false, Ordering::SeqCst);
        self.last_visible = false;
    }
}
