use super::{
    RefreshMode, TimedCache, Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    dashboard_action_row_egui_id, default_refresh_throttle_secs, edit_typed_settings,
    jump_action_identity, jump_response_clicked, observe_owned_search_publication, plugin_names,
    query_suggestions, refresh_schedule, refresh_settings_ui, run_refresh_schedule,
};
use crate::actions::Action;
use crate::common::query::{apply_action_filters, split_action_filters};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::time::Duration;

fn default_engine() -> String {
    "omni_search".into()
}

fn default_query() -> String {
    "o list".into()
}

fn default_limit() -> usize {
    6
}

fn default_refresh_interval() -> f32 {
    45.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ClickBehavior {
    #[default]
    RunAction,
    FillQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedQueryResultsConfig {
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default = "default_query")]
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: f32,
    #[serde(default)]
    pub refresh_mode: RefreshMode,
    #[serde(default = "default_refresh_throttle_secs")]
    pub refresh_throttle_secs: f32,
    #[serde(default)]
    pub manual_refresh_only: bool,
    #[serde(default)]
    pub click_behavior: ClickBehavior,
}

impl Default for PinnedQueryResultsConfig {
    fn default() -> Self {
        Self {
            engine: default_engine(),
            query: default_query(),
            limit: default_limit(),
            refresh_interval_secs: default_refresh_interval(),
            refresh_mode: RefreshMode::Auto,
            refresh_throttle_secs: default_refresh_throttle_secs(),
            manual_refresh_only: false,
            click_behavior: ClickBehavior::default(),
        }
    }
}

pub struct PinnedQueryResultsWidget {
    cfg: PinnedQueryResultsConfig,
    cache: TimedCache<Vec<Action>>,
    error: Option<String>,
    refresh_pending: bool,
    last_search_generation: u64,
    awaiting_ticket: Option<u64>,
}

impl PinnedQueryResultsWidget {
    pub fn new(cfg: PinnedQueryResultsConfig) -> Self {
        let interval = Duration::from_secs_f32(cfg.refresh_interval_secs.max(1.0));
        Self {
            cfg,
            cache: TimedCache::new(Vec::new(), interval),
            error: None,
            refresh_pending: false,
            last_search_generation: 0,
            awaiting_ticket: None,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(
            ui,
            value,
            ctx,
            |ui, cfg: &mut PinnedQueryResultsConfig, ctx| {
                let mut changed = false;

                let all_engines = if let Some(infos) = ctx.plugin_infos {
                    infos.iter().map(|(name, _, _)| name.clone()).collect()
                } else if let Some(manager) = ctx.plugins {
                    manager.plugin_names()
                } else {
                    Vec::new()
                };
                let engines = plugin_names(ctx);
                let original_engine = cfg.engine.trim().to_string();
                let mut warning = None;

                if original_engine.is_empty() {
                    if let Some(first) = engines.first() {
                        cfg.engine = first.clone();
                    } else {
                        cfg.engine = default_engine();
                    }
                    changed = true;
                } else if let Some(enabled) = ctx.enabled_plugins {
                    if !enabled.contains(original_engine.as_str()) {
                        if let Some(first) = engines.first() {
                            if first != &original_engine {
                                cfg.engine = first.clone();
                                changed = true;
                                warning = Some(format!(
                                    "Engine '{original_engine}' is disabled in plugin settings. Using '{first}'.",
                                ));
                            } else {
                                warning = Some(format!(
                                    "Engine '{original_engine}' is disabled in plugin settings.",
                                ));
                            }
                        } else {
                            warning = Some(format!(
                                "Engine '{original_engine}' is disabled in plugin settings.",
                            ));
                        }
                    }
                } else if !all_engines.iter().any(|name| name == &original_engine) {
                    if let Some(first) = engines.first() {
                        cfg.engine = first.clone();
                        changed = true;
                        warning = Some(format!(
                            "Engine '{original_engine}' is not available. Using '{first}'.",
                        ));
                    } else {
                        warning = Some(format!("Engine '{original_engine}' is not available."));
                    }
                }

                if engines.is_empty() {
                    ui.colored_label(egui::Color32::YELLOW, "No enabled engines available.");
                } else {
                    egui::ComboBox::from_label("Engine")
                        .selected_text(&cfg.engine)
                        .show_ui(ui, |ui| {
                            for engine in &engines {
                                changed |= ui
                                    .selectable_value(&mut cfg.engine, engine.clone(), engine)
                                    .changed();
                            }
                        });
                }

                if let Some(warn) = warning {
                    ui.colored_label(egui::Color32::YELLOW, warn);
                }

                let prefix_candidates = Self::suggestion_prefixes(&cfg.engine);
                let prefix_refs: Vec<&str> = prefix_candidates.iter().map(|s| s.as_str()).collect();
                let default_suggestions = Self::default_queries(&cfg.engine);
                let default_refs: Vec<&str> =
                    default_suggestions.iter().map(|s| s.as_str()).collect();
                let suggestions = query_suggestions(ctx, &prefix_refs, &default_refs);

                ui.horizontal(|ui| {
                    ui.label("Query");
                    changed |= ui.text_edit_singleline(&mut cfg.query).changed();
                });

                if !suggestions.is_empty() {
                    egui::ComboBox::from_label("Suggestions")
                        .selected_text(if cfg.query.trim().is_empty() {
                            "Pick a query"
                        } else {
                            &cfg.query
                        })
                        .show_ui(ui, |ui| {
                            for suggestion in &suggestions {
                                changed |= ui
                                    .selectable_value(
                                        &mut cfg.query,
                                        suggestion.clone(),
                                        suggestion,
                                    )
                                    .changed();
                            }
                        });
                } else if cfg.query.trim().is_empty() {
                    cfg.query = default_query();
                    changed = true;
                }

                ui.horizontal(|ui| {
                    ui.label("Limit");
                    let resp = ui
                        .add(egui::DragValue::new(&mut cfg.limit).range(1..=30))
                        .on_hover_text("Maximum number of actions to pin from the query results.");
                    changed |= resp.changed();
                });

                changed |= refresh_settings_ui(
                    ui,
                    &mut cfg.refresh_interval_secs,
                    &mut cfg.refresh_mode,
                    &mut cfg.refresh_throttle_secs,
                    Some(&mut cfg.manual_refresh_only),
                    "Query results are cached. The widget refreshes after this many seconds unless you click Refresh.",
                );

                egui::ComboBox::from_label("Click behavior")
                    .selected_text(match cfg.click_behavior {
                        ClickBehavior::RunAction => "Run action",
                        ClickBehavior::FillQuery => "Fill search box",
                    })
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(
                                &mut cfg.click_behavior,
                                ClickBehavior::RunAction,
                                "Run action",
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut cfg.click_behavior,
                                ClickBehavior::FillQuery,
                                "Fill search box",
                            )
                            .changed();
                    });

                changed
            },
        )
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs_f32(self.cfg.refresh_interval_secs.max(1.0))
    }

    fn update_interval(&mut self) {
        self.cache.set_interval(self.refresh_interval());
    }

    fn refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let (actions, error, ticket) = self.run_query(ctx);
        self.error = error;
        self.cache.refresh(|data| *data = actions);
        self.awaiting_ticket = ticket;
    }

    fn maybe_refresh(&mut self, ctx: &DashboardContext<'_>) {
        self.update_interval();
        let generation = ctx.plugins.search_generation_for(self.cfg.engine.trim());
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        let request_resolved = self.awaiting_ticket.is_some_and(|ticket| {
            ctx.plugins
                .search_ticket_resolved(self.cfg.engine.trim(), ticket)
        });
        observe_owned_search_publication(
            schedule.mode,
            generation,
            &mut self.last_search_generation,
            request_resolved,
            &mut self.awaiting_ticket,
            &mut self.refresh_pending,
        );
        if run_refresh_schedule(
            ctx,
            schedule,
            &mut self.refresh_pending,
            &mut self.cache.last_refresh,
        ) {
            self.refresh(ctx);
            if schedule.mode != RefreshMode::Manual {
                self.awaiting_ticket = None;
            }
        }
    }

    fn run_query(&self, ctx: &DashboardContext<'_>) -> (Vec<Action>, Option<String>, Option<u64>) {
        let query = self.cfg.query.trim();
        if query.is_empty() {
            return (
                Vec::new(),
                Some("Set a query in the widget settings.".into()),
                None,
            );
        }

        let engine_name = self.cfg.engine.trim();
        if let Some(enabled) = ctx.enabled_plugins
            && !enabled.contains(engine_name)
        {
            return (
                Vec::new(),
                Some(format!(
                    "Engine '{engine_name}' is disabled in plugin settings."
                )),
                None,
            );
        }

        let (filtered_query, filters) = split_action_filters(query);
        let (mut actions, ticket) = match ctx
            .plugins
            .search_plugin_with_ticket(engine_name, filtered_query.trim())
        {
            Ok(result) => result,
            Err(error) => return (Vec::new(), Some(error.into()), None),
        };
        actions = apply_action_filters(actions, &filters);
        let limit = self.cfg.limit.max(1);
        if actions.len() > limit {
            actions.truncate(limit);
        }
        (actions, None, ticket)
    }

    fn build_click_action(&self, action: &Action) -> WidgetAction {
        match self.cfg.click_behavior {
            ClickBehavior::RunAction => WidgetAction {
                query_override: Some(action.label.clone()),
                action: action.clone(),
            },
            ClickBehavior::FillQuery => WidgetAction {
                query_override: Some(action.label.clone()),
                action: Action {
                    label: action.label.clone(),
                    desc: action.desc.clone(),
                    action: format!("query:{}", action.label),
                    args: None,
                },
            },
        }
    }

    fn suggestion_prefixes(engine: &str) -> Vec<String> {
        if engine.eq_ignore_ascii_case("omni_search") {
            vec!["o".into()]
        } else {
            vec![engine.to_string()]
        }
    }

    fn default_queries(engine: &str) -> Vec<String> {
        if engine.eq_ignore_ascii_case("omni_search") {
            vec!["o list".into(), "o".into()]
        } else if engine.is_empty() {
            vec![default_query()]
        } else {
            vec![format!("{engine} list"), format!("{engine} ")]
        }
    }
}

impl Default for PinnedQueryResultsWidget {
    fn default() -> Self {
        Self::new(PinnedQueryResultsConfig::default())
    }
}

impl Widget for PinnedQueryResultsWidget {
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

        let mut clicked = None;
        ui.vertical(|ui| {
            if self.cache.data.is_empty() {
                ui.label("No pinned results. Adjust the query in settings.");
                return;
            }

            let button_width = ui.available_width();
            for (order, action) in self.cache.data.iter().enumerate() {
                let occurrence = self.cache.data[..order]
                    .iter()
                    .filter(|previous| {
                        previous.action == action.action
                            && previous.args == action.args
                            && previous.label == action.label
                    })
                    .count();
                let native_id = dashboard_action_row_egui_id(
                    ui,
                    action,
                    Some(&self.cfg.query),
                    Some(&action.label),
                    None,
                    occurrence,
                );
                ui.push_id(native_id, |ui| {
                    let response =
                        ui.add_sized([button_width, 28.0], egui::Button::new(&action.label));
                    if jump_response_clicked(
                        ui,
                        ctx,
                        &response,
                        || {
                            jump_action_identity(
                                action,
                                Some(&self.cfg.query),
                                Some(&action.label),
                                None,
                                occurrence,
                            )
                        },
                        || action.label.clone(),
                        order as u64,
                    ) {
                        clicked = Some(self.build_click_action(action));
                    }
                });
            }
        });

        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<PinnedQueryResultsConfig>(settings.clone()) {
            self.cfg = cfg;
            self.update_interval();
            self.cache.invalidate();
            self.refresh_pending = true;
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, ctx: &DashboardContext<'_>) -> Option<WidgetAction> {
        let schedule = refresh_schedule(
            self.refresh_interval(),
            self.cfg.refresh_mode,
            self.cfg.manual_refresh_only,
            self.cfg.refresh_throttle_secs,
        );
        let tooltip = match schedule.mode {
            RefreshMode::Manual => "Manual refresh only.".to_string(),
            RefreshMode::Throttled => {
                format!(
                    "Minimum refresh interval {:.0}s.",
                    schedule.throttle.as_secs_f32()
                )
            }
            RefreshMode::Auto => format!(
                "Cached for {:.0}s. Refresh to update pinned results immediately.",
                self.cfg.refresh_interval_secs
            ),
        };
        let response = ui.small_button("Refresh").on_hover_text(tooltip);
        if jump_response_clicked(
            ui,
            ctx,
            &response,
            || "refresh".into(),
            || "Refresh".into(),
            0,
        ) {
            self.refresh_pending = true;
        }
        None
    }
}

#[cfg(test)]
mod jump_mode_tests {
    use super::*;
    use crate::actions::ActionIdLookup;
    use crate::dashboard::data_cache::DashboardDataCache;
    use crate::gui::jump_mode::ui::JumpModeUi;
    use crate::gui::jump_mode::{SurfaceId, TargetGeneration};
    use once_cell::sync::Lazy;
    use std::collections::HashMap;

    fn input(events: Vec<egui::Event>) -> egui::RawInput {
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

    fn body_scope(jump: &JumpModeUi) -> crate::gui::JumpModeScope {
        jump.scope(
            SurfaceId::Dashboard,
            TargetGeneration(7),
            "dashboard-test",
            egui::ViewportId::ROOT,
            true,
        )
        .expect("enabled Jump Mode creates a dashboard scope")
        .child_namespace("slot-id:pinned-query-results")
        .child_namespace("body")
    }

    fn dashboard_context<'a>(
        plugins: &'a crate::plugin::PluginManager,
        data_cache: &'a DashboardDataCache,
        jump_mode_scope: crate::gui::JumpModeScope,
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
            todo_version: 0,
            calendar_version: 0,
            clipboard_version: 0,
            snippets_version: 0,
            dashboard_visible: true,
            dashboard_focused: true,
            reduce_dashboard_work_when_unfocused: false,
            diagnostics: None,
            show_diagnostics_widget: false,
            jump_mode_scope: Some(jump_mode_scope),
            jump_mode_activation_allowed: true,
        }
    }

    fn enter_event() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn jump_mode_pinned_result_focus_then_enter_preserves_widget_action_payload() {
        let expected_action = Action {
            label: "Safe fixture action".into(),
            desc: "A payload used only to verify routing.".into(),
            action: "fixture:run".into(),
            args: Some("--safe-value 17".into()),
        };
        let mut config = PinnedQueryResultsConfig {
            query: "fixture source query".into(),
            refresh_mode: RefreshMode::Manual,
            ..Default::default()
        };
        config.click_behavior = ClickBehavior::RunAction;
        let mut widget = PinnedQueryResultsWidget::new(config);
        widget.cache.data = vec![expected_action.clone()];

        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        let initial_context = dashboard_context(&plugins, &data_cache, body_scope(&jump));
        let mut initial_action = None;
        egui_ctx
            .run_ui(input(Vec::new()), |ui| {
                initial_action = widget.render(ui, &initial_context, WidgetActivation::Click);
            })
            .drop_without_applying_deltas();
        assert!(initial_action.is_none());
        jump.finish_ui_pass();
        let target_id = jump
            .pass_registry()
            .and_then(|registry| registry.targets().first())
            .expect("the actual pinned result response is registered")
            .id()
            .clone();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            target_id.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();

        let context = dashboard_context(&plugins, &data_cache, body_scope(&jump));
        let mut first_frame_action = None;
        egui_ctx
            .run_ui(input(Vec::new()), |ui| {
                first_frame_action = widget.render(ui, &context, WidgetActivation::Click);
            })
            .drop_without_applying_deltas();
        assert!(first_frame_action.is_none(), "jump focus must not activate");
        jump.finish_ui_pass();
        let registered = jump
            .pass_registry()
            .and_then(|registry| registry.get(&target_id))
            .expect("the actual pinned result response is registered");
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(registered.response_id()),
            "the registered button response receives native focus"
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let context = dashboard_context(&plugins, &data_cache, body_scope(&jump));
        let mut activation_count = 0;
        let mut activated_action = None;
        egui_ctx
            .run_ui(input(vec![enter_event()]), |ui| {
                if let Some(action) = widget.render(ui, &context, WidgetActivation::Click) {
                    activation_count += 1;
                    activated_action = Some(action);
                }
            })
            .drop_without_applying_deltas();

        assert_eq!(
            activation_count, 1,
            "fresh Enter uses the widget route once"
        );
        let activated_action = activated_action.expect("the focused row returns its action");
        assert_eq!(activated_action.action, expected_action);
        assert_eq!(
            activated_action.query_override.as_deref(),
            Some("Safe fixture action")
        );
        assert_eq!(widget.cfg.query, "fixture source query");
    }
}
