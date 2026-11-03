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

const MIN_PREFERRED_COLUMNS: usize = 1;
const MAX_PREFERRED_COLUMNS: usize = 8;
const DEFAULT_PREFERRED_COLUMNS: usize = 3;
const MIN_VISIBLE_ROWS: usize = 1;
const MAX_VISIBLE_ROWS: usize = 12;
const DEFAULT_VISIBLE_ROWS: usize = 2;
const MIN_QUICK_TOOL_CELL_WIDTH: f32 = 96.0;

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

fn default_preferred_columns() -> usize {
    DEFAULT_PREFERRED_COLUMNS
}

fn default_visible_rows() -> usize {
    DEFAULT_VISIBLE_ROWS
}

#[derive(Deserialize)]
#[serde(untagged)]
enum IntegerPreference {
    Signed(i64),
    Unsigned(u64),
}

fn clamp_layout_preference(value: IntegerPreference, min: usize, max: usize) -> usize {
    match value {
        IntegerPreference::Signed(value) => value.clamp(min as i64, max as i64) as usize,
        IntegerPreference::Unsigned(value) => value.clamp(min as u64, max as u64) as usize,
    }
}

fn deserialize_preferred_columns<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    IntegerPreference::deserialize(deserializer).map(|columns| {
        clamp_layout_preference(columns, MIN_PREFERRED_COLUMNS, MAX_PREFERRED_COLUMNS)
    })
}

fn deserialize_visible_rows<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    IntegerPreference::deserialize(deserializer)
        .map(|rows| clamp_layout_preference(rows, MIN_VISIBLE_ROWS, MAX_VISIBLE_ROWS))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuickToolsConfig {
    #[serde(default = "default_queries")]
    pub queries: Vec<QuickToolEntry>,
    #[serde(
        default = "default_preferred_columns",
        deserialize_with = "deserialize_preferred_columns"
    )]
    pub preferred_columns: usize,
    #[serde(
        default = "default_visible_rows",
        deserialize_with = "deserialize_visible_rows"
    )]
    pub visible_rows: usize,
}

impl Default for QuickToolsConfig {
    fn default() -> Self {
        Self {
            queries: default_queries(),
            preferred_columns: default_preferred_columns(),
            visible_rows: default_visible_rows(),
        }
    }
}

impl QuickToolsConfig {
    fn normalize_layout(&mut self) {
        self.preferred_columns = self
            .preferred_columns
            .clamp(MIN_PREFERRED_COLUMNS, MAX_PREFERRED_COLUMNS);
        self.visible_rows = self.visible_rows.clamp(MIN_VISIBLE_ROWS, MAX_VISIBLE_ROWS);
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

fn effective_columns(preferred: usize, available_width: f32, spacing: f32) -> usize {
    let columns_that_fit = ((available_width + spacing) / (MIN_QUICK_TOOL_CELL_WIDTH + spacing))
        .floor()
        .max(1.0) as usize;
    preferred
        .clamp(MIN_PREFERRED_COLUMNS, MAX_PREFERRED_COLUMNS)
        .min(columns_that_fit)
}

fn render_action_button(
    row_ui: &mut egui::Ui,
    row_id: egui::Id,
    ctx: &DashboardContext<'_>,
    entry: &QuickToolEntry,
    action: WidgetAction,
    duplicate_occurrence: usize,
    visual_order: u64,
    button_size: egui::Vec2,
) -> Option<WidgetAction> {
    let source_query = action.query_override.as_deref();
    let execution_query = Some(action.action.label.as_str());
    let full_label = if entry.auto_submit {
        format!("{} ↵\nAuto submit enabled", action.action.label)
    } else {
        action.action.label.clone()
    };
    let button = egui::Button::new(action.action.label.clone()).truncate();
    let button = if entry.auto_submit {
        button.right_text("↵")
    } else {
        button
    };
    let response = row_ui
        .scope_builder(egui::UiBuilder::new().id(row_id), |ui| {
            ui.add_sized(button_size, button)
        })
        .inner
        .on_hover_text(full_label);
    if response.gained_focus() {
        response.scroll_to_me(Some(egui::Align::Center));
    }

    let activated = jump_response_clicked(
        row_ui,
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
    );
    activated.then_some(action)
}

pub struct QuickToolsWidget {
    cfg: QuickToolsConfig,
}

impl QuickToolsWidget {
    pub fn new(cfg: QuickToolsConfig) -> Self {
        let mut cfg = cfg;
        cfg.normalize_layout();
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

            ui.label("Quick Tools layout");
            ui.small("These settings apply inside this dashboard slot.");
            ui.horizontal(|ui| {
                ui.label("Preferred columns");
                let response = ui.add(
                    egui::DragValue::new(&mut cfg.preferred_columns)
                        .range(MIN_PREFERRED_COLUMNS..=MAX_PREFERRED_COLUMNS),
                );
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "layout/preferred-columns",
                    "Preferred columns inside this dashboard slot",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                changed |= response.changed();
            });
            ui.horizontal(|ui| {
                ui.label("Visible rows");
                let response = ui.add(
                    egui::DragValue::new(&mut cfg.visible_rows)
                        .range(MIN_VISIBLE_ROWS..=MAX_VISIBLE_ROWS),
                );
                ctx.register_jump_mode_target(
                    ui,
                    &response,
                    "layout/visible-rows",
                    "Visible rows inside this dashboard slot",
                    crate::gui::jump_mode::TargetKind::Slider,
                );
                changed |= response.changed();
            });

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

        let content_rect = ui.clip_rect().intersect(ui.available_rect_before_wrap());
        let available_width = content_rect.width().max(0.0);
        let available_height = content_rect.height().max(0.0);
        if available_width <= f32::EPSILON || available_height <= f32::EPSILON {
            return None;
        }

        let spacing = ui.spacing();
        let row_height = (ui.text_style_height(&egui::TextStyle::Button)
            + spacing.button_padding.y * 2.0)
            .max(spacing.interact_size.y);
        let item_spacing_x = spacing.item_spacing.x;
        let item_spacing_y = spacing.item_spacing.y;
        let visible_rows = self
            .cfg
            .visible_rows
            .clamp(MIN_VISIBLE_ROWS, MAX_VISIBLE_ROWS);
        let preferred_height = row_height * visible_rows as f32
            + item_spacing_y * visible_rows.saturating_sub(1) as f32;
        let scroll_height = available_height.min(preferred_height);

        let valid_actions = self
            .cfg
            .queries
            .iter()
            .filter(|entry| !entry.query.trim().is_empty())
            .count();
        if valid_actions == 0 {
            ui.label("Add tools in the widget settings.");
            return None;
        }

        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt(ui.id().with("quick-tools-actions-scroll"))
            .max_width(available_width)
            .max_height(scroll_height)
            .min_scrolled_height(scroll_height.min(row_height))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show_viewport(ui, |scroll_ui, viewport| {
                let viewport_width = viewport.width().min(available_width).max(0.0);
                if viewport_width <= f32::EPSILON {
                    return;
                }
                let columns =
                    effective_columns(self.cfg.preferred_columns, viewport_width, item_spacing_x)
                        .min(valid_actions);
                let gap_width = item_spacing_x * columns.saturating_sub(1) as f32;
                let cell_width = ((viewport_width - gap_width) / columns as f32).max(0.0);
                let button_size = egui::vec2(cell_width, row_height);
                let mut tools = self
                    .cfg
                    .queries
                    .iter()
                    .filter_map(|entry| Self::action_for(entry).map(|action| (entry, action)));
                let mut duplicate_occurrences = HashMap::new();
                let mut visual_order = 0_u64;

                // Keep the identity parent independent of row/column layout so
                // resizing does not move Jump focus to a different action.
                scroll_ui.push_id("quick-tools-action-identities", |identity_ui| {
                    loop {
                        let mut row_actions = Vec::with_capacity(columns);
                        for _ in 0..columns {
                            let Some((entry, action)) = tools.next() else {
                                break;
                            };
                            let source_query = action.query_override.as_deref();
                            let execution_query = Some(action.action.label.as_str());
                            let base_identity = jump_action_identity(
                                &action.action,
                                source_query,
                                execution_query,
                                None,
                                0,
                            );
                            let duplicate_occurrence = next_jump_duplicate_occurrence(
                                &mut duplicate_occurrences,
                                base_identity,
                            );
                            let row_id = dashboard_action_row_egui_id(
                                identity_ui,
                                &action.action,
                                source_query,
                                execution_query,
                                None,
                                duplicate_occurrence,
                            );
                            row_actions.push((
                                entry,
                                action,
                                row_id,
                                duplicate_occurrence,
                                visual_order,
                            ));
                            visual_order = visual_order.saturating_add(1);
                        }
                        if row_actions.is_empty() {
                            break;
                        }
                        identity_ui.horizontal(|row_ui| {
                            row_ui.spacing_mut().item_spacing.x = 0.0;
                            for (column, (entry, action, row_id, duplicate, order)) in
                                row_actions.into_iter().enumerate()
                            {
                                if column > 0 {
                                    row_ui.add_space(item_spacing_x);
                                }
                                let activated = render_action_button(
                                    row_ui,
                                    row_id,
                                    ctx,
                                    entry,
                                    action,
                                    duplicate,
                                    order,
                                    button_size,
                                );
                                if activated.is_some() {
                                    clicked = activated;
                                }
                            }
                        });
                    }
                });
            });
        clicked
    }

    fn on_config_updated(&mut self, settings: &serde_json::Value) {
        if let Ok(mut cfg) = serde_json::from_value::<QuickToolsConfig>(settings.clone()) {
            cfg.normalize_layout();
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
        dashboard_input_with_size(egui::vec2(900.0, 650.0), events)
    }

    fn dashboard_input_with_size(size: egui::Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
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

    fn tab_event() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: Some(egui::Key::Tab),
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

    fn render_responsive_frame(
        egui_ctx: &egui::Context,
        jump: &mut JumpModeUi,
        widget: &mut QuickToolsWidget,
        plugins: &crate::plugin::PluginManager,
        data_cache: &DashboardDataCache,
        size: egui::Vec2,
        events: Vec<egui::Event>,
    ) -> (Option<WidgetAction>, crate::gui::JumpModeScope) {
        jump.begin_frame();
        jump.begin_ui_pass();
        let scope = body_scope(jump, "responsive");
        let context = dashboard_context(plugins, data_cache, scope.clone(), true);
        let mut activated = None;
        egui_ctx
            .run_ui(dashboard_input_with_size(size, events), |ui| {
                activated = render_in_slot(ui, widget, &context, "responsive");
            })
            .drop_without_applying_deltas();
        jump.finish_ui_pass();
        jump.complete_frame(egui_ctx, true, true);
        (activated, scope)
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
            preferred_columns: 5,
            visible_rows: 7,
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
        assert_eq!(cfg.preferred_columns, 5);
        assert_eq!(cfg.visible_rows, 7);
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

        let serialized = serde_json::to_value(&cfg).unwrap();
        assert_eq!(
            serialized,
            json!({"queries":[
                {"query":"A","auto_submit":true},
                {"query":"C","auto_submit":true},
                {"query":"B","auto_submit":false}
            ], "preferred_columns":5, "visible_rows":7})
        );
        assert_eq!(
            serde_json::from_value::<QuickToolsConfig>(serialized).unwrap(),
            cfg
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
        assert_eq!(legacy.preferred_columns, DEFAULT_PREFERRED_COLUMNS);
        assert_eq!(legacy.visible_rows, DEFAULT_VISIBLE_ROWS);
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
    fn persisted_negative_layout_preferences_are_clamped_without_losing_queries() {
        let cfg: QuickToolsConfig = serde_json::from_value(json!({
            "queries": ["legacy", {"query":"automatic", "auto_submit":true}],
            "preferred_columns": -3,
            "visible_rows": -1
        }))
        .unwrap();

        assert_eq!(cfg.preferred_columns, MIN_PREFERRED_COLUMNS);
        assert_eq!(cfg.visible_rows, MIN_VISIBLE_ROWS);
        assert_eq!(cfg.queries.len(), 2);
        assert_eq!(cfg.queries[0].query, "legacy");
        assert!(!cfg.queries[0].auto_submit);
        assert!(cfg.queries[1].auto_submit);
    }

    #[test]
    fn persisted_large_layout_integers_clamp_to_the_supported_maximum() {
        let cfg: QuickToolsConfig = serde_json::from_value(json!({
            "queries": ["preserved"],
            "preferred_columns": u64::MAX,
            "visible_rows": u64::MAX
        }))
        .unwrap();

        assert_eq!(cfg.preferred_columns, MAX_PREFERRED_COLUMNS);
        assert_eq!(cfg.visible_rows, MAX_VISIBLE_ROWS);
        assert_eq!(cfg.queries[0].query, "preserved");
    }

    #[test]
    fn opening_settings_normalizes_negative_preferences_and_preserves_legacy_queries() {
        let mut settings = json!({
            "queries": ["legacy manual", {"query":"saved automatic", "auto_submit":true}],
            "preferred_columns": -2,
            "visible_rows": -5
        });
        let context = WidgetSettingsContext::empty();
        let egui_context = egui::Context::default();

        egui_context
            .run_ui(dashboard_input(Vec::new()), |ui| {
                let result = QuickToolsWidget::settings_ui(ui, &mut settings, &context);
                assert!(result.changed);
                assert!(result.error.is_none());
            })
            .drop_without_applying_deltas();

        let migrated: QuickToolsConfig = serde_json::from_value(settings.clone()).unwrap();
        assert_eq!(migrated.preferred_columns, MIN_PREFERRED_COLUMNS);
        assert_eq!(migrated.visible_rows, MIN_VISIBLE_ROWS);
        assert_eq!(migrated.queries.len(), 2);
        assert_eq!(migrated.queries[0].query, "legacy manual");
        assert!(!migrated.queries[0].auto_submit);
        assert_eq!(migrated.queries[1].query, "saved automatic");
        assert!(migrated.queries[1].auto_submit);
    }

    #[test]
    fn direct_config_construction_is_normalized_by_widget() {
        let widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: vec![QuickToolEntry::manual("preserved")],
            preferred_columns: usize::MAX,
            visible_rows: 0,
        });

        assert_eq!(widget.cfg.preferred_columns, MAX_PREFERRED_COLUMNS);
        assert_eq!(widget.cfg.visible_rows, MIN_VISIBLE_ROWS);
        assert_eq!(widget.cfg.queries[0].query, "preserved");
    }

    #[test]
    fn config_updates_apply_layout_defaults_and_ignore_invalid_documents() {
        let mut widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: vec![QuickToolEntry::manual("old")],
            preferred_columns: 6,
            visible_rows: 9,
        });
        widget.on_config_updated(&json!({"queries":["updated"]}));
        assert_eq!(widget.cfg.preferred_columns, DEFAULT_PREFERRED_COLUMNS);
        assert_eq!(widget.cfg.visible_rows, DEFAULT_VISIBLE_ROWS);
        assert_eq!(widget.cfg.queries[0].query, "updated");

        let before = widget.cfg.clone();
        widget.on_config_updated(&json!({"queries":42, "preferred_columns":2}));
        assert_eq!(widget.cfg, before);
    }

    #[test]
    fn serialization_is_always_canonical_after_legacy_input() {
        let cfg: QuickToolsConfig = serde_json::from_value(json!({"queries":["fs"]})).unwrap();
        assert_eq!(
            serde_json::to_value(cfg).unwrap(),
            json!({
                "queries":[{"query":"fs","auto_submit":false}],
                "preferred_columns":DEFAULT_PREFERRED_COLUMNS,
                "visible_rows":DEFAULT_VISIBLE_ROWS
            })
        );
    }

    #[test]
    fn defaults_and_new_entries_are_manual() {
        assert!(default_queries().iter().all(|entry| !entry.auto_submit));
        let defaults = QuickToolsConfig::default();
        assert_eq!(defaults.preferred_columns, DEFAULT_PREFERRED_COLUMNS);
        assert_eq!(defaults.visible_rows, DEFAULT_VISIBLE_ROWS);
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
            ..QuickToolsConfig::default()
        });
        let mut second = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
            ..QuickToolsConfig::default()
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
                let first_rect =
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(340.0, 120.0));
                let second_rect =
                    egui::Rect::from_min_size(egui::pos2(360.0, 0.0), egui::vec2(340.0, 120.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(first_rect), |ui| {
                    assert!(render_in_slot(ui, &mut first, &first_context, "first").is_none());
                });
                ui.scope_builder(egui::UiBuilder::new().max_rect(second_rect), |ui| {
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
    fn responsive_grid_bounds_buttons_and_keeps_duplicate_ids_when_resized_and_scaled() {
        let entries = vec![
            QuickToolEntry::manual("sys"),
            QuickToolEntry::manual("sys"),
            QuickToolEntry {
                query: "sys".into(),
                auto_submit: true,
            },
            QuickToolEntry::manual("日本語🌿かなり長いツール名を表示します"),
            QuickToolEntry::manual("five"),
            QuickToolEntry::manual("six"),
            QuickToolEntry::manual("seven"),
            QuickToolEntry::manual("eight"),
            QuickToolEntry::manual("last"),
        ];
        let mut widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
            preferred_columns: 3,
            visible_rows: 2,
        });
        let plugins = crate::plugin::PluginManager::new_inert_for_test();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        let mut jump = JumpModeUi::new(true);

        let (_, scope) = render_responsive_frame(
            &egui_ctx,
            &mut jump,
            &mut widget,
            &plugins,
            &data_cache,
            egui::vec2(540.0, 160.0),
            Vec::new(),
        );
        let first_duplicate = target_id(&scope, &entries[0], 0);
        let second_duplicate = target_id(&scope, &entries[1], 1);
        let wide_targets = jump.retained_registry().targets();
        let wide_first = wide_targets
            .iter()
            .find(|target| target.id() == &first_duplicate)
            .expect("the first duplicate is initially visible");
        let wide_second = wide_targets
            .iter()
            .find(|target| target.id() == &second_duplicate)
            .expect("the second duplicate is initially visible");
        let wide_first_response = wide_first.response_id();
        let wide_second_response = wide_second.response_id();
        assert_ne!(wide_first_response, wide_second_response);
        let wide_first_rect = wide_first.rect();
        let wide_second_rect = wide_second.rect();
        let wide_third_id = target_id(&scope, &entries[2], 0);
        let wide_third = wide_targets
            .iter()
            .find(|target| target.id() == &wide_third_id)
            .expect("the third preferred column is used when there is room");
        assert_eq!(wide_first_rect.center().y, wide_second_rect.center().y);
        assert_eq!(wide_first_rect.center().y, wide_third.rect().center().y);
        assert!(wide_first_rect.center().x < wide_second_rect.center().x);
        assert!(wide_second_rect.center().x < wide_third.rect().center().x);

        let _ = render_responsive_frame(
            &egui_ctx,
            &mut jump,
            &mut widget,
            &plugins,
            &data_cache,
            egui::vec2(250.0, 160.0),
            Vec::new(),
        );
        let small_targets = jump.retained_registry().targets();
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(250.0, 160.0));
        assert!(small_targets.iter().all(|target| {
            target.rect().min.x >= viewport.min.x
                && target.rect().max.x <= viewport.max.x
                && target.clipped_rect().is_positive()
                && target.clipped_rect().min.x >= viewport.min.x
                && target.clipped_rect().max.x <= viewport.max.x
        }));
        let first_small = small_targets
            .iter()
            .find(|target| target.id() == &first_duplicate)
            .expect("the first duplicate remains visible after resize");
        let second_small = small_targets
            .iter()
            .find(|target| target.id() == &second_duplicate)
            .expect("the second duplicate remains visible after resize");
        let third_small = small_targets
            .iter()
            .find(|target| target.id() == &wide_third_id)
            .expect("the third action wraps into the next row at the narrower width");
        assert_eq!(first_small.response_id(), wide_first_response);
        assert_eq!(second_small.response_id(), wide_second_response);
        assert_eq!(
            first_small.rect().center().y,
            second_small.rect().center().y
        );
        assert!(third_small.rect().center().y > first_small.rect().center().y);
        assert!(first_small.rect().width() < wide_first_rect.width());

        egui_ctx.set_pixels_per_point(1.5);
        let theme = egui_ctx.theme();
        let mut style = egui_ctx.style_of(theme).as_ref().clone();
        for font in style.text_styles.values_mut() {
            font.size *= 1.35;
        }
        egui_ctx.set_style_of(theme, style);
        let _ = render_responsive_frame(
            &egui_ctx,
            &mut jump,
            &mut widget,
            &plugins,
            &data_cache,
            egui::vec2(250.0, 160.0),
            Vec::new(),
        );
        let scaled_targets = jump.retained_registry().targets();
        assert!(scaled_targets.iter().all(|target| {
            target.rect().min.x >= viewport.min.x
                && target.rect().max.x <= viewport.max.x
                && target.clipped_rect().is_positive()
                && target.clipped_rect().min.x >= viewport.min.x
                && target.clipped_rect().max.x <= viewport.max.x
                && target.clipped_rect().min.y >= viewport.min.y
                && target.clipped_rect().max.y <= viewport.max.y
        }));
        assert!(
            scaled_targets
                .iter()
                .any(|target| target.id() == &first_duplicate)
        );
    }

    #[test]
    fn tab_focus_scrolls_offscreen_quick_tool_into_view() {
        let entries = (1..=7)
            .map(|number| QuickToolEntry::manual(format!("tool {number}")))
            .collect::<Vec<_>>();
        let mut widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: entries.clone(),
            preferred_columns: 1,
            visible_rows: 1,
        });
        let plugins = crate::plugin::PluginManager::new_inert_for_test();
        let data_cache = DashboardDataCache::new();
        let egui_ctx = egui::Context::default();
        egui_ctx.global_style_mut(|style| {
            style.scroll_animation = egui::style::ScrollAnimation::none();
        });
        let mut jump = JumpModeUi::new(true);
        let size = egui::vec2(320.0, 90.0);

        let (_, final_scope) = render_responsive_frame(
            &egui_ctx,
            &mut jump,
            &mut widget,
            &plugins,
            &data_cache,
            size,
            Vec::new(),
        );
        let final_target = target_id(
            &final_scope,
            entries.last().expect("the fixture contains a final tool"),
            0,
        );
        assert!(
            !jump.retained_registry().targets().iter().any(|target| {
                target.id() == &final_target && target.clipped_rect().is_positive()
            }),
            "the final tool starts outside the one-row viewport"
        );

        let mut final_target_focused_and_visible = false;
        for _ in 0..entries.len() * 2 {
            let _ = render_responsive_frame(
                &egui_ctx,
                &mut jump,
                &mut widget,
                &plugins,
                &data_cache,
                size,
                vec![tab_event()],
            );
            let _ = render_responsive_frame(
                &egui_ctx,
                &mut jump,
                &mut widget,
                &plugins,
                &data_cache,
                size,
                Vec::new(),
            );
            let _ = render_responsive_frame(
                &egui_ctx,
                &mut jump,
                &mut widget,
                &plugins,
                &data_cache,
                size,
                Vec::new(),
            );
            final_target_focused_and_visible = jump
                .retained_registry()
                .targets()
                .iter()
                .find(|target| target.id() == &final_target)
                .is_some_and(|target| {
                    target.clipped_rect().contains_rect(target.rect())
                        && egui_ctx.memory(|memory| memory.focused()) == Some(target.response_id())
                });
            if final_target_focused_and_visible {
                break;
            }
        }
        assert!(
            final_target_focused_and_visible,
            "Tab reaches the final tool, focuses it, and scrolls it fully into view"
        );
        let focused_target = jump
            .retained_registry()
            .targets()
            .into_iter()
            .find(|target| target.id() == &final_target)
            .expect("the final tool registers after it scrolls into view");
        assert_eq!(
            egui_ctx.memory(|memory| memory.focused()),
            Some(focused_target.response_id()),
            "the revealed final tool is the native keyboard focus target"
        );
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
            ..QuickToolsConfig::default()
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
            ..QuickToolsConfig::default()
        });
        let mut clipped_widget = QuickToolsWidget::new(QuickToolsConfig {
            queries: vec![QuickToolEntry::manual("visible query")],
            ..QuickToolsConfig::default()
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
