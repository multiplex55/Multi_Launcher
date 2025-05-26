use crate::actions::Action;
use crate::launcher::launch_action;
use eframe::egui;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct LauncherApp {
    pub actions: Vec<Action>,
    pub query: String,
    pub results: Vec<Action>,
    pub matcher: SkimMatcherV2,
    pub error: Option<String>,
}

impl LauncherApp {
    pub fn new(actions: Vec<Action>) -> Self {
        Self {
            actions: actions.clone(),
            query: String::new(),
            results: actions,
            matcher: SkimMatcherV2::default(),
            error: None,
        }
    }

    fn search(&mut self) {
        if self.query.is_empty() {
            self.results = self.actions.clone();
        } else {
            self.results = self
                .actions
                .iter()
                .filter(|a| {
                    self.matcher.fuzzy_match(&a.label, &self.query).is_some()
                        || self.matcher.fuzzy_match(&a.desc, &self.query).is_some()
                })
                .cloned()
                .collect();
        }
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
                        }
                    }
                }
            });
        });
    }
}
