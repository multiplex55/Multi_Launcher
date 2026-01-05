use super::{
    edit_typed_settings, find_plugin, refresh_interval_setting, TimedCache, Widget, WidgetAction,
    WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_limit() -> usize {
    8
}

fn default_refresh_interval() -> f32 {
    5.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessesConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl Default for ProcessesConfig {
    fn default() -> Self {
        Self {
            refresh_interval_secs: default_refresh_interval(),
            limit: default_limit(),
        }
    }
}

pub struct ProcessesWidget {
    cfg: ProcessesConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
}

impl ProcessesWidget {
    pub fn new(cfg: ProcessesConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            error: None,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut ProcessesConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show up to");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.limit).clamp_range(1..=25))
                    .changed();
                ui.label("processes");
            });
            changed |= refresh_interval_setting(
                ui,
                &mut cfg.refresh_interval_secs,
                "Process enumeration is cached. The widget will skip refreshing until this many seconds have passed. Use Refresh to update immediately.",
            );
            changed
        })
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let (actions, error) = Self::load_processes(ctx);
        self.error = error;
        self.cache.refresh(|data| *data = actions);
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        if self.cache.should_refresh() {
            self.refresh(ctx);
        }
    }

    fn load_processes(ctx: &DashboardContext<'_>) -> (Vec<Action>, Option<String>) {
        let Some(plugin) = find_plugin(ctx, "processes") else {
            return (Vec::new(), Some("Processes plugin not available.".into()));
        };
        (plugin.search("ps"), None)
    }

    fn grouped_actions(&self) -> Vec<(String, Option<Action>, Option<Action>)> {
        let mut grouped: Vec<(String, Option<Action>, Option<Action>)> = Vec::new();
        for action in &self.cache.data {
            let label = action.label.to_lowercase();
            let idx = grouped.iter().position(|(desc, _, _)| desc == &action.desc);
            let entry = if let Some(idx) = idx {
                &mut grouped[idx]
            } else {
                grouped.push((action.desc.clone(), None, None));
                grouped.last_mut().unwrap()
            };
            if label.starts_with("switch to") {
                entry.1 = Some(action.clone());
            } else if label.starts_with("kill") {
                entry.2 = Some(action.clone());
            } else {
                entry.1.get_or_insert_with(|| action.clone());
            }
        }
        grouped.truncate(self.cfg.limit);
        grouped
    }
}

impl Default for ProcessesWidget {
    fn default() -> Self {
        Self::new(ProcessesConfig::default())
    }
}

impl Widget for ProcessesWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.maybe_refresh(ctx);

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::YELLOW, err);
        }

        if self.cache.data.is_empty() {
            ui.label("No processes found.");
            return None;
        }

        let mut clicked = None;
        for (desc, switch, kill) in self.grouped_actions() {
            ui.horizontal(|ui| {
                if let Some(action) = &switch {
                    if ui.button(&action.label).clicked() {
                        clicked = Some(action.clone());
                    }
                }
                if let Some(action) = &kill {
                    if ui.small_button("Kill").clicked() {
                        clicked = Some(action.clone());
                    }
                }
                ui.label(egui::RichText::new(desc).small());
            });
        }

        clicked.map(|action| WidgetAction {
            query_override: Some(action.label.clone()),
            action,
        })
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<ProcessesConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let tooltip = format!(
            "Cached for {:.0}s. Refresh to enumerate processes immediately.",
            self.cfg.refresh_interval_secs
        );
        if ui.small_button("Refresh").on_hover_text(tooltip).clicked() {
            self.refresh(ctx);
        }
        None
    }
}
