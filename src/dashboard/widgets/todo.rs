use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult, edit_typed_settings,
    jump_response_clicked, opaque_dashboard_action_identity, query_suggestions,
    register_jump_response,
};
use crate::actions::Action;
use crate::dashboard::DashboardRefreshRequest;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::todo::{TODO_FILE, TodoEntry, mark_done};
use eframe::egui;
use serde::{Deserialize, Serialize};

const TODO_STATUS_FILTER_IDENTITY: &str = "todo:filter-status";
const TODO_TAG_FILTER_IDENTITY: &str = "todo:filter-tags";

fn register_todo_scoped_response(
    ui: &egui::Ui,
    ctx: &DashboardContext<'_>,
    scope: Option<&crate::gui::JumpModeScope>,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: u64,
) {
    if let Some(scope) = scope {
        let mut scoped_context = ctx.clone();
        scoped_context.jump_mode_scope = Some(scope.clone());
        register_jump_response(
            ui,
            &scoped_context,
            response,
            || identity.as_ref().to_owned(),
            || label.into(),
            kind,
            visual_order,
        );
    }
}

fn todo_entry_identity(entry: &TodoEntry, index: usize, snapshot_version: u64) -> String {
    if entry.id.is_empty() {
        let text = opaque_dashboard_action_identity(&format!("legacy-todo-text/{}", entry.text));
        format!("todo-legacy:{text}:index:{index}:snapshot:{snapshot_version}")
    } else {
        let id = opaque_dashboard_action_identity(&format!("todo-id/{}", entry.id));
        format!("todo-entry:{id}:index:{index}:snapshot:{snapshot_version}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TodoSort {
    #[default]
    Priority,
    Created,
    Alphabetical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TodoStatusFilter {
    All,
    #[default]
    Open,
    Done,
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

#[derive(Default)]
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
                let response = ui.add(egui::DragValue::new(&mut cfg.count).range(1..=50));
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "todo/count",
                    "Todo item limit",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                changed |= response.changed();
                ui.label("todos");
            });
            let response = ui.checkbox(&mut cfg.show_progress, "Show progress bar");
            ctx.register_jump_mode_target(
                ui,
                &response,
                "todo/show-progress",
                "Show progress bar",
                crate::gui::jump_mode::TargetKind::Checkbox,
            );
            changed |= response.changed();
            let status_anchor = "todo/status";
            let status_popup = ctx.combo_popup_scope(status_anchor);
            let status_combo = egui::ComboBox::from_id_salt(ui.id().with("todo-status"))
                .selected_text(match cfg.status {
                    TodoStatusFilter::All => "All",
                    TodoStatusFilter::Open => "Open",
                    TodoStatusFilter::Done => "Done",
                })
                .show_ui(ui, |ui| {
                    for (id, status, label) in [
                        ("all", TodoStatusFilter::All, "All"),
                        ("open", TodoStatusFilter::Open, "Open"),
                        ("done", TodoStatusFilter::Done, "Done"),
                    ] {
                        let response = ui.selectable_value(&mut cfg.status, status, label);
                        ctx.register_jump_mode_target_in_scope(
                            status_popup.as_ref(),
                            ui,
                            &response,
                            format!("todo/status/{id}"),
                            "Todo status option",
                            crate::gui::jump_mode::TargetKind::Selectable,
                        );
                        changed |= response.changed();
                    }
                });
            ctx.register_jump_mode_target(
                ui,
                &status_combo.response,
                status_anchor,
                "Todo status",
                crate::gui::jump_mode::TargetKind::ComboBox,
            );
            ui.horizontal(|ui| {
                ui.label("Min priority");
                let response = ui.add(egui::DragValue::new(&mut cfg.min_priority).range(0..=255));
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "todo/min-priority",
                    "Minimum priority",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                changed |= response.changed();
            });
            let sort_anchor = "todo/sort";
            let sort_popup = ctx.combo_popup_scope(sort_anchor);
            let sort_combo = egui::ComboBox::from_id_salt(ui.id().with("todo-sort"))
                .selected_text(match cfg.sort {
                    TodoSort::Priority => "Priority",
                    TodoSort::Created => "Created order",
                    TodoSort::Alphabetical => "Alphabetical",
                })
                .show_ui(ui, |ui| {
                    for (id, sort, label) in [
                        ("priority", TodoSort::Priority, "Priority"),
                        ("created", TodoSort::Created, "Created order"),
                        ("alphabetical", TodoSort::Alphabetical, "Alphabetical"),
                    ] {
                        let response = ui.selectable_value(&mut cfg.sort, sort, label);
                        ctx.register_jump_mode_target_in_scope(
                            sort_popup.as_ref(),
                            ui,
                            &response,
                            format!("todo/sort/{id}"),
                            "Todo sort option",
                            crate::gui::jump_mode::TargetKind::Selectable,
                        );
                        changed |= response.changed();
                    }
                });
            ctx.register_jump_mode_target(
                ui,
                &sort_combo.response,
                sort_anchor,
                "Todo sort order",
                crate::gui::jump_mode::TargetKind::ComboBox,
            );

            ui.separator();
            ui.heading("Open action");
            let suggestions = query_suggestions(ctx, &["todo"], &["todo", "todo list", "todo add"]);
            if cfg.query.is_none()
                && let Some(s) = suggestions.first()
            {
                cfg.query = Some(s.clone());
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.label("Query override");
                let mut query = cfg.query.clone().unwrap_or_default();
                let response = ui.text_edit_singleline(&mut query);
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "todo/query-override",
                    "Todo query override",
                    crate::gui::jump_mode::TargetKind::TextEntry,
                );
                if response.changed() {
                    cfg.query = if query.trim().is_empty() {
                        None
                    } else {
                        Some(query)
                    };
                    changed = true;
                }
            });
            if !suggestions.is_empty() {
                let anchor = "todo/query-suggestions";
                let popup_scope = ctx.combo_popup_scope(anchor);
                let combo = egui::ComboBox::from_id_salt(ui.id().with("todo-query-suggestions"))
                    .selected_text(
                        cfg.query
                            .as_deref()
                            .unwrap_or("Pick a todo query from your plugins"),
                    )
                    .show_ui(ui, |ui| {
                        for (index, suggestion) in suggestions.iter().enumerate() {
                            let response = ui.selectable_value(
                                &mut cfg.query,
                                Some(suggestion.clone()),
                                suggestion,
                            );
                            let option = crate::dashboard::widgets::opaque_settings_key(suggestion);
                            ctx.register_jump_mode_target_in_scope(
                                popup_scope.as_ref(),
                                ui,
                                &response,
                                format!("todo/query-suggestion/{option}/{index}"),
                                "Todo query suggestion",
                                crate::gui::jump_mode::TargetKind::Selectable,
                            );
                            changed |= response.changed();
                        }
                    });
                ctx.register_jump_mode_target(
                    ui,
                    &combo.response,
                    anchor,
                    "Todo query suggestions",
                    crate::gui::jump_mode::TargetKind::ComboBox,
                );
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

    fn render_summary(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        todos: &[TodoEntry],
    ) -> Option<WidgetAction> {
        let normalized_filter_tags = self.normalized_filter_tags();
        let filtered: Vec<&TodoEntry> = todos
            .iter()
            .filter(|t| self.priority_match(t) && self.tags_match(t, &normalized_filter_tags))
            .collect();
        let done = filtered.iter().filter(|t| t.done).count();
        let total = filtered.len();
        let remaining = total.saturating_sub(done);
        let show_progress = self.cfg.show_progress;
        let mut action = None;
        let mut discarded_cfg = None;
        let cfg = if ctx.jump_mode_activation_allowed {
            &mut self.cfg
        } else {
            // Keep controls interactive for response collection/native focus,
            // while discarding edits from a pass that cannot commit them.
            discarded_cfg.insert(self.cfg.clone())
        };
        let status_popup_scope = ctx
            .jump_mode_scope
            .as_ref()
            .map(|scope| scope.combo_popup_scope(TODO_STATUS_FILTER_IDENTITY));
        let tags_popup_scope = ctx
            .jump_mode_scope
            .as_ref()
            .map(|scope| scope.combo_popup_scope(TODO_TAG_FILTER_IDENTITY));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let status_combo = egui::ComboBox::from_id_salt(ui.id().with("todo_filter_status"))
                    .selected_text(match cfg.status {
                        TodoStatusFilter::All => "All",
                        TodoStatusFilter::Open => "Open",
                        TodoStatusFilter::Done => "Done",
                    })
                    .show_ui(ui, |ui| {
                        for (order, (status, identity, label)) in [
                            (TodoStatusFilter::All, "all", "All"),
                            (TodoStatusFilter::Open, "open", "Open"),
                            (TodoStatusFilter::Done, "done", "Done"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let response = ui.selectable_value(&mut cfg.status, status, label);
                            register_todo_scoped_response(
                                ui,
                                ctx,
                                status_popup_scope.as_ref(),
                                &response,
                                format!("option/{identity}"),
                                "Todo status option",
                                crate::gui::jump_mode::TargetKind::Selectable,
                                order as u64,
                            );
                        }
                    });
                register_jump_response(
                    ui,
                    ctx,
                    &status_combo.response,
                    || TODO_STATUS_FILTER_IDENTITY.to_owned(),
                    || "Todo status filter".to_owned(),
                    crate::gui::jump_mode::TargetKind::ComboBox,
                    0,
                );
                let priority_response = ui.add(
                    egui::DragValue::new(&mut cfg.min_priority)
                        .range(0..=255)
                        .prefix("p≥"),
                );
                register_jump_response(
                    ui,
                    ctx,
                    &priority_response,
                    || "todo:filter-min-priority".to_owned(),
                    || "Todo minimum priority".to_owned(),
                    crate::gui::jump_mode::TargetKind::Slider,
                    1,
                );
                let available_tags = Self::available_tags(todos);
                let selected = if cfg.filter_tags.is_empty() {
                    "Tag: any".to_string()
                } else {
                    format!("Tags: {}", cfg.filter_tags.join(","))
                };
                let tags_combo = egui::ComboBox::from_id_salt(ui.id().with("todo_filter_tags"))
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        let clear_response = ui.button("Clear tags");
                        register_todo_scoped_response(
                            ui,
                            ctx,
                            tags_popup_scope.as_ref(),
                            &clear_response,
                            "clear",
                            "Clear todo tag filter",
                            crate::gui::jump_mode::TargetKind::Button,
                            0,
                        );
                        if clear_response.clicked() && ctx.jump_mode_activation_allowed {
                            cfg.filter_tags.clear();
                        }
                        for (order, tag) in available_tags.into_iter().enumerate() {
                            let mut enabled = cfg
                                .filter_tags
                                .iter()
                                .any(|selected| selected.eq_ignore_ascii_case(&tag));
                            let response = ui.checkbox(&mut enabled, &tag);
                            if let Some(scope) = tags_popup_scope.as_ref() {
                                let tag_identity =
                                    opaque_dashboard_action_identity(&format!("todo-tag/{}", tag));
                                register_todo_scoped_response(
                                    ui,
                                    ctx,
                                    Some(scope),
                                    &response,
                                    format!("option/{tag_identity}"),
                                    "Todo tag option",
                                    crate::gui::jump_mode::TargetKind::Checkbox,
                                    order as u64 + 1,
                                );
                            }
                            if response.changed() {
                                if enabled {
                                    cfg.filter_tags.push(tag.clone());
                                } else {
                                    cfg.filter_tags
                                        .retain(|selected| !selected.eq_ignore_ascii_case(&tag));
                                }
                                cfg.filter_tags.sort_by_key(|value| value.to_lowercase());
                                cfg.filter_tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                            }
                        }
                    });
                register_jump_response(
                    ui,
                    ctx,
                    &tags_combo.response,
                    || TODO_TAG_FILTER_IDENTITY.to_owned(),
                    || "Todo tag filter".to_owned(),
                    crate::gui::jump_mode::TargetKind::ComboBox,
                    2,
                );
            });
            ui.horizontal(|ui| {
                ui.label(format!("Todos: {done}/{total} done"));
                let response = ui.button("Open todos");
                if jump_response_clicked(
                    ui,
                    ctx,
                    &response,
                    || "todos:open".into(),
                    || "Open todos".into(),
                    0,
                ) {
                    action = Some(WidgetAction {
                        action: Action {
                            label: "Todos".into(),
                            desc: "Todo".into(),
                            action: "todo:dialog".into(),
                            args: None,
                        },
                        query_override: cfg.query.clone().or_else(|| Some("todo".into())),
                    });
                }
            });
            if show_progress {
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
            .id_salt(scroll_id)
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, entries.len(), |ui, range| {
                let min_row_width = ui.available_width().max(400.0);
                for (idx, entry) in entries[range].iter().cloned() {
                    let mut entry_done = entry.done;
                    let native_row_id = if entry.id.is_empty() {
                        ui.id()
                            .with(("todo-entry-row-legacy", entry.text.as_str(), idx))
                    } else {
                        ui.id().with(("todo-entry-row", entry.id.as_str()))
                    };
                    ui.scope_builder(egui::UiBuilder::new().id(native_row_id), |ui| {
                        ui.horizontal(|ui| {
                            ui.set_min_width(min_row_width);
                            let done_response = ui.checkbox(&mut entry_done, "");
                            register_jump_response(
                                ui,
                                ctx,
                                &done_response,
                                || {
                                    format!(
                                        "{}/done",
                                        todo_entry_identity(&entry, idx, ctx.todo_version)
                                    )
                                },
                                || {
                                    if entry.done {
                                        "Mark todo open".to_owned()
                                    } else {
                                        "Mark todo done".to_owned()
                                    }
                                },
                                crate::gui::jump_mode::TargetKind::Checkbox,
                                idx as u64,
                            );
                            if done_response.changed() && ctx.jump_mode_activation_allowed {
                                if let Err(err) = mark_done(TODO_FILE, idx) {
                                    tracing::error!("Failed to toggle todo #{idx}: {err}");
                                } else {
                                    ctx.data_cache
                                        .request_refresh(DashboardRefreshRequest::Todos);
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
                            ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
                            if !entry.tags.is_empty() {
                                let tags = entry.tags.join(", ");
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("#{tags}")).small(),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                            }
                            let response = ui.small_button("Open");
                            if jump_response_clicked(
                                ui,
                                ctx,
                                &response,
                                || {
                                    format!(
                                        "{}/open",
                                        todo_entry_identity(&entry, idx, ctx.todo_version)
                                    )
                                },
                                || "Open".into(),
                                idx as u64,
                            ) {
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

impl Widget for TodoWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        let snapshot = ctx.data_cache.snapshot();
        let todos = snapshot.todos.as_ref();
        let mut action = self.render_summary(ui, ctx, todos);
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
    use super::*;
    use crate::actions::ActionIdLookup;
    use crate::dashboard::data_cache::{DashboardDataCache, DashboardDataSnapshot};
    use crate::gui::jump_mode::ui::JumpModeUi;
    use crate::gui::jump_mode::{SurfaceId, TargetGeneration};
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn dashboard_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 650.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn dashboard_scope(jump: &JumpModeUi) -> crate::gui::JumpModeScope {
        jump.scope(
            SurfaceId::Dashboard,
            TargetGeneration(7),
            "dashboard-test",
            egui::ViewportId::ROOT,
            true,
        )
        .expect("enabled Jump Mode creates a dashboard scope")
        .child_namespace("slot-id:todo")
        .child_namespace("body")
    }

    fn dashboard_context<'a>(
        plugins: &'a crate::plugin::PluginManager,
        data_cache: &'a DashboardDataCache,
        todo_version: u64,
        jump_mode_scope: Option<crate::gui::JumpModeScope>,
        jump_mode_activation_allowed: bool,
    ) -> DashboardContext<'a> {
        static EMPTY_USAGE: Lazy<HashMap<String, u32>> = Lazy::new(HashMap::new);
        static EMPTY_LOOKUP: Lazy<ActionIdLookup> = Lazy::new(ActionIdLookup::default);
        DashboardContext {
            actions: &[],
            actions_by_id: &EMPTY_LOOKUP,
            usage: &EMPTY_USAGE,
            plugins,
            enabled_plugins: None,
            default_location: None,
            data_cache,
            actions_version: 0,
            fav_version: 0,
            notes_version: 0,
            todo_version,
            calendar_version: 0,
            clipboard_version: 0,
            snippets_version: 0,
            dashboard_visible: true,
            dashboard_focused: true,
            reduce_dashboard_work_when_unfocused: false,
            diagnostics: None,
            show_diagnostics_widget: false,
            jump_mode_scope,
            jump_mode_activation_allowed,
        }
    }

    fn render_with_input(
        widget: &mut TodoWidget,
        ctx: &DashboardContext<'_>,
        egui_ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Option<WidgetAction> {
        let mut action = None;
        egui_ctx
            .run_ui(dashboard_input(events), |ui| {
                ui.push_id("slot-id:todo", |ui| {
                    ui.push_id("body", |ui| {
                        action = widget.render(ui, ctx, WidgetActivation::Click);
                    });
                });
            })
            .drop_without_applying_deltas();
        action
    }

    fn fixture_todo(id: &str) -> TodoEntry {
        TodoEntry {
            id: id.into(),
            text: "private todo body Ω".into(),
            done: false,
            priority: 5,
            tags: vec!["private todo tag".into()],
            entity_refs: Vec::new(),
        }
    }

    fn sample_entries() -> Vec<TodoEntry> {
        vec![
            TodoEntry {
                id: String::new(),
                text: "alpha".into(),
                done: false,
                priority: 1,
                tags: vec!["work".into()],
                entity_refs: Vec::new(),
            },
            TodoEntry {
                id: String::new(),
                text: "beta".into(),
                done: true,
                priority: 4,
                tags: vec!["home".into(), "urgent".into()],
                entity_refs: Vec::new(),
            },
            TodoEntry {
                id: String::new(),
                text: "gamma".into(),
                done: false,
                priority: 5,
                tags: vec!["urgent".into()],
                entity_refs: Vec::new(),
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

    #[test]
    fn jump_mode_dashboard_simple_controls_todo_filters_and_done_action() {
        use crate::gui::jump_mode::TargetKind;

        let workspace = crate::performance::workloads::IsolatedWorkspace::new();
        let todo = fixture_todo("fixture-todo-id");
        std::fs::write(
            workspace.root().join(crate::plugins::todo::TODO_FILE),
            serde_json::to_vec(std::slice::from_ref(&todo)).unwrap(),
        )
        .unwrap();

        let data_cache = DashboardDataCache::new();
        let mut snapshot = DashboardDataSnapshot::default();
        snapshot.todos = Arc::new(vec![todo.clone()]);
        data_cache.set_snapshot_for_test(snapshot);
        let plugins = crate::plugin::PluginManager::new_inert_for_test();
        let mut widget = TodoWidget::new(TodoWidgetConfig::default());
        let todo_version = crate::plugins::todo::todo_version();
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(
            &plugins,
            &data_cache,
            todo_version,
            Some(scope.clone()),
            true,
        );
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let registry = jump.pass_registry().unwrap();
        let expected_labels = [
            "Todo status filter",
            "Todo minimum priority",
            "Todo tag filter",
            "Mark todo done",
            "Open",
        ];
        for label in expected_labels {
            assert!(
                registry
                    .metadata()
                    .any(|metadata| metadata.label() == label),
                "missing real todo target {label:?}"
            );
        }
        let done_identity = format!("{}/done", todo_entry_identity(&todo, 0, todo_version));
        let done_target = scope.target_id(done_identity);
        let done_response = registry.get(&done_target).unwrap();
        assert_eq!(done_response.metadata().kind(), TargetKind::Checkbox);
        let combo_targets = registry
            .targets()
            .iter()
            .filter(|target| target.metadata().kind() == TargetKind::ComboBox)
            .count();
        assert_eq!(
            combo_targets, 2,
            "status and tags use actual ComboBox anchors"
        );
        assert!(registry.metadata().all(|metadata| {
            matches!(
                metadata.label(),
                "Todo status filter"
                    | "Todo minimum priority"
                    | "Todo tag filter"
                    | "Open todos"
                    | "Mark todo done"
                    | "Open"
            )
        }));
        for target in registry.targets() {
            let semantic = format!("{:?}", target.id().semantic());
            assert!(!semantic.contains("private todo body"));
            assert!(!semantic.contains("private todo tag"));
        }
        let original =
            std::fs::read(workspace.root().join(crate::plugins::todo::TODO_FILE)).unwrap();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            done_target.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, todo_version, Some(scope), true);
        assert!(render_with_input(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let focused = jump.pass_registry().unwrap().get(&done_target).unwrap();
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(focused.response_id())
        );
        assert_eq!(
            std::fs::read(workspace.root().join(crate::plugins::todo::TODO_FILE)).unwrap(),
            original
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = dashboard_scope(&jump);
        let context = dashboard_context(&plugins, &data_cache, todo_version, Some(scope), true);
        assert!(
            render_with_input(
                &mut widget,
                &context,
                &egui_ctx,
                vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: Some(egui::Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            )
            .is_none()
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);
        let persisted: Vec<TodoEntry> = serde_json::from_slice(
            &std::fs::read(workspace.root().join(crate::plugins::todo::TODO_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.len(), 1);
        assert!(
            persisted[0].done,
            "a distinct native Space toggles exactly the focused row"
        );
        assert_eq!(persisted[0].text, todo.text);
        assert_eq!(persisted[0].tags, todo.tags);
    }
}
