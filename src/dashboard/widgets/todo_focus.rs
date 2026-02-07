use super::{
    edit_typed_settings, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{mark_done, TodoEntry, TODO_FILE};
use eframe::egui;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoFocusStatusFilter {
    All,
    Open,
    Done,
}

impl Default for TodoFocusStatusFilter {
    fn default() -> Self {
        TodoFocusStatusFilter::Open
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoFocusConfig {
    #[serde(default)]
    pub status: TodoFocusStatusFilter,
    #[serde(default)]
    pub min_priority: u8,
    #[serde(default)]
    pub filter_tags: Vec<String>,
    #[serde(default)]
    pub show_done: bool,
    #[serde(default)]
    pub query: Option<String>,
}

impl Default for TodoFocusConfig {
    fn default() -> Self {
        Self {
            status: TodoFocusStatusFilter::Open,
            min_priority: 0,
            filter_tags: Vec::new(),
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
        Self {
            cfg: migrate_config(cfg),
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut TodoFocusConfig, _ctx| {
            let mut changed = false;
            egui::ComboBox::from_label("Status")
                .selected_text(match cfg.status {
                    TodoFocusStatusFilter::All => "All",
                    TodoFocusStatusFilter::Open => "Open",
                    TodoFocusStatusFilter::Done => "Done",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoFocusStatusFilter::All, "All")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoFocusStatusFilter::Open, "Open")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut cfg.status, TodoFocusStatusFilter::Done, "Done")
                        .changed();
                });
            ui.horizontal(|ui| {
                ui.label("Min priority");
                changed |= ui
                    .add(egui::DragValue::new(&mut cfg.min_priority).clamp_range(0..=255))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label("Filter tags");
                let mut tags = cfg.filter_tags.join(", ");
                if ui.text_edit_singleline(&mut tags).changed() {
                    cfg.filter_tags = parse_tags(&tags);
                    changed = true;
                }
            });
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
        todos.retain(|(_, t)| self.entry_matches_filters(t));
        todos.sort_by(|a, b| b.1.priority.cmp(&a.1.priority).then_with(|| a.0.cmp(&b.0)));
        todos.into_iter().next()
    }

    fn entry_matches_filters(&self, entry: &TodoEntry) -> bool {
        let status_match = match self.cfg.status {
            TodoFocusStatusFilter::All => true,
            TodoFocusStatusFilter::Open => !entry.done,
            TodoFocusStatusFilter::Done => entry.done,
        };
        let tag_match = if self.cfg.filter_tags.is_empty() {
            true
        } else {
            self.cfg.filter_tags.iter().any(|tag| {
                entry
                    .tags
                    .iter()
                    .any(|entry_tag| entry_tag.eq_ignore_ascii_case(tag))
            })
        };
        status_match && entry.priority >= self.cfg.min_priority && tag_match
    }
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.to_string())
        .collect()
}

fn migrate_config(mut cfg: TodoFocusConfig) -> TodoFocusConfig {
    if cfg.show_done && cfg.status == TodoFocusStatusFilter::Open {
        cfg.status = TodoFocusStatusFilter::All;
    }
    cfg.filter_tags = parse_tags(&cfg.filter_tags.join(","));
    cfg
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
            self.cfg = migrate_config(cfg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TodoFocusConfig, TodoFocusStatusFilter, TodoFocusWidget};
    use crate::plugins::todo::TodoEntry;

    #[test]
    fn focus_filters_by_status_priority_and_tags() {
        let widget = TodoFocusWidget::new(TodoFocusConfig {
            status: TodoFocusStatusFilter::Open,
            min_priority: 3,
            filter_tags: vec!["urgent".into()],
            show_done: false,
            query: None,
        });
        let entries = vec![
            TodoEntry {
                text: "low".into(),
                done: false,
                priority: 1,
                tags: vec!["urgent".into()],
            },
            TodoEntry {
                text: "done".into(),
                done: true,
                priority: 5,
                tags: vec!["urgent".into()],
            },
            TodoEntry {
                text: "focus".into(),
                done: false,
                priority: 4,
                tags: vec!["urgent".into()],
            },
        ];

        let picked = widget.pick_focus(&entries).expect("focus todo exists");
        assert_eq!(picked.1.text, "focus");
    }

    #[test]
    fn focus_config_persists_filter_state() {
        let cfg = TodoFocusConfig {
            status: TodoFocusStatusFilter::Done,
            min_priority: 2,
            filter_tags: vec!["home".into()],
            show_done: false,
            query: Some("todo".into()),
        };
        let value = serde_json::to_value(&cfg).expect("serialize focus config");
        let restored: TodoFocusConfig =
            serde_json::from_value(value).expect("deserialize focus config");
        assert_eq!(restored.status, TodoFocusStatusFilter::Done);
        assert_eq!(restored.min_priority, 2);
        assert_eq!(restored.filter_tags, vec!["home"]);
    }
}
