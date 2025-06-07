use crate::actions::{load_actions, Action};
use crate::actions_editor::ActionsEditor;
use crate::launcher::launch_action;
use crate::plugin::PluginManager;
use crate::plugins_builtin::{CalculatorPlugin, WebSearchPlugin};
use crate::indexer;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config, EventKind};
use std::sync::mpsc::{channel, Receiver};
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
    pub show_editor: bool,
    pub actions_path: String,
    pub editor: ActionsEditor,
    #[allow(dead_code)]
    watchers: Vec<RecommendedWatcher>,
    rx: Receiver<WatchEvent>,
    plugin_dirs: Option<Vec<String>>,
    index_paths: Option<Vec<String>>,
}

impl LauncherApp {
    pub fn new(
        actions: Vec<Action>,
        plugins: PluginManager,
        actions_path: String,
        plugin_dirs: Option<Vec<String>>,
        index_paths: Option<Vec<String>>,
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

        Self {
            actions: actions.clone(),
            query: String::new(),
            results: actions,
            matcher: SkimMatcherV2::default(),
            error: None,
            plugins,
            usage: HashMap::new(),
            show_editor: false,
            actions_path,
            editor: ActionsEditor::default(),
            watchers,
            rx,
            plugin_dirs,
            index_paths,
        }
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
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use egui::*;

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
            if ui.button("Edit Commands").clicked() {
                self.show_editor = !self.show_editor;
            }
            if let Some(err) = &self.error {
                ui.colored_label(Color32::RED, err);
            }

            let input = ui.text_edit_singleline(&mut self.query);
            if input.changed() {
                self.search();
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
    }
}
