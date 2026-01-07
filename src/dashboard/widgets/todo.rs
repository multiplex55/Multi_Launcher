use super::{
    edit_typed_settings, query_suggestions, Widget, WidgetAction, WidgetSettingsContext,
    WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{mark_done, TodoEntry, TODO_FILE};
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

fn default_show_progress() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoWidgetConfig {
    #[serde(default = "default_count")]
    pub count: usize,
    #[serde(default)]
    pub show_done: bool,
    #[serde(default = "default_show_progress")]
    pub show_progress: bool,
    #[serde(default)]
    pub filter_tags: Vec<String>,
    #[serde(default)]
    pub sort: TodoSort,
    #[serde(default)]
    pub query: Option<String>,
}

impl Default for TodoWidgetConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_done: false,
            show_progress: default_show_progress(),
            filter_tags: Vec::new(),
            sort: TodoSort::default(),
            query: None,
        }
    }
}

pub struct TodoWidget {
    cfg: TodoWidgetConfig,
}

impl TodoWidget {
    pub fn new(cfg: TodoWidgetConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoWidgetConfig, ctx| {
            let mut changed = false;
            ui.heading("List");
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
            changed |= ui
                .checkbox(&mut cfg.show_progress, "Show progress bar")
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

            ui.separator();
            ui.heading("Open action");
            let suggestions = query_suggestions(ctx, &["todo"], &["todo", "todo list", "todo add"]);
            if cfg.query.is_none() {
                if let Some(s) = suggestions.first() {
                    cfg.query = Some(s.clone());
                    changed = true;
                }
            }
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
            if !suggestions.is_empty() {
                egui::ComboBox::from_label("Suggestions")
                    .selected_text(
                        cfg.query
                            .as_deref()
                            .unwrap_or("Pick a todo query from your plugins"),
                    )
                    .show_ui(ui, |ui| {
                        for suggestion in &suggestions {
                            changed |= ui
                                .selectable_value(
                                    &mut cfg.query,
                                    Some(suggestion.clone()),
                                    suggestion,
                                )
                                .changed();
                        }
                    });
            }

            changed
        })
    }

    fn tags_match(&self, entry: &TodoEntry) -> bool {
        if self.cfg.filter_tags.is_empty() {
            return true;
        }
        self.cfg.filter_tags.iter().any(|tag| {
            entry
                .tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(tag))
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

    fn render_summary(&mut self, ui: &mut egui::Ui, todos: &[TodoEntry]) -> Option<WidgetAction> {
        let filtered: Vec<&TodoEntry> = todos.iter().filter(|t| self.tags_match(t)).collect();
        let done = filtered.iter().filter(|t| t.done).count();
        let total = filtered.len();
        let remaining = total.saturating_sub(done);
        let mut action = None;
        ui.vertical(|ui| {
            let mut tags_value = self.cfg.filter_tags.join(", ");
            ui.horizontal(|ui| {
                ui.label("Filter tags");
                if ui.text_edit_singleline(&mut tags_value).changed() {
                    self.cfg.filter_tags = parse_tags(&tags_value);
                }
            });
            ui.horizontal(|ui| {
                ui.label(format!("Todos: {done}/{total} done"));
                if ui.button("Open todos").clicked() {
                    action = Some(WidgetAction {
                        action: Action {
                            label: "Todos".into(),
                            desc: "Todo".into(),
                            action: "todo:dialog".into(),
                            args: None,
                        },
                        query_override: self.cfg.query.clone().or_else(|| Some("todo".into())),
                    });
                }
            });
            if self.cfg.show_progress {
                ui.label(format!("Remaining: {remaining}"));
                if total > 0 {
                    let pct = done as f32 / total as f32;
                    ui.add(egui::ProgressBar::new(pct).show_percentage());
                }
            }
        });
        action
    }

    fn render_list(
        &self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        todos: &[TodoEntry],
    ) -> Option<WidgetAction> {
        let mut entries: Vec<(usize, TodoEntry)> = todos
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, t)| self.tags_match(t))
            .collect();
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
        let row_height =
            ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y + 8.0;
        let scroll_id = ui.id().with("todo_list_scroll");
        egui::ScrollArea::vertical()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, entries.len(), |ui, range| {
                for (idx, entry) in entries[range].iter().cloned() {
                    let mut entry_done = entry.done;
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut entry_done, "").changed() {
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
                        if !entry.tags.is_empty() {
                            let tags = entry.tags.join(", ");
                            ui.label(egui::RichText::new(format!("#{tags}")).small());
                        }
                        if ui.small_button("Open").clicked() {
                            clicked.get_or_insert(WidgetAction {
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
            });

        clicked
    }
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.to_string())
        .collect()
}

impl Default for TodoWidget {
    fn default() -> Self {
        Self {
            cfg: TodoWidgetConfig::default(),
        }
    }
}

impl Widget for TodoWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let todos = snapshot.todos.as_ref();
        let mut action = self.render_summary(ui, todos);
        ui.separator();
        if action.is_none() {
            action = self.render_list(ui, ctx, todos);
        } else {
            self.render_list(ui, ctx, todos);
        }
        action
    }
}
