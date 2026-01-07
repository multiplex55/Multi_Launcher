use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{mark_done, TodoEntry, TODO_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FocusedTodoSelection {
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl FocusedTodoSelection {
    fn normalized(text: &str, tags: &[String]) -> Self {
        let mut normalized_tags: Vec<String> =
            tags.iter().map(|tag| tag.trim().to_lowercase()).collect();
        normalized_tags.sort();
        normalized_tags.dedup();
        Self {
            text: text.trim().to_lowercase(),
            tags: normalized_tags,
        }
    }

    fn display_label(original: &TodoEntry) -> String {
        if original.tags.is_empty() {
            original.text.clone()
        } else {
            format!("{} #{}", original.text, original.tags.join(" #"))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFocusConfig {
    #[serde(default)]
    pub show_done: bool,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub focused_todos: Vec<FocusedTodoSelection>,
}

impl Default for TodoFocusConfig {
    fn default() -> Self {
        Self {
            show_done: false,
            query: Some("todo".into()),
            focused_todos: Vec::new(),
        }
    }
}

pub struct TodoFocusWidget {
    cfg: TodoFocusConfig,
}

impl TodoFocusWidget {
    pub fn new(cfg: TodoFocusConfig) -> Self {
        Self {
            cfg: Self::normalize_config(cfg),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoFocusConfig, ctx| {
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
            ui.separator();
            ui.push_id("notes_snapshot", |ui| {
                ui.label("Notes snapshot");
                if let Some(notes) = ctx.notes {
                    if notes.is_empty() {
                        ui.label("No notes available.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(120.0)
                            .show(ui, |ui| {
                                for note in notes {
                                    let mut label = if note.title.is_empty() {
                                        note.slug.clone()
                                    } else {
                                        note.title.clone()
                                    };
                                    label.push_str(&format!(" ({})", note.slug));
                                    if !note.tags.is_empty() {
                                        label.push_str(&format!(" #{}", note.tags.join(" #")));
                                    }
                                    ui.label(label);
                                }
                            });
                    }
                } else {
                    ui.label("Notes data unavailable.");
                }
            });
            ui.separator();
            ui.push_id("focused_todos", |ui| {
                ui.label("Focused todos");
                ui.label(
                    egui::RichText::new(
                        "Matching uses exact todo text + tags (case-insensitive). Tags are sorted \
                        and deduplicated. If text/tags change, re-select the todo.",
                    )
                    .small(),
                );
                if let Some(todos) = ctx.todos {
                    if todos.is_empty() {
                        ui.label("No todos available.");
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for todo in todos {
                                    let selection =
                                        FocusedTodoSelection::normalized(&todo.text, &todo.tags);
                                    let mut is_selected =
                                        cfg.focused_todos.iter().any(|t| t == &selection);
                                    let label = FocusedTodoSelection::display_label(todo);
                                    if ui.checkbox(&mut is_selected, label).changed() {
                                        if is_selected {
                                            cfg.focused_todos.push(selection);
                                        } else {
                                            cfg.focused_todos.retain(|t| t != &selection);
                                        }
                                        changed = true;
                                    }
                                }
                            });
                    }
                } else {
                    ui.label("Todo data unavailable.");
                }
            });
            changed
        })
    }

    fn pick_focus(&self, entries: &[TodoEntry]) -> Option<(usize, TodoEntry)> {
        let mut todos: Vec<(usize, TodoEntry)> = entries.iter().cloned().enumerate().collect();
        if !self.cfg.focused_todos.is_empty() {
            let focused: HashSet<FocusedTodoSelection> =
                self.cfg.focused_todos.iter().cloned().collect();
            todos.retain(|(_, entry)| {
                focused.contains(&FocusedTodoSelection::normalized(&entry.text, &entry.tags))
            });
        }
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
                    ctx.data_cache.refresh_todos();
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
            self.cfg = Self::normalize_config(cfg);
        }
    }
}

impl TodoFocusWidget {
    fn normalize_config(mut cfg: TodoFocusConfig) -> TodoFocusConfig {
        for selection in &mut cfg.focused_todos {
            *selection = FocusedTodoSelection::normalized(&selection.text, &selection.tags);
        }
        cfg.focused_todos.sort_by(|a, b| a.text.cmp(&b.text).then_with(|| a.tags.cmp(&b.tags)));
        cfg.focused_todos.dedup();
        cfg
    }
}
