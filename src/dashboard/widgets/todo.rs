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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatusFilter {
    All,
    Open,
    Done,
}

impl Default for TodoStatusFilter {
    fn default() -> Self {
        TodoStatusFilter::Open
    }
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
    #[serde(default = "default_show_progress")]
    pub show_progress: bool,
    #[serde(default)]
    pub status: TodoStatusFilter,
    #[serde(default)]
    pub min_priority: u8,
    #[serde(default)]
    pub filter_tags: Vec<String>,
    #[serde(default)]
    pub show_done: bool,
    #[serde(default)]
    pub sort: TodoSort,
    #[serde(default)]
    pub query: Option<String>,
}

impl Default for TodoWidgetConfig {
    fn default() -> Self {
        Self {
            count: default_count(),
            show_progress: default_show_progress(),
            status: TodoStatusFilter::default(),
            min_priority: 0,
            filter_tags: Vec::new(),
            show_done: false,
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
        Self {
            cfg: migrate_config(cfg),
        }
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
                .checkbox(&mut cfg.show_progress, "Show progress bar")
                .changed();
            egui::ComboBox::from_label("Status")
                .selected_text(match cfg.status {
                    TodoStatusFilter::All => "All",
                    TodoStatusFilter::Open => "Open",
                    TodoStatusFilter::Done => "Done",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoStatusFilter::All, "All")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoStatusFilter::Open, "Open")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoStatusFilter::Done, "Done")
                        .changed();
                });
            ui.horizontal(|ui| {
                ui.label("Min priority");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.min_priority).clamp_range(0..=255))
                    .changed();
            });
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

    fn normalized_filter_tags(&self) -> Vec<String> {
        self.cfg
            .filter_tags
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect()
    }

    fn tags_match(&self, entry: &TodoEntry, normalized_filter_tags: &[String]) -> bool {
        if normalized_filter_tags.is_empty() {
            return true;
        }
        normalized_filter_tags.iter().any(|filter| {
            entry
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(filter) || tag.to_lowercase().contains(filter))
        })
    }

    fn status_match(&self, entry: &TodoEntry) -> bool {
        match self.cfg.status {
            TodoStatusFilter::All => true,
            TodoStatusFilter::Open => !entry.done,
            TodoStatusFilter::Done => entry.done,
        }
    }

    fn priority_match(&self, entry: &TodoEntry) -> bool {
        entry.priority >= self.cfg.min_priority
    }

    fn entry_matches_filters(&self, entry: &TodoEntry, normalized_filter_tags: &[String]) -> bool {
        self.status_match(entry)
            && self.priority_match(entry)
            && self.tags_match(entry, normalized_filter_tags)
    }

    fn filter_entries(
        &self,
        todos: &[TodoEntry],
        normalized_filter_tags: &[String],
    ) -> Vec<(usize, TodoEntry)> {
        todos
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, t)| self.entry_matches_filters(t, normalized_filter_tags))
            .collect()
    }

    fn available_tags(todos: &[TodoEntry]) -> Vec<String> {
        let mut tags: Vec<String> = todos
            .iter()
            .flat_map(|todo| todo.tags.iter())
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect();
        tags.sort_by_key(|tag| tag.to_lowercase());
        tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        tags
    }

    fn sort_entries(entries: &mut Vec<(usize, TodoEntry)>, sort: TodoSort) {
        match sort {
            TodoSort::Priority => {
                entries.sort_by(|a, b| b.1.priority.cmp(&a.1.priority).then_with(|| a.0.cmp(&b.0)))
            }
            TodoSort::Created => entries.sort_by_key(|(idx, _)| *idx),
            TodoSort::Alphabetical => {
                let mut keyed_entries: Vec<(String, usize, TodoEntry)> = entries
                    .drain(..)
                    .map(|(idx, entry)| (entry.text.to_lowercase(), idx, entry))
                    .collect();
                keyed_entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                entries.extend(
                    keyed_entries
                        .into_iter()
                        .map(|(_, idx, entry)| (idx, entry)),
                );
            }
        }
    }

    fn render_summary(&mut self, ui: &mut egui::Ui, todos: &[TodoEntry]) -> Option<WidgetAction> {
        let normalized_filter_tags = self.normalized_filter_tags();
        let filtered: Vec<&TodoEntry> = todos
            .iter()
            .filter(|t| self.priority_match(t) && self.tags_match(t, &normalized_filter_tags))
            .collect();
        let done = filtered.iter().filter(|t| t.done).count();
        let total = filtered.len();
        let remaining = total.saturating_sub(done);
        let mut action = None;
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source(ui.id().with("todo_filter_status"))
                    .selected_text(match self.cfg.status {
                        TodoStatusFilter::All => "All",
                        TodoStatusFilter::Open => "Open",
                        TodoStatusFilter::Done => "Done",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.cfg.status, TodoStatusFilter::All, "All");
                        ui.selectable_value(&mut self.cfg.status, TodoStatusFilter::Open, "Open");
                        ui.selectable_value(&mut self.cfg.status, TodoStatusFilter::Done, "Done");
                    });
                ui.add(
                    egui::DragValue::new(&mut self.cfg.min_priority)
                        .clamp_range(0..=255)
                        .prefix("p≥"),
                );
                let available_tags = Self::available_tags(todos);
                let selected = if self.cfg.filter_tags.is_empty() {
                    "Tag: any".to_string()
                } else {
                    format!("Tags: {}", self.cfg.filter_tags.join(","))
                };
                egui::ComboBox::from_id_source(ui.id().with("todo_filter_tags"))
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        if ui.button("Clear tags").clicked() {
                            self.cfg.filter_tags.clear();
                        }
                        for tag in available_tags {
                            let mut enabled = self
                                .cfg
                                .filter_tags
                                .iter()
                                .any(|selected| selected.eq_ignore_ascii_case(&tag));
                            if ui.checkbox(&mut enabled, &tag).changed() {
                                if enabled {
                                    self.cfg.filter_tags.push(tag.clone());
                                } else {
                                    self.cfg
                                        .filter_tags
                                        .retain(|selected| !selected.eq_ignore_ascii_case(&tag));
                                }
                                self.cfg
                                    .filter_tags
                                    .sort_by_key(|value| value.to_lowercase());
                                self.cfg
                                    .filter_tags
                                    .dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                            }
                        }
                    });
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
        let normalized_filter_tags = self.normalized_filter_tags();
        let mut entries = self.filter_entries(todos, &normalized_filter_tags);
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
        egui::ScrollArea::both()
            .id_source(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, entries.len(), |ui, range| {
                let min_row_width = ui.available_width().max(400.0);
                for (idx, entry) in entries[range].iter().cloned() {
                    let mut entry_done = entry.done;
                    ui.horizontal(|ui| {
                        ui.set_min_width(min_row_width);
                        if ui.checkbox(&mut entry_done, "").changed() {
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
                        ui.add(egui::Label::new(text).wrap(false));
                        if !entry.tags.is_empty() {
                            let tags = entry.tags.join(", ");
                            ui.add(
                                egui::Label::new(egui::RichText::new(format!("#{tags}")).small())
                                    .wrap(false),
                            );
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

fn migrate_config(mut cfg: TodoWidgetConfig) -> TodoWidgetConfig {
    if cfg.show_done && cfg.status == TodoStatusFilter::Open {
        cfg.status = TodoStatusFilter::All;
    }
    cfg.filter_tags = parse_tags(&cfg.filter_tags.join(","));
    cfg
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

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<TodoWidgetConfig>(settings.clone()) {
            self.cfg = migrate_config(cfg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TodoSort, TodoStatusFilter, TodoWidget, TodoWidgetConfig};
    use crate::plugins::todo::TodoEntry;

    fn sample_entries() -> Vec<TodoEntry> {
        vec![
            TodoEntry {
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec!["work".into()],
            },
            TodoEntry {
                text: "beta".into(),
                done: true,
                priority: 4,
                tags: vec!["home".into(), "urgent".into()],
            },
            TodoEntry {
                text: "gamma".into(),
                done: false,
                priority: 5,
                tags: vec!["urgent".into()],
            },
        ]
    }

    #[test]
    fn filters_combine_before_sorting() {
        let mut cfg = TodoWidgetConfig::default();
        cfg.status = TodoStatusFilter::Open;
        cfg.min_priority = 3;
        cfg.filter_tags = vec!["urgent".into()];
        cfg.sort = TodoSort::Priority;
        let widget = TodoWidget::new(cfg);

        let normalized_filter_tags = widget.normalized_filter_tags();
        let mut filtered = widget.filter_entries(&sample_entries(), &normalized_filter_tags);
        TodoWidget::sort_entries(&mut filtered, TodoSort::Priority);
        let texts: Vec<String> = filtered.into_iter().map(|(_, entry)| entry.text).collect();
        assert_eq!(texts, vec!["gamma"]);
    }

    #[test]
    fn config_serialization_persists_filter_state() {
        let cfg = TodoWidgetConfig {
            count: 7,
            show_progress: false,
            status: TodoStatusFilter::Done,
            min_priority: 2,
            filter_tags: vec!["urgent".into(), "home".into()],
            show_done: false,
            sort: TodoSort::Alphabetical,
            query: Some("todo list".into()),
        };

        let json = serde_json::to_value(&cfg).expect("serialize todo config");
        let restored: TodoWidgetConfig =
            serde_json::from_value(json).expect("deserialize todo config");
        assert_eq!(restored.status, TodoStatusFilter::Done);
        assert_eq!(restored.min_priority, 2);
        assert_eq!(restored.filter_tags, vec!["urgent", "home"]);
    }
}
