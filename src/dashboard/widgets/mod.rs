use crate::actions::Action;
use crate::dashboard::dashboard::{DashboardContext, WidgetActivation};
use crate::plugin::PluginManager;
use eframe::egui;
use serde_json::Value;
use std::collections::HashSet;

mod active_timers;
mod browser_tabs;
mod calendar;
mod clipboard_recent;
mod clipboard_snippets;
mod command_history;
mod context_links;
mod diagnostics;
mod frequent_commands;
mod gesture_cheat_sheet;
mod gesture_health;
mod gesture_recent;
mod layouts;
mod notes_graph;
mod notes_recent;
mod notes_tags;
mod now_playing;
mod pinned_commands;
mod pinned_query_results;
pub(crate) mod plugin_home;
mod process_list;
mod query_list;
mod query_suggestions;
mod quick_tools;
mod recent_commands;
mod recent_notes;
mod recycle_bin;
mod registry;
mod render;
mod scratchpad;
mod snippets_favorites;
mod stopwatch;
mod system_actions;
mod system_controls;
mod system_status;
mod tempfiles;
mod todo;
mod todo_focus;
mod volume;
mod weather_site;
mod window_list;
mod windows_overview;

pub use active_timers::ActiveTimersWidget;
pub use browser_tabs::BrowserTabsWidget;
pub use calendar::CalendarWidget;
pub use clipboard_recent::ClipboardRecentWidget;
pub use clipboard_snippets::ClipboardSnippetsWidget;
pub use command_history::CommandHistoryWidget;
pub use context_links::ContextLinksWidget;
pub use diagnostics::DiagnosticsWidget;
pub use frequent_commands::FrequentCommandsWidget;
pub use gesture_cheat_sheet::GestureCheatSheetWidget;
pub use gesture_health::GestureHealthWidget;
pub use gesture_recent::GestureRecentWidget;
pub use layouts::LayoutsWidget;
pub use notes_graph::NotesGraphWidget;
pub use notes_recent::NotesRecentWidget;
pub use notes_tags::NotesTagsWidget;
pub use now_playing::NowPlayingWidget;
pub use pinned_commands::PinnedCommandsWidget;
pub use pinned_query_results::{ClickBehavior, PinnedQueryResultsConfig, PinnedQueryResultsWidget};
pub use plugin_home::PluginHomeWidget;
pub use process_list::ProcessesWidget;
pub use query_list::QueryListWidget;
pub use quick_tools::QuickToolsWidget;
pub use recent_commands::RecentCommandsWidget;
pub use recent_notes::RecentNotesWidget;
pub use recycle_bin::RecycleBinWidget;
pub use registry::{WidgetDescriptor, WidgetFactory, WidgetMetadata, WidgetRegistry};
pub use render::{RefreshMode, TimedCache};
pub use scratchpad::ScratchpadWidget;
pub use snippets_favorites::SnippetsFavoritesWidget;
pub use stopwatch::StopwatchWidget;
pub use system_actions::SystemWidget;
pub use system_controls::SystemControlsWidget;
pub use system_status::SystemStatusWidget;
pub use tempfiles::TempfilesWidget;
pub use todo::TodoWidget;
pub use todo_focus::TodoFocusWidget;
pub use volume::VolumeWidget;
pub use weather_site::WeatherSiteWidget;
pub use window_list::WindowsWidget;
pub use windows_overview::WindowsOverviewWidget;

#[derive(Debug, Clone)]
pub struct WidgetAction {
    pub action: Action,
    pub query_override: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct WidgetSettingsUiResult {
    pub changed: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
pub struct WidgetSettingsContext<'a> {
    pub plugins: Option<&'a PluginManager>,
    pub plugin_infos: Option<&'a [(String, String, Vec<String>)]>,
    pub plugin_commands: Option<&'a [Action]>,
    pub actions: Option<&'a [Action]>,
    pub favorites: Option<&'a [crate::plugins::fav::FavEntry]>,
    pub usage: Option<&'a std::collections::HashMap<String, u32>>,
    pub default_location: Option<&'a str>,
    pub enabled_plugins: Option<&'a HashSet<String>>,
}

impl<'a> WidgetSettingsContext<'a> {
    pub fn empty() -> Self {
        Self {
            plugins: None,
            plugin_infos: None,
            plugin_commands: None,
            actions: None,
            favorites: None,
            usage: None,
            default_location: None,
            enabled_plugins: None,
        }
    }
}

pub type SettingsUiFn =
    fn(&mut egui::Ui, &mut Value, &WidgetSettingsContext<'_>) -> WidgetSettingsUiResult;

pub trait Widget: Send {
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &DashboardContext<'_>,
        activation: WidgetActivation,
    ) -> Option<WidgetAction>;

    fn on_config_updated(&mut self, _settings: &Value) {}

    fn header_ui(
        &mut self,
        _ui: &mut egui::Ui,
        _ctx: &DashboardContext<'_>,
    ) -> Option<WidgetAction> {
        None
    }
}

/// Register one real dashboard response when Jump Mode is enabled, and apply
/// only its native focus directive. Click activation remains with the
/// widget's existing response branch and is suppressed on discarded
/// Jump-Mode passes.
pub(crate) fn jump_response_clicked(
    ui: &egui::Ui,
    ctx: &DashboardContext<'_>,
    response: &egui::Response,
    target_identity: impl FnOnce() -> String,
    label: impl FnOnce() -> String,
    visual_order: u64,
) -> bool {
    register_jump_response(
        ui,
        ctx,
        response,
        target_identity,
        label,
        crate::gui::jump_mode::TargetKind::Button,
        visual_order,
    );

    response.clicked() && ctx.jump_mode_activation_allowed
}

/// Register a current response with its real interaction kind and apply only
/// the focus directive. Non-button controls use this when their ordinary
/// widget behavior remains owned by the rendering component.
pub(crate) fn register_jump_response(
    ui: &egui::Ui,
    ctx: &DashboardContext<'_>,
    response: &egui::Response,
    target_identity: impl FnOnce() -> String,
    label: impl FnOnce() -> String,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: u64,
) {
    if let Some(scope) = ctx.jump_mode_scope.as_ref()
        && let crate::gui::jump_mode::ui::RegistrationResult::Registered {
            directive: Some(directive),
            ..
        } = scope.register(ui, response, target_identity(), label(), kind, visual_order)
    {
        directive.apply_to_response(response);
    }
}

/// Stable, unambiguous identity for query/history action rows. The duplicate
/// occurrence distinguishes identical payloads in one visible list while
/// keeping unique rows independent of their current visual order.
pub(crate) fn jump_action_identity(
    action: &Action,
    source_query: Option<&str>,
    execution_query: Option<&str>,
    history_timestamp: Option<i64>,
    duplicate_occurrence: usize,
) -> String {
    fn push_part(output: &mut String, part: &str) {
        output.push_str(&part.len().to_string());
        output.push(':');
        output.push_str(part);
    }

    let mut identity = String::from("action-row/");
    push_part(&mut identity, &action.action);
    match action.args.as_deref() {
        Some(args) => {
            identity.push('1');
            push_part(&mut identity, args);
        }
        None => identity.push('0'),
    }
    for query in [source_query, execution_query] {
        match query {
            Some(query) => {
                identity.push('1');
                push_part(&mut identity, query);
            }
            None => identity.push('0'),
        }
    }
    match history_timestamp {
        Some(timestamp) => {
            identity.push('1');
            identity.push_str(&timestamp.to_string());
            identity.push(';');
        }
        None => identity.push('0'),
    }
    identity.push_str("/duplicate:");
    identity.push_str(&duplicate_occurrence.to_string());
    identity
}

/// Opaque Jump Mode identity for a canonical dashboard action identity. The
/// digest avoids exposing action arguments, history queries, or other row
/// payload strings through target metadata.
pub(crate) fn opaque_dashboard_action_identity(canonical_identity: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(canonical_identity.as_bytes());
    format!("dashboard-action:{}", hex::encode(digest))
}

/// Native egui identity for a dynamic action row. It hashes borrowed payload
/// fields directly, so rendering with Jump Mode disabled does not allocate a
/// semantic target identity string.
pub(crate) fn dashboard_action_row_egui_id(
    ui: &egui::Ui,
    action: &Action,
    source_query: Option<&str>,
    execution_query: Option<&str>,
    history_timestamp: Option<i64>,
    duplicate_occurrence: usize,
) -> egui::Id {
    ui.id().with((
        "dashboard-action-row",
        action.action.as_str(),
        action.args.as_deref(),
        source_query,
        execution_query,
        history_timestamp,
        duplicate_occurrence,
    ))
}

pub(crate) fn next_jump_duplicate_occurrence(
    occurrences: &mut std::collections::HashMap<String, usize>,
    base_identity: String,
) -> usize {
    let next = occurrences.entry(base_identity).or_default();
    let occurrence = *next;
    *next = next.saturating_add(1);
    occurrence
}

pub(crate) use query_suggestions::query_suggestions;
pub(crate) use render::{
    BackgroundLoader, default_refresh_throttle_secs, edit_typed_settings, find_plugin,
    gesture_focus_action, gesture_toggle_action, merge_json, observe_owned_search_publication,
    observe_search_generation, plugin_names, refresh_schedule, refresh_settings_ui,
    run_refresh_schedule, submit_background_refresh,
};

#[cfg(test)]
mod jump_mode_tests {
    use super::*;
    use crate::actions::ActionIdLookup;
    use crate::dashboard::data_cache::DashboardDataCache;
    use crate::gui::jump_mode::ui::JumpModeUi;
    use crate::gui::jump_mode::{HintMap, SurfaceId, TargetGeneration};
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

    fn root_scope(jump: &JumpModeUi) -> crate::gui::JumpModeScope {
        jump.scope(
            SurfaceId::Dashboard,
            TargetGeneration(7),
            "dashboard-test",
            egui::ViewportId::ROOT,
            true,
        )
        .expect("enabled Jump Mode creates a dashboard scope")
    }

    fn dashboard_context<'a>(
        plugins: &'a crate::plugin::PluginManager,
        data_cache: &'a DashboardDataCache,
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
            todo_version: 0,
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
    fn jump_mode_dashboard_focus_then_enter_activates_only_the_body_response() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let initial_root = root_scope(&jump);
        let slot_scope = initial_root.child_namespace("slot-id:main");
        let header_scope = slot_scope.child_namespace("header");
        let body_scope = slot_scope.child_namespace("body");
        let target_id = body_scope.target_id("refresh");
        assert_ne!(header_scope.target_id("refresh"), target_id);
        let duplicate_slot_scope = initial_root
            .child_namespace("slot-id:secondary")
            .child_namespace("header");
        assert_ne!(
            header_scope.target_id("refresh"),
            duplicate_slot_scope.target_id("refresh")
        );
        jump.finish_ui_pass();

        jump.begin_frame();
        assert!(jump.queue_focus_intent(
            target_id.clone(),
            egui::ViewportId::ROOT,
            jump.navigation_epoch(),
        ));
        jump.begin_ui_pass();
        let root = root_scope(&jump);
        let header_scope = root
            .child_namespace("slot-id:main")
            .child_namespace("header");
        let second_header_scope = root
            .child_namespace("slot-id:secondary")
            .child_namespace("header");
        let body_scope = root.child_namespace("slot-id:main").child_namespace("body");
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let header_context = dashboard_context(&plugins, &data_cache, Some(header_scope), true);
        let second_header_context =
            dashboard_context(&plugins, &data_cache, Some(second_header_scope), true);
        let body_context = dashboard_context(&plugins, &data_cache, Some(body_scope), true);
        let egui_ctx = egui::Context::default();
        let mut header_response_ids = Vec::new();
        let mut body_response_id = None;
        let mut body_activated = false;

        egui_ctx
            .run_ui(input(Vec::new()), |ui| {
                ui.push_id("slot-id:main", |ui| {
                    ui.push_id("header", |ui| {
                        let response = ui.small_button("Refresh");
                        header_response_ids.push(response.id);
                        assert!(!jump_response_clicked(
                            ui,
                            &header_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        ));
                    });
                });
                ui.push_id("slot-id:secondary", |ui| {
                    ui.push_id("header", |ui| {
                        let response = ui.small_button("Refresh");
                        header_response_ids.push(response.id);
                        assert!(!jump_response_clicked(
                            ui,
                            &second_header_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        ));
                    });
                });
                ui.push_id("slot-id:main", |ui| {
                    ui.push_id("body", |ui| {
                        let response = ui.small_button("Refresh");
                        body_response_id = Some(response.id);
                        assert!(!jump_response_clicked(
                            ui,
                            &body_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        ));
                    });
                });
            })
            .drop_without_applying_deltas();
        assert_eq!(header_response_ids.len(), 2);
        assert_ne!(header_response_ids[0], header_response_ids[1]);
        assert_eq!(egui_ctx.memory(|memory| memory.focused()), body_response_id);
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        jump.begin_frame();
        jump.begin_ui_pass();
        let root = root_scope(&jump);
        let header_scope = root
            .child_namespace("slot-id:main")
            .child_namespace("header");
        let second_header_scope = root
            .child_namespace("slot-id:secondary")
            .child_namespace("header");
        let body_scope = root.child_namespace("slot-id:main").child_namespace("body");
        let header_context = dashboard_context(&plugins, &data_cache, Some(header_scope), true);
        let second_header_context =
            dashboard_context(&plugins, &data_cache, Some(second_header_scope), true);
        let body_context = dashboard_context(&plugins, &data_cache, Some(body_scope), true);
        egui_ctx
            .run_ui(input(vec![enter_event()]), |ui| {
                ui.push_id("slot-id:main", |ui| {
                    ui.push_id("header", |ui| {
                        let response = ui.small_button("Refresh");
                        assert!(!jump_response_clicked(
                            ui,
                            &header_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        ));
                    });
                });
                ui.push_id("slot-id:secondary", |ui| {
                    ui.push_id("header", |ui| {
                        let response = ui.small_button("Refresh");
                        assert!(!jump_response_clicked(
                            ui,
                            &second_header_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        ));
                    });
                });
                ui.push_id("slot-id:main", |ui| {
                    ui.push_id("body", |ui| {
                        let response = ui.small_button("Refresh");
                        body_activated = jump_response_clicked(
                            ui,
                            &body_context,
                            &response,
                            || "refresh".into(),
                            || "Refresh".into(),
                            0,
                        );
                    });
                });
            })
            .drop_without_applying_deltas();
        assert!(body_activated);
    }

    #[test]
    fn jump_mode_discarded_pass_does_not_repeat_direct_mutation() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let body = root_scope(&jump)
            .child_namespace("slot-id:scratchpad")
            .child_namespace("body");
        let target = body.target_id("scratchpad:append-timestamp");
        jump.finish_ui_pass();

        jump.begin_frame();
        assert!(jump.queue_focus_intent(target, egui::ViewportId::ROOT, jump.navigation_epoch(),));
        jump.begin_ui_pass();
        let body = root_scope(&jump)
            .child_namespace("slot-id:scratchpad")
            .child_namespace("body");
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let first_pass_context = dashboard_context(&plugins, &data_cache, Some(body), true);
        let egui_ctx = egui::Context::default();
        let mut mutation_count = 0;
        egui_ctx
            .run_ui(input(Vec::new()), |ui| {
                let response = ui.button("Append timestamp");
                assert!(!jump_response_clicked(
                    ui,
                    &first_pass_context,
                    &response,
                    || "scratchpad:append-timestamp".into(),
                    || "Append timestamp".into(),
                    0,
                ));
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        jump.complete_frame(&egui_ctx, true, true);

        for activation_allowed in [true, false] {
            jump.begin_frame();
            jump.begin_ui_pass();
            let body = root_scope(&jump)
                .child_namespace("slot-id:scratchpad")
                .child_namespace("body");
            let pass_context =
                dashboard_context(&plugins, &data_cache, Some(body), activation_allowed);
            egui_ctx
                .run_ui(input(vec![enter_event()]), |ui| {
                    let response = ui.button("Append timestamp");
                    if jump_response_clicked(
                        ui,
                        &pass_context,
                        &response,
                        || "scratchpad:append-timestamp".into(),
                        || "Append timestamp".into(),
                        0,
                    ) {
                        mutation_count += 1;
                    }
                })
                .drop_without_applying_deltas();
            jump.finish_ui_pass();
            jump.complete_frame(&egui_ctx, true, true);
        }
        assert_eq!(mutation_count, 1);
    }

    #[test]
    fn jump_mode_clipped_and_absent_dashboard_controls_have_no_hint() {
        let mut jump = JumpModeUi::new(true);
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = root_scope(&jump)
            .child_namespace("slot-id:visible")
            .child_namespace("body");
        let plugins = crate::plugin::PluginManager::new();
        let data_cache = DashboardDataCache::new();
        let dashboard_context = dashboard_context(&plugins, &data_cache, Some(scope), true);
        let egui_ctx = egui::Context::default();
        egui_ctx
            .run_ui(input(Vec::new()), |ui| {
                let mut clipped = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect()));
                clipped.set_clip_rect(egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::ZERO));
                let response = clipped.button("Clipped control");
                assert!(!jump_response_clicked(
                    &clipped,
                    &dashboard_context,
                    &response,
                    || "clipped".into(),
                    || "Clipped control".into(),
                    0,
                ));
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        let registry = jump.pass_registry().expect("the pass was collected");
        assert!(registry.is_empty());
        assert!(
            HintMap::assign(registry.metadata().cloned())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn jump_mode_action_row_identity_tracks_execution_payload_and_order_stably() {
        let action = Action {
            label: "Visible label".into(),
            desc: "Display-only description".into(),
            action: "command:launch".into(),
            args: Some("--profile test".into()),
        };
        let mut presentation_changed = action.clone();
        presentation_changed.label = "Renamed label".into();
        presentation_changed.desc = "Changed description".into();
        let identity = jump_action_identity(
            &action,
            Some("source query"),
            Some("execution query"),
            Some(123),
            0,
        );
        assert_eq!(
            identity,
            jump_action_identity(
                &presentation_changed,
                Some("source query"),
                Some("execution query"),
                Some(123),
                0,
            )
        );
        assert_ne!(
            identity,
            jump_action_identity(
                &action,
                Some("source query"),
                Some("execution query"),
                Some(124),
                0,
            )
        );
        assert_ne!(
            identity,
            jump_action_identity(
                &action,
                Some("other source query"),
                Some("execution query"),
                Some(123),
                0,
            )
        );
        assert_ne!(
            identity,
            jump_action_identity(
                &action,
                Some("source query"),
                Some("other execution query"),
                Some(123),
                0,
            )
        );
        let mut args_changed = action.clone();
        args_changed.args = Some("--profile other".into());
        assert_ne!(
            identity,
            jump_action_identity(
                &args_changed,
                Some("source query"),
                Some("execution query"),
                Some(123),
                0,
            )
        );
        assert_ne!(
            jump_action_identity(&action, None, None, None, 0),
            jump_action_identity(&action, None, None, None, 1)
        );
        let another_action = Action {
            action: "command:other".into(),
            ..presentation_changed
        };
        assert_ne!(
            jump_action_identity(&action, None, None, None, 0),
            jump_action_identity(&another_action, None, None, None, 0)
        );
        egui::__run_test_ui(|ui| {
            let before = dashboard_action_row_egui_id(
                ui,
                &action,
                Some("source query"),
                Some("execution query"),
                Some(123),
                0,
            );
            let reordered = [another_action.clone(), action.clone()];
            let after = dashboard_action_row_egui_id(
                ui,
                &reordered[1],
                Some("source query"),
                Some("execution query"),
                Some(123),
                0,
            );
            assert_eq!(before, after);
        });
    }
}
