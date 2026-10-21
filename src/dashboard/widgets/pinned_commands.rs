use super::{
    Widget, WidgetAction, WidgetSettingsContext, WidgetSettingsUiResult,
    dashboard_action_row_egui_id, edit_typed_settings, jump_action_identity, jump_response_clicked,
    opaque_dashboard_action_identity,
};
use crate::actions::{Action, ActionIdLookup};
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugins::fav::FavEntry;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_layout() -> PinnedLayout {
    PinnedLayout::List
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PinnedLayout {
    Grid,
    #[default]
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PinnedCommandsConfig {
    #[serde(default)]
    pub action_ids: Vec<String>,
    #[serde(default = "default_layout")]
    pub layout: PinnedLayout,
}

pub struct PinnedCommandsWidget {
    cfg: PinnedCommandsConfig,
    cached_favorites: Vec<FavEntry>,
    cached_resolved: Vec<Action>,
    last_actions_version: u64,
    last_fav_version: u64,
    last_action_ids_hash: u64,
}

impl PinnedCommandsWidget {
    pub fn new(cfg: PinnedCommandsConfig) -> Self {
        Self {
            cfg,
            cached_favorites: Vec::new(),
            cached_resolved: Vec::new(),
            last_actions_version: u64::MAX,
            last_fav_version: u64::MAX,
            last_action_ids_hash: u64::MAX,
        }
    }

    pub fn settings_ui(
        ui: &mut egui::Ui,
        value: &mut serde_json::Value,
        ctx: &WidgetSettingsContext<'_>,
    ) -> WidgetSettingsUiResult {
        edit_typed_settings(ui, value, ctx, |ui, cfg: &mut PinnedCommandsConfig, ctx| {
            let mut changed = false;
            let options = available_choices(ctx);
            if cfg.action_ids.is_empty() && !options.is_empty() {
                cfg.action_ids.push(options[0].0.clone());
                changed = true;
            }

            let mut remove_idx = None;
            for (idx, action_id) in cfg.action_ids.iter_mut().enumerate() {
                let token = ctx
                    .property_row_token("pinned-commands", idx)
                    .unwrap_or(idx as u64);
                ui.scope_builder(
                    egui::UiBuilder::new().id(ui.id().with(("pinned-command-row", token))),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(format!("Action {}", idx + 1));
                            let anchor = format!("pinned-commands/row/{token}/choice");
                            let popup_scope = ctx.combo_popup_scope(&anchor);
                            let combo =
                                egui::ComboBox::from_id_salt(ui.id().with(("pin-choice", token)))
                                    .selected_text(label_for(action_id, &options))
                                    .show_ui(ui, |ui| {
                                        for (id, label) in &options {
                                            let response =
                                                ui.selectable_value(action_id, id.clone(), label);
                                            let option =
                                                crate::dashboard::widgets::opaque_settings_key(id);
                                            ctx.register_jump_mode_target_in_scope(
                                                popup_scope.as_ref(),
                                                ui,
                                                &response,
                                                format!("pinned-commands/choice/{option}"),
                                                "Pinned action option",
                                                crate::gui::jump_mode::TargetKind::Selectable,
                                            );
                                            changed |= response.changed();
                                        }
                                    });
                            ctx.register_jump_mode_target(
                                ui,
                                &combo.response,
                                anchor,
                                "Pinned action",
                                crate::gui::jump_mode::TargetKind::ComboBox,
                            );
                            let remove = ui.small_button("✕");
                            ctx.register_jump_mode_target(
                                ui,
                                &remove,
                                format!("pinned-commands/row/{token}/remove"),
                                "Remove pinned action",
                                crate::gui::jump_mode::TargetKind::Button,
                            );
                            if remove.clicked() {
                                remove_idx = Some(idx);
                            }
                        })
                    },
                );
            }
            if let Some(idx) = remove_idx {
                ctx.remove_property_row_token("pinned-commands", idx);
                cfg.action_ids.remove(idx);
                changed = true;
            }
            let add_response = ui.button("Add action");
            ctx.register_jump_mode_target(
                ui,
                &add_response,
                "pinned-commands/add",
                "Add pinned action",
                crate::gui::jump_mode::TargetKind::Button,
            );
            if add_response.clicked() {
                cfg.action_ids.push(
                    options
                        .first()
                        .map(|o| o.0.clone())
                        .unwrap_or_else(|| "".into()),
                );
                changed = true;
            }

            let layout_anchor = "pinned-commands/layout";
            let layout_popup = ctx.combo_popup_scope(layout_anchor);
            let layout_combo = egui::ComboBox::from_id_salt(ui.id().with("pinned-commands-layout"))
                .selected_text(match cfg.layout {
                    PinnedLayout::Grid => "Grid",
                    PinnedLayout::List => "List",
                })
                .show_ui(ui, |ui| {
                    for (id, layout, label) in [
                        ("grid", PinnedLayout::Grid, "Grid"),
                        ("list", PinnedLayout::List, "List"),
                    ] {
                        let response = ui.selectable_value(&mut cfg.layout, layout, label);
                        ctx.register_jump_mode_target_in_scope(
                            layout_popup.as_ref(),
                            ui,
                            &response,
                            format!("pinned-commands/layout/{id}"),
                            "Pinned layout option",
                            crate::gui::jump_mode::TargetKind::Selectable,
                        );
                        changed |= response.changed();
                    }
                });
            ctx.register_jump_mode_target(
                ui,
                &layout_combo.response,
                layout_anchor,
                "Pinned layout",
                crate::gui::jump_mode::TargetKind::ComboBox,
            );
            changed
        })
    }

    fn resolve_action<'a>(
        &self,
        actions_by_id: &'a ActionIdLookup,
        favorites: &'a [FavEntry],
        id: &str,
    ) -> Option<Action> {
        if let Some(label) = id.strip_prefix("fav:")
            && let Some(f) = favorites
                .iter()
                .find(|f| f.label.eq_ignore_ascii_case(label))
        {
            return Some(Action {
                label: f.label.clone(),
                desc: "Fav".into(),
                action: f.action.clone(),
                args: f.args.clone(),
            });
        }

        actions_by_id.get(id).cloned().or_else(|| {
            favorites.iter().find(|f| f.action == id).map(|fav| Action {
                label: fav.label.clone(),
                desc: "Fav".into(),
                action: fav.action.clone(),
                args: fav.args.clone(),
            })
        })
    }

    fn refresh_cache(&mut self, ctx: &DashboardContext<'_>) {
        let action_ids_hash = action_ids_hash(&self.cfg.action_ids);
        if self.last_actions_version == ctx.actions_version
            && self.last_fav_version == ctx.fav_version
            && self.last_action_ids_hash == action_ids_hash
        {
            return;
        }

        let snapshot = ctx.data_cache.snapshot();
        self.cached_favorites = snapshot.favorites.as_ref().clone();
        self.cached_resolved.clear();
        for id in &self.cfg.action_ids {
            if let Some(action) = self.resolve_action(ctx.actions_by_id, &self.cached_favorites, id)
            {
                self.cached_resolved.push(action);
            }
        }
        self.last_actions_version = ctx.actions_version;
        self.last_fav_version = ctx.fav_version;
        self.last_action_ids_hash = action_ids_hash;
    }
}

impl Default for PinnedCommandsWidget {
    fn default() -> Self {
        Self {
            cfg: PinnedCommandsConfig::default(),
            cached_favorites: Vec::new(),
            cached_resolved: Vec::new(),
            last_actions_version: u64::MAX,
            last_fav_version: u64::MAX,
            last_action_ids_hash: u64::MAX,
        }
    }
}

impl Widget for PinnedCommandsWidget {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        _activation: WidgetActivation,
    ) -> Option<WidgetAction> {
        self.refresh_cache(ctx);
        if self.cached_resolved.is_empty() {
            ui.label("Pick actions or favorites in the widget settings.");
            return None;
        }

        let mut duplicate_occurrences = HashMap::new();
        let mut clicked = None;
        let mut visual_order = 0;
        match self.cfg.layout {
            PinnedLayout::List => {
                for action in &self.cached_resolved {
                    let base_row_id = dashboard_action_row_egui_id(
                        ui,
                        action,
                        None,
                        Some(&action.label),
                        None,
                        0,
                    );
                    let occurrence =
                        next_action_occurrence(&mut duplicate_occurrences, base_row_id);
                    let activated =
                        render_pinned_action_button(ui, ctx, action, occurrence, visual_order);
                    if clicked.is_none() {
                        clicked = activated;
                    }
                    visual_order += 1;
                }
            }
            PinnedLayout::Grid => {
                ui.horizontal_wrapped(|ui| {
                    for action in &self.cached_resolved {
                        let base_row_id = dashboard_action_row_egui_id(
                            ui,
                            action,
                            None,
                            Some(&action.label),
                            None,
                            0,
                        );
                        let occurrence =
                            next_action_occurrence(&mut duplicate_occurrences, base_row_id);
                        let activated =
                            render_pinned_action_button(ui, ctx, action, occurrence, visual_order);
                        if clicked.is_none() {
                            clicked = activated;
                        }
                        visual_order += 1;
                    }
                });
            }
        }

        clicked
    }
}

fn render_pinned_action_button(
    ui: &mut egui::Ui,
    ctx: &DashboardContext<'_>,
    action: &Action,
    duplicate_occurrence: usize,
    visual_order: u64,
) -> Option<WidgetAction> {
    let row_id = dashboard_action_row_egui_id(
        ui,
        action,
        None,
        Some(&action.label),
        None,
        duplicate_occurrence,
    );
    let response = ui
        .scope_builder(egui::UiBuilder::new().id(row_id), |ui| {
            ui.button(&action.label)
        })
        .inner;
    jump_response_clicked(
        ui,
        ctx,
        &response,
        || {
            let identity = jump_action_identity(
                action,
                None,
                Some(&action.label),
                None,
                duplicate_occurrence,
            );
            format!("{}/run", opaque_dashboard_action_identity(&identity))
        },
        || "Pinned command".to_owned(),
        visual_order,
    )
    .then(|| WidgetAction {
        query_override: Some(action.label.clone()),
        action: action.clone(),
    })
}

fn next_action_occurrence(occurrences: &mut HashMap<egui::Id, usize>, base_id: egui::Id) -> usize {
    let next = occurrences.entry(base_id).or_default();
    let occurrence = *next;
    *next = (*next).saturating_add(1);
    occurrence
}

fn available_choices(ctx: &WidgetSettingsContext<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(actions) = ctx.actions {
        for a in actions {
            out.push((a.action.clone(), format!("{} ({})", a.label, a.action)));
        }
    }
    if let Some(favorites) = ctx.favorites {
        for f in favorites {
            out.push((format!("fav:{}", f.label), format!("Favorite: {}", f.label)));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

fn label_for(id: &str, options: &[(String, String)]) -> String {
    options
        .iter()
        .find(|(opt_id, _)| opt_id == id)
        .map(|(_, label)| label.clone())
        .unwrap_or_else(|| id.to_string())
}

fn action_ids_hash(action_ids: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    action_ids.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::data_cache::DashboardDataCache;
    use crate::plugin::PluginManager;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_action(id: &str, label: &str) -> Action {
        Action {
            label: label.into(),
            desc: "desc".into(),
            action: id.into(),
            args: None,
        }
    }

    fn make_context<'a>(
        actions: &'a [Action],
        actions_by_id: &'a ActionIdLookup,
        data_cache: &'a DashboardDataCache,
        plugins: &'a PluginManager,
        usage: &'a HashMap<String, u32>,
        actions_version: u64,
        fav_version: u64,
    ) -> DashboardContext<'a> {
        DashboardContext {
            actions,
            actions_by_id,
            usage,
            plugins,
            enabled_plugins: None,
            default_location: None,
            data_cache,
            actions_version,
            fav_version,
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
            jump_mode_scope: None,
            jump_mode_activation_allowed: true,
        }
    }

    fn jump_input(events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 500.0),
            )),
            focused: true,
            events,
            ..Default::default()
        }
    }

    fn pinned_body_scope(
        jump: &crate::gui::jump_mode::ui::JumpModeUi,
    ) -> crate::gui::JumpModeScope {
        jump.scope(
            crate::gui::jump_mode::SurfaceId::Dashboard,
            crate::gui::jump_mode::TargetGeneration(7),
            "dashboard-test",
            egui::ViewportId::ROOT,
            true,
        )
        .expect("enabled Jump Mode creates a dashboard scope")
        .child_namespace("slot-id:pinned")
        .child_namespace("body")
    }

    fn render_pinned(
        widget: &mut PinnedCommandsWidget,
        ctx: &DashboardContext<'_>,
        egui_ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Option<WidgetAction> {
        let mut activated = None;
        egui_ctx
            .run_ui(jump_input(events), |ui| {
                ui.push_id("slot-id:pinned", |ui| {
                    ui.push_id("body", |ui| {
                        activated = widget.render(ui, ctx, WidgetActivation::Click);
                    });
                });
            })
            .drop_without_applying_deltas();
        activated
    }

    #[test]
    fn refreshes_when_action_ids_change() {
        let data_cache = DashboardDataCache::new();
        let plugins = PluginManager::new();
        let usage = HashMap::new();
        let mut widget = PinnedCommandsWidget::new(PinnedCommandsConfig {
            action_ids: vec!["a".into()],
            layout: PinnedLayout::List,
        });
        let actions = Arc::new(vec![make_action("a", "First"), make_action("b", "Second")]);
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let ctx = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            1,
            1,
        );
        widget.refresh_cache(&ctx);
        assert_eq!(widget.cached_resolved[0].label, "First");

        widget.cfg.action_ids = vec!["b".into()];
        let ctx = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            1,
            1,
        );
        widget.refresh_cache(&ctx);
        assert_eq!(widget.cached_resolved[0].label, "Second");
    }

    #[test]
    fn refreshes_when_actions_version_changes() {
        let data_cache = DashboardDataCache::new();
        let plugins = PluginManager::new_inert_for_test();
        let usage = HashMap::new();
        let mut widget = PinnedCommandsWidget::new(PinnedCommandsConfig {
            action_ids: vec!["a".into()],
            layout: PinnedLayout::List,
        });
        let actions = Arc::new(vec![make_action("a", "First")]);
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let ctx = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            1,
            1,
        );
        widget.refresh_cache(&ctx);
        assert_eq!(widget.cached_resolved[0].label, "First");

        let actions = Arc::new(vec![make_action("a", "Updated")]);
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let ctx = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            2,
            1,
        );
        widget.refresh_cache(&ctx);
        assert_eq!(widget.cached_resolved[0].label, "Updated");
    }

    #[test]
    fn settings_choices_use_supplied_favorites_snapshot() {
        let favorites = vec![FavEntry {
            label: "Snapshot favorite".into(),
            action: "query:snapshot".into(),
            args: None,
        }];
        let ctx = WidgetSettingsContext {
            favorites: Some(&favorites),
            ..WidgetSettingsContext::empty()
        };

        assert_eq!(
            available_choices(&ctx),
            vec![(
                "fav:Snapshot favorite".into(),
                "Favorite: Snapshot favorite".into()
            )]
        );
    }

    #[test]
    fn jump_mode_dashboard_commands_pinned_rows_are_opaque_stable_and_focus_only() {
        use crate::gui::jump_mode::ui::JumpModeUi;
        use crate::gui::jump_mode::{SemanticTargetId, TargetId};

        let data_cache = DashboardDataCache::new();
        let plugins = PluginManager::new_inert_for_test();
        let usage = HashMap::new();
        let actions = Arc::new(Vec::new());
        let actions_by_id = ActionIdLookup::new(Arc::clone(&actions));
        let mut widget = PinnedCommandsWidget::new(PinnedCommandsConfig {
            action_ids: Vec::new(),
            layout: PinnedLayout::List,
        });
        let no_args = make_action(
            "plugin:private-snippet-command",
            "Private clipboard payload label",
        );
        let empty_args = Action {
            args: Some(String::new()),
            ..no_args.clone()
        };
        let private_args = Action {
            args: Some("--private=snippet body".into()),
            ..no_args.clone()
        };
        widget.cached_resolved = vec![
            no_args.clone(),
            no_args.clone(),
            empty_args.clone(),
            private_args.clone(),
        ];
        widget.last_actions_version = 0;
        widget.last_fav_version = 0;
        widget.last_action_ids_hash = action_ids_hash(&widget.cfg.action_ids);

        let mut jump = JumpModeUi::new(true);
        let egui_ctx = egui::Context::default();
        let mut context = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            0,
            0,
        );

        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = pinned_body_scope(&jump);
        context.jump_mode_scope = Some(scope.clone());
        assert!(render_pinned(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let first_registry = jump.pass_registry().expect("pinned pass was collected");
        assert_eq!(first_registry.len(), 4);
        assert!(
            first_registry
                .metadata()
                .all(|metadata| metadata.label() == "Pinned command")
        );
        let first_targets = first_registry
            .targets()
            .iter()
            .map(|target| {
                assert_eq!(
                    target.metadata().kind(),
                    crate::gui::jump_mode::TargetKind::Button
                );
                assert!(
                    !format!("{:?}", target.id().semantic()).contains("private-snippet-command")
                );
                assert!(!format!("{:?}", target.id().semantic()).contains("Private clipboard"));
                assert!(!format!("{:?}", target.id().semantic()).contains("snippet body"));
                (target.id().clone(), target.response_id())
            })
            .collect::<HashMap<TargetId, egui::Id>>();
        assert_eq!(first_targets.len(), 4, "duplicate rows stay distinct");
        assert!(first_targets.keys().all(|target| matches!(
            target.semantic(),
            SemanticTargetId::Named(value) if value.contains("dashboard-action:")
        )));
        jump.complete_frame(&egui_ctx, true, true);

        widget.cached_resolved = vec![
            private_args,
            empty_args.clone(),
            no_args.clone(),
            no_args.clone(),
        ];
        jump.begin_frame();
        jump.begin_ui_pass();
        let mut context = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            0,
            0,
        );
        context.jump_mode_scope = Some(pinned_body_scope(&jump));
        assert!(render_pinned(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let reordered_registry = jump.pass_registry().expect("reordered pass was collected");
        let reordered_targets = reordered_registry
            .targets()
            .iter()
            .map(|target| (target.id().clone(), target.response_id()))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            first_targets, reordered_targets,
            "list row identity survives reorder"
        );
        jump.complete_frame(&egui_ctx, true, true);

        let focus_identity = opaque_dashboard_action_identity(&jump_action_identity(
            &empty_args,
            None,
            Some(&empty_args.label),
            None,
            0,
        ));
        let target = scope.target_id(format!("{focus_identity}/run"));
        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            target.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        widget.cfg.layout = PinnedLayout::Grid;
        let mut context = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            0,
            0,
        );
        context.jump_mode_scope = Some(pinned_body_scope(&jump));
        assert!(render_pinned(&mut widget, &context, &egui_ctx, Vec::new()).is_none());
        jump.finish_ui_pass();
        let focused_response = jump
            .pass_registry()
            .expect("focused grid pass was collected")
            .get(&target)
            .expect("the exact grid command remains registered")
            .response_id();
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(focused_response),
            "hint completion focuses the actual pinned command button"
        );
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let mut context = make_context(
            actions.as_slice(),
            &actions_by_id,
            &data_cache,
            &plugins,
            &usage,
            0,
            0,
        );
        context.jump_mode_scope = Some(pinned_body_scope(&jump));
        let activation = render_pinned(
            &mut widget,
            &context,
            &egui_ctx,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: Some(egui::Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);
        let activation = activation.expect("a distinct Enter activates the focused row");
        assert_eq!(activation.action.action, empty_args.action);
        assert_eq!(activation.action.args, Some(String::new()));
        assert_eq!(activation.query_override, Some(empty_args.label));
    }
}
