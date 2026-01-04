use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{mark_done, TodoEntry, TODO_DATA, TODO_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoSort {
    Priority,
    Created,
    Alphabetical,
}

impl Default for TodoSort {
    fn default() -> Self {
        TodoSort::Priority
    }
}

fn default_count() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoListConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub show_done: bool,
    #[serde(default)]
    pub sort: TodoSort,
}

impl Default for TodoListConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_done: false,
            sort: TodoSort::default(),
        }
    }
}

pub struct TodoListWidget {
    cfg: TodoListConfig,
}

impl TodoListWidget {
    pub fn new(cfg: TodoListConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoListConfig, _ctx| {
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("Show");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.count).clamp_range(1..=50))
                    .changed();
                ui.label("todos");
            });
            changed |= ui
                .checkbox(&mut cfg.show_done, "Include completed")
                .changed();
            egui::ComboBox::from_label("Sort by")
                .selected_text(match cfg.sort {
                    TodoSort::Priority => "Priority",
                    TodoSort::Created => "Created order",
                    TodoSort::Alphabetical => "Alphabetical",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut cfg.sort, TodoSort::Priority, "Priority")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.sort, TodoSort::Created, "Created order")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.sort, TodoSort::Alphabetical, "Alphabetical")
                        .changed();
                });
            changed
        })
    }

    fn sort_entries(entries: &mut Vec<(usize, TodoEntry)>, sort: TodoSort) {
        match sort {
            TodoSort::Priority => {
                entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority).then_with(|| a.0.cmp(&b.0)))
            }
            TodoSort::Created => entries.sort_by_key(|(idx, _)| *idx),
            TodoSort::Alphabetical => entries.sort_by(|a, b| {
                a.1.text
                    .to_lowercase()
                    .cmp(&b.1.text.to_lowercase())
                    .then_with(|| a.0.cmp(&b.0))
            }),
        }
    }
}

impl Default for TodoListWidget {
    fn default() -> Self {
        Self {
            cfg: TodoListConfig::default(),
        }
    }
}

impl Widget for TodoListWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let todos = TODO_DATA.read().ok().map(|t| t.clone()).unwrap_or_default();

        let mut entries: Vec<(usize, TodoEntry)> = todos.into_iter().enumerate().collect();
        if !self.cfg.show_done {
            entries.retain(|(_, t)| !t.done);
        }
        Self::sort_entries(&mut entries, self.cfg.sort);
        entries.truncate(self.cfg.count);

        if entries.is_empty() {
            ui.label("No todos to show");
            return None;
        }

        let mut clicked = None;
        for (idx, entry) in entries.into_iter() {
            let mut entry_done = entry.done;
            ui.horizontal(|ui| {
                if ui.checkbox(&mut entry_done, "").changed() {
                    if let Err(err) = mark_done(TODO_FILE, idx) {
                        tracing::error!("Failed to toggle todo #{idx}: {err}");
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
                if !entry.tags.is_empty() {
                    let tags = entry.tags.join(", ");
                    ui.label(egui::RichText::new(format!("#{tags}")).small());
                }
                if ui.small_button("Open").clicked() {
                    clicked = Some(WidgetAction {
                        action: Action {
                            label: entry.text.clone(),
                            desc: "Todo".into(),
                            action: format!("todo:edit:{idx}"),
                            args: None,
                        },
                        query_override: Some(format!("todo edit {}", entry.text)),
                    });
                }
            });
        }

        clicked
    }
}
