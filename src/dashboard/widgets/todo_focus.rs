use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{mark_done, TodoEntry, TODO_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFocusConfig {
    #[serde(default)]
    pub show_done: bool,
    #[serde(default)]
    pub query: Option<String>,
}

impl Default for TodoFocusConfig {
    fn default() -> Self {
        Self {
            show_done: false,
            query: Some("todo".into()),
        }
    }
}

pub struct TodoFocusWidget {
    cfg: TodoFocusConfig,
}

impl TodoFocusWidget {
    pub fn new(cfg: TodoFocusConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoFocusConfig, _ctx| {
            let mut changed = false;
            changed |= ui
                .checkbox(&mut cfg.show_done, "Include completed")
                .changed();
            ui.horizontal(|ui| {
                ui.label("Query override");
                let mut query = cfg.query.clone().unwrap_or_default();
                if ui.text_edit_singleline(&mut query).changed() {
                    cfg.query = if query.trim().is_empty() {
                        None
                    } else {
                        Some(query)
                    };
                    changed = true;
                }
            });
            changed
        })
    }

    fn pick_focus(&self, entries: &[TodoEntry]) -> Option<(usize, TodoEntry)> {
        let mut todos: Vec<(usize, TodoEntry)> = entries.iter().cloned().enumerate().collect();
        if !self.cfg.show_done {
            todos.retain(|(_, t)| !t.done);
        }
        todos.sort_by(|a, b| b.1.priority.cmp(&a.1.priority).then_with(|| a.0.cmp(&b.0)));
        todos.into_iter().next()
    }
}

impl Default for TodoFocusWidget {
    fn default() -> Self {
        Self::new(TodoFocusConfig::default())
    }
}

impl Widget for TodoFocusWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let Some((idx, entry)) = self.pick_focus(snapshot.todos.as_ref()) else {
            ui.label("No todos to focus on.");
            return None;
        };

        let mut done = entry.done;
        let mut clicked = None;
        ui.horizontal(|ui| {
            if ui.checkbox(&mut done, "").changed() {
                if let Err(err) = mark_done(TODO_FILE, idx) {
                    tracing::error!("Failed to toggle todo #{idx}: {err}");
                } else {
                    ctx.data_cache.request_refresh_todos();
                }
            }
            let mut label = entry.text.clone();
            if entry.priority > 0 {
                label.push_str(&format!(" (p{})", entry.priority));
            }
            let text = if entry.done {
                egui::RichText::new(label).strikethrough()
            } else {
                egui::RichText::new(label)
            };
            ui.label(text);
            if ui.small_button("Open").clicked() {
                clicked = Some(WidgetAction {
                    action: Action {
                        label: entry.text.clone(),
                        desc: "Todo".into(),
                        action: format!("todo:edit:{idx}"),
                        args: None,
                    },
                    query_override: self
                        .cfg
                        .query
                        .clone()
                        .or_else(|| Some(format!("todo edit {}", entry.text))),
                });
            }
        });
        if !entry.tags.is_empty() {
            ui.label(egui::RichText::new(format!("#{}", entry.tags.join(" #"))).small());
        }
        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<TodoFocusConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}
