use crate::actions::Action;
use crate::launcher::launch_action;
use crate::plugin::PluginManager;
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;

pub struct LauncherApp {
    pub actions: Vec<Action>,
    pub query: String,
    pub results: Vec<Action>,
    pub matcher: SkimMatcherV2,
    pub error: Option<String>,
    pub plugins: PluginManager,
    pub usage: HashMap<String, u32>,
}

impl LauncherApp {
    pub fn new(actions: Vec<Action>, plugins: PluginManager) -> Self {
        Self {
            actions: actions.clone(),
            query: String::new(),
            results: actions,
            matcher: SkimMatcherV2::default(),
            error: None,
            plugins,
            usage: HashMap::new(),
        }
    }

    fn search(&mut self) {
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

        CentralPanel::default().show(ctx, |ui| {
            ui.heading("🚀 LNCHR");
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
    }
}
