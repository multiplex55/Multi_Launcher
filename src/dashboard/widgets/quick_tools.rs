use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    dashboard_action_row_egui_id, edit_typed_settings, jump_action_identity, jump_response_clicked,
    next_jump_duplicate_occurrence, opaque_dashboard_action_identity,
};
use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use eframe::egui;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QuickToolEntry {
    pub query: String,
    #[serde(default)]
    pub auto_submit: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum QuickToolEntryRepr {
    Legacy(String),
    Current {
        query: String,
        #[serde(default)]
        auto_submit: bool,
    },
}

impl<'de> Deserialize<'de> for QuickToolEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match QuickToolEntryRepr::deserialize(deserializer)? {
            QuickToolEntryRepr::Legacy(query) => Self::manual(query),
            QuickToolEntryRepr::Current { query, auto_submit } => Self { query, auto_submit },
        })
    }
}

impl QuickToolEntry {
    fn manual(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            auto_submit: false,
        }
    }
}

fn default_queries() -> Vec<QuickToolEntry> {
    ["sys", "net", "info cpu", "vol", "bright"]
        .into_iter()
        .map(QuickToolEntry::manual)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuickToolsConfig {
    #[serde(default = "default_queries")]
    pub queries: Vec<QuickToolEntry>,
}

impl Default for QuickToolsConfig {
    fn default() -> Self {
        Self {
            queries: default_queries(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuickToolRowAction {
    MoveUp(usize),
    MoveDown(usize),
    Remove(usize),
}

fn apply_row_action(cfg: &mut QuickToolsConfig, action: QuickToolRowAction) -> bool {
    match action {
        QuickToolRowAction::MoveUp(idx) if idx > 0 && idx < cfg.queries.len() => {
            cfg.queries.swap(idx, idx - 1);
            true
        }
        QuickToolRowAction::MoveDown(idx)
            if idx < cfg.queries.len() && idx + 1 < cfg.queries.len() =>
        {
            cfg.queries.swap(idx, idx + 1);
            true
        }
        QuickToolRowAction::Remove(idx) if idx < cfg.queries.len() => {
            cfg.queries.remove(idx);
            true
        }
        _ => false,
    }
}

pub struct QuickToolsWidget {
    cfg: QuickToolsConfig,
}

impl QuickToolsWidget {
    pub fn new(cfg: QuickToolsConfig) -> Self {
        Self { cfg }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut QuickToolsConfig, ctx| {
            let mut changed = false;
            let mut row_action = None;
            let query_count = cfg.queries.len();
            for (idx, entry) in cfg.queries.iter_mut().enumerate() {
                let token = ctx
                    .property_row_token("quick-tools", idx)
                    .unwrap_or(idx as u64);
                ui.scope_builder(
                    egui::UiBuilder::new().id(ui.id().with(("quick-tool-row", token))),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("Tool {}", idx + 1));
                            let query = ui.text_edit_singleline(&mut entry.query);
                            ctx.register_jump_mode_target(
                                ui,
                                &query,
                                format!("row/{token}/query"),
                                "Tool query",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                            );
                            changed |= query.changed();
                            let auto_submit = ui.checkbox(&mut entry.auto_submit, "Auto submit");
                            ctx.register_jump_mode_target(
                                ui,
                                &auto_submit,
                                format!("row/{token}/auto-submit"),
                                "Auto submit",
                                crate::gui::jump_mode::TargetKind::Checkbox,
                            );
                            changed |= auto_submit.changed();
                            let move_up = ui
                                .add_enabled(idx > 0, egui::Button::new("↑").small())
                                .on_hover_text("Move up");
                            ctx.register_jump_mode_target(
                                ui,
                                &move_up,
                                format!("row/{token}/move-up"),
                                "Move tool up",
                                crate::gui::jump_mode::TargetKind::Button,
                            );
                            if move_up.clicked() {
                                row_action = Some(QuickToolRowAction::MoveUp(idx));
                            }
                            let move_down = ui
                                .add_enabled(idx + 1 < query_count, egui::Button::new("↓").small())
                                .on_hover_text("Move down");
                            ctx.register_jump_mode_target(
                                ui,
                                &move_down,
                                format!("row/{token}/move-down"),
                                "Move tool down",
                                crate::gui::jump_mode::TargetKind::Button,
                            );
                            if move_down.clicked() {
                                row_action = Some(QuickToolRowAction::MoveDown(idx));
                            }
                            let remove = ui.small_button("✕").on_hover_text("Remove");
                            ctx.register_jump_mode_target(
                                ui,
                                &remove,
                                format!("row/{token}/remove"),
                                "Remove tool",
                                crate::gui::jump_mode::TargetKind::Button,
                            );
                            if remove.clicked() {
                                row_action = Some(QuickToolRowAction::Remove(idx));
                            }
                        })
                    },
                );
            }
            if let Some(action) = row_action {
                match action {
                    QuickToolRowAction::MoveUp(idx) if idx > 0 => {
                        ctx.swap_property_row_tokens("quick-tools", idx, idx - 1)
                    }
                    QuickToolRowAction::MoveDown(idx) if idx + 1 < query_count => {
                        ctx.swap_property_row_tokens("quick-tools", idx, idx + 1)
                    }
                    QuickToolRowAction::Remove(idx) => {
                        ctx.remove_property_row_token("quick-tools", idx)
                    }
                    _ => {}
                }
                changed |= apply_row_action(cfg, action);
            }
            let add = ui.button("Add tool");
            ctx.register_jump_mode_target(
                ui,
                &add,
                "tools/add",
                "Add tool",
                crate::gui::jump_mode::TargetKind::Button,
            );
            if add.clicked() {
                cfg.queries.push(QuickToolEntry::manual(""));
                changed = true;
            }
            changed
        })
    }

    fn action_for(entry: &QuickToolEntry) -> Option<WidgetAction> {
        let query = entry.query.trim();
        if query.is_empty() {
            return None;
        }
        Some(WidgetAction {
            action: Action {
                label: query.to_string(),
                desc: "Tool".into(),
                action: format!(
                    "{}:{query}",
                    if entry.auto_submit {
                        "queryexec"
                    } else {
                        "query"
                    }
                ),
                args: None,
            },
            query_override: (!entry.auto_submit).then(|| query.to_string()),
        })
    }
}

impl Default for QuickToolsWidget {
    fn default() -> Self {
        Self::new(QuickToolsConfig::default())
    }
}

impl Widget for QuickToolsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        if self.cfg.queries.is_empty() {
            ui.label("Add tools in the widget settings.");
            return None;
        }

        let mut clicked = None;
        let mut duplicate_occurrences = HashMap::new();
        let mut visual_order = 0_u64;
        ui.horizontal_wrapped(|ui| {
            for entry in &self.cfg.queries {
                let Some(action) = Self::action_for(entry) else {
                    continue;
                };
                let source_query = action.query_override.as_deref();
                let execution_query = Some(action.action.label.as_str());
                let identity =
                    jump_action_identity(&action.action, source_query, execution_query, None, 0);
                let duplicate_occurrence =
                    next_jump_duplicate_occurrence(&mut duplicate_occurrences, identity);
                let row_id = dashboard_action_row_egui_id(
                    ui,
                    &action.action,
                    source_query,
                    execution_query,
                    None,
                    duplicate_occurrence,
                );
                let label = if entry.auto_submit {
                    format!("{} ↵", action.action.label)
                } else {
                    action.action.label.clone()
                };
                let response = ui
                    .scope_builder(egui::UiBuilder::new().id(row_id), |ui| ui.button(label))
                    .inner;
                let response = if entry.auto_submit {
                    response.on_hover_text("Auto submit enabled")
                } else {
                    response
                };
                if jump_response_clicked(
                    ui,
                    ctx,
                    &response,
                    || {
                        let identity = jump_action_identity(
                            &action.action,
                            source_query,
                            execution_query,
                            None,
                            duplicate_occurrence,
                        );
                        format!("{}/run", opaque_dashboard_action_identity(&identity))
                    },
                    || "Quick tool".to_owned(),
                    visual_order,
                ) {
                    clicked = Some(action);
                }
                visual_order = visual_order.saturating_add(1);
            }
        });
        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(cfg) = serde_json::from_value::<QuickToolsConfig>(settings.clone()) {
            self.cfg = cfg;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionIdLookup;
    use crate::dashboard::data_cache::DashboardDataCache;
    use crate::gui::jump_mode::ui::JumpModeUi;
    use crate::gui::jump_mode::{SemanticTargetId, SurfaceId, TargetGeneration, TargetId};
    use once_cell::sync::Lazy;
    use serde_json::json;
    use std::collections::HashMap;

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
    }

    fn body_scope(jump: &JumpModeUi, slot: &str) -> crate::gui::JumpModeScope {
        dashboard_scope(jump)
            .child_namespace(format!("slot-id:{slot}"))
            .child_namespace("body")
    }

    fn dashboard_context<'a>(
        plugins: &'a crate::plugin::PluginManager,
        data_cache: &'a DashboardDataCache,
        jump_mode_scope: crate::gui::JumpModeScope,
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
            jump_mode_activation_allowed,
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

    fn target_id(
        scope: &crate::gui::JumpModeScope,
        entry: &QuickToolEntry,
        duplicate_occurrence: usize,
    ) -> TargetId {
        let action = QuickToolsWidget::action_for(entry).expect("test entry is nonblank");
        let identity = jump_action_identity(
            &action.action,
            action.query_override.as_deref(),
            Some(&action.action.label),
            None,
            duplicate_occurrence,
        );
        scope.target_id(format!(
            "{}/run",
            opaque_dashboard_action_identity(&identity)
        ))
    }

    fn render_in_slot(
        ui: &mut egui::Ui,
        widget: &mut QuickToolsWidget,
        ctx: &DashboardContext<'_>,
        slot: &str,
    ) -> Option<WidgetAction> {
        ui.push_id(format!("slot-id:{slot}"), |ui| {
            ui.push_id("body", |ui| widget.render(ui, ctx, WidgetActivation::Click))
                .inner
        })
        .inner
    }

    fn config_with_abc() -> QuickToolsConfig {
        QuickToolsConfig {
            queries: vec![
                QuickToolEntry {
                    query: "A".into(),
                    auto_submit: true,
                },
                QuickToolEntry {
                    query: "B".into(),
                    auto_submit: false,
                },
                QuickToolEntry {
                    query: "C".into(),
                    auto_submit: true,
                },
            ],
        }
    }

    #[test]
    fn row_actions_move_complete_entries_up_and_down() {
        let mut moved_up = config_with_abc();
        assert!(apply_row_action(
            &mut moved_up,
            QuickToolRowAction::MoveUp(1)
        ));
        assert_eq!(
            moved_up.queries,
            vec![
                QuickToolEntry {
                    query: "B".into(),
                    auto_submit: false,
                },
                QuickToolEntry {
                    query: "A".into(),
                    auto_submit: true,
                },
                QuickToolEntry {
                    query: "C".into(),
                    auto_submit: true,
                },
            ]
        );

        let mut moved_down = config_with_abc();
        assert!(apply_row_action(
            &mut moved_down,
            QuickToolRowAction::MoveDown(1)
        ));
        assert_eq!(
            moved_down.queries,
            vec![
                QuickToolEntry {
                    query: "A".into(),
                    auto_submit: true,
                },
                QuickToolEntry {
                    query: "C".into(),
                    auto_submit: true,
                },
                QuickToolEntry {
                    query: "B".into(),
                    auto_submit: false,
                },
            ]
        );
    }

    #[test]
    fn invalid_and_boundary_row_actions_are_no_ops() {
        for action in [
            QuickToolRowAction::MoveUp(0),
            QuickToolRowAction::MoveDown(2),
            QuickToolRowAction::MoveUp(3),
            QuickToolRowAction::MoveDown(3),
            QuickToolRowAction::MoveDown(usize::MAX),
            QuickToolRowAction::Remove(3),
            QuickToolRowAction::Remove(usize::MAX),
        ] {
            let mut cfg = config_with_abc();
            let original = cfg.clone();
            assert!(!apply_row_action(&mut cfg, action));
            assert_eq!(cfg, original);
        }
    }

    #[test]
    fn remove_row_action_removes_only_the_selected_entry() {
        let mut cfg = config_with_abc();
        assert!(apply_row_action(&mut cfg, QuickToolRowAction::Remove(1)));
        assert_eq!(
            cfg.queries,
            vec![
                QuickToolEntry {
                    query: "A".into(),
                    auto_submit: true,
                },
                QuickToolEntry {
                    query: "C".into(),
                    auto_submit: true,
                },
            ]
        );
    }

    #[test]
    fn reordered_entries_serialize_and_generate_actions_in_the_new_sequence() {
        let mut cfg = config_with_abc();
        assert!(apply_row_action(&mut cfg, QuickToolRowAction::MoveDown(1)));

        assert_eq!(
            serde_json::to_value(&cfg).unwrap(),
            json!({"queries":[
                {"query":"A","auto_submit":true},
                {"query":"C","auto_submit":true},
                {"query":"B","auto_submit":false}
            ]})
        );
        assert_eq!(
            cfg.queries
                .iter()
                .map(|entry| QuickToolsWidget::action_for(entry).unwrap().action.action)
                .collect::<Vec<_>>(),
            ["queryexec:A", "queryexec:C", "query:B"]
        );
    }

    #[test]
    fn deserializes_legacy_structured_mixed_and_missing_flag_entries() {
        let legacy: QuickToolsConfig =
            serde_json::from_value(json!({"queries":["fs","mm","cm"]})).unwrap();
        assert_eq!(
            legacy
                .queries
                .iter()
                .map(|e| e.query.as_str())
                .collect::<Vec<_>>(),
            ["fs", "mm", "cm"]
        );
        assert!(legacy.queries.iter().all(|entry| !entry.auto_submit));

        let current: QuickToolsConfig = serde_json::from_value(json!({"queries":[
            {"query":"sys","auto_submit":true}, {"query":"net","auto_submit":false}
        ]}))
        .unwrap();
        assert!(current.queries[0].auto_submit);
        assert!(!current.queries[1].auto_submit);

        let mixed: QuickToolsConfig = serde_json::from_value(json!({"queries":[
            "legacy", {"query":"current","auto_submit":true}, {"query":"missing"}, ""
        ]}))
        .unwrap();
        assert_eq!(mixed.queries.len(), 4);
        assert!(!mixed.queries[0].auto_submit);
        assert!(mixed.queries[1].auto_submit);
        assert!(!mixed.queries[2].auto_submit);
        assert_eq!(mixed.queries[3].query, "");
        assert!(QuickToolsWidget::action_for(&mixed.queries[3]).is_none());
    }

    #[test]
    fn serialization_is_always_canonical_after_legacy_input() {
        let cfg: QuickToolsConfig = serde_json::from_value(json!({"queries":["fs"]})).unwrap();
        assert_eq!(
            serde_json::to_value(cfg).unwrap(),
            json!({"queries":[{"query":"fs","auto_submit":false}]})
        );
    }

    #[test]
    fn defaults_and_new_entries_are_manual() {
        assert!(default_queries().iter().all(|entry| !entry.auto_submit));
        assert_eq!(
            QuickToolEntry::manual(""),
            QuickToolEntry {
                query: String::new(),
                auto_submit: false
            }
        );
    }

    #[test]
    fn entry_edits_remain_independent_and_middle_removal_preserves_neighbors() {
        let mut entries = vec![
            QuickToolEntry {
                query: "one".into(),
                auto_submit: true,
            },
            QuickToolEntry::manual("two"),
            QuickToolEntry {
                query: "three".into(),
                auto_submit: true,
            },
        ];
        entries[1].auto_submit = true;
        assert!(entries[0].auto_submit);
        entries.remove(1);
        assert_eq!(
            entries,
            vec![
                QuickToolEntry {
                    query: "one".into(),
                    auto_submit: true
                },
                QuickToolEntry {
                    query: "three".into(),
                    auto_submit: true
                },
            ]
        );
    }

    #[test]
    fn actions_obey_manual_auto_submit_and_whitespace_contracts() {
        let manual =
            QuickToolsWidget::action_for(&QuickToolEntry::manual("  Fs  internal  Space "))
                .unwrap();
        assert_eq!(manual.action.label, "Fs  internal  Space");
        assert_eq!(manual.action.action, "query:Fs  internal  Space");
        assert_eq!(
            manual.query_override.as_deref(),
            Some("Fs  internal  Space")
        );

        let automatic = QuickToolsWidget::action_for(&QuickToolEntry {
            query: "  MM x ".into(),
            auto_submit: true,
        })
        .unwrap();
        assert_eq!(automatic.action.label, "MM x");
        assert_eq!(automatic.action.action, "queryexec:MM x");
        assert_eq!(automatic.query_override, None);
        assert!(QuickToolsWidget::action_for(&QuickToolEntry::manual(" \t ")).is_none());
    }

    #[test]
    fn quicktools_jump_targets_distinguish_duplicates_modes_and_widget_slots() {
        let entries = vec![
            QuickToolEntry::manual("same query"),
            QuickToolEntry::manual("same query"),
            QuickToolEntry {
                query: "same query".into(),
                auto_submit: true,
            },
            QuickToolEntry::manual("  \t "),
        ];
        let mut first = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
        });
        let mut second = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
        });
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let first_scope = body_scope(&jump, "first");
        let second_scope = body_scope(&jump, "second");
        let first_context = dashboard_context(&plugins, &data_cache, first_scope.clone(), true);
        let second_context = dashboard_context(&plugins, &data_cache, second_scope.clone(), true);
        let first_action = target_id(&first_scope, &entries[0], 0);
        let duplicate_action = target_id(&first_scope, &entries[1], 1);
        let auto_action = target_id(&first_scope, &entries[2], 0);
        let second_widget_action = target_id(&second_scope, &entries[0], 0);

        egui_ctx
            .run_ui(dashboard_input(Vec::new()), |ui| {
                ui.horizontal(|ui| {
                    assert!(render_in_slot(ui, &mut first, &first_context, "first").is_none());
                    ui.separator();
                    assert!(render_in_slot(ui, &mut second, &second_context, "second").is_none());
                });
            })
            .drop_without_applying_deltas();

        jump.finish_ui_pass();
        let registry = jump.pass_registry().expect("Jump Mode pass was collected");
        assert_eq!(
            registry.targets().len(),
            6,
            "blank rows have no action target"
        );
        let expected_ids = [
            first_action.clone(),
            duplicate_action.clone(),
            auto_action.clone(),
            second_widget_action.clone(),
        ];
        assert_eq!(
            expected_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            expected_ids.len()
        );
        let responses = expected_ids
            .iter()
            .map(|id| {
                registry
                    .get(id)
                    .expect("each semantic quick tool response is registered")
            })
            .collect::<Vec<_>>();
        assert!(
            responses
                .iter()
                .map(|target| target.response_id())
                .collect::<std::collections::HashSet<_>>()
                .len()
                == responses.len()
        );
        assert!(expected_ids.iter().all(|id| matches!(
            id.semantic(),
            SemanticTargetId::Named(value)
                if value.contains("dashboard-action:") && !value.contains("same query")
        )));
        jump.complete_frame(&egui_ctx, true, true);
    }

    #[test]
    fn quicktools_jump_focus_then_enter_activates_manual_and_auto_actions_once() {
        let entries = vec![
            QuickToolEntry::manual("manual query"),
            QuickToolEntry {
                query: "automatic query".into(),
                auto_submit: true,
            },
        ];
        let mut widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
        });
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = body_scope(&jump, "focus");
        let context = dashboard_context(&plugins, &data_cache, scope.clone(), true);
        egui_ctx
            .run_ui(dashboard_input(Vec::new()), |ui| {
                assert!(render_in_slot(ui, &mut widget, &context, "focus").is_none());
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        for (entry, expected_action, expected_override) in [
            (&entries[0], "query:manual query", Some("manual query")),
            (&entries[1], "queryexec:automatic query", None),
        ] {
            let target = target_id(&scope, entry, 0);
            jump.begin_frame();
            assert!(jump.queue_focus_intent(
                target.clone(),
                egui::ViewportId::ROOT,
                jump.navigation_epoch(),
            ));
            jump.begin_ui_pass();
            let scope = body_scope(&jump, "focus");
            let context = dashboard_context(&plugins, &data_cache, scope, true);
            let mut focused_action = None;
            egui_ctx
                .run_ui(dashboard_input(Vec::new()), |ui| {
                    focused_action = render_in_slot(ui, &mut widget, &context, "focus");
                })
                .drop_without_applying_deltas();
            assert!(
                focused_action.is_none(),
                "hint selection only focuses the button"
            );
            jump.finish_ui_pass();
            let registered = jump
                .pass_registry()
                .and_then(|registry| registry.get(&target))
                .expect("the current real button response is registered");
            assert_eq!(
                egui_ctx.memory(|memory| memory.focused()),
                Some(registered.response_id())
            );
            jump.complete_frame(&egui_ctx, true, true);

            jump.begin_frame();
            jump.begin_ui_pass();
            let scope = body_scope(&jump, "focus");
            let discarded_context = dashboard_context(&plugins, &data_cache, scope, false);
            let mut discarded_action = None;
            egui_ctx
                .run_ui(dashboard_input(vec![enter_event()]), |ui| {
                    discarded_action = render_in_slot(ui, &mut widget, &discarded_context, "focus");
                })
                .drop_without_applying_deltas();
            assert!(
                discarded_action.is_none(),
                "discarded passes cannot activate"
            );
            jump.finish_ui_pass();
            jump.complete_frame(&egui_ctx, true, true);

            jump.begin_frame();
            jump.begin_ui_pass();
            let scope = body_scope(&jump, "focus");
            let context = dashboard_context(&plugins, &data_cache, scope, true);
            let mut activated = Vec::new();
            egui_ctx
                .run_ui(dashboard_input(vec![enter_event()]), |ui| {
                    if let Some(action) = render_in_slot(ui, &mut widget, &context, "focus") {
                        activated.push(action);
                    }
                })
                .drop_without_applying_deltas();
            jump.finish_ui_pass();
            jump.complete_frame(&egui_ctx, true, true);

            assert_eq!(
                activated.len(),
                1,
                "fresh Enter activates one widget action"
            );
            assert_eq!(activated[0].action.action, expected_action);
            assert_eq!(activated[0].query_override.as_deref(), expected_override);
        }
    }

    #[test]
    fn quicktools_jump_omits_blank_and_clipped_buttons() {
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);
        let mut blank_widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: vec![QuickToolEntry::manual(" \t ")],
        });
        let mut clipped_widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: vec![QuickToolEntry::manual("visible query")],
        });

        jump.begin_frame();
        jump.begin_ui_pass();
        let blank_scope = body_scope(&jump, "blank");
        let clipped_scope = body_scope(&jump, "clipped");
        let blank_context = dashboard_context(&plugins, &data_cache, blank_scope, true);
        let clipped_context = dashboard_context(&plugins, &data_cache, clipped_scope, true);
        egui_ctx
            .run_ui(dashboard_input(Vec::new()), |ui| {
                assert!(render_in_slot(ui, &mut blank_widget, &blank_context, "blank").is_none());
                let mut clipped_ui = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                clipped_ui
                    .set_clip_rect(egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::ZERO));
                assert!(
                    render_in_slot(
                        &mut clipped_ui,
                        &mut clipped_widget,
                        &clipped_context,
                        "clipped"
                    )
                    .is_none()
                );
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        assert!(
            jump.pass_registry()
                .expect("Jump Mode pass was collected")
                .is_empty(),
            "blank and fully clipped controls are not navigation targets"
        );
    }
}
